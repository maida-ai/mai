#!/usr/bin/env python3
"""Offline synthetic manual-validation comparison; no queue/MTTR qualification."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--mai", type=Path, required=True)
    parser.add_argument("--iterations", type=int, required=True)
    args = parser.parse_args()
    if not 1 <= args.iterations <= 20:
        parser.error("--iterations must be between 1 and 20")
    binary = args.mai.resolve(strict=True)
    git = shutil.which("git")
    shell = shutil.which("sh")
    if not git or not shell:
        parser.error("git and sh must be on PATH")
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    env.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull,
               GIT_TERMINAL_PROMPT="0", GIT_AUTHOR_NAME="mai synthetic fixture",
               GIT_COMMITTER_NAME="mai synthetic fixture",
               GIT_AUTHOR_EMAIL="fixture@example.invalid",
               GIT_COMMITTER_EMAIL="fixture@example.invalid",
               GIT_AUTHOR_DATE="2000-01-01T00:00:00Z",
               GIT_COMMITTER_DATE="2000-01-01T00:00:00Z")
    samples = []

    def save(name, data):
        (root / name).write_text(json.dumps(data, indent=2) + "\n")

    def run(label, command, cwd=None, expected=0, extra_env=None):
        command = [str(x) for x in command]
        start = time.monotonic()
        try:
            result = subprocess.run(command, cwd=cwd, env=env | (extra_env or {}),
                                    capture_output=True, timeout=30)
            code, stdout, stderr = result.returncode, result.stdout, result.stderr
        except subprocess.TimeoutExpired as error:
            code, stdout, stderr = "outer_timeout", error.stdout or b"", error.stderr or b""
        elapsed = time.monotonic() - start
        (root / (label + ".log")).write_bytes(stdout + b"\n--- stderr ---\n" + stderr)
        record = dict(label=label, command=command, cwd=str(cwd) if cwd else None,
                      elapsed_s=elapsed, exit=code, expected_exit=expected,
                      log=label + ".log")
        with (root / "commands.jsonl").open("a") as stream:
            stream.write(json.dumps(record) + "\n")
        if code != expected:
            raise RuntimeError(f"{label}: expected exit {expected}, got {code}; retained {record['log']}")
        return stdout.decode("utf-8").strip(), elapsed

    def g(label, repo, *command):
        return run(label, [git, "-C", repo, *command])[0]

    provenance = dict(schema_version=1, scope="synthetic_manual_validation_only",
                      utc=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                      platform=platform.platform(), python=sys.version,
                      cpu_count=os.cpu_count(), iterations=args.iterations,
                      binary_path=str(binary), binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                      harness_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                      cache_policy="No bootstrap or declared caches; cold=fresh clone/slot, warm=reused clone/slot; OS caches not flushed")
    save("provenance.json", provenance)
    try:
        provenance["git_version"] = run("git-version", [git, "--version"])[0]
        provenance["mai_version"] = run("mai-version", [binary, "--version"])[0]
        provenance["shell_path"] = str(Path(shell).resolve())
        provenance["shell_sha256"] = hashlib.sha256(Path(shell).resolve().read_bytes()).hexdigest()
        product = Path(__file__).resolve().parents[2]
        provenance["harness_revision"] = g("harness-revision", product, "rev-parse", "HEAD")
        provenance["harness_status"] = g("harness-status", product, "status", "--short")
        source = root / "source"
        source.mkdir()
        g("source-init", source, "init", "-b", "main")
        (source / "message.txt").write_text("hello\n")
        (source / "gate.sh").write_text('set -eu\ntest "$(cat "$MAI_CANDIDATE_ROOT/message.txt")" = hello\n')
        (source / "mai.toml").write_text('''version = 1
[check]
program = "sh"
script = "gate.sh"
timeout_ms = 5000
output_tail_bytes = 1024
[validators]
slots = 1
port_base = 31000
ports_per_slot = 10
''')
        g("source-add", source, "add", ".")
        g("source-commit", source, "commit", "-m", "Synthetic passing fixture")
        passing = g("passing-id", source, "rev-parse", "HEAD")
        (source / "message.txt").write_text("wrong\n")
        g("failure-add", source, "add", "message.txt")
        g("failure-commit", source, "commit", "-m", "Synthetic failing candidate")
        failing = g("failure-id", source, "rev-parse", "HEAD")
        trees = {candidate: g("tree-" + candidate, source, "rev-parse", candidate + "^{tree}")
                 for candidate in (passing, failing)}
        g("source-restore", source, "checkout", "--detach", passing)
        provenance.update(passing_commit=passing, failing_commit=failing, candidate_trees=trees,
                          tracked_files=3, tracked_bytes=sum((source / name).stat().st_size
                          for name in ("message.txt", "gate.sh", "mai.toml")),
                          gate_sha256=hashlib.sha256((source / "gate.sh").read_bytes()).hexdigest())
        save("provenance.json", provenance)
        for iteration in range(args.iterations):
            order = ("git", "mai") if iteration % 2 == 0 else ("mai", "git")
            for side in order:
                prefix = f"{iteration:02d}-{side}"
                destination = root / prefix
                if side == "git":
                    _, setup = run(prefix + "-setup", [git, "clone", "--no-local", "--no-hardlinks", source, destination])
                    g(prefix + "-import", destination, "fetch", source, failing)
                else:
                    _, setup = run(prefix + "-setup", [binary, "--json", "init", destination, "--from", source])
                    # Fixture plumbing only, deliberately not claimed as receive.
                    g(prefix + "-import", destination / "authority.git", "fetch", source,
                      failing + ":refs/experiments/failing")
                samples.append(dict(iteration=iteration, side=side, phase="setup", elapsed_s=setup))
                for phase, candidate, expected in (("cold", passing, "success"), ("warm", passing, "success"),
                                                    ("failing", failing, "failure"), ("after-failure", passing, "success")):
                    label = prefix + "-" + phase
                    start = time.monotonic()
                    if side == "git":
                        g(label + "-checkout", destination, "checkout", "--detach", candidate)
                        actual_tree = g(label + "-tree", destination, "rev-parse", "HEAD^{tree}")
                        if actual_tree != trees[candidate]:
                            raise RuntimeError(label + ": baseline tree mismatch")
                        _, gate_elapsed = run(label + "-gate", [shell, source / "gate.sh"], cwd=source,
                                              expected=0 if expected == "success" else 1,
                                              extra_env={"MAI_CANDIDATE_ROOT": str(destination), "MAI_POLICY_ROOT": str(source)})
                        if g(label + "-dirty", destination, "status", "--porcelain"):
                            raise RuntimeError(label + ": baseline mutated candidate")
                    else:
                        output, gate_elapsed = run(label + "-validate", [binary, "--json", "validator", "run", destination, candidate],
                                                   expected=0 if expected == "success" else 3)
                        result = json.loads(output)
                        binding = result.get("binding", {})
                        if any(binding.get(key) != value for key, value in {"candidate": candidate, "tree": trees[candidate], "base": passing, "policy": passing}.items()):
                            raise RuntimeError(label + ": validator evidence identity mismatch")
                        if result.get("outcome") != expected:
                            raise RuntimeError(label + ": unexpected gate outcome")
                        save(label + "-result.json", result)
                    samples.append(dict(iteration=iteration, side=side, phase=phase, candidate=candidate,
                                        tree=trees[candidate], expected_outcome=expected, verified=True,
                                        elapsed_s=time.monotonic() - start, operation_elapsed_s=gate_elapsed))
                if side == "mai":
                    if g(prefix + "-trunk", destination / "authority.git", "rev-parse", "refs/heads/trunk") != passing:
                        raise RuntimeError(prefix + ": validator advanced trunk")
                save("samples.json", samples)
        save("summary.json", dict(schema_version=1, success=True, sample_count=len(samples),
                                  claims="Gate truth and bounded synthetic timings only; no throughput or MTTR claim"))
    except Exception as error:
        save("samples.json", samples)
        save("summary.json", dict(schema_version=1, success=False, error=str(error)))
        print(str(error), file=sys.stderr)
        return 1
    print(f"Evidence retained in {root}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Measure the real manual-validator product against pinned public fixtures.

No model calls, receive, landing or experiment-workflow dispatches. All writes are
under a newly created output root. The caller explicitly chooses the mai binary.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import time

FIXTURES = {
    "rust": ("https://github.com/BurntSushi/ripgrep.git", "4649aa9700619f94cf9c66876e9549d83420e16c", "Cargo.toml", "crates/core/main.rs"),
    "python": ("https://github.com/pallets/itsdangerous.git", "096c8d42545d3b68ea21a4f890fb2b2d8979c0bd", "pyproject.toml", "src/itsdangerous/__init__.py"),
    "typescript": ("https://github.com/colinhacks/zod.git", "e30870369d5b8f31ff4d0130d4439fd997deb523", "package.json", "src/index.ts"),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--mai", type=Path, required=True)
    parser.add_argument("--fixtures", type=Path, help="optional pre-fetched fixture clones")
    parser.add_argument("--fixture", choices=["all", *FIXTURES], default="all")
    args = parser.parse_args()
    binary = args.mai.resolve(strict=True)
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, GIT_TERMINAL_PROMPT="0", CI="true", HUSKY="0", PYTHONDONTWRITEBYTECODE="1")
    env.update(UV_CACHE_DIR=str(root/"uv-download-cache"), npm_config_cache=str(root/"npm-download-cache"), YARN_CACHE_FOLDER=str(root/"yarn-download-cache"))
    env.update(GIT_AUTHOR_NAME="mai product experiment", GIT_COMMITTER_NAME="mai product experiment", GIT_AUTHOR_EMAIL="experiment@example.invalid", GIT_COMMITTER_EMAIL="experiment@example.invalid")
    records = root / "commands.jsonl"

    def run(label, command, cwd=None, expect=None, timeout=900):
        start = time.monotonic()
        log = root / (label + ".log")
        with log.open("wb") as out:
            try:
                result = subprocess.run([str(x) for x in command], cwd=cwd, env=env, stdout=out, stderr=subprocess.STDOUT, timeout=timeout, check=False)
                code = result.returncode
            except subprocess.TimeoutExpired:
                code = "outer_timeout"
        data = {"label": label, "command": [str(x) for x in command], "cwd": str(cwd) if cwd else None, "elapsed_s": time.monotonic()-start, "exit": code, "log": log.name}
        with records.open("a") as out:
            out.write(json.dumps(data) + "\n")
        if expect is not None and code != expect:
            raise RuntimeError(f"{label}: expected {expect}, got {code}; see {log}")
        return log.read_text(errors="replace")

    def git(label, repo, *command):
        return run(label, ["git", "-C", repo, *command], expect=0).strip()

    provenance = {"environment": {k: env.get(k) for k in ["TMPDIR", "TEMP", "TMP", "UV_CACHE_DIR", "YARN_CACHE_FOLDER", "npm_config_cache", "CARGO_HOME", "CARGO_TARGET_DIR"]}, "utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "platform": platform.platform(), "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "harness_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), "fixtures": FIXTURES}
    product_root = Path(__file__).resolve().parents[2]
    source_paths = [product_root/"Cargo.toml", product_root/"Cargo.lock", *sorted((product_root/"src").rglob("*.rs"))]
    provenance["product_source_sha256"] = {str(p.relative_to(product_root)): hashlib.sha256(p.read_bytes()).hexdigest() for p in source_paths}
    (root / "provenance.json").write_text(json.dumps(provenance, indent=2)+"\n")
    for program, flags in [(binary, ["--version"]), ("git", ["--version"]), ("rustc", ["-vV"]), ("cargo", ["--version"]), ("uv", ["--version"]), ("python3", ["--version"]), ("node", ["--version"]), ("yarn", ["--version"])]:
        run("tool-"+Path(program).stem, [program, *flags])
    run("product-revision", ["git", "-C", Path(__file__).resolve().parents[2], "rev-parse", "HEAD"])
    run("product-working-tree", ["git", "-C", Path(__file__).resolve().parents[2], "status", "--short"])

    failures = []
    for name, (url, revision, input_path, poison) in FIXTURES.items():
        if args.fixture not in ("all", name):
            continue
        try:
            source = root / (name+"-source")
            origin = args.fixtures.resolve()/name if args.fixtures else url
            run(name+"-clone", ["git", "clone", "--no-local", "--no-hardlinks", "--no-checkout", origin, source], expect=0)
            git(name+"-checkout", source, "checkout", "--detach", revision)
            identities = {"url": url, "revision": revision, "locks": {str(p.relative_to(source)): hashlib.sha256(p.read_bytes()).hexdigest() for p in [source/"Cargo.lock", source/"requirements/tests.txt", source/"yarn.lock"] if p.is_file()}}
            (root/(name+"-identity.json")).write_text(json.dumps(identities, indent=2)+"\n")
            # Some pinned upstream history contains retired submodule gitlinks,
            # outside product import scope. Import its exact source tree as a
            # new repository; retain upstream revision+archive hash evidence.
            upstream_tree = git(name+"-upstream-tree", source, "rev-parse", revision+"^{tree}")
            archive = root/(name+"-pinned-source.tar")
            git(name+"-snapshot-archive", source, "archive", "--format=tar", "--output", str(archive), revision)
            source = root/(name+"-pinned-snapshot")
            source.mkdir()
            git(name+"-snapshot-init", source, "init")
            # Fetch only to FETCH_HEAD, then create a parentless commit. Git's
            # native checkout preserves index modes/symlinks on each platform;
            # imported reachable history contains only this pinned source tree.
            git(name+"-snapshot-fetch", source, "fetch", "--no-tags", str(root/(name+"-source")), revision)
            git(name+"-snapshot-read", source, "read-tree", upstream_tree)
            git(name+"-snapshot-checkout", source, "checkout-index", "--all")
            snapshot = git(name+"-snapshot-commit", source, "commit-tree", upstream_tree, "-m", "Import exact pinned source tree without upstream history")
            git(name+"-snapshot-head", source, "update-ref", "HEAD", snapshot)
            snapshot_tree = git(name+"-snapshot-tree", source, "rev-parse", "HEAD^{tree}")
            if upstream_tree != snapshot_tree:
                raise RuntimeError(f"{name}: snapshot tree differs from pinned upstream tree")
            identities["tree"] = upstream_tree
            identities["import_mode"] = "pinned_tree_snapshot_without_upstream_history"
            identities["archive_sha256"] = hashlib.sha256(archive.read_bytes()).hexdigest()
            (root/(name+"-identity.json")).write_text(json.dumps(identities, indent=2)+"\n")
            scripts = source / "mai-experiment"
            scripts.mkdir()
            boot, gate, caches, tools = fixture_commands(name)
            (scripts/"bootstrap.sh").write_text('#!/bin/sh\nset -eu\ncd "$MAI_CANDIDATE_ROOT"\n'+boot+"\n")
            (scripts/"check.sh").write_text('#!/bin/sh\nset -eu\ncd "$MAI_CANDIDATE_ROOT"\n'+gate+"\n")
            (source/"mai.toml").write_text(policy(input_path, caches, tools))
            git(name+"-policy-add", source, "add", "mai.toml", "mai-experiment")
            git(name+"-policy-commit", source, "commit", "-m", "Install explicit trusted experiment policy")
            baseline = git(name+"-baseline", source, "rev-parse", "HEAD")
            project = root/(name+"-authority")
            run(name+"-init", [binary, "--json", "init", project, "--from", source], expect=0)
            # Import fixture candidates through local plumbing only: receive stays disabled.
            repo = project/"authority.git"
            (source/"mai-experiment-marker").write_text("candidate switch\n")
            git(name+"-switch-add", source, "add", "mai-experiment-marker")
            git(name+"-switch-commit", source, "commit", "-m", "Non-code candidate switch")
            switched = git(name+"-switch-id", source, "rev-parse", "HEAD")
            with (source/poison).open("a") as out:
                out.write("\nTHIS_IS_A_DELIBERATELY_INVALID_PRODUCT_EXPERIMENT\n")
            git(name+"-failing-add", source, "add", poison)
            git(name+"-failing-commit", source, "commit", "-m", "Deliberately failing candidate")
            failing = git(name+"-failing-id", source, "rev-parse", "HEAD")
            git(name+"-import", repo, "fetch", str(source), "HEAD:refs/experiments/failing")
            for label, candidate in [("cold", baseline), ("warm", baseline), ("switch", switched), ("return", baseline), ("failing", failing), ("after-failure", baseline)]:
                stamp_before = bootstrap_stamp(project)
                result = run(name+"-"+label, [binary, "--json", "validator", "run", project, candidate])
                decoded = json.loads(result)
                decoded["experiment_bootstrap_before"] = stamp_before
                decoded["experiment_bootstrap_after"] = bootstrap_stamp(project)
                expected = "failure" if label == "failing" else "success"
                if label == "cold" and (decoded.get("bootstrap_reused") or not decoded.get("bootstrap")):
                    raise RuntimeError(f"{name}: cold run did not record fresh bootstrap")
                if label in ("warm", "switch", "return", "failing", "after-failure") and not decoded.get("bootstrap_reused"):
                    raise RuntimeError(f"{name}-{label}: expected bootstrap reuse")
                if decoded.get("outcome") != expected:
                    raise RuntimeError(f"{name}-{label}: expected {expected}, got {decoded.get('outcome')}")
                (root/(name+"-"+label+".json")).write_text(json.dumps(decoded, indent=2)+"\n")
            # Explicit disposable-authority trunk update tests trusted-policy invalidation;
            # this is fixture plumbing, never an engine landing or production operation.
            git(name+"-reset-source", source, "checkout", "--detach", baseline)
            with (source/input_path).open("a") as out:
                out.write("\n" if name == "typescript" else "\n# product bootstrap fingerprint invalidation\n")
            git(name+"-invalidate-add", source, "add", input_path)
            git(name+"-invalidate-commit", source, "commit", "-m", "Change declared trusted bootstrap input")
            changed = git(name+"-invalidate-id", source, "rev-parse", "HEAD")
            git(name+"-invalidate-import", repo, "fetch", str(source), "HEAD:refs/experiments/invalidation")
            git(name+"-fixture-trunk-update", repo, "update-ref", "refs/heads/trunk", changed, baseline)
            invalidation_before = bootstrap_stamp(project)
            invalidated = json.loads(run(name+"-invalidation", [binary, "--json", "validator", "run", project, changed]))
            if invalidated.get("bootstrap_reused") or not invalidated.get("bootstrap"):
                raise RuntimeError(f"{name}: changed input did not force recorded rebootstrap")
            if invalidated.get("outcome") != "success":
                raise RuntimeError(f"{name}: invalidated bootstrap/gate failed: {invalidated.get('outcome')}")
            (root/(name+"-invalidation.json")).write_text(json.dumps({"product": invalidated, "bootstrap_before": invalidation_before, "bootstrap_after": bootstrap_stamp(project)}, indent=2)+"\n")
            workspace = json.loads(run(name+"-workspace-fresh", [binary, "--json", "workspace", project, "experiment-agent", root/(name+"-workspace")], expect=0))
            (root/(name+"-workspace-fresh.json")).write_text(json.dumps(workspace, indent=2)+"\n")
            if workspace.get("bootstrap_skipped") or (workspace.get("bootstrap") or {}).get("outcome") != "success":
                raise RuntimeError(f"{name}: automatic workspace bootstrap did not succeed")
        except Exception as exc:
            failures.append({"fixture": name, "error": str(exc)})
            (root/(name+"-error.txt")).write_text(str(exc)+"\n")
    (root/"summary.json").write_text(json.dumps({"failures": failures, "complete": not failures}, indent=2)+"\n")
    return bool(failures)


def bootstrap_stamp(project):
    return [{"name": str(p.relative_to(project)), "mtime_ns": p.stat().st_mtime_ns, "content": p.read_text()} for p in sorted((project/".mai").glob("validator-bootstrap-*.json"))]


def fixture_commands(name):
    if name == "rust":
        return "mkdir -p target\ncargo fetch --locked", "cargo test --locked --workspace", ["target"], ["cargo", "rustc"]
    if name == "python":
        return "uv venv --allow-existing --python 3.12 .venv\nuv pip install --python .venv -r requirements/tests.txt flit_core==3.9.0\nuv pip install --python .venv --no-build-isolation -e .", 'if [ -f .venv/Scripts/python.exe ]; then .venv/Scripts/python.exe -m pytest; else .venv/bin/python -m pytest; fi', [".venv"], ["uv", "python3"]
    return "yarn install --frozen-lockfile --non-interactive --ignore-scripts", "yarn run test:ts-jest --runInBand", ["node_modules"], ["node", "yarn"]


def policy(input_path, caches, tools):
    inputs = [input_path, {"Cargo.toml": "Cargo.lock", "pyproject.toml": "requirements/tests.txt", "package.json": "yarn.lock"}[input_path]]
    command = 'program = "sh"\nscript = "mai-experiment/{script}.sh"\ninputs = '+json.dumps(inputs)+'\ntools = '+json.dumps(tools)+'\ntimeout_ms = 600000\noutput_tail_bytes = 16384\n'
    return 'version = 1\n[check]\n'+command.format(script="check")+'\n[validators]\nslots = 1\nport_base = 24000\nports_per_slot = 10\ncaches = '+json.dumps(caches)+'\n[validators.bootstrap]\n'+command.format(script="bootstrap")+'\n[workspaces.bootstrap]\n'+command.format(script="bootstrap")


if __name__ == "__main__":
    raise SystemExit(main())

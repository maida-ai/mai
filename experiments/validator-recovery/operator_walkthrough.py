#!/usr/bin/env python3
"""Disposable operator walkthrough; never targets an existing authority."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[2])
parser.add_argument("--binary", type=Path, help="Built mai executable; defaults to REPO/target/debug/mai")
parser.add_argument("--output", type=Path, required=True, help="New evidence directory; must not exist")
parser.add_argument("--temp-root", type=Path, required=True, help="Existing short absolute scratch path outside any Git repository")
args = parser.parse_args()
repo = args.repo.resolve()
source_binary = (args.binary or repo / "target/debug/mai").resolve()
out = args.output.resolve()
out.mkdir(parents=True)
binary = out / "mai"
shutil.copy2(source_binary, binary)
fixture_root = Path(tempfile.mkdtemp(prefix="mai-op-", dir=args.temp_root.resolve()))
seed, authority, client, control = (fixture_root / name for name in ("seed", "authority", "client", "control"))
seed.mkdir()
control.mkdir()
environment = os.environ.copy()
environment.update({
    "TMPDIR": str(args.temp_root.resolve()),
    "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": "/dev/null",
    "GIT_AUTHOR_NAME": "operator-fixture", "GIT_COMMITTER_NAME": "operator-fixture",
    "GIT_AUTHOR_EMAIL": "fixture@localhost", "GIT_COMMITTER_EMAIL": "fixture@localhost",
})
transcript = (out / "commands.jsonl").open("w")

def run(args, *, expected=0):
    result = subprocess.run([str(arg) for arg in args], env=environment, capture_output=True, text=True, timeout=90)
    transcript.write(json.dumps({"argv": [str(arg) for arg in args], "exit": result.returncode,
                                 "stdout": result.stdout, "stderr": result.stderr}) + "\n")
    transcript.flush()
    assert result.returncode == expected, (args, result.returncode, result.stderr)
    return result.stdout.strip()

def git(root, *args):
    return run(["git", "-C", root, *args])

def mai(*args):
    return run([binary, "--json", *args])

def await_file(path):
    deadline = time.monotonic() + 20
    while not path.is_file():
        assert time.monotonic() < deadline, path
        time.sleep(0.02)

metadata = {
    "binary": str(binary), "source_binary": str(source_binary),
    "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    "source_head": git(repo, "rev-parse", "HEAD"),
    "tracked_diff_sha256": hashlib.sha256(subprocess.check_output(["git", "-C", str(repo), "diff", "--binary"])).hexdigest(),
    "platform": os.uname()._asdict() if hasattr(os.uname(), "_asdict") else list(os.uname()),
    "temporary_root": environment["TMPDIR"],
    "fixture_root": str(fixture_root),
    "branch": git(repo, "branch", "--show-current"),
    "git_version": run(["git", "--version"]),
    "mai_version": run([binary, "--version"]),
    "rustc_version": run(["rustc", "--version"]),
}
(out / "identity.json").write_text(json.dumps(metadata, indent=2) + "\n")
git(seed, "init", "-b", "main")
(seed / "work.txt").write_text("baseline\n")
(seed / "gate.sh").write_text('printf run >> "$1/invocations"\nprintf ready > "$1/ready"\ni=0\nwhile ! test -f "$1/release"; do\n i=$((i+1))\n if test "$i" -ge 600; then exit 19; fi\n sleep 0.05\ndone\nprintf done > "$1/done"\n')
(seed / "mai.toml").write_text('version=1\n[check]\nprogram="sh"\nscript="gate.sh"\nargs=[' + json.dumps(str(control)) + ']\ntimeout_ms=60000\noutput_tail_bytes=1024\n[validators]\nslots=1\nport_base=40000\nports_per_slot=10\nrecovery="supervised-local-v1"\n')
git(seed, "add", ".")
git(seed, "commit", "-m", "operator recovery fixture")
mai("init", authority, "--from", seed)
engine_log = (out / "engine.log").open("w")
engine = subprocess.Popen([str(binary), "serve", str(authority), "--foreground"], env=environment,
                          stdout=subprocess.DEVNULL, stderr=engine_log)
request = None
try:
    deadline = time.monotonic() + 20
    while True:
        status = subprocess.run([str(binary), "status", str(authority), "--no-autostart"], env=environment,
                                capture_output=True, text=True, timeout=3)
        if status.returncode == 0:
            break
        assert engine.poll() is None, engine_log.name
        assert time.monotonic() < deadline
        time.sleep(0.02)
    mai("receive", "enable", authority, "--max-input-bytes", "1048576", "--max-file-bytes", "65536", "--max-changed-files", "16")
    run(["git", "clone", authority / "authority.git", client])
    baseline = git(authority / "authority.git", "rev-parse", "trunk")
    (client / "work.txt").write_text("submitted\n")
    git(client, "add", "work.txt")
    git(client, "commit", "-m", "operator submitted change")
    submitted = git(client, "rev-parse", "HEAD")
    git(client, "push", "-o", "agent=operator", "origin", "HEAD:refs/for/main")
    change = "operator-" + submitted
    (client / "work.txt").write_text("later dirty work\n")
    (client / "untracked.txt").write_text("later untracked work\n")
    before = git(client, "status", "--porcelain=v1")
    request = subprocess.Popen([str(binary), "--json", "integrate", str(authority), change,
                                "--generation", "1", "--mode", "textual", "--wait-seconds", "60"],
                               env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    await_file(control / "ready")
    interrupted_at = time.monotonic()
    milestones = {"engine_kill_seconds": 0.0}
    engine.kill()  # Only this walkthrough's explicitly created engine.
    engine.wait(timeout=10)
    request_stdout, request_stderr = request.communicate(timeout=20)
    assert request.returncode != 0
    (out / "interrupted-request.json").write_text(json.dumps({"exit": request.returncode,
        "stdout": request_stdout, "stderr": request_stderr}, indent=2) + "\n")
    slot = authority / ".mai/validators/slot-0"
    await_file(slot / "cleanup-receipt.json")
    milestones["receipt_observed_seconds"] = time.monotonic() - interrupted_at
    receipt = json.loads((slot / "cleanup-receipt.json").read_text())
    (out / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    inspected = json.loads(mai("validator", "inspect", authority, "--slot", "0"))
    claim = inspected["claim_id"]
    assert inspected["cleanup_receipt"]
    recovered = json.loads(mai("validator", "recover", authority, "--slot", "0", "--claim", claim))
    milestones["recovery_completed_seconds"] = time.monotonic() - interrupted_at
    repeated = json.loads(mai("validator", "recover", authority, "--slot", "0", "--claim", claim))
    assert recovered == repeated
    assert git(authority / "authority.git", "rev-parse", "trunk") == baseline
    mai("status", authority)
    interrupted_state = json.loads(mai("show", authority, change, "--generation", "1"))
    (control / "release").write_text("release")
    landed = json.loads(mai("integrate", authority, change, "--generation", "1", "--mode", "textual", "--wait-seconds", "60"))
    milestones["successful_landing_seconds"] = time.monotonic() - interrupted_at
    final_state = json.loads(mai("show", authority, change, "--generation", "1"))
    assert (control / "invocations").read_bytes() == b"runrun"
    assert git(client, "status", "--porcelain=v1") == before
    assert (client / "work.txt").read_text() == "later dirty work\n"
    assert (client / "untracked.txt").read_text() == "later untracked work\n"
    final_trunk = git(authority / "authority.git", "rev-parse", "trunk")
    assert final_trunk != baseline
    assert git(authority / "authority.git", "rev-parse", "trunk^") == baseline
    mai("stop", authority)
    (out / "outcome.json").write_text(json.dumps({"inspection": inspected, "recovery": recovered,
        "repeated_recovery_identical": recovered == repeated, "interrupted_state": interrupted_state,
        "landed": landed, "final_state": final_state, "baseline": baseline, "final_trunk": final_trunk,
        "gate_invocations": 2, "agent_work_preserved": True, "elapsed_milestones": milestones,
        "measurement_scope": "One small disposable functional fixture; not a production MTTR or scale benchmark."}, indent=2) + "\n")
finally:
    (control / "release").write_text("release")
    if engine.poll() is None:
        engine.kill()
        engine.wait(timeout=10)
    if request is not None and request.poll() is None:
        request.kill()
        request.wait(timeout=10)
    engine_log.close()
    transcript.close()
print(out)

#!/usr/bin/env python3
"""Two-agent continuation/conflict/correction on fresh disposable repositories."""
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
parser.add_argument('--repo', type=Path, default=Path(__file__).resolve().parents[2])
parser.add_argument('--binary', type=Path)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--temp-root', type=Path, required=True)
args = parser.parse_args()
repo = args.repo.resolve()
source_binary = (args.binary or repo / 'target/debug/mai').resolve()
out = args.output.resolve()
out.mkdir(parents=True)
binary = out / 'mai'
shutil.copy2(source_binary, binary)
fixture = Path(tempfile.mkdtemp(prefix='mai-cont-', dir=args.temp_root.resolve()))
seed, authority, alice, bob, copied, failed, correction = [fixture / n for n in ('seed', 'authority', 'alice', 'bob', 'continued', 'conflicted', 'correction')]
seed.mkdir()
env = os.environ.copy()
env.update({'TMPDIR': str(args.temp_root.resolve()), 'GIT_CONFIG_NOSYSTEM': '1', 'GIT_CONFIG_GLOBAL': '/dev/null',
            'GIT_OPTIONAL_LOCKS': '0', 'GIT_AUTHOR_NAME': 'continuation-fixture', 'GIT_COMMITTER_NAME': 'continuation-fixture',
            'GIT_AUTHOR_EMAIL': 'fixture@localhost', 'GIT_COMMITTER_EMAIL': 'fixture@localhost'})
log = (out / 'commands.jsonl').open('w')

def run(argv, expected=0):
    result = subprocess.run([str(a) for a in argv], env=env, capture_output=True, text=True, timeout=90)
    log.write(json.dumps({'argv': [str(a) for a in argv], 'exit': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr}) + '\n')
    log.flush()
    if expected is None:
        assert result.returncode != 0, argv
    else:
        assert result.returncode == expected, (argv, result.returncode, result.stdout, result.stderr)
    return result.stdout.strip()

def git(root, *args):
    return run(['git', '-C', root, *args])

def mai(*args, expected=0):
    return run([binary, '--json', *args], expected)

def snapshot(root):
    result = {}
    for path in sorted(root.rglob('*')):
        name = str(path.relative_to(root))
        if path.is_symlink():
            result[name] = {'symlink': os.readlink(path)}
        elif path.is_file():
            result[name] = {'sha256': hashlib.sha256(path.read_bytes()).hexdigest(), 'mode': path.stat().st_mode & 0o777}
        else:
            result[name] = {'directory': True}
    return result

def commit(root, message):
    git(root, 'add', '.')
    git(root, 'commit', '-m', message)
    return git(root, 'rev-parse', 'HEAD')

def submit(root, agent):
    oid = git(root, 'rev-parse', 'HEAD')
    git(root, 'push', '-o', 'agent=' + agent, 'mai', 'HEAD:refs/for/main')
    return agent + '-' + oid

def land(change, expected=0):
    return json.loads(mai('integrate', authority, change, '--generation', '1', '--mode', 'textual', '--wait-seconds', '60', expected=expected))

identity = {'source_head': git(repo, 'rev-parse', 'HEAD'),
            'tracked_diff_sha256': hashlib.sha256(subprocess.check_output(['git', '-C', str(repo), 'diff', '--binary'])).hexdigest(),
            'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'binary': str(binary),
            'platform': list(os.uname()), 'git_version': run(['git', '--version']), 'rustc_version': run(['rustc', '--version']),
            'fixture': str(fixture), 'measurement_scope': 'One disposable functional walkthrough; no production performance claim.'}
(out / 'identity.json').write_text(json.dumps(identity, indent=2) + '\n')
git(seed, 'init', '-b', 'main')
(seed / 'shared.txt').write_text('base\n')
(seed / 'parent.txt').write_text('baseline\n')
(seed / 'gate.sh').write_text('test -f "$MAI_CANDIDATE_ROOT/shared.txt"\n')
(seed / 'mai.toml').write_text('version=1\n[check]\nprogram="sh"\nscript="gate.sh"\ntimeout_ms=5000\noutput_tail_bytes=1024\n[validators]\nslots=1\nport_base=42000\nports_per_slot=10\n')
commit(seed, 'Seed two-agent fixture')
mai('init', authority, '--from', seed)
mai('status', authority)
try:
    mai('receive', 'enable', authority, '--max-input-bytes', '1048576', '--max-file-bytes', '65536', '--max-changed-files', '32')
    mai('workspace', authority, 'alice', alice)
    for root in [alice]:
        git(root, 'config', 'user.name', 'Alice fixture')
        git(root, 'config', 'user.email', 'alice@localhost')
    (alice / 'parent.txt').write_text('accepted Alice parent\n')
    accepted = commit(alice, 'Alice parent')
    change = submit(alice, 'alice')
    first = land(change)
    assert first['kind'] == 'landed'
    (alice / 'shared.txt').write_text('Alice intent\n')
    original_suffix = commit(alice, 'Alice later local work')
    (alice / '.git/info/exclude').write_text('ignored-build.bin\n')
    (alice / 'ignored-build.bin').write_bytes(b'preserve ignored source material\x00')
    before = snapshot(alice)
    mai('workspace', authority, 'bob', bob)
    git(bob, 'config', 'user.name', 'Bob fixture')
    git(bob, 'config', 'user.email', 'bob@localhost')
    (bob / 'parent.txt').write_text('Bob updated accepted parent\n')
    (bob / 'bob.txt').write_text('Bob additional trunk work\n')
    commit(bob, 'Bob accepted edits')
    second = land(submit(bob, 'bob'))
    started = time.monotonic()
    continued = json.loads(mai('continue', authority, alice, copied, '--change', change, '--generation', '1', '--skip-bootstrap'))
    continuation_seconds = time.monotonic() - started
    assert (copied / 'parent.txt').read_text() == 'Bob updated accepted parent\n'
    assert (copied / 'bob.txt').read_text() == 'Bob additional trunk work\n'
    assert (copied / 'shared.txt').read_text() == 'Alice intent\n'
    assert not (copied / 'ignored-build.bin').exists()
    assert git(copied, 'rev-parse', 'HEAD^') == second['commit']
    assert git(copied, 'rev-list', '--count', second['commit'] + '..HEAD') == '1'
    assert snapshot(alice) == before
    copied_before = snapshot(copied)
    repeated = mai('continue', authority, alice, copied, '--change', change, '--generation', '1', '--skip-bootstrap', expected=None)
    assert snapshot(copied) == copied_before and snapshot(alice) == before
    (bob / 'shared.txt').write_text('Bob overlapping intent\n')
    commit(bob, 'Bob overlapping edit')
    third = land(submit(bob, 'bob'))
    rejected = land(submit(copied, 'alice'), expected=3)
    assert rejected['kind'] == 'rejected', rejected
    replay_failure = mai('continue', authority, alice, failed, '--change', change, '--generation', '1', '--skip-bootstrap', expected=None)
    assert failed.is_dir() and (failed / '.mai-continuation-incomplete').exists()
    assert snapshot(alice) == before
    mai('workspace', authority, 'alice', correction)
    git(correction, 'config', 'user.name', 'Alice fixture')
    git(correction, 'config', 'user.email', 'alice@localhost')
    (correction / 'shared.txt').write_text('Bob overlapping intent + Alice intent\n')
    commit(correction, 'Reconcile both intended edits')
    final = land(submit(correction, 'alice'))
    assert final['kind'] == 'landed' and final['revision'] == 4, final
    assert snapshot(alice) == before
    assert (failed / '.mai-continuation-incomplete').exists()
    (out / 'source-snapshot.json').write_text(json.dumps(before, indent=2) + '\n')
    (out / 'outcome.json').write_text(json.dumps({'accepted_submission': accepted, 'original_suffix': original_suffix,
        'continuation': continued, 'continuation_seconds': continuation_seconds, 'repeated_invocation': repeated,
        'integration_rejection': rejected, 'replay_failure': replay_failure, 'final_landing': final,
        'source_all_bytes_modes_preserved': True, 'accepted_parent_excluded': True, 'additional_trunk_edits_preserved': True,
        'failed_destination_preserved': True, 'ignored_source_preserved_not_copied': True}, indent=2) + '\n')
finally:
    mai('stop', authority)
    log.close()
print(out)

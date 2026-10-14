# Your first mai revision

`mai` has one authority and an independent workspace per agent. Start in a disposable local directory. You can use it without GitHub, a daemon, or a Python runtime.

## Install from source

You need Rust 1.85+, Git 2.36+, and a C compiler for bundled SQLite. Linux, macOS, and Windows have CI coverage; Windows remains experimental pending native filesystem qualification. On Windows, install Git for Windows and MSVC C build tools for the native MSVC target. No separate SQLite installation is needed.

```sh
git clone https://github.com/maida-ai/mai.git
cd mai
cargo install --path . --locked --bin mai
mai --version
```

The version command prints `mai 0.0.1`. Cargo installs to its bin directory, normally `~/.cargo/bin`; add that directory to your shell's PATH if `mai` is not found. Native prebuilt releases and a standalone installer are not provided yet.

To try the existing demos from your clone without installing:

```sh
cargo build --release --locked --bin mai
bash examples/demo.sh
```

Windows PowerShell:

```powershell
cargo build --release --locked --bin mai
./examples/demo.ps1
```

Both scripts create disposable projects and retain their location for inspection.

## Create, edit, submit

Run in a scratch directory. The authority starts with an empty snapshot at `r0`:

```sh
mai init demo
mai workspace demo alice
echo 'Hello from Alice' > demo/workspaces/alice/hello.txt
mai submit demo alice -m 'Add greeting'
mai show demo r1 hello.txt
```

The final command prints your greeting. `submit` creates `r1` and updates Alice's workspace. In PowerShell, the same commands work; file redirection follows that shell's encoding defaults. For explicit UTF-8 bytes, replace the `echo` line with:

```powershell
[IO.File]::WriteAllText('demo/workspaces/alice/hello.txt', "Hello from Alice`n")
```

Create two workspaces before submitting to see independent changes integrate from a shared base; the [front-page example](../README.md#two-agents-one-history) demonstrates this. Both files remain at HEAD even though the second agent started at r0. Same-file and ancestor/descendant conflicts reject conservatively; there is no automatic text merge.

## Review and repeat

```sh
mai diff demo alice
mai --json status demo
mai history demo
```

`diff` also stages pending changes. Keep each agent in its own workspace and stop editing during submission or refresh. Update a clean, stale workspace with `mai refresh demo alice`. Avoid `--discard` unless you intend to lose local edits and nonignored untracked files.

For contention-budget errors, retry the unchanged submission after backoff. For overlaps, preserve the draft and resolve from a fresh workspace. An accepted receipt with `workspace_updated:false` needs reconciliation before further work; see [submission and recovery](cli.md#submission-results-and-recovery).

## Import an existing project

Import the source repository's Git HEAD as the original snapshot `r0` into a new, empty target:

```sh
mai init imported --from /path/to/git-repository
mai workspace imported alice
mai show imported r0 README.md
```

The source history is retained, but mai numbering begins at the imported HEAD snapshot. No source repository is migrated in place. A Git URL can also be supplied explicitly; local paths need no network. Existing mai stores are covered by [compatibility](compatibility.md).

## Optional Git mirror

```sh
mai github-link demo /path/to/downstream.git
mai github-sync demo
```

The remote can be a local bare Git repository or a GitHub-compatible URL. It must be empty or have compatible ancestry. Sync pushes accepted main without force; divergence rejects safely. It does not create pull requests or import incoming remote changes. Mirroring is explicit and synchronous; basic source control works without it.

## Connect your agent

Give each agent a unique identity, create its workspace, and point its editor/tools there. Use `mai --json` for machine responses. Begin with the [integration prompt](../examples/agent-prompt.md); add leases and checks only where needed. A lease coordinates and blocks foreign publication while active; it is not filesystem access control.

The authority and metadata must stay on one local filesystem with working native locks. Do not use NFS/SMB or expose the store to hostile tenants. See [current limits](project.md#current-limits) before evaluating important data.

[All docs →](README.md)

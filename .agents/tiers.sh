# Tier -> harness and model. Human-owned; agents choose tiers, never models.
# Each run_<tier> receives the composed prompt file as $1 and runs inside the
# agent's worktree. Non-interactive runs need tools pre-approved (cargo, git):
# set that up in each harness's own config. Check flags against installed versions.

CHEAP_MODEL="gpt-6-luna"
MID_MODEL="gpt-6-sol"
HIGH_MODEL="gpt-6-astra"


CHEAP_DESC="opencode <provider/model>"
run_cheap() { codex exec --approve-for-me -m $CHEAP_MODEL "$(cat "$1")"; }

MID_DESC="codex exec <model>"
run_mid() { codex exec --approve-for-me -m $MID_MODEL "$(cat "$1")"; }

HIGH_DESC="claude -p opus"
run_high() { codex exec --approve-for-me -m $HIGH_MODEL "$(cat "$1")"; }

# Executor: interactive session. $1 = role file, $2 = opening message.
run_executor() { codex -m $HIGH_MODEL "Read $1; it defines your role for this session. $2"; }
# Harnesses without a system-prompt append flag: the first message carries the role.
# run_executor() { codex -m "<model>" "Read $1; it defines your role for this session. $2"; }

# Evaluator: non-interactive, on a different vendor than the executor.
# It must be able to build and run the project, but it should not commit.
run_evaluator() { codex exec --approve-for-me -m $HIGH_MODEL "Read $1; it defines your role. $2"; }
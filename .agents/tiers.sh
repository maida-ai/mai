# Tier -> harness and model. Human-owned; agents choose tiers, never models.
# Each run_<tier> receives the composed prompt file as $1 and runs inside the
# agent's worktree. Check flags against your installed CLI versions.

CHEAP_MODEL="gpt-6-luna"
MID_MODEL="gpt-6-sol"
HIGH_MODEL="gpt-6-astra"

# Agents must write to .git (worktrees, commits) and to runs/, which Codex's sandbox
# blocks. The bypass is only acceptable inside the container; .agents/run enforces that
# (MAI_IN_CONTAINER=1), so agents don't need to check it themselves.
CODEX_FLAGS="--dangerously-bypass-approvals-and-sandbox"

codex_exec() { local model=$1 prompt=$2; codex exec $CODEX_FLAGS -m "$model" "$(cat "$prompt")"; }

CHEAP_DESC="codex exec $CHEAP_MODEL"
run_cheap() { codex_exec "$CHEAP_MODEL" "$1"; }

MID_DESC="codex exec $MID_MODEL"
run_mid() { codex_exec "$MID_MODEL" "$1"; }

HIGH_DESC="codex exec $HIGH_MODEL"
run_high() { codex_exec "$HIGH_MODEL" "$1"; }

# Executor: interactive. $1 = role file, $2 = opening message.
# Codex has no system-prompt append, so the first message carries the role.
run_executor() { codex $CODEX_FLAGS -m "$HIGH_MODEL" "Read $1; it defines your role for this session. $2"; }

# Executor, unattended (.agents/run auto): same role, non-interactive.
run_executor_batch() { codex exec $CODEX_FLAGS -m "$HIGH_MODEL" "Read $1; it defines your role for this session. $2"; }

# Evaluator: should be a different vendor than the executor. Same-vendor is a known
# weakness until a second provider is wired in here.
run_evaluator() { codex exec $CODEX_FLAGS -m "$HIGH_MODEL" "Read $1; it defines your role. $2"; }

# Tier -> harness and model. Human-owned; agents choose tiers, never models.
# Each run_<tier> receives the composed prompt file as $1 and runs inside the
# agent's worktree. Check flags against your installed CLI versions.

# Two knobs per tier: model size and reasoning effort. Effort values depend on the model
# (typically minimal | low | medium | high | xhigh); check what yours supports.
CHEAP_MODEL="gpt-6-luna";  CHEAP_EFFORT="low"
MID_MODEL="gpt-6-sol";     MID_EFFORT="medium"
HIGH_MODEL="gpt-6-astra";  HIGH_EFFORT="high"

# Agents must write to .git (worktrees, commits) and to runs/, which Codex's sandbox
# blocks. The bypass is only acceptable inside the container; .agents/run enforces that
# (MAI_IN_CONTAINER=1), so agents don't need to check it themselves.
CODEX_FLAGS="--dangerously-bypass-approvals-and-sandbox"

codex_exec() {
  local model=$1 effort=$2 prompt=$3
  codex exec $CODEX_FLAGS -m "$model" -c model_reasoning_effort="\"$effort\"" "$(cat "$prompt")"
}

CHEAP_DESC="codex exec $CHEAP_MODEL effort=$CHEAP_EFFORT"
run_cheap() { codex_exec "$CHEAP_MODEL" "$CHEAP_EFFORT" "$1"; }

MID_DESC="codex exec $MID_MODEL effort=$MID_EFFORT"
run_mid() { codex_exec "$MID_MODEL" "$MID_EFFORT" "$1"; }

HIGH_DESC="codex exec $HIGH_MODEL effort=$HIGH_EFFORT"
run_high() { codex_exec "$HIGH_MODEL" "$HIGH_EFFORT" "$1"; }

# Executor: interactive. $1 = role file, $2 = opening message.
# Codex has no system-prompt append, so the first message carries the role.
run_executor() { codex $CODEX_FLAGS -m "$HIGH_MODEL" -c model_reasoning_effort="\"$HIGH_EFFORT\"" "Read $1; it defines your role for this session. $2"; }

# Executor, unattended (.agents/run auto): same role, non-interactive.
run_executor_batch() { codex exec $CODEX_FLAGS -m "$HIGH_MODEL" -c model_reasoning_effort="\"$HIGH_EFFORT\"" "Read $1; it defines your role for this session. $2"; }

# Evaluator: should be a different vendor than the executor. Same-vendor is a known
# weakness until a second provider is wired in here.
run_evaluator() { codex exec $CODEX_FLAGS -m "$HIGH_MODEL" -c model_reasoning_effort="\"xhigh\"" "Read $1; it defines your role. $2"; }

# Kit settings. Precedence: environment > .agents/config.local.sh > the defaults below.
#   one run:     CHEAP_MODEL=other-model .agents/run auto <slug> "<directive>"
#   your machine: put the same `: "${NAME:=value}"` lines in .agents/config.local.sh
#                 (gitignored; not seen by teammates)
# Every run records the values it used in runs/<id>/config.txt.

_cfg_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=/dev/null
[[ -f $_cfg_dir/config.local.sh ]] && source "$_cfg_dir/config.local.sh"

# Models: two knobs per tier, model size and reasoning effort. Effort values depend on
# the model (typically minimal | low | medium | high | xhigh); check what yours supports.
# The policy is many cheap attempts plus strong verification, not the smartest model
# everywhere. Astra is opt-in per run, as an explicit choice recorded in the run:
#   HIGH_MODEL=gpt-6-astra HIGH_PRICE="10.00 1.00 50.00" .agents/run auto <slug> "..."
#   EVAL_MODEL=gpt-6-astra EVAL_PRICE="10.00 1.00 50.00" .agents/run auto <slug> "..."
: "${CHEAP_MODEL:=gpt-6-luna}"
: "${CHEAP_EFFORT:=low}"
: "${MID_MODEL:=gpt-6.1-sol}"
: "${MID_EFFORT:=medium}"
: "${HIGH_MODEL:=gpt-6.1-sol}"     # the executor and high-tier agents
: "${HIGH_EFFORT:=high}"
: "${EVAL_MODEL:=gpt-6.1-sol}"     # same model as the executor: see FUTURE.md
: "${EVAL_EFFORT:=xhigh}"

# Prices per tier, USD per 1M tokens: "input cached-input output". Used to estimate
# spend from token counts, because models differ up to ~100x per token. Keep them in
# step with the provider's pricing page when you change models.
: "${CHEAP_PRICE:=0.10 0.10 0.50}"
: "${MID_PRICE:=2.00 0.10 10.00}"
: "${HIGH_PRICE:=2.00 0.10 10.00}"
: "${EVAL_PRICE:=2.00 0.10 10.00}"
# When a harness doesn't report cached input separately, assume this share was cached.
# Calibrate it against the provider's usage dashboard after a few runs.
: "${MAI_ASSUMED_CACHE_RATE:=0.8}"

# Delegation depth: executor = 1.
: "${MAI_MAX_DEPTH:=3}"

# Git identity for agent commits. %role% and %id% are filled in per agent: role is
# executor | planner | implementer | verifier; id is the agent id (the run id for the
# executor). (Not {braces}: they'd end the ${...} default early.)
# Commit metadata isn't authentication; see the README on attribution.
: "${MAI_GIT_NAME:=mai-%role% %id%}"
: "${MAI_GIT_EMAIL:=%id%@agents.mai.local}"

MAI_CONFIG_VARS=(CHEAP_MODEL CHEAP_EFFORT MID_MODEL MID_EFFORT HIGH_MODEL HIGH_EFFORT
                 EVAL_MODEL EVAL_EFFORT CHEAP_PRICE MID_PRICE HIGH_PRICE EVAL_PRICE
                 MAI_ASSUMED_CACHE_RATE MAI_MAX_DEPTH MAI_GIT_NAME MAI_GIT_EMAIL)
export "${MAI_CONFIG_VARS[@]}"

# Set the git author and committer for an agent: mai_git_identity <role> <id>
mai_git_identity() {
  local name=${MAI_GIT_NAME//%role%/$1} email=${MAI_GIT_EMAIL//%role%/$1}
  name=${name//%id%/$2}; email=${email//%id%/$2}
  export GIT_AUTHOR_NAME="$name" GIT_AUTHOR_EMAIL="$email"
  export GIT_COMMITTER_NAME="$name" GIT_COMMITTER_EMAIL="$email"
}
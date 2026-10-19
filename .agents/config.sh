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
: "${CHEAP_MODEL:=gpt-6-luna}"
: "${CHEAP_EFFORT:=low}"
: "${MID_MODEL:=gpt-6-sol}"
: "${MID_EFFORT:=medium}"
: "${HIGH_MODEL:=gpt-6-astra}"
: "${HIGH_EFFORT:=high}"
# The evaluator: ideally a different vendor than the executor (see FUTURE.md).
: "${EVAL_MODEL:=$HIGH_MODEL}"
: "${EVAL_EFFORT:=xhigh}"

# Delegation depth: executor = 1.
: "${MAI_MAX_DEPTH:=3}"

# Git identity for agent commits. %role% and %id% are filled in per agent: role is
# executor | planner | implementer | verifier; id is the agent id (the run id for the
# executor). (Not {braces}: they'd end the ${...} default early.)
# Commit metadata isn't authentication; see the README on attribution.
: "${MAI_GIT_NAME:=mai-%role% %id%}"
: "${MAI_GIT_EMAIL:=%id%@mai.maida.ai}"

MAI_CONFIG_VARS=(CHEAP_MODEL CHEAP_EFFORT MID_MODEL MID_EFFORT HIGH_MODEL HIGH_EFFORT
                 EVAL_MODEL EVAL_EFFORT MAI_MAX_DEPTH MAI_GIT_NAME MAI_GIT_EMAIL)
export "${MAI_CONFIG_VARS[@]}"

# Set the git author and committer for an agent: mai_git_identity <role> <id>
mai_git_identity() {
  local name=${MAI_GIT_NAME//%role%/$1} email=${MAI_GIT_EMAIL//%role%/$1}
  name=${name//%id%/$2}; email=${email//%id%/$2}
  export GIT_AUTHOR_NAME="$name" GIT_AUTHOR_EMAIL="$email"
  export GIT_COMMITTER_NAME="$name" GIT_COMMITTER_EMAIL="$email"
}

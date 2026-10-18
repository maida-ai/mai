# Ledger
| id | parent | role | model | task | spend | status | retries |
| -- | ------ | ---- | ----- | ---- | ----- | ------ | ------- |
| executor | — | executor | session model not exposed | scope, integrate report, verify, claim | dollar/token usage unavailable | complete; acceptance failures recorded | 0 |
| 175832-planner-16045 | executor | planner (cheap) | gpt-6-luna | smoke-planner | 10,115 tokens reported by harness; dollars unavailable | needs_scope | 0 |

Source: spawns.jsonl and agents/175832-planner-16045.log (tokens used).
Totals by tier: cheap 1 spawn / 10,115 reported tokens; mid 0; high executor usage unavailable.
Total versus $5 budget: dollar charges unavailable, so compliance cannot be confirmed. No estimated charges substituted; no further spawns.
Implementers: 0. No escalation or retry, preserving the single-planner directive.

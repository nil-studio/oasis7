# Playability L4 Summary Template

## Execution Result

Reuse the existing [RunResult contract](https://github.com/nil-studio/oasis7/blob/main/doc/testing/governance/agent-runtime-self-playtest.design.md#6-接口与数据合同) and [source-to-scene records](https://github.com/nil-studio/oasis7/blob/main/doc/testing/governance/agent-runtime-self-playtest.design.md#selftest-source-scene). This is a summary projection in the existing L4 artifact, not a new approval record. Reference the current attempt even when it is blocked; keep earlier attempts and their reviewed conclusions separately.

- `current_attempt`:
- `run_result_ref`:
- `candidate_and_build_ref`:
- `viewer_build_profile`:
- `execution_mode`:
- `actual_session_player_ref`:
- `phase`: `preparing`
- `stop_reason`:
- `blocking_reason`:
- `failure_signature`:
- `artifact_refs`:
- `cleanup_result`: not yet recorded
- `retained_gap`:

Record actual values and authorized, redacted references; do not copy credentials or infer an authenticated driver from a different headed session. `phase` follows `preparing -> ready -> playing -> verifying -> reported -> cleanup`; any phase may end blocked/aborted with a reason. Preserve each selected item's `execution_state` (`planned / located / implemented / executed`), `test_tier`, `evidence_level` and `verdict` (`pass / fail / blocked / inconclusive / not_applicable`) in the linked RunResult. Missing prerequisites are blocked; unknown results are inconclusive; an unexecuted play session is neither a gameplay pass nor a gameplay fail.

## Observed Facts

- Actions actually executed and their results:
- State changes and their source:
- Evidence references and collection times:
- What the fixed route or selected driver did not observe:

Automatic collection may fill these facts. A tick/progress change, ACK or matching player ID does not establish the five player-leverage criteria or a desire to continue. See [the judgment contract](https://github.com/nil-studio/oasis7/blob/main/doc/testing/prd.md#l4-evidence-judgment).

## Verdict Summary

- `review_status`: `unreviewed`
- `reviewed_attempt`:
- `reviewed_by`:
- `l4a_synthetic_verdict`: `unreviewed`
- `l4b_agentic_verdict`: `missing`
- `optional_internal_human_corroboration`: `not_run`
- `combined_stage_recommendation`: `unreviewed`

The existing field names remain compatible with copied L4 summaries. `unreviewed` is an empty review placeholder, not an execution verdict or a stage decision. Producer/QA must bind a completed review to `reviewed_attempt` and its evidence before setting `review_status=reviewed`. After that review, use the existing L4A values `synthetic_continue_likely / synthetic_continue_unclear / synthetic_continue_unlikely`, L4B values `agent_continue_observed / agent_continue_mixed / agent_continue_not_observed / missing`, corroboration values `not_run / corroborates_l4b / mixed / contradicts_l4b`, and stage values `go / watch / hold / block`.

If actual play evidence is missing, keep L4B `missing`; a build/login blocker cannot become `agent_continue_not_observed`. Thin or unreviewed evidence does not support a continue claim. Old runner output remains `unreviewed` even if it prefilled a score or verdict. The current code may still overwrite those fields: preserve reviewed artifacts, collect a rerun in a fresh attempt, and review its facts separately. [#4438 was closed as not_planned after the documentation handoff](https://github.com/nil-studio/oasis7/issues/4438#issuecomment-6127544475) to PR #4641. It is a historical source, not the current execution ledger. Closure did not implement this separation or complete the retained L1/L2/L3 behavior and acceptance obligations.

## Required Summary Questions

- What does `L4A` currently prove?
- What does `L4B` currently prove?
- How close is the current `L4B` evidence to the human-level evaluation effect you want?
- Where do `L4A` and `L4B` agree?
- Did any optional internal human corroboration confirm or challenge `L4B`?
- Which claim is still out of scope until `L5` real-human or live evidence exists?

## Follow-up

- Required reruns:
- Required role follow-ups:
- Required agent-play follow-ups:
- Optional internal human corroboration follow-ups:
- Required `L5` follow-ups:
- Claim boundary reminder:

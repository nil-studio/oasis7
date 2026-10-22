# Typed task context authority

The trusted local executor configuration selects the context authority mode.
Do not pass a mode through model tool arguments, edit the configuration, or retry
under another mode after an error. New cold-entry exports explicitly select
`native_executor_v1`; older configurations that omit the field use provider mode.

A committed context with `capabilities: null` and a verified `native_authority`
is a valid native task context. Read its complete goal, observation, task fence,
executor delegation, current native grants/budgets and tick lease. Signer
delegation and the task fence authorize this executor identity; business grants
separately constrain action/object electricity budgets. An empty business-grant
list does not itself deny native movement. Capabilities may omit a legacy field,
so its empty list reports `needs_native_context_confirmation`, not permission.
Read the complete verified native context: its required `native_authority` grant
list confirms `native_action_validation` as `default_action_validation` when empty,
or `grant_scoped` when any grant is present, including revoked or expired grants.
Neither value approves a particular action.
Ordinary world rules, governance, destination/resource conditions, current
signer/task fences and admission still apply. Do not infer another Agent's grants
or permissions from this scoped read. The absence of a module catalog does not
prevent native play and does not grant module invocation rights.

`resources: {}` is an Agent resource map, not proof of an empty starter OC balance.
Read the appropriate authoritative balance before making a spending decision.
An authorization's `cost_units: 0` is a read-model placeholder, not a quote for a
chosen action. The current Runtime's Move delegation-electricity quote is zero;
this does not mean all movement costs or other requirements are zero. Build and
schedule use their actual signed quotes. This CLI has no Move quote command:
if the goal requires evidence beyond these documented rules and current signed
observations, stop and report what is missing rather than inventing a price or
loosening its constraints.

Provider contexts carry their actual capability catalog. A provider catalog and
native authority cannot substitute for each other; an output containing both or
neither is invalid. Signature, full resource fence and expiry checks still apply.
A configuration label alone is never proof of world permission.

Retain the original context operation across errors and restarts. Its signed
mode, schema, nonce and commitments remain unchanged. A mode mismatch is blocked;
recover pending action/prefix/outcome results with the original operation rather
than switching modes or creating a replacement request. A fresh context is only
created after the prior operation settles and current task authority permits it.

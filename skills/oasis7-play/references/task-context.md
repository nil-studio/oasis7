# Typed task context authority

The trusted local executor configuration selects the context authority mode.
Do not pass a mode through model tool arguments, edit the configuration, or retry
under another mode after an error. New cold-entry exports explicitly select
`native_executor_v1`; older configurations that omit the field use provider mode.

A committed context with `capabilities: null` and a verified `native_authority`
is a valid native task context. Read its complete goal, observation, task fence,
executor delegation, current native grants/budgets and tick lease. Use only the
supported native actions allowed by those current grants. The absence of a module
catalog does not prevent native play and does not grant module invocation rights.

Provider contexts carry their actual capability catalog. A provider catalog and
native authority cannot substitute for each other; an output containing both or
neither is invalid. Signature, full resource fence and expiry checks still apply.
A configuration label alone is never proof of world permission.

Retain the original context operation across errors and restarts. Its signed
mode, schema, nonce and commitments remain unchanged. A mode mismatch is blocked;
recover pending action/prefix/outcome results with the original operation rather
than switching modes or creating a replacement request. A fresh context is only
created after the prior operation settles and current task authority permits it.

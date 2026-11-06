# Restricted remote account tools

Use this profile only when the trusted host explicitly supplies an authenticated remote connector and the fixed `dev_test_accounts_call` tool. The connector owns its HTTPS origin and authentication; model tool arguments cannot supply credentials, URLs, paths or principal identity. Do not request secrets, raw HTTP, signing, account management or fallback tools.

Use the [common account/session workflow](dev-test-accounts.md) for the exact closed method set, immutable session binding, full goal/context checks, operation identity and canonical result rules. That reference's local UID/stdio installation paragraphs apply to local transport only; this remote profile uses the host's preconfigured connector. Model requests and backend business responses otherwise have the same meaning.

Remote disconnection does not prove rejection. Retain the original request ID, body, account/session binding and any observation. An `unknown` result is unresolved: obtain a current authorized session when needed and use `lookup` for the original operation. Never invent a replacement Move ID, update the original body or silently change account. Every admitted business request consumes the shared tool-call budget, including same-ID recovery; reconnecting does not create a fresh budget.

Require the complete `active_goal.content` goal text and constraints plus its verified source binding before choosing a move. An allowlisted destination is only a restriction, not the task goal. Stop when goal content/source, authorization or a canonical original is unavailable. Root transport success is not world commitment; distinguish unknown, pending, rejection and committed outcomes.

Discovery/initialization authenticates the connector but does not read accounts or execute business tools. Only valid tool calls reach the common private service, which reauthenticates and performs cumulative admission. The remote adapter does not maintain a separate grant, session, operation or usage journal.

A local trusted TLS proxy fixture is protocol evidence only. It does not certify a public deployment, actual Dot connector permissions, external model isolation or gameplay. Those remain separately blocked until an authorized real combination and its canonical world evidence exist; this Skill grants no model spending or deployment authority.

# Native identity prerequisite

This reference describes only the separate trusted local owner-control surface.
The executor profile in the main Skill remains prepared-world. It does not
support autonomous cold world entry, create canonical Agent ownership or bind a
task. OpenClaw and Codex cold-entry combinations remain unverified.

A trusted local user/launcher can use the supplied executable's
`--identity-config <identity.json>` entry. Configuration pins the HTTPS identity
origin, service/environment and audience. Private state uses password-encrypted
identity storage on supported Unix hosts. Unlock input comes only from the
trusted local control surface; never ask for a password, seed, cookie or token
in chat, model tools, argv or environment. A model must not read the vault or
the underlying request originals. Missing local unlock/permission is a human
confirmation stop, not permission to create a plaintext key.

The local control surface explicitly initializes the identity once, then uses
`identity-operation-id` to choose and retain a stable operation before
`identity-register --operation <id> --name <name>`. Only advertised, enabled
registration is allowed; never change deployment flags or retry through a
private/admin endpoint. Register creates an account receipt only. A separate
`identity-login --operation <different-id>` establishes a session for the same
key. `identity-status --operation <id>` returns nonsecret historical progress;
confirmed history is not current authorization.

After timeout or restart, resume the original command and operation. The SDK
reuses the stored canonical body, cookie/CSRF and signature. Never replace the
operation, key or nonce to bypass an unknown result. A consumed login whose
session cookie was lost returns `action_required/explicit_fresh_login`; only
the trusted caller may explicitly request a fresh login. A confirmed login
still does not establish world admission, Agent ownership, starter eligibility,
task/goal publication or an executor grant. Stop until those real prerequisites
are supplied by a supported world-entry profile. Never use the identity key as
an executor signer or approve your own owner permissions.

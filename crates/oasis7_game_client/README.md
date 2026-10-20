# Native game client

The SDK and `oasis7_game_client` executable share authenticated World Service reads,
private task Context and quotes, signing, immutable operation recovery, and bounded HTTP.
They depend on `oasis7_client_api`, `oasis7_agent_api` and serialization/crypto/transport
libraries. They do not link the Oasis7 application, World, Viewer, or a model SDK.

```sh
env -u RUSTC_WRAPPER cargo build -p oasis7_game_client --bin oasis7_game_client
./target/debug/oasis7_game_client --help
```

The executable keeps the existing device configuration and commands documented by
[the player Skill](../../skills/oasis7-play/SKILL.md). The signer file and encrypted
operation cache retain their existing formats. Secure local file storage currently
requires Unix; unsupported platform guarantees fail closed.

Rust callers use `device_client::Client::open` for the same durable operation flow as
the CLI, or `WorldServiceTaskClient` for typed native task protocol calls. The lower
level `WorldServiceTransport` verifies request-bound service envelopes; callers of
its generic methods must also validate world, scope, evidence and business correlation.
A service signature is not an independent consensus or durable receipt proof.

## Native machine identity

Unix callers can use `identity_client::IdentityClient` and the same executable's
`--identity-config <identity.json>` entry for explicit identity creation,
registration, login and original-operation recovery. This owner-control entry is
separate from the executor's `--config` commands. It does not start a Runtime or
create an Agent/task. Full autonomous world entry remains unimplemented.

The nonsecret configuration pins an exact HTTPS origin, service/environment and
audience, plus an absolute private state directory without symlink components:

```json
{
  "endpoint": "https://identity.example",
  "identity_service_id": "configured-identity-service",
  "environment_id": "configured-environment",
  "audience": "https://identity.example",
  "state_directory": "/private/identity-state"
}
```

The trusted local user/launcher supplies the unlock passphrase as 12–1024 exact
bytes on private stdin. Never put it in arguments, environment, model tools or
chat. A terminal stdin is rejected to avoid echoing secrets. The SDK can instead
receive unlock bytes directly from a trusted local control surface. Windows
and other unsupported local-storage guarantees fail closed.

```sh
<client> identity-operation-id
<client> --identity-config <identity.json> identity-init
<client> --identity-config <identity.json> identity-register --operation <saved-id> --name <display-name>
<client> --identity-config <identity.json> identity-login --operation <different-saved-id>
<client> --identity-config <identity.json> identity-status --operation <saved-id>
```

These commands require private unlock input; the examples omit its transport
deliberately. Identity initialization is explicit and local only. It generates a
random login key in the existing Argon2id/XChaCha password envelope, with a
separate random key for encrypted request originals. It never writes a raw
identity key file or places a long-term key in the outbox. Keep the original
vault; missing/corrupt originals block recovery rather than generating a new
identity. This profile does not claim hardware or OS-keystore protection.

Registration requires both deployment-controlled registration switches and the
advertised machine capability. The SDK verifies pinned discovery/challenge
fields and recomputes signed bytes. It persists cookie/CSRF, canonical request
body, original challenge and proof before sending. Repeating an operation uses
the original request; changing its name/content conflicts. A registration
receipt creates no session or world authority. A login whose reply was lost is
reconciled with its original proof; if no original session cookie was saved,
`action_required/explicit_fresh_login` requires an explicit new login operation.
Registration closure preserves existing-key login. Neither local progress nor
an identity receipt proves world admission, canonical ownership, starter funds,
delegation, task activation or complete cold entry.

The separate Unix `--entry-config` owner surface now connects participant
registration/recovery, current canonical owner reads, starter eligibility/claim,
task creation, private goal preparation/publication and explicit independent
executor delegation/binding/stop/revoke. See the [trusted control reference](../../skills/oasis7-play/references/identity-entry.md)
for command options. These stages retain their own authority and durable
originals; current account/session plus signed world admission and current
resource fences are checked at the authenticated gateway.

`--executor-config <executor.json>` accepts the existing native game commands.
Its fields are `identity`, `login_operation`, `executor_vault_directory`,
`service_public_key`, complete `fence` and `state_directory`; unlock comes from
trusted private stdin. The signer is a distinct encrypted executor vault, and
raw signer config fields remain empty. Original writes are queried before an
exact retry. Historical lookup supplies current authorization around the
unchanged original payload, allowing reconciliation without resurrecting an
old executor fence. Private responses verify their original typed CBOR domains.

`broker-serve --profile <private-profile.json>` exposes bounded game commands
on a private Unix socket; `broker-call --socket <socket>` consumes a bounded
JSON request on stdin. Profiles pin Runtime version/binary, source attestation, Skill, client binary and
resource configs by SHA256 and keep resource executor/state/budgets separate.
The same trusted login identity may serve several resources. Vault-pinned
operation and budget directories fail closed after loss; they are not reset by
a restart or resource rename. Source
attestation is operator-approved build material. Configure actual Runtime OS
isolation separately: the model must see only socket and Skill. A socket owned
by the same user does not hide vaults. Profile validation is not cold-entry
acceptance, provider fee enforcement or an automatic Runtime launcher.

`lookup_typed` retains error categories for recovery decisions. After one
submission, the high-level client retries only the original result query for
transport failure, HTTP 503 or bounded Retry-After advice. It keeps forty attempts
within an eight-second query window; an already in-flight read retains its
configured timeout. Assurance/configuration failures stop immediately. Neither
query retry nor an unknown result creates a replacement action or signature.
Native Harness model-call reservations are separately encrypted and locked in
the local operation store; they do not become world task authority.

Private preparation may retry the exact durable signed Prepare after bounded
transport/503/cooldown failures; it never renews the stored Context or nonce.
Context reads and factory/recipe quotes retry only their route-specific snapshot
drift codes, at most three attempts starting within eight seconds. Context reads
reuse the same signed Get; quote refreshes preserve the full prepared identity,
selection and TTL. Other conflicts, permission failures and expired contexts stop.
The server rejects a Context read whose head changes during delivery, after checking
current authority and validity. This does not make a later Action atomic with the
read. Once an Action is cached, recovery retains that original; a rejected Action
is not re-signed or replaced to make progress.

Public JSON observations explicitly use `/v1/world/view-json`: the server applies
the same pinning and authorization as `/v1/world/view`, normalizes the projection to
JSON, then signs it in the distinct path domain. `VerifiedTaskWorldView` retains the
full authenticated projection and validates the consumed scope/runtime/task fields.
The old typed-CBOR signature must never be interpreted as a JSON-profile signature.
Servers without this profile fail explicitly; the client does not silently fall back.

The native task wire profile rejects legacy provider/browser/lease/continuation
request variants. It does not expand the CLI's supported gameplay or migrate the
legacy built-in executor. See [the API scope](../../doc/world-simulator/llm/external-agent-game-api.design.md)
and [implementation scope](https://github.com/eng-cc/oasis7/issues/4726).

The feedback foundation exposes bounded, service-authenticated scoped history.
`record_caller_persisted_feedback_acceptance` resolves a record inside a
`VerifiedTaskWorldView`; call it only after the caller durably commits its private
feedback effects and acceptance digest. The SDK records that signed caller
statement, not proof of private-memory correctness. It does not run or restore
Harness cognition, reorder private effects, or turn server delivery state into a
local acceptance marker. Only committed service feedback supports the existing
ACK protocol; other feedback remains readable without gaining ACK authority.

Acceptance and the original signed ACK share the existing encrypted operation
store. The view and journal bind the exact configured read authority; changing
that trust configuration cannot authorize acceptance or recovery of the old ACK. Immutable feedback-ID and session/sequence indexes must agree before an
ACK can be recovered or sent. A repeated acceptance must retain the same digest,
generation and feedback identity; recovery uses `recover_caller_persisted_feedback_ack`
without applying private effects again. `acknowledge_caller_persisted_feedback`
submits that exact cached payload and uses bounded original-result polling.
Unknown/Pending remains unresolved, and no authoritative ACK-completion marker is
persisted locally. A missing or conflicting index blocks sending; an interrupted
initial write can be retried with the exact original acceptance. This foundation
alone does not complete the legacy feedback/memory consumer migration.
## Bounded recovery of original operations

`Client` sends each public intent once and queries its immutable original after
uncertain delivery. Queries retry only transport failures, HTTP 503 and an already
identified shared cooldown, within 40 attempts and an eight-second window.
They never resubmit, re-sign, or allocate another operation ID. Rejected, expired,
configuration and assurance failures remain explicit; Unknown remains unresolved.

Private context preparation retries only the same durable signed prepare proof on
transport failures, HTTP 503 or an identified cooldown. This is the idempotent
private prepare protocol, not a public action submission retry. The service keeps
the immutable prepared winner and checks the original fence and lifetime again.

Factory/recipe quotes refresh the same prepared context only for the two fixed,
route-specific base/delivery drift codes. Context reads retry the exact same signed
GET proof only for their fixed delivery drift code. Both allow at most three
requests within eight seconds; an already in-flight bounded HTTP request may
finish after the window, but no further request starts. Generic conflicts, expiry,
permission and verification errors are not retried. Quote refresh preserves the
complete prepared input, context identity, operation, nonce, lifetime and business
selection; existing Action/Outcome records take priority over fresh quotes.

These checks do not eliminate the race between receiving an observation and
canonical action admission. A stale or otherwise rejected original remains rejected;
callers must retain its evidence and choose an explicit recovery policy. This SDK
slice does not implement a built-in model runner or claim autonomous play success.


## Native execution call budget

`OperationStore::initialize_native_run` fixes the local run identity and call
limit. `reserve_native_model_call` durably increments its encrypted counter under
a capability-relative file lock before a physical model request starts. Restart,
a lost response, or repeating an operation label does not refund that reservation.
Changed bindings, changed limits, missing counter files, and corrupt records fail
closed. This counter is local consumption accounting; it grants no World authority
and does not replace the session credential and token limits of a BYOK connection.

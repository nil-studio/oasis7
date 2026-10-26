# Cloudflare custody foundation

This implements the package foundation and native approval slice of [Issue #5076](https://github.com/nil-studio/oasis7/issues/5076), following the [adopted custody design](../../doc/p2p/blockchain/hosted-public-join-managed-identity-custody.design.md#cloud-custody-rust). It supports staging build and local contract verification, with execution, provisioning approval and managed-key routes disabled. Explicit configuration additionally enables same-origin management authentication, draft provisioning and conservative manager-wide custody freeze. It does not implement a usable custody service, native Agent transport or production custody.

## Packages and isolation

`oasis7_custody_core` owns pure checked accounting, immutable original-operation transitions, bounded session/heartbeat timing, monotonic recovery rejection, envelope authenticated-data encoding, typed native approval contracts and recoverable provisioning records. These primitives do not authorize requests or verify world receipts. A future adapter must apply current ACL, identity, PoP, fence and purpose checks and persist admission, budgets, originals and audit in one authority transaction before any dispatch. Deserializing a snapshot does not establish its freshness or authorize recovery.

`oasis7_custody_worker` uses four mutually exclusive role features. Each Wasm module exports only its role-local Durable Object classes: Identity `NativeSession`, Owner `AccountAuthority` and `ManagerAssociations`, Executor `ExecutorKeys`, Recovery Fence `RecoveryFence`. They use separate Worker names and SQLite migrations. Identity session storage and Executor storage remain disabled. Owner account storage accepts only private draft initialization and freeze; it cannot create a managed key, grant, native account/session or world action. Recovery Fence has only an internal read: it initializes its independent SQLite namespace frozen and retains the existing epoch across restarts. Public ingress does not proxy this internal read. All roles expose public `GET /v1/metadata`, reporting `execution_enabled=false`. The Identity management routes below require explicit pinned configuration. Unknown routes/query strings return 404 and other metadata methods return 405. No account data or credentials appear in these responses.

Identity has one fixed `OWNER_MANAGEMENT` binding to the Owner `OwnerManagement` named entrypoint. The separate Identity `IdentityNativeApproval` entrypoint exposes only typed proof verification to a configured trusted caller; it has no public HTTP verify route. No Executor binding can select these methods. There are no provisioned secrets, production environments or publish commands; this is not a working executor. Configuration separation is not evidence that Cloudflare publisher IAM identities have been provisioned or verified. Use four separately scoped publisher identities when deployment is explicitly authorized, and never grant Executor Identity/Owner/Fence publish or secret-read privileges.

## Reproducible local verification

Use the repository's pinned Rust toolchain (1.96.0) and wasm32 target. The platform lock is worker/worker-build 0.7.5, wasm-bindgen 0.2.127, Wrangler 4.152.0 and compatibility date 2026-10-25. Node 22 or newer is required; npm's lockfile pins the complete platform dependency tree. `undici` is overridden to 7.30.1 within its existing major version to avoid known advisories in the pinned Wrangler dependency graph.

From the repository root:

```sh
bash crates/oasis7_custody_worker/scripts/verify.sh
bash crates/oasis7_custody_worker/scripts/verify.sh --platform
```

The first command runs host tests, core Wasm checking, four role checks and no-role/all-pair negative builds. The platform command also installs the pinned builder if needed, performs four real Worker builds, Wrangler dry-runs, checks generated HTTP contracts, typechecks the TS consumer and tests actual Wasm HTTP handlers in local workerd. No Cloudflare credentials, deployment, model invocation or world writes are used.

`worker-build` fixes its staging path to `build/`; packaging is serialized by `.role-build-lock` and each complete result is copied to `artifacts/<role>/`. Concurrent packaging fails explicitly. Inspect any surviving lock before removing it after an interrupted build. Cargo uses the worktree-local target; wait for build locks without changing the target directory. Generated JS/Wasm and dry-run outputs are ignored local artifacts, not source or deployment evidence.

## Rust HTTP contract

The actual handler consumes `oasis7_custody_core::http` DTOs. A developer-only Rust test derives JSON Schema from those types and emits serialized fixtures; `scripts/contract.mjs` derives the TypeScript declarations from that schema. The exports include foundation metadata/errors and the actual native approval options, completion, verification and session-status DTOs. The existing signed-read bootstrap generator and World Service numeric-wire protocol are unchanged. Role business forwarding, PoP and native tool contracts remain unimplemented. Management draft/freeze DTOs are exported from the actual Rust handlers alongside the native metadata contracts.

```sh
node crates/oasis7_custody_worker/scripts/contract.mjs write
node crates/oasis7_custody_worker/scripts/contract.mjs check
```

`check` compares generated content without rewriting committed files. Public HTTP DTOs reject unknown fields. State primitives reuse the existing Agent API `DecimalU64` serializer for full-width persistence; AAD uses Rust CBOR bytes with separate value/wrapping domains and role/account/environment/key/version bindings. Full-width state strings are verified through actual SQLite TEXT storage and restart. The narrow synchronous SQL adapter has a workerd rollback fixture covering state and audit writes; this does not prove an AccountAuthority admission/outbox transaction, which is not integrated. WebCrypto encryption and world signing remain unimplemented.

## Owner approval handoff component

The Owner Rust-only component records a bounded approval handoff and historical one-time consumption. It binds the native proof, exact typed intent, pinned key, current manager/source versions, Owner-generated verification nonce, fence and deadlines. The synchronous SQLite transaction rechecks the current context and clock after asynchronous cryptography, and commits the original plus audit together. Retries preserve the original and never extend approval or source expiry. Record capacity is 256 with no nonce evidence eviction; a full book rejects new operations.

Every signature must pass strict Ed25519 validation and actual Worker WebCrypto `Ed25519` verification over the Rust canonical bytes. The transaction cache compares the complete immutable check material. There is no public authorization boolean or deserializable verified cache.

This component has no production fetch or RPC caller. Owner approval, key and dispatch routes remain disabled. The automatic native source channel below still needs deployed authority/IAM verification, and complete business forwarding must be established before approval enablement; a consumed historical record does not grant authority, create keys or dispatch. The `approval-harness` feature requires `owner` and exports a local-test-only class in a separate artifact with no Wrangler deployment configuration. The four production artifacts must not export that class. Test identities are public fixed fixtures, never operational credentials.

## Same-origin management and draft directory

Identity supports `GET /v1/management/session-status`, `POST /v1/account-provisions`, `GET /v1/account-provisions/{id}` and `POST /v1/manager-revocations`. Management status uses the native fixed `/custody/management-session-status` handler, which requires an actual Passkey management session. Its GET is read-only; POST validates the existing native Origin/CSRF rules before a draft or freeze request. The ordinary native `/custody/session-status` still accepts managed native-key sessions and cannot authenticate a manager.

The browser-facing Worker must use the same HTTPS origin as the native RP. Identity forwards only one `__Host-oasis7_sid` cookie and the original exact Origin/CSRF, never an arbitrary Cookie header or caller URL. It rejects redirects, oversized/noncanonical responses and mismatched namespace/epoch. Host-only native cookies are not a cross-domain login mechanism. Status reads do not renew a session. The native adapter credential is held only by Identity and authorizes only its fixed private proof verification path.

A draft request contains `api_version`, `client_request_id` and the fixed-route `ProvisionPlan`. Owner derives the manager from freshly authenticated metadata, checks its deployment bootstrap allowlist, and persists the stable association before initializing the Account DO. The complete original plan and configured world/genesis/branch/reorg/service-key pins are immutable and recoverable. An expired same-body retry only returns history. Directory initialization is not native identity creation or approval.

`manager-revocations` accepts only `api_version` and a stable `operation_id`, and freezes the current manager's entire custody directory. It does not revoke the native identity. The directory closes new association admission first, persists all account targets including pending initialization, then confirms at most eight accounts per call. Repeat the same original request to continue the bounded fan-out. Account freeze tombstones win over late initialization. State and audit commit together; restart preserves pending targets and exact receipts. Only a known empty draft inventory can currently be confirmed closed. Unknown rows, schemas, tables, triggers or unconfirmed canonical operations remain pending; future active-grant/outbox integration must supply its complete inventory and receipts. Legacy snapshots without the new complete association inventory are rejected rather than migrated into an unproven closed state.

Identity requires the complete public config group `CUSTODY_NATIVE_ORIGIN`, `CUSTODY_NATIVE_RP_ID`, `CUSTODY_NATIVE_NAMESPACE`, `CUSTODY_NATIVE_INSTANCE_EPOCH`, `CUSTODY_NATIVE_ENVIRONMENT`, `CUSTODY_NATIVE_AUDIENCE`, `CUSTODY_NATIVE_APPROVAL_KEY_ID`, `CUSTODY_NATIVE_APPROVAL_PUBLIC_KEY`, plus the separate `CUSTODY_NATIVE_ADAPTER_SECRET` secret. Absence disables the lane; partial configuration rejects. Owner directory configuration additionally requires the same namespace/instance/environment pins, `CUSTODY_BOOTSTRAP_MANAGERS` (JSON array), `CUSTODY_PROVISION_ROUTES` (JSON object), `CUSTODY_PROVISION_WORLD_IDENTITY` (complete `ApprovedWorldV1` JSON), and decimal `CUSTODY_POLICY_VERSION`. Routes must match the configured world ID. No operational values are committed.

The production Identity/Owner deployment entries are fetch-only wrappers in `artifacts/<role>/entry.mjs`; the raw builder output exposes generated Rust RPC methods and must not be used as the deployment entry. Only the named wrapper methods select private Rust operations. JavaScript transports opaque strings and platform I/O; Rust owns authentication, DTOs, canonical digests and policy.

The platform test suite also starts a developer-only loopback native HTTP authority with real SQLite/WebAuthn fixture state. Its test-only outbound bridge maps the pinned HTTPS origin to that isolated local HTTP process; it does not verify a deployed TLS route or authorize a production HTTP fallback. Test credentials and persisted fixture artifacts are removed after testing.

## Remaining integration

Native identity approval/source/session metadata now exist behind explicit disabled-by-default configuration. The next slices must integrate those contracts with Owner, finish provisioning and role-local encryption; trusted host isolation, pairing/PoP and purpose credentials; Account DO transactional admission/outbox and independent Owner Stop/Revoke; native tools and management UI/Skill. Real Cloudflare IAM, AccountAuthority transactional admission/outbox, WebCrypto encryption/world-signing interoperability, restore/rotation and prepared-world CT-008 remain unverified. No fixture, metadata response or dry-run represents canonical world execution or Agent acceptance. dot remains blocked until a real trusted connector signing adapter and OAuth/MCP binding exist; there is no Bearer fallback.

These are REQ-CLOUD-CUSTODY-006 foundations with partial support for REQ-CLOUD-CUSTODY-003/004/005/007. Full AC-CLOUD-01–11 and CT-001–008 completion must be evaluated after the integrations, not inferred from these primitive tests. The Issue remains open and PRs use `Refs #5076`.

## Automatic native source revocation

The native authority saves typed, signed source events in the same transaction as account security/policy/status changes, credential state/version changes, or session revocation/security changes and shortened deadlines. Routine last-use updates and extended idle windows do not emit revocation. Polling never consumes an event; delivery order rotates durably so an unknown closure cannot starve other managers. Only a fixed Owner-signed closed acknowledgement for the exact event removes it from the pending delivery set. The native store retains at most 4096 full signed events, including acknowledged originals. Full custody history must never reject native logout or credential revocation: a participant then receives a permanently fenced reserved overflow record with an immutable first trigger and a Native-signed monotonic checkpoint. Later security changes update that bounded checkpoint and hash chain, not full per-event originals. A closed Owner acknowledgement confirms the permanent manager-wide authorization barrier; it neither individually confirms every compressed mutation nor removes the fence. Owner stores at most 256 source acknowledgements per manager; its capacity limit keeps custody pending and cannot roll back native security changes.

Identity has a scheduled fixed poll → named `OwnerRevocations.closeSourceEvent` → native ack path. It forwards no management cookie and has no caller-selected URL. Owner verifies the native signature with both strict Rust and WebCrypto, freezes admission before bounded account fan-out, and signs/stores its exact acknowledgement in the closure transaction. Multiple source events can confirm one permanent manager-wide closure without replacing its first notice. Missing directory or legacy snapshots without an explicit complete inventory marker are frozen with unknown inventory and remain pending; an empty newly created record is not evidence of zero prior associations. Fresh login does not unfreeze this conservative directory.

The first bridge supports the real SQLite identity store only; enabling it on Tablestore is rejected. Once bridge metadata is registered, removing its configuration is rejected rather than silently suppressing events. Offline epoch maintenance requires verified signed acknowledgements and the writer fence; boolean attestations alone do not prove a completed bridge history.

The durable participant registry is explicit, namespace-scoped and limited to 256 existing unique manager accounts. Startup configuration must exactly match its persisted set; missing registry is never inferred to mean zero participants. Ordinary non-participants generate no custody events and require no Owner closure for identity epoch maintenance. All custody approval/metadata routes require participation and reject a permanent overflow fence; ordinary native login/logout/revocation routes remain available. Legacy approval-only configuration may start but cannot issue or verify custody approval without the registry. Registry changes and managed-native-account reverse association require a separate authorized workflow.

The native additional group is all-or-none: `OASIS7_CUSTODY_REVOCATION_POLL_SECRET`, `OASIS7_CUSTODY_REVOCATION_ACK_SECRET`, `OASIS7_CUSTODY_REVOCATION_OWNER_KEY_ID`, `OASIS7_CUSTODY_REVOCATION_OWNER_PUBLIC_KEY`, `OASIS7_CUSTODY_REVOCATION_OWNER_AUDIENCE`, `OASIS7_CUSTODY_REVOCATION_PARTICIPANTS` (JSON array of account IDs). Poll/ack secrets are distinct from each other, the approval verifier credential, session secret and native signing seed. Identity holds only the two corresponding `CUSTODY_NATIVE_REVOCATION_POLL_SECRET` / `CUSTODY_NATIVE_REVOCATION_ACK_SECRET` values. Owner holds `CUSTODY_REVOCATION_OWNER_SIGNING_KEY`, `CUSTODY_REVOCATION_OWNER_KEY_ID` and the fixed native public namespace/epoch/environment/audience/signing pins. Neither native verifier nor Executor receives this Owner private key. No operational values or deployments are created here.

This channel confirms custody directory/account authorization closure only. It neither signs nor dispatches canonical world Stop/Revoke and does not make unknown active authorization inventories safe to close. Owner approval, native managed identity/key creation, execution and CT-008 remain disabled or unverified as described above.

## Native identity authority configuration

The existing launcher identity v2 service now owns `/custody/approvals/options`, `/complete`, `/verify` and `/custody/session-status` under `/api/public/hosted-account/v2`. These are native handlers, not publicly enabled Worker routes. They use the same identity store, account/session/credential truth, WebAuthn verifier and browser Origin/CSRF protection. An authenticated Passkey management session is required for approvals; a managed native-key session cannot substitute. The read-only status exposes authoritative absolute/idle expiries as decimal strings without renewing the session. The separate `/custody/management-session-status` GET/POST requires a Passkey management session; its POST keeps the normal native Origin/CSRF protection.

The four base signer/adapter configuration values are required together; leaving all absent disables this lane. The complete revocation group and persisted participation registry above are additionally required before any custody approval can proceed. Partial configuration and identical signing, adapter or identity-session secrets fail startup:

- `OASIS7_CUSTODY_APPROVAL_SIGNING_KEY`: dedicated native Ed25519 seed as 32-byte hex, separately provisioned in the authorized secret environment.
- `OASIS7_CUSTODY_APPROVAL_KEY_ID`: fixed non-secret approval signing pin identifier.
- `OASIS7_CUSTODY_ACCOUNT_AUTHORITY_AUDIENCE`: fixed intended AccountAuthority audience.
- `OASIS7_CUSTODY_NATIVE_ADAPTER_SECRET`: independent 32-byte hex service credential, owned only by the Identity adapter and this native authority.

The service credential authorizes only `/verify`; that exact route rejects browser Origin/Cookie and requires `Authorization: Bearer` containing the configured hex credential. It cannot log in, register identities or issue Owner/world operations. Approval complete requires a real UV WebAuthn assertion and saves the exact signed original before response. The 120-second ceremony, 30-second proof-consumption window, and originally approved plan deadlines remain separate. `inspect_origin` reports current source status after proof expiry without renewing or re-consuming authority. Lost complete responses retry the same original request; an expired proof cannot be regenerated from that retry.

Only the current configured signing key/pin is accepted. Rotation to an unknown key ID fails closed; overlapping multi-key rotation and complete role/session forwarding still require implementation/validation. The proof is not a world signature and does not itself consume an AccountAuthority approval or create a grant. Per-account ACL, registered-source lineage, Recovery Fence, budget, exact intent and final claim must still be verified by the future Owner integration. Primitive provisioning records cannot establish those facts or native/world receipt authenticity by themselves.

Native targeted regression entrypoint:

```sh
env -u RUSTC_WRAPPER cargo test -p oasis7_launcher_backend --bin oasis7_game_launcher hosted_identity::custody::tests --locked
```

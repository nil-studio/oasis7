# Cloudflare custody foundation

This is the first implementation slice of [Issue #5076](https://github.com/nil-studio/oasis7/issues/5076), following the [adopted custody design](../../doc/p2p/blockchain/hosted-public-join-managed-identity-custody.design.md#cloud-custody-rust). It supports staging build and local contract verification, with all business routes disabled. It does not implement a usable custody service, native Agent transport or production custody.

## Packages and isolation

`oasis7_custody_core` owns pure checked accounting, immutable original-operation transitions, bounded session/heartbeat timing, monotonic recovery rejection and envelope authenticated-data encoding. These primitives do not authorize requests or verify world receipts. A future adapter must apply current ACL, identity, PoP, fence and purpose checks and persist admission, budgets, originals and audit in one authority transaction before any dispatch. Deserializing a snapshot does not establish its freshness or authorize recovery.

`oasis7_custody_worker` uses four mutually exclusive role features. Each Wasm module exports only its own Durable Object class: Identity `NativeSession`, Owner `AccountAuthority`, Executor `ExecutorKeys`, Recovery Fence `RecoveryFence`. They use separate Worker names and SQLite migrations. The storage classes reject all requests with `503 not_ready`; no key, grant, session or world action can be created. Fetch exposes only public `GET /v1/metadata`, reporting `execution_enabled=false`. Unknown routes/query strings return 404 and other metadata methods return 405. No account data or credentials appear in these responses.

There are deliberately no cross-service bindings, secrets, production environments or publish commands yet. Adding a binding requires its narrow typed contract and role authentication; this baseline cannot be deployed as a working executor. Configuration separation is not evidence that Cloudflare publisher IAM identities have been provisioned or verified. Use four separately scoped publisher identities when deployment is explicitly authorized, and never grant Executor Identity/Owner/Fence publish or secret-read privileges.

## Reproducible local verification

Use the repository's pinned Rust toolchain (1.96.0) and wasm32 target. The platform lock is worker/worker-build 0.7.5, wasm-bindgen 0.2.127, Wrangler 4.152.0 and compatibility date 2026-10-25. Node 22 or newer is required; npm's lockfile pins the complete platform dependency tree. `undici` is overridden to 7.30.1 within its existing major version to avoid known advisories in the pinned Wrangler dependency graph.

From the repository root:

```sh
bash crates/oasis7_custody_worker/scripts/verify.sh
bash crates/oasis7_custody_worker/scripts/verify.sh --platform
```

The first command runs host tests, core Wasm checking, four role checks and no-role/all-pair negative builds. The platform command also installs the pinned builder if needed, performs four real Worker builds, Wrangler dry-runs, checks generated HTTP contracts, typechecks the TS consumer and tests actual Wasm HTTP handlers in local workerd. No Cloudflare credentials, deployment, model invocation or world writes are used.

`worker-build` fixes its staging path to `build/`; packaging is serialized by `.role-build-lock` and each complete result is copied to `artifacts/<role>/`. Concurrent packaging fails explicitly. Inspect any surviving lock before removing it after an interrupted build. Cargo's shared target is retained. Generated JS/Wasm and dry-run outputs are ignored local artifacts, not source or deployment evidence.

## Rust HTTP contract

The actual handler consumes `oasis7_custody_core::http` DTOs. A developer-only Rust test derives JSON Schema from those types and emits serialized fixtures; `scripts/contract.mjs` derives the TypeScript declarations from that schema. These are only the foundation metadata/error contracts. The existing signed-read bootstrap generator and World Service numeric-wire protocol are unchanged. This does not yet export the future approval, POST request-body, PoP or native tool contracts.

```sh
node crates/oasis7_custody_worker/scripts/contract.mjs write
node crates/oasis7_custody_worker/scripts/contract.mjs check
```

`check` compares generated content without rewriting committed files. Public HTTP DTOs reject unknown fields. State primitives reuse the existing Agent API `DecimalU64` serializer for full-width persistence; AAD uses Rust CBOR bytes with separate value/wrapping domains and role/account/environment/key/version bindings. This slice does not perform WebCrypto encryption, world signing, JS/SQL integer round-trips or transaction rollback verification.

## Remaining integration

The next slices must implement native identity approval/source/session metadata, recoverable provisioning and role-local encryption; trusted host isolation, pairing/PoP and purpose credentials; Account DO transactional admission/outbox and independent Owner Stop/Revoke; native tools and management UI/Skill. Real Cloudflare IAM, SQLite rollback, WebCrypto interoperability, restore/rotation and prepared-world CT-008 remain unverified. No fixture, metadata response or dry-run represents canonical world execution or Agent acceptance. dot remains blocked until a real trusted connector signing adapter and OAuth/MCP binding exist; there is no Bearer fallback.

These are REQ-CLOUD-CUSTODY-006 foundations with partial support for REQ-CLOUD-CUSTODY-003/004/005/007. Full AC-CLOUD-01–11 and CT-001–008 completion must be evaluated after the integrations, not inferred from these primitive tests. The Issue remains open and PRs use `Refs #5076`.

# Local Node Lifecycle Runbook

- 对应项目文档: GitHub Issue / GitHub Project
- 实现入口: `crates/oasis7_chain_runtime/src/host/node_lifecycle.rs`
- 整体计划与未完成项: [Persistent World Node Lifecycle](persistent-world-node-lifecycle.runbook.md)

审计轮次: 1

## Source availability and deployed-binary gate

[Code PR #4683](https://github.com/nil-studio/oasis7/pull/4683) is merged as `da890a9125a435969b82f5a14ab3c9143cc0690e`. At the fixed main baseline `b5476a616dd3d4dbc29f332ee42ed848903c1f60`, [command routing](https://github.com/nil-studio/oasis7/blob/b5476a616dd3d4dbc29f332ee42ed848903c1f60/crates/oasis7/src/bin/oasis7_chain_runtime/cli_contract.rs), [dispatch](https://github.com/nil-studio/oasis7/blob/b5476a616dd3d4dbc29f332ee42ed848903c1f60/crates/oasis7/src/bin/oasis7_chain_runtime/identity_receipt.rs), and the `node_lifecycle.rs` / `node_operations.rs` implementations are present. Waiting for #4683 to merge is no longer a prerequisite.

Before operating a deployed instance, verify that its binary was built from a source version containing these commands, and inspect its `--help` plus the relevant subcommand `--help` for `node` / `op` and every required option. Source availability and a documentation merge do not establish the installed binary's version, successful lifecycle execution, synchronization readiness or production acceptance. Keep the lifecycle regression and role-specific readiness checks below as separate verification obligations.

## Local capability and verification boundary

`node init/start/status/stop` manages one owner's local Unix process and its persistent identity. On Unix, the supported profiles are `compatibility_observer_v1`, `formal_public_observer_v1`, and `formal_full_state_sync_read_only_v1`; each has a separate trust and readiness contract. The compatibility observer uses the existing public-testnet v1 runtime. The formal public observer verifies and serves a public projection only. The formal full-state sync profile downloads and cold-restores the latest qualified full state under an issuer-signed node grant; it is read-only and does not grant proposal, validator, archive, or BFT membership rights. A public projection never means the node has the owner's private Snapshot or Journal.

A process being `running` means its owned supervisor exists. For the compatibility profile, `verified_sync_readiness` remains null. A formal public observer is ready only for its verified projection contract. A full-state sync node is ready only after it has verified the current formal cut, artifact closure and retained-history tree, cold-restored the Snapshot/Journal, and completed a second current-head read. An absent, expired or wrong-epoch grant leaves it unready. `formal_readiness` is role-specific read readiness, never validator or proposal readiness; startup or HTTP availability alone is not verified readiness.

## Trusted chain configuration

The chain owner supplies one existing-world ChainConfig. Ordinary node owners do not need SSH credentials, deployment inventory, or a centrally assigned node ID. The manifest and its registry must already be localized together using the supported observer authority contract. This command does not mint those authority documents.

```json
{
  "api_version": "oasis7.chain-config/v1",
  "runtime_profile": "compatibility_observer_v1",
  "world_id": "existing-world-id",
  "chain_id": "existing-world-id",
  "genesis_sha256": "<trusted existing genesis file SHA256>",
  "network_tier_manifest": "./localized-manifest.json",
  "manifest_sha256": "<trusted existing manifest file SHA256>"
}
```

The two SHA256 values authenticate material supplied out of band; downloading a file and computing its hash yourself is not independent trust. `genesis_sha256` is the material digest for the compatibility adapter, not a new formal WorldService proof or activation certificate. The v1 adapter requires equal existing world/chain IDs, `public_testnet`, `allow_observer_nodes=true`, and the manifest-bound existing registry. Unknown schemas fail before initialization effects.

Init captures absolute paths and byte hashes for the manifest, genesis, registry, release bundle and seed file. Start rechecks those pins and invokes existing runtime artifact/config validation. Change discovery addresses through node.json's explicit additional_peers, not by replacing the fixed chain anchor. Chain materials remain external files and are listed by status; retain their referenced artifact closure with the existing deployment bundle.

### Formal read-only chain configuration

Formal profiles use `oasis7.chain-config/v2` for `formal_public_observer_v1` and `oasis7.chain-config/v3` for `formal_full_state_sync_read_only_v1`. The chain owner supplies the World identity and genesis digest, service endpoint and service public key, an `InitialActivationTrustV1`, and a `FormalReadCheckpointV1` with its digest. These trust values must be pinned independently of the service response; a service-provided checkpoint or issuer key cannot establish its own authority.

For the v3 full-state profile, `formal_full_sync` also fixes the expected role (`state_sync` or `full_read_only`), polling interval, request timeout, and total transfer budget. Set `grant` to `null` in the chain configuration used by `node init`: the node key does not exist until initialization completes. The node identity is local and does not change the World or its formal authority. Keep the chain configuration and the issuer trust material in a private, backed-up operator-controlled location.

The full-state service serves only the current qualified cut. It binds every manifest and object request to the World, exact position and record ID, state root, active and finalizing authorities, current authority epoch, node public key, role, and lease window. Objects are sent in bounded chunks. A client independently checks the record closure and retained-history paths, restores the World, and rereads the head before becoming ready. This is full state synchronization for a read-only node, not an archive guarantee or permission to read arbitrary historical private snapshots.

The source service binds to loopback. To sync across machines, use an authenticated, encrypted tunnel or an HTTPS front end that terminates at the source's loopback service; configure the node with the local tunnel endpoint when using plain HTTP. Do not expose the private full-state routes directly on a public interface. The request body is signed by the node key and the service response is signed by the service key; signatures provide integrity and identity, not content confidentiality.

## Daily commands

```bash
oasis7_chain_runtime node init --chain ./existing-world.chain.json --data-dir /var/lib/oasis7/observer-a
oasis7_chain_runtime node start --config /var/lib/oasis7/observer-a/node.json
oasis7_chain_runtime node status --data-dir /var/lib/oasis7/observer-a
oasis7_chain_runtime node stop --data-dir /var/lib/oasis7/observer-a
```

Use an owned canonical directory path with no symlink components. Its parent must exist. Init creates the data directory and keys with 0700/0600 permissions. A local label is generated unless --node-id is supplied; it is not a validator ID. RPC defaults to 127.0.0.1:9080; --rpc-bind may select another explicit loopback port during initialization. Public RPC exposure is outside this adapter's current capability.

For a formal full-state sync node, initialize from its v3 chain configuration, install the signed grant after the node key has been created, then start and monitor it:

```bash
oasis7_chain_runtime node init --chain ./full-state-sync.chain.json --data-dir /var/lib/oasis7/full-sync-a
oasis7_chain_runtime node full-sync-grant --data-dir /var/lib/oasis7/full-sync-a --grant /secure/issuer-signed-node-grant.json
oasis7_chain_runtime node start --config /var/lib/oasis7/full-sync-a/node.json
oasis7_chain_runtime node status --data-dir /var/lib/oasis7/full-sync-a
```

The grant file is a `FormalNodeReadGrantV1` signed by the issuer key in the independently trusted initial activation. Its signed body binds the exact World/genesis, chain, this node's public key, role, monotonically increasing `grant_epoch`, exact `authority_epoch`, and `not_before`/`expires` Unix-millisecond times. The lease can last at most 30 days. The CLI does not create the grant or issuer key.

`node full-sync-grant` is a stopped-only operation: the target instance must be `stopped` or `failed`, and no unresolved start intent may exist. It checks the issuer signature, node key, World, chain, role, externally pinned authority floor, time window, and grant generation before atomically replacing the private node config. A replacement grant must advance `grant_epoch`; a node already running must be stopped before installation. Preserve the grant only in private operator storage.

Status reports `formal_full_state_sync`, `verified_sync_readiness`, `can_propose: false`, and the full-sync CAS/history paths. Wait for `verified_sync_readiness: "ready"`; a running process with a missing grant, expired lease, unavailable source, failed proof, or incomplete restore is not synchronized. The full-state node has no validator or proposal rights.

The issuer grant is bound to one exact authority epoch. After a legal recovery advances that epoch, obtain a new issuer-signed grant for the new epoch and install it while the node is stopped. An old-epoch grant cannot authorize the new current cut. A higher `grant_epoch` is only an install-time local ordering check: the source does not track a per-node grant high-water mark or revocation list. Reissuing a grant therefore does not immediately revoke a still-unexpired grant for the same node and authority epoch; that older signed lease may remain usable until its expiry, up to the 30-day maximum. Treat expiry as the revocation window.

Repeated init with the same input returns the existing configuration and identity. Changed world, chain, label or initialization config is rejected. An unbound nonempty directory requires explicit migration. Existing missing or damaged identities fail; they are never replaced with a new key to repair startup. An initialization intent fixes the original input before key provisioning so an incomplete first initialization can resume.

The local layout includes node.json, node-binding.json, initialization.json, keys/node-keypair.toml, state/world, state/records, state/cas, state/runtime, state/replication, logs/runtime.log, actions/, management-requests/, and instance.json. A full-state sync node additionally keeps verified content-addressed objects under `state/full-sync/blobs/` and its durable external-origin proof-chain head in `state/full-sync/formal-full-sync-history-head.json`; `formal-full-state-sync.json` is its current readiness status. The existing reward-prefixed execution bridge filenames beneath runtime remain execution state, even when the reward worker is disabled. Stop retains these files. Data destruction, directory migration, signer transfer, validator withdrawal, and world authority fencing are separate operations.

## Process and action ownership

Start persists a stable local action intent before launching the supervisor. Each receiver result binds an incarnation, monotonically increasing local generation and configuration digest. Local generation is not governance, consensus or authority epoch. PID fields are diagnostic only.

The supervisor retains the process ownership lock and controls only its owned unreaped Child handle. The runtime child inherits that same lock description. An abnormal supervisor death with a surviving runtime therefore blocks new ownership rather than allowing a second process. A private Unix socket authenticates requests/replies with the exact persisted incarnation/generation/config digest. Its volatile path is reported separately from the persistent data layout.

Stop queries the original instance, persists its action identity, asks that same receiver to drain/terminate, and waits for its durable terminal result. A timeout ends waiting and returns the original pending outcome; it neither invents success nor targets another generation. Repeating stop reconciles the same action. A late stop against a replaced incarnation is rejected. There is no PID-based fallback or automatic force-kill of an unauthenticated process.

If the ownership lock is retained but authenticated control is unavailable, status reports the blocker and readiness remains unknown. Resolve that exact instance with the local operator before any new start; do not delete process.lock, rewrite instance.json, or clear action records to bypass the fence. A supervisor crash with no durable terminal result also remains unknown rather than being silently declared stopped. This local fence does not replace protocol writer or signer fencing.

## Verification and remaining scope

The focused local tests cover identity preservation, wrong-world/material/role rejection, no world creation during init, actual subprocess stop and data retention, delayed stop against a replacement, and original-action reconciliation after stop timeout. Runtime parameter construction uses the existing parser and one explicit local layout; it does not recreate startup/governance/sync logic.

First start requires a complete RuntimeWorld checkpoint in the manifest-bound release bundle's `generated_world_sidecar`, with matching world/chain IDs and `genesis_ref` equal to the pinned genesis SHA256 (optionally prefixed `sha256:`). It installs the existing artifact's complete byte tree into a private stage, verifies its bundle hash/count/size and actual runtime world binding, fsyncs the files/directories and publishes it by rename. It does not generate a fresh world or rewrite its identity. Existing local world data is validated and retained. A simulator-only generation artifact, unbound legacy checkpoint, wrong genesis, partial world directory or corrupt tree is rejected; a separate explicit migration is required. This compatibility snapshot attachment is not a formal history or light-sync proof. The first-version copy budget is 100000 filesystem entries, depth 64 and 64 GiB of bytes; larger attachments are unsupported. Status also expands the bundle's external world/provenance paths. The legacy runtime still verifies those original bundle artifacts on restart, so retain them alongside the external chain materials when backing up or relocating this compatibility node; a data-directory-only backup is insufficient for this adapter.

The optional operating-domain OperationStore may reuse these local action and exact-instance boundaries. Management operation records remain orchestration receipts and cannot confer world authority. The current operation provider manages compatibility observer process actions; the formal full-state profile is operated through its direct local lifecycle commands. Automatic issuer provisioning, archive history service, validator authority, distributed signer safety and formal authority replacement/recovery remain separate capabilities.

## Optional local operating-domain operations

`op` is optional. Public node initialization, start and stop do not require a management store. This first native provider manages compatibility observer processes on this machine. Authority replacement, validator changes, archive migration, full-state grant installation, and formal recovery are unsupported operation kinds.

A request has `api_version: "oasis7.local-operation/v1"`, a caller-fixed `operation_id`, the exact `world_id` and `chain_id`, `kind` (`start`, `stop` or `restart`), absolute `config`, `expected_generation`, `expected_config_digest`, and `expected_instance` (null for start, the complete instance from node status for stop/restart). Node status reports the current configuration digest and local generation even before the first start. Restart uses an exact stop followed by a new generation under the same frozen configuration. Process start does not establish synchronization or validator readiness.

```sh
./target/debug/oasis7_chain_runtime op plan --request /absolute/request.json
./target/debug/oasis7_chain_runtime op submit --request /absolute/request.json --store /absolute/private-domain-store
./target/debug/oasis7_chain_runtime op status --store /absolute/private-domain-store --id operation-1
./target/debug/oasis7_chain_runtime op resume --store /absolute/private-domain-store --id operation-1 --revision 1
```

Plan has no effects. Submit persists the immutable request and reserves the domain's world and target. It does not dispatch a process. Resume advances one step, first querying the original derived action identity and then using the real local provider. A pending or unknown outcome retains the reservation. Read status and supply its current revision on the next resume. A same-ID submit returns the original record before reevaluating changed process facts; changing its payload is rejected. Different domains do not share this lock and still rely on the native exact-instance fence and protocol permissions.

`op cancel --store ... --id ... --revision ...` can cancel an unexecuted prepared request without affecting the process. After a dispatch intent it first persists the cancellation request and queries the original effect. When the native adapter proves that neither an action receipt nor a matching queued start pointer was admitted, cancellation can still succeed. An admitted termination or launched process cannot be retracted. Resume completes the original goal. These are local process boundaries, not formal committed decisions or world authority evidence.

The private store uses one writer lock, durable atomic JSON replacement, CAS revisions, immutable request digests and stable per-step action identities. Its explicit first-version limit is 4096 receipts and 4 MiB of metadata; capacity refusal preserves existing receipts. Do not delete unknown records to release resources. Native action receipts remain in the node data directory and are authoritative for these process effects if the management CLI is interrupted. Lost ledger recovery is explicit: `op recover --request /absolute/original-request.json --store /absolute/private-domain-store`. Before each native effect, the executor preserves the full immutable original management request and its digest in the node's private `management-requests/` directory. Recovery compares this original binding and queries the original native action receipts; it never starts or signals a process. A changed final goal (including replacing Restart with Stop after its stop step) is rejected. Existing admissions/request bindings without native receipts remain Unknown, with no invented receipt or resend. The immutable binding check/write and native admission share a short-lived node admission lock, preventing a concurrent action from acquiring a parent goal afterward. Old native actions lacking the original management request binding cannot be retroactively adopted. A stopped start receipt without durable historical start-success evidence remains Unknown during recovery; diagnostic PIDs do not establish that history. Completed stop recovery retains the old instance even when a replacement is running. A restart whose stop is proved complete and whose start has never been bound or admitted can be explicitly resumed forward after recovery. Remote deployment and formal authority/validator operations remain unsupported.

The real-runtime regression is `scripts/local-node-lifecycle.test.sh`. Supply `OASIS7_CHAIN_RUNTIME_BIN` (normal runtime executable) and `OASIS7_NODE_LIFECYCLE_TEST_BIN` (compiled runtime unit-test executable). A test-only helper exports a typed fixture checkpoint from existing repository world fixture data; the production CLI has no fixture or genesis generation command. The regression requires all three process generations to emit runtime ready, then verifies exact managed restart, original-operation retry, and identity/data retention. Ready remains a process startup observation, not verified sync or governance readiness.

# Legacy Observer Migration Runbook

- 对应项目文档: GitHub Issue / GitHub Project
- 实现入口: `crates/oasis7_chain_runtime/src/host/node_lifecycle/legacy_migration.rs`
- 方案: [Persistent World Node Lifecycle](persistent-world-node-lifecycle.runbook.md)

审计轮次: 1

## Source availability and deployed-binary gate

The implementation from [code PR #4683](https://github.com/eng-cc/oasis7/pull/4683) is merged and available in main. Source availability does not establish deployed-binary compatibility or a successful migration. Before running the commands below, verify that the deployed binary includes that implementation and that its `--help` plus the relevant subcommand `--help` confirm `legacy-migrate` / `legacy-run` and the required options. Retain the source, backup, verification and stopped-observer restrictions below; a documentation or code merge alone does not prove an actual migration.

## Source contract and supported boundary

`legacy-migrate` consumes the old flat runtime launch contract. Its `--config` argument inside `argv` remains the original keypair TOML. It does not accept a new local `node.json` as an old runtime source. Only a stopped legacy observer without auto-attest, provider authority, guarded authority or formal authority is supported. The original execution world must already have its world/chain/genesis binding and a complete Snapshot/Journal. Missing state or keys fail; the importer never provisions them.

Supply `source.json` with `api_version: oasis7.legacy-launch-source/v1`, an absolute `cwd`, the exact original `runtime_binary_sha256`, actual argument-tail `argv` after wrapper expansion, and literal `declared_env`. The importer requires the same runtime interpreter so omitted defaults cannot be silently reinterpreted by another binary. The migrated Plan and Launch also pin this SHA256; every `legacy-run` rejects a changed executable. A binary compatibility upgrade is separate work. Declared environment keys are limited to CONFIG_PATH, RUNTIME_ROOT, EXECUTION_BRIDGE_STATE, EXECUTION_WORLD_DIR, EXECUTION_RECORDS_DIR, STORAGE_ROOT, REPLICATION_ROOT, NODE_ID and REWARD_RUNTIME_SIGNER_NODE_ID. Their values must agree with actual effective arguments and cwd. This tool never sources or executes node.env. Preserve all actual traffic, identity, signer, peer and reward arguments in argv.

Snapshot verification loads a disposable copy using the original persisted format before applying any new release policy. Migration must preserve the recorded state rather than normalize it during comparison. A missing or null simulator mirror is valid for records without simulator actions; the physical cache must match the latest complete recorded mirror, or the canonical initial kernel if no record ever contained one. Partial mirror references, mismatched hashes and unrecoverable journals are refused.

The first adapter verifies complete hot execution records at heights 1 through the original committed head, up to height 10000. Missing cold-only history, unknown record formats, unbound worlds and missing replication guard/cursor material fail. This bounded adapter does not claim production validator migration, signer exclusivity, cold-history conversion or formal history qualification.

## Plan, copy and resume

```sh
oasis7_chain_runtime legacy-migrate plan --source /private/source.json --destination /private/migrated-observer --operation-id observer-move-1
oasis7_chain_runtime legacy-migrate apply --source /private/source.json --destination /private/migrated-observer --operation-id observer-move-1
oasis7_chain_runtime legacy-migrate resume --source /private/source.json --destination /private/migrated-observer --operation-id observer-move-1
oasis7_chain_runtime legacy-migrate status --source /private/source.json --destination /private/migrated-observer --operation-id observer-move-1
```

Plan is read-only for source/destination. It resolves actual paths using the existing parser, manifest defaults and runtime path resolver. The inventory owns distinct components for the complete runtime root, arbitrary keypair TOML, external world/records/CAS/replication and simulator mirror; nested components share their parent's copied tree rather than being merged or omitted. Reward-prefixed bridge/world/records/state/probe/report data stay in the byte inventory even when the reward worker is disabled. The plan reports existing root/peer/reward signer identities, original head, full canonical Snapshot/cognition digests, pending counters, feedback outbox and both CAS stores.

Apply acquires the same execution-world writer fence used by the old runtime. A live source is refused; no PID is signalled and no writer handoff is inferred. It first fsyncs an immutable parent-directory intent binding the original plan, then captures a fixed immutable plan and fixed private stage `.oasis7-legacy-migrate-<operation-id>` beside the destination. Files use bounded streaming hashes, private no-follow temporary copies, fsync and immutable publication. Every existing copied file is checked on resume, not trusted from a progress counter. Source or partial-target drift blocks continuation. The complete staged inventory and typed world/head/business/CAS closure are verified again before atomic no-replace directory publication. Source directories are retained; the receipt explicitly reports `switched_source_launch: false`.

Limits are 16000 inventory entries per scan, depth 64, 1 TiB total bytes, 64 MiB decoded bytes per CAS object and 4 MiB immutable plan metadata. CAS validation uses the existing streaming codec and verifies decoded content identity; physical copy digests still bind the original file representation. Capacity checks include remaining bytes, per-entry metadata and 64 MiB reserve. Symlinks, special files, foreign-owned data, nested undeclared mounts and overlapping source/destination are refused. An admitted empty stage or only incomplete metadata can resume using its exact parent intent. A populated stage that lost its plan, or a missing stage/destination after copying/publication, remains Unknown; do not delete its data to manufacture a successful retry.

## Migrated aggregate launch and backups

The result contains `legacy-launch.json`, `migration-plan.json`, `migration-progress.json`, `migration-receipt.json` and private `components/`. This is a separate `oasis7.legacy-runtime-launch/v1` schema and `legacy_observer_compatibility_v1` profile, not a local NodeConfig or formal activation. It preserves argv semantics and explicitly maps every effective path, retains the original reward signer ID and derives the same keys. `legacy-run --config /private/migrated-observer/legacy-launch.json` is a thin adapter to the existing runtime implementation; it checks the immutable launch binding, effective-option fingerprint, world/head and external dependency pins first. It does not synthesize a world if files disappeared.

Immutable external chain files and release world/provenance artifacts remain explicit hash-pinned backup dependencies. Rewriting a signed manifest or its absolute artifact refs is not part of this migration. A backup must include the displayed external dependencies as well as the complete destination. Data-directory-only relocation is insufficient when those external paths are retained.

Publication means a verified stopped copy is available. It does not automatically reconfigure a service manager, retire an original process or delete old directories. Stop the original deployment and change its launch configuration explicitly before starting this copy. Migration status verifies the original published byte closure; after runtime progress, use runtime health/status rather than treating changed files as a new copy authorization. Permanent removal, validator/signer transfer and formal writer fencing remain separate operations.

## Local verification

The focused Rust tests export an existing-source fixture using the real execution bridge, verify the original committed head and distinct signer identities, retain a typed pending action and idempotently queued feedback outbox, and exercise fixed-stage interruption/resume, drift rejection, live writer exclusion and missing-material refusal. The fixture creates test data only; production commands cannot generate that world.

`scripts/legacy-observer-migration.test.sh` takes OASIS7_CHAIN_RUNTIME_BIN and OASIS7_NODE_LIFECYCLE_TEST_BIN, invokes no Cargo, runs the normal migration executable, kills its exact owned migration child after durable progress, resumes the fixed stage, and compares every original/copied file plus head/identity/business closure. It retains failure evidence. These tests do not represent a real remote deployment, validator transfer or a formal consensus proof.

Each restart requires the destination directory to remain private and owned by the current user. It revalidates complete snapshot/journal and simulator mirror recovery material, then checks the original migration-height record digest, execution hash and state root. A higher head with different history at the original cut cannot replace that preserved origin.

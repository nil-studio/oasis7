# Public Testnet Governed Bootstrap Runbook (2026-06-06)

- 对应项目文档: GitHub Issue / GitHub Project
- 关联证据:
  - `doc/testing/evidence/public-testnet-governed-bootstrap-manifest-2026-06-06.json`
  - `doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json`
  - `doc/testing/evidence/public-testnet-governed-bootstrap-genesis-2026-06-06.json`
  - `doc/testing/evidence/public-testnet-governed-bootstrap-validator-registry-2026-06-06.json`
  - `doc/testing/evidence/public-testnet-governed-bootstrap-world-2026-06-06/world`
  - `doc/testing/evidence/public-testnet-governed-bootstrap-bootstrap-peers-2026-06-06.txt`
  - `doc/testing/evidence/public-testnet-governed-bootstrap-topology-2026-06-06.md`
  - `doc/testing/evidence/public-testnet-five-node-inventory-2026-06-23.md`
审计轮次: 2
## Stable authority and evidence boundary

- This is the stable operator authority for public-testnet topology, deployment truth, rollout, rollback and recovery drills. The frozen bootstrap topology remains historical provenance; the managed-five-node inventory is an execution target only after a fresh Phase A truth capture. Neither records present fleet health or release readiness by itself.
- Dated manifests, bundles, peer lists, package hashes, status snapshots and evidence files remain evidence-window inputs. Before each deployment, recovery update, or readiness assertion, operators must recapture the current deployment truth and bounded same-window health evidence; QA owns release-blocking verdicts and LiveOps/community owns external status or recovery wording.
- Restart, port cleanup, reseed, data isolation and waiting for recovery may collect evidence or provide temporary recovery only. For peer-head drift, execution/resource divergence, signer/peer identity mismatch, or degraded replication, identify and repair the code, configuration, package, manifest, deployment artifact, or state-contract root cause before clean redeploy/rebuild; never treat an operational restart as the durable resolution.

## 1. Purpose
本 runbook 定义可重复执行的 `public_testnet` 部署、补更、重建和恢复流程：

1. 从冻结的 governed bootstrap truth 出发。
2. 先拉起 2 个 validator。
3. 再以 observer 身份接入当前 operator inventory 中的非 validator 节点。
4. 将每一步的失败边界压缩为可判定的 gate，而不是运行中再猜。

这份 runbook 明确吸收了本轮 live rebuild 的几个教训：

1. `node-keypair.toml` 是受保护真值，删除它会连带改变派生 signer truth。
2. libp2p bootstrap peer id 是部署真值，不是可以长期硬编码的常量。
3. observer 超过低高度阈值后不能从 genesis 硬 replay；正常路径应由节点自动拉取受验证的高位 replication checkpoint 并补尾。
4. 这里的自动路径是 observer/light-node 的 high-head checkpoint catch-up，不是完整 execution snapshot restore，也不替代正确的 deployment truth。
5. recovery 只可消费 signed V2 checkpoint protocol 产物；该产物必须绑定同一 snapshot/status/latest identity 并携带可验证的闭包。直接从运行中的 validator 拷贝目录（包括 `seed-from-remote`、`cp`、`tar`、`rsync`）已禁用：它们不能保证 `world`、`execution-records`、`store/blobs` 与 replication/bridge metadata 属于同一提交。

## 2. Scope and Topology
冻结的 governed bootstrap 拓扑仍是两台 validator 加两台 observer；当前 operator 部署清单在此基础上扩展为五个受管节点。部署/补更时必须以本节的当前 operator inventory 为准，不能从旧 `.tmp` bootstrap 目录推断节点数量。

### 2.1 Governed bootstrap topology
| node_id | role | location | in validator set |
| --- | --- | --- | --- |
| `triad-testnet-sequencer` | validator / sequencer | ECS `39.104.204.172` | yes |
| `triad-testnet-storage` | validator / storage | ECS `39.104.205.67` | yes |
| `triad-testnet-local` | observer | Linux LAN / local observer family | no |
| `triad-testnet-fourth-local` | observer | macOS local observer family | no |

### 2.2 Current operator inventory
| node_id | role | host / lane | stack root | service manager | bounded evidence endpoint |
| --- | --- | --- | --- | --- | --- |
| `triad-testnet-sequencer` | validator / sequencer | `root@39.104.204.172` | `/opt/oasis7/p2p-testnet` | `oasis7-triad-sequencer.service` | `http://127.0.0.1:6631/v1/chain/rebuild-proof` + capability-gated identity-v2 four-command evidence |
| `triad-testnet-storage` | validator / storage | `root@39.104.205.67` | `/opt/oasis7/p2p-testnet` | `oasis7-triad-storage.service` | `http://127.0.0.1:6632/v1/chain/status` |
| `triad-testnet-local` | observer | Linux LAN observer | `/opt/oasis7/p2p-testnet-local` | `oasis7-testnet-observer.service` | `http://127.0.0.1:6633/v1/chain/status` |
| `triad-testnet-windows-observer` | observer | Windows observer | `C:\oasis7-deploy` | scheduled task `Oasis7Observer` | `http://127.0.0.1:5121/v1/chain/status` |
| `triad-testnet-fourth-local` | observer | macOS local observer | `$OASIS7_TESTNET_FOURTH_ROOT` | launchd `oasis7.testnet.fourth` | `http://127.0.0.1:19083/v1/chain/status` |

The 204/sequencer row is intentionally not a full-status contract. Governed
rebuild, rollout, and fleet evidence must use the signed
`oasis7.rebuild_status.v1` response from `/v1/chain/rebuild-proof` and, once
the identity-v2 capability is provisioned, a separately verified
`oasis7.identity_receipt.v2` envelope. Legacy `oasis7.identity_receipt.v1` is
raw runtime metadata only, retired for direct admission, and may be retained
solely as the exact byte payload bound by the v2 digest; there is no v1
compatibility fallback. The identity-v2 path is currently **NOT PROVISIONED /
CAPABILITY BLOCKED**: a copied signature, callback, or `verified: true` field
is not evidence. A 204 `/v1/chain/status` request is forbidden in this
runbook; it may only be used under a separately recorded local diagnostic
exception that is not deployment or readiness evidence.

Credential files may be used by an operator as local access aids, but this runbook only records target identities and never records secret values.

### 2.2.1 Bounded 204 proof and identity-v2 contract
The 204 proof remains a bounded, same-window artifact. Identity-v2 evidence is
separate and is not created by the proof verifier. The current required set is
exactly `sequencer-204`, `storage-205`, `linux-lan-observer`,
`windows-observer`, and `macos-observer`, once each; a plan may not shrink it to
nodes with convenient receipts.

1. `/v1/chain/rebuild-proof` returns `oasis7.rebuild_status.v1` with the
   bounded top-level fields `schema_version`, `observed_at_unix_ms`, `ok`,
   `liveness`, `readiness`, `heights`, `network_head`, `checkpoint`,
   `local_peer_id`, `connected_peers`, `connected_peer_count`, and `proof`.
2. For every managed node, retain exact raw `oasis7.identity_receipt.v1` bytes and run section 7. The raw object is never admitted; its byte SHA-256 and exact
   key tuple (`node_id`, `peer_id`, `key_sha256`, `key_size_bytes`, `key_mode=0600`, `key_uid`, `key_gid`) remain authoritative in the v2 envelope beside
   `signature_hex` and `canonical_digest`; `key_path` remains raw-only. The payload binds these fields to task/HEAD, pre-receipt plan-intent digest, context digest, capture window, rotation, and freshness.
3. The sidecar emits one exact `oasis7.identity_v2_evidence_map.v2` map per node. Its top-level fields are `schema_version`, `network_id`, `task_uid`, `head_oid`, `context`, `plan_intent`, and `entries`; context/plan-intent are `{path, sha256, size_bytes}` descriptors. Each entry has `node_name`, `node_id`, `peer_id`, and exactly seven descriptors: `raw_v1`, `prepare_manifest`, `payload`, `provider_attestation`, `unsigned_envelope`, `signed_envelope`, and `verification`. The adapter/planner check regular non-symlink paths, owner-only file/parent modes, and exact bytes; they never derive `key_path`, synthesize JSON, or pair by position.
4. Aggregate exactly five one-node maps with `scripts/p2p-public-testnet-identity-v2-evidence-aggregate.py`, one each for `storage-205`, `sequencer-204`, `linux-lan-observer`, `windows-observer`, and `macos-observer`. It requires common network/task/HEAD/context/plan-intent bindings and invokes canonical planner validation before atomically writing the five-node map; it never rewrites or deletes an input artifact.
5. Context is exact `oasis7.identity_v2_context.v1` with
   `schema_version`, `network_id`, `task_uid`, `head_oid`, `capture_window_id`,
   `capture_start`, `capture_end`, `rotation_epoch`, `issued_at`, and
   `expires_at`. Plan intent is exact `oasis7.clean_room_plan_intent.v2` with
   `schema_version`, `context_digest`, `adapter_action`, `peer_registry_sha256`, `peer_registry_epoch`, and `nodes`. Each node has exactly `node_name`, `node_id`, `peer_id`, `role`, and `reset_surface_ids`; all five managed nodes are required, and nodes and reset-surface IDs are sorted and unique.
   The order is context -> context digest -> plan intent -> plan digest -> payload -> signature -> verification receipt -> final plan; final-plan digest never flows upstream.
   Peer tuples/digest/epoch come from independently pinned `/operator/truth/managed-peer-registry.json`, shared by planner, adapter and signing validation. Its pin is unprovisioned: no historical/synthetic peers or caller overrides substitute for authority. Rotation invalidates old intent/evidence/plan/resume even with unchanged peers; v1 is rejected without silent migration. Obtain fresh governed evidence per the [provisioning contract](./public-testnet-governance-trust-root-provisioning.md#managed-peer-registry-not-provisioned).
6. Only `oasis7.identity_v2_verification_receipt.v1` with
   `mode=current_admission`, `verified=true`, and `apply_authorized=true` may
   enter a destructive plan or adapter transaction. `historical_audit` is
   forensic only (`historical_only=true`, `apply_authorized=false`), including
   when a retired key was valid at issuance. Its raw, payload, envelope,
   trust-config, registry, verifier, task/HEAD, node/peer, window, rotation,
   and authorization digests must be retained and independently checked.
7. Identity-v2 trust config, provider registry, public-key pin, and verifier
   executable pin must be independently installed under the existing governed
   root authority described in
   `public-testnet-governance-trust-root-provisioning.md`. Caller/plan/template
   supplied digests are not anchors. Until those pins and the verifier exist,
   the four-command path remains capability blocked; req-v2/proof-v1 freshness and map uniqueness do not prove provider-side one-shot replay, whose external authority is not provisioned.
   Synthetic fake-provider, fake-transport, or test-only module-cache harness receipts are orphan evidence and cannot satisfy this
   production provisioning gate.
The nested proof envelope is exactly `oasis7.rebuild_proof.v1`:
`schema_version`, `signer_id`, `signer_public_key_hex`, `signed_payload_sha256`, and `signature_hex`.
Before projection into any legacy capture shape, an independent verifier must
verify the complete response claims. The deployed runtime writes a separate
`oasis7.rebuild_proof_verification.v1` receipt:
```bash
<verified-runtime>/oasis7_chain_runtime verify-rebuild-proof --proof .tmp/public-testnet-sequencer-204-rebuild-proof.json --trusted-signer-id <sequencer-node-id> --trusted-signer-public-key-hex <deployment-truth-rebuild-proof-signer-key> > .tmp/public-testnet-sequencer-204-rebuild-proof-verification.json
```
The verification receipt is valid only as a pair with the exact raw proof file
passed to `--proof`; its required fields are `schema_version`,
`proof_schema_version`, `signer_id`, `signer_public_key_hex`,
`signed_payload_sha256`, `local_peer_id`, `proof_sha256`, and `verified`.
`proof_sha256` is the SHA-256 of the exact raw `oasis7.rebuild_status.v1` bytes,
including serialized whitespace; `local_peer_id` must equal the top-level raw
proof. Consumers must retain both files and pass both references to the governed
pair executor; missing, tampered, or cross-paired files fail closed.
The verifier rejects unsupported schema, oversized/malformed document, digest or Ed25519 signature mismatch, and signer-id/public-key mismatch.
`<deployment-truth-rebuild-proof-signer-key>` is an allowlisted public key from governed deployment registry/provenance, derived by the exact release contract
for `<sequencer-node-id>-feedback-submit`; it must never be copied from proof.
The expected signer id for this proof is the deployment-truth node id. Identity-v2
uses its own dedicated signer profile and domain; it must not reuse this proof
signer or a node-consensus key. The identity-v2 payload, context, raw-map,
freshness, current-admission, and independent-pin requirements are listed
above. This envelope is a node-signed bounded status receipt, not
validator-quorum finality or a replacement for `WorldHeadProofV1`.

### 2.3 Stack roots and deprecated bootstrap dirs
- ECS validator stack root: `/opt/oasis7/p2p-testnet`
- Linux LAN observer stack root: `/opt/oasis7/p2p-testnet-local`
- Windows observer stack root: `C:\oasis7-deploy`
- macOS observer stack root: `$OASIS7_TESTNET_FOURTH_ROOT`
- Old `.tmp/testnet-local-node-bootstrap` and `.tmp/testnet-fourth-node-bootstrap` directories are bootstrap staging artifacts only. If they have no runtime binary, `CURRENT_VERSION`, and service definition, they are not managed installs and must not be counted as deploy targets.

`$OASIS7_TESTNET_FOURTH_ROOT` is the operator-local path for the managed macOS observer root; do not hard-code a user home path in repo docs.

## 3. Truth Model
必须先区分两类真值：

### 3.1 Frozen repo truth
仓库里冻结的治理/启动真值：

- manifest
- genesis
- validator registry
- canonical bootstrap world
- bootstrap peers
- release bundle schema

这些文件定义“网络应当是什么”。

### 3.2 Deployment truth
部署时动态生成或确认的真值：

- validator host 上实际存在的 `config/node-keypair.toml`
- 从 root node key 派生出的 consensus signer public key
- validator 实际 libp2p `local_peer_id`
- 当前 runtime package sha256
- 当前可用于 observer attach 的 state-sync / seed bundle

这些值定义“当前这一轮 live rebuild 实际用了什么”。

结论:

1. repo truth 冻结后，不代表 deployment truth 永远不变。
2. 只要 validator host key 变化，deployment truth 就必须重新生成。
3. observer 接入必须消费 deployment truth，不能继续使用旧的 bootstrap peer id / signer / seed artifact。

## 4. Hard Rules
必须遵守以下规则：

1. 禁止对 `config/` 目录使用会删除未列出文件的同步方式。
   - 特别是禁止裸用 `rsync --delete` 覆盖整目录。
2. 禁止删除 validator 的 `config/node-keypair.toml`，除非明确要做 key rotation。
3. 一旦发生 key rotation，必须把以下真值一并重建：
   - derived validator signer truth
   - validator registry
   - governed bootstrap world
   - observer `REPLICATION_REMOTE_WRITERS_CSV` for the currently deployed wrapper contract. Keep this field until the wrapper and runtime have both been upgraded and checked; changing only the env spelling can omit the required allowlist.

   Candidate naming migration in [code PR #4683](https://github.com/eng-cc/oasis7/pull/4683): after that PR merges, deploy its matching wrapper and binary, then confirm `--help` exposes `--replication-fetch-requester-public-key` and that the wrapper accepts `REPLICATION_FETCH_REQUESTERS_CSV` before switching. The new name grants additional fetch permission only; writers remain bound to verified validator signers. The candidate retains the old spelling as a warning-emitting adapter and rejects conflicting old/new env values. This documentation does not authorize switching a current deployment to a variable its wrapper does not recognize. In that candidate, flat CLI scalars occur once, opposite switches conflict, and validator IDs/signer bindings are unique; peer/listener/authority/requester lists remain repeatable. Escape leading `--` values with `--node-id -- --literal`. Omitted relay/probe hints use live detection; explicit hints override it.

4. libp2p bootstrap peer id 必须在 validator 启动后实时读取，不能沿用旧证据文件中的值。
5. 当 validator 高度超过低高度阈值后，observer 不得使用空 world 从 genesis replay 追链；正常 attach 必须依赖自动 high-head checkpoint catch-up 与 tail gap sync。
6. 自动 high-head checkpoint catch-up 只覆盖 observer/light-node 边界；execution-required 节点不能用它跳过历史执行，也不能把它当作完整 snapshot state-sync。
7. 自动 checkpoint catch-up 失败后，observer 必须先 `reset-state`，再只消费 signed V2 checkpoint protocol 的 recovery 产物；live raw copy（包括 `seed-from-remote`）不得作为 recovery 路径。产物必须包含 execution restore 所需 blob 闭包，否则 observer 会在 restore snapshot/journal 时落入 `BlobNotFound`。
8. 禁止把手工复制 validator 数据目录、手工拷 checkpoint、或从一台 validator 直接覆盖另一台 validator 的 `data/` 目录当作 testnet 同步/恢复流程。validator 恢复只能走两条路径：
   - 先让节点按 manifest bootstrap peers、replication fetch、peer-head exchange 自动恢复同步。
   - 自动恢复失败且根因是 deployment truth 漂移或本地状态污染时，从当前 deployment truth 从零重建 validator pair。
9. 若曾经执行过手工 checkpoint/data copy，该状态只能作为被隔离的故障现场或无效恢复尝试记录；不能作为 readiness 证据、不能继续接在正式 testnet world state 上运行，也不能对外宣称为“已同步”。
10. 遇到多节点分叉、execution/resource 状态不一致、peer head 长期 stale、validator pair 互相拒绝 commit、或类似状态不对的问题，不得把临时运维动作当作最终修复结论。标准处理顺序必须是：
   - 先保留现场并查清根因，至少覆盖代码路径、deployment truth、world/resource snapshot、replication/head exchange 和节点身份。
   - 修复根因对应的代码、配置、包、manifest、部署产物或重建脚本。
   - 再按修复后的 canonical deployment truth 重新部署或从零重建受影响节点。
   - 重启服务、清理端口、手工 reseed、复制数据、或等待自愈只能作为取证/验证手段，不能替代根因修复，也不能作为“分叉已解决”的对外口径。
### 4.1 Runtime high-state sync contract
`public_testnet` observer 的标准 attach 设计是不要求操作者预先提供 seed/state-sync 目录。新 observer 在发现远端链头远高于本地高度时，必须先尝试从 P2P replication 网络拉取一个受验证的高位 execution checkpoint，再从该 checkpoint 后继续 tail gap sync。

运行时实现必须满足以下约束：

1. 候选 checkpoint 不能只看 advertised head 或最近一个对齐边界；必须覆盖 storage profile 保留窗口内的多个 checkpoint 边界。
2. `release_default` 当前按 64 高度间隔保留 8 个 execution checkpoint，因此 observer gap sync 至少要回看 8 个 64-height checkpoint windows。
3. checkpoint commit payload 的 `execution_block_hash`、`execution_state_root` 与 checkpoint descriptor 必须完全匹配，blob 内容必须按 content hash 和 size 校验后才能安装。
4. 如果 advertised head 不是 checkpoint 高度，observer 可以安装 head 之前最近仍被保留且可验证的 checkpoint；安装后再由正常 gap sync 追尾。
5. storage/full-storage provider 即使不是原始 sequencer writer，也必须能在 fetch-commit 响应中为本地 retained execution checkpoint 动态附加 checkpoint descriptor，并以自身 authorized replication writer 身份重新签发增强 commit message；否则只连到 storage peer 的冷 observer 无法获得可安装的高状态入口。
6. 若保留窗口内没有可获取的 checkpoint，状态接口应继续报告 `state_sync_fallback_required=true`，此时才进入 break-glass seed/state-sync recovery。
## 5. Required Inputs
开始前，操作者必须准备以下输入：

### 5.1 Repo artifacts

- `doc/testing/evidence/public-testnet-governed-bootstrap-manifest-2026-06-06.json`
- `doc/testing/evidence/public-testnet-governed-bootstrap-genesis-2026-06-06.json`
- `doc/testing/evidence/public-testnet-governed-bootstrap-validator-registry-2026-06-06.json`
- `doc/testing/evidence/public-testnet-governed-bootstrap-world-2026-06-06/world`
- `doc/testing/evidence/public-testnet-governed-bootstrap-bootstrap-peers-2026-06-06.txt`

### 5.2 Runtime/package artifact

必须是 Linux package truth，不允许用本机 macOS binary 直接替代 ECS runtime。

最少校验项：

```bash
./scripts/release-candidate-bundle.sh validate \
  --bundle doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json
```

### 5.3 Host access

- validator ECS SSH 可写权限
- local observer stack 可写权限

### 5.4 Fresh erased-host no-start subprocedure

当且仅当本轮 deployment truth 已确认需要物化一台已擦除 validator host，进入
`doc/testing/manual/public-testnet-fresh-validator-host-bootstrap-2026-07-28.manual.md`。
该 narrow procedure 只安装包与受治理配置、生成节点身份并留下 public receipt；
它必须保持服务 disabled/inactive，不能替代本 runbook 的 topology review、后续
phase、rollback/recovery、readiness 或 release 判断。

## 6. Phase Overview
执行顺序固定为：

1. Phase A: Preflight and truth capture
2. Phase B: Build deployment truth
3. Phase C0: Routine CI package update
4. Phase C: Preflight, reset, stage, and rebuild validators when state or signer truth requires it
5. Phase D: Verify validator pair
6. Phase E: Optional recovery seed/state-sync bundle
7. Phase F: Attach or reseed observers
8. Phase G: Final health verification
9. Phase H: Failure handling and rollback

任何 phase 未通过，不进入下一 phase。
## 7. Phase A - Preflight and Truth Capture
### Goal
在任何 destructive 操作前，读取并记录本轮 live rebuild 真值。

### Identity-v2 evidence preparation (capability-gated)
The executor must first produce exact raw-v1 bytes, an unsigned deployment-truth template, context, and plan intent for each of the five managed nodes. Context and intent precede the payload and contain no receipt, signature, verdict, or final-plan digest. The governed sidecar is `scripts/p2p-public-testnet-identity-receipt-v2.py`; bridge mode accepts `--raw-v1`, `--template`, `--out`, `--context`, `--plan-intent`, `--trust-config`, `--provider-registry`, `--provider-ref`, `--signer-tool`, `--verifier-tool`, `--evidence-map-out`, and `--evidence-dir`, invokes the four fixed commands below, and retains exact evidence. Its legacy shape-only mode is not admission. The sequence below is the only approved argument shape; repeat it in one capture window. `<identity-v2-tool>` is a deployment-provided, independently pinned executable. Until the trust config, registry, provider, and verifier pins exist, this sequence is capability blocked.

The planner handoff must include its exact `--identity-v2-evidence-map <verified-map.json>` option: `python3 scripts/p2p-public-testnet-full-network-clean-room.py --input <authenticated-five-node-truth-envelope> --identity-v2-evidence-map <verified-map.json> --out <plan.json> --json`. Every adapter invocation carrying identity-v2 inputs, including an `--apply` validation request, must include that exact map and `--identity-v2-mode current_admission`: `python3 scripts/p2p-public-testnet-full-network-clean-room-adapter.py --plan <plan.json> --authority <authority.json> --journal <journal.jsonl> --ledger <ledger.jsonl> --identity-v2-evidence-map <verified-map.json> --identity-v2-mode current_admission`. The current map path is validation-only and does not authorize execution; these placeholders are operator-bound artifacts, not live paths; the pair executor remains blocked until it consumes this retained map.

```bash
<identity-v2-tool> prepare --raw-v1 <node>.identity-receipt.v1.raw --template <node>.identity-v2.unsigned-template.json --context <node>.identity-v2.context.json --plan-intent <managed-five-node.plan-intent.json> --trust-config /operator/truth/identity-v2-trust-config.json --provider-registry /operator/truth/identity-v2-provider-registry.json --payload-out <node>.identity-v2.payload.bin --manifest-out <node>.identity-v2.prepare.json
<identity-v2-tool> sign --payload <node>.identity-v2.payload.bin --manifest <node>.identity-v2.prepare.json --provider-registry /operator/truth/identity-v2-provider-registry.json --provider-ref <approved-custody-provider-id> --signature-out <node>.identity-v2.signature.hex --attestation-out <node>.identity-v2.provider-attestation.json
<identity-v2-tool> assemble --payload <node>.identity-v2.payload.bin --manifest <node>.identity-v2.prepare.json --signature <node>.identity-v2.signature.hex --attestation <node>.identity-v2.provider-attestation.json --provider-registry /operator/truth/identity-v2-provider-registry.json --out <node>.identity-v2.envelope.json
<identity-v2-tool> verify --mode current_admission --envelope <node>.identity-v2.envelope.json --attestation <node>.identity-v2.provider-attestation.json --raw-v1 <node>.identity-receipt.v1.raw --context <node>.identity-v2.context.json --plan-intent <managed-five-node.plan-intent.json> --trust-config /operator/truth/identity-v2-trust-config.json --provider-registry /operator/truth/identity-v2-provider-registry.json --out <node>.identity-v2.verified.json --verification-out <node>.identity-v2.verification.json
```

`assemble` must receive and byte-check the explicit payload before writing an envelope. `verify` must independently reconstruct the prefixed payload, check
current trust/revocation, and emit a fresh receipt. The bridge validates the
code-owned registry/trust inputs before the provider request; the independently
pinned/governed verifier must run before any mutation or admission decision, and
only its fresh receipt can authorize current evidence. A historical-audit receipt
is forensic only. Planner, adapter, and validator-pair integration
remains a prerequisite for pair-executor production admission; the bridge,
planner, and adapter map flags above are the implemented file contract and must
not be replaced with guessed alternatives.

### Required checks
1. 优先使用标准 truth capture 脚本：

```bash
# 204/sequencer is intentionally sampled through the bounded, signed proof
# surface; do not feed its full /v1/chain/status payload into a rebuild gate.
curl -fsS http://39.104.204.172:6631/v1/chain/rebuild-proof \
  > .tmp/public-testnet-sequencer-204-rebuild-proof.json
ssh root@39.104.204.172 '/opt/oasis7/p2p-testnet/current/bin/oasis7_chain_runtime identity-receipt --node-id "<sequencer-node-id>" --config-dir /opt/oasis7/p2p-testnet/config' > .tmp/public-testnet-sequencer-204-identity-receipt.v1.raw
# Run scripts/p2p-public-testnet-identity-receipt-v2.py in bridge mode per
# section 7; its legacy shape-only mode is not admission evidence.
<verified-runtime>/oasis7_chain_runtime verify-rebuild-proof \
  --proof .tmp/public-testnet-sequencer-204-rebuild-proof.json \
  --trusted-signer-id <sequencer-node-id> \
  --trusted-signer-public-key-hex <deployment-truth-rebuild-proof-signer-key> \
  > .tmp/public-testnet-sequencer-204-rebuild-proof-verification.json
jq '{node_id:.proof.signer_id,
    replication:{local_peer_id:.local_peer_id},
    consensus:{committed_height:.heights.committed_height,
      network_committed_height:.heights.network_committed_height,
      last_execution_height:.heights.last_execution_height,
      last_block_hash:.network_head.block_hash,
      last_execution_block_hash:(.checkpoint.execution_block_hash // .network_head.execution_block_hash),
      last_execution_state_root:(.checkpoint.execution_state_root // .network_head.execution_state_root),
      network_head:.network_head},
    readiness,
    chain_proof:{latest_execution_checkpoint:.checkpoint}}' \
  .tmp/public-testnet-sequencer-204-rebuild-proof.json \
  > .tmp/public-testnet-sequencer-204-capture.json
./scripts/p2p-public-testnet-capture-truth.sh \
  --bundle doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json \
  --sequencer-status-json .tmp/public-testnet-sequencer-204-capture.json \
  --storage-status-url http://39.104.205.67:6632/v1/chain/status \
  --sequencer-ssh-host root@39.104.204.172 \
  --storage-ssh-host root@39.104.205.67 \
  --out .tmp/public-testnet-deployment-truth.json
```

2. 若只做最小手工校验，至少要覆盖：

```bash
./scripts/release-candidate-bundle.sh validate \
  --bundle doc/testing/evidence/public-testnet-governed-bootstrap-bundle-2026-06-06.json
```

3. 记录当前 ECS host key material 是否已存在：

```bash
ssh root@39.104.204.172 'ls -l /opt/oasis7/p2p-testnet/config/node-keypair.toml'
ssh root@39.104.205.67 'ls -l /opt/oasis7/p2p-testnet/config/node-keypair.toml'
```

4. 记录当前远端 runtime hash：

```bash
ssh root@39.104.204.172 'sha256sum /opt/oasis7/p2p-testnet/current/bin/oasis7_chain_runtime'
ssh root@39.104.205.67 'sha256sum /opt/oasis7/p2p-testnet/current/bin/oasis7_chain_runtime'
```

5. 若 validator 已在运行，读取当前实际 libp2p peer id：

```bash
curl -fsS http://39.104.204.172:6631/v1/chain/rebuild-proof | jq -r .local_peer_id
ssh root@39.104.205.67 'curl -s http://127.0.0.1:6632/v1/chain/status | jq -r .replication.local_peer_id'
```

### Pass criteria
1. bundle validate 通过
2. 已知每台 validator 的当前 runtime hash
3. 已知每台 validator 的 `node-keypair.toml` 是否会被保留
4. 若节点当前可读，已记录 live `local_peer_id`

## 8. Phase B - Build Deployment Truth
### Goal
显式生成本轮 live deploy 所需的动态真值。

### When to rebuild deployment truth
满足任一条件就必须重建 deployment truth：

1. `node-keypair.toml` 不存在
2. `node-keypair.toml` 被替换
3. validator 实际 `local_peer_id` 与旧 bootstrap peers 不一致
4. validator 派生 signer 与旧 registry 不一致

### Required outputs
至少生成或确认以下内容：

1. current validator signer truth
2. current bootstrap peer ids
3. current deployment-only validator registry
4. current deployment-only bootstrap world
5. current deployment-only manifest/bundle wiring

### Required checks
1. 若 key rotation 发生，先从 host 当前 `node-keypair.toml` 派生 signer truth。
2. 用新的 signer truth 生成 deployment-only validator registry。
3. 用新的 registry 重建 deployment-only governed bootstrap world。
4. 读取 validator 实际 `local_peer_id`，更新当前 deployment truth 中的 manifest/bootstrap peer artifact；manifest-backed public_testnet 启动不再要求 observer env 维护 `REPLICATION_NETWORK_BOOTSTRAP_PEERS_CSV`。
5. 按当前部署 wrapper 合同更新 observer 的 `REPLICATION_REMOTE_WRITERS_CSV`，对齐当前 deployment truth 中需要 fetch 权限的 requester signer；除 validator signer 外，必须包含会提供 retained execution checkpoint 的 storage/full-storage provider signer。只有 #4683 合入、匹配 wrapper/binary 已部署且上述 help/配置支持检查完成后，才改用 `REPLICATION_FETCH_REQUESTERS_CSV`；不得仅改 env 名称而丢失 allowlist。

### Pass criteria
1. deployment-only registry 与 host 当前 signer truth 一致
2. deployment-only bootstrap world 已重新生成并验证
3. manifest/bootstrap peer artifact 里的 peer ids 与 validator live `local_peer_id` 一致

manifest/bootstrap peer truth 刷新应写回 deployment artifact，例如：

```bash
curl -fsS http://39.104.204.172:6631/v1/chain/rebuild-proof | jq -r '.local_peer_id'
curl -fsS http://39.104.205.67:6632/v1/chain/rebuild-proof | jq -r '.replication.local_peer_id // .local_peer_id'
$EDITOR doc/testing/evidence/public-testnet-governed-bootstrap-bootstrap-peers-2026-06-06.txt
```

`scripts/p2p-public-testnet-refresh-bootstrap-peers.sh` 只适用于 legacy/non-manifest observer env 维护；不得把它作为 formal manifest-backed public_testnet 的必要同步路径。

## 8.5 Phase C0 - Routine CI Package Update
### Goal
在不需要重建 signer/genesis/world 的情况下，把当前五节点升级到同一代码线的 CI artifact，并保留可回滚 release。

### CI package scope selection
1. 只升级 ECS Linux validators 和 Linux LAN observer 时，可使用 `Testnet Packages` 的 Linux artifact。
2. 同时升级 macOS local observer 时，使用包含 `macos-arm64` 的 package run，并分别校验 Linux 与 macOS runtime hash/DMG native identity；当前 arm64 observer 不得由 `macos-x64` artifact 覆盖。
3. 需要升级 Windows observer 时，必须使用包含 Windows artifact 的 CI scope；如果先前 run 只有 Linux/macOS，不得把 Linux artifact 复制到 Windows。
4. 最终 fleet 允许存在 package version 后缀差异，但必须说明原因。当前 workflow 中 `all_existing` 仍只生成 `linux-x64`、`macos-x64`、`windows-x64`；完整 managed-fleet packaging 必须对同一 requested ref/commit 协调执行 `all_existing` plus `linux_macos_arm64` runs，并记录两组 run ID、BUILDINFO 与 SHA256SUMS。验收以 runtime hash、commit lineage、status health 和高度对齐为准。

Wasmtime48.0.3 → 49.0.3 不得采用运行中 rolling/restart/单节点 rollback；完整 fleet 的审读授权计划、quiescence、identity/checkpoint/closure admission 和 recovery hold 依 [协调维护分册](wasmtime-49-coordinated-maintenance.runbook.md#wasmtime-49-maintenance) 执行，条件不满足保持 `hold`。
### Routine observer rollout capability hold
本节 observer mutation 分支当前保持 **hold**。现有 `p2p-public-testnet-package-rollout.py` 的 `canonical_provider_status_urls` 要求 sequencer/storage 都提供 `/v1/chain/status`，生成的 observer gate 也读取该 full-status checkpoint；这与 2.2 对 204 部署证据的禁止条款不兼容。不能用 local diagnostic exception、改 URL 为 `/v1/chain/rebuild-proof`、手写字段投影或跳过 wrapper 解除 hold。

解除该分支 hold 前，必须先完成并独立评审受治理的 proof-aware gate 接线，使其消费独立验证、同窗绑定的 204 signed proof，并保留既有双 provider checkpoint identity、高度兼容、完整 closure probe 和 identity-v2 capability 要求；随后以实际候选验证拒绝缺失、过期、错误签名/绑定与 closure 不完整等情况。本说明没有实现该 adapter，也不授权部署。下列顺序保留原验收条件，用于能力修复后的执行；不是当前可执行的 observer 放行路径。

### Safe update order
1. 记录五节点 `CURRENT_VERSION`、runtime hash、service manager、status endpoint。
2. 先升级两个 validators，并确认 validator pair `ready`、高度推进、互相有 fresh peer head。
3. **Observer checkpoint gate（硬门）**：在任何 Linux、Windows 或 macOS observer 的 mutation command 前，两个 canonical provider 必须从其 manifest `status_url` 同时返回 `chain_proof.latest_execution_checkpoint`。每个对象必须有 `schema_version >= 2`、非空 `checkpoint_id`、64 位 hex `manifest_hash` 和正 `height`；两端 `checkpoint_id`、`manifest_hash` 必须相同，且 `abs(sequencer.height-storage.height) <= 1`。`null`、v1、malformed identity、仅一个 provider、identity mismatch 或 height 不兼容都 fail closed。identity 一致只是必要条件：正式 observer rollout 前还必须用 clean-room observer/probe 从至少一个 authorized、当前 connected provider 拉取并逐 hash/size 验证该 descriptor 的 manifest 与全部 blob closure；只有 status identity、没有完整可获取 closure，或 publication 中缺任一 blob，都不得视为可恢复。允许单个过期 provider 返回 not-found 后转向另一个 connected complete provider；所有 advertised providers 的 rate-limit 必须进入 cooldown，不能用 generic/connected route 绕过。`p2p-public-testnet-package-rollout.py` 生成的 observer wrapper/PowerShell/macOS script 内置 identity gate；closure probe 的成功 evidence 必须在执行 wrapper 前写入本次 rollout 记录。不得绕开 wrapper 直接调用 Linux primitive、Windows installer 或 macOS promotion script。
   clean-room probe 只能由 `scripts/p2p-observer-checkpoint-closure-probe.py` 执行：它从待部署 package 解出 runtime，在新的 `APP_ROOT` 复制 governed startup truth、重写为 loopback observer 和全新的 state roots，然后以不少于 32 字符的不可预测 nonce 启动 `p2p-triad-node-start.sh`。在 probe 启动包内 runtime 前，rollout helper 必须先验证该 package 的 SHA256SUMS、BUILDINFO 和 assets；即使本次仅 mutation Windows/macOS observer，也必须验证 probe 实际执行的 `linux-x64` bundle。校验失败不得启动 probe 或写 observer mutation gate。runtime 仅在完整 closure 已通过 signed network fetch、hash/size binding 并安装后写出 `oasis7.checkpoint_closure_verification_receipt.v1`；probe 验证 nonce、每个 `network_fetch`、`signed_request`、connected candidate、hash/size 以及 package runtime SHA-256 后，生成带 canonical digest 的 `oasis7.observer_checkpoint_closure_probe.v1`。rollout helper 对任何 observer 直接执行该 probe；不得传入 `--checkpoint-closure-receipt`，也不得设置 `OASIS7_CHECKPOINT_CLOSURE_RECEIPT`。发布 CLI 不接受 `OASIS7_TESTING` 或 `OASIS7_TEST_CHECKPOINT_CLOSURE_PROBE` 作为 probe 选择输入；测试只能在进程内注入替身。调用方自带、过期、篡改或 free-form JSON 一律不是证据并 fail closed。生成的每个平台 observer gate 仍把 live provider checkpoint identity 与 probe identity 再次比对。
4. gate 失败时，停止 observer rollout；先修复 provider package/config/deployment truth 或等待 pointer+manifest validation 后的 checkpoint 发布，再重新生成并执行 observer plan。不得用 restart、旧 seed 或 validator-to-validator 手工 copy 作为 gate 的替代。
   对五节点 clean-room mutation，唯一受治理的外部 adapter 必须以 plan/dry-run 为默认，并在任何 apply 前接收 code-owned peer/endpoint inventory、冻结 task/head/plan digest、独立验证的 authority/provenance receipts 及 nonce ledger。每个 preflight、backup、stop/delete/rebuild/start、verify、fresh-root-probe、fleet-health 和 rollback/re-observe phase 都必须提交当前 transaction/capture-window 绑定的签名 receipt；forensic backup 的 manifest 固定 `seed_eligible=false`，operator-authorized no-backup 则必须有独立 verifier 校验的、未过期且非未来签发的 signed authority。observer 的 destructive phases 只能在 validator fresh-root probe 已验证完整 blob/runtime/provider/recovery closure 后开始；任何 receipt、journal、known-host、path/symlink、capacity、credential-ledger 或 trust-root file metadata 不匹配都 fail closed。journal 仅为 adapter-owned audit/resume evidence，不是 apply authority；rollback 只允许 clean-redeploy 并保留失败态与 re-observation receipt，禁止旧状态恢复或跨节点复制。
   Recovery scope 必须同时写入 `reobserve-failed-state` 与 `rollback-clean-redeploy` 两份签名 receipt 的 `bindings.rollback_candidates`，并与 journal 顶层 `rollback_candidates` 完全一致。该列表是 plan `global_order` 中 `stop:`、`delete:`、`rebuild:`、`start:` mutation operations 的唯一、有序前缀，包含已准入但 callback 抛错或 receipt 校验失败的当前 operation；不得只从成功 receipts 推导。adapter 在调用 mutation callback 前先持久化准入后的候选列表；crash 后该记录表示结果可能不确定，不是安全重放许可。两份 recovery receipts 的 `failed_operation`、`failed_state_digest` 也必须彼此及 journal 一致。遗漏、截断、重排、重复或替换候选 scope 均 fail closed；旧 v2 journal 缺少此字段也不得推断补齐后 resume。dry-run 的候选列表固定为 `[]`，不会形成 recovery mutation authority。若 capture/no-backup authority 在副作用后过期，保留候选 scope 与 `reconciliation-blocked` 失败态，不得据此执行过期授权下的 re-observe/rollback；必须重新取得受治理的有效 authority 与证据后另行协调恢复，本契约不延长授权或解除现有 live admission capability blocker。
5. 再升级 observers；每个 observer 升级后单独验证，不要把上一个 observer 的 ready 当作整个 fleet ready。
6. 如果 observer 从空状态或旧高度启动后报告 `consensus_peer_head_unavailable` 或 network head 暂不可用，macOS rollout 会在 120 秒启动窗口内重试；持续未就绪会触发 `readiness_timeout` rollback，但不会仅因这两个 transient 信号触发 state-sync escalation。rollback 后若仍有 `replication no connected providers`、长期不追高或再次启动仍无法取得 network head，使用 Phase E/F 的 recovery seed 路径。`execution driver peer mismatch` 仍是立即终止启动等待的 authority failure。
7. 如果 validator pair 自身出现 `execution driver peer mismatch`，不要继续 reseed observers；先恢复 validator pair，再用恢复后的 storage/sequencer fresh state 重新 reseed observers。
8. validator clean rebuild 后，本地 observer 不能只升级 runtime/package 并隐式保留旧 `world` / `world-simulator-mirror` / `execution-records` / `replication-root` / `runtime-root` / `store`。`scripts/p2p-public-testnet-local-node-install.sh` 遇到既有状态默认 fail-closed；同链普通升级必须显式 `--preserve-state`，clean rebuild/redeploy 必须显式 `--reset-state`，随后走自动 checkpoint catch-up 或已验证的 signed V2 checkpoint recovery。不得以 `seed-from-remote` 恢复。
9. 如果 observer 本地 `committed_height` 高于 clean-rebuilt validator pair 的 fresh head，视为旧链状态接入新链；不要用重启作为修复结论，先修正部署/状态合同，再从 clean state 或受信 checkpoint 重建 observer。

### Platform-specific update entrypoints
1. Linux nodes use `scripts/p2p-public-testnet-package-node-upgrade.sh` with the node root and Linux bundle.
2. macOS local arm64 observer uses the generated `macos-arm64` operator script and an explicit `system/oasis7.testnet.fourth` or `gui/<numeric-uid>/oasis7.testnet.fourth` target; do not verify it with the Linux runtime hash or a verification-only DMG check.
3. Windows observer uses the Windows installer artifact, updates `C:\oasis7-deploy\CURRENT_VERSION` and deploy metadata, and restarts scheduled task `Oasis7Observer`.

### macOS arm64 observer fail-closed contract

The package rollout helper generates a plan-only macOS script for the managed observer. It accepts only a concrete `system/<label>` or `gui/<numeric-uid>/<label>` target, derives the matching bootstrap domain (`system` or `gui/<uid>`), and rejects generic `user`, non-numeric GUI identities, or incomplete targets. Before bootout it executes the fail-closed canonical-provider checkpoint gate through native `curl` and `plutil`; this macOS contract does not require Python. It then re-verifies the exact planning-time DMG SHA-256, mounts the DMG at an explicit temporary mountpoint, requires Darwin/arm64 host capability and native arm64 payload identity, and creates a verified attempt-local preflight backup of the active runtime, every declared config, `CURRENT_VERSION`, and `DEPLOYED_BUILDINFO`—but not persistent state while the observer is running. Declared state roots must be normalized, unique, non-nested descendants of the node root and may not equal the node root or overlap/contain the runtime, config, `CURRENT_VERSION`, `DEPLOYED_BUILDINFO`, or launchd plist. The generated shell rejects symlink state roots again before backup, failed-state capture, and restore, before any rollback `rm -rf`.

Next it boots out the original target. If bootout fails, it ensures the untouched original service is loaded and verifies `/healthz` contains `ok=true` and `/v1/chain/status` contains `running=true`; requiring `running=true` from healthz is invalid. After successful bootout—and while the observer is stopped—it copies and verifies every declared persistent-state path. A stopped-state backup failure follows the same original-service restart/health path. Runtime or provenance promotion begins only after this state closure passes, then the script bootstraps the derived domain and checks the same endpoint-specific fields while continuing to inspect the status payload for authority failures.

Any post-promotion failure first preserves the failed active state under the attempt root for evidence. Rollback then replaces every active state path with its stopped pre-upgrade snapshot, restores runtime/config/`CURRENT_VERSION`/`DEPLOYED_BUILDINFO`, bootstraps the original domain, and verifies health. Rollback is idempotent and the attempt root is retained; EXIT cleanup only detaches and removes the explicit DMG mountpoint. During the bounded 120-second startup window, `network_head_available=false` and `consensus_peer_head_unavailable` are transient unready signals: the script keeps retrying, and persistent absence ends in `rollback "readiness_timeout"` without `state_sync_escalation_required=true`. Explicit `state_sync_fallback_required=true`, `authority_failure`, and `execution_driver_peer_mismatch` remain fatal authority signals; the status classifier scans the full alert array and fatal status error fields so a transient alert cannot mask a later fatal signal. An authority failure always rolls back and emits `state_sync_escalation_required=true`, even when failed-state capture or rollback itself cannot complete. It is not permission to restart repeatedly or to use validator data copying as recovery.

The legacy package-upgrade script validates package identity but does not add a
signing/notarization requirement. The canonical pair transaction below adds a
verified detached provenance receipt; release policy remains the authority for
any additional signing requirements.

## 9. Phase C - Clean Rebuild Validators
### Goal
从零重建 validator pair。
本 phase 的顺序是硬约束：`consumer-impact record gate -> preflight both -> reset both -> stage both -> sequencer liveness -> storage`。任一步未通过，都不得提前执行后一步；特别是 consumer-impact record 未通过时不得建立 SSH/network connection 或执行 host preflight，且不得在只 reset 一台 validator 后向任一 host staging。

五节点 clean-room planner/adapter 也必须在同一 Phase C0 口径下工作：输入 envelope 必须携带 `consumer_impact_record: {"path": <absolute-record-path>, "sha256": <record-file-sha256>}`；planner 会在生成 plan 前读取并校验精确 schema、RFC3339 新鲜度及 `active`/`unknown` 的 outage/recovery/producer wording 绑定，`none` 仍要求三个字段存在但允许 `n/a`。生成的 plan、authority、transaction journal 与每份 provider receipt 固定该 path+SHA-256；adapter 在 trust-root、nonce reservation、provider connection 或 destructive callback 前再次校验。缺失、过期、内容/绑定漂移均 fail closed；record 只控制 testnet reset/redeploy，不扩大 testnet 边界，也不构成旧 chain state、资产或 mainnet continuity 的恢复承诺。

五节点 adapter 的 execute/resume 先获取固定 `/operator/truth/full-network-clean-room.lock` 再获取 journal lock，持续到 callback/recovery/durable completion 后逆序释放；锁与父目录身份漂移阻断后续动作，mutation/recovery callback 前重新校验 live governance root。此锁只协调同一执行主机共享路径，不构成跨主机授权；部署 owner/mode 与完整边界见 [execution-lock contract](./public-testnet-governance-trust-root-provisioning.md#full-network-clean-room-execution-lock)。
### C0.5 Canonical pair transaction (task #3324; predecessor task #3318)

新的受治理入口是 `scripts/p2p-public-testnet-rebuild-validators.sh plan|apply|resume|rollback`，它将请求转发到 local-first 执行器 `scripts/p2p-public-testnet-validator-pair-rebuild.py`。它必须先以 `plan` 模式验证签名的 `oasis7.validator_pair_rebuild_provenance.v1`、package `BUILDINFO`/`SHA256SUMS`、deployment manifest、genesis、registry、bootstrap 和 world 的 hash/size 绑定，以及每台 host 的同文件系统容量/inode receipt；plan 同时执行 live GitHub/SSH bounded read-only re-observation，但不调用 systemd 或修改节点。可复核的证据模板位于 `doc/testing/templates/public-testnet-validator-pair-rebuild-evidence-v1.json`。下面的 invocation 保留当前 legacy `--identity-receipts` 形状，仅供接口迁移参考；identity-v2 bridge、planner、adapter map validation 已实现，但在 pair-executor v2 admission integration and provisioned anchors 之前不得执行 identity-v2 admission 或 destructive apply。

每个 governed tree 在进入容量预算前都必须先完成无跟随 symlink 的完整 inventory：`entry_count` 必须等于 `link_count + dir_count + file_count`，并同时绑定 `total_bytes`。world tree 的任何嵌套 symlink（包括目录或 broken link）都直接拒绝；inode 预算覆盖备份、package、governed entries 以及允许复制的目录、文件和 symlink。apply 阶段重新采集并比对同一 inventory，staged governed receipt 也必须逐项一致。

```bash
scripts/p2p-public-testnet-rebuild-validators.sh plan \
  --package-dir <verified-package-dir> \
  --provenance <verified-pair-provenance.json> \
  --trust-root <governed-provenance-trust-root.json> \
  --identity-receipts <validator-identity-receipts.json> \
  --sequencer-rebuild-proof <signed-204-rebuild-proof.json> \
  --sequencer-rebuild-proof-verification <signed-204-rebuild-proof-verification.json> \
  --sequencer-proof-verifier <verified-runtime>/oasis7_chain_runtime \
  --consumer-impact-record <record.json> \
  --human-direct-ssh-request <executor-bound-human-stop-request.json> \
  --known-hosts <pinned-known-hosts> \
  --quiescence-transaction-id <executor-bound-observation-id> \
  --capacity-json <per-node-capacity.json> \
  --node storage-205=local:<stopped-node-root-205> \
  --node sequencer-204=local:<stopped-node-root-204> \
  --sequencer-proof-url <bounded-proof-endpoint> \
  --out-dir <transaction-dir>
scripts/p2p-public-testnet-rebuild-validators.sh apply \
  --transaction <transaction-dir>/transaction.json \
  --human-direct-ssh-request <same-executor-bound-human-stop-request.json> \
  --known-hosts <same-pinned-known-hosts> \
  --host-adapter <governed-host-adapter>
```
#### External nonce-ledger deployment prerequisite

Before any `human_direct_ssh`, `plan`, `apply`, `resume`, or `rollback`, provision this executor-owned external replay barrier: `/var/lib/oasis7/p2p-public-testnet/validator-pair-nonces.jsonl`.
It must already be a regular non-symlink file owned by the executor identity with mode `0600`; its parent must be deployment-owned and not writable by unrelated users, and the executor fails closed rather than creating it.
Every request must bind the exact absolute `nonce_ledger_path`; `OASIS7_VALIDATOR_PAIR_NONCE_LEDGER` must resolve to that same path, never to repository or output state.
Deployment preflight must create/check the parent and file with equivalents of `install -d -m 0700` and `install -m 0600`, then verify UID, regular-file type, mode, and path before an outage window.
Retain the append-only ledger across process restarts, output cleanup, rebuilds, rollback drills, backups, and rotation; preserve all rows and never truncate or reuse a consumed nonce. Failed checks are `capability_blocked`.

`human_direct_ssh` 是本流程唯一的人工作业停机后观察入口：独立的人类 stop 完成后，executor 必须从 live GitHub task/user authority 或 authenticated control-plane approval 重新授权，authority 绑定 repository、issue/task UID、actor、动作、两角色目标、nonce 与 expiry；调用者 stop JSON、历史 receipt、任意 adapter JSON 均不是 authority。
executor 只允许读取固定 production inventory `/opt/oasis7/p2p-testnet/config/public-testnet-validator-pair-inventory.v1.json` 与 pinned host-key 文件 `/opt/oasis7/p2p-testnet/config/public-testnet-validator-pair-known-hosts`；它们绑定两角色 host、`/opt/oasis7/p2p-testnet` root、service 与 key fingerprint，不接受 CLI 覆盖或任意 target。SSH 必须使用 `StrictHostKeyChecking=yes`、`UserKnownHostsFile=<canonical-pinned-file>` 与 `GlobalKnownHostsFile=/dev/null`，并校验 canonical 文件 owner/mode 与每个 role 唯一且无冲突的 key；凭据只能经 temporary FD 或 temporary environment binding（`--credential-fd`/`--credential-env`）进入，绝不出现在 argv、日志、transaction 或 receipt。
停机后只运行 executor-owned fixed read-only quiescence commands 和 bounded role-specific readbacks，不得 stop/start、preflight、reset、stage、restart 或修改 observer；结果必须在同一 executor 进程内供 plan/apply 使用。`--out-dir` 的 `oasis7.validator_pair_rebuild_quiescence_proof.v1` 仅为 audit trail，不能被后续进程或 caller 重写后作为 admission authority；authority、inventory/pin、strict SSH、readback、临时凭据审计或复观测缺失即 `capability_blocked`。
这些 SSH/readback 只证明当时看到的主机输出，不证明主机未被攻陷；受攻陷主机仍可伪造服务状态、listener 或 health。本流程不宣称 compromised-host resistance，仍须依赖独立 identity、provenance、package 与 trust-root gates。
对于 active/unknown consumer-impact，先取得 live authority 并由人类 separate stop，再调用 direct mode；旧的 quiesce adapter phase 不再是有效入口：
```bash
scripts/p2p-public-testnet-rebuild-validators.sh human_direct_ssh \
  --request <executor-bound-human-stop-request> \
  --known-hosts /opt/oasis7/p2p-testnet/config/public-testnet-validator-pair-known-hosts \
  --out-dir <audit-dir> \
  [--credential-env <temporary-env-name> | --credential-fd <temporary-fd>]
```
`human_direct_ssh` 不把 request envelope 当 authority，不接受 arbitrary adapter JSON 或 caller assertion；它必须在同一调用中 live-read GitHub authority，以固定 inventory/pin 观察两角色 `active=false`、`running=false` 与 bounded health/listener。供 `plan`/`apply` 使用的 executor-bound request 必须同时绑定同一个 `--consumer-impact-record` 文件的路径与 SHA-256（`impact_record_path` 与 `impact_record_sha256`）；executor 返回的 direct receipt digest 必须与该 CLI 文件逐字节一致，不能由 `validators_already_stopped`、status snapshot 或 persisted proof 推断。`impact=none` 仍须 direct read-only observation；旧 receipt、遗漏/重复/伪造/stale/binding-mismatch 均不得进入 destructive phase。
`apply` 在任何 mutation 前必须在同一 executor 进程内重新读取 live authority 并 re-observe；不能复观测时 fail closed。跨进程时新进程必须重新取得 authority、读取固定 inventory/pins、建立 strict SSH、重新观察；持久化 proof/receipt 只供审计、故障追溯和 rollback 关联，永远不作授权。
#### Crash recovery / resume

如果 executor 在 transaction promotion、backup 或 staging 中途退出，使用下面的 copyable recovery operation；所有输入都必须显式提供，wrapper 不从 transaction、旧 proof 或输出目录猜测或回退：

```bash
export OASIS7_VALIDATOR_PAIR_NONCE_LEDGER=/var/lib/oasis7/p2p-public-testnet/validator-pair-nonces.jsonl
scripts/p2p-public-testnet-rebuild-validators.sh resume \
  --transaction <transaction-dir>/transaction.json --host-adapter <governed-host-adapter> \
  --human-direct-ssh-request <fresh-live-human-stop-request.json> --known-hosts /opt/oasis7/p2p-testnet/config/public-testnet-validator-pair-known-hosts \
  [--credential-env <temporary-env-name> | --credential-fd <temporary-fd>]
```

The request must carry the exact absolute `nonce_ledger_path`, and the exported environment path must be the same already-provisioned `0600` external ledger. `--transaction`, `--host-adapter`, fresh live GitHub-bound request, canonical known-hosts pin, and exactly one temporary credential seam are mandatory. Missing authority, adapter, pin, credential, or nonce binding is a `capability_blocked` input error; persisted quiescence proofs are never resume authority. The wrapper forwards these values unchanged to the executor and publishes the returned transaction through the same durable receipt envelope.

Resume is permitted only for phases `prepared`, `preflight`, `preflight_failed`, `backup_in_progress`, `backed_up`, `staging`, `staged`, and `rollback_required`. In the first seven phases it reauthorizes live GitHub/direct SSH, adopts or cleans only transaction-bound backups, and continues missing callbacks idempotently. `rollback_required` enters governed rollback and must re-observe the stopped boundary before callback or restore. `rolled_back` is terminal with no recovery mutation; `planned`, `applied`, `rollback_failed`, and unknown phases are not resumable.
The plan is deterministic (`oasis7.validator_pair_rebuild_plan.v1`) but performs bounded live read-only GitHub/SSH re-observation; it is not local-only, does not call systemd, and does not perform destructive activity. Runtime transaction IDs and timestamps are assigned only after the plan is durably journaled (each JSON write flushes the file and its parent directory). The transaction is fail-closed and records a complete stopped-state backup manifest before mutation. Mutation order is always `storage-205` then `sequencer-204`; startup order is always `sequencer-204` then `storage-205`.
`--sequencer-proof-verifier` must point to an executable regular file whose SHA-256 and size match the verified package runtime; the executor invokes that exact binary over the raw proof and compares its complete receipt before any host mutation.
The host adapter remains mandatory for the `preflight`, `backup`, `apply`, and `rollback` callbacks; it cannot establish human stop authority or replace direct observation. On automatic or manual rollback, the executor must first reauthorize live human authority and perform a fresh `human_direct_ssh` read-only observation of both fixed validators; only then may the rollback callback and local restore run. Missing/failed re-observation, callback, or restore is recorded as `rollback_failed`; generic adapter `quiesce` can never authorize restore. Each transaction-bound receipt covers active/running, bounded healthz, role listener (`6632/6832` storage-205; `6631/6831` sequencer-204), `NRestarts=0`, no OOM/panic/segfault, exact runtime hash/size, and `full_chain_status_called=false` for 204.
The receipt must consume cryptographically verified identity-v2 envelopes and
their `current_admission` verification receipts, plus the signed 204 proof
under the governance-root-approved allowlists; host self-report alone is not
evidence. The observer gate remains `hold` until provider closure is
independently verified; without this evidence the executor cannot mark
`applied`.
The current pair-executor CLI still exposes the legacy `--identity-receipts`
input and has no identity-v2/raw-map admission option. Do not invent a
replacement flag or pass a v2 file through the legacy shape. The exact v2 map
schema and planner/adapter validation bridge are published in Phase A, but the
pair executor has not integrated that input; this path remains capability
blocked for identity-v2 admission until its own reviewed integration and the
provisioned anchors exist.
Every mutation callback receipt (`backup`/`apply`/`rollback`) must explicitly include `observer_mutation=false`, a non-empty executor-bound `transaction_id`, and the strict current-window binding; human direct observations are not callbacks. `--quiescence-transaction-id` binds only the in-memory observation/audit reference and never substitutes for a callback ID/file/field (`--quiescence-id` is its CLI alias).
The deterministic `plan` has no runtime `transaction_id`; `apply` assigns it after journaling and later receipts echo it exactly. Before each mutation callback, persist `oasis7.validator_pair_rebuild_adapter_binding.v1` fixing immutable `plan_digest`, transaction, phase, the five-node identity-v2 evidence paths/digests with role/node/peer IDs, the exact raw map, raw 204 proof, and current-admission verification receipts; the adapter must echo these bindings plus `captured_at` in-window, or fail closed on omission, substitution, alternate proof, or staleness.
Any failed identity, capacity, health, restart, OOM, panic or segfault gate invokes the recorded same-filesystem rollback and retains its receipt. The pair executor never mutates an observer. A sequencer/204 proof uses a bounded endpoint; full `/v1/chain/status` on 204 is forbidden.

壳脚本的 `oasis7.validator_pair_rebuild_receipt_contract.v1` envelope 将 plan/transaction digest 绑定到每台节点的 platform 与完整无跟随 symlink 的 state-root inventory，并记录精确 destructive target 解析、indexed sidecar/archive/module store、execution/bridge/runtime/replication root 及 observer 等价物、direct read-only observation reference、带 metadata/capacity 的 forensic backup manifest hash 与 machine-checkable `seed_eligible=false` proof。
Apply receipt 必须包含 reset/stage/start 与 same-window fleet-health phase references。rollback boundary 明确禁止把已删除的 chain state 恢复为新 chain seed；plan 输出不包含 runtime timestamp，但会执行上述 bounded live read-only re-observation，并且不执行 systemd 或 destructive activity。

### C0. Consumer-impact record (hard gate)

调用标准重建脚本必须传入 `--consumer-impact-record <path>`。脚本在创建 SSH control connection、执行 host preflight、process cleanup、reset 或 staging 前验证该文件。记录必须是 JSON，格式如下：

```json
{
  "impact": "active | none | unknown",
  "evidence_source": "status/traffic/operator/community evidence reference",
  "timestamp": "2026-07-15T10:30:00+08:00",
  "validators_already_stopped": true,
  "outage_update_channel": "channel/reference or n/a when impact=none",
  "recovery_update_checkpoint": "Phase G snapshot/update reference or n/a when impact=none",
  "producer_wording_approval": "approval reference or n/a when impact=none",
  "decision": "proceed"
}
```

这个 record 是 preflight/reset/redeploy continuation 的硬 gate：

1. `impact` 只能是 `active`、`none` 或 `unknown`；`evidence_source` 必须非空；`timestamp` 必须是带 `Z` 或显式 UTC offset 的 RFC3339 时间；`validators_already_stopped` 必须是 JSON boolean，但不构成停机证据；`decision` 必须严格等于 `proceed`。缺失、malformed JSON、字段类型/值无效或 `decision=hold` 都必须 fail closed。
2. `impact=none` 时，三个 communication 字段仍须存在且非空，但允许使用 `n/a`。
3. `impact=active` 或 `unknown` 时，按存在外部 consumer 处理。TPM 与 LiveOps/community 共同负责 outage 和 recovery updates，`producer_system_designer` 批准对外 wording；`outage_update_channel`、`recovery_update_checkpoint` 和 `producer_wording_approval` 必须非空且不能是 `n/a`。
4. 如果 validator 在 record 生成前已经停止，也不能跳过 determination：在任何 host preflight、目录删除、reset、staging、启动或其他 redeploy continuation 前，先取得 live GitHub human authority，并以固定 production inventory/pins 完成两角色 direct read-only observation。已停止这一事实本身、boolean 或 persisted receipt 都不能推断 `active`、`none` 或恢复完成。
5. 本 record 只授权继续执行受治理的 testnet reset/redeploy，不改变 `testnet`、`resettable`、`non-mainnet` 边界，也不授权承诺旧 chain state、testnet asset 或 mainnet continuity。

### C1. Preflight both validators

consumer-impact record 通过后，canonical `apply` 必须先进入 `preflight` phase，并在任何 backup、reset 或 stage 前调用 governed host adapter；两台 validator 必须由一份 transaction-bound `oasis7.validator_pair_rebuild_host_receipt.v1/v2` 同时闭合。Receipt 必须发出 `phase=preflight`、`observer_mutation=false`、非空 `transaction_id`、当前 `captured_at`、固定 mutation/startup order，以及恰好 `storage-205` 与 `sequencer-204` 两个 node。
每个 node 必须发出 `role`、`root`、`active=false`、`running=false`、`service_state=stopped`、`independently_observed=true`、`preflight_observer_mutation=false`，以及均为 `true` 的 `preflight_verified`、`runtime_executable`、`repair_rebuild_helper_executable`、`generated_world_dir_contract`、`governance_registry_importer_executable`、`python_available`、`tar_available`、`systemd_available`、`process_inspection_available` 字段。任何缺失、role/root mismatch、stale receipt 或 gate=false 都必须 fail closed；未取得完整双 validator receipt 不得进入 backup、reset、stage。

### C2. Reset both validators
以下 stop 是 live authority 下由人类单独执行的动作；`human_direct_ssh`
executor 只在 stop 后 read-only 复观测，再允许受治理 destructive reset：

```bash
systemctl stop oasis7-triad-sequencer.service
systemctl stop oasis7-triad-storage.service
```

人类必须分别停下两台 validator，并由同一 executor 在 reset 前及每次 mutation 前完成 direct read-only re-observation，之后才能 stage config/world。不要在 sequencer reset 后立即 staging，再处理 storage；旧 storage runtime 与新 sequencer staging 不得交错共存。

从第一台 validator 停止到 Phase G full-fleet health criteria 全部通过，按 testnet outage 窗口处理；缓存可读、单个 endpoint 恢复或 sequencer 单机存活都不是网络恢复。

必须清理旧链数据目录，但保留受保护的 `config/node-keypair.toml`，除非本轮明确要轮换 key。
标准重建脚本必须清理 `data/execution-records`、`data/execution-world`、
`data/execution-world-simulator-mirror`、`data/storage`、`data/runtime-root`、
`data/replication-root`、`output/chain-runtime` 和 `output/node-distfs`，避免继承
旧 peerstore、runtime root 或未释放端口。

在删除目录前，脚本必须等待或终止 stack-local 残留 `start-node.sh` / `oasis7_chain_runtime` 进程，避免旧进程继续占用 gossip/status/replication 端口。若 `systemctl stop` 后端口仍被旧 runtime 占用，本轮 rebuild 必须视为未清干净，不能继续把后续 readiness 失败归因于链同步慢。

### C3. Stage both validators
reset 两台 validator 后，才向两台 host stage current package/runtime、manifest、
genesis、validator registry、bootstrap peers 和 deployment-only world。使用上面的
canonical pair transaction；旧 `p2p-public-testnet-rebuild-validators.sh` 只供历史审计，
其 204 sequencer 参数不属于本 contract。两台 host staging 成功后才进入启动步骤。

### C3.1. Validator-47 no-start staging and cold cutover
The managed triad inventory at `scripts/public-testnet-validator-triad-inventory.v1.json` is authority for validator-47 host/service/role/port/provider/world/manifest/registry bindings. Retain its SHA-256 in deployment truth and the bootstrap receipt; stale pair `node.env`, wrong fields, or a different inventory digest fail preflight.
The `--bootstrap-peers-file` input is also authority-bound: it must be byte-identical to `doc/testing/evidence/public-testnet-governed-bootstrap-validator-triad-bootstrap-peers-2026-09-15.txt` (SHA-256 `c7d0b977937adb5d27733ed0ad3e2212ccd0f3ac1b2273214e8cc57df110e5d6`). The inventory's source registry path and digest are checked before any stage output is materialized; a stale, wrong, or merely self-consistent peer/registry file is rejected.
The source registry is a build input (`source_registry_ref`/`source_registry_sha256`), not the runtime registry identity. Stage generation records the exact generated `config/public-testnet-governed-bootstrap-validator-registry-2026-06-06.json` bytes (`generated_registry_sha256`) and its canonical effective-registry digest (`generated_registry_semantic_sha256`); runtime `validator.registry_sha256` must report the generated bytes digest.
The inventory role is `validator`, while runtime `NODE_ROLE=storage` and `P2P_NODE_ROLE=full_storage` are independent supported settings; retain both provider flags in `node.env`. PoS validator identity comes from the governed consensus/registry truth, not an unsupported runtime `NODE_ROLE=validator`. The deployment-stage generator emits fixed triad inventory/source evidence and `deployment_inventory` only for the exact governed sequencer/storage/validator-47 set after identity/signer/stake/registry/bootstrap-peer checks; pair-only or arbitrary extra-validator stages bind their own generated registry by exact ref/raw SHA-256/semantic SHA-256, omit fixed-triad artifacts, and must not enter validator-47 bootstrap. During an existing pair→triad rebuild, the sequencer/storage stage must install that same triad inventory under `config/` and rewrite each existing host's own `node.env` with `DEPLOYMENT_INVENTORY_PATH` plus digest; never copy validator-47 `node.env` onto either host.
The validator-47 node identity is a separately staged ceremony output. Require
its `node-keypair.toml` and public `identity-receipt.json` to be regular files
with mode `0600` and matching ownership before staging. The final host imports
those bytes exactly and validates the public finality key against the governed
registry; it must not regenerate the identity.
Add the inventory's validator-47 signer to the existing pair, then stage and read
back the empty host with this explicit no-start contract:
The following first two commands are the exact stage-to-bootstrap handoff. The
stage's canonical execution world is
`/srv/oasis7/stage/validator-47/generated-world/world/`; pass its containing
`generated-world/` directory directly to bootstrap. Do not hand-copy the world
or rewrite the generated sidecar/provenance paths. The bootstrap receipt records
the consumed layout and remains a no-start artifact.
```bash
./scripts/p2p-public-testnet-build-deployment-stage.sh --runtime-build-ref /srv/oasis7/oasis7_chain_runtime --bootstrap-peers-file /srv/oasis7/bootstrap-peers.txt --sequencer-finality-public-key <sequencer-finality-public-key> --storage-finality-public-key <storage-finality-public-key> --extra-validator triad-testnet-validator-47:<validator-47-finality-public-key>:100 --validator-47-identity-dir /srv/oasis7/staged-validator-47-identity --out-dir /srv/oasis7/stage/validator-47
./scripts/p2p-public-testnet-bootstrap-fresh-validator-host.sh --package-deb /srv/oasis7/oasis7-linux-x64.deb --ops-tools-tar /srv/oasis7/oasis7-linux-x64-ops-tools.tar.gz --config-dir /srv/oasis7/stage/validator-47/config --world-dir /srv/oasis7/stage/validator-47/generated-world --identity-dir /srv/oasis7/stage/validator-47/identity --node-id triad-testnet-validator-47 --service-name oasis7-triad-validator-47.service --receipt /opt/oasis7/p2p-testnet/evidence/fresh-validator-host-bootstrap-receipt.json
```

The independent readback is a separate observation command and must run only
after bootstrap returns successfully:
```bash
/opt/oasis7/p2p-testnet/current/bin/service-readback --read-only --role validator-47 --root /opt/oasis7/p2p-testnet --service oasis7-triad-validator-47.service
```
Bootstrap renders but never enables or starts validator-47 and must prove
`UnitFileState=disabled`, inactive/dead, `no_process=true`, and `no_listener=true`.
Both bootstrap and independent readback perform a bounded scan for stack-local `start-node.sh` or `oasis7_chain_runtime` orphans under the canonical root (including when systemd reports `MainPID=0`); unrelated oasis7 processes outside that root are not treated as validator-47 evidence.
Readback is independent and precedes launch. Keep the existing pair unchanged
(pair preservation); an approved cold cutover is staggered: stop/read back one
member, stage/verify it, then handle the second, never both at once.
On any failure leave validator-47 disabled, preserve the pair, and retain receipt
and readback. After proving no process and no `6634`/`6834` listeners, clean redeploy
from a newly generated stage and newly ceremonied identity; never reuse pair `node.env`,
peerstore, runtime state, or world directories as rollback material.
### C4. Start sequencer and confirm liveness
1. start `triad-testnet-sequencer`
2. confirm sequencer liveness（`running=true` 且 `last_error` 为空）
sequencer liveness 未通过时不得启动 storage，也不得开始恢复 observer 或对外 endpoint。
### C5. Start storage after sequencer liveness
1. start `triad-testnet-storage`
2. confirm storage joins sequencer

### Validator launch invariants
1. `NETWORK_TIER_MANIFEST_PATH` 必须指向 governed bootstrap manifest
2. `GENESIS_VALIDATOR_REGISTRY_PATH` 必须指向 manifest 绑定的当前 deployment truth registry，并通过精确 ref、raw SHA-256 与 semantic SHA-256 校验；非 managed observer 不传 `DEPLOYMENT_INVENTORY_PATH`，managed triad identity 则必须另外提供 manifest 绑定的完整三-validator inventory 及其 SHA-256
3. `EXECUTION_WORLD_DIR` 必须预先放好 deployment truth world
4. runtime binary hash 必须与当前 staged package 一致
5. world staging 必须是“单次传输 + 远端复制”，不能复用同一 stdin tar 流做双重解包

### Deployment artifact retention and cleanup
标准 Linux package 升级入口是 `scripts/p2p-public-testnet-package-node-upgrade.sh`。该脚本默认在每次成功升级后执行 release retention：

1. 永远保留 `<node-root>/current` 解析到的真实 release 目录。
2. 本次升级前的 previous current release 目录也会被保留，作为当前升级尝试的回滚保护。
3. 额外保留 `<node-root>/releases` 下按 mtime 排序最新的 3 个 release 目录。
4. 删除其余非隐藏旧 release 目录；不会把 `<node-root>/data`、`<node-root>/config`、`<node-root>/backups`、service 文件、journal 或 live logs 当作 package 残留清理。
5. 如需临时扩大回滚窗口，可用 `--release-retention-count <N>` 调整最新 release 保留数量；该参数不取消 current realpath 和 previous current 的强保留。
6. 升级事务会记录并在 metadata/current promotion 与 rollback 时原子保留受治理文件的 uid、gid、mode；不得用 root 重写替代原有 ownership/mode 合同。
7. 受限 validator 的替换后存活检查使用显式 `--post-restart-health-url <.../healthz>`，成功只输出 `post_restart_health=ok`；不得在该低成本 gate 中请求完整 `/v1/chain/status`。validator pair 的 readiness、同高与 checkpoint identity 仍按 Phase D 的独立 status/checkpoint 证据闭合。

外层 operator/自动化负责清理脚本作用域之外的上传和 driver 临时目录：

```bash
tmp_upload_dir="/tmp/oasis7-upgrade-${run_id}"
cleanup_upload_dir() {
  if [[ "${OASIS7_KEEP_DEPLOY_TMP:-0}" != "1" ]]; then
    rm -rf "$tmp_upload_dir"
  fi
}
trap cleanup_upload_dir EXIT
```

上传目录如 `/tmp/oasis7-upgrade-<run_id>`、per-run driver 目录如 `/tmp/oasis7-run*-<node>` 必须在部署流程成功或失败收口时清理；只有设置明确 debug keep 开关时才允许保留。`<node-root>/tmp` 仅用于本轮 staging/bundle/bootstrap 临时产物，部署成功后应清空本轮产物。`<node-root>/backups` 不属于 package cleanup，必须按单独的备份保留策略处理。

## 10. Phase D - Verify Validator Pair
### Goal
确认两台 validator 形成可持续推进的 governed chain。

### Required checks
对两台 validator 分别检查：

```bash
# 204/sequencer: use healthz plus the runtime's bounded proof endpoint;
# never request the full /v1/chain/status payload in the pair transaction.
curl -s http://127.0.0.1:6631/healthz | jq '{ok,ready,running,last_error}'
curl -fsS <sequencer-204-bounded-proof-endpoint> |
  jq '{schema_version,observed_at_unix_ms,ok,liveness,readiness,heights,network_head,checkpoint,local_peer_id,connected_peers,connected_peer_count,proof}'
# Save the complete response, run the deployed runtime verifier from section
# 2.2.1 with the governed signer allowlist, and retain its
# oasis7.rebuild_proof_verification.v1 receipt beside the response. Do not
# accept a jq projection, an identity receipt, or the proof's self-declared
# public key as the trust root.
curl -s http://127.0.0.1:6632/v1/chain/status | jq '{running,last_error,committed_height:.consensus.committed_height,last_execution_height:.consensus.last_execution_height,connected_peers:.replication.connected_peers,local_peer_id:.replication.local_peer_id}'
```

### Pass criteria
1. `running=true`
2. `last_error=null`
3. sequencer:
   - `committed_height > 0`
   - `last_execution_height > 0`
4. storage:
   - `committed_height > 0`
   - `network_head.height >= committed_height`
   - `connected_peers` 至少包含 sequencer
5. sequencer 和 storage 互相出现在 `connected_peers`
6. 记录本轮 live `local_peer_id`
7. 若 storage `last_execution_height == 0`，本轮不得把 storage 当作 observer seed source；但只要 storage 已验证并传播 sequencer 的 execution binding，就不应把这件事本身视作 validator rebuild 失败

### Hard stop conditions
出现以下任一项立即停止并回到 Phase B：

1. signer binding mismatch
2. libp2p `WrongPeerId`
3. empty-world `height 1` execution mismatch
4. validator 自身 `BlobNotFound`

## 11. Phase E - Optional Recovery Seed/State-Sync Bundle
### Goal
正常 observer 接入不再要求预先导入 seed。自动 high-head checkpoint catch-up 失败时，先保留故障现场并 `reset-state`；恢复唯一入口是 runtime 生成、签名可验证的 V2 checkpoint protocol artifact。运行中 validator 的目录 raw copy 已禁用，不能作为离线加速或手工恢复来源；shell 不得生成 zero-signature checkpoint，也不得把任何 shell export 标记为恢复或诊断 producer。

### Required contents when this recovery path is used
runtime signed V2 checkpoint protocol 必须生成完整且不可变的 recovery closure，并把 checkpoint descriptor、签名、status、execution record 与 snapshot 内容绑定为同一 identity。closure 至少覆盖：

1. `world/`
2. `execution-records/`
3. `store/`
4. restore snapshot/journal 所需全部 `store/blobs/`
5. 若运行时依赖存在，还要包含:
   - simulator mirror
   - execution bridge state
   - replication head metadata

### Important rule
不要把最小 snapshot state-sync bundle 与完整 seed/restore artifact 混成同一个合同，更不能用 shell 生成的 checkpoint 替代 runtime signed V2 protocol。完整 closure 如果只包含 `world/` 与 `execution-records/`、却没有 restore 所需的 blob 闭包，observer 会在如下阶段失败：

- `restore snapshot ref ... BlobNotFound`
- `restore journal ref ... BlobNotFound`

所以 runtime signed V2 recovery artifact 必须保证 storage closure 完整、checkpoint/status/execution record/snapshot identity 绑定且签名可验证；不能做“最小猜测拷贝”，也不能从运行中 validator 的活目录逐项复制。

### Pass criteria
1. runtime signed V2 checkpoint protocol 产物通过 `p2p-upgrade-preflight.sh --require-state-sync-bundle --verify-state-sync-bundle-semantics --verify-trusted-checkpoint-signatures`，并验证 checkpoint descriptor、status、execution record、snapshot identity 与签名。
2. 若使用完整 recovery artifact，对单个 observer 恢复后 runtime 不报 `BlobNotFound`、`authority_failure` 或 execution-driver mismatch。
3. 若使用完整 recovery artifact，可被所有当前 observers 重复消费；raw live copy 不得作为替代。

在 observer 启动前，完整 seed/restore artifact 必须执行闭包校验：

```bash
./scripts/p2p-verify-state-sync-closure.sh \
  --world-dir <seed-world-dir> \
  --execution-records-dir <seed-execution-records-dir> \
  --store-dir <seed-store-dir>
```

## 12. Phase F - Attach Observers with Automatic High-Head Checkpoint Catch-Up
### Goal
把当前 operator inventory 中的 observers 接进 validator 网络，或在旧状态分叉后从恢复好的 validator/storage fresh state 重新 seed。

### Required prep
1. observer manifest 使用当前 deployment truth bootstrap peer ids；legacy/non-manifest observer env 若仍存在，必须标记为旁路兼容信息而不是 formal startup source
2. observer env 使用当前 deployment truth writer allowlist；除 validator signer 外，必须包含会提供 retained execution checkpoint 的 storage/full-storage provider signer。fetch requester 不再需要逐个 observer 手动加 allowlist，只要 providers 运行的 runtime 对 `public_testnet` + `allow_observer_nodes=true` 开启开放签名读取策略
3. observer manifest 指向当前 deployment truth genesis/manifest，并包含精确的 `deployment_validator_registry` ref、raw digest 与 semantic digest；`p2p-public-testnet-local-observer-sync.sh apply` 必须先把该不可变 registry 本地化到 observer config。三 validator rollout 还必须显式传入并校验匹配的 `--deployment-inventory`（只作输入 authority，非 managed observer env 不输出 inventory），缺失或 drift 必须 fail closed；managed triad 启动仍须额外满足完整 deployment inventory authority
4. observer 的 `WORLD_ID`、governed registry/manifest 及 raw/semantic digest、manifest bootstrap peers、remote writer allowlist、node identity、listen/status ports 必须全部来自当前 deployment truth；managed triad 另须核对 inventory/digest
5. systemd service unit 必须是当前 testnet observer unit；不得让旧 devnet/triad observer service 继续占用状态端口或被误当作 public testnet 节点
6. observer state 可先 reset；正常路径不导入 seed bundle，启动后由 runtime 自动拉取受验证的 high-head replication checkpoint boundary，再执行 tail gap sync。若该路径 fail closed，则保持 no-start，消费 signed V2 checkpoint recovery artifact；不得从运行中的 validator live directory 导入。
7. local observer 使用的 runtime/package hash 必须对齐当前本机 runtime 真值；不要直接复用 validator Linux package hash 去校验本地 macOS debug/release binary
8. detached local observer 启动时不要让 `start-node.sh` 自己作为长期父进程驻留；若需要后台常驻，应直接启动 `logs/last-command.sh` 里展开后的 runtime binary 命令

### Start / reseed order
1. `triad-testnet-local` on Linux LAN, then verify.
2. `triad-testnet-windows-observer` on Windows, then verify.
3. `triad-testnet-fourth-local` on macOS, then verify.
4. If all observers were seeded before a validator recovery, reseed all affected observers again from the recovered storage/sequencer state.

### Required checks
```bash
ssh <linux-lan-observer> 'curl -fsS http://127.0.0.1:6633/v1/chain/status' \
  | jq '{node_id,running,last_error,readiness:.readiness.status,failed_gates:.readiness.failed_gates,committed_height:.consensus.committed_height,network_committed_height:.consensus.network_committed_height,last_execution_height:.consensus.last_execution_height,connected_peers:.replication.connected_peers}'
ssh <windows-observer> 'powershell -NoProfile -Command "Invoke-RestMethod -UseBasicParsing http://127.0.0.1:5121/v1/chain/status | ConvertTo-Json -Compress -Depth 8"' \
  | jq '{node_id,running,last_error,readiness:.readiness.status,failed_gates:.readiness.failed_gates,committed_height:.consensus.committed_height,network_committed_height:.consensus.network_committed_height,last_execution_height:.consensus.last_execution_height,connected_peers:.replication.connected_peers}'
curl -fsS http://127.0.0.1:19083/v1/chain/status \
  | jq '{node_id,running,last_error,readiness:.readiness.status,failed_gates:.readiness.failed_gates,committed_height:.consensus.committed_height,network_committed_height:.consensus.network_committed_height,last_execution_height:.consensus.last_execution_height,connected_peers:.replication.connected_peers}'
```

Use each node's actual `STATUS_BIND` from its env/deploy metadata when it differs from the legacy examples above.

### Pass criteria
1. `running=true`
2. 所有当前 observers 都能看到 validator peer
3. `last_error=null`
4. `committed_height` 和 `last_execution_height` 向 validator 高度收敛

### Hard stop conditions
出现以下任一项，observer 接入失败；先保留现场排查自动同步原因，确认需要人工恢复时再回到 Phase E：

1. `WrongPeerId`
2. `fetch requester is not authorized`；这通常表示 validator 仍在旧 runtime、manifest 不是 `public_testnet`、`allow_observer_nodes` 未开启，或 requester 签名无效，而不是需要手动登记每个 observer
3. `BlobNotFound`
4. `height 1 peer commit execution mismatch`
5. `network tier runtime bundle hash mismatch`

## 13. Phase G - Final Health Verification
### Goal
分别判定“节点已跑起来”和“整个 fleet 真正 ready”，不混淆两者。

### Required final snapshot
需要同时保留所有 current operator inventory 节点的原始状态快照；204 使用 bounded proof，不能用 full-status 路径抄表：

- 204 proof：`liveness.running`、`liveness.last_error`、`heights.committed_height`、`heights.network_committed_height`、`heights.last_execution_height`、`connected_peers`、`readiness.status`、`readiness.failed_gates`、`network_head.decision`，以及原 `checkpoint` 和独立 proof-verification receipt。
- storage / observers full status：`running`、`last_error`、`consensus.committed_height`、`consensus.network_committed_height`、`consensus.last_execution_height`、`replication.connected_peers`、`readiness.status`、`readiness.failed_gates`、`consensus.network_head.decision`。
- 缺失字段必须记为采集/结构错误，不能把错误路径得到的 `null` 当作无故障或 ready；合法 `liveness.last_error: null` 与字段缺失分别记录。原始 proof 和独立 verification receipt 必须保留，投影本身不提供验证或部署 authority。

在作出 `Fleet healthy` 或 recovery update 结论前，必须保留一个
bounded same-window artifact。204 的输入先按 2.2.1 采集、独立验证并
保留 proof/identity-v2 evidence。五节点闭包还必须保留每个 managed node
的 raw-v1 map entry、context、plan-intent、prepare manifest、immutable
payload、provider attestation、envelope、current-admission verified
envelope 和 verification receipt。缺少任一节点或任一 digest binding，
不得形成 `Fleet healthy` 或 recovery 结论。Use the retained Phase A proof,
proof-verification receipt, and section 7 identity-v2 artifacts; do not rerun a
legacy sidecar command here.

The existing `p2p-public-testnet-fleet-health.py` collector is status-shaped
and has no signed-proof input. Do not pass it a 204 `/v1/chain/status` URL or
silently feed an unverified jq projection under the `sequencer` name. Until a
proof-aware collector adapter is separately approved, its
`--managed-five-node` closure must remain blocked after the independently
verified 204 receipts are retained; storage and observer status samples may
still be collected for non-204 diagnostics. A blocked collector is safer than
claiming same-window fleet health from an unauthenticated 204 payload.

`--managed-five-node` is mandatory for this deployment closure. It fail-closes unless the supplied identities are exactly `sequencer` (ECS 204 provider), `storage` (ECS 205 provider), `linux-lan-observer`, `windows-observer`, and `macos-observer`, once each. The collector without that flag remains only for explicitly non-closure diagnostics and cannot be used for a `Fleet healthy` or recovery conclusion.

### Verdict rules
1. **Fleet live**
   - 当前 operator inventory 中所有节点都在跑
   - validator 与 observer 都已接入
   - 高度在收敛或已收敛

2. **Fleet healthy**
   - 在满足 fleet live 的基础上
   - `last_error=null`
   - 不存在阻断性 gate
   - observer 不再依赖缺失 blob / 缺失 state-sync artifact
   - `readiness.status=ready`
   - `readiness.failed_gates=[]`
   - `consensus.committed_height`、`consensus.network_committed_height`、`consensus.last_execution_height` 与 validator head 对齐或在允许 lag 内
   - `consensus.network_head.decision=ready`
   - 没有 repository-defined lag threshold 时，所有 current operator inventory 节点必须在**同一个 same capture window** 的快照中，使 `consensus.committed_height`、`consensus.network_committed_height`、`consensus.last_execution_height` 都与 sequencer validator head **exactly equal**；不得把不同采样时间的“分别已追平”拼成 fleet healthy 结论

如果只满足第一条，不得对外宣称 healthy、恢复完成或“完全健康”。

### Operator communication boundary

1. **Merge announcement: n/a.** 合并 runbook、代码或 package 变更不等于 live deployment 已执行，不发布“网络已恢复”或“部署已完成”的 merge announcement。
2. Phase C0 的 consumer-impact record 是 deployment/outage messaging 的来源。`active` 或 `unknown` 都必须按存在外部 consumer 处理：TPM 与 LiveOps/community 负责 outage 和 recovery updates，`producer_system_designer` 批准 wording，并在任何 host preflight/reset/redeploy continuation 前记录 update channel、recovery checkpoint 和 approval reference。只有 `none` 且 evidence source 与 timestamp 已留存时，才可只保留内部 operator 记录。validator 已停止也不豁免这项 determination 或沟通 gate。
3. 需要对外说明时，必须使用 `testnet`、`resettable`、`non-mainnet` 边界，例如：`Oasis7 public testnet is undergoing a governed clean rebuild. This testnet is resettable and non-mainnet. Validators are temporarily unavailable; RPC, explorer, and guarded faucet may be unavailable or stale until full-fleet verification completes.`
4. validator pair、单个 observer、RPC、explorer 或 faucet 单独恢复都不能触发 healthy announcement。只有本 Phase 的 `Fleet healthy` 全部满足、每个节点的 `consensus.network_head.decision=ready`、并留存满足上述 exact-equality same-capture-window head criterion 的 full-fleet snapshot 后，才可由已记录的 recovery update checkpoint 说明服务恢复；恢复说明仍必须保留 `testnet`、`resettable`、`non-mainnet` 边界，不得承诺旧 chain state、testnet asset 或 mainnet continuity。

## 14. Phase H - Failure Handling and Rollback
### Deployment rollback
如果 validator 新包启动失败：

1. 切回上一版 `current` symlink
2. 恢复上一版 manifest/config
3. 保留失败现场日志和当前 deployment truth 快照

上述 package/config rollback 只恢复 binary 与配置选择，不会恢复 Phase C 已删除的 chain state。只要 destructive reset 已发生，就不得把 symlink/config 回切描述为链状态回滚或完整恢复；canonical recovery 是先修正 package/config/deployment truth，再按 Phase C clean rebuild validator pair，并按 Phase F reset/reseed 所有受旧链状态影响的 observers。

### Observer rollback
如果 observer 接入失败：

1. 不改 validator 链真值
2. 保留 observer 当前 seed/state-sync 失败现场
3. reset 受旧链状态影响的 observer，再从 clean-rebuilt storage/sequencer state 自动 catch-up 或 signed V2 current-chain recovery artifact 恢复
4. 不得使用绑定旧链状态的 seed bundle，也不得保留旧 observer state 重新接入 clean-rebuilt validator pair

### Windows package rollback 后的人工 handoff

Windows package rollout 只要输出 `rollback_required=true`，就进入仍在进行中的 incident；`rollback_applied=true` 只证明已尝试恢复 known-good binary/config/deployment provenance，不代表 observer 已恢复，也不得触发 recovery messaging。值班 operator 必须先保留 rollout 输出列出的 `attempt_stdout`、`attempt_stderr`、`attempt_exit_marker` 及 `C:\oasis7-deploy\logs\package-rollout-attempts\` 下对应 attempt 文件；不得在 incident closeout 前删除 attempt staging、覆盖日志或只摘录最后一行替代原始证据。随后采集 `staged_sha_closure_complete=true`、`promotion_begin=true`、`promotion_complete=true` 三个 marker 是否存在，以及 `Get-ScheduledTask` 的实际 state/action，并按下列分支 handoff：

1. **Preflight / closure failure, no restart.** 若缺少 `staged_sha_closure_complete=true` 或 `promotion_begin=true`，则 promotion/stop boundary 未获证明；即使 task 当前为 `State=Ready`，也不得执行 `Start-ScheduledTask`。保留 preflight/closure diagnostics，修复 staging closure、路径、hash、rollback root 或 task-action precondition 后重新生成并执行一次受控 rollout；若观察到 `State=Running`，也只保留现场并排查，不得以手工 restart 覆盖 preflight failure。
2. **Promotion entered but not completed.** 若有 `promotion_begin=true` 但没有 `promotion_complete=true`，保留所有 marker 和 attempt diagnostics，并把 observed task state 作为 stop 的事实来源：生成器没有独立的 stop marker，不能从 `promotion_begin=true` 推断 task 已停止。只有同时得到 `rollback_required=true`、`rollback_applied=true`，并确认 task 为已停止的 `State=Ready` 时，才进入下一分支；`State=Running`、缺失 state 或任何其他 state 都继续 incident，禁止手工 start。
3. **Restored post-stop rollback.** 仅当 `promotion_begin=true` 已记录、`rollback_applied=true` 已记录、installed/known-good runtime SHA-256、active governed config、`CURRENT_VERSION` / `DEPLOYED_BUILDINFO` 和 scheduled task action 都已闭合，且当前观察到 `State=Ready` 时，Windows node operator 才可人工执行 `Start-ScheduledTask -TaskName Oasis7Observer`。rollout 的“不自动重启”是安全边界；不得用修改脚本或删除诊断绕过。若已观察到 `State=Running`，不得再次 Start，先采集状态和日志。
4. 在任何手工 start 后，运行本 runbook `Current five-node status` 的现有 Windows/fleet status checks，并记录 scheduled task `State=Running`、RPC `running=true`、`last_error=null`、`readiness.status=ready`、`readiness.failed_gates=[]`、每个节点 `consensus.network_head.decision=ready`，以及满足本 Phase exact-equality same-capture-window criterion 的 `consensus.committed_height`、`consensus.network_committed_height`、`consensus.last_execution_height`。任一项不满足时，`rollback_required=true` incident 继续 active。
5. 在 task running、error clear、readiness、network-head decision 和 full-fleet same-window head checks 全部通过且证据已留存前，禁止发布“已恢复”“healthy”或同义 external messaging。外部 recovery update 仍遵循上文 `Operator communication boundary`：TPM 与 LiveOps/community 准备并发布 update，`producer_system_designer` 批准 wording；Windows operator 的本机检查结果本身不构成对外发布批准。

最小 Windows handoff 命令：

```powershell
Get-ScheduledTask -TaskName Oasis7Observer | Select-Object TaskName,State,Actions
Get-Content -LiteralPath '<attempt_stdout>','<attempt_stderr>','<attempt_exit_marker>'
Get-ScheduledTask -TaskName Oasis7Observer | Select-Object TaskName,State
Invoke-RestMethod -UseBasicParsing http://127.0.0.1:5121/v1/chain/status |
  ConvertTo-Json -Depth 8
```

### What not to do
1. 不要为 observer attach 问题去修改 validator registry 真值
2. 不要在 peer id 已变的情况下继续使用旧 bootstrap peer list
3. 不要在缺少完整 blob 闭包时宣布 state-sync 已可用
4. 不要手工把 sequencer 的 checkpoint、execution world、execution records、storage 或 replication root 拷到 storage validator 上来“同步”。这种做法绕过了 manifest/bootstrap peer/replication 协议和 validator signer binding，不能作为 testnet 恢复路径。
5. 不要在一次手工 copy 后继续等待它“自然变 ready”并把结果记为自动同步；live raw copy 已禁用，必须隔离既有状态，回到自动恢复或 signed V2 checkpoint recovery。

### Allowed recovery choices
当 validator pair 出现 `readiness=not_ready`、peer-head stale、state-sync fallback 或 replication transport degraded 时，operator 只能按下面顺序处理：

1. **自动恢复**：保持 current deployment truth 不变，修复 manifest bootstrap peers、端口占用、runtime 进程残留、network reachability 或 provider discovery 后，让节点通过 replication/head exchange 自己追平。
2. **从零重建**：如果自动恢复被 signer drift、runtime bundle hash drift、stale bootstrap peer id、stale peerstore 或本地链状态污染阻断，则重新生成 deployment truth，清空 validator runtime data/root，按 Phase C 重建 validator pair。
3. **signed V2 checkpoint recovery**：只适用于 observer/light-node recovery 或经验证的 break-glass restore drill；必须有签名 checkpoint、validator-set proof、snapshot/status/latest identity binding、bundle manifest 和闭包校验。它不是 validator-to-validator 手工同步，也不允许 live raw copy。

## 15. Standard Command Checklist
### Preflight
For current five-node fleet preflight, first load each managed node's real env/deploy metadata and use its actual status bind. The legacy `.tmp/testnet-*-bootstrap/node.env` paths below are only valid when intentionally rebuilding those bootstrap staging directories.

Complete the Phase A 204 proof and independent verifier steps first. Also
complete the section 7 four-command identity-v2 sequence for the exact managed
node set and retain its `current_admission` verification receipts. The
preflight consumes the resulting bounded projection file; it must not treat a
legacy sidecar output or a `historical_audit` receipt as admission evidence.
`--sequencer-status-url` must not point at the 204 full-status endpoint.
Use the Phase A capture and the existing preflight entrypoint with its current
deployment-truth arguments; this section intentionally does not duplicate that
long command block.

### Package version replacement
```bash
./scripts/p2p-public-testnet-package-rollout.py \
  --manifest .tmp/public-testnet-package-rollout/nodes.json \
  --package-dir .tmp/testnet-packages/<run-id> \
  --known-hosts <operator-local-pinned-known-hosts> \
  --out-dir .tmp/public-testnet-package-rollout \
  --readiness-policy rpc-running
```

默认模式只生成计划，不改节点；脚本会校验各平台 `*-BUILDINFO` 和 `*-SHA256SUMS`，并输出 Linux/Windows operator 命令和 `rollout-plan.json`。当 manifest 包含远端 canonical Linux provider 时，`--known-hosts` 必须指向 operator 本机的非空 pinned host-key regular file，不得填写 provider 上的 `/opt/oasis7/p2p-testnet/...` 路径；脚本会在创建输出目录、信任校验或任何 transport/probe 前 fail closed。本地 Linux 节点需要显式加 `--apply-local` 才会调用 `p2p-public-testnet-package-node-upgrade.sh` 执行替换；远端 SSH、Windows PowerShell 和凭据注入仍由 operator 在脚本外执行，manifest 不写密码。

Windows 计划要求 `windows-x64-SHA256SUMS` 覆盖并传输受治理的 bundle、genesis、network manifest、bootstrap peers、`governance_bootstrap_refs` 中全部非空 evidence 源，以及 public-testnet 必需的 world snapshot、递归 generated-world sidecar 和 world-generation provenance。每个带 `host` 的远端 Windows node manifest entry 必须显式声明非空 `rollback_backup_root`，并指向该节点已验证的时间戳备份目录；计划生成不得为远端节点隐式回退到 `backups\known-good`。每个计划按 node/version/commit/run 派生唯一 attempt id，installer、完整 governed closure 和 upgrade script 只能传到 `C:\oasis7-deploy\staging\package-rollout\<attempt-id>`；pre-apply 命令不得写 `config/`、install root 或其他 active deployment 路径。每次传输前先在远端创建并确认 staging 父目录，传输后按精确预期 SHA-256 远端复核并输出 `staging_transfer_ack`；全部传输闭合后，apply 必须使用精确的 `user@host` SSH target，并通过同步的 `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File <staged-script>` 子进程从该 attempt staging root 执行脚本，再把子进程 `$LASTEXITCODE` 原样返回给 SSH。远端 PowerShell 在停止 scheduled task 或执行 installer 前初始化唯一、只追加的 stdout、stderr、exit-marker 诊断，完成 staged 文件与目录树完整性、来源路径、basename 冲突及已配置 known-good rollback root 预检，并依次输出 `staged_sha_closure_complete=true`、`promotion_begin=true`；只有此后才能停止 task、安装 runtime 或把 staged config 通过临时文件提升到 active tree，全部提升完成后输出 `promotion_complete=true`。

rollback runtime 优先从唯一的 `backup-manifest.json` 或 `backup-provenance.json` runtime path 解析，并在声明 SHA-256 时闭合校验；没有 manifest/provenance 时仅支持按顺序检查 `runtime/oasis7_chain_runtime.exe`、`bin/oasis7_chain_runtime.exe`，两者同时存在或都不存在均 fail closed，且最终路径必须保持在 rollback root 内。该 root 还必须提供与 node-local 相同相对路径的 `config/` bundle/genesis/manifest，以及 `CURRENT_VERSION`、`DEPLOYED_BUILDINFO`。脚本把所有已知 path-bearing bundle/manifest 字段改写为 active node-local 路径，并通过结构化路径分类拒绝残留 Unix/build-worktree 或非 deploy/install root 路径，随后用无 BOM UTF-8 临时文件原子替换本地化配置。常规替换使用 `rpc-running`，在 60–300 秒有界窗口内同时要求 runtime process 与 RPC `running=true`；`strict-ready` 另要求可用的 readiness truth 为 true。scheduled-task child 一旦终止且结果非零（包括 `255`），脚本立即保留并输出本次诊断文件的精确路径、传播 child exit code，并以 `rollback_required=true` 进入 known-good rollback。rollback 在任何 runtime 解锁等待前先比较 installed 与 known-good SHA，并输出 `rollback_runtime_restore_required`：hash 相等时跳过 runtime unlock/replacement；hash 不等时才执行有界解锁和 runtime 恢复。config 与 deployment provenance 也逐项比较 SHA，只恢复发生变化的 component，并输出 `rollback_component_restored` 或 `rollback_component_unchanged`。整个路径保留 identity/state；锁仍阻断确需执行的 runtime 恢复时才以 `lock_remains=true` fail closed，且不自动重启。

### Validator status
Use the retained Phase A proof, proof-verification receipt, and section 7
identity-v2 artifacts; the current five-node status commands below provide the
additional live health samples.

### Current five-node status
```bash
curl -fsS http://39.104.204.172:6631/v1/chain/rebuild-proof
ssh root@39.104.205.67 'curl -fsS http://127.0.0.1:6632/v1/chain/status'
ssh <linux-lan-observer> 'curl -fsS http://127.0.0.1:6633/v1/chain/status'
ssh <windows-observer> 'powershell -NoProfile -Command "Invoke-RestMethod -UseBasicParsing http://127.0.0.1:5121/v1/chain/status | ConvertTo-Json -Compress -Depth 8"'
curl -fsS http://127.0.0.1:19083/v1/chain/status
```

For the 204 proof, record `schema_version`, `liveness.running`, `liveness.last_error`,
`readiness.status`, `readiness.failed_gates`, the three fields under
`heights`, `network_head.decision`, `checkpoint`, `local_peer_id`, and the
`oasis7.rebuild_proof_verification.v1` receipt. Record the separate identity
receipt's `peer_id`, key SHA-256, size, mode, uid, and gid. For storage and
observers, record `CURRENT_VERSION`, runtime hash or artifact lineage,
`running`, `last_error`, `readiness.status`, `readiness.failed_gates`, the
consensus heights, and `consensus.network_head.decision`.

### Peer id truth
Use the Phase A bounded proof/status capture and the governed bootstrap-peer
artifact; do not derive peer IDs from an unverified status projection.

### Seed closure
Use the Phase E closure command and retain its receipt; this checklist does not duplicate the recovery command.

## 16. Open Design Follow-Ups
这份 runbook 可以让部署更稳，但它也明确保留两项后续硬化方向：

1. 自动 high-head checkpoint sync 已覆盖 observer/light-node 的正常接入；execution-required 节点仍不能跳过历史执行
2. runtime signed V2 checkpoint recovery closure 必须作为唯一标准 artifact 提供，并持续验证完整闭包、immutable identity binding 与签名；不得再依赖 live raw copy 或 shell-generated checkpoint

所以后续必须补两类自动化：
1. 空 world observer 自动 high-head checkpoint sync 对 validator truth 的回归测试
2. signed V2 checkpoint recovery artifact 的闭包与 identity-binding 回归测试，至少覆盖 restore snapshot/journal 所需 blob 闭包和并发写入拒绝

## 17. Completion Boundary
这份 runbook 的交付物是“标准流程”，不是单次 live 执行记录。

每次真实执行仍须额外记录：

1. 当前 runtime/package hash
2. 当前 validator signer truth
3. 当前 validator live peer ids
4. 自动 high-head checkpoint catch-up 状态快照；若使用 recovery path，再记录 runtime signed V2 recovery artifact 的来源、identity 与 hash
5. 最终 current operator inventory 全节点状态快照

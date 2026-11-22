# Public Testnet Governed Bootstrap — Phase C Clean Rebuild Validators

- Main runbook: [`public-testnet-governed-bootstrap.runbook.md`](public-testnet-governed-bootstrap.runbook.md)
- This companion runbook carries Section 9 (`Phase C - Clean Rebuild Validators`) split from the main runbook under #6507 without changing any operational rules, commands, or verification boundaries.

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


# oasis7: Game World State Sync and Commit Closure Test Plan

- 对应设计文档: `doc/testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md`
- 可变任务状态与历史: PR、实际 CI 与评审记录（Issue 按需）

审计轮次: 1

## 1. Executive Summary
- Problem Statement: 现有模块级测试能覆盖 runtime 执行、receipt、state hash、node/net/consensus/distfs 合同，但不能单独证明游戏世界状态在多节点提交后能持续同步、追高、恢复，并投影到 API/viewer。
- Proposed Solution: 建立一套从单节点合同到三节点 proxy、五节点真实游戏长跑、state-sync/blob closure、real-env readiness 的分层测试方案，专门承接 `action -> consensus -> execution -> receipt -> committed world state -> peer/gap/state sync -> API/viewer projection`。
- Success Criteria:
  - SC-1: 明确 `module_required`、`module_full`、`integration_required`、`release_full` 四档 claim boundary。
  - SC-2: 每档都有现有命令入口、产物目录、通过标准和不可声明边界。
  - SC-3: 多节点验证至少覆盖 sequencer、storage、observer/validator 角色，且检查 committed height、consensus hash、peer heads、gap sync、blob closure、observer catch-up。
  - SC-4: 任何 release/public-testnet 级结论都必须有同窗口 real-env 或 public_testnet readiness lane 证据。

## 2. User Experience & Functionality
- User Personas:
  - QA engineer: 需要判断状态同步/提交链路是否能放行到更高验证层。
  - Runtime engineer: 需要定位 commit、receipt、state hash、checkpoint、rollback 和 replay 失败。
  - Blockchain ops engineer: 需要确认多节点部署、observer 追高、state-sync bundle 与 readiness lane。
  - Release owner: 需要清楚知道模块绿灯不能冒充真实多节点 ready。
- User Scenarios & Frequency:
  - runtime/world-state 改动: 至少执行 `module_required`。
  - node/net/consensus/distfs/state-sync 改动: 执行 `module_required` 并按影响面追加 `module_full`。
  - 修复 peer-head stale、gap sync、commit divergence、blob closure 类问题: 执行 `module_full`，必要时追加 `integration_required`。
  - public_testnet 或 live-candidate claim: 执行 `release_full`。
- Critical User Flows:
  1. Flow-GWSC-001: `提交 action -> 形成 execution record/receipt -> committed_height 推进 -> state hash 可追溯`
  2. Flow-GWSC-002: `sequencer commit -> storage/validator/observer peer heads 更新 -> gap sync/state sync 追高`
  3. Flow-GWSC-003: `checkpoint/export bundle -> blob closure 验证 -> observer 从 seed/checkpoint 自动恢复`
  4. Flow-GWSC-004: `API/viewer status/projection -> 与 committed world state 对账 -> 写入 evidence packet`
- Functional Specification Matrix:
| 功能点 | 字段定义 | 执行动作 | 状态转换 | 通过规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| Commit contract | `action_id`, `execution_record`, `receipt`, `state_hash`, `committed_height` | 跑 runtime required/full 与定向 persistence/replay 测试 | `submitted -> executed -> committed` | replay same input same result; receipt/event sequence 可追溯 | runtime engineer owns |
| Multi-node commit propagation | `network_height`, `committed_height`, `consensus_hash`, `peer_heads` | 跑 S9A `module_required` 和 mixed-topology matrix | `local_commit -> network_seen -> peer_synced` | committed height 单调推进，consensus hash 一致，peer heads 非空新鲜 | QA gates, runtime/node/net owners fix |
| Gap/state sync closure | `checkpoint_height`, `checkpoint_hash`, `state_sync_bundle`, `missing_blob_count` | export bundle 并跑 closure verifier | `seeded -> verified -> recoverable` | missing blob count = 0; checkpoint manifest 与 status 一致 | runtime + blockchain ops |
| Observer/API/viewer projection | `/v1/chain/status`, `world_state_projection`, `viewer_status` | 采样 API/status 与 viewer/projection evidence | `synced -> projected -> auditable` | projection 与 committed world state 同窗口一致 | viewer/runtime/QA |
| Recovery and chaos | `restart`, `pause`, `disconnect`, `rollback`, `catch_up_latency` | 跑 S9 longrun chaos 或 S10 soak | `fault -> recovery -> caught_up` | no consensus divergence; no stale execution; catch-up within gate | QA blocks release on failure |
- Acceptance Criteria:
  - AC-1: `module_required` 不得只跑单节点 runtime；必须包含 node/net/libp2p/consensus/distfs 和 mixed-topology required。
  - AC-2: `module_full` 必须包含三节点 proxy 或 triad 类长跑，并检查 consensus/gap/blob/peer-head 指标。
  - AC-3: `integration_required` 必须使用真实游戏 world state 或 seed snapshot，验证 action 到 API/viewer projection 的完整链路。
  - AC-4: `release_full` 必须使用 real-env/public_testnet readiness evidence，且证据必须同窗口。
  - AC-5: release gate 中的 `insufficient_data` 不得当作 pass；跳过 S9/S10 后不得写成完整 release coverage。
  - AC-6: 组合验证计划必须独立保留 GWSC 设计中的 17 QA negative cases、10 runtime scenario groups 和 8 SC-9 cells；每个执行结论按 [testing QA receiver](../prd.md#qa-infrastructure-receiver) 绑定当前 source/candidate、environment、oracle、raw evidence 和 tier。结构/schema通过不能代替语义断言或实际执行。
- Non-Goals:
  - 不把本方案改造成所有 PR 默认必跑的重型 gate。
  - 不用单模块、单节点或 dry-run 结果声明真实多节点 ready。
  - 不替代玩法好玩性、UI 视觉质量或 L5 真实玩家验证。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用。本方案是 runtime/network/testing 验证计划，不依赖 AI 推理系统。
- Evaluation Strategy: 不适用。

## 4. Technical Specifications
- Architecture Overview: 本方案把 S1/S3/S4 的模块合同、S9A 的链上大世界状态底座自闭环、S9B mixed-topology、S9 longrun、S10 five-node soak 和 state-sync closure 脚本串成一条可声明边界清晰的验证矩阵。
- Existing Command Entry Points:
```bash
./scripts/game-world-state-sync-commit-module-required.sh
./scripts/network-tier-public-testnet-readiness.sh --manifest <manifest> --lanes-tsv <lanes.tsv>
```
- 等价展开命令：
```bash
env -u RUSTC_WRAPPER cargo test -p oasis7 --tests --features test_tier_required
env -u RUSTC_WRAPPER cargo test -p oasis7_node
env -u RUSTC_WRAPPER cargo test -p oasis7_net --lib
env -u RUSTC_WRAPPER cargo test -p oasis7_net --features libp2p --lib
env -u RUSTC_WRAPPER cargo test -p oasis7_consensus --lib
env -u RUSTC_WRAPPER cargo test -p oasis7_distfs --lib
./scripts/p2p-mixed-topology-matrix.sh --tier required
```
```bash
./scripts/p2p-mixed-topology-matrix.sh --tier full
./scripts/p2p-longrun-soak.sh --profile soak_smoke --topologies triad --duration-secs 600 --no-prewarm
./scripts/p2p-verify-state-sync-closure.sh --world-dir <seed-world-dir> --execution-records-dir <seed-execution-records-dir> --store-dir <seed-store-dir> --out <report.json>
./scripts/s10-five-node-game-soak.sh --duration-secs 300 --no-prewarm --max-stall-secs 240 --max-lag-p95 50 --out-dir .tmp/release_gate_s10
```
- Environment Constraints:
  - `p2p-longrun-soak.sh` 和 `s10-five-node-game-soak.sh` 需要 Bash 4+；macOS 默认 Bash 3.2 会 fail-fast。
  - heavy longrun/soak 不应隐式进入所有 PR required gate；必须按 claim 强度显式执行。
- Evidence Artifacts:
  - `summary.json`, `summary.md`, `timeline.csv`, `failures.md`
  - `nodes/*/{command.txt,stdout.log,stderr.log}`
  - `chaos_events.log`, `feedback_events.log`
  - state-sync closure evidence packet copied from `doc/testing/templates/state-sync-closure-evidence-packet-template.md`
  - state-sync closure report JSON
  - S10 `summary.json` 中的 `api_viewer_projection` object 与 `summary.md` 中的 `API / Viewer Projection Contract` section
  - public_testnet readiness lane `api_viewer_projection_ready`
  - `/v1/chain/status` 同窗口采样和 API/viewer projection 截图或 JSON
- Blocker Signatures:
  - `consensus_hash_divergence`
  - `committed_height_not_monotonic`
  - `known_peer_heads_zero_samples`
  - `http_failure_samples`
  - `sequencer_committed_height_zero`
  - `sequencer_execution_stale_height`
  - stale peer-head / stale execution
  - `missing_blob_count > 0`
  - `observer_catch_up_failed`
  - readiness lane `partial` / `block`
  - manifest 是 example/template/placeholder/private-only endpoint
  - 非同窗口 real-env 证据
  - 手工复制 validator `data/`、checkpoint 或 seed 造成的同步假象
- Pass Criteria:
  - S9/S9A: command `rc=0`, `overall_status == "ok"`, `topology_failed_count == 0`, `committed_height` 单调推进, `consensus_hash_consistent == true`, `consensus_hash_mismatch_count == 0`。
  - State-sync closure: execution/state hash、receipt/event sequence 可追溯；peer heads 非空且新鲜；gap sync 成功；replication error 不持续；blob/store closure 完整；observer 自动追高。
  - S10: `summary.json` 标记 run ok，metric gate pass，`timeline.csv` 存在；settlement apply failure ratio、DistFS failure ratio、lag、mint/asset invariant 均在阈值内。
- Module-only Non-Claims:
  - S1/S4/S9B required exact 绿，只能声明本地合同/确定性子系统可集成。
  - 不得声明多节点 world state sync 已成立、commit 在真实拓扑中稳定、observer catch-up 已可靠、public_testnet ready、physical NAT/CGNAT 已覆盖、真实公网可达、游戏整机体验成立或 `release_full` 可放行。
  - S9/S9B proxy 绿不得冒充 dedicated sentry/NAT lab 或真实公网证据。
  - 多节点分叉/漂移不得以重启恢复收口；必须定位根因并在 clean rebuild/redeploy 后复验。
- Non-Functional Requirements:
  - NFR-GWSC-1: 所有证据必须能回溯到 commit、world id、node ids、运行窗口和命令。
  - NFR-GWSC-2: failure signatures 必须可复现或可归档为 follow-up task。
  - NFR-GWSC-3: claim boundary 必须写入 evidence summary，避免把未执行层级当作覆盖完成。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP (GWSC-1): 文档方案落地，并作为 S9A/S10 的补充阅读入口。
  - v1.1 (GWSC-2): 增加一键 `module_required` wrapper，避免手工漏跑 node/net/libp2p。
  - v1.2 (GWSC-3): 增加 state-sync closure evidence packet 模板。
  - v2.0 (GWSC-4): 将 S10 five-node soak 的 state projection/API 对账纳入 summary。
  - v2.1 (GWSC-5): 对 real-env/public_testnet lanes 接入同窗口 projection evidence。
- Technical Risks:
  - 风险-1: 单机 proxy triad 不能代表 physical NAT/CGNAT 或 dedicated sentry lab。
  - 风险-2: longrun 资源噪声造成误报，需要保留机器/端口/窗口上下文。
  - 风险-3: 缺少真实 seed world 时，integration_required 容易退化成底座自测。
  - 风险-4: state-sync bundle 验证通过但 observer 未自动追高，仍不能声明 integration ready。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-TESTING-LONGRUN-GWSC-001 | GWSC-1/2 | `module_required` | S1/S4 + mixed-topology required | commit/state-sync 基础合同 |
| PRD-TESTING-LONGRUN-GWSC-002 | GWSC-2/3 | `module_full` | mixed-topology full + triad longrun + state-sync closure | 多节点追高和恢复 |
| PRD-TESTING-LONGRUN-GWSC-003 | GWSC-3/4/5 | `integration_required` / `release_full` | S10 + real-env readiness + API/viewer projection | 真实游戏世界状态提交与投影 |
| PRD-TESTING-LONGRUN-GWSC-004 | GWSC verification receivers | `module_required` / `integration_required` / `release_full` | Keep all 17 QA cases, 10 runtime groups and eight SC-9 cells separately traceable; record structural validation separately from semantic oracle results and execution state using [QA method](../design.md#qa-infrastructure-evidence-method) and [QA tier boundary](../prd.md#qa-infrastructure-tier-boundaries). | Same-candidate state-sync, execution, consumer and full-environment evidence; missing required cells remain blocked/unverified. |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-GWSC-001 | 将方案落在 `doc/testing/longrun/` | 只写在聊天或 task log | 状态同步/提交闭环属于长稳与多节点测试治理，需要正式可检索入口。 |
| DEC-GWSC-002 | 四档 claim boundary | 单一 pass/fail | 避免 module green 被误读为 release ready。 |
| DEC-GWSC-003 | 多节点为必需条件 | 单节点 runtime 代替多节点 | 状态同步、peer head、gap sync、observer catch-up 无法由单节点证明。 |
| DEC-GWSC-004 | state-sync closure 使用独立 evidence packet 模板 | 只依赖 closure report JSON | blob closure、peer heads、observer catch-up 和 manual-copy 边界需要同包审查，否则容易把局部 closure 误读成多节点追高通过。 |
| DEC-GWSC-005 | S10 summary 固定输出 API/viewer projection contract 字段 | 只在人工证据中描述 projection | release/integration 评审需要稳定读取 `api_viewer_projection`；默认 `not_collected`，避免 soak metrics 被误读为 projection pass。 |
| DEC-GWSC-006 | public_testnet readiness 增加 `api_viewer_projection_ready` active lane | 仅依赖 claims boundary review 文字说明 | release/public_testnet 级声明必须有同窗口 API/viewer projection evidence；缺 lane 或模板 evidence 不能 pass。 |

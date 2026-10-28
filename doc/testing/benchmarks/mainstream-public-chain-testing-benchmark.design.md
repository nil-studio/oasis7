# oasis7 主流公链测试体系对标与缺口矩阵（设计文档）

- 对应需求文档: `doc/testing/benchmarks/mainstream-public-chain-testing-benchmark.prd.md`
- 可变任务状态与历史: PR、实际 CI 与评审记录（Issue 按需）
- 原始基准日期: `2026-03-24`

审计轮次: 2

> Authority boundary: this design preserves the original benchmark background
> and dated follow-up observations only. Current network-tier status and
> blockers live in `doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.prd.md`
> and its companion runbook. A dated controlled `ready_for_live_candidate`
> packet is bounded to resettable, non-mainnet testnet candidacy; it does not
> establish public launch, a currently operating public fleet, open validator
> onboarding, or mainnet readiness.
> Historical update (2026-07-03): the external verifier /
> light-client-lite operator path gained bounded sampled world-head verification.
> That historical closure did not establish full light-client security,
> multi-client consensus equivalence, current network readiness, or release
> maturity. The dated gap snapshot remains at
> `doc/p2p/blockchain/p2p-current-mainstream-public-chain-gap-benchmark-2026-07-03.md`.

## 设计目标
- 将“主流公链怎么测”的经验整理为 oasis7 可执行的对标矩阵。
- 明确当前已有测试层、关键缺口和 producer 下一步优先级。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [bench-layer-model](mainstream-public-chain-testing-benchmark.prd.md#bench-layer-model) | SC-1 / AC-1 / NFR-2：六层 mainstream expectation 与 oasis7 equivalent；单实现不伪造第二客户端 | [des-bench-layers](#des-bench-layers) | QA / runtime | 不证明任何运行层或独立客户端已通过；产品适用性 N/A，reason=历史对标资料维护，scope=本行专业机制，公开边界另列 |
| [bench-historical-snapshot](mainstream-public-chain-testing-benchmark.prd.md#bench-historical-snapshot) | SC-2..4 / AC-2..4 / NFR-1：原日期基础与缺口，不更新为今天 | [des-bench-snapshot](#des-bench-snapshot) | QA / network-tier owners | 命令存在不等于已执行；不更新 current fleet；产品适用性 N/A，reason=历史对标资料维护，scope=本行专业机制，公开边界另列 |
| [ac-p2p-bench-005](mainstream-public-chain-testing-benchmark.prd.md#ac-p2p-bench-005) | AC-5：三份 finality pass/block/restore 独立样本，保留更广缺口 | [DES-BENCH-G1 scope](#des-bench-g1-scope) | QA / runtime / custody / ceremony owners | 未重放 live drill；不证明完整 custody/ceremony/slot/failover；产品适用性 N/A，reason=历史对标资料维护，scope=本行专业机制，公开边界另列 |
| [ac-p2p-bench-006](mainstream-public-chain-testing-benchmark.prd.md#ac-p2p-bench-006) | AC-6：带日期矩阵供 producer 排序；动态状态回 formal authority | [DES-BENCH-L5 boundary](#des-bench-l5-boundary) | producer / network-tier owners | 不证明当前 public launch、fleet 或 mainnet；产品适用性 N/A，reason=历史对标资料维护，scope=本行专业机制，公开边界另列 |
| [bench-producer-priority](mainstream-public-chain-testing-benchmark.prd.md#bench-producer-priority) | SC-5 / NFR-3：G1–G4 的严重度、owner、next action 与既有顺序 | [des-bench-gaps](#des-bench-gaps) | producer / QA / runtime / LiveOps | 不启动或完成这些 workstreams；产品适用性 N/A，reason=历史对标资料维护，scope=本行专业机制，公开边界另列 |
| [bench-manual-entry](mainstream-public-chain-testing-benchmark.prd.md#bench-manual-entry) | AC-7：共享 testing-manual 中存在本专题入口 | [des-bench-gaps](#des-bench-gaps) | QA / shared manual owner | 不修改共享手册或测试 tier；产品适用性 N/A，reason=历史对标资料维护，scope=本行专业机制，公开边界另列 |
| [REQ-ENTRY-CLAIM-001](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-001) | 历史/局部 green 不得生成当前公开事实；Trace refs: O7-REQ-e20ed8c7-6f8d-400a-a883-57b2e772acff | [des-bench-public-window](#des-bench-public-window) | QA 提供受限输入；producer / LiveOps 审核公开 claims | 不覆盖全渠道发布、撤回、LiveOps 审核、软件回滚或四层全域验收 |
| [REQ-ENTRY-CLAIM-003](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-003) | 事实、目标、假设、证据边界分开；Trace refs: O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385 | [des-bench-public-categories](#des-bench-public-categories) | QA 提供受限输入；producer / LiveOps 审核公开 claims | 不覆盖全渠道发布、撤回、LiveOps 审核、软件回滚或四层全域验收 |


<a id="des-bench-layers"></a>

## 主流公链测试分层模型
| Layer | 主流公链常见做法 | oasis7 等价要求 |
| --- | --- | --- |
| `L0 spec/reference` | 规范向量、格式/状态转换参考样例、静态门禁 | 文档治理、工件一致性、协议/治理 registry reference fixture |
| `L1 deterministic core` | unit/integration/property/状态机类测试 | `required/full`、runtime/simulator/viewer live/unit integration、后续补 fuzz/property gate |
| `L2 distributed system` | 多节点/网络/复制/升级兼容 | `S4` + `S9/S10` + world/consensus/distfs 多节点一致性 |
| `L3 user-facing closure` | 钱包/浏览器/节点运维路径真实可用 | `S6` Web-first UI 闭环、producer playtest、viewer auth 路径 |
| `L4 longrun/chaos/drill` | 长跑、故障注入、事故演练、key rotation/validator failover | `S9/S10` 长跑 + governance rotation/revocation/failover 实际 drill |
| `L5 network/release train` | devnet/testnet/canary、共享环境升级演练、发布列车 | 等价映射包括 formal `public_testnet` readiness、mainnet gates 和 legacy `shared_devnet` rehearsal evidence；具体当前状态回到各自专业权威 |

<a id="des-bench-snapshot"></a>

## oasis7 历史映射快照
| Layer | 基准窗口 / 后续 scoped evidence | 证据 / authority | 结论 |
| --- | --- | --- | --- |
| `L0 spec/reference` | 已具备 | `./scripts/doc-governance-check.sh`、专题 PRD/project/design、registry import/audit 工具 | `present` |
| `L1 deterministic core` | 已具备，但缺 fuzz/property gate | `./scripts/ci-tests.sh required/full`、`main-token-regression.sh`、crate tests | `present_with_gap` |
| `L2 distributed system` | 已具备基础，但以库测和长跑为主 | `S4`、`S9`、`S10`、node/net/consensus/distfs tests | `present_with_gap` |
| `L3 user-facing closure` | 已具备 | `S6` Web-first UI 闭环、producer playtest 手册 | `present` |
| `L4 longrun/chaos/drill` | 长跑具备；后续窄范围 default/live `governance.finality.v1` drill 已记录 `2-of-3` pass、negative block 与 baseline restore，但不覆盖完整治理、custody 或 ceremony | `S9/S10`；`doc/testing/evidence/governance-registry-live-world-drill-finality-2026-03-24.md` 及下方历史补充 evidence | `partial` |
| `L5 network/release train` | legacy `shared_devnet` 仅作历史 rehearsal provenance。正式网络 readiness 由 network-tier authority 的独立 gate 决定；2026-07-06 记录的 11-lane packet 只支持 controlled/resettable/non-mainnet `ready_for_live_candidate`，不表示公开 launch、当前 live fleet 或 mainnet。每次当前状态须重新查正式 authority | `doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.runbook.md`；2026-07-06 dated packet | `historical_evidence_only` |

## 后续窄范围 governance evidence（证据记录：2026-03-24；本次复核：2026-09-26）

<a id="des-bench-g1-scope"></a>

以下记录作为原始 benchmark 快照的 scoped follow-up evidence 单独列出（source records dated 2026-03-24；本次复核 dated 2026-09-26）。它们修正“首轮真实 finality drill 仍未执行”的旧描述；范围仍限于 `governance.finality.v1` 的列明样本，不证明当前 fleet 状态、完整 signer custody、genesis ceremony、广泛 slot/failover 覆盖或 release readiness。

| 样本 | 观察结果 | 边界 |
| --- | --- | --- |
| [`governance-registry-live-world-drill-finality-2026-03-24.md`](../evidence/governance-registry-live-world-drill-finality-2026-03-24.md) | `signer03 -> signer04` rotation 保持 `2-of-3` 并通过；negative `2-of-2` audit 返回 `failover_blocked`；restore audit 回到 `ready_for_ops_drill`。 | 单一 finality rotation、threshold block 和 baseline restore 语义。 |
| [`governance-registry-live-world-drill-finality-multi-loss-rejoin-2026-03-24.md`](../evidence/governance-registry-live-world-drill-finality-multi-loss-rejoin-2026-03-24.md) | `signer01 + signer02` dual-loss 在 import policy 阶段被拒绝；baseline restore audit 通过。 | 不覆盖仍可写入 registry 的复杂 partial failover 或非-baseline rejoin 变体。 |
| [`governance-registry-live-world-drill-finality-baseline-rejoin-signer02-2026-03-24.md`](../evidence/governance-registry-live-world-drill-finality-baseline-rejoin-signer02-2026-03-24.md) | temporary offline 的 `signer02` 在 `2-of-2` degraded state 被 block，baseline rejoin 与 restore audit 回到 `ready_for_ops_drill`。 | 单一 signer temporary-offline / baseline-rejoin 样本。 |

因此，L4 不再把“首轮 default/live finality drill 没有 evidence”作为 blocker；L4 仍为 `partial`，因为这些记录没有关闭更广治理、custody、ceremony 和 long-run / release-train 证据。

<a id="des-bench-gaps"></a>

## 原始缺口经后续证据收窄后的剩余范围

SC-5 的内部排序沿用下方 [Producer 结论](#des-bench-l5-boundary) 的四项顺序；本节保留 G1–G4 的 severity、owner 和 next action。AC-7 的专题入口在 [testing-manual](../../../testing-manual.md) 中只读核对，不改共享手册、tier 或执行选择。

| Gap ID | 缺口 | 严重度 | owner | 下一步 |
| --- | --- | --- | --- | --- |
| `BENCH-G1` | 更广的 governance / negative-drill QA 范围仍不完整；窄范围 default/live finality pass/block/restore 已有记录，但 signer custody、genesis ceremony、更多 slot 与 failover 变体未由这些样本关闭 | `high` | `qa_engineer` + `runtime_engineer` | 扩充代表性 failure/restore 样本，并由对应 custody、ceremony 与 QA authority 验证；不得把已完成的 finality 样本写成整体 G1 完成 |
| `BENCH-G2` | fuzz/property-based gate 缺失 | `medium` | `runtime_engineer` + `qa_engineer` | 先定义最小 fuzz/property 切入面，再决定工具 |
| `BENCH-G3` | formal `public_testnet` candidate 与 `mainnet` readiness 使用独立 network-tier gates；legacy `shared_devnet` pass 不可替代目标网络状态。受控 candidate packet 不等于 public launch 或 mainnet readiness | `high` | `producer_system_designer` + `liveops_community` + `runtime_engineer` | `public_testnet` 当前状态只读 formal network-tier runbook 与当轮 required-lane packet；mainnet 另走 exit review、`MAINNET-1~4` 与 frozen-genesis / ceremony gates |
| `BENCH-G4` | 多客户端公链的“独立实现差分测试”在 oasis7 无等价替代 gate | `medium` | `runtime_engineer` | 评估独立 replay/verifier 或只读审计器路径 |

<a id="des-bench-l5-boundary"></a>

## Producer 结论
1. 原始快照已经不是“完全没有测试体系”，而是有一套偏 preview-to-hardening 的测试骨架；后续 scoped finality evidence 又补上了有限的真实 pass/block/restore 样本。
2. 这些证据没有关闭完整 `L4` 治理/custody/ceremony 范围，`L5` 当前状态也不由 benchmark 维护；成熟度判断不能外推成 live fleet 或 release 结论。
3. 因此下一步优先级应是：
   - `扩充 governance drill evidence，并补齐 custody / ceremony 范围`
   - `negative/fault drill`
   - `network rehearsal / release-train readiness`
   - `fuzz/property gate`

<a id="des-bench-public-window"></a>

## 测试成熟度表述边界

Trace refs: O7-REQ-e20ed8c7-6f8d-400a-a883-57b2e772acff

局部承接 [REQ-ENTRY-CLAIM-001](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-001)：历史 finality pass、QA green 与 dated candidate 均不能单独生成当前公开事实。适用范围仅为本 benchmark 的测试成熟度输入；同版本、窗口、入口与 README envelope 仍由公开 claims authority 审核，不承接发布、撤回、软件回滚或全渠道 LiveOps 流程。

<a id="des-bench-public-categories"></a>

Trace refs: O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385

局部承接 [REQ-ENTRY-CLAIM-003](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-003)：六层原始快照是历史事实，三份 drill 是带日期的窄观察，producer 顺序是计划，当前 network tier 是外部 authority 的独立结论。下面支持/不支持的措辞只是受限示例；不把目标、假设或已定位源码当成当前可发布事实，不覆盖全部文案与四层组合验收。

- 以下只示范本 benchmark 支持或否定的测试成熟度表述；当前 release/network claims 仍须核对产品 claims authority 与 formal network-tier runbook。
- 本 benchmark 支持的成熟度表述：
  - `limited playable technical preview`
  - `crypto-hardened preview`
  - `testing foundations exist, but mainstream public-chain-grade testing maturity is not yet complete`
- 本 benchmark 不支持的成熟度表述：
  - `mainstream public-chain-grade testing`
  - `mainnet-grade testing maturity`
  - `production release train is established`

## 11 验证映射

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [bench-layer-model](mainstream-public-chain-testing-benchmark.prd.md#bench-layer-model) | [des-bench-layers](#des-bench-layers) | SC-1 / AC-1 / NFR-2：六层 mainstream expectation 与 oasis7 equivalent；单实现不伪造第二客户端 | 逐层核对六行及 PRD evidence limits；S0 静态  [testing-manual](../../../testing-manual.md)；| 六层映射与单实现限制逐项记录 | 不证明任何运行层或独立客户端已通过 |
| [bench-historical-snapshot](mainstream-public-chain-testing-benchmark.prd.md#bench-historical-snapshot) | [des-bench-snapshot](#des-bench-snapshot) | SC-2..4 / AC-2..4 / NFR-1：原日期基础与缺口，不更新为今天 | 逐项核对命令、手册和 dated snapshot；S0 静态  [testing-manual](../../../testing-manual.md)；| 六层历史状态、来源路径与日期 | 命令存在不等于已执行；不更新 current fleet |
| [ac-p2p-bench-005](mainstream-public-chain-testing-benchmark.prd.md#ac-p2p-bench-005) | [DES-BENCH-G1 scope](#des-bench-g1-scope) | AC-5：三份 finality pass/block/restore 独立样本，保留更广缺口 | 逐读三个 2026-03-24 evidence，核对 rotation、dual-loss、signer02 rejoin 各 outcome；S0 静态  [testing-manual](../../../testing-manual.md)；| 三份 linked drill summaries 与措辞复核 | 未重放 live drill；不证明完整 custody/ceremony/slot/failover |
| [ac-p2p-bench-006](mainstream-public-chain-testing-benchmark.prd.md#ac-p2p-bench-006) | [DES-BENCH-L5 boundary](#des-bench-l5-boundary) | AC-6：带日期矩阵供 producer 排序；动态状态回 formal authority | 核对 2026-07-06 runbook packet 的 controlled/resettable/non-mainnet 边界；S0 静态  [testing-manual](../../../testing-manual.md)；| 历史六层和当前 authority 分离记录 | 不证明当前 public launch、fleet 或 mainnet |
| [bench-producer-priority](mainstream-public-chain-testing-benchmark.prd.md#bench-producer-priority) | [des-bench-gaps](#des-bench-gaps) | SC-5 / NFR-3：G1–G4 的严重度、owner、next action 与既有顺序 | 逐项核对四 gap 与 Producer 四项顺序；S0 静态  [testing-manual](../../../testing-manual.md)；| 原 owner、优先级与独立残余清单 | 不启动或完成这些 workstreams |
| [bench-manual-entry](mainstream-public-chain-testing-benchmark.prd.md#bench-manual-entry) | [des-bench-gaps](#des-bench-gaps) | AC-7：共享 testing-manual 中存在本专题入口 | 只读检索 testing-manual.md 的 mainstream-public-chain-testing-benchmark 入口；S0 静态  [testing-manual](../../../testing-manual.md)；| 入口定位及目标路径记录 | 不修改共享手册或测试 tier |
| [REQ-ENTRY-CLAIM-001](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-001) | [des-bench-public-window](#des-bench-public-window) | 历史/局部 green 不得生成当前公开事实；Trace refs: O7-REQ-e20ed8c7-6f8d-400a-a883-57b2e772acff | 按 AC-ENTRY-CLAIM-001 静态核对历史 snapshot / dated candidate / 示例措辞负例；S0 文档候选  [testing-manual](../../../testing-manual.md)；| 同 Trace 局部审核记录；五 MD inventory 与文档检查结果 | 不覆盖全渠道发布、撤回、LiveOps 审核、软件回滚或四层全域验收；没有实际发布验收 |
| [REQ-ENTRY-CLAIM-003](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-003) | [des-bench-public-categories](#des-bench-public-categories) | 事实、目标、假设、证据边界分开；Trace refs: O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385 | 按 AC-ENTRY-CLAIM-003 静态核对历史 snapshot / dated candidate / 示例措辞负例；S0 文档候选  [testing-manual](../../../testing-manual.md)；| 同 Trace 局部审核记录；五 MD inventory 与文档检查结果 | 不覆盖全渠道发布、撤回、LiveOps 审核、软件回滚或四层全域验收；没有实际发布验收 |


## 决策、风险与结构边界

本设计组织测试对标资料，不引入 runtime 架构、协议、状态机或 API。六层模型对应资料结构，历史快照与 evidence 表对应判读流程，G1–G4 和 producer 顺序对应原有决策输入；不启动所列执行工作流。测试层 L4/L5 不等于 playability L4/L5。

产品适用性：N/A；reason=分层字段、历史资料组织和内部排序本身不增加玩家承诺；scope=des-bench-layers、des-bench-snapshot、des-bench-g1-scope、des-bench-gaps 的专业资料维护。公开事实与分类边界分别由 des-bench-public-window / des-bench-public-categories 承接，不能以本 N/A 排除。

源码/文档定位基线为 `a0b6ab12ad14c6c881e48cecdb4adbdb322dd30e`。S0、`scripts/doc-governance-check.sh`、`scripts/ci-tests.sh`、`scripts/main-token-regression.sh` 和 `testing-manual.md` 的 benchmark 入口已定位；本次只核读源字节与文档静态结构，不执行这些运行套件。三个 2026-03-24 drill 的各自 pass/block/restore 保持原日期；2026-07-03 verifier 观察与 2026-07-06 network packet 仍是历史输入，不是新的验证日期或当前安全结论。

剩余风险由原 gap owners 处理：更广 custody/ceremony/slot/failover、fuzz/property gate、独立实现差分与当前 release-train readiness 均不能因文档规范化关闭。当前网络判断只回正式 PRD/runbook，不在本历史矩阵改状态。

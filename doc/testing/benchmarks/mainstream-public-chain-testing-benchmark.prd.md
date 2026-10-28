# oasis7 主流公链测试体系对标与缺口矩阵

- 对应设计文档: `doc/testing/benchmarks/mainstream-public-chain-testing-benchmark.design.md`
- 可变任务状态与历史: PR、实际 CI 与评审记录（Issue 按需）
- 原始基准日期: `2026-03-24`

审计轮次: 1

> Authority boundary: this benchmark preserves the original 2026-03-24 testing-
> maturity snapshot and explicitly scoped follow-up evidence. It must not
> maintain current `shared_devnet`, `public_testnet`, `mainnet`, release, or
> live-fleet status. Current network-tier status belongs to
> `doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.prd.md` and its
> companion runbook. A dated `ready_for_live_candidate` result means only a
> controlled, resettable, non-mainnet `public_testnet` candidate; it does not
> mean public launch, a currently operating public fleet, open validator
> onboarding, mainnet readiness, or production settlement.

## 1. Executive Summary
- Problem Statement: 在原始 2026-03-24 基准窗口，oasis7 已有签名交易、生产 signer custody、治理 signer 外部化和创世 ceremony 的专题规格，也已开始治理 registry import/audit/runbook；团队仍缺少说明主流公链测试分层、该窗口覆盖层级及剩余缺口的正式基准。若继续只用零散 required/full、长跑和 drill 结果沟通，很容易把“局部门禁已存在”误判成“整体测试体系已接近主流公链”。
- Proposed Solution: 由 testing 专业域维护测试体系对标 PRD，把主流公链常见测试层拆成 `spec/reference -> deterministic/unit/integration -> distributed/multi-node -> ui/playability -> longrun/chaos/drill -> network rehearsal / release-train readiness` 六层，再把 oasis7 可追溯的命令、证据与缺口映射为带日期的 benchmark snapshot；producer 只据此决定后续 execution workstreams 的优先级，不由本 benchmark 维护当前网络状态或成熟度。
- Success Criteria:
<a id="bench-layer-model"></a>

  - SC-1: 明确给出不少于 6 层的“主流公链测试体系”分层模型，并区分“单客户端项目的等价替代要求”与“多客户端公链的原生做法”。
<a id="bench-historical-snapshot"></a>

  - SC-2: 明确给出 oasis7 在基准窗口的 `已有 / 部分具备 / 缺失` 测试矩阵，而不是只给抽象建议；该快照不代表当前运行状态。
  - SC-3: 明确指出原始基准窗口的强项至少包括：`required/full 基础门禁`、`分布式子系统库测试`、`Web-first UI 闭环`、`长跑套件`、`governance registry audit + drill runbook`。
  - SC-4: 明确指出原始基准窗口的缺口至少包括：`fuzz/property-based gate 缺失`、formal `public_testnet` / `mainnet` readiness 需要独立 gate、`governance / ceremony evidence 未充分`；后续窄范围 drill evidence 与当前 tier status 分别按 design 和正式 network-tier authority 解读。
<a id="bench-producer-priority"></a>

  - SC-5: 形成 producer 可直接使用的后续顺序：`补齐剩余治理 drill / ceremony evidence -> 扩充 fault/chaos/negative gate -> 再补 network rehearsal / release-train readiness`；已记录的窄范围 finality drill 不能当作整体治理或 release-train 收口。

## 2. User Experience & Functionality
- User Personas:
  - `producer_system_designer`：需要知道 oasis7 的测试体系离主流公链还差哪些层，避免过早升级阶段口径。
  - `qa_engineer`：需要把“主流公链怎么测”翻译成 oasis7 的 `test_tier_required` / `test_tier_full` / S4 / S6 / S9 / S10 / governance drill 组合。
  - `runtime_engineer`：需要知道 execution workstreams 应补哪些类型的验证，而不只是继续加功能。
  - `liveops_community`：需要知道对外为何仍不能说“对标主流公链安全/测试成熟度”。
- User Scenarios & Frequency:
  - 每次有人提“现在是不是已经接近主流主链测试体系”时，先对照本专题。
  - 需要更新测试成熟度评估时，新增带日期、范围和来源的比较；当前 `MAINNET-1~3` 执行进度留在相应 PR/Task，不回填覆盖原始基准窗口。
  - 每次准备做更高安全口径或创世前评审时，先复查 network rehearsal、drill、chaos、QA 证据是否到位。
- User Stories:
  - PRD-P2P-BENCH-001: As a `producer_system_designer`, I want one explicit benchmark for mainstream public-chain testing systems, so that oasis7 phase decisions are grounded in testing reality rather than intuition.
  - PRD-P2P-BENCH-002: As a `qa_engineer`, I want oasis7 suites mapped to benchmark layers, so that release gates and missing evidence are visible.
  - PRD-P2P-BENCH-003: As a `runtime_engineer`, I want the missing testing layers frozen as execution backlog, so that hardening work includes validation design instead of only code paths.
  - PRD-P2P-BENCH-004: As a `liveops_community`, I want public claims tied to benchmark coverage, so that the project does not overstate maturity.
- Critical User Flows:
  1. Flow-P2P-BENCH-001: `读取 testing-manual 与安全/readiness 专题 -> 提炼 oasis7 当前测试层 -> 对照主流公链分层模型 -> 形成 gap matrix`
  2. Flow-P2P-BENCH-002: `有人提出“接近主流公链测试体系” -> 检查 benchmark layers -> 若关键层仍缺失则直接 block 该口径`
  3. Flow-P2P-BENCH-003: `读取 drill/chaos/network-rehearsal 的固定窗口证据 -> 比较测试成熟度及剩余边界 -> 当前执行排序回到相应 PR/Task`
- Functional Specification Matrix:
| 功能点 | 字段定义 | 动作行为 | 状态转换 | 计算/判定规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 主流公链分层模型 | `layer_id/layer_name/mainstream_expectation/oasis7_equivalent` | 冻结主流链常见测试层，并给出 oasis7 的等价要求 | `draft -> frozen` | 单客户端项目不得照搬“多客户端”字面要求，必须给出本项目等价替代层 | `producer_system_designer` 拍板，`qa_engineer` 联审 |
| oasis7 历史映射快照 | `layer_id/coverage_at_snapshot/evidence_paths/status` | 将基准窗口内的 `required/full/S4/S6/S9/S10/governance audit` 映射到 benchmark | `unknown -> mapped` | 必须引用仓库内已有命令、手册或专题文档；当前网络状态由其正式 authority 查询 | `qa_engineer` 主持，producer 收口 |
| 缺口矩阵 | `gap_id/layer_id/severity/owner/next_action` | 为每个缺失层给出 owner、优先级和下一动作 | `identified -> prioritized` | 若缺口直接影响安全口径或创世 gate，则至少记为 `high` | `producer_system_designer` 拍板 |
| 口径门禁 | `claim_phrase/min_layer_status/reject_reason` | 根据 benchmark 覆盖状态决定是否允许某类外部表述 | `draft -> enforced` | 只要 network rehearsal、chaos/drill 或真实 ceremony evidence 未完成，就不得宣称“对标主流公链测试成熟度” | `liveops_community` 执行，producer 审批 |
- Acceptance Criteria:
  - AC-1: 本专题必须明确写出主流公链测试体系至少包含六层：`spec/reference`、`deterministic/unit/integration`、`distributed/multi-node`、`ui/playability`、`longrun/chaos/drill`、`network rehearsal / release-train readiness`。
  - AC-2: 本专题必须明确写出原始基准窗口已具备 `S0/S1/S2/S4/S6/S9/S10 + governance registry audit/runbook` 这一类基础，但这些不足以推导出“主流公链级测试体系已完成”，也不构成当前运行状态。
  - AC-3: 本专题必须保留原始基准窗口的判断：当时仓库没有冻结成正式 gate 的 fuzz/property-based 测试层。
  - AC-4: 本专题必须保留原始基准窗口的判断：当时仓库没有共享 `devnet/testnet/canary release train` 的正式执行层；后续 network-tier candidate 状态由独立 authority 管理，不改写此历史测试成熟度快照。
<a id="ac-p2p-bench-005"></a>
  - AC-5: 本专题必须保留原始基准窗口对 drill / ceremony 缺口的记录，并引用后续窄范围 default/live finality drill 的 pass/block/restore evidence；这组 evidence 不等于 signer custody、genesis ceremony、广泛 failover 覆盖或整体 QA 收口。详见 design 的历史补充证据节。
<a id="ac-p2p-bench-006"></a>
  - AC-6: 本专题必须输出一份带日期的 `已有/部分具备/缺失/下一步` 矩阵，可供 producer 排序；动态 task 与网络状态不由此矩阵维护。
<a id="bench-manual-entry"></a>

  - AC-7: `testing-manual.md` 必须能找到本专题入口，避免测试分层与对标口径分离。
- Non-Goals:
  - 不在本专题内直接实现 fuzz 框架、legacy shared_devnet rehearsal / formal test environment 或新的 chaos 平台。
  - 不用本 benchmark 升级任何当前 network tier、release 或 live-fleet verdict。
  - 不替代具体 execution workstream 的详细测试卡。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用。
- Evaluation Strategy: 不适用。

## 4. Technical Specifications
- Architecture Overview: 本专题属于“解释 + 对标 + 执行优先级”文档，不新增协议。它把 `testing-manual.md`、`p2p-mainnet-crypto-security-baseline`、`p2p-mainnet-grade-readiness-hardening`、`p2p-governance-signer-externalization` 当前已存在的测试与门禁，映射到主流公链更完整的测试体系上，判断 oasis7 目前在哪一层做得够、哪一层还没有正式形成 release gate。
- Integration Points:
  - `testing-manual.md`
  - `doc/p2p/blockchain/p2p-mainnet-security-governance-readiness.prd.md`
  - `README.md`
  - `doc/product/player-entry-distribution/release-communications-and-public-claims.prd.md`
  - `GitHub Issue / GitHub Project`
<a id="bench-evidence-limits"></a>

- Edge Cases & Error Handling:
  - 若引用“主流公链多客户端一致性测试”，必须同时说明 oasis7 当前是单实现栈，因此等价要求应落在 `独立 replay/verifier`、`跨 world/节点副本一致性` 与 `共享网络升级演练`，而不是伪造不存在的第二客户端。
  - 若 oasis7 某层已有命令，但没有固定成 release gate、没有周期执行、没有证据模板，则该层最多只能记为 `partial`。
  - 若某项 drill 只有 clone-world 脚本，没有 default/live execution world 真实证据，则仍不得记为完成。
  - 若某项 network rehearsal 能力只有本地单机 smoke，没有共享节点、升级窗口或版本漂移管理，则不得记为 `devnet/testnet/release train complete`。
- Non-Functional Requirements:
  - NFR-P2P-BENCH-1: 所有“已有/缺失”判断都必须能回链到仓库命令、手册或正式专题文档。
  - NFR-P2P-BENCH-2: benchmark 必须同时给出 `mainstream expectation` 与 `oasis7 equivalent`，避免机械照搬其他链结构。
  - NFR-P2P-BENCH-3: benchmark 的下一步必须按 producer 视角排序，明确什么是当前最高杠杆验证工作。
  - NFR-P2P-BENCH-4: benchmark 结论必须与根 `README.md` 的当前公开状态和产品层公开口径规则一致，不得制造第二套安全成熟度口径。
- Security & Privacy: 本专题只记录测试层与验证缺口，不记录任何私钥、助记词、离线签名材料或 operator 敏感信息。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP: 建立 benchmark 文档，冻结原始基准窗口的 oasis7 映射与缺口矩阵。
  - v1.1: 扩充已存在的窄范围 finality drill evidence，继续补代表性 governance failure、signer custody、genesis ceremony 与 QA 证据。
  - v1.2: 补 network rehearsal / release-train readiness / 升级演练定义。
  - v2.0: 再决定是否需要引入 fuzz/property-based gate 或独立 verifier 路径。
- Technical Risks:
  - 风险-1: 若把 `required/full` 当成“主流公链完整测试体系”，会低估 network rehearsal、事故 drill、release train 的工程成本。
  - 风险-2: 若只在 clone-world 做 drill，不在真实 default/live execution world 留证据，readiness 评估会继续停在 spec-heavy 状态。
  - 风险-3: 若不把 fuzz/property-based 或 chaos 层明确记为缺口，后续很容易一直不补。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-P2P-BENCH-001 | BENCH-0/1 | `test_tier_required` | benchmark PRD/project/design 建档、分层模型冻结、模块入口映射 | producer 安全/测试阶段判断 |
| PRD-P2P-BENCH-002 | BENCH-1/2 | `test_tier_required` | `testing-manual`/governance/readiness 文档映射、suite 对位与 gap matrix 固化 | QA release gate 与证据体系 |
| PRD-P2P-BENCH-003 | BENCH-2/3 | `test_tier_required` | 缺口优先级、owner 与 execution next step 冻结 | runtime/QA 后续验证设计 |
| PRD-P2P-BENCH-004 | BENCH-3 | `test_tier_required` | public-claims 边界复核与对外口径回链 | liveops 对外成熟度表述 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-P2P-BENCH-001 | 用“主流公链测试分层 + oasis7 等价映射”做 benchmark | 直接照抄多客户端公链 checklist | oasis7 当前是单实现栈，必须给出等价要求才有执行意义。 |
| DEC-P2P-BENCH-002 | 把 governance audit/runbook 与窄范围 default/live finality drill 记为正向证据，但不等于完整治理 drill 或 ceremony 收口 | 因为已有 import/audit 工具或单条 pass case 就直接记整体完成 | 后续 evidence 已记录 `2-of-3` finality pass、negative gate block 和 baseline restore；custody、ceremony 与更广覆盖仍需独立证据，详见 design 的历史补充证据节。 |
| DEC-P2P-BENCH-003 | 把 `network rehearsal / release-train readiness` 记为当前明显缺口 | 继续只在本地/clone-world dry-run 评估 readiness | 主流公链的成熟度很大一部分来自共享环境、升级窗口和持续演练。 |

## 测试成熟度的局部产品承接

<a id="bench-public-window"></a>

Trace refs: O7-REQ-e20ed8c7-6f8d-400a-a883-57b2e772acff

本条仅承接 [REQ-ENTRY-CLAIM-001](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-001) 的历史或局部 green 不得单独生成公开事实这一边界。AC-5 的三份 dated finality 样本、AC-6 的历史矩阵与 controlled/resettable/non-mainnet candidate 不能外推为当前 fleet、public launch、open validator onboarding 或 mainnet；不承接全渠道发布、撤回或 LiveOps 审核。

<a id="bench-public-categories"></a>

Trace refs: O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385

本条仅承接 [REQ-ENTRY-CLAIM-003](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-003) 的事实、目标、假设与证据边界分离。原始六层快照、后续带日期观察、producer 优先级和正式 network authority 的当前结论分别解释；示例措辞不是当下发布许可，不代签完整拟发布文案或四层全域组合验收。

# playability_test_result PRD

审计轮次: 6

## 目标
- 建立 playability_test_result 模块设计主文档，统一需求边界、技术方案与验收标准。
- 确保模块后续改动可追溯到 PRD-ID、任务和测试。

## 范围
- 覆盖 playability_test_result 模块当前能力设计、接口边界、测试口径与演进路线。
- 覆盖 PRD-ID 到 GitHub task issue evidence comments 的任务映射。
- 不覆盖实现代码逐行说明与历史过程记录。

## 接口 / 数据
- PRD 主入口: `doc/playability_test_result/prd.md`
- 可变任务状态与历史: GitHub task issue evidence comments
- 文件级索引: `doc/playability_test_result/prd.index.md`
- 追踪主键: `PRD-PLAYABILITY_TEST_RESULT-xxx`
- 测试与发布参考: `testing-manual.md`

## 里程碑
- M1 (2026-03-03): 完成模块设计 PRD 主体重写与任务改造。
- M2: 补齐模块设计验收清单与关键指标。
- M3: 建立 PRD-ID -> Task -> Test 的长期追踪闭环。

## 风险
- 模块边界演进快，文档同步可能滞后。
- 指标口径不稳定会降低验收一致性。
## 1. Executive Summary
- Problem Statement: 可玩性反馈卡片与结论分散在历史记录中，缺乏支撑版本比较、问题收口与发布决策的统一结构。
- Proposed Solution: 将 playability_test_result 模块定义为可玩性证据层，统一反馈数据结构、评分口径、缺陷闭环与发布引用规则。
- Success Criteria:
  - SC-1: 每个发布候选版本至少生成 1 轮标准化反馈卡片集合。
  - SC-2: 反馈卡片包含场景、操作链路、问题等级、复现证据四类核心字段。
  - SC-3: 高优先级可玩性问题在下一版本前完成闭环或风险豁免登记。
  - SC-4: 测试手册与发布流程可引用同一份反馈结果集合。
  - SC-5: 正式可玩性证据必须显式区分 `player leverage` 与 `world activity`，不能再用“世界很活跃”替代“玩家有效参与”。

## 2. User Experience & Functionality
- User Personas:
  - 体验评测者：需要标准模板快速记录体验。
  - 玩法负责人：需要按问题等级追踪修复进度。
  - 发布负责人：需要可直接引用的可玩性门禁证据。
- User Scenarios & Frequency:
  - 日常体验记录：每次体验会话结束后立即填写卡片。
  - 版本对比：每个候选版本至少一次横向对比。
  - 缺陷闭环复核：高优先级问题修复后必须复测。
  - 发布门禁引用：发布评审阶段统一引用同一证据包。
  - 信任门 / 留存样本复核：每次需要判断“值得继续玩”时，必须补 1 份玩家杠杆摘要，而不是只摘世界活动指标。
- 新手工业引导回归：影响首个制成品/停机恢复/首座工厂单元时，按专题卡组执行 required-tier 手动复核。
- User Stories:
  - PRD-PLAYABILITY_TEST_RESULT-001: As an 评测者, I want a normalized feedback template, so that results are comparable across sessions.
  - PRD-PLAYABILITY_TEST_RESULT-002: As a 玩法负责人, I want issue severity and ownership, so that follow-up is actionable.
  - PRD-PLAYABILITY_TEST_RESULT-003: As a 发布负责人, I want traceable evidence packages, so that release decisions are auditable.
  - PRD-PLAYABILITY_TEST_RESULT-004: As a `producer_system_designer`, I want playability reviews to separate player leverage from ambient world activity, so that “interesting simulation” cannot be misreported as “meaningful play”.
- Critical User Flows:
  1. Flow-PLY-001: `启动体验会话 -> 填写标准卡片 -> 提交归档 -> 进入问题池`
  2. Flow-PLY-002: `对比多版本卡片 -> 提取高频问题 -> 指派修复 -> 回填结果`
  3. Flow-PLY-003: `汇总证据包 -> 绑定发布候选 -> 输出可玩性放行结论`
  4. Flow-PLY-004: `识别工业引导改动 -> 选择首产出/停机恢复/首座工厂卡片 -> 执行手动回归 -> 回写正式卡片与阻断结论`
  5. Flow-PLY-005: `回看正式样本 -> 回答“玩家做了什么/世界因此变了什么/是否打开新决策” -> 计算 leverage verdict -> 防止 world activity 误报为可玩性通过`
- Functional Specification Matrix:
| 功能点 | 字段定义 | 按钮/动作行为 | 状态转换 | 排序/计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 反馈卡片采集 | 场景、步骤、问题描述、截图、等级、`player_action`、`world_change_due_to_player`、`opened_decision`、`return_hook`、`player_leverage_score`、`world_activity_only` | 提交卡片并自动归档 | `draft -> submitted -> archived` | 按版本与时间排序；`world_activity_only=yes` 的样本不得直接支撑“继续可玩”结论 | 评测者可创建，负责人可编辑等级 |
| 问题闭环追踪 | 问题ID、责任人、修复提交、复测结论 | 更新状态并写入闭环记录 | `opened -> fixing -> verified -> closed` | 高严重级优先处理 | 玩法负责人可变更状态 |
| 发布证据包 | 卡片集合、缺陷清单、结论、`leverage_summary`、`world_activity_only_samples` | 生成证据包供发布引用 | `collecting -> bundled -> approved` | 按候选版本唯一绑定；至少要有 1 条代表性样本明确回答“玩家做了什么、世界因此变了什么” | 发布负责人审批 |
| 工业引导卡组 | 首个制成品、停机恢复、首座工厂单元、失败签名、证据路径 | 按场景卡选择手动回归链路 | `planned -> executed -> closed/blocked` | 影响首产出/停机/建厂体验时优先执行 | `qa_engineer` 维护，`producer_system_designer` 联审口径 |
- Acceptance Criteria:
  - AC-1: PRD 明确卡片字段、评分口径、问题分级标准。
  - AC-2: PRD、模板与 evidence bundle 定义采集、汇总、复盘三类可重复任务。
  - AC-3: 与 `doc/playability_test_result/game-test.prd.md`、`testing-manual.md` 口径一致。
  - AC-4: 历史卡片可按版本进行检索与对比。
  - AC-5: 前期工业引导的 `首个制成品 / 停机恢复 / 首座工厂单元` 具备可重复执行的专题卡组与 required-tier 手动回归链路。
  - AC-6: 正式卡片与 evidence bundle 都必须显式记录 `player_leverage_score` / `leverage verdict` / `world_activity_only`，并回答“玩家做了什么、世界因此变了什么、是否打开新决策”。
- Non-Goals:
  - 不在本 PRD 中定义玩法实现细节。
  - 不替代自动化压测脚本的设计文档。

<a id="feedback-score-product-contract"></a>
### 问卷评分与证据边界

按[唯一评分合同](playability_test_card.md#feedback-score-contract)采集并复算：五个现有维度各占 `20%`；题目映射、Q7 排除、Q14 转换、四档题等距分值、缺答、精度与多卡汇总均由该处定义，模板只引用。此选择落实 [#4516](https://github.com/eng-cc/oasis7/issues/4516) 对五维权重的补齐建议，不改变 runtime 指标或执行行为；历史卡片继续按当时原始口径解释，不追溯改成通过。

AC-1 的复算验收包括：原反例五维 `5/5/5/3/3` 得到 `4.2`；四档题中间答案能唯一映射；缺计分题保持未知；`3.2/4.2` 及邻近值按未舍入总分无空隙判档；多卡保留来源、完整性与独立阻断。数值标签只构成评审输入，既有低理解度/控制感、`world_activity_only`、玩家杠杆、L5 与发布治理要求仍适用。

<a id="feedback-score-evidence-level"></a>
#### 评分证据的层级承接

Trace refs: O7-REQ-1a903b9b-03df-4ae2-86f2-2a767affc9c1；局部承接[REQ-WR-PE-002](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-002)及其[AC](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#ac-wr-pe-002)。评分卡保留当前尝试、原始回答与人类/AI 来源及评审归属；数值标签不升级为产品 go、L5 或公开发布结论。该子义务由[评分证据隔离](design.md#feedback-score-evidence-boundary)承接；不覆盖所有 claim 消费者或实际试玩证据。

<a id="feedback-score-comparison-scope"></a>
#### 评分汇总与产品比较单位

Trace refs: O7-REQ-bacdddac-0615-4447-9242-0145c1cc4e07；局部承接[REQ-WR-PE-003](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-003)及其[AC](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#ac-wr-pe-003)。卡片合同定义的同组均值仅是描述性数值汇总；产品建议仍独立核对体验目标、formal 入口、候选/变更、证据窗口及影响解释的适用环境。同评分组不证明该比较单位相同，其他组的正向均值不得抵消当前阻断。该子义务由[评分比较边界](design.md#feedback-score-comparison-boundary)承接；不新增评分字段、重分组算法或产品 go 判定器。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 可玩性卡片模板、结果聚合脚本、手动长玩记录流程。
- Evaluation Strategy: 以问题检出率、重复缺陷比例、闭环时长、版本体验评分变化，以及 `world_activity_only` 误报被提前拦截的比例评估。

## 4. Technical Specifications
- Architecture Overview: playability_test_result 作为发布证据层，对接 game/testing 模块，负责收集和沉淀面向玩家体验的定性与定量证据。
- Integration Points:
  - `doc/playability_test_result/README.md`
  - `doc/playability_test_result/game-test.prd.md`
  - `doc/playability_test_result/topics/industrial-onboarding-required-tier-cards-2026-03-15.md`
  - `testing-manual.md`
- Edge Cases & Error Handling:
  - 空卡片：缺关键字段时禁止提交并提示必填项。
  - 证据缺失：无截图/日志的高优问题不得进入发布结论。
  - 活跃世界误判：若样本只能证明世界在运行、但不能证明玩家让世界发生了什么变化，必须标记 `world_activity_only=yes`，且不能支撑 `继续可玩`。
  - 权限不足：非负责人不得关闭高优先级问题。
  - 并发更新：同问题并发修改时保留最近版本并记录冲突。
  - 数据异常：历史卡片格式不兼容时标记迁移需求并隔离展示。
  - 回归超时：复测未完成不得标记闭环。
- Non-Functional Requirements:
  - NFR-PLY-1: 版本可玩性证据包覆盖率 100%。
  - NFR-PLY-2: 高优先级问题闭环前必须有复测记录。
  - NFR-PLY-3: 卡片模板字段口径与 testing 手册一致率 100%。
  - NFR-PLY-4: 历史卡片检索延迟保持在可接受范围内。
  - NFR-PLY-5: 敏感信息脱敏合规率 100%。
  - NFR-PLY-6: 每份正式可玩性证据包都必须能在 30 秒内回答“玩家做了什么、世界因此变了什么”，不允许评审时再二次猜测。
- Security & Privacy: 反馈内容应避免记录敏感凭据；截图与日志需遵守最小化采集原则。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP (2026-03-03): 固化反馈卡片标准字段与评审流程。
  - v1.1: 建立版本间可玩性差异报告模板。
  - v2.0: 将可玩性结果纳入发布门禁趋势分析。
- Technical Risks:
  - 风险-1: 主观反馈标准不一致导致结果不可比较。
  - 风险-2: 卡片填写不完整导致问题复现困难。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-PLAYABILITY_TEST_RESULT-001 | TASK-PLAYABILITY_TEST_RESULT-001/002/006 | `test_tier_required` | 模板字段与评分口径检查 | 反馈采集一致性 |
| PRD-PLAYABILITY_TEST_RESULT-002 | TASK-PLAYABILITY_TEST_RESULT-002/003/006 | `test_tier_required` | 问题分级与闭环状态抽样复核 | 缺陷收敛效率 |
| PRD-PLAYABILITY_TEST_RESULT-003 | TASK-PLAYABILITY_TEST_RESULT-003/004/006 | `test_tier_required` + `test_tier_full` | 发布证据包完整性与可追溯检查 | 发布可玩性风险控制 |
| PRD-PLAYABILITY_TEST_RESULT-004 | playability-player-leverage-rubric | `test_tier_required` | 卡片模板 / evidence bundle / trust-gate 样例同时出现 `player leverage` 与 `world_activity_only` 字段 | 防止“有趣世界”误报为“玩家有杠杆” |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-PLY-001 | 标准化卡片模板统一采集 | 自由文本记录 | 可比性和复现性更高。 |
| DEC-PLY-002 | 高优问题必须闭环或豁免登记 | 发布时统一兜底 | 可提前暴露体验风险。 |
| DEC-PLY-003 | 版本证据包作为发布输入 | 分散引用历史记录 | 统一证据包更易审计。 |
| DEC-PLY-004 | 独立增加 `player leverage` 裁决层 | 继续只看总体有趣度 / 世界活跃度 | `#166` 说明当前风险不是“世界没动”，而是“玩家是否真的造成了有意义变化”未被单独验证。 |

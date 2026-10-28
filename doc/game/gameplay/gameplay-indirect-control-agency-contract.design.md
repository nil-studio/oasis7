# Gameplay 间接控制 agency 合同设计文档

- 对应需求文档: `doc/game/gameplay/gameplay-indirect-control-agency-contract.prd.md`
- 产品承诺入口：[`间接控制下的玩家能动性与续接`](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001)
- 可变执行状态: 对应 GitHub Project task 与 issue evidence comments

审计轮次: 1

## 设计目标
- 将“间接控制仍然像控制”明确为 gameplay 正式合同，而非零散体验判断。
- 为 runtime / viewer / agent / QA 定义共同的 agency 标准，避免只在单侧修补。
- 明确本专题如何承接 `PRD-GAME-012` 的第 4 条 lane“间接控制因果与下一步”。

## 四项 guarantees

### 1. Intent Legibility Before/At Commit
- 玩家必须能了解系统当前接受的主意图。
- 可以是 commit 前预览，也可以是 commit 后立即确认，但不能完全隐形。
- 一旦新意图取代旧意图，必须显式标记 `replaced`、`reprioritized` 或等价结果。

### 2. Execution Acknowledgement With Causality
- 玩家不能只看到世界变化，还必须看到它与自己刚发出意图的关系。
- canonical 状态至少要区分：
  - `executing`
  - `blocked`
  - `overridden`
  - `completed_no_progress`
  - `completed_with_progress`

<a id="agency-interruption-hooks"></a>
### 3. Interrupt / Reprioritize / Recover Hooks
- 玩家必须有方式改变当前方向，而不是只能被动等待。
- 允许不同 surface 的入口不同，但必须在同一语义层里解释“新旧意图如何交接”。
- 重连与回流时必须恢复 agency 面，而不是只恢复原始事件流。

### 4. Bounded Consequence Readability
- 玩家必须看懂 4 件事：
  - 付出了什么
  - 世界改了什么
  - 为什么当前卡住
  - 下一步最值得做什么
- 后果信息只需足以支持下一步决策，不是全知预测。

## 设计原则
- 先 accepted intent，再 debug history：主意图必须优先于原始日志。
- 先主因果，再次级噪音：玩家第一眼看到的必须是“为什么现在这样”。
- 先可重排，再谈更宽动作面：agency 地板优先于功能扩张。
- 先 canonical truth，再多端展示：Viewer 与 pure API 不得各自拼装出不同的控制叙事。

## 与现有专题的边界
- `PRD-GAME-004`
  - 管“反馈可见性”和节奏可靠性。
  - 本专题更窄，专门管“间接控制如何仍然像控制”。
- `PRD-GAME-007`
  - 管 `PostOnboarding` 目标链。
  - 本专题要求该目标链必须被玩家感知为“我在推动”，而不是系统自走。
- `PRD-GAME-008`
  - 管 pure API 与 UI 的正式玩家等价。
  - 本专题要求 parity 不仅是字段有无，还包括 control-feeling guarantees 的等价。
- `PRD-GAME-012`
  - 管 trust gate / capability gate 的当前冲刺与 verdict。
  - 本专题是 lane-4 的正式合同，不替代 gate verdict。

## 角色切片
- `producer_system_designer`
  - 冻结 guarantees、失败签名与 scope boundary。
  - 裁决哪些 future 变更是增强还是削弱 agency。
- `runtime_engineer`
  - 对齐 canonical accepted intent、execution status、blocker/override reason、resume anchor。
- `viewer_engineer`
  - 收口 headed Web/UI 首屏与续玩 surface，确保主意图、主因果、下一步不被噪音淹没。
- `agent_engineer`
  - 对齐 dual-mode / action contract 的 interruption、reprioritization 与 override 语义。
- `qa_engineer`
  - 建立 control-feeling matrix，并把 failed guarantee 作为正式 blocker 分类。

<a id="实施顺序"></a>
## 设计依赖与历史来源

canonical accepted intent/status/blocker/next-step truth 是 Viewer/API 呈现和 interrupt/reprioritize/override 的共同输入；QA 矩阵按下节独立验证这些保证。历史 `TASK-GAME-071…075` 的规划见 [历史实施与验收记录](https://github.com/eng-cc/oasis7/issues/1193#issuecomment-6162410298)；accepted-intent 的实施来源另见 [Task #1543](https://github.com/eng-cc/oasis7/issues/1543)。不把原编号队列当作当前排期或子项完成证明。

## 验证口径
- required:
  - 文档互链与任务映射。
  - runtime/viewer/API/agent contract 对账。
  - QA control-feeling matrix 与 blocker 签名。
- full:
  - fresh bundle active-LLM 正式样本复核。
  - pure API / headed Web agency surface 等价抽样。

## 风险
- 如果只在 Viewer 表面补提示，而 canonical truth 仍不完整，合同会再次回退成文案层错觉。
- 如果只把 issue #164 解释成“增加 direct control”，会偏离当前间接控制主路线并冲淡 retention 主 blocker。
- 若不将 failed guarantee 列为独立 blocker，后续容易再次静默退化为“系统能跑但像旁观”。

### 2.1 需求承接与分配表

| 上游 requirement | 承接义务 | 本设计条款 | 权威职责 | 边界 |
| --- | --- | --- | --- | --- |
| [Intent 可读](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001) Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185 | 主意图与替换结果显式可读 | [Intent 可读](gameplay-indirect-control-agency-contract.design.md#1-intent-legibility-beforeat-commit) | Gameplay 定义 agency 保证 | 不代签 runtime、headed 或 API 实玩 |
| [因果确认](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001) Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185 | 执行状态与玩家意图的主因果保持关联 | [因果确认](gameplay-indirect-control-agency-contract.design.md#2-execution-acknowledgement-with-causality) | Gameplay 定义 agency 保证 | 不代签 runtime、headed 或 API 实玩 |
| [中断重排恢复](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001) Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185 | 方向可改变且重连恢复 agency 面 | [中断重排恢复](gameplay-indirect-control-agency-contract.design.md#agency-interruption-hooks) | Gameplay 定义 agency 保证 | 不代签 runtime、headed 或 API 实玩 |
| [有界后果](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001) Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185 | 成本、变化、阻塞和下一步可判读 | [有界后果](gameplay-indirect-control-agency-contract.design.md#4-bounded-consequence-readability) | Gameplay 定义 agency 保证 | 不代签 runtime、headed 或 API 实玩 |

### 11.1 验证映射表

| 上游 requirement | 本设计条款 | 独立 obligation | 验证方法 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [Intent 可读](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001) Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185 | [Intent 可读](gameplay-indirect-control-agency-contract.design.md#1-intent-legibility-beforeat-commit) | 主意图与替换结果显式可读 | N/A: reason=本次仅归位历史计划且四项保证正文未改变; scope=当前文档编辑不进行 runtime 或 headed 玩家验收; owner_role=qa_engineer; evidence_ref=https://github.com/eng-cc/oasis7/issues/1193#issuecomment-6162410298; re-evaluate=改变或实施对应保证时按同候选实际流程分别验证 | 实施 PR 的候选/环境/原世界结果和 headed/API 样本 | 文档/来源检查不证明玩家控制感通过 |
| [因果确认](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001) Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185 | [因果确认](gameplay-indirect-control-agency-contract.design.md#2-execution-acknowledgement-with-causality) | 执行状态与玩家意图的主因果保持关联 | N/A: reason=本次仅归位历史计划且四项保证正文未改变; scope=当前文档编辑不进行 runtime 或 headed 玩家验收; owner_role=qa_engineer; evidence_ref=https://github.com/eng-cc/oasis7/issues/1193#issuecomment-6162410298; re-evaluate=改变或实施对应保证时按同候选实际流程分别验证 | 实施 PR 的候选/环境/原世界结果和 headed/API 样本 | 文档/来源检查不证明玩家控制感通过 |
| [中断重排恢复](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001) Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185 | [中断重排恢复](gameplay-indirect-control-agency-contract.design.md#agency-interruption-hooks) | 方向可改变且重连恢复 agency 面 | N/A: reason=本次仅归位历史计划且四项保证正文未改变; scope=当前文档编辑不进行 runtime 或 headed 玩家验收; owner_role=qa_engineer; evidence_ref=https://github.com/eng-cc/oasis7/issues/1193#issuecomment-6162410298; re-evaluate=改变或实施对应保证时按同候选实际流程分别验证 | 实施 PR 的候选/环境/原世界结果和 headed/API 样本 | 文档/来源检查不证明玩家控制感通过 |
| [有界后果](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001) Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185 | [有界后果](gameplay-indirect-control-agency-contract.design.md#4-bounded-consequence-readability) | 成本、变化、阻塞和下一步可判读 | N/A: reason=本次仅归位历史计划且四项保证正文未改变; scope=当前文档编辑不进行 runtime 或 headed 玩家验收; owner_role=qa_engineer; evidence_ref=https://github.com/eng-cc/oasis7/issues/1193#issuecomment-6162410298; re-evaluate=改变或实施对应保证时按同候选实际流程分别验证 | 实施 PR 的候选/环境/原世界结果和 headed/API 样本 | 文档/来源检查不证明玩家控制感通过 |

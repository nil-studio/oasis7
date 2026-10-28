# 产品文档内容规范

## 文档身份

- 治理主题：产品文档内容规范与模板
- 配对规范正文：[`product-documentation-standard.design.md`](product-documentation-standard.design.md)
- 配套模板：[`product-documentation-standard.templates.md`](product-documentation-standard.templates.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- 结构与检查协作：`repository_health_engineer`
- 当前治理任务真值：由 Git、PR、实际 CI 和评审记录 提供。

本文把产品文档的内容质量约定落到现有四模块产品树和专业权威边界中。它规定产品作者要表达什么、如何验收和如何追踪；不创建第五个产品模块、第二套任务台账或新的发布门禁。

## 1. 问题与目标

产品文档需要让读者回答：为什么做、玩家如何参与、选择有什么代价、世界如何回应、失败后怎样继续，以及什么证据能够证明目标达成。现有文档已有四模块入口、产品/专业分工和验收追踪；本专题统一内容粒度，减少读者、实现者和评审者对同一语义的重复解释。

目标是让每个长期产品主题都有一条可读的正常路径、一组可单独引用的叶子要求、主要失败与恢复路径、权威来源和验收场景。目标态要求不自动表示当前实现；实现、测试、发布和任务状态仍由各自专业权威与 Git、PR 与实际 CI 记录 维护。

## 2. 适用范围与边界

- 适用于 `doc/product/` 四个既有模块的根 PRD、长期专题 PRD 和配对产品 design。
- 适用于产品承诺、跨域组合、玩家体验、交互状态和端到端验收；精确规则、数值、接口、实现、测试步骤、运维和任务进度仍归专业 authority。
- 根 PRD 保留现有身份字段、章节顺序、稳定 Product PRD-ID、完整 SC 清单和六列验收追踪。
- 专题不冒用根 PRD 身份；专题的局部 `REQ-*` / `AC-*` 只在自身文档内稳定引用。
- 目标、已验证事实、设计假设和公开承诺必须分开写；`active` 表示当前文档权威，不表示全部目标已实现或已发布。

## 3. 采纳承诺

规范正文 [`product-documentation-standard.design.md`](product-documentation-standard.design.md) 是本主题的内容权威，模板分册提供可复用结构。内容规范的机械诊断按[增量检查边界](product-documentation-standard.design.md#advisory-content-checks)执行：PR 新增或实质修改的适用产品/系统设计文档规范 finding 阻断 CI，未触达历史只警告；范围/输入/执行错误及既有身份拓扑、库存、安全等完整性检查不降级。

采纳后，新建或实质改写的产品主题应满足：

1. 按玩家/使用者问题、范围、正常路径、关键选择、成本与后果、失败恢复和未决问题组织内容。
2. 将复杂成功标准保留在根 ID 下，并把可独立失败的义务下钻到专题叶子要求；每个 MUST/MUST NOT 叶子都有验收引用。
3. 对 Agent 间接控制明确区分意图、授权、实际行动和权威回执，不把目标或 Prompt 写成世界动作保证。
4. 对异步、持久化、外部变化和多入口体验区分草稿、接受、应用、保存、历史和可重试授权等玩家语义。
5. 对经济和持续玩法说明资源来源、用途、持有、流转、取舍与损失边界；精确公式和参数只引用专业权威。
6. 将规则/能力正确性与玩家体验有效性分开，按适用入口、版本/范围和证据窗口描述可以证明与不能证明的结论。
7. 对 `doc/product/**/*.prd.md` 与 `doc/product/**/*.design.md` 的新建和实质变更执行明确 `base`/`head`/worktree 的机械检查：`base` 是集成目标、`head` 是待集成源，范围按 `merge-base(base, head)...head` 计算，worktree 另纳入 staged/unstaged/untracked；target-only、部分/格式错误输入不得进入或静默替代源范围。metadata、适用文档最低内容、真实 authority/fragment、显式 REQ/AC/anchor 唯一性与可达性必须接受检查；规范不符合按本次增量失败/历史警告处理；仅空白行、行尾空白和 HTML 注释变化排除，行首缩进变化算实质内容，不能用作者标签或 allowlist 任意绕过。
8. 检查先移除 fenced code block；其中的 metadata、路径、REQ/AC 和 trace 示例既不能满足也不能触发诊断。配对 design 可以通过带路径和 fragment 的 Markdown 链接承接 paired PRD 的 REQ/AC，不要求复制；REQ/AC 块和追踪表单元格中的每个关联都必须解析，`REQ-X → AC-TYPO` 等未声明关联必须报告规范诊断，本次触达范围内阻断 CI，未触达历史只警告。自动检查不判断趣味性、策略深度、数值合理性或玩家留存。

9. 对每个新建或实质修改功能点保留[独立的系统设计状态与实现状态](product-documentation-standard.design.md#feature-design-implementation-status)、适用 Trace ID、准确设计条款和 Task/PR/验证证据；“部分”列剩余范围，“已验证”限定固定基线、环境与验收范围，要求变化后重核。详细过程仍在 Task，不强制全仓回填或新增任务审批流程。

## 4. 三类代表性试点

采纳验收固定包含三类试点，分别验证汇总层、产品交互层和玩法产品层：

| 试点 | 现行权威 | 本规范采纳范围 |
| --- | --- | --- |
| Prompt 交互 | [`Agent 对话与 Prompt 控制`](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md) 与配对 design | 将原有控制权、草稿、接受/应用、外部变化和恢复义务落成稳定 REQ/AC 追踪 |
| 首局与持续游玩 | [`首局与持续游玩`](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md) | 完整接收“首个工业闭环 walkthrough”主题；保留原节点、阻塞、恢复、receipt、重复提交和双入口义务 |
| 复杂玩法根 SC-31 | [`世界规则与玩法系统 PRD`](../../product/world-rules-core-gameplay/prd.md) 与 [`gameplay` 专业设计](../../game/gameplay/gameplay-top-level-design.prd.md) | 保留根 SC-31 的叶子拆分方向和专业边界；具体玩法语义由 gameplay 专业 authority 按长期治理与验收规则维护，本规范不将局部试点表述为完整迁移 |

三类试点都必须保留原条款和专业引用。试点通过不代表其他产品主题已完成迁移，也不代表游戏当前可玩性、发行或运行时能力已经通过。

## 5. Done：成功标准与验收

- **PDOC-1 治理落位**：规范正文、模板分册、现有结构规范、治理专题 README 和工程索引形成可达链路；正文明确四模块与专业 authority 边界。
- **PDOC-2 内容合同**：正文覆盖专题最低内容、原子化要求、状态/交互、资源/涌现、证据边界、未决问题和变更处理。
- **PDOC-3 Prompt 追踪**：Prompt 主题的现有产品承诺全部保留，并可从叶子 REQ 追踪到 AC、配对 design 和专业 authority。
- **PDOC-4 首局主题追踪**：首个工业闭环 walkthrough 的正常链路、八个节点、原材料子循环、Feasibility Gate、失败恢复和既有 FS 证据可从叶子 REQ/AC 定位。
- **PDOC-5 三类试点边界**：根 SC-31 作为独立玩法试点保留，明确其专业迁移 owner；不得以本规范采纳范围宣称完整玩法语义已经迁移。
- **PDOC-7 功能状态可核对**：本次触达功能点的设计/实现状态独立表达，能追到同一要求、准确承接设计和实际交付/验证依据；未知与剩余范围明确，需求变化后不会沿用不适用的完成结论。
- **PDOC-6 结构与语义诚实**：适用文档检查、链接检查和 diff 检查通过；不新增第五模块、任务台账、实现能力或自动检查器未证明的语义。现行 bounded checker 以 canonical 正文定义的真实 paired-design backlink 与 `topic-missing` 诊断契约为准；结构检查不代替产品语义、专业规则或体验证据评审。

验收证据由当前 PR、实际命令结果和适用专业评审共同提供，Issue 按跟踪需要使用；本 PRD 不复制可变执行日志。

## 6. Non-Goals

- 不批量重写 `doc/product/` 其他专题，不删除仍承载专业真值的来源。
- 不把产品规范扩展为 runtime、WASM、Agent、Viewer、QA、LiveOps 或发布实现；现行文档治理 checker 仅承担本规范明确的 bounded 结构检查。
- 不冻结 UI 布局、字段 schema、数值公式、tick、经济参数、测试命令或发布状态。
- 不将规范采纳、文档 active、局部检查通过或 synthetic/Agent 证据外推为当前版本已可玩、可发行或真实玩家愿意持续游玩。

## 7. 变更与后续

后续产品主题应在同一产品模块内按稳定语义逐文件迁移：新建或实质修改的文件触发本规范的内容检查；另按内容规范对设计目标变更引发的新增无效设计依据或新增功能关系不匹配执行增量差分检查，即使引用方产品 PRD 未改。历史已存在的同样关系失效和未触碰文档的旧字段缺口不因此要求全面重写。只有语义完整接收、专业权威仍可达、活跃引用修复后，才可删除来源。跨文件语义迁移必须记录旧条款到新叶子的映射、接收 owner、未接收语义和删除条件；该迁移记录属于 Git、PR 与实际 CI 记录，不在产品目录另建任务台账。产品输入按“产品意图 → system design → 代码/专业实现合同”传递，并在受影响条款处保留回链。

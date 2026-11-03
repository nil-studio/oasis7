# 首局工业体验内容与表现设计

- 主责：`game_visual_interaction_designer`；所属模块为[智能体、世界模拟与交互](../../product/agents-world-simulation/prd.md)，专业入口为 [Viewer](README.md)。`viewer_engineer` 负责实际消费者，玩法和 Runtime 继续拥有规则与结算。
- 生命周期：`active`。适用于 `starter-industrial-smelter-to-assembler-v1` / `production_only` 的 Viewer 内容制作与验收；pure API 仅作为同义核对入口。
- 审读基线：`b442bf9350832825f6ba98eea2b9865a669cd22b` 的产品合同、源码和仓库素材。本文的建议文案是设计交付，不表示已替换运行文案；实际运行结果回到消费它的 PR 与原证据入口。
- 上游：[首局具体样本](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#first-session-concrete-sample)、[首产物结算合同](../../game/gameplay/gameplay-industrial-starter-completion-contract.prd.md#3-五阶段结算表)、[玩家可读表面](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001)。具体 Trace refs 见 §2.1。

## 1. 问题、目标与范围

现有产品设计已经说明“先完成一笔可解释的生产”，Viewer 已有成果卡、反馈、恢复预览和示意稿。本专题补齐它们作为可维护内容的交接：什么事实可以写成什么话、哪些内容能复用、编辑源在哪里、如何检查后交给实际入口。

玩家应能从同一对象的内容读出当前目标、本次投入、已确认结果、主要阻塞与下一步，并分清首笔铁锭、持续生产能力、Assembler 候选及交付。本设计拥有内容单元、表达与制作规格；产品选择沿[首局配对设计](../../product/world-rules-core-gameplay/first-session-and-continuation.design.md#des-first-concrete-sample)，页面布局沿[首局交互](viewer-gameplay-release-experience-overhaul.design.md#starter-plan-interaction)，不复制数值、权限、状态机或进度权威。

本范围交付中文内容建议、原有双语标签的复用约束、实际文件索引与验证方法。它不新增剧情、配方、模型、音频或官方英文译文；稳定窗口、terminal-admission、批量美术生产和正式发行分别由原专业范围承接。

## 2. 上游要求与约束

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-FIRST-INDUSTRIAL-001](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-001)；Trace refs: O7-REQ-68214dc7-c00b-4c0d-8c3b-c1c24fb52502 | 推荐前表达当前 profile、可行性与真实缺口 | [内容来源](#content-source-boundary) | gameplay / Runtime 的 fresh feasibility | 不实现 Gate，不证明全部 profile 可行 |
| [REQ-FIRST-INDUSTRIAL-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-002)；Trace refs: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95 | 将建厂、准备、排产和结算分成可读内容单元 | [首局内容矩阵](#content-units) | M4 同方案报价、Viewer 原动作入口 | 来源精炼/handoff/物流只按真实 profile 依赖适用 |
| [REQ-FIRST-INDUSTRIAL-004](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-004)；Trace refs: O7-REQ-81c0bc21-36ef-400d-9398-31859b9a08b2 | 首产物只表达匹配结算，保留 produced/undelivered 边界 | [成果与归因](#outcome-attribution) | owner-bound ledger、持久 milestone、原 receipt | 不证明稳定窗口、终端交付或奖励 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005)；Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | 失效报价、结果未知与重入不引导重复提交 | [未知与回访](#unknown-and-return) | Runtime 原请求查询、当前读取资格 | 不实现幂等、持久化或跨节点恢复 |
| [REQ-FIRST-INDUSTRIAL-006](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-006)；Trace refs: O7-REQ-d0041aa9-1dfb-46a0-95e0-67417b39bfa3 | 主 blocker、保留/已耗价值、复查条件与真实动作同时可读 | [首局内容矩阵](#content-units) | gameplay / M4 / Runtime 恢复查询 | 不补造补料、改道或退款能力 |
| [REQ-FIRST-INDUSTRIAL-007](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-007)；Trace refs: O7-REQ-df0617cf-2ab6-40ca-934b-f050d6a66812 | Viewer 与 pure API 的同一结果表达同义 | [语言与可访问表达](#language-accessibility) | 两入口同候选的权威投影 | 本文不是双入口运行通过证明 |
| [REQ-FIRST-INDUSTRIAL-008](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-008)；Trace refs: O7-REQ-757636b1-c9e9-4e7a-97d1-460de105daf3 | 首产物连接真实后继条件，并允许自然离场 | [成果与归因](#outcome-attribution) | 产品后继用途、当前合法动作 | 不宣称持续能力、经营分支或续玩意愿成立 |
| [REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001)；Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef | 窄屏、语言和图形降级仍保留目标/阻塞/可信反馈/下一步 | [语言与可访问表达](#language-accessibility) | Viewer 视觉规格、locale 与恢复合同 | 不代签所有语言、字体和辅助技术组合 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)；Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6 | 本人反馈、公开世界事件与观察焦点不混同 | [成果与归因](#outcome-attribution) | 已验证玩家 scope、原操作关联 | 不授予读取权限或暴露另一玩家私有信息 |
| [REQ-AGENT-SURFACE-004](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-004)；Trace refs: O7-REQ-e3a70494-b3f3-4220-a654-6c9f07cdb8a4 | 已知拒绝原因支持真实恢复；未知不改写成失败 | [首局内容矩阵](#content-units) | typed 错误与动作可用性 | 不把所有失败统一改写为重连 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 定位实际交付物、源/版本/依赖与权利核验边界 | [交付索引](#delivery-index) | 原非代码组织规范、实际消费 PR | 本行产品层 N/A；理由：组织规范不新增玩家功能；范围：本专题交付信息。上列玩家义务仍适用，不代表全游戏领域完成。 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 共用制作、交接与变化传播可执行 | [制作交接](#delivery-handoff) | 设计制作方、Viewer 消费方与 QA | 本行产品层 N/A；理由：制作协作不新增玩家功能；范围：本专题交接。文件或工具存在不证明规模化产能。 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 组合验证能定位实际消费的内容版本与环境 | [候选组合](#candidate-composition) | 原构建/资源/运行记录 | 本行产品层 N/A；理由：证据组合不新增玩家功能；范围：本专题候选定位。不新增平行 manifest，不代签发行。 |

### 2.2 技术职责的实际承接

| 依赖 | 主责合同与本内容的消费边界 |
| --- | --- |
| 规则、报价、输入与实际结算 | [M4 同方案绑定](../m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)拥有 factory/recipe/batches、消费账本及报价失效；[未知结果与恢复](../m4/industrial-resource-flow-contract.design.md#request-outcome-observation-separation)拥有原请求处置。内容只解释结果，不以另一账本或示例常量补全。 |
| 反馈、权限与恢复 | [反馈隔离](viewer-control-plane-split-live-playback.design.md#gameplay-feedback-isolation)和[原请求回读](viewer-control-plane-split-live-playback.design.md#gameplay-original-result-lookup)拥有身份 fence、原请求与结果可见性；内容保留其归属与未知边界。 |
| 布局、渲染、资源与性能 | [首局交互](viewer-gameplay-release-experience-overhaul.design.md#starter-plan-interaction)、[资源交付](viewer-gameplay-release-experience-overhaul.design.md#viewer-payload-delivery)及[预算验证](viewer-gameplay-release-experience-overhaul.design.md#viewer-performance-verification)拥有实际加载、恢复、性能和环境选择；本次不新增 renderer 或性能预算。 |

## 3. 内容与表现设计条款

<a id="content-source-boundary"></a>
### 3.1 首局内容输入与来源边界

| 内容对象 | 已核实的编辑/规则来源 | 本专题的使用方法 |
| --- | --- | --- |
| profile 与后继资格 | [starter_industrial.rs](../../../crates/oasis7_world_state/src/state/starter_industrial.rs)定义 profile/revision、完成边界与只读 feasibility | 先核当前候选的实际 profile；缺 authority 显示原因和恢复条件，不默认采用 production_only。 |
| Smelter、Assembler | [canonical factory catalog](../../../crates/oasis7_agent_api/src/world_quote/factory_catalog.rs)；`factory.smelter.mk1`、`factory.assembler.mk1` | 已有合法 Smelter 不重复建造；Assembler 只在当前规则允许时作为后继候选。材料、用电、时长与槽位读取实际规则/报价，不抄进文案。 |
| 铁锭生产内容 | [canonical recipe catalog](../../../crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs)的 `recipe.smelter.iron_ingot` | 基线 native 配方包含 `iron_ore`、`carbon_fuel` 输入，`iron_ingot` 产出及 `slag` 副产物。实际方案/结果按消费来源逐项展示；图稿只含 iron_ore 的示例不能成为完整成本。其他 module-backed 配方按自己的 authority 核对。 |
| 已结算成果 | [首产物合同](../../game/gameplay/gameplay-industrial-starter-completion-contract.prd.md#4-currenttarget-evidence-cutline)、[成果归一化](../../../crates/oasis7_viewer/software_safe_src/industrial_starter_outcome.js)与[现有成果卡](../../../crates/oasis7_viewer/software_safe_src/industrial_starter_outcome_card.jsx) | 源码已有实际投入/产出、owner、job/tick、首产物及候选分离的读面。前端字段校验不代替上游 owner/profile/ledger 的权威核验。 |

<a id="content-units"></a>
### 3.2 可直接交接的首局内容矩阵

下列 C01～C09 仅为本专题编辑单元，不是新要求 ID。引号内是可供实施采用的**中文设计目标文案**；标为“源码已有”的句子直接复用原文。花括号表示必须取自同一合法来源的值，不是新增 DTO。未发布的值写明未知或未发布，不填零，也不从叙事推算。

| 单元与触发事实 | 玩家首读与反馈内容 | 表达依据与实际入口 | 未证或不可替代范围 |
| --- | --- | --- | --- |
| C01：同一 fresh feasibility 给出当前可用首局候选 | “生产第一笔铁锭，为评估 Assembler 建设准备材料。”同时说明当前对象、主要投入/缺口与可选的暂缓路径；无安全链时改为“当前无法确认安全的首产物方案：{原因}。” | [首局样本](../../product/world-rules-core-gameplay/first-session-and-continuation.design.md#des-first-concrete-sample)；原 Next Move | 采用目标不扣料；候选不是已建成或已具备全部投入。 |
| C02：尚无合法可用 Smelter，或建厂结果已确认 | 准备时显示“先核对 Smelter 的建造条件”；接受时显示“建厂请求已接受，等待建设结果”；只有真实建成后转为工厂就绪。已有合法工厂直接核对当前状态。 | [首产物因果链](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-002)；原建设动作与行动回执 | 选中模型、建造预览或接受均不是建成；不承诺 Agent 已支持代建。 |
| C03：取得与正式动作同源的固定方案/报价 | “核对本次方案：{工厂} · {配方} · {批次}。”呈现实际材料、能源、消费账本、预计产出与副产物等已发布决策信息；独立可编辑查询标“仅查询，尚未配置提交方案”。 | [M4 同方案绑定](../m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)；现有 Gameplay Details/Quote | 报价不预留资源；未发布单位不自造 kWh 等单位；未知副产物不按“没有副产物”展示。 |
| C04：当前权威主 blocker 或明确拒绝 | “当前受阻：{主要原因}。”随后列当前已保留/已消费值、真实可用动作及复查条件；材料不足指向当前消费账本，电力不足指向当前有效约束。无可执行恢复只交还下一决策。 | [恢复查询](viewer-gameplay-release-experience-overhaul.design.md#subject-capability-recovery-query)、[失败说明](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-004)；原 blocker/fallback 区 | 等待、补足、减量、改道只在当前动作真实支持时出现；现有 fallback 比较卡本身没有执行按钮。 |
| C05：当前适用权威状态明确确认尚未结算，并能区分已接受未开始与正在执行 | 已接受未开始：“生产请求已接受，尚未开始生产。”正在执行：“正在生产，尚未结算。”同时保留当前主要阻塞与下一复查条件；只展示权威已确认的阶段。 | [五阶段结算](../../game/gameplay/gameplay-industrial-starter-completion-contract.prd.md#3-五阶段结算表)、[状态置信度](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#41-重叠状态的主语义与真值优先级)；原行动回执 | 旧 accepted 或缺 receipt 不能证明当前尚未生效；无法确认当前结果转 C06。排队、扣减和世界动态不使用成果完成表达。 |
| C06：原请求是否生效未知，或报价已失效 | 结果未知：“原操作结果尚未确认。”下一步只用实际查询能力核对原请求；报价失效：“条件已变化，请重新核对本次方案。”两者保留不同上下文。 | [原件状态标签](viewer-brand-system-2026-06-05.design.md#25-bilingual-terminal-copy-matrix-target)、[未知恢复](#unknown-and-return) | 查询失败不能改成原动作被拒绝；新报价不消除旧提交未知。 |
| C07：匹配的 owner-bound 正量铁锭结算与持久 milestone | “第一笔铁锭已结算。”先显示已结算产出、实际投入、已发布能源与归属，再给原回执定位；成功强调只用于这个有限成果。 | [成果卡源码](../../../crates/oasis7_viewer/software_safe_src/industrial_starter_outcome_card.jsx)、[R-GAME-18](viewer-gameplay-details-uiux.design.md#r-game-18)；同一 Action Receipt | 现有卡展示 `consume/produce/powerRequired`，不保证所有副产物/能源细目已投影；来源缺项保留内容缺口，不用图稿补值。 |
| C08：真实后继资格已开放 | 源码已有：“下一候选：Assembler MK1。具备候选资格，建造仍需满足当前材料、用电与授权条件。”结合已满足条件/真实缺口选择查看、比较或当前合法建设动作。 | [现有成果卡](../../../crates/oasis7_viewer/software_safe_src/industrial_starter_outcome_card.jsx)、[后继阅读边界](viewer-gameplay-release-experience-overhaul.design.md#83-成果与后继目标的阅读边界) | 不把候选当建成；只读卡不因文案增加提交。稳定与交付继续分开。 |
| C09：离场、重入或观察焦点变化 | 回来先展示已保留成果、当前目标、待决请求与当前合法下一步；读取旧成果时使用摘要，避免再次庆祝为新产出。原结果的对象保持不变。 | [回访边界](#unknown-and-return)、[个人反馈](#outcome-attribution)；原 Next Move/Receipt | 关闭页面不取消旧请求；不复活旧授权，不承诺离线收益。 |

<a id="outcome-attribution"></a>
### 3.3 成果、归属与后继

成果信息依次回答“实际付出/所得”“归谁”“下一步用途”。只有原意图—action/job—settlement 的精确因果成立时，才增加“本次决定带来的变化”；同 owner、时间相近或当前选中对象仅能支持各自已确认的事实。公开世界事件保留为世界上下文，另一玩家和匿名失败不进入本人结果。

现有成果卡的“首产物里程碑已记录（production_only）。稳定运行与交付仍需分别验证。”作为本范围的边界文案复用。首产物成功可以是自然停点；内容不能靠新奖励、强制事故或“立即建成 Assembler”延长会话。后继条件未被实际投影时，交付的是设计目标与缺口，不能将空卡命名为完成的持续能力方案。

<a id="unknown-and-return"></a>
### 3.4 未知、重连与安全返回

按[表面主语义优先级](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#41-重叠状态的主语义与真值优先级)表达：已有结算不因断连撤销，已确认未生效与无法确认是否生效分别措辞；重连成功只更新连接状态。原请求不可查询时说明当前限制及真实恢复入口，不提供“重新生产一次”来消除未知。

回访读取当前有效授权和原结果，不依赖旧缓存恢复私有文字。关闭、返回与暂停未来委托只按各自实际能力命名；不能把离开详情写成撤销已受理工作。实施时复用[原请求回读](viewer-control-plane-split-live-playback.design.md#gameplay-original-result-lookup)，缺能力保留未知，不用内容层代做协议恢复。

<a id="language-accessibility"></a>
### 3.5 视觉、语言与可访问表达

内容进入原有世界、目标、指令三条主路由，沿[视觉层级](viewer-visual-design-spec-2026-06-05.design.md#7-visual-language)和[世界分层](viewer-visual-design-spec-2026-06-05.design.md#9-pixel-world-layering)先保留目标/阻塞/原结果，再折叠诊断。颜色、图标或动画同时有可见文本意义；受理/等待不得使用结算成功语义。选择高亮不表示控制权或原操作归属。

术语与现有中英标签复用[品牌文案矩阵](viewer-brand-system-2026-06-05.design.md#25-bilingual-terminal-copy-matrix-target)。[locale 源码](../../../crates/oasis7_viewer/software_safe_src/viewer_locale_preferences_module.js)已有选择与存储/fallback 路径，本文不新增语言机制。C01～C09 新建议中的中文并非新增正式英文译文；采用时须在原双语消费者中完成对应审校，尚缺译文/字体覆盖的组合不计入内容交付范围。两入口核对事实、归属、完成边界与真实下一步，不要求逐字或布局一致。

长中文、窄屏、文字放大与 reduced motion 下保留关键句和原结果入口；动作、返回和恢复不得只靠 hover。音频不是本范围已交付的反馈渠道，所有必要含义在静音时可读；将来加入声音仍需其自身制作、权利和试听依据。

## 4. 制作规格与交付物

<a id="delivery-index"></a>
### 4.1 已有交付入口与实际可用范围

下表文件均可从审读基线取得；固定版本表示可定位内容，不表示消费者已加载。`coverage-catalog.json` 的源采集版本为 `113868200d1f91bef1008babb689bd257f241428`，其 `source_pointer_only_not_executed` 和图稿 `not_run` 状态继续保留。新文案以采纳它的实际 PR 提交为版本，不另设全仓素材清单。

| 内容/素材与主责 | 编辑源、输出与依赖 | 当前可用边界、消费方 | 来源与权利核验边界 |
| --- | --- | --- | --- |
| 首局内容单元；视觉交互设计 | 本文 [C01～C09](#content-units) 为交接原文；实现沿原 JSX/JS 文字和双语函数，没有假设一个尚不存在的统一译文表 | 可供设计/实施审读；本次不改变构建输出。采用方须回链具体单元与实际修改 | 本次编写的文字；没有引入外部图文或宣称已有翻译审校。 |
| 首产物双语卡；Viewer | [成果卡](../../../crates/oasis7_viewer/software_safe_src/industrial_starter_outcome_card.jsx)、[归一化](../../../crates/oasis7_viewer/software_safe_src/industrial_starter_outcome.js)，由既有 Viewer 构建消费 | 源码可读；实际值依赖 `starter_industrial_feasibility`。本次没有目标环境加载结果 | Git 可定位源代码；本次未逐项核验全部内容作者/来源或商业素材权利。 |
| 首产物示意稿；视觉交互设计 | [industrial-settled.svg](assets/ui-ux/full-function/gameplay/frames/industrial-settled.svg)，可编辑 SVG，1440×1000；引用 `Noto Sans CJK SC`；原 [R-GAME-18](viewer-gameplay-details-uiux.design.md#r-game-18) 为字段说明 | 文档参考，不是 runtime 资源。36/48/24 等均为示例；不覆盖当前完整 native 配方、字体实装和运行画面 | Git 版本与字体名称可核；文件未附逐项作者/授权凭证。本次未核字体文件、取得来源、再分发许可与署名要求。 |
| 阻塞、无安全方案与世界构图参考；视觉交互设计 | [goal-execution-blocked.svg](assets/ui-ux/full-function/gameplay/frames/goal-execution-blocked.svg)、[fallback-choices.svg](assets/ui-ux/full-function/gameplay/frames/fallback-choices.svg)、[fallback-no-safe.svg](assets/ui-ux/full-function/gameplay/frames/fallback-no-safe.svg)、[W01](assets/ui-ux/full-function/world-diagnostics/W01-world-targets.svg)、[W09](assets/ui-ux/full-function/world-diagnostics/W09-narrow-world.svg)；字段关系沿 [R-GAME-11](viewer-gameplay-details-uiux.design.md#r-game-11)/[R-GAME-15](viewer-gameplay-details-uiux.design.md#r-game-15) | 可编辑参考；只承接各自画出的状态，不提供未画出的所有首局帧，也不作为生产世界渲染实现 | 可核仓库文件及固定版本；外部依赖与逐项权利未完整审定，不能把仓库 LICENSE 当成所有素材自动授权。 |

本次不交付 Smelter/Assembler 专用模型、动画、音效包或图稿内字体文件；[voxel 工具合同](../../../tools/voxel_asset_pipeline/DESIGN.md#asset-contract)存在也不能证明这些素材已制作。若实际候选消费其他模型/字体/音频，实施方从其原资产入口补定位与权利依据，再纳入该候选验收。

<a id="delivery-handoff"></a>
### 4.2 共用制作、多人交接与变更传播

内容编辑单元由“触发事实、语义状态、文字、值来源、合法下一步、适用语言”组成。一个单元可以服务正常、未知、阻塞和回访变体，不能为缩短文字合并不同结算阶段。原句与设计稿可共同审读；实际值绑定、翻译实现及构建输出由原消费者维护。

| 环节 | 制作与交接判据 |
| --- | --- |
| 编辑与合并 | 沿现有 PR 分工声明写入范围。Markdown/JSX 可按明确单元分工；同一 SVG 由一个写入者整合，避免两人覆盖图层。源/导出/引用改名或删除一并核对原索引、消费者与测试，不另加仓库锁系统。 |
| 转换与导入 | SVG 原件留在原文档素材目录，预览留原本地验证输出位置且标为设计预览。文字被采用时修改实际 JSX/JS 消费点并记录原构建工具/锁文件版本；不把 SVG 自动导入 runtime，也不假称当前已有文案生成管线。 |
| 制作方交付 | 提供固定版本、已画状态/语言、数值来源、依赖与已核权利范围；按[静态检查](first-session-content-delivery.manual.md#fs-content-static)和[语义审读](first-session-content-delivery.manual.md#fs-content-review)指出缺项。收到文件不等于已集成。 |
| 消费方交接 | Viewer 核同源方案、原结果与实际加载；QA 按[入口验证](first-session-content-delivery.manual.md#fs-content-runtime)留候选、操作与 authority 对照。缺投影/动作的内容回到其技术 owner，不能由文案补造。 |
| 变更传播 | profile/配方/报价变化重查 C01～C08 和数值示例；反馈/权限变化重查 C04～C09；语言、字体或布局变化重查长文本和可访问性；资产、工具或配置变化重建实际受影响产物并核 freshness。旧候选截图、旧语言审校不能直接覆盖新组合。 |

本批内容量为九个编辑单元：C08 复用现有成果卡的整句，C06 复用原件未知标签；其余是在原首局体验与状态合同上细化的中文表达方案，不新增玩法内容。六份 SVG、既有双语成果卡和 canonical 工厂/配方均为复用；后续预览是相应原件的衍生物，不计为新的运行资产。本批结束边界是两份可审读设计/方法文档与来源定位，运行文案采用、译文审校、素材制作和玩家验证由实际消费 PR 另行交付。

尚无重复批次制作/集成/返工工时测量，产能与成本未估算。需要扩大内容时，在实际制作 PR 分开记录首次建立、重复编辑、导入验证、返工与等待；本设计不据此给出大型游戏产能或预算承诺。

<a id="candidate-composition"></a>
### 4.3 实际候选的内容组合

组合验证复用当前 PR、既有 `session.meta`、构建/资源记录与原运行证据，关联实际程序 source/tree、规则/profile revision、有效场景或生成输入、Viewer bundle、实际资源/字体版本、入口、语言、viewport、质量设置和 execution lane。未使用本专题参考 SVG 的 runtime 无需把它计入运行资源。

最终产物按[同来源与新鲜度](../../engineering/workflow/package-source-security.md#final-artifact-freshness)核对；生成世界依[来源绑定合同](../scenario/scenario-files.prd.md#generated-world-provenance-contract)记录真实有效输入与产物身份，设计合同存在不等于当前加载器已经完成全部验证。外部资产保留其固定版本与实际加载关系；缺少加载/交付证据时只能声明材料可取得。

本文件的静态审读不构成运行候选。正式采纳时在原记录分别说明哪些文本/素材实际使用、哪些仅为参考，以及未完成语言、权限、平台和玩家验证范围，不新增另一份全仓 manifest。

## 11. 验证与验收

设计层核语义与来源，素材层核文件/依赖/版本，目标环境核真实加载、操作反馈与 authority，真人层核理解。静态检查随本专题或相关源变化执行；后续实现/素材/语言变更命中哪个消费范围，就按手册选择相应场景，不为文档修改启动完整游戏。结果回到实际消费 PR 和原证据，不在长期文档累计每轮 PASS。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-FIRST-INDUSTRIAL-001](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-001)；Trace refs: O7-REQ-68214dc7-c00b-4c0d-8c3b-c1c24fb52502 | [内容来源](#content-source-boundary) | 候选/不可行文案与同一 profile 的真实 Gate 相符 | [FS-CONTENT-RUNTIME R1](first-session-content-delivery.manual.md#fs-content-runtime)，同候选可行/缺前置样例；静态语义先依手册 §2 核 C01 | 消费 PR 的来源审读与 R1 原状态/截图 | 全部 profile 的可行性与入场资源 |
| [REQ-FIRST-INDUSTRIAL-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-002)；Trace refs: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95 | [内容矩阵](#content-units) | 工厂、同源方案、排产与结算不混同 | [FS-CONTENT-RUNTIME R1/R2](first-session-content-delivery.manual.md#fs-content-runtime) 核真实报价、动作与原结果；设计审读依手册 §2 的 C02/C03/C05 | 消费 PR 中同方案/原请求/结算对照 | 非当前 profile 的来源/handoff/物流 |
| [REQ-FIRST-INDUSTRIAL-004](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-004)；Trace refs: O7-REQ-81c0bc21-36ef-400d-9398-31859b9a08b2 | [成果与归因](#outcome-attribution) | 有/无结算分别显示，首产物不代签交付 | [FS-CONTENT-RUNTIME R2](first-session-content-delivery.manual.md#fs-content-runtime) 核实际 receipt、ledger、milestone 与页面；改消费者时按手册 §3 选择已有聚焦测试 | 原测试输出；消费 PR 的 R2 世界证据与截图 | 仅 UI 测试不证明真实结算；稳定与 delivery 未涵盖 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005)；Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | [未知与回访](#unknown-and-return) | 原未知结果与旧成果重入不引导新效果 | [R3](first-session-content-delivery.manual.md#fs-content-runtime)，已有合法结果与真实断连/回读场景；无法制造未知则标未覆盖 | 消费 PR 的原请求、前后世界记录与页面反馈 | 未注入的丢回复、重启、乱序、跨节点场景 |
| [REQ-FIRST-INDUSTRIAL-006](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-006)；Trace refs: O7-REQ-d0041aa9-1dfb-46a0-95e0-67417b39bfa3 | [内容矩阵](#content-units) | 当前真实 blocker、价值和恢复边界可读 | [R4](first-session-content-delivery.manual.md#fs-content-runtime)，同一准备来源的一个真实材料/电力约束；设计审读依手册 §2 的 C04 | 消费 PR 的报价/拒绝与页面对照 | 其他工业 blocker 和尚无真实动作的恢复路径 |
| [REQ-FIRST-INDUSTRIAL-007](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-007)；Trace refs: O7-REQ-df0617cf-2ab6-40ca-934b-f050d6a66812 | [语言与可访问表达](#language-accessibility) | 同候选两入口的结果、下一步同义 | [R5](first-session-content-delivery.manual.md#fs-content-runtime)，同 world/window/subject 的 Viewer 与 pure API；脚本另跑只能证明其自身窗口 | 原 API 输出、Viewer state/截图及 join 信息 | 不同候选的文本相似、Agent parity、全部入口组合 |
| [REQ-FIRST-INDUSTRIAL-008](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-008)；Trace refs: O7-REQ-757636b1-c9e9-4e7a-97d1-460de105daf3 | [成果与归因](#outcome-attribution) | 玩家分清后继候选、实际成果与自然离场 | [FS-CONTENT-PLAYER](first-session-content-delivery.manual.md#fs-content-player) 的成果/后继题，前置使用手册 R2/R3 实际候选 | 消费 PR 的结果与玩家原始回答 | 持续能力、留存、经营分支或主体感的完整结论 |
| [REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001)；Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef | [语言与可访问表达](#language-accessibility) | 实际支持表面的关键内容可读且可达 | [R6](first-session-content-delivery.manual.md#fs-content-runtime) 按 PW-ACCESSIBILITY 选桌面/窄屏、zh/en 与长文本；理解结论另依手册 §6 | 原 S6 截图/操作及玩家回答 | 未测字体、辅助技术与平台组合 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)；Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6 | [成果与归因](#outcome-attribution) | 他人/匿名反馈与本人结果隔离 | [R7](first-session-content-delivery.manual.md#fs-content-runtime)，两名已鉴权主体及匿名拒绝；不能取得前置则记录缺口 | 消费 PR 的各主体独立页面与授权查询对照 | 单人截图不证明多主体隔离 |
| [REQ-AGENT-SURFACE-004](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-004)；Trace refs: O7-REQ-e3a70494-b3f3-4220-a654-6c9f07cdb8a4 | [内容矩阵](#content-units) | 已知拒绝与未知结果支持不同恢复选择 | [R3/R4](first-session-content-delivery.manual.md#fs-content-runtime) 保留 typed 原因和原请求；设计审读依手册 §2 | 消费 PR 原错误/状态与玩家读面 | 未覆盖失败类别不由单一 blocker 代签 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [交付索引](#delivery-index) | 源/版本/依赖/权利核验范围可查 | [FS-CONTENT-STATIC](first-session-content-delivery.manual.md#fs-content-static) 核文件、SVG/目录解析及依赖审读 | 本文交付 PR 的静态结果 | 文件存在不证明品质或权利齐备 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [制作交接](#delivery-handoff) | 制作方和消费方可定位实际单元、输入、检查与缺口 | [FS-CONTENT-REVIEW](first-session-content-delivery.manual.md#fs-content-review) 按实际采用单元核来源、语义与依赖，结果留原 PR | 交付 PR 的逐单元审读与实际消费方接线范围 | 未估算产能/成本，不证明重复批次已就绪 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [候选组合](#candidate-composition) | 验证对象与实际加载的内容、程序和环境一致 | [组合准备](first-session-content-delivery.manual.md#fs-content-candidate) 核固定候选、资源与 authority join | 实际采用 PR 的原构建/资源/运行记录 | 静态素材版本不证明实际加载或发行就绪 |

## 12. 决策、风险与未决问题

采用原 Viewer 专业域专题，是因为内容维护、素材交接与实际入口验收具有持续职责；产品配对 design 和页面稿继续拥有各自原范围。本专题没有另建产品 PRD、任务状态表或素材完成率。

主要风险是旧图稿/测试 fixture 的示例被当作当前配方，或部分投影被当作完整成本；按当前 canonical 来源逐项复核，缺副产物/能源细目时由 M4/Viewer 确认消费者范围。新中文建议尚需实际消费者采用及双语审校；字体、逐资产来源权利、真实组合运行与真人理解仍按手册保留未核范围。上游 profile、权限或后继条件变化时，先由原 owner 更新合同，再重审受影响内容单元。

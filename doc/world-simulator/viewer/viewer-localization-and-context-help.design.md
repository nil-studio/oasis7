# Viewer 本地化与上下文帮助内容设计

- 主责：`game_visual_interaction_designer`；实际 Viewer 消费由 `viewer_engineer` 承接。模型连接语义继续由 `agent_engineer` 的专业合同拥有，Launcher 与官网沿各自入口维护语言行为。
- 生命周期：`active`。本专题承接既有中文、英文可读义务及六类上下文帮助，不新增语言、游戏能力或全局设置中心。
- 审读基线：`84d4feb5314257c09612711b71346afe1481a117` 的需求、设计和源码。源码事实、既有目标与本文新交付分别注明；**本文新中英文正文是待采用、待语言审校的设计交付，不是已集成或运行通过的文案**。
- 专业入口：[Viewer](README.md)。短标签继续使用[品牌双语矩阵](viewer-brand-system-2026-06-05.design.md#25-bilingual-terminal-copy-matrix-target)；首局 C01～C09 继续由[首局内容设计](first-session-content-delivery.design.md#content-units)拥有。本专题不建立第二份品牌标签或工业物品命名权威。

## 1. 问题、目标与范围

当前 Viewer 已有双语字符串、语言偏好模块和局部帮助，但这些材料分布在实际组件、产品合同与手册中。维护者需要能够判断一段话从哪里来、哪些词不能互换、帮助应该回答什么，以及改变文字后要核对哪些入口。玩家则应在切换语言、阅读未知结果或打开帮助时，仍能分清对象、权限、世界事实和真正可行的下一步。

本设计交付入口语言与字符串来源索引、领域术语与参数规则、六类完整中英文帮助正文、缺译与字体降级要求，以及制作和验证方法。Viewer 是实际采用范围；Launcher 与官网列为相邻语言边界，防止把它们的默认语言、存储或发布过程误套到 Viewer。本文不要求改写这些入口，不创建翻译框架，不把运行中的 Agent 自由文本、玩家输入或协议标识自动翻译为世界事实。

原[次级帮助条款](viewer-page-module-design-2026-06-18.design.md#uiux-secondary-help-privacy)拥有帮助出现的位置与渐进披露原则。本文补齐该条款已经要求的六类内容，不重画页面，不将全部正文塞进已有表现层比例提示。文档交付不改变产品条目原有的设计覆盖、实现或验收状态。

## 2. 上游要求与约束

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001)；Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef | 已支持语言的选择、默认与 fallback 来源可辨 | [入口语言边界](#locale-scope) | Viewer locale 模块；Launcher、官网各自消费者 | 不统一各入口优先级，不承诺云端同步 |
| [REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001)；Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef | 中英文保留关键领域概念的差别 | [领域术语](#domain-terminology) | 原规则、成果卡、Prompt 与模型合同 | 不替换品牌短标签或 canonical 物品名称 |
| [REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001)；Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef | 动态参数与原文不因翻译改变事实 | [参数与原文](#parameter-expression) | 当前合法投影与文本消费者 | 不翻译签名原文或从文案反推世界值 |
| [REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001)；Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef | 缺译、缺字体仍保留关键含义及真实恢复 | [语言降级](#language-fallback) | 双语字符串与实际字体链 | 不以缺译删除既有语言支持承诺 |
| [REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001)；Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef | 长文本、窄屏、缩放和局部帮助仍可读可操作 | [可访问表达](#language-accessibility) | Viewer 原布局、输入与帮助组件 | 不代签所有辅助技术或环境组合 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | 不支持、已受理、已确认结果与传输未知保留原语义 | [原操作结果](#original-outcome) | Runtime 原请求、状态与读取资格 | 不实现去重、状态机或查询能力 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)；Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6 | 个人反馈不随语言或观察对象变成另一主体的结果 | [身份与归因](#identity-expression) | 当前经验证主体及原请求关联 | 文案不授予读取或控制资格 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005)；Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096 | 帮助解释点选与实际行动的区别 | [H01 点选](#help-select) | 原世界舞台与当前合法动作 | 不把观察焦点变成控制或因果依据 |
| [REQ-PROMPT-002](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-002)；Trace refs: O7-REQ-a82f1ee8-9d70-442c-9972-5f49f6665293 | 帮助区分对话、草稿与持续配置 | [H02 高层指导](#help-command) | 对话与 Prompt 原 authority | 不新增 Prompt、持久化或直接操作能力 |
| [REQ-AGENT-SURFACE-004](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-004)；Trace refs: O7-REQ-e3a70494-b3f3-4220-a654-6c9f07cdb8a4 | 阻塞帮助保留原因并给真实下一步 | [H03 不可提交](#help-submit-blocked) | typed 原因、当前动作可用性 | 不把所有失败改成重连或重试 |
| [REQ-AGENT-SURFACE-005](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-005)；Trace refs: O7-REQ-e393156d-2326-4d75-a0cb-ffba6ed74e5d | 仅在已有预检的行动中解释检查与正式提交 | [H03 不可提交](#help-submit-blocked) | 同状态预检与提交资格 | 不要求每项行动增加预检，不预留未来资格 |
| [REQ-BYOK-002](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-002)；Trace refs: O7-REQ-26cefe7a-deb5-4b21-845a-a2d4556f7dca | 保存和检查帮助保留凭证去向、保存范围与费用 | [H04 保存、检查、开始](#help-save-check-start) | 浏览器模型连接及原凭证合同 | 帮助不替代保存前的可见告知 |
| [REQ-BYOK-003](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-003)；Trace refs: O7-REQ-84294c93-3275-4d19-8faf-389950b87464 | 模型就绪、显式开始、游戏授权和世界结果不混同 | [H04 保存、检查、开始](#help-save-check-start) | 浏览器任务、执行许可与可见性 | 不承诺离页托管或模型赋予游戏权限 |
| [REQ-BYOK-005](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-005)；Trace refs: O7-REQ-e0ae0d80-31ca-488b-8ba2-a728bfb9ce8d | 模型检查通过不扩大供应商、模型或可玩性证明 | [H04 保存、检查、开始](#help-save-check-start) | 当前配置及独立供应商验收 | 不由一个预设推导全部模型可用 |
| [REQ-AGENT-SURFACE-004](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-004)；Trace refs: O7-REQ-e3a70494-b3f3-4220-a654-6c9f07cdb8a4 | 结果未知时帮助玩家沿原操作核对 | [H05 核对原结果](#help-original-result) | 当前受支持的授权只读查询 | 不把刷新连接或重新开始当原件查询 |
| [REQ-BYOK-004](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-004)；Trace refs: O7-REQ-d3d63078-ef30-45fb-8ae6-2f6db1f46c34 | 停止、更换与忘记不重放或撤销原世界行动 | [H06 停止与忘记](#help-stop-forget) | 原执行上下文、停止及恢复合同 | 不承诺撤销供应商 Key、已发费用或世界作业 |
| [REQ-BYOK-002](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-002)；Trace refs: O7-REQ-26cefe7a-deb5-4b21-845a-a2d4556f7dca | 停止保留、忘记删除与删除失败各有准确表达 | [H06 停止与忘记](#help-stop-forget) | 本机保存、账号隔离和实际操作结果 | 不把旧托管凭证停用说成本机 Key 删除 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 内容单元、编辑源与实际消费者可定位 | [交付索引](#language-delivery) | 本专题制作方与原文件 owner | 本行产品层 N/A；理由：交付组织不新增玩家功能；范围：本批语言与帮助材料。上列玩家义务仍适用。 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 翻译审校、采用与变化传播可执行 | [制作交接](#language-handoff) | 设计、双语 reviewer、Viewer 与 QA | 本行产品层 N/A；理由：制作协作不新增玩家功能；范围：本批交接。不代签团队产能或语言审校。 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 验证绑定实际文本、构建与目标环境 | [候选组合](#language-candidate) | 原 PR、构建及运行记录 | 本行产品层 N/A；理由：证据定位不新增玩家功能；范围：实际采用本内容的候选。不新增平行全仓 manifest。 |

### 2.2 技术职责的实际承接

| 职责 | 实际承接与本专题边界 |
| --- | --- |
| 语言选择、存储、URL 和页面语言 | [locale 模块](../../../crates/oasis7_viewer/software_safe_src/viewer_locale_preferences_module.js)拥有这些操作；[legacy_core.js](../../../crates/oasis7_viewer/software_safe_src/legacy_core.js)提供归一化与 `localeText`。本设计规定交付语义和失败表达，不另设 locale 状态或同步服务。 |
| 字符串与局部帮助展示 | [viewer_app.jsx](../../../crates/oasis7_viewer/software_safe_src/viewer_app.jsx)使用 `tr(locale, zh, en)` 等实际调用；其 `InlineHelpTip` 当前承接舞台表现层说明。六类新正文由各原上下文消费者采用，不能因存在一个 tooltip 就声称已全部实现。 |
| 领域事实、身份、请求和恢复 | [反馈隔离](viewer-control-plane-split-live-playback.design.md#gameplay-feedback-isolation)、[原请求回读](viewer-control-plane-split-live-playback.design.md#gameplay-original-result-lookup)与[同方案绑定](../m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)拥有字段和有效性。本专题只解释合法投影，缺 authority 时保留未知。 |
| 浏览器与旧托管模型入口 | [browser_byok_surface.jsx](../../../crates/oasis7_viewer/software_safe_src/browser_byok_surface.jsx)组合浏览器任务/连接与 `managementOnly` 旧连接管理；[浏览器模型面板](../../../crates/oasis7_viewer/software_safe_src/browser_model_connection_panel.jsx)消费本机保存与检查，[旧面板](../../../crates/oasis7_viewer/software_safe_src/model_connection_panel.jsx)保留旧会话说明。技术目标沿[本地凭证](../llm/self-supplied-model-connection.design.md#des-byok-002)和[停止恢复](../llm/self-supplied-model-connection.design.md#des-byok-004)，不能将两条路径的凭证生命周期拼成一句话。 |
| 布局、输入与可访问性 | 位置沿[原次级帮助条款](viewer-page-module-design-2026-06-18.design.md#uiux-secondary-help-privacy)；实际操作与字体按本文手册核验。本文不新增设置中心、渲染器、存储格式或键盘框架。 |

## 3. 内容与表现设计条款

<a id="locale-scope"></a>
### 3.1 入口语言与真实字符串来源

下表记录审读基线的源码行为，不把“源码存在”表述成当前部署已验证。`zh` / `en` 是 Viewer 内部取值；其他入口沿各自解析器，不能假设别名完全一致。

| 入口 | 选择顺序与保存边界 | 真实编辑/消费源 | 内容交付约束 |
| --- | --- | --- | --- |
| Viewer `/viewer.html`、`/software_safe.html`、`/` | 有效 `locale` → 有效旧 `language` → 已存偏好 → `en`。三路径共用 `viewer` 存储段；仍读旧路径键并尝试迁移。选语言更新内存、可用的 localStorage、`document.lang` 及 URL；存储/history 失败不抛掉当前选择。 | [locale 模块](../../../crates/oasis7_viewer/software_safe_src/viewer_locale_preferences_module.js)；[归一化及内嵌文字](../../../crates/oasis7_viewer/software_safe_src/legacy_core.js)；[主组件](../../../crates/oasis7_viewer/software_safe_src/viewer_app.jsx) | 当前无浏览器/OS 自动选语言步骤；不得把官网的自动跳转行为写成 Viewer 行为。持久化失败后的下一次打开不能保证沿用本次选择。 |
| 其他 Viewer pathname | 按当前 pathname 隔离存储段，仍遵循上述 query/存储/默认顺序 | 同一 locale 模块 | 入口映射改变时核 alias 与隔离负例；不擅自扩展共享路径。 |
| 原生 Launcher | `OASIS7_CLIENT_LAUNCHER_LANG` → `LANG` → `ZhCn`，由 `UiLanguage` 解析；与 Viewer 的英文默认不同 | [main.rs](../../../crates/oasis7_client_launcher/src/main.rs)，`UiLanguage::from_tag` / `detect_from_values` 及现有双语分支 | 本专题只记录边界。Launcher 配置说明仍沿[原配置设计](../launcher/game-client-launcher-guided-configuration-and-usability.design.md)，不被替换成 Viewer 偏好存储。 |
| 官网配对页面 | 首次语言判断使用浏览器语言并尊重已有手动选择；只沿页面声明的 `data-locale-en` / `data-locale-zh` 同源目标跳转，保留 query/hash | [site/assets/app.js](../../../site/assets/app.js)与实际页面 | 没有真实配对页不制造目标；`/doc/cn/`、`/doc/en/` 不参与自动跳转。中文故事不因此获得英文版本。 |
| 公开 Viewer 手册 | 仓库手册是内容真源；`cn` / `en` 是静态只读镜像路径 | [viewer-manual.manual.md](viewer-manual.manual.md)、[site 手册入口](../../site/manual/README.md)、[静态镜像设计](../../site/design.md#static-manual-design) | 修改本文不会自动更新公开镜像。实际采用后，涉及用户操作的变更先核仓库手册，再同步对应中英文页面、锚点和导航；页面生成成功不等于部署生效。 |

Viewer 的文本分散在主组件、[首产物卡](../../../crates/oasis7_viewer/software_safe_src/industrial_starter_outcome_card.jsx)、[认领选择卡](../../../crates/oasis7_viewer/software_safe_src/agent_claim_choice_card.jsx)、[首次对话预览](../../../crates/oasis7_viewer/software_safe_src/first_chat_unlock_preview.jsx)及两个模型连接面板中。它们有内嵌双语分支，也消费外部状态中的自由文本；当前不能把全部字符串描述为已经集中入库、完成提取或具备逐条缺译检测。

浏览器模型面板在本基线明确写有 DeepSeek 官方 / `deepseek-flash` 和“OpenRouter 尚未支持”；旧面板属于另一管理路径。该源码事实只限定此消费者，既不撤销产品既定供应商目标，也不证明当前部署已完成真实供应商验收。供应商、模型与能力文案的更新必须随实际能力证据，而不能仅改正文扩大支持声明。

<a id="domain-terminology"></a>
### 3.2 领域术语与语义边界

下列 G01～G16 是编辑单元，非新 requirement ID。中英文用于帮助与解释性长文；按钮、状态短标签仍引用品牌矩阵。具体工厂、材料、配方和模型的 canonical 名称/ID 仍来自原目录，本表不替它们另定中文商品名。

| 单元 | 中文 / English | 使用含义与不能混用的概念 | 语义来源 |
| --- | --- | --- | --- |
| G01 | 控制资格 / control authority | 当前玩家是否有权对目标行动；选中、可见或持有模型 Key 均不产生该资格。 | [Prompt 目标与资格](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-001) |
| G02 | 本地草稿 / local draft | 尚待玩家确认的输入；填入预设不等于发送或应用，也不自带跨会话保存承诺。 | [草稿与配置](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-002) |
| G03 | Prompt 覆盖值 / Prompt override | 有别于默认值的配置内容；采用、清除及应用范围依当前 authority，不能直译为永久保存。 | [默认与覆盖](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-003) |
| G04 | 配方 / recipe | 正式方案引用的规则身份，不能用图标、自然语言目标或相似名称代替。 | [同方案绑定](../m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding) |
| G05 | 批次 / batch | 当前方案与原结果中的批次数量；计划、接受和结算的数量各按来源表达。 | [首局内容来源](first-session-content-delivery.design.md#content-source-boundary) |
| G06 | 投入、产出、副产物 / inputs, outputs, by-products | 同一配方/结果中的不同项目；实际投入与预计投入不同，副产物不从示例图补全。 | [首局内容来源](first-session-content-delivery.design.md#content-source-boundary) |
| G07 | 消费账本 / consumption ledger | 本次动作实际使用的账本；不能拿另一余额或背包显示补成本。 | [同方案绑定](../m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding) |
| G08 | 产出归属 / output ownership | 产出归属于哪个主体；“属于我”不自动证明“由我刚才这次决定造成”。 | [成果与归因](first-session-content-delivery.design.md#outcome-attribution) |
| G09 | 生产结算 / production settlement | 已由原权威确认的生产结果；接受、进度动画或余额变化不能替代匹配结算。 | [首产物结算](../../game/gameplay/gameplay-industrial-starter-completion-contract.prd.md#3-五阶段结算表) |
| G10 | 稳定运行 / stable operation | 需另行验证的持续能力；一笔结算不等于稳定运行。 | [成果与归因](first-session-content-delivery.design.md#outcome-attribution) |
| G11 | 交付 / delivery | 原目标要求的交付事实；生产、入账、候选开放不能自动写成交付完成。 | [首产物边界](../../game/gameplay/gameplay-industrial-starter-completion-contract.prd.md#4-currenttarget-evidence-cutline) |
| G12 | 候选资格 / candidate eligibility | 可以评估后继候选；仍需当前材料、用电、授权等建设条件。 | [现有成果卡](../../../crates/oasis7_viewer/software_safe_src/industrial_starter_outcome_card.jsx) |
| G13 | 原操作记录 / original operation record | 发起过的同一操作及其权威结果；此处“原件”不是美术源文件，也不是新建的一笔请求。 | [原请求回读](viewer-control-plane-split-live-playback.design.md#gameplay-original-result-lookup) |
| G14 | 执行方 / executor | 在既有任务与授权内执行 Agent 循环的一方；模型供应商和游戏世界 authority 是不同主体。 | [模型就绪边界](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-003) |
| G15 | 模型供应商 / model provider | 接收模型数据、按其账户计费的服务方；模型名称或路由不能隐去实际接收范围。 | [凭证与费用](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-002) |
| G16 | 已报告用量、估计费用 / reported usage, estimated cost | 可取得的用量与费用估计分开；未知不填零，应用预算不等于供应商账户硬封顶。 | [凭证与费用](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-002) |

专业审读先核事实，再核自然语言。`Agent`、`Smelter MK1`、`Assembler MK1`、供应商品牌和实际模型 ID 保留当前正式身份；需要解释时加说明，不悄悄改名。若其他目录随后给出已采纳译名，本专题引用那个目录并更新说明，不能维持冲突的第二份真值。

<a id="parameter-expression"></a>
### 3.3 参数、原文与格式

花括号仅表示本设计中的内容插槽，不新增 DTO 或翻译键。采用方在原组件中绑定当前合法字段，并在 PR 中指出绑定位置；不能把动态世界描述当成已核定的静态译文。

| 输入类型 | 制作与消费规则 | 缺失或异常时 |
| --- | --- | --- |
| UI 静态说明、固定原因分类 | 对照同一事实写完整中英文句子；带上下文交付，保留否定、条件和时间含义，不逐词拼接。 | 沿 §3.8 明示缺译；不能统一改成“错误”。 |
| `{agentName}`、工厂/配方显示名 | 名字作为文本显示；实体身份仍以当前有权读取的 ID 关联。中英文外壳可以不同，名字不自动翻译成另一实体。 | 用可读的缺失说明，不把空名字改成“我的 Agent”。 |
| `{reason}`、`{nextStep}` | 原因来自 typed 类别及可公开说明；下一步来自当前真实可用动作。不得把供应商原始错误体、秘密或另一主体私有字段直接插入。 | 原因不足写不能确认的范围；无动作时给真实限制、允许的查看/暂缓/返回，不造按钮。 |
| 数量、单位、批次、时间 | 展示层可按语言排版，单位与精度沿原字段语义；请求/签名里的 ID、原始数量和精度不受显示格式影响。计划值、接受值、结算值不互换。 | 未知、未发布、读取失败和真实零分开；tick 不未经依据改成现实时间，非法数值不继续生成成功叙事。 |
| 请求、作业、profile、model 等标识 | 原样保留可公开的标识；自然语言中解释其用途。换行不能改动复制结果，不靠截断文本做身份判断。 | 保留查不到/不可读边界；缺标识不生成替代原操作。 |
| 玩家名字、聊天、Prompt、Agent 自由文本 | 保留原文及当前可见范围，按不可信文本显示。界面语言切换只改外壳，不偷偷改写玩家意图或把模型建议标为世界结果。 | 字体/语言不足仍保留可读原文和来源；本专题未引入自动翻译服务。 |

中英文插槽须一一对应。例如设计句“本次方案：{recipeName}；批次：{batchCount}。”对应 “Plan: {recipeName}. Batches: {batchCount}.”；两边绑定同一方案，避免单复数拼接造成 `1 batches`。需要自然句单复数时交付相应完整变体，不能让译者猜未给出的数据范围。示例不含固定配方数值，不把中文量词或英文小数分隔符写回业务参数。

<a id="original-outcome"></a>
### 3.4 原操作结果的语言不变量

沿产品[重叠状态优先级](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#41-重叠状态的主语义与真值优先级)解释相同事实：只有当前权威确认仍未结算时，才说“已受理，仍在等待”；只剩旧 accepted、无回执或无法确认当前状态时，保留原结果未知。已确认结果不因断网失效；已确认拒绝不因随后断网变成网络错误；不支持行动仍由能力边界解释。

本文不重定这些状态的短标签。长文中必须同时保留原对象、确认来源和可行下一步；原请求查询失败描述查询失败，不能据此说原行动已失败、已取消、未消耗或可以安全重发。英语 `pending`、`unknown`、`rejected`、`completed` 不能作为同义词替换以缩短文字。

<a id="identity-expression"></a>
### 3.5 身份、观察与归因

“你/你的”只用于当前身份有权读取且归属已确认的内容。公共事件描述事件主体；观察另一对象只改变查看焦点，不把旧反馈重新称为该对象的行动。翻译和帮助复用不能移除 world、玩家、Agent、原请求或会话代次等真实关联，也不能用显示名代替关联校验。

账号切换、退出、权限撤销和旧响应到达时，消费者继续遵守原反馈隔离；没有新权限就不展示旧私有正文、最新 Prompt 或差异。安全草稿是否可保留沿原控制合同处理，不把“保留输入”误写成“仍有权提交”。两种语言都不得在帮助、提示朗读、复制内容或截图中回显 Key、令牌和他人私有信息。

### 3.6 六类上下文帮助的采用方式

H01～H06 细化原 §10.9.6 的六个问题，是六个编辑单元，不是新功能 ID。以下中文和英文均是**完整目标正文**，供相应既有上下文采用；按钮/状态名称引用原品牌标签。采用时保留完整关键含义，可以将正文放入原折叠/帮助区域；不能只采用标题就声称该帮助已交付。

当前 `InlineHelpTip` 已有点击开关、外部指针关闭、Escape 与 ARIA 关联，但审读只确认它用于舞台比例说明。认领卡和首次对话预览有局部说明；六类完整帮助尚不能仅由这些组件存在推定完成。模型两种路径必须显示各自适用版本，不能把旧托管面板写成浏览器本机保存。

<a id="help-select"></a>
#### H01：点选是查看 / Selecting is for inspection

**上下文：** 世界对象被点选后的原详情/帮助区域。源为原舞台和权限语义，不新增选择即操作。

**中文目标正文：** 点选一个 Agent、工厂或地块，是查看这个对象当前允许展示的信息。点选不会授予控制资格，也不会下达移动、建造或生产指令。需要采取行动时，先核对操作入口中的对象、当前权限和影响，再明确提交。换一个查看对象不会改变上一笔操作的归属或结果；核对那笔操作时，仍要看它自己的记录。

**English target copy:** Selecting an Agent, factory or plot shows the information you are currently allowed to inspect. Selection does not grant control or issue a movement, construction or production order. Before taking action, check the target, your current authority and the stated effect in the action area, then submit explicitly. Selecting a different object does not change who owns an earlier operation or what happened to it. Use that operation's own record to check its outcome.

<a id="help-command"></a>
#### H02：Command 是高层指导 / Command gives high-level guidance

**上下文：** 指令输入与高级 Prompt 之间的原帮助位置，绑定当前目标 Agent 与操作类型。

**中文目标正文：** 在指令（Command）中，你可以向当前有权控制的 Agent 说明目标、询问状态或提出下一步。Agent 的行动仍受当前任务、授权和世界规则约束；一句指导不表示移动、施工或生产已经发生。发送对话是一条消息；修改 Prompt 或目标是另一项需要确认的配置操作。填入预设只改变草稿，收到请求也不等于配置已经应用。先确认对象和操作类型，再分别查看对话、配置结果和实际世界结果。

**English target copy:** Use Command to explain a goal, ask about progress or suggest a next step to an Agent you are currently allowed to control. Its actions still depend on its task, authority and world rules; giving guidance does not mean movement, construction or production has happened. Sending a chat message and changing a Prompt or goal are separate operations; a configuration change requires your confirmation. Filling a preset changes only the draft, and receipt of a request does not mean a configuration has been applied. Check the target and operation type, then read the chat response, configuration result and actual world outcome separately.

<a id="help-submit-blocked"></a>
#### H03：为什么不可提交 / Why submission is unavailable

**上下文：** 当前不可提交动作附近；首行原因和下一步使用当前可公开事实，不能以帮助替代主阻塞提示。

**中文目标正文：** 当前还不能提交：{reason}。下一步：{nextStep}。先区分输入问题、身份或权限问题、资源或规则条件，以及当前入口不支持的行动；它们的处理方式不同。若这里提供提交前检查，检查只说明当前状态下的资格，不预留资源，也不保证之后提交成功。若先前已经发送过操作但没有确认结果，请先核对那笔原操作。重连只能恢复通信，不能补授权限、取消原操作或证明可以再提交一次。

**English target copy:** Submission is currently unavailable: {reason}. Next step: {nextStep}. Input problems, identity or authority problems, resource or rule constraints, and actions unsupported by this entry require different responses. If this action offers a preflight check, it reports eligibility for the current state; it does not reserve resources or guarantee a later submission. If you already sent an operation and its outcome is unconfirmed, check that original operation first. Reconnecting restores communication; it does not grant authority, cancel the operation or prove that a new submission is safe.

若原因或下一步未提供，采用方不能把两个插槽留空：先依 §3.3 表达无法确认的范围和当前实际允许的查看/暂缓/返回。没有预检的行动不显示预检成功状态；上述说明不要求添加新的预检按钮。

<a id="help-save-check-start"></a>
#### H04：保存、检查和开始有什么不同 / Saving, checking and starting

**上下文：** 浏览器模型连接的保存/检查与原任务执行的开始区域。以下主正文适用于浏览器本机保存路径；凭证去向和费用告知仍须在保存或检查前直接可见。

**中文目标正文：** 保存 Key 只把凭证保存在此浏览器的本站数据中，刷新或关页后仍会保留。同源脚本可以读取它；共享设备请谨慎使用。模型请求只把 Key 发送给所选、受支持的供应商，不上传 Oasis7 后端。保存不会认领 Agent、授予游戏权限或自动开始运行。

测试连接需要你单独同意，可能产生供应商费用，但不写入游戏世界。通过只证明这项配置当时完成了相应技术检查，不证明所有模型可用、游戏授权有效或玩法已经完成。

开始还需要确认当前 Agent、实际任务目标和本次运行限制。页面可见且执行许可有效时才可发起新的推理和提交；离开、关闭或刷新页面不能当作持续托管承诺。世界已经受理的作业仍按世界规则推进。未知用量不是零，本页限制也不能限制同一 Key 在其他应用中的消费。

**English target copy:** Saving the key keeps it in this browser's data for this site, including after a reload or closing the page. Same-origin scripts can read it; use care on shared devices. Model requests send the key only to the selected, supported provider, not to the Oasis7 backend. Saving does not claim an Agent, grant game authority or start a run.

Testing the connection requires separate consent and may incur provider charges, but it does not write to the game world. A passed check proves only the technical checks completed for that configuration at that time. It does not prove that every model is supported, that game authority is valid or that gameplay has completed.

Starting requires confirmation of the current Agent, its actual task goal and the limits for this run. New inference and submissions require a visible page and valid execution permission. Leaving, closing or reloading the page does not provide continued hosting. Jobs already accepted by the world still follow world rules. Unknown usage is not zero, and this page cannot cap spending by the same key in other apps.

**旧托管管理版本（中文）：** 此处只管理旧托管连接：查看状态、停止或忘记。这里不保存新的浏览器 Key，不执行模型检查，也不开始或续期旧运行。旧连接的凭证和历史结果仍按它原有的范围核对。准备使用浏览器执行前，先确认旧执行已经停止，再进入当前受支持的浏览器任务与连接流程。

**Legacy management version (English):** This area only manages legacy hosted connections: inspect their status, stop them or forget them. It does not save a new browser key, run a model check, start a legacy run or renew its lease. Check the credential scope and historical outcomes against the original connection. Before using browser execution, confirm that legacy execution has stopped, then use the currently supported browser task and connection flow.

<a id="help-original-result"></a>
#### H05：怎样核对原结果 / Checking the original outcome

**上下文：** 操作未知、重入或恢复处，沿本入口实际已有的授权只读查询。

**中文目标正文：** 先确认你要核对的是哪笔操作：它当时的对象，以及本入口提供的操作标识。使用原操作的结果入口读取当前有权查看的记录。只有权威记录确认后，才按记录说明已受理、仍在等待、已完成或已拒绝；没有回执、列表尚未读完、连接失败或暂时无权读取，都不能单独证明操作失败或没有产生效果。

刷新连接状态只核对连接，不等于查到世界原结果。这里暂不能查询原结果时，请保留原操作信息，避免再次提交。若有其他已经提供且你有权使用的结果入口，可在那里核对；没有可用入口时，结果仍是未确认。更换语言、观察对象、模型或页面，都不会把原操作变成另一笔新操作。

**English target copy:** First identify the operation you want to check: its original target and any operation reference this entry provides. Use the original operation's result entry to read the records you are currently allowed to access. Describe acceptance, continued waiting, completion or rejection only when confirmed by the authoritative record. A missing receipt, an incompletely read list, a connection failure or temporarily lacking read access does not by itself prove failure or the absence of effects.

Refreshing connection status checks the connection; it does not establish the world's original outcome. If you cannot look up the original result here, keep the operation information and avoid submitting it again. If another result entry is already available and you are allowed to use it, check there. If none is available, the outcome remains unconfirmed. Changing language, the inspected object, model or page does not turn the original operation into a new one.

采用方必须在实际无查询能力处显示限制，只提供已存在且当前主体有权使用的入口；这段正文不授权新增、猜测或绕过读取路径。

<a id="help-stop-forget"></a>
#### H06：停止与忘记分别做什么 / Stopping and forgetting

**上下文：** 浏览器模型连接停止/忘记动作附近，与原操作结果并列显示。操作尚未确认时不能提前显示成功正文。

**中文目标正文：** “停止新决策”停止新的模型调用和提交，保留此账号在本站保存的 Key。再次开始仍需你确认。“断开并忘记”停止当前运行并删除此账号在本站保存的 Key，同时让其他同源页面停止使用旧配置。只有删除结果确认后，才能说本机 Key 已删除；如果失败，按页面提示清除本站数据，并核对操作结果。

两项操作都不撤销供应商账户中的 Key，不保证已发送模型请求退款，也不取消世界已经受理的作业。已发请求仍可能产生费用，原世界结果仍需沿原操作核对。需要撤销供应商 Key 时，请在供应商账户完成相应操作；本应用不会把断开显示成已经替你撤销。

**English target copy:** Stop new decisions stops new model calls and submissions while retaining the key saved for this account in this site's data. Starting again still requires your confirmation. Disconnect and forget stops the current run and deletes this account's saved key from this site, while directing other same-origin pages to stop using the old configuration. The key can be described as deleted locally only after deletion is confirmed. If deletion fails, follow the page's instructions to clear this site's data and verify the result.

Neither action revokes the key in your provider account, guarantees refunds for model requests already sent, or cancels jobs the world has accepted. Sent requests may still incur charges. Check world outcomes through the original operations. If you need to revoke the provider key, do so in the provider account; disconnecting here must not be presented as having done that for you.

**旧托管管理版本（中文）：** 停止保留原有效会话连接；忘记停用该旧连接的凭证。两者都不表示本站另存的浏览器 Key 已删除。只有本次停止或忘记被确认后，才显示相应完成；原世界结果仍单独核对，不因旧连接消失就当作未执行。供应商 Key、已发费用和已受理世界作业仍遵守上述边界。

**Legacy management version (English):** Stop retains the original valid session connection; Forget disables that legacy connection's credential. Neither means that a separate browser key saved by this site has been deleted. Show completion only after the stop or forget operation is confirmed. Check original world outcomes separately; a missing legacy connection does not prove that no action ran. The same limits still apply to the provider key, charges for sent requests and world jobs already accepted.

当停止已确认但删除失败时，目标反馈为“已停止，但本机 Key 尚未删除。请按指引清除本站数据并再次核对。” / “Stopped, but the local key has not been deleted. Follow the instructions to clear this site's data, then verify again.”。若停止也未确认，改为“停止与本机删除结果尚未确认，请核对状态。” / “Stopping and local deletion are unconfirmed. Check their status.”；不能先套用已停止的句子。

<a id="language-accessibility"></a>
### 3.7 可访问表达与字体

目标/阻塞/可信反馈/下一步须有可读文字，不能只靠颜色、动画、图标或音效表达。长中文、长英文、实际名字和错误可换行或滚动，不以缩小文字、截断关键否定或只留图标满足布局。按当前支持范围核 200% 文字、窄屏和低高度；减少动态效果后仍能发现状态变化，已确认原结果不因切语言重复播报为新事件。

帮助可以通过当前可用的键盘与触控路径打开、阅读和关闭，焦点可见，关闭后回到发起处；普通状态刷新不抢焦点。输入法组合期间的 Escape 不应误关帮助或丢失草稿，中文组合结束不误发指令。现有 `InlineHelpTip` 的 ARIA 与 Escape 代码不能单独证明这些组合通过，采用方按手册实测。

Viewer [viewer.html](../../../crates/oasis7_viewer/viewer.html)当前定义 body/display/mono 系统字体栈，如 `IBM Plex Sans`、`Segoe UI`、`Georgia` 与 `Menlo`，再经语义别名消费；[终端样式](../../../crates/oasis7_viewer/viewer_terminal_shell.css)使用这些别名。字体族名不证明目标机器的 CJK 字形可用。原生 Launcher 的 `configure_egui_fonts` 使用现有嵌入 `third_party/ms-yahei.ttf` 或 `OASIS7_CLIENT_LAUNCHER_FONT` 覆盖，是另一条加载链；存在文件与加载代码不等于已经核定全部许可或实际渲染。

本批不生成、添加或分发字体。采用候选需记录实际字体来源和 fallback，并检查中文标点、数字、拉丁标识、长名字及混排，不把设计 SVG 中的字体名称当作 runtime 已携带字体。

<a id="language-fallback"></a>
### 3.8 缺译与可读降级

当前 `tr` / `localeText` 的双语选择不等于已有全局缺译系统。下列为制作与采用的目标规则，需在实际消费者检查后才能声称实现：

1. 静态关键正文交付中英文全句和相同插槽；已知缺译不能交付空白、原始键名、乱码或一串错误码来代替目标/阻塞/原结果/下一步。
2. 暂缺当前语言时保留完整、可读的源语言正文，附可辨认说明。中文目标提示为“此段暂缺中文译文，以下保留原文。”；英文目标提示为 “An English translation is not yet available for this passage. The source text is shown below.”。未知世界事实仍按未知表达，不因原文存在而补成确定结果。
3. 字体缺字先沿实际可用字体链恢复可读文字；未能恢复时显式说明受影响内容，保留原信息和当前安全的查看/停止/返回路径。不能自动转成含义不明的缩写或把关键原因移出页面。
4. 记录缺译/缺字及受影响入口，修复后复核原候选。中文、英文仍是既有承诺范围；不能以本专题的未完成翻译为理由静默删去语言、降低验收或声称“该语言不适用”。是否新增语言或改变正式支持范围须回到原产品决定。

## 4. 制作规格与交付物

<a id="language-delivery"></a>
### 4.1 交付索引与可编辑源

| 交付内容 | 本批可编辑源与实际采用位置 | 本批交付状态与边界 |
| --- | --- | --- |
| 入口语言、参数与原结果表达规则 | 本文 §3.1、§3.3～§3.5；实际消费者为 locale 模块、`viewer_app.jsx`、反馈与各专业卡片 | 来源审读和内容规则；没有新提取器、JSON 翻译包或已集成声明。 |
| 领域术语 G01～G16 | 本文 §3.2；品牌短标签仍在原品牌矩阵，物品/配方身份仍在原目录 | 16 个语义编辑单元；英文为待审校设计建议，非第二套物品命名表。 |
| 帮助 H01～H06 | 本文 §3.6；H01/H02/H03/H05 由原舞台、Command、动作与恢复上下文采用；H04/H06 对接浏览器模型面板、任务执行区域及旧管理面板 | 六类完整中英文正文；H04/H06 含适用旧管理路径的变体。未声称已挂接帮助入口。 |
| 缺译与操作未确认表达 | 本文 §3.8 及 H06 的条件反馈 | 可用目标正文；采用时核实际条件，不能只改文案掩盖停止/删除实现问题。 |
| 验证与后续制作方法 | [配套手册](viewer-localization-and-context-help.manual.md) | 提供静态、语义、消费者、环境与真人方法，不预填运行通过。 |

本批只有两份仓库文档，不产生字体、配音、图稿、翻译服务数据或自动生成的公开手册。品牌和产品名按原来源；后续如引入外部翻译、字体或其他素材，在实际采用 PR 留来源、版本及已核权利范围，不能以根 LICENSE 推定全部外部材料可再分发。

<a id="language-handoff"></a>
### 4.2 制作交接与变化传播

| 环节 | 具体责任与交付判据 |
| --- | --- |
| 建立语境 | 内容制作方给出 H/G 单元、触发事实、当前/目标身份、实际消费文件、上游版本、完整中英文、参数来源与安全下一步。语言 reviewer 不应凭孤立字符串猜执行阶段。 |
| 双语审校 | 具备中英文能力的 reviewer 对照同一情境分别读两种正文，核否定、时态、主体、费用、未知与条件；保留实际意见与修订。作者自产译文或机器翻译输出不能登记为已完成独立语言审校。 |
| Viewer 采用 | `viewer_engineer` 修改原消费者的双语分支和帮助接点，保留既有 locale 链与状态来源；模型语义不符由 `agent_engineer` 核原执行/存储合同。没有字段或查询能力时记录缺口，文案不代造。 |
| 手册同步 | 实际行为/帮助被采用后，核 canonical Viewer 手册中受影响操作；涉及公开说明时按原 site 手册路径同步中英文镜像、锚点和导航。未采用的目标正文不以已上线说明发布。 |
| QA 接收 | 按配套手册留下文字/代码版本、实际语言与环境、输入来源、截图和原结果对照。结构检查、双语语义、实际操作与真人理解分别记录，不能相互代签。 |
| 变更传播 | 原因/状态/权限变更重核 H01～H03/H05 与参数、归因；模型执行/存储/费用变更重核 H04/H06 及可见告知；规则/结算变更重核对应 G 与首局原内容；语言、字体、布局改变重核长文本、焦点与可访问性。旧审校和旧截图只按原候选引用。 |

稳定单元用本文件锚点和 Git 历史定位；采用 PR 记录哪些句子实际进入哪个原组件。不建立与现有源码并行的“已上线译文表”。多人可按单元分工，同一段落/组件由当前写入者整合；重命名或删除时同步原引用及适用验证。

本批没有重复翻译、审校、集成和返工工时数据，不据此估算大型游戏语言产能。后续真实批次可在原任务中分别记录字量/单元、语种、审校轮次、集成和返工时间；扩量决策应基于这些实际记录。

<a id="language-candidate"></a>
### 4.3 实际候选组合

内容采用后沿原 PR、构建记录和 `session.meta` 固定 source/tested tree、Viewer bundle、实际 H/G 修订、world/主体/原操作可见范围、入口、locale、viewport、缩放、输入方式、实际字体与 execution lane。受影响的 Launcher 或官网另记自身版本与入口，不能借 Viewer 结果代签。原 UI 文本未被替换时明确它仍消费哪一版文字。

只记录当前验证实际使用的材料，不将本文所有参考文件变成运行依赖。不新增平行 manifest；最终构建新鲜度沿[原产物规则](../../engineering/workflow/package-source-security.md#final-artifact-freshness)。本批静态审读不能成为供应商、世界执行、部署、辅助技术或真人结果证据。

## 11. 验证与验收

静态来源、双语含义、实际消费者行为、目标环境呈现和真人理解分别核验。文档修改执行适用静态检查；相关文字或组件采用后再选择实际受影响场景，不为本批文档启动游戏或付费模型调用。结果进入原 PR/证据入口，不在长期规范累计每轮 PASS。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001)；Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef | [入口语言边界](#locale-scope) | 语言优先级、存储失败与 alias/隔离保持实际合同 | [LH-LOCALE](viewer-localization-and-context-help.manual.md#lh-locale)，同候选分别核 query、偏好、默认和不可写负例；相邻入口按自身方法选择 | 原 PR 的来源与语言操作结果 | 跨设备同步、其他语言及未测入口 |
| [REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001)；Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef | [领域术语](#domain-terminology) | G01～G16 在中英文同一事实下保留区别 | [LH-REVIEW G](viewer-localization-and-context-help.manual.md#lh-review)，审读实际采用文本与原 authority；未集成时仅记设计审读 | 双语 reviewer 原意见和修订 | 全目录译名、世界规则与真人理解 |
| [REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001)；Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef | [参数与原文](#parameter-expression) | 参数、单位、原文和未知值不被翻译改写 | [LH-PARAMETERS](viewer-localization-and-context-help.manual.md#lh-parameters)，同输入跨语言核名字、数量、缺失与不可信文本 | 原输入/显示/原操作对照 | 所有数据类型及上游投影正确性 |
| [REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001)；Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef | [语言降级](#language-fallback) | 缺译或缺字保留关键含义与真实安全路径 | [LH-FALLBACK](viewer-localization-and-context-help.manual.md#lh-fallback)，在受控候选分别构造缺译/字体不可用，记录实际降级 | 缺陷、恢复截图与受影响语言 | 无缺陷证明、所有平台字体或新增语言 |
| [REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001)；Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef | [可访问表达](#language-accessibility) | 两语言的长文、帮助、焦点和输入可达 | [LH-ACCESSIBILITY](viewer-localization-and-context-help.manual.md#lh-accessibility)，按实际支持组合选宽窄/低高、200%、IME、键盘/触控及 reduced motion | 组合清单与实际操作证据 | 未测辅助技术和环境 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64 | [原操作结果](#original-outcome) | 已受理、未知、拒绝、已确认结果不因断连互换 | [LH-OUTCOME O1](viewer-localization-and-context-help.manual.md#lh-outcome)，同候选按真实状态源对照中英文独立负例 | 原请求/权威状态/显示对照 | Runtime 幂等与未覆盖故障 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)；Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6 | [身份与归因](#identity-expression) | 语言与观察切换不污染个人反馈 | [LH-OUTCOME O2](viewer-localization-and-context-help.manual.md#lh-outcome)，实际双主体/匿名及身份切换，缺条件则记未执行 | 各主体可见信息与原反馈 | 全部权限组合或安全认证 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005)；Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096 | [H01 点选](#help-select) | 点选帮助不暗示控制和行动效果 | [LH-REVIEW H01](viewer-localization-and-context-help.manual.md#lh-review)，逐语言对照查看、无权限和旧结果情境 | H01 语义审读与采用差异 | 选择实际效果和真人理解 |
| [REQ-PROMPT-002](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-002)；Trace refs: O7-REQ-a82f1ee8-9d70-442c-9972-5f49f6665293 | [H02 高层指导](#help-command) | 草稿、聊天和配置应用保持可分辨 | [LH-REVIEW H02](viewer-localization-and-context-help.manual.md#lh-review)，比较未发送、聊天接受及配置已应用的独立输入 | H02 双语审读与反例 | Prompt 运行合同或持久化 |
| [REQ-AGENT-SURFACE-004](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-004)；Trace refs: O7-REQ-e3a70494-b3f3-4220-a654-6c9f07cdb8a4 | [H03 不可提交](#help-submit-blocked) | 原因对应真正可行下一步 | [LH-REVIEW H03a](viewer-localization-and-context-help.manual.md#lh-review)，分别核输入、权限、资源/规则、不支持及未知 | 原因/下一步逐项双语对照 | 所有动作和端到端恢复 |
| [REQ-AGENT-SURFACE-005](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-005)；Trace refs: O7-REQ-e393156d-2326-4d75-a0cb-ffba6ed74e5d | [H03 不可提交](#help-submit-blocked) | 已有预检不被描述成受理或成功保证 | [LH-REVIEW H03b](viewer-localization-and-context-help.manual.md#lh-review)，同状态合格/不合格及检查后状态变化，未提供预检另记不适用理由 | 检查/提交含义审读 | 实际预检资格一致性 |
| [REQ-BYOK-002](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-002)；Trace refs: O7-REQ-26cefe7a-deb5-4b21-845a-a2d4556f7dca | [H04 保存、检查、开始](#help-save-check-start) | 保存范围、接收方和付费含义准确 | [LH-MODEL M1](viewer-localization-and-context-help.manual.md#lh-model)，核实际浏览器/旧管理路径和保存前告知；静态与真实调用分记 | 消费者与中英文告知对照 | 凭证安全和供应商账单证明 |
| [REQ-BYOK-003](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-003)；Trace refs: O7-REQ-84294c93-3275-4d19-8faf-389950b87464 | [H04 保存、检查、开始](#help-save-check-start) | 开始、游戏授权及离页边界不被连接成功覆盖 | [LH-MODEL M2](viewer-localization-and-context-help.manual.md#lh-model)，比较保存、检查就绪、无任务许可及明确开始情境 | 独立阶段和运行限制表达 | 浏览器生命周期与世界执行完整验收 |
| [REQ-BYOK-005](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-005)；Trace refs: O7-REQ-e0ae0d80-31ca-488b-8ba2-a728bfb9ce8d | [H04 保存、检查、开始](#help-save-check-start) | 检查结果只覆盖实际配置和检查范围 | [LH-MODEL M3](viewer-localization-and-context-help.manual.md#lh-model)，对照实际预设/未支持配置与独立供应商证据 | 支持声明与证据边界 | 全部模型、生产与发行就绪 |
| [REQ-AGENT-SURFACE-004](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-004)；Trace refs: O7-REQ-e3a70494-b3f3-4220-a654-6c9f07cdb8a4 | [H05 核对原结果](#help-original-result) | 未知原操作不引导新提交 | [LH-OUTCOME O3](viewer-localization-and-context-help.manual.md#lh-outcome)，有/无可用原查询及查询失败分别核真实下一步 | H05 与实际查询限制 | 不存在的查询能力与自动恢复 |
| [REQ-BYOK-004](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-004)；Trace refs: O7-REQ-d3d63078-ef30-45fb-8ae6-2f6db1f46c34 | [H06 停止与忘记](#help-stop-forget) | 停止/忘记保留历史世界后果 | [LH-MODEL M4](viewer-localization-and-context-help.manual.md#lh-model)，分别核停止已确认、未知停止与历史已受理/未知操作 | 操作结果与 H06 条件文案 | 撤销供应商 Key、退款、世界取消 |
| [REQ-BYOK-002](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-002)；Trace refs: O7-REQ-26cefe7a-deb5-4b21-845a-a2d4556f7dca | [H06 停止与忘记](#help-stop-forget) | 本机保留/删除/删除失败与旧会话停用区分 | [LH-MODEL M5](viewer-localization-and-context-help.manual.md#lh-model)，按原存储故障方法核各结果，不用单一成功截图 | 路径、存储结果与显示 | 所有存储实现和账号隔离证明 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [交付索引](#language-delivery) | 编辑源、单元和消费者可定位 | [LH-STATIC](viewer-localization-and-context-help.manual.md#lh-static)，核两文件、稳定锚点及声明采用的实际源码 | 来源摘要和链接检查结果 | 集成、运行或语义通过 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [制作交接](#language-handoff) | 修订能传播到受影响文本与公开说明 | [LH-HANDOFF](viewer-localization-and-context-help.manual.md#lh-handoff)，沿一个真实改动追上游、两语言、实际组件和相关手册 | 原 PR 的采用/未采用及审校记录 | 生产吞吐和未经发生的完整批次 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [候选组合](#language-candidate) | 运行/理解结果可定位到实际文字和环境 | [LH-CANDIDATE](viewer-localization-and-context-help.manual.md#lh-candidate)，复用原记录固定候选、语言、字体和范围 | 原 bundle/运行记录与实际单元关联 | 未运行、未采用或未测环境 |

## 12. 决策、风险与未决问题

本专题采用既有产品语言义务、六类帮助问题和真实组件作为最小持续职责；不另建品牌短标签、全局 Help 中心或翻译资源框架。两份文档是本批完成边界。新正文的独立语言审校、实际组件采用、缺译处理、字体环境、辅助技术与真人理解均须取得各自证据，不能由文档合入代签。

需要产品决定的边界是新增正式语言、自动翻译玩家/Agent 原文、跨设备语言偏好、改变正式支持范围，以及新增设置/帮助产品能力。现有中英文、未知结果、权限和费用义务已经采纳，缺少实现或证据时应补齐，不再作为待产品决定搁置。

浏览器 BYOK 的当前源码与旧专业设计中的迁移状态可能不处于同一更新点；采用方要沿实际候选、组件与原实施证据核对，不用本专题重写原产品/专业状态。原实现的任何权限、原件查询、存储或停止错误仍由相应技术 owner 修复，不能靠翻译弱化义务。与页面重构或模型迁移同时实施时，复用实际合入后的消费者和版本，保持本文锚点与内容职责稳定。

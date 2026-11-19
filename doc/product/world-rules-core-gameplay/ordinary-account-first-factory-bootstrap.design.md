# 普通账号首厂工业起步专项设计

## 文档身份

- 配对产品 PRD：[`first-session-and-continuation.prd.md`](first-session-and-continuation.prd.md)
- 上位产品 PRD：[`prd.md`](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：2026-11-19
- 专业域权威：[`game 专业 PRD`](../../game/prd.md)、[`首局工业 walkthrough`](../../game/gameplay/gameplay-industrial-representative-execution-walkthrough.prd.md#3-五阶段玩家-walkthrough)、[`受治理的区域能力与扩展`](governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007)、[`工业原生内容交付设计`](../../game/gameplay/industrial-native-content-delivery.design.md)、[`world-runtime 专业 PRD`](../../world-runtime/prd.md)、[`testing 专业 PRD`](../../testing/prd.md)

本文是配对首局 PRD 的专项设计，补齐“普通账号已经进入世界并取得有效 Agent/Context，却没有预置工厂、厂址、建设许可或建材”时，首局怎样继续。它继承配对 PRD 的 `REQ-FIRST-INDUSTRIAL-*`、`REQ-FIRST-BRANCH-*`、AC 与 FS 组合验收，不创建第二套产品要求或完成状态。

专业 authority 继续拥有地点权利、建设许可、资产账本、资金结算、空间冲突、BuildFactory、持久化、receipt、API 与测试实现。本文只定义首局怎样消费这些事实，使首厂成为玩家学会独立推动一次世界目标的体验载体，而不是把全部内部系统变成新手课程。

<a id="first-factory-experience-shell"></a>
## 1. 首局目标在普通账号起步中的应用

### 1.1 首局真正要完成什么

首局目标、知识预算及各项成果的分判统一沿[最小可独立游玩](first-session-and-continuation.prd.md#minimum-independent-play)。本文将它应用到普通账号的地点和建材缺口：玩家理解当前目标、自己与 Agent 的分工、主要承诺和真实结果，并能独立决定补足条件、改道或停止；取得地点、建厂和首产物分别记录，不能代替该能力的观察。

### 1.2 本情境的成果边界

| 结果 | 成功含义 | 不能代签什么 |
| --- | --- | --- |
| 首厂建成 | 建设结果与归属、位置和类型一致的 factory asset 可核对 | 首次生产、稳定能力、交付或需求满足 |
| 首产物完成 | 配对 PRD 选定的 `production_only` completion profile 成立 | 稳定能力、终端交付或需求满足 |

真实 blocker 下，学习能力按主 PRD 的独立观察判定，首厂和首产物仍保持未完成。prepared/bootstrap 环境中的建设成功只证明其实际世界结果；不能由此推断玩家理解、持续能力或继续意愿。

### 1.3 起步时的知识预算

| 本次信息 | 呈现方式 | 原因 |
| --- | --- | --- |
| 地点用途/期限、总费用与占用、主要风险、原请求状态 | 改变当前承诺的部分进入首读 | 玩家必须知道确认和离场的实际后果 |
| 地点授权、材料来源和结算引用 | 按需展开，影响当前决定的缺口提升为主阻塞 | 玩家理解“允许建设、材料可用”，不必先学习全部内部对象 |
| 长期权利转让、复杂物流、组织治理、多阶段补偿 | 后续遇到时解释 | 不作为此次普通账号起步的学习门槛 |

按主 PRD 的信息优先级，默认突出一个主要目标、一个主要 blocker、一个当前决定和一个下一动作。其他真实可用路径可比较，系统目录不变成新玩家任务清单。

## 2. 当前断点与设计取舍

Issue #6042 的试玩输入表明：普通账号已经完成登录、Agent/Task 绑定和有效 Context 读取，但工业观察没有给出该主体可用的材料、厂址或位置授权；现有 starter 样例依赖内部 bootstrap 预置。这个事实证明当前代表性链的普通账号前置尚不能由现有样例代签，但不证明全仓相关能力均不存在。

本设计采用以下取舍：

- **能力先于推荐**：首局工业主推荐沿原整链 Gate；缺条件时先给可执行的准备动作或真实替代目标，不把解释缺口当作整链可达。
- **一个默认路径先于完整目录**：首读提供一条当前安全的 starter 方案；其他路径按阻塞或主动比较展开。
- **一次关键确认先于逐 authority 点选**：地点使用、建设许可、建材和费用可以组合成一个玩家可理解的承诺，系统证据仍分别成立。
- **玩家语言先于专业术语**：默认说“可用地点、允许建设、建材已到位、总承诺、实际结果”；专业对象按需展开。
- **Agent 处理机械步骤**：玩家确认目标和主要承诺；普通校验、同条件刷新、材料转移和建设提交可在有效授权内代办。
- **真实不可达先于假顺滑**：缺地点、资格、许可、供给或实现时返回 blocker 和替代目标，不静默注入资产。
- **首厂与首产物分开**：Build evidence 和 factory asset 只完成工厂节点，不签发生产里程碑。

<a id="ordinary-bootstrap-journey"></a>
## 3. Canonical 玩家旅程

### 3.1 进入：先给目标，不先给系统目录

玩家完成 Agent/Context 入场后，系统在后台读取世界、区域、主体、地点和供给能力。满足原整链 Gate 时可以说明“建立基础冶炼设施并生产第一笔铁锭”；尚缺地点或建材时，先说明当前真实可做的准备目标及其完成边界。玩家不需要先浏览区域治理、市场和资产系统，也不会把“准备地点”误解为已经承诺首产物可达。

首读只显示：

- 设施将带来的真实能力；
- 当前推荐的地点和整体方案；
- 主要成本、期限和风险；
- 当前最主要 blocker；
- 需要玩家确认的一个决定，以及下一步。

### 3.2 比较与确认：一次有意义的承诺

当前专业合同及正式入口支持时，starter 方案可以组合有限期地点使用权、建设许可和 build-kit。玩家确认的是完整承诺：“在这个地点、以这些实际成本和期限建立这座设施”。原确认绑定主体、地点、用途/期限、费用和占用上限、允许代办的动作、有效期及失败/退出边界；一次界面确认不使多个 authority 变成一个已经原子提交的事务。

提交前必须能回答：

- 我将得到什么；
- 我将付出、占用或可能损失什么；
- 哪些条件仍可能变化；
- 失败时哪些价值保留、释放或仍待确认。

若当前只有一条合法路径，不伪造多路线策略。玩家仍可以暂缓、查看证据或选择真实可用的替代目标。

### 3.3 授权内代办：Agent 处理机械步骤

确认目标与承诺后，Agent 或协调器可以在有效授权内代办报价刷新、地点/许可取得、材料采购或预留和建设提交。以下变化必须重新交还玩家：

- 总成本、地点、期限、用途或主要风险实质变化；
- 需要原确认之外的权利、支付、不可逆效果，或原授权已失效/不覆盖当前动作；
- 默认路径失效，必须在真实替代目标间选择；
- 结果未知，且恢复方式会产生新的承诺或风险。

原承诺已明确包含的付款、权利取得和建设提交，可以在范围和额度内代办；每步仍按当前 authority 重验，不因其不可逆就重复索要同一许可。普通字段转换、校验和同条件重查不应成为连续确认按钮。Agent 的模型回复不能代替 authority 接受、世界效果或 receipt。

### 3.4 反馈：先讲结果，再讲证据

反馈顺序固定为：

1. **发生了什么**：已完成、正在处理、被拒绝或结果尚未确认；
2. **为什么**：成功的关键事实或当前主要 blocker；
3. **付出与保留**：实际消费、占用、损失、仍有效的权利和材料；
4. **下一步**：进入生产、修复、改道、等待或安全停止；
5. **证据**：按需展开原 request、receipt、asset 和 authority refs。

首厂建成和首产物完成使用不同反馈与 evidence refs。后续铁锭只归其实际结算账本；开放 Assembler 候选不表示铁锭已经转入 builder 账本或支付了建设，只有真实受支持的转移与消费才能表述该用途。重新连接只恢复既有结果，不把旧成功再次播报成新事件。

### 3.5 离场与回访

首厂建成、遇到可理解 blocker 或主动暂缓，都可以成为自然停点。离场前保留：当前目标、最近决定、已成立事实、未决请求、主要 blocker 和回来后的第一动作。

回访从权威事实重新投影，不复活过期授权，不假定旧报价仍有效，不重新结算旧效果。若原请求结果未知，恢复沿原 identity 查询；刷新世界快照不能代替原请求核对。

<a id="first-factory-profile-gate"></a>
## 4. 普通账号准备动作与原整链 Gate

本文复用配对 PRD 的 [Starter Industrial Feasibility Gate](first-session-and-continuation.prd.md#222-starter-industrial-feasibility-gate)及[同初态前向组合](../../game/gameplay/industrial-native-content-delivery.design.md#starter-fresh-forward-gate)，不另立首厂 Gate 顶层结果：

- `candidate_available`：同一份 fresh authority snapshot 已证明所选 starter chain 可达原 profile 的完成边界。当前 `production_only` 必须包含适用建厂及生产所需的累计材料/电力、授权、实际生产结算与原里程碑条件，不能只证明工厂能建成。
- `no_safe_starter_chain`：整链尚不可证明，保留 `current-evidence-backed` / `target-contract` / `unknown/not_tracked` 分类、主要 blocker、已保留/消费价值、`next_action`、`next_recheck` 和真实替代目标。缺实现或未知来源不能填成安全候选。

缺地点、授权或建材时，原 Gate 仍可给出受支持的准备动作；玩家不必先拥有材料才能查看合法取得材料的方案。这些动作按自身报价、资格、当前可执行入口和独立结果作决定，完成后重新评估整链；准备动作可用不把原 Gate 自动改成 `candidate_available`。若只证明建设可达，明确该建设目标及生产缺口，不承诺首产物可达。无法证明具体准备或替代动作时，保留缺口与安全停止，不把未来市场、租赁或代建名称当成按钮。

Gate 和准备方案预览都只读，不产生 reservation、权利、资金 hold、材料移动或建设效果。原共享评估者仍只模拟当前 profile 规定的有限建厂/生产路径，不因本设计暗加采购、tenure 或外部服务调用。取得前置依真实已结算事实进入后续快照；每次实际提交仍须 fresh revalidation。

一个发布普通账号首厂能力的世界至少声明：适用主体、默认地点/材料来源、目标 factory、权利和许可边界、报价/预留/过期语义、无安全路径时的替代目标，以及运维 bootstrap 与普通玩家动作的隔离。Profile 是能力声明，不是每个账号的资产注入脚本。

<a id="first-factory-authority-model"></a>
## 5. 权威对象与玩家语言

| 专业事实 | 玩家首读表达 | 玩家可以依赖 | 不能由它推导 |
| --- | --- | --- | --- |
| Agent / Context | “你的 Agent 可以在这些范围内代办” | 谁能请求和代办哪些动作 | 地点、许可、材料或免费供给 |
| Site offer | “可申请这个地点，费用和条件为……” | 当前报价及其条件 | 已取得使用权、已预留地点或允许建设 |
| 有效地点使用依据；适用时为 tenure | “你可以在这里按这些用途和期限使用” | 已成立的地点、用途和时间窗口 | 当前建设许可、材料或已建成 |
| Construction authorization | “当前允许建设这类设施” | 当前主体、地点和用途可建 | 长期地点权、材料或完成结果 |
| Supply / material fact | “建材已到账 / 已为本次建设保留” | 来源、数量、成本、在途/到账/预留 | 地点/许可有效或建设必成 |
| Capital / hold | “这些资金可用，其中一部分将暂时占用” | 可支付余额和当前占用 | 地点或材料所有权 |
| Build disposition | “已接受 / 正在处理 / 被拒绝 / 尚未确认” | 原请求当前权威状态 | 首产物或稳定能力 |
| Factory asset | “这座工厂已归属并位于这里” | 工厂身份、归属、位置和状态 | 已完成生产 cycle |

专业实现可以保留 tenure、permit、ledger、reservation、escrow、receipt 等准确名称，但首局默认入口必须先翻译成玩家问题。

组合方案可以由一次玩家确认触发多个 authority，以下不变量仍成立：

- Context 不生成地点、许可或材料；
- 所选世界/profile 要求的地点使用依据和建设许可分别成立；
- 材料必须由本次建设的合法消费账本提供；代保管或绑定原请求的预留仅在专业建设合同明确支持时适用；
- Build request 引用同一主体、地点、许可、材料和当前版本；
- Factory asset 与 build evidence 的 owner、location 和 type 一致；
- 冲突或缺失保持 blocked/unknown，不由客户端猜测补齐。

<a id="first-factory-supply-boundary"></a>
## 6. 地点、建材与 bootstrap 边界

### 6.1 默认 starter 方案

推荐默认路径是受治理、数量和用途有限、具有真实费用/配额的公共 starter 方案。它可以组合有限期地点使用权、建设许可和 build-kit，减少新玩家在多个系统之间切换；系统仍保留各自 authority、有效期和结算证据。

首厂需要所选世界/profile 声明的合法地点使用依据和当前建设许可，不要求永久土地所有权，也不把完整长期 tenure 生命周期自动加成所有已有合法 starter 的前置。现有 site/location authority 只在其原适用范围内使用，不能冒充长期 tenure。公共方案若采用 tenure，取得、用途/期限、维护及退出按 [GR-007](governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007) 和其专业合同成立，期限应覆盖方案所承诺的建设及使用窗口；当前建设许可不替代该权利，该权利也不替代当前许可。

这条目标路径的接点按下表核对；它们是评审和后台执行节点，不要求玩家逐项操作。未发布的正式能力标为待实现，不能由页面组合补齐。

| 节点 | 入口、资格和权威来源 | 实际成立的证据与失败后动作 |
| --- | --- | --- |
| 读取当前处境 | 当前账号的有效 Agent/Context、原世界与可读的地点/材料报价；沿原 first-claim 来源读取 | 注册、OC 领取、资源观察分开；缺来源或身份时查询/恢复，不借用其他主体库存 |
| 取得可用地点与建设许可 | 当前世界发布的地点 offer 与正式申请/授权入口；核原主体、资格、用途、期限和空间冲突 | 原使用权/许可结果均可核对后才进入建设；拒绝或过期则保留已成立事实，比较其他真实地点或停止 |
| 取得首批建材 | 当前公开供给的正式购买/领取合同及真实库存；材料种类和数量沿[工厂内容与原建造 plan](../../game/gameplay/industrial-native-content-delivery.design.md#native-content-catalog) | 真实转移/领取结果及正确消费账本；在途、缺货、资金不足分别显示，未支持自制的建材不能建议自行生产 |
| 建设并接回生产 | 当前 BuildFactory 入口重验地点、权限、材料、费用与原请求 | build evidence 与 factory asset 一致只完成首厂；重新评估原整链 Gate，缺生产输入/电力时保留生产缺口 |

### 6.2 其他真实来源

默认路径不可达或玩家主动比较时，才展开：玩家/组织市场、迁移到其他区域、自行生产建材、代建合同和租用既有产能。租用既有产能可以进入真实生产链，但不能标为玩家自己的首厂建成。

### 6.3 公共供给与运维 bootstrap

公共 build-kit 必须来自真实库存，其来源可以是世界创生、区域资本采购、实际生产或受治理的运维初始化。普通玩家只消费公开 offer/contract；内部 `/bootstrap`、`/grant` 或等价能力不可见、不可调用，也不能作为玩家步骤或普通账号试玩证据。

入场的 `starter OC` / LLM 帮助、只服务 slot-1 claim/upkeep 的受限余额、世界可采资源、玩家可消费材料分别沿[原资源边界](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md#22-cold-start-resource-boundary)与[first claim 来源](../agents-world-simulation/prd.md#first-claim-generated-context)读取。只有合同允许的真实余额可支付该报价；OC 领取或矿点可见都不直接生成建材。公共库存若来自初始化，仍须有合法世界来源和普通玩家可消费的公开合同，不能用给指定账号追加物料代替该链。

报价中的 available、世界总库存和矿点剩余量都不是玩家已拥有材料。材料须实际到达正确消费账本；受权保管或原建设请求预留只有在当前专业合同允许建设消费时才满足该前置。在途和未结算采购保留其状态，不作为当前可用投入。

### 6.4 Fresh feasibility 与建设

不可逆提交前重新核对：actor/Context、适用地点使用依据、建设许可、空间冲突、建材、资本/费用、profile 的其他前置、世界时钟和原 request identity。专业结果至少区分：

- precondition rejected；
- accepted / unstarted；
- active / pending；
- completed with build evidence + factory asset；
- result unknown；
- failed with effects。

玩家不需要记住全部内部状态名，但必须读懂当前含义、已发生价值和下一步。首厂完成后只把真实 factory identity、控制/权利、位置和 operational state 交给既有生产链，不自动发放生产输入、电力或首产物里程碑。

<a id="first-factory-state-and-recovery"></a>
## 7. 状态、阻塞与原请求恢复

### 7.1 玩家级状态

入口使用少量玩家级状态组织体验：

- **可以决定**：方案和主要承诺可读；
- **正在处理**：原请求已接受或执行中，但未宣称完成；
- **需要处理阻塞**：明确主要原因和当前动作；
- **结果尚未确认**：可能已生效，沿原请求查询/恢复；不能用同一投入或相冲突的新请求绕过未知；
- **建设完成**：build evidence 与 factory asset 一致；
- **改用替代目标**：进入既有产能、代建等真实路径，但自己的首厂未完成。

内部实现可以有更多阶段，不能要求玩家理解完整状态机才能继续。

### 7.2 阻塞与恢复

| blocker | 首读说明 | 当前动作 | 禁止行为 |
| --- | --- | --- | --- |
| 无安全地点 | 当前区域没有可用方案及 next recheck | 等待、迁移、市场或既有产能 | 免费生成坐标 |
| 资格/许可不足 | 缺少哪项条件及已保留事实 | 满足资格、换方案、申诉或停止 | 把 Context/付款当授权 |
| 建材不足 | 已到账、在途、预留和缺口 | 补购、自制、等待、代建或改目标 | 用 OC/世界总量补零 |
| 报价陈旧 | 哪项事实改变、旧承诺如何处理 | requote 或停止 | 静默换地点/价格/期限 |
| Authority unknown | 原请求和最后可信事实 | query/reconcile 或安全等待 | 新 identity 盲重试 |
| 部分效果 | 已消费、占用、可追回、损失分别说明 | 释放、补偿或新计划 | 宣称完全回滚 |
| 实现未接线 | 当前能力不可用 | 转向已实现目标并记录交付缺口 | 调用运维接口掩盖 |

恢复建议不保证成功，但必须来自当前 action contract，并说明会保留什么、承担什么新成本、何时再次核对。没有安全恢复动作时返回重新定目标，不无限等待或重复同一失败调用。

### 7.3 幂等和未知结果

同一 request identity + 同一规范 payload 返回原 disposition；同一 identity 不得换 payload。断连、重放和回访恢复原历史，不把旧成功重播成新事件。取消只作用于合同允许且尚未结算的范围；结果未知时禁止默认生成新 identity 重提。

缺少 factory 投影不足以证明 BuildFactory 未生效；支付被 hold 也不足以证明建设已经接受。只有权威确认无效果，或专业合同明确允许安全重试时，才开放重新提交。

## 8. 入口信息层级与 Agent 能动性

### 8.1 三层信息

| 层级 | 默认内容 | 使用时机 |
| --- | --- | --- |
| 首读层 | 目标、主要方案、总成本/占用、影响本次承诺的用途/期限与损失风险、一个 primary blocker、主动作、结果和下一步 | 每次当前决定 |
| 展开层 | 原政策条款、材料来源与费用计算明细、其他真实方案的比较 | 玩家主动查看；不隐藏改变本次决定的事实 |
| 证据层 | authority、policy/version、ledger/reservation、request identity、receipt/asset refs | 审计、恢复、高级玩家和 QA |

任何会改变权利、不可逆损失、资源占用或恢复可用性的事实都必须从展开/证据层提升到首读；其余内部细节不能挤占首读层。

Viewer、pure API 和适用 Agent 可以采用不同呈现，但必须对目标、承诺、主要 blocker、结果、允许动作、复查条件和 evidence refs 保持同义。没有专业 command 的按钮只能查看/比较，不能伪装成可提交。

### 8.2 Agent 主体感

首厂样本至少能追踪：

`玩家目标/承诺 -> Agent 或协调器代办 -> 权威校验 -> 建设结果/阻塞 -> 玩家下一决定`

验证玩家能动性时，需要指出哪项玩家目标或约束改变了后续行为；Agent 活动量、解释篇幅、确认次数或最终建设成功都不能单独证明控制感。原确认之外的新成本、权利承诺、不可逆效果或越界变化必须回到玩家决定；原范围内的合法代办按 §3.3 执行。

<a id="first-factory-validation"></a>
## 9. 验证与证据边界

### 9.1 最小独立游玩观察

按主 PRD 的[最小独立游玩](first-session-and-continuation.prd.md#minimum-independent-play)及原真人方法，不给按钮路径和专业术语答案，只给真实目标与必要情境。完成当前决定后，观察玩家能否自行说明并继续：

- 当前目标是什么；
- 哪部分由自己决定，哪部分由 Agent 代办；
- 主要承诺或成本是什么；
- 系统实际接受/发生了什么，或为什么被阻塞；
- 下一步为什么是继续、恢复、改道、等待或停止。

术语背诵不计通过；工作人员代操作不计自主完成。真实 blocker 下，仍须取得上述五维观察及一个与当前事实相符的自主决定，才能按主 PRD 判定学习；只会复述“不可用、退出”不足以证明全部能力。首厂未完成和学习各维未观察到的部分分别保留，工厂碰巧建成也不能补齐缺失的理解证据。

### 9.2 分层证据

| 需要证明 | 证据 | 不可替代边界 |
| --- | --- | --- |
| 世界执行正确 | rights/supply/build/asset 的专业测试和真实候选结果 | 玩家自述不代签世界事实 |
| 信息可读 | formal 入口任务完成、错误路径和提示介入记录 | 字段存在、截图或作者走查不代签 |
| 玩家理解 | 无逐步指导的解释与下一决定；单约束变化可迁移 | 完成脚本或术语问答不代签 |
| Agent 主体感 | 玩家目标/约束与实际行为/结果精确关联 | Agent 活动量和长解释不代签 |
| 继续动机 | 无额外催促的下一目标、自然离场和实际回访 | 理解、继续点击或预约第二场不代签 |

### 9.3 代表性场景

| 场景 | 核心判定 | 层级 |
| --- | --- | --- |
| 普通账号正常首厂 | 一个主要方案、原承诺内代办，无特权路径、build/asset 成立；整链推荐另外满足原 Gate | `test_tier_required` + actual play |
| 无安全路径 | blocker、保留价值、next recheck 和替代目标可理解 | `test_tier_required` + actual play |
| 资格/许可不足 | 不产生假成功，玩家知道怎样修复或停止 | `test_tier_required` |
| 建材不足/供应耗尽 | 资本、世界资源、在途和可支配材料不混同 | `required`；争用 `full` |
| Quote/policy drift | requote，无旧条件不可逆提交；只有原承诺实质变化才重新确认 | `test_tier_required` |
| 仅建设可达 | 建材足够但生产输入/电力不足时，整链仍为 `no_safe_starter_chain`，可用准备动作和生产缺口分别表达 | `test_tier_required` |
| 断连/结果未知 | 原 identity 恢复，不盲重试 | `test_tier_full` |
| Build vs production | 建成与首产物反馈/receipt 分开 | `test_tier_required` |
| 多入口 | Viewer/API/Agent 同目标、承诺、blocker、结果和下一步 | `test_tier_full` + entry evidence |
| 首局理解 | 不靠步骤提示完成解释和下一决定 | actual human play |

文档 checker、合成 fixture、运维 bootstrap、局部自动化或 prepared-world 成功只能证明其实际范围，不能证明普通账号首局、玩家理解、继续意愿或发行就绪。

## 10. 实施拆分与未决边界

后续实施按独立结果拆分：

1. 原整链 Gate 的普通账号缺口、准备动作与玩家级 read model；
2. 地点/许可的正式 offer、issuance、过期与查询；
3. 真实 build-kit 供应、到账/预留和 bootstrap 隔离；
4. fresh feasibility、BuildFactory、build evidence、factory asset 与原请求恢复；
5. Viewer、pure API、Agent 的首读/展开/证据层；
6. 普通账号端到端和真人最小独立游玩验收。

本普通账号组合的实现与玩家验证尚未核实；已有 starter 局部实现与历史样本仍按原专业来源理解。公共 starter 方案由何种世界主体发布、地点权利/许可是否同一协调事务、build-kit 与 BuildFactory 是否原子预留、B1 世界的数值/配额/补货政策，均由后续专业 owner 决定；不能削弱首局知识预算、真实权利/材料、无特权路径、幂等和无假成功要求。

相邻 authority：[`first claim / generated context`](../agents-world-simulation/prd.md#first-claim-generated-context)拥有入场和 Context；[`REQ-WR-GR-007`](governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007)及区域治理专业设计拥有地点权利；[`工业原生内容交付设计`](../../game/gameplay/industrial-native-content-delivery.design.md)拥有工业内容和 bootstrap 专业边界；配对首局 PRD 与其主设计拥有首产物、持续能力和回访；runtime、账本、空间/建设与持久化 authority 拥有事务、receipt 和 replay；testing/QA 拥有实际方法和 verdict。

## PRD REQ/AC fragment mapping

本文继承配对 PRD 的要求和验收，不建立第二套 requirement authority。下表列出本情境对原 REQ/AC 的承接；完整首局、持续能力及其余情境仍由配对 PRD 与主设计覆盖。

| PRD requirement | Design coverage | PRD acceptance |
| --- | --- | --- |
| [REQ-FIRST-INDUSTRIAL-001](first-session-and-continuation.prd.md#req-first-industrial-001) Trace refs: O7-REQ-68214dc7-c00b-4c0d-8c3b-c1c24fb52502 | [体验壳](#first-factory-experience-shell)、[能力 Gate](#first-factory-profile-gate)、§3 | [AC-FIRST-INDUSTRIAL-001](first-session-and-continuation.prd.md#ac-first-industrial-001) / [AC-FIRST-INDUSTRIAL-002](first-session-and-continuation.prd.md#ac-first-industrial-002) |
| [REQ-FIRST-INDUSTRIAL-002](first-session-and-continuation.prd.md#req-first-industrial-002) Trace refs: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95 | §3、§8、[验证](#first-factory-validation) | [AC-FIRST-INDUSTRIAL-001](first-session-and-continuation.prd.md#ac-first-industrial-001) |
| [REQ-FIRST-INDUSTRIAL-003](first-session-and-continuation.prd.md#req-first-industrial-003) Trace refs: O7-REQ-d4944a59-ccf3-4f96-95ac-45c0ca50cf51 | [供应边界](#first-factory-supply-boundary)、§6.3 | [AC-FIRST-INDUSTRIAL-001](first-session-and-continuation.prd.md#ac-first-industrial-001) / [AC-FIRST-INDUSTRIAL-002](first-session-and-continuation.prd.md#ac-first-industrial-002) |
| [REQ-FIRST-INDUSTRIAL-004](first-session-and-continuation.prd.md#req-first-industrial-004) Trace refs: O7-REQ-81c0bc21-36ef-400d-9398-31859b9a08b2 | §3.4、§6.4、§9.3 | [AC-FIRST-INDUSTRIAL-001](first-session-and-continuation.prd.md#ac-first-industrial-001) / [AC-FIRST-INDUSTRIAL-005](first-session-and-continuation.prd.md#ac-first-industrial-005) |
| [REQ-FIRST-INDUSTRIAL-005](first-session-and-continuation.prd.md#req-first-industrial-005) Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | [状态与恢复](#first-factory-state-and-recovery)、§7.3 | [AC-FIRST-INDUSTRIAL-003](first-session-and-continuation.prd.md#ac-first-industrial-003) |
| [REQ-FIRST-INDUSTRIAL-006](first-session-and-continuation.prd.md#req-first-industrial-006) Trace refs: O7-REQ-d0041aa9-1dfb-46a0-95e0-67417b39bfa3 | §7.2、§9.1 | [AC-FIRST-INDUSTRIAL-002](first-session-and-continuation.prd.md#ac-first-industrial-002) |
| [REQ-FIRST-INDUSTRIAL-007](first-session-and-continuation.prd.md#req-first-industrial-007) Trace refs: O7-REQ-df0617cf-2ab6-40ca-934b-f050d6a66812 | §3.5、§8、§9.2 | [AC-FIRST-INDUSTRIAL-004](first-session-and-continuation.prd.md#ac-first-industrial-004) |
| [REQ-FIRST-INDUSTRIAL-008](first-session-and-continuation.prd.md#req-first-industrial-008) Trace refs: O7-REQ-757636b1-c9e9-4e7a-97d1-460de105daf3 | §3.5、§8、§9 | [AC-FIRST-INDUSTRIAL-004](first-session-and-continuation.prd.md#ac-first-industrial-004) |
| [REQ-FIRST-BRANCH-001](first-session-and-continuation.prd.md#req-first-branch-001) Trace refs: O7-REQ-4c622b94-22bd-4559-9595-6b134bdc00b1 | §2、§3.2、§6.2、§7.2 | [AC-FIRST-BRANCH-001](first-session-and-continuation.prd.md#ac-first-branch-001) |
| [REQ-FIRST-BRANCH-002](first-session-and-continuation.prd.md#req-first-branch-002) Trace refs: O7-REQ-2e82fef6-bbb5-43b6-a7d0-833905e8e8a3 | §3.5、§7、§9.1 | [AC-FIRST-BRANCH-002](first-session-and-continuation.prd.md#ac-first-branch-002) |

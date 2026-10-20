# 游戏可玩性设计总纲 v0.1设计

- 对应需求文档: `doc/game/gameplay/gameplay-top-level-design.prd.md`
- 产品承诺入口：[`首局与持续游玩`](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-001)、[`可玩性证据与承诺边界`](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-001)
- 可变执行状态: 对应 GitHub Project task 与 issue evidence comments

## 1. 设计定位
定义 Gameplay 专题设计，统一玩法循环、运行时/治理边界、玩家反馈与发布验收口径。

## 2. 设计结构
- 玩法循环层：明确微循环、中循环、长循环或发布闭环中的关键行为链。
- 运行接线层：把玩法规则与 runtime、wasm、治理、观测或 longrun 门禁对齐。
- 反馈验收层：定义玩家可感知反馈、平衡目标与质量门禁。
- 演进收口层：将生产化、发布差距或模块化切片纳入可追踪计划。

### 2.1 需求承接与分配表

以下只分配可精确承接的局部义务，未覆盖的原四层职责见 §8；不以有效链接代替完整设计。

| 上游 requirement / product AC（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-FIRST-INDUSTRIAL-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-002) | 当前 production_only 的工厂、合法输入/电力、排程与 owner-bound 正量铁锭结算分开；结算后开放 Assembler 候选。 | [首产物完成边界](#first-product-boundary) | gameplay/M4 定义适用方案，Runtime 拥有结算，Viewer 投影，QA 验证。 | 不代签稳定窗口、Assembler 建成、terminal delivery 或全部工业路径。 |
| [REQ-FIRST-INDUSTRIAL-008](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-008) | 一次成果连接真实下一目标及离场/重入，保留原成果、未决义务和适用授权；一次产出与持续能力分开。 | [成果承接与自然离场](#continuation-boundary) | gameplay 拥有后继条件，Runtime 拥有结果与未决状态，Viewer/QA 验证续玩入口。 | 不由候选可见推出持续能力、后续两 beat、离线收益或真人回访。 |
| [REQ-FIRST-BRANCH-001](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-branch-001) | 分支收益、风险/锁定、两个后续 beat 与回访第一动作按既有专业合同解释；当前阶段与真实合法动作保持同义。 | [分支解释与阶段](#branch-explanation) | gameplay 定义分支语义，Runtime 发布合法能力，Viewer 投影，QA 核对。 | 不新开路线或强制站队，不以阶段文案证明路线可达或两 beat 已实现。 |
| [REQ-FIRST-BRANCH-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-branch-002) | 适用时回退期限、成本与保留/损失收益同义；不可回退或无安全路径必须显式说明。 | [回退窗口边界](#rollback-window) | gameplay 提供适用阶段与窗口，Viewer 消费，QA 检查；真实回退能力由 Runtime 专业域决定。 | 现有 attraction fixture 窗口不证明真实 Runtime 回退；不新增回退权限或费用。 |
| [REQ-WR-PE-002](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-002) | 本专题评审将结构回归、live/provider、L4A/L4B 和 L5 的证明范围分开；低层不能代签真人继续意愿。 | [总纲评审证据分层](#evidence-layer-boundary) | testing 拥有方法与证据规则，QA 守边界，producer 作适用产品建议；gameplay 消费。 | 不接管其他 claim 消费者，不以本次局部条款宣称整项 PE-002 覆盖或发布就绪。 |
| [REQ-WR-PE-003](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-003) | 本专题建议固定体验目标、formal 入口、候选或明确变更范围、证据窗口；环境影响解释时纳入范围。 | [总纲评审比较单位](#comparison-unit-boundary) | producer 定义 claim envelope，QA 核对证据，testing 持方法；公开状态仍归发行与根 README。 | 不新增产物 schema、统一 verdict 或替代全部消费者核对。 |

## 3. 关键接口 / 入口
- gameplay 规则/模块入口
- runtime / wasm / governance / viewer 接线点
- 玩法反馈与平衡指标
- gameplay 回归与发布门禁

## 4. 约束与边界
- 玩法设计需与运行时边界、治理规则和测试口径保持一致。
- 生产化收口不得牺牲核心可玩性与反馈清晰度。
- 不在单个专题中扩张到全量玩法体系重写。

## 5. 设计演进计划
- 先冻结该专题的玩法闭环与关键指标。
- 再补 runtime/治理/观测接线与验证口径。
- 最后以发布或长期回归为门禁完成收口。

## 6. 问题、目标与适用范围

本专题保留 §1–§5 的玩法循环、运行接线、反馈验收和演进收口全部义务。本次只把已有合同中可准确定位的首产物/承接、分支及本专题评审消费者职责分项接收；这是有限专业设计分配与验证计划，不是全路径设计闭合或当前运行通过。

职责沿用[配对 PRD 的 authority](gameplay-top-level-design.prd.md#文档-authority-与适用范围)：跨循环体验脊柱、PRD-GAME-012 early-retention 和本专题可玩性验收。game 模块根仍拥有模块基线、路由与状态；M4、Runtime/WASM、agency、Viewer、testing、P2P/ops 和发行域分别拥有其详细合同。本文件不重新选择数值、循环、模式、权限、状态字段、schema 或模块级 authority。

审读合同基线为 `6542494837db9d8fb65689491276b82b0bdc7d02`。当前实现、执行结果和发布判断仍回链各自候选与实际验证记录；此处没有新的业务运行结果。

## 7. 局部承接的主责与边界

<a id="first-product-boundary"></a>
### 7.1 首产物完成边界

Trace refs: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95；声明：[REQ-FIRST-INDUSTRIAL-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-002)；仅承接本条具体范围。

当前切线沿用[专业 PRD](gameplay-top-level-design.prd.md#当前交付切线)：`starter-industrial-smelter-to-assembler-v1` / `production_only`，合法地点的 `factory.smelter.mk1` 以 `recipe.smelter.iron_ingot` 形成匹配 owner-bound 正量铁锭结算与持久首产物里程碑，再开放 Assembler 候选。排程、资源扣减、入列和普通库存变化不构成结算证据；保持 produced/undelivered，交付只在适用 profile 中评价。专业执行与交互分别由 [M4](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding) 和 [Viewer](../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.design.md#starter-plan-interaction) 承接；本条只维护总纲完成边界，不补算成本或声明整条工业链通过。

<a id="continuation-boundary"></a>
### 7.2 成果承接与自然离场

Trace refs: O7-REQ-757636b1-c9e9-4e7a-97d1-460de105daf3；声明：[REQ-FIRST-INDUSTRIAL-008](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-008)；仅承接本条具体范围。

首产物、10 分钟信任窗口、15–45 分钟或 1–3 次会话持续能力窗口、单次会话与长期成长分别命名。沿用专业 PRD 当前交付切线及 early-retention 合同，不要求凑足时间。成果阅读、真实下一目标、自然离场与重入由 [Viewer 当前方案交互](../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.design.md#starter-plan-interaction) 消费同一权威结果；候选不等于建成，已结算成果不误归新的意图，未决义务和失效授权不得被重入抹去。持续能力、路线后续 beat 与真人回访必须另有各自窗口证据。

<a id="branch-explanation"></a>
### 7.3 分支解释与阶段

Trace refs: O7-REQ-4c622b94-22bd-4559-9595-6b134bdc00b1；声明：[REQ-FIRST-BRANCH-001](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-branch-001)；仅承接本条具体范围。

沿用配对 PRD 首次分支承诺与 PRD-GAME-012 合同：扩张、稳定/恢复、专业化/服务是可比较的选择类别，不是固定职业或强制站队。真实支持的路线说明即时收益、两个后续 beat 的实质变化、风险/锁定和回访第一动作；没有真实差异不能用标签冒充实质取舍。当前阶段及合法能力投影由 [Viewer 分支投影](../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.design.md#branch-stage-projection) 承接；本条不新开路线、决定数值或声称所有路线已实现。

<a id="rollback-window"></a>
### 7.4 回退窗口边界

Trace refs: O7-REQ-2e82fef6-bbb5-43b6-a7d0-833905e8e8a3；声明：[REQ-FIRST-BRANCH-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-branch-002)；仅承接本条具体范围。

回退可用性、期限、成本及保留/损失收益沿用现有 branch 产品与专业合同，不可回退也要可读。现有 [Viewer 样例回退窗口](../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.design.md#fixture-rollback-window) 只拥有 attraction scenario 的 fixture route phase 判断，不能作为真实 Runtime 回退机制。截止时和恢复后的信息必须保持同一窗口语义；缺 phase 或冲突不能恢复可用性。真实能力未证明时保留缺口，不从样例推导权限。

<a id="evidence-layer-boundary"></a>
### 7.5 总纲评审证据分层

Trace refs: O7-REQ-1a903b9b-03df-4ae2-86f2-2a767affc9c1；声明：[REQ-WR-PE-002](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-002)；仅承接本条具体范围。

总纲评审沿用专业 PRD §2.7.1/§8 和 testing 权威：trust/capability、progression/attraction/motivation、内容量诊断及真人意愿分别判定。required 的结构与回归不代签正式入口或 provider；live 也不自动代签 L5。结构代理 meaningful_decision_count、默认 hook/replay 分数及历史 34/30、22/18 样本不成为真人选择或回访意愿。内容量诊断不是通用最低通关量。QA/producer 在当前目标范围内记录证明与未证明项；本条只约束总纲评审消费者，其他消费者与全部路径尚未核清。

<a id="comparison-unit-boundary"></a>
### 7.6 总纲评审比较单位

Trace refs: O7-REQ-bacdddac-0615-4447-9242-0145c1cc4e07；声明：[REQ-WR-PE-003](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-003)；仅承接本条具体范围。

总纲评审建议只在同一体验目标、formal 玩家入口、候选/明确变更范围和证据窗口内比较；环境影响可进入性、执行或结论解释时，环境属于明确范围。Viewer 与 pure_api 不互相代签，历史或不同环境正向信号不能抵消当前阻断。范围不明按上游产品 AC 保持 watch，若正被用于更强 claim 则 hold；producer 的建议不升级根 README/发行 authority 的公开状态。本条只接收本专题评审职责，不新增证据 schema，也未核清全部 go/watch/hold/block 消费者。

<a id="balance-review-method"></a>
### 7.7 配方与能力的数值审阅方法

Trace refs: O7-REQ-4c622b94-22bd-4559-9595-6b134bdc00b1; O7-REQ-bacdddac-0615-4447-9242-0145c1cc4e07；分别承接[分支真实取舍](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-branch-001)和[比较单位](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-003)。这是验证方法，不增设世界公式、配方参数或调参准入协议。

阅读导航：§7.7.3–6核有效成本、批量和分账初态；§7.7.7–10比较具体候选与材料取舍；§7.7.11–17核输出、物流、维护、能源和回本；§7.7.18–22研究恢复/供给与实验优先级。各节的固定来源事实、条件算例和未采纳候选分别有效，不跨profile拼接。

先按当前专业 authority 取得配方、工厂、批次和实际执行规则，再填分析样例；不能在 PRD 维护第二份常量表。目录 `base_duration`、槽位和标称耗电不单独证明有效工期、并行产能或每 tick 实际扣费。批量材料线性也不证明时间/占槽线性；须追踪授权、排程、容量和真实结算。当前只读研究不足以判定大批最优或无限批漏洞。

| 分析量 | 口径与前提 | 防误读 |
| --- | --- | --- |
| 可用库存 | 同一主体/消费账本的可消费量，依据权威余额与已计入的预留/待决占用 | 不使用世界总量；已扣除的占用不重复扣；unknown不填零 |
| 理论生产速率 | 在完整配方、有效cycle时长和可并行槽数已知时，以产物单位/世界tick表达；时长未知或非正不计算 | 输入、加工、运输、能源与输出容量须换成同一目标产物速率后才能取最小上界；只适用于稳定供给、无争抢/停机且路径确定等声明条件，不是交付保证 |
| 实际吞吐 | 观察窗口内匹配结算量/实际推进tick，另列启动、稳定、阻塞、恢复区间 | 库存增长不是吞吐，生产结算不是交付；实际推进 tick 为零时吞吐未定义、不计算，不报零或无穷 |
| 储备覆盖 | 可用库存与承诺需求在同一单位/窗口下计算；分别查看正常净流出与供应全停时的消耗 | 净流入为正不等于永远安全；所选消费窗口内确认零需求时为不适用，需求未识别或未知时仍为未知；不能把库存、速率和覆盖tick相加 |
| 方案机会成本 | 同起点、目标、权限和窗口，对照真实材料、电力、占槽、时机、风险及后续两次决定 | 不把不同目标/初态或额外免费资源的方案当等成本；不只比较单周期产出 |

候选数值研究一次改变一个主约束，再作输入供给、能源、输出空间与并发占用的边界组合；记录偏好是否随条件改变，是否存在有意义但非等收益的多条策略。若一条方案在适用范围内始终更优，先核对是否有真实场景支持另一方案，不靠隐藏惩罚、虚构限时或强制事故制造平衡。未执行的算例只叫分析假设；是否好玩仍需对应玩家证据。

#### 7.7.1 固定配方的手算样例：首产物之后还缺什么

以下只解释固定版本 `9df20824f3f0418eef7bd732782bf99205ab7b5d` 的[配方目录](https://github.com/nil-studio/oasis7/blob/9df20824f3f0418eef7bd732782bf99205ab7b5d/crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs#L31-L63)与[工厂目录](https://github.com/nil-studio/oasis7/blob/9df20824f3f0418eef7bd732782bf99205ab7b5d/crates/oasis7_agent_api/src/world_quote/factory_catalog.rs#L22-L36)，不是另一份当前配置或运行验收。目录变化后重新取值；提交仍以当前正式报价和全部前置为准。

该版本每批铁锭配方消费 4 单位铁矿、1 单位碳燃料、8 单位配方执行电力，生产 3 单位铁锭和 1 单位 slag。Assembler 的材料建设成本为 10 单位铁锭、8 单位铜线、8 单位 structural_frame。

- 若全部铁锭只由这一配方取得、没有损耗/另作他用，材料上最少需要 4 个完整批次才能覆盖 10 单位：累计得到 12 铁锭、4 slag，消费 16 铁矿、4 碳燃料、32 单位**配方执行电力小计**。
- 第一批完成后的 3 铁锭已是首产物成果，但离 10 铁锭仍差 7；后续至少再生产 3 个完整批次，另需 12 铁矿、3 碳燃料、24 单位配方执行电力。不能因候选已开放而显示“立即建设”。
- 上述电力不含建设、基础运行、维护、运输等实际适用成本；也不证明可以连续排入、3 ticks 完成、稳定生产或足够电力恢复。整数批数只用于材料算术，不能推导时间或吞吐。

再以**假设已经合法可用、无预留/未知占用**的 12 铁锭、8 铜线、8 框架作比较，且分别位于对应动作真正可消费的账本；这是分析初态，不发放启动资源：

| 材料去向候选 | 一次行动的材料结果 | 下一步真正要判断 |
| --- | --- | --- |
| 完成并结算合法 Assembler 建设（分析假设） | 仅按建设材料扣除，余额为 2 铁锭、0 铜线、0 框架；建设电力和其他前置另核 | 建成不等于开工；选择其首个合法配方、补齐电力/材料并协调原Smelter供给。未通过地点/权限等前置时仍不可建 |
| 先在合法 Smelter 做 1 批合金板 | 消耗 2 铁锭、2 铜线及9单位配方执行电力，得到2合金板和1slag；余额10铁锭、6铜线、8框架 | 若仍想建设，至少缺2铜线；先决定合金是否有真实当前用途，再决定补铜/延期建设。没有可达用途时不凭“高级材料”推荐 |

这说明共享铜线会改变后继准备，不证明两条路线等价、长期互斥或哪条更好。需再核对合金用途、实际配方可达、供铜成本、槽位/时间与两次后继决定。当前数值先保留：只有在同目标同初态的验证中发现支配策略、不可恢复断链或无意义重复，才提出具体调参候选及反例，不把算术正确写成平衡或乐趣已通过。

### 7.7.2 现有配方的依赖，而非十一项强制任务

同一固定版本的 [recipe catalog](https://github.com/nil-studio/oasis7/blob/9df20824f3f0418eef7bd732782bf99205ab7b5d/crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs) 中，Smelter 的基础投入分别导向铁锭、铜线和树脂；铁锭与铜线又可合成合金板。Assembler 的依赖可按用途前沿解释，而不一次向新人罗列全部配方：

| 依赖组 | 必需材料关系（不代替目录数量） | 决策时检查 |
| --- | --- | --- |
| 基础零件 | gear 依赖铁锭；control_chip 依赖铜线与树脂 | 铜线可能同时服务建设、合金与芯片；依据真实下游用途分配，不以物品等级决定 |
| 电机与无人机 | motor_mk1 依赖gear与铜线；logistics_drone 同时依赖motor、control_chip与铁锭 | 两条上游必须会合；生产出名为drone的物品不证明已获得运输能力 |
| 传感与模块 | sensor_pack 依赖control_chip与铜线；module_rack 依赖sensor_pack与control_chip | 共享芯片/铜线形成争用，不能把缺芯片误诊成多建一台设备即可解决 |
| 核心部件 | factory_core 依赖module_rack与alloy_plate | 这是材料依赖，不证明core已可启用工厂、解锁区域或承担某项真实需求 |

此图不定义科技树、解锁顺序或互斥职业，也不证明全部配方已在当前玩家入口可执行；准入、能力与实际用途分别核对。存在配方只说明怎样转换，不能推出逆向拆解、退款或产物自动消费。每到一个用途前沿，只展示当前目标需要的上游与真实存在的竞争用途；若不存在竞争，则明确仅一条可达用途或用途尚未接入，允许维持基础生产、履行已有承诺或不追求末端产物。十一项配方不等于十一项有意义选择：若下游没有已接入用途，先标明缺口而非靠继续囤积制造成长。

<a id="effective-numeric-review"></a>
### 7.7.3 有效成本：目录值不等于实际可执行投入

以下数值审读按配置分别取值，固定源码 `2d9e32263d3dbc22595ff33b194b1da59165cf1f`，与 §7.7.1 的历史目录算例分开。它是源码及静态算术，不是游戏运行结果、当前线上初态或新的默认系数表。现行值仍由 recipe/factory/product 与执行 authority 唯一决定；本节的候选均未采纳、未部署。

[bootstrap 产品维护定义](https://github.com/nil-studio/oasis7/blob/2d9e32263d3dbc22595ff33b194b1da59165cf1f/crates/oasis7_runtime/src/world/bootstrap_economy.rs#L423-L467)与[执行合并 helper](https://github.com/nil-studio/oasis7/blob/2d9e32263d3dbc22595ff33b194b1da59165cf1f/crates/oasis7_runtime/src/state/apply_domain_event_industry_helpers.rs#L121-L133)仅在已显式安装 M4 默认 product profile 且走该执行路径的世界，使下列额外材料按产物数量在启动消费中生效。[安装入口](https://github.com/nil-studio/oasis7/blob/2d9e32263d3dbc22595ff33b194b1da59165cf1f/crates/oasis7_runtime/src/world/bootstrap_economy.rs#L175-L180)不是 World::new 的默认行为；本地 starter 的空 product profile 初态不能与此表混拼。这不是时间流逝维护费，也不是完整上游 BOM；读取已合并的 effective quote 时不可再加一次。

| 每单位产物 | 目录之外的直接附加输入 |
| --- | --- |
| control_chip / motor_mk1 / sensor_pack | 各 1 hardware_part |
| logistics_drone / module_rack | 各 2 hardware_part |
| factory_core | 4 hardware_part + 1 alloy_plate |

该版本 core 配方本身需要 1 rack、3 alloy，合并后为 1 rack、4 alloy、4 hardware。假设这些输入在实际消费账本、归属/权限有效且无占用：有 rack1/alloy3/hardware3 仍各缺 1 alloy 与 1 hardware，不是“有目录原料就可开工”。上游芯片/传感器/rack 的 hardware 尚须另算。每批 alloy 产2，需求3→4的最少完整批数同为2，但余量1→0；批数不变不能证明维护免费。跨账本、未知占用或未结算产物不抵扣缺口。

### 7.7.4 条件 BOM 与规模断点

只在**已安装 M4 默认 product profile、上述 sink 生效**的分析世界，假设设备合法可用、无中间库存、无损耗且各级材料有合法到达消费账本的途径，按[同版本配方目录](https://github.com/nil-studio/oasis7/blob/2d9e32263d3dbc22595ff33b194b1da59165cf1f/crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs)逆拓扑展开并向上取整。表内均为生产1单位终产物所需的外部投入；0表示该展开不需该物料，不表示实际余额。hardware 是外部必需供应，未展开为免费矿物。

| 终产物1单位 | 铁矿 | 铜矿 | 燃料 | 硅矿 | hardware | 配方执行电小计 |
| --- | --- | --- | --- | --- | --- | --- |
| gear | 4 | 0 | 1 | 0 | 0 | 12 |
| control_chip | 0 | 3 | 2 | 2 | 1 | 19 |
| logistics_drone | 16 | 9 | 6 | 2 | 5 | 105 |
| module_rack | 0 | 12 | 6 | 6 | 7 | 89 |
| factory_core | 8 | 15 | 8 | 6 | 11 | 143 |

core 的11 hardware=3个chip×1+2个sensor×1+1个rack×2+core×4。core总配方电143不含设备建设、运输、来源获取或任何另行实际生效费用，不是总成本/真实最优排程/分钟数。副产物也保留：1 core同时产生slag4、waste_resin6、calibration_scrap2、precision_scrap1、structural_waste1，不能按废料价值抵扣电力。

| 一次联合规划 core 数 | 仅目录所需 alloy 批 | 含 sink 所需 alloy 批 | 仅目录/含sink配方电 | 含sink hardware |
| --- | --- | --- | --- | --- |
| 1 | 2 | 2 | 143 / 143 | 11 |
| 2 | 3 | 4 | 261 / 278 | 22 |
| 4 | 6 | 8 | 516 / 556 | 44 |

每种中间产物合并需求后再向上取整，余料留账而非销毁；不可将单core成本简单乘数量。例如2 core多出17电、4 core多出40电，证明单件余料掩盖的维护成本会在规模变化时显现。供应路径未接入、产品模块未安装或当前quote不同，整张表不能作为可提交报价。

### 7.7.5 同一批量的三个分母

[基础时长](https://github.com/nil-studio/oasis7/blob/2d9e32263d3dbc22595ff33b194b1da59165cf1f/crates/oasis7_agent_api/src/world_quote/duration.rs#L1-L33)与 [ScheduleRecipe admission](https://github.com/nil-studio/oasis7/blob/2d9e32263d3dbc22595ff33b194b1da59165cf1f/crates/oasis7_runtime/src/world/event_processing/action_to_event_economy.rs)要分开读：所审新提交路径绑定 factory.input_ledger，缺料拒绝；在已齐套的 site-only 路径，不走 world fallback 稀缺延迟，基础 duration 为1 tick。slots=2限制 pending jobs，不是每次最多2批。目录 throughput_bps=10000 与 base_power_draw=20 在已审 admission/start/complete 路径未成为额外吞吐限制/逐tick电扣款；不能据字段名补算20电/tick。其他模块、队列、profile和tick推进条件另核。

| 通过admission、完成验证另核的铁任务批数 b | 铁 / slag | 执行电 | 该路径基础 duration | 占pending job槽 |
| --- | --- | --- | --- | --- |
| 1 | 3 / 1 | 8 | 1 tick | 1 |
| 4 | 12 / 4 | 32 | 1 tick | 1 |
| 12 | 36 / 12 | 96 | 1 tick | 1 |

这只揭示大批在该路径的单位时间/任务槽优势；材料、电力与合法整数仍约束可提交量，不能以12是示例推定统一上限，不能把tick换成现实分钟或把两个槽保证为两倍持续吞吐。

未来**时长候选实验**只改变 D=ceil(b/K)：K=1、4、12时，b=12对应12、3、1 tick，单位铁的执行电仍为8/3。以同初态总12批比较一次12与三次4的总电、完工顺序、等待与槽占用；若改变时长只增加无决策等待，拒绝该候选。当前不推荐直接部署限批/改duration，先检验现有推荐批量能否保留真实后继选择；该公式不是新的runtime规则。

### 7.7.6 本地 starter 初态：不是从零冶炼建设材料

本表只读同版本[本地 host starter bootstrap](https://github.com/nil-studio/oasis7/blob/2d9e32263d3dbc22595ff33b194b1da59165cf1f/crates/oasis7/src/runtime_host/bootstrap.rs)所安装的初态；world_service、chain-linked 空世界及其他fixture不得套用。选中的本地seed路径仍须确认实际配置；它没有因此安装上一节的 M4 产品维护 profile。

| 独立账本/资源 | 已审初态 | 真实用途与限制 |
| --- | --- | --- |
| builder | frame20、heat4、brick6、iron10、wire8 | Smelter用frame12/heat4/brick6；Assembler用frame8/iron10/wire8 |
| smelter site | ore60、fuel20 | 铁配方输入；其产铁不是builder自动余额 |
| assembler site | iron8 | 只足4批gear；不是从刚才site铁免费搬入 |
| owner Electricity | 132 | 两工厂各10 start-only建设电；铁每批8、gear每批4 |

没有已实现site→builder桥的证据，首产铁不能同时冒作建设预算。[legacy gameplay动作](https://github.com/nil-studio/oasis7/blob/2d9e32263d3dbc22595ff33b194b1da59165cf1f/crates/oasis7/src/runtime_host/gameplay_actions.rs)默认铁12批、gear4批，纯API其他合法正批数不因此被禁止。两建设均完成时，静态余额 E=132−20−8b−4g。b12/g4得36铁、4gear、E0，smelter仍余12矿/8燃料，但下一正费排产不能靠有原料自动支付。

顺序反例：先建Smelter再做15铁批，得到45铁，剩132−10−120=2电，后继Assembler建设需10，缺8。首产里程碑仍成立，后继行动仍被资源条件阻断；它不是永久软锁或死亡证明。runtime Electricity、Agent battery及其他能源模块是不同系统，恢复须核真实报价与路径，不能免费补电。此算例不代签四个动作实际运行、冷启动入口或所有玩家初态。

### 7.7.7 优先试验推荐批量，而非先改产率

在上一节同初态、两建设和4gear均需完成的条件下，先保持全部运行系数不变，比较以下**文档实验候选**。批数只是建议，不改变授权、合法上限或结算；无真实铁用途时，不因追求大数推荐更多生产。

| 候选铁批 b | 铁成果 | 铁执行电 | 最终Electricity余额 | 相对默认牺牲 |
| --- | --- | --- | --- | --- |
| 4：小额探索 | 12 | 32 | 64 | 少24铁，保留较多下一决定预算 |
| 8：宽余量 | 24 | 64 | 32 | 少12铁 |
| 10：建议首测 | 30 | 80 | 16 | 少6铁 |
| 12：当前legacy默认对照 | 36 | 96 | 0 | 无余量，不自动判为不安全/死亡 |

建议先测b10对照b12：少6铁换16电，能够检验玩家是否看懂“首产不是电力用尽才完成”，且不改变任何配方产率。16电仅在原料、账本、权限和槽位允许时可支付2个额外铁批，不代表2次维修或保证恢复；完成4gear后该site铁已耗尽，不能说仍可做4gear。b8是恢复报价高于16时的备选，仍须真实报价才能选择；没有报价不贴“安全档”。

对要求保留R电的候选，b最大值=floor((96−R)/8)，另受矿/燃料等约束。R=0/4/8/12/16/24/32分别给b=12/11/11/10/10/9/8，实际留0/8/8/16/16/24/32：保留目标跨8电断点，不能线性插值。首轮比较固定初态、真实用途及相同可见信息；只要低批量导致关键已承诺用途无法完成，或高批量仍能合法便宜补电，推荐可反转。尚无运行、玩家偏好或最优策略结论。

### 7.7.8 改产率、建设门槛与电价不是等价调节

以下是另一个**反事实分析初态**：Smelter已合法可用，中间铁/铜库存为零，建设所需8框架另有合法来源，所有材料可合法送达建设账本。目标仅为凑齐Assembler材料，不是本地starter既已预置iron10/wire8的关键路径。以 §7.7.1 同配方独立改变一个参数，未修改实际配置。

| 单因素候选 | 铁产率/批 | 建设需铁 | 铁电/批 | 最少铁批 / 矿 / 燃料 | 铁电 / 加2铜批后的执行电 | 铁余量 |
| --- | --- | --- | --- | --- | --- | --- |
| 保持现值 | 3 | 10 | 8 | 4 / 16 / 4 | 32 / 44 | 2 |
| H1：产率4 | 4 | 10 | 8 | 3 / 12 / 3 | 24 / 36 | 2 |
| H2：建设需8铁 | 3 | 8 | 8 | 3 / 12 / 3 | 24 / 36 | 1 |
| H3：铁电7 | 3 | 10 | 7 | 4 / 16 / 4 | 28 / 40 | 2 |

两铜批另需6铜矿，产8铜线、执行电12；表中未含建造电、获取/运输/其他实际费用。H1和H2虽都省一铁批，但H1同时提高每次首产及所有下游铁供给，H2只降低建设门槛；H3仅降能耗，最少铁批数不变；操作/提交次数仍由入口支持决定。不能把三者叠加后仍归因单因素。

当前建议保持系数，先做 §7.7.7 推荐批量对照。只有真实玩家仍因无意义重复而放弃、且后继用途/补给已可达，才优先检验局部H2而非全链H1；若问题实为恢复能耗而非批次负担，再单独检验H3。接受条件不是“数值变大”，而是同目标达成后仍有可解释代价和可达下一选择；没有用途时调低成本不能填平玩法缺口。

### 7.7.9 首产后缺一单位，不等于上一成果失败

沿用 §7.7.8 的条件初态，但已有3铁已结算且仍可用于目标，铜线8及框架8另行可用。下表仅给额外铁批的资源上界 B=min(floor(矿/4),燃料,floor(执行电/8))，不承诺槽位、时间、输出空间、合法运输或提交通过。每行只改一个约束。

| 余矿 / 燃料 / 执行电 | B | 完成后总铁上界 | 距建设10铁的缺口 | 主瓶颈 |
| --- | --- | --- | --- | --- |
| 12 / 3 / 24 | 3 | 12 | 0 | 三约束同时紧 |
| 11 / 3 / 24 | 2 | 9 | 1 | 矿差1却少一完整批 |
| 12 / 2 / 24 | 2 | 9 | 1 | 燃料差1 |
| 12 / 3 / 23 | 2 | 9 | 1 | 电差1 |

这是输入完整批的台阶效应，不能输出2.75批或用9铁冒充10。补1矿/燃料/电只解除对应行的该瓶颈；报价后另一前置变化仍须重查。已有3铁若在另一账本且无合法桥，以上“总铁”只能作物料汇总，建设缺口不能据此归零。候选引导先显示最小可解缺口，再比较合法补料与延期，不自动消费全部剩余原料或强制借债。

### 7.7.10 铜线先用在哪里：同初态可比，不混能力阶段

在 §7.7.1 的假设可用库存12铁/8铜线/8框架、Smelter已可用、材料可合法送达相应账本下，比较首次Assembler建设与先做k批合金。下表追加补料只使用现有基础配方，尚需真实来源、权限和输出条件；无补料路径就只显示缺口。

| 先做合金批k | 合金成果 | 做合金后、补料前铁/铜线 | 为随后建设补铁批/铜批 | 自初态起新增配方电 | 建设后铁/铜余量 |
| --- | --- | --- | --- | --- | --- |
| 0 | 0 | 12 / 8 | 0 / 0 | 0 | 2 / 0 |
| 1 | 2 | 10 / 6 | 0 / 1 | 15 | 0 / 2 |
| 2 | 4 | 8 / 4 | 1 / 1 | 32 | 1 / 0 |
| 3 | 6 | 6 / 2 | 2 / 2 | 55 | 2 / 2 |
| 4 | 8 | 4 / 0 | 2 / 2 | 64 | 0 / 0 |

例如k1的15电=合金9+铜6，另需3铜矿；不是只多9电。下一决定先核合金的已接入用途，再决定补铜还是延期建设。尚无Assembler时，做芯片不是该初态的现成第三路线。

若另选**已经拥有可用Assembler**的扩建初态，8铜线可支撑2芯片的铜部分；仍另需4树脂、相应电力，启用M4 sink时另需2 hardware。全做芯片后铜为0，而sensor仍需铜；因此“高级产物”不是自动更优或自动第二次收益。研究启发来自 [Against the Storm 的配方替代与限产](https://eremitegames.com/recipes-cookbook-update/)：按真实用途和稀缺来源比较，不把物品等级、解锁标签或囤积本身当收益。

### 7.7.11 副产物容量：先问容量量纲，再算能做几批

以下仅是现有 mandatory-output 合同的**待验证容量夹具**，不声称当前native路径已实现相同背压：4铁批产12铁和4slag，假设无中途合法出库、slag独立容量且初始占用0。

| slag可用容量 | 4批总slag需求 | 差额 | 条件结果 |
| --- | --- | --- | --- |
| 3 | 4 | 缺1 | atomic四批应在首sink前拒绝；若逐批独立合法提交，仅前三批能满足该容量项 |
| 4 | 4 | 0 | 恰满，不保证主产物容量/其他前置 |
| 5 | 4 | 余1 | 可容下一批slag，但下一3铁仍需其独立空间 |

若实际authority规定主/副产物共用同一输出容器、每单位铁与slag各占1 item且以下为初始可用余量，4批需要16个item而非4；共享余量3/4/5只能完整容纳0/1/1批。若空间已占2，名义容量5只有3可用。不能把catalog stack_limit、validator单次输出上限或仓储总容量混成同一分母。

候选比较是合法转出、降低本次批量与延期，不默认垃圾可丢弃、自动出售或免费换空间。持续净堆积率p−d>0时，剩余容量C的理想填满时间C/(p−d)仅在同量纲稳定流率且无离散突发时成立；例如C6、每tick产2/合法处置1，只给6tick连续近似，实际批事件另查。外部工厂设计的价值在于显式副产支路，而不是替oasis7新增回收奖励或免费sink。

### 7.7.12 路线运输的损耗、时间与拆单代价

同版本 [logistics authority](https://github.com/nil-studio/oasis7/blob/2d9e32263d3dbc22595ff33b194b1da59165cf1f/crates/oasis7_runtime/src/world/logistics.rs)区分路线运输与direct handoff。已审路线参数为100km/tick、最大10000km、全局pending transit上限2、最多8hops；无profile按Low，Low/Medium/High分别为5/10/20 bps/km。以下固定已安装M4默认铁矿High profile、单条100km路线，损失=floor(发送量×km×bps/km÷10000)，再夹至[0,发送量]，不是对到货量向下取整。

| 发送铁矿 | 损失 | 到货 | 解释 |
| --- | --- | --- | --- |
| 99 | 19 | 80 | 多发至100不增加这一步到货 |
| 100 | 20 | 80 | 20%来自本距离与profile，不是全局固定比例 |
| 101 | 20 | 81 | 跨过下一整数到货点 |
| 分49+51两单 | 9+10 | 40+41=81 | 只证明取整数量差，不证明净获利 |

拆单还需逐单tariff、两个确实空闲的在途位、来源/权限、同路线条件和独立arrival身份；若只能占一个位就有等待机会成本，不能无代价拆成无限小单。99/100/101km对应理想路线时长1/1/2tick，route-bound零距离仍至少1tick；quote的ready_at还包括当前time之后的1tick处理边界，表述的路线时长不是从读quote到receipt的总等待；无route/path绑定且有效距离为0的direct handoff不套这条运输等待/损耗计算，也不是可自由绕过权限的免费替代。

候选优先展示总发送、预计到货、全部费用及槽占用差，而非先调损耗系数。下一轮运行验证需对单单与拆单比较相同总发送和真实总费用，确认差额由既有rounding还是重复结算产生；后者必须修实现，前者是否合理由真实稀缺/费用/用途决定。已在途与预留不得同时计为可发，未到货不抵扣当前输入缺口。

### 7.7.13 同量拆分的维护压力：按真实时序取样

[折旧公式](https://github.com/nil-studio/oasis7/blob/2d9e32263d3dbc22595ff33b194b1da59165cf1f/crates/oasis7_runtime/src/world/economy.rs#L714-L782)在额外manufacturing workload为0、维护参数M=1的recipe-only比较中可简化为base_decay=1000M ppm；util=min(10000,floor(active_pending_jobs×10000/max(slots,1)))；本折旧tick扣floor(base_decay×(10000+util)/10000)。完整实现先加入manufacturing workload，并对base_decay与最终decay分别钳至耐久基数1000000；非零额外负载不能套用下表。M是公式输入系数，不是额外材料/电力扣款；建成当tick不折旧。

| M=1、slots=2，折旧时观察到的pending jobs | 当次扣耐久ppm | 满耐久1000000无修复固定该负载时归零的算术tick |
| --- | --- | --- |
| 0 | 1000 | 1000 |
| 1 | 1500 | 667 |
| 2 | 2000 | 500 |

归零不是自动停机寿命：已审ScheduleRecipe路径未据此证明durability0必拒绝。[step顺序](https://github.com/nil-studio/oasis7/blob/2d9e32263d3dbc22595ff33b194b1da59165cf1f/crates/oasis7_runtime/src/world/step.rs#L400-L420)为time+1→折旧→pending actions→到期jobs→到期transit；同tick尚未结算产物/到货不能提前支付pending后继订单。

可复核的候选夹具把12铁批在同一观察窗口前分别作为1个12批job或2个6批job，均在折旧前已合法入列、同起始耐久、无维修或额外manufacturing workload且到期条件可比：投入均48矿/12燃料/96配方电，产出均36铁/12slag，但该次折旧观察可为1500或2000ppm。不能由这一时点推出所有拆批总磨损更高；完整窗口累计各tick实际负载、空闲和结算顺序。候选推荐不得靠拆任务刷首产/稳定奖励，也不得为“平衡”偷加未生效的20电/tick。

### 7.7.14 维修报价：申请上限与实际消耗不能互换

[维修admission](https://github.com/nil-studio/oasis7/blob/2d9e32263d3dbc22595ff33b194b1da59165cf1f/crates/oasis7_runtime/src/world/event_processing/action_to_event_economy.rs#L290-L354)每hardware_part可恢复25000ppm，但先检查requested_parts库存，最终按ceil(实际恢复ppm/25000)扣除；满耐久拒绝。以下仅固定request2、库存至少2、其他前置合法且缺损不超过50000的算术。

| 缺损ppm | 实际恢复ppm | 实际hardware消耗 | 提交前最少库存（request2） |
| --- | --- | --- | --- |
| 1 | 1 | 1 | 2 |
| 24999 | 24999 | 1 | 2 |
| 25000 | 25000 | 1 | 2 |
| 25001 | 25001 | 2 | 2 |
| 50000 | 50000 | 2 | 2 |

库存1、缺损1时不能据“最终只扣1”推荐request2；可重新核对是否支持request1及其现时报价，不私改原请求。缺损从25000跨到25001多耗1 hardware，早修小缺损又可能浪费每份可恢复额度，两者构成真实取舍，而不是固定每隔N tick点击维修。

候选实验以缺损24000/25000/26000比较合法修复与继续，记录下一窗口实际磨损、需保留的hardware用途、维修后的可用预算与失败后果。若缺乏已核停机阈值/用途，不发明“26000危险线”，也不宣称晚修必更省或早修永远正确。产品sink硬件、维修硬件和建设材料需求分别算，一份库存不得同时承诺三次。

### 7.7.15 能源不可跨系统相加，恢复先看真实报价

| 已审系统/适用范围 | 数字及含义 | 禁止推断 |
| --- | --- | --- |
| runtime本地starter owner Electricity | 初态132，按建设/配方真实费用扣 | 余额0不是Agent死亡，也不能当battery0 |
| simulator Agent power默认配置 | battery100、cap100、idle每tick1、decision1；以pct=floor(current×100/capacity)判定：current≤0 shutdown，否则pct≤5 critical、再否则pct≤20 low | 排产quote前后runway相同，不把8配方电再扣battery8 |
| 可选[M1 storage power](https://github.com/nil-studio/oasis7/blob/2d9e32263d3dbc22595ff33b194b1da59165cf1f/crates/oasis7_builtin_wasm_modules/m1_power_storage/src/lib.rs#L117) | 初始6、cap12，采集/移动用其自身规则 | 不能用6补runtime132或由M1存在推默认所有世界可采集 |

[simulator power](https://github.com/nil-studio/oasis7/blob/2d9e32263d3dbc22595ff33b194b1da59165cf1f/crates/oasis7_simulator/src/power.rs)与Viewer投影分别核配置和安装条件；资源余额不是power projection fallback。数值表必须注明实际使用系统，不能合计为238或把两个cap当共享电池。

设某已授权恢复动作的真实同账本费用为C，余额E≥C只通过这项支付检查，仍须材料、地点、权限和状态有效。例如额外铁批的已知执行费C=8，E0/8/16/32分别在能源项上支持0/1/2/4批；这是生产预算，不是维修/救援报价。若只做条件压力扫描，R8/16/24与假设恢复费C4/8/12的最多支付次数floor(R/C)分别为(2,1,0)/(4,2,1)/(6,3,2)，必须与真实恢复费未知并列，不写成玩家按钮承诺。

runtime兑换电力的credits、reserve、nonce和授权条件不能省略；legacy入口有只读survival quote不代表已有BuyPower/Harvest/Mine闭环。候选预警应指出“下一项已核合法动作的缺口”，而不是统一20%死亡线；无安全可付恢复时诚实保留损失、既有债务、未知提交与退出选择，不免费发电或暂停世界。

### 7.7.16 后继回本：先证明用途，再比较相同服务量

本地starter两工厂20建设电是实际支出，不是Assembler未来每tick自动创造的收益；gear/core等名称也不自带市场价格、被动收入或运输能力。没有真实可达使用者、消费或节省，就把回本写“不可计算”，不把cost_index或物品等级乘产量当利润。

相同最终36铁目标下，12批一次生产耗96执行电；10批后再2批仍耗80+16=96。延后保留16是流动性与重新决策机会，不是最终节省16；如果没有新信息、合法替代用途或风险差异，只增加一次提交/等待，不能计为策略深度。

仅对已确认的同种服务、同币值/同资源计价，投资I与每周期净增益ΔV可比较。条件敏感性I=20、ΔV=1/2/4时，连续近似回本20/10/5周期；若只能整周期结算取ceil(I/ΔV)，ΔV≤0无有限回本。这里20与1/2/4是分析量纲统一后的假设，不能冒充上述建设电可与虚构销售收入直接相减。

比较窗口H=5/10/20周期时分别检查回本是否落在窗口内；持久世界窗口结束仍有剩余库存、设备和义务，末值不得默认为0。新厂增益必须扣新增维护、物流、等待及受影响旧链的机会成本。同目标下只看毛产量或最短回本容易排除更有恢复弹性的方案；将收益未知、无真实需求与窗口外回本分开呈现。

### 7.7.17 合作可更划算，不以封死独立路线造优势

依据[自愿合作与独立止损](gameplay-top-level-design.prd.md#crisis-voluntary-cooperation)，只在两条真实可用、授权及结算边界明确的路径上比较同量交付；下列是未采纳的分析数字，不新增组织费、优惠或自动共享权限。

令独立路径总成本F_i+v_i n，合作路径F_c+v_c n，ΔF=F_c−F_i、a=v_i−v_c。合作较省的条件是ΔF<a n。假设同一可比成本单位下ΔF=12：a=1/2/4时严格更省需整数n≥13/7/4；分别在n12/6/3相等，不能把持平称收益。a=0且ΔF>0时永不更省；a<0时此正固定差下也无正n交叉。ΔF<0时必须重新解不等式，不能机械套正值阈值。

玩家仍需比较交付风险、占用、退出损失和未知提交，而非只看平均成本。小额独立可能更省，稳定大额合作可能摊薄成本，但真实需求上限低于交叉点时此成本模型中的较省优势不成立，另有证据的风险/时效收益分别比较。候选首测n=交叉点前一档/持平档/后一档，不复制竞品回收率或虚构组织效率奖励。无真实第二路径时只报告能力缺口；不能为通过数值对照封锁独立补给、清除债务或免费回收旧资产。

### 7.7.18 恢复尾部与再补库：扩仓不能修复负循环

生存建造研究强调区分历史版本与统计口径：[Timberborn Update5历史公告](https://store.steampowered.com/news/posts/?appids=1062090&enddate=1708005632&feed=steam_community_announcements)把easy食水需求60%改40%，相同库存、零供给下续航是×1.5，不是只多20%。不复制其难度或人口系数；oasis7以已支持资源/危机和真实恢复路径为限。

以下是**归一化压力模型**，不声称oasis7存在每日人口消费。消费C=60单位/日、完全停产3日、恢复尾部2日也完全无到货、必须保留启动库存Q=40且不消费。单次最小可用库存为Q+C×5=340。恢复后毛供给P=100/日、正常生产窗W=5日，净补库仅(P−C)W=200，停产耗300，整周期亏100；340撑过一次不证明下一次可持续。

| 条件 | 完全停供需覆盖量（不含Q） | 补回所需正常生产日 |
| --- | --- | --- |
| C60、P100、停供含尾部5日 | 300 | 300/(100−60)=7.5 |
| C60、P80、同5日 | 300 | 15 |
| C80、P100、同5日 | 400 | 20 |
| P≤C且缺口正 | 按实际缺口 | 无有限补回时间 |

P降20%却使补库时间翻倍，不能按产量同比线性估恢复。Q是库存底线，不在每个周期重复扣；若实际启动会消费Q，另加入周期负担。候选先比较恢复尾部0/1/2日：C60时缺口180/240/300、加Q库存220/280/340，而非武断给10%安全库存。真实耗用或到货有批事件时逐事件核最低余额：初存10、每小时耗1、第20小时才到24，按日汇总会算出日终余额10>0，仍在第10小时耗尽并在补给到达前无法继续满足需求。未知分布不能宣称95%安全。

### 7.7.19 数值改善要说明每批、每分钟还是每个真实需求

[Against the Storm 1.7官方历史更新](https://eremitegames.com/quality-of-life-update-1-7/)将Parts配方油2→1、时间150→90秒；仅以批产量不变、连续满负荷且无上下游瓶颈推导：批吞吐0.4→2/3批/分钟，增加66⅔%；油/批减少50%，油流量却从0.8→2/3单位/分钟，只减少16⅔%。同样完成10批时才是20→10油、1500→900秒的比较；不据此宣称全城总成本降低50%。这些不是oasis7现行配方或时间单位。

回到oasis7已审铁配方，1/4/12批的执行电/铁均为8/3，但基础1tick路径的产量/任务槽tick不同；若只需要12铁，生产36不自动产生3倍价值。候选报告分别列需求满足量、无已核用途的余量、原料/电力总投入、等待与可撤回时点，不能以满负荷产量掩盖无需求囤积。

建议需求压力点为已确认目标的0.8/1/1.2倍，仅作待测变化：例如目标12铁对应需求9.6/12/14.4时，仅在本分析假设需求向上取整、起始可用铁为0时，目标为10/12/15，最少4/4/5批、余铁2/0/0；真实authority舍入或初态不同须重算，不能隐式按小数交付或把本分析取整当默认。若真实需求固定不可改，改做不同已存在订单样本而非生成买家。稀缺材料替代路线使用实际机会成本，不把市场cost_index当单价；无真实需求/价格时保留未知。

### 7.7.20 扩产投向瓶颈，恢复需要净余量

下面是工厂/城市经济研究迁移出的**归一化网络夹具**，不是oasis7运输带档位。先将上游、物流、下游统一成同种最终交付单位/分钟，并假设稳态、同路径且可用：

| 上游 / 物流 / 下游上界 | 交付上界min | 相比基线增益 |
| --- | --- | --- |
| 1200 / 780 / 1000 | 780 | 基线 |
| 1500 / 780 / 1000 | 780 | 0，单扩上游未解瓶颈 |
| 1200 / 1200 / 1000 | 1000 | 220，而非420 |

真实oasis7需从该路径有效能力取得同量纲上界，再以已结算窗口吞吐对照；观测吞吐不能自动当容量上界，瓶颈还须核空转/阻塞与供给状态；库存60不是每tick供给60，工厂名义slot不是到货率。未知一段时不能把另外两段最小值称全链保证。缓冲只削峰，不提高永久瓶颈；若恢复供给与持续需求都780，缓冲耗掉后永远不能在维持该需求时补回。

另一恢复候选固定需求D100单位/分钟、复工供给S125，积压仅指可合法延期的欠单而非可事后补偿的生存消耗；若另作补库存模型须单独记账，不把同一产出同时用于补库与清欠：断供1/3/5分钟造成积压100/300/500，恢复后仍满足新需求，净追赶25/分钟，所以追平4/12/20分钟，而不是0.8/2.4/4。S≤D没有有限追平时间。与 §7.7.18 共同比较扩仓、缩短恢复尾部、增加真实净余量或合法减少承诺；不得把减需求隐瞒成“恢复更快”，也不自动取消旧债务。

候选先测瓶颈供给±10%/±20%及恢复尾部0/1/2，观察哪项改变真正增加可交付量、何时只转移瓶颈；不要同时修改全部参数后失去归因。额外工厂/路线成本、能源、维护与空间另计，不能只按上界增益自动推荐扩建。

### 7.7.21 已安装validator的压力边界，不是统一批上限

仅在已安装适用product validator并经step_with_modules结算的路径，[到期产物验证](https://github.com/nil-studio/oasis7/blob/2d9e32263d3dbc22595ff33b194b1da59165cf1f/crates/oasis7_runtime/src/world/economy.rs#L498-L689)逐个读取job.produce与job.byproducts的stack，先校验后production settlement。它不是世界总库存或扣除维护后的净库存变化；native step不能直接套用该模块边界。

固定[内置铁validator](https://github.com/nil-studio/oasis7/blob/2d9e32263d3dbc22595ff33b194b1da59165cf1f/crates/oasis7_builtin_wasm_modules/m4_product_material_iron_ingot/src/lib.rs)铁输出单stack上限500，则铁每批产3：

| 单个合法启动压力job的批数 | 铁output stack | 矿 / 燃料 / 执行电启动投入 | 仅铁stack上限这一项 |
| --- | --- | --- | --- |
| 166 | 498 | 664 / 166 / 1328 | 未超500；其他产物/副产验证仍须通过 |
| 167 | 501 | 668 / 167 / 1336 | 超500，结算前拒绝 |

这是另行供足资源的合成压力初态，不是132电/60矿starter能触达的默认路线，也不把500当仓储容量或通用admission cap。源代码明确启动时input/power已sink；后续validator拒绝没有production settlement，不能许诺自动退款或产出501。普通step本身失败的staged rollback与这个已经成功记录的生产blocked不是同一情况。

候选首选在提交前对已知有效validator作有来源的可读预检，呈现每个output及副产物的限制与最小超额，而不是盲目调高所有产率/降低能耗。预检缺validator、版本漂移或无法证明最终执行结果时保持未知，不承诺规避拒绝；适用规则改动仍归对应authority。对166/167分别验收启动、验证、sink与receipt，重复重试不能复制产物或恢复已消耗资源。该预检尚未实现，当前结论仅为源码条件与算术风险。

### 7.7.22 数值方案的收敛顺序与反证

当前建议不改部署系数。按真实可达初态先检验推荐与可读性，再决定是否值得改规则；不存在相应可达路径时保留缺口，不能借未安装模块、免费资源或跨账本相加满足样例。

| 独立比较通道 | 首测候选与具体对照 | 推翻推荐的条件 | 尚未证明 |
| --- | --- | --- | --- |
| 本地已核starter初态 | b10/g4余16，对照legacy b12/g4余0；其他系数相同 | 已承诺36铁用途更值钱且后续真实补给可付，或分段只增加手续、无新信息 | 正式入口选择理解、恢复可达及继续意愿 |
| 零中间库存且有合法材料路径 | 保持现值对照H2建设铁8；先不试全链H1产率4 | 瓶颈其实是铜线/框架/来源/权限，降铁门槛不增加真实下一选择 | 非bootstrap端到端建设与后继用途 |
| 已安装M4产品profile | core1/2/4及其11/22/44 hardware需求，核整数余料 | 产品profile不同或hardware来源不可达；BOM不作为可执行报价 | 合法供应/物流/每级运行 |
| 已安装validator的合成压力初态 | iron166/167的提交前风险呈现 | 验证器/模块路径不同，或尚无足额资源，不能称当前玩家受影响 | 预检实现及完整负例运行 |
| 归一化供给/恢复模型 | 尾部0/1/2、供给±10%/±20%，比较真实净余量 | 输入、需求或恢复事件未知，不能给保证存活概率 | oasis7适用机制、实际分布及真人行为 |

一次完整阅读样例先选其中一个通道，明确成果、剩余可消费库存与已承诺占用，再算下一动作：例如local b10/g4的30铁是成果，16电是同账本余额，gear site铁已经用尽，不能把30铁免费搬去再做gear；若补料路径和报价未知，下一安全决定是核对/延期而非自动恢复。生产结果未知先核原请求，不为凑本轮继续而重复提交。

静态验证优先核等号两侧量纲、整数批/取整边界、共享上游合并、逐账本守恒、module/入口/能源分离和所选真实顺序。后续runtime验证再核同source/profile的quote→admission→sink→完成/blocked→下一合法动作；玩家审阅与真人样本另证是否看懂牺牲、愿意继续或自然离场。纸面计算、AI角色审阅、CI及目录存在都不升级为游戏平衡/留存通过。

研究迁移只采用有版本边界的方法：工厂的非线性代价/瓶颈与副产，城市经济的分母/劳力/物流，生存建造的恢复尾部和周期净补给。竞品系数不成为oasis7常量；例如[Captain of Industry官方Update3](https://coigame.com/Blog/update3-is-out)把空闲维护33%改20%、最大回收效率90%改60%（办公室可另加15个百分点至75%），旧Wiki值可能失效，引用须保留版本且以当前authority解决冲突。该事实仅说明资料冲突应核版本，不给本世界复制20%维护或60%回收。

扩展研究的可迁移问题还包括：[Satisfactory clock speed](https://satisfactory.wiki.gg/wiki/Clock_speed)比较同产量下设备数/运行强度与功耗，不把其指数写入本世界；[Anno 2017 pre-alpha装卸与运输开发日志](https://www.anno-union.com/devblog-pushing-carts/)区分库存总量、位置与装卸速率，不从共享库存推免费送达；[Frostpunk官方Survivor历史公告](https://store.steampowered.com/news/posts/?appids=323190&enddate=1538658945&feed=steam_community_announcements)用于分开经济压力与主动暂停/存档等操作限制，不为制造难度新增强制在线或世界暂停。连同前述Against the Storm和Timberborn，研究覆盖工厂优化、城市经济和生存恢复三类；每个迁移只采用本节已标明的分析问题，不把六个游戏拼成新机制清单。

## 8. 未决问题、风险与仍未覆盖范围

- 原 §2 四层和 §5 演进步骤继续适用。跨全部受支持路径的玩家因果→持续价值→下一选择、微/中/长循环的完整接线、失败恢复/可观测性、长期连续积累和发布/长稳验证仍未完整分配；局部首产物或分支不能吞并它们。
- 原 §3 运行/WASM/治理/观测接口及反馈/平衡仍需其专业 receiver 与适用验证逐项接收。缺少完整设计不自动等于需要新产品政策；优先核对既有合同，真正互斥的选择交对应专业 owner 裁决。未建立关系的义务不是 N/A。
- 危机参与方向按[自愿合作与独立止损](gameplay-top-level-design.prd.md#crisis-voluntary-cooperation)统一：合作可以更有利，不以加入组织作为唯一继续入口。Trace refs: O7-REQ-3f259d96-508e-462d-a8c9-e1851e85e1ed。gameplay 负责选择与代价解释，Runtime/组织与结算 authority 负责真实可执行恢复、资产和既有义务，Viewer/API 展示同义结果，QA 对照独立/协作与退出负例；未实现路径保持缺口，不能凭本条宣称已可用。恢复方案不得免除已发生的成本、债务、对方权利、归属/托管或未知提交；死亡和组织连续性不因更换目标自动逆转。
- 配对 PRD §9 的既有指标目标与危机频次保留数值，适用与证据边界以该节为准；不重平衡、不宣称达到。生产窗口 W、资源奖励/难度、mode、权限/schema、战争重启均不在此次决定范围。
- runtime readiness、文档检查、fixture/单元回归或历史 pass/Go 不代签当前玩法、正式入口、live/provider、真人留存、release、rollback 或 longrun。原 §4 的核心体验与反馈清晰度约束不变；长期设计与验证债不能随本次结构诊断消失。

## 11. 验证计划与证据边界

以下是待执行的分项验证计划，没有本轮业务运行结果。选择当前候选、profile、正式入口与必要环境；缺前置如实记录 blocked/未验证，不能为得到正例启用未授权能力。测试方法沿现有手册，判断 oracle 来自各行对应的产品/专业合同。QA/producer 分别维护技术、体验理解与继续意愿判断，不从文件存在推断运行通过。

### 11.1 验证映射表

| 上游 requirement / product AC | 本设计条款 | 验证对象与失败边界 | 验证方法 / test/manual source | 证据与结果定位 | 覆盖限制 |
| --- | --- | --- | --- | --- | --- |
| [REQ-FIRST-INDUSTRIAL-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-002) | [首产物完成边界](#first-product-boundary) | 在同一 candidate/profile 下逐步观察 quote→submit→receipt→候选；负例分别停在排程、扣减、普通库存变化或不匹配 owner/output，均不得称首产物结算。 | 待执行：[测试手册](../../../testing-manual.md)。按适用 S6 真实入口核对同一对象、参数、authority、receipt 和可读结果；缺合法前置记录 blocked，未执行保持未验证。 | 当前 PR 与实际验证记录：各阶段状态/事件与可见反馈、owner/output 对账、缺失前置；结算与后继分别记录。 | 不代签稳定窗口、Assembler 建成、terminal delivery 或全部工业路径。 |
| [REQ-FIRST-INDUSTRIAL-008](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-008) | [成果承接与自然离场](#continuation-boundary) | 覆盖成果到达、后继当前不可执行、自然离场及重入；旧成果不误归新意图，未决请求不当未生效，失效授权不复活，不制造重复效果。 | 待执行：[测试手册](../../../testing-manual.md)。按适用 S6 的真实入口恢复场景核对成果与目标；实际持续能力窗口及无提示续玩观察单列，未覆盖保持未验证。 | 当前 PR 与实际验证记录：原结果/未决义务与重入目标、实际可用动作及提示介入记录。 | 不由候选可见推出持续能力、后续两 beat、离线收益或真人回访。 |
| [REQ-FIRST-BRANCH-001](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-branch-001) | [分支解释与阶段](#branch-explanation) | 对照当前阶段主指引、完整分支承诺与合法动作；拒绝虚构未开放阶段。选可达路线时分别记录两个后续 beat 的实际差异，只有标签变化不得作为真实取舍。 | 待执行：[测试手册](../../../testing-manual.md)。按适用 S6 对照正式入口与权威状态；结构回归和实际路线执行分开，缺可达路线/后续窗口如实列缺口。 | 当前 PR 与实际验证记录：阶段与合法动作对账，分支成本/收益和各后续 beat；缺失或不一致证据。 | 不新开路线或强制站队，不以阶段文案证明路线可达或两 beat 已实现。 |
| [REQ-FIRST-BRANCH-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-branch-002) | [回退窗口边界](#rollback-window) | 分开核对截止前、截止时、incident 开始、修复后和完成态；不得在关闭后恢复 available，也不得以缺失/冲突 phase 推出可回退。 | 待执行：[测试手册](../../../testing-manual.md)。按适用 S6 场景核对展示与已授权能力；现有 Viewer fixture 窗口结构检查独立记为 fixture_structure_only，真实回退缺实现/入口时不执行或宣称通过。 | 当前 PR 与实际验证记录：窗口判断、回退报价与阶段一致性；fixture/真实入口分列及缺失前置。 | 现有 attraction fixture 窗口不证明真实 Runtime 回退；不新增回退权限或费用。 |
| [REQ-WR-PE-002](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-002) | [总纲评审证据分层](#evidence-layer-boundary) | 评审仅自动化通过、仅 fixture/默认分数、L4A/L4B 正向但缺 L5、或存在高价值反证的输入；报告必须列直接证据/推断/未证明项，不升级为真人意愿或留存。 | 待执行：[测试手册](../../../testing-manual.md)。按手册 L4A/L4B/L5 与 Playability review 流程由 QA/producer 审阅当前输入；缺必要来源保留不足/阻断，不把评审计划当运行证据。 | 当前 PR 与实际验证记录：各层来源、证明范围、反证、未证明项及适用建议；不复制历史 pass。 | 不接管其他 claim 消费者，不以本次局部条款宣称整项 PE-002 覆盖或发布就绪。 |
| [REQ-WR-PE-003](../../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-003) | [总纲评审比较单位](#comparison-unit-boundary) | 分别提供入口/目标/候选/窗口/适用环境不同的样本与无法回链样本；不匹配只作相邻观察，范围不明仅支持 watch，若支撑更强 claim 则 hold，不能抵消另一单位阻断。 | 待执行：[测试手册](../../../testing-manual.md)。按手册 Playability review 对照当前 packet 的 change scope、formal surfaces、evidence 与 blockers 及本条产品 AC 逐项人工复核；同单位也不能越过证据层级。 | 当前 PR 与实际验证记录：每个结论的比较单位、来源与缺项；不合并理由和残余 blocker。 | 不新增产物 schema、统一 verdict 或替代全部消费者核对。 |

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

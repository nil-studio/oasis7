# 工业工作图与配方生命周期系统设计

- 状态：`target` 技术合同；本文冻结目标逻辑记录、权威边界、转换和恢复不变量，不声明实现或验收完成。
- Owner：`runtime_engineer` 负责工作图、事务和恢复；M4/domain owner 负责批次、数量、执行单位和处置规则；配方治理 owner 负责版本准入与生命周期；`gameplay_designer` 负责政策和机会成本；Agent、Viewer/API 与 QA 负责消费者和独立验收。
- 审读基线：`nil-studio/oasis7@e4bfdb8d2f03a3ccdc4a61a622881c19dbd272c6`。范围来源 [#4856](https://github.com/nil-studio/oasis7/issues/4856)；历史实现证据继续按产品状态卡的固定版本解释。

## 1. 问题、目标与非目标

现有普通 recipe job、材料与电力扣减、部分物流预留和完成回执不能表达多阶段工作图，也不能表达配方版本的六状态准入。目标是使一个工业承诺从接受、齐套、加工、运输、缓冲到终端处置始终保留身份、资源义务和单次效果，并让配方状态变化通过明确接口处置存量工作。

阶段 A 交付 SC-20/21 的工业工作图和 receipt authority；阶段 B 交付 SC-24 的配方准入状态机，只通过不可变 `RecipeVersionRefV1`、准入判定与六类工作处置接口消费 A。两者可以分阶段实施，不能由 A 完成推导 B 完成，也不能把配方治理和工作执行合为一个 authority 枚举。本文不制定配比、价格、产能数值、公平策略参数、奖励或维护政策；不实现额外 Agent 制造、完整首局、部署、全部工业产品验证或工厂升级。

## 2. 上游约束与相关角色

产品适用条件、成功/失败边界和 [§5.1 测试层级](../../product/world-rules-core-gameplay/prd.md#51-验收追踪)保持不变；下表只承接对应独立叶子，不以专题通过代签全部 SC-21 或 SC-24。领域输入来自 [M4 多阶段合同](../../world-simulator/m4/industrial-resource-flow-contract.prd.md#4-technical-specifications)；当前与目标由 [工业执行矩阵](../prd.md#industrial-execution-status-and-authority-matrix)区分。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 未覆盖范围 |
| --- | --- | --- | --- | --- |
| [工业任务续接与因果换向](../../product/world-rules-core-gameplay/prd.md#root-sc20-jobcontinue)；Trace refs: O7-REQ-15abc2d8-2dc7-43ed-bb20-895dc11e8452 | 同因果续接保持 root；因果变更链接新候选且 W=0；停机、权限变化、重试不迁移或复制效果 | [身份与因果](#industrial-identity) | Runtime、M4、gameplay | 不替代全部任务恢复选择的产品验收 |
| [多输入 join](../../product/world-rules-core-gameplay/prd.md#root-sc21-join)；Trace refs: O7-REQ-6e7590d6-923d-407a-93df-eef373cccaea | 固定完整 parent set；默认齐套前无 sink；支持的 staged intake 保留逐 parent 消费与剩余义务 | [join](#industrial-join) | M4 parent/适用性，Runtime 并发和资源 | 不新定品质算法 |
| [共享容量分配](../../product/world-rules-core-gameplay/prd.md#root-sc21-capacity)；Trace refs: O7-REQ-7f40ee7a-192f-4579-a69d-9946a6e4d5bf | 同一快照完整 contender 集、full/partial/deferred/denied、单次释放和公平重评 | [分配](#industrial-allocation) | M4 容量、gameplay 公平策略 | 不以普通 route busy 拒绝代签 |
| [service window](../../product/world-rules-core-gameplay/prd.md#root-sc21-service)；Trace refs: O7-REQ-a3562631-35fa-4581-86e3-589e530ad253 | 绑定全部 reservation chain，重验世界时界与 none/soft/hard，各次释放和续期可追踪 | [服务窗口](#industrial-window) | M4 窗口模式、世界时间 | 不把 production on-time 当 delivery |
| [诊断归因](../../product/world-rules-core-gameplay/prd.md#root-sc21-diagnostic)；Trace refs: O7-REQ-3373ec45-3ce8-4539-8bdd-f85acca2c252 | 接受时冻结 baseline，receipt-only actual、互斥 bucket、损耗补偿与主/派生 blocker 不重计 | [诊断](#industrial-diagnostic) | M4 分类与指标，授权读面 | 不新增指标公式或 SLA 数值 |
| [有限缓冲与背压](../../product/world-rules-core-gameplay/prd.md#root-sc21-backpressure)；Trace refs: O7-REQ-65e2849e-3b51-4440-a5dd-78cdd7c0692a | 下游失效时保持、有限 buffer 或原子拒绝，禁止丢失、瞬移与无限堆积 | [背压](#industrial-backpressure-terminal) | M4 buffer/route authority | 普通运输容量不代签中间品背压 |
| [cutover 旧工作处置](../../product/world-rules-core-gameplay/prd.md#root-sc21-cutover)；Trace refs: O7-REQ-38521519-d028-4768-abc5-723e239192c4 | 单一边界逐项处置旧 hold/WIP/transit/buffer，不支持转换拒绝，保留父 receipt 与数量/损耗 | [cutover](#industrial-cutover) | M4 处置能力、配方/工厂 owner | 不自动授予 successor 或免费转换 |
| [有限终端](../../product/world-rules-core-gameplay/prd.md#root-sc21-terminal)；Trace refs: O7-REQ-012b7904-2468-48c6-996d-14698aa7f798 | owner-bound 有限存储或明确交付承诺；终端失效保留有界义务，生产与交付分离 | [终端](#industrial-backpressure-terminal) | 终端 ledger/admission owner | 不定义市场或需求结算政策 |
| [配方六状态](../../product/world-rules-core-gameplay/prd.md#root-sc24-recipestate)；Trace refs: O7-REQ-e912cdd3-0222-4a88-b920-983a4118a6d1 | 六状态独立于 blocker reason；生效前无配方效果；Agent 只能按授权提议确认 | [配方转换](#recipe-lifecycle) | 配方治理、M4 验证、Agent 权限 | 不能由模块发布状态代签 |
| [配方六类工作处置](../../product/world-rules-core-gameplay/prd.md#root-sc24-recipedispose)；Trace refs: O7-REQ-df2f1a58-f435-4e86-b6c1-810f13f8267d | 各状态对新排程、accepted 未 sink、WIP、transit、buffer、terminal-pending 独立处置 | [动作矩阵](#recipe-work-disposition) | 配方治理 + 工业执行两个 owner | 不将通用处置清单授权给 retired |

## 3. 当前状态、目标状态与差距

| 对象 | 当前基线 | 目标 | 差距与证据 |
| --- | --- | --- | --- |
| 工作身份与扣减 | `RecipeJobState` 保存 job、recipe、输入/产物、账本、路径与 ready_at；普通任务真实扣料 | 持久 root/revision/child、join、窗口及终态 | [当前状态](../../../crates/oasis7_runtime/src/state.rs)仍不是工作图；不能把 `committed_recipe_origin` 当 recipe authority |
| 原子 apply | recipe 与 transit 有局部 prepared transition 和单次完成检查 | 工作图、库存、容量、receipt 在同一事务提交 | [recipe reducer](../../../crates/oasis7_runtime/src/state/industry_transition/recipe_lifecycle.rs)、[transit reducer](../../../crates/oasis7_runtime/src/state/industry_transition/material_transit.rs)是可复用子能力；[统一事务](../design.md#62-unified-executiontransaction-architecture-p0)总体仍 partial |
| 配方生命周期 | `RecipeProfileV1` 为 recipe_id、tags、stage_gate、preferred_factory_tags 四字段 | 不可变版本 + 六状态 authority + 存量处置 | [当前 ABI](../../../crates/oasis7_wasm_abi/src/economy.rs)没有六状态；模块 release 与设施 retired 检查不能替代 |
| 对外读取 | 现有普通 job/receipt 读取 | 授权工作图、held/consumed/unmet、窗口、处置及下一步 | 新 DTO、API/Viewer/Agent parity 与端到端恢复均待实现验证 |

## 4. 边界与结构

Runtime 拥有目标记录的校验、顺序、准入、事务提交和恢复；M4 只提供版本化领域判定与 effects proposal，不能直接改 canonical ledger、图或 journal。配方治理给出版本有效范围与转换授权，不能写入工业 hold 或伪造工作完成。所有 native/module/API 入口消费同一 reducer，不能各保存一份工作真值。

实现可以把领域记录承载于版本化 module instance state；这一存储选择不改变 Runtime 对提交、receipt 及恢复的权威，也不授权模块直接写 Kernel。不得为本专题复制一套共识、节点存储或泛治理状态机。外部性、产品验证、需求结算等独立权威通过 `ChildRefV1` 和它们自己的 receipt 关联；此处的 production receipt 不替它们做判定。

<a id="industrial-identity"></a>
### 4.1 身份与因果

一个已认证的 intent 使用世界身份、主体及原始幂等键解析唯一接受决定；同 key 同 payload 读取原决定，同 key 不同 payload 为 `idempotency_conflict`。网络接收、preview 和尚未权威裁定的 pending 均不发 root。真正的 accepted outcome，包括 accepted 后 allocation 为 deferred/denied，原子创建一个 `RootOperationV1` 与初始 baseline；原子拒绝且没有 accepted intent 不创建。两项独立 intent 即使 payload 完全相同，也产生不同 root。

`RootOperationV1` 固定 accepted intent、候选、owner/执行作用域、初始 recipe ref 与父 cutover（若有）；`RevisionV1` 固定该修订的配置和 baseline；`ChildRefV1 = (root_id, revision_id, segment_id, child_id, parent_ref, role)` 绑定每个 stage、join、bundle/branch、transit、buffer、terminal、window、checkpoint 和 receipt。`child_id` 由已提交的 root、局部稳定角色和 ordinal 分配，分配游标随事务持久化；不能用 wall clock、读次数、重试次数或数组到达顺序创建身份。跨 root 的原料来源用显式 parent settlement ref 表达，不把同一 receipt 的拥有者改成当前 root。

同因果恢复保持 root 和 owning revision，只能新增 continuation segment；纯元数据修订可新建同 root 的 revision。改变已冻结配方定义的投入/产物/规格、工厂能力、阶段/边、电力/物流或终端因果内容，创建 parent-linked child root/new candidate，W 从零计数，旧实际、receipt、reservation、顺位和奖励保持原归属。不能证明非因果时拒绝原地修订，要求显式新候选。切换按 [单一 cutover](#industrial-cutover)执行，身份的事务、终态和恢复按 [原子恢复合同](#industrial-transaction-recovery)执行。已冻结 profile 明示的等价替代不等于修改配方定义，但只有 [替代因果合同](recipe-substitution-decision-and-conversion.design.md#substitution-causality)证明全部产出、品质、电力、时间、副产物、终端和 value class 不变后，才可保留 pinned recipe/root 并追加 child decision；source/batch 选择仍必须经过下述 parent-binding 接口，不能从同名或相同比例推断等价。

## 5. 关键运行流程

<a id="industrial-join"></a>
### 5.1 接受、齐套与单次消费

1. `SubmitIndustrialIntent` 校验 actor、世界、版本、当前可用 profile 与大小界限，冻结 candidate/config、输入输出拓扑、recipe ref 和 requested/admissible quantities；未知前置可进入无效果 pending，不能隐含持有容量。
2. 接受时创建 root/baseline；对 required parent edges 按 profile 的 canonical edge order 建立 `JoinRecordV1`。每个 slot 固定所需数量、规格版本、到达条件和初始 `ParentBindingRevisionV1`，后者以本次 accepted intent/root 为 origin，明确 parent settlement/batch refs 与 ledger；未到达 parent 仍是明确未决 slot，不能从后来“最新同名材料”填入。
3. `atomic_kit` 为缺省。单一快照验证全部 parent 已结算、已到达、适用、可独占且数量满足合法执行单位后，才原子写 parent consumption、资源 sink、join-start receipt 和 WIP。缺任何 parent 时可保留有限未消费 hold，不能提前 sink 或记进度。
4. `staged_intake` 只有 profile 显式声明实现支持、remaining obligation 和失效处置后才可启用；否则 `unsupported_profile_mode`。v1 仅支持整 parent slot 消费：一个 slot 可以等待其固定来源的分批到达，但必须等该 slot 的规定数量全部可用后一次 sink，不支持同 slot 部分 sink。消费键固定为 `(join_id, parent_slot, parent_sink)`，role 不由请求者选择；多个可独立消费的来源必须在接受前声明为不同 slot，不能在重试时追加。未到达/失效 parent、已消费数量和未消费 hold 分开保留。完成 join 只签发一次 stage-start receipt；请求未声明的分批 parent sink 返回 `unsupported_profile_mode`。
5. staged 互等须在 admission 检查依赖环并拒绝无可终结规则的环；运行期等待由已冻结 deadline/recheck/abort policy 有界处置。不能通过 hold 过期释放已消费材料，也不能把未知 partial kit 当作完成。重复 arrival 只查原 slot disposition，新的 arrival 只重评 unmet set。

同因果的合法 source/材料替代仅可对未 sink slot 提交 `ApplyParentBindingV1`：绑定明确确认的 substitution decision、旧/新完整 parent set、expected join/binding revision、需要释放的旧未消费 hold、真实 source/ledger 与授权。decision-only 不改变 binding、不释放 hold。实际绑定事务原子记录新 binding revision、旧选择 provenance 和显式受权的 hold 处置；不改 required slot 集、数量/spec、baseline 或已消费 parent。新 source 重新取得合法 reservation/容量分配，不继承旧 source 的 hold 或顺位。消费永远以原 join+slot 为键，不能借 binding revision 重置；若 required slot/产出因果变化或 slot 已 sink，必须走 cutover/旧工作处置，禁止原地换 parent。

<a id="industrial-allocation"></a>
### 5.2 共享容量与重评

`AllocationRoundV1` 绑定同一 canonical snapshot、完整 contender 集、既有 reservations、shared resource set、capacity epoch 和版本化公平策略。Profile 必须给出 round boundary、稳定排序/tie key、等待保护及失效/重评条件；缺策略拒绝激活该 allocation profile，不能采用调用到达顺序。Runtime 在一轮内确定 full、受支持的 partial、deferred 或 denied；接受身份与容量获得分开。

`ReservationV1` 的单位、数量和持有范围由 profile 定义，同资源上 `held + consumed_for_boundary <= capacity`。partial 还必须通过 recipe quantum/rounding 策略，零合法 cycle 不扣料；unmet 和 residual 留在原 intent。释放、过期或上游到达以 `(trigger_event_id, allocation_domain_id)` 生成一次重评，先原子释放未消费义务，再在新 snapshot 重评仍有效 contender；已消费量不回滚。重复 trigger 不新建轮次，新 trigger 不能复评已终态项。

重试、重连和恢复保留原接受时点、稳定 tie key、累计等待与上轮理由，不刷新顺位。每次未分配都保存 policy reason 和下一重评条件；在容量反复可用时，要按 profile 的等待保护实际取得分配或显式终止/重规划，不能长期只输出同一个无解释 deferred。一次 round 超过配置的 contender/资源上限时不取前 N 偷换完整集合，应无效果返回容量边界错误或使用事先声明的独立 allocation domain 分区。

<a id="industrial-window"></a>
### 5.3 服务窗口

`ServiceWindowV1` 固定 start/target/end 的 canonical world-time boundary、none/soft/hard policy，以及 stage slot、edge throughput、buffer、terminal mandatory reservation chain。每个 reservation mode 独立为 `held_through_boundary`、`boundary_revalidated` 或 `non_reserved`；缺失 mandatory reservation 不得承诺 hard guarantee，未预留未来容量不算持有。

Start、stage finish、transit launch/arrival、buffer admission 和 terminal settlement 各自重验剩余时间、owner、资格和容量。hard failure 阻止下一个不可逆 sink；已有 WIP/运输只执行预先支持的 pause/hold/finish/return 等处置。soft 模式只有在声明允许时继续，并记录 at-risk/late；none 明确 best-effort。pre-start expiry 至多释放一次未消费 hold；已发生效果不因此被撤销。

续期是新的受权 `RenewServiceWindow` 决定，绑定旧窗口、当前义务与新的边界，生成 linked revision/receipt；它不能改写旧 on-time/late 事实、复制 hold、重置等待或把过期结果回填为及时。重放使用已提交世界时间和当时版本，重连不改变时间边界。生产及时、运输迟到、终端未完成必须分别读取。

<a id="industrial-backpressure-terminal"></a>
### 5.4 背压、输出与终端

每个 edge 和 terminal 具有显式有限 `CapacityRefV1`；整数存储上界不构成业务容量。上游开始前验证 profile 所需输出去向：默认 bundle 全部主/副产物 branch 在任何 output credit/progress 前共同预检；split fan-out 仅在真实支持的 profile 下逐 branch 结算，未决 branch 和剩余义务仍属于原 bundle。此处消费既有 M4 fan-out/quantum 合同，不把本专题验收扩展为这些独立叶子的全部证明。

下游不可用或 buffer 满时，未 sink 的输入保持原 ledger/有限 hold 或原子拒绝；已产生的批次只能进入仍有容量且有权接收的 buffer，或保持受支持的有界 WIP/hold/quarantine。每个批次占用必须计入唯一容量账本；不能同时记为源库存和在途/目的库存，不得用后台队列充当无限仓库。无支持的去向/保持方式时必须在更早的不可逆边界拒绝新的上游承诺。

`TerminalObligationV1` 固定 quantity、owner、recipient、destination ledger、purpose/admission、capacity reservation、parent production/arrival refs 和 disposition。production receipt 只证明产出；只有匹配 terminal settlement 才能交给需求、市场、服务或交付奖励消费者。terminal owner/资格/容量失效后维持原义务的有界 blocked，或执行已声明的受权处置；不能悄悄改 recipient/destination、提前结算、自动转卖或重复释放容量。查询失败显示 unknown，不写工作状态。

<a id="industrial-cutover"></a>
### 5.5 单一 cutover 与旧工作处置

`PrepareIndustrialDisposition` 只读枚举切换 boundary 上全部受影响旧 work node、reservation 与 terminal obligation，冻结 graph/config revision 及有序 item set。每个 `DispositionItemV1` 说明旧 bucket、所选动作、执行 profile、权限、资源/损耗、pending obligation、父 receipt 和目标引用；缺合法动作时整次 cutover 无效果拒绝，不能漏掉旧工作。

`CommitIndustrialCutover` 重新校验同一 item set 与所有 authority revision，在一次事务中关闭旧 candidate 的新准入、记录 `CutoverReceiptV1`、为每项固定一次处置选择、创建新 candidate/child root 并令 W=0。并发完成先于 cutover 时必须重新准备；cutover 先提交时，旧完成只能沿已固定处置执行，不能同时退款和完成。旧 reservation 不传给新 root。

处置选择与后续执行分开：`hold` 的选择可以一次提交，日后 resume/finish 各有链接该选择的独立 execution receipt；“每项一次处置”不表示 hold 后永远不能继续。修改已选路径需新的受权、parent-linked disposition revision，并仅处理尚未终结义务；旧 sink、已释放量与结果不能再消费。同资源、同单位的 handoff 显式守恒 source quantity = destination quantity + declared loss。跨材料或单位的 conversion 不直接相加裸数量，必须消费已批准且不可变的 conversion profile，固定输入/输出单位、比例、量子与舍入、额外投入、损耗和目的地；逐 source 对账一次真实扣减，逐 target 对账一次合法入账，并按该 profile 的变换关系验证数量、资源成本和损耗。所有转换保存旧/新 root 与全部父 receipt；未知单位换算、缺额外成本或未支持迁移在首 sink 前拒绝，无免费产物或损耗回填。

<a id="industrial-diagnostic"></a>
### 5.6 不可变计划与诊断

`PlanBaselineV1` 在 authoritative accepted outcome 与 root 同事务冻结，包含 candidate/config epoch、requested/admissible/committed/executable/unmet/residual 和当时的各项资源/窗口义务。后续配置和容量变化不覆盖 baseline；重规划只建立 linked revision。`DiagnosticWindowV1` 绑定 baseline、world-time start/end、journal range、state-root anchor 与唯一 opening bucket set。

每个数量 lot 在同一切点只属于 backlog、join_pending、WIP、transit、buffer、terminal_pending 或 settled 中一个 bucket；holding 是其义务属性，不另计一份库存。拆分生成显式 child lots，不能重复整批计数。actual 只取 owning revision 的已提交 sink/production/delivery/loss/compensation receipt；同一补偿只与原效果对账，不改写历史 actual。primary blocker 取最早的 canonical 未满足边界；稳定 precedence 解决同时原因，派生 blocker 只关联不重复累计。

合法 measurement checkpoint 只能在 profile 声明的原子边界关闭一次 review segment，不能切开 join/cycle/bundle/handoff；待决义务作为新 segment 的 opening state，不能当成新的接受、sink 或产出。checkpoint 不续租、不重置 W、不释放容量。无完整 journal/config/anchor 时窗口为 unknown/degraded，不用当前配置或 UI 采样补历史。

## 6. 接口与数据合同

<a id="industrial-model"></a>
### 6.1 持久记录

以下为目标 `v1` 逻辑 schema，所有 ref 含 world、schema version 和所属 domain/instance；编码、字段顺序及 hash domain 由现有版本化 ABI/manifest 明确发布，不能用日志字段或 `serde(default)` 伪造 authority。数量使用领域定义的非负整数单位和 checked arithmetic，集合按稳定 key 编码。

| 记录 | 必需字段/关联 | 权威和生命周期 |
| --- | --- | --- |
| `RootOperationV1` / `RevisionV1` | accepted intent/key、payload digest、candidate/config、recipe ref、owner/scope、parent cutover/revision、baseline、status、next child ordinal | Runtime 只在合法接受/修订时创建；root 可 open/draining/closed，终态保留原因和 final receipt |
| `WorkNodeV1` | `ChildRefV1`、kind、bucket、batch/lot、quantity/unit、parent settlement refs、factory/edge/ledger、stage status、obligation refs、blocker、revision | Runtime 负责唯一拥有者、转换及持久化；M4 决定领域适用性 |
| `JoinRecordV1` / `ParentBindingRevisionV1` | child ref、policy、有序 required parent slots、active binding refs、已到达/已验证/已消费量、remaining、start receipt；binding 包含 revision、初始时为空的前一 binding ref、完整 source/parent/ledger set、origin(initial acceptance 或 confirmed substitution decision)、授权、hold disposition | required set 与每份 binding 不可改写；未 sink slot 才能显式追加新 binding，消费键不变 |
| `ReservationV1` / `AllocationRoundV1` | resource/capacity epoch、holder root/child、amount、consumed/released/remaining、expiry/mode、trigger、完整 contenders、policy/order witness、result | 同一权威容量与领域单位；release/consume 分量各单次守恒 |
| `ServiceWindowV1` / `PlanBaselineV1` / `DiagnosticWindowV1` | 身份、父 revision、配置/世界时界/journal/root anchor、reservation chain 或计划数量、opening/closing buckets | 记录不可变；状态变化追加 receipt，不改旧窗口 |
| `TerminalObligationV1` | parent production/arrival、quantity、owner/recipient/destination、purpose/admission/capacity、pending/settled/disposition refs | 终端消费者只消费 matching settlement；关闭前保留未决义务 |
| `IndustrialReceiptV1` | receipt id、child ref、effect role、input parent refs、actual deltas/units/loss、authority revisions、before/after commitment、journal/commit ref、outcome | `(root, revision, child, effect_role)` 唯一；同键冲突拒绝，不按新 profile 重算 |
| `DispositionPlanV1` / `DispositionItemV1` | cutover/boundary、expected graph/config revision、有序受影响 item refs、选定动作/授权/profile、资源与未决义务、执行 receipts | plan 预览无效果；commit 固定选择；后续效果有独立幂等键 |
| `RecipeVersionV1` / `RecipeLifecycleTransitionV1` | 见 [配方 authority](#recipe-lifecycle)；graph 只保存不可变 ref 与准入/处置 receipt | 配方 owner 独立签发；Runtime 只消费当前有效决定 |

### 6.2 跨组件接口

| 接口 | producer → consumer | identity/version | ordering/idempotency | success/error | compatibility |
| --- | --- | --- | --- | --- | --- |
| `ResolveRecipeAdmission` | 配方 authority → Runtime scheduler | exact recipe/version、lifecycle revision、factory/scope/config refs | 只读判定；submit/first sink 重验 | allowed + applicability / blocked + reason/next recheck | 无六态能力的 legacy profile 不自动变 active |
| `SubmitIndustrialIntent` / `ApplyIndustrialBoundary` | 已认证 API/native/module → Runtime | intent key、payload digest；后续 child/boundary + expected revision | 原子提交 root/图/资源/receipt；同键重读 | accepted allocation / rejected-no-effect / commit-unknown | target envelope 独立版本；拒绝 silent legacy fallback |
| `ApplyParentBindingV1` | 受权 substitution consumer → Runtime join authority | join/slot、expected binding revision、decision 与完整旧/新 parents | 未 sink slot 的绑定/旧 hold 处置原子提交；原 sink key 保持 | applied binding / already_consumed / stale_binding / causal_change_requires_cutover | 不修改旧 baseline、required set 或历史 provenance |
| `ReevaluateAllocation` / `RenewServiceWindow` | canonical event/受权主体 → Runtime | trigger+domain；window+renewal intent | 同一快照、有序 contender；续期独立决定 | full/partial/deferred/denied；expired/conflict | 不把普通 ready_at 改名为窗口 |
| `PrepareIndustrialDisposition` / `CommitIndustrialCutover` | recipe/factory authority → Runtime | exact recipe/config、graph revision、plan digest、actor authority、boundary | 先只读枚举，再一次 CAS 校验与提交 | committed cutover / stale_plan / unsupported_disposition | 所有六类工作必须被接口解释 |
| `ReadIndustrialOperation` | Runtime projection → 有权 API/Viewer/Agent | root/child、read anchor、audience | 只读分页；返回 pending 与 committed 分层 | 当前可读结果/unknown/not_authorized | 不泄露其他 contender 私密 payload、身份或精确 queue position |

错误至少区分 `missing_parent`、`wrong_root`、`stale_authority`、`unsupported_profile_mode`、`capacity_unavailable`、`window_expired`、`terminal_unavailable`、`idempotency_conflict`、`commit_unknown`；错误包含原 identity、已知效果/未决义务、真实 next action/recheck，不能把 timeout 当作无效果许可重建 root。

## 7. 状态、事务与持久化

<a id="recipe-lifecycle"></a>
### 7.1 配方版本与六状态转换

`RecipeVersionV1` 固定 recipe identity/version、parent version/candidate、content digest、causal fingerprint、M4 validation policy/evidence refs、factory fit、input/output/spec/power/logistics/terminal refs。`RecipeLifecycleStateV1` 单独保存六态、authority/scope revision、restrictions、effective boundary、transition receipt 和是否已禁止新准入；`blocker reason` 不新增第七状态。Agent 提案/确认只是受权请求，不能直接改记录。

`RecipeLifecycleTransitionV1` 包含 transition intent/key、exact version、expected state/revision、from/to、issuer 与授权链、evidence/policy revision、有效边界、受影响 graph cut 与 disposition plan digest。配方治理决定是否有权进入目标态，工业 owner 决定存量处置是否可执行；任一否定或 unknown 都无转换效果。准入资格与当前运行权限不能由 display name、相邻版本或 module release 继承。

| 转换 | 必须成立的条件 | 拒绝与未决语义 |
| --- | --- | --- |
| 新候选 → `pending_validation` | 新 identity、完整因果内容和有效提案权限；注册只记录候选 | 无材料/容量/生产效果；缺内容/权限拒绝 |
| `pending_validation` → `validated_pending_admission` | 匹配 immutable version 的权威验证通过且证据仍有效 | 不通过/unknown 留原态并记录 blocker；验证不产生排程权 |
| `validated_pending_admission` → `active` 或 `restricted` | 对目标 scope 的准入授权、兼容、资源规则和有效边界已确认；restricted 明确允许子范围 | 生效前仍不可排程；证据失效回到 pending_validation 需显式 transition，不能静默重验升级 |
| `active` ↔ `restricted` | 受权的 restriction 变更/解除与 fresh validation；对存量变化先形成完整 disposition plan | 不能借解除限制改 recipe 因果内容；漂移/缺方案无转换 |
| `pending_validation` / `validated_pending_admission` / `active` / `restricted` → `retiring` | 受权撤回/退役；枚举全部存量工作，关闭新准入并原子绑定每项适用处置 | 空工作集同样需明确；缺 drain 不阻止其他已支持的安全处置 |
| `retiring` → `retired` | 无待执行生产、容量或 terminal 义务；全部处置已终结且对账完成 | 仍有 WIP/hold/terminal-pending 时保持 retiring + blocker；不得提前退休后再发新效果 |
| `retired` → 任意生产态 | 不支持 | 仅历史读取/回放及已记录结果；重新提供能力必须建 parent-linked 新版本重新准入 |

版本改变不原地清空验证/历史；无法证明只是非因果元数据时新版本重新准入。普通 evidence 过期可阻断当前动作，不能假造一个生命周期转换；安全收缩由受权 restriction transition 生效。

<a id="recipe-work-disposition"></a>
### 7.2 六状态 × 六类工作的动作矩阵

以下“依 profile”要求 profile 显式支持该 bucket 的动作并且 fresh authority/资源条件通过；通用词表本身不授予动作。`R` 表示维持原身份重验后执行原承诺或受支持处置；`D` 表示只可执行退役处置；各自的具体动作在下表后定义。

| recipe state | 新排程 | accepted，尚无不可逆 sink | WIP | transit | buffer | terminal-pending |
| --- | --- | --- | --- | --- | --- | --- |
| `pending_validation` | 禁止 | 不新增；若历史异常存在则 blocked，须核原 authority | 不新增；保留历史和异常义务，禁止推定合法 | 同左 | 同左 | 同左，保留原终端义务 |
| `validated_pending_admission` | 禁止 | 不新增；准入未生效不能开始 | 不新增；不能以验证 receipt 解锁生产 | 同左 | 同左 | 同左 |
| `active` | fresh revalidation 后允许 | R，首 sink 前重验 | R | R | R | R，matching settlement 才完成 |
| `restricted` | 仅明确允许 scope | scope 内 R；外只 hold/release/replan/terminate 等安全处置 | scope 内 R；外依显式限制处置 | 依原运输权与限制分别重验 | 依当前 custody/用途重验 | 保留原承诺；不得扩大 scope 或静默换目的地 |
| `retiring` | 禁止 | D：hold/release/replan/terminate | D：finish/pause/hold/rework/salvage/terminate | D：finish/hold/return/reroute/reject | D：hold/handoff/conversion/reject | D：原合法承诺继续或 hold/return/reroute/handoff/reject |
| `retired` | 禁止 | 仅历史结果 | 仅历史结果 | 仅历史结果 | 仅历史结果 | 仅已记录最终处置；不得有待执行义务 |

R 使用与相同 bucket 的 D 相同的受支持动作集合，并可在当前条件允许时继续原生产；任何选择都不能以终止为名销毁库存或释放已消费量。`reject/terminate` 只终止对应未发生动作；对已存在材料仍须保持合法 custody 或显式处置，不能把错误返回当材料删除。没有 finish/drain 能力时仍可独立选择合法 hold/return 等安全路径；缺少全部合法处置时拒绝转换而保留旧状态。

successor 只是导航关系。实际转换必须有明确授权、fresh compatibility、quantity/loss 和 parent-linked conversion receipt；不能继承旧 root、reservation、排队顺位、W 或终端结算。terminal-pending 无论哪条合法处置都保留 quantity、recipient、destination、admission、capacity obligation 与 provenance，目的地改变需新的受权 handoff/reroute，并记录原义务如何关闭。

<a id="industrial-transaction-recovery"></a>
### 7.3 原子性、并发与恢复

一次边界提交共同覆盖 touched graph nodes、allocator、recipe lifecycle/disposition（若转换）、材料/电力账本、容量/hold、milestone delta、receipt、journal 与 durable publication。采用 [ExecutionTransaction](../design.md#62-unified-executiontransaction-architecture-p0)：prepare 基于不可变 base，commit 前对 expected world/graph/config/authority revisions 做 CAS；任何失败不留下部分新效果。存量较多不能中途发布部分 cutover；先构造有界只读计划，超过批次预算拒绝或按事先定义的独立作用域处理。

同 root 不要求全程串行，但触及相同 lot/parent slot/reservation/terminal obligation 的事务必须冲突检测；输出 branch 分离只在 split profile 允许时成立。first sink、stage completion、release、terminal settlement 与补偿分别用稳定 effect role 去重。root 只有在全部已登记义务结清后关闭；explicit abort 也必须携带完整终态处置，未知不能关闭。closed root 的普通新效果拒绝，迟到通知只重读结果；合法补偿使用独立且链接历史的补偿 receipt，不复活旧工作。

snapshot 保存图、parent-binding revisions、receipt 消费索引、allocator、处置计划/状态、窗口/基线和配方记录；journal 的版本化事件能够还原同一结构。完整 generation 同时承诺业务状态、控制状态和未决义务，禁止从不同 generation 拼接。恢复先验证 world/manifest/历史 schema/journal/root，再重建 graph closure、唯一引用、数量守恒和终态一致性；缺 parent、半个 cutover、重复 consume、未知 required schema 均拒绝安装。

重放使用历史决策和已提交 delta，不调用 validator/模块重判、不重新报价或分配、不延长窗口、不发送外部效果。恢复后的已知未生效 pending 才在当前 authority 下继续；提交未知先查询原 intent/receipt 与 durable generation，不创建新 root 或再次扣款。失效授权只影响未发生效果，不能改写原合法历史；已提交结果与读面发布失败通过原 receipt/outbox 恢复。

## 8. 部署、安全与运行约束

本合同不依赖单节点或多节点拓扑，正式写入仍受既有 authority-profile commit/recovery gates 保护。新 profile 只有生产者、消费者、持久 schema、replay 与 capability 支持同时成立后才可启用；缺 staged intake、hard window、conversion 或有限 terminal 的实现时显式拒绝这些模式。

主体只有在当前 delegation/scope 内才能提交排程、变更或恢复；配方治理授权不能代替材料 owner、工厂控制权或终端 recipient/admission。公共投影只给可授权的资源争用理由和自身 held/unmet；不公开其他 actor 的私密意图、贡献、预算或原始身份。图节点、parent/branch 集、活动 roots、hold 数、pending 大小、单次 cutover 和读取分页都有部署 profile 明确界限；达到界限拒绝新承诺并保留既有义务，不能驱逐未决状态节省容量。

## 9. 质量与容量

| 场景 | 环境、规模与资源 | 预期响应 | 指标/阈值来源 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- | --- |
| 确定性与守恒 | 同候选三阶段、2–3 parent joins、并发相同 payload roots | 同 canonical 有序输入 replay 得到相同 graph/receipt/root；网络乱序经排序/去重后等价，不同合法权威顺序各自守恒且无重复 sink/credit | 原 SC-20/21/24 与 M4 单位/容量 profile | §11 各独立场景 | 目标，未运行 |
| 容量争用 | 至少两个 accepted intents、有限 stage/edge/buffer/terminal | 完整 contender 分配，不超额；重试不插队；持续可用容量下按策略推进 | M4 已发布 capacity/fairness policy；不在本文指定数值 | allocation/recheck 故障场景 | 目标，未运行 |
| 资源边界 | 在 profile 上限及超过上限一项的 graph/plan/input | 上限内完整处理；越界在首 sink 前拒绝，既有 obligation 不丢失 | 版本化 profile 的 max nodes/parents/round/cutover/bytes | 边界属性测试和恢复测试 | 不承诺无测量的吞吐/延迟 |

## 10. 兼容、迁移与回滚

旧 ActionId/job/caused_by 和旧 recipe profile 保持原 legacy 执行/读取语义，不能因新 schema 缺字段而补造 root、六态 active、批次 provenance 或有限容量。target graph 由明确新 envelope/profile 生效边界创建；不把历史 job 隐式挂到新 root。确需接收旧未完工作时，用显式 legacy disposition/handoff 证明历史投入和未决义务，无法证明则隔离读取，原合法 legacy 继续范围由既有合同决定。

先提供双格式读取、deterministic replay 和兼容测试，再启用新的工作写入，最后启用消费 A 的配方生命周期 B；不必把 A/B 放进同一实现 PR。新格式生效后的回退须保留其完整 reader/reducer，旧 binary 不能读取时应拒绝写入或回到隔离状态，不能删新字段恢复服务。已发生生产、交付、释放和转换不可通过代码回滚撤销；修正走有权的 linked compensation，保留原历史。

## 11. 验证设计与可追溯性

测试方法遵循 [testing-manual](../../../testing-manual.md)及原 SC §5.1；各实现候选记录 native/module、API/Viewer/Agent 的真实覆盖。下面定义测试目标，不把源码测试存在、文档检查或局部 route 能力当作已通过。required 与 full 仍按每条上游适用条件执行；full 包含多阶段、多窗口、持久化、重连/乱序/replay 和授权消费者一致性。

### 11.1 验证映射表

| 上游 requirement / product AC（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [root continuation](../../product/world-rules-core-gameplay/prd.md#root-sc20-jobcontinue)；Trace refs: O7-REQ-15abc2d8-2dc7-43ed-bb20-895dc11e8452 | [身份与因果](#industrial-identity) | 同因果保留、新因果 W=0，各 child 唯一 | `IG-CONTINUE`：同计划停机/断连/恢复与因果换线分叉；重复 intent 与同 payload 新 intent；快照+非空 journal 对照；[现有恢复入口](../../../crates/oasis7_runtime/src/tests/persistence_recovery_tests.rs)追加目标场景 | 实施 PR 的 graph、receipt、W、world root 对账 | 原 SC 全部其他任务选择不代签 |
| [join](../../product/world-rules-core-gameplay/prd.md#root-sc21-join)；Trace refs: O7-REQ-6e7590d6-923d-407a-93df-eef373cccaea | [join](#industrial-join) | 齐套 gate、逐 parent 单次 sink、remaining | `IG-JOIN`：2–3 parents 全排列、缺失/未知/过期/换线、staged 有界互等；整 slot sink 与同 slot 分批 sink 拒绝；未 sink slot 的受权 source rebind、旧 hold 单次释放及原 slot 消费键；atomic/supported staged 各测，未支持模式明确拒绝；同候选 runtime fixture；按 [testing-manual](../../../testing-manual.md)选择对应 required/full 层级 | parent set、hold/sink、remaining、start receipt | 不能只证明 aggregate 材料扣减 |
| [allocation](../../product/world-rules-core-gameplay/prd.md#root-sc21-capacity)；Trace refs: O7-REQ-7f40ee7a-192f-4579-a69d-9946a6e4d5bf | [分配](#industrial-allocation) | 多意图全量同快照、公平及单次重评 | `IG-ALLOCATE`：至少两个 accepted intents 共享资源，full/partial/deferred/denied、不同 submit/arrival/retry 顺序、释放/过期与长期容量反复可用；记录 policy/order witness；按 [testing-manual](../../../testing-manual.md)选择对应 required/full 层级 | contender set、容量守恒、等待连续及 recheck | 普通单动作 route busy 不代签 |
| [window](../../product/world-rules-core-gameplay/prd.md#root-sc21-service)；Trace refs: O7-REQ-a3562631-35fa-4581-86e3-589e530ad253 | [窗口](#industrial-window) | 每边界校验、续期与 delivery 独立 | `IG-WINDOW`：none/soft/hard；在六个运行边界改变 owner/容量/世界时间；pre-start expiry、production on-time/terminal late、重复 renewal/replay；按 [testing-manual](../../../testing-manual.md)选择对应 required/full 层级 | reservation chain、边界时刻、lease/receipt | 未预留未来容量不算 guarantee |
| [diagnostic](../../product/world-rules-core-gameplay/prd.md#root-sc21-diagnostic)；Trace refs: O7-REQ-3373ec45-3ce8-4539-8bdd-f85acca2c252 | [诊断](#industrial-diagnostic) | immutable baseline、互斥 bucket 与 actual 归因 | `IG-DIAGNOSTIC`：preview/atomic reject 无 baseline，四种 accepted allocation 有 baseline；3+ revision/cutover/checkpoint、loss/compensation、缺 anchor、同 journal 重建；按 [testing-manual](../../../testing-manual.md)选择对应 required/full 层级 | baseline digest、lot buckets、主/派生 blocker、window snapshot | 无 receipt 的估计不可列 actual |
| [backpressure](../../product/world-rules-core-gameplay/prd.md#root-sc21-backpressure)；Trace refs: O7-REQ-65e2849e-3b51-4440-a5dd-78cdd7c0692a | [背压](#industrial-backpressure-terminal) | 下游失效、满 buffer 有界且不丢失 | `IG-BACKPRESSURE`：满 buffer/下游失效分别在 first sink 前后发生，保持输入/合法有限 buffer/拒绝；崩溃后重放逐 lot 守恒；按 [testing-manual](../../../testing-manual.md)选择对应 required/full 层级 | 源/WIP/目的量、容量账本及 disposition | 不以 transport capacity 代签 |
| [cutover](../../product/world-rules-core-gameplay/prd.md#root-sc21-cutover)；Trace refs: O7-REQ-38521519-d028-4768-abc5-723e239192c4 | [cutover](#industrial-cutover) | 每项一次选择、旧新身份隔离和有权转换 | `IG-CUTOVER`：旧 hold/WIP/transit/buffer 各非空；完成与换线竞态、部分 prepare 失败、unsupported migration、支持 handoff/conversion；同单位 handoff 守恒、跨材料比例/单位和额外成本的转换及未知换算拒绝；hold 后合法 finish 不被误判重复；按 [testing-manual](../../../testing-manual.md)选择对应 required/full 层级 | plan/item set、cutover/执行 receipts、quantity/loss/W | 无免费转换或自动 successor |
| [terminal](../../product/world-rules-core-gameplay/prd.md#root-sc21-terminal)；Trace refs: O7-REQ-012b7904-2468-48c6-996d-14698aa7f798 | [终端](#industrial-backpressure-terminal) | 有限 owner ledger、义务与 settlement 分离 | `IG-TERMINAL`：有限容量边界、owner/recipient/admission 漂移、arrival 与 release/retry 竞态、持久 terminal pending；正例 matching receipt 单次 settlement；按 [testing-manual](../../../testing-manual.md)选择对应 required/full 层级 | quantity、capacity、recipient/destination、production/delivery receipts | 不代签所有市场/需求用途 |
| [recipe state](../../product/world-rules-core-gameplay/prd.md#root-sc24-recipestate)；Trace refs: O7-REQ-e912cdd3-0222-4a88-b920-983a4118a6d1 | [六态](#recipe-lifecycle) | 全部转换、独立授权与 blocker | `IG-RECIPE-STATE`：逐条合法/非法 transition、stale evidence/revision、越权 Agent 确认、retired 复活拒绝；API/Viewer/Agent 使用同一状态集；无生产副作用；按 [testing-manual](../../../testing-manual.md)选择对应 required/full 层级 | transition/evidence/authority refs、read projection | module lifecycle 测试不代签 |
| [recipe disposition](../../product/world-rules-core-gameplay/prd.md#root-sc24-recipedispose)；Trace refs: O7-REQ-df2f1a58-f435-4e86-b6c1-810f13f8267d | [六类矩阵](#recipe-work-disposition) | 六态与六工作类型、终端义务独立处置 | `IG-RECIPE-DISPOSE`：逐格证明允许/拒绝；retiring 无 finish 但支持 return/hold 的正例；successor 显式转换；retired 无未决；scope 漂移/重试/恢复单次；required/full 沿原 SC24；按 [testing-manual](../../../testing-manual.md)选择对应 required/full 层级 | 每 bucket 原/后状态、disposition/settlement、消费者 parity | A 工作图通过不代签 B；文档不证明运行 |

## 12. 决策、长期风险与未决问题

采用独立工作图与配方 authority：避免用一个 job 或模块发布状态承载两套不同生命周期；代价是转换必须显式调用图枚举和处置接口。采用不可变 baseline、receipt 和 linked revision：保留历史与恢复因果，不能靠更新当前 profile 修正旧工作。采用原子 cutover 固定选择、后续处置独立执行：兼顾切换一致性与长时 WIP/运输，不把长期流程放进一笔无限事务。

M4/gameplay owner 必须在激活具体 profile 前给出执行单位、fairness/recheck、staged 互等处理、buffer/terminal 容量、窗口、loss/conversion 和 disposition 的实际政策值；未指定时相关模式拒绝，本文不擅自补数值。Runtime/WASM owner 在实现时发布确定编码、存储 schema 和消费者版本；QA 在同候选证明 native/module 与持久化路径一致。任何新 profile、因果分类或 ABI 变化触发受影响条款复核，既有 legacy 能力和历史证据保持原范围。

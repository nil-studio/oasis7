# 小行星碎片场景内容与交付设计

- 主责：`gameplay_designer` 与 Simulator 场景维护者；所属模块为[世界规则与玩法系统](../../product/world-rules-core-gameplay/prd.md)及[智能体、世界模拟与交互](../../product/agents-world-simulation/prd.md)。生成、Runtime、Viewer 与 QA 分别维护原消费者和验证。
- 生命周期：`active`；适用于四个现有碎片场景的 Simulator 初态、内容编辑与交接，不新增地图、规则或正式世界方案。
- 审读基线：`74867d54d92a771d968e91be8ba991abc81e5775`，`2026-11-03`。下列 JSON、默认值和调用关系是固定源码的静态事实，不是生成、部署、试玩或产能验收结果。
- 配对方法：[场景内容制作与验证手册](asteroid-scenario-content-delivery.manual.md#scope)。

<a id="scope"></a>
## 1. 问题、目标与范围

选择碎片场景时，内容作者需要知道其中有哪些地点、主体和初始资源，哪些由文件明确给定，哪些由有效配置生成；消费方需要知道导入后仍保留哪些事实。只看到场景名称或初始化成功，无法判断它是否适合纯碎片观察、多区域比较或玩家开局。

本设计交付 `asteroid_fragment_bootstrap`、`asteroid_fragment_detail_bootstrap`、`asteroid_fragment_twin_region_bootstrap`、`asteroid_fragment_triad_region_bootstrap` 四个已有数据变体的具体目录、有效初态来源、消费者差异和制作验证方法，补充 GD-06 及关联的 GD-07/18/20 子范围。其他八个内置场景继续查[场景文件 PRD 的完整场景矩阵](scenario-files.prd.md)，本文不复制全目录或测试能力表。

场景提供可比较的空间与资源输入。玩家在实际入口选择采集对象、路线或工业目标时，仍须读取当前可见地点、资源来源、权限、成本和目标条件；出生在 frag 附近、场景中有电力或 region 地点，不表示这些选择已可执行或有稳定收益。本文维护制作输入及其解释边界，玩家提示和首局流程继续由原产品与 Viewer 条款承接。

本文不改变生成参数、空间/预算公式、出生算法、材质颜色、工业账本、持久化或权限。正式 canonical seed、starter、业务 record 与消费者绑定继续沿[原四项未决范围](unified-world-seed-fragment-runtime.design.md#unified-decision-profile)。分布品质、不可用开局、可达性、玩家理解和规模化制作仍需实际候选证据。

## 2. 上游要求与约束

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 四份实际场景的身份、内容、源与适用边界可定位 | [场景目录](#scenario-content-catalog) | 场景 JSON、canonical ID 与内容维护者 | 产品层 N/A；理由：本行组织既有内容交付，不增加玩家功能；范围：四场景目录。不能代表全游戏或实际可玩性。 |
| [场景加载合同](scenario-files.prd.md#scenario-loading-contract)；Trace refs: O7-REQ-3cf9f857-8083-4527-9648-95e9425a8448 | 区分内置嵌入与 demo 显式文件的编辑、重建和加载 | [编辑源与导入](#editing-source-and-import) | Simulator 场景解析与实际程序版本 | 仅 SC-1 的初始化来源切片；不证明后续权威事件、交互或生产热加载。 |
| [场景接入配置](chunked-fragment-generation.prd.md#chunk-init-contract) | 使用原 seed positions、bootstrap chunks、出生点触发顺序及有效 spacing | [有效生成输入](#effective-generation-inputs) | Simulator 初始化与分块合同 | 仅原条款关联的 SC-1 初始化切片，不选择正式 starter 或新的分布策略。 |
| [生成场景物理真值](../../product/world-rules-core-gameplay/prd.md#generation-physical-truth)；Trace refs: O7-REQ-b4be4414-7def-4831-8302-f9523e82f944 | 具体材质/组成解释读取真实 profile，表现不改写物理事实 | [组成与数量来源](#material-and-quantity-sources) | 原 fragment 物理模型与表现消费者 | 不重定密度/化学/预算公式，不证明所有参数组合或表现入口。 |
| [来源作用域与数量](../../product/world-rules-core-gameplay/prd.md#source-location-quantity-truth)；Trace refs: O7-REQ-218aeae3-6a52-423a-9b69-9570975edc3c | 区分普通地点、fragment/chunk 预算及初始化库存，不挪用其他来源数量 | [组成与数量来源](#material-and-quantity-sources) | Simulator model 与当前资源读面 | 不建立自然资源到 native 工业材料的到账关系；边界归属与正式消费仍需原验收。 |
| [确定性出生选择](agent-frag-initial-spawn-position.prd.md#fsp-selection-contract)；Trace refs: O7-REQ-4d98352e-f146-4672-b159-b99545708c78 | 说明四场景采用的 frag 优先候选及无候选失败 | [出生内容](#spawn-content-boundary) | `build_world_model` 与场景输入 | 产品适用性 N/A；reason=沿原身份只交接离线初始化候选选择；scope=`build_world_model` 候选排序与 seed 选取，不接收正式 claim、资源 provenance 或玩家推荐理由；相邻产品消费仍须独立核实。 |
| [初始化 standoff 与边界](agent-frag-initial-spawn-position.prd.md#fsp-standoff-contract)；Trace refs: O7-REQ-3bef4fa6-57cd-41f6-baa3-74272ea8adcc | 交接已生成坐标与原边界降级，避免把位置偏移解释成导航能力 | [出生内容](#spawn-content-boundary) | `fragment_spawn_pos`、整数厘米与空间边界 | 产品适用性 N/A；reason=沿原身份只交接初始化整数坐标与边界算法；scope=`fragment_spawn_pos` 上方坐标和既有降级，不含 Viewer 消费保留、一般避障或玩家可辨认结果；相邻产品链不能由坐标代签。 |
| [场景消费入口矩阵](scenario-files.prd.md#场景初始化与消费入口矩阵)；Trace refs: O7-REQ-3cf9f857-8083-4527-9648-95e9425a8448 | 交接实际保留的模型、默认 Agent 与库存覆盖 | [消费者差异](#consumer-content-boundary) | Runtime bootstrap、Viewer seed model | 不以 Simulator 初态或 named scenario 标签证明 Runtime 原样恢复。 |
| [generated-world 来源合同](scenario-files.prd.md#generated-world-provenance-contract)；Trace refs: O7-REQ-3cf9f857-8083-4527-9648-95e9425a8448 | 分开文件存在、内容绑定、固定输入重建与实际消费 | [来源与候选](#provenance-and-candidate) | 原生成器、sidecar 消费与 governed 工件 owner | 当前存在检查不证明来源绑定；不把 sidecar 作为正式激活或链资源证明。 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 内容/默认值变化传到消费者、产物与适用验证 | [变更交接](#content-change-handoff) | 内容作者、生成/消费维护者与 QA | 产品层 N/A；理由：本行维护制作依赖；范围：场景内容交接。不能据此改变现存世界。 |
| [规模化内容生产证据](../../engineering/doc-governance/product-documentation-standard.design.md#content-scale-evidence) | 区分现有变体数、实际制作成本和扩大范围的估算 | [制作范围与成本](#production-scope-and-cost) | 实际制作、导入与验证记录 | 产品层 N/A；理由：本行规范制作能力声明；范围：场景内容。未声明工期、产能或预算完成。 |

### 2.2 原技术职责与产品链边界

[场景加载设计](scenario-files.design.md#scenario-loading-design)拥有字段、默认值覆盖、内置重编译与消费入口合同；[来源绑定设计](scenario-files.design.md#generated-world-provenance-design)拥有 provenance 缺口和目标验收；[分块既有义务](chunked-fragment-generation.design.md#chunk-generation-existing-obligations)拥有物理、预算、触发及可重现边界。本文只交接它们生成或消费的内容。

出生的两个工程身份沿用[原配对设计](agent-frag-initial-spawn-position.design.md)已经声明的有限产品 N/A：候选选择、整数坐标与边界算法。需要玩家启动或推荐时，分别回到[首次认领生成来源](../../product/agents-world-simulation/prd.md#first-claim-generated-context)、[FS-7 条件理由](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#fs7-starter-resource-reason)及[Viewer 保留位置](agent-frag-initial-spawn-position.design.md#fsp-viewer-preservation-design)核对真实产品链；本文不补造产品 AC，也不把这些相邻玩家义务纳入工程 N/A。物理真值、数量作用域与 SC-1 的接收也只覆盖表中切片，原功能状态和组合验收保持。

## 3. 场景内容、边界与结构

<a id="scenario-content-catalog"></a>
### 3.1 四个已有场景的具体内容

下表是固定基线的源数据摘要。Electricity/Data 表示 Simulator `ResourceStock` 的初值；空库存及没有设施由有效默认值补齐。实际地点坐标、frag 列表和总数量须从同候选初始模型取得。

| canonical ID 与编辑源 | seed / fragment offset | 显式 bootstrap chunks；额外 spacing | 普通地点、Agent 与显式库存 | 选用目的与未提供内容 |
| --- | --- | --- | --- | --- |
| [asteroid_fragment_bootstrap](../../../crates/oasis7_simulator/scenarios/asteroid_fragment_bootstrap.json) | `9 / 77`；派生 fragment 根 `86` | `(2,2,0)`；`50_000 cm` | `origin` 保留，Data=10；`agent-0`，Electricity=15；无设施 | 单 Agent 的碎片/预算初态基线；没有预先选定的矿点、完整首工业投入或目标路线。 |
| [asteroid_fragment_detail_bootstrap](../../../crates/oasis7_simulator/scenarios/asteroid_fragment_detail_bootstrap.json) | `42 / 111`；派生 fragment 根 `153` | `(1,1,0)`、`(1,2,0)`、`(2,1,0)`、`(2,2,0)`；`0` | 无 origin、无普通生成地点、Agent=0；无设施 | 四 chunk 纯碎片观察输入；关闭额外 spacing，不表示关闭全部几何约束或已有 Web 纯碎片入口。 |
| [asteroid_fragment_twin_region_bootstrap](../../../crates/oasis7_simulator/scenarios/asteroid_fragment_twin_region_bootstrap.json) | `10 / 101`；派生 fragment 根 `111` | `(1,2,0)`、`(3,2,0)`；`50_000 cm` | 无 origin；`region-0/1`；`agent-0/1` 各 Electricity=20；无设施 | 双区域与碎片的组合输入；没有阵营分配、专属矿种、隐式工厂或运输完成事实。 |
| [asteroid_fragment_triad_region_bootstrap](../../../crates/oasis7_simulator/scenarios/asteroid_fragment_triad_region_bootstrap.json) | `11 / 131`；派生 fragment 根 `142` | `(1,2,0)`、`(3,2,0)`、`(2,3,0)`；`50_000 cm` | 无 origin；`region-0/1/2`；`agent-0/1/2` 各 Electricity=20；无设施 | 三个普通区域地点与碎片的组合输入；不能由场景名称推出合作、竞争或三条资源路线已验证。 |

`region-*` 的显示名是 `Region 0/1/2`，位置由 seed、序号和有效空间生成；它们采用[普通地点默认 profile](../../../crates/oasis7_simulator/src/types.rs)：Silicate、半径 `100 cm`、辐射 `0`，库存为空。名称和默认材质不授予阵营、所有权、可采量或工业资格。`origin` 未指定位置时取有效空间中心；Agent 前缀与起始序号来自[初始化默认值](../../../crates/oasis7_simulator/src/init.rs)。

四个文件均无 `power_plants` 或 `module_visual_entities` 声明。设施仍须按[显式设施合同](world-initialization.prd.md#init-explicit-facilities)提供；Agent 电力库存、fragment 辐射和发电设施分别解释，不相互替代。

<a id="effective-generation-inputs"></a>
### 3.2 有效生成输入与空间结构

场景 JSON 不是完整生成配置。输入组合为固定程序版本、场景 spec、有效 `WorldConfig`、初始化配置及 chunk 触发顺序。[空间常量](../../../crates/oasis7_agent_api/src/world_geometry.rs)在当前默认下形成 `100 km × 100 km × 10 km`、`5 × 5 × 1` 个索引 chunk；固定分块为 `20 km × 20 km × 10 km`。索引存在不表示其中已有碎片。

[scenario.rs](../../../crates/oasis7_simulator/src/scenario.rs)先按 spec seed 生成普通地点，再形成初始化配置；[init.rs](../../../crates/oasis7_simulator/src/init.rs)依次生成普通地点所在 `seed_positions`、显式 bootstrap chunks 和 Agent 出生坐标所在 chunks。bootstrap 列表会排序去重；该列表只是显式预生成输入，**不能当作最终生成 chunk 数量或全部生成顺序**。有 origin 或 region 的场景尤其需要核对实际模型。

fragment 根来自场景 seed 与 offset 的 `wrapping_add`，再按 chunk coord 派生；普通地点和 Agent 选择仍使用有效场景 seed。修改 seed 必须发生在 spec 转换前，不能先生成普通地点再只改 `init.seed`；显式 `WorldInitConfig.locations` 的坐标行为沿[有效 seed 合同](scenario-seed-locations.prd.md#location-effective-seed)。

生成器复制 `WorldConfig.asteroid_fragment`，只应用场景显式支持的 override；这四个文件均以 `min_fragment_spacing_cm` 覆盖额外 spacing，其他生成参数沿[有效默认值源](../../../crates/oasis7_simulator/src/world_model_config.rs)。当前均采用 `uniform`；代码支持另一种分布策略不表示场景已采用它。保底、starter core、间距与预算上限继续按[原生成合同](chunked-fragment-generation.prd.md#chunk-floor-contract)，不保证固定数量、品类齐全、首局收益或工业可行性。

<a id="material-and-quantity-sources"></a>
### 3.3 材质组成、资源数量与解释边界

四场景复用五种材质。下表只说明当前[material_compounds](../../../crates/oasis7_simulator/src/fragment_physics.rs)的内容关系，精确 ppm、密度、元素映射与恢复率留在原来源；实际片段还经过 profile 合成、块预算截断及预算计算。

| 材质 | 当前组成方向 | 内容交接时应保留的区别 |
| --- | --- | --- |
| Silicate | 硅酸盐基质为主，含水矿物、硫化物矿、稀土氧化物与水冰参与 | 普通 region 的同名默认材质不等于已有这一 fragment profile 或预算。 |
| Metal | 铁镍合金为主，伴随硫化物矿、硅酸盐及铀/钍矿相 | 材质名不能直接换算成 native `iron_ore` 库存或首配方收益。 |
| Ice | 水冰为主，伴随含水矿物、碳质有机物和硅酸盐 | 实际可采种类及数量取当前预算，不从图像或名称补数。 |
| Carbon | 碳质有机物为主，伴随硫化物矿、含水矿物、水冰和稀土氧化物 | 不能据此声明已取得工业燃料。 |
| Composite | 硅酸盐、铁镍、碳质、含水矿物及少量稀土/铀/钍的组合 | 多种组成不保证某一资源必然足够或某个配方已解锁。 |

当前 uniform 权重在原配置中为 `52/8/18/18/4`，顺序为 Silicate/Metal/Ice/Carbon/Composite；这是采样输入，不是单次世界的实测比例。辐射还受尺寸、材质因素与生成配置影响；内容作者应引用实际 profile，不能仅按材质名排出安全路线或风险收益等级。

fragment ID 形如 `frag-{x}-{y}-{z}-{idx}`，由生成路径分配；缺号可能来自候选过滤，目录不能补出不存在的实体。`Location.fragment_profile` 描述组成，`fragment_budget` 与 `chunk_resource_budgets` 提供各自作用域的 total/remaining；`Location.resources` 和 `Agent.resources` 是其他初始库存。[生成即定量合同](chunked-fragment-generation.prd.md#chunk-budget-contract)继续拥有数值与守恒，目录不临时重算当前 remaining。自然资源、legacy Data 与 native 工业材料的区别沿[现有账本边界](chunked-fragment-generation.prd.md#legacy-simulator-refining-ledger)及[工业初态来源](../../game/gameplay/industrial-native-content-delivery.design.md#starter-and-initial-state-applicability)。

<a id="spawn-content-boundary"></a>
### 3.4 出生内容与不可用初态

有 Agent 的三个场景沿 `build_world_model` 收集并排序 `frag-*` 候选，以有效 seed 和 Agent 序号选取；没有 frag 时沿既有普通地点候选，允许多个 Agent 选中同一地点。全部候选为空且 Agent 数大于零时返回 `SpawnLocationMissing`。detail 的 Agent 数为零，因此无需为它虚构出生或玩家第一步。

[frag_spawn.rs](../../../crates/oasis7_simulator/src/frag_spawn.rs)使用 frag 的 `LocationProfile.radius_cm`，保持 x/y、向上增加半径加 `5000 cm`；越界则尝试半径加 `2000 cm`，仍越界返回中心。交接同时保留 `location_id` 和实际 Agent 坐标，不能用地点归属替代站位，也不能把这一选择写成最近、最安全或最优矿点。

初始化成功只说明本次模型通过相关构建校验。生成数量过少、缺某种资源、出生回退中心、没有目标所需合法路线或工业前置，需要按实际观察记录；当前没有把这些情况统一转换为“可玩开局通过”的质量门槛。出现初始化错误应修正实际输入/实现并重验，不交付部分模型；既有测试没有覆盖的 fallback、全部 seed 分布和玩家不可用开局仍属剩余验证。

<a id="consumer-content-boundary"></a>
### 3.5 同一场景在不同消费者中的内容

| 消费入口与当前源 | 保留或改变的内容 | 可作出的交付判断 |
| --- | --- | --- |
| Simulator `build_world_model` / `initialize_kernel`；[init demo](../../../crates/oasis7_simulator/src/bin/oasis7_init_demo.rs) | 按场景生成 origin、普通地点、frag、Agent 和显式设施；kernel 在时间 0 保留初始模型及适用 Init 事件 | 可核四场景的 Simulator 初态；demo 文本摘要不包含全部位置/profile/预算。 |
| named runtime-live；[bootstrap_runtime_world_with_chunk_runtime](../../../crates/oasis7/src/runtime_host/bootstrap.rs)与[入口分流](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/support.rs) | 从模型投影 Runtime；返回路径不保留完整 seed model | 适合其 Runtime 调试用途，不能据场景标签宣称完整模型原样保留。 |
| runtime-live `--generated-world-dir`；同 bootstrap 源 | 加载 sidecar snapshot/config/model/chunk runtime，并保留 seed model 参与 Viewer 映射；Agent 仍由 Runtime 投影 | 能验证被实际保留的地点/profile/引用，不自动保留零 Agent 或完整原始库存。 |
| 本地无场景默认入口；[formal_release_default_seed_model](../../../crates/oasis7/src/runtime_host/gameplay_actions.rs) | 程序内构造 seed `0x0A51_5700_0000_0001`、bootstrap `(0,0,0)`、关闭 origin 与一个 starter Agent，不读取这四份 JSON | 是另一份已采用本地输入；函数名不证明正式 starter 或 chain provenance 已选。 |
| world-service / 正式世界激活 | [world-service 分流](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/support.rs)等待权威读取；[正式初始化](../../../crates/oasis7_chain_runtime/src/host/world_initialization.rs)接外部准备材料 | 以实际权威 snapshot/配置为源，不把本目录当正式世界初态。 |

named runtime 和 generated sidecar 共用 `bootstrap_runtime_world_from_model`：模型无 Agent 时补入 `runtime-agent-0/1`；每个 Runtime Agent 的 Electricity/Data 至少为 `32/8`，低于该值的场景初值会被提高；另有固定 world 库存与工业准备。因此 detail 的零 Agent 以及其他场景的 `15/20` 电力只描述 Simulator 初态。工业账本、owner/site/profile 与首局可行性继续由[工业内容设计](../../game/gameplay/industrial-native-content-delivery.design.md#starter-and-initial-state-applicability)维护。

<a id="provenance-and-candidate"></a>
### 3.6 来源、生成产物与实际候选

[现有生成器](../../../crates/oasis7_ops_tools/src/bin/oasis7_generate_world_artifact.rs)消费内置场景和默认 WorldConfig，保存初始化 kernel，并输出 `artifact_kind`、`scenario_id`、`seed`、场景 spec `config`、public manifest 摘要和条目数。spec 不包含全部继承默认值，manifest 摘要也不是 sidecar 内容摘要。

当前 Runtime bootstrap 对 provenance 只执行 `is_file()`，随后加载 `generated-scenario-world`。CLI 另检查所需 snapshot/journal 文件存在，但没有因此完成 provenance 内容绑定。已有[sidecar 测试](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/snapshot_generated_world.rs)使用场景 seed=9、provenance seed=42 的 fixture，断言映射而非来源一致。交接应分别报告“文件可取得”“模型已载入”“内容来源已绑定”“同输入独立重建一致”，不能将前三者合并为当前通过结论。

组合验收固定实际程序/场景文件/有效默认配置/触发顺序、输出 snapshot/journal、manifest 和目标入口；原证据保留其原版本。generated sidecar 与 audited governed world 的分离沿[已采用 artifact 设计](unified-world-seed-fragment-runtime.design.md#artifact-separation-design)，不能用 sidecar 覆盖正式材料或用目录存在证明消费者已绑定。

## 4. 制作规格与交付物

<a id="editing-source-and-import"></a>
### 4.1 编辑源、格式与导入

四个 JSON 是可编辑源，canonical ID 与文件名保持对应，原 `name` 不在本文另立译名。顶层字段由 `WorldScenarioSpec` 解析；普通地点仅用既有 `location_generator`，origin 可用原位置表达，设施/视觉实体沿原 schema。复杂区域 DSL、任意矿种强制布置、sub-location 坐标或新出生政策需要原专业设计与实现，不靠在 JSON 增加未知字段获得能力。

内置场景通过 `include_str!` 进入程序，改 JSON 后重新编译实际消费者；`oasis7_init_demo --scenario-file` 在初始化时读取显式 JSON，改外部数据后重新运行即可。当前 artifact CLI 只接受已注册的 `--scenario`，不接受任意 `--scenario-file`。具体操作、来源固定与模型核对见[导入方法](asteroid-scenario-content-delivery.manual.md#asc-import)。现存世界不因源文件变动而迁移；需要新产物时使用独立输出位置，旧 snapshot/journal 按原恢复合同处理。

来源与版本沿 Git 和实际产物记录；仓库自有场景/文档的许可入口为 [LICENSE](../../../LICENSE)。本专题没有新导入外部素材或导出二进制，不能据此宣称全部依赖的再分发权利已审清。需要视听输出时复用[世界视觉内容目录](../viewer/viewer-world-visual-content-delivery.design.md#visual-content-catalog)，其源、表现与目标环境验证另行交接。

<a id="content-change-handoff"></a>
### 4.2 变更传播与交接样例

| 改动 | 必须核对的依赖和产物 | 可以保留的边界 |
| --- | --- | --- |
| 本目录的说明或链接 | 实际 JSON/源与本次解释，受影响导航 | 无数据/消费变化时不生成新世界或补造运行结果。 |
| 场景 seed、offset、origin、region/Agent 数、spacing 或 bootstrap | 转换前有效输入、普通地点、触发顺序、frag/profile/预算、Agent 站位；嵌入消费者与实际 sidecar | 只有核实不受影响的源/结论才能继续复用；相同 ID 不保证相同内容。 |
| 共用默认值、生成/profile 算法或模型 schema | 所有实际使用该默认值/算法的场景和消费者，重建与恢复兼容范围 | 本文四场景不能代替共享变更的完整影响分析；其他场景仍按原总矩阵检查。 |
| 目标消费者或生成产物替换 | Runtime 补 Agent/库存、seed model 保留、来源验证、权威入口与旧状态 | 文件可读不等于原样保留；不把新初态写入已有世界历史。 |

**具体样例：获准调整 twin 场景的 seed。** 在其 JSON spec 中改变 seed，保留原版本；`region-0/1` 的身份仍在，但生成位置需重取；fragment 根随之改变，普通地点所触发 chunks、各 frag/profile/budget 及 Agent 候选/坐标均可能变化。显式 bootstrap 列表和 spacing 未改仍不能推出最终地图不变。作者交接新有效输入；生成者重建选定产物；消费者按实际入口检查内容保留和 Runtime 覆盖；QA 使用新候选重核适用初态、来源与体验证据。不能组合新 JSON、旧 sidecar 和旧截图为同一次验收。

若只改变 spacing，普通地点生成输入可不变，但 fragment 接受/过滤、预算和 Agent 候选仍须重核；不得据相同 seed 复用旧出生或资源结论。以上是传播方法，不是已执行的内容变更。多人编辑沿原[单一写入与集成规则](../../engineering/workflow/source-of-truth.md#parallel-task-development)：同一 JSON/共享默认值由明确维护者整合，交接包含受影响源、关联文档、消费版本与实际验证，冲突后重核组合而非按最后一次保存覆盖。

<a id="production-scope-and-cost"></a>
### 4.3 制作范围与成本

当前范围为四份现有 JSON、共用生成器与五类既有材质组成；这些数量是可查内容，不是四个经过质量验收的完整可玩世界。没有手工地图、额外模型或固定矿点导出物需要本轮制造。

后续实际制作按变化选择代表性样本：单 Agent 场景核基础内容，detail 核零主体/四 chunk 保留，twin 或 triad 核普通地点与 fragment 的组合依赖；不要求每次运行全部样本。制作记录区分首次工具/格式准备、源编辑、重建/导入、专业审读、目标环境验证、返工人时和等待日历时间。相同格式的 seed 变体成本不能外推为新算法、正式世界集成或玩家体验设计成本。

当前没有代表性重复制作、规格变化传播和目标环境验收的实测记录，因此未给出产能、工期或预算结论。扩大制作范围前沿[测量方法](asteroid-scenario-content-delivery.manual.md#asc-production)报告实际复杂度、成本区间及未覆盖范围。

## 11. 验证与验收

### 11.1 验证映射表

以下分配方法与证据去向。源/字段检查、专业内容审读、真实模型及目标入口运行分别记录；测试定义存在不表示本候选运行通过。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [场景目录](#scenario-content-catalog) | 四文件身份、显式内容和默认来源可核对 | [ASC-CATALOG](asteroid-scenario-content-delivery.manual.md#asc-catalog)；固定 source revision 的只读 JSON/文本审读 | 原 PR 的源版本、目录输出与逐项结论 | 不证明世界已生成或内容品质。 |
| [场景加载合同](scenario-files.prd.md#scenario-loading-contract)；Trace refs: O7-REQ-3cf9f857-8083-4527-9648-95e9425a8448 | [编辑源与导入](#editing-source-and-import) | 内置/外部文件/工件生成的加载方式真实 | [ASC-IMPORT](asteroid-scenario-content-delivery.manual.md#asc-import)；固定实际入口、程序与输入 | 原制作 PR 的命令、产物和实际结果 | 文档方法不代表生成、热加载或正式激活通过。 |
| [场景接入配置](chunked-fragment-generation.prd.md#chunk-init-contract) | [有效生成输入](#effective-generation-inputs) | bootstrap 与实际生成范围、有效默认值和触发顺序分清 | [ASC-EFFECTIVE-INPUT](asteroid-scenario-content-delivery.manual.md#asc-effective-input)；同候选源与适用初始模型 | 原 QA 的有效输入、模型/summary 对账 | JSON 检查不证明全部 seed、历史 replay 或正式 starter。 |
| [生成场景物理真值](../../product/world-rules-core-gameplay/prd.md#generation-physical-truth)；Trace refs: O7-REQ-b4be4414-7def-4831-8302-f9523e82f944 | [组成与数量来源](#material-and-quantity-sources) | 材质解释与实际 profile 的组成来源一致 | [ASC-MATERIAL](asteroid-scenario-content-delivery.manual.md#asc-material)；源审读及选定模型检查 | 原 PR/QA 的 profile、来源与差异 | 不证明全部物理参数或视觉品质。 |
| [来源作用域与数量](../../product/world-rules-core-gameplay/prd.md#source-location-quantity-truth)；Trace refs: O7-REQ-218aeae3-6a52-423a-9b69-9570975edc3c | [组成与数量来源](#material-and-quantity-sources) | 普通地点、片段/区块预算与库存不混用 | [ASC-QUANTITY](asteroid-scenario-content-delivery.manual.md#asc-quantity)；固定 snapshot/资源作用域 | 原 QA 的地点、所属账本与原始数量 | 不证明正式可采权、工业到账或全部边界守恒。 |
| [确定性出生选择](agent-frag-initial-spawn-position.prd.md#fsp-selection-contract)；Trace refs: O7-REQ-4d98352e-f146-4672-b159-b99545708c78 | [出生内容](#spawn-content-boundary) | 有 frag 优先、无候选失败及所用输入可查 | [ASC-SPAWN](asteroid-scenario-content-delivery.manual.md#asc-spawn)；同候选 Simulator 层 | 原测试/模型的候选、Agent/ref/坐标及输出 | 当前单个 frag 前缀断言不证明全部确定性、正式 claim 或推荐理由。 |
| [初始化 standoff 与边界](agent-frag-initial-spawn-position.prd.md#fsp-standoff-contract)；Trace refs: O7-REQ-3bef4fa6-57cd-41f6-baa3-74272ea8adcc | [出生内容](#spawn-content-boundary) | 坐标与边界降级沿原合同解释 | [ASC-SPAWN](asteroid-scenario-content-delivery.manual.md#asc-spawn)；固定空间、半径和输入 | 原 QA 的位置/分支与真实结果 | 区间测试不覆盖两个 fallback 全部分支或 Viewer 保留。 |
| [场景消费入口矩阵](scenario-files.prd.md#场景初始化与消费入口矩阵)；Trace refs: O7-REQ-3cf9f857-8083-4527-9648-95e9425a8448 | [消费者差异](#consumer-content-boundary) | 场景原值和实际消费值逐项区分 | [ASC-CONSUMER](asteroid-scenario-content-delivery.manual.md#asc-consumer)；指定真实入口及 snapshot | 原运行记录的源模型与消费模型对比 | sidecar 映射不证明所有场景原样保留或可玩。 |
| [generated-world 来源合同](scenario-files.prd.md#generated-world-provenance-contract)；Trace refs: O7-REQ-3cf9f857-8083-4527-9648-95e9425a8448 | [来源与候选](#provenance-and-candidate) | 存在、绑定、重建与正式身份结论分别成立 | [ASC-PROVENANCE](asteroid-scenario-content-delivery.manual.md#asc-provenance)；同次工件/输入/消费记录 | 原制作/QA 记录的来源链与未验证项 | 当前加载器与 fixture 不证明绑定、重建或正式资源承载。 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [变更交接](#content-change-handoff) | 一项实际输入变化传到受影响产物与验证 | [ASC-CHANGE](asteroid-scenario-content-delivery.manual.md#asc-change)；实际变更及其候选 | 原 PR 的依赖核对、交接和重验 | 纸面样例不证明制作试验或已有世界迁移。 |
| [规模化内容生产证据](../../engineering/doc-governance/product-documentation-standard.design.md#content-scale-evidence) | [制作范围与成本](#production-scope-and-cost) | 内容数量、重复制作与估算依据分开 | [ASC-PRODUCTION](asteroid-scenario-content-delivery.manual.md#asc-production)；适用实际样本 | 原制作记录的成本、等待、返工与覆盖边界 | 尚无实测产能，不能外推全游戏预算。 |

<a id="decisions-and-risks"></a>
## 12. 决策、风险与未决范围

选择四份已有碎片场景作为独立内容类型，是因为它们共享输入模型与消费者，并有单主体、零主体和多区域的实际差异；完整场景矩阵、原数值算法及相邻工业/视觉设计继续各有一个主责来源。

本轮能交付源可定位、解释一致和方法具体的设计。实际生成数量/分布、不可用开局比例、边界与导航、原料到工业输入、Viewer 保留与玩家理解、正式链资源和规模化制作仍需各自候选验收。region 名称、uniform 权重、保底和本地默认常量不能替这些领域作决定。

generated-world 只做存在检查、零 Agent 在 Runtime 被补入、资源最低值覆盖，是交接时必须明确的现行差异。CG6 重建与 CG7 生成/补种 delta 的[原回放冲突](chunked-fragment-generation.design.md#chunk-replay-conflict-register)仍待原 owner 裁决；本内容设计不改写历史恢复规则，也不以重新生成初始化世界替代原世界恢复。

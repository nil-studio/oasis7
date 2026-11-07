# Native 工业内容目录与制作交接设计

- 主责：`gameplay_designer`；所属模块为[世界规则与玩法系统](../../product/world-rules-core-gameplay/prd.md)。Runtime、WASM 和场景 owner 继续拥有执行、构建与初始化合同。
- 生命周期：`active`。本文拥有当前 native 工业内容的编辑目录、来源关系和制作交接；不新增配方、工厂或解锁规则。
- 审读基线：`84d4feb5314257c09612711b71346afe1481a117`，`2026-11-03`。数量及源码关系是该基线的静态事实，不是当前世界、部署或生产验收结果。
- 配对方法：[内容制作与验证手册](industrial-native-content-delivery.manual.md)。玩家表达继续由[首局内容设计](../../world-simulator/viewer/first-session-content-delivery.design.md#content-units)承接。

## 1. 问题、目标与范围

玩家看到“生产铁锭，准备评估 Assembler”时，制作方需要知道所指的是哪份配方、哪些输入和副产物、哪种工厂，以及当前世界从哪里取得这些前置。现有 native 目录、WASM 定义、bootstrap 和首局合同各自承担一部分；把文件名或材料同名直接连成“已可生产的完整工业链”，会遗漏账本、授权与实际初态。

本设计交付 **2 类 native 工厂、11 个 native 配方**的有界目录、内容引用关系、初态适用说明、变更交接和制作范围测量方法，支撑 GD-07/18/20 的当前工业子范围。目录是源码的可读索引；数值、权威身份、执行准入与结算仍读取原来源。全游戏物品、所有 WASM 模块、地图、叙事、资产量产和全项目预算不由这个数量代表。

本专题不改变运行状态、协议、存储或权限。相关技术义务依赖 §2.2 的现行设计；文档和静态检查可以交付内容制作输入，实际组合、工件构建和目标环境可用性另按受影响实现验收。

## 2. 上游约束与职责

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 给出当前 native 内容的稳定身份、数量、引用和源文件 | [内容目录](#native-content-catalog) | gameplay / agent API canonical 来源 | 产品层 N/A；理由：本行组织已有内容，不新增玩家功能；范围：native 目录与来源。不是全游戏库存或已解锁清单。 |
| [REQ-FIRST-INDUSTRIAL-001](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-001)；Trace refs: O7-REQ-68214dc7-c00b-4c0d-8c3b-c1c24fb52502 | 当前首局候选必须使用实际初态与 fresh authority，不能由目录推出可行 | [初态适用关系](#starter-and-initial-state-applicability) | Runtime bootstrap、starter feasibility、场景 owner | 不为全部世界或配置提供可行性证明 |
| [REQ-FIRST-INDUSTRIAL-001](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-001)；Trace refs: O7-REQ-68214dc7-c00b-4c0d-8c3b-c1c24fb52502 | 推荐前从同一fresh初态证明当前profile完整有限路径，预算逐步累计且只读 | [前向组合Gate](#starter-fresh-forward-gate) | Runtime共享评估者、M4原准入/完成转换、入口能力 | 本节是目标合同，不把单步quote或现有milestone投影当完整Gate已实现。 |
| [REQ-FIRST-INDUSTRIAL-003](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-003)；Trace refs: O7-REQ-d4944a59-ccf3-4f96-95ac-45c0ca50cf51 | 按真实profile判定来源/handoff/物流适用性；当前直接输入不能伪造前置，真实未知不能跳过 | [当前来源边界](#starter-canonical-source-boundary) | M4原来源、ledger与运输authority | 仅核当前已采纳canonical路径；未来选定条件来源链需独立专业接线与验收。 |
| [REQ-FIRST-INDUSTRIAL-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-002)；Trace refs: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95 | 制作方能追踪工厂、配方输入和实际产物的关系，按 profile 纳入真实前置 | [内容依赖与消费](#content-dependency-and-consumption) | M4 同方案报价；gameplay 完成边界 | 关系图不提供物流、解锁或自动排程能力 |
| [REQ-FIRST-INDUSTRIAL-006](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-006)；Trace refs: O7-REQ-d0041aa9-1dfb-46a0-95e0-67417b39bfa3 | 初态、账本或 authority 缺失时保留具体 blocker 和复查条件 | [阻塞处置](#content-blocker-boundary) | gameplay / Runtime 的当前有效查询与动作 | 不补发资源、不替换配方、不发明跨账本转移 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 内容变更有实际源、消费者、版本/权利和交接路径 | [变更交接](#content-change-handoff) | Runtime / WASM / 场景 / Viewer 原 owner | 产品层 N/A；理由：本行规范制作交接；范围：当前 native 内容变更。不授予模块激活或发行权限。 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 以已知内容规模和代表性复杂度组织制作测量，区分事实、实测与估算 | [制作测量](#production-measurement) | 制作方、实际消费者与 QA 的原记录 | 产品层 N/A；理由：本行规范制作范围测量；范围：native 工业内容。未声明工期、人员产能、外包额或全项目预算。 |

### 2.2 原专业 authority 与技术依赖

| 原主责条款 | 本内容的消费边界 |
| --- | --- |
| [M4 同方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding) | factory/recipe/batches、有效消费账本、权限和预览/提交由同一方案确定；目录不计算另一份报价。 |
| [首产物完成合同](gameplay-industrial-starter-completion-contract.prd.md#4-currenttarget-evidence-cutline) | 当前 canonical profile、匹配生产结算、持久 milestone 与 Assembler 候选的规则；首产物仍为 produced/undelivered。 |
| [配方生命周期](gameplay-industrial-recipe-lifecycle-decision-contract.prd.md#1-currenttarget-evidence-cutline) | 文件存在不能推出 active、restricted、retired 或当前可排程；内容版本变化不能静默改绑旧工作。 |
| [WASM 版本化源码输入](../../world-runtime/wasm/wasm-deterministic-build-pipeline.design.md#build-source-input-rule)及[工件身份](../../p2p/consensus/builtin-wasm-identity-consensus.design.md#builtin-artifact-version-binding) | Rust 源文件、共享模板、构建身份、WASM 字节与世界激活分别定位；本专题不另算 source closure 或生成平行 manifest。 |
| [场景消费矩阵](../../world-simulator/scenario/scenario-files.prd.md#场景初始化与消费入口矩阵) | Simulator 模型、named runtime-live 投影和 generated-world 消费分别解释，JSON 解析成功不证明同一初态。 |

## 3. 内容结构与适用边界

<a id="native-content-catalog"></a>
### 3.1 当前 native 内容目录

工厂规则来源为 [factory_catalog.rs](../../../crates/oasis7_agent_api/src/world_quote/factory_catalog.rs)。`builtin_factory_spec(factory_kind, factory_id)` 先按 kind 选择规格，再把实例 `factory_id` 写为调用方传入值。下表统计 **kind**，不是世界中的工厂实例数量；当前 starter 恰好使用与 kind 同字面的实例 ID，不能据此将二者合并。

| factory kind | 源码显示名 / tags | 建造所需材料种类 | 对应 WASM 定义源 |
| --- | --- | --- | --- |
| `factory.smelter.mk1` | Smelter MK1 / `smelter`、`thermal` | `structural_frame`、`heat_coil`、`refractory_brick` | [m4_factory_smelter_mk1](../../../crates/oasis7_builtin_wasm_modules/m4_factory_smelter_mk1/src/lib.rs) |
| `factory.assembler.mk1` | Assembler MK1 / `assembler`、`precision` | `structural_frame`、`iron_ingot`、`copper_wire` | [m4_factory_assembler_mk1](../../../crates/oasis7_builtin_wasm_modules/m4_factory_assembler_mk1/src/lib.rs) |

配方规则来源为 [recipe_catalog.rs](../../../crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs)。前四项的对应 WASM 定义要求 `smelter` marker，后七项要求 `assembler` marker；实际工厂匹配和准入仍由所用 execution lane 的 authority 校验。表中“无”仅指该定义的副产物列表为空。

| recipe ID | 输入种类 | 主产物 | 副产物 | 对应 WASM 定义源 |
| --- | --- | --- | --- | --- |
| `recipe.smelter.iron_ingot` | `iron_ore`、`carbon_fuel` | `iron_ingot` | `slag` | [m4_recipe_smelter_iron_ingot](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_smelter_iron_ingot/src/lib.rs) |
| `recipe.smelter.copper_wire` | `copper_ore` | `copper_wire` | 无 | [m4_recipe_smelter_copper_wire](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_smelter_copper_wire/src/lib.rs) |
| `recipe.smelter.polymer_resin` | `carbon_fuel`、`silicate_ore` | `polymer_resin` | `waste_resin` | [m4_recipe_smelter_polymer_resin](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_smelter_polymer_resin/src/lib.rs) |
| `recipe.smelter.alloy_plate` | `iron_ingot`、`copper_wire` | `alloy_plate` | `slag` | [m4_recipe_smelter_alloy_plate](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_smelter_alloy_plate/src/lib.rs) |
| `recipe.assembler.gear` | `iron_ingot` | `gear` | 无 | [m4_recipe_assembler_gear](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_assembler_gear/src/lib.rs) |
| `recipe.assembler.control_chip` | `copper_wire`、`polymer_resin` | `control_chip` | `waste_resin` | [m4_recipe_assembler_control_chip](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_assembler_control_chip/src/lib.rs) |
| `recipe.assembler.motor_mk1` | `gear`、`copper_wire` | `motor_mk1` | 无 | [m4_recipe_assembler_motor_mk1](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_assembler_motor_mk1/src/lib.rs) |
| `recipe.assembler.logistics_drone` | `motor_mk1`、`control_chip`、`iron_ingot` | `logistics_drone` | `assembly_scrap` | [m4_recipe_assembler_logistics_drone](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_assembler_logistics_drone/src/lib.rs) |
| `recipe.assembler.sensor_pack` | `control_chip`、`copper_wire` | `sensor_pack` | `calibration_scrap` | [m4_recipe_assembler_sensor_pack](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_assembler_sensor_pack/src/lib.rs) |
| `recipe.assembler.module_rack` | `sensor_pack`、`control_chip` | `module_rack` | `precision_scrap` | [m4_recipe_assembler_module_rack](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_assembler_module_rack/src/lib.rs) |
| `recipe.assembler.factory_core` | `module_rack`、`alloy_plate` | `factory_core` | `structural_waste` | [m4_recipe_assembler_factory_core](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_assembler_factory_core/src/lib.rs) |

每个 recipe ID 在此目录是一项内容；批次是该项的参数，不另算配方变体。native 选择要求精确 ID 和正批次，拒绝别名、大小写变体和前后空格；[duration helper](../../../crates/oasis7_agent_api/src/world_quote/duration.rs)为其他消费者兼容旧 ID，不扩大 native catalog 的选择范围。输入、主/副产物数量和功耗随批次缩放；基础时长沿 helper，最终报价可能受执行规则影响，不能把所有字段统一乘批次。

工厂的建造成本、基础用电和 construction power profile 分属实际字段，不能互相代替；WASM 工厂模块的判定常量也不等于 native factory spec 的全部字段。两者同名不是逐字段等价证明。native 的 2/11 不是全部 WASM 内容：[Miner MK1 定义](../../../crates/oasis7_builtin_wasm_modules/m4_factory_miner_mk1/src/lib.rs)就在本目录之外。trusted/custom 与 module-backed plan 保留原执行路径，不由此表转成 native 选择。

<a id="content-dependency-and-consumption"></a>
### 3.2 内容依赖与实际消费者

本目录提供的直接引用可用于检查内容链：铁锭进入 gear 和 alloy_plate，并用于 Assembler 建造；铜线同时进入多种 Assembler 配方及建造；polymer_resin 与铜线进入 control_chip；factory_core 经 module_rack、sensor_pack、control_chip、alloy_plate 回溯到 Smelter 产物。`iron_ore`、`copper_ore`、`carbon_fuel`、`silicate_ore` 和 Smelter 的建造材料没有在这 11 项内定义生产配方，来源须在所选世界的合法初态或后续来源系统中证明。

材料同名只建立内容引用。它不证明货物在正确账本、同 owner、已结算、可运输，或能被当前目的地消费。副产物是完整方案的一部分，内容表、成本说明和集成验证都不能因其不在主链而省略；也不为它们自动添加回收收益、处置动作或终端能力。

| 实际消费者 | 使用的源与交接检查 |
| --- | --- |
| [native recipe quote](../../../crates/oasis7_runtime/src/world/builtin_recipe_quote.rs)与[选择校验](../../../crates/oasis7_agent_api/src/world_quote/recipe.rs) | 从 canonical ID/batches 构造 plan；私有 world clone 复用真实授权与准入，预览不预留或结算。核对实际 consume/output ledger、产物/副产物和原拒绝。 |
| [native factory quote](../../../crates/oasis7_runtime/src/world/builtin_factory_quote.rs)与[选择校验](../../../crates/oasis7_agent_api/src/world_quote/factory.rs) | kind 与实例 ID 分别绑定；实际 owner、site/location authority、建造消费与 construction power 一并核验。 |
| [首局动作构造](../../../crates/oasis7/src/runtime_host/gameplay_actions.rs)、[feasibility](../../../crates/oasis7_world_state/src/state/starter_industrial.rs)与[结算写入](../../../crates/oasis7_world_state/src/state/industry_transition/recipe_lifecycle.rs) | 固定动作参数、profile/revision 与持久 milestone 各自沿原权威；目录更新不能自动修改首局动作、完成条件或已确认历史。 |
| [首局表达](../../world-simulator/viewer/first-session-content-delivery.design.md#content-source-boundary) | 采用当前候选已发布的事实，不用此表补齐未发布数量、单位、状态或执行按钮。 |

<a id="starter-and-initial-state-applicability"></a>
### 3.3 Starter 与初始配置的来源关系

当前 canonical profile 为 `starter-industrial-smelter-to-assembler-v1`，revision 来源为 [starter_industrial.rs](../../../crates/oasis7_world_state/src/state/starter_industrial.rs)，完成边界为 `production_only`。它核对固定 Smelter 实例和 `recipe.smelter.iron_ingot` 的匹配结算、owner-bound 正量产出及持久 milestone，开放的是 Assembler 候选。其他 Smelter 实例即使 kind 相同，也不能由目录推定符合该里程碑。

| 配置或来源 | 已核实的作用 | 内容交接时必须保留的差异 |
| --- | --- | --- |
| [minimal.json](../../../crates/oasis7_simulator/scenarios/minimal.json) / [resource_bootstrap.json](../../../crates/oasis7_simulator/scenarios/resource_bootstrap.json) | named Simulator 场景输入；后者显式配置 Agent 的 data/electricity 与 origin 的 data | JSON 未列出的值仍受有效默认配置和具体初始化消费者影响；场景名不证明完整工业投入。 |
| [formal_release_default_seed_model](../../../crates/oasis7/src/runtime_host/gameplay_actions.rs) | 程序内构造 seed/config、关闭 origin、启用碎片及 bootstrap chunk、生成一个 starter Agent | 该入口不是直接读取上述两个 JSON；交接保留程序版本和 effective config，不能只记录 seed 或场景标签。 |
| [bootstrap_formal_release_runtime_world](../../../crates/oasis7/src/runtime_host/bootstrap.rs)及 `install_starter_factory_authorities` | 建立 starter 主体、地点/厂址 authority、两种固定实例的 profile/construction power；分别准备 owner 建造账本、`site-smelter` 的铁矿/燃料和 `site-assembler` 的铁锭输入 | world 总库存、owner 建造材料与 site 配方输入是不同来源。bootstrap 的预置是初态，不是玩家已生产、已运输或已完成目标的证据；具体数值沿源及当前快照。 |
| 同文件的 `bootstrap_runtime_world_with_chunk_runtime` / generated sidecar 路径 | 将选定 Simulator 模型投影为 Runtime，并沿自己的初始化路径处理主体和工业准备 | 与上述固定 starter 入口分别取证；按[场景消费矩阵](../../world-simulator/scenario/scenario-files.prd.md#场景初始化与消费入口矩阵)核对默认 Agent 与模型保留范围。 |
| [DevLocal preview 初始化](../../../crates/oasis7_chain_runtime/src/host/preview_world_init.rs) / [formal world 初始化](../../../crates/oasis7_chain_runtime/src/host/world_initialization.rs) | 前者声明 fresh technical preview；后者消费外部准备的 snapshot/journal 与授权材料 | 函数名含 formal/release 不授予正式世界发行、治理激活或 finality；已有 snapshot 的内容取自其真实来源，不假定由本 starter bootstrap 生成。 |

bootstrap 在 owner 建造账本预置 iron_ingot/copper_wire，而 Smelter 的配方输入位于自己的 site ledger，Assembler 的 gear 输入另在 Assembler site ledger。该首局路径当前未提供把 Smelter site 首产物送回 builder 或送入 Assembler site 的可用转运动作，这一限制不推广为项目没有通用 transfer 能力。因此，“第一笔铁锭为评估 Assembler 做准备”包含候选开放与玩法意义，不能叙述为“这笔首产物已经转入 builder 账本并支付了建造”。实际用途仍须绑定实际消费。

首次推荐按[产品适用范围](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#starter-profile-applicability)核查同一 fresh snapshot 的 factory/profile、site/owner、合法输入、电力和完成边界。generated-world 的当前加载仅检查 provenance 文件存在，按[来源合同](../../world-simulator/scenario/scenario-files.prd.md#generated-world-provenance-contract)保留“来源未验证”；文档列出初态不补齐该技术缺口。

<a id="starter-fresh-forward-gate"></a>
### 3.3.1 同一初态的前向可行性组合

Trace refs: O7-REQ-68214dc7-c00b-4c0d-8c3b-c1c24fb52502；承接 [REQ-FIRST-INDUSTRIAL-001](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-001)。本节是 Runtime 共享只读评估者的目标合同。现有 `starter_industrial_feasibility` 及两个 native 单步 quote 是可复用接点，尚不等于本节完整组合已经实现。

评估从同一份当前可合法读取的 Runtime 快照出发，绑定 world/genesis、owner/Agent、入口能力、profile/revision、规则版本、工厂实例与 site/location authority、实际输入/输出账本、owner 电力、grant 及已用累计额度、当前 tick 和未决操作。候选摘要必须覆盖实际读取且改变可达性的全部事实；缺失或无法绑定的事实不能用旧 hash、默认 owner 或世界总库存补齐。相同上下文与候选参数得到同一判断，输入变化后旧候选失效。

当前 canonical 候选只评估一条有限路径：已有合法 Smelter 时，从其实际就绪状态进入指定 iron 配方；尚未建厂时，先评估一次规范建造及其合法完成，再评估一次指定批量的 iron 排程及其匹配结算。完成后核 owner-bound 正量产出、原持久 milestone 条件和 Assembler 候选边界；不把建成 Assembler、gear 或稳定窗口加入首产物门槛。存在相关未决操作时先查询原件，不以假设其取消、重复建厂或再开 job 绕过未知结果。

| 组合步骤 | 共享评估者的职责 | 必须保持的边界 |
| --- | --- | --- |
| 固定本次选择 | 使用当前入口真实可提交的 factory/recipe/batches 与原 canonical plan；区分已有工厂和待建工厂 | Viewer 固定批量不能被评估者悄悄替换为 native 小批；后者只有入口支持且玩家实际选择后才是新候选。 |
| 形成条件后继 | 在不可写回 live 世界的私有投影中，复用真实授权、准入、资源消费和完成转换；仅推进本候选已有规则定义的建造/生产完成边界 | 不直接插入一个 ready 工厂、产物、receipt 或 milestone；不调用外部模型/服务，不为补足条件推进无关任务、补资源或临时安装模块。 |
| 逐步消耗并复验 | 建厂使用 owner 建造账，配方使用实际 site 输入账；后继使用前一步的剩余电力、材料与原 grant 累计额度 | 两个各自单步 quote 都成功，不能证明同一余额足够完成整链。授权有效期、执行限制、输出/副产物接收及真实完成条件须在其适用阶段检查。 |
| 形成结论 | 到达原完成边界且全部适用条件可证明时，按原 Gate 返回候选及风险、首成果、下一 beat 和复查点；否则返回最早 blocker 与实际恢复选择 | 私有投影中的条件后继只是同一当前条件下的可行性依据，不是已发生的结算、容量预留、未来价格/电力保证或执行承诺。 |

私有投影的工作量由该 profile 固定的至多一次建厂和一次配方决定；它不是任意配方搜索或全世界未来模拟。实际已开始工作的结果、无法有界求出的完成依赖、外部输入及未支持转换保持 `unknown/not_tracked`，返回 `no_safe_starter_chain`。时间推进只用于核对该路径规定的完成与到期条件，不把环境未来补给当作确定输入；共享评估者缺失或该入口尚无适用组合证据时，仍依产品原 `target-contract` / 当前证据切线停止正式推荐。

Viewer、pure API 和 Agent 推荐消费同一结果与上下文绑定；它们可以展示不同布局或不支持某种参数，但不能在客户端重新判定可行。真实提交仍逐动作按当前 authority 重新报价/重验；旧候选可达不允许在漂移后继续扣款。原请求结果未知时先按原身份恢复；切换候选只替换未提交的选择，不撤销已接受义务。验证由 [IC-STARTER-GATE](industrial-native-content-delivery.manual.md#ic-starter-forward-gate)统一承接。

<a id="starter-canonical-source-boundary"></a>
### 3.3.2 当前直接输入与条件来源链

Trace refs: O7-REQ-d4944a59-ccf3-4f96-95ac-45c0ca50cf51；承接 [REQ-FIRST-INDUSTRIAL-003](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-003)的原适用条件。

§3.3 已列明当前合法 starter 的三类初始账本。native iron 排程的 `logistics_route_ids` 与 `logistics_path_ids` 为空，消费已有 `site-smelter` 输入；owner 建造材料与 `site-assembler` 预置输入各自独立。对这个已采纳的直接输入路径，来源/精炼、source→owner handoff 与运输不是前置，不生成相应 receipt，也不把后续 Assembler 预置料说成新产铁的用途。

这一适用判断要求实际初态及当前账本与原 authority 可核对；仅知道 bootstrap 文件或场景名称不够。输入不足、来源不明、权限失效或当前 profile 真正要求另一来源链时，返回对应缺口，不能沿“直接输入”标签跳过。未来采纳确需来源/精炼、handoff、运输或目的地检验的 profile，须先由原 M4/来源 owner 接通各阶段真实生产者、独立结果与消费资格，再启用该候选；本节不代签这些尚未选定路径的设计或实现完成。正反例见 [IC-STARTER-GATE](industrial-native-content-delivery.manual.md#ic-starter-forward-gate)。

<a id="content-blocker-boundary"></a>
### 3.4 内容集成中的阻塞处置

| 核查发现 | 交回原消费者的处理 |
| --- | --- |
| ID 缺失、别名被当 canonical、native 与 module-backed 来源混用 | 修正实际选择和来源映射，再按原 lane 查询；不静默选择名字相似的替代项。 |
| 相同材料只存在于其他账本，或 owner/site/profile 不匹配 | 保留对应材料/账本/authority blocker；仅列当前支持的准备或恢复动作，不把 world 总量抵销 site 缺口。 |
| 缺 profile、来源不明、旧报价或无法确认原请求 | 推荐保持 `no_safe_starter_chain` 或原查询的真实未知/拒绝状态；保留下一复查条件，不默认 production_only 或再次提交。 |
| 生产结果已有，但无法证明建造消费、稳定运行或 delivery | 保留已证生产结果，分别标识待验证用途；沿原结算查询继续核对，不发放派生成果。 |

## 4. 制作规格与交付物

<a id="content-change-handoff"></a>
### 4.1 源、版本与变更交接

本专题的可编辑交付是本文目录和配对方法；运行内容仍在原 Rust 配方/工厂、WASM 定义/模板及 bootstrap 中。目录采用 ASCII canonical ID，中文用于解释，不另造显示名或英文译文的权威。新内容采用原类型和声明方式；数量/成本/时长/权限的修改先落原规则与实现，不手改本表制造另一份平衡数据。

每次实际内容变更在原 PR 交接以下可核对对象：受影响 ID 与变更理由、原源版本和新源版本、所用 native/module-backed lane、实际消费者、需要重建的工件及既有身份记录、初态/文案依赖、适用验证及剩余边界。源码文件摘要只能定位该文件；WASM 输入闭包与工件身份仍由原管线计算。共享 `_templates`、path dependencies 或 workspace/lock 变化按原[输入版本合同](../../world-runtime/wasm/wasm-deterministic-build-pipeline.design.md#build-source-input-rule)复核受影响集合，不能只改模块一行并复用旧身份。

| 变更类型 | 必须审视的实际交付与消费者 | 结束条件 |
| --- | --- | --- |
| 目录解释或链接修正 | 本目录、确有变化的首局内容表达和原 source | ID/关系与源一致；说明修改仅影响文档。 |
| 既有 recipe 的输入、产物、副产物、功耗或时长改变 | canonical plan、对应 WASM 定义和 duration authority；quote、执行/结算、使用该产物的后续内容及初态 | 原专业合同确认因果变化与版本处置；源一致性和受影响运行证据使用新候选，旧工作保持原绑定。 |
| 新增 recipe 或 factory kind | 原产品/玩法范围、canonical 来源与适用模块、factory capability、选择/准入、资源来源和内容表达 | 有明确用途、真实前置和失败路径；仅进入源目录不视为当前世界解锁。本文没有预先批准新增内容。 |
| 修改 starter/bootstrap/config | 实际入口的 effective config、owner/site ledger、profile、固定动作与玩法说明 | 预置与玩家成果分清；首局可行/阻塞以及结算证据按新初态重新判断。 |
| 修改共用模板、ABI 或构建依赖 | WASM owner 的输入闭包、构建/identity、实际消费 candidate | 按原 pipeline 重新取得适用证据，不在内容 PR 复制构建或激活协议。 |

**具体变更传播样例。** 若获准调整 `recipe.smelter.iron_ingot` 的输入或副产物，作者先改原规则/plan 和对应 WASM 定义，核对 duration 是否实际受影响；随后核 starter 的 site 输入、首局报价/结算、milestone 识别、gear/alloy_plate 及 Assembler 建造对 iron_ingot 的引用。引用存在意味着需要评估，不意味着每个文件都必改。以新候选重核适用证据；不能用旧首局截图、新配方源码和旧 WASM identity 拼成同一次交付。这个样例是变更方法，未执行配方变更或生产试验。

来源记录沿 Git 及原构建产物；仓库自有源/文档的许可入口为 [LICENSE](../../../LICENSE)，外部依赖、导入内容和二进制再分发仍核各自许可与署名。此轮没有新导入资产或新导出二进制，不能据此宣布全部依赖的发行权利已审清。

<a id="production-measurement"></a>
### 4.2 制作范围和测量模型

当前可直接核数的范围是 2 个工厂 kind、4 个 Smelter 配方和 7 个 Assembler 配方。11 个配方中 8 个有副产物、3 个无副产物；这些是内容差异，不能当作 11 份等量制作工作。共享模板、材料复用和初态适用性会改变集成与返工量。

| 代表内容 | 选择理由 | 需要观察的制作成本 |
| --- | --- | --- |
| `recipe.assembler.gear` | 单输入、无副产物；适合观察已有工厂/模板中的简单重复制作 | 源编辑、双定义核对、选择/报价及既有输入账本集成。 |
| `recipe.smelter.iron_ingot` | 双输入/副产物，绑定首产物和后继候选 | bootstrap 依赖、完整成本表达、owner-bound 结算与旧证据失效。 |
| `recipe.assembler.factory_core` | 通过 module_rack 与 alloy_plate 依赖多层、两类工厂产物，并有副产物 | 上游依赖检查、目标场景准备、缺来源/缺可用运输的阻塞和组合返工；目录关系本身不证明这条链可玩。 |

这些条目用于选择与本次制作变化相符的代表样本，不要求每个 PR 制作三件或重新跑三条链。新增工厂类型、全新资源来源或共享模板变化另列复杂度类别，不能用 gear 的单项耗时外推。首次建立流程与后续重复制作分开记录，已有适用生产记录可复用。

在原制作记录中按类别记录样本数、实际人时与 elapsed time、工具/格式准备、内容制作、等待依赖、集成、验证、返工和验收范围。预算估算可使用 `固定准备人时 + Σ(计划数量 × 该类重复制作人时区间) + 集成人时 + 验证人时 + 返工人时`；重复项的口径不再含后三项，避免重复计费。等待依赖单记日历时间，不能直接当作人时，也不能把总人时除以人数当作承诺排期。外包/采购或货币预算只有实际报价和适用范围成立后另列。

本次已给出可数内容与测量方法，尚没有代表性重复制作、目标环境集成和规格变化传播的实测记录，因此不声明生产速度、预算或规模化就绪。后续扩大生产按[规模证据要求](../../engineering/doc-governance/product-documentation-standard.design.md#content-scale-evidence)使用真实批次、实际候选和传播记录，报告样本差异及未覆盖复杂度。

## 11. 验证

### 11.1 验证映射表

以下为方法分配。实际执行结果记录在原 PR / QA 证据入口；静态审读不能代签游戏、构建或生产能力。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [内容目录](#native-content-catalog) | 当前 native ID、种类关系与对应定义完整可定位 | [IC-CATALOG](industrial-native-content-delivery.manual.md#ic-catalog)；同 source revision，静态目录/源检查 | 原 PR 的版本、脚本输出与语义审读 | 不证明工件已构建、世界已注册或当前可生产 |
| [REQ-FIRST-INDUSTRIAL-001](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-001)；Trace refs: O7-REQ-68214dc7-c00b-4c0d-8c3b-c1c24fb52502 | [初态适用关系](#starter-and-initial-state-applicability) | starter 推荐使用真实初始化来源和同一候选 authority | [IC-INITIAL-STATE](industrial-native-content-delivery.manual.md#ic-initial-state)；先审源，运行声明使用实际入口/profile/snapshot | 原 PR / QA 的来源链、账本与候选定位 | 文档审读不证明任一 live 世界就绪 |
| [REQ-FIRST-INDUSTRIAL-001](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-001)；Trace refs: O7-REQ-68214dc7-c00b-4c0d-8c3b-c1c24fb52502 | [前向组合Gate](#starter-fresh-forward-gate) | 同一fresh初态的完整条件路径、累计预算、绑定与无副作用 | [IC-STARTER-GATE FG01/03–07](industrial-native-content-delivery.manual.md#ic-starter-forward-gate)；同候选/真实入口分别核静态、组合与运行层 | 原PR/QA的各阶段预算、live不变和入口输出 | 不能由两个单步quote或静态算术代签组合实现与正式推荐。 |
| [REQ-FIRST-INDUSTRIAL-003](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-003)；Trace refs: O7-REQ-d4944a59-ccf3-4f96-95ac-45c0ca50cf51 | [当前来源边界](#starter-canonical-source-boundary) | 真实直接输入不增source/handoff前置，真正依赖未知仍拒绝 | [IC-STARTER-GATE FG02/04/05](industrial-native-content-delivery.manual.md#ic-starter-forward-gate)；固定当前profile、实际三类账本和route/path，并做单变量反例 | 原来源/账本核对、适用判断与拒绝结果 | 不证明未来来源/精炼/运输profile的完整生产消费链。 |
| [REQ-FIRST-INDUSTRIAL-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-002)；Trace refs: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95 | [内容依赖与消费](#content-dependency-and-consumption) | 主/副产物、消费来源与实际方案一致 | [IC-INTEGRATION](industrial-native-content-delivery.manual.md#ic-integration)；按受影响 plan/quote/初态选择原测试及实际候选 | 原测试输出及适用组合运行记录 | source parity 不代签编译 WASM、物流或端到端全工业链 |
| [REQ-FIRST-INDUSTRIAL-006](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-006)；Trace refs: O7-REQ-d0041aa9-1dfb-46a0-95e0-67417b39bfa3 | [阻塞处置](#content-blocker-boundary) | ID、账本/authority、结果未知各自保留真实阻塞 | [IC-BLOCKERS](industrial-native-content-delivery.manual.md#ic-blockers)；静态反例与实际受影响拒绝/恢复分开记录 | 原 PR 的逐项判定；运行部分回原 QA | 没有真实输入的反例不能标为运行通过 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [变更交接](#content-change-handoff) | 源/消费者/身份与初态变化能传到交付和证据 | [IC-CHANGE](industrial-native-content-delivery.manual.md#ic-change)；使用实际受影响 ID 与原构建流程 | 原变更 PR 的依赖核查、产物引用及剩余项 | 纸面传播演练不证明生产试验或世界激活 |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [制作测量](#production-measurement) | 范围数、代表性、实测与预算外推可区分 | [IC-PRODUCTION](industrial-native-content-delivery.manual.md#ic-production)；按实际复杂度选择生产样本 | 原制作记录的样本、成本拆分、传播记录与外推边界 | 当前未测重复产能，不证明全游戏预算或排期 |

## 12. 决策与剩余范围

目录放在玩法专业域，便于复用材料关系和首局适用性；数值继续留在原规则，WASM 构建与初态继续由各自 owner 管理。这样既能交接具体内容，又不新增运行 manifest 或平行配置协议。

剩余范围包括全游戏内容清单、其他 module-backed 内容、合法资源取得与运输的全部组合、各世界当前安装/激活状态，以及实测制作成本。它们需要按原产品范围逐批承接。当前最具体的风险是把 site 产出误当 owner 建造库存、把源一致性误当已部署，以及用简单配方的制作成本外推多层内容；本专题的来源表、反例方法和分类测量分别用于暴露这些风险。

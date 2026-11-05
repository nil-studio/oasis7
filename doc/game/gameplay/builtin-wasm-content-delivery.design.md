# 内置 WASM 内容目录与制作交接设计

- 主责：`gameplay_designer` 维护内容用途与组合说明；Runtime、WASM 构建和相应领域 owner 维护规则、工件及消费者；制作与 QA 负责交接和核查。
- 生命周期：`active`。本文组织现有内容，不新增玩法、默认世界选择、模块发布或激活授权。
- 源审读基线：`a23bc41943893da72dbbd78df65214a8a0ae964c`。目录和参数是该版本的可复核内容快照，运行采用情况须另绑定实际候选与世界。
- 上游：[游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)、[非代码交付索引](../../engineering/doc-governance/doc-structure-standard.design.md#non-code-deliverable-index)。操作方法见[配套手册](builtin-wasm-content-delivery.manual.md#scope)。

## 1. 问题、目标与范围

[制作范围](content-production-and-rights.design.md#production-scope)已经盘点 38 个直属 WASM 源包，但包数不能回答“包含什么内容、如何组合、哪些规则实际生效”。`m1.storage.cargo` 的名字不能证明完整实体背包，Product 的质量标签不能证明品质玩法，M5 Council 的结算也不能证明宪制治理已交付。内容制作需要逐项找到真实输入、输出、规则关联和消费者。

本文补齐 GD-06／GD-07 中现有 builtin 源内容的可读目录、有效默认、组合差异和生产交接。范围是 37 个 canonical builtin module ID，以及独立的 `local_test_provider`；目录中的 source、artifact、注册、激活、领域结果和玩家可用分别取证。它不是“37 项已经采用的玩家功能”，也不据此宣布 GD-06／GD-07 或全游戏设计完成。

四个碎片场景继续使用[场景内容交付](../../world-simulator/scenario/asteroid-scenario-content-delivery.design.md#scenario-content-catalog)，native 的 2 类工厂／11 配方继续使用[原工业内容目录](industrial-native-content-delivery.design.md#native-content-catalog)。其他 scenario JSON 的工程用途沿[场景文件矩阵](../../world-simulator/scenario/scenario-files.prd.md)，不改称玩家关卡。正式统一 seed、starter、首 Agent 落点和正式业务记录仍沿[统一初态决策](../../world-simulator/scenario/unified-world-seed-fragment-runtime.design.md#unified-decision-profile)。

## 2. 上游约束与职责

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 对当前有界源集合逐项说明身份、用途与内容边界，保留已完整的相邻设计 | [内容目录](#builtin-content-catalog) | 原 manifest 映射、Gameplay 与相邻内容 owner | 产品层 N/A；理由：整理已有源内容；范围：目录与来源关系。不宣布 38 个包等于 38 个玩家功能或正式采纳。 |
| [非代码交付索引](../../engineering/doc-governance/doc-structure-standard.design.md#non-code-deliverable-index)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 可读参数、标签、profile 与真实规则消费者分开，含拒绝和未覆盖能力 | [内容语义与有效默认](#builtin-content-semantics) | M1／M4／M5 原规则、模板和领域消费者 | 产品层 N/A；理由：解释现有内容规格；范围：本目录参数与关联。不调整经济、能力、品质或治理规则。 |
| [候选内容组合](../../engineering/doc-governance/doc-structure-standard.design.md#content-candidate-composition)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 区分可调用安装组合、有效初态、实际调用者和世界采用证据 | [组合与采用](#builtin-composition) | Runtime bootstrap、原模块生命周期、所选世界 | 产品层 N/A；理由：识别现有组合；范围：内容与消费关联。不决定正式初态，不把 fixture 当正式接入路径。 |
| [非代码交付索引](../../engineering/doc-governance/doc-structure-standard.design.md#non-code-deliverable-index)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 交接可编辑源、共享输入、导出身份、来源与消费者边界 | [制作交接](#builtin-delivery-handoff) | 内容作者、构建／发行维护者与接收方 | 产品层 N/A；理由：制作交接；范围：本专题材料。不新增 builder、许可或发布审批规则。 |
| [候选内容组合](../../engineering/doc-governance/doc-structure-standard.design.md#content-candidate-composition)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 单项参数、共享模板或默认 profile 变化后定位受影响内容与候选 | [变更传播](#builtin-content-change) | 原 source closure／identity authority、领域 owner | 产品层 N/A；理由：内容变更关联；范围：受影响项与验收。不授权热替换、重算历史或已有世界迁移。 |
| [规模化制作证据](../../engineering/doc-governance/product-documentation-standard.design.md#content-scale-evidence)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | 区分参数型、规则型、组合型工作，取得适用制作与复验样本 | [生产与质量](#builtin-production) | 制作 owner、专业作者、集成与 QA；原成本模型 | 产品层 N/A；理由：生产安排；范围：该内容集合的质量与产能输入。不把包数换算为工期或预算。 |

### 2.2 既有专业合同与技术依赖

本专题的 N/A 只限上述内容组织关系。玩家义务继续由原产品和专业条款承担；当前代码与目标设计不同之处不能借目录说明变为“不适用”。

| 原 authority | 本文消费的边界 |
| --- | --- |
| [默认模块的安装、治理与兼容](../../world-runtime/module/agent-default-modules.prd.md#agent-default-modules-install-governance-compatibility)及其[设计](../../world-runtime/module/agent-default-modules.design.md#4-约束与边界) | power 先于可选 default package，安装走原治理路径；七类目标能力与当前 helper 安装集合分开。 |
| [工业领域计算](../../world-simulator/m4/industrial-resource-flow-contract.design.md#industrial-quote-calculation-design)、[方案绑定](../../world-simulator/m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)、[产物 stack 准入合同](../../world-simulator/m4/industrial-resource-flow-contract.design.md#known-product-stack-admission) | 领域材料、有效 profile、真实账本、报价／提交与完成验证分别取证。内容目录不另定扣费、预检或恢复规则。 |
| [战争政治数值基线](gameplay-war-politics-mvp-baseline.design.md#目标)、[区域冲突产品边界](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-001)、[公共决策边界](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-001) | 现有 M5 结算内容与未来区域控制、软赛季、宪制执行等承诺分别判断，不靠同名模块扩张覆盖。 |
| [WASM 输入闭包](../../world-runtime/wasm/wasm-deterministic-build-pipeline.design.md#build-source-input-rule)、[materialization](../../world-runtime/wasm/wasm-deterministic-build-pipeline.design.md#build-materialization-contract)、[发行政策](../../world-runtime/wasm/wasm-deterministic-build-pipeline.design.md#build-release-policy-contract)、[工件与版本绑定](../../p2p/consensus/builtin-wasm-identity-consensus.design.md#builtin-artifact-version-binding) | 源、共享模板、receipt、实际 WASM hash、identity 与治理版本各自有主责；目录不替代这些事实，不补造生产构建或激活证据。 |
| [模块治理与生命周期](../../world-runtime/module/module-lifecycle.md#模块治理与兼容性（草案）)、[制作来源与权利](content-production-and-rights.design.md#source-and-rights)、[成本与容量](content-production-and-rights.design.md#cost-and-capacity) | ABI／沙箱、状态、持久化、权限、恢复、来源和成本沿原合同。本文没有改变这些技术义务，也不新增平行台账。 |

## 3. 当前内容与参数目录

<a id="builtin-content-catalog"></a>
### 3.1 内容单元和唯一身份

37 个 builtin ID 的登记来源是 [`builtin_module_manifest_map.txt`](../../../crates/oasis7_runtime/src/world/artifacts/builtin_module_manifest_map.txt)，分组分别由原 `m1_builtin_module_ids.txt`、`m4_builtin_module_ids.txt`、`m5_builtin_module_ids.txt`维护。下面的表是其可读内容解释；新增或删除仍先改实际 authority，不维护另一份可装载 manifest。每个源链接对应一个包的 `src/lib.rs`，包自己的 Cargo.toml、共享模板及依赖共同构成可继续编辑的输入。

| 组 | 基线单元 | 实际边界 |
| --- | --- | --- |
| M1 | 10 module ID | 三个规则模块、一个 body 模块、四个默认能力模块、两个 power 模块；不等于全部由默认 helper 安装。 |
| M4 | 22 module ID：3 factory、11 recipe、8 product | 工厂建造判定、配方计划与产物验证三个职责；相同配方的 native／WASM 路径不算两种玩法。 |
| M5 | 5 module ID | 战争、普通票局、周期危机、经济反馈和进度奖励的现有生命周期内容。 |
| local_test_provider | 1 独立源包 | `module.runtime.local-test-provider` 不在 37 项 canonical builtin map 内；用于显式 DevLocal W3 初始化的纯命令样本，该 WASM 自身无状态、effects 或 provider 访问，不算生产内容服务。源见 [local_test_provider](../../../crates/oasis7_builtin_wasm_modules/local_test_provider/src/lib.rs)，调用材料由原 [DevLocal initializer](../../../crates/oasis7_runtime/src/world/local_test_provider_bootstrap.rs)接收。 |

<a id="builtin-content-semantics"></a>
### 3.2 M1：动作规则、默认能力与模块私有状态

| module ID 与可编辑源 | 当前源内容及参数 | 组合／消费者边界 |
| --- | --- | --- |
| `m1.rule.move`：[源](../../../crates/oasis7_builtin_wasm_modules/m1_rule_move/src/lib.rs) | 位置事件跟踪；移动距离按整公里向上取整、默认每公里 1 单位电力，输出 `rule.decision` | 独立规则源，不在 power + default 的 6 项集合内。与 mobility 的作用相近，不因此断言双装无重复成本。 |
| `m1.rule.visibility`：[源](../../../crates/oasis7_builtin_wasm_modules/m1_rule_visibility/src/lib.rs) | 观察动作的可见范围判定，默认 `10_000_000 cm`，输出 `rule.decision` | 独立规则源；不是地图内容或完整感知服务交付证明。 |
| `m1.rule.transfer`：[源](../../../crates/oasis7_builtin_wasm_modules/m1_rule_transfer/src/lib.rs) | 正数量且双方位置齐备、同点时提出 `EmitResourceTransfer`；否则 deny | 独立规则源；不提供跨地点物流，实际资源转移仍由 Runtime 处理。 |
| `m1.body.core`：[源](../../../crates/oasis7_builtin_wasm_modules/m1_body_core/src/lib.rs) | 将 `BodyAction` 的 mass/radius/thrust/cross-section 视图转为 `EmitBodyAttributes`，当前规则成本 10 Electricity | 不在 default helper 的四项内；不能据此宣称七能力机体、扩容道具来源和完整装配已交付。 |
| `m1.sensor.basic`：[源](../../../crates/oasis7_builtin_wasm_modules/m1_sensor_basic/src/lib.rs) | 位置跟踪与 QueryObservation 规则，默认范围 `10_000_000 cm` | default 四项之一；bootstrap 的 PreAction / PostEvent 路由负责实际调用，不凭源包产生观察结果。 |
| `m1.mobility.basic`：[源](../../../crates/oasis7_builtin_wasm_modules/m1_mobility_basic/src/lib.rs) | 位置跟踪与 MoveAgent 规则；移动距离按整公里向上取整、默认每公里 1 单位电力 | default 四项之一；位置变化以领域结果为准，规则输出不是已移动。 |
| `m1.memory.core`：[源](../../../crates/oasis7_builtin_wasm_modules/m1_memory_core/src/lib.rs) | 保存受识别领域事件的时间、标签和可选 Agent ID；保留最近 256 项 | default 四项之一；是模块事件摘要，不能代签完整 LLM 记忆、玩家日志或任意历史检索。 |
| `m1.storage.cargo`：[源](../../../crates/oasis7_builtin_wasm_modules/m1_storage_cargo/src/lib.rs) | 处理 `BodyInterfaceExpanded`／`BodyInterfaceExpandRejected`，记录消耗道具计数、Agent 扩容级别和拒绝次数 | default 四项之一；当前源没有因此提供完整实体货物清单、体积容量和搬运链。 |
| `m1.power.radiation_harvest`：[源](../../../crates/oasis7_builtin_wasm_modules/m1_power_radiation_harvest/src/lib.rs) | 跟踪注册／移动位置；在有效动作输入时，对已知 Agent 私有 level 加入位置相关的 1 或 2，发出 `power.radiation_harvest` | power 两项之一；源码 helper 名含 per_tick，但当前 bootstrap 订阅 PreAction，不据此称每个世界 tick 自动给世界 Electricity 入账。 |
| `m1.power.storage`：[源](../../../crates/oasis7_builtin_wasm_modules/m1_power_storage/src/lib.rs) | 注册事件初始 level 6、容量 12；移动规则及成功移动事件的私有电量结算，带 `last_settled_event_id` | power 两项之一；基线 bootstrap 选择 0.2.0 并检查对应 artifact／历史状态。event era 与状态迁移约束见实际 [bootstrap](../../../crates/oasis7_runtime/src/world/bootstrap_power.rs)，不承诺任意旧世界可直接激活。 |

这些模块的私有 state、规则 emit 和世界 Electricity／Data 余额不是一个账本。source 中的默认空状态也不能证明给已有 Agent 自动重建过全部历史；已有世界的初始化、事件补齐和迁移仍由原生命周期验证。完整机体槽位、接口扩容物来源及实体 cargo 的设计责任保留在[默认能力 PRD](../../world-runtime/module/agent-default-modules.prd.md)，不以包名结清。

### 3.3 M4：工厂、配方和 Product 各司其职

Factory 模块共用 [factory template](../../../crates/oasis7_builtin_wasm_modules/_templates/m4_factory_module_template.rs)：核对请求的 `factory_id`、可用 power 和材料后返回 `economy.factory_build_decision`。材料是领域 `MaterialStack.kind`，不是把铁、齿轮新增为 Simulator 的 `ResourceKind`。源中的 factory ID 是种类身份；实际放置实例、场地、拥有者、权限与账本还需 Runtime 接收。

| module ID / factory ID | 基线建造判定参数 | 可编辑源与内容边界 |
| --- | --- | --- |
| `m4.factory.miner.mk1` / `factory.miner.mk1` | structural_frame 8、circuit_board 2、servo_motor 2；最低 power 4；duration 1 tick | [Miner 源](../../../crates/oasis7_builtin_wasm_modules/m4_factory_miner_mk1/src/lib.rs)。仅这份建造判定不证明已具矿床、采矿配方或自动开采链；不在 native 2-kind 目录内。 |
| `m4.factory.smelter.mk1` / `factory.smelter.mk1` | structural_frame 12、heat_coil 4、refractory_brick 6；最低 power 6；duration 1 tick | [Smelter 源](../../../crates/oasis7_builtin_wasm_modules/m4_factory_smelter_mk1/src/lib.rs)。与 native 同名种类的来源和账本仍按原工业设计区分。 |
| `m4.factory.assembler.mk1` / `factory.assembler.mk1` | structural_frame 8、iron_ingot 10、copper_wire 8；最低 power 8；duration 1 tick | [Assembler 源](../../../crates/oasis7_builtin_wasm_modules/m4_factory_assembler_mk1/src/lib.rs)。建造 accepted 不是工厂已在世界投产。 |

11 份 Recipe 共用 [recipe template](../../../crates/oasis7_builtin_wasm_modules/_templates/m4_recipe_module_template.rs)，返回 `economy.recipe_execution_plan`。module ID 去掉开头 `m4.` 后就是该行的 `RECIPE_ID`；不能把 module ID 直接当 recipe ID。精确每批 consume／produce／byproducts／power 继续由各源常量拥有，其读者关系沿[原 11 配方目录](industrial-native-content-delivery.design.md#native-content-catalog)，这里不再维护一份配比权威。

| module ID | 可编辑源 | 默认 recipe profile 的 stage / preferred tags |
| --- | --- | --- |
| `m4.recipe.smelter.iron_ingot` | [源](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_smelter_iron_ingot/src/lib.rs) | bootstrap / smelter |
| `m4.recipe.smelter.copper_wire` | [源](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_smelter_copper_wire/src/lib.rs) | bootstrap / smelter |
| `m4.recipe.smelter.polymer_resin` | [源](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_smelter_polymer_resin/src/lib.rs) | bootstrap / smelter, thermal |
| `m4.recipe.smelter.alloy_plate` | [源](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_smelter_alloy_plate/src/lib.rs) | scale_out / smelter |
| `m4.recipe.assembler.gear` | [源](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_assembler_gear/src/lib.rs) | bootstrap / assembler |
| `m4.recipe.assembler.control_chip` | [源](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_assembler_control_chip/src/lib.rs) | bootstrap / assembler, precision |
| `m4.recipe.assembler.motor_mk1` | [源](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_assembler_motor_mk1/src/lib.rs) | bootstrap / assembler, precision |
| `m4.recipe.assembler.logistics_drone` | [源](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_assembler_logistics_drone/src/lib.rs) | bootstrap / assembler |
| `m4.recipe.assembler.sensor_pack` | [源](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_assembler_sensor_pack/src/lib.rs) | scale_out / assembler, precision |
| `m4.recipe.assembler.module_rack` | [源](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_assembler_module_rack/src/lib.rs) | governance / assembler, precision |
| `m4.recipe.assembler.factory_core` | [源](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_assembler_factory_core/src/lib.rs) | governance / assembler, heavy |

模板以请求批次、可用材料和电力的最小限制求 `accepted_batches`；consume、produce、byproducts 和 power 随接受批次缩放，当前 11 源的 `DURATION_TICKS=1` 不随批次数自动相乘。模板仅用 factory ID 文本中的 smelter／assembler marker 检查兼容，Runtime 还检查实际工厂和 profile；模板通过不等于后续条件齐备。`deterministic_seed` 在当前模板中未用于改变产物，不能宣传随机品质或随机掉落。

8 个 Product 共用 [product template](../../../crates/oasis7_builtin_wasm_modules/_templates/m4_product_module_template.rs)。请求仅含 kind／amount；kind 错配、非正量、超过下表单 stack 上限会拒绝，否则输出 `economy.product_validation`。quality_levels 是输出的标签列表，当前模板没有按某个请求品质分级的算法。tradable 元数据也不替代交易的拥有权、权限与有效 product profile。

| module ID / PRODUCT_ID | STACK_LIMIT | TRADABLE | QUALITY_LEVELS | 可编辑源 |
| --- | ---: | --- | --- | --- |
| `m4.product.material.iron_ingot` / iron_ingot | 500 | true | industrial, refined | [源](../../../crates/oasis7_builtin_wasm_modules/m4_product_material_iron_ingot/src/lib.rs) |
| `m4.product.material.alloy_plate` / alloy_plate | 400 | true | industrial, hardened | [源](../../../crates/oasis7_builtin_wasm_modules/m4_product_material_alloy_plate/src/lib.rs) |
| `m4.product.component.control_chip` / control_chip | 200 | true | industrial, precision | [源](../../../crates/oasis7_builtin_wasm_modules/m4_product_component_control_chip/src/lib.rs) |
| `m4.product.component.motor_mk1` / motor_mk1 | 128 | true | industrial, precision, hardened | [源](../../../crates/oasis7_builtin_wasm_modules/m4_product_component_motor_mk1/src/lib.rs) |
| `m4.product.finished.logistics_drone` / logistics_drone | 32 | true | baseline, field_ready, fleet_grade | [源](../../../crates/oasis7_builtin_wasm_modules/m4_product_finished_logistics_drone/src/lib.rs) |
| `m4.product.component.sensor_pack` / sensor_pack | 96 | true | precision, survey | [源](../../../crates/oasis7_builtin_wasm_modules/m4_product_component_sensor_pack/src/lib.rs) |
| `m4.product.finished.module_rack` / module_rack | 48 | true | industrial, governance_ready | [源](../../../crates/oasis7_builtin_wasm_modules/m4_product_finished_module_rack/src/lib.rs) |
| `m4.product.infrastructure.factory_core` / factory_core | 24 | false | critical, governance | [源](../../../crates/oasis7_builtin_wasm_modules/m4_product_infrastructure_factory_core/src/lib.rs) |

Product 数量为 8，不等于每种材料都有 validator。copper_wire、polymer_resin、gear 没有这组 canonical Product 源；这不删除它们在配方中的产物身份。logistics_drone、module_rack 和 factory_core 在这里是领域材料 kind，不凭名称自动变成物流实体、装配槽位或治理权。

实际 module-backed 请求、artifact 调用和三类 emit 解码由 [`economy.rs`](../../../crates/oasis7_runtime/src/world/economy.rs)接收，领域准入继续检查当前 Agent／工厂／材料和权限。产物完成阶段的 attempt／receipt 与 `ProductValidated` 投递见 [`economy_product_validation.rs`](../../../crates/oasis7_runtime/src/world/economy_product_validation.rs)。模板的 accepted、Runtime 的准入、生产完成和持久化 receipt 是不同事实，交接时必须指出核到了哪一层。

### 3.4 M4 profile：安装时的默认值和实际采用值

默认值的编辑真值是 [`bootstrap_economy.rs`](../../../crates/oasis7_runtime/src/world/bootstrap_economy.rs)，不是上述 WASM 常量。`install_m4_profile_catalog_defaults` 按 key **仅补缺失项**：已有 profile 保留。当前缺省集合有 23 MaterialProfile、10 ProductProfile 和 11 RecipeProfile；有 profile 不意味着库存、工厂或其关联模块已经存在。

| MaterialProfile 的 kind | tier / category | stack_limit | transport_loss_class |
| --- | --- | ---: | --- |
| iron_ore, copper_ore | 1 / ore | 1200 | High |
| carbon_fuel, silicate_ore | 1 / ore | 1000 | High |
| rare_earth_raw | 1 / ore | 800 | High |
| iron_ingot, copper_wire | 2 / intermediate | 900 | Medium |
| alloy_plate, polymer_resin, circuit_substrate | 2 / intermediate | 700 | Medium |
| gear, control_chip, motor_mk1 | 3 / component | 600 | Low |
| sensor_pack, power_core | 3 / component | 500 | Low |
| logistics_drone, field_repair_kit, survey_probe | 4 / product | 200 | Low |
| module_rack | 4 / product | 150 | Low |
| factory_core, relay_tower_kit, grid_buffer_pack | 5 / infrastructure | 100 | Medium |
| hardware_part | 3 / component | 500 | Low |

这些默认 MaterialProfile 的 `decay_bps_per_tick=0`；default_priority 均为 Standard，只有 hardware_part 为 Urgent。profile 的 stack_limit 与 active Product validator 的上限是不同来源。例如 logistics_drone 的两个默认数字分别为 200 和 32，不能任选较大值，也不能宣称发生数值冲突就自动同步。产物验证按实际所选 validator；可信预检、未知限制和历史任务继续沿[原 stack 准入合同](../../world-simulator/m4/industrial-resource-flow-contract.design.md#known-product-stack-admission)验证。

| ProductProfile 的 product_id | role_tag | unlock_stage | tradable | 每个主产出单位的 maintenance_sink |
| --- | --- | --- | --- | --- |
| iron_ingot | scale | bootstrap | true | 无 |
| control_chip | scale | scale_out | true | hardware_part 1 |
| motor_mk1 | scale | scale_out | true | hardware_part 1 |
| sensor_pack | explore | scale_out | true | hardware_part 1 |
| logistics_drone | explore | scale_out | true | hardware_part 2 |
| module_rack | governance | governance | true | hardware_part 2 |
| factory_core | governance | governance | false | hardware_part 4 + alloy_plate 1 |
| field_repair_kit | survival | bootstrap | true | 无 |
| survey_probe | explore | scale_out | true | 无 |
| alloy_plate | scale | scale_out | true | 无 |

RecipeProfile 的 11 个 stage／preferred tags 已在 §3.3 列出，默认 substitution_rules 为空；bottleneck_tags 由原函数维护，不当作 consume 清单。例如 motor 配方 profile 的 bottleneck tag 是 control_chip，但原配方投入仍从自己的 `CONSUME_PER_BATCH` 读取。Runtime 的[排产消费者](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_economy.rs)同时检查 recipe stage、实际工厂 tags 和产物 unlock_stage：logistics_drone 的 recipe stage 为 bootstrap，不消除 product 的 scale_out 条件。

维护 sink 由[共享工业 helper](../../../crates/oasis7_world_state/src/state/apply_domain_event_industry_helpers.rs)按每个正数量主产出乘对应 sink 后合并到消耗。例如两架 drone 除原配方投入外，在对应 profile 有效时增加 hardware_part 4；有 factory_core profile 时，产出一件增加 hardware_part 4 和 alloy_plate 1。这里说明当前 profile 对成本的影响，不预存这些材料、不改 native 首局配置，也不把 profile 名称解释为自动授予阶段或治理能力。

### 3.5 M5：事件驱动的现有玩法内容

| module ID 与可编辑源 | 当前消费的内容与结果 | 参数／未覆盖边界 |
| --- | --- | --- |
| `m5.gameplay.war.core`：[源](../../../crates/oasis7_builtin_wasm_modules/m5_gameplay_war_core/src/lib.rs) | 跟踪联盟、战争、声望及疲劳；到期输出 WarConclude 与参与者结果 | 当前评分包含 fatigue/4，区别于 core fallback；成本、时长、强度与结算数值由[战争政治基线](gameplay-war-politics-mvp-baseline.design.md#目标)拥有。不是区域占领、摧毁或恢复项目交付证明。 |
| `m5.gameplay.governance.council`：[源](../../../crates/oasis7_builtin_wasm_modules/m5_gameplay_governance_council/src/lib.rs) | 跟踪 proposal opened／vote cast／finalized；到 closes_at 依 quorum、pass_bps 和重投后的 tally 输出 GovernanceFinalize | 处理普通票局；通过结果不自动成为宪制执行、区域收费或技术 manifest 激活。 |
| `m5.gameplay.crisis.cycle`：[源](../../../crates/oasis7_builtin_wasm_modules/m5_gameplay_crisis_cycle/src/lib.rs) | 输出 CrisisSpawn／CrisisTimeout；当前类型为 supply_shock、solar_storm、network_outage、governance_deadlock | 初始自动间隔 8 ticks；超时 streak 影响后续间隔、severity、期限和惩罚，不能全部按固定 8/6 ticks 宣传。类型标签不证明各自完整视听、任务或恢复体验。 |
| `m5.gameplay.economic.overlay`：[源](../../../crates/oasis7_builtin_wasm_modules/m5_gameplay_economic_overlay/src/lib.rs) | 对危机成功、合约结算和过期作反馈，向 resilience／economy／reliability 轨道输出 MetaGrant；记录合约去重和连续成功 | `system.economic.overlay` 是当前输出 operator；不是玩家可自由发行积分、币或合约的授权。 |
| `m5.gameplay.meta.progression`：[源](../../../crates/oasis7_builtin_wasm_modules/m5_gameplay_meta_progression/src/lib.rs) | 消费 MetaProgressGranted，累计每 Agent／track 分数，达到新 tier 后排队下一 tick 的奖励 | platinum 150 分奖励 10、diamond 300 分奖励 20；当前状态记录已解锁 tier。不是付费成长、赛季重置或完整成就目录。 |

五者共用 emit kind `gameplay.lifecycle.directives`，由 [`gameplay_loop.rs`](../../../crates/oasis7_runtime/src/world/gameplay_loop.rs)在实际 active Gameplay Tick 路径收取，解析为具型 directive，再交给领域事件处理。私有 state 或 emit 存在不等于世界结算已持久化。manifest 的 sandbox／ranked 标签和 min_players 元数据也不证明两个玩家入口已经提供对应模式。

## 4. 组合、有效初态与实际采用

<a id="builtin-composition"></a>
### 4.1 现有安装 helper 的集合

| helper 与源 | 成功执行时请求的内容组合 | 有效初态与限制 |
| --- | --- | --- |
| `install_m1_power_bootstrap_modules`：[源](../../../crates/oasis7_runtime/src/world/bootstrap_power.rs) | radiation_harvest + power.storage，共 2 项 | 先核 storage 版本工件及适用历史 state；不是填满每个 Agent 世界 Electricity。 |
| `install_m1_agent_default_modules`：同源 | sensor + mobility + memory + cargo，共 4 项 | 没有 body.core，也没有三项独立 rule；不是七目标能力全装。 |
| `install_m1_scenario_bootstrap_modules`：同源 | power 先执行，再按 `install_default_module_package` 安装四项；默认开关为 true | 空白初态成功时覆盖 6 或 2 项。false 只是不执行这次四项安装，不删除世界里已有的四项。 |
| `install_m4_economy_bootstrap_modules`：[源](../../../crates/oasis7_runtime/src/world/bootstrap_economy.rs) | 先补缺省 profile，再处理 22 项模块 | profiles 不是材料赠送；已有 profile 不重置。后续 artifact／治理步骤失败不能推断先前 profile 写入已整体回滚。 |
| `install_m5_gameplay_bootstrap_modules`：[源](../../../crates/oasis7_runtime/src/world/bootstrap_gameplay.rs) | 5 个 M5 Gameplay 模块 | 当前按 Tick + PostEvent domain.* 订阅；已有 private state／实例和恢复条件独立核对。 |
| `install_gameplay_scenario_bootstrap_modules`：同源 | 依次 M1 scenario → M4 → M5 | 无额外模块的适用初态且全部成功时，true 覆盖 33 项，false 覆盖 29 项；这不是世界模块总数上限、原子事务承诺或正式发行默认。 |

这些入口按既有 `propose → shadow → approve → apply` 安装；同版本已 active 可跳过变更，已注册但未 active 走原重新激活路径。返回／失败和每个阶段以实际结果为准，不将模块材料存在或 Shadow 通过写成安装生效；现有 helper 使用 `saturating_add` 形成 manifest version 的事实也不等于本文替其证明所有数值边界。

基线仓库检索中，上述组合与安装函数的调用位于 bootstrap helper 和 Runtime 测试；未据此定位到正式 oasis7 host 自动使用整套组合的证据。交接单应写“helper 支持的组合、对应测试来源”，采用结论另外提供真实入口、所选 world、active manifest／实例和结果。默认正式 seed 的 native starter 不因 builtin map 存在就变成这套 33 模块世界。

### 4.2 采用状态的判定

| 可定位事实 | 可以得出的结论 | 仍须取得的下一层事实 |
| --- | --- | --- |
| 源包、MODULE_ID、共享模板／依赖 | 内容定义可以编辑和审读 | 适用构建输入闭包、工件与 source/build 身份 |
| 正确的工件 hash 与 identity | 选定 artifact 的材料身份可核 | 对该世界适用的注册／manifest 版本、治理和激活结果 |
| 注册记录存在，但未激活 | 世界可定位该记录 | active manifest／实例和当前版本的真实选择 |
| 已激活且绑定当前调用路径 | 模块具备被该消费者调用的条件 | 实际动作／事件、领域接收与结果；不能跳过权限和领域准入 |
| 领域 receipt／event／state 成立 | 该次规则或生产结果可核 | 所声明玩家入口的可见反馈、理解与组合验收 |

M5 的组合特别需要验证：`process_gameplay_cycles_with_modules` 先判断是否存在任一 active Gameplay Tick 模块；存在时走模块 directive 收取路径，不存在时才调用 core gameplay fallback。它不是逐领域“缺一个模块就只 fallback 那个领域”的自动组合器。只安装 Council、停用 War、替换 subscriptions 等变化都要核整个生命周期影响，不能用全五项安装样本代签部分集合。

M4 则须固定所选 factory／recipe／validator 以及有效 profile。Native 无对应 validator／profile 的路径，不能套用本表某个 module 的限制和维护费用；module 路径的材料量、阶段或 validation 成功，也不能反向证明 native starter 已采用同样配置。

## 5. 制作、交接与变更

<a id="builtin-delivery-handoff"></a>
### 5.1 一项内容的交接材料

沿原制作入口记录以下关系，不新增另一份发行清单。接收方应能从一行内容定位源和消费者，而不是只收到一个 `.wasm`。

| 材料 | 本专题要求及具体来源 |
| --- | --- |
| 编辑源与用途 | 本文对应 module ID、源 commit、Cargo.toml／src 及适用共享模板；附实际 recipe／factory／product 身份和当前用途边界。 |
| 规则规格 | 采用的原数值／领域设计；本批改动前后参数、有效 profile 和 stage／tags 关系；未改动规则不另抄一套。 |
| 构建与工件 | 原构建流程产生的 receipt、source/build/identity、实际 WASM hash 和适用 release 记录。按原输入闭包处理包外模板与依赖，不能只散列单个 lib.rs。 |
| 世界与消费者 | 目标 world／候选、manifest 及版本、注册／active／实例状态、有效 profile、实际入口与消费结果；尚未采用就明确停在源或构建材料阶段。 |
| 来源与继续编辑 | 原作者／取得来源、适用许可与 notice、所需编辑工具和依赖；沿[来源与权利](content-production-and-rights.design.md#source-and-rights)核实，不用 Git 作者或根 LICENSE 替代逐项来源。 |
| 质量与恢复 | 对应手册条目、正负例和实际结果；涉及旧 state、运行中任务或回放时由原 owner 提供兼容／恢复证据，不只交新空白世界样本。 |

源包和原 templates 是本专题已经定位的实际内容源。本文没有生成新的 WASM、identity、receipt 或授权材料；这些交付物按真正采用阶段从原流程取得。生产 builder、远程 hash 验证、本地 materializer fallback 与生产 release policy 的边界保持原设计，不把其中一处检查当成完整生产发布闭环。

<a id="builtin-content-change"></a>
### 5.2 变更传播与接收判断

| 变更位置 | 必须重新判断的内容范围 | 接收判据 |
| --- | --- | --- |
| 单个 Product 常量，例如 logistics_drone 的 STACK_LIMIT | 该 validator 的边界、实际选择者、可信预检、完成验证及内容说明；MaterialProfile 的 200 不自动随之改变 | 原规则 owner 已说明变更目的；新工件与有效选择可核；limit／limit+1 和未知或旧版本边界有适用验证。 |
| `_templates/m4_product_module_template.rs` | 当前 include 此模板的 8 个 Product；按真实 include／closure 判断依赖，不将 38 包一律归入 | 输入身份变化可核；逐项身份与差异参数都保留；模板样本通过不豁免 false tradable 等差异项。 |
| Recipe／Factory 模板或单项常量 | 实际依赖的 11／3 个包或所改单项；native 对照、profile、stage、投入／副产物／成本消费者按影响检查 | 不遗漏同名 native 与 WASM 路径的差异，也不未经决定自动同步二者。 |
| bootstrap 的默认 profile | 缺省安装世界、已有同 key profile 世界、字段实际消费者 | 两类世界分别核对；不得将“修改默认值”写成已迁移全部旧世界或提供了新库存。 |
| M1／M5 有状态规则或组合 | state 格式、历史事件／恢复、订阅、受影响动作及 M5 fallback 分支 | 原兼容与治理合同成立后接收；保留历史及原结果，不用当前源重算过去。 |

一个可复核的单项交接样例是现有 drone Product：ID 为 `m4.product.finished.logistics_drone`，内容 kind 为 logistics_drone，当前单 stack limit 为 32、tradable=true、三个质量标签；默认 MaterialProfile limit 为 200，ProductProfile 为 explore／scale_out 并按产出扣 hardware_part 2。任何一项获准改变，都要说明改的是哪一层、哪些消费仍保持原义。这个样例是现有内容关系，不提出新的数值，也不代替创作者实际编辑、构建、发布或采用的工作流。

<a id="builtin-production"></a>
### 5.3 生产复杂度、质量与批次

制作单位按真实工作区分：同一模板的参数型内容、M1／M5 的有状态规则内容、安装／profile 的组合内容。38 个源包不按同一单价或速度计工；已有模板并不消除消费者和历史状态的验证成本。

| 工作类 | 代表单元 | 本批交接前的质量要求 |
| --- | --- | --- |
| 参数型 | 单个 Product 或 Recipe 常量 | ID、单位、每批／每 stack 语义和正负边界准确；源、导出和消费者可对应；标签不冒充新能力。 |
| 共享规则型 | 三类 M4 template；M1 私有状态；M5 生命周期 | 列出真实受影响包；读写 state／事件和拒绝路径有专业 owner；独立审阅具有相应 Runtime／领域能力。 |
| 组合型 | M1 toggle、M4 profile 补缺、M5 部分集合 | 新／旧世界、已有 profile、未激活、部分失败与实际正式入口分别检查；不由单模块样本外推。 |

首次建立闭包、工具和方法，重复制作、集成、验证、返工、维护及等待分别沿[成本与容量](content-production-and-rights.design.md#cost-and-capacity)取证。先取得适用的参数型和有状态／组合型样本，再估算相同复杂度的剩余批次；未取得样本时只报告估算范围及缺失输入。外包可以承接有明确规格的材料，内部仍负责规则裁决、工件／消费者接收和来源核验；本设计不据此决定人员、采购或工期。

## 11. 验证设计与可追溯性

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [游戏设计领域覆盖](../../engineering/doc-governance/product-documentation-standard.design.md#game-design-coverage)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [内容目录](#builtin-content-catalog) | 有界源集合与 ID、用途一一可定位，保留已完整相邻设计 | [BC-CATALOG](builtin-wasm-content-delivery.manual.md#bc-catalog)；固定源 commit 的静态集合、源码常量和映射核对 | 原 PR 的 commit、实际集合差异、37+1 分类结果 | 静态目录不证明工件、正式采用或全部游戏内容。 |
| [非代码交付索引](../../engineering/doc-governance/doc-structure-standard.design.md#non-code-deliverable-index)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [内容语义与有效默认](#builtin-content-semantics) | 参数、标签、profile 和消费者准确，正负例不跨层冒领 | [BC-CONTENT](builtin-wasm-content-delivery.manual.md#bc-content)；固定源参数核对与可执行语义读审；运行场景独立标记 | 逐项源／参数输出、反例判断、适用层与未验证项 | 文档审查不代签数值平衡、实际 Runtime／WASM 结果或完整玩家能力。 |
| [候选内容组合](../../engineering/doc-governance/doc-structure-standard.design.md#content-candidate-composition)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [组合与采用](#builtin-composition) | helper 组合、有效初态和实际世界采用分别取证 | [BC-COMPOSITION](builtin-wasm-content-delivery.manual.md#bc-composition)；调用者检索、新旧初态与 M5 部分集合场景 | 原 PR 的检索范围；实际候选的有效 profile／manifest／结果 | 测试定义不证明正式入口已采用；未执行场景不标通过。 |
| [非代码交付索引](../../engineering/doc-governance/doc-structure-standard.design.md#non-code-deliverable-index)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [制作交接](#builtin-delivery-handoff) | 编辑源、共享输入、工件和消费者可追，来源和授权不被替代 | [BC-HANDOFF](builtin-wasm-content-delivery.manual.md#bc-handoff)；以真实一项内容和所声明交付阶段逐项接收 | 原制作／构建／采用记录，缺失项与阶段结论 | 单个完整包不证明其余包、许可审清或生产发布完成。 |
| [候选内容组合](../../engineering/doc-governance/doc-structure-standard.design.md#content-candidate-composition)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [变更传播](#builtin-content-change) | 改动能定位真实依赖、数值层和适用旧状态 | [BC-CHANGE](builtin-wasm-content-delivery.manual.md#bc-change)；drone 现有参数关系、模板和默认 profile 三类变更推演 | 影响集合及保留边界；真实变更时的新旧版本／候选证据 | 推演不构成数值变更、构建、激活或迁移授权。 |
| [规模化制作证据](../../engineering/doc-governance/product-documentation-standard.design.md#content-scale-evidence)；Trace refs: O7-REQ-2ed6648d-2987-474a-a58f-d5b77f683748 | [生产与质量](#builtin-production) | 适用复杂度、角色、样本和维护成本可核 | [BC-PRODUCTION](builtin-wasm-content-delivery.manual.md#bc-production)；原生产模型下的代表样本与容量复核 | 实际制作记录、范围取舍、角色投入和估算依据 | 未取得样本不宣称规模化产能；源码数不能证明预算。 |

## 12. 决策与剩余边界

本设计选择在原 module ID／manifest 上加可读内容解释，保留原规则和构建 authority，避免以同名内容制造第二套数值或发布规则。源目录这一层可以独立交付；它为制作和审阅提供完整的 37+1 有界集合，并使未实现能力、正式采用和实际验证各有明确落点。

仍由原主责解决的事项包括：正式入口是否采用哪套模块／profile、正式统一初态的四项决定、默认机体／实体 cargo 等目标能力、M5 产品承诺的实现与玩家反馈、旧 state 的具体迁移、生产构建／工件长期可用性，以及适用来源与实际产能材料。它们不是缺一张目录表就能消除的空白，也不能在本专题收尾时改写为已完成。

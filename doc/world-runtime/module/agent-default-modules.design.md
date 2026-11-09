# Agent 默认模块、机体接口与实体 cargo 设计

- 对应要求：[Agent 默认模块体系 PRD](agent-default-modules.prd.md#agent-default-modules-applicability)；方法：[配对核验手册](agent-default-modules.manual.md#adm-manual-scope)。
- 主责：`runtime_engineer`；模块实现由 `wasm_platform_engineer`、能力消费者由 `agent_engineer`、独立验收由 `qa_engineer` 协作。
- 源码审读基线：`4f6f20ece923c5705e26ef879c2b83e0e637b3f9`。下列“当前”只指该树的直接实现与测试定义；目标设计不表示已实现、已运行或已发布。实际交付状态留在 GitHub task/PR 与 Git history。

<a id="1-设计定位"></a>
## 1. 问题、目标与非目标

默认模块安装已有治理入口，机体也已有槽位和接口扩容的局部状态；但安装集合、槽位占位、实体库存和实际能力是不同事实。设计要让消费者回答：这台 Agent 采用什么机体、哪些槽位绑定什么能力、哪个接口实体被消费、cargo 还能容纳多少、当前调用为何可用或被拒绝，以及中断后如何核对原结果。

本设计承接 PRD 已有的全部默认框架义务，保留七类能力、5/7/10 槽规格、standard 的 6 加 1 方案、扩容消耗、实体存取、降级和治理。完整接口道具来源、工业制造链、复杂网络感知、高保真连续动力学沿[原范围](agent-default-modules.prd.md#2-user-experience-functionality)排除；不新增初始赠送、物品交易或把测试注入变成正式获得路径。WASM First 继续适用，世界位置、资源、基础物理与提交不变量由 Kernel 保护。

## 2. 上游约束与相关角色

Trace refs: O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；产品关系是[默认机体与能力框架](../../product/agents-world-simulation/prd.md#req-agent-default-001)及[组合验收](../../product/agents-world-simulation/prd.md#ac-agent-default-001)，通过配对 PRD 的各专业细条款承接。它归位原有目标，不升级当前可玩或发行阶段。

本专题的具体规则声明在配对专业 PRD；[模块生命周期](module-lifecycle.md#稳定实例、交易与升级合同)拥有注册、实例、目标、安装/升级/停用等通用合同，[WASM 接口](../wasm/wasm-interface.md#3-capability、权限和-kernel-边界)拥有 capability 与模块输出校验，[runtime 提交与恢复](../prd.md#runtime-authority-profile-commit)拥有权威提交、结果未知和历史恢复边界。本设计说明这些机制如何承接原默认能力，不新设权限或持久化 authority。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-c7720872-01e1-43a8-910c-8a75d87c357c, O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；[默认包](agent-default-modules.prd.md#agent-default-modules-package) | 保留七类目标；三个 helper 的真实集合、治理、幂等、停用恢复与 power-first/toggle 各自可核对 | [安装与组合](#adm-install-governance) | runtime、WASM；合法工件与原治理流程 | installer 不证明七类完整功能或正式入口，适用阶段保持原范围 |
| Trace refs: O7-REQ-fd4a4f52-8f50-429f-be33-86e2dea5ab00, O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；[机体与槽位](agent-default-modules.prd.md#agent-default-modules-body-slots) | 机体规格、唯一有限槽位、类型/锁定/挂载和有效模块身份一致 | [机体绑定](#adm-body-binding) | runtime、WASM；精确实例、模块与有效配置 | 现有字符串占位不证明完整挂载适配，5 槽不自动容纳 6 个独立模块 |
| Trace refs: O7-REQ-3e9b7d30-4c21-486c-8b49-a4e6420ea582, O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；[接口扩容](agent-default-modules.prd.md#agent-default-modules-interface-expansion) | 合法接口实体恰好消费 1，增加 1 个合法空槽；拒绝无部分库存/槽位变化，结果可审计恢复 | [扩容](#adm-interface-expansion) | runtime；已有合法实体/明确 fixture、授权和提交 authority | 制造/交易/回收/奖励来源保持后续，不增加初赠；新槽不是新能力 |
| Trace refs: O7-REQ-41edae60-a7fa-4be4-a7e7-8efd0ca4e2cf, O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；[发电与储能](agent-default-modules.prd.md#agent-default-modules-power) | 辐射/距离/热/效率约束、有限缓冲、移动前余额校验及低辐射为 0 | [电力能力](#adm-power-capability) | WASM、runtime；逻辑时间、环境输入、原资源结算 | 私有储能、audit emission 与世界 Electricity 不混账；当前简化行为不覆盖完整目标 |
| Trace refs: O7-REQ-962b06bd-3526-4c1c-bf7e-cb87d66f6b8b, O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；[感知](agent-default-modules.prd.md#agent-default-modules-sensor) | 按合法可见域产生 Agent/Location/关键事件的结构化 observation | [感知能力](#adm-sensor-capability) | WASM、Agent；世界可见性与数据读取授权 | 噪声/多谱段/多跳等仍是原扩展方向；当前附近 Agent 输出不代签其余目标 |
| Trace refs: O7-REQ-ad7036fb-7d53-48db-918b-9a94b3a162e9, O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；[移动](agent-default-modules.prd.md#agent-default-modules-mobility) | 档位、能量、状态、冷却和额外损耗服从 Kernel 几何/速度边界 | [移动能力](#adm-mobility-capability) | WASM、runtime；当前位置和独立资源账本 | 不增加推进剂/姿态连续动力学；移动意图和规则 allow 均不是已移动 |
| Trace refs: O7-REQ-4f0ac206-3dcb-44f4-95b6-8559898c5af9, O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；[记忆](agent-default-modules.prd.md#agent-default-modules-memory) | observation/action result/关键事件进入有限短期缓冲、摘要和可检索长期索引 | [记忆能力](#adm-memory-capability) | WASM、Agent；已提交来源、状态/写速配额 | 不把事件标签环形缓冲或 Provider 私有上下文冒充完整记忆；不许诺模型理解或遗失记忆复原 |
| Trace refs: O7-REQ-0c719883-fa7c-4ffe-9714-99e58d241f02, O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；[实体 cargo](agent-default-modules.prd.md#agent-default-modules-cargo) | 三类实体的存/取/列举、有限容量、来源/归属/数量与源目的原子一致 | [cargo 操作](#adm-cargo-flow) | runtime、WASM；合法实体与容器来源、当前授权 | 不把 material ledger 或 artifact 自动转换成实体；现有 helper 不构成正式存入 API |
| Trace refs: O7-REQ-55098169-7d01-4282-94f3-c40640748bf4, O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；[依赖与降级](agent-default-modules.prd.md#agent-default-modules-degradation) | 核对原建议顺序和选定策略，模块失效不破坏位置/资源/审计，恢复不复制效果 | [降级](#adm-degradation) | runtime、WASM、Agent；固定依赖与版本 | 当前只拒绝移动的策略不能代签全部降级；不以重装重新赠送资产或清空历史 |

<a id="adm-current-baseline"></a>
## 3. 当前状态、目标状态与差距

| 对象 | 当前基线与直接来源 | 目标与仍需接线的范围 |
| --- | --- | --- |
| 默认安装 | [bootstrap_power.rs](../../../crates/oasis7_runtime/src/world/bootstrap_power.rs) 经治理安装两个 power 和四个 default；`m1.body.core` 不在两个 helper 中 | 七类能力须分别验证；世界级激活不自动证明每个 Agent 的槽位、实例、资源和授权齐备 |
| 机体 | [AgentBodyState](../../../crates/oasis7_world_state/src/agent_state.rs) 固定默认 standard、7 槽、6 个 module ID 占位、1 空槽；cargo 初始为空 | light/heavy 的配置消费者、槽位与精确模块实例的绑定、完整挂载校验仍须实现；字符串占位不是可交易物品 |
| 扩容 | [body.rs](../../../crates/oasis7_runtime/src/world/body.rs) 修剪输入 item ID，拒绝空 ID、缺件和容量达到 128，成功事件含新容量/层级/槽 ID/Universal 类型 | 当前 `saturating_add` 与 raw event 的兼容行为不证明新目标的全部状态一致性、授权或请求去重；正式来源仍未由本入口解决 |
| cargo | 同文件 `add_agent_cargo_entity` 直接修改 `cargo_entries`，校验正数量/尺寸，同 ID 合并用饱和加法；[cargo Wasm](../../../crates/oasis7_builtin_wasm_modules/m1_storage_cargo/src/lib.rs) 只记录扩容消费计数、层级与拒绝数 | 无完整 `CargoState` 容量/metadata、源目的事务或存/取/列表消费者；helper 无入账事件，不能作为来源或公开库存写接口 |
| body core | [body Wasm](../../../crates/oasis7_builtin_wasm_modules/m1_body_core/src/lib.rs) 把 `BodyAction` 改写成属性更新，规则成本为世界 Electricity 10；runtime 校验物理范围/变化幅度 | 属性变更与接口扩容是两条操作；该成本不适用于扩容，属性变化不增加槽位或 cargo 容量 |
| 感知/移动 | [sensor](../../../crates/oasis7_builtin_wasm_modules/m1_sensor_basic/src/lib.rs) 跟踪位置，输出范围内 Agent；[mobility](../../../crates/oasis7_builtin_wasm_modules/m1_mobility_basic/src/lib.rs) 拒绝零距离、按距离提出 Electricity 成本 | Location/关键事件观测、完整档位/状态/冷却与降级不能由这些局部分支推出 |
| 记忆 | [memory](../../../crates/oasis7_builtin_wasm_modules/m1_memory_core/src/lib.rs) 保存 `time/kind/agent_id`，截留最新 256 条已识别事件标签；损坏私有状态 decode 会回默认值 | 未形成要求中的上下文摘要、长期检索和按写速配额的完整消费者；默认解码回空不是可靠恢复证明 |
| 提交与恢复 | [core policy reducer](../../../crates/oasis7_world_state/src/state/core_policy_transition.rs) 准备同一 Agent 的消费/增槽；[publication 回归](../../../crates/oasis7_runtime/src/tests/core_policy_publication_transaction_regressions.rs) 定义 raw 扩容事件的失败不发布与前态加 tail replay | 局部事件准备不证明整个正式请求、来源入账或跨故障域持久提交；[body 回归](../../../crates/oasis7_runtime/src/tests/body.rs) 的扩容 restore 用扩容后的 snapshot，不是从无道具的旧世界回放获得来源 |

模块内容、内置资源与可编辑来源的更广目录继续由[内置 WASM 内容交付设计](../../game/gameplay/builtin-wasm-content-delivery.design.md)拥有。上表沿直接消费者核查，不以检索零命中断言所有外部部署均不存在相关能力。

<a id="4-约束与边界"></a>
## 4. 边界与结构

| 事实 | 唯一主责及使用方式 | 不得互相替代的事实 |
| --- | --- | --- |
| 机体与物理属性 | World 持有机体、位置、槽位及 Kernel 物理视图；模块提出经过宿主验证的变化 | BodyKernelView 的质量/半径不决定槽位数或 cargo 空间 |
| 槽位绑定 | 机体记录槽 ID、类型、锁定和绑定；目标适配需核对实际实例/版本/工件与安装目标 | `BodySlotType` 不是 ABI `cap_slots`；空槽不是 capability grant |
| 模块可执行性 | registry、精确工件、实例/target、active、ABI/limits、当前授权共同决定；已有显式实例会影响 singleton 路由 | 同名 artifact、注册、全局 active、slot 字符串与 Agent 可调用能力分别成立 |
| 实体库存 | World 的实体位置/归属/数量与 cargo 占用是权威；WASM 私有状态可作有来源的投影 | cargo entity、数值 MaterialLedger、artifact ownership、世界 Electricity/Data 与私有电池不能互相抵账 |
| 结果 | 合法世界提交与原请求/事件关联证明效果，读面注明来源和观察位置 | emit、缓存、LLM 文本、helper 返回或请求已送出不证明正式已提交 |

## 5. 关键运行流程

<a id="adm-install-governance"></a>
### 5.1 默认安装与组合

三个入口保留各自合同：power 安装两个模块；agent-default 安装 sensor/mobility/memory/cargo 四个；scenario 先 power，再按 `install_default_module_package` 安装四个。toggle 为 false 表示本次不安装默认包，不能解释为卸载已经存在的包。

先按既有工件、ABI、版本、权限和 limits 校验取得精确候选，经 `propose -> shadow -> approve -> apply` 后读取真实 registry/active 与事件。相同目标版本已激活时不重复 register；相同版本已注册但停用时只恢复 activate。升级、回滚和状态迁移沿[原生命周期](module-lifecycle.md#instance-upgrade-version-reuse)，不把重新 install 当作迁移。

当前 scenario 是两个顺序 helper 调用，不是整个默认包的一笔事务：power 成功、四模块安装失败时，应保留已成功的 power 事实并报告四模块未完成；重试前读真实状态，复用幂等入口。不能回写“全部失败”、自动卸载 power，或从部分成功宣布七类能力完整。现有 manifest 版本的饱和加法保持兼容事实，不承诺精确数值推进。

完整目标场景还须由场景装配者通过既有 `ModuleChangeSet` 的 register/activate 和同一治理流程，独立注册、激活并验证 `m1.body.core` 的真实工件、Body角色、reduce导出与 `action.body_action` pre_action路由。现有 [body回归的装配](../../../crates/oasis7_runtime/src/tests/body.rs) 给出了该治理接点，但其中测试签名和主体不用于正式环境；当前0.1.0工件也只证明属性规则。目标配置采用支持本文绑定/状态适配的精确候选，body core基础绑定不占附件槽，再逐项核对core及六附件的实际调用和宿主槽位/cargo适配，方能称完整目标组合。

这不把原三个helper改为七项installer，也不新增公开安装API。采用新策略时由场景装配者沿原治理准备精确的新模块变更，不能调用只选旧工件的helper后改标签声称新策略生效；候选准备与最终配置选择按§10保持同一策略。body core或其路由未就绪时保留已经合法安装的部分及原可用配置，明确完整包未完成；重试先读原状态，不重复注册或补发物品。

<a id="adm-body-binding"></a>
### 5.2 机体规格、槽位与挂载目标

以下是原三种机体的已选择目标组合，配置名用于识别组合，不是新增发行档位或玩家入口。body core 是机体基础能力，不另占附件槽；仍须有其真实执行接线。

| 配置身份 | 初始槽位与安装布局 | 能力与 cargo 设计初值 |
| --- | --- | --- |
| `adm.standard.full.v1` | standard，7 槽；Power×2、Sensor、Mobility、Cognitive、Cargo 各自绑定原模块，Universal×1 为空 | 默认完整七能力；cargo 64 units、最多 64 条目 |
| `adm.heavy.full.v1` | heavy，10 槽；同六个附件，Universal×4 为空 | 完整七能力；cargo 128 units、最多 128 条目 |
| `adm.light.without-memory.v1` | light，5 槽；Power×2、Sensor、Mobility、Cargo，初始无空槽 | 六类能力，明确缺 `m1.memory.core`，读取为 `absent_by_configuration`；cargo 32 units、最多 32 条目；不称完整默认包 |

三个组合初始 expansion_level 为 0、槽未锁、cargo 为空；仅复用原合法初始化和治理，不生成可提现模块实体或接口道具。light 放入完整包的六个独立附件必须拒绝；若以后从真实来源扩容获得空 Universal，再合法挂载 memory，其当前能力读面可显示七项齐备，但原五槽组合身份不被回写成默认 standard。heavy 的额外空槽不自动激活更多能力。

1. 新绑定核对槽 ID 唯一、数量与容量一致、类型匹配、未锁、未占用；Universal 只放宽类别匹配，仍需全部权限/兼容校验。精确绑定回查 `(module_id, version, wasm_hash)`、实际 instance 与 SelfAgent target、当前 owner/主体、配置身份和修订；当前 `installed_module: String` 不足以独自承载这些证明。
2. 默认配置绑定来自合法初始化/治理记录，不自动成为可以取出的 `module_item`。若挂载消费已有合法实体，实体从 cargo 移出与槽位绑定共同提交；解除挂载仅在原来源允许返还同一实体、目的 cargo 有空间且依赖可安全停用时成功。默认非实体绑定可经原治理解除并保留实例/状态历史，不能从 artifact 或 slot 标签铸造返还物。
3. 停用保留槽位和实例历史；解除挂载是独立经校验操作。升级保留同一绑定身份并原子更新版本，失败不换一半模块。任何重装都读取实际状态，不按配置表覆盖库存、扩容数、绑定或私有状态。
4. 三种布局只定义合法初始化/已有独立授权配置的结果，本文不提供现有 Agent 跨 frame 转换入口；standard 改名为 heavy 不能免费增加3槽或把cargo64改为128，改名为light也不能删槽/删能力。不存在独立合法转换 authority 时，运行中跨frame请求直接拒绝；不为其新增机体物品、制造或收费机制。现有机体增槽继续每次消费1接口实体，保留全部已提交扩展槽与层级。
5. 同frame的合法挂载组合、模块版本或受支持参数迁移须先准备完整映射，保持原槽ID、容量和扩容事实，重验cargo容量/条目/字节、实际绑定及依赖；不符合则整体拒绝。已占用/锁定槽、无空间返还实体、无法承载的旧库存，均不得删除以完成迁移。初始配置名不覆盖当前绑定，停用/解除memory后必须保留其历史并明确当前能力缺失，不能继续声称完整包有效。

当前通用 `ModuleInstallTarget::SelfAgent` 不含 `slot_id`；接入新绑定必须提供版本化适配和拒绝行为，不能给所有旧安装动作偷偷增加占槽副作用。

<a id="adm-interface-expansion"></a>
### 5.3 接口扩容

当前 `Action::ExpandBodyInterface { agent_id, interface_module_item_id }` 仍是内部动作形式；`action.expand_body_interface` 是模块路由标签。正式消费者接入时必须从可信调用上下文取得主体和原请求身份，不能只信 payload 的 `agent_id`。

目标路径按同一权威视图执行：

1. 核对当前控制/授权、机体配置、合法 item 来源与归属、条目种类为 `interface_module_item`、可用数量至少 1，且未被另一个消费占用。缺正式来源的 fixture 只能用于隔离验证。
2. 核对容量、槽位及扩容层级一致，并以精确整数算出增加 1 的结果；当前上限 128 必须保留，不能靠饱和加法把溢出或损坏状态包装为成功。原兼容路径的输入 trim 行为保留，新实体来源和存入路径须采用同一明确 ID 合同，不能让不同 ID 因消费时修剪而合并。
3. 默认新增未锁、未占用的 Universal 槽；如采用 PRD 允许的机体专用映射，映射必须来自同一已生效配置，结果记录实际类型。槽 ID 按固定规则产生且不复用既有 ID。
4. 同一提交中消费恰好 1、更新容量/层级/槽位并留下原请求与真实结果关联；消耗最后一个单位时移除耗尽条目，释放该单位 cargo 占用。不增加 cargo 的 `capacity_units`，不自动安装能力。
5. 缺 Agent 沿当前 `ActionRejected(AgentNotFound)`，空/缺件/上限沿当前 `BodyInterfaceExpandRejected`；目标新增拒绝保留可解释原因，不伪称 item 已被消费。业务拒绝保持本次 cargo/槽位不变；现有拒绝 audit、`last_active` 和按原合同独立计量的模块执行费用不是库存成功效果。

当前成功事件已有 `agent_id/slot_capacity/expansion_level/consumed_item_id/new_slot_id/slot_type`。目标需要的来源或请求关联复用已有权威提交身份；若必须扩展 wire，应版本化并按 §10 迁移，不把本文目标字段写成已经存在的事件。

<a id="adm-cargo-flow"></a>
### 5.4 实体 cargo 的存入、取出和列举目标

`store_entity`、`withdraw_entity`、`list_entities` 是 PRD 的能力名，当前不是可以照抄调用的正式 API。后续接入复用已授予的 module command/合法动作适配；缺 declaration、授权、实体来源、容量配置或真实消费者时，该能力不得显示为可执行，也不能回退调用 `add_agent_cargo_entity`。

| 操作 | 提交前重验 | 同一结果与失败行为 |
| --- | --- | --- |
| 存入 | 原主体有源实体处分和目标 cargo 写入权限；源/目标位于规则允许的交接范围；实体身份、种类、尺寸、数量和来源一致；目标容量/条目限制足够 | 源减少 `q` 与目标增加 `q`、容量变化及事件一起提交；不部分接纳。失败保留两端库存，不从数值资源或 metadata 补造实体 |
| 取出 | cargo 有可用 `q`；明确目的容器/位置合法、可接收且在权限和交接范围；对象未挂载/消费/锁定；目的空间足够 | cargo 减少 `q` 与目的增加 `q` 同时成立；不隐式丢弃、不凭空远程转移，不能把“取出”解释为出售/销毁 |
| 列举 | 当前主体有该 cargo 的读取权限，使用同一已确认视图与实际模块/配置状态 | 返回实体 ID/类别/数量/尺寸、容量/占用、来源定位及观察版本；稳定排序，有界分页。无权时不泄露条目，失去权威读面时不能把缓存标成当前库存 |

相同实体/批次只在种类、尺寸、来源和所有权语义一致时合并数量；同 ID 异义直接拒绝。分拆和移动保留原身份及可核对数量，不复制实体。数字材料账本与实体矿物间不存在本文定义的自动转换；需要转换的来源链由其专业规则另行采纳。

并发竞争同一最后一个实体或剩余容量时，在权威排序下重新读取本次前态，至多一个相冲突的变更成功；旧预览无预留权。已经提交的相同原请求查回原结果，明确的新请求仍需新的库存；payload 相同不等于同一请求。当前内部 `submit_action` 分配的新 ActionId 不能自动提供跨重连去重。

<a id="adm-data-contract"></a>
## 6. 数据与能力合同

### 6.1 机体、实体与配额

PRD 的 `AgentBodyState/ModuleSlot/CargoState/CargoEntry` 继续是目标模型，当前数据形状以 [agent_state.rs](../../../crates/oasis7_world_state/src/agent_state.rs) 为准。§5.2 三组合共同采用目标策略 `adm-capabilities-v1`、状态 schema revision 1；以下数值是为原 ADM-1～ADM-4 选定的有限工程初值，尚未平衡或实测，不是当前部署参数。配置内容包含布局、全部表中常量和算法身份，按 [WASM canonical/hash 合同](../wasm/wasm-interface.md)计算 `H("oasis7/adm-config/v1", config)`；记录真实 world/Agent、配置 hash、精确工件及绑定修订，不填虚构 artifact hash。修改任何常量或算法产生新配置身份，经原治理激活；既有工件继续原行为。

沿用当前可核常量的是储能上限 12、首次注册 6、普通移动私有电池每向上取整公里 3、世界 Electricity 每公里 1、sensor 范围 10,000,000 cm 和 memory 256 条。cargo 容量、其他限额、辐射分段、热、移动档位和降级阈值是本设计新增初值。所有计算采用精确整数；涉及距离先按本节最大范围短路排除，再以 checked widened integers 计算平方和，不能让坐标差、乘法、序号或累计计数溢出后继续。

`quantity > 0`、`size_per_unit > 0`，`used_units = Σ(quantity × size_per_unit)`，并满足 `0 <= used_units <= capacity_units`；乘积、合并和总和均精确检查，超出可表示范围拒绝，不截断、不饱和。`used_units` 是可重算投影，不是另一套可独立加减的库存。体积单位是配置约定的容量单位，不自动等于质量、槽位、材料 stack limit 或 WASM 内存字节。

实体及批次身份、位置/容器、归属、来源结果与版本由权威世界提供；客户端 metadata 只能携带说明字段，不能覆盖种类、尺寸、数量、owner、工件身份或能力。新目标实体 ID 为 1～128 UTF-8 bytes，原值不做 Unicode 归一化，首尾空白拒绝；旧 trim 路径按原版本读取。metadata 仅允许 `label`（最多 64 UTF-8 bytes）和 `note`（最多 128 bytes）字符串，canonical 编码整体最多 256 bytes，未知键拒绝；完整条目最多 1 KiB，超出不截断入账。分页每页最多 32 条，按原 entity ID 的 UTF-8 bytes 排序，游标绑定同一观察版本。

cargo 容量/条目数采用 §5.2；接口扩容不改变这两项。存取的源/目的容器须在同一 world，权威锚定位置距离不超过 100 cm，且还满足已有实体规则和双方授权；没有合法位置或交接规则时拒绝，数值不是远程转移许可。不同批次/唯一模块实体能否合并仍以真实来源身份为准。新增读取明确公开与受限字段，不能将其他 Agent 的 inventory 或 memory 经 observation 泄露。

<a id="adm-power-capability"></a>
### 6.2 发电与有限储能

当前 [radiation](../../../crates/oasis7_builtin_wasm_modules/m1_power_radiation_harvest/src/lib.rs) 和 [storage](../../../crates/oasis7_builtin_wasm_modules/m1_power_storage/src/lib.rs) 在 action 调用中按位置补能，不是每逻辑 tick 一次；当前轴向距离公式也不是下述衰减公式。新策略必须使用新工件，不能用现有版本名称宣称已经实现这些规则。

| 目标量 | `adm-capabilities-v1` 的确定规则 |
| --- | --- |
| 私有电池 `B` | 整数 0～12；仅新的合法 Agent 注册沿现有规则初始化为 6，恢复/重装/切换不重新赋 6 |
| 热状态 `H` | 离散工程量 0～12，非摄氏度；新注册为 0，每次新逻辑 tick 先降 `max(0,H-1)`；移动成功按 §6.4 增热 |
| 抽象辐射场 | 配置参考点 `(0,0,0)`；距离平方 `r²` 不超过 `800000² cm²` 时 `R=2000`，不超过 `1600000²` 时 `R=1000`，其余 `R=0`，单位为千分之一电量/tick。800,000 cm 复用现有距离尺度作工程初值，分段衰减是新算法；不新增天体实体或改写其他世界环境事实 |
| 效率与热系数 | 配置效率 `E=1000`，合法范围 0～1000；冷却后 `H<=4` 时 `T=1000`，`5<=H<=7` 时 `T=500`，`H>=8` 时 `T=0`。三个组合用同一 E，调整需新配置身份 |
| 产生、接收与弃能 | `P=floor(R×E×T/1,000,000,000)`；`accepted=min(P,12-B)`；`B'=B+accepted`，`curtailed=P-accepted`。不足 1 的部分不累积，低辐射/零效率/过热均可为 0，无负产量或溢出 |

储能模块拥有同一 Agent 的 `B/H/last_harvest_tick/last_settled_move_event/next_move_tick` 及降级状态；宿主把同一已确认投影供给依赖模块。每个 tick 按“冷却 → 按 tick 起点的权威位置产能 → 接收/弃能 → 降级状态 → 该 tick 按权威顺序处理动作”执行。宿主的逻辑 tick 调度触发结算，不由点击次数、查询次数或墙钟驱动。重复 tick/已处理事件不改状态；多个 tick 的恢复逐 tick 使用原位置和事件序列，不以最终位置一次补算全部时间。处理预算不足时保持明确 catch-up，完成前不接纳依赖新电量的写入。

一次补能机会的稳定身份为 `(world_id, Agent_id, logical_tick)`，处理水位随稳定Agent保留；body/instance/配置/owner只是该次来源，不能重新定义机会。重装、换实例及合法同frame迁移必须携带原水位，不能从空模块state重新取得同tick信用，新绑定自下一个合法tick参与补能。旧tick结果未决时先按原身份核定，不越过后再补发；晚到的已处理tick只查原结果。本文未开放跨frame转换，这一水位也不为后续合法迁移预留重置例外。

完整目标中，成功移动的位置、私有B/H/cooldown、原世界费用和staged module状态进入同一root transaction，按§7全部提交或全部不提交；不得靠异步post-event补账形成目标业务效果。既有兼容路径或派生读面仍可能落后：执行下一依赖操作前，宿主须证明所用电池投影已覆盖当前权威位置之前的全部相关tick/已提交移动，不能只检查bytes可解码或B足够。旧/派生投影落后时标为stale并暂停依赖该投影的写入，按原已提交材料追平一次；此恢复不能代签完整目标原子事务。已接纳意图不自动预留旧电量，执行仍按权威顺序重验；不得回滚已提交位置、重发移动或以新ActionId补扣。

发电 audit 记录 tick、原位置/配置、冷却后 H、P/accepted/curtailed 和电池前后值；审计 emission 不另记第二份电量。`m1.power.radiation_harvest` emission、`m1.power.storage` 私有电池及世界 Electricity 分账，产能不自动 credit 世界资源。首次注册、迁移、暂停、重启和已提交移动分别记录原身份；移动前仅校验，真正的电池/热/冷却变更与实际移动结果关联并仅结算一次。原 storage 激活和 event-era 限制继续由 [bootstrap](../../../crates/oasis7_runtime/src/world/bootstrap_power.rs) 校验，不能跳过后覆盖旧状态。

<a id="adm-sensor-capability"></a>
### 6.3 感知

目标 query 绑定 Agent、实际感知模块/配置和当前权威观察位置；宿主先过滤该主体可知范围，再输出附近 Agent、Location 与允许获知的关键事件。半径固定为 10,000,000 cm，按精确距离平方 `<= range²` 包含边界；权限更窄时取交集。关键事件只取最近 64 个逻辑 tick 的已确认事件，且源规则已授权该主体观察其发生位置/内容；无合法位置或可见性证据的事件不因进入时间窗口而公开。

每次新 observation 每类最多 32 条；Agent/Location 按稳定 ID、事件按提交序号排序，原对象引用/公开投影每条最多 512 bytes，完整 observation 最多 64 KiB。超量返回截断及绑定同一观察位置的 continuation；超大单项返回获准的引用与 `detail_omitted`，不能裁出似乎完整的对象。正常最多每 tick 一次新观察，低电按 §6.6 变为间隔至少 2 tick，重复查询只读原缓存。新生成和缓存返回都重新核对当前读取授权；失去授权后不能凭缓存泄露旧许可内容。

每个 observation 保留 as-of、范围、实际来源和截断/陈旧含义；容量限制不能被解释成“范围内没有对象”。事件引用仅指向获准可见的字段，不能通过全量世界事件或 private memory 绕过范围。无 prior cache 而尚未到合法采样时返回 `not_yet_sampled`，不造空的实时视野。

新观察沿sensor已有的 `rule.decision` 携带 `EmitObservation` override适配，一次调用至多一个规则emit，宿主校验后形成一次结构化observation，保持 `max_effects=0/max_emits=1`；不按对象逐条emit。分页/缓存读取复用同一获准观察的宿主只读投影，带原as-of，不通过多次emit绕过采样间隔或制造新采样事实。

当前位置缺失、模块/授权失效时拒绝；低频策略返回最后一次合法观察时必须显示其时间与限制。输入准备、结构化输出及 Agent 消费各自需要证据；LLM 能读 JSON 不代表它理解了环境。当前 sensor 仅输出附近 Agent，新增 Location/事件字段必须有版本化消费者，保留旧 observation 兼容。

<a id="adm-mobility-capability"></a>
### 6.4 移动

继续使用兼容的 `MoveAgent { agent_id, to }`，不要求它已有 mode 字段。新策略按目标距离确定唯一档位，并在规则/结果中给出该档位；本版只有下表离散档位，不开启其他特殊模式或连续姿态模拟。`D=ceil_sqrt(Δx²+Δy²+Δz²)` cm，`k=ceil(D/100000)`；这是新策略的整数距离算法，旧工件仍用其原取整算法。

| 档位 | 距离与私有电池成本 | 成功后的热与冷却 |
| --- | --- | --- |
| `economy` | `0<D<=100000`；`3k` | `H+=1`，`next_move_tick=t+1` |
| `cruise` | `100000<D<=200000`；`3k` | `H+=2`，`next_move_tick=t+1` |
| `burst` | `200000<D<=300000`；`4k`，额外损耗为每公里 1 私有电量 | `H+=4`，`next_move_tick=t+2` |

`D=0`、超过本策略距离上限、`t<next_move_tick`、热状态 `H>=8`、相应模式被 §6.6 禁用、能量不足或依赖失效均拒绝；热增量也须精确满足 `H+增量<=12`。检查同时经过 Kernel 的原几何/速度硬边界，表内合法不保证 Kernel 放行。被禁止的长距离模式不自动改终点或拆成多次动作；消费者可明确提出更短的新请求，原请求保持真实拒绝。

世界 Electricity 仍由原 mobility 费用规则对原 from/to 独立计算：当前是距离 `round` 到 cm 后再向上取整公里，乘 1；不能用本节新整数距离的 `k` 替代。比如位移 `(100000,1,0)` cm 的新策略 D 为 100001、私有成本为 6，而原世界费用取整后仍是 1；这里保留原费用消费者和资源结算。body 属性更新仍有独立 Electricity 10 规则；burst 的额外损耗只进入私有电池，不新设收费资源。感知、记忆和 cargo 本策略仅按电量限制可用性/配额，不追加每次操作的电池扣款，已有 WASM 执行费用照原合同。预检查不扣移动电池、不变更位置/热/冷却；实际成功移动按同一原结果结算一次。该 tick 已完成的合法充能仍是独立事实，不因随后移动拒绝而回滚。

<a id="adm-memory-capability"></a>
### 6.5 短期记忆、摘要与长期检索

目标输入是该 Agent 获准的 observation、实际 action result 和关键事件。宿主提供同一已确认分支上的稳定 `(commit_sequence, ordinal)`、source tick、源引用/摘要和该事件时的降级状态；source tick 是该来源的权威提交 tick，并须沿此游标单调不减，不是外来内容自报的发生时间。无此证明、晚到却冒用已越过旧 tick 的新序号或相冲突来源时停止/隔离该输入，不重开旧额度。延迟消费仍按原单调来源顺序进行，因此只需保存当前 source tick 接受数，无需无限逐 tick map。新策略采用确定性结构摘要 `adm-summary-v1`，不调用模型。

| 状态/输出 | 有限初值与编码限制 |
| --- | --- |
| 短期缓冲 | 最多 256 条，每条 canonical bytes 不超过 512，共不超过 128 KiB；保存来源位置/引用、源 tick、类别、原请求引用（如有）、结果状态、最多 256 bytes 的获准结构投影 |
| 写速 | 按 source tick 接受最多 16 个新条目；降级后为 4 或 0，见 §6.6；重复来源不计数，已在该 tick 接受的内容不因后来降级被删除 |
| 长期摘要索引 | 最多 128 行，每行不超过 256 bytes，共不超过 32 KiB；键为 `(floor(source_tick/64), kind, outcome)` |
| 游标和限制说明 | 至多 4 KiB，包含已消费来源游标、当前 tick 接受数、累计跳过/摘要淘汰计数、首末未保留来源位置与 `history_complete`；不是无界待处理队列 |
| 检索/上下文 | 长期按 bucket 区间、kind、outcome 过滤，键序分页每页最多 32 行；短期摘要最多 32 组，两类响应均不超过 16 KiB，并携带观察版本与截断/缺失说明 |

本版选择受授权宿主只读投影作为检索/上下文消费者：它读取指定 Agent、精确 memory instance 的已提交状态，按本节规则过滤/归纳，通过既有 Agent 读取边界返回有限结果。该目标适配仍须实现、声明并核对当前读取权限，本文不声称现有CLI或SDK已有此方法。memory reducer仅产生 `new_state`，保持当前 `max_effects=0/max_emits=0`，不从 post-event module 发送不存在的 query-result 或新emit。宿主查询不改变写速/游标/世界结果；上下文消费也不把 Provider 私有记忆改为世界权威。

固定 `kind` 为 observation、action_result 或原事件类；关键事件类限 registered、moved、body_changed、interface_expanded、resource_transferred、action_rejected。`outcome` 区分 pending、unknown、committed_success、committed_rejected、not_applicable；接纳通知仍为 pending，只有原权威结果支持时才标 committed。源事件到类别/结果的映射随策略版本固定，同一来源不重复分类；后续终局结果是新的来源条目，以原请求引用关联，不篡改旧 pending。

映射优先级固定为：已确认observation→observation；独立行动结果记录→action_result；其余获准Domain事件按 AgentRegistered/AgentMoved/BodyAttributesUpdated/BodyInterfaceExpanded/ResourceTransferred 映射五个同义类别，ActionRejected/BodyAttributesRejected/BodyInterfaceExpandRejected映射action_rejected。结果记录和世界事件是不同原来源时可以各留一条并保留关联，不能把同一来源换类别再收一次；未列事件不进入本版关键事件索引。键排序先比较整数bucket，再比较kind/outcome的UTF-8 bytes，不用语言环境或数字的字符串排序。

按以下算法处理来源，所有计数加法精确检查：

1. 每批至多 32 个输入，按来源位置递增处理；重复已验证位置跳过。游标、quota 和内容更新一并提交，响应丢失后重新读原状态，不能重新计算为一批新输入。
2. 对每个新输入，以原 source tick 和当时降级状态确定限额；延迟处理不改用现在的 tick 获取新配额。超写速或超 512-byte 条目限制时推进已消费游标并增加对应 skipped 计数和首末未保留位置，`history_complete=false`，不无限排队。输入来源无效则停止该批，不把未经验证位置记成已消费。
3. 接受条目时追加短期缓冲；超过 256 条前，按来源位置淘汰最旧条目，将它并入对应长期键。摘要只含精确 count、首末来源位置/引用、原类别与 outcome；不写自由文本，不声称这些范围内所有事件都在本摘要中。
4. 新长期键超 128 行时，移除字典序最早键，累加淘汰计数并更新未保留范围；不是随机淘汰。短期上下文用相同分组算法即时归纳仍在缓冲中的条目，按各组最后来源位置倒序取前 32 组，再以键排序输出；超量明确 truncated。
5. 查询回传已保留来源、策略/状态版本及不完整标记。任何 skipped/evicted 历史都不能被“没有匹配条目”覆盖；原细节仍需以当前权限查询世界来源，来源已不可得时只保留摘要的有限事实。

待决结果可以在容量压力下转为保留 pending/unknown 身份的摘要，不据此宣称已完成；本地记忆不是持久待办或原请求恢复 authority。宿主来源读取受有界批次及原 manifest 限制，尚未追到目标观察位置时返回 catch-up 与游标，不把旧记忆标为最新。输入、摘要、游标及限制说明总状态最多 164 KiB；超总编码限额或计数溢出时暂停写入并保留前态，不能以回空消除错误。

恢复读取原状态和来源版本；坏 bytes、缺失索引来源或不兼容格式不得静默回空后宣布“记忆已恢复”。可保留可信部分、报告不可得范围并等待受支持的恢复/重建；这不允许猜写历史。当前 256 条事件标签缓冲仍是局部实现，Provider 私有上下文另有 authority，两者不互相代签。

<a id="adm-degradation"></a>
### 6.6 依赖与降级

新策略采用 PRD 建议顺序，不再留待实现选择。`D0～D4` 表示由轻到重的状态，初次注册或合法切入新策略时以当前电量落入下表；它们不改变模块 registry 的 active 事实。

| 状态 | 无历史时的 B 区间 / 向下降级边界 | 感知 / 记忆 / 移动 / cargo |
| --- | --- | --- |
| D0 | 9～12 | 新观察间隔至少 1 tick；16 新记忆/tick；三模式按能量许可；cargo 可写 |
| D1 | 7～8，B<=8 | 观察间隔至少 2 tick；其余同 D0 |
| D2 | 5～6，B<=6 | 观察 2 tick；4 新记忆/tick；其余同 D1 |
| D3 | 2～4，B<=4 | 观察 2 tick；4 新记忆/tick；仅 economy；cargo 可写 |
| D4 | 0～1，B<=1 | 待机：不生成新观察、不接收新记忆、不移动、不作 cargo/挂载/扩容写入；保留获准的状态、库存、旧记忆/观察与原结果读取 |

充能或成功耗能后立即计算其所属下降级别；如果比当前严重，直接进入该级并清零恢复计数，不能等下一次 UI 查询。回升采用滞回：D4→D3、D3→D2、D2→D1、D1→D0 分别要求 `B>=3/6/8/10`，连续两个新逻辑 tick 在充能后的检查点满足才提升一级；每 tick 最多一级，提升后清零计数。中途电量低于本级回升门槛即清零；同 tick 多个动作或重复结算不能累加恢复次数。降级后按当前配额拒绝更多新写，不追溯删除该 tick 已合法接收的记忆。热/冷却或权限限制独立取更严格结果，不能被高电量解除。

待机的记忆限额 0 指不接收新内容条目；宿主仍可按已确认来源维护有限游标和缺失计数，以明确待机期间没有记住哪些范围，不能把这类恢复元数据当作完整记忆写入。

发电失效停止新充能，合法已有电池仍按余额工作；储能投影缺失/损坏时依赖写入与新感知暂停，保留受权只读事实。memory 在 light 组合缺失时读面保持 `absent_by_configuration`，电量恢复不能把它变成已安装。任一模块停用、缺工件、超限或损坏时报告受影响能力，其余合法能力按原依赖继续；cargo 不可写不删除实体，memory 不可写不删世界结果，移动失败不改位置。

待机状态仍允许逻辑 tick 的合法冷却/发电与状态恢复，不赠送电力或重置 Agent。恢复后重查实际绑定、配置、授权和原请求状态，只继续明确仍可执行的未生效动作，不回放旧未确认意图制造新效果。

<a id="adm-state-recovery"></a>
## 7. 状态、事务与持久化

机体槽位、实体源/目的数量、容量投影、绑定身份，以及成功移动的位置、私有B/H/cooldown、原世界费用、staged module状态和本次结果，必须在原权威提交边界内一起生效。目标实现先从不可变前态准备并验证全部变化，再一次发布；只有确认未提交的失败才保留无本次效果的前态，不能留下源已扣cargo未入、物品已消耗槽未加或位置已更新电池未扣的中间态。不同正式入口消费同一合同，不通过直接helper改共享map。

持久发布严格承接[普通generation三态与G1调用方合同](../prd.md#runtime-generation-persistence-contract)，不另设身体专用提交判定：原操作身份、base generation与完整staged候选必须保留到原结果核定。仅`NotCommitted`允许丢弃该业务候选，并按原协议处理唯一无业务效果的拒绝/故障记录；`Committed`安装或恢复原候选，后续镜像/GC等维护失败不撤销原结果。`StatusUnknown`保留完整候选和关联，隔离整个旧World的写入及tick发布，包括其他Agent操作；旧内存可读不证明未提交，也不恢复旧World追加权。确认committed后安装原候选、确认not committed后才允许丢弃并经原服务闸门恢复，仍未知则保持隔离，不换键重做或追加矛盾终态。

三态描述本地generation持久结果；本地`Committed`不自行证明[合法authority profile的正式提交、独立故障域条件或当前可服务](../prd.md#runtime-authority-profile-commit)。正式效果仍核对原证明及历史manifest/activation，重新开放写入仍经过原恢复闸门，不能以本地候选安装代替这些条件。

| 可观察阶段 | 可作出的结论与恢复方式 |
| --- | --- |
| 本地准备/已送出/校验接纳 | 尚不能证明效果；保留原输入、主体、版本与请求关联 |
| 明确拒绝且无效果 | 本次 cargo/绑定/槽位保持前态；修改条件后由有权主体显式形成新请求 |
| 提交结果未知 | 查同一原请求与持久提交决定；generation为StatusUnknown时保留完整候选/base generation并隔离旧World写入和tick，不能换ActionId再消费一次 |
| 已合法提交 | 从权威状态和事件核对同一结果；重复读/重放不再次执行存取、扣费或扩容 |
| 只读恢复完成、当前服务前置未齐 | 保留已恢复事实和限制；依当前 authority/工件/授权重新开放写入 |

snapshot 必须保留机体、cargo、绑定及重建所需模块版本/私有状态；从操作前 checkpoint 加完整已提交 tail 恢复，与直接操作后 snapshot 恢复分别对照。直接 helper 的 fixture 入账不在 journal 中，因此只能在明确包含该 fixture 的 checkpoint 之后测试消费回放，不能据此证明从原始空 cargo 恢复道具来源。

WASM 的扩容统计是派生状态。核心扩容已提交、后续模块处理失败时，不能撤销世界事实或重发扩容补计数；重建投影需重用原事件、原模块版本和去重位置。当前 post-event 输出和完整请求的事务覆盖不同，局部 typed publication 回归不代签正式来源、请求去重和跨故障域持久性；后者仍由 [runtime authority](../prd.md#runtime-authority-profile-commit) 验收。

## 8. 权限、安全与运行约束

正式存取、扩容和实体挂载只允许当前有权主体作用于其被授权的 Agent、实体和目的容器；提交前重验 owner/委托范围、模块/实例、版本、撤销与配额。读取库存和记忆同样限于当前允许的内容。当前 `Action` 中存在 `agent_id`、能够序列化或能够通过内部 `submit_action` 调用，都不证明公开入口已有鉴权。

若采用 module command，复用 [WASM capability](../wasm/wasm-interface.md#3-capability、权限和-kernel-边界) 的 live subject、declaration、scope 与执行重验，LLM 不可自授 grant；物理槽位与 ABI capability slot 分开。正式接线前必须核对真实 catalog/adapter，不能把内置 manifest 的空 `required_caps` 或当前 cargo 的无 command 声明当作自由写权限。

WASM 无直接 I/O，输出由宿主核对并按资源/燃料/字节预算执行。模块失败不触发越权 fallback 或吞掉世界校验。测试 helper 仅证明该私有测试环境，生产、公开节点、其他 world 或外部 Runtime 不复用其获得物品/授权结论。

## 9. 质量与容量

| 环境 / 刺激 | 预期响应与指标来源 | 验证入口 |
| --- | --- | --- |
| 同一机体配置，0/正常/上限附近的扩容输入 | 保留初始 5/7/10；当前容量上限 128，正常加 1、满额拒绝；ID 唯一、无饱和伪成功 | ADM-M-03；畸形前态/数值边界是目标补充，现有正常测试不自动覆盖 |
| 同一 cargo 配置的空、恰满、超满、超条目/metadata 和整数边界 | §5.2/6.1 的 32/64/128 units 与相同条目上限、条目 1 KiB、metadata 256 bytes；精确 `Σ(q×s)` 不越 C，拒绝不部分存取 | ADM-M-04 |
| 同一模块版本与最大受支持输入 | 当前 helper 为 64 MiB 内存、2,000,000 gas；新策略保留这两上限和原调用限额。sensor/memory/cargo 新工件的 `max_output_bytes` 为 512 KiB，宿主适配同时限制编码后完整输入为 512 KiB；现有 ModuleLimits 没有 max_input_bytes 字段，不伪称该输入限额已可声明。其他模块沿原 manifest，任何最终编码超限均失败 | ADM-M-01、06；限额不是实测时延或可保证执行完成的证明 |
| 记忆连续写入、低电量和模块失效 | §6.5 的短期 128 KiB、长期 32 KiB、控制 4 KiB，单响应 16 KiB，输入批次最多 32；按 16/4/0 接受条目及 §6.6 滞回核对；当前 256 条标签不覆盖目标摘要 | ADM-M-06 |
| 同候选并发消费、提交中断、前后态恢复 | 不双花、不丢物、不重复扩容；同来源/版本恢复到一致状态，未知保持未知 | ADM-M-04、07 |

性能验收记录实际配置、规模、并发、环境、耗时和失败；本 PRD 未规定时延阈值，本设计不编造通过值。完整包需同时满足模块配额、观测/记忆大小和 cargo/槽位边界；单模块成功不证明组合在有限预算内可用。

新策略按实际 Agent 绑定实例隔离上述状态与预算，不能把每 Agent 的有限表无限堆入单一 world 私有 map 后仍声称满足限额。宿主只投影该实例获准的有界输入。当前 [ABI](../../../crates/oasis7_wasm_abi/src/lib.rs) 的 state/new_state/event/action 是普通 `Vec<u8>`，[SDK encoder](../../../crates/oasis7_wasm_sdk/src/lib.rs) 使用 serde_cbor；字节值大于等于24时，外层数组可能每个字节占2 bytes，不能用内层状态长度直接加响应长度当完整输出。

因此采用保守的组合上界：原始状态至多164 KiB，外层按328 KiB计；其余 payload 的外层实际编码合计至多128 KiB，ctx/字段/数组头等剩余编码合计至多16 KiB，总计至多472 KiB，小于512 KiB。memory宿主响应16 KiB、cargo宿主分页32 KiB、sensor observation64 KiB各自还须满足专用上限；宿主只读响应不冒充ModuleOutput新增字段。批次/投影超限时有界分页或暂停，不悄然截掉业务必需字段。最终按真实完整输入/输出编码复验；新 manifest/适配未采用这些值时按旧限额执行，不能假设现有 memory 的8 KiB output已支持完整状态。预算字段是限制，不是已实现的ABI扩展或性能结论。

这些是进程内WASM候选预算。当前[executor默认配置](../../../crates/oasis7_wasm_executor/src/lib.rs)与[受控执行策略](../../../crates/oasis7_chain_runtime/src/host/controlled_execution_policy.rs)的4 MiB输出上限容得下512 KiB，但实际采用还须满足该world的`module_limits_max`、真实executor、command及承载transport各自更严格限制；后者不因此调大。较小上限下候选须明确不适用，或经原治理作合法版本化配置/适配与受支持分页；不能把内部预算当作玩家或Provider接口的新增额度。

<a id="adm-compatibility"></a>
## 10. 兼容、迁移与回滚

保留当前 default serde、默认 standard 布局、既有动作/事件含义及模块版本行为。`BodyInterfaceExpanded` 的历史解释不能因新不变量而被任意改写；新路径要补来源、绑定或精确容量时，必须先有明确版本、旧状态识别、reader/writer 与迁移验证。新增检查不是拒绝合法旧历史或回填不存在来源的理由。

旧 `cargo_entries` 缺完整来源/容量、slot 仅有 module ID 时，如实保留原快照事实；不能推断为新体系中可交易/可提现实体，也不静默删除。目标写入所需证明不齐时只暂停受影响新操作，待受支持的确定性迁移或来源核对；现有未迁移路径按原合同验证。恢复旧数据不能重新运行默认安装以“修好”历史、再次加道具、消耗接口或改变原槽位。

升级/回滚绑定原精确实例和工件，依原生命周期处理状态兼容、费用和调度。恢复旧 WASM 不会自动撤销已经提交的扩容或已转移实体；旧版本不能表示新状态时停止该能力写入并保留可读事实，不能裁剪槽位、减少库存或随机重建记忆。

`adm-capabilities-v1` 的切入在已确认逻辑 tick `t0` 的结算边界完成：原版本已接纳的移动/库存变更先查清并结算，结果未知时不切换。候选模块均声明支持同一配置 hash、精确状态版本和绑定目标，再按原治理/生命周期发布配置选择；候选有一项不兼容就保持原选择，不能让新发电公式配旧 storage 结算。同一原实例不能同时采用两套策略。

- 电池从合法旧状态精确保留 `B`，不能清零或赋回 6；旧格式没有热字段时在迁移记录中明确初始化 `H=0`、`last_harvest_tick=t0`、`next_move_tick=t0+2`，只从 `t0+1` 起按新策略冷却/充能。旧事件游标继续保留；这不是对历史热量的估计，也不为停机经过的墙钟时间补电。已有新策略状态恢复则完整保留 H、冷却、tick 游标和滞回计数，不再次应用迁移初值。
- 旧 memory 标签保留在原版本状态及其受权只读视图；不伪造其欠缺的完整来源/结果。新结构从切入边界建立空缓冲/索引，来源游标设为该已确认边界，并标明此前未被新索引覆盖、`history_complete=false`；只有完整且可验证的旧来源能按原确定映射重建。此举不删除旧标签，读取者同时看见新索引起点和旧数据的有限含义。
- 机体、cargo、精确绑定及已消费扩展槽来源能完整验证才开放目标写入；缺证明则继续原受支持路径或保留只读，不为适配新 32/64/128 容量裁剪旧库存。配置切换另按 §5.2 全量检查，不能把 schema 迁移当作机体改型或模块实体来源。

回退到旧工件前验证其能精确表达当前状态；不能表达则保留新数据和只读范围，等待兼容恢复。原 power/state 与 memory/index 的迁移记录纳入 snapshot/tail，前态加 tail 和直接后态须得到相同 tick、配额、摘要和资产事实；回放不重新执行充能、移动或摘要模型。

<a id="adm-validation"></a>
## 11. 验证设计与可追溯性

### 11.1 验证映射表

本表是方法，所有实际版本、命令、结果和缺项记录在实施 Task/PR。默认安装及能力组合测试受 `test_tier_full` 控制；不能用仅启用 required 的零匹配运行声称通过。

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-c7720872-01e1-43a8-910c-8a75d87c357c, O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；[默认包](agent-default-modules.prd.md#agent-default-modules-package) / AC-ADM-01 | [安装](#adm-install-governance) | helper 集合、幂等、停用恢复、power-first/toggle 与完整能力分开核对 | [ADM-M-01/02](agent-default-modules.manual.md#adm-manual-baseline)：同候选 Wasmtime，`power_bootstrap.rs` 的 install/register/reactivate/toggle 用例为 full；另按 ADM-M-05 核对默认包停用恢复、顺序/部分成功及真实绑定 | registry/active、治理事件、实际计数和命令；原 PR/CI 输出 | 不代签七能力目标或正式发布、manifest 数值安全 |
| Trace refs: O7-REQ-fd4a4f52-8f50-429f-be33-86e2dea5ab00, O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；[机体](agent-default-modules.prd.md#agent-default-modules-body-slots) / AC-ADM-02 | [绑定](#adm-body-binding) | 规格/容量/类型/锁定与实例关系，非法组合零部分挂载 | [ADM-M-03/05](agent-default-modules.manual.md#adm-manual-binding)：先核当前源码；目标候选需有真实配置和绑定入口，5/7/10、错类型/锁槽/缺槽/错 owner 分别比较前后态 | 固定配置、原实体/实例来源、事件与状态对照 | 现有 7 槽 fixture 不证明 light/heavy、完整挂载或公开操作 |
| Trace refs: O7-REQ-3e9b7d30-4c21-486c-8b49-a4e6420ea582, O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；[扩容](agent-default-modules.prd.md#agent-default-modules-interface-expansion) / AC-ADM-03 | [扩容](#adm-interface-expansion) | 消费 1 加 1、拒绝不变、前后态恢复和未知提交 | [ADM-M-02/03](agent-default-modules.manual.md#adm-manual-expansion)：`tests/body.rs` 的三个 `expand_body_interface_*` 与 `expanded_body_state_persists_after_restore`；ADM-M-07 用 raw-event publication 回归并另验正式请求，required/同候选本地 fixture 与实际集成分别标记 | 实体数量/容量/层级/槽、原 action/event、恢复对照 | 无来源制造/交易证明；后态 snapshot 不代前态 tail，raw event 不代公开鉴权或持久提交 |
| Trace refs: O7-REQ-41edae60-a7fa-4be4-a7e7-8efd0ca4e2cf, O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；[电力](agent-default-modules.prd.md#agent-default-modules-power) / AC-ADM-04 | [电力](#adm-power-capability) | 非负辐射、有限储能、移动前拒绝和独立账本 | [ADM-M-06](agent-default-modules.manual.md#adm-manual-capabilities)：full 的 `radiation_module_emits_harvest_event`、`storage_module_blocks_continuous_move_when_power_runs_out` 与目标环境/热/效率矩阵；固定模块/配置 | 私有电池、世界资源、实际移动及原 emission 的分开记录 | audit emission 不代真实 world credit；完整参数和依赖仍须目标消费者 |
| Trace refs: O7-REQ-962b06bd-3526-4c1c-bf7e-cb87d66f6b8b, O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；[感知](agent-default-modules.prd.md#agent-default-modules-sensor) / AC-ADM-05 | [感知](#adm-sensor-capability) | 合法范围、完整目标对象、结构化结果与失效限制 | [ADM-M-06](agent-default-modules.manual.md#adm-manual-capabilities)：full 的 `default_sensor_module_emits_observation` 仅附近 Agent；目标增加范围边界、Location/事件、无权和陈旧场景，由独立世界前态给预期 | 输入位置/权限/配置、observations、消费者实际读取 | 不代签高级感知、全可见域目标或 LLM 理解 |
| Trace refs: O7-REQ-ad7036fb-7d53-48db-918b-9a94b3a162e9, O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；[移动](agent-default-modules.prd.md#agent-default-modules-mobility) / AC-ADM-06 | [移动](#adm-mobility-capability) | 档位/状态/冷却/能量与 Kernel 约束一致 | [ADM-M-06](agent-default-modules.manual.md#adm-manual-capabilities)：full 的 `default_mobility_module_rejects_zero_distance_move`、低电移动用例；目标按选定配置补合法/不合法档位与冷却，核位置和两账本 | 实际动作、规则、费用、AgentMoved 与前后位置 | 零距离/低电拒绝不代完整模式；无实际入口时不称可玩 |
| Trace refs: O7-REQ-4f0ac206-3dcb-44f4-95b6-8559898c5af9, O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；[记忆](agent-default-modules.prd.md#agent-default-modules-memory) / AC-ADM-07 | [记忆](#adm-memory-capability) | 来源、有限缓冲、摘要/检索、配额和损坏恢复 | [ADM-M-06/07](agent-default-modules.manual.md#adm-manual-capabilities)：full 的 `default_memory_module_records_domain_events` 只证明局部；目标以已知事件/结果序列及配额独立核对索引和恢复 | 原输入序列、状态/索引查询、淘汰/拒绝与真实来源 | 不代模型私有记忆、理解或无法取回的原内容 |
| Trace refs: O7-REQ-0c719883-fa7c-4ffe-9714-99e58d241f02, O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；[cargo](agent-default-modules.prd.md#agent-default-modules-cargo) / AC-ADM-08 | [cargo](#adm-cargo-flow) | 三类存/取/列举、精确容量、来源归属与并发守恒 | [ADM-M-04](agent-default-modules.manual.md#adm-manual-cargo)：有真实来源/容量/adapter 的候选，执行手算正例、恰满/超满/溢出/异义合并/错归属/并发反例；缺前置记录未可执行 | 两端完整前后态、原请求、数量与占用手算、提交结果 | 当前 helper 和 cargo 扩容统计均不证明实体存取；不补造正式来源 |
| Trace refs: O7-REQ-55098169-7d01-4282-94f3-c40640748bf4, O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；[降级](agent-default-modules.prd.md#agent-default-modules-degradation) / AC-ADM-09 | [降级](#adm-degradation) | 依赖失效可解释、保留事实、恢复重验、不重复效果 | [ADM-M-05/06/07](agent-default-modules.manual.md#adm-manual-recovery)：full 的 scenario 局部组合，目标再核停用/工件缺失/状态损坏/未知结果/迁移回退；固定完整配置与候选 | 策略与实际差异、每类事实保留及重新开放写入证据 | 当前低电先挡移动不代 PRD 全建议顺序；本地恢复不代跨节点/跨故障域验收 |

<a id="5-设计演进计划"></a>
## 12. 决策、长期风险与未决边界

| 决策或边界 | 理由、影响与责任 |
| --- | --- |
| 保留原目标，分别核对安装、槽位、库存和有效能力 | 同名或同 ID 无法说明全链路；runtime/WASM owner 在默认集合或消费者变化时复核，不把局部完成写成整包完成 |
| body core 作为机体基础能力，完整默认附件保持原 6 加 1 | standard/full 与 heavy/full 承接七能力；light/without-memory 是明确缺记忆的原五槽可组合配置，不新增发行表面，也不替代完整包 |
| 物理实体、artifact 和数值账本分开 | 避免一份资产两处可用；gameplay/runtime owner 在正式来源或转换规则采纳时联验，不由本设计开通交易或赠送 |
| 采用 `adm-capabilities-v1` 的有限配置与确定性算法 | §5.2/6 固定本轮原目标所需初值、状态/摘要算法和恢复次序；runtime/WASM owner 实施时交付对应新工件、精确配置身份及边界验证。尚缺实施和实测，不再把这些设计选择交给未来实现临时决定 |
| 复用通用授权、生命周期和恢复 authority | 避免第二套权限和库存；新槽绑定、来源引用或 schema 变化须独立审读，当前原生目标不因产品根卡缺独立名称而消失 |

实施沿原 ADM-1～ADM-4 推进真实消费者和验证；ADM-5 的来源分册继续后续阶段。优先解决目标输入/绑定及其原子边界，再接能力读写和组合场景；产品采用、入口开放和来源闭环各自依其实际证据，不能通过补文档或测试 fixture 宣布已完成。

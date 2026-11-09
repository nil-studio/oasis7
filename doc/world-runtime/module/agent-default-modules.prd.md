# oasis7 Runtime：Agent 默认模块体系（Body/Power/Sense/Move/Memory/Storage）

- 对应设计文档：[Agent 默认模块设计](agent-default-modules.design.md)。
- 验证方法：[机体、接口与默认能力核验手册](agent-default-modules.manual.md)。
- 专业主责：`runtime_engineer`；模块实现由 `wasm_platform_engineer` 协作，能力消费者由 `agent_engineer`、验收由 `qa_engineer` 承接。
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history。

审计轮次: 4


## 1. Executive Summary
- 定义 Agent 的 **默认模块包（Default Module Package v1）**，保证个体在无外部工业设施时具备最基本的生存与行动能力。
- 统一“机体接口数量可配置、可扩容”的规则：扩容必须消耗“接口模块”，扩容行为可审计、可回放。
- 统一七类核心能力模块边界：`身体`、`发电`、`储能`、`感知`、`移动`、`记忆`、`存储`。
- 与现有 WASM First 路线对齐：除位置/资源/基础物理不变量外，能力语义尽量由模块承载。

> 现状对齐：默认模块安装与执行已统一为 wasm-only 链路。`World::install_m1_power_bootstrap_modules` 安装两个 power 模块；`World::install_m1_agent_default_modules` 安装 sensor/mobility/memory/cargo；`World::install_m1_scenario_bootstrap_modules` 先安装 power，再按配置安装默认包。

## 2. User Experience & Functionality
### In Scope
- 默认模块包的组成、模块 ID 约定、依赖关系、降级策略。
- 身体模块的接口槽位模型（不同机体规格、可扩容规则）。
- 存储模块对“世界实体”的收纳语义（矿物、未安装模块等）。
- 接口扩容动作与事件草案（安装接口模块后增加槽位）。
- 与 runtime 治理链路的接入策略（register/activate/upgrade）。

### Out of Scope
- “接口模块”在世界中的完整来源机制（制造/交易/回收/任务奖励）本期不定稿。
- 新增工业制造链（材料加工、装配产线、批量生产）。
- 复杂网络化感知（多跳中继、群体共享感知）。
- 高保真连续动力学（推进剂质量变化、姿态控制细节）。


<a id="agent-default-modules-applicability"></a>
## 3. 适用阶段与专业验收边界

本文保有默认模块框架的既有专业目标：七类能力、机体规格、槽位与挂载、消耗接口实体的扩容，以及实体 cargo 的容量和存取。默认安装入口、某个模块同名工件或测试 fixture 只证明其实际承接的部分，不能缩小这些目标，也不表示完整包已在玩家入口可用。具体消费者接入和当前覆盖见[设计基线](agent-default-modules.design.md#adm-current-baseline)。

这些是原专业 PRD 的原生义务，产品采用关系归于[默认机体与能力框架](../../product/agents-world-simulation/prd.md#req-agent-default-001)及[组合验收](../../product/agents-world-simulation/prd.md#ac-agent-default-001)；补齐设计与验收映射不新增当前可玩承诺。本文不决定接口道具制造、交易、回收、奖励或初始赠送；无正式来源时可以用明确标记的隔离 fixture 检查消费规则，但不能把 fixture 记为世界产物或经济闭环。来源机制仍按第 3.3 节和 ADM-5 后续推进。

LLM 或规则策略可消费观测、提出行动，不能决定槽位、库存、所有权或最终结果。新增正式入口必须复用当时有效的主体、授权、模块和世界校验，不能从 `agent_id`、槽位字符串或模块 emission 推导权限。验收分别核对原要求、直接消费者、独立预期与实际结果；文档校验只证明文档本身。

## 4. Technical Specifications
<a id="agent-default-modules-package"></a>
## 1) 默认模块包总览（V1）

Trace ID: O7-REQ-c7720872-01e1-43a8-910c-8a75d87c357c

Trace refs: O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；本节承接产品[默认机体与能力框架](../../product/agents-world-simulation/prd.md#req-agent-default-001)中的对应专业义务，具体规则由本节拥有。

本条专业要求拥有模块 ID、角色和安装集合的组合约束。下表保留 V1 默认能力目标；“Active”是目标组合状态，实际安装集合与有效能力必须分别核对。

完整 Default Module Package v1 对应默认 `standard_frame` 的六个附件和机体基础能力，也可放入合法的 `heavy_frame` 完整组合。`light_frame` 的五槽规格不承诺同时承载六个独立附件；设计中明确命名的五附件组合只说明该既有机体如何合法组合，必须对消费者显示缺少记忆能力，不替换默认完整包，也不成为另一个正式入口或发行档位。

| 能力 | 模块 ID（建议） | 目标角色 | 目标默认状态 | 说明 |
| --- | --- | --- | --- | --- |
| 身体 | `m1.body.core` | Body | Active | 管理机体、接口槽位、模块挂载关系 |
| 发电 | `m1.power.radiation_harvest` | Body/Domain | Active | 提供低速辐射发电 |
| 储能 | `m1.power.storage` | Body | Active | 提供短时储能与移动前能量校验 |
| 感知 | `m1.sensor.basic` | Body | Active | 提供基础可见域观测与事件感知 |
| 移动 | `m1.mobility.basic` | Body | Active | 提供位移能力、速度/姿态档位 |
| 记忆 | `m1.memory.core` | AgentInternal | Active | 短期记忆 + 长期摘要索引 |
| 存储 | `m1.storage.cargo` | Body | Active | 收纳实体（矿物、未安装模块等） |

> 当前代码已包含上述七个内置模块的局部实现；power helper 安装两个，agent-default helper 安装四个，`m1.body.core` 不在这两个集合中。模块的当前行为与目标仍有差距，尤其不能把 cargo 的扩容统计、memory 的事件标签缓冲解释为完整实体存取或长期记忆索引。目标 ID、角色及完整职责继续由本表与下列各节约束。

<a id="agent-default-modules-body-slots"></a>
## 2) 身体模块（Body Core）

Trace ID: O7-REQ-fd4a4f52-8f50-429f-be33-86e2dea5ab00

Trace refs: O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；本节承接产品[默认机体与能力框架](../../product/agents-world-simulation/prd.md#req-agent-default-001)中的对应专业义务，具体规则由本节拥有。

### 2.1 机体规格与槽位
- 不同机体规格可拥有不同初始接口数量：
  - `light_frame`: 5 槽
  - `standard_frame`: 7 槽（默认）
  - `heavy_frame`: 10 槽
- 默认 `standard_frame` 建议预装 6 个模块并保留 1 个空槽，支持早期扩展。

### 2.2 槽位类型
- `power`：发电/储能类模块。
- `sensor`：感知类模块。
- `mobility`：移动类模块。
- `cognitive`：记忆/规划类模块。
- `cargo`：存储类模块。
- `universal`：通用扩展槽。

### 2.3 数据结构草案
```rust
struct AgentBodyState {
    frame_kind: String,
    slot_capacity: u16,
    slots: Vec<ModuleSlot>,
    expansion_level: u16,
}

struct ModuleSlot {
    slot_id: String,
    slot_type: SlotType,
    installed_module: Option<String>, // module_id
    locked: bool,
}
```

该草案规定机体与挂载语义，不把 `installed_module` 的字符串当作注册、激活、实例 owner 或 capability 证明。配置必须能解释槽位总数、类型、锁定和实际绑定，缺槽、冲突或不兼容时不得静默删减默认能力；不同机体规格的合法组合由[机体与挂载设计](agent-default-modules.design.md#adm-body-binding)核对。

设计已选择 standard/full、heavy/full 与 light/without-memory 的有限组合，并给出能力参数策略 `adm-capabilities-v1`。这是 ADM-1～ADM-4 的可实施设计初值；新策略只能随新工件及合法治理激活采用，不能把已有版本的行为改名为新策略，或将未经实现、平衡及实测的初值写成当前世界事实。重装和切换必须重新校验实际槽位、挂载、库存与状态，不能通过丢弃实体或静默停用原完整包的能力适配五槽。

<a id="agent-default-modules-interface-expansion"></a>
## 3) 接口扩容（消耗接口模块）

Trace ID: O7-REQ-3e9b7d30-4c21-486c-8b49-a4e6420ea582

Trace refs: O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；本节承接产品[默认机体与能力框架](../../product/agents-world-simulation/prd.md#req-agent-default-001)中的对应专业义务，具体规则由本节拥有。

### 3.1 扩容动作
- 动作草案：`action.expand_body_interface { agent_id, interface_module_item_id }`
- 成功后：
  - `slot_capacity += 1`
  - 新增一个 `universal` 槽位（或按机体策略映射为专用槽位）
  - 消耗 1 个接口模块实体

### 3.2 事件草案
- `domain.body_interface_expanded { agent_id, slot_capacity, consumed_item_id }`
- `domain.body_interface_expand_rejected { agent_id, reason }`

### 3.3 接口模块来源（待定）
- 本期仅冻结“消耗品语义”，来源机制保持 TBD。
- 预留来源方向：制造、交易、遗迹回收、任务奖励。
- 后续来源机制应作为独立分册推进，不阻塞默认模块框架落地。

扩容的成功义务是同一结果中恰好消费 1 个可用接口实体并新增 1 个合法槽位；拒绝不得消费该实体或留下半个槽位。新增槽位本身不安装模块、不增加 cargo 容量，也不授予新能力。库存来源、主体、输入与结果必须可核对，恢复不能再次消费；[扩容流程](agent-default-modules.design.md#adm-interface-expansion)保留现有动作兼容及当前实现差距。

<a id="agent-default-modules-power"></a>
## 4) 发电模块（Radiation Harvest）

Trace ID: O7-REQ-41edae60-a7fa-4be4-a7e7-8efd0ca4e2cf

Trace refs: O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；本节承接产品[默认机体与能力框架](../../product/agents-world-simulation/prd.md#req-agent-default-001)中的对应专业义务，具体规则由本节拥有。

本要求同时覆盖本节与第 5) 节的发电、有限储能及移动前校验。

- 职责：根据 Agent 所在位置辐射条件，按 tick 产生低速电能。
- 约束：输出受距离、热状态、模块效率上限共同约束。
- 失败降级：辐射不足时发电为 0，不应产生负向副作用。

建议最小输出事件：
- `module.emitted(kind="power.radiation_harvest")`

## 5) 储能模块（Storage Power）

- 职责：维护 Agent 的可用电量缓冲，支持充放电和移动前校验。
- 约束：容量有限，连续高速移动应触发拒绝或降级（短时等待充能）。
- 与移动联动：移动动作在 `pre_action` 阶段校验储能余额。

建议最小拒绝语义：
- `rule.decision.verdict = deny`，`notes` 给出缺口电量。

<a id="agent-default-modules-install-governance-compatibility"></a>

### 安装、治理与兼容边界

Trace refs: O7-REQ-c7720872-01e1-43a8-910c-8a75d87c357c；承接[默认包](#agent-default-modules-package)的安装组合与治理边界。

- 两个 power module ID 固定为 `m1.power.radiation_harvest` 与 `m1.power.storage`：前者低速补能并发出 audit emission，后者维护私有有限 capacity/level，并在 `pre_action` 移动前拒绝能量不足。
- 三个安装入口都通过 `propose -> shadow -> approve -> apply`，重复安装或“已注册但停用”必须幂等地跳过重复 register 并恢复 activate。
- `install_m1_scenario_bootstrap_modules` 的 power-first composition 与默认-package toggle 是场景兼容合同；不得把 `install_m1_agent_default_modules` 误写成安装七个模块。
- 当前安装代码以 `manifest.version.saturating_add(1)` 构造治理 manifest；这是既有实现残余，不构成数值安全方案或精确版本推进承诺。

<a id="agent-default-modules-sensor"></a>
## 6) 感知模块（Sensor Basic）

Trace ID: O7-REQ-962b06bd-3526-4c1c-bf7e-cb87d66f6b8b

Trace refs: O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；本节承接产品[默认机体与能力框架](../../product/agents-world-simulation/prd.md#req-agent-default-001)中的对应专业义务，具体规则由本节拥有。

- 职责：输出 Agent 可见世界切片（附近 Agent/Location/关键事件）。
- 输入：世界状态与可见范围配置。
- 输出：结构化 observation（支持后续 LLM 或规则策略消费）。
- 扩展方向：噪声、延迟、多谱段感知、目标优先级过滤。

<a id="agent-default-modules-mobility"></a>
## 7) 移动模块（Mobility Basic）

Trace ID: O7-REQ-ad7036fb-7d53-48db-918b-9a94b3a162e9

Trace refs: O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；本节承接产品[默认机体与能力框架](../../product/agents-world-simulation/prd.md#req-agent-default-001)中的对应专业义务，具体规则由本节拥有。

- 职责：定义移动档位与执行前置条件（能量、状态、冷却）。
- 与 Kernel 分工：
  - Kernel 继续负责几何边界/速度上限等硬约束；
  - 移动模块负责机体语义（档位、额外损耗、特殊移动模式）。
- 建议最小动作：`action.move_agent { agent_id, to }`（保持与现有动作兼容）。

<a id="agent-default-modules-memory"></a>
## 8) 记忆模块（Memory Core）

Trace ID: O7-REQ-4f0ac206-3dcb-44f4-95b6-8559898c5af9

Trace refs: O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；本节承接产品[默认机体与能力框架](../../product/agents-world-simulation/prd.md#req-agent-default-001)中的对应专业义务，具体规则由本节拥有。

- 职责：维护短期记忆缓冲与长期记忆索引摘要。
- 输入：observation、action result、关键事件。
- 输出：
  - 决策上下文摘要（短期）
  - 可检索条目（长期）
- 约束：受状态容量与写入速率配额限制。

<a id="agent-default-modules-cargo"></a>
## 9) 存储模块（Storage Cargo）

Trace ID: O7-REQ-0c719883-fa7c-4ffe-9714-99e58d241f02

Trace refs: O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；本节承接产品[默认机体与能力框架](../../product/agents-world-simulation/prd.md#req-agent-default-001)中的对应专业义务，具体规则由本节拥有。

- 职责：存储“世界实体”，而不仅是数值资源。
- V1 支持实体类别：
  - `mineral`（矿物/材料实体）
  - `module_item`（未安装模块）
  - `interface_module_item`（接口扩容模块）
- 基础能力：`store_entity`、`withdraw_entity`、`list_entities`。

数据结构草案：
```rust
struct CargoState {
    capacity_units: i64,
    used_units: i64,
    entries: Vec<CargoEntry>,
}

struct CargoEntry {
    entity_id: String,
    entity_kind: String,
    quantity: i64,
    size_per_unit: i64,
    metadata: serde_json::Value,
}
```

容量按受支持的正数量、正尺寸与有限条目核算；同一实体的源减少和目的 cargo 增加必须属于同一权威结果，取出反向核对明确目的地。不能用数值资源余额、材料名称、artifact hash 或任意 metadata 代替实体来源与归属。来源不明、容量不足、数值不能精确表示或权限失效时不发布部分库存。此处仍是目标合同，当前 `AgentBodyState.cargo_entries` 和扩容统计不是完整 `CargoState` 实现；具体输入、拒绝与回放见[实体 cargo 设计](agent-default-modules.design.md#adm-cargo-flow)。

<a id="agent-default-modules-degradation"></a>
## 10) 依赖与降级策略

Trace ID: O7-REQ-55098169-7d01-4282-94f3-c40640748bf4

Trace refs: O7-REQ-f2da1c09-dedf-4118-b3ab-bce6828abb75；本节承接产品[默认机体与能力框架](../../product/agents-world-simulation/prd.md#req-agent-default-001)中的对应专业义务，具体规则由本节拥有。

- 依赖链：`发电 -> 储能 -> 移动/感知/记忆/存储`。
- 低电量时建议降级顺序：
  1. 降低感知频率
  2. 降低记忆写入频率
  3. 限制移动档位
  4. 进入待机（保留最小生命体征）
- 任一模块失效不应破坏内核不变量（位置/资源守恒/审计链）。

## 5. Risks & Roadmap
- **ADM-1**：冻结默认模块包与模块 ID 规范。
- **ADM-2**：落地身体槽位模型与接口扩容动作/事件。
- **ADM-3**：落地 `sensor/mobility/memory/storage` 四类默认模块最小可用版本。
- **ADM-4**：补齐场景初始化接入、治理安装入口与回放一致性测试。
- **ADM-5**：接口模块来源机制定稿（制造/交易/回收三选一或组合）。

### Technical Risks
- **模块耦合风险**：电力、移动、感知存在链式依赖，若边界不清晰易出现循环依赖。
- **状态膨胀风险**：记忆与实体存储均可能快速增长，需严格配额与淘汰策略。
- **平衡性风险**：默认发电/储能参数若过强会削弱经济系统驱动力，过弱会影响可玩性。
- **来源未定风险**：接口模块获取机制未冻结，可能影响扩容节奏与经济闭环。

## 6. Validation & Decision Record

<a id="agent-default-modules-acceptance"></a>
### 专业验收

以下 AC 对应原能力义务；完整目标的未实现项不能用当前局部测试抵销。方法与当前测试层级由[设计验证表](agent-default-modules.design.md#adm-validation)和[配对手册](agent-default-modules.manual.md#adm-manual-scope)给出，运行事实保留在实际 Task/PR。

| AC | 原要求 | 独立通过判据 | 方法 |
| --- | --- | --- | --- |
| AC-ADM-01 | [默认包及治理](#agent-default-modules-package) | 七类目标职责逐项有真实消费者；三个安装入口保持各自集合、power-first、toggle、幂等和停用恢复。注册/激活成功与完整功能通过分别判定 | ADM-M-01、02、06 |
| AC-ADM-02 | [机体与槽位](#agent-default-modules-body-slots) | 保留 5/7/10 初始规格与 standard 的 6 加 1 方案；槽位唯一、有界，类型/锁定/挂载与实际模块身份一致；不合法组合零部分挂载 | ADM-M-01、03、05 |
| AC-ADM-03 | [接口扩容](#agent-default-modules-interface-expansion) | 合法可用实体一次消费 1、容量增加 1、仅新增合法空槽；缺件/错类/超限/权限或状态漂移不改本次库存与槽位；未知结果和恢复不再消费 | ADM-M-02、03、07 |
| AC-ADM-04 | [发电与储能](#agent-default-modules-power) | 位置辐射、距离、热与效率规则来源明确，低辐射为 0；储能始终有限，移动前不足拒绝/按合法档位降级，充能与实际移动结算可审计 | ADM-M-06、07 |
| AC-ADM-05 | [感知](#agent-default-modules-sensor) | 合法可见域内的 Agent、Location、关键事件输出结构化 observation；范围外、失效或无权数据不泄露，陈旧或受限输出不冒充实时完整视野 | ADM-M-06 |
| AC-ADM-06 | [移动](#agent-default-modules-mobility) | 能量/状态/冷却/档位与额外损耗按同一有效配置判断，Kernel 的几何/速度硬边界仍生效；实际位置与费用只按权威结果成立 | ADM-M-06、07 |
| AC-ADM-07 | [记忆](#agent-default-modules-memory) | observation、action result、关键事件能按来源进入有限短期缓冲、上下文摘要与可检索长期索引；容量/速率、淘汰和恢复均可解释，不补造遗失内容 | ADM-M-06、07 |
| AC-ADM-08 | [实体 cargo](#agent-default-modules-cargo) | 三类实体的存入、取出、列表与容量均有消费者；源/目的数量守恒、尺寸和 metadata 不扩权，满载/溢出/错归属/并发失败无部分入账；不与数值资源双记 | ADM-M-04、07 |
| AC-ADM-09 | [依赖与降级](#agent-default-modules-degradation) | 逐项核对既有建议降级顺序与实际选定策略；策略差异明确而非被隐藏。失效/停用/恢复保留位置、资源、槽位、cargo 与审计链，并重验能力而非重新赠送或重复消费 | ADM-M-05、06、07 |

### 设计取舍与复核触发

| 取舍 | 依据与边界 | 主责与复核触发 |
| --- | --- | --- |
| 保留全部既有默认能力目标，按实际消费者拆分验收 | installer、slot ID 和私有模块状态各有职责；同名不能替代完整功能 | runtime 与模块 owner；安装集合、manifest、能力实现或调用者变化时重核 |
| 为原机体和能力目标选择有限、版本化设计初值 | [目标配置与算法](agent-default-modules.design.md#adm-data-contract)固定容量、距离/热/效率、移动、记忆及降级规则；light 的减配组合显式缺记忆，standard/full 的原七能力义务保留 | runtime 与模块 owner；参数变更形成新配置身份，实施验证与后续平衡分别记录 |
| 接口实体消费先有框架，完整来源仍是 ADM-5 | 不把 fixture 或直接 helper 变成初赠、制造或交易规则；来源缺失不删除扩容/存取义务 | gameplay 与 runtime owner；来源分册采纳并给出真实入账消费者后联验 |
| 沿既有治理、授权和持久恢复边界实施 | 不新增并行的库存/权限权威，不从模块输出直接写世界 | runtime 与模块 owner；新增正式入口、实例绑定或状态格式变更时独立审读 |

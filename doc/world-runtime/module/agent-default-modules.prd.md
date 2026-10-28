# oasis7 Runtime：Agent 默认模块体系（Body/Power/Sense/Move/Memory/Storage）

- 对应设计文档: `doc/world-runtime/module/agent-default-modules.design.md`
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


## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用（文档迁移任务）。
- Evaluation Strategy: 通过文档治理校验、引用扫描与任务日志检查验证迁移质量。

## 4. Technical Specifications
## 1) 默认模块包总览（V1）

| 能力 | 模块 ID（建议） | 角色 | 默认状态 | 说明 |
| --- | --- | --- | --- | --- |
| 身体 | `m1.body.core` | Body | Active | 管理机体、接口槽位、模块挂载关系 |
| 发电 | `m1.power.radiation_harvest` | Body/Domain | Active | 提供低速辐射发电 |
| 储能 | `m1.power.storage` | Body | Active | 提供短时储能与移动前能量校验 |
| 感知 | `m1.sensor.basic` | Body | Active | 提供基础可见域观测与事件感知 |
| 移动 | `m1.mobility.basic` | Body | Active | 提供位移能力、速度/姿态档位 |
| 记忆 | `m1.memory.core` | AgentInternal | Active | 短期记忆 + 长期摘要索引 |
| 存储 | `m1.storage.cargo` | Body | Active | 收纳实体（矿物、未安装模块等） |

> 说明：`m1.power.*` 与 `m1.body.core` 已有实现基础；其余模块在本分册定义为默认能力目标与接口草案。

## 2) 身体模块（Body Core）

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

## 3) 接口扩容（消耗接口模块）

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

## 4) 发电模块（Radiation Harvest）

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
- 两个 power module ID 固定为 `m1.power.radiation_harvest` 与 `m1.power.storage`：前者低速补能并发出 audit emission，后者维护私有有限 capacity/level，并在 `pre_action` 移动前拒绝能量不足。
- 三个安装入口都通过 `propose -> shadow -> approve -> apply`，重复安装或“已注册但停用”必须幂等地跳过重复 register 并恢复 activate。
- `install_m1_scenario_bootstrap_modules` 的 power-first composition 与默认-package toggle 是场景兼容合同；不得把 `install_m1_agent_default_modules` 误写成安装七个模块。
- 当前安装代码以 `manifest.version.saturating_add(1)` 构造治理 manifest；这是既有实现残余，不构成数值安全方案或精确版本推进承诺。

## 6) 感知模块（Sensor Basic）

- 职责：输出 Agent 可见世界切片（附近 Agent/Location/关键事件）。
- 输入：世界状态与可见范围配置。
- 输出：结构化 observation（支持后续 LLM 或规则策略消费）。
- 扩展方向：噪声、延迟、多谱段感知、目标优先级过滤。

## 7) 移动模块（Mobility Basic）

- 职责：定义移动档位与执行前置条件（能量、状态、冷却）。
- 与 Kernel 分工：
  - Kernel 继续负责几何边界/速度上限等硬约束；
  - 移动模块负责机体语义（档位、额外损耗、特殊移动模式）。
- 建议最小动作：`action.move_agent { agent_id, to }`（保持与现有动作兼容）。

## 8) 记忆模块（Memory Core）

- 职责：维护短期记忆缓冲与长期记忆索引摘要。
- 输入：observation、action result、关键事件。
- 输出：
  - 决策上下文摘要（短期）
  - 可检索条目（长期）
- 约束：受状态容量与写入速率配额限制。

## 9) 存储模块（Storage Cargo）

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

## 10) 依赖与降级策略

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
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-ENGINEERING-006 | 文档内既有任务条目 | `test_tier_required` | `./scripts/doc-governance-check.sh` + 引用可达性扫描 | 迁移文档命名一致性与可追溯性 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-DOC-MIG-20260303 | 逐篇阅读后人工重写为 `.prd` 命名 | 仅批量重命名 | 保证语义保真与审计可追溯。 |

## 原文约束点映射（内容保真）
- 原“目标” -> 第 1 章 Executive Summary。
- 原“范围” -> 第 2 章 User Experience & Functionality。
- 原“接口 / 数据” -> 第 4 章 Technical Specifications。
- 原“里程碑/风险” -> 第 5 章 Risks & Roadmap。

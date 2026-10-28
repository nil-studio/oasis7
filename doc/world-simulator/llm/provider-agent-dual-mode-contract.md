# Local Provider 双轨模式 Observation / Action Contract 冻结（2026-03-16）

- 关联 PRD: `PRD-WORLD_SIMULATOR-040`
- 专题入口与权威边界: `doc/world-simulator/llm/README.md`
- owner: `agent_engineer`
- 联审: `producer_system_designer`、`runtime_engineer`、`viewer_engineer`、`qa_engineer`
- 执行任务: `TASK-WORLD_SIMULATOR-149`

## 1. 目的
- 冻结 `player_parity` / `headless_agent` 的 observation/action contract，避免后续实现阶段边做边改导致 parity、回放与回归口径漂移。
- 明确哪些信息可暴露给 `player_parity`，哪些只允许在 `headless_agent` 中以结构化形式提供。
- 明确所有模式共享的动作语义、失败语义、schema version 与禁止事项。

## 2. 统一原则
1. 所有模式共享同一 runtime 权威动作校验，不允许模式专属捷径动作。
2. `player_parity` 与 `headless_agent` 的区别只在 observation 表达层，不在动作语义层。
4. 所有 benchmark / replay / summary 样本必须记录 `mode`、`observation_schema_version`、`action_schema_version`。
5. 若 provider 或 adapter 不识别 schema version，必须返回结构化失败，不得静默降级。

## 3. 模式定义

### 3.1 `player_parity`
- 目标：评估“这像不像玩家在玩”。
- 允许输入：受约束的局部环境信息、附近实体、当前任务提示、当前角色状态、最近少量事件摘要。
- 禁止输入：全图真值、隐藏碰撞拓扑、未来事件、未被正常感知的 AI 内部状态。
- 推荐用途：producer/QA 对照、默认体验评审、parity 证据采样。

### 3.2 `headless_agent`
- 目标：评估“在无 GUI / 无 GPU 条件下是否稳定可跑、可回放、可批量回归”。
- 允许输入：结构化局部状态、平台/碰撞拓扑摘要、附近敌人/机关状态、任务目标、最近事件、最近失败动作摘要。
- 禁止输入：绕过 runtime 规则的直接状态修改能力、未来事件、未声明字段。
- 推荐用途：CI、夜间回归、低配/无图形环境、批量 benchmark。

## 4. Observation Contract（冻结版）

### 4.1 顶层元数据
所有模式必须携带以下字段：
- `mode`: `player_parity` | `headless_agent`
- `observation_schema_version`: 首期固定为 `oc_dual_obs_v1`
- `action_schema_version`: 首期固定为 `oc_dual_act_v1`
- `fixture_id?`: benchmark / replay / parity 采证时必填
- `world_time`
- `agent_id`
- `agent_profile`
- `seed?`
- `replay_id?`

### 4.2 公共 observation 段
所有模式共享以下逻辑段，但允许粒度不同：
- `self_state`
  - `location_ref`
  - `pose_hint`
  - `velocity_hint`
  - `health_state`
  - `status_flags`
  - `equipped_item?`
- `mission_context`
  - `goal_id`
  - `goal_summary`
  - `priority`
  - `blocked_reason?`
- `nearby_entities`
  - `entity_ref`
  - `kind`
  - `relation`
  - `relative_hint`
  - `interaction_hint?`
- `recent_events`
  - `event_ref`
  - `kind`
  - `summary`
  - `age_ticks`
- `last_action`
  - `action_ref`
  - `result`
  - `failure_reason?`
  - `decision_rewrite?`
- `action_catalog_ref`
  - 引用统一动作白名单版本，而不是每轮展开未冻结的新字段

### 4.3 `player_parity` 观测约束
`player_parity` 允许的增强仅限“人类玩家可感知信息的结构化压缩”，例如：
- 允许把附近平台/实体用 `relative_hint` 表达为“前方可落脚平台”“左上敌人”
- 允许把 UI 可见状态压缩成 `health_state`、`equipped_item`
- 允许把最近显式可见事件压缩成 `recent_events`

`player_parity` 明确禁止：
- `full_map_graph`
- `hidden_collision_mesh`
- `future_spawn_schedule`
- `unrevealed_enemy_ai_state`
- `global_optimal_path`
- 任意未通过玩家正常感知可获得的精确真值

#### 4.3.1 最近事件的逐 Agent 投影

`recent_events` 与请求 envelope 的 `recent_event_summary` 必须来自同一次、同一 Agent、同一 mode 的可见事件投影；不能在 Agent 循环外截取全局 journal 后复制给所有 Agent。先判断事件和字段是否可见，再应用最近事件数量上限；内部 journal 可读不等于玩家感知授权。

| 事件类别 | 可输出条件与摘要字段 | 不能由该事件额外暴露的信息 |
| --- | --- | --- |
| 自身动作 accepted/rejected/最终结果 | 关联到本 Agent 的动作；输出动作引用、阶段、结果和玩家可见失败原因 | accepted 不冒充执行成功；内部诊断、其他 Agent 载荷不直接透传 |
| `AgentMoved` | 自身移动可输出已知起止；其他 Agent 仅输出正常感知到的移动片段及已暴露实体引用、相对位置提示 | 仅因终点可见就公开视野外 `from`；远处 Agent 的精确 `from/to` |
| `AgentRegistered`、身体或地点状态变化 | 该 Agent 可正常观察对应实体/状态；只输出该观察已允许的字段 | 未揭示的精确位置、属性或隐藏状态 |
| 意图、问答、Observation 类事件 | 本 Agent/玩家按相应交互合同可见的目标或反馈摘要 | 其他 Agent 的 Observation、内部意图全文、私有推理上下文 |
| 公共公告 | 事件种类已有明确公告合同；仅使用该合同列出的公开字段 | 将整个治理、模块或经济事件 body 默认当作公告 |
| 未定义可见规则的事件 | 不进入玩家摘要，待该事件的专业合同明确可见字段 | 因目标 ID 出现在 nearby 列表就透传整个 body |

两份摘要只允许在表现形式上不同，事件集合和字段权限相同。序列化适配与 prompt 拼装不得重新拼入原始 body；`headless_agent` 独立遵循 §4.4，不把本修正解释为所有模式均可读取全局真值。

### 4.4 `headless_agent` 观测约束
`headless_agent` 可额外暴露以下结构化辅助信息：
- `local_navigation_graph`
  - 仅限局部可达拓扑，不得直接给全图解
- `hazard_summary`
  - 附近陷阱/伤害源摘要
- `timing_window_hint`
  - 已建模的跳跃/交互窗口类别摘要，禁止泄露未来 deterministic timeline
- `interaction_targets`
  - 附近可交互对象及其约束

`headless_agent` 仍然禁止：
- 全图最优策略
- 未经声明的直接状态写入
- runtime 内部未来计划或未发生事件

## 5. Action Contract（冻结版）

### 5.1 统一动作 schema
- `action_schema_version`: `oc_dual_act_v1`
- 顶层：
  - `action_ref`
  - `kind`
  - `args`
  - `why?`
  - `confidence?`

### 5.2 首期允许动作
首期双轨模式冻结为与 Local Provider `P0` 对齐的低频动作白名单：
- `wait`
- `wait_ticks`
- `move_agent`
- `speak_to_nearby`
- `inspect_target`
- `simple_interact`

这些动作代表当前正式的间接控制 contract；`jump`、`attack`、`use_item`、`block_editing` 等具身/逐块动作若要进入产品面，必须先升级 schema，并通过独立 gameplay gate，而不是沿用本文件直接宣称为现行能力。

### 5.3 统一动作语义
- `wait`: 当前无低风险有效动作时让出一轮
- `wait_ticks`: 在明确等待外部变化时做有界等待
- `move_agent`: 向局部可达目标移动，不保证瞬移或穿模
- `speak_to_nearby`: 低成本短语义沟通，不承担复杂长对话流程
- `inspect_target`: 请求更多与目标相关的上下文，不等于执行目标动作
- `simple_interact`: 对已暴露的轻量可交互对象执行一次标准交互

### 5.4 参数约束
- 所有动作参数必须来自 `action_catalog_ref` 引用的白名单定义
- 所有枚举/目标引用必须使用 observation 中已暴露的 `*_ref`
- 不允许自由拼接未出现的目标 ID、位置 ID 或动作字段

## 6. 失败与恢复 Contract
- 所有模式共享以下失败语义：
  - `unsupported_schema_version`
  - `unsupported_agent_profile`
  - `invalid_action_kind`
  - `invalid_action_args`
  - `rule_denied`
  - `target_not_visible`
  - `target_not_reachable`
  - `provider_timeout`
  - `provider_unreachable`
  - `insufficient_information`
- 恢复原则：
  - schema / profile 不匹配：立即阻断并升级配置问题
  - 可恢复观察不足：优先 `inspect_target` 或有界 `wait_ticks`
  - 非法动作：下一轮避免复现相同行为，并在 `last_action.failure_reason` 中可见
  - 图形链路失败：记录 `fallback_reason`，不映射为玩法失败；独立 Agent Runtime 按原有 observation/action 合同及原 execution lane 继续。可以另启隔离的 headless/software-safe 调试链路，但不得静默切换 primary access mode、运行 lane 或观测权限。若实际切 lane，显式记录切换前后 lane、可见性差异和原因，并将后续样本重新归属；headless 增强样本不能计入 player-parity。`fallback_reason` 仅记录原因，不构成切换授权或验收等价证明。此处与[双模式主 PRD](provider-agent-dual-mode.prd.md)对齐，分别区分观战失败、另启调试和实际切换。

## 7. Mode Metadata Contract
以下字段必须写入 replay / summary / parity 样本：
- `mode`
- `observation_schema_version`
- `action_schema_version`
- `agent_profile`
- `provider_id`
- `provider_version?`
- `fallback_reason?`
- `environment_class`（示例：`gpu_webgl` / `software_renderer` / `headless_linux`）

## 8. 禁止事项
- 禁止 `player_parity` 与 `headless_agent` 使用不同动作语义但复用同名动作。
- 禁止未升级 schema version 就偷偷增删 observation/action 字段。
- 禁止把 headless 增强样本混入 player-parity 验收结果。

## 9. 验证要求
- `test_tier_required`
  - contract 文档冻结评审
  - fixture diff：验证 `player_parity` / `headless_agent` 差异仅在观测层
  - schema 元数据校验：所有样本必须携带 `mode/schema version`
  - 事件可见性离线反例：A 与 B 在彼此视野外，B 的 `AgentMoved` 位于 journal 最近窗口；A 的两份摘要、最终请求 JSON 和 prompt 均不含该移动的精确 body。自身、正常可见移动片段和有公开字段合同的公告分别保留；未知事件默认不输出。此回归验证感知合同，不需要真实 provider。
- `test_tier_full`
  - 同场景双模式对照采证
  - 偏差分析：成功率、等待时延、失败签名分布

## 10. 后续交接
- 交给 `runtime_engineer`：实现 `mode metadata`、replay/summary 落盘与统一失败语义
- 交给 `qa_engineer`：基于本 contract 设计双模式对照样本与阻断标准

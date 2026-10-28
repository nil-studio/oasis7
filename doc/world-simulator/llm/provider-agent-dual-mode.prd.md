# Agent 直连执行 Lane（provider: player_parity / headless_agent）（2026-03-16）

- 专题入口与权威边界: `doc/world-simulator/llm/README.md`
- 上游产品目标: [外部 Agent Runtime 自主游戏](../../product/agents-world-simulation/external-agent-runtime-play.prd.md)
- 关联专题:
  - `doc/world-simulator/llm/provider-agent-experience-parity.prd.md`
  - `doc/world-simulator/viewer/viewer-web-entry-compatibility.prd.md`
  - `doc/product/player-entry-distribution/prd.md`

审计轮次: 1

## 目标
- 建立 provider-backed Local Provider 当前组合（`agent_decision_source=provider_backed + agent_provider_backend=provider_local_bridge + agent_provider_contract=worldsim_provider_v1 + agent_provider_transport=loopback_http`）的统一执行 lane 口径（`player_parity` / `headless_agent`）；`agent_direct_connect/provider_loopback_http` 只保留为兼容 alias。
- 按 `PRD-CORE-009` 明确本专题定义的是 agent 直连接入下的 execution lane；正式玩家访问模式只有 `viewer` / `pure_api`，`software_safe` 仅为兼容 alias，不是第三种访问模式。
- 明确图形界面是可选观战/调试层，而不是 Local Provider Agent 主执行闭环的必需依赖。
- 为后续 `agent_engineer` / `runtime_engineer` / `viewer_engineer` / `qa_engineer` 的 contract、实现与验证任务提供正式 PRD 边界。

## 范围
- 覆盖当前 provider-backed Local Provider 组合的执行 lane 目标态、模式边界、统一动作语义、观测口径与验收标准。
- 覆盖 headless 回归、玩家视角对照与 Viewer 旁路调试三类使用场景。
- 覆盖与 `viewer` / `pure_api` 两种现行玩家访问模式的衔接约束，但不重定义玩家访问模式 taxonomy。
- 不覆盖本轮具体 runtime/adapter/viewer 实现细节与逐行代码方案。

## 产品目标与验收适用边界

本专题服务于[外部 Agent Runtime 自主游戏](../../product/agents-world-simulation/external-agent-runtime-play.prd.md)的持续运行目标，只拥有 execution/observation lane 的差异与可比性。下文原 P0 smoke、SC-1 至 SC-5 及六动作合同仍按其场景范围验收，不能代替 OpenClaw、Codex 各自完成产品代表性首局、生产、恢复与回访的证据。

无 GUI 运行的前提是外部 Runtime 仍在线、预算和委托仍有效；关闭观战界面不停止独立的 Agent 决策循环，关闭 Runtime 或授权到期也不会因选择 `headless_agent` 而继续获得计算或行动资格。证据中的 primary access mode 必须是 `viewer` 或 `pure_api`，本专题已有 `mode` 字段表示 execution/observation lane；两种维度分别记录，不相互代签。下文旧 software-safe 专题名与历史证据保留兼容含义。

## 接口 / 数据
- PRD 主文档: `doc/world-simulator/llm/provider-agent-dual-mode.prd.md`
- 专题入口与权威边界: `doc/world-simulator/llm/README.md`
- 关联 parity 专题: `doc/world-simulator/llm/provider-agent-experience-parity.prd.md`
- 关联 software-safe 专题: `doc/world-simulator/viewer/viewer-web-entry-compatibility.prd.md`
- supporting contract: `doc/world-simulator/llm/provider-agent-dual-mode-contract.md`
- product taxonomy: `doc/product/player-entry-distribution/prd.md`
- 追踪主键: `PRD-WORLD_SIMULATOR-040`
- 执行追踪: `TASK-WORLD_SIMULATOR-148/149/150/151/152/153`


## 1. Executive Summary
- Problem Statement: 当前 provider-backed Local Provider 路径（兼容 alias=`agent_direct_connect/provider_loopback_http`）真实试玩与 parity 采证虽然已经回答了“能否接入”“是否接近内置 agent 体验”，但默认工作流仍容易将 agent 可玩性验证与图形界面、GPU/浏览器环境耦合，导致自动化回归、低配部署与失败分诊成本偏高。若不把“图形界面是可选调试层而非主执行依赖”写成正式产品要求，后续 agent 直连玩法能力会继续受 GUI 环境稳定性制约。
- Proposed Solution: 为当前 provider-backed Local Provider 路径定义双轨执行 lane：`player_parity` 保留受约束、接近玩家的信息视角，用于评估“像不像玩家在玩”；`headless_agent` 提供不依赖图形界面的结构化观测与统一动作接口，用于稳定自动化、回归和低配环境运行。
- Success Criteria:
  - SC-1: 在无 GUI / 无 GPU / 无浏览器环境下，`headless_agent` 能完成首期 `P0` Local Provider 核心玩法 smoke，成功率不低于 95%。
  - SC-2: 同一 seed / 同一 observation fixture 下，`headless_agent` 模式的关键结果可复现率不低于 99%。
  - SC-3: `player_parity` 与 `headless_agent` 在首期纳入场景中的任务结果偏差保持在专题定义阈值内，不得破坏 `PRD-WORLD_SIMULATOR-038` 的 parity 判定口径。
  - SC-4: Viewer / Web / native 图形链路失败时，独立运行的 Agent Runtime 在预算与委托仍有效的条件下可继续执行、回放并输出结构化失败签名；图形失败不成为玩法闭环的单点阻断。
  - SC-5: 所有 agent 直连 execution lane 都必须通过同一 runtime 动作校验与回放链路，禁止出现“headless 走旁路作弊、GUI 走正式规则”的双重标准。

<a id="provider-execution-user-flows"></a>
## 2. User Experience & Functionality
- User Personas:
  - 玩家 / 制作人：希望确认 agent 直连 provider 真正在“玩游戏”，而不是只在跑脚本。
  - `agent_engineer`: 需要稳定、低成本、可批量运行的 headless 观测/动作契约，用于训练、回归与 provider 评估。
  - `qa_engineer`: 需要在无图形依赖的环境中复现问题，同时保留一条接近玩家视角的对照模式。
  - `viewer_engineer`: 需要把 Viewer 明确定位为观战/解释层，而不是 Agent 主执行依赖。
  - `runtime_engineer`: 需要所有模式共享同一动作语义、规则校验与 replay contract。
- User Scenarios & Frequency:
  - 日常 CI / 夜间回归：默认使用 `headless_agent`。
  - 制作人体验验收 / 玩家感知评估：使用 `player_parity`。
  - 低配开发机 / 无 GPU 服务器：只运行 `headless_agent`，不要求图形界面。
  - 若对外描述玩家入口，必须先标明实际对应的正式玩家访问模式（`viewer` 或 `pure_api`），再附加本专题 lane；不从有无 GUI 或 lane 名推导另一模式已经通过验收。
- User Stories:
  - PRD-WORLD_SIMULATOR-040: As a 玩家 / 制作人, I want provider-backed Local Provider agents to support both player-parity and headless execution lanes, so that we can separately judge “does it feel like playing” and “can it run stably at scale”.
  - PRD-WORLD_SIMULATOR-040A: As a `qa_engineer`, I want Local Provider gameplay regression to stay runnable without GUI dependencies, so that graphics environment failures do not block gameplay validation.
  - PRD-WORLD_SIMULATOR-040B: As an `agent_engineer`, I want a unified observation/action contract across dual modes, so that model training, replay, and debugging do not fork into incompatible paths.
- Critical User Flows:
  1. Flow-PROVIDER-DUAL-001（headless 回归）:
     `启动 Local Provider 场景 -> 注入 headless observation -> Agent 选择统一动作 -> runtime 校验执行 -> 记录结果 / 失败签名 / replay -> 输出 required 结论`。
  2. Flow-PROVIDER-DUAL-002（玩家视角对照）:
     `以 player_parity 观测模式运行相同场景 -> Agent 使用同一动作接口 -> 产出任务结果、等待时延、失败原因 -> 与 headless / builtin 样本对比`。
  3. Flow-PROVIDER-DUAL-003（模式降级）:
     `GUI / WebGL / 浏览器环境不可用 -> 图形侧记录降级或阻断原因 -> 独立 Agent Runtime 按原授权与观测合同继续 -> 若需切换 lane，显式记录观测可见性差异并重新归属证据`。图形失败不自动改变 primary access mode 或授予 headless-only 信息。
- Functional Specification Matrix:

| 功能点 | 字段定义 | 按钮/动作行为 | 状态转换 | 排序/计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| `player_parity` 模式 | `mode=player_parity`、有限视野、附近实体、当前任务与可交互对象 | Agent 仅通过标准动作集操作 | `idle -> observing -> acting -> resolved` | 以接近玩家的受约束信息为准，不得泄露隐藏真值 | 默认可用于制作人/QA 对照 |
| `headless_agent` 模式 | `mode=headless_agent`、结构化局部状态、碰撞/平台拓扑、最近事件、任务上下文 | Agent 使用同一动作集，不依赖 GUI | `idle -> observing -> acting -> resolved` | 优先保证稳定性、复现性与可回放性 | CI / 服务器默认模式 |
| 统一动作接口 | `wait / wait_ticks / move_agent / speak_to_nearby / inspect_target / simple_interact` | 所有模式共享同一低频间接控制动作语义 | `proposed -> validated -> applied/rejected` | 以 runtime 权威校验结果为准；未来 embodied 动作需单独升版并通过 `PRD-GAME-013` gate | 禁止模式专属作弊动作 |
| 统一观测版本 | `observation_schema_version`、`action_schema_version` | 模式切换必须显式暴露版本 | `draft -> frozen -> audited` | 回放与 benchmark 必须记录 schema 版本 | 仅 owner / 联审可升级 |
| 模式降级与失败签名 | `fallback_reason`、`blocked_by`、`environment_class` | 环境失败时给出显式降级/阻断 | `normal -> degraded -> recovered/blocked` | 优先区分玩法失败与图形失败 | QA / producer 可审阅 |

- Acceptance Criteria:
  - AC-1: `headless_agent` 不要求 GUI、GPU、浏览器也能跑通 Local Provider 核心玩法 smoke。
  - AC-2: `player_parity` 与 `headless_agent` 共用同一动作 contract 与 runtime 校验，不得出现“某模式专属捷径”。
  - AC-4: 回放 / summary / benchmark 产物中必须带有 `mode`、`observation_schema_version`、`action_schema_version`。
  - AC-5: QA 可以对同一场景同时产出 `player_parity` 与 `headless_agent` 对照证据，并给出偏差结论。
  - AC-6: 若 `player_parity` 未通过而 `headless_agent` 通过，系统仍不得宣称“玩家体验等价”；两条口径必须分开汇报。
  - AC-6.1: 本专题恢复 `completed` 只表示 dual-mode contract / reachability / audit remediation 已收口；是否允许默认启用或扩大覆盖范围，仍必须单独满足 `PRD-WORLD_SIMULATOR-038` 的 `behavior_parity_pass + latency_class` 门禁。
- Non-Goals:
- 本专题不追求像素级视觉智能 benchmark，也不要求 Local Provider 首期仅靠屏幕像素完成全部感知。
- 本专题不允许为 `headless_agent` 提供绕过 runtime 的直接状态改写能力。
- 本专题不把 Viewer 做成 Agent 的必需输入源。
- 本专题不把 `player_parity` / `headless_agent` 升格成新的玩家访问模式。
- 本专题不把 `jump / attack / use_item / block_editing` 之类 embodied 动作写成当前正式 contract；若未来需要，必须作为独立 candidate lane 审核。
- 本专题不单独授予某个外部 Runtime 支持声明或默认启用资格；完整产品验收由上游产品 PRD 定义，默认启用与扩面还须满足 `PRD-WORLD_SIMULATOR-038`。
- 本专题不在此轮解决全部多 Agent 并发策略，只先定义模式与契约边界。

## 3. AI System Requirements
- Tool Requirements:
  - 统一 observation adapter（可按模式切换观察粒度）
  - 统一 action API（所有模式共用）
  - replay / seed 固定能力
  - mode / schema version 元数据透传
- Evaluation Strategy:
  - 任务成功率：按场景目标达成率评估
  - 一致性：同 seed 多次运行结果差异
  - 体验偏差：`player_parity` 与 `headless_agent` 的结果偏差、等待时延、失败分布
  - 解释性：失败时是否能还原“观测 -> 动作 -> 事件 -> 结果”链路
  - 运行成本：无图形模式的耗时、失败率、环境依赖数量

## 4. Technical Specifications
- Architecture Overview:
  - Runtime 仍是唯一权威执行层，负责动作校验、事件生成、回放与结果归档。
  - Observation Adapter 负责把同一世界状态映射成 `player_parity` 或 `headless_agent` 观测结构。
  - Agent 仅通过统一动作接口与 runtime 交互。
- Integration Points:
  - `agent_engineer`: 观测/动作 contract 与 provider 适配
  - `runtime_engineer`: 权威执行、replay、mode metadata、失败签名
  - `viewer_engineer`: Viewer 观战/调试可观测性与执行通道元数据展示（含 software-safe 兼容表述）
  - `qa_engineer`: 双模式对照回归、偏差报告、阻断结论
- Edge Cases & Error Handling:
  - GUI / 浏览器不可用：图形侧记录 `fallback_reason`；独立 Agent 主流程可继续，不静默改变 primary access mode、lane 或观测权限。
  - Viewer 启动失败：只影响观战，不阻断仍在线且预算、委托有效的独立 Agent Runtime；Runtime 本身不可用时必须如实报告阻塞。
  - observation 字段缺失或 schema 漂移：阻断执行并输出结构化错误。
  - 动作非法或超权：runtime 拒绝并保留统一失败签名。
  - 模式标签缺失：不得进入 benchmark / parity 汇总，避免混淆样本。
  - 多模式结果不一致：必须生成差异报告，而不是静默选取某一路结果。
- Non-Functional Requirements:
  - `headless_agent` 默认可运行于 Linux server / CI，无图形依赖。
  - 所有模式都必须可回放、可归档、可追溯到版本与 schema。
  - 模式切换不改变 runtime 规则，只改变观测表达与验收口径。
  - 后续允许按专业合同扩展 execution/observation lane，但不得破坏现有 replay contract，也不自动新增正式玩家访问模式。
- Security & Privacy:
  - 禁止通过调试接口直接修改世界状态或绕过动作校验。
  - `player_parity` 不得泄露玩家正常不可见的隐藏真值。
  - provider / benchmark 结果必须携带模式标识，避免把“增强 headless 样本”伪装成玩家体验样本。

## 5. Risks & Roadmap
- Phased Rollout:
- M1: 定义双轨模式 PRD、追踪字段与口径边界。
  - M2: 落地 observation/action contract 与 mode metadata，优先跑通 `headless_agent` required smoke。
  - M3: 接入 `player_parity` 受约束观察，对照 `headless_agent` 与 builtin/Local Provider parity 结果。
- Technical Risks:
  - 风险-1: `headless_agent` 若暴露过强真值，可能破坏“像玩家在玩”的产品口径。
  - 风险-2: `player_parity` 若过弱，可能导致稳定性不足、评测成本过高。
  - 风险-3: 双模式若共享动作但不共享失败签名，会造成 QA 分诊断层。
  - 风险-4: 若 Viewer 仍隐式承担控制责任，名义上双轨、实际上仍被 GUI 单点耦合。

<a name="6-validation-decision-record"></a>

## 6. Validation & Decision Record
- Test Plan & Traceability:

| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-WORLD_SIMULATOR-040 | TASK-WORLD_SIMULATOR-148 | `test_tier_required` | `./scripts/doc-governance-check.sh` | 双轨模式产品口径、模块索引、owner 边界 |
| PRD-WORLD_SIMULATOR-040 | T1 | `test_tier_required` | contract schema review + fixture diff | observation/action contract、一致性约束 |
| PRD-WORLD_SIMULATOR-040 | T2/T3/T3.5 (`TASK-WORLD_SIMULATOR-150/151/152`) | `test_tier_required` | `headless_agent` / `player_parity` 真实 smoke + replay metadata verification | 无 GUI 回归主链路、mode metadata、真实 player_parity lane |
| PRD-WORLD_SIMULATOR-040 | T4 (`TASK-WORLD_SIMULATOR-153`) | `test_tier_full` | `player_parity` vs `headless_agent` 对照采证 + QA/producer 评审 | 玩家体验口径、Viewer 旁路调试与默认模式策略 |
| PRD-WORLD_SIMULATOR-040 | T4.6 (`TASK-WORLD_SIMULATOR-306`) | `test_tier_required` | repo-owned recertification evidence + `runtime_provider_compat_snapshot` + `software-safe-feedback-contract` | `completed` vs `experimental` 口径边界、provider 实际 readiness 真值与 Viewer 旁路表达 |

- Decision Log:

| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-PROVIDER-DUAL-001 | 采用双轨执行模式：`player_parity` + `headless_agent` | 只保留 GUI / 像素驱动模式 | 产品需要同时满足“像玩家”与“可回归”，单一路径无法兼顾。 |
| DEC-PROVIDER-DUAL-002 | 将 Viewer 定位为旁路观战/解释层 | 让 Viewer 继续承担 Agent 主执行依赖 | 观战层不应成为玩法验证的单点阻断。 |
| DEC-PROVIDER-DUAL-003 | 所有模式共享动作 contract 与 runtime 权威校验 | 允许 headless 走专用捷径接口 | 若动作语义分叉，会破坏 parity 与 replay 的可比性。 |
| DEC-PROVIDER-DUAL-004 | 将“无 GUI 也可运行”设为默认回归能力 | 继续把 agent 直连回归绑定在图形环境上 | 当前产品需要低成本、批量、稳定的 agent 验证能力。 |

## 风险
- 风险 1：`headless_agent` 若暴露过强真值，会让 Agent 更像“求解器”而不是“玩家”。
  - 缓解：冻结 observation contract，明确禁止在 `player_parity` 中泄露隐藏真值，并将模式标识写入所有 benchmark/replay 产物。
- 风险 2：`player_parity` 若过弱，会导致稳定性不足、评测成本过高。
  - 缓解：把 `headless_agent` 设为默认回归主线，把 `player_parity` 作为体验对照与准入门禁，而非唯一执行路径。
- 风险 3：Viewer 若继续承担控制责任，会导致名义双轨、实际仍被 GUI 单点绑定。
  - 缓解：保持 Viewer 不承担权威执行责任，并把执行通道元数据与 provider readiness 分开展示。

## 里程碑
- M1：完成双轨模式 PRD / Project 建模，并回写模块主文档、索引与 devlog。
- M2：冻结 `player_parity` / `headless_agent` 的 observation/action contract 与模式元数据字段。
- M3：落地 headless 回归主链路、统一 replay/summary metadata 与 Viewer 旁路调试标签。
- M4：完成 `player_parity` vs `headless_agent` 对照采证，形成默认模式与上线口径结论。

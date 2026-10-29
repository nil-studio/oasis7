# world-simulator PRD

> 专业域 authority：本文件拥有场景、Agent/LLM、Viewer/Launcher 与交互模拟合同，并向 [`doc/product/agents-world-simulation/prd.md`](../product/agents-world-simulation/prd.md)、[`doc/product/player-entry-distribution/prd.md`](../product/player-entry-distribution/prd.md) 与 [`doc/product/world-rules-core-gameplay/prd.md`](../product/world-rules-core-gameplay/prd.md) 汇报；涉及设施或底座时消费 [`doc/product/world-infrastructure/prd.md`](../product/world-infrastructure/prd.md) 的产品边界。

审计轮次: 9

## 目标
- 建立 world-simulator 模块的 active canonical PRD，统一当前能力边界、跨模块接口、验收总口径与 PRD-ID 追踪方式。
- 让 scenario / viewer / LLM / launcher / runtime-live 相关需求可导航、可审计，并可从主入口稳定跳转到专题文档。
- 将历史 milestone、逐任务执行证据、round review logs 和完整专题长表移出日常阅读面，避免主 PRD 继续膨胀成历史大全。

## 范围
- 覆盖 world-simulator 当前模块目标、能力主线、模式 taxonomy、跨模块接口、测试口径、风险与 active requirement baseline。
- 覆盖 PRD-ID 到 GitHub task、`doc/world-simulator/prd.index.md` 与专题 PRD 的追踪关系。
- 不覆盖实现代码逐行说明、历史过程记录、完整专题目录、逐任务执行证据或 round review logs。

## 文档边界

本文件是 world-simulator 的 active canonical PRD：保留当前模块目标、范围、跨模块接口、active requirement baseline 与验收总口径。

本文件不承载完整专题目录、历史 milestone 流水、逐任务执行证据或 round review logs。完整专题配对关系进入 `doc/world-simulator/prd.index.md`；当前执行状态进入 GitHub task issue / Project；历史证据进入 Git history、PR、实际 CI 与评审记录（Issue 按需） 与 archive-only review logs。

### Intent 批次与派生观测边界

- 同 tick intents 必须按稳定冲突键和稳定优先序完成确定性裁决，accepted actions 再按确定顺序应用并产出事件；该过程不是具备整体回滚能力的原子事务批次，不得以“统一提交”掩盖顺序应用与局部拒绝语义。
- `IntentBatchReport` 用于运行期解释和诊断；恢复真值是权威状态、journal 与事件，而不是进程内最后一份 report。
- runtime-live bridge 的 Viewer snapshot/event 同样只是观测与传输面；重连、replay 与恢复必须回到权威 state + journal/event chain，不能从 Viewer snapshot 或 DecisionTrace 反建世界真值。
- `intel_ttl_ticks` 等观测配置必须跨 snapshot 保持兼容；observation cache/hook 属于进程内派生状态，恢复时重建或清空，不能作为 Agent 长期记忆或权威世界事实。
- logistics SLA 与 threat heatmap 是由权威运输、战争和危机状态生成的观测/调度数据；heatmap 当前是 alliance/global 聚合而非空间格网，并可能被治理规则消费，因此不能描述成纯 UI 指标或独立玩法承诺。

<a id="kernel-rule-cost-contract"></a>

### Kernel 规则成本兼容边界

`PRD-WORLD_SIMULATOR-002/003` 的历史 rule-hook adapter 保留 `KernelRuleDecision.cost` 作为决策与合并结果中的数据。`WorldKernel` 当前只聚合该字段，不据此自动检查余额或扣账；字段存在也不承诺成本已作为独立记录写入 journal。已有动作自身的资源规则继续由动作实现负责，正式 runtime 的规则成本结算由 [world-runtime 专业合同](../world-runtime/prd.md) 及其执行链负责。

后续若改变该字段的结算作用，须明确付款主体、成本符号、拒绝与原子性、事件/replay 和旧消费者兼容方案，并同步需求、设计与相应行为验证。本条补回现有边界，不新增成本生效或资源结算功能。实现解释与验证关系见 [设计条款](design.md#kernel-rule-cost-boundary)。

<a id="runner-metrics-action-window-contract"></a>
### RunnerMetrics 的动作与统计窗口

`RunnerMetrics.total_actions`、`actions_per_tick` 与 `success_rate` 必须解释 Agent 动作，不能以 journal 事件数冒充动作数。同一动作的 admission、业务结果和后续派生事件只对应一个动作；`ActionAccepted` 的 `accepted_for_gameplay_processing` 不等于最终成功。系统自行产生的事件不增加动作分母或成功分子。

统计按同一世界和受支持 action identity 去重，以权威最终动作结果区分成功/拒绝；等待裁决或结果不可确认的动作不能计成功。动作接受后异步 job 的后续事件不再增加动作数，也不能把动作成功解释为 job 已完成。报告必须说明累计范围或 retained window，并对计数和 tick 分母使用同一范围；恢复、截断或缺终态不能静默重置为全历史统计。

空样本的兼容数值 `0` 不代表测得 0% 成功率；消费者必须能够区分未测、部分窗口与有效测量。现行 numeric wire 尚缺充分的范围/可用性表达时，明确标注统计不可用于验收，实施时同步兼容消费者，不能凭文档假设字段已存在。该目标补齐 [#4575](https://github.com/eng-cc/oasis7/issues/4575) 所承接的 #4478；当前 runtime-live 的事件计数和普通 simulator 的成功率占位均不构成目标完成证据。详见[指标设计与验证](design.md#runner-metrics-action-window-design)。
## 接口 / 数据

- PRD 主入口: `doc/world-simulator/prd.md`
- 文档 landing: `doc/world-simulator/README.md`
- 设计总览: `doc/world-simulator/design.md`
- 当前任务入口: 对应 GitHub task issue / Project
- 文件级索引与完整专题配对: `doc/world-simulator/prd.index.md`
- 根级旧跳转入口: root world-simulator PRD/project legacy redirect shells 已删除
- 追踪主键: `PRD-WORLD_SIMULATOR-xxx`
- 测试与发布参考: `testing-manual.md`
- 跨模块模式 taxonomy: `doc/product/player-entry-distribution/prd.md`
- 普通用户发行体验: `doc/product/player-entry-distribution/prd.md`；Pages、Release 资产、静态镜像与下载门禁的专业合同: `doc/site/prd.md` 与 `doc/site/prd.md`
- UI 视觉短期样本池: `doc/ui_review_result/README.md`
- 常用 active supporting docs:
  - `doc/world-simulator/viewer/README.md`
  - `doc/world-simulator/viewer/viewer-manual.manual.md`
  - `doc/world-simulator/viewer/viewer-visual-design-spec-2026-06-05.design.md`
  - `doc/world-simulator/viewer/viewer-brand-system-2026-06-05.design.md`
  - `doc/product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md`
  - `doc/world-simulator/prd/acceptance/unified-checklist.md`
  - `doc/world-simulator/prd/acceptance/web-llm-evidence-template.md`
  - `doc/world-simulator/prd/quality/experience-trend-tracking.md`
  - `doc/world-simulator/launcher/game-client-launcher-cross-surface-action-parity.prd.md`

## 模块基线
- world-simulator 连接 runtime 与 viewer，负责把世界状态、场景初始化、规则执行、LLM/provider 决策与启动器控制面收敛为可交互体验。
- 模块覆盖六个主题域：scenario、viewer、launcher、kernel、llm、m4；热点子域和完整文档清单以 `prd.index.md` 为准。
- 主 PRD 只保留模块级 Why / What / Done；专题级 How、细节矩阵、历史 decision 和任务证据由专题 authority、PR、实际 CI 与评审记录（Issue 按需） 与 archive-only review logs 承担。已完成的 simulator kernel rule-hook / Wasm 接线历史以 `design.md` 的简明边界说明和 Git history 追溯，不恢复为 active 专题三件套。

## Active Requirement Baseline

| 领域 | Active PRD-ID / 专题 | 当前模块级要求 |
| --- | --- | --- |
| 场景与世界契约 | `PRD-WORLD_SIMULATOR-001/002/003` | 场景初始化、资源变化、关键交互和发布证据必须可复现，并通过统一验收清单映射到测试证据。 |
| kernel builtin resource boundary | `PRD-WORLD_SIMULATOR-002/003` | 内建 `ResourceKind` 固定为 `Electricity | Data`；`Compound/Hardware` 不是兼容别名或内建输入。工业材料、模块资产、ABI 与执行权限分别服从 M4/domain 与 world-runtime/WASM 专业 authority。 |
| kernel rule cost adapter | `PRD-WORLD_SIMULATOR-002/003` | [规则成本兼容边界](#kernel-rule-cost-contract)：`KernelRuleDecision.cost` 只在决策合并中保留，不自动结算，也不证明存在持久成本记录。 |
| 多阶段工业批次与资源流 | `PRD-WORLD_SIMULATOR-047` / `m4/industrial-resource-flow-contract.prd.md` | 中间品批次适用性、数量有界的权威预留、有限 buffer/背压、确定性 split/merge 与跨阶段 lineage 必须由 M4/domain 和 runtime 专业合同共同约束；产品 SC-21 组合玩家结果，但不替代专业执行语义或声称当前已实现。 |
| runtime live / LLM / provider authority | `PRD-WORLD_SIMULATOR-016-019`, `036-040` | runtime live 是主驱动方向；provider 决策层保持 provider advisory / runtime authoritative。provider 失败产生结构化 trace/error，并可收敛为不提交动作的 `Wait` 或显式 catalog/runtime rejection；不得回退到状态变更的启发式替代动作。runtime event/snapshot mapping 不得静默丢字段，trace/action/result 保持因果关联。AgentChat、PromptControl、profile/override、鉴权与实际应用结果属于本专业域；产品层只组合对话、草稿、持续调整与反馈恢复的玩家语义。 |
| primary Web viewer | `PRD-WORLD_SIMULATOR-039/041/046` | `viewer` 是低保真但正式可玩的主要 Web 入口；`software_safe` 仅作为兼容 alias；退役第二 Viewer surface 已退出 active delivery。 |
| first-session goal and control feedback | `PRD-WORLD_SIMULATOR-001/039/041/046` | Viewer 首局面向玩家呈现一个主目标及可收起、可找回且不冲突的次目标；主目标消费权威进度并展示动作、完成条件、阶段预期、剩余条件、blocker 与玩家可选择的恢复动作。首个推荐需解释价值、可达性与首次持续能力关联；完成后交接到可达的后引导目标。Web Test API 以 `describeControls()` / `fillControlExample(action)` 暴露支持动作、payload schema 与示例，`sendControl(action, payload)` 稳定返回 accepted、action、parsedControl、reason、hint，`getState().lastControlFeedback` 保留最近反馈供人工与脚本复核；新增字段不得静默破坏自动化兼容。正式玩家摘要只从 canonical `player_gameplay` / feedback 派生 Player Intent、World Consequence、Recovery Move、Next Move 等解释面，不新增 runtime truth。具体阈值、布局与测试锚点由当前 Viewer 实现和验证证据维护，不进入产品层。 |
| launcher feedback / transfer / explorer / control plane | `PRD-WORLD_SIMULATOR-020-031`, `033/034/044` | native/Web Launcher 的受控动作共享真实的 executable/blocked/result/recovery 语义，但保留平台字段与适配差异；稳定入口为 `launcher/game-client-launcher-cross-surface-action-parity.prd.md`。submit acceptance 不等于 settlement，近期历史不等于持久账本；反馈、explorer、链状态、GUI Agent 和 stale execution-world 恢复仍由各自专业 authority 维护。 |
| slot-1 onboarding | `PRD-WORLD_SIMULATOR-045` | 新账号玩家读取 canonical quote 并显式确认后直接进入 `ClaimAgent`；dedicated pool 足够时自动补足 restricted starter amount，不生成 operator-review 审批状态。 |
| release asset entrypoints | `PRD-WORLD_SIMULATOR-042/043` | 三平台公开资产必须提供平台原生安装或启动入口，并通过 bundle/installer 入口验证；`doc/product/player-entry-distribution/prd.md` 只组合玩家体验，普通用户发行的 Windows codesigning trust chain 与 macOS notarized `.app + .dmg` blocker 仍由本专业域和站点流水线处理。 |

## 用户与模式边界
- 模拟层开发者需要统一场景与运行语义。
- Viewer 体验开发者需要明确 Web/Native 行为边界、模式 taxonomy 与验收标准。
- 发布与测试人员需要可执行的闭环测试、日志/截图/指标与证据产物。
- 启动器玩家需要在同一入口内完成链状态查看、资产查询、转账、explorer 与反馈，不依赖命令行。
- 新账号玩家需要可解释的 `slot-1` direct claim：quote、auto-funding amount、资金缺口、claim result 与 provenance 必须玩家可见。

模式分层按 `PRD-CORE-009` 执行：`viewer` 承接低保真但正式可玩的主要 Web 入口；`software_safe` 仅作为兼容 alias / 历史入口名保留；仓库已删除旧第二 Viewer surface，不再保留独立第二 Viewer taxonomy。Local Provider 当前路径使用 `agent_decision_source=provider_backed + agent_provider_backend=provider_local_bridge + agent_provider_contract=worldsim_provider_v1 + agent_provider_transport=loopback_http`；`player_parity` 与 `headless_agent` 是 execution lane，不是新的玩家访问模式。

## Critical Flows
1. Web-first 闭环：选择场景 -> 启动 Viewer Web -> 执行关键交互 -> 采集日志/截图/指标 -> 产出 `test_tier_required` 结论。
2. LLM / provider loop：触发 prompt/chat/provider 决策 -> runtime 权威校验 -> 执行动作、traceable Wait 或结构化拒绝 -> 归档 trace 与错误语义；provider failure 不触发启发式替代动作。
3. Runtime-live bridge：一个可处理 mailbox/event 触发一次 runtime drive -> runtime step/action/event -> viewer 兼容快照/事件 -> DecisionTrace / error context 可被订阅和复核；空 mailbox/零结果不伪造进展，重连/replay 不重复执行已接受动作。
4. Launcher transfer / explorer：链状态就绪 -> 提交转账或查询 explorer -> 返回 action_id / tx_hash / structured error -> native/web 同层展示。
5. Slot-1 direct claim：读取 quote -> 显式确认 -> dedicated pool auto-funding 或 insufficient-funds blocker -> `ClaimAgent` result/provenance 可见。
6. 3D 暂停路由：新的 3D scene/camera/render/material/light/post-process 需求进入 paused pool；非 3D viewer / launcher / runtime interaction 继续走 active delivery。

## Cross-Module Interfaces
- `world-runtime`: runtime/world authority、DomainEvent、storage/output path、provider / LLM action bridge、module/WASM gates。
- `viewer`: Web entry, auth/bootstrap, right-panel state, diagnostics, visual design and manual surfaces。
- `testing`: `testing-manual.md`, required/full tiers, agent-browser/Web-first evidence, playability cards and release gates。
- `site`: static viewer manual mirrors are read-only publication surfaces; canonical manual remains under `doc/world-simulator/viewer/`.
- `core`: player access mode taxonomy, release readiness and cross-module governance contracts。

## Edge Cases / Invariants
- 网络、链状态、provider、LLM 或 explorer 请求失败时必须返回结构化错误，保留可诊断上下文，避免 silent fail。
- 转账与 chain-facing action 必须经过 nonce anti-replay、余额约束和 runtime 权威校验。
- Web / native / bundle-first 路径必须保持 operator-facing 入口一致，但允许按平台分流 native-only 字段。
- `viewer` Web 主路径不得依赖高保真 3D 硬件前提；图形环境 blocker 必须与 gameplay/protocol bug 可区分。
- 历史品牌、旧 alias、兼容 payload 或负向测试输入只允许作为兼容/移除语义存在，不得重新成为当前入口真值。
- 新增 runtime event/state 必须经过版本化 schema 或 exhaustiveness gate；snapshot/event 保持因果顺序，恢复只认权威 state + journal/event chain。

## Non-Functional Requirements
- 启动器链状态探针刷新周期默认 <= 1s，状态可见延迟 <= 2s。
- 本地链路下转账提交、explorer 查询和 GUI Agent 查询类动作应保持可交互延迟，失败路径需在可诊断时间内返回结构化结果。
- Viewer / launcher / runtime-live required 路径必须能产出命令、产物路径、指标和结论证据。
- 日志与证据不得输出私钥、口令或完整凭据；敏感字段必须脱敏。
- 新专题细节应优先进入专题 PRD/design/project 或 `prd.index.md`，避免主 PRD 回到长表聚合。

## Validation
- 文档治理与引用可达：`./scripts/doc-governance-check.sh`
- Kernel cost 边界：按 [设计验证映射](design.md#111-验证映射表) 回读决策聚合与动作执行入口；现有 hook 测试不得被表述成已经验证非零 cost 不扣账。

- Markdown / whitespace sanity：`git diff --check`
- 行数回归观察：`wc -l doc/world-simulator/prd.md doc/world-simulator/README.md doc/world-simulator/prd.index.md`
- 行为变更、runtime/Viewer/WASM/launcher 语义变更不由本 PRD 瘦身任务放行；需按对应专题 PRD 的验证矩阵和 owner role 复核。

## Risks & Roadmap
- 风险-1: 前端体验迭代快，若专题索引和 project 追踪不同步，仍可能出现入口漂移。
- 风险-2: LLM / provider 外部依赖波动会影响端到端稳定性，必须保持错误语义与 evidence template 可用。
- 风险-3: 分册索引维护缺失会导致需求追踪断链，因此完整专题配对关系必须继续由 `prd.index.md` 承担。
- Roadmap: 主 PRD 继续保持 active spec；新增大专题优先进入子域 PRD/design/project；若某子域再度高体量，按 `viewer/README.md` 模式补路径级 landing。

## Decision Record
- DEC-WS-001: 采用 Web-first 作为默认 UI 闭环链路，native 或外部浏览器作为环境差异复核路径。
- DEC-WS-002: 主 PRD 保持模块级总览和验收总口径，专题细节下沉到 `doc/world-simulator/**` 专题文档与 `prd.index.md`。
- DEC-WS-012/014: Viewer live 迁移采用 runtime 驱动、协议兼容适配和 runtime-only 收敛，避免长期 simulator fallback 双轨。
- DEC-WS-024: 暂停新的 3D 可视化推进，把当前交互主路径聚焦到 `viewer` Web、launcher/runtime interaction 和 gameplay closure。
- 历史 decision log 与逐任务 traceability 由 `doc/world-simulator/prd.index.md`、Git history 与 PR、实际 CI 与评审记录（Issue 按需） 保持可追溯。

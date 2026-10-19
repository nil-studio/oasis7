# Viewer 发行体验总改造设计

- 对应需求文档: `doc/world-simulator/viewer/viewer-gameplay-release-experience-overhaul.prd.md`
- 历史阶段实施与验收记录：GitHub task issue evidence。

## 1. 设计定位
本文是 Viewer-domain 的视觉/交互配对设计；产品语义仍以
[`doc/product/agents-world-simulation/player-readable-world-stage.prd.md`](../../product/agents-world-simulation/player-readable-world-stage.prd.md)
为上游权威，终端 shell 的 Viewer 边界以配对 PRD 的
`Terminal Viewer Authority` 为准。本文不把目标 shell 当作已发货或发行放行事实。

## 2. 设计结构
- **Player shell（当前默认）**：全舞台世界棋盘、紧凑权威 HUD、低存在感 edge dock、按需 contextual console。
- **Director（显式 opt-in，安全边界已实现）**：可保留密集工具，但不改变玩法语义、进度或回执因果；生产成功进入仍依赖可信 issuer。
- **信息层级**：`World / Targets / Command` 为主路径；`Search` 是 Targets 内的
  `#entity-search` 兼容过滤/导航能力，`Diagnostics` 次级；`Quote` 属于 Command/Console 上下文。
- **反馈**：`Action Receipt` 负责玩家因果，已实现的 `world_feed/v1` 仅是环境上下文投影。
- **Fullscreen HUD**：桌面以 `Next Move` 主动作区和 `Objective / Player Leverage` 从属
  上下文区并列；窄屏把主动作放在第一行、上下文放在第二行，保持 DOM 与键盘顺序一致。
  `More / Diagnostics` 仍从次级入口可达。

### 2.1 需求承接与分配表

本表承接首局方案、分支指引、吸引力样例以及 Viewer 性能要求；未触及的 shell、权限及发布条款保持其原有适用范围。

| 上游 requirement / acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-FIRST-BRANCH-001](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-branch-001) Trace refs: O7-REQ-4c622b94-22bd-4559-9595-6b134bdc00b1 | 当前区域专业化的主指引与完整分支承诺同义，保留真实可用工业动作 | [分支投影职责](#branch-stage-projection) | gameplay / runtime 发布动作；Viewer 消费 | 不新增治理或冲突机制，不重排 shell |
| [REQ-FIRST-BRANCH-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-branch-002) Trace refs: O7-REQ-2e82fef6-bbb5-43b6-a7d0-833905e8e8a3 | 回退报价和样例 gate 在截止前、边界及恢复后保持同一窗口判断 | [吸引力样例回退窗口](#fixture-rollback-window) | gameplay 样例阶段；Viewer / QA 结构回归 | 不新增真实 Runtime 回退，不代签真人可玩性 |
| [REQ-FIRST-INDUSTRIAL-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-002) Trace refs: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95 | 查看、查询、提交及成果表达同一对象和实际方案。 | [当前方案交互](#starter-plan-interaction) | gameplay/M4 参数与报价；Runtime 因果；Viewer/QA 入口验证。 | 不重建 Shell/Inspector，不由 UI 补算成本。 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005) Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | 报价失效、迟到响应和未知结果保留原请求语义，不重复提交或误归因。 | [当前方案交互](#starter-plan-interaction) | M4/Runtime identity 与重验；Viewer/QA 时序验证。 | 不新增恢复权限，不以退出界面取消已提交动作。 |
| [REQ-FIRST-INDUSTRIAL-008](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-008) Trace refs: O7-REQ-757636b1-c9e9-4e7a-97d1-460de105daf3 | 首产物成果连接真实后继目标与自然离场，重入保留既有成果和未决义务。 | [当前方案交互](#starter-plan-interaction) | gameplay 后继条件；Runtime 结算；Viewer/QA 续玩验证。 | 不把候选当建成，不扩张 Agent 授权或生成离线收益。 |
| [REQ-ENTRY-PERFORMANCE-001 / AC](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-performance-001)；Trace refs: O7-REQ-7e64b369-3c32-4671-a1af-205bc5a75251 | 世界刷新时可交互，过载可解释且不丢事实 | [显示投影与调度](#viewer-render-projection) | Viewer Host / bridge / QA；Runtime 继续提供权威快照 | 不承诺任意世界规模或低端设备，不改变动作执行 |
| [REQ-ENTRY-PERFORMANCE-002 / AC](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-performance-002)；Trace refs: O7-REQ-25030f5c-c246-4b72-b637-de9dc6b520ae | 有界等待，同版本复用，损坏/版本切换安全恢复 | [资源交付与恢复](#viewer-payload-delivery) | Web build / loader / HTTP deployment / QA | 不解决账号、provider 或后端故障，不开放离线可玩 |

## 3. 当前实现与目标边界
- 当前 `main` 的 Player 是 fullscreen、world-first 单舞台：世界棋盘为首屏主体，
  `Objective / Next Move / Player Leverage` 是叠加 HUD，`World / Targets / Command`
  由按需 drawer 提供。旧“左侧/中间/右侧”三栏只适用于显式、能力门控的 Director
  工具密度，不能作为 Player 的当前 IA；主动作及从属上下文也不是常驻工具栏。
- 当前实现保留 focus/right-panel 兼容 hooks，且 `#entity-search` 仍是 Targets
  过滤入口；这些是实现基线/债务，不是第二产品模式或终端 authority。
- 当前页面仍以 Recent Events/Gameplay Feedback 等现有投影为准；它们不能被重命名为
  World Feed。独立 `world_feed/v1` 由 runtime journal 投影并通过 Viewer transport/panel
  呈现，`#viewer-world-feed` 是稳定实现 anchor，`receipt_ref` 无明确 runtime 身份时为 `null`。
- 目标契约暂不声称存在 `OASIS7_VIEWER_EXPERIENCE_MODE`、`ViewerExperienceMode`
  或 `RightPanel*` 运行时资源；需要这些字段时另开 runtime/viewer slice。
- Director 的 capability verifier、fail-closed endpoint 和 Player↔Director state machine
  已实现；launcher 无可信 issuer 时返回 `director_capability_unavailable`，所以这是
  已实现安全边界而非已放行的 operator 能力。目标实现仍须由 Viewer/QA 以真实桌面、移动、
  键盘和空/不可用状态验证。

<a id="shell-interaction-recovery"></a>
### Shell 交互与恢复实现边界

本节对应配对 PRD 的 Shell 局部合同。2026-10-12 基准 `5e930fa` 已有路由、Prompt 草稿、
列表/舞台投影、Feed state 和 renderer loader 接点；待修的具体时序按 #3425 独立验收，
不因同一文件已有功能就宣称该路径通过。

- 路由切换在一个 transition 内协调受管理浮层、disclosure、aria 与 focus。离开 Diagnostics
  时关闭实际 fixed 面板，而非仅改 hash；只管理注册的 shell surface，保留普通 disclosure。
  Escape 使用实际打开状态和既有 IME 优先级，返回入口仍可获得焦点。
- 选择模型与草稿模型分离：同 Agent 重选只刷新 selected object；dirty 草稿继续引用原编辑
  baseline。显式 reload 或对应 Apply 的权威确认才按原规则刷新草稿；新的授权失效仍优先
  进入安全 blocked，不借“保护草稿”显示无权读取的新值。
- Targets filter 在列表 projection 上执行。host/bridge 输入使用同一快照中受会话可见性约束、
  未被关键词裁剪的实体/地点/设施集合；真实删除和权限变化在源集合生效，不用旧场景缓存补回。
- Feed transport 的 page 状态与合并后的显示状态分开。state reducer 在同 world/epoch 的
  连续 cursor 上保留/去重已有窗口，空增量不清空也不覆盖为 empty；replay 未追平前仍保留
  replay 语义。panel 与 HUD 共用这一结果；gap/reorg/unavailable 按原恢复边界处理。
- renderer loader 区分未初始化、在途、成功和失败。失败 promise 不得永久缓存为后续 Retry
  的结果；只清除该失败 attempt 的引用，避免迟到失败清掉较新的初始化。每次重试重新走
  descriptor、bytes、size/hash 与 init；当前在途请求共享，成功实例复用，持续失败保持
  unavailable。Host Retry 不触发整页刷新、测试模块重置或另一 renderer 的隐式切换。

| 独立场景 | 最小有效验证与现有接点 | 不能证明 |
| --- | --- | --- |
| S3 导航/浮层 | `viewer_terminal_shell` 路由及相关 UI 测试；实际 1440×1000、390×844 下 More→World/Back/Forward、Targets/Command→World，核 geometry/visibility/focus 与 IME | hash 或 DOM attribute 单独正确不证明舞台可操作 |
| S4 同 Agent 草稿 | 实际 selection→state→Prompt panel 路径，覆盖 dirty/clean、成功 Apply、明确 reload、跨对象、control-lost/stale | 草稿存在不证明授权或应用成功 |
| S5 搜索与舞台 | `pixel_world_host` 及列表集成测试：输入关键词、再接收 snapshot，核列表裁剪而世界 entity/fragment/facility 不受裁剪，另核删除/不可见 | mock 列表结果不证明真实渲染 |
| S7 Feed 空增量 | `world_feed_transport`→`world_feed_state`→`world_feed_panel` 与 HUD；非空→同 cursor 空、多页 replay、追平、首次空、gap/unavailable | reducer 单测不代签 transport/panel；空页不证明 LIVE |
| G3 页面内 Retry | `pixel_world_runtime_module_wasm` 和 Host 集成：同模块首次 503、下一次正确 bytes、连续失败、成功复用和并发 single-flight；可见变化再做 S6 | 测试 reset/重载不证明页面内恢复；mock bytes 不证明 GPU readiness |

这些都是后续行为验证的选择规则；本次只修改合同，不启动 Runtime/provider 或宣称浏览器验收。

### Expanded visual contract (target handoff)
- Player HUD 的层级固定为条件性 attention、`Objective`、一个主导性的 `Next Move`
  动作、`Player Leverage`/selection context、`Action Receipt`；World Feed 仍是收起的
  ambient chip。`world_read` 的 `tick/agents/routes/fragments/hotspots` 只有在既有
  projection 明确发布时才可作为次级摘要，不能假设 Power/Data/Consensus 等字段存在。
- `Next Move` 使用单一 CTA + 内联 blocker；不要复制 Stage Hero 文案或让 World Feed
  代替 receipt。主动作/从属上下文的结构已实现；剩余视觉和交互验收按当前候选、场景及
  受影响表面记录，不能把结构存在当作整个视觉目标已通过。
- Agent Console 默认先显示 selected context、授权 command/chat、blocker 与 receipt；
  raw snapshot/JSON、auth/capability、prompt override、world-scale 和 Director 工具
  默认收起到 `Diagnostics / More`，不改变控制权或 fail-closed 语义。
- Focus 的用户-facing 名称为 `Cinematic View / Minimal HUD`；保留 `focusMode` 兼容
  hooks。显式进入后只保留世界、选择标记、当前 objective/blocker、存在中的 receipt 和
  退出入口；Escape 按既有 local-surface → Focus 顺序处理，IME composition 优先。
- 这些约束不新增协议/runtime 字段，不改变仿真、replay、renderer、Director capability、
  ownership、经济或玩家权限；真实 headed 证据必须覆盖 `1440x1000` 与 `390x844`。

### Player visual handoff（既有结构与持续验收边界）

The next bounded visual pass uses this reading order:

`connection/renderer/blocker attention → Objective → one dominant Next Move → selected Agent context / Player Leverage → Action Receipt → collapsed World Feed`.

- Objective and Next Move must not repeat the same instruction. Objective and Player Leverage
  share the supporting context; only Next Move has the dominant actionable surface. The current
  `PixelWorldCommercialHud` and terminal-shell CSS implement that structure, with Next Move first
  on narrow screens. Geometry, input and accessibility acceptance remain candidate-specific.
- Action Receipt outranks ambient events and helper copy whenever present. Player and Cinematic
  must not show duplicate causal receipts; first verify the current normal/compact overlay geometry
  in a headed browser, then make presentation ownership explicit rather than masking overlap.
- Player-facing identity uses `name → label → id`. Raw IDs, `boundPlayer`, internal enums, and raw
  confidence values are secondary metadata or Diagnostics. Accessible names follow the same rule,
  selection has semantic state, and visible focus cannot depend on color alone.
- `LIVE` is a factual status, not decoration. The HUD must visually distinguish current, stale,
  reconnecting, offline, and renderer-unavailable states only from authoritative existing state;
  it must not synthesize liveness from an old snapshot or a running animation.
- Renderer Unavailable keeps the world shell usable and offers an honest visible recovery action.
  Retry does not promise that an incapable browser can acquire WebGL2 support.
- On narrow screens, Command keeps the selected readable Agent identity, truthful status,
  Objective, and close/back path visible while details scroll. Cinematic entry, readout, feed,
  fallback, diagnostics, and receipt each need a distinct safe-area slot.

#### Agent Context presentation boundary

`Agent Context Lite` may present explicit selected identity/location, Objective, published Next
Move, blocker, Player Leverage/execution state, and the latest explicit causal Action Receipt.
Non-causal feedback may remain visible as status only; it is never a Receipt. The formal causal
surface is the existing Viewer `data-viewer-overlay="receipt"` surface, with the existing
`#viewer-action-receipt` Player anchor where applicable, and there must be exactly one per active
Player/Cinematic presentation. Unknown, missing, local-pending, and stale values remain visibly
distinct. World Feed, recent event text, cursor movement, wall-clock time, coordinates, or raw debug
objects cannot create intent, progress, success, ETA, relationship, rationale, or a second Receipt.

`Agent Console V2` is a later contract-gated design surface. ETA, energy, cargo, module health,
relationships, plan/memory/rationale, provider or cost data, multi-Agent graphs, and new control
verbs require producer/runtime/agent definitions for source, freshness, permission, missing-state,
idempotency, and receipt semantics before visual design is treated as implementable.

#### P1-A Agent Context Lite（既有实现与分项验收边界）

P1-A 先实现一个 Agent-only composition，作为后续实体 Inspector 的窄入口；Location、
Facility、Territory、Organization、Depot、Module 的 generalized Inspector 明确是 post-P1-A。
它不把当前 Player shell 扩成常驻三栏，也不声称已实现完整的 Agent Console V2。

- **Composition seam**：设计绑定现有
  `crates/oasis7_viewer/software_safe_src/viewer_app.jsx` `InteractionPanel`。该组件继续负责
  Command/chat 的 route 和 control boundary；Agent Context Lite 作为其中的 selected-Agent
  display region，而不是新的 dock 或独立 runtime surface。
- **Display-model seam**：canonical extraction 固定为
  `crates/oasis7_viewer/software_safe_src/viewer_agent_context_display_model.js`（纯
  source-level projection）与 `crates/oasis7_viewer/software_safe_src/agent_context_lite.jsx`
  （只接收 display inputs/callbacks 的组件）；二者已实现。输入只包括
  当前 selected Agent、`snapshot.model.agents[selectedAgentId]` 的显式 identity/activity、
  显式绑定同一 Agent 的 gameplay projection、connection/freshness 和既有
  semantic feedback/explicit receipt。反馈只能输出 non-causal status；它输出可渲染的
  sections 与 source/freshness/permission 状态，不发送动作、不写 core state、不读取 raw
  debug object 来补字段。Player-global `core.buildGameplaySummary(locale())` 保留在 Player HUD，
  不得注入 Agent Context；`InteractionPanel` 只负责组合，不继续 inline expansion。
- **Existing semantic surfaces**：`crates/oasis7_viewer/software_safe_src/agent_activity_surface.jsx`
  仍只表达 activity，`crates/oasis7_viewer/software_safe_src/agent_intent_surface.jsx` 仍只
  表达权威 Intent；它们可被组合但不能合并语义。世界舞台
  `crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx` 的 `PixelWorldCommercialHud`
  和现有 `Action Receipt` 保持回执所有权，Context Lite 不创建副本。
- **Reading order**：Command/chat → Agent Context Lite → gameplay details → collapsed
  Diagnostics/raw state。Context 内部为 Identity → State/freshness → Objective/Next Move /
  blocker/Leverage → explicitly matched Intent；Next Move 仍只有一个主 CTA。
- **Non-Agent selection**：Location、Facility、Territory、Organization、Depot、Module 选择时显示选中实体 identity
  与 honest unavailable/待同步状态，不保留或借用上一 Agent 的 State、Intent、capability、
  owner、resource 或 relation。只有未来 projection contract 齐备后才可增加对应 section。
- **Causal discipline**：Context 只引用当前 Player/Cinematic 的唯一 Receipt anchor。Feed、
  recent events、chat echo、cursor、coordinates 和 wall-clock 不得产生 Intent、progress、
  success、ETA、relationship 或 rationale。

P1-A 的视觉验收只针对 selected Agent、缺失/陈旧/不可用态和非 Agent honest-unavailable 态；
必须覆盖 `1440x1000`、`1024x768`、`768x1024`、`390x844`、键盘/IME/CJK/长文本、Focus 返回
路径和单一正式 Receipt，并覆盖 current、last-known/stale、reconnecting、unknown、conflict、
replay、gap、reorg、unavailable 与 control/permission lost。Context Lite 源码与聚焦自动化已经
存在。早期外部 Chrome 样本只覆盖 GPU-unavailable fallback（`1440x1000`、`390x844`），
不能据此给 `ready=true` WebGL2/Cinematic、probe 后异步失败或 tablet `641–1240` 放行。
后续 #3624/#3638/#3668 的证据按各自候选、场景与表面范围保留；旧 fallback 样本不再表示
全部后续工作未验收。本节不单独构成完整 headed 验收或发行放行结论。

P1-A 的 canonical acceptance crosswalk 为：`P1A-CTX-01 → ENT-01/CTX-01/EMP-01`、
`P1A-CTX-02 → STA-01/CTX-01/FED-01/FED-02`、`P1A-CTX-03 → INT-01/REC-01/REC-02`、
`P1A-CTX-04 → CAP-01/CON-01`、`P1A-CTX-05 → IA-01/HUD-01/TXT-01/FOC-01/FOC-02`。
这些是分项实现与 QA 的证据映射；发行判断仍需适用的当前候选证据，不恢复已退休的历史回执流程。

#### P1-B World semantic presentation（当前切片源码与 headed 验收已完成）

P1-B 将既有权威 world projection 组织成稳定的 stage visual language。可呈现的内容包括
Agent、Location、Facility、Territory、Organization、Depot、Module 的 identity/state/activity，
以及已发布的 route/assignment、power/resource、facility/module cues 和 relation kind/status；
没有 projection 时保持 unknown/unavailable，不由 Viewer 补造。

- **Attention order**：普通 Agent < selected Agent < active Intent < blocker < receipt target。
  Major event 只在 P1-C authority gate 通过后作为后置扩展；persistent selection 与 transient
  attention 必须有不同的视觉/语义状态；
  hit target、keyboard focus、selected state 和 readable identity 在高注意力状态下仍保持。
- **Authority boundary**：relation、Trust、ownership、resource、capability 和事件因果只能
  来自权威 projection；距离、同屏、proximity、文本、坐标或 renderer activity 不能推断
  这些语义。关系线只有在 projection 提供 relation kind/status 时才可绘制。
- **Renderer separation**：Renderer Unavailable、probe 后异步失败、fallback、retry、
  safe-area 和 recovery 属于跨切片安全验收（`REN-01`），不是 P1-B 的语义来源。P1-B 不新增或
  修改 upstream runtime/world/protocol/WASM ABI 字段；Viewer host/bridge 只可从既有 authoritative
snapshot 派生 additive、只读、fail-closed 的 presentation DTO，不得新增事实、权限、生命周期、
控制或因果语义。`active_intent_target` 只能来自 `snapshot.player_gameplay.primary_intent`。
  即使 Intent/Agent position 存在，只要 authoritative world bounds 缺失，也不得用 Agent ID 或
  fallback 坐标生成 stage cue。
  headed evidence 已覆盖 `1440x1000`、`1024x768`、`768x1024`、`390x844`、键盘/CJK/长文本，
  并以语义层级、焦点与无横向溢出为判据，而非把 canvas readiness 当作语义证明。

#### P1-C Major World Events（当前切片已实现 crisis-only 最小合同）

runtime 已用独立 additive projection 冻结 crisis-only authority：三种 canonical Crisis lifecycle、
severity `1..=5`、journal identity、显式 freshness/permission、非 Receipt lineage 与 gap/reorg
清理规则。World Feed wire 保持向后兼容，Viewer 对缺失、未知、冲突或未授权字段 fail closed。
当前 crisis 没有权威空间 anchor，因此只显示 ambient feed/status，不生成 marker/highlight，也不从
event text、距离、同屏、时间、cursor、resolver 或 renderer activity 推断空间与因果。War、
Governance 和其他类别仍明确排除；生产入口只读取独立的 runtime/operator audience policy，
默认 Unknown，不从登录、选择、控制或 Director diagnostics 推导 Public。

### World-native interaction composition (target handoff)

The visual reference contributes a stable frame, not a permanent Player layout. Keep stable slots for
the stage, Objective, selected-agent context, one Next Move, Receipt, and edge navigation; let
Inspector, Command, Feed, and Diagnostics appear on demand and return focus to their invoker. A
Player left/center/right column or an always-on bottom bar is explicitly rejected.

- **World explains semantics**: stage marks should make published identity, state, activity, route,
  relationship kind/status, and major events legible at the entity. UI copy may explain or act on the
  mark, but may not turn the stage into background decoration or infer a relation from distance,
  proximity, co-presence, or event text.
- **Entity-first context (post-P1-A)**: P1-A is selected-Agent-only. A later P1 slice may let
  selecting a Location, Facility, Territory, Organization, Depot, or Module open a consistent
  Inspector/Console composition, but it cannot override or widen the Agent-only P1-A boundary.
  Market, War, Governance, Production, Contracts, and Relations are contextual capabilities,
  filtered by published data and player permission, not peer navigation routes. A missing projection
  is an honest unavailable state.
- **Indirect-control rhythm**: the primary action is a high-level guidance/template (`Ask`, `Suggest`,
  `Prioritize`, `Negotiate`, `Investigate`, `Propose`, `Warn`, or `Delegate`). Render the sequence
  `guidance → Intent → accepted/rejected/blocked → Activity → world action → Receipt`; accepted is
  never executing, executing is never completed, and only a bound receipt can announce world change.
- **Persistent causal anchor**: keep one compact Receipt anchor visible for the active Player or
  Cinematic presentation until a newer authoritative receipt replaces it. Transient toasts may call
  attention but cannot replace or create a receipt. Keep World Feed as a collapsed ambient chip with
  loading/empty/replay/gap/unavailable states, never as causal feedback.
- **Player language**: use Program, Protocol, Module, and Contract in ordinary copy. Keep WASM, ABI,
  hashes, deployment transactions, runtime IDs, provider details, and auth material behind explicit
  Diagnostics/Director disclosure; raw fields do not create an apparent capability.

Visual states must carry source and freshness cues without exposing raw enums: current, Last known,
stale, reconnecting, unknown, conflict, unavailable, and control lost need distinct treatment. Missing
or unauthorized data is hidden/redacted or labeled unavailable; it is never replaced with a guessed
resource, owner, trust, relationship, liveness, or action result. These are target design rules, not
evidence that the current implementation already satisfies them.

#### Headed visual and interaction matrix

- `1440x1000`: GPU-ready Player and Cinematic; one Receipt; default hierarchy and focus return.
- `1440x1000`: Renderer Unavailable, visible recovery, and post-probe asynchronous failure.
- `1024x768` and `768x1024`: tablet/narrow-desktop HUD, feed, drawer, receipt, and safe areas.
- `390x844`: Player, Targets, Command, Cinematic, long entity names, long receipt copy, fallback.
- `390x844`: empty world and missing/stale/permission-lost entity data retain a usable stage,
  explicit recovery copy, and non-overlapping Receipt/Feed/fallback slots.
- Entity-selected desktop/mobile states (post-P1-A): Agent is covered by P1-A; Location, Facility,
  Territory, Organization, Depot, and Module require later slices and their own authority. Stage
  semantics and relation lines remain readable in sparse worlds.
- Program/Protocol states: ordinary copy contains no raw WASM/ABI/hash/transaction/runtime fields
  until the user explicitly opens Diagnostics.
- Every viewport: English/Chinese/CJK, long labels, no horizontal overflow, Tab/Shift+Tab,
  Escape priority, IME composition, visible focus, and semantic selected state.
- Stateful regressions: reconnect continuation, gap/reorg recovery, stale/offline truth, duplicate
  receipt, raw enum/ID leakage, and Advanced disclosure containment.

These rows are target evidence requirements. Historical GPU-ready or GPU-unavailable screenshots,
source tests, and CSS arithmetic do not prove exact-head headed readiness.

## 4. 约束与边界
- 本设计只重排 Viewer 的视觉/交互，不改网络协议、仿真核心逻辑和 `third_party`。
- Search/selection 只导航、居中和打开上下文；执行只能从已授权 Command/Console 路径发生。
- queued/accepted 不等于完成；无适用 receipt 时明确显示结果尚未确认，`No action receipt yet` 不能解释成世界未生效或确定失败。
- 所有实现必须保留键盘焦点、关闭/`Escape`、CJK/长文案和无横向溢出的可读性契约。

<a id="5-设计演进计划"></a>
## 5. 设计演进与未决边界
按以下阶段演进；每一阶段都必须回到配对 PRD 的 source/freshness/permission/receipt 合同，
不能把设计稿或静态截图当作实现完成：

1. **P0 Shell Lite**：冻结稳定 slot、edge navigation、Objective/selected-agent context、
   单一 Next Move、compact persistent Receipt 和 collapsed Feed；在不增加协议字段的前提下
   收敛现有 shell 的默认可见性与响应式布局。
2. **P1 Context + world semantics**：由 `viewer_engineer`/runtime 组合统一 Inspector/Console，
   让已发布的实体状态、Intent/Activity、路线、关系和重大事件可在 World Stage 或 contextual
   surface 中解释，并由 `qa_engineer` 用 headed desktop/mobile、键盘/CJK/空态验证。
3. **Deferred / contract-gated**：全局资源框、完整 capability hotbar、未经来源/权限/时效
   合同的经济/关系/健康字段，以及新的控制 verbs 延后；需 producer/gameplay/runtime/agent
   owner 先冻结语义与权限。GPU-enabled WebGL2 `ready` 与 GPU-disabled `Renderer Unavailable`
   fallback 的已有证据不代表任一阶段的发行结论。

## 6. 终端交互与实施边界
- Fresh load/reload/new tab/session 一律进入 Player。Director 只能由 Player 的
  capability-gated Diagnostics/operator action 显式打开，且仅改变可见性/密度、
  不持久化、不改变 command/auth/ownership/runtime 或 receipt 因果。
- Director entry 无效、过期、撤销或未授权时 fail closed 回 Player，清理 Director
  surfaces，保留 world/selection，并给出恢复文案。
- `World / Targets / Command` 是同一概念路径；Search 留在 Targets，Quote 留在
  contextual Command；选择只导航/高亮，不执行。
- Escape 先关闭局部 surface/drawer，再退出 Focus/presentation；IME composition
  期间由 IME 消费 Escape，关闭后焦点回到 invoker。
- 实施顺序为 anchors/tests → shell layout → console/receipt → focus/a11y →
  World Feed v1 → headed QA。已有源码/协议实现和历史分项证据不作为未做事项重跑；新修改按
  实际命中的表面、renderer 与时序补验证。GPU-unavailable fallback 证据不覆盖 GPU 正常路径，
  后续 GPU/headed 结果也只证明自己的范围。issuer-backed Director allowed path 和仓库级
  runtime/QA release verdict 仍需各自适用证据。

World Feed 现作为 `world_feed/v1` additive ambient projection：identity/dedup 为
`(world_id,reorg_epoch,event_seq)`，source order ascending；nullable receipt link
只能来自显式 runtime causal identity，当前 runtime 明确保持 `receipt_ref=null`。
loading、empty、replay、gap/reorg 和
unavailable 不可合并，当前 Recent Events/Feedback 名称保持不变。Fullscreen Player
默认将其作为右上紧凑折叠 edge overlay/ambient chip，按需展开，不形成常驻列。

### 已知 feed/readout 缺陷的修复状态（#3315）

历史实施、测试与 headed 观察见 [历史实施与验收记录](https://github.com/eng-cc/oasis7/issues/3315#issuecomment-6162410991)。该窗口只复核 GPU-unavailable fallback 的 `1440x1000` / `390x844`；不覆盖 `ready=true` WebGL2/Cinematic、probe 后异步 GPU failure、tablet `641–1240`，不构成发行放行。#3248 的浏览器结果仍是自己的历史窗口。

持续验收分别核对 initial-page cursor continuation、gap/reorg 后清空旧 events 再 reload，以及多资源逐项格式化，不能将 reconnect 连续性外推为已覆盖。测试定位为 `world_feed_transport.test.js`、`world_feed_state.test.js` 和 `render_selected_resource_readout.rs`；当前合同和后续验证要求继续由本文件持有。

<a id="branch-preview-consistency"></a>
## 7. 分支指引与回退预览的一致性

本节承接首局产品合同的 [REQ-FIRST-BRANCH-001](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-branch-001) / [AC-FIRST-BRANCH-001](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#ac-first-branch-001) 和 [REQ-FIRST-BRANCH-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-branch-002) / [AC-FIRST-BRANCH-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#ac-first-branch-002)。它限定既有 gameplay projection 与吸引力样例的解释，不新增真实世界回退动作、协议字段、结算或持久化语义。

<a id="branch-stage-projection"></a>
### 7.1 分支投影职责

- `crates/oasis7_viewer_runtime/src/viewer/runtime_live/gameplay_snapshot.rs` 发布当前主目标、阶段和主指引；`runtime_live/branch_commitment.rs` 依据同次投影的 `available_actions` 选择分支，只推荐已发布且未禁用的动作。
- `IndustryStage::Governance` 是现有内部阶段，不单独证明治理、投票、领土、安全或冲突玩法开放。其 `regional_specialist` 路径的 route label、immediate gain、future beats、risk 和 return hook 继续解释本地生产与专业化用途。
- 模块、工厂核心和合金仍是合法工业候选；其对应需求、产出和下一用途沿现有 `first_delivery_preview` 发布。修正文案不改变配方、材料、动作过滤、完成边界或区域需求的地理含义。
- Viewer 消费发布的承诺并保留不可用状态，不从内部 enum、历史说明或客户端推断额外能力。更换目标或刷新后以当前快照重读；旧推荐不能作为执行授权。

验证时，`branch_commitment.rs` 内的 `regional_specialist_recommendations_keep_enabled_industrial_routes` 单元样例同时启用模块、工厂核心与合金，逐条核对承诺，再禁用模块候选检查过滤；下表的真实 Runtime 场景补充主指引与合金路线投影。两者组合，避免仅凭一条已启用路线推断所有分支文案均已覆盖。

<a id="fixture-rollback-window"></a>
### 7.2 吸引力样例的回退窗口

`crates/oasis7_viewer/software_safe_src/gameplay_attraction_scenario.js` 的 `TASK-GAME-076` 样例自有 route phase 决定 `rollback_quote`。报价生成与证据 gate 共用同一窗口判断；仅检查非空文案不能证明可回退。规则只作用于该样例版本的 `accelerate` / `stabilize` 路线，不把样例阶段解释为真实 Runtime 的世界状态。

| 样例时点 | 回退含义 | 必须保留的后果与下一步 |
| --- | --- | --- |
| incident recovery 尚未开始 | 窗口内可回退；截止条件、代价、保留和失去价值均可读 | 已兑现的结果与既有约束仍可追踪 |
| incident recovery 开始的边界 | 窗口立即关闭，不等待后续订单或交付 | 明确截止原因；保留当前合法恢复选择 |
| repair / replan 及之后的完成态 | 原回退继续不可用，恢复完成不重开旧窗口 | 保留恢复收益、已形成能力、义务和回访动作 |
| route phase 缺失、未知或与样例状态冲突 | 不承诺可回退，相关样例不能获得窗口一致性通过 | 给出资料缺失或冲突原因，不能用旧 quote 补齐 |

修复不清空已提交结果，不改变 `production_only` 的首产物边界，也不把后续交付变成隐藏前置。样例可同时具有“已完成恢复”和“仍有后续用途”，但不能因此恢复已截止的回退能力。真实入口若将来发布回退动作，仍由其专业合同独立判断当前窗口和实际执行结果。

<a id="starter-plan-interaction"></a>
## 8. 当前首局方案与成果的交互

本节落实[首局具体样本](../../product/world-rules-core-gameplay/first-session-and-continuation.design.md#des-first-concrete-sample)，复用现有 Next Move、Gameplay Details 和唯一 Action Receipt。它不是新控制台、通用实体 Inspector 或新的导航路线；当前 Agent-only context 边界不变，工厂方案只在已有 gameplay 操作上下文表达。

### 8.1 从 Next Move 完成这一次决定

默认阅读顺序是：**当前对象与目标 → 本次实际方案 → 主要成本/阻塞 → 明确下一动作 → 可展开依据与诊断**。对象、方案、提交范围及返回路径在进入详情后仍可辨认；原始 JSON、内部 ID 和次要诊断默认收起。影响损失、权限或不可逆承诺的信息不能折叠到玩家无法发现的位置。

查看对象只导航，请求预览只读取，提交才进入执行链；动作名称与 pending 反馈必须表达这一区别。固定方案的 factory/recipe/batches 与实际提交同源；可编辑但未接入提交的查询明确标为独立查询，不能让玩家以为编辑了 Next Move。保留现行 direct 动作白名单的合法行为，不把非 direct 入口仅靠文案变成提交；同一动作不保留两份竞争的 submit 控件，也不用 CSS 视觉移位造成与 DOM/键盘不同的顺序。完整成本从 [M4 同方案报价](../m4/industrial-resource-flow-contract.design.md#starter-quote-submit-binding)读取；缺失字段保留未确认，不由前端常量或另一账本补齐。

### 8.2 状态、合法操作与焦点

下表是现有状态的交互映射，不要求新的 runtime enum。查询 attempt、方案适用性和原提交结果分开；具体身份与失效规则遵循 M4/Runtime，而非用一个 loading 布尔值覆盖全部。

| 触发状态 | 保留的信息与合法操作 | 返回、焦点与禁止行为 |
| --- | --- | --- |
| 打开详情 / 返回世界 | 保留当前目标、对象和本次规范方案；查看/查询/提交入口明确 | 返回原 invoker；关闭不提交、不取消、不重置世界 |
| 查询中 | 保留方案和查询归属，明确尚无可用报价；允许退出或受支持的重新选择 | 不把 pending 当成零成本或可执行；异步响应不夺焦点 |
| 参数或对象改变 | 旧报价失去提交资格，保留可解释的变化原因；新查询绑定新方案 | 旧响应不得覆盖新对象；无需因更新世界快照清空全部上下文 |
| 报价失效 / snapshot 更新 | 禁用旧方案提交；保留原查询关联并待复核，或显式废弃该 attempt 并提供重查 | 不无声转 idle 丢弃在途结果，不用旧 allowed 恢复提交按钮 |
| 提交中 / 结果未知 | 保留原请求身份与已知范围，提供真实的核对/安全重试能力或限制说明 | 不显示确定失败/未生效，不以新身份重提；退出不取消旧请求 |
| 权限失效 | 当前权限决定脱敏；保留允许读取的已知结果和恢复说明，执行按当前授权重验 | 不借保留草稿/请求泄露数据，不继承旧授权 |
| 部分或完整成果到达 | 实际所得、已确认范围与尚待决范围分别可读；引用唯一 Receipt，再展示真实后继条件 | 无关旧结果不替换当前方案；不强制弹窗领奖、不夺焦点、不把部分成功合成全目标完成 |
| 离场 / 重新进入 | 恢复成果摘要、当前目标、待决请求与有效委托；核对当前条件后提供第一动作 | 不重复播报旧成功、不产生离线收益；焦点回可用主入口而不是已失效按钮 |

正常更新使用非打断式状态播报；仍可从同一 Receipt 入口主动回看详情。不要为了只保留一个主 CTA 而隐藏合法返回、暂停未来委托或核对原请求的入口；这些是上下文恢复，不是第二条主线。

### 8.3 成果与后继目标的阅读边界

先显示已被领域结果证明的实际投入/所得，再在原意图—action/job—settlement 精确关联成立时解释本次玩家贡献，最后说明当前真实用途与剩余条件。只有同 owner、最近结果或同名材料时可显示归属成果，但不称为当前决定的功劳；预览不是实际结果，Agent 解释不是归因证据。

成果摘要引用同一 Action Receipt，不建立第二份回执或重复浮层。Assembler 候选可被查看、比较并显示已满足条件与缺口，即使当前不能建设；只有实际可执行动作使用提交型 CTA。候选未知时诚实解释，不把不可用候选包装成正式可行推荐。首产物、持续能力、建成 Assembler 和交付保持独立，首产物后可自然离场。

### 8.4 该操作链的入口验收

针对同一候选，在现有 quote state/response、gameplay action 提交、Gameplay Details 与 Receipt 路径验证正常提交、对象/参数改变、报价在途失效、迟到旧响应、权限失效和未知结果恢复；不能只测 reducer 或静态按钮存在。图形验证按既有矩阵覆盖桌面和窄屏，特别检查中文 IME composition、Tab/Shift+Tab、Escape/返回焦点、长文本和文字放大后的主要成本/动作/关闭入口可达；成果播报不夺焦点。具体验证版本、运行命令与证据留在 Task/PR；本节不声明实际 GUI 或真人体验已通过。

### 11.1 验证映射表

| 上游 requirement / acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法与 test/manual source | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-FIRST-BRANCH-001](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-branch-001) Trace refs: O7-REQ-4c622b94-22bd-4559-9595-6b134bdc00b1 | [分支投影职责](#branch-stage-projection) | 当前区域专业化不发布未开放的阶段承诺，并保留合法合金路线 | [industrial_progression.rs](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/industrial_progression.rs) 的 `runtime_gameplay_action_promotes_to_generic_midloop_after_governance_ready`；当前候选 `test_tier_required` | PR 中的聚焦回归结果；主指引、全部承诺及可执行工业路线 | 普通合同回归，实际入口体验另验 |
| [REQ-FIRST-BRANCH-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-branch-002) Trace refs: O7-REQ-2e82fef6-bbb5-43b6-a7d0-833905e8e8a3 | [吸引力样例回退窗口](#fixture-rollback-window) | 同一路线截止前、incident 开始、修复后及完成态；拒绝反向 available、缺失或冲突 phase | [gameplay-attraction-scenario.test.mjs](../../../crates/oasis7_viewer/scripts/gameplay-attraction-scenario.test.mjs)；当前候选 `node` 结构正反例 | PR 中的样例测试结果；报价与 gate 窗口一致 | `fixture_structure_only`，不证明真实回退或 human playability |
| [REQ-FIRST-INDUSTRIAL-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-002) Trace refs: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95 | [当前方案交互](#starter-plan-interaction) | 查看/查询/提交分开，同参数与实际成果可读；承接 AC-FIRST-INDUSTRIAL-001。 | 待执行：按 [测试手册](../../../testing-manual.md) 选择当前 candidate 的适用 S6 场景，沿 §8.1/8.4 的 quote→submit→receipt 真实消费者核对对象、参数、成本和反馈；覆盖桌面/窄屏、IME、键盘、文字放大与非打断播报。 | PR 中的实际参数、权威结果与 headed captures；记录缺失前置。 | 单元/组件测试不代签 GUI；图形正确不证明真人理解、主体感或留存。 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005) Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | [当前方案交互](#starter-plan-interaction) | 失效/迟到结果不污染当前方案，原请求未知与已结算范围分开；承接 AC-FIRST-INDUSTRIAL-003。 | 待执行：按 [测试手册](../../../testing-manual.md) 在当前 candidate 的适用 S6 场景执行 §8.2 的参数/对象改变、报价在途失效、旧响应迟到、权限失效及未知结果恢复；核对实际提交、焦点和原请求身份，缺恢复入口记录 blocked。 | PR 中的消息时序、原请求与领域结果、无重复效果证据及 headed captures。 | reducer 或固定脚本不代签真实消费者、GUI 或自主试玩。 |
| [REQ-FIRST-INDUSTRIAL-008](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-008) Trace refs: O7-REQ-757636b1-c9e9-4e7a-97d1-460de105daf3 | [当前方案交互](#starter-plan-interaction) | 单一成果回执、可见后继目标和自然离场分别成立；承接 AC-FIRST-INDUSTRIAL-004。 | 待执行：按 [测试手册](../../../testing-manual.md) 在当前 candidate 的适用 S6 场景验证 §8.2/8.3 的成果到达、后继不可执行、自然离场和重入；检查旧成果不误归给新意图、不重复播报、不复活失效授权；无提示续玩观察单独记录。 | PR 中的成果/后继条件、离场与重入状态、headed captures 和提示介入记录。 | 图形与恢复正确不证明 Agent 主体感或真人留存，未覆盖窗口保持未验证。 |
| [REQ-ENTRY-PERFORMANCE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-performance-001)；Trace refs: O7-REQ-7e64b369-3c32-4671-a1af-205bc5a75251 | [显示投影与调度](#viewer-render-projection) | 按[预算](#viewer-performance-verification)验收响应/事实一致性。 | 按[测试手册 S6](../../../testing-manual.md)执行 Host/DTO/Rust 具体解析的正负回归：代理不外泄、未知字段不遍历、同 revision 不转换、每项有效变化必刷新、输出上限、旧代次丢弃、逐条 receipt 不丢；真实浏览器场景各 3×60 秒，p95 按每次完整操作采样，报告样本数与最差一次，不能平均掩盖失败 | 实施 PR：source/build/scene、基线与优化后 trace、分段耗时/转换次数/输入输出大小/正确性及失败 | mock 不代签 WASM/实际 GPU；本设计交付不执行游戏自主试玩，不改变 Skill 或行动行为 |
| [REQ-ENTRY-PERFORMANCE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-performance-002)；Trace refs: O7-REQ-25030f5c-c246-4b72-b637-de9dc6b520ae | [资源交付与恢复](#viewer-payload-delivery) | 按[预算](#viewer-performance-verification)验收公共资源加载/复用及失败恢复。 | 按[测试手册 S6](../../../testing-manual.md)执行 loader 故障回归与真实 HTTP：br/gzip/identity 的解码 hash 相同，响应头/encoded bytes 正确；冷启动 10 次/热启动 20 次均逐次满足预算；切版本/回滚/损坏/超时/离线/淘汰/并发 Retry/迟到 init 正确恢复，记录阶段时长和 body transfer | 实施 PR：相同发布的 manifest/hash/headers、network waterfall、缓存来源、阶段状态与实际退出码；实际部署验证以原 runbook 继续 | 本地服务器、curl 压缩、LCP 或 shell 出现单独不能证明 renderer ready；热性能不证明冷性能，资源就绪不证明可玩 |

本节定义实现与验收边界；具体候选、执行过的测试和未覆盖入口记录在对应 PR。代码或样例回归成功不能升级为真实自主试玩、图形交互或发行验收通过。

<a id="viewer-performance-contract"></a>
## Viewer 性能优化合同（目标设计）

Owner：`viewer_engineer`；QA 维护测量与场景，发行/部署 owner 维护真实 HTTP 交付配置。本节只设计浏览器性能，不修改世界协议、权限或游戏规则；已开始分阶段实施，不把上一轮站点诊断或源码存在当作当前候选验收。线上单次 CPU/下载样本只支持定位假设，不定义普适硬件能力。

性能要求在本文 §2.1 分配，§11.1 对应验证；下述条款是完整目标合同；当前实现范围与剩余项见本节末尾。

<a id="viewer-render-projection"></a>
### 显示投影与调度

1. **投影先于跨 WASM 边界**。Host 从同一有效、经会话可见性过滤的 snapshot 构造普通、无 Solid Proxy 的 `RenderInputV1`；仅包含当前显示所需的实体 identity/位置/标签、地形/设施显示字段、选择/相机、目标/阻塞/回执展示字段及 world/scope/revision。明确字段白名单，不直接传 snapshot、modelLists 的原实体、完整 gameplay/history 或 auth 对象。嵌套数组/对象同样投影成普通数据，单独 `unwrap` 或 `JSON.stringify(snapshot)` 不算解决完整树遍历。Rust 改为具体 DTO 解析，不先递归转换完整 `serde_json::Value` 再挑字段；Rust 继续拥有已有渲染派生规则，JS 只裁剪/复制，不补算游戏后果。
2. **限制数据成本**。输入与输出均有 schema/version、字段类型、数目、文本长度及总字节上限，未知版本明确 unavailable。目标每个方向序列化等价 UTF-8 大小不超过 256 KiB；按实体稳定排序，场景投影最多 500 个普通可见标记，文字标签最多 100 个、每个最多 128 Unicode 标量。当前选择、主目标和主要 blocker 的语义优先，装饰先裁剪；无法容纳必需语义时明确“显示容量不足”，不得悄悄丢掉关键结果。可见细节裁剪不改变 Targets 完整的有权列表、世界尺度与实际实体计数。沿用 fragment LOD/空间查询权威；被裁剪数量与原因可诊断，输入输出都检查预算，不发送截断的非法 JSON。
3. **按显示 revision 更新**。memo 依赖只读显示投影的确切输入（相关字段 revision、locale、selection、相机/LOD、权限 scope），网络心跳、diagnostics、无关 feed 和 renderer meta 不使全世界重新派生。世界 tick 可经轻量 readout 更新；同一显示内容不因 tick 变化重新转换几何。旧协议没有 revision 时，在接收边界按有关字段计算 revision，禁止每帧深度比较全 snapshot。实体删除、不可见、目标/阻塞和最新动作结果变化必须使正确子投影失效。
4. **合并展示，保留权威事件**。同一 animation frame 只消费一个最新待显示投影，最多一个派生在途及一个待处理槽；旧待处理投影可覆盖，动作提交、回执、journal/feed 的权威 reducer 仍逐条处理，不能套用 latest-only。显示保留真实 world revision 和 freshness；追赶期间不能显示 LIVE。单次转换超预算时先缩小 DTO 和拆分无关派生；Worker 仅在测量证明仍需隔离纯派生时采用，不把 Bevy/WebGL 或 auth 转移到 Worker 当作本轮前提。
5. **隔离代次与失效**。每份投影绑定 world、branch/epoch、可见性 scope、session generation、schema 和相关显示 revision。切世界、注销、权限收缩、reorg 或 renderer retry 立即失效旧投影、待处理槽和派生缓存，迟到结果在提交前核对 generation。公开 bytes 缓存不能装玩家状态；旧画面可以表示追赶但不能保留已撤销可见内容。重入从当前授权数据重建显示，不恢复旧 authority。
6. **动画与真实更新分开**。维持现有 ambient 约 12 Hz 上限、不可见暂停和减少动效策略；相机交互按帧更新必要投影，hover 仅更新相关命中/提示层，不重送世界。渲染发回 camera/meta 事件仅在值变化时更新 Host，避免 effect→WASM→meta→effect 自激。输入、焦点、命令草稿和失败恢复不等待环境动画完成；降级装饰不切换第二套事实源或伪装 renderer ready。

<a id="viewer-payload-delivery"></a>
### 公共图形资源交付与恢复

- **构建 owner**：沿用 [Web Build Truth](viewer-web-build-truth.design.md) 的 canonical bundle、optional manifest 与 separate artifact；先用 optimized WASM 构建、剔除调试/无用 feature 及经回归验证的体积优化，不用开发构建代表性能。manifest 的 size/hash 始终指解压后的 canonical bytes，压缩变体可另列 encoded size；发布前自动核对三者。未生成资源仍按 optional unavailable，不把空文件或 fallback 标成成功。
- **发布 owner**：每份 WASM 使用含完整内容 SHA-256 的 URL；同一 URL 的解码 bytes 永不覆盖。HTML 与 manifest 通过 revalidation 或 no-store 获取一致的发布指针，manifest 绑定 bridge/schema/build 标识；旧页面固定其 descriptor，不在加载中途跟随新 manifest 混版。先发布资源及压缩变体，再发布同候选 HTML/JS/manifest；保留仍受支持版本引用的资源。回滚切回完整兼容发布集合，不覆盖 hash URL 或回滚世界。
- **HTTP owner**：WASM 提供 `application/wasm`、经协商的 Brotli/gzip（`Content-Encoding`、`Vary: Accept-Encoding`）及 identity fallback；hash URL 返回 `Cache-Control: public, max-age=31536000, immutable`。压缩各变体解码内容相同；identity fallback 保持可用但单独报告加载预算。JS/CSS 仅在同样使用内容寻址后采用 immutable，稳定名字不得长缓存造成混版。既有外部 optional base 保留原允许来源/CORS/CSP 边界，不为性能新增第三方 origin 或放宽权限。
- **Loader owner**：公共 hash URL 使用正常 HTTP 缓存语义，去掉 payload 的无条件 no-store；manifest 仍新鲜核验。冷下载、warm disk/memory cache 和淘汰后重取都对解码 bytes 核对 size/hash，再 init；HTTP 缓存命中和文件名本身不是校验。第一阶段保留完整 bytes 校验后编译，分别测 fetch/read/hash/compile/init，不以未验证流式执行换加载分数。若未来采用流式编译，仍须先证明未验证 module 不实例化、不调用导出，另行评审兼容与完整性边界。
- **资源状态**：入口 shell 先可操作，再表示资源下载/核对/准备图形/就绪或不可用；下载进度只能用同一编码口径的可测值，未知长度显示不确定进度。资源预算超时提供原因、重试与退出，不触发整页刷新或游戏动作重放。图形 ready 仍独立于身份 ready、world synced、action permitted；资源失败时保留实际受支持的安全入口，不自动宣称另一模式可玩。
- **恢复状态**：继续遵守[已有 loader Retry](#shell-interaction-recovery)：同 descriptor 在途 single-flight，成功实例复用，失败只清除本 attempt，迟到 attempt 不覆盖新 generation。损坏缓存只允许一次 `cache: reload` 重取当前 hash URL 并重新核对；再失败保持 unavailable，显式 Retry 开启新 attempt，禁止后台无限下载循环或改用旧 hash 逃避失败。每 attempt 下载/读 body 30 秒预算，hash/compile/init 另计 10 秒预算，AbortController/代次丢弃阻止迟到挂载；无法中断的初始化保持独立实例所有权，废弃实例释放，不与新 attempt 共用可变 singleton。重复失败允许玩家稍后重试，不删除账号/会话；离线且缓存有效只恢复公共资源，不代表在线世界就绪。

<a id="viewer-performance-verification"></a>
### 预算、验证与实施顺序

承接 [REQ-ENTRY-PERFORMANCE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-performance-001)（Trace refs: O7-REQ-7e64b369-3c32-4671-a1af-205bc5a75251）与 [REQ-ENTRY-PERFORMANCE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-performance-002)（Trace refs: O7-REQ-25030f5c-c246-4b72-b637-de9dc6b520ae），分别验证运行响应和资源准备。

以下是后续实现验收目标，不是当前已达到的指标。复用 `viewer-performance-probe.mjs`、`performance_metrics`、Host/loader 回归和测试手册 S6，补齐缺失测量，不新增全仓通用门禁或付费模型测试。

| 测量范围 | 目标与明确口径 | 条件 / 不能代签 |
| --- | --- | --- |
| 入口与冷资源 | 导航至可操作 shell ≤2 秒；首个 renderer ready ≤30 秒；Brotli 或 gzip 实际传输 WASM ≤10 MiB，解码大小另报 | 冷 HTTP cache，5 Mbps 下行/1 Mbps 上行/80 ms RTT；服务可达、初始只读 snapshot ≤1 MiB；ready 不含账号交互、后端故障或 GPU 不支持，但这些必须给出失败状态 |
| 同版本重入 | 导航至 renderer ready ≤5 秒；WASM body 网络传输为 0（允许 manifest revalidation） | 同一有效发布、HTTP cache 未淘汰、在线环境；不得将每次重入清缓存或浏览器手动刷新禁缓存与此混测；另测缓存淘汰后的冷路径 |
| 派生与提交 | JS 投影、跨 WASM 解析/派生/回传、bridge update 的端到端耗时 p95 ≤16 ms；单次不超过 50 ms；相同显示 revision 转换次数为 0 | 常规/高负载分别报告，包含复制与校验成本，不只计 Rust 函数体；初次建场景单列初始化预算 |
| 连续交互 | 输入/选择/开命令至下一次绘制的事件延迟 p95 ≤200 ms；60 秒窗口主线程 busy 时间占比 ≤20%，大于 50 ms 的 Long Task 累计占比 ≤1% | 真实 headed Chrome 硬件加速，不用软件 renderer/no-op/隐藏标签证明；空闲观察与持续操作分开报告；不把 ambient 12 Hz 当 60 FPS 承诺 |
| 有界显示与追赶 | DTO 输入输出各 ≤256 KiB、在途 1/待处理 1；有限突发结束后 1 秒内显示最新有效 revision 或明确 unavailable | 以每秒 10 次显示输入突发 5 秒检验，权威动作/回执不丢；持续超载可标明追赶，不伪造固定刷新速率 |

参考设备先固定 Apple M3 级桌面、同一 stable Chrome/硬件加速，记录具体 OS、浏览器、DPR 与 GPU；1440×1000 和 390×844 均测。窄视口只证明布局，不代表移动硬件性能；若发行声明支持真实移动设备，须另固定该设备并满足同一预算，未测不扩大支持范围。网络节流与 CPU 节流分别记录，不能从浏览器 UI 未节流推断外网链路无限带宽。

场景固定为：空/不可用世界；1 Agent、1 location、16 terrain 的稀疏世界；常规 1,000 个可见性允许的源实体/视口 100 标记；高负载 10,000 个源实体/视口至多 500 标记。输入保持既有 schema，另带 5 MiB 非显示历史字段验证不跨 WASM 边界；每秒两次世界输入，穿插相机、hover、命令和目标/阻塞/回执变化。所有场景仍核对权限、删除、尺度、LOD 与事实一致性，不能靠只保留空世界达预算。

验证责任与证明边界见 §11.1 的两条性能要求映射。

实施按两条可独立验证的切片推进：先有界 DTO、具体 Rust 解析与 revision 调度，再做资源体积、hash URL、HTTP 配置和 loader 缓存恢复；每条保留正确性与真实性回归。上线前用同候选重新测两条路径，若未达预算继续定位，不降低阈值掩盖瓶颈；线上部署需后续授权，本次实现不新增 CI 控制、发布 claim 或无关后端修复范围。

### 2026-10-19 实施范围与验收边界

设计合入 `fb15585d8811ef5c7ea010139f25908812289213` 后，第一阶段实施以下路径：

- `pixel_world_render_input.js` 在 Host 读取白名单并复制普通数据；输入/输出各 256 KiB，上层 schema/version 严格校验，Rust 从 JSON 字符串解析显示 DTO，避免对 Solid Proxy 逐属性反序列化。实体列表最多 500，超限明确 unavailable；启用已有 worldView/LOD 投影时不再复制未被消费的完整源列表。
- Host 每帧合并展示请求；adapter 对有界 DTO 的显示内容做签名，相同内容不重新跨 WASM 派生，tick 单独刷新 HUD；有社会关系到期语义时保留时间失效。权限/会话与 renderer 代次继续由既有 scope guard 和 adapter generation 隔离，权威 reducer 不作 latest-only。
- 精简 Bevy feature 和发布 WASM 的调试节，交付 `viewer-optional-payload/<sha256>.wasm` 及确定性 gzip；manifest size/hash 仍针对解码 bytes。后端仅 hash WASM 使用 immutable；JS/CSS 可协商 gzip 但保持 no-cache，HTML 不预压缩以保留既有安全注入。
- Loader 使用正常 hash URL 缓存语义，每次核验解码 bytes；损坏缓存只自动 reload 一次。失败 init promise 可显式重试，同 descriptor 初始化保持 single-flight；资源下载/校验/准备/就绪阶段独立于世界同步。hash payload 下载/读 body 有 30 秒中止预算。

局部证据包括 Host/loader/DTO 回归、Rust 显示派生和静态 HTTP 回归、打包解压/manifest 一致性；生产构建解码 35,019,856 bytes、gzip 7,664,037 bytes。真实 Chrome 的桌面与窄屏稀疏只读样例均显示 WebGL 画布、实体选择与真实 OFFLINE 状态；85,812 ms 窄屏观察中脚本 1,714 ms、INP 78 ms，不是同世界受控前后对比，也不覆盖常规/高负载预算。浏览器仍观察到初始化/交互长帧，不能宣称所有转换都在 50 ms 内。

未完成项仍按原阈值保留：稳定排序与关键语义优先裁剪、100 标签/128 标量细分限制、hash/compile/init 独立 10 秒预算及不可中断初始化的实例所有权隔离、完整设备/网络/大世界/突发更新测量。现阶段采用明确容量失败保障有界输入，不代表 10,000 源实体的常规显示验收通过。线上发布保留多版本资源与实际缓存策略仍需部署 owner 验证；本轮实现不改变世界准入或发布就绪声明。

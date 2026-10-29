# Viewer Web 语义化测试 API（Phase 9 发行验收支撑）

- 对应设计文档: `doc/world-simulator/viewer/viewer-web-semantic-test-api.design.md`
- 历史 API 实施与验证记录：GitHub task issue evidence。

审计轮次: 5

## 1. Executive Summary
- 当前 canonical 实现位于 `crates/oasis7_viewer/software_safe_src/legacy_core.js`；本文件保留测试专用 API 合同，不把历史 Bevy/EGUI automation 实现当作当前 source truth。
- 本节其余 round-1 至 round-4 叙述保留为历史迁移记录；当前 API 的可调用控制和状态字段以“当前实现合同”为准。
- 为 Web 端 `oasis7_viewer` 注入一套稳定的语义化测试 API，降低 agent-browser 对像素坐标点击的依赖。
- 复用现有 `viewer_automation` 步骤执行器，统一测试动作语义（`mode/focus/select/zoom/orbit/wait`）。
- 在不暴露生产攻击面的前提下，提供测试模式专用入口：`window.__AW_TEST__`。
- 补齐 round-1 目标：对齐人类高频操作中的“面板显隐 / 模块显隐 / 选中聚焦 / 材质预览切换”四类语义动作，减少脚本对键盘事件与像素点击的耦合。
- 补齐 round-2 目标：对齐“顶部区显隐 / 语言切换 / 玩家布局预设（Mission/Command/Intel）”三类 UI 状态动作，进一步降低脚本对界面点击路径的依赖。
- 补齐 round-3 目标：对齐“聊天消息发送 / Prompt 三字段覆盖提交”两类高频输入动作，减少脚本对 Chat 面板逐控件输入的依赖。
- 补齐 round-4 目标：对齐“时间轴 seek / 标记过滤 / 标记跳点”三类时间轴操作，减少脚本对 timeline 区块逐按钮点击的依赖。

## 2. User Experience & Functionality

<a id="viewer-web-semantic-test-api-current-contract"></a>

### 当前实现合同（Batch 22 回填）

- `window.__AW_TEST__` 仅在测试模式启用。当前 control-action surface 是 `sendControl("play"|"pause"|"step", payload?)`、`runSteps(payload?)` 与 `getState()`；其他测试 helper 不扩大 live 控制集合，`seek`/`seek_event` 不得在 live profile 作为可提交动作出现。
- `sendControl("step", payload?)` 的 count grammar 为：缺省/`null` 为 1；有限 number `>= 1` 取 `Math.floor`；string 为空、`step`、有限数字或 `step:<n>`/`step=<n>`；object 读取有限 `count >= 1` 并取整。目标合同要求其余 payload 返回 rejected feedback，不抛出浏览器异常，也不发送控制请求；当前 parser 差距见下段。
- `runSteps(payload?)` 使用同一 count grammar 并委派为一次 step；它不再是历史 panel/layout/timeline DSL 的通用执行器。历史 DSL、Bevy command queue、2D/3D camera automation 和 EGUI module actions 已退役，不得据此编写当前 Web 回归。
- `getState()` 至少用于读取 `connectionStatus`、`logicalTime`、`eventSeq`、兼容 `tick`、`controlProfile`、`lastError` 和 `lastControlFeedback`。`tick === logicalTime` 是兼容 alias；时间和事件序号仅来自 runtime 消息，空结果不得被客户端合成为进展。
- control feedback 必须区分 rejected、blocked、queued、发送失败以及后续观察到的 completed/no-progress；queued 不是 completion。live seek 的 unsupported-action、WebSocket 断链和 runtime 无增量均必须保留为不同可诊断状态。

### 当前 step parser 的实现差距

`legacy_core.js::parseStepCount` 当前的 `step` 正则为未锚定的 `/step\s*[:=]\s*(\d+)/i`，可从带前后缀的字符串中提取数字，也可能从小数形式截取整数部分；匹配分支没有再次检查有限性。

object 路径直接读取 `payload.count` 并执行 `Number` 转换，getter、Symbol 或自定义转换可以抛错。`sendControl`/`runSteps` 调用 parser 时没有把这些异常统一转为 rejected feedback。

因此上面的完整 grammar/no-throw/非法输入不发包仍是目标合同，不能由当前源码或普通有效输入用例推定已实现；需要覆盖前后缀、非有限值与抛错转换的独立负例。这里只澄清现状，不放宽拒绝目标，也不改变 live 控制或测试构建边界。

### Chat 输入验证边界

- sendAgentChat(agentId, message) 是语义入口，不能模拟 compositionstart/update/end、焦点切换或逐键换行；helper 成功不能替代真实输入验证。
- 分别验证语义层权限/空消息/in-flight 拒绝、timeout/error 与显式重试；真实 UI 组合态不误发、事件去重、Enter/Shift+Enter 多行编辑、焦点/世界快捷键隔离、无页面卡死与可见恢复。main.test.jsx 的 queued timeout 样例只覆盖部分语义路径。
- 真实输入验收保留 state、console、截图和请求次数/消息文本证据；不得用本 helper 或 Escape 局部保护宣称完整 IME 通过。产品承诺见 Agent 对话与 Prompt 控制，当前操作见 Viewer 手册。

### Browser runtime-fatal state

- 测试模式发现浏览器 rendering/runtime fatal 时，`getState()` 必须暴露 `connectionStatus=error`、非空 `lastError` 与单调递增的 `errorCount`；fatal 不能继续伪装为 `connecting`、成功连接或世界进展。
- 已知 graphics fatal 最多自动 reload 一次；再次失败后保持可观测错误，避免无限恢复循环。rejected、blocked、queued、completed/no-progress 与 runtime fatal 必须继续分开。
- 自动化应归档 state、console 与 screenshot；WebGL/SwiftShader fatal 属于浏览器环境/图形 gate，不是 gameplay 失败或成功证据。

### Historical Bevy API record (retired; not current contract)

The following scope, command queue, state snapshot, roadmap, and decision-log records preserve the completed Bevy/EGUI implementation history. They do not override the current implementation contract above.

#### In Scope
- 新增 `window.__AW_TEST__` 最小 API：
  - `runSteps(steps: string)`
  - `setMode(mode: "2d" | "3d")`
  - `focus(target: string)`
  - `select(target: string)`
  - `sendControl(action: "play" | "pause" | "step" | "seek", payload?: object)`
  - `getState()`
- `runSteps(steps: string)` round-1 补齐语义：
  - `panel=<show|hide|toggle>`
  - `module=<controls|overview|chat|overlay|diagnosis|event_link|timeline|details>:<show|hide|toggle>`
  - `focus=selection`（或 `focus_selection=current`）
  - `material_variant=<next|cycle>`
- `runSteps(steps: string)` round-2 补齐语义：
  - `top_panel=<show|hide|toggle>`
  - `locale=<zh|en|toggle>`（或 `language=<zh|en|toggle>`）
  - `layout=<mission|command|intel>`
- `runSteps(steps: string)` round-3 补齐语义：
  - `chat=<agent_id>|<message>`（`message` 支持 `%xx` 文本解码）
  - `prompt_system=<agent_id>|<text|clear>`
  - `prompt_short=<agent_id>|<text|clear>`
  - `prompt_long=<agent_id>|<text|clear>`
- `runSteps(steps: string)` round-4 补齐语义：
  - `timeline_seek=<tick>`
  - `timeline_filter=<err|llm|peak>:<show|hide|toggle>`
  - `timeline_jump=<err|llm|peak>`
- 通过命令队列将 JS 调用转为主线程逐帧消费，避免并发修改 Bevy 资源。
- 通过 `getState()` 返回闭环测试关键状态：
  - 连接状态
  - 当前 tick
  - 当前选中对象
  - 相机状态（`cameraMode` / `cameraRadius` / `cameraOrthoScale`）
  - 错误计数与最近错误
  - 事件/trace 计数
- 接入 `app_bootstrap` 生命周期（startup 注册、update 消费与状态发布）。

#### Out of Scope
- 不改 Viewer 协议（`ViewerRequest/ViewerResponse` 语义不变）。
- 不扩展到通用远程控制协议（仅用于 Web UI 测试辅助）。
- 不在本阶段引入完整 agent-browser E2E 套件重写（先提供 API 与最小回归）。
- 不在本轮覆盖文本输入、鼠标拖拽轨迹细节、IME 组合输入等低层“逐事件复刻”能力。
- 不把 `--auto-play`、WebSocket readiness probe 或 reconnect timer 变成 API 成功判据；它们是 launcher/manual 的操作合同，不能伪造连接或世界事件。

## 3. AI System Requirements (If Applicable)
- N/A: 本专题不新增 AI 专属要求。

## 4. Technical Specifications

#### 测试入口
- JS 全局对象：`window.__AW_TEST__`（仅测试模式启用）。
- 运行时开关：
  - `cfg(debug_assertions)` 下默认启用；
  - 或 URL query 包含 `?test_api=1`（release 场景）。

### Current connection and no-throw boundary

- socket `error` 必须留下 `viewer.ws` 诊断，socket `close` 进入 `connecting` 并安排重连；这些状态不等于已连接、已排队或已推进。
- 历史“非法 payload 只 warning、不 panic”的目标由当前 rejected-feedback/no-throw 合同继承。自动化应断言返回值、`lastError` 和 state，而不是依赖 console 文案或伪造 time/event 增量。
- runtime fatal 优先于普通 reconnect 状态：首次已知 graphics fatal 可触发一次有界 reload；后续 fatal 必须停留在 error 并保留最近错误。

#### 命令队列
- JS -> queue -> Bevy Update 消费：
  - `RunSteps(String)`
  - `SetMode(ViewerCameraMode)`
  - `Focus(ViewerAutomationTarget)`
  - `Select(ViewerAutomationTarget)`
  - `SendControl(ViewerControl)`
- `RunSteps` round-1 扩展步骤类型：
  - `PanelVisibility`
  - `PanelModuleVisibility`
  - `FocusSelection`
  - `CycleMaterialVariant`
- `RunSteps` round-2 扩展步骤类型：
  - `TopPanelVisibility`
  - `SetLocale`
  - `PlayerLayoutPreset`
- `RunSteps` round-3 扩展步骤类型：
  - `SendAgentChat`
  - `ApplyPromptOverride`
- `RunSteps` round-4 扩展步骤类型：
  - `TimelineSeek`
  - `TimelineMarkFilter`
  - `TimelineMarkJump`

#### 状态快照
- Bevy 每帧发布到可读快照：
  - `connection_status`
  - `tick`
  - `selected_kind` / `selected_id`
  - `camera_mode` / `camera_radius` / `camera_ortho_scale`
  - `error_count` / `last_error`
  - `event_count` / `trace_count`

### 历史实现处置

- 下列段落记录已完成的 Bevy/EGUI round-1 至 round-4 迁移，仅作历史追溯；其中的 `viewer_automation`、命令队列、camera、panel/module、timeline DSL 和 `sendControl.seek` 不定义当前 SolidJS API。

#### Risks & Roadmap
- WTA-0：设计/GitHub Issue/Project task truth建档。
- WTA-1：`viewer_automation` 支持运行时步骤入队。
- WTA-2：`web_test_api`（wasm）桥接层实现与 `window.__AW_TEST__` 注入。
- WTA-3：`app_bootstrap` 接入命令消费与状态发布系统。
- WTA-4：单测与回归验证（`oasis7_viewer`）。
- WTA-5：文档状态与 devlog 收口。
- WTA-6：`testing-manual.md` S6 示例迁移到语义 API。
- WTA-7：`getState()` 扩展相机语义字段，支撑 zoom 可验证门禁。
- WTA-8（已完成）：round-1 补齐需求建模与任务拆解（文档任务）。
- WTA-9（已完成）：落地 `viewer_automation` round-1 语义步骤（panel/module/focus_selection/material_variant）。
- WTA-10（已完成）：执行 round-1 定向回归并完成文档收口。
- WTA-11：round-2 补齐需求建模与任务拆解（文档任务）。
- WTA-12（已完成）：落地 `viewer_automation` round-2 语义步骤（top_panel/locale/layout）并补齐解析/映射测试。
- WTA-13（已完成）：执行 round-2 定向回归、更新手册示例与文档状态收口。
- WTA-14（已完成）：round-3 补齐需求建模与任务拆解（聊天发送/Prompt 覆盖提交语义）。
- WTA-15（已完成）：落地 `viewer_automation` round-3 语义步骤（chat/prompt）并补齐解析/映射测试。
- WTA-16（已完成）：执行 round-3 定向回归、更新手册示例与文档状态收口。
- WTA-17（已完成）：round-4 补齐需求建模与任务拆解（timeline seek/filter/jump 语义）。
- WTA-18（已完成）：落地 `viewer_automation + web_test_api` round-4 语义步骤（timeline + sendControl.seek）并补齐定向测试。
- WTA-19（已完成）：执行 round-4 定向回归、更新手册示例与文档状态收口。

### Technical Risks
- Web 线程与 Bevy 主线程并发风险：
  - 缓解：只允许 JS 入队，不允许直接改资源。
- 测试 API 暴露到生产风险：
  - 缓解：仅在测试模式启用，默认 release 不开放。
- 语义命令解析失败导致测试不稳定：
  - 缓解：复用已有 `viewer_automation` 解析规则，并对非法输入忽略/记录。

#### Validation & Decision Record
- Test Plan & Traceability:
  - PRD-WTA-R1-001 -> WTA-8 -> `test_tier_required`（文档存在性与条目一致性检查）
  - PRD-WTA-R1-002 -> WTA-9 -> `test_tier_required`（`oasis7_viewer` 定向单测）
  - PRD-WTA-R1-003 -> WTA-10 -> `test_tier_required`（`cargo check` + 文档回写追溯）
  - PRD-WTA-R2-001 -> WTA-11 -> `test_tier_required`（文档存在性与条目一致性检查）
  - PRD-WTA-R2-002 -> WTA-12 -> `test_tier_required`（`oasis7_viewer` 定向单测）
  - PRD-WTA-R2-003 -> WTA-13 -> `test_tier_required`（`cargo check` + 手册示例可达 + 文档回写追溯）
  - PRD-WTA-R3-001 -> WTA-14 -> `test_tier_required`（文档存在性与条目一致性检查）
  - PRD-WTA-R3-002 -> WTA-15 -> `test_tier_required`（`oasis7_viewer` 定向单测）
  - PRD-WTA-R3-003 -> WTA-16 -> `test_tier_required`（`cargo check` + 手册示例可达 + 文档回写追溯）
  - PRD-WTA-R4-001 -> WTA-17 -> `test_tier_required`（文档存在性与条目一致性检查）
  - PRD-WTA-R4-002 -> WTA-18 -> `test_tier_required`（`oasis7_viewer` 定向单测）
  - PRD-WTA-R4-003 -> WTA-19 -> `test_tier_required`（`cargo check` + 手册示例可达 + 文档回写追溯）
- Decision Log:
  - 采用 `runSteps` 语义扩展而不是新增大量 JS API 方法，避免 `web_test_api.rs` 持续膨胀并贴近单文件上限。
  - round-1 优先补齐“操作语义”而非“原始输入事件回放”，以降低 Web 闭环脚本脆弱性。
  - round-2 继续优先覆盖“UI 状态切换语义”（top/locale/layout），暂不进入聊天输入文本等高自由度 payload 场景。
  - round-3 采用“结构化参数 + `%xx` 文本解码”承载 chat/prompt 文本输入，保持 `runSteps` 单入口而不新增 `sendChat/applyPrompt` 等方法。
  - round-4 采用“runSteps 承载 timeline 视图语义 + sendControl 承载 seek 控制语义”的双层方案，兼容回放与 live 模式差异（live 下 seek 明确无效回执）。
- 追溯: 历史 API 实施与验证记录由 GitHub task issue evidence 维护；当前合同以本文的“当前实现合同”为准。

# oasis7 Web UI Playwright 实跑闭环测试手册

## 定位

本手册是 Playwright 实跑测试系列的入口，管理从真实浏览器进入游戏、执行玩家操作、等待 runtime/provider 返回、并归档证据的端到端测试。

它和 `doc/testing/manual/web-ui-agent-browser-closure-manual.manual.md` 的关系：
- `agent-browser` manual 仍是通用 Web UI 闭环与截图采证入口。
- 本手册专管直接 Playwright 脚本，目标是逐步覆盖“人实际玩游戏会做的所有关键操作流程”。
- Playwright 系列可以使用 `window.__AW_TEST__` 做 readiness/state/断言读取，但玩家动作本身必须优先通过可见 UI 控件完成；除非用例明确标成 protocol-only/debug-only，否则不得用测试 API 代替点击、输入、选择或提交。

## 总目标

Playwright 实跑系列长期目标是形成一套可重复执行的玩家操作流程矩阵：

1. 启动纯本地真实栈，或复用一个已经按 runbook 启动的本地 test 环境 URL。
2. 打开真实 Viewer。
3. 完成玩家可见 UI 操作。
4. 覆盖 runtime / live websocket / viewer state / provider / message-flow 等链路。
5. 输出 screenshot、state、summary、console log 与明确 pass/fail。

这类测试回答“这条玩家操作链路在真实本地运行时是否闭环”。它不能单独证明游戏好玩，也不能替代 `L4B` embodied-agent playtest、内部真人校准或 `L5` 真实人类 / 线上样本。

## 当前入口

### PWT-001: Real Agent Chat

命令：

```bash
./scripts/viewer-real-agent-chat-regression.sh
```

复用已有 Viewer URL：

```bash
./scripts/viewer-real-agent-chat-regression.sh --url "http://127.0.0.1:4173/?ws=ws://127.0.0.1:5011&test_api=1&locale=zh"
```

若目标是本地启动 test 环境上的玩家 UI 实跑，先按 `doc/testing/manual/local-public-testnet-letai-test-environment-2026-06-23.manual.md` 启动并复核环境，再用 `--url` 指向该 runbook 输出的 URL。不要把 PWT-001 默认自启动栈当作 public_testnet attach-existing-node 证明；默认栈只证明纯本地真实 provider-backed 玩法链路。

覆盖链路：
- 本地真实 LetAI provider bridge。
- `oasis7_game_launcher` / `oasis7_viewer_live` / websocket bridge。
- 真实 Viewer 页面。
- UI 输入流程：进入沉浸模式、打开命令抽屉、填写 `#agent-chat-message`、点击 `button[data-chat-send="1"]`。
- runtime `agent_chat` ack。
- provider-backed Agent reply。
- Message Flow / `chatHistory` 中的真实 `AgentSpoke`。

通过标准：
- `summary.json` 中 `inputMode=ui`。
- `chatInputMode=ui`。
- 收到 `agentReply`。
- 回复包含默认 required fragments：`runtime:`、`data 8`、`electricity 32`。
- 回复不得包含 `[local-mock-receipt]` 或 `[local-mock-chat]`。
- 脚本结束后默认清理自己启动的本地端口。

当前 oracle 限制：`real-agent-chat-regression.mjs` 在整个 `chatHistory` 中按 Agent 与文本片段寻找回复，没有以本次发送前的历史边界或 request/correlation ID 绑定新回复；`lastChatFeedback.stage=ack` 也未绑定本次请求。旧的匹配回复或 ACK 因此可能满足现有检查。上述因果闭环仍是验收目标，脚本成功只能证明实际 UI 操作和所匹配的观测，不能单独证明本次输入导致新的 provider 回复。本说明未修改 oracle，也未声称发生过误通过。

选择契约：
- WebGPU/canvas 渲染路径必须暴露稳定 DOM Agent marker：`[data-pixel-world-agent-marker="true"][data-agent-id="<agent-id>"]`。
- `PWT-001` 必须先点击该 marker，并在 summary 中记录 `selectionMode=ui-agent-marker`，再继续执行真实 UI 聊天输入/发送。
- 若该 marker 不可见，测试必须失败并保留 failure state / screenshot，而不是退回 preselected 兜底。

主要产物：

```text
output/playwright/viewer-real-agent-chat/<run-id>/
output/playwright/viewer-real-agent-chat/<run-id>/playwright/real-agent-chat-summary.json
output/playwright/viewer-real-agent-chat/<run-id>/playwright/final_state.json
output/playwright/viewer-real-agent-chat/<run-id>/playwright/real-agent-chat.png
output/playwright/viewer-real-agent-chat/<run-id>/playwright/browser-console.log
```

<a id="pwt-planned-matrix"></a>
## 计划中的流程矩阵

新增用例时使用 `PWT-###` 编号。每个条目都必须说明玩家动作、真实依赖、断言、产物和边界。

| ID | 流程 | 状态 | 目标 |
| --- | --- | --- | --- |
| PWT-001 | Real Agent Chat | active | 覆盖真实 UI 输入聊天到真实 provider Agent 回复 |
| PWT-002 | Select Agent From World | planned | 覆盖从世界/地图可见目标选择 Agent，并验证 detail / command surface 切换 |
| PWT-003 | Submit Recommended Gameplay Action | planned | 覆盖点击推荐玩法动作、等待 ack/receipt/world delta |
| PWT-004 | Prompt Preview / Apply / Rollback | planned | 覆盖 prompt 控制面 UI 输入、预览、应用、回滚和版本刷新 |
| PWT-005 | Pause / Play / Progress Feedback | planned | 覆盖玩家控制 runtime 推进、阻塞提示和恢复反馈 |
| PWT-006 | Diagnostics And Recovery | planned | 覆盖连接异常、provider blocker、重试/刷新后的可理解报错 |
| PWT-007 | Pure API / Viewer Parity Spot | planned | 用同一场景交叉验证 Viewer UI 操作和 pure API 观测一致性 |

这四个接收用例继续保持 `planned`，直到同一候选有实际运行与留存证据；计划条目和 deterministic/local contract 不能升级成玩家流程通过。它们消费 [QA infrastructure receiver](../prd.md#qa-infrastructure-receiver) 的 source/scene/environment/oracle/artifact/tier 记录规则，场景语义与 SC/REQ/AC/DC/DE 映射见 [scene families](../prd.md#qa-infrastructure-scene-families)。

| 用例 | Source/scene 接收 | 玩家可见动作与独立 oracle | 同候选产物及未证明范围 |
| --- | --- | --- | --- |
| PWT-003 | `V-INDUSTRIAL` / `V-CONSUMER`；按 gameplay clause 固定目标、窗口与当前状态 | 通过可见推荐动作提交一次；ack 只表示请求被接收，随后需分别核对 canonical receipt、同世界 state delta、数量/lineage 与下一步反馈 | 保存输入/候选和 world identity、UI截图、console、summary、API/journal/receipt refs 和 digest；无 canonical effect 时不得写成已推进 |
| PWT-005 | `V-EXECUTION` / `V-CONSUMER`；使用当前 formal viewer profile 与明确的 runtime/provider lane | 可见地执行 pause/play/progress 控件；`queued`、`completed_advanced`、显式 blocker 与正向 world delta 分别判读，不能用按钮响应本身作为推进 oracle | 保存交互前后 state、feedback stage、同候选事件/receipt、截图和 console；headless/debug 观测不替代真实玩家 surface |
| PWT-006 | `V-RECOVERY` / `V-CONSUMER`；记录真实故障依赖、窗口及恢复起点 | 经授权注入/观察 provider、连接或恢复 blocker；重试/刷新后比较 current status、旧历史、receipt 数量和下一步，不重放非幂等动作 | 保存故障阶段序列、原始错误、fresh readback、重试次数和失败截图；没有真实依赖时标 blocked/unverified，mock 只作 plumbing |
| PWT-007 | `V-CONSUMER` / `V-WORLD-SCOPE`；viewer 与 pure API 必须锁定同一 world、branch、candidate 和 observation window | 先做一项可见动作，再从 active-provider pure API 读取同一 canonical 状态；比较语义状态、blocker、receipt/root 与 next action，不要求本地化文本相同 | 保存两侧原始字段、候选与窗口身份、provider四项身份及API/UI artifact refs；不同候选或 no-LLM observer/debug 仅为 blocked，不能记 parity pass |

provider-backed evidence 必须分别记录 provider kind、model、endpoint route、preflight evidence/provenance；凭据只留在受控环境，artifact 只记安全 digest。`test_tier_full` 的完整 headed 环境要求真实 provider-backed path 与外部 browser 的 desktop 和 narrow 两种 viewport 截图/console，并包含 provider-backed Agent parity；S6 手册和当前 PWT-004 blocker 仍分别约束其适用性，不能把本表中的计划填充当作 full proof。

### PWT-004 当前状态（2026-09-17）

PWT-004 的 2026-09-17 local-contract / HTTP 403 / no hello/tool-call 观察和当时复测交接见 [历史实施与验收记录](https://github.com/eng-cc/oasis7/issues/3732#issuecomment-6162416929)。#3732 与汇总 #3670 已以 `not_planned` 关闭，不据此安排 Provider 复跑，也不声称今天仍为 403 或已经通过。

local contract 不能代签 `AC-PROMPT-009` 或 live headed gameplay。实际 preflight 与 browser acceptance 必须分别保留同候选、环境、原始结果及未覆盖范围；当前条目无新实际玩家证据时仍为 planned/unverified。

## 新增用例规则

每个新 Playwright 实跑用例必须满足：

- 默认启动真实本地栈；若允许 `--url` 复用已有栈，必须在 summary 中记录 `gameUrl`。
- 默认禁止 mock 作为通过证据；如果某用例确实是 mock plumbing smoke，文件名和 summary 必须显式标 `mock` / `debug-only`。
- 玩家动作优先使用可见 UI locator，例如 role、label、id、data attribute；`__AW_TEST__` 只用于 readiness、state snapshot、history 查证和失败诊断。
- 失败必须落盘 `failure_state.json`、截图、console log，并给出明确错误文本。
- summary 至少包含：`ok`、`caseId` 或脚本名、`gameUrl`、关键输入、关键输出、`inputMode`、mock 禁用/检测结果、artifact 路径。
- 如果脚本启动了本地栈，默认负责清理端口；需要保留时必须提供 `--keep-stack` 并在输出中写明。

## 运行前检查

推荐先确认：

```bash
lsof -nP -iTCP:4173 -iTCP:5011 -iTCP:5023 -iTCP:5841 -sTCP:LISTEN || true
bash -n scripts/viewer-real-agent-chat-regression.sh
```

如果当前 shell 没有 `node`，脚本会尝试使用 Codex bundled Node。若在非 Codex 环境运行，需要设置：

```bash
export OASIS7_NODE_BIN=/path/to/node
export OASIS7_PLAYWRIGHT_NODE_MODULES=/path/to/node_modules
```

## 证据判读

实际 renderer 与 `renderMode`、headless/headed、候选/profile 和本轮验证目标必须一并记录。按 [S6 分流](../../../testing-manual.md#testing-software-renderer-scope)允许的软件 DOM 最小闭环只证明对应功能；不能补足 Bevy/WASM/GPU、完整视觉、producer、release 或真实 provider 的未执行部分。


一次有效 Playwright 实跑证据至少需要：

- 命令行通过输出。
- `real-agent-chat-summary.json` 或同级 summary。
- `final_state.json`。
- screenshot。
- console log。
- 若涉及 provider/LLM，必须明确是真实 provider 路径，并有禁止 mock 的断言。

PWT 系列可以作为 S6 Web UI 闭环证据，也可以作为 `L4A synthetic` 的一部分输入；但只有当后续 L4 review packet、role cards、persona cards 或 L4B agent playtest 收口后，才能支撑对应 playability 结论。

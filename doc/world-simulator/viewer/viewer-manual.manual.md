# oasis7 Viewer 使用说明书

审计轮次: 11

## 文档定位
- 本文件是 Viewer 使用说明的 canonical `*.manual.md` 入口。
- 历史兼容路径已删除；请直接使用本 canonical `*.manual.md` 手册。
- 系统级测试分层与 suite 选择仍以 `testing-manual.md` 为权威总入口。

## 目标
- 提供 `viewer` Viewer Web 主入口的统一操作手册，并说明 `software_safe` 兼容 alias。
- 统一 live server、Web 静态入口、agent-browser 闭环与常见排查步骤。
- 明确当前仓库不再提供退役的第二 Viewer 工具链。

## 适用范围
- live server：`crates/oasis7_viewer_runtime --bin oasis7_viewer_live`
- Web 静态入口：源码只保留 canonical `crates/oasis7_viewer/viewer.html`；build 将其复制到 dist 并生成 compat `software_safe.html`、`software_safe.js` 与 claim evidence route
- Web 启动脚本：`scripts/run-viewer-web.sh`
- Web 回归脚本：
  - `scripts/viewer-primary-web-entry-regression.sh`
  - `scripts/viewer-software-safe-step-regression.sh`
  - `scripts/viewer-software-safe-chat-regression.sh`
- 边界说明：本手册只适用于 `viewer` Viewer Web 页面（兼容 `software_safe` alias），不适用于 `oasis7_web_launcher` / launcher Web 控制面；后者默认先走 GUI Agent。
- 本地真实 LetAI provider-backed 游戏试玩不从裸 `run-viewer-web.sh` 开始；统一使用 `./scripts/run-local-letai-game-test.sh` 启动完全本地的 bridge + runtime/game 栈。该入口不连接 formal/public testnet。

## 本地真实 LLM 游戏测试

以下 LetAI 专属路径后续开发暂时 hold，非当前主开发路径；保留已有运行说明不表示恢复排期。具体范围与既有安全/订单义务见[LetAI 开发状态](../../p2p/token/mainchain-token-newapi-quota-bridge.prd.md#letai-development-hold)。通用 Viewer/Provider 和其他供应商接入不因此暂停。

以下为 Bash 下的源码启动路径，使用 LetAI provider 和本地 standalone world，不连接 formal/public testnet。首次构建与已有产物后的日常启动分开操作。

### 首次运行准备

- 在仓库根目录使用[指定 Rust 工具链](../../../rust-toolchain.toml)，准备 Node.js/npm、Python 3 和 curl。首次构建需要可下载依赖的网络；本地端口须未被其他进程占用。
- 准备自己的有效 LetAI 配置文件，存放在仓库外，用 `--config` 传入绝对路径。已有项目 token 可按下面的格式填写；账号需要不同模型时，在启动命令加 `--model <可用模型>`。
- 没有本机默认代理时使用 `--no-default-proxy`；它只禁止脚本补入默认代理，保留已有 HTTP/SOCKS 代理环境。需要代理时使用自己的可用配置。

```text
base_url = https://api.letai.run/v1
token_key = <自己的有效项目 token>
```

配置规范化见[现有 helper](../../../scripts/ensure-letai-local-token-config.sh)：已有 `token_key/api_key` 时生成本地规范化配置；只有 `platform_key` 时会调用平台 API 创建或更新测试用户/项目。

真实 provider smoke 与 Agent 决策会使用账号额度。脚本的 `--auto-topup-usd` 默认金额为 `0.1` USD；遇到额度错误，且最终配置或继承环境同时提供平台 key 与用户 ID 时，bridge 才会尝试补充平台用户额度。启动前确认已有平台环境与账号额度策略，具体条件见[自动补额实现](../../../crates/oasis7_provider_local_bridge/src/bin/oasis7_provider_local_bridge/letai_direct_topup.rs)。不要提交凭据文件或把密钥贴进 Issue。

可以先检查本地启动条件：

```bash
./scripts/run-local-letai-game-test.sh \
  --config /absolute/path/to/letai.env \
  --no-default-proxy \
  --preflight-only \
  -- \
  --viewer-port 48420 \
  --web-bind 127.0.0.1:48421 \
  --live-bind 127.0.0.1:48422
```

`--preflight-only` 在 provider 启动前退出，只检查配置文件存在、端口、Node/npm 与所选模式的构建产物；不验证模型凭据、额度或真实玩法。

### 首次前台启动

首次下载源码或缺少构建产物时，**不要使用 `--local-world-playtest`**。该 preset 强制启用 `--reuse-existing-build`，仅删除同名显式参数不能恢复构建。

先编译 provider bridge，避免把首次冷编译放进 bridge 的短就绪等待窗口：

```bash
./scripts/cargo-dev.sh build \
  -p oasis7_provider_local_bridge \
  --bin oasis7_provider_local_bridge
```

再以前台方式启动。此命令不跳过 launcher 的 Cargo 构建步骤；Cargo 按源码状态复用或更新缓存，launcher 按需准备后端二进制和 Viewer 资产。默认 strict provider smoke 会请求真实模型，`--no-auto-play` 只要求进入页面后手动 Play。

```bash
./scripts/run-local-letai-game-test.sh \
  --config /absolute/path/to/letai.env \
  --no-default-proxy \
  --no-auto-play \
  -- \
  --viewer-port 48420 \
  --web-bind 127.0.0.1:48421 \
  --live-bind 127.0.0.1:48422 \
  --json-ready
```

保留启动终端。等待本次输出目录的 `launcher/session.meta` 出现 `STACK_READY=1`，打开其中的 `GAME_URL`；该字段只说明 HTTP/WebSocket listener 就绪。本地 chain 的持久化就绪还要核对下方的 `snapshot.json` 和 `journal.json`，真实玩法则要观察 provider 决策、动作与世界结果，不能只以页面打开判定成功。

停止这条**前台运行**时，在原终端按 `Ctrl+C`，由现有 cleanup 回收本次 launcher 与 bridge。不要将它当作后台 preset 的停止方法。

### 已有构建后的日常启动

```bash
./scripts/run-local-letai-game-test.sh \
  --local-world-playtest \
  --config /absolute/path/to/letai.env \
  --no-default-proxy
```

该 preset 默认后台运行，关闭终端不会停止服务；运行元数据、supervisor PID/脚本和日志在本次输出目录，macOS 还可能使用 launchctl。它没有统一的跨平台停止入口。首次使用或需要明确的终端停止方式时，继续使用上面的前台路径；不要按进程名称批量结束其他任务。

- 该入口负责 LetAI token config 规范化、默认 Rust direct `127.0.0.1:5841` provider bridge、Rust bridge chat probe/provider contract smoke 与 launcher/runtime/viewer 启动。
- 已有与当前源码匹配的构建后，可使用 `--local-world-playtest` 日常 preset。它固化 `--startup-profile playtest`、`--provider-smoke-mode skip`、`--reuse-existing-build`、`--detach`、默认 `--auto-play`、viewer/web/live 端口 `48420/48421/48422`、`--json-ready`，以及 wrapper 默认的本地 standalone chain。若需要复现暂停态或手动 Play 调试，可显式传 `--no-auto-play`。
- 启动后等 `<output-dir>/launcher/session.meta` 出现 `STACK_READY=1`，再打开其中 `GAME_URL`。默认日常 URL 形如 `http://127.0.0.1:48420/?ws=ws://127.0.0.1:48421&test_api=1&locale=zh`。
- 更新源码后先重新构建，再使用日常 preset；首次冷构建采用上面的前台路径。需要严格 provider smoke 时显式传 `--provider-smoke-mode strict`；临时换 HTTP 端口可在末尾透传 `-- --viewer-port 4174 --json-ready`。
- 默认 chain-enabled 路径会启动 launcher-managed chain runtime，并通过 `--chain-local-standalone-test` 保持本地 submit -> commit -> snapshot 闭环可在单节点试玩栈内完成。该路径只有在 `output/chain-runtime/<node-id>/reward-runtime-execution-world/snapshot.json` 与 `journal.json` 都出现后，才算 chain-enabled 本地世界就绪。
- launcher 的 `--chain-execution-world-dir` 统一 writer/Observer 的本地 root，默认是 `output/chain-runtime/<node-id>/reward-runtime-execution-world`。外部 status 必须由操作者显式传入本地 root；远端 `execution_world_dir` 不参与读文件。Observer 等待可验证的 immutable generation checkpoint，缺失显示初始化中；`RecoveryRequired` 等待 writer 修复，`ResourceLimited` 拒收并保留上一观察状态，不能裁剪队列或修改世界。
- launcher 管理 `oasis7_chain_runtime`、进程编排与持久 execution-world readiness；Viewer 只观察 runtime/world 输出。snapshot/event 是观测面，恢复与 replay 仍以权威 state + journal/event chain 为准。
- 如果页面停在“认领已提交，正在等待链上 committed 快照同步”，先检查启动命令是否漏掉 local standalone chain 配置；漏掉时 gameplay action 可能已进入 pending consensus queue，但本地单节点不会完成 commit/snapshot。
- 如果页面短暂打开后出现 `viewer.ws` / WebSocket 错误，先检查脚本输出目录下的 `launcher/oasis7_viewer_live.log` 是否报 execution-world persistence ready gate，而不要先把它归因成 Viewer 前端问题。
- 如只为人工查看页面或排查 WebSocket，可在 wrapper 参数后透传 `--chain-disable` 作为临时 page-play mitigation；该模式不代表本地 standalone chain-enabled 世界启动通过，更不能作为 public_testnet 证据。
- 下方 `oasis7_viewer_live` + `run-viewer-web.sh` 流程只用于 Viewer/debug 或定向 Web 回归，不代表本地真实 provider-backed gameplay 栈。

需要初始暂停并手动点击 Play 时使用：

```bash
./scripts/run-local-letai-game-test.sh --local-world-playtest --no-auto-play \
  --config /absolute/path/to/letai.env --no-default-proxy
```

`--no-auto-play` 只改变启动后的自动播放选择，不跳过该启动 profile 仍会执行的预检或远端请求；
默认命令会自动 Play，不能把它记录成暂停态样本。

## 底层 Viewer Debug 快速开始

### 1）启动 live server
```bash
env -u RUSTC_WRAPPER cargo run -p oasis7_viewer_runtime --bin oasis7_viewer_live -- llm_bootstrap --allow-debug-scenario --llm --bind 127.0.0.1:5023 --web-bind 127.0.0.1:5011
```

Major World Event audience policy is a separate explicit runtime option. The default is
`--major-world-event-visibility unknown`, which exposes no Crisis event. A trusted deployment may
set `public` or `restricted` only when its operator/session policy has supplied that authority;
`denied` explicitly suppresses the event. Login, Agent selection/control, hosted join, and Director
diagnostics never change this option implicitly.

For the isolated synthetic loopback DevLocal W3 recipe, pass
`--major-world-event-visibility restricted` explicitly. This is an operator policy for that
synthetic lane only; the launcher default remains `unknown`, and hosted/public runs must not
inherit this value without their own authority decision.

- `oasis7_viewer_live` 当前默认走 runtime/world 链路。
- `oasis7_viewer_live` 负责 Viewer live server 与可选 Web bridge，不内嵌 node、consensus、reward runtime、topology 或 execution-world persistence，也不提供 `--tick-ms` 作为外部推进时钟。它仍可通过 `--chain-status-bind` 观察 committed world，并在配置 status bind 时通过 `--chain-submit-bind` 提交 chain-linked action；这些是 client endpoints，不授予 node ownership。Viewer event delivery 不能等同于 node/consensus tick。
- 正式 gameplay 要求已配置且可连通的 LLM provider。
- 若显式改用 `--no-llm`，则该链路只可用于 observer/debug，不计入正式 gameplay 证据。

### 2）启动 Web Viewer
```bash
env -u NO_COLOR ./scripts/run-viewer-web.sh --address 127.0.0.1 --port 4173
```

- 默认访问地址：`http://127.0.0.1:4173/?ws=ws://127.0.0.1:5011`
- 当前仓库只提供 `viewer` 单一 Web / UI 入口；`software_safe` 只保留为兼容 alias，不再作为正式模式名。

### 3）前置依赖
- Node.js / npm
- `python3`
- 若要跑 agent-browser 闭环，使用并通过 `agent-browser 0.37.1` 的 `agent-browser --version` 验证；首次安装浏览器二进制执行 `agent-browser install`
- 每轮闭环先执行 `agent-browser doctor --offline --quick --json`；若 daemon/session 异常，只诊断并回收本轮自有 session，不影响其他任务
- 每轮 agent-browser 操作必须使用 worktree-scoped named session；禁止使用默认 session 或全局清理

### agent-browser 会话生命周期
- 创建：`AB_SESSION="$(agent-browser session id --scope worktree --prefix viewer)"`，并把 `AB_SESSION` 传给本轮的每一条命令。
- 记录：执行 `agent-browser --session "$AB_SESSION" session info --json`；需要审计并行任务时使用 `agent-browser session list --json`，不要猜测或复用其他任务的 session。
- 等待：页面加载使用 `wait --load domcontentloaded`，应用就绪使用 `wait --fn` / `wait --text`；Viewer 的长连接不以 `networkidle` 作为唯一稳定条件。
- 回收：headed session 不依赖默认 idle 回收；脚本必须用 `trap` 在退出时显式关闭本轮自有 session。共享 CDP tab 时先使用 `--pin-tab`，仍由创建者负责关闭。

## 页面能力
- 当前页面聚焦 `viewer` 实时观察与正式玩法摘要。
- 支持 `locale=zh|en` 初始化和页面内中英文切换。
- 支持最小 prompt/chat 控制面；仅在 auth/bootstrap 可用时开放。
- 在 `hosted_public_join` 路径下，页面支持获取/释放 hosted `player_session`、`reconnect_sync` 恢复，以及 `prompt_control` 的 preview-grade `strong_auth`（需 `Backend Approval Code`）。
- 面向普通用户的启动入口默认且只暴露 `hosted_public_join`。旧 `trusted_local_only` 本地可信预览不再作为可选用户流程。账号接入沿[免邮箱注册与多凭证账号合同](../../p2p/blockchain/hosted-public-join-key-based-onboarding.prd.md)：identity v2 在支持环境推荐 Passkey，邮箱可选，实际入口受部署 capability 与注册开关限制；关闭新注册不等于关闭已有账号登录。账号已登录、世界准入与 runtime 控制绑定分别确认，不由登录成功推导可控制 Agent。操作及验收见[Hosted 多凭证账号手册](../../testing/manual/hosted-passkey-account.manual.md)。
- 当前 `PasskeyAccountSurface` 仅在 capabilities HTTP 404 时显示 legacy 登录表单；其他能力检查错误保留明确失败，不自动降级。legacy 只适用于[专业兼容条件](../../p2p/blockchain/hosted-public-join-key-based-onboarding.design.md#migration)允许的部署，不是并行推荐的新账号路径。
- [Viewer 整体旅程与状态稿](viewer-page-module-design-2026-06-18.design.md#uiux-entry-session-world)补充账号、安全退出、目标与绑定、模型连接和原结果阅读。图稿是设计目标，不证明当前部署、真实 Passkey 设备、供应商调用、恢复或完整可玩性已经通过。

### Hosted public join email-free test login（仅本地 QA）

该入口只用于验证 hosted join 后续的 W3 runtime、签名和 registration grant 闭环，默认关闭，不替代生产 Passkey、邮箱 OTP、邮箱所有权验证或 hosted account 映射。

- 启用条件同时满足：launcher 使用 `hosted_public_join`；Viewer 静态 HTTP 只绑定 loopback（`127.0.0.1`、`::1` 或 `localhost`）；并显式设置 `OASIS7_HOSTED_TEST_LOGIN_ENABLED=1`。未满足任一条件时，test-login route 返回 `404`。
- 页面还必须在 URL 中显式加入 `hosted_test_login=1`；浏览器随后生成正常的临时 Ed25519 device key，只提交 public key，并消费后端实际签发的 player/session/release/registration grant。浏览器不生成 player id、release token、grant 或签名。
- 若验证 W3 的 hosted `prompt_control` strong-auth，启动前只检查以下三个变量均已存在且非空：`OASIS7_HOSTED_STRONG_AUTH_PUBLIC_KEY`、`OASIS7_HOSTED_STRONG_AUTH_PRIVATE_KEY`、`OASIS7_HOSTED_STRONG_AUTH_APPROVAL_CODE`。检查只输出 presence，不输出任何值；缺少任一项时保持 blocked，不能用占位值或伪造批准继续。
- 本地 QA 启动 recipe（沿用现有 launcher/provider 前置条件）：

```bash
OASIS7_HOSTED_TEST_LOGIN_ENABLED=1 \
OASIS7_HOSTED_REGISTRATION_ISSUER_PRIVATE_KEY=<issuer-private-key> \
./scripts/run-launcher-stack.sh \
  --deployment-mode hosted_public_join \
  --viewer-host 127.0.0.1
```

打开 launcher 输出的 `GAME_URL`，或直接访问 `http://127.0.0.1:<viewer-port>/software_safe.html?hosted_test_login=1`，再选择 `Email-free Test Login`。不要用 `--viewer-host 0.0.0.0`、`::` 或其它非 loopback 地址启用该入口。

- 这条 recipe 只证明 issuer 签发之后的 W3 downstream 行为；不覆盖生产 Passkey、OTP 发送/校验、邮箱所有权、hosted account 映射或普通账号登录验收。生产及普通 hosted join 使用部署实际开放的多凭证账号入口；新注册关闭、邮箱不可用与 Passkey 不支持分别处理，不能用此 QA 入口绕过。账号/世界控制分层和真实设备验收见[Hosted 多凭证账号手册](../../testing/manual/hosted-passkey-account.manual.md#world-control)。
- `main_token_transfer` 仍保持阻断，页面只显示 lane verdict，不提供资产转账表单。
- 页面不再提供第二 Viewer 跳转，也不再承担退役视觉专项工具职责。

### Terminal shell target versus current operation
- 终端 shell 的 Player/Director、`World / Targets / Command`、Search/Quote、
  Action Receipt 与 World Feed v1 产品契约，以
  `viewer-gameplay-release-experience-overhaul.prd.md` 为准。
- 当前任务分支的 Player 操作路线已经是 stage-first：Targets 继续使用
  `#entity-search` 过滤/导航，Targets 与 Command/Chat 通过按需 drawer 进入，
  Diagnostics 保持次级，Recent Events/Feedback 仍按现名显示。Focus hooks 只服务
  Player 沉浸呈现，不是第二产品模式。World Feed v1 已有 runtime transport、状态机
  与 `#viewer-world-feed` panel，但仍是只读环境上下文，不是 Action Receipt 替代。
- 当前 Player 已是 fullscreen world-first 单舞台；不要按旧“左侧/中间/右侧”三栏 IA
  操作或编写说明。HUD 由单一 `Next Move` 主动作区和 `Objective / Player Leverage`
  从属上下文组成；桌面并列，窄屏主动作在上、上下文在下。密集三栏仅可能在显式、
  能力门控的 Director 工具视图出现。
- Director 的 `#viewer-director-entry` / `#viewer-director-exit`、Viewer 状态机与
  `/api/public/director/capability` fail-closed endpoint 已实现。当前 endpoint 在没有
  可信 issuer 绑定 live `session_epoch` 时返回 `director_capability_unavailable`，
  因此生产环境成功进入仍是 `capability_blocked`；queued/accepted 也不等于完成。

### Expanded Player visual contract (target handoff; not current release evidence)
- 实现目标的阅读顺序是 attention（仅在需要时）→ `Objective` → 一个主导性的
  `Next Move` 动作 → `Player Leverage`/selected context → `Action Receipt`；World Feed
  继续作为收起的环境 chip。`world_read` 的 `tick/agents/routes/fragments/hotspots` 只
  在既有权威 projection 提供时显示，不能补造 Power、Data、Consensus 或其他指标。
- `Next Move` 已按单一 CTA 和内联 blocker 组织，当前源码不再是三张等权 HUD 卡片。
  具体输入、浮层遮挡、失败/恢复和可访问性是否通过，仍以实际候选的行为证据为准。
- Agent Console 默认显示当前选择、已授权的 command/chat、blocker 与 receipt。无选择或
  不可控时只提供 Targets/认领/等待 binding 的恢复说明；raw snapshot/JSON、auth/
  capability、Prompt override、world-scale 和 Director 工具均保持在收起的
  `Diagnostics / More` 中。
- 用户-facing Focus 名称为 `Cinematic View / Minimal HUD`，内部 `focusMode` hooks
  保留。显式进入后保留世界、选择标记、当前 objective/blocker、存在中的 receipt 和
  退出入口；Escape 先关局部 drawer，再退出呈现层，IME composition 不被打断。
- 这些视觉约束不改变 runtime/protocol、仿真/replay、renderer、权限、
  Director capability 或 receipt 因果。受影响 QA 必须提供 headed `1440x1000` 与 `390x844`
  截图，并覆盖键盘、IME、CJK、长文本和无横向溢出。

### World-native target handoff（目标契约，不是当前实现声明）

本轮升级借鉴的是稳定的阅读框架，不是 Agentbox 的永久三栏布局。稳定的是世界舞台、
Objective、selected-agent context、单一 `Next Move`、Receipt 和边缘导航的阅读顺序；
Inspector、Command、World Feed 与 Diagnostics 仍按需出现，并在关闭后回到触发它的入口。
Player 不采用永久左/中/右列，也不采用可直接执行世界动作的底部 hotbar。

- **世界解释语义**：World Stage 直接表达已发布的实体身份/状态、活动/路线、关系类型/状态
  和重大事件。周边 UI 只负责解释、定位和操作；不能由距离、同屏、靠近、数量或 event text
  猜 owner、resource、trust、relation 或因果。
- **实体优先（P1-A 后续目标）**：P1-A 只从选中的 Agent 进入 Agent Context；Location、
  Facility、Territory、Organization、Depot、Module 的 generalized Inspector/Console 属于
  后续 P1 slice，不能覆盖或扩大 Agent-only P1-A。Market、War、Governance、Production、
  Contracts、Relations 是后续实体能力，不是新的一级 Player 路由；缺少权威 projection 或
  权限时显示 unavailable/只读恢复路径。
- **间接控制**：玩家发送 `Ask`、`Suggest`、`Prioritize`、`Negotiate`、`Investigate`、
  `Propose`、`Warn` 或 `Delegate` 等 guidance/template。必须能区分
  `Guidance → Intent → accepted/rejected/blocked → Activity → World Action → Receipt`；
  accepted 不等于 executing，executing 不等于 completed。
- **因果与环境**：当前 presentation 只保留一个紧凑、可持续复查的 Action Receipt；新 toast
  不能覆盖或伪造 Receipt，只有新的权威 receipt 才能替换它。World Feed 默认是收起的 ambient
  chip，loading/empty/replay/gap/unavailable 各自有状态，绝不代替 Receipt。
- **玩家语言与诊断**：普通界面使用 `Program`、`Protocol`、`Module`、`Contract`。WASM、ABI、
  hash、deployment tx、runtime id、provider 和 auth material 只在显式 Diagnostics/Director
  disclosure 中展示；raw object 不能创造一个玩家可用的 capability。

可见值必须带有可追溯 source、logical position、freshness 和 permission 语义：current、Last
known/stale、reconnecting、unknown、conflict、unavailable、control lost 要有可区分的玩家
反馈；缺失或无权限时隐藏/脱敏/说明恢复，不用默认值或浏览器时间填空。以上是分阶段目标，
不表示当前 Web 已完成这些改造或已获得发行放行。

### World-native rollout（目标顺序）

1. **P0 Shell Lite**：收敛现有 fullscreen shell 的阅读顺序、单一 Next Move、轻量 Objective/
   selected-agent context、Receipt anchor、collapsed Feed 和 edge navigation。
2. **P1 Context + semantics**：在已有 projection 上组合按需 Inspector/Console，并补充实体/Intent/
   Activity/关系/事件的 headed、键盘、CJK、空态和 fallback 证据。
3. **Deferred / contract-gated**：全局资产框、完整 capability hotbar、新资源/ownership/
   relationship/经济字段和新的取消/批准/重排动作，待产品、玩法、runtime、agent 的规则、
   source、freshness、permission、idempotency 与 receipt 合同明确后再实现。

### P1-A Agent Context Lite（源码与聚焦自动化已完成；headed 证据部分完成）

P1-A 是当前 Viewer 的第一个 Agent-only context slice，不是通用多实体 Inspector。

- 其固定组合是按需 Command/Inspector 中的 selected-Agent context。它把选中 Agent 的
  可读 identity/location、activity/state、Objective、已发布 Next Move、blocker、Player
  Leverage 和明确匹配的 Intent 放入上下文；Chat 仍是授权控制入口。本手册不规定源码模块
  或组件名称，具体实现边界以配对 PRD/design 为准。
- display model 只格式化已有的权威世界/玩法投影、connection/freshness 和语义反馈；
  普通 feedback 只能作为非因果状态。它不新增 runtime/protocol 字段，不从 Feed、事件文字、
  坐标、时间或 raw diagnostics 推断计划、成功、关系、ETA 或资源。
- `Action Receipt` 仍是唯一玩家因果反馈：只有明确的权威因果身份才算 Receipt；Intent
  生命周期、普通 feedback 和 World Feed 不能创建或替换它。Player/Cinematic 每个呈现只有
  一个正式 Receipt；`accepted`、`queued`、`executing` 和 `completed` 按实际权威状态区分。
- 选中 Location、Facility、Territory、Organization、Depot 或 Module 时，当前 Context Lite 展示
  可读 identity 和 honest unavailable/待同步恢复说明；不得显示上一 Agent 的上下文，也不
  得凭 proximity、同屏或名称生成 ownership、resource、relation 或 capability。相应实体
  projection 和权限合同完成后，才可由后续 slice 注入内容；通用多实体 Inspector 是
  P1-A 之后的目标，不属于本切片。

该 slice 的完整验收仍要求：Agent selected 的 current、last-known/stale、missing、reconnecting、
unknown、conflict、replay、gap、reorg、unavailable、control/permission-lost 状态可区分；
非 Agent 选择不泄漏 Agent 状态；Player/Cinematic 每个呈现只有一个正式 Receipt；并补齐
headed `1440x1000`、`1024x768`、`768x1024`、`390x844`、键盘/IME、CJK、长文本和空态验证。
Context Lite 的源码与聚焦自动化已存在。早期外部 Chrome 样本仅验证 GPU-unavailable fallback
（`1440x1000`、`390x844`），不能给 `ready=true` WebGL2/Cinematic、probe 后异步 GPU 失败
或 tablet `641–1240` 放行。后续 #3624/#3638/#3668 的实际证据仍按各自范围保留，不由旧环境
限制改写为全部未验收；本段也不单独构成完整 headed 验收或发行放行结论。

### P1-B/P1-C 当前实现边界

- **P1-B World semantic presentation**：当前把已发布的 Agent、Location、Facility、Territory、
  Organization、Depot、Module identity/state/activity，以及 route/assignment、power/resource、
  facility/module cues 和 relation kind/status 组织到稳定的世界舞台语言。注意力优先级为
  普通 Agent < selected Agent < active Intent < blocker < receipt target；major event 只在
  P1-C authority gate 通过后作为后置扩展。persistent selection 与 transient attention 必须
  区分，命中目标、键盘焦点和可读身份不能因高亮消失。
  不得从距离、同屏、proximity、文字、坐标或动画推断 relation、Trust、ownership、resource、
  capability 或因果。
- P1-B 内部 presentation DTO 只允许从既有 authoritative snapshot 派生；例如 active Intent 的
  舞台目标只能来自 `snapshot.player_gameplay.primary_intent`。来源缺失、陈旧、冲突或未授权时
  不显示，不得从 summary、feedback、Activity、文本或 renderer 状态回退推断。
- Renderer Unavailable、探测后异步失败、fallback、retry、safe-area 和恢复属于跨切片的安全
  验收，不是 P1-B 的语义来源。Renderer 不可用时仍保持可读世界入口和诚实 retry；retry 不承诺
  为无能力浏览器添加 WebGL2，也不生成 `LIVE`、world progress 或 Receipt。P1-B 不改变
  runtime/protocol，也不新增玩家可见字段。
- **P1-C Major World Events**：当前只接受 runtime journal 投影的 CrisisSpawned/Resolved/TimedOut，
  显示数值 severity `1..=5` 和 active/resolved/timed_out 生命周期。它们只属于 ambient World Feed；
  runtime 会把 journal 与 canonical `CrisisState` join；缺失或字段冲突时不显示，初始 feed 只有
  canonical 最新 transition 为 current，较早 transition 与 cursor replay 都是历史。
  feed 保留 canonical opaque subtype；同 identity 冲突 payload 或未声明的 epoch 切换会清空
  attention 并进入 snapshot reload，不会任取一条或静默拼接。
  crisis 没有权威空间位置，所以没有 stage marker/highlight，也不能替代 Receipt。回放只保留历史，
  gap/reorg/权限丢失会清空事件注意力；生产 session 没有显式 audience policy 时不显示。War、
  Governance 与其他类别仍不在 allowlist 内。

### Terminal Player shell playbook
This section describes the implemented Player-shell, World Feed, and Director
security-boundary routes in the current task branch. It is not a release claim:
World Feed has no inferred receipt links, and production Director success remains
blocked until a trusted issuer is available.

1. Fresh load/reload/new tab/session starts in Player. Read the world-first stage,
   the single `Next Move` primary action and subordinate `Objective / Player Leverage`
   context, then use the quiet `World / Targets / Command` route. On narrow screens,
   the primary action occupies the first row and supporting context the second.
   The [shell PRD](viewer-gameplay-release-experience-overhaul.prd.md#current-implementation-boundary)
   owns this layout; `More / Diagnostics` remains a reachable secondary route.
2. Search remains inside Targets and only filters the authoritative visible list.
   Selecting a target recenters/highlights and opens context; it never executes.
   Quote is reached from contextual Command, not a peer rail item.
3. Action Receipt is the only player-causality surface. `accepted`/`queued` is
   waiting, not completion; no receipt is shown explicitly as `No action receipt yet`. The target
   compact presentation keeps one anchor available for review until a newer authoritative receipt
   replaces it; transient feedback cannot create a second causal surface. This target is not current
   release evidence.
4. World Feed is the implemented `world_feed/v1` ambient projection. Its identity/dedup
   key is `(world_id,reorg_epoch,event_seq)`, source order is ascending, and the current
   runtime emits nullable `receipt_ref=null`; a link is rendered only when runtime
   supplies explicit causal identity. In fullscreen Player it is a compact collapsed
   top-right edge overlay/ambient chip that opens on demand, never a persistent column.
   Wire `reorg_epoch` and `event_seq` are exact decimal strings (legacy numeric input
   remains accepted); the Viewer compares them exactly and presents canonical ascending
   event order.
5. Director is a capability-gated secondary Diagnostics/operator action, ephemeral to
   the tab. The server validates a short-lived `director_open` grant; invalid/stale/
   revoked/unauthorized/unavailable entry returns to Player, sanitizes Director
   surfaces, preserves world/selection, and explains recovery. No trusted issuer is
   currently wired, so the successful production path remains `capability_blocked`.
6. Escape closes the local surface/drawer first, then Focus; while IME composition
   is active it must not close the Viewer surface. Closing returns focus to the invoker.
7. Target Director controls use `#viewer-director-entry` and `#viewer-director-exit`;
   these source anchors are implemented but do not persist capability. Fresh load,
   reload, and new tab remain Player; denied/stale/revoked/unavailable capability fails
   closed to Player.

8. The canonical pixel renderer is the WebGL2 runtime. GPU-enabled environments have a
   `ready` path; when WebGL2 is unavailable (including `canvas.getContext("webgl2")`
   returning null), the page exposes `Renderer Unavailable` with a diagnostic reason and
   does not present a false ready canvas.

### Resolved World Feed and resource-readout defects (#3315)

The following focused implementation defects are resolved on this branch; use the
named tests as the current evidence, not the historical browser artifacts:

- **P1 cursor continuation:** `world_feed_transport.js` now continues an initial page
  from the returned cursor and refreshes once after world activity. See
  `world_feed_transport.test.js` tests
  `continues an initial page asynchronously from the returned cursor` and
  `refreshes the latest cursor once after world activity and does not overlap in-flight requests`.
- **P2 gap/reorg retention:** `consumeWorldFeed()` clears stale `events` on a gap before
  snapshot reload. See `world_feed_state.test.js` test
  `stops appending on gap/reorg and requires authoritative snapshot reload`.
- **P2 multi-resource fallback:** the selected-resource formatter now parses every
  ` · `-joined host entry. See Rust test
  `render_selected_resource_readout.rs::resource_readout_formats_each_structured_entry_without_raw_json`.

The #3248 browser evidence referenced by the surrounding Viewer docs is historical. For
#3315, external Chrome freshly verified only the GPU-unavailable fallback at headed
`1440x1000` and `390x844`; the `ready=true` WebGL2/Cinematic path, post-probe async GPU
failure, and tablet widths `641–1240` remain unverified. This is browser evidence for the
fallback surface, not a release-readiness claim.

### Target QA evidence checklist
The implementation owner must supply headed evidence for desktop `1440x1000`, tablet orientations,
and mobile `390x844` shell hierarchy, Director allowed/denied/stale/revoked entry (the allowed path is
currently blocked by the missing trusted issuer), duplicate-ID/anchor
checks, keyboard/IME/Escape focus return, receipt states, world semantic entity/relationship/event
rendering, entity-first contextual capabilities, Programs/Protocols disclosure, empty-world and
missing/stale/permission-loss states, feed loading/empty/replay/gap/unavailable/reorg recovery, and
English/Chinese long-text overflow. Source/package
tests now cover the World Feed and fail-closed Director boundary; this manual does not
turn those tests into issuer-backed production access or a release claim.

<a id="uiux-design-prototype-walkthrough"></a>
### UI/UE 设计原型走查（#5527）

本节验证[三条可操作设计旅程](viewer-page-module-design-2026-06-18.design.md#uiux-interactive-prototype)与[展示/状态合同](viewer-page-module-design-2026-06-18.design.md#uiux-prototype-presentation)，对象是本地 fixture 原型；上文生产 Viewer 验收继续沿原消费者。以下是可复用方法，不登记英文审校、真实浏览器、原生 IME 或辅助技术已经通过。

从仓库根运行 `python3 -m http.server 8765 --directory doc/world-simulator/viewer/assets/ui-ux`，同机访问 `http://127.0.0.1:8765/interactive/`。使用获准的托管 HTTP 预览时，完整保留 `interactive/`、`pixel-world/` 相对路径，并确认页面与素材属于同一候选。浏览器不能访问该入口时记录阻断及受影响项目，不改用被策略禁止的协议或绕过沙箱。原型不需要游戏账号、Key、模型或世界后端。

源码状态检查复用 Viewer 已声明的 `jsdom` 开发依赖；依赖安装后，从仓库根执行 `node doc/world-simulator/viewer/assets/ui-ux/interactive/prototype.test.cjs`。该脚本只向 stdout 输出逐项结果及受测 JS 哈希，失败返回非零，不写原型文件；需要报告时由调用方显式重定向。它覆盖原请求、草稿/权限、三条旅程及展示切换，不能替代下面的真实布局和输入走查。

先由非作者按页面说明依次完成 `#entry` 的进入/发送/查询、`#prompt` 的清除/应用/过期恢复、`#industry` 的补给读回/提交/原结果/重入，再走下表。顶部旅程和情境会重置演示；元素 `#locale`、`#copy-mode` 仅改变展示。它们不是 URL 旅程 hash，也不新增玩家 Help 页面；帮助仍在操作所在面板，生产语言入口沿既有 More/语言表面。

| 项目 | 操作 | 应核对的结果 |
| --- | --- | --- |
| UP-01 语言与身份 | 在 `#entry` 选“观察 B / 草稿 C / 原请求 A”，填写多行中文草稿，依次切换 `#locale` 的 `zh-CN`、`en` 与 `#copy-mode`，再查询原指导结果；在 `#prompt` 的 stale/unknown 情境重复展示切换 | 草稿逐字保留，观察 B、草稿 C、原请求 A 及其状态不串写；切换不发送、查询、丢弃编辑或重置情境。界面名称随语言切换，玩家输入、Prompt 值与原请求标识不被翻译 |
| UP-02 长文与完整画面 | 三条旅程分别选 `long`，检查长世界名、Targets 显示名和对应面板说明；再返回 `normal` | 同一对象身份和配方/成本数值不变；主动作、错误、关闭、原回执和帮助可读、可达，长名称不覆盖世界/面板入口 |
| UP-03 缺译回退 | 在 `#prompt` 选择 `en` + `missing`，读取第五种来源说明，再切回 `normal` | 缺译处显示中文原文和明确提示，原文 `lang="zh-CN"`；无翻译 key、空白或假成功。恢复普通模式只改变文案；本例只验证指定缺译点 |
| UP-04 当前对象的位置 | 在 `#entry` 选“位置未知”，依次查看 A、C、未知目标 | A/C 显示各自已知位置及当前获准动作；未知目标只保留文字状态，不出现虚构坐标或定位动作 |
| UP-05 集合成员选择 | 在 `#entry` 选“同位置密集目标”，查看独立的未知目标，再查看 A/B/C 任一成员 | 查看未知目标时，三人集合不保留选中外框；选中成员时恢复集合选中态，目标身份与草稿主体各自保持 |
| UP-06 失权后的 Targets | 在 `#prompt` 选“控制权失效”，打开 Targets 并观察 A/C | 不出现可指导状态或新建指导草稿动作；允许的观察与已隐藏私有值保持一致，语言/文案切换也不恢复权限 |
| UP-07 退出和重入工具栏 | 在 `#entry` 选身份分离情境，打开示例账号并退出，再选择“恢复已有示例会话” | 工具栏依次显示实际 `first`、`return` 情境，旅程/示例回读/供给读回同步；不会留下旧情境标签冒充当前状态。此步骤只验证本地演示重置，不证明真实会话持久化 |

布局取同一候选的 `1440×900`、`1280×720`、`1024×768`、`390×844`，以及 Command 展开的 `844×390`。至少在窄屏/低高度检查英文与 `long`，在 Prompt 检查 `missing`；`文字 200%` 是原型根字号模式，真实浏览器 200% zoom 另记结果。以 Tab/Shift+Tab、Enter/Shift+Enter、Escape 走发送、查询、关闭、丢弃确认与取消；原生中文组合输入另验，不能把合成 `isComposing`/229 事件计为真实 IME 通过。

在原 Issue/PR 记录候选 commit 或明确的未提交差异、预览入口、浏览器、视口、语言/文案模式/情境、实际步骤及截图；非作者指出看不懂或无法完成的步骤。源级 DOM 结果只证明状态逻辑，截图只证明实际捕获的画面，缺少原生输入或辅助技术条件的项目分别写待核/受阻。对照[截至 4a477f8 的源码差异](viewer-page-module-design-2026-06-18.design.md#uiux-prototype-source-delta)时，`LiveControlStatus` 只表示 Viewer 新决策调度，不能读作世界暂停或在途作业取消。设计原型的验证不替代生产消费者验收；生产接入也不新增为 #5527 的关闭条件。

<a id="industry-detail-acceptance"></a>
### 产业关系与细节层级验收入口（未来适用）

当正式 surface 支持产业 projection 时，按 Pixel-world player-readable rendering 的目标合同，在同候选与权威数据下进行 external headed desktop+narrow 验收：确认节点/流类型/方向/吞吐/损耗、R1~R5 与阶段/告警区分，World/Region/Node 从热区主干流到配方库存的层级，location/Agent/module 定位与目标/下一步保留；检查未知、推断根因不冒充事实，密集节点/边裁剪负载和键盘/CJK 可读性。保留来源对账、state、console、截图和负载证据。该方法未执行，当前 routes/HUD/热点不证明产业图谱或 semantic zoom 已实现。

### 当前 Agent Chat 与高级 Prompt 设置

#### 文本、输入与稳定性验收要求

- 控制/详情/事件/诊断等正文应可选择和复制；中英文均须可读，长文本不得截断主要动作或结果。输入区、drawer 与控件内的点击、拖拽、滚轮不得误触世界相机或拾取。
- Chat 必须支持中文组合输入；预编辑文字不得被当作已提交消息，compositionend/input 的浏览器时序差异不得重复插入或发送。切换 Agent、文本焦点和 drawer 时保持明确的草稿/焦点归属，文本编辑与组合态不得触发世界快捷键。
- 当前多行编辑和显式 Send Chat 边界不变：Enter/Shift+Enter 不触发发送。历史 Enter 发送只可从固定 Git 基线追溯。
- 输入和发送不得阻塞页面；失败显示原因及真实恢复下一步，pending 不冒充成功，恢复不得静默重放。当前 in-flight 拒绝和发送超时/显式重试有 main.test.jsx 与 viewer_agent_chat_auth_module.test.js 定向样例；它们不证明真实浏览器完整 IME 兼容。
- 上述为持续验收要求。Escape 的 isComposing/229 保护与测试只证明局部焦点隔离；完整 Chat IME 去重、焦点切换、CJK/复制、输入隔离和无卡死仍须专项 external headed desktop+narrow 证据，本文不新增已通过声明。


- Chat 和 Prompt 控制只对当前账号已绑定/权威认领且当前可控制的 Agent 开放；选中共享世界中的其他 Agent 不会授予控制权。无可控制 Agent 时，页面保持 blocked，并引导先认领 Agent 或等待 binding sync。
- `Agent Chat` 是面向当前 Agent 的消息入口。发送成功只表示对应请求结果，不会绕过 runtime 权威裁决，也不证明产生了世界效果。
- 当前 canonical Web 使用普通多行输入框和显式 `Send Chat` 动作；未定义 Enter 发送快捷键，Enter/Shift+Enter 按浏览器多行文本编辑处理。当前文档不声明中文 IME 组合态或自动聚焦已经获得跨浏览器专项验证。
- `Advanced Prompt Settings` 属于 operator-level 控制，默认收起，不与玩家主路径竞争。显式展开后，当前页面提供 system/short-term/long-term override、preview、apply、rollback 与最近 Prompt feedback。
- preview 不等于 apply；apply acceptance 不等于 runtime 已应用。以页面返回的实际 feedback 为准，失败或未授权必须保持 blocker/error，不能显示假成功。
- `hosted_public_join` 下的 Prompt apply 需要有效 `player_session` 与后端重新授权；当前入口使用 `Backend Approval Code` 满足 preview-grade `strong_auth`。缺失、过期或拒绝时应重新注册/认证或按页面提示恢复。
- 产品层的对话、草稿、默认/override 与反馈承诺见 [`Agent 对话与 Prompt 控制`](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md)；自动化 grammar 与状态字段见 [`viewer-web-semantic-test-api.prd.md`](viewer-web-semantic-test-api.prd.md)。

浏览器 BYOK 另沿[网页自带模型系统设计](../llm/self-supplied-model-connection.design.md)和[整体模型交互](viewer-page-module-design-2026-06-18.design.md#uiux-byok-control-flow)。当前源码将账号原绑定、任务、执行身份、模型检查与明确开始分开；浏览器表面为 DeepSeek 官方 `deepseek-flash`，OpenRouter 明确未支持。模型 Key 按账号保存到本站 localStorage，保存/检查不自动开始；Stop 保留 Key，Forget 删除对应本站 Key，均不取消已受理世界作业。该静态源码说明不代签部署、真实供应商或游戏验收；旧托管运行手册不能证明浏览器路径已通过。

<a id="prompt-source-clear-stale-validation"></a>
### Prompt 来源、清除和 stale 的独立验收

Trace refs: O7-REQ-395be972-59c0-4cd2-babc-f582ae659c5d；O7-REQ-a9797fdb-39f7-48fb-9202-9700d9d86159；O7-REQ-6f348a2d-cb87-4838-8447-21c8eebf6d07；O7-REQ-3bbcade1-3f73-4899-b324-f857fd27b245。方法对应[授权值投影](viewer-control-plane-split-live-playback.design.md#prompt-authorized-value-projection)、[五源交互](viewer-account-agent-uiux.design.md#prompt-source-clear-stale-consumer)与[输入/失败接收](viewer-account-agent-uiux.design.md#prompt-input-and-failure-consumer)，补充 REQ-PROMPT-003/006/008/010 的完整消费者，不改变上文当前支持状态。

**来源与前置。** 固定 source/build、平台/入口、真实 Runtime authority epoch、世界/Agent、当前获权 player/session/binding 及测试前 profile version/digest。使用正常配置和已注册 Builtin：三项已加载 config 分别为 `system-A`、`short-A`、`long-A`；B 是该固定 source 的 `runtime_live_phase1_short_term_goal()` 原文本，初始化时 short-term profile 为 None、driver 实际使用 B。B 与 `short-A` 不同，不能在 fixture 中偷换为同一值。C、D、P 分别使用不同的 `short-C`、`short-D`、`short-P` 代表已提交 override、未提交草稿与本地 preset。需要 C 的样例先经原合法 Apply 建立，再记录新的 baseline；各样例独立开始，不能把上一样例清除后的 runner 当初始化前态。

确定性层在现有 `prompt_control_enhanced_preview.rs`、`prompt_control_enhanced_race.rs` 和真实 Builtin/runner 接点增加以下断言；允许 test-only 受控 driver 暂缓消费，但 production reader/patch/鉴权/回包必须真实经过对应 adapter。独立 oracle 来自下表的固定前态与原 `sanitize_patch_string`/完整 profile 下发规则，不能调用被测投影函数生成 expected。配置/profile/runner 原件、版本、driver 调用、mutation、nonce/ledger、回包及 UI 各自取证；伪造一份 UI JSON 不证明读 producer。

实现接通后按本手册和 S6 启动真实受支持 Viewer，在桌面 1440×900 与窄屏 390×844、软键盘展开及长中英文文本下核最终消费者；实际支持 native/API Prompt 的组合另按原能力分别执行，未提供此能力不伪造完整支持。这里无需启动真实付费模型；以 read/preview/受控 runner 验证不能代签实际模型使用或真人可用性。

| 方法 | 操作及独立预期 |
| --- | --- |
| PV01 五源正常读取 | 先读新初始化实例：config 为三个 A，profile 三项 None，runner effective 为 `(system-A,B,long-A)`，B 标初始化来源。再合法 Apply `short-C` 并读回：authority profile 与 runner 匹配后 short-term effective 为 C；在实际支持 preset 的表面填 P 再编辑为 D，profile/effective 仍为 C，P/D 均标本地来源。只读和编辑阶段 model call、Prompt mutation、driver 更新均为 0；不把默认值填入 set |
| PV02 unchanged 与未修改默认 | 在上述合法基准上不改字段；确认三个 patch 均为 unchanged。显式原 Preview/无变化 Apply 可消耗其原合法认证/结果槽位，但 `accepted/no_change`、版本不变、mutation=0、driver 更新=0；普通读取本身不消耗这些写资源。未知/hidden 字段不能按 None 参与构造 |
| PV03 已有 override 的 clear | 在 profile short-term=C、driver=C 的前态明确 clear；提交前显示“C→未设置，成功消费后使用 short-A”。Preview 不 mutation；原 Apply 成功只有一项 profile changed、版本+1、mutation=1、完整 driver 更新一次；匹配读回后 effective=`short-A`，不是 B 或空串；scope 仍为 `runtime_instance/none/none` |
| PV04 空 profile 的 no-op | 回到新初始化前态 `(None,None,None)`、effective `(system-A,B,long-A)`，仅 clear short-term，再独立测试 `set(" \t\n ")`。原规范化后整个 profile 均无变化，故 accepted/no_change、版本不变、mutation=0、driver 更新=0，B 仍是 effective；不能显示“已恢复 short-A”。UI 的无需清除说明与 API 实际 no-op 分别核对 |
| PV05 完整 profile 的间接影响 | 同 PV04 前态，仅 set system 为 `system-D`，short/long unchanged。profile 差异只有 system，但完整候选 `(system-D,None,None)` 下发一次后 effective=`(system-D,short-A,long-A)`。预览必须同时说明 short-term 的 B→short-A，不能只根据 `applied_fields` 画一个变化。Apply 未消费时仍显示原 effective 与等待，读回后才确认这组三值 |
| PV06 合法 stale 与失败恢复 | 编辑 D 后由另一当前合法窗口把 C 改为 C2；用原 expected version 提交 D，应得到 stale、无该次 mutation。保留 D 和旧基准，当前授权读取 C2/effective 后可比较；模拟重读失败，旧值不得叫最新且 D 不丢失，显示真实再次读取/退出路径。重新编辑并明确采用新基准才形成新 request；刷新、比较和重载不得隐式 Apply，旧请求结果保持原身份 |
| PV07 失权优先与隐私 | 在 PV06 的变化同时使原 session/binding/control 失效；分别在读取开始前、runner 回包前和最终投递前发生。输出按原 control_lost/hidden，不能泄露 config、最新 profile/effective、digest 或差异；原安全草稿未提交且不能重发/转移。切账号/Agent 后注入迟到旧回包，当前页面/缓存/日志/新主体均不得出现旧私有正文；world-service 原 private goal redaction 不被新字段补露 |
| PV08 异步读回与旧响应 | 真实 authority Apply 已提交但受控 runner 尚未消费时，读出 authority 新 profile 和带时点的旧 effective/等待，不伪造 driver ACK。随后只消费一次完整更新并读回对应关联；再注入旧 read correlation、旧 runner/config 实例、乱序 Preview/Apply 响应，不能覆盖当前值/草稿或清新等待。其间 profile/权限变化时不拼混合“同切点”投影 |
| PV09 兼容、配置来源与 unsupported | 已初始化后只改外部配置文件，不重启：显示的默认仍为实际已加载 A。经原实例更换取得新 epoch/新配置后才读新值，旧 epoch 提交保持原 result_unknown，不自动重执行。旧客户端无新投影、runner 未初始化、ProviderBacked production 和缺合法私有 goal 读权分别显示准确边界；合法正常 Builtin 已实现的 producer 不得一律 unknown，Hosted local-mock 成功也不代签 production |
| PV10 有界读取和实际输入 | 用真实配置/profile 值使生产 reader 形成编码后 65,536 bytes/65,537 bytes 的完整合法候选投影，并核实际更小下层限制；后者明确不可用，不截断成完整值、不改 profile。限制同读取上下文至多一个 runner read 在途；超时及后续恢复不触发 Prompt/model/world 写或取消。桌面/窄屏分别核来源标签、完整三字段预告、Compare/Re-edit/Reload/Discard 与失败下一步可达，CJK/IME/换行不提交；文字规格或原 SVG 不代签实际浏览器结果 |
| PV11 输入、焦点与低高度 | 依原布局样本在 1280×720、390×844、844×390 及 200% 文本/软键盘状态，分别编辑 Chat 和 Prompt 长中英文。触发 compositionstart→组合输入→Enter/Escape 或 229→compositionend/input：只形成一次文本，无请求、无关闭；非组合态 Enter/Shift+Enter 只换行，显式主动作才发对应一次请求。Tab/Shift+Tab、展开/关闭、旋转、选择复制及区域滚动后核 Agent/草稿归属、动作/错误/恢复可达、原焦点返回；真正模态与非模态分别验，不让输入点击/滚轮穿透世界或 resize 自动提交。保留实际键盘/IME/浏览器及 state/console/截图/请求计数，不以 synthetic Escape 单测代签全部输入 |
| PV12 原失败与恢复再失败 | 分别用原无效 rollback target、认证/批准失败、ProviderBacked unsupported、result_unknown 及读取/容量失败触发真实拒绝；当前页面只能给接收设计规定且原 authority 确实提供的修正/认证/检查/读取/退出。再使该恢复失败，保留两次各自原因，不无限 pending、自动换请求或显示 applied。用受控 transport 连续两次 session_not_found 并替换 feedback/注入迟到响应，自动恢复至多一次且第二次终止；A rollback(999) 旧 error 到达 B Apply 期间不能改 B timer/结果。旧结果未知时业务重发为 0，即使最新 profile 可读也不能改旧结果为成功；现场不支持安全查询时明确停止，并保持当前有权的独立读取 |

每例记录配置/初始化/profile/driver 关联、读取与写请求身份、三字段前后值和原 authority 结果；记录只保留获权的脱敏测试材料。通过要求完整 producer、协议权限过滤、真实表面与独立 expected 一致。当前缺投影/driver 读取接线时保持该层未实施，缺实际 UI/环境时保留未验证；不会用本方法文字、旧 snapshot 或 profile-only 测试宣称整个 Prompt 已可用。

<a id="prompt-command-entry-validation"></a>
### 实际入口的 Chat/Prompt 接收方法

Trace refs: O7-REQ-14ddd9ba-2d94-48e8-b2e4-0dbae26ddc36；O7-REQ-a82f1ee8-9d70-442c-9972-5f49f6665293；O7-REQ-b6140b6c-ca2c-4734-84ce-7963f9385fff。分别验收 REQ-PROMPT-001/002/005，唯一消费者目录为[跨入口接收](viewer-control-plane-split-live-playback.design.md#prompt-command-entry-applicability)，并复用原六个 PromptControl 合同和上述 PV01–PV12。以下为目标验证方法，本轮未运行。

**固定对象与方法层。** 同一候选记录 build/入口/适配版本、endpoint/world、实际 backend、Hello 协商、当前 player/session/binding/authority、可调用 route、真实支持的操作与结果恢复子集。分别列 canonical Web、native Launcher 实际打开的 Web、确实已声明的 native 协议适配、TCP legacy/enhanced、HTTP/SDK 包装；未声明 native 编辑器不凭历史 egui 或服务端类名生成一个样本。当前 HTTP GET-only 与只提供签名类型/Chat 的 SDK 用无 Prompt route 的反例接收；不把该行记为整种 pure_api 不支持。其余声明采用但消费者缺接线的格如实 blocked/not_run，不删掉以换取全入口通过。

每例独立准备原合法 controller P 与可控 Agent A、只可观察不可控制的 Agent X，采用实际原授权/注册流程；有 enhanced 能力时固定 authority E、合法 session/binding 与 profile version 7，三字段/历史均由原合法变更建立。需要回滚的例子保存可解析 v6（short-term=`C0`）及当前 v7（`C1`），其余字段相同，保证回滚不是 no-op；本地 preset `P-text`（仅实际提供时）、草稿 `D-text`、Chat `M-text` 互异。不得用自造授予、测试页面缓存或无来源 JSON 替代权限与历史。只读、Chat 提交、Prompt Preview、Apply/Rollback、profile mutation 和原结果查询各自计数，expected 来自上述固定前态与原动作语义，不能调用被测显示函数生成。

先在实际 parser/签名/authority/回包和消费者接点用受控 transport、合法 Builtin/runner 做确定性检查；旧 CLI 不改协商就不能用 enhanced fixture 代签。真实支持的 Web/Native 表面再按 S6 记录实际输入和可见结果；非 GUI 消费者记录实际 argv/请求（凭据脱敏）及完整结构化输出。无需在本轮启动模型；受控接点与协议检查不证明真实 provider/真人或发行阶段。

| 方法与原义务 | 正常、失败与恢复的独立预期 |
| --- | --- |
| PV13 目标/当前控制（001） | 每个实际有 Chat/Prompt route 的组合，进入 A 时目标、当前主体和原 authority 的控制状态相符；本地选择 X、换 `--agent-id` 或仅看到 snapshot 不得到控制。发送前失去 binding/session/control 后，原签名请求由真实 authority 拒绝/blocked，Prompt mutation=0，私有当前值隐藏，消费者不能仍显示可操作成功；这里的故障请求用于验证 authority，不作为自动探测权限的产品动作。恢复只走当前真实认证/绑定同步/合法读取，恢复失败保持原因与受阻。在 A 有原在途请求时切到 X，再注入 A 的旧 ACK/error：归属 A 的原操作，不改 X 的内容、状态或等待；当前无此权限读面的入口显示待核/受阻，不能把本地参数标为已获权。native 打开 Web 只取得实际 Web 这一份证据，不能重复计作独立 native 编辑器通过 |
| PV14 Chat/preset/draft/profile（002） | 在实际有 preset/编辑器的表面选 `P-text`、改 `D-text`，以及非 GUI 客户端仅准备对应未发送参数/请求：Chat 与 Prompt 发送计数、profile mutation 均为0，v7/C1仍当前值，来源不能显示成已发送/applied。未提供 preset 的组合明确未提供，不制造按钮。显式 Send `M-text` 仅派发本次原 Chat 请求，其 ACK/intent receipt 按真实合同显示；PromptControl 发送计数仍为0，不将 M 自动复制为 profile，也不以“一次消息”否认原 canonical intent 后续合法目标效果。独立重置前态后显式 Preview D：无 profile mutation；合法不同值 Apply 才由原 authority 形成 v8、一次 mutation，实际 runner 消费另按 PV08 核。故意丢 Chat 或 Prompt 回包时，草稿/当前值与该原请求未知分开，重读/切换入口不自动再次 Send/Apply；失败后的编辑仍为草稿，新操作须当前有权主体明确决定，不得把旧未知静默改成成功 |
| PV15 能力/回执/scope（005） | 在有该能力的组合，Preview 后 authority 当前 profile 仍为 v7且 mutation=0（候选 ACK 可报预览 v8）；合法 Rollback v6 应追加当前 v8、short-term=C0、一次 mutation，并保留 `rolled_back_to_version=6`，不能把当前版本直接倒退为6。不存在 target 999 或失权则按原错误且无 mutation，Reload current 不计作 rollback。分别测 legacy 三命令真实可用子集、完整协商 enhanced、未协商却带增强字段、协商后缺 epoch/身份：后两者按原 capability/required-field 错误且无 mutation；legacy 原回包缺失的身份/scope不得补成增强证明。enhanced 的完整关联结果保持 `runtime_instance/none/none`；适配器缺完整关联时即使收到某类 ACK也不能结案。Apply 已提交但丢 ACK后，同 instance 且原身份/ledger仍有效时仅原 same-key精确重试，第二次 mutation=0；换 E/重启/缓存丢失时原结果未知，授权读当前 profile不改判，恢复再失败仍受阻且新 Apply=0。当前 GET-only/无 Prompt route 包装的 Prompt 写请求计数始终为0，输出真实不支持/只读边界与原已提供下一步，不拼造 preview/rollback、durable历史或跨入口同步 |

验收保留每个实际组合的目录行、原请求/回包/权限关联、独立计数、当前与历史版本及适用表面记录；记录和展示均遵守当前读权与隐私。PV13–PV15 全部适用分支通过才支持对应能力的消费者结论；明确未提供的能力只通过其边界/零副作用反例，不被计为正向业务能力。仅补协议/类型、只读 snapshot、旧 R-CMD 单一表面或本手册文字，均不代签其余实际入口；缺实现与缺验证分别列出。

### 已退役的 legacy 操作

历史 Standard-3D / egui 表面曾提供 auto select、右侧模块显隐及本地缓存、选中详情、Agent 快速定位、2D 全览图分层缩放和可复制文本面板；该表面及这些操作均已退役，不能作为当前 canonical `viewer` Web 页面的使用说明或能力声明。当前支持边界仍为实时观察、当前选中对象与 prompt/chat；`window.__AW_TEST__` 仅在显式 visual-test 构建且 opt-in 时可用，三条回归脚本的结果按各自构建与场景解释。

## 证据边界
- `doc/testing/evidence/software-safe-primary-web-entry-evidence-2026-04-07.md` 是固定历史样本；其中 2026-04-08 的 LLM-enabled follow-up PASS 仅代表原候选、环境和场景，不是今天默认生效的 formal gameplay 或 release-grade PASS。
- 当前结论先回到 [证据入口](../../testing/evidence/README.md) 与 [总测试手册](../../../testing-manual.md)：核对适用 source/build、实际测试树、环境、入口、场景、范围和 gate，再引用相匹配的真实结果。
  - 缺少当前适用证据时写 unknown；所选验证缺前置时写 blocked/inconclusive。这不表示当前玩法已失败，也不授权把旧证据改日期、换 hash 或重命名后作为新候选通过。
  - 历史样本的失败、输出、来源与原结论保持不变；页面观察、截图、S6 和自主玩法验收不能互相代签。
- `doc/world-runtime/evidence/formal-release-fixed-genesis-default-viewer-2026-05-16.md` 只证明 formal-release 技术启动状态在 `--no-llm` 观察链路下可加载、可连接、可选中实体。
  - 这份截图证据属于 observer/debug，不替代 formal gameplay 证明。

## Web 闭环

### 进程与 Launcher 边界

- `oasis7_viewer_live` 不启动或拥有 chain node、consensus gate、reward runtime、topology 或 execution-world directory。
- 独立 `oasis7_viewer_live --chain-status-bind <addr>` 必须同时传 `--chain-execution-world-dir <local-path>`；目录由受信 writer 管理。
- chain-linked formal/local evidence 使用 Launcher 管理的 `oasis7_chain_runtime`；Viewer 的 `--chain-status-bind` / `--chain-submit-bind` 只承担 client linkage。
- 退役的 embedded-node flags 必须失败并引导到 `oasis7_chain_runtime` / `oasis7_game_launcher`；编排、stale-world 与恢复合同见 `../launcher/game-client-launcher-runtime-session-continuity.prd.md`。
- `--no-llm` 仍只可作为 observer/debug，不是 formal gameplay evidence。

### 底层 Viewer Debug 人工闭环
终端 A：
```bash
env -u RUSTC_WRAPPER cargo run -p oasis7_viewer_runtime --bin oasis7_viewer_live -- llm_bootstrap --allow-debug-scenario --llm --bind 127.0.0.1:5023 --web-bind 127.0.0.1:5011
```

终端 B：
```bash
env -u NO_COLOR ./scripts/run-viewer-web.sh --address 127.0.0.1 --port 4173
```

默认终端 B 服务 release/UI 构建；即使 URL 带 `test_api=1`，也不得安装 `window.__AW_TEST__`。默认采证只检查页面、可见状态与 console/screenshot，不把缺少测试 API 当失败。

需要语义测试 API 时，另在独立终端构建并服务隔离产物（前提为 Viewer 的 Node/Rust/WASM 构建依赖已准备好）：
```bash
npm --prefix crates/oasis7_viewer run build:viewer:visual-test && \
python3 -m http.server 4174 --bind 127.0.0.1 --directory crates/oasis7_viewer/.viewer-test-dist
```
不要把 `.viewer-test-dist` 当 release 发布，也不要用 `run-viewer-web.sh` 代替这个测试产物服务。下方终端 C 默认 `release`；只在上述测试服务就绪后，于终端 C 显式设置 `export VIEWER_CAPTURE_MODE=visual-test`。`test_api=1` 是测试构建的额外 opt-in，不能把 release 转成测试构建；它也不是玩家权限或正式接入凭证。记录所选 profile、source/build 身份和 URL，不用测试 API 结果代签真实输入或正式发布。

终端 C：
```bash
set -euo pipefail
VIEWER_CAPTURE_MODE="${VIEWER_CAPTURE_MODE:-release}"
case "$VIEWER_CAPTURE_MODE" in
  release) VIEWER_CAPTURE_PORT=4173 ;;
  visual-test) VIEWER_CAPTURE_PORT=4174 ;;
  *) echo "unknown VIEWER_CAPTURE_MODE: $VIEWER_CAPTURE_MODE" >&2; exit 2 ;;
esac
command -v agent-browser >/dev/null || { echo "missing agent-browser" >&2; exit 1; }
agent-browser --version
agent-browser doctor --offline --quick --json
AB_SESSION="$(agent-browser session id --scope worktree --prefix viewer-manual)"
export AB_SESSION
mkdir -p output/playwright/viewer
VIEWER_EVIDENCE_DIR="$(mktemp -d output/playwright/viewer/viewer-manual.XXXXXX)"
cleanup() {
  local main_status=$?
  local close_status=0
  trap - EXIT INT TERM
  if agent-browser --session "$AB_SESSION" close >"$VIEWER_EVIDENCE_DIR/close.log" 2>&1; then
    close_status=0
  else
    close_status=$?
  fi
  if (( close_status != 0 )); then
    printf 'cleanup blocker: close exited %s; see %s/close.log\n' "$close_status" "$VIEWER_EVIDENCE_DIR" >&2
  fi
  if ! printf 'main_status=%s\nclose_status=%s\n' "$main_status" "$close_status" >"$VIEWER_EVIDENCE_DIR/cleanup.status"; then
    printf 'could not write cleanup status\n' >&2
  fi
  if (( main_status != 0 )); then
    exit "$main_status"
  fi
  exit "$close_status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
agent-browser --session "$AB_SESSION" --headed open "http://127.0.0.1:${VIEWER_CAPTURE_PORT}/viewer.html?ws=ws://127.0.0.1:5011&render_mode=viewer&test_api=1"
agent-browser --session "$AB_SESSION" wait --load domcontentloaded
if [[ "$VIEWER_CAPTURE_MODE" == "visual-test" ]]; then
  agent-browser --session "$AB_SESSION" wait --fn "typeof window.__AW_TEST__ === 'object'"
else
  agent-browser --session "$AB_SESSION" wait --fn "typeof window.__AW_TEST__ === 'undefined'"
fi
agent-browser --session "$AB_SESSION" session info --json | tee "$VIEWER_EVIDENCE_DIR/session-info.json"
agent-browser --session "$AB_SESSION" snapshot -i
if [[ "$VIEWER_CAPTURE_MODE" == "visual-test" ]]; then
  agent-browser --session "$AB_SESSION" eval "JSON.stringify(window.__AW_TEST__.getState())" | tee "$VIEWER_EVIDENCE_DIR/state.json"
fi
agent-browser --session "$AB_SESSION" console | tee "$VIEWER_EVIDENCE_DIR/console.log"
agent-browser --session "$AB_SESSION" screenshot "$VIEWER_EVIDENCE_DIR/viewer-web.png"
```

本轮主动作失败时保留其退出码；主动作成功而 close 失败时返回 close 退出码。`close.log` 和
`cleanup.status` 独立保留收尾诊断，不能用 `|| true` 或丢弃输出把未关闭记录成成功。

### 推荐自动化脚本
- 主入口 contract：
```bash
./scripts/viewer-primary-web-entry-regression.sh --headed
```
- 实时玩法推进 / blocker 观测：
```bash
./scripts/viewer-software-safe-step-regression.sh --headed
```
- prompt/chat 回归：
```bash
./scripts/viewer-software-safe-chat-regression.sh --headed
```

## 最小通过标准

<a id="viewer-infrastructure-capture-receiver"></a>
### Infrastructure capture receiver

基础设施 claim 使用 [Viewer readback contract](viewer-control-plane-split-live-playback.prd.md#viewer-infrastructure-readback-contract)、[QA receiver](../../testing/prd.md#qa-infrastructure-receiver) 与 [Harness evidence join](../llm/continuous-agent-harness.design.md#harness-infrastructure-evidence-join)。先冻结 candidate、world/root/revision/child/direct-parent receipt、实际 source freshness、provider 四字段与 evidence window，再按 [Playwright capture method](../../testing/manual/web-ui-playwright-closure-manual.design.md#playwright-source-capture-method) 捕获 before/outage/fresh/recovery/retry/replay 的可见 action/result、截图、console/state；以同候选只读 authority source/journal/API oracle 核对 disposition、no-effect、history/W 与 effect count，归档 artifact identity/hash 和差异。不能用 ACK、stale/debug 样本或 surface connection 替代 authoritative result。

以下页面最低通过只证明 surface observation。required/full 由 [QA tier boundaries](../../testing/prd.md#qa-infrastructure-tier-boundaries) 和 [S6 receiver](../../../testing-manual.md#testing-s6-qa-receiver) 判定：required 包含 active-provider pure API，full 另需真实 provider-backed Agent parity 与实际 external headed desktop+narrow evidence。缺失 mandatory identity、phase、freshness、oracle join 或 artifact 保持 blocked/unverified；实际语义违反为 failed。PWT planned scenes 和 equal-version `1→1` semantic negative 保持 planned/unexecuted；历史证据、当前 technical-preview/GPU 限制与 Launcher/login/auth 合同保留，本段不升级任何现有状态。

- release/UI：页面可加载，`window.__AW_TEST__` 不存在（包括带 `test_api=1` 时）；检查页面可见的连接状态或可追溯 blocker，至少保留截图与 console。页面加载不等于 renderer-ready 或玩法通过。
- visual-test：显式测试构建且 opt-in 后 `window.__AW_TEST__` 可用，另采 `getState()`；`renderMode=viewer`（兼容 alias 可回出 `software_safe`），`connectionStatus=connected` 或有明确 blocker。测试 API 可用不替代真实输入、图形或正式玩法验收。

## 常用调试点
以下仅适用于显式 visual-test 构建且已 opt-in 的测试会话；控制/Prompt/chat 调用仍遵守当前账号权限与确认要求。
- `window.__AW_TEST__.getState()`
- `window.__AW_TEST__.sendControl("step")`
- `window.__AW_TEST__.sendPromptControl("preview", { agentId: "agent-0", shortTermGoal: "test" })`
- `window.__AW_TEST__.sendPromptControl("apply", { agentId: "agent-0", shortTermGoal: "test" })`
- `window.__AW_TEST__.sendAgentChat("agent-0", "hello from viewer")`

## 常见问题排查
- 页面空白：确认 `run-viewer-web.sh` 已完成构建并监听目标端口。
- 连接失败：确认 `oasis7_viewer_live` 已启动，且 `ws=` 参数与 `--web-bind` 一致。
- 无法进入正式玩法：检查 LLM provider 配置；若显式 `--no-llm`，只允许 observer/debug。
- `agent-browser` 失败：先运行 `agent-browser --version`、`agent-browser doctor --offline --quick --json` 与 `agent-browser --session <owned-session> session info --json`；若是 daemon/IPC 读取失败，只重建本轮 session，不重放非幂等动作，也不影响其他任务 session。
- 有状态但不推进：优先跑 `viewer-software-safe-step-regression.sh`，确认是正常推进还是显式 blocker。
- 显式 visual-test 构建且 `test_api=1` 下停在 `connecting` 且 `logicalTime=0`：立即检查 `getState().lastError` / `errorCount` 并归档 state、console、screenshot。已知 WebGL/SwiftShader fatal 最多自动 reload 一次；再次失败是 S6 环境/图形 blocker，不得当作 gameplay 证据。只有确认 Web 图形阻塞后，native 才可作为诊断 fallback。
- 如果只看到 `--no-llm` 截图证据：不要把它当成 formal gameplay PASS；按本手册[证据边界](#证据边界)查找与实际候选、环境和场景匹配的 LLM-enabled 结果。Apr07/08 follow-up 仅为历史示例，不能替代当前结论。

## 已移除能力
- 退役的第二 Viewer 启动路径
- 退役的辅助可视化检视 surface
- 退役的视觉资产、抓帧与专项检视工具链

## 参考文档
- `testing-manual.md`
- `doc/testing/manual/web-ui-agent-browser-closure-manual.manual.md`
- `doc/world-simulator/viewer/viewer-web-entry-compatibility.prd.md`
- `doc/testing/evidence/software-safe-primary-web-entry-evidence-2026-04-07.md`
- `doc/world-runtime/evidence/formal-release-fixed-genesis-default-viewer-2026-05-16.md`

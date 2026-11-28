# oasis7：Web UI agent-browser 闭环测试操作手册

审计轮次: 11

## 文档定位
- 本文件是 Web UI `agent-browser` 闭环的 canonical `*.manual.md` 操作手册。
- 需求边界、成功标准与决策记录仍以 `doc/testing/manual/web-ui-agent-browser-closure-manual.prd.md` 为准。
- 任务与历史执行状态以 PR、实际 CI 与评审记录（Issue 按需） 为准。

## 适用范围
- 适用于 `oasis7_viewer_live` + `software_safe` Viewer Web 页面闭环。
- 不适用于 `oasis7_web_launcher` / launcher Web 控制面产品动作；后者默认先走 GUI Agent，再用页面校验状态与字段。
- 本手册只覆盖当前仍存在的 Web 链路，不再覆盖历史第二 Viewer 或退役视觉专项工具。
- 若目标是纯本地真实 provider-backed 游戏试玩或复现 `agent_chat`，先启动通用本地 `oasis7_provider_local_bridge` 与 launcher/runtime/game/local-standalone-chain 栈；下方 live server + `run-viewer-web.sh` 步骤只作为 Viewer/debug 闭环。该入口不连接 formal/public testnet；纯本地测试不等同于“本地启动 test 环境”。

## 前置条件
- 已安装并验证当前仓库基线 `agent-browser 0.37.1`（`agent-browser --version`）
- 已安装 Node.js / npm
- 已安装 `python3`
- 首次安装或浏览器二进制异常时执行 `agent-browser install`；daemon/session 异常优先执行 `agent-browser doctor --offline --quick --json`
- 每轮必须使用 worktree-scoped named session；禁止使用默认 session 或影响其他任务的全局清理

### agent-browser 会话生命周期约定

- 运行前使用 `agent-browser session id --scope worktree --prefix <purpose>` 生成本轮唯一 session；同时保留 `session info --json` 作为 daemon/浏览器诊断证据。需要核对并行任务时使用 `agent-browser session list --json`，不得猜测或复用其他任务的 session。
- 所有命令显式传 `--session "$AB_SESSION"`，不要依赖 unnamed/default session。并行 actor 使用不同 session；共享 CDP 时额外使用一次性 `--pin-tab`，避免 tab fallback。
- `open` 后先使用 `wait --load domcontentloaded`，再使用 `wait --fn` 或 `wait --text` 等应用就绪信号；长连接 Viewer 不把 `networkidle` 作为唯一就绪条件。
- 退出路径必须通过 `trap` 只关闭本轮拥有的 `$AB_SESSION`。禁止全局关闭其他任务 session；回收失败必须保留诊断并标为 blocker。
- 需要持久登录时使用 `--restore --restore-save auto`，并用 `--restore-check-url`、`--restore-check-text` 或 `--restore-check-fn` 校验恢复结果；一次性验收默认不启用 restore。

## 纯本地真实 LLM 游戏测试入口

- 若需本地真实 provider-backed gameplay 栈，启动通用本地 `oasis7_provider_local_bridge`（`127.0.0.1:5841`）配合 `scripts/run-producer-playtest.sh` 或 `scripts/worktree-harness.sh`。
- 需要对已经启动的页面做 agent-browser 留证时，优先复用输出的 `GAME_URL`，再执行本手册的采样步骤。
- 不要把单独的 `run-viewer-web.sh` 当作本地真实 provider-backed gameplay 启动方式；它不负责 provider bridge 或 launcher bootstrap。
- 若目标是本地启动 test 环境并证明本机入口接入 formal `public_testnet` 大世界，先按对应网络接入 runbook 启动并复核，再用本手册做页面采样。

### W3：DevLocal Builtin LLM 免邮箱启动

W3 的本地验收需要真实 WASM 与 build-suite metadata、DevLocal authority、Builtin LLM
和 HostedPublicJoin 的 loopback session projection。它不使用 ProviderBacked authority，
也不连接 formal/public testnet。先确认本 worktree 构建出的 launcher、viewer、chain
runtime 和 probe 来自同一个 HEAD，并确认真实 artifact 存在：

```bash
ROOT_DIR="$(pwd)"
LOCAL_PROVIDER_DIR="$ROOT_DIR/.tmp/wasm-build-suite/local-test-provider"
test -f "$LOCAL_PROVIDER_DIR/module.runtime.local-test-provider.wasm"
test -f "$LOCAL_PROVIDER_DIR/module.runtime.local-test-provider.metadata.json"
env -u RUSTC_WRAPPER cargo build -p oasis7_launcher_backend -p oasis7_provider_tools -p oasis7_viewer_runtime -p oasis7_chain_runtime \
  --bin oasis7_llm_provider_probe \
  --bin oasis7_game_launcher \
  --bin oasis7_viewer_live \
  --bin oasis7_chain_runtime
```

为每次运行选择新的输出目录和空闲端口；下面端口只是示例。显式开启 loopback 的免邮箱
test-login，并将四个 local authority 参数一起传给 `run-launcher-stack.sh`：

```bash
RUN_ID="w3-local-builtin-$(date +%Y%m%d-%H%M%S)"
OUTPUT_DIR="$ROOT_DIR/.pm/scratch/w3-real-qa/$RUN_ID"
mkdir -p "$OUTPUT_DIR"
export OASIS7_HOSTED_TEST_LOGIN_ENABLED=1
export OASIS7_LOCAL_TEST_PROVIDER_SESSION_MODE=hosted_public_join
test -n "${OASIS7_HOSTED_STRONG_AUTH_PUBLIC_KEY:-}"
test -n "${OASIS7_HOSTED_STRONG_AUTH_PRIVATE_KEY:-}"
test -n "${OASIS7_HOSTED_STRONG_AUTH_APPROVAL_CODE:-}"

./scripts/run-launcher-stack.sh \
  --run-id "$RUN_ID" \
  --output-dir "$OUTPUT_DIR" \
  --viewer-host 127.0.0.1 \
  --viewer-port 4289 \
  --live-bind 127.0.0.1:5189 \
  --web-bind 127.0.0.1:5289 \
  --deployment-mode trusted_local_only \
  --allow-trusted-local-playtest \
  --major-world-event-visibility restricted \
  --chain-enable \
  --chain-local-standalone-test \
  --agent-decision-source builtin_llm \
  --skip-llm-provider-preflight \
  --local-test-provider-authority "$OUTPUT_DIR/local-test-provider-authority.json" \
  --local-test-provider-wasm "$LOCAL_PROVIDER_DIR/module.runtime.local-test-provider.wasm" \
  --local-test-provider-metadata "$LOCAL_PROVIDER_DIR/module.runtime.local-test-provider.metadata.json" \
  --local-test-provider-agent-id starter-agent-0 \
  --local-test-provider-owner-binding local-test-owner-0 \
  --local-test-provider-finality-block-hash "blake3:0000000000000000000000000000000000000000000000000000000000000000" \
  --local-test-provider-session-mode hosted_public_join \
  --with-llm \
  --auto-play \
  --json-ready
```

The launcher defaults its chain storage profile to `dev_local`; direct
`oasis7_game_launcher` invocations must add `--chain-storage-profile dev_local` explicitly.
The wrapper creates run-scoped file account/session/replay ledgers and a local issuer key in
the process environment; do not replace those with values copied into a command, report, or
task log. Keep `--agent-decision-source builtin_llm`: the local authority opt-in is rejected
for `provider_backed`. The DevLocal provider command grant has a finite 300-tick lifetime.
Startup reuses it only when at least 240 ticks remain; otherwise startup issues one fresh
canonical grant through Runtime admission and atomically persists the new authority/context.
The prior grant remains immutable. This is a startup-only local lifecycle: HostedPublicJoin
login, browser refresh, Prompt strong-auth, and periodic timers do not renew it, and it grants
no clock/time capability.

Check the output `session.meta` for `STACK_READY=1`, then verify the actual chain process and
the email-free issuer route:

```bash
CHAIN_STATUS_BIND="127.0.0.1:5399"
curl -sS "http://$CHAIN_STATUS_BIND/v1/chain/status" | jq '{ok, readiness: .readiness.status, h: .consensus.committed_height, nh: .consensus.network_committed_height, runtime_last_error}'
curl -sS -X POST "http://127.0.0.1:4289/api/public/hosted-account/test-login" \
  -H 'Content-Type: application/json' \
  -d '{"public_key":"4848484848484848484848484848484848484848484848484848484848484848"}' \
  | jq '{ok, player_id, has_registration_grant: (.registration_grant != null)}'
```

The chain process smoke is green only when status is `ok=true`, readiness is `ready`, the
committed height advances to at least 3, and the persisted execution snapshot time matches
that height. The local authority setup leaves the persisted world at baseline height 2, so
the first real proposal is height 3; do not synthesize a height-2 record or reset the world
clock. A fresh DevLocal setup also applies the canonical 325 OC genesis and immediate vesting
claim for `starter-agent-0` at that baseline without advancing the clock; reuse validates the
immutable funding journal and does not refill a spent balance. A subsequent restart should
restore height 3 and advance to height 4. This process
smoke is separate from the focused Rust driver regression and is required before browser
claims for ordinary starter completion.

Before browser interaction, read the execution-world snapshot and record only safe structural
fields: current `state.time`; active local provider module/version; matching identity, owner
binding, and generation; a matching grant with `state.time < expires_at_tick`; invocation
context grant identity equality; available cognition allowance; and matching world/branch/
finality audience. If the grant is expired or below the 240-tick startup margin, discard the
generated run outputs and start a fresh run-scoped root. Do not edit snapshots, authority JSON,
signatures, or expiry values.

## 底层 Viewer Debug 闭环

### 1. 启动 live server
```bash
env -u RUSTC_WRAPPER cargo run -p oasis7_viewer_runtime --bin oasis7_viewer_live -- llm_bootstrap --allow-debug-scenario --llm --bind 127.0.0.1:5023 --web-bind 127.0.0.1:5011
```

无 LLM 场景：
```bash
env -u RUSTC_WRAPPER cargo run -p oasis7_viewer_runtime --bin oasis7_viewer_live -- llm_bootstrap --allow-debug-scenario --no-llm --bind 127.0.0.1:5023 --web-bind 127.0.0.1:5011
```

### 2. 启动 Web Viewer
```bash
env -u NO_COLOR ./scripts/run-viewer-web.sh --address 127.0.0.1 --port 4173
```

### 3. 打开页面并采样
```bash
set -euo pipefail
command -v agent-browser >/dev/null || { echo "missing agent-browser" >&2; exit 1; }
agent-browser --version
agent-browser doctor --offline --quick --json
AB_SESSION="$(agent-browser session id --scope worktree --prefix viewer-manual)"
export AB_SESSION
mkdir -p output/playwright/viewer
VIEWER_EVIDENCE_DIR="$(mktemp -d "output/playwright/viewer/${AB_SESSION}.XXXXXX")"
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
    printf 'cleanup blocker: close_status=%s; see %s/close.log\n' "$close_status" "$VIEWER_EVIDENCE_DIR" >&2
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
agent-browser --session "$AB_SESSION" --headed open "http://127.0.0.1:4173/?ws=ws://127.0.0.1:5011&render_mode=viewer&test_api=1"
agent-browser --session "$AB_SESSION" wait --load domcontentloaded
agent-browser --session "$AB_SESSION" wait --fn "typeof window.__AW_TEST__ === 'object'"
agent-browser --session "$AB_SESSION" session info --json | tee "$VIEWER_EVIDENCE_DIR/session-info.json"
agent-browser --session "$AB_SESSION" snapshot -i
agent-browser --session "$AB_SESSION" eval "JSON.stringify(window.__AW_TEST__?.getState?.() ?? null)" | tee "$VIEWER_EVIDENCE_DIR/state.json"
agent-browser --session "$AB_SESSION" console | tee "$VIEWER_EVIDENCE_DIR/console.log"
agent-browser --session "$AB_SESSION" screenshot "$VIEWER_EVIDENCE_DIR/viewer-web.png"
```

本次采样失败保留原退出码；采样成功而 close 失败则返回 close 的非零码。`close.log` 与 `cleanup.status` 保存主动作和收尾诊断，close 失败标为 cleanup blocker；非零只证明关闭命令失败，不能单凭它断言进程仍活着。只关闭本次 session，不全局清理或无界重试。

## 推荐回归脚本
- 主入口 contract：
```bash
./scripts/viewer-primary-web-entry-regression.sh --headed
```
- 实时推进 / blocker：
```bash
./scripts/viewer-software-safe-step-regression.sh --headed
```
- prompt/chat：
```bash
./scripts/viewer-software-safe-chat-regression.sh --headed
```

## 最小通过标准

本节只描述本次选定 surface 的最小采样。按 [S6 renderer/目的分流](../../../testing-manual.md#testing-software-renderer-scope)记录实际 renderer、renderMode 与验证目标；软件 viewer/compat DOM 通过不代签 Bevy/WASM/GPU、完整视觉、producer 或发布可玩性。真实图形 fatal 仍阻断受影响范围。

- `snapshot -i` 可见交互树，主视区正常加载。
- `state.json` 中 `renderMode=viewer`（compat 专项可接受 `software_safe` alias）。
- `connectionStatus=connected`，或页面显式返回可追溯 blocker。
- 至少产出 1 张截图、1 份 console 日志、1 份状态快照。

### QA infrastructure receiver evidence boundary

若本次采样用于状态提交、恢复、world scope 或 consumer parity 的 QA 证据，先按 [QA receiver source/scene method](../prd.md#qa-infrastructure-source-scene-method) 固定候选、world/branch/window、access mode 与 execution lane，再使用可见页面动作；`__AW_TEST__` 只可读取 readiness、当前状态、history 和诊断，不得代替玩家输入或权威 API/journal oracle。按 before/action/pending-or-outage/fresh-recovery/final 顺序保存截图、页面 state、console/request error 与可读的同候选 API/Agent authority refs，并校验每份 artifact 的路径、字节数和 digest。

provider-backed 样本另外记录 provider kind、model、endpoint route 和 preflight evidence/provenance（不写 secret）。Viewer 截图或文本相似不能独自证明状态相同；缺少候选/窗口/权威 join 时标 `blocked`/`unverified`，观察到字段冲突或重复效果时保留 exact failure signature。当前底层 Viewer Debug 和 mock/plumbing 样本仍只用于相应诊断，不提升到 provider、S6 full 或 release proof。桌面+narrow 外部 headed 和 provider-backed Agent parity 的 full 要求由 S6/QA root 界定。

## software_safe 专项
- 若只做 formal gameplay / summary 路径，优先跑 `viewer-software-safe-step-regression.sh`。
- 若验证 prompt/chat/rollback，优先跑 `viewer-software-safe-chat-regression.sh`。
- 若需要稳定观测一条标准 `AgentSpoke`，可在 runtime 启动前设置：
```bash
OASIS7_RUNTIME_AGENT_CHAT_ECHO=1
```

## launcher 控制面边界
- `oasis7_web_launcher` 的产品动作默认走 GUI Agent 接口：
  - `/api/gui-agent/capabilities`
  - `/api/gui-agent/state`
  - `/api/gui-agent/action`
- `agent-browser` 在 launcher 场景里只用于页面加载、字段核对和截图留证。

## Fail-Fast
- F1 `ERR_CONNECTION_REFUSED`：先检查 4173/5011 监听与主页可达。
- F2 页面初始化崩溃：立即归档证据并判失败。
- F3 长时间无推进：优先使用 `viewer-software-safe-step-regression.sh` 判断是正常 blocker 还是异常卡死。
- F4 URL 被 shell 截断：带 `&` 的 URL 一律加引号。
- F5 `connecting` 且 `logicalTime=0`：读取 `window.__AW_TEST__.getState().lastError` / `errorCount`，归档 screenshot、console 与 state；WebGL/SwiftShader fatal 按 S6 环境/图形失败分类，已知 fatal 最多允许一次自动 reload，不能当作 gameplay 进展。
- F6 `agent-browser` daemon/IPC 读取失败（如 `Resource temporarily unavailable`）：保留 `session info --json` 与命令日志，确认本轮 session 仍存在后只重建本轮 session；不得重放非幂等 UI 动作，也不得全局关闭其他 session。

## 发布与延伸入口
- 系统总手册：`testing-manual.md`
- Viewer 操作手册：`doc/world-simulator/viewer/viewer-manual.manual.md`
- 发布前人工体验清单：`doc/testing/launcher/launcher-manual-test-checklist-2026-03-10.prd.md`

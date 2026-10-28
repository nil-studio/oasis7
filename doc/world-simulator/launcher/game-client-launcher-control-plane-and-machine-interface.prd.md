# 游戏客户端启动器控制面与机器接口

- 对应设计文档: `doc/world-simulator/launcher/game-client-launcher-control-plane-and-machine-interface.design.md`
- 历史迁移、验证与 task 状态：GitHub task issue evidence。

## 目标与权威边界

本专题是 `oasis7_web_launcher` 的稳定 launcher control-plane authority，收敛已完成的 native/Web UI 同层复用、共享 UI schema、静态资源服务、控制台、控制面以及 GUI-agent 机器接口历史专题。它描述 launcher 如何呈现和受控地调用既有进程控制能力；不定义 runtime 世界状态演化、链执行规则或 Agent 决策语义。

native 与 Web 客户端消费同一份状态和动作合同。Web 入口可托管 launcher 静态资源，控制面提供状态读取及游戏、链相关的既有受控操作；实际可用状态、配置约束和错误均以当前响应为准。

## 范围

覆盖 launcher 的共享表现、控制状态和 operator HTTP-JSON 适配面；不覆盖 runtime 世界规则、执行状态演化、transfer 可用性或 Agent 决策。

## 人员与使用场景

- 桌面用户和 operator：通过 native 或 Web launcher 观察相同的控制状态，并使用相同语义的受控启动、停止和诊断入口。
- 自动化 operator：通过 HTTP JSON 机器接口发现当前能力、读取状态、提交一个已声明动作并读取统一结果；它不是世界内 autonomous Agent，也不是 `DecisionProvider`。
- launcher 开发者：维护共享 UI schema、客户端表现和控制面映射，不复制进程编排或 runtime 规则。

## 接口 / 数据

### 客户端与控制面

<a id="launcher-state"></a>

- `GET /api/state` 是 launcher 控制状态读取入口；native/Web 均以服务端快照而非客户端推断来展示状态。
- 既有控制路由包含 `POST /api/start`、`POST /api/stop`、`POST /api/chain/start` 和 `POST /api/chain/stop`；它们的可执行性、参数和错误以运行中服务的当前合同为准。
<a id="launcher-static-api"></a>

- launcher 静态资源和 API 可并存；静态目录缺失或资源不可用不得改变既有 API 路由语义。
<a id="launcher-schema"></a>

- 共享 UI schema 负责字段分组、文案和可见性的一致映射。它是表现层合同，不是 runtime 配置或世界规则的第二真源。

### 受控停止与 Web 诊断

- `POST /api/stop` 的 launcher 表现必须将用户请求、当前结果和随后状态快照如实呈现；它不是对 chain、所有子进程或 runtime 最终清理完成的承诺。native launcher 对其直接管理的子进程先请求优雅停止，在有界等待后才使用终止回退；用户选择停止和关闭窗口走同一停止生命周期，失败必须保留可诊断结果。
<a id="launcher-result"></a>

- Web 表面不得因初始化、浏览器环境、状态读取或控制面请求失败而把 launcher 显示为 ready。它应保留可见的 blocked/error 诊断及可恢复下一步，并继续以服务端状态快照为准；这条表现边界不声明浏览器 clock、轮询、WASM lifecycle 或 runtime session 的具体实现。
- chain runtime、execution-world 输出、stale-session/recovery，以及浏览器/WASM 的运行时兼容细节，统一转至 `game-client-launcher-runtime-session-continuity.prd.md` 的 runtime 专业 authority；本控制面只保留请求、结果和状态的呈现合同。

<a id="launcher-action-lifecycle"></a>
### 控制动作的终态、异常与启动输入

本节细化 [#4406](https://github.com/eng-cc/oasis7/issues/4406) L1–L4、L6–L8，沿用既有
operator 路由与同步控制结果；用户启动/停止的本地结果不得由传输时长或旧快照猜测。

<a id="launcher-l1"></a>

- 停止请求的客户端预算必须覆盖服务端当前支持的完整有界生命周期，包括优雅等待和必要的
  终止/等待回退。约 2 秒退出及约 4 秒优雅超时后回退不应在 1.5 秒被误报 QueryFailed。
  尚无终态时保持停止中/结果未知；只有实际 stop/wait 确认才显示已停止。真无响应仍有界。
<a id="launcher-l2"></a>

- 控制服务的正常退出与不可恢复 accept 错误都须进入同一托管子进程 stop/wait 收尾路径；
  连接上限或日志不能代替收尾。可恢复 accept 错误可有界退避，不能无限忙转；清理失败须与
  原始错误一并保留，不宣称进程/端口已释放。
<a id="launcher-l3"></a>

- Start 的拒绝不替换仍在运行的 child/PID/配置，后续 Stop 仍能管理原进程；该先检查、后变更
  的既有约束由[会话重新验证设计](game-client-launcher-runtime-session-continuity.design.md#session-revalidation)
  拥有。GUI-agent 与普通控制路由消费同一结果，不因新请求 disabled/hosted/invalid-mode 丢失句柄。
<a id="launcher-l4"></a>

- `/api/start`、`/api/chain/start` 对坏 JSON、字段类型错误与未知 tier 返回完整可解码的
  `ApiResponse`：`ok=false`、`error_code=invalid_request`、安全的具体原因及当前 `state`；
  不 spawn、不修改原状态。不强制某个 HTTP 状态码；使用非 2xx 时客户端仍须读取结构化诊断。
<a id="launcher-l6"></a>

- 静态目录优先级按输入来源判断：显式 `--viewer-static-dir` 高于环境默认，即使值恰好是
  `web`；省略 CLI 才消费非空环境默认和既有 fallback。显式有效目录不被错误环境路径阻断。
  Windows wrapper 保留同一参数来源语义。
<a id="launcher-l7"></a>

- provider-backed 默认启动的 preflight 必须验证现有健康响应的必需字段和语义。
  HTTP 200 但 `ok=false`/degraded、缺失或错类型的必需字段都不能写成功产物或继续启动。
  显式 `--skip-llm-provider-preflight` 保留既有受控调试/负向用途且不产出健康通过结论；
  HTTP 存活、provider 健康与真实 decision/tool-call 各是不同结论。
<a id="launcher-l8"></a>

- headed producer 与子 Launcher stack 对 `--meta-file` 使用同一规范化路径；相对、绝对、
  含空格及重复参数有唯一解析结果。若入口不支持覆盖，须在启动 child 前拒绝，不能等错文件
  直到超时。非 headed 直接 exec 与默认入口保留。

验收按[配对动作生命周期设计](game-client-launcher-control-plane-and-machine-interface.design.md#launcher-action-lifecycle-design)
分别使用真实可控 child、完整 HTTP、离线参数/健康响应 fixture；不要求真实付费 provider。
Windows 活跃子进程停止须独立验证，Unix/mock/no-op 不代签。

### 机器接口

<a id="launcher-capability"></a>

- `GET /api/gui-agent/capabilities` 是机器客户端的 capability discovery 入口；响应中的版本、`actions[]` 与 `query_targets` 是当前动作清单和查询目标的真值，文档不维护永久且穷尽的动作清单。payload 约束不由该响应发现，而是在提交 action 时按当前实现校验。
- `GET /api/gui-agent/state` 提供与 `/api/state` 对齐的机器可读状态别名。
<a id="launcher-machine-action"></a>

- `POST /api/gui-agent/action` 接收严格的枚举 action 与 JSON payload；不支持的 action、缺失/非法 payload 和被当前状态拒绝的请求必须返回可分类结果。
- 动作响应保持统一的 `ok`、`action`、可选 `error_code`/`error`/`data` 以及最新 `state` 结构，便于调用方不解析 Web UI 即可继续诊断或恢复。
<a id="launcher-query"></a>

- 查询代理只可经服务端已声明的目标和参数映射；机器接口不提供任意 URL 或任意命令穿透。

<a id="launcher-security"></a>

## 部署与安全非主张

`gui-agent` 是 operator-plane HTTP JSON 接口，不是玩家世界操作面，也不授予 runtime 直接写入权。`hosted_public_join` 对已枚举的 operator 路由使用 peer-IP gate；该事实不表示所有 API 都是私有的、服务仅绑定 loopback，或已经具备认证、RBAC、TLS、rate limit、公开可用性或任何 network/readiness/SLA 结论。wildcard bind 和公开路由仍按各自当前实现及部署配置处理。

本专题不承诺 transfer 可用性、永久动作覆盖，或将 GUI-agent 与 `DecisionProvider`、自治 Agent 等同。涉及 runtime 执行、身份、安全部署或世界状态语义的变更，必须回到相应专业 authority 和当前任务证据。

## 里程碑

- 已完成：共享 schema、Web 静态资源与 control-plane 并存、机器能力发现/状态/统一 action 合同。
- 维护：后续实现或安全边界变更以新任务、代码和当前 capability discovery 响应重新验证。

## 风险

- 动作清单、查询目标和 action payload 约束可能随实现演进；调用方必须发现当前能力并处理执行期校验结果，不得把本文档当成永久 action catalog 或 payload schema。
- operator-plane 路由边界不能替代 runtime 权威、安全部署或公开 readiness 的专业结论。
- 停止请求被接受、Web 未崩溃或诊断可见，都不等于 runtime 已收敛、所有进程已退出，或浏览器/WASM 环境已经具备通用兼容性。

## 验收与追溯

- 文档入口与链接：`./scripts/doc-governance-check.sh`、`./scripts/readme-link-check.sh`。
- 机器接口合同：`oasis7_web_launcher` 的定向测试覆盖 capabilities、状态别名、严格 JSON/action 解析、统一响应以及 hosted operator-path gate。
- 停止与诊断表现：实现变更时验证有界停止/失败回退、停止与窗口关闭的一致入口，以及 Web 对不可用状态的非崩溃诊断呈现；runtime/session/WASM 正确性由其 successor authority 复核。
- 本稳定专题吸收的历史完成证据保留在 Git history 和 PR、实际 CI 与评审记录（Issue 按需）；不再把已删除的日期化专题作为当前入口。

# Local Provider 本地 HTTP Provider 接入 world-simulator 首期方案（2026-03-12）设计

- 对应需求文档: `doc/world-simulator/llm/provider-loopback-http-contract.prd.md`
- 专题入口与权威边界: `doc/world-simulator/llm/README.md`
- 本地会话产品要求：[本地启动会话连续性与恢复](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md)

<a id="1-设计定位"></a>
## 1. 设计定位与目标
定义“安装在用户机器上的 `Local Provider` 如何通过本地 HTTP 参与 world-simulator 的 agent 决策”的首期工程方案，覆盖本地发现、握手、配置、决策请求、反馈回写、状态可观测与失败回退。

## 2. 设计结构
- 用户侧本地 provider 层：`Local Provider` 以独立本地服务运行，仅监听 `127.0.0.1`。
- Adapter 层：world-simulator 内新增 `Local ProviderAdapter`，把 Continuous Harness outer request/response/feedback wrappers（其中保留 `DecisionRequest/DecisionResponse` inner DTO，并以 `FeedbackEnvelopeV1` 作为 target feedback）与本地 HTTP API 互转。
- 配置与发现层：launcher 负责 provider 模式选择、base URL/token 配置、发现与 health-check。
- 运行与裁决层：runtime/kernel 继续负责动作白名单、规则校验、状态演化与事件产出。
- 观测层：viewer 与 launcher 展示 provider 连接状态、最近延迟、最后错误、最近动作与 trace 摘要。
- 测试层：用 mock local HTTP server 替代真实 `Local Provider`，保证 required 回归可离线执行。

### 2.1 需求承接与分配表

| 上游要求 | 承接与分配 | 本设计条款 | 主责与消费方 | 边界 |
| --- | --- | --- | --- | --- |
| [REQ-EXT-005](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#req-ext-005)；Trace refs: O7-REQ-a27895ed-2ca4-45cc-901a-466befd02cf7 | target/legacy 路由、身份与 Runtime 裁决保持独立 | [Harness 对齐](#41-harness-contract-alignment) | runtime_engineer、agent_engineer；HTTP adapter | 本地 callback 不替代完整 Runtime/Game API |
| [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001)；Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1 | 仅承接配置输入与已应用配置的区分，以及停止不撤销已接受世界请求；产物选择和进程清理是专业依赖，不单独算作产品覆盖。 | [交付与生命周期](#local-provider-delivery-contract) | 启动脚本与 Launcher；runtime_engineer、QA | 不停止独立 Provider，不撤销世界已接受请求 |
| [Flow-OC-LOCAL-004（失败恢复）](provider-loopback-http-contract.prd.md#local-provider-user-flows) | native probe 流式预算与安全诊断 | [Native probe](#native-provider-probe-boundary) | client launcher；status/UI/log | 仅探测工程边界，不新增异步或取消 UX |
| [Flow-OC-LOCAL-004（失败恢复）](provider-loopback-http-contract.prd.md#local-provider-user-flows)；同节专业验收 AC-2、AC-6、AC-7（列表条款） | 仅承接单请求 transport 的重试分类、模型 SSE 完成及 URL/许可分离；不承接跨会话恢复重验。 | [HTTP 边界](#local-provider-http-boundaries) | adapter、模型桥接器；agent_engineer、QA | 产品适用性：N/A；reason=本行只分配单请求transport兼容验证方法，不定义停止或陈旧状态后的会话/权限恢复；scope=同一请求attempt/deadline、模型SSE终结与URL解析/许可的工程验证。HTTP功能实际产品消费链尚未准确建立；本行不将整个HTTP功能判为产品不适用，亦不填补已撤的LAUNCH-002关系。 |

## 3. 关键接口 / 入口
- `GET /v1/provider/info`
- `GET /v1/provider/health`
- `POST /v1/world-simulator/decision-context`：target lane 使用 `ContinuousAgentRequestContextV1` / `ContinuousAgentResponseContextV1` outer wrappers，严格要求 Runtime binding 与 capability context。
- `POST /v1/world-simulator/feedback-context`：target lane 使用 `FeedbackEnvelopeV1`，严格要求 Runtime-authoritative receipt/request correlation。
- `POST /v1/world-simulator/decision` 与 `POST /v1/world-simulator/feedback`：bare route 仅为 legacy compatibility-only；旧 DTO 不得进入 target cognition proof、关闭 target turn 或进入 target memory/continuation。当前 HTTP body 没有 `compatibility_lane` wire 字段，route/adapter 配置是 lane 选择依据。
- launcher provider 设置入口
- viewer provider 状态与 trace 调试入口

## 4. 约束与边界
- provider 服务只允许监听本地回环地址，不允许默认对局域网/公网开放。
- adapter 只传输结构化决策，不允许 provider 直接调用 runtime 内部写接口。
- 本地决策 callback 首期不支持反向 callback 与复杂 streaming；一次决策通过单次 request/response 完成。桥接器向模型服务读取 SSE 是另一传输边界，不能据此把本地 callback 改为流式协议。
- 本地 HTTP 失败返回结构化错误与有界重试/等待结果；用户可以显式选择内置 provider 或禁用本地 provider。不得因失败静默替换已选择的 Runtime、改变持续目标或将待决动作当作已取消；保留用户重新选择内置 provider 的入口。
- 首期动作集严格白名单，只覆盖低频、低破坏性动作。
- required 回归不依赖真实用户安装的 `Local Provider`，必须使用 mock 服务即可跑通。

### 4.1 Harness contract alignment

本条只承接 REQ-EXT-005 的 loopback feedback correlation 与 Runtime 结果边界；专业细项沿用[Flow-OC-LOCAL-003、AC-2/7](provider-loopback-http-contract.prd.md#local-provider-user-flows)，不证明结果已进入所有真实 Runtime 的后续认知。

Loopback is an adapter with a strict target lane and a fenced legacy compatibility lane for the Continuous Harness. Its tagged `decision` response
preserves `wait/wait_ticks/act/query/module_command/module_command_response` variants; `query` is
read-only against the frozen world snapshot, while `module_command_response` carries the complete
host-bound `AgentCommandResponse` and requires catalog/context/nonce validation before Runtime.
Its response may carry `module_command?`, but the field is only a typed candidate and follows host schema encoding plus
Runtime validation. The target loopback decision endpoint is
`/v1/world-simulator/decision-context` and carries the outer
`ContinuousAgentRequestContextV1`/`ContinuousAgentResponseContextV1` wrappers; the old
`DecisionRequest`/`DecisionResponse` DTO body is accepted only by the bare
`/v1/world-simulator/decision` legacy route. The target feedback endpoint is
`/v1/world-simulator/feedback-context` and carries `FeedbackEnvelopeV1`; the old
`FeedbackEnvelope` DTO is accepted only by the bare `/v1/world-simulator/feedback` legacy route.
The current HTTP body has no `compatibility_lane` field; route/adapter selection is the explicit
compatibility boundary. Missing session/turn/request/digest/receipt correlation is not target proof.
The target lane
must use the non-recursive V1 `request_digest` (outer context without its output digest or
`transport_attempt`) and the
shared provider invocation derivation. Any heuristic action recovery is legacy fixture/profile
behavior only and is excluded from target parity/proven evidence and automatic
memory/continuation semantics. Trace and feedback limits,
redaction and overflow behavior follow the Continuous Harness documents; raw HTTP bodies are not
authority or replay inputs. Target response memory candidates use `MemoryWriteIntentV1`; the existing
inner DTO is legacy-route only, and a committed Runtime outcome is still required.

## 5. 设计演进计划
- 先落地 launcher provider 配置、发现和 health-check。
- 再落地 `Local ProviderAdapter` 与 mock local HTTP contract tests。
- 再接 viewer 状态/trace 面板与低频 NPC 闭环。
- 最后基于单 NPC 试点结果决定是否扩展动作集、是否引入双向 callback 或更底层 IPC。

<a id="local-provider-delivery-contract"></a>
## 6. 本地交付、配置与停止合同

本节是专业设计要求，不是启动脚本、HTTP 实现或真实 Runtime 已通过验收的声明。它细化配对 PRD 的发现、绑定、失败恢复流程，并仅承接本地会话产品 REQ-ENTRY-LAUNCH-001 的配置输入/实际应用及停止不撤销世界请求边界；REQ-ENTRY-LAUNCH-002 的跨会话重新验证由 Launcher 会话设计负责，是外部依赖而非本节覆盖；外部 Runtime 的完整 Game API/Skill 旅程仍由上游产品与 Runtime adapter 设计拥有。

| 上游要求 | 本设计责任 | 主责与消费方 | 验证边界 |
| --- | --- | --- | --- |
| 配对 PRD Flow-OC-LOCAL-001/002、AC-1 | 同一产物选择用于预检和真实启动，区分配置输入与已应用配置 | 启动脚本、Cargo wrapper、Launcher；Agent adapter 消费结果 | 路径存在或配置保存不证明进程已采用它 |
| 配对 PRD Flow-OC-LOCAL-004、AC-5 | 受控停止、结构化失败、显式 provider 切换 | 本次创建的 supervisor/子进程；Launcher 展示结果 | 不终止用户独立启动的 Runtime 或无关进程 |
| 配对 PRD AC-2/6/7 | 同一请求的有限重试、HTTP 终结与地址解析 | HTTP adapter、模型桥接器、离线合同测试 | 本地 callback、上游模型 SSE 与 Game API 分别验证 |

### 6.1 产物与有效配置

预检和实际启动必须使用同一个 canonical Cargo target/产物解析结果。显式配置的 target 中存在合法产物而 checkout 没有 `target/` 时应可选择；checkout 遗留同名旧 binary 不能优先于已选的有效产物。保留现有默认、自定义 target、raw/CI 与显式 `SOURCE_BIN_DIR` 的各自适用范围，不新设全局绝对路径，也不以补符号链接掩盖选择器差异。没有可执行产物时输出对应 preset/入口的构建下一步，不把 `cargo build` 泛化为任意 binary 已可启动。

bootstrap 的模型 base URL 在其自身边界内遵循：显式 `--base-url` 优先，否则取受支持配置文件中的值，再取已有环境/默认回退。`url`、`OASIS7_LLM_BASE_URL` 等兼容输入按同一解析结果归一化；供子进程使用的已净化配置和环境不能保留另一份覆盖显式参数的旧值。下游 Runtime 的 `config.toml > env` 规则保持独立，不能用 bootstrap 优先级静默改写。诊断仅说明来源类别与是否应用，不输出 key 或敏感配置内容。

### 6.2 进程所有权与停止终态

生成的 detached supervisor 是其创建的 bridge/launcher 子进程的生命周期所有者。对返回的 supervisor PID 发出受支持停止请求后，进入停止中；只有本次拥有的进程组及受控后代退出、其占用端口释放后，才能报告已停止。无法确认时保留失败或未知及诊断，父 shell 退出、状态文件消失和停止请求已发送都不是停止完成。

停止覆盖启动尚未 Ready、子进程初始化失败和正常运行三个阶段。终止与必要强制终止有界，沿用对应入口的预算；清理不能杀死独立启动的用户 Provider、其他会话或未知归属进程。HUP 与显式 TERM 的语义分别测试，不能为了 detached 生存而吞掉显式停止。`DETACH_NO_SUBMIT`、假 supervisor 和直接运行子 binary 不能代替实际生成入口验收。停止本地进程不撤销世界已接受的请求；重启后按原请求关联查询结果。

<a id="local-provider-http-boundaries"></a>
## 7. HTTP 失败、终结和地址边界

本节保留单请求传输的技术合同，专业细项沿用[Flow-OC-LOCAL-004、AC-2/6/7](provider-loopback-http-contract.prd.md#local-provider-user-flows)。它不实现停止、陈旧状态或失败后重新进入时的 authority、session、配置和 primary mode 重验；该产品义务仍由 Launcher 会话连续性专业设计负责，不能由 HTTP 重试或 SSE/URL 验证代签。

### 7.1 一个决策请求的重试

transport 与 adapter 使用一致的 retryable 分类。对现有允许重试一次的失败（包括首次 429），总尝试数最多为二；两次尝试仍属于同一 session/turn/request，保留 request digest，只增加 `transport_attempt`。重试、等待和请求读取共同受已有总 deadline/取消预算约束，不能叠加成新的回合预算。

400/401、解码失败和明确不可重试的业务结果不能由包装层重新标成可重试；已有 `provider_error` 保留原类别与有用诊断。重试耗尽只结束该请求的失败处理，不生成成功动作、取消已有提交或自动切换 provider。本节不新增 Retry-After、指数退避、无限重试或跨 Runtime 自动降级承诺。

### 7.2 上游模型 SSE 的完成信号

对当前使用 `[DONE]` 的模型 SSE 路径，读取到该完整事件后立即结束流读取，再按既有规则验证已累积内容、usage 和结构化输出。合法 content/usage/[DONE] 后连接保持打开、晚到 I/O 错误或读超时，都不能把已收到的完整响应改写成读取失败；也不得等待 EOF 才结束。

`[DONE]` 不是无条件成功：在它之前的坏 JSON、I/O 失败以及只有 `[DONE]` 而没有合法必要内容仍失败。保留已有 usage 的累计结果；不额外读取或拼接完成信号之后的数据。Rust 与 Python 路径分别验证，不能以一种客户端通过代签另一种；这不改变本地 JSON callback 的路由、DTO 或权威边界。

### 7.3 URL 语法与端点许可分离

受支持 URL 解析器对方括号 IPv6 host、scheme 默认端口和 path 的解释保持一致；HTTPS 省略端口与显式 443 的同一地址语义一致。错误括号、含糊的裸 IPv6 和非法/零端口明确拒绝，不猜测拆分位置。语法能够解析不意味着端点获准：本地 callback 继续遵守 loopback/allowlist 限制，上游模型 endpoint 沿用自己的配置和权限边界，不因支持 IPv6 放开远程 callback。

<a id="native-provider-probe-boundary"></a>
### 7.4 Native launcher compatibility probe 的读取与诊断

Trace ID: O7-REQ-50ef76f3-2e7e-46c6-9154-54b1eb900e57

本节唯一拥有 native `ProviderProbeHttpClient` 的响应字节预算与错误摘要合同；主责 client launcher，UI/status/local log 为消费者。承接配对专业 PRD [发现/失败恢复与 AC-2/6/7](provider-loopback-http-contract.prd.md#local-provider-user-flows) 的探测子范围。产品适用性：N/A；reason=只收紧现有 probe IO/诊断，不新增玩家流程；scope=native info/health probe，不改变决策 callback、模型 SSE 或 Viewer。目标由 [#5103](https://github.com/nil-studio/oasis7/issues/5103) 跟踪。

1. 保留 `info -> health -> evaluate_provider_compatibility` 的顺序、首步失败停止、现有 timeout、端点许可与兼容/降级判断；不得用此改动新增自动 probe/retry、付费调用或 redirect 策略。同步 UI 是否改 worker、取消交互及代次 fencing 不属于本合同，不把现有同步实现称作违反取消承诺。
2. 每个 info/health HTTP 响应的应用可见 body 上限为 `64 KiB`，包含 HTTP 客户端解压后交给 JSON decoder 的字节；两次顺序请求各自计数，不把总计当单响应预算。读取前设限，按流累计，最多读取上限加一个字节用于判超限；不得先 `.bytes()`/`.text()` 全量分配再截断。若采用压缩传输也必须限制解压后的读取，不以压缩长度替代。该值为 native probe 的实现常量；不依赖远端长度预分配。
3. `Content-Length` 只可用于提前拒绝，缺失、伪小值或 chunked 不能绕过实际计数；禁止按不可信长度预分配。达到上限仍需确认 EOF；第一个超额字节即停止读取、丢弃累积 body，返回固定 `response_too_large`，不继续 drain 或 JSON 解析。恰好上限且完整合法 JSON 可继续正常解析；慢响应仍受原 timeout 约束。
4. 只返回本地固定分类/说明，可附本地 endpoint 枚举（info/health）、数字 HTTP status 和饱和有界计数。至少区分 unauthorized、其他 HTTP failure、timeout、transport/read failure、response_too_large、decode failure；保留既有 UI 状态含义，不把任意错误归兼容失败。非 2xx 可在状态头处直接拒绝而不读 body；若读取任何 body，预算一律适用。
5. 错误投影禁止透传供应商 body、任意 error/message、网络库原始错误或包含 token/query/userinfo 的 URL，UI、本地日志与 Debug/Display 使用同一安全摘要。成功 health 中的 `last_error` 也是诊断自由文本，不能因 2xx 直接写 UI/log；映射固定 provider-reported-error 或明确 allowlist 分类，保留 health/兼容结果本身。不改变其余合法结构化 info/health 的业务判定，也不宣称所有成功文本字段已完成通用隐私治理。

[HTTP client](../../../crates/oasis7_client_launcher/src/launcher_core_http.rs) 通过有界 reader 累计读取至上限加一字节，并在解析前拒绝超限；401/非 2xx 在状态头处拒绝。固定错误摘要通过 [status](../../../crates/oasis7_client_launcher/src/provider_check_status.rs) 与现有日志入口投影。成功业务字段仍按第 5 条的范围处理。

离线验收须在真实 HTTP reader 与 status/log seam 覆盖：缺长度、虚假长度、chunked、压缩（启用时）、恰好上限/上限加一、超大合法/畸形 body、慢响应和读中断；断言预算生效且超限不保留 body。401/其他非 2xx/坏 JSON/2xx health.last_error 放短 canary，断言 UI 和本地日志均无原文；检查固定分类、info/health 次序和原兼容/降级正例不变。不调用真实付费 provider；可复用回归位于 `config_ui_provider_probe_tests.rs` 与 HTTP reader 的预算测试。HTTP 库按小 Content-Length 提前结束的响应只允许已解码字节进入 JSON，帧外字节不能绕过预算。测试不代签异步/取消 UX 或反馈导出隐私验收。

## 8. 对应验收与交付判定

| 设计边界 | 最小正例 | 关键反例/结果 |
| --- | --- | --- |
| 产物解析 | 仅显式配置的 target 有产物；默认/自定义/raw/CI 入口选择一致 | checkout 陈旧产物不被误选；无产物给准确构建指引 |
| 有效配置 | 显式 URL 与文件/环境冲突时，净化文件和真实 child env 均采用显式值 | 无显式参数保留文件与兼容回退；不更改下游 Runtime 优先级 |
| 停止 | 实际生成入口的返回 PID 接收 TERM；自己的后代退出、端口释放 | Ready 前停止、失败退出、HUP 分别覆盖；未知/独立进程不被清理 |
| 重试 | 首次 429、第二次成功，关联不变且尝试数为二 | 不可重试错误仅一次；重试耗尽与取消不产生新回合或假成功 |
| SSE | content + usage + `[DONE]` 后仍保持连接，立即完成且只产出一次结果 | 完成前坏 JSON/I/O、空内容完成标记失败；不读取完成后错误 |
| URL | 方括号 IPv6、默认/显式端口与 path 一致 | 坏括号、裸 IPv6、非法/零端口拒绝；语法通过不绕过端点许可 |

产物、配置、HTTP 与进程测试按各自层次记录。HTTP/config 回归可使用假 key 和离线 mock；进程停止必须观察真实生成的本地子进程，不能用返回 0 的替身代签。上述结果均不代替玩家 Claim、完整生产首局、真实付费 Provider、公开发行或部署验收。实际实现状态和测试结果留在 PR/Issue 中，不因本设计合入勾选整项完成。

## 11. 验证

### 11.1 验证映射表

| 上游要求 | 本设计条款 | 机制与场景 | 验证来源 | 执行证据 | 未覆盖与状态 |
| --- | --- | --- | --- | --- | --- |
| [REQ-EXT-005](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#req-ext-005)；Trace refs: O7-REQ-a27895ed-2ca4-45cc-901a-466befd02cf7 | [Harness 对齐](#41-harness-contract-alignment) | target outer/feedback correlation 与 legacy 隔离；沿用 mock 合同，真实 Runtime 单独验收 | [HTTP合同测试](../../../crates/oasis7/src/simulator/tests/provider_loopback_http.rs)：`env -u RUSTC_WRAPPER cargo test -p oasis7 provider_loopback_http_client_round_trips_info_health_decision_and_feedback -- --nocapture` 当前仅验证bare legacy DTO往返；target outer/feedback关联与legacy不能关闭target的场景按§4.1另补并记录真实测试ID，不以该legacy正例代签 | 实际 source、命令、退出码和失败反例，记录在 PR/Issue | 本次只建立映射，未执行；mock 不证明真实首局 |
| [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001)；Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1 | [交付与生命周期](#local-provider-delivery-contract) | 对账输入配置与真实child有效配置，并核对停止不取消世界已接受请求；§8产物选择/TERM/HUP仅属专业依赖验证，不代签该产品子义务。 | [本地 LetAI 回归](../../../scripts/run-local-letai-game-test.test.sh)：`bash scripts/run-local-letai-game-test.test.sh` 现有检查包括help/preflight、缺配置/产物和占用端口诊断；§8全部真实进程停止条件仍是待补/单独验证场景。待补：同候选先记录世界已接受请求的request/Agent/任务关联，停止本地自有进程后按原request查询权威结果，对账停止前后的pending/committed/rejected含义及是否误取消/重放；现有shell不证明此项，不声称已有fixture。 | source、选中产物、输入与真实child有效配置；另存原request关联、停止前后权威查询/receipt及未取消/未重放断言，子树退出和端口释放仅为依赖证据。 | planned；现有测试入口不表示这些新增用例已实现或通过 |
| [Flow-OC-LOCAL-004（失败恢复）](provider-loopback-http-contract.prd.md#local-provider-user-flows) | [Native probe](#native-provider-probe-boundary) | 上限/长度头/慢响应及 canary，保留顺序与兼容判断 | [回归方法](../../../testing-manual.md#regression-oracle)；HTTP probe 测试入口 `../../../crates/oasis7_client_launcher/src/launcher_core_http.rs`与 status/log 回归，场景见§7.4 | 实际读取量、固定分类、UI/log 无 canary 和请求序列 | reader 与 status/log 离线回归已接线；实际执行结果见当前 PR，不含 async/cancel 或真实供应商 |
| [Flow-OC-LOCAL-004（失败恢复）](provider-loopback-http-contract.prd.md#local-provider-user-flows)；同节专业验收 AC-2、AC-6、AC-7（列表条款） | [HTTP 边界](#local-provider-http-boundaries) | §8 的同请求两次尝试、DONE 后保持连接/禁止额外读取和 IPv6 反例 | [HTTP合同测试](../../../crates/oasis7/src/simulator/tests/provider_loopback_http.rs)：运行 `env -u RUSTC_WRAPPER cargo test -p oasis7 provider_http_failure_retryability_is_deterministic -- --nocapture`，只证明RequestFailed/429/503可重试与401/400不可重试分类；`provider_loopback_http_client_surfaces_http_401_on_decision`只证明401映射。两次尝试/耗尽/取消、SSE完成边界与Rust/Python URL组合仍须各自新增fixture和准确测试ID，不借分类单测声称已证明 | source、客户端类型、尝试/读取计数、结果和命令退出码 | planned；不以 shell 通过或任一客户端结果代签全部 HTTP 验收 |

新增场景与已存在测试入口明确区分。实现任务须把本表场景落实到对应测试并记录真实结果；本表不构造已通过证据，也不以通用手册引用免除 §8 的逐项验收。

## 12. 决策与残余风险

审读基线为 `4e94163a3dff9ba69f6959b406ca3fcedf8aa097`。保留 target/legacy 路由隔离与单次 JSON callback，不把上游 SSE 或 URL 解析能力扩大为公网 callback 许可。§8 与 §11 是分层验证计划，不声明全部负例已有测试；agent/runtime/launcher owner 在重试预算、进程所有权、配置选择或协议版本变化时复核。真实 provider 鉴权、完整首局与跨入口玩家反馈仍需独立证据。

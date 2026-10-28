# 游戏客户端启动器控制面与机器接口设计

- 对应需求文档: `doc/world-simulator/launcher/game-client-launcher-control-plane-and-machine-interface.prd.md`
- 历史迁移、验证与 task 状态：GitHub task issue evidence。

### 2.1 需求承接与分配表

本轮仅补有限结果与重核关系。Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1；O7-REQ-1d9a996e-dd9d-4d59-9b3b-b79191f17676。引用既有产品声明，不创建 Trace ID。真实 authority/session/config/mode 重验仍由[会话重新验证](game-client-launcher-runtime-session-continuity.design.md#session-revalidation)拥有；Web runtime 失败沿用该设计 §5.2。

根产品部分覆盖/部分实现、当前尝试诊断、凭据/权限、Web 与世界恢复缺口不变。以下专业行不是通用 N/A：分别保留具体工程合同与验收方法，不借产品 ID 代表 schema、清理、目录、metadata 或机器能力覆盖。

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [L1 慢停止](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l1) | L1 专业机制：端到端预算、优雅等待/有界终止/wait；真实 terminal/unknown 与重核另列产品关系 | [L1 慢停止设计](#launcher-l1-design) | Launcher/platform/QA；session authority | 预算/stop-wait 机制不代签 world outcome 或完整权限/配置；Windows 活跃 child 仍独立验证；产品独立覆盖 N/A；理由：端到端预算/stop-wait 是平台进程机制，有限结果/重核另列产品行；范围：L1 同步停止预算、回退及 Windows 活跃 child 验证 |
| [L2 服务异常](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l2) | L2 专业机制：accept-loop 统一收尾、stop/wait、PID/端口、可恢复退避和各清理尝试；失败结果另列 | [L2 服务异常设计](#launcher-l2-design) | 平台进程/QA | 不以连接上限或局部 no-op 代签 teardown；不新增全局 kill；产品独立覆盖 N/A；理由：accept-loop 收尾和错误保留是托管进程工程合同；范围：L2 正常/异常退出及 PID/端口/退避矩阵 |
| [L3 拒绝 Start](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l3) | L3 专业机制：先检查后变更、保留原 child/PID/config、双入口与后续 Stop；拒绝和会话结果另列 | [L3 拒绝 Start设计](#launcher-l3-design) | session-revalidation owner；Launcher/QA | 短命 child、空态拒绝不能代签活动句柄/双入口；产品独立覆盖 N/A；理由：拒绝后句柄/配置不变是 session owner 的先检查后变更机制；范围：L3 活动 child 的双入口拒绝及后续 Stop |
| [L4 非法 payload](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l4) | L4 专业机制：两 Start route JSON/type/tier 校验、完整 ApiResponse、零 spawn/不改状态、非 2xx 解码；结果另列 | [L4 非法 payload设计](#launcher-l4-design) | HTTP/Launcher/QA | 不强制新增 HTTP code、chunked 或 429；产品独立覆盖 N/A；理由：HTTP 解码与零 spawn 是控制协议验证，不能代签产品结果；范围：L4 两个 Start 路由的非法 payload 和结构化诊断 |
| [L6 显式 static-dir](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l6) | 显式来源优先非空 env 再 fallback，含显式 web 与 Windows wrapper | [L6 显式 static-dir设计](#launcher-l6-design) | CLI/packaging/QA | 值比较或 trim 单测不代签来源矩阵；产品独立覆盖 N/A；理由：参数来源和路径优先级是 CLI/包装器解析规则；范围：L6 显式与省略 static-dir 及 Windows 透传 |
| [L7 健康语义](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l7) | L7 专业机制：健康字段/类型/语义预检、退出码/未启动/成功产物隔离、skip 调试用途；失败不得假成功另列 | [L7 健康语义设计](#launcher-l7-design) | 脚本/provider-contract/QA | 健康不等于 decision/tool-call 或玩法；产品独立覆盖 N/A；理由：健康字段预检是脚本输入门禁，不是模型或玩法成功；范围：L7 离线健康/结构/网络失败和产物隔离 |
| [L8 metadata](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l8) | 父子唯一规范路径、默认/相对/绝对/空格/重复参数、非 headed exec；本次 ready URL | [L8 metadata设计](#launcher-l8-design) | producer/Launcher scripts/QA | metadata 不是 session 或世界结果证据；产品独立覆盖 N/A；理由：父子路径一致是启动脚本产物定位合同；范围：L8 metadata 解析与本次 ready URL 消费 |
| [共享 schema](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-schema) | native/Web 字段、分组、文案、可见性/状态错误映射一致 | [共享 schema设计](#launcher-schema-design) | UI/schema/QA | 表现层不是 runtime 配置或世界规则第二真源；产品独立覆盖 N/A；理由：共享字段/可见性映射是表现层一致性合同；范围：native/Web schema 映射，不拥有 runtime 规则 |
| [控制状态真值](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-state) | 控制状态协议：GET state、game/chain 路由与机器 state alias 对齐；快照结果语义另列 | [控制状态真值设计](#launcher-state-design) | 控制面/客户端/QA | 不由客户端推断 runtime 收敛；产品独立覆盖 N/A；理由：服务端快照与 alias 对齐是控制状态协议合同；范围：state/既有控制路由/机器 state alias |
| [静态与 API 并存](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-static-api) | 静态缺目录/资源不可用不得改变既有 API 路由语义 | [静态与 API 并存设计](#launcher-static-api-design) | Web server/QA | 不扩展为公开 readiness 或目录解析全覆盖；产品独立覆盖 N/A；理由：静态资源失败不改变 API 是 Web 服务路由隔离合同；范围：静态缺失和 API 并存，不承担目录来源解析 |
| [能力发现](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-capability) | 当前版本/actions/query_targets 是清单真值；payload 提交时校验 | [能力发现设计](#launcher-capability-design) | 机器接口/QA | 不声称永久穷尽 action catalog/payload schema；产品独立覆盖 N/A；理由：能力发现只描述当前 operator 动作/查询清单；范围：capabilities 版本/actions/query_targets，非 payload schema |
| [严格动作与响应](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-machine-action) | 机器协议：严格 action/payload、统一字段/分类和 state alias；真实拒绝/错误/当前状态语义另列 | [严格动作与响应设计](#launcher-machine-action-design) | 机器接口/HTTP/QA | 不是世界 Agent、DecisionProvider 或 runtime 写权限；产品独立覆盖 N/A；理由：严格枚举及统一响应是机器接口协议合同；范围：action/payload 分类与响应/state alias |
| [声明查询映射](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-query) | 查询适配：仅声明 target/参数与越界拒绝；不可用查询结果语义另列 | [声明查询映射设计](#launcher-query-design) | 机器接口/security/QA | 不承诺 transfer 当前可用性；产品独立覆盖 N/A；理由：声明 target/参数限制是查询适配边界；范围：声明映射及越界拒绝，非 transfer 可用性 |
| [部署安全非主张](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-security) | hosted_public_join 枚举 operator 路由 peer-IP gate 与 wildcard/public 路由边界 | [部署安全非主张设计](#launcher-security-design) | 部署/security/QA | 不宣称认证/RBAC/TLS/rate limit、loopback-only、公开 readiness/SLA；产品独立覆盖 N/A；理由：peer-IP gate 与非主张是部署/安全专业边界；范围：hosted 枚举 operator 路由，不证明认证或公开 readiness |
| [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | 初始化/环境/状态/请求失败显示 blocked/error 和恢复下一步，局部成功不代签世界结果；专业来源：[Web 结果呈现](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-result) | [Web 结果呈现设计](#launcher-result-design) | Web/Launcher；session authority/QA | 不接管 clock、WASM、轮询或世界恢复 |
| [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | 只消费 L1 的 terminal/unknown 如实呈现，本地控制结果不等于世界结果；专业来源：[L1 慢停止](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l1) | [L1 慢停止设计](#launcher-l1-design) | Launcher/platform/QA；session authority | 不承接停止预算/平台清理算法或完整产品 AC |
| [REQ-ENTRY-LAUNCH-002](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-002) | 只消费 L1 的 timeout/停止/失败后现有 state 重核；未知不当成功且不自动重发非幂等动作；专业来源：[L1 慢停止](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l1) | [L1 慢停止设计](#launcher-l1-design) | Launcher/platform/QA；session authority | 完整 authority/session/config/mode 重验归 session-revalidation；不接管恢复存储 |
| [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | L2 只承接主错误/各清理失败如实保留、不把未确认退出或端口释放报成功；不把本地清理当世界结果；专业来源：[L2 服务异常](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l2) | [L2 服务异常设计](#launcher-l2-design) | 平台进程/QA | accept-loop/stop-wait 机制仍由专业 owner 验证，不新增进程发现 |
| [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | L3 只承接新 Start 被拒绝时，旧活动进程/配置与当前状态不被新请求伪装为成功启动；专业来源：[L3 拒绝 Start](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l3) | [L3 拒绝 Start设计](#launcher-l3-design) | session-revalidation owner；Launcher/QA | 保留句柄机制及双入口全部实现矩阵仍专业验收；不证明新配置已应用 |
| [REQ-ENTRY-LAUNCH-002](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-002) | L3 仅已有可存活活动 child 的重复 Start 被拒绝时，继续消费原会话当前状态，不静默换成新请求上下文；无活动进程的新启动失败仍清除旧证据并按 session-revalidation 重验；专业来源：[L3 拒绝 Start](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l3) | [L3 拒绝 Start设计](#launcher-l3-design) | session-revalidation owner；Launcher/QA | 不新增恢复协议，不代签完整 authority/权限/配置/模式重验 |
| [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | L4 只承接非法请求的真实拒绝原因与当前 state，不将输入或请求受理误报启动/世界成功；专业来源：[L4 非法 payload](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l4) | [L4 非法 payload设计](#launcher-l4-design) | HTTP/Launcher/QA | ApiResponse 字段/HTTP 解码/零 spawn 由 L4 专业机制验证 |
| [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | L7 只承接预检不健康/失败不得继续显示启动成功；健康探针与真实 decision/tool-call/玩法分开；专业来源：[L7 健康语义](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l7) | [L7 健康语义设计](#launcher-l7-design) | 脚本/provider-contract/QA | provider 健康字段/协议/脚本算法不归产品；不证明模型或世界结果 |
| [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | state 只承接服务端当前快照和受控请求结果如实呈现；本地状态不代签 world outcome；专业来源：[控制状态真值](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-state) | [控制状态真值设计](#launcher-state-design) | 控制面/客户端/QA | 不替代 state/alias 协议一致性或真实 authority/session/config 验证 |
| [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | action 只承接拒绝/错误及最新 state 的真实结果，机器响应局部成功不代签世界成功；专业来源：[严格动作与响应](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-machine-action) | [严格动作与响应设计](#launcher-machine-action-design) | 机器接口/HTTP/QA | 严格枚举/payload/统一字段属机器专业合同；不是世界 Agent 能力 |
| [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | query 只承接已声明查询不可用/chain_disabled 的准确错误和当前状态，不以 operator 查询可达宣称世界可用；专业来源：[声明查询映射](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-query) | [声明查询映射设计](#launcher-query-design) | 机器接口/security/QA | target/参数映射与安全边界仍专业；不保证 transfer 可用性 |

## 目标

launcher 的 native/Web 表现层共享 schema 和控制状态映射；`oasis7_web_launcher` 承担静态资源、状态读取和既有受控操作路由。客户端不自行重演进程或 runtime 规则，而是渲染服务端的当前状态与结果。

## 范围

本设计覆盖 launcher/Web 表现层、控制面映射与 operator HTTP-JSON 接口；不覆盖 runtime 规则、执行语义或 `DecisionProvider`。

## 接口 / 数据

<a id="launcher-schema-design"></a>

1. 共享表现层：native 与 Web 共用字段、分组、文案和状态/错误映射。
<a id="launcher-state-design"></a>

2. 控制层：`/api/state` 与既有 game/chain 控制路由提供服务端快照和受控请求结果；停止 UI 统一呈现用户停止和窗口关闭所触发的有界优雅停止、终止回退或失败诊断，不将请求结果扩展为 runtime 收敛结论。
<a id="launcher-capability-design"></a>

3. 机器层：`/api/gui-agent/capabilities`、`/state`、`/action` 组成 operator HTTP-JSON 适配面；capabilities 响应是 action-list truth。
<a id="launcher-security-design"></a>

4. 安全边界：查询/动作必须经过已声明映射；`hosted_public_join` 仅对枚举 operator 路由应用 peer-IP gate，不扩大为部署或安全 readiness 声明。
<a id="launcher-result-design"></a>

5. Web 诊断层：初始化、浏览器环境、状态读取或控制面失败必须映射为可见 blocked/error，而不是 ready 或静默崩溃；具体浏览器 clock、轮询、WASM lifecycle 与 runtime session 仍由 `game-client-launcher-runtime-session-continuity.prd.md` 及对应专业 authority 定义。


## 边界与结构

<a id="launcher-static-api-design"></a>
静态资源与 API 并存按配对 PRD 的客户端与控制面合同承接：静态目录或资源不可用不改变既有 API 路由语义；这是 Web 服务工程约束，不以目录或页面可见性证明世界成功。

<a id="launcher-machine-action-design"></a>
机器 action 消费服务端严格枚举与 payload 校验，返回统一 `ok`、`action`、可选 `error_code`/`error`/`data` 和最新 `state`；state alias 与 `/api/state` 对齐。此处承接既有机器接口合同，不新增 action 或公共 schema。

<a id="launcher-query-design"></a>
查询仅消费服务端已声明 target 和参数映射，不提供任意 URL/命令穿透；capability discovery 不是永久穷尽的 payload schema。

<a id="launcher-action-lifecycle-design"></a>
## 控制动作与启动输入设计

本节承接[控制动作生命周期要求](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-action-lifecycle)。
不增加 action endpoint、持久化控制任务或 hosted 本地链能力；以下待修项按实际实现 PR 验证。

### 停止和服务退出

沿用同步 stop 响应，使用动作专用的端到端等待预算，不让普通状态查询的短超时覆盖停止。
预算覆盖当前 `GRACEFUL_STOP_TIMEOUT_MS`、有界终止/wait 与传输余量；客户端收到该动作
终态才显示完成。响应丢失或预算耗尽时结束本次网络等待，显示结果未知并通过现有 state 重新核实，不能
把超时解释成进程已停或停止必定失败。后续真实 snapshot 可以恢复准确状态，不自动重发
非幂等控制操作。服务端停止回退同样有界，失败时保留可管理句柄与具体诊断。

accept loop 区分正常停止、可恢复错误与不可恢复错误。不可恢复错误先保存主错误，再退出
循环进入统一收尾；不得从循环内部直接返回绕过 game/chain stop/wait。收尾逐一尝试现有
托管对象，记录各自结果，保留主错误；没有 child 的路径不制造清理成功证据。已声明的
托管子进程之外，不新增全局 kill、进程发现或 Windows graceful signaling 语义。

基准 `5e930fa` 的 native `app_process.rs` HTTP 超时为 1500ms，服务端优雅等待为 4000ms；
`server.rs` 的非 WouldBlock accept 错误直接返回，确有预算/收尾缺口。`start_chain_process`
已有先拒绝 running 再变更的 guard，须保留并覆盖 GUI-agent 与 HTTP 两入口，不为本次设计
重新实现已存在的分支。

### 解析与启动前验证

1. 路由在既有访问检查之后，把 parse/validation 失败映射成当前 `ApiResponse`；错误分支同样
   读取准确快照和写完整 HTTP 响应。不得把 parse error 只写服务端日志后断开连接，也不把
   非 2xx 统一变成无内容的客户端 QueryFailed。无效输入不提交新配置、不 spawn。
2. CLI parser 保留“用户是否显式提供静态目录”的来源信息，resolver 按 explicit→environment→
   existing fallback 选择。比较字符串是否等于默认值不能判断来源；空环境按未设置处理。
   不增加候选目录搜索或让有效显式路径受错误环境值覆盖。
3. `run-launcher-stack.sh` 的 provider preflight 解析 info/health 后按现有 provider 健康合同
   校验必需字段类型、协议/能力及 healthy 语义，再生成成功记录；网络、JSON、结构或语义
   失败返回非零，并且不进入 Launcher 启动分支。不把健康探针升级为真实推理或工具调用。
4. `run-producer-playtest.sh` 在启动 child 前解析最终 metadata 参数并规范化一次，父级读取
   和子级写入使用同一值。沿用子入口当前重复参数规则，父级不得保留另一默认值；若选择
   不支持某类参数则启动前清晰拒绝。打开浏览器只消费本次 metadata 的 ready URL。

| 独立项 | 最小有效验证 | 不可替代的范围 |
| --- | --- | --- |
| <a id="launcher-l1-design"></a>L1 慢停止 | 真实 HTTP handler＋可控 child：快停、约 2 秒退出、约 4 秒 graceful 回退、真无响应；native transport 读取真实终态并保留有界未知状态 | Windows active-child 单独运行；普通 Unix/mock/no-op 不代签 |
| <a id="launcher-l2-design"></a>L2 服务异常 | 已托管 child 后注入 accept failure，核 stop/wait、PID/端口退出；低 FD/短连接峰值、正常 SIGINT、无 child 与可恢复退避 | 连接数量限制不证明 teardown；不纳入 detached shell 转发 |
| <a id="launcher-l3-design"></a>L3 拒绝 Start | 已有可存活 child 后从 GUI-agent 与 `/api/chain/start` 提交 disabled/hosted/invalid-mode，拒绝后同 PID/配置仍可 Stop；合法重复启动/空态另测 | 只测空 ServiceState 不证明保留原句柄 |
| <a id="launcher-l4-design"></a>L4 非法 payload | 两个 Start 路由的坏 JSON/字段类型/未知 tier，断言完整可解码响应、invalid_request、原 state 和零 spawn，另测有效输入 | parser 单测不证明完整 HTTP；不改 chunked/429 |
| <a id="launcher-l6-design"></a>L6 显式 static-dir | 两目录有效及坏环境目录下比较省略、显式 web、./web、绝对路径、空环境；Windows wrapper 参数透传 | 不声称所有目录解析或打包均已验证 |
| <a id="launcher-l7-design"></a>L7 健康语义 | mock HTTP 200 healthy/ok=false/degraded/缺字段/错类型，加网络/JSON 失败；断言退出码、未启动和成功产物未生成 | 不需要真实模型；preflight pass 不等于玩法 pass |
| <a id="launcher-l8-design"></a>L8 metadata | 受控 stack/browser fixture 覆盖默认、绝对/相对/含空格/重复参数与非 headed 直接 exec，核父子实际读写路径 | 不需要真实 provider 或公开发布 |

实际测试文件优先复用 `crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/control_plane/tests.rs`、
`crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/oasis7_web_launcher_tests.rs`、
`oasis7_client_launcher` 的 process tests、`crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/viewer_static_dir_tests.rs` 和
相关 launcher/producer shell tests；新增回归绑定实际代码，不在本文虚构函数或通过结果。
可见状态变化按 S6 验证，原控制权限和 runtime/session 专业合同保持不变。

## 里程碑

- 已完成：shared schema、静态资源/控制面并存、GUI-agent capabilities/state/action 合同。
- 后续：任一实现变更以新的任务和运行中 capability discovery 响应重审。

## 风险

- 机器 action 必须是服务端枚举的严格 JSON 请求；响应始终附带最新状态，避免 UI/机器客户端各自猜测。
- 保留既有 `/api/*` 和静态资源行为，不能把 GUI-agent 变成任意代理。
- GUI-agent 是 launcher operator interface；它不实现或替代 world Agent、DecisionProvider、runtime authorization 或执行规则。
- 控制面只呈现 stop 请求和快照结果；chain/runtime/session、execution-world recovery 及 WASM 兼容性不在此设计中重演。
- UI/schema 变更应保持浏览器可自动化；可视化代码变更另按 `testing-manual.md` S6 做浏览器与截图验证。本次为文档迁移，无 UI 产物变更。

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 验证方法、test/manual source 与环境 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [L1 慢停止](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l1) | [L1 慢停止设计](#launcher-l1-design) | L1 专业机制：端到端预算、优雅等待/有界终止/wait；真实 terminal/unknown 与重核另列产品关系 | 本设计独立项 L1 的真实 HTTP＋可控 child 矩阵；`crates/oasis7_client_launcher/src/app_process.rs` 当前 `CONTROL_PLANE_HTTP_TIMEOUT_MS=1500`，stop 仍走同 `http_json_request`；`crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/oasis7_web_launcher_tests.rs` 两个 stop no-op 测试只保留原错误；planned，Windows active child 独立；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | 预算/stop-wait 机制不代签 world outcome 或完整权限/配置；Windows 活跃 child 仍独立验证；产品独立覆盖 N/A；理由：端到端预算/stop-wait 是平台进程机制，有限结果/重核另列产品行；范围：L1 同步停止预算、回退及 Windows 活跃 child 验证；未执行业务验收 |
| [L2 服务异常](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l2) | [L2 服务异常设计](#launcher-l2-design) | L2 专业机制：accept-loop 统一收尾、stop/wait、PID/端口、可恢复退避和各清理尝试；失败结果另列 | 本设计独立项 L2 的 accept failure、真实托管 child、低 FD/短连接、SIGINT、无 child/退避矩阵；尚无完整执行证据；planned；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | 不以连接上限或局部 no-op 代签 teardown；不新增全局 kill；产品独立覆盖 N/A；理由：accept-loop 收尾和错误保留是托管进程工程合同；范围：L2 正常/异常退出及 PID/端口/退避矩阵；未执行业务验收 |
| [L3 拒绝 Start](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l3) | [L3 拒绝 Start设计](#launcher-l3-design) | L3 专业机制：先检查后变更、保留原 child/PID/config、双入口与后续 Stop；拒绝和会话结果另列 | `crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/control_plane/tests.rs`::session_duplicate_start_keeps_running_process_and_evidence 使用 Unix exit 0；本设计 L3 可存活 child 双入口与后续 Stop 矩阵仍 planned；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | 短命 child、空态拒绝不能代签活动句柄/双入口；产品独立覆盖 N/A；理由：拒绝后句柄/配置不变是 session owner 的先检查后变更机制；范围：L3 活动 child 的双入口拒绝及后续 Stop；未执行业务验收 |
| [L4 非法 payload](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l4) | [L4 非法 payload设计](#launcher-l4-design) | L4 专业机制：两 Start route JSON/type/tier 校验、完整 ApiResponse、零 spawn/不改状态、非 2xx 解码；结果另列 | `crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/oasis7_web_launcher_tests.rs`::gui_agent_action_rejects_unknown_action_with_invalid_request 仅定义 ok/action/error_code/state 断言；本设计 L4 两 route 完整 HTTP 矩阵仍 planned；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | 不强制新增 HTTP code、chunked 或 429；产品独立覆盖 N/A；理由：HTTP 解码与零 spawn 是控制协议验证，不能代签产品结果；范围：L4 两个 Start 路由的非法 payload 和结构化诊断；未执行业务验收 |
| [L6 显式 static-dir](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l6) | [L6 显式 static-dir设计](#launcher-l6-design) | 显式来源优先非空 env 再 fallback，含显式 web 与 Windows wrapper | `crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/viewer_static_dir_tests.rs` 仅默认/env、非默认显式绝对路径与缺目录样例；本设计 L6 显式 web/./web/错误 env/Windows 矩阵 planned；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | 值比较或 trim 单测不代签来源矩阵；产品独立覆盖 N/A；理由：参数来源和路径优先级是 CLI/包装器解析规则；范围：L6 显式与省略 static-dir 及 Windows 透传；未执行业务验收 |
| [L7 健康语义](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l7) | [L7 健康语义设计](#launcher-l7-design) | L7 专业机制：健康字段/类型/语义预检、退出码/未启动/成功产物隔离、skip 调试用途；失败不得假成功另列 | `scripts/run-launcher-stack.sh` 为实现接点；本设计 L7 离线 HTTP 健康/结构/网络/JSON fixture 为 planned；未审完整脚本矩阵且未运行，不声称测试不存在；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | 健康不等于 decision/tool-call 或玩法；产品独立覆盖 N/A；理由：健康字段预检是脚本输入门禁，不是模型或玩法成功；范围：L7 离线健康/结构/网络失败和产物隔离；未执行业务验收 |
| [L8 metadata](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l8) | [L8 metadata设计](#launcher-l8-design) | 父子唯一规范路径、默认/相对/绝对/空格/重复参数、非 headed exec；本次 ready URL | `scripts/run-producer-playtest.sh` 为实现接点；本设计 L8 受控 stack/browser fixture planned；未审完整脚本矩阵且未执行，不需付费 provider；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | metadata 不是 session 或世界结果证据；产品独立覆盖 N/A；理由：父子路径一致是启动脚本产物定位合同；范围：L8 metadata 解析与本次 ready URL 消费；未执行业务验收 |
| [共享 schema](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-schema) | [共享 schema设计](#launcher-schema-design) | native/Web 字段、分组、文案、可见性/状态错误映射一致 | 配对 PRD 共享 schema 合同与本设计接口项 1；同候选 native/Web 字段映射对照及可见变更 S6；planned，历史里程碑不作为本轮执行；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | 表现层不是 runtime 配置或世界规则第二真源；产品独立覆盖 N/A；理由：共享字段/可见性映射是表现层一致性合同；范围：native/Web schema 映射，不拥有 runtime 规则；未执行业务验收 |
| [控制状态真值](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-state) | [控制状态真值设计](#launcher-state-design) | 控制状态协议：GET state、game/chain 路由与机器 state alias 对齐；快照结果语义另列 | `crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/oasis7_web_launcher_tests.rs`::gui_agent_action_response_includes_state_snapshot_fields 为空态 stop 定义；同候选 state alias/控制结果与服务端快照对账 planned，未执行；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | 不由客户端推断 runtime 收敛；产品独立覆盖 N/A；理由：服务端快照与 alias 对齐是控制状态协议合同；范围：state/既有控制路由/机器 state alias；未执行业务验收 |
| [静态与 API 并存](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-static-api) | [静态与 API 并存设计](#launcher-static-api-design) | 静态缺目录/资源不可用不得改变既有 API 路由语义 | 配对 PRD 客户端与控制面原条款；同候选缺目录/资源失败下读取 state 及既有控制 route 矩阵 planned；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | 不扩展为公开 readiness 或目录解析全覆盖；产品独立覆盖 N/A；理由：静态资源失败不改变 API 是 Web 服务路由隔离合同；范围：静态缺失和 API 并存，不承担目录来源解析；未执行业务验收 |
| [能力发现](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-capability) | [能力发现设计](#launcher-capability-design) | 当前版本/actions/query_targets 是清单真值；payload 提交时校验 | `crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/oasis7_web_launcher_tests.rs`::gui_agent_capabilities_include_expected_actions_and_targets 仅特定 action 与 transfer.status 存在断言；当前响应/版本与真实声明对照 planned，未执行；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | 不声称永久穷尽 action catalog/payload schema；产品独立覆盖 N/A；理由：能力发现只描述当前 operator 动作/查询清单；范围：capabilities 版本/actions/query_targets，非 payload schema；未执行业务验收 |
| [严格动作与响应](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-machine-action) | [严格动作与响应设计](#launcher-machine-action-design) | 机器协议：严格 action/payload、统一字段/分类和 state alias；真实拒绝/错误/当前状态语义另列 | `crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/oasis7_web_launcher_tests.rs` 的 unknown_action、query_chain_disabled、action_response 状态测试定义；完整 strict payload/alias/响应矩阵 planned，未执行；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | 不是世界 Agent、DecisionProvider 或 runtime 写权限；产品独立覆盖 N/A；理由：严格枚举及统一响应是机器接口协议合同；范围：action/payload 分类与响应/state alias；未执行业务验收 |
| [声明查询映射](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-query) | [声明查询映射设计](#launcher-query-design) | 查询适配：仅声明 target/参数与越界拒绝；不可用查询结果语义另列 | `crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/oasis7_web_launcher_tests.rs`::gui_agent_query_chain_disabled_returns_structured_error 仅 chain_disabled/data/error_code/state；declared target/参数及拒绝越界矩阵 planned；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | 不承诺 transfer 当前可用性；产品独立覆盖 N/A；理由：声明 target/参数限制是查询适配边界；范围：声明映射及越界拒绝，非 transfer 可用性；未执行业务验收 |
| [部署安全非主张](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-security) | [部署安全非主张设计](#launcher-security-design) | hosted_public_join 枚举 operator 路由 peer-IP gate 与 wildcard/public 路由边界 | 配对 PRD 部署与安全原条款；同候选枚举 operator-path、peer-IP 正负例和公开路由对照 planned，不以文档边界作为安全通过；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | 不宣称认证/RBAC/TLS/rate limit、loopback-only、公开 readiness/SLA；产品独立覆盖 N/A；理由：peer-IP gate 与非主张是部署/安全专业边界；范围：hosted 枚举 operator 路由，不证明认证或公开 readiness；未执行业务验收 |
| [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | [Web 结果呈现设计](#launcher-result-design) | 初始化/环境/状态/请求失败显示 blocked/error 和恢复下一步，局部成功不代签世界结果；专业来源：[Web 结果呈现](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-result) | 配对 PRD Web 表现条款及本设计接口项 5；同候选 native/Web 失败状态与服务端结果对照，Web 可见结果按 testing-manual S6；planned；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | 不接管 clock、WASM、轮询或世界恢复；未执行业务验收 |
| [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | [L1 慢停止设计](#launcher-l1-design) | 只消费 L1 的 terminal/unknown 如实呈现，本地控制结果不等于世界结果；专业来源：[L1 慢停止](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l1) | 本设计独立项 L1 的真实 HTTP＋可控 child 矩阵；`crates/oasis7_client_launcher/src/app_process.rs` 当前 `CONTROL_PLANE_HTTP_TIMEOUT_MS=1500`，stop 仍走同 `http_json_request`；`crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/oasis7_web_launcher_tests.rs` 两个 stop no-op 测试只保留原错误；planned，Windows active child 独立；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | 不承接停止预算/平台清理算法或完整产品 AC；未执行业务验收 |
| [REQ-ENTRY-LAUNCH-002](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-002) | [L1 慢停止设计](#launcher-l1-design) | 只消费 L1 的 timeout/停止/失败后现有 state 重核；未知不当成功且不自动重发非幂等动作；专业来源：[L1 慢停止](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l1) | 本设计独立项 L1 的真实 HTTP＋可控 child 矩阵；`crates/oasis7_client_launcher/src/app_process.rs` 当前 `CONTROL_PLANE_HTTP_TIMEOUT_MS=1500`，stop 仍走同 `http_json_request`；`crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/oasis7_web_launcher_tests.rs` 两个 stop no-op 测试只保留原错误；planned，Windows active child 独立；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | 完整 authority/session/config/mode 重验归 session-revalidation；不接管恢复存储；未执行业务验收 |
| [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | [L2 服务异常设计](#launcher-l2-design) | L2 只承接主错误/各清理失败如实保留、不把未确认退出或端口释放报成功；不把本地清理当世界结果；专业来源：[L2 服务异常](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l2) | 本设计独立项 L2 的 accept failure、真实托管 child、低 FD/短连接、SIGINT、无 child/退避矩阵；尚无完整执行证据；planned；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | accept-loop/stop-wait 机制仍由专业 owner 验证，不新增进程发现；未执行业务验收 |
| [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | [L3 拒绝 Start设计](#launcher-l3-design) | L3 只承接新 Start 被拒绝时，旧活动进程/配置与当前状态不被新请求伪装为成功启动；专业来源：[L3 拒绝 Start](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l3) | `crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/control_plane/tests.rs`::session_duplicate_start_keeps_running_process_and_evidence 使用 Unix exit 0；本设计 L3 可存活 child 双入口与后续 Stop 矩阵仍 planned；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | 保留句柄机制及双入口全部实现矩阵仍专业验收；不证明新配置已应用；未执行业务验收 |
| [REQ-ENTRY-LAUNCH-002](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-002) | [L3 拒绝 Start设计](#launcher-l3-design) | L3 仅已有可存活活动 child 的重复 Start 被拒绝时，继续消费原会话当前状态，不静默换成新请求上下文；无活动进程的新启动失败仍清除旧证据并按 session-revalidation 重验；专业来源：[L3 拒绝 Start](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l3) | `crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/control_plane/tests.rs`::session_duplicate_start_keeps_running_process_and_evidence 使用 Unix exit 0；本设计 L3 可存活 child 双入口与后续 Stop 矩阵仍 planned；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | 不新增恢复协议，不代签完整 authority/权限/配置/模式重验；未执行业务验收 |
| [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | [L4 非法 payload设计](#launcher-l4-design) | L4 只承接非法请求的真实拒绝原因与当前 state，不将输入或请求受理误报启动/世界成功；专业来源：[L4 非法 payload](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l4) | `crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/oasis7_web_launcher_tests.rs`::gui_agent_action_rejects_unknown_action_with_invalid_request 仅定义 ok/action/error_code/state 断言；本设计 L4 两 route 完整 HTTP 矩阵仍 planned；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | ApiResponse 字段/HTTP 解码/零 spawn 由 L4 专业机制验证；未执行业务验收 |
| [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | [L7 健康语义设计](#launcher-l7-design) | L7 只承接预检不健康/失败不得继续显示启动成功；健康探针与真实 decision/tool-call/玩法分开；专业来源：[L7 健康语义](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-l7) | `scripts/run-launcher-stack.sh` 为实现接点；本设计 L7 离线 HTTP 健康/结构/网络/JSON fixture 为 planned；未审完整脚本矩阵且未运行，不声称测试不存在；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | provider 健康字段/协议/脚本算法不归产品；不证明模型或世界结果；未执行业务验收 |
| [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | [控制状态真值设计](#launcher-state-design) | state 只承接服务端当前快照和受控请求结果如实呈现；本地状态不代签 world outcome；专业来源：[控制状态真值](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-state) | `crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/oasis7_web_launcher_tests.rs`::gui_agent_action_response_includes_state_snapshot_fields 为空态 stop 定义；同候选 state alias/控制结果与服务端快照对账 planned，未执行；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | 不替代 state/alias 协议一致性或真实 authority/session/config 验证；未执行业务验收 |
| [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | [严格动作与响应设计](#launcher-machine-action-design) | action 只承接拒绝/错误及最新 state 的真实结果，机器响应局部成功不代签世界成功；专业来源：[严格动作与响应](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-machine-action) | `crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/oasis7_web_launcher_tests.rs` 的 unknown_action、query_chain_disabled、action_response 状态测试定义；完整 strict payload/alias/响应矩阵 planned，未执行；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | 严格枚举/payload/统一字段属机器专业合同；不是世界 Agent 能力；未执行业务验收 |
| [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | [声明查询映射设计](#launcher-query-design) | query 只承接已声明查询不可用/chain_disabled 的准确错误和当前状态，不以 operator 查询可达宣称世界可用；专业来源：[声明查询映射](game-client-launcher-control-plane-and-machine-interface.prd.md#launcher-query) | `crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/oasis7_web_launcher_tests.rs`::gui_agent_query_chain_disabled_returns_structured_error 仅 chain_disabled/data/error_code/state；declared target/参数及拒绝越界矩阵 planned；[测试手册](../../../testing-manual.md) | 同候选 PR、命令、状态/产物与真实输出；本轮仅静态审读 | target/参数映射与安全边界仍专业；不保证 transfer 可用性；未执行业务验收 |

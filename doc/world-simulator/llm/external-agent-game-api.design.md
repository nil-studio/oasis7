# 外部 Agent Game API 系统设计

- 设计状态：target contract，待实现与独立评审；不是当前能力或发布报告。
- Canonical repository：`eng-cc/oasis7`；审读基线：`03fb58784a65de9f02a43dfeafc3e882bb04e5f2`（已合入产品 PR #4493）。
- Owner role：`agent_engineer`；最后审读：2026-10-10；新增合同 revision：`1`。
- 产品入口：[外部 Runtime 游玩](../../product/agents-world-simulation/external-agent-runtime-play.prd.md)；[玩家与 Runtime 协作](../../product/agents-world-simulation/player-runtime-collaboration.prd.md)。
- 系统总入口：[LLM / external runtime 系统入口](README.md)；写作规范：[系统设计规范](../../engineering/doc-governance/system-design-writing-standard.design.md)。

本文中的新组件、路由、记录和测试场景均为后续实施输入。`accepted` 只表示所属权威已受理，世界效果必须读取既有权威回执；文档合入不代表这些接口已经上线。

### 已实施切片：本机签名读取与持久任务控制 bootstrap

`oasis7_viewer_live --game-api-read` 在现有 `--web-bind` 端口启用普通 HTTP GET，同时保留 WebSocket；默认关闭，只允许 loopback IP 绑定。需要既有 `OASIS7_WORLD_SERVICE_*` 配置及 `agent:<id>` scope，后端仅允许 HTTPS 或 loopback IP。入口不读取世界目录，不使用 Viewer 的读私钥替调用者签名；底层 `RemoteWorldServiceClient::read_actor_view` 将设备签名原样转发，核对服务签名、请求关联、scope、commit 与 projection 一致性。服务断连、拒绝或不可信回复不返回本地缓存。

- `GET /v1/game/info` 公布 `signed_read_bootstrap`、可路由资源、实际读取时检查可用性、`writes_available=false`、`schema_revision`、`openapi_url` 与实际执行的 `limits`。
- `GET /v1/game/openapi.json` 无需主体证明，发布同版本 OpenAPI 3.1 bootstrap 合同；只列上述 discovery 与三个已实施签名读资源，不把目标 context/action/operation 宣称为可用。合同包含签名 header 的 decoded World Service schema、签名 domain、数值编码差异、错误与重试语义、实际大小/深度/并发限制；路径相对当前服务，不发布配置、公私钥或可信世界信息。
- `GET /v1/game/worlds/{world}/agents/{agent}/observation` 返回已授权 AgentCell 的 `data`、`logical_tick` 和 committed projection version `world_binding`；这里是自身状态投影，尚不是完整首局观察。
- 同路径 `capabilities` 只返回权威 projection 中该 Agent 的 capability catalog；实际 World Service 的 agent read 处理器调用 `canonical_agent_service_context` 填充此 context，没有 context 时为 `503 capability_unavailable`，不合成或使用旧 Provider 目录。
- 三个读取资源使用 `X-Oasis7-World-Read-Proof` header：值为 `hex(UTF8 JSON SignedReadRequest<ReadWorldViewRequest>)`。调用者在设备端以现有 `authority::sign_read_request` codec 和 `/v1/world/view` domain 签署配置匹配的 world/scope。没有证明返回 428；重复 header/key、未知字段、篡改、错误 scope、带 token 的 query 或 Origin 均拒绝。固定历史 commit 不支持，避免旧快照代替当前权限检查。
- header 最多 8192 bytes、整个 HTTP header 最多 16384 bytes；JSON 同时消费共享严格解析器；最多 8 个后端读取、响应最多 2 MiB。bootstrap `data` 和 `world_binding` 中整数统一为十进制字符串，顶层 API revision 保持整数。服务签名认证不代签共识最终性。

这是设备签名 bootstrap 实施中间阶段；未实现 opaque token/配对、proof prepare、上下文/行动/持久 Game operation、完整任务协作、普通 Viewer/browser 授权、worker 或首局验收。正式 token 接入仍须在入口验证会话后保留此主体证明和世界授权链路。共享 `oasis7_agent_api::game` 已定义 action request、decimal u64 和操作状态 registry，但 DTO 不授予权限。对应测试为 `game_wire`、`viewer::game_read_api` 和 `world_service::client`；测试通过不表示上述未实施能力已交付。

### 持久任务控制前置

Game Task 创建复用既有签名 World Service submit/lookup 与 Runtime 共识执行链路；Game HTTP bootstrap 继续只开放 GET，`writes_available=false` 不表示底层 World Service 没有任务控制提交能力。创建使用 `game_task_create` 签名 domain（`WorldServicePayloadV1::GameTaskCreate` / JSON kind `game_task_create`），绑定完整世界身份、branch/reorg、Agent、owner binding/generation 和稳定 client operation ID。当前 owner 校验在提交和实际执行时分别完成，queued 请求不能沿用失效 owner 或旧世界 fence。

创建 body 使用 `GameTaskCreateV1`，`api_rev=1`；`owner_generation` 与 `reorg_epoch` 使用 canonical decimal string。调用者使用 `authority::sign_read_request(GAME_TASK_CREATE_DOMAIN, request, owner_private_key)`，以 `correlation::derive_correlation(expected_world, &payload)` 构造 `SubmitIntentRequest`，经 `WorldServicePort::submit` 发送到 `/v1/world/submit`。查询使用同一原 payload 与原 RequestKey 发送到 `/v1/world/lookup`；两条原生路径返回既有签名 World Service 合同，不混用 Game action 的 actor challenge。

任务控制的唯一权威是已提交的 `world_service_results` canonical receipt，沿用现有日志与快照恢复边界。没有第二套 gateway 内存任务表；相同操作标识重放复用原回执，同键不同内容发生关联冲突，同一 Agent 当前 owner/world fence 下的重复创建被拒绝，包括同块多个创建。服务返回 volatile Received 时尚不构成任务持久化或 Game pending；响应丢失后保留原签名 payload/correlation 查询，只有 committed canonical receipt 才能证明任务创建。

`GET /v1/game/worlds/{world}/agents/{agent}/task-control` 通过原设备读签名返回当前 owner 或当前绑定且真实 delegation 有效的 executor 的控制记录及 committed `world_binding`，没有可见任务时返回 `404 task_control_unavailable`。普通 delegate 的 Agent 观察权限不授予任务读取权限。控制记录初始 `suspended`，目标 revision 为零、executor epoch 为零且没有执行方；发布目标后附带 public-only `active_goal_reference`（版本、不透明承诺、准备/发布操作标识和激活 committed height）。读取 bootstrap schema revision 4 还描述可选 executor binding；旧初始记录仍省略这些字段。任务初始 `suspended`；此记录不是完整 TaskSnapshot 或可行动 context，也不把已有 owner chat goal 当作任务目标。

### 已实施切片：owner 目标准备、发布与当前正文回读

这段流使用原生 World Service，不提前开放 Game HTTP 写路由或 token 会话。默认关闭的私有准备与回读端点、operator 密钥配置、单记录幂等和不可枚举的 salted commitment 见[协作合同](../../world-runtime/runtime/player-runtime-collaboration.design.md#6-接口与数据合同)。`oasis7::world_service::private_goal_read` 导出路径、请求/响应及 owner 客户端验证方法。

1. owner 保留原 `SignedGoalPrepareV1`，POST `/v1/world/private-goal/prepare`；服务验证当前 owner/world/history/task，再保存完整密文 proof。同操作重试返回原 `PreparedGoalReceiptV1`，同操作异内容为 409，不重新生成目标引用。该 HTTP 入口要求 operator 同时配置独立私有目录和 mode-0600 的精确 32 字节密钥文件，实际监听 IP 仅限 loopback。
2. 客户端调用 `verify_prepare_response(original, response, configured_service_key)`，再 `publication(original, receipt, publish_operation_id)`；检查原意图、重算私有盐承诺后，使用 `game_goal_publish/v1` 签署 public-only `GameGoalPublishV1`。正文、正文摘要、私有 proof/签名和盐均不进入公开 payload。
3. `WorldServicePayloadV1::GameGoalPublish` 经原 `/v1/world/submit` 提交；响应未知时只查询或重投原 payload/correlation。Received 不代表激活，canonical committed receipt 才推进 task/latest/active goal revision。共识只检验公共 owner 签名和前驱控制，不查询节点私有正文；同块前驱、版本冲突和恢复后的控制链都进行确定性验证。
4. owner 以 `/v1/world/private-goal/current` domain 签署 `SignedReadRequest<CurrentGoalRequest>` 并 POST 同路径。返回带受信服务签名的 `CurrentGoalResponse`，包含当前控制、当前读取 CommitRef、真实历史激活 CommitRef 和完整私有准备记录。`verify_current_response` 验证请求关联、服务信任、owner/history、目标版本、proof 和盐承诺。已激活但正文缺失/损坏返回 503 recovery_required，不能以 active_goal=null 冒充完整目标；只有确实无激活目标才返回 null。

`RemoteWorldServiceClient::prepare_goal` 与 `read_current_goal` 执行上述原证明、请求关联和配置服务签名校验，使用独立禁用代理和重定向的客户端；发送前拒绝弱或非规范服务公钥，只接受 HTTPS 或 literal loopback IP HTTP，响应上限为配置值与 64 KiB 的较小值。明文 loopback HTTP 仅支持所有可监听本机端口的进程均受信的 operator host，不支持不受信的其他本机用户；它不会在发送正文前认证对端，服务签名只验证回复。需要不受信多用户隔离时必须使用发送前验证服务端的 TLS 或等效传输，TLS 入口不得转发可公开访问的明文私有端点。

私有服务在读取前及交付前重验当前控制和分支；owner 失效为 403，操作或版本竞态为 409，恢复问题为 503。query、Origin、Authorization 与转发 header 均拒绝，响应禁止缓存。私有存储需独立备份密钥、验证记录与密文；不声明跨节点私有恢复。该切片交付 owner 目标控制与回读，仍不提供完整 TaskSnapshot、executor/Context、采纳确认或目标驱动的实际决策，也不表示首局/真实 Runtime 实玩已经通过。

<a id="当前实施候选原生任务执行链"></a>
### 已实施切片：原生任务执行链

原生 executor 控制、完整私有 Context 捕获、task-fenced 行动及原结果恢复的交付过程见 [历史实施与验收记录](https://github.com/eng-cc/oasis7/pull/4628#issuecomment-6162407180)；实际模型候选、失败和清理边界见[原生执行证据](../../testing/evidence/external-task-native-execution-2026-10-12/README.md)。本节维护支持范围，不代表部署或完整产品验收。

Owner 使用 `game_executor_control/v1` 签署 `GameExecutorControlV1`，经原 `/v1/world/submit` 提交 Bind/替换/Stop。Bind 必须指向真实当前 delegation，推进 task revision 与 executor epoch，使任务 Active；Stop 推进 fence、清空 binding 并 Suspended。目标发布推进目标/task revision，保留现有 executor binding。全部控制由既有 canonical receipt 链恢复；新变体启用前执行、验证及恢复节点必须同步升级，旧 binary 不能读取新历史。

当前 executor 以当前完整 fence 签署 POST `/v1/world/private-task/context-prepare`；同操作原 nonce 可恢复，异 nonce 冲突。私有记录包含完整 GoalContent/proof、同 pin 的原始 `world-agent-state/v1` AgentState、实际授权目录、Runtime/base binding、随机盐和原准备证明。独立 AEAD 密文保存并 fsync，不截断到旧 Harness 的 512 字节目标摘要。一个 Context 的租期为 `min(catalog.valid_until_tick, checked(base_tick + 16))`；canonical prefix 同样检验原 captured tick 的 16 tick 上限并拒绝溢出，不能靠后续 current tick 延期。超过期限不能获得正文或授权新效果。

活跃任务的恢复证据保留必须覆盖该任务当前完整 fence 下所有已提交原 Context prefix、终止 outcome 与动作结果的实际历史提交材料，包括已消费或已过期 Context；不能只按 Context 当前可写性决定 GC pin。派生 pin 仅保留签名回读所需 snapshot、journal 与 head proof，不授予旧 Context 新写权，也不延长 16 tick 租期；任务／完整 fence 退休后释放对应依赖。原结果已确认但历史材料在既定保留范围外时返回 `HistoryUnavailable` 并保留原身份、停止新副作用；记录头仍存在但引用已压缩同样属于历史不可用，不伪装成可重试服务故障或 `Unknown`。损坏或冲突证据继续失败关闭。正常维护与重启可从当前 hot 权威结果重建依赖，并将已有派生 scope 指向的历史切点作为候选，与当前 hot／cold canonical 结果和完整 fence 逐条重验；scope 本身不证明结果或权限。既有 scope 损坏必须阻止 GC。所有派生 scope 均丢失、且原结果已全部转 cold 时的完整发现与重建尚未由此路径提供，不得宣称该损坏恢复能力已验收。

准备不等于可用。客户端先提交 identity-only `SchedulerOperationV1::TaskProviderPrefix`，等待 canonical committed，再 POST `/v1/world/private-task/context-read`。回应包含原密文记录、实际捕获 CommitRef/receipt 及当前同 pin 的公共 base binding/观察/能力。客户端核对配置服务签名、原意图、完整目标 proof、盐承诺、捕获回执和新鲜 bundle。世界公开历史只存完整 fence、session/turn/request 标识及三项私有随机盐隔域承诺，不保存正文、原私有摘要、盐或完整 Provider request。受信服务签名证明所交付内容，不代替独立共识证书。

当前目标激活、当前完整 fence 下原 Context 与动作恢复所依赖的真实历史 snapshot/journal/head proof，由 canonical 世界状态派生专用 CAS pin scope；普通提交、catchup、checkpoint install 和全量维护保留这些依赖，目标替换或完整任务 fence 退休后释放对应材料，Context 消费或过期本身不触发释放。派生 cache 不构成任务权威，丢失后的重建受前述 hot／已知 scope 发现边界约束，损坏时阻止 GC。已经被旧版本清理且不可恢复的历史依赖返回 `recovery_required`，不能用当前 CommitRef 冒充历史激活证明。

`CognitionIntentV1.task_fence` 将行动绑定当前任务、目标版本和 executor epoch；实际裁决沿用既有能力、预算、MVCC 和 turn 生命周期。一 Agent 同时只有一个活动 turn。新目标、替换或 Stop 退役旧 pending context 并释放匹配 Reserved lease；首次 Bind 也退役旧 Hosted Provider pending work。已管理 Agent 的旧 unfenced cognition/旧 Provider prefix 不得恢复执行。精确已提交请求可找回原回执；它仍需真实当前读授权，不因此授予新行动。过期 Context 的外部 CLI 必须先查询原 prefix/action/outcome：仅在 prefix 已提交、无竞争写原件、当前任务 Active 且完整 fence 一致、当前已验签 tick 严格超过原 `valid_until_tick` 时，用原公共 prefix 身份与当前签名 base 提交固定公共 `context_expired` outcome。等待该原 outcome committed 后才创建新 Context；不得假设新的 Context 自动清理旧活动 turn，不得解密过期正文或沿用过期 grant。已有 action/outcome、未知、pending 或 Stop 按原件恢复/停止规则处理，不生成竞争写。

普通 `oasis7_game_client` 使用 operator 提供的严格设备配置、mode-0600 原始签名文件与独立加密 operation cache。它在网络发送前保存原 prepare nonce/签名与公共 prefix/action/outcome，请求结果未知时只 lookup 或重投原请求；改变同一 Context 的 action 不覆盖原签名。正式游戏 Skill 为 [oasis7-play](../../../skills/oasis7-play/SKILL.md)，CLI 提供 `native.move_agent` 三个 i64 厘米坐标、原生转运、内置建厂/排产及固定 provider outcomes。完整 catalog 的任意 action CLI、配对/token、问答/审批/报告、完整协作 TaskSnapshot、Hosted 旧执行器目标接入，以及 Codex/OpenClaw 两 Runtime 完整自主首局仍在后续范围。Game HTTP bootstrap 仍 `writes_available=false`；原生私有路径不冒充 `/v1/game` 写路由。

### 共享原生客户端与 JSON 观察 profile

`oasis7_game_client` 包同时提供共享 Rust 客户端和同名 CLI；构建入口为 `cargo build -p oasis7_game_client --bin oasis7_game_client`，设备配置、原操作缓存和命令参数保持兼容。公共 Action/报价与纯 cognition 合同由 `oasis7_agent_api` 唯一定义；World Service wire/纯验证由 `oasis7_client_api` 唯一定义；主应用仅保留兼容导出和权威执行。SDK 不依赖主应用、World/Viewer 或模型 SDK。API、存储和使用边界见 [SDK](../../../crates/oasis7_game_client/README.md)。Trace refs: O7-REQ-21f1c7b1-8be0-438e-8f92-d4d1b34be0e6；O7-REQ-70188c23-ddbf-49ca-8a70-3fa05c152ba5。

原生 SDK 的公共观察使用显式 POST `/v1/world/view-json`。它与 `/v1/world/view` 复用 `ReadWorldViewRequest`、pin/minimum/fixed commit 和主体授权逻辑，但以自身路径为读签名及回应签名 domain；服务在授权投影转为 JSON 后签署 `ReadWorldViewResponse<serde_json::Value>`。旧路径继续签署原类型化投影，两者不能互换签名。新路径没有额外权限，public 及 executor 不因此取得 owner 私有目标；未知执行证据 scope、错误世界/切点、主体不符或篡改响应均拒绝。JSON 中的完整投影参与签名，客户端不能先裁掉未知字段再验签。服务签名仍只证明交付内容，不证明独立共识最终性。

内置模型消费使用受信任 launcher BYOK 会话入口：认证 owner、绑定合法 GameClient executor，再由 `oasis7_agent_harness` 通过同一 SDK 执行受权 task/context→模型→严格工具→原操作回读循环。模型连接本身不授予世界权限，缺少受信任绑定时拒绝启动；不再提供 node-process 环境 Key worker 入口。世界时钟、任务/目标权威、executor fence、活动 turn、资源和收据仍归 Runtime。独立控制轮询发现 Stop、目标/epoch 变化、过期或超时后丢弃迟到结果，未知原请求不换身份。预算在物理模型调用前加密持久化预留，重启不清零，同 run 改配置/提高上限拒绝；本地存储不提供硬件防回滚保证。真实模型试玩仍待独立授权与验证，脚本测试不代签。库边界见 [Harness](../../../crates/oasis7_agent_harness/README.md)。Trace refs: O7-REQ-ad3ea59c-596c-4c50-8da8-fe47608c6f01；O7-REQ-d452396d-291d-41a5-a912-6d615b58c29e。

新 profile 支持观察、工业/能力查询、报价、移动、转运、内置建厂/排产、有界等待世界 tick 与明确放弃，不支持的工具硬拒绝。旧 Provider 的模块、聊天、私有记忆、continuation 和 portable/WASM 消费者仍保留明确旧 profile；不能同时为同一 Agent 执行。显式绑定任务后旧 unfenced prefix/行动由既有 Runtime 围栏拒绝。新 profile 不宣称全部内置能力已迁移；完整消费者迁移、真实模型/首局、性能和部署验收仍由 [#4726](https://github.com/eng-cc/oasis7/issues/4726) 承接。

<a id="当前实施候选原生物流执行"></a>

### 已实施切片：原生物流执行

普通客户端增加 `industry` 授权观察与 `native.transfer_material`：只接收账本、材料、正 i64 数量、非负 i64 公里距离、标准/紧急优先级及有序线路参数；requester 固定为设备配置 Agent，不接收原始 Action、授权、报价或配方计划。转运和移动共用同一个 Context 的原签名缓存；同 Context 改变动作种类或任一业务字段必须冲突，未知原结果只能核对或重投原请求。

`capabilities` 分开描述客户端支持的原生提案、实际模块命令 catalog 和当前 Agent 的 native action/object electricity grants。World Service 授权 Agent 投影返回该 Agent 的委托预算读模型；generic projection 和其他 Agent 的 grants 不进入该字段。revoked、expired、not_yet_valid、exhausted 与 current 是读取时状态，不代替 Runtime 的最终 admission。无 native grant 不表示已获权，signer delegation 也不替代业务预算授权。

`industry` 复用已经授权的公共物料账本、线路、工厂和 transit 投影，每个集合最多显示 128 项，并报告总量与截断、真实读取版本和 tick。它不提供签名转运报价，不能把读取库存等同于预留容量或物料。canonical cognition 可能只创建在途转运；实际成功必须按原行动回执与后续目标账本到账核对。[历史实施与验收记录](https://github.com/eng-cc/oasis7/pull/4634#issuecomment-6162407791) 与[物流证据](../../testing/evidence/native-task-logistics-2026-10-12/README.md)分别承载交付过程和实际试玩候选。配对、完整 Game HTTP 写路由、完整首局及客户端自动 stale-context abort 刷新缺口继续保留；部署暂停。

<a id="当前实施候选原生权威配方报价与排产"></a>
### 已实施切片：原生权威配方报价与排产

普通客户端的 `quote-schedule` / `schedule` 只接收当前 Context、工厂、规范 built-in recipe ID 和正 u32 批次。共享目录提供十一种内置配方的完整计划；native grant-backed `ScheduleRecipe` admission 按 ID 与批次重算并比较完整计划，预算使用规范电费。trusted 无 native grant 与模块生成的自定义计划保留既有路径，历史回执不按当前目录重算。

签名 POST `/v1/world/private-task/recipe-quote` 同时绑定原 Context 读取证明、当前 fence、主体与预期 base。服务在当前 canonical pin 的私有 clone 上复用实际授权和生产准入，返回规范 action、电费扣除预览、有效输入/输出账本、材料、耗时或拒绝原因；交付前再验当前 pin 与授权。报价不步进世界、不预留库存、不扣预算，电费账本预览不表示电池续航变化。

首次 `schedule` 使用已验证报价的 action 和同 pin 公共 context 保存签名原行动，再提交 cognition。已有 Action cache 时先核对工厂/配方/批次并查询原请求，Unknown 不重新报价或替换请求。`industry` 增加有界 pending recipe jobs 观察；`RecipeStarted` 仅证明排产，真实首产物须核对关联的 `RecipeCompleted` 和输出账本变化。[历史实施与验收记录](https://github.com/eng-cc/oasis7/pull/4650#issuecomment-6162408442)；[首轮证据](../../testing/evidence/native-task-production-2026-10-13/README.md)与[有界复测](../../testing/evidence/native-task-production-2026-10-13/bounded-retry/README.md)保留各自候选、预算失败和原结果。这不证明完整 starter chain、稳定生产、交付、两 Runtime parity 或部署；部署暂停。

<a id="当前实施候选建厂到首批生产"></a>
### 已实施切片：建厂到首批生产

本批从已有 claimed Agent、建设材料和显式世界授权开始，补齐没有工厂到建厂、转运投料、首批内置配方产物的连续链路。普通客户端增加 `quote-build` / `build`，只接收 Context、site、factory 与规范 kind ID；builder 固定为配置 Agent。共享目录提供 Smelter/Assembler 的完整内置建设规格。native grant-backed `BuildFactory` 按可信建设电力 profile 的 kind 重算完整规格，预算使用当前建设电力义务，不以工厂运行耗电或 WASM 最低门槛替代。trusted 无 native grant 和历史回放保留既有语义。

签名 POST `/v1/world/private-task/factory-quote` 绑定主体、原 Context 证明、当前 fence 和预期 base，在 canonical pin 的私有 clone 上复用真实建设准入，返回规范 action、材料账本、耗时、site/location revision、电力义务及拒绝原因；交付前再核对 pin 与授权。报价不预留材料或扣预算。对已验证当前 executor/Context 的对象选择缺少匹配授权，可返回绑定原请求和当前切点的签名只读拒绝；它不授予 action、不终结 Context，也不能由通用403推断。分类、同 batch 工具截断与有界重新决策消费[共享客户端失败合同](agent-game-client-boundary.design.md#des-agc-004)，未知内部错误和真实撤权仍停止。首次提交先保存原签名；已有缓存先核对完整业务选择并恢复原结果，Unknown 不替换请求。

`industry` 增加有界 pending build、建设场地、Agent 位置和建设电力授权观察。新增投影只交付该 Agent 的建设和位置范围，场地不泄露其他主体 owner/allowlist。既有公共 anchor 与工厂电力观察边界沿用原合同。建设开始须关联唯一完成事件，再核对投料到账和真实首产物；[历史实施与验收记录](https://github.com/eng-cc/oasis7/pull/4662#issuecomment-6162409087)；[首轮证据](../../testing/evidence/native-task-starter-chain-2026-10-13/README.md)与[最后有界复测](../../testing/evidence/native-task-starter-chain-2026-10-13/bounded-retry/README.md)分别保存 inconclusive 和 targeted 结果，不以合入源冒充实际试玩源。身份注册、配对/token、多工厂完整 starter chain、稳定生产与两 Runtime 完整首局仍未由该切片证明；部署暂停。

## 1. 问题、目标与非目标

把“游戏向 Provider 索取决策”与“外部 Runtime 主动玩游戏”分开。后者必须有可独立调用的观察、能力、行动、任务和结果接口，而不是把旧 `/v1/provider/*` 换个名字。首期复用既有 Runtime 行动校验与持久回执，不让 Viewer、模型或 API 服务器成为新的世界写入者。

目标：普通 HTTP 客户端可以完成授权、发现首局能力、提交一次有效意图并在响应丢失后找回同一结果；Web、桌面及外部 Runtime 共享业务语义，但不共享默认写权限。非目标：本 PR 实现服务、设计新共识、创建模型额度桥、立即发布任意 Runtime 适配或另建通用任务平台。

## 2. 上游约束与相关角色

Agent owner 负责协议与能力投影；Runtime owner 负责最终裁决、授权与持久化；Viewer owner 消费相同的只读状态；QA 负责普通客户端和真实 Runtime 的分层证据。具体协作状态、适配器与可视化分别消费配套系统设计，不能复制第二份状态机。

### 2.1 需求承接与分配表

| 上游条款 | 具体 obligation 与适用条件 | 本设计条款 | 外部 owner / dependency | 排除与未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-d30925fe-d798-4056-9641-062255931a61；[AC-EXT-002](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-002) | 合法绑定、身份隔离与费用路由不能授予游戏权力 | [api-auth](#api-auth) | agent_engineer / runtime_engineer | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-aad7b341-83b1-4402-8ebc-8540db36ccfb；[AC-EXT-003](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-003) | 首局能力目录、参数和实际执行一致 | [api-context](#api-context) | agent_engineer / runtime_engineer | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-a27895ed-2ca4-45cc-901a-466befd02cf7；[AC-EXT-005](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-005) | 成功、拒绝、未知结果可恢复且不重复世界效果 | [api-actions](#api-actions) | agent_engineer / runtime_engineer | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-7283b901-ea06-49e3-956d-2bad46b6c89d；[AC-EXT-010](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-010) | 两个真实 Runtime 分别完成首局与组合恢复 | [api-composition](#api-composition) | agent_engineer / runtime_engineer | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-8120dffe-17cc-458a-9c70-8121217e6f7b；[AC-EXT-012](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-012) | 普通客户端不依赖旧 Bridge 完成完整协议闭环 | [api-wire](#api-wire) | agent_engineer / runtime_engineer | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-7d6cef7f-9419-49ed-b445-663382383458；[AC-EXT-013](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-013) | 鉴权、越权、限流与授权撤销在服务端生效 | [api-auth](#api-auth) | agent_engineer / runtime_engineer | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-68ffc843-8679-4ad9-8cef-10a239a29c73；[AC-EXT-014](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-014) | 关闭旧 Provider Bridge 后新主路径仍可运行 | [api-cutover](#api-cutover) | agent_engineer / runtime_engineer | 不单独证明真实模型、客户端或公开发行就绪 |

## 3. 当前状态、目标状态与差距

| 对象 | 基线事实及定位 | 本设计目标 / 差距 |
| --- | --- | --- |
| 共享 DTO | `crates/oasis7_agent_api/src/lib.rs` 已导出观察、能力、目标、反馈和 RuntimeBinding，且明确不执行世界 | 新增 `game` 命名空间的外部协议 DTO，复用已有强类型；不能把 crate 存在当网络服务存在 |
| 旧 Provider | `doc/world-simulator/llm/README.md` 记录 decision-context/feedback-context 回调方向 | 新 `/v1/game` 是主动调用入口，不接收旧回调 DTO 冒充新请求 |
| 世界运行 | Viewer 手册明确 `oasis7_chain_runtime` 管理 execution-world，Viewer 是 client/observer | API 经受信 RuntimePort 调用当前世界权威，不能写 snapshot/journal 文件来执行行动 |
| 认知 | `agent-cognition-lifecycle.design.md` 已定义 Harness identity、AgentDecisionEnvelope、MVCC、journal 目标 | API 只作外部适配；其生产恢复边界仍需实际配套实现，不宣称已完全具备 |

## 4. 边界与结构

<a id="api-composition"></a>
### 4.1 最小部署与模块分工

```text
玩家 Web / 桌面 ---- authenticated owner session ----┐
                                                   v
外部 Runtime + Skill -- outbound HTTPS --> GameApiIngress
                                            |     |     |
                              CollaborationPort  |  WorldReadPort
                                            |  DecisionAdmissionPort
                                            v     v     v
                              当前世界权威 Runtime + 持久 journal/checkpoint
```

箭头表示调用，不表示上游获得世界写权。`GameApiIngress` 是拟新增的路由组件，首期挂载于现有 `oasis7_viewer_live` 网络服务，并可在不启用 GUI/内置模型/旧 Provider 的情况下运行。渲染页面断开不销毁服务。它不内嵌 node 或共识；服务边界继续沿用当前 chain submit/status 与可信 Runtime adapter，必要的协议扩展须进入 Runtime 端。

`oasis7_agent_api` 拥有新 machine-facing DTO 与 schema；`oasis7_client_api` 消费公共标识/投影，禁止反向循环依赖。HTTP router、credential verifier、schema validation、限流和 serialization 位于 native host；World::step 不等待 HTTP 或模型。协作服务共用同一进程及 RuntimePort，不要求 Kafka、Redis 或另一套控制数据库。

- 协作存储与写入顺序：[协作权威设计](../../world-runtime/runtime/player-runtime-collaboration.design.md)。
- 实际 Runtime 生命周期与 Skill：[外部适配设计](external-agent-runtime-adapter.design.md)。
- 面向用户的世界投影：[Web/native 世界视图设计](../viewer/external-agent-world-view.design.md)。

### 4.2 发布基线补充：复用已合入的 World Service

产品审读基线保持 `03fb58784a65de9f02a43dfeafc3e882bb04e5f2`。发布前核对 `7039bc376882ab8f84f88ca014f6d7ce642539a8` 时，main 已增加独立、认证且不要求应用挂载世界目录的 World Service；以下是对本设计物理接入的收敛，不是再造一套基础设施。

- 当前 `crates/oasis7_runtime/src/service_admission/wire.rs` 已定义 POST `/v1/world/describe`、`submit`、`lookup`、`view`、`changes`，并使用 `SignedReadRequest<T>`、`SignedServiceResponse<T>`、`WorldServicePayloadV1`。`client` / `verified_view` 是新 Game API 的底层调用与验证位置；不能改为读共享 snapshot 文件或由 Viewer 本地 World 代替。
- `GameApiIngress` 是面向玩家和执行器的 `/v1/game` 资源适配层，底层 WorldReadPort 使用 signed view/changes，DecisionAdmissionPort 使用 signed Cognition/Scheduler 的 submit/lookup。已有服务回复签名只证明受信服务断言，不等于共识最终性；继续核对 CommitRef、WorldBinding、receipt 与请求关联。
- 已有 `AgentSignerDelegationChangeV1` 的 owner 签名、Agent identity generation、delegation generation、nonce 和 delegate public key 不得被新 opaque token 取代。协作设计的 executor_epoch 是更窄的任务执行实例 fence，验证组合为 `(agent_identity_generation, delegation_generation, task_executor_epoch)`；任务 fence 只能收窄已有委托，不能自行授予或恢复 world signer 权限。
- 当前 payload 覆盖 GameplayJson、Cognition、FeedbackAck、Delegation、Scheduler，不包含本文的完整任务/目标/问答/审批写入。实施时为这些控制命令增加版本化的 typed payload 与世界侧校验，不把自由 JSON 当现成管理权限。服务器不支持所需变体时返回 capability_unavailable，而非绕过 World Service 直写文件。

**主体签名接续**：首期默认保留设备端的 owner/delegate 私钥，不把全局 owner 私钥交给 Game API。opaque token 只认证入口会话；需要 World Service 主体证明的读写由本机受限 signer（玩家端或 executor 工具代理）签署确切的现有 operation domain 和 canonical payload。Game API 不能替客户端改写已签 payload，也不能借服务响应签名密钥冒充 owner。

为使普通 HTTP 客户端与两个 Runtime 都可执行该步骤，增加目标 `POST /v1/game/proofs/prepare`：输入固定 allowlist 中的业务路由、原 client_operation_id、业务正文和已有 context；返回 `challenge_id / operation_domain / canonical_payload / payload_digest / signer_role / binding / expires_at`。它只准备提案、不提交行动或授予权限。调用方使用固定版本 codec 重算 digest，并检查原业务意图、world/Agent/task、目标、授权范围和 signer_role 后本地签名；在原业务路由附 `actor_proof={challenge_id,subject_public_key,signature_hex}` 继续。入口缺少证明时返回 `428 actor_proof_required`，不伪造成功。GameRequest 内部可以自动完成纯协议签名往返，但 owner 审批等高影响决定仍须既有明确确认，不让模型自行批准。

业务幂等 digest 不包含 token、transport_attempt 或 actor_proof；同一个操作在 prepare 时固定其待签 canonical payload 和底层 correlation，重试返回原 challenge/operation，不能通过重新签名重置 nonce、预算或重复 World Service submit。状态漂移导致原请求不可用时先 lookup 原结果，再明确生成新业务操作与 context。pairing 的 owner 批准仅是开始世界委托注册，只有对应 Delegation 提交和受权回读成立后才进入 execution_ready。设备配对通过、API token 有效或 proof prepare 成功都不代签此点。

复用验证入口为 `crates/oasis7/tests/world-service-conformance.manual.md` 与 `crates/oasis7/scripts/world-service-conformance.sh`。这些现有测试可证明其实际覆盖的基础服务合同；不代签新的 Game API、协作控制或真实外部 Runtime 体验。私有正文仍由协作设计所述 Runtime 受控存储保存，应用不获取世界存储挂载权。

## 5. 关键运行流程

<a id="api-auth"></a>
### 5.1 连接、配对和最小授权

玩家继续使用现有 hosted account / player session / device proof，控制权取自 Runtime 的实际 Agent 所有权与有效委托；不另发“公开用户 ID 即 bearer”凭证。

外部执行器首次连接采用显式设备配对：生成本地设备公钥，申请一次性 pairing；玩家在可信 Web/桌面会话核对该设备、世界、Agent、有效期与 requested scopes 并批准。配对码只是定位待批申请，不是最终权限。服务验证设备私钥持有证明后，交付短期 opaque access token 与轮换 refresh credential；refresh 权限始终不超过原配对范围。

默认 executor 只有 `world:observe / capability:read / context:read / action:submit / receipt:read / task:read / task:report / message:write / approval:request`；没有 `goal:write / approval:decide / executor:replace / grant:write`。owner 的任务写入和审批使用独立玩家会话及适用强认证。模型 API key、支付路由、Skill 文本和自报 Agent ID 均不能兑换游戏身份。

认证与世界授权分层：入口验证 token/device/session，Runtime 再验证 world、Agent、task、当前 executor epoch、goal revision、grant revision 与剩余额度。撤销或替换执行方不依赖 token 自然到期。无权读取返回不泄露对象存在性的错误；先校验权限，再返回版本冲突或最新值。

<a id="api-context"></a>
### 5.2 一次可执行的决策上下文

1. 执行器完成现有 World Service 委托的签名注册及回读后，读取任务 snapshot，取得当前 executor fence、完整激活目标正文及其 revision/digest 和授权；正文必须通过[协作快照合同](../../world-runtime/runtime/player-runtime-collaboration.design.md#6-接口与数据合同)验证，缺失/不一致阻止 context 创建，陈旧版本先重新同步。
2. `POST .../agents/{agent}/contexts` 申请上下文，host-side Harness 分配已有 session/turn/request identity，记录同一已提交 WorldBinding、目标/授权版本及有效期，返回不可猜测 `context_id`。
3. 能力目录从 Runtime capability registry 和当前主体授权投影。每项包含稳定 action_ref、参数 schema、是否有副作用、前置与不可用原因；过滤目录不能取代提交时再校验。
4. 客户端可查询更多合法信息；副作用提案携带 context_id 和稳定 client_operation_id。服务端回读上下文，填充而非相信客户端提供的 authority/capability/MVCC 字段，转换为既有 AgentDecisionEnvelope。
5. 一个 Agent 至多一个活动 cognition turn；不同 Agent 可并行。新 context 不得在旧未知提交未核对时自动开启第二个副作用请求。只读 query 和结果查询不因等待模型而阻塞。

context 创建也必须携带 client_operation_id；超时重试返回原 context 或明确已过期，不能隐式再分配第二个 turn。除认证配对/兑换使用专用一次性 nonce 外，所有变更类 POST/PUT 都适用稳定操作身份和同键异内容拒绝。

只读查询后需要等待、提问或放弃轮次时，执行器用 outcomes 显式收口；成功 action 结果、有效 outcomes 或明确失效才释放活动 turn。wait 的唤醒提案仍经既有 Runtime continuation 校验，不能由 worker 私自推进世界；无回应的轮次到期按 Runtime 规则失效并保留原操作查询。

上下文过期、分支变化、授权撤销、目标版本不符时返回可区分的 `context_stale / authority_changed / goal_changed / recovery_required`。新 Context 不能仅改写旧提案上的版本号；必须使执行器重读并重新作出决策。

<a id="api-actions"></a>
### 5.3 提交、超时与唯一世界效果

幂等键作用域为 `(world_id, agent_id, authenticated_actor_id, operation_kind, client_operation_id)`，不包含可变化的 goal revision 或 executor epoch，以免重连后同一操作变成新键。服务器用版本化 canonical body 计算内容 digest；同键同内容返回既有 operation，同键异内容 `409 idempotency_conflict`，不覆盖历史。

先在权威 ingress journal 持久记录操作关联，再进入既有 admission/commit 流程。`202 pending` 只表示操作已登记；业务拒绝也生成可查询 disposition。仅匹配的 Runtime durable receipt 和有效 finality binding 允许 `committed`。投递超时返回原 operation_id 或由客户端按原 client_operation_id 查询，不能自动生成新键重做。

如果 admission 已成功而 HTTP 回复丢失，重试必须先找到原操作；授权改变后仍先验证当前主体是否允许读取该历史，不能借幂等回放绕过读权限。资源扣减、grant 消费及 effect/receipt 在 Runtime 的同一提交边界处理；入口缓存仅是加速，不能作为去重唯一来源。不同 client_operation_id 的重复业务意图仍需世界规则限制，本设计不声称任意语义操作全局 exactly-once。

## 6. 接口与数据合同

<a id="api-wire"></a>
### 6.1 统一路由与身份

以下是 revision 1 的**目标合同**，不是现有端点。路径前缀 `/v1/game`；`W=/worlds/{world_id}`、`T=W/tasks/{task_id}`。服务器发布同版本 schema / OpenAPI；所有 owner 与 executor 使用相同资源，但动作权限不同。

| 方法与路径 | 输入 / 输出 | 语义及主责 |
| --- | --- | --- |
| GET `/info` | api major、schema revision、受支持能力与 LimitsV1 | 只描述部署合同，不宣称真实 Runtime 组合已验收 |
| POST `/proofs/prepare` | 固定业务路由、operation ID、正文/context；待签 challenge | 主体证明准备；无世界副作用，签名后在原路由附 actor_proof 重试 |
| POST `/pairings`；POST `/pairings/{id}/approve`；POST `/pairings/{id}/exchange` | 设备公钥、一次性挑战；owner 决定；设备 proof | Auth 主责；批准与兑换分离，返回 token 只发给验证过的设备 |
| POST `/tokens/refresh`；POST `/tokens/revoke` | 轮换 refresh proof / token handle | 撤销可读回，旧 refresh 重放整族失效；不返回原始 secret |
| GET `W/agents`；GET `W/agents/{A}/observation` | 授权过滤的实体/观察及 WorldBinding | ReadPort；不允许 query 参数扩大 audience |
| GET `W/agents/{A}/capabilities`；POST `W/agents/{A}/queries` | 条件化能力目录 / schema-validated query | 只读；查询不产生 world effect |
| POST `W/agents/{A}/contexts` | client_operation_id、task_id、executor fence、expected goal revision | Harness 生成 ContextHandle；消费单 Agent 活动 turn 约束 |
| POST `W/agents/{A}/contexts/{C}/outcomes` | client_operation_id、kind=wait/blocked/abandoned、reason、wake_proposal? | 无动作轮次收口；校验当前 context 后关闭或走既有 Runtime continuation admission，不伪造世界效果 |
| POST `W/agents/{A}/actions`；GET `W/operations/{id}` | context_id、client_operation_id、action_ref、arguments；OperationView | AdmissionPort；待决、结果待恢复确认、拒绝、失败和结算分开 |
| GET `W/operations/by-client-id` | actor-scoped client_operation_id + operation_kind + agent_id | 不确定响应时找回原操作；不能读取他人操作 |
| POST `W/tasks`；GET `T/snapshot` | agent_id / TaskSnapshotV1（含受权 active_goal 正文） | 协作设计拥有任务唯一性、正文验证与一致切点；不能只返回 revision/digest |
| PUT `T/goal`；POST `T/goal-receipts` | expected revision / executor read-adopt report | 保存、激活、读取、采纳、用于决策五类证据分开 |
| POST `T/reports`；POST `T/questions`；POST `T/questions/{Q}/answers` | 稳定消息 ID、关联版本、source 与有界正文 | 协作主责；普通文本不授权 |
| POST `T/approval-requests`；POST `T/approval-requests/{P}/decisions` | 精确方案与范围 / owner 明确决定 | 只申请不自行批准；世界执行仍重验 |
| POST `T/executor-bindings`；POST `T/heartbeats`；POST `T/stop` | 当前绑定 / 诊断心跳 / 撤销后续执行 | 协作主责；状态不是取消既有世界效果 |
| GET `T/events` | cursor、limit、wait_ms | 至少支持有界增量轮询；过期游标必须显式重同步 |
| POST `W/views/query`；GET `W/views/entities/{E}`；GET `W/views/search`；GET `W/views/events` | 视点、尺度、搜索、授权投影与游标 | 世界视图设计主责，不能复用 executor 私有观察给普通 Viewer |

### 6.2 核心 DTO

```text
ContextHandleV1 = {api_revision, context_id, world_binding: RuntimeBindingV1,
  task_id, agent_id, executor_id, executor_epoch, goal_revision,
  grant_revision, capability_digest, request_digest, valid_until_tick,
  observation, capabilities}
ActionSubmissionV1 = {api_revision, client_operation_id, task_id,
  executor_epoch, context_id, action_ref, arguments, actor_proof}
OperationViewV1 = {api_revision, operation_id, client_operation_id,
  status: pending|recovery_required|committed|rejected|failed, disposition_reason,
  receipt_ref?, world_binding, observed_at,
  retry_advice: lookup_original|wait|none, recovery?: OperationRecoveryV1}
OperationRecoveryV1 = {reason, last_known_status: registered|pending,
  reconciliation: queued|running|blocked, last_attempt_at?, next_lookup_after_ms?}
ErrorV1 = {code, message, request_trace_id, operation_id?, retryable,
  retry_after_ms?, refresh: none|context|task_snapshot|reauth}
```

`operation_id`、world/Agent/task/executor ID 是不透明字符串；u64 revision/tick/epoch 在 JSON 使用十进制字符串，避免 JS 精度损失。严格校验请求的未知字段、类型、深度和重复 JSON key；响应可以增添可选字段，未知状态必须阻塞而非默认成功。HTTP 401/403 不作为业务失败的自动重放信号；409 表示版本/幂等冲突，410 表示已退出的协议版本，413/429/503 分别为大小、配额、服务不可用。

完整 WorldBinding/finality、ActionCatalogEntry、请求 digest、反馈和 receipt lineage 复用共享 crate 与 Runtime 合同，不在此另定哈希算法。客户端不能凭 receipt_ref 字符串自证提交成功，必须权威回读。

### 6.3 操作状态、回查和恢复协议

| OperationView 状态 | 可依赖事实 | 后续行为 |
| --- | --- | --- |
| pending | 权威登记/受理已确认，仍沿正常处理路径等待结果 | retry_advice=wait；按原 operation 查询，不新建重复行动 |
| recovery_required | 已有操作关联，但因恢复/回读缺口无法证明其执行结果；不是终态失败 | retry_advice=lookup_original，recovery 必填；保留原身份和最后已验证 binding，暂停相关新副作用并对账 |
| committed | 原请求的持久 receipt 与有效 finality binding 已核对 | retry_advice=none；回放同一结果，不再执行 |
| rejected / failed | 权威 disposition 已确认拒绝或终态失败，不是由超时推断 | retry_advice=none；展示真实原因及已产生的费用/预留等结果，不自动换键重做 |

`GET operations/{id}` 和 `by-client-id` 在当前读取获权且操作记录可读时，以 `200 OperationViewV1` 返回 recovery_required；不得伪装成 404、failed 或无区别的 pending。`world_binding` 在该状态是最后已验证的关联，不宣称当前最终性。若连操作记录也无法受信回读，返回 503/明确 error 并保留客户端原键，而非制造终态；401/403 仍先执行且不得泄露操作存在性。传输 ErrorV1 的 retryable 仅是对应请求的恢复建议，绝不授权重建世界意图。

`pending -> recovery_required` 必须留下持久原因与最后已知关联；恢复服务重查原 request/correlation/receipt 和有效世界分支后，才可 `recovery_required -> pending`（确认仍受理待决）或进入上述有证据终态。轮询时间、重试次数或用户停止不能把未知结果改成失败或取消。recovery.reason 有界且脱敏；reconciliation 与 last_attempt_at 表达恢复进展，阻塞时可升级人工诊断但不授予重放权限。终态历史与 reorg/finality 变化仍按既有世界合同处理，不由本地计时器反向改写。

同键重试首先查询既有 operation，返回相同的 recovery 状态或已经核实的新状态；禁止更换 client_operation_id、重新签 nonce、开启新的副作用 context 或靠 outcome=abandoned 遗忘未知操作。只有恢复服务核实底层仍持久保有原身份去重、允许原请求重投时，才可重送原 outbox 内容；否则只回查并保留阻塞。Worker、Web 和 native 将它显示为“结果待恢复确认”，不复用表示普通排队的 pending 或原模型调用的 run_unknown；只读查询、任务沟通和原结果核对仍可按当前权限进行。

## 7. 状态、事务与持久化

API 入口无独立世界事务。token/配对的凭据记录由可信身份服务保存；task/goal/grant/fence 由协作权威保存；context/operation/dedup/disposition 由 Runtime/Harness 持久边界保存；渲染缓存随时可重建。各自的状态转换与崩溃恢复不可互相代签。

操作登记与 Runtime admission 跨进程时使用 durable ingress + outbox，不声称网络原子提交：outbox 可能重复，Runtime 按稳定 operation/request identity 去重。恢复扫描 registered/pending/recovery_required 操作；无法证明是否执行时，持久保留 recovery_required 和原 request/correlation，再按 §6.3 查询或在已核实的底层持久去重保护下重投原请求，不盲目再次发送。终态去重索引至少与权威操作历史同寿命；大正文可压缩，但删除键后不得把迟到请求作为新操作重新执行。

## 8. 部署、安全与运行约束

远程必须 HTTPS，CORS 使用部署级 allowlist，禁止携带凭据的 wildcard origin；Cookie 模式校验 CSRF/Origin。外部机器 token 不进入 URL、页面 JS、聊天或诊断包。循环地址也须认证，不能仅凭 loopback 或 `Origin:null` 授权；开发认证豁免不得进入默认发行配置。

入口与 Runtime 通道只接受固定配置的目标与受信调用者，拒绝客户端提供 file path、任意 callback URL 或费用路由作为后端目的地。授权/能力 registry 不可用时停止新写入，保留许可范围内的已确认结果读取。第三方 provider 网关的密钥仅在用户实际 runtime host，不上传到 Game API。

## 9. 质量与容量

以下为拟实施的可配置初始上限，不是当前测量值：JSON body 256 KiB、嵌套深度 16；goal 4 KiB、message/report 各 8 KiB；列表默认 100、最大 200；单响应 2 MiB；每身份并发读取 8、每 Agent 活动 cognition turn 1。`LimitsV1` 发布实际值，超限返回结构化错误，不截断目标、授权或回执。

HTTP 普通请求客户端 deadline 初值 10 秒；事件 long-poll 服务端最多 25 秒、客户端 30 秒。读轮询可退避，写重试必须先核对幂等操作；这些 wall-clock 预算不决定世界 logical tick/finality。容量验收测 P95、错误率、队列长度和 world tick 不被 I/O 阻塞，不为尚无基线的公网写入虚构 SLA。

## 10. 兼容、迁移与回滚

<a id="api-cutover"></a>
新路径只使用 Game API revision 1，不接收旧 Provider wire 作为 alias。实施顺序：接通权威读与鉴权 → context/catalog/operation → 协作与 worker → Viewer/native → 两 Runtime 首局。上线前先验证唯一 writer、持久 replay 与权限负例，随后把推荐 Skill 改指向新路径。

旧 Bridge 允许直接删除，不强制双写或永久兼容层；删除前列清仍在使用的 builtin、mock、运维与独立消费者并各自处置。撤回新入口是禁止新提交、继续查询既有 operation，不是把旧 bridge 静默接管当前 task；新旧存储不能无检查相互降级。schema 不兼容时旧服务拒绝打开新快照，恢复需采用明确的备份/转换方案，世界已提交历史不回滚。

本次在尚未发布的 revision 1 草案中补齐 recovery_required；OpenAPI、共享 DTO、Worker 和 Viewer 消费者必须同时识别，旧草案消费者不能把新状态降格为 pending/failed。

## 11. 验证设计与可追溯性

普通 CI 补齐 parser、schema、授权、目录执行遍历、outbox/幂等及故障注入；只有真实模型行为、真实 Web/native 与实际部署恢复无法由普通 CI 证明的部分使用专项验证。文档检查不能代签功能完成。

### 11.1 验证映射表

| 上游条款 | 本设计条款 | 独立 obligation 与适用条件 | 准确验证方法与场景 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-d30925fe-d798-4056-9641-062255931a61；[AC-EXT-002](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-002) | [api-auth](#api-auth) | 合法绑定、身份隔离与费用路由不能授予游戏权力 | [api-auth](../../testing/manual/external-agent-runtime-contract-validation.manual.md#api-auth)；按该场景的候选、环境、故障注入和判据执行 | 对应 source HEAD 的 CI 输出或脱敏实测记录，保留世界回执关联 | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-aad7b341-83b1-4402-8ebc-8540db36ccfb；[AC-EXT-003](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-003) | [api-context](#api-context) | 首局能力目录、参数和实际执行一致 | [api-catalog](../../testing/manual/external-agent-runtime-contract-validation.manual.md#api-catalog)；按该场景的候选、环境、故障注入和判据执行 | 对应 source HEAD 的 CI 输出或脱敏实测记录，保留世界回执关联 | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-a27895ed-2ca4-45cc-901a-466befd02cf7；[AC-EXT-005](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-005) | [api-actions](#api-actions) | 成功、拒绝、未知结果可恢复且不重复世界效果 | [api-idempotency](../../testing/manual/external-agent-runtime-contract-validation.manual.md#api-idempotency)；按该场景的候选、环境、故障注入和判据执行 | 对应 source HEAD 的 CI 输出或脱敏实测记录，保留世界回执关联 | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-7283b901-ea06-49e3-956d-2bad46b6c89d；[AC-EXT-010](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-010) | [api-composition](#api-composition) | 两个真实 Runtime 分别完成首局与组合恢复 | [api-first-session](../../testing/manual/external-agent-runtime-contract-validation.manual.md#api-first-session)；按该场景的候选、环境、故障注入和判据执行 | 对应 source HEAD 的 CI 输出或脱敏实测记录，保留世界回执关联 | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-8120dffe-17cc-458a-9c70-8121217e6f7b；[AC-EXT-012](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-012) | [api-wire](#api-wire) | 普通客户端不依赖旧 Bridge 完成完整协议闭环 | [api-wire](../../testing/manual/external-agent-runtime-contract-validation.manual.md#api-wire)；按该场景的候选、环境、故障注入和判据执行 | 对应 source HEAD 的 CI 输出或脱敏实测记录，保留世界回执关联 | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-7d6cef7f-9419-49ed-b445-663382383458；[AC-EXT-013](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-013) | [api-auth](#api-auth) | 鉴权、越权、限流与授权撤销在服务端生效 | [api-safety](../../testing/manual/external-agent-runtime-contract-validation.manual.md#api-safety)；按该场景的候选、环境、故障注入和判据执行 | 对应 source HEAD 的 CI 输出或脱敏实测记录，保留世界回执关联 | 不单独证明真实模型、客户端或公开发行就绪 |
| Trace refs: O7-REQ-68ffc843-8679-4ad9-8cef-10a239a29c73；[AC-EXT-014](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-014) | [api-cutover](#api-cutover) | 关闭旧 Provider Bridge 后新主路径仍可运行 | [api-cutover](../../testing/manual/external-agent-runtime-contract-validation.manual.md#api-cutover)；按该场景的候选、环境、故障注入和判据执行 | 对应 source HEAD 的 CI 输出或脱敏实测记录，保留世界回执关联 | 不单独证明真实模型、客户端或公开发行就绪 |

## 12. 决策、长期风险与未决问题

ADR-GAPI-1：复用既有服务进程和世界权威，增加主动游戏协议，不扩展旧 Provider callback 充当通用 API；代价是需要显式新 schema 与迁移验证，收益是消除调用方向和鉴权混淆。

ADR-GAPI-2：以 at-least-once 投递和持久幂等实现同一 operation 的唯一世界效果；不承诺网络 exactly-once。权威不可达时牺牲新写可用性，不能靠本地缓存放行。

剩余解除项：identity owner 必须证明当前 hosted 认证可签发最小执行凭据；Runtime owner 必须把新 context/operation 扩展接入实际持久 admission 而非仅内存 sidecar。没有这两项，部署保持 read-only/blocked。API major、上限与观测指标变更时由 agent/runtime/QA 复核相应 schema 和回归，不扩大本次产品范围。

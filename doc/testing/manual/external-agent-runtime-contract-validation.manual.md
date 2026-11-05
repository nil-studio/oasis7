# 外部 Runtime 游戏接口与协作验证手册

- 适用基线：`03fb58784a65de9f02a43dfeafc3e882bb04e5f2` 之后按四份系统设计实现的候选；本手册是验证计划，不是当前运行结果。
- Owner role：`qa_engineer`；日期：2026-10-10。
- 总测试策略：[testing-manual.md](../../../testing-manual.md)；设计入口：[系统设计导航](../../world-simulator/llm/README.md)。

## 执行边界

设计阶段仅运行文档结构、链接、schema 示例和一致性检查；以下 API、driver、native 场景在实现存在后执行。不存在的端点、安装包或 test fixture 必须记为未覆盖，不生成绿色证明。普通 CI 应先覆盖确定性合同和故障注入，缺测试先补普通 CI。付费模型、真实机器持久恢复及正式 Web/native 体验只在获准环境和预算下验证，不让每个普通文档 PR 调用模型。

所有场景记录 source HEAD、测试合并树（若有）、世界/玩法版本、Runtime/driver/Skill/API/Viewer build、平台、许可和测试开始结束状态。保留脱敏请求、操作 ID、world receipt、故障点、控制版本及实际结果；不保存密码、token、私有推理链或其他任务数据。只清理本轮创建的进程/任务/世界目录，不改线上世界或他人会话。

## 显式 context 模式与原件恢复

Trace refs: O7-REQ-126d54f5-6d4a-4406-8d75-915c04d5d196；[AC-EXT-018](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-018)。冷入场导出的 executor 配置应明确包含 `"authority_mode":"native_executor_v1"`；旧配置省略字段仍使用 provider，不能将 provider 失败视为 native 准入。核对实际签名 prepare/snapshot 请求及 context identity 的 revision/mode，并验证回复恰好一种 authority。native 输出无 module catalog，必须检查真实 task fence、delegation、预算和 lease；配置标记本身不算认证成功。

保留 provider 旧原件的 JSON/CBOR、签名和 digest 作为兼容对照；重启后原 bytes 与 nonce 不变。对同一 operation 改 mode 必须在提交前拒绝，恢复旧 mode 后仍读取同一原件。native/provider 混合 authority、错误 revision、错误 fence、失效 delegation 和过期 context 都应失败。broker 模型请求不能传 mode；更改可信配置会改变固定哈希及持久预算 profile 绑定，不能用修改配置重置额度。unknown 仍先查询原操作，不能更换模式重提。

## 统一准备与判定

既有 API/协作场景创建两个 owner、两个独立 executor、一 Agent 一任务，使用明确的授权额度与可回读 world root；api-cold-entry 的被测新主体不适用这些预置，改用该场景的无角色/任务/执行 grant 起点，合法身份获取与准入资格可预先获准但产品步骤不能代做。fixture 名称由实现 PR 注册。所有正常与负例均读取实际前后状态和持久回执，禁止仅验证 HTTP 200 或执行器自报。超时、取消和无响应先核对原操作。

对于真实 Runtime，分别运行 OpenClaw 与 Codex。对于图形体验，分别运行正式 Web 和受支持原生桌面包；不得相互代签。首局沿既有 `production_only` 完成边界，不以任务关闭推导稳定生产或发行就绪。

## 场景清单

<a id="api-cold-entry"></a>
### 场景 api-cold-entry

客户端编排回归须覆盖完整新主体步骤，并在注册或绑定响应丢失后重启恢复；不能手工写入 `login-session`、Agent、task、goal 或 grant 作为该路径的起点。真实 IdentityService 与 WorldService/commit loop 的组合验证与客户端合成 HTTP 回包测试分别记录。验证成功出口时重新读取当前 canonical task/goal/executor fence；不以本地阶段日志代替当前权限。DevLocal 运营工具另验证一致快照、错 namespace/epoch、前态变化、签名缺失和失败无世界写入。普通客户端组合通过不自动宣布任一真实 Runtime 已验收。

- Trace refs: O7-REQ-126d54f5-6d4a-4406-8d75-915c04d5d196；产品 [AC-EXT-018](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-018)，系统 [bootstrap](../../world-simulator/llm/external-agent-game-api.design.md#api-autonomous-entry)与[客户端编排](../../world-simulator/llm/external-agent-runtime-adapter.design.md#adapter-autonomous-entry)。
- 层/环境：普通网络客户端合同/持久故障注入为 `test_tier_required`；每个声明支持的真实 Runtime/profile 的独立冷入场为 `test_tier_full`。仅使用获准隔离世界和有界预算，不操作公开部署或真实账号。
- 给定：世界已经正常存在；新机器主体没有预置游戏 Agent、task、goal 或执行 grant（合法身份/准入证明不在此禁止之列）。输入仅含可信 Skill/发现入口、获准凭据获取/必要确认通道及预算；记录预置清单，不给 operator 权限、隐藏世界答案或人工代建路径。正常身份准入资格与必要人类核验可以由测试环境合法提供，不能代做待测产品步骤。
- 正向：记录可信发现→注册/登录→世界准入→正常首次注册/绑定→canonical owner 与必要 starter→task/自选目标→独立 delegation/executor→观察/行动/权威成果→断连和进程重启恢复。每段保存原 operation/receipt、主体/world/Agent/task/goal/fence 关联及实际执行者；必要确认记录 action_required 与实际恢复，不算缺陷或自动略过。
- 负向：错误/未受信身份服务或世界、重放/错用途/过期 PoP、撤销凭据、未获准入或配额、未知/冲突绑定、他人 Agent、自报 owner、starter 不足/重复领取、owner key 当 delegate、executor 自改目标/自审批、注册关闭及未发布公开能力。均应安全拒绝或明确阻塞，无额外角色、资源、费用或世界效果。
- 故障：分别在账号提交、角色绑定、starter、task/goal/delegation 提交成功后丢回包并重启；同键同内容回读原结果，同键异内容拒绝，pending/unknown 不自动换 key/nonce 重提。账号成功但世界准入失败恢复原账号；canonical owner 条件未齐不得先启动 executor。并发首次请求只产生同主体/世界的一个合法绑定，不复用其他账号角色；注入受保护原请求 outbox 丢失时只能按支持路径重新认证/回查，无法确认原结果则阻塞，不重新注册或重签。
- 判定：原世界不重建，各阶段只确认本 authority 的结果，首局成果仍依玩法 receipt；恢复不重复建号、角色、task、grant、领取或世界副作用。查询缺权限/证据时 blocked/unknown，不泄露主体可枚举状态。两 Runtime 分别成案，不能拼接不同候选/主体或用普通客户端代签真实自主入场。
- 证明边界：prepared-world、预建 task/grant、人工代做角色绑定只证明其后执行范围；现有 private loopback 或 import_only 不能作为公开冷入场成功。缺安全入口时报告 blocked/not_run，禁止临时暴露私有接口以通过。自有/受托共存另执行 [api-auth](#api-auth)。



<a id="api-auth"></a>
### 场景 api-auth

- 共存扩展：Trace refs: O7-REQ-d30925fe-d798-4056-9641-062255931a61；按 [AC-EXT-002](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-002)，同一 Runtime 分别控制自己的 A 和玩家 B 委托的 B。先证明两者正常行动，再交换 token/context/goal/grant/receipt 与 owner proof，均须拒绝跨资源使用；B executor 不得选择 owner 角色或调用 owner signer 为自己改目标/审批。仅撤销 B 后拒绝其旧 fence 的新效果，A 与其他独立委托不受影响；账号级撤销按其真实范围另测。目标、私有正文、预算和原 pending 结果分别对账。

- 层/环境：API / auth / contract。
- 给定：固定 world、owner、Agent、两个配对设备与只读/执行/owner 三类身份。
- 执行：合法配对后列能力；以别人的 Agent、公开 user_ref、模型 token、过期 refresh 和执行器调用 owner 批准作负例。
- 判定：仅有效主体拿到对应 scopes；公开标识无授权作用；拒绝不披露私有对象。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="api-catalog"></a>
### 场景 api-catalog

- 层/环境：API / gameplay registry。
- 给定：同一候选的 starter-industrial-smelter-to-assembler-v1 / production_only 初始世界与 Runtime registry。
- 执行：枚举每个对该 Agent 宣告可执行的 action_ref，按合法参数逐项解析/admit；另改资源/目标/权限。
- 判定：宣告项全部有同一 schema 与执行路径；缺失/变化是明确不可用或拒绝，不凭 action 名数推导可玩。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="api-idempotency"></a>
### 场景 api-idempotency

- 层/环境：API + Runtime / durable fault injection。
- 给定：有持久 operation ledger 的临时世界，记录初始资源和 world receipt。
- 执行：在登记前、登记后、admission 后、commit 后、HTTP reply 前分别 kill 自有测试进程；用同一键重试并用异内容同键请求。
- 判定：同键同内容只关联一个 effect/receipt；异内容冲突；未知先查询；授权撤销后不因缓存返回私密历史。
- 不确定结果回归：在 admission 已接受而权威结果暂时无法回读时，持久保留同一 operation 为 recovery_required；两个查询入口都以受权 OperationView 返回该状态、原因和 lookup_original，不伪装 failed、404 或普通 pending。重复同键、进程重启、context 到期与 stop 均不能新建第二个 action/nonce 或清除未知结果。
- 对账恢复：分别恢复“确认仍待决”“确认已结算”“确认拒绝/终态失败”的真实证据，验证只能转为对应 pending/终态且保留原操作身份；原 receipt/effect 至多一份。无法确认或底层去重保护不可验证时只回查/阻塞，不盲重投；只有原请求可安全幂等重投的已验证条件成立才允许 outbox 重投。
- 客户端消费：Worker/Web/native 显示结果待恢复确认并停止该 Agent 新副作用，仍可读取授权允许的信息。操作存储不可读返回 503，不能据此创建新键；401/403 先拒绝，不能从回放泄露历史。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="api-first-session"></a>
### 场景 api-first-session

- 层/环境：真实 Runtime / gameplay / 组合。
- 给定：两个明确获准使用的真实 Runtime 组合，同一玩法候选、可比较初始资源、合法 Agent；另备 Web/native 候选。
- 执行：两个 Runtime 分别自助接入、取得首产物，经历资源阻塞/目标更新/回访及断连重启，逐项记录对应 operation。
- 判定：首产物以主责生产回执判定，全部恢复义务分别留证；一次成功、mock、另一 Runtime 或另一 surface 不代签。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

Trace refs: O7-REQ-7283b901-ea06-49e3-956d-2bad46b6c89d, O7-REQ-8120dffe-17cc-458a-9c70-8121217e6f7b。按[首局组合设计](../../world-simulator/llm/external-agent-game-api.design.md#api-first-session-composition)，先用普通 HTTP 客户端验证协议与目录，然后对 Codex/OpenClaw 的每个声明 profile 分别完成下表。同一组合在原世界、Agent、task 历史上覆盖全部适用项；可用独立 attempt 注入故障，但必须保留起点、原件与该组合的失败，不能跨 Runtime 拼接。

| ID / 操作 | 独立 oracle 与通过判据 |
| --- | --- |
| FS01 可信 Skill、版本与合法接入 | 真正 driver/session 和已公布 schema/能力相符；当前主体、goal、fence 分别回读。cold-entry 与预置合法 Agent 路径独立，不用 operator 代做待测步骤 |
| FS02 当前 starter 条件及动态目录 | 固定 `starter-industrial-smelter-to-assembler-v1 / production_only` 及其 revision；逐项对照真实所需查询/转运/建造/排产与目录 schema、parser、admission。删一个实际必需能力、改 schema 或撤权限必须明确阻塞，不能靠固定名字继续 |
| FS03 自主生产与原反馈 | Runtime 从获准观察自行选择动作；匹配 production receipt、owner-bound 铁锭及原 milestone 证明 produced/undelivered。scheduled、cognition committed、库存自然增长或模型“完成”均不通过；原结果进入该 Runtime 输入另按 RF01 |
| FS04 真实资源/生产 blocker 后继续 | 在真实世界改变该路线一个前置；看到与 authority 一致的 blocker，Runtime 自主采取合法修复/等待/减量/重规划，并以其后实际结果证明继续；不能人工代补隐藏资源 |
| FS05 目标接受与实际应用 | 新目标正文/revision 真正进入后续 context；旧 pending 保留原身份，迟到结果不归新目标。已保存、已读、已采纳、context-used 与新世界结果分别留证 |
| FS06 声明入口离开与回访 | viewer 关闭/重开、pure_api 断开/重读分别执行；独立运行条件真实成立时，离开期间的成果由原 receipt 证明。没有成果时如实报告等待/阻塞，不预填继续 |
| FS07 断连与实际进程重启 | 两种故障各一次；原操作/成本/反馈位点对账，无重复效果或新额度；私有会话不可恢复时用当前可信基线，不能要求保持原私有 session ID 才算恢复 |

`test_tier_required` 的协议/受控故障只证明接缝；真实 Runtime 为 `test_tier_full`。Web/native 的指导、文本与图形恢复另按各表面执行，不能由纯 API 或另一端代签；本表不证明稳定生产、交付、真人喜欢继续或发行就绪。只读本手册或审查代码时所有运行项保持 not_run。

<a id="api-wire"></a>
### 场景 api-wire

- 层/环境：HTTP contract / 不调用模型。
- 给定：已实现新 Game API 的服务，普通 HTTP 客户端，不启动任何 Provider Bridge、OpenClaw 或 Codex。
- 执行：按发现→鉴权→task/context→query→action→operation→events 顺序请求，再验证 query-only 后用 context outcomes 进入等待/收口，覆盖非法 JSON、未知字段、超大整数和错误 schema。
- 判定：状态/幂等/版本可消费，accepted 不代签 committed；只读无世界效果；旧 provider DTO 不被当新请求。
- 消费 DTO：验证 active_goal 完整正文与 null 条件，以及 OperationView 的 recovery_required/recovery/retry_advice 能被严格解码；不识别这些必要语义的旧草案客户端应停止，不默认成功或盲重试。
- 反馈与目录：协商 OperationFeedbackV1 的目标能力；available 必须有匹配原 cognition fence/context 的真实 record，pending/history_unavailable 不得夹带伪造 record，非 cognition 才允许 not_applicable。旧 schema 不支持时报告反馈未接通；空字段不当作完成。按目录发现的 feedback_history 查询验证同切点/原 session sequence 范围、100 条/2 MiB 分页、末页完整性、过期 cursor、跨 audience/branch 及未知 schema；只读无世界 mutation、反馈 ACK 或模型调用。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="api-safety"></a>
### 场景 api-safety

- 层/环境：API / safety / deterministic races。
- 给定：两个主体、一个累计 grant、多个连接与并发动作，构造已提交的撤销和新目标。
- 执行：并发消耗 grant，在 commit 前撤销或转让；重连重试，模拟 auth unavailable、跨 origin cookie 请求和任意 callback URL。
- 判定：累计额度不复制；新受限动作在权威生效后拒绝；CORS/CSRF/route 限制生效，无新写 fallback。
- 恢复矩阵：按[共同安全边界](../../world-simulator/llm/external-agent-game-api.design.md#api-retry-safety)分别注入发送前拒绝、登记后 429/503、commit 后回复丢失、同键异内容、多连接同时用一个 grant、context 过期和 authority 不可用。预期是原请求查询/明确拒绝或真实终态；不能因网络码、重连、换 token、目标或新键得到第二份 effect/receipt 或重置额度。不同键的重复业务意图另核其玩法约束，不以本测试宣称任意语义 exactly-once。
- 有界回查：耗尽新执行配额时，已获权的原结果与 Stop 仍使用独立控制/读取队列；读取到限返回 retry_after，停止并发增长并查询同一原件。读取授权也撤销时拒绝且无对象存在性泄漏。分别检查本地与 HTTPS 远程；TLS 成功不代签主体权利。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="api-cutover"></a>
### 场景 api-cutover

- 层/环境：部署候选 / 新旧路径隔离。
- 给定：固定新 API/worker/Viewer candidate，列清旧 Bridge 真实消费者和存储版本。
- 执行：停用旧 Bridge，在不改变世界历史下执行新路径的首局与恢复；再禁用新写并查询已有结果。
- 判定：新入口不依赖旧回调；回滚不静默切旧 executor、不丢 pending，不把移除旧代码当产品验收。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="collab-goals"></a>
### 场景 collab-goals

- 层/环境：Task control / concurrent revision。
- 给定：任务 latest/active goal revision 已知；owner Web 与桌面两个草稿。
- 执行：发布新目标并保持旧规划在途，再发送旧草稿、旧已读、旧副作用和当前版本动作。
- 判定：旧写入不覆盖；active/read/adopt/context-use/world-result 分开；新硬约束不等待 ack。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="collab-recovery"></a>
### 场景 collab-recovery

- 层/环境：Runtime / journal/checkpoint。
- 给定：临时单权威世界与私有内容 store，包含已确认目标、未答问题、待批申请和未结算操作。
- 执行：在正文 fsync、control commit、outbox 发布之间注入 crash；恢复并核验缺正文/坏 digest/旧备份情况。
- 判定：不丢已确认引用；缺正文保持 recovery_required；不以空目标、旧 epoch 或旧批准继续。
- 冷启动回归：先激活含非空完整正文的 G1，使激活事件早于切点 S 并从事件重放窗口移除；另保存未激活 G2。使用无任何本地缓存的新受权 executor，仅 GET snapshot 的完整分页序列和 after-S 事件初始化。必须取得 G1 的完整 GoalVersionProjectionV1，task/revision/digest/activation ref 与切点一致，可确认 received 并申请对应 context；不能用 G2、摘要或空目标替代。
- 失败与竞态：缺正文、损坏 digest、正文超限均不得返回伪完整 snapshot 或启动认知；撤销/转让后不泄露正文/指纹；真正无激活目标才有 active_goal=null。snapshot 后激活 G2，按 G1 申请 context 必须被拒绝并重取 G2，不重新标注旧正文。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="collab-snapshot-pages"></a>
### 场景 collab-snapshot-pages

- 层/环境：普通 HTTP contract + 临时持久存储 + 确定性并发，不调用真实模型。
- 主责与追踪：runtime_engineer / agent_engineer / qa_engineer；承接
  [同切点分页合同](../../world-runtime/runtime/player-runtime-collaboration.design.md#collab-snapshot-pages)
  的 AC-COLLAB-006 / AC-EXT-008，复用 collab-recovery、collab-events、api-wire 的 fixture。
- 给定：已提交 G1，合法的私有目标/待决记录和未结算 operation；移除 S 前的传输事件，
  客户端无任何本地缓存。按实际 JSON 编码构造正文、元数据和转义边界，不以字符串长度估算容量。

| 用例 | 执行与可判定预期 |
| --- | --- |
| 空集、200/201/256 个同类项 | 分别只放未完问题线程、只放申请；线程同时覆盖未答和已答未处理。200 项可能因字节提前分页，201/256 项不得被 200 的列表上限拒收或截断。最终唯一 ID 集合与 S 上授权集合完全相等，最后页单独不能初始化 |
| 混合集合 | 128 个未完线程 + 128 个申请，另有未结算操作和当前报告；用 limit=1、200 验证 pending_question_threads → pending_approvals → unsettled_operation_refs → latest_reports、total_counts 与原 operation 关联，不能只读问题列表 |
| 最大编码边界 | goal 4 KiB、message/report 8 KiB，包含引号、反斜线、控制字符、多字节字符；另测 64 KiB 当前项/头部及超出一个字节。整个响应不超过 2 MiB，metadata 不能绕过检查，单项不切片 |
| 条数与大小拒绝 | 第 257 个待决项、非法 limit、超大新记录在接受前明确拒绝且无持久副作用；256 个既有项及其正文保持不变。满额时已有线程的合法答案/接收/处理报告使用预留容量，不能因槽位已满拒收；旧草案过大记录保留原数据并 recovery_required，不伪完整 |
| 重试、乱序与断连 | 重复同 cursor 返回同一页；客户端重复页幂等，缺第零页/中间页、另一 snapshot_id 或被改写头部不能提交基线；重试不延长 pin 寿命 |
| 页间新增、回答与批准 | 取得第一页后新增问题、回答原问题、处理答案或终结待批申请；其 S 上旧形态仍在原页，变化恰好经 after-S 接续。已回答未处理线程仍保留，处理/关闭/失效终态才移出；不复活旧阶段、不重放批准或原世界副作用 |
| 日志压缩、到期与重启 | 原事件过期仍从当前态读齐；pin 内所需版本和 after-S 窗口可用。强制 pin 到期、服务重启丢 pin 或窗口不足，显式 reset 后从新 S 重建，不能返回空的成功尾页 |
| 撤权与历史分支变化 | 读页后、解密前及交付前分别撤销/转让/替换 executor；无权请求不返回正文、计数或差异。仍有读权但 pin 绑定失效须 reset；分支不混用 |
| 恢复到决策 | 完整基线及增量形成后再取 context；页间已激活 G2 或 grant 已失效，旧 G1/context 仍被当前权威拒绝。原未知 operation 先对账，读取快照不会生成任何批准或重提 |

每例保留 source/build、S、授权版本、脱敏稳定 ID 集合、页号/实际编码字节数、原操作与前后世界结果。
游标只记录不可反用的关联标识，不把可重放游标、私有正文或私有 digest 写进公开证据。
本表是实现后的验证计划；模型化分页算例只能证明算例本身，不能证明真实持久化、权限或 HTTP 实现。

<a id="collab-bind"></a>
### 场景 collab-bind

- 层/环境：Identity / executor fencing。
- 给定：同一 Agent 有 owner、executor E1；另外一个身份及 E2。
- 执行：owner 显式替换为 E2，E1 迟到读私有消息/提交行动，另让错误身份访问相同 ID。
- 判定：task 和世界身份稳定；旧 executor 失去新写资格，历史读取按当前 audience 限制；内部 session 不新建 Agent。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="collab-adoption"></a>
### 场景 collab-adoption

- 层/环境：Task / observable evidence。
- 给定：executor 离线发布目标，再接入，另一个窗口并发更新。
- 执行：分别记录保存、激活、received、adoption_reported、context 和合法提案；旧确认迟到。
- 判定：所有状态绑定其实际版本；自报不冒充模型理解或世界完成，旧状态不前推当前目标。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="collab-report"></a>
### 场景 collab-report

- 层/环境：Reports / outcome verification。
- 给定：一项合法生产 receipt、一项他人 receipt、一项模型自报完成与陈旧 report。
- 执行：请求写入报告并查看玩家 task panel，尝试将文字 status=committed 注入成果字段。
- 判定：只展示授权的引用；reported/estimated/stale 与权威成果区别清楚；无机器 evaluator 时不宣称机械完成。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="collab-questions"></a>
### 场景 collab-questions

- 层/环境：Messages / stale reply。
- 给定：同一 task 的 Q1 绑定旧目标，Q2 绑定当前目标；Q1 的一份答案在失效前已持久接受，延迟到失效后交付；Web 和桌面重复回答。
- 执行：回读/交付旧 Q1 原答案，另在失效后首次提交旧写入；重复发送同内容/异内容 message_id，发送“同意”及包含指令的世界文本。
- 判定：关联和去重正确，已接受旧答复保留 stale，失效后新提交拒绝；文字不自动改目标、不生成 grant、不改变工具权限。逐条接收与处理继续按下案验证。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="collab-question-delivery"></a>
### 场景 collab-question-delivery

- 层/环境：普通 HTTP/RuntimePort 合同、实际临时持久存储、scripted driver 与确定性故障注入；不调用真实模型。玩家内存清理在候选实际支持的 Web/native surface 分别验证，不能由 HTTP fake 代签。
- 主责与追踪：runtime_engineer / agent_engineer / viewer_engineer / qa_engineer；Trace refs: O7-REQ-2f463269-c3c0-4da1-95f8-90011c35799a、O7-REQ-6044cb04-5fbb-4c68-81a6-fc57cfdd608a、O7-REQ-4a8be58c-bcda-4efe-b199-1cf9bd9b3330；分别承接 AC-COLLAB-004 的交付、AC-COLLAB-006 与 AC-EXT-008 的恢复子义务，不抬高其余未验收范围。
- 合同来源：[Runtime 交付](../../world-runtime/runtime/player-runtime-collaboration.design.md#collab-question-delivery)、[当前态恢复](../../world-runtime/runtime/player-runtime-collaboration.design.md#collab-question-recovery)、[API 端点](../../world-simulator/llm/external-agent-game-api.design.md#api-question-delivery)、[adapter 消费](../../world-simulator/llm/external-agent-runtime-adapter.design.md#adapter-question-delivery)。fixture 记录实际支持的存储、客户端与 schema；不存在端点/实现则 blocked/not_run。
- 给定：一项当前 task/G1/E1，两个 owner 窗口和一个受限 executor；分别建立 owner 提问与 executor 提问的线程，另有原 run_unknown 和未结算世界 operation。保存可比较的世界状态、grant 与预算，不用消息自报代替结果。

| 用例 | 执行与可判定预期 |
| --- | --- |
| 双向角色与阶段 | 两种提问方向各走问题保存 → 问题 received_reported → 答案保存 → 答案 received_reported → 答案 processed_reported；每步回读原线程与 result_ref。awaiting_answer/answered_pending_processing/processed_reported 与阶段一一对应，接收前后不能提前显示已答或已处理 |
| 问题尚未回答 | 接收端离线、已收到但尚未回答分别读取完整线程；未答问题正文和 question_received 可恢复，不因无答案而漏掉交付状态；等待不授予新权限 |
| 接收依据与玩家内存 | executor 在 inbox 持久化前不能发 durable_inbox；owner 只能 volatile_client。玩家重开、切账号或失去读权清除私有内存，不新增浏览器/本机持久正文缓存；重新获权读取完整线程后才允许回答/标记查看。旧收到报告不表示新窗口有正文或人已读 |
| 处理声明与业务效果 | owner 仅显式操作产生 viewed；渲染/焦点/重连不产生它，executor 使用 viewed 拒绝。considered/needs_goal_change/needs_authorization/cannot_use 都可按合法角色保存，但不改变 goal/grant/世界账本。问题没有 processed_reported，回答者不能处理确认自己的答案 |
| 前序和乱序 | 未确认问题接收就 POST answer、未确认答案接收就 POST processed_reported，返回 receipt_precondition_required；服务器不自动生成前序。先让接收请求超时，再查原键/原线程，确认其 persisted 才提交后序 |
| 同键、竞答与更正 | 同 client_operation_id 同内容回原结果，异内容 operation_conflict；两窗口同 thread_revision 竞答仅一份被接受。换键占用已有阶段返回 phase_already_recorded，不新增事件/别名记录；原正文不可覆盖，更正用新的 related_question_id 或明确目标修改 |
| inbox/outbox 故障窗口 | 分别在 inbox 同步前后、接收报告提交前后、答案/处理输出持久化前后掉进程。执行端只允许持久原件驱动发送；丢响应先回查原键，persisted/rejected/not_recorded/recovery_required 不混淆，not_recorded 仅按合同重投相同原件。玩家窗口丢原键不自动重发，先读原线程，新的显式操作仍与在途原请求争用同一阶段槽位 |
| 权限与协议负例 | 错 actor、接收者、task、分支、goal、epoch、私有 audience；非法枚举/混合字段/超限正文分别拒绝。401/403 不泄露对象存在或差异；409/413/429 不产生线程、配额或事件变动，不伪世界 receipt |
| 窗口外的未完线程 | 同时保留未答 Q 与已答未处理 A，令原传输事件超出 7 天或 100000 项窗口；无本地缓存的受权端仅靠全部 snapshot 页及 after-S 恢复原文/阶段。回答不能使线程提前离开 pending_question_threads，也不能依赖补回 S 前事件 |
| 201/256 项与预留 | 构造 201/256 个未完线程或混合待批项；满额后仍可保存已有线程的合法答案和阶段报告，第 257 新问题/申请拒绝且不驱逐旧项。并发创建竞争最后槽位，配额与正文/事件同提交；关闭/处理/失效释放一次且不解除 gameplay blocker |
| 64 KiB / 2 MiB / 200 边界 | 联合 collab-snapshot-pages 按真实编码校验完整线程、报告、头部、转义和分页。验证大项提前分页，不能声称 31 个 64 KiB 项加 64 KiB 头部仍有 JSON 分隔余量；页数按实际集合和字节记录，不把手算写成实测 |
| Stop 与原历史 | 在每个阶段提交前后提交 Stop/目标更新/owner 转让/executor 替换；已接受旧记录仍保持原结果并为 stale，失效后新写入拒绝。新 owner 不默认获旧正文，已处理终态不迁到新目标；Stop 不等 ACK、模型或私有正文解密 |
| 新进程与未知原结果 | 本地 inbox 清空但服务端已有收到报告时，先重建完整原文；新 epoch 的旧线程只作历史，需当前沟通则显式新建。原 run_unknown 先 reconcile、原 world operation 先对账，不因收到了答案或处理报告就再次调用模型/提交世界行动 |
| 原件与投影故障 | 缺失/损坏已承诺私有原件、不可验证索引或控制切点时，相关回读 recovery_required，不回空成功或假 ACK；已生效 Stop 不回退，原世界操作按独立合同回查。完整基线只能在所有页读齐后发布 |

证据保存候选 source/build、私有存储能力、原请求/消息/阶段的脱敏稳定关联、S 与 control/fence、实际字节/页数和原结果；不公开正文、原签名、可重放 cursor 或可枚举私有 digest。最小路径有 5 次串行私有提交，故障回查与本地同步也计入原请求/时间预算；报告缺预算或超时时保留未完状态，不用完成报告凑终态。本节计划和容量手算均不证明实际持久时延、真实 Runtime、玩家已读或付费调用；真实组合仅在获准时按 collab-session 留证。

<a id="collab-approval"></a>
### 场景 collab-approval

- 层/环境：Approval / cumulative budget / races。
- 给定：一项带 proposal digest、owner/goal/fence 版本与额度的申请。
- 执行：明确批准后并发消费，分别测试拒绝、超时、撤销、owner/goal/executor 变化与重复决定。
- 判定：一次终态决定；无回应不批准；消费在世界提交时重验，额度不重置；本机/第三方工具另需授权。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="collab-events"></a>
### 场景 collab-events

- 层/环境：Event replay / snapshot cut。
- 给定：task 在 cut S 后持续写消息，保留期可在测试中压缩，另有 audience 变化。
- 执行：snapshot 读取与增量并发；重复、乱序、游标到期/篡改、重连与权限收窄。
- 判定：S 与 after-S 没有静默缺口；过期显式 reset；private 数据不串线，未答及已答未处理的 pending_question_threads 不会因传输日志压缩丢失。完整度和容量联合 [collab-snapshot-pages](#collab-snapshot-pages) 与 [collab-question-delivery](#collab-question-delivery) 验证，不能只测第一页。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="collab-executor"></a>
### 场景 collab-executor

- 层/环境：Control / stop / restart。
- 给定：有运行中 driver、待决世界动作和保存后的新目标。
- 执行：stop 提交时推理仍在，之后重启旧实例并启动 owner 认可的新绑定。
- 判定：stop 从权威提交点阻止新动作，不等 ack；世界旧结果可查询，新 worker 不继承旧授权。该分项本身不代签 AC-COLLAB-007，组合取证见 [collab-session](#collab-session)。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="collab-session"></a>
### 场景 collab-session

- 层/环境：真实外部 Runtime + 独立 worker + 实际使用的 Viewer surface + 临时授权世界。
- 主责与追踪：qa_engineer 联合 agent_engineer / runtime_engineer / viewer_engineer；承接
  [同任务组合验收](../../world-runtime/runtime/player-runtime-collaboration.design.md#collab-runtime-journey)
  的 AC-COLLAB-007。这是现有场景的组合取证规则，不是另一套测试平台或发行 gate。

**一案一 Runtime/profile。** 每个声明支持的 OpenClaw/Codex 组合各自建立 case_id，记录实际
Runtime/driver/worker/Skill/API/Viewer build、source HEAD、primary mode、平台、许可与预算。
同案始终使用同一 world/genesis/branch、Agent 和 task；G1→G2、E1→E2 的变化保留明确的
goal/control/grant revision 与提交引用，不把新版本伪装成相同值。各案可用独立临时世界，不要求共用任务，
也不能拿一个 Runtime 的完整过程补另一 Runtime 的缺段。

| 同案阶段 | 必须能关联的事实与预期 |
| --- | --- |
| 启动并离开 Viewer | E1 对 G1 有真实 Runtime session/run/context 和调用记录；关闭 Viewer 后，在原合法预算内仍能继续一次决策/查询/行动。健康或 heartbeat 单独不足以证明继续执行 |
| 停止执行器 | 记录 StopDelegation 的权威 control commit/fence 变化及自有 worker/driver 的停止结果。前者阻止新受限动作，后者证明进程确已离线；未知停止结果不得写成已停止，不能终止 world authority 或他人进程来制造成功 |
| 离线更新 | world authority 保持运行，owner 在同一 task 发布并激活 G2；保存、激活引用和 awaiting_executor 分别可读。离线不是未保存，也不是运行中已采纳 |
| 重启续接 | 用当前有权主体认可的绑定取得 E2，经完整 snapshot/after-S 恢复 G2；不新建 Agent/task，不把 stop 前的授权复活。记录真实新 session、context 和 G2 的 received/adoption/decision 关联 |
| 问题与回答 | 在 G2/E2 下产生 Q2 与 message_id，owner 的回答以 in_reply_to 指回该问题；分别记录 owner 的 volatile_client 接收、答案保存、executor 的 durable_inbox 接收及实际 Runtime 的处理报告。问题保存/答案保存/接收/处理与世界结果分别可读；旧 G1 的已接受问题/答复保持 stale，失效后新写入拒绝，普通“同意”不生成 grant |
| 明确批准 | 当前 executor 发起绑定 task/G2/E2、proposal digest、owner revision、scope/累计预算和 expiry 的申请 P2；owner 通过专用 decisions 入口生成唯一 decision_ref/grant_ref，不以聊天或截图当授权 |
| 后续世界结果 | P2 的授权关联到实际 context/decision、client_operation_id/operation_id、admission 与权威 receipt；回读前后状态确认真实效果且只一次。accepted、adoption_reported 或 completion_reported 不代签 committed |
| 回访与收尾 | 重开实际 Viewer，读取同一 task 的目标、问题/决定和原 operation 结果；失败、未知与 world result 分层可读。只清理本案资源，保留原失败和恢复过程 |

证据可分多个文件或直接复用现有记录，但须有一份小型关联表：
`case_id → runtime/profile/session → world/task/Agent → goal revision → executor epoch →
question/answer → approval/decision/grant → context/operation → world receipt`。
每条边都来自实际请求、受信边界记录或权威回读；QA 检查它们属于同候选的同一过程。只按时间接近、
同一用户名、另一 task 的绿色结果或任意 receipt 字符串拼接，均为证据缺口；脱敏不得破坏关联，也不得公开秘密。

可复用同候选且范围匹配的 collab-executor/questions/approval、api-idempotency 等确定性负例，
不要求每个真实 Runtime 重跑全部故障点，不增加所有平台/模型的笛卡尔矩阵。图形交互仍按实际 surface
分别验证；本案只覆盖所记录的平台/入口。后续世界效果可复用同案 api-first-session 的 production_only
回执，不能据此宣称稳定生产、完整工业链、跨 Runtime 私有记忆迁移或公开发行就绪。

缺接口、独立试玩、Runtime、GUI、有效授权或预算时，记录缺失阶段为 blocked/not_run；保留已完成的
分项范围，但 AC-COLLAB-007 的该组合不能整体通过。文档检查或 fake driver 不代签真实会话，
也不因本验证计划存在而调用未经批准的付费模型。

<a id="adapter-connect"></a>
### 场景 adapter-connect

- 层/环境：Driver / fake + real。
- 给定：版本锁定的 Codex stdio driver 和 OpenClaw Gateway driver，各有独立任务/会话。
- 执行：先以 scripted fake driver 验证消息关联，再经批准运行真实会话；版本不兼容、schema 漂移和错误 binary 作负例。
- 判定：真实执行方、session/run/event 关联可核对；不存在静默直连模型或跨 Runtime fallback。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="adapter-loop"></a>
### 场景 adapter-loop

- 层/环境：Worker / cognition / scripted driver。
- 给定：fake driver 需要两次查询、一次提案与一次等待，运行时同时发布新目标。
- 执行：控制循环在推理未完成时读取变更；送达新 context，旧提案和新的合法提案分别请求 admission。
- 判定：控制消息不被模型长调用阻塞，旧 context 拒绝；后续版本与许可有效，world 不等待模型。
- 恢复输入：首次运行没有本地目标缓存、历史激活事件已不在重放范围时，必须从 snapshot 正文而非仅 revision/digest 得到目标；收到 recovery_required 的世界操作则保持原关联并对账，不开新的副作用轮次。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="adapter-result-feedback"></a>
### 场景 adapter-result-feedback

Trace refs: O7-REQ-a27895ed-2ca4-45cc-901a-466befd02cf7。覆盖 [AC-EXT-005](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-005)，对应 [API 原结果](../../world-simulator/llm/external-agent-game-api.design.md#api-result-feedback)、[adapter 输入](../../world-simulator/llm/external-agent-runtime-adapter.design.md#adapter-result-feedback)和[玩家投影](../../world-simulator/viewer/external-agent-world-view.design.md#view-runtime-results)。此处是新增目标方法，不是已运行记录；fixture 注册与实现由交付 PR 提供。

固定同候选 API/Runtime/worker/driver schema 与合法临时世界。准备 A/B 两主体、各自 task/session/fence，原 operation 及由真实 authority 产生的 committed、rejected、failed、pending 与 recovery_required；状态不能只靠 fake HTTP 字符串。受控 driver 用接收日志和 input/run identity 观察交付，世界 oracle 另核原 receipt/资源/effect；两真实 Runtime 的协议接收/会话来源独立再测。公开证据只保留脱敏关联，不保存秘密、私有推理或完整 Prompt。

| ID / 输入与故障 | 独立预期结果与必须核对的消费者 |
| --- | --- |
| RF01 原成功、拒绝、失败与未决逐项到达 | operation 原结果、Runtime authoritative feedback、worker 持久接收、实际 driver 输入确认、下一次 context/decision 与玩家显示分别留证。成功必须进入正确后续输入；ACK/HTTP/model 文本不生成新 receipt。pending/恢复未知不释放新副作用 |
| RF02 A/B 交错、同一 A 新 goal、原 run 完成后晚到 | B 的记录不进入 A；旧 goal 保留原归属。旧 session 不进入新活动请求，可信恢复只引用受权历史；换选中对象不能改结果主体 |
| RF03 seq 1、3、重复 1，再取得原 seq 2 | 3 先 held，不按到达时间排序；同序同 digest 去重，补 2 后按源序恢复。另以同序异 digest、错 request/receipt/finality、伪 committed、缺原 fence 作负例，必须阻断对应投影而不改世界终态 |
| RF04 ACK 后淘汰摘要，随后回到仍有效旧 session | 原水位延续，不从 1 重启、不把正常高 seq 永久 held；删除水位或超保留窗口时准确停止投影，不能把较大 seq 当恢复起点 |
| RF05 在 inbox 持久前/后、ACK 后、输入发送后/上游接收后但本地确认前崩溃 | 未持久不得 ACK；重启恢复同代位点。确认已接收恢复原位点，确认未收且协议允许才重送原输入；无法确认保留 run_unknown，不新建付费调用。原已 committed 世界效果始终至多一次 |
| RF06 33 条或超过 128 KiB 输入批次，257 条或超过 4 MiB 未交付正文，超大/敏感单条 | 按 count/bytes 较低上限停止新 turn并有界排空；不裁掉关键结果后报完整，不无限 Prompt/缓存。超限单条显示可验证身份/状态和 projection 不可用，不伪造已交付 |
| RF07 新旧本地代际拼接、同 operation 换 fence/branch、私有缓存换 audience | 原件与水位/交付位点必须一致；不一致暂停并从 authority 对账。旧 fence 不复活、原反馈不重编号，权限外内容从文本/图形/费用及迟到响应中全部隔离 |
| RF08 operation 已 committed，但 feedback 尚不可读、页未全或历史缺失 | 玩家可以看到已获权的真实结果，同时显示 Runtime 接收未确认；不得降为失败、谎报全部反馈已消费或重新执行。末页前 partial，缺页/切点变化/未知 schema 不当作空历史 |
| RF09 执行预算耗尽或 Stop，随后有原结果和晚到费用 | 新模型/行动停止，机械查询和当前获权结果仍可消费；输入需要新调用时保留待交付，不能为“补反馈”越权启动。owner 原结果读取与撤销 executor 分别核对，无当前读权不泄漏缓存 |

每例保存各阶段的真实来源、原/当前身份、执行/读取权限、提交次数和结果数量；对于“实际 Runtime 已接收”，必须有其原 run/input 的可核接收依据。只有 tool response 已写入本地而上游无接收事实时不得通过该项。对后续决策观察实际输入/输出关联，不从同一句自然语言断言“理解正确”；新世界成果另验。

先执行 required 的真实本地持久/权限/恢复接缝与 scripted driver，再对支持的 Codex/OpenClaw 实际协议及 Web/native/pure_api 消费分别取 full 证据。fake driver 不能证明真实 Runtime 输入、玩家理解或端到端首局。

<a id="adapter-private-context-recovery"></a>
### 场景 adapter-private-context-recovery

- Trace refs: O7-REQ-4a8be58c-bcda-4efe-b199-1cf9bd9b3330；承接 [AC-EXT-008](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-008) 和[适配器私有上下文恢复](../../world-simulator/llm/external-agent-runtime-adapter.design.md#adapter-private-context-recovery)。本场景验证外部 driver、worker、既有任务恢复及报告消费者的组合，不替代认知 journal/profile 的同 provider_invocation_key 投递测试。
- 层/环境：普通合同验证使用 scripted driver、临时持久 inbox/outbox/预算记录、实际候选的任务读写与原操作路径，并消费已验证的分页、权限及回执 fixture。真实 Runtime、Viewer/pure_api 组合另按下述同案要求取证；不存在的接口、故障注入或 fixture 记录 blocked/not_run，不能用设计中的名称冒充已执行。
- 给定：受权 Agent A 的当前目标 G1、完整目标正文、明确许可与剩余预算、一个已结算成果 C、未完问答和待批申请；保留实际 driver/profile、session/run/invocation、world/task/goal/fence 及原操作关联。需要未知运行/世界结果的用例另创建原 M/P。准备同进程独立 Agent B，用于检查范围隔离；不触碰其他用户的会话或状态目录。
- 故障边界：只使本案的私有会话不可接续，保留任务、调用、操作、inbox/outbox 和预算原件；另用专门负例损坏这些原件。分别统计原调用及额外 driver start、原业务效果和预算前后值，不能用 HTTP 200、日志中的“恢复成功”或当前 UI 一句话代替实际关联。

**正常、丢失与待核对的区别。** 每例记录故障发生点、当前 S/goal/fence/audience、送入实际 Runtime 的基线、原运行核对结果和玩家可见报告。

| 用例 | 执行与可判定预期 |
| --- | --- |
| 只有观战或传输断连 | 保持专用 session 和执行方有效，断开并重连观察入口。回访仍读取原 task，执行端按原预算继续；不得自动报告记忆丢失或为同一原调用增加 start。检查原 invocation 关联与世界结果，不能以 heartbeat 单独证明继续 |
| 进程重启且会话可接续 | 在旧 run 已知终态后重启 worker/driver，先核对原件和当前完整基线再 open/resume。新决策接收 G1、当前权限和 C；C 的原业务效果仍只有一次。会话接续仅报告 driver 能核实的范围，不宣称所有旧记忆完整 |
| 私有会话明确不可接续 | 在原 run、世界操作均已核清时，使本案 resume 返回不可用。有效同设备续接许可下直接以当前基线建立隔离会话，允许一次有界重规划，不强制重复人工确认；报告保留“旧私有上下文未接续”，关联真实新 session/run/result。新计划出现不把 C 重做，也不自动产生批准或世界成果 |
| 只有旧私有内容待核对 | 让 driver 无法核实旧内容，同时能确认原 run 已结束、原世界操作允许继续。选择不使用旧记忆，从完整当前基线重规划；既不能凭超时宣称记忆已丢失，也不能只因记忆未知永久阻塞。若无法确认的是原 run，本行不适用，按下表保留 run_unknown |
| 新运行失败或没有输出 | 在合法新调用中返回真实错误、无计划的终态或只发送 heartbeat。报告区分已开始、失败/阻塞与新计划；没有匹配输出就没有“重规划已有结果”。保留原关联和预算支出，不填默认计划或 Wait 冒充模型结果 |

**交错与故障必须保留原身份。** 以下用例可复用同候选的确定性合同证据，仍要检查适配器实际消费，不要求两个真实 Runtime 重复全部注入点。

| 用例 | 执行与可判定预期 |
| --- | --- |
| 私有上下文丢失 + 原模型运行未知 | 在上游接受 M、RunRef 回包前断连/崩溃，再使旧会话不可接续。恢复先查原 invocation/session；Unknown 或 Active 分别保持核对/追读，额外 start 为 0，不换 session/key 试探。原 M 终态可确认后才按当前条件继续；取消请求或 worker 退出不替代终态 |
| 私有上下文丢失 + 世界结果未知 | 原 P 已提交但回包丢失，或返回 recovery_required。只按原 client_operation_id/context/nonce 查结果，私有会话换新不解除原副作用限制；只读核对和获权沟通仍可用。P 最终 committed 时其效果一次、原件身份未变；pending、查询失败或超时均不能改写为未执行/取消 |
| 必需正文缺失与不完整快照 | 分别删除/损坏 active_goal 正文、仅返回 revision/digest、遗漏中间页或已答未处理线程、让 cursor 失效。即使 driver 报会话正常也不得启动新决策；沿原合同 reset/recovery_required，读齐后恢复，当前获权的原操作回查及 Stop 仍可达。真正 active_goal=null 的任务保持无有效目标，不能用私有记忆补成当前目标 |
| 读取后目标、权限或分支变化 | 在基线读齐后、模型接受前、输出返回前及工具出站前分别激活 G2、Stop、换 executor/owner 或改变 branch。旧输入/报告不能改标 G2 或新 fence；新受限提交被当前权威拒绝，原结果仍按当前读权查询。新许可必须来自原授权通道，不由旧 session 或恢复报告生成 |
| audience 收窄但 session 尚在 | 保持相同进程和 session，使当前读权限不再覆盖原私有材料。旧缓存/session 不得继续供新输入或工具使用，后续读不泄露正文/计数/指纹；按当前合法范围重读后才可用隔离新会话，不能把旧记忆摘要搬进去。新 owner 同样不能默认读旧私有历史 |
| inbox 丢失与已有处理记录 | 保留服务端当前线程，删除本案本地 inbox；同时提供已保存收到/处理报告。必须重新取得完整获权正文，不能把旧 ACK 当作本进程已有内容；已处理线程不再启动原调用，旧 epoch 线程只按当前资格作历史。未完问答继续沿 collab-question-delivery 的原阶段与原操作恢复 |
| 迟到、重复与陈旧报告 | 新计划报告之后再送达旧 session 输出、旧不可用报告和重复消息；另让源时钟偏移、通信过期及报告提交回包丢失。原 message_id/观察时间不变，不按到达时间或模型时间戳把旧内容提成当前；无法比较来源则显示最近已知/待核对。没有原调用/输出依据的一般文字不升级为恢复完成 |
| 当前预算不足或原件缺失 | 让剩余预算为零、原用量 ledger 不可恢复、原 invocation/outbox 无法读取，或旧费用预留已占满本次模型限额。保持相应阻塞/保守预留，额外 start 为 0；不得创建空目录、重签 nonce、换 operation/profile 或重置累计值来继续。报告和机械回查计入原请求/时间预算，不能静默换付费路由 |
| 同 Runtime 的 A/B 隔离 | 对调 A/B 的 session、goal、来源引用、原 operation 和预算关联，或用 A 的自报正文冒充 B。错误绑定拒绝，B 不消费 A 私有材料、成果或批准；A 的恢复阻塞不扩大为 B 的失权，除非实际共用凭据/预算的失效范围确实覆盖 B |
| 报告保存与隐私 | 观察 report 保存前后崩溃、断连重发和玩家换设备/身份；未确认保存不显示已送达，人未查看不显示已读。报告沿当前获权渠道可回读，正文/完整编码受原限额约束；公开记录不含完整 Prompt、记忆、推理链、秘密、cursor/签名、私有 digest/路径或任意结果 URL，玩家端无新增持久正文缓存 |

**每个真实 Runtime 的同案取证。** OpenClaw 与 Codex 各自在声明的版本/profile 上分别覆盖连接中断和进程重启，并至少完成一次本案专用私有会话不可接续后的可信重规划。可复用同候选 [collab-session](#collab-session) 的实际阶段，不能以另一个 Runtime 的成功补缺。用获准的临时会话或受支持的故障注入制造不可接续，保留原调用及世界操作记录；没有安全可用方法时如实保留该缺口，不删除用户会话或伪造 driver 结论。

每案需能关联 `Runtime/profile → 原 session/run/invocation → 原 world/task/Agent → 当前 S/goal/fence/audience → 新会话的实际输入与输出 → 恢复报告 → 原 operation/receipt`。核对实际 driver 消费了完整当前目标、权限、未完义务和已确认结果，报告和实际运行来源一致；只观察到模型自述“记得”不构成这条链。实际 Viewer 和 pure_api 输出分别检查来源、时间、限制、下一步及世界结果分层，按所声明入口留证；Web/native 不相互代签。

原证据保存在受保护测试记录中，公开 PR/CI 只保留不可反用的脱敏关联、次数、范围和结论；不能为了核对输入而公布完整目标/Prompt、私有正文、可重放 cursor/签名或可枚举私有摘要。新计划、服务保存报告和已有世界成果分别取证；本场景不证明旧记忆完整、模型内部理解、跨 Runtime 私有记忆迁移或公开发行就绪。缺实现、真实 Runtime、许可、预算或对应消费者时记录 blocked/not_run，文档检查和 scripted driver 通过不填补这些缺项。

<a id="adapter-liveness"></a>
### 场景 adapter-liveness

- 层/环境：Worker / process lifecycle。
- 给定：用户明确启动的 worker 和一个打开的 Viewer，另有不属于测试的运行进程。
- 执行：先关 Viewer，再停 worker，再模拟健康检查成功但 driver 未启动及 heartbeat 过期。
- 判定：关 Viewer 不停仍有效任务；停 worker 后不声称继续推理；不杀其他进程，健康与实际执行不同。
- 四层输入：固定每个 observer 连接、实际 driver run/source time、当前 task/delegation/fence/额度，以及原 pending/committed operation；分别读取 [四层设计](../../world-simulator/llm/external-agent-runtime-adapter.design.md#adapter-liveness) 的来源，不能以一个 online 字段作 oracle。下列 LV01–LV06 在 `viewer` 与 `pure_api` 分别成案，Web/native 各有自己的实际入口证据。

| ID / 改变的唯一条件 | 必须观察到的分层结果 |
| --- | --- |
| LV01 只关 Viewer / 断开观察客户端，其他独立条件有效 | observer 离线，实际 run 继续或按真实条件等待；原操作结果仍由世界产生。回访读取原结果，不自动启动第二个 worker/任务 |
| LV02 停 owned worker/driver，但 grant 尚有效 | 新决策停止；资格可以仍有效，世界 pending 可继续，已确认成果保留。其他不属测试的进程存活 |
| LV03 健康检查成功、控制 sync 正常，但没有 started run | 显示 awaiting/等待或实际诊断，不显示 thinking；没有模型调用事实也不报正在运行 |
| LV04 heartbeat/driver 来源过期，之后重连 | 来源变未知/通信过期，不由墙钟撤销世界资格。恢复先核原 run/operation 与当前 fence，不复用旧报文填 fresh |
| LV05 执行 grant 到期/撤销或模型额度归零，远端取消不可证 | 新受限请求停止，取消状态/费用未知保留；原结果按当前读权可查，不能宣称远端已停止计费或旧行动已取消 |
| LV06 执行实际寄生于页面，或读取资格也撤销 | 前者明确不支持独立无 GUI 组合；后者清除旧私有图形/文本/费用并拒绝迟到结果。两种失败不能由另一个入口或其他 owner 的权限代签 |

- 代际恢复：重启和只重连 observer 分开；前者按合法 CAS 恢复实际执行代际，后者不改变控制权。比较旧新 fence、预算与 feedback 位点，无复活旧授权或重复效果。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="adapter-budget"></a>
### 场景 adapter-budget

- 层/环境：Worker / timeout / cost。
- 给定：fake driver 提供延迟、未知上游状态、缺失 usage 与不响应取消的子进程。
- 执行：用有限任务预算运行重试、取消和进程恢复；OpenClaw CLI profile 验证 timeout 单位是秒，不自动 --local。
- 判定：总 deadline 不重置；只结束自有进程；unknown/partial 如实披露，无盲目第二次推理或自动充值。
- 量值与去重：fake driver 在同一 meter 依次报告累计 input_tokens=100/150、output_tokens=20/30、cached_input_tokens=40/60、cost=0.01/0.02 USD。交错 poll/stream/reconcile 重放、乱序旧 sequence、相同 measurement_id，以及消费持久化前后的崩溃恢复；结果应是 150/30/60 和 0.02 USD，而非相加后的 250/50/100 或 0.03 USD，总 token 不重复包含 cached 子集。
- 范围与缺失：加入另一不重叠 meter 的 50/10 token 与 0.03 USD，任务已知合计为 input=200、output=40、cost=0.05 USD；其他币种单列，不叠加父运行汇总与已计入子项。只报告 token 而缺 cost 时保留已知 token 和未知费用；无 usage 不等于零。
- 负例：同 ID/sequence 异内容、累计值倒退、冲突终值、非法单位/币种/小数、未知指标、超过条数/字节限制与无法证明作用域不重叠，必须拒绝或标计量未核对且不静默增加可用预算。delta-only driver 先持久去重后规范为累计值；本地 ledger 丢失不能重置消费。估计/预留与实际报告分列，计量完成不由取消请求推断。
- 世界成本与读权按[预算合同](../../world-simulator/llm/external-agent-runtime-adapter.design.md#adapter-budget)再独立执行下表，不能只测 usage 字段。样例金额与资源数量是测试输入，不是游戏价格或产品参数。

| ID / 固定输入或故障 | 独立预期与原记录核对 |
| --- | --- |
| BG01 原世界业务实际消费 4 ore、8 Electricity；另有 2 单位占用，其中实际释放 1、剩余 1。外部两无重叠 meter 分别 0.02/0.03 USD，另有 0.01 EUR | 显示世界消费 4/8、当前占用 1及原释放，不把 reserved 再算消费；外部已知 USD=0.05、EUR=0.01 分列，不换算为世界资源或一个完整总额。与原 payer/ledger/job/receipt 逐项对账 |
| BG02 在 BG01 中重放排程、completion、查询和 UI 摘要，穿插另一无关任务余额变化 | 相同 effect/receipt 行只计一次；不从余额差归因本任务，不把模型自然语言成本当世界账。跨切点晚到费用保留原来源，不重复相加 |
| BG03 有 token，无实际 cost；未来维护估计 0.08 USD；又有一个下游工具费用未知 | 保留已知 token/实际分项、估计与未知，总费用仍不完整；不能把未知写零、把估计记实际或声称全链硬限额 |
| BG04 模型/行动预算耗尽，原世界 action 仍 pending，后来 committed | 停新模型/行动；原结果查询仅用独立有界读取配额，原结果和晚到成本可在当前读权下补齐。不得为读结果充值、重绑 executor或重发 action |
| BG05 Stop/撤销 executor 执行权，再分别保留/撤销其读取权；owner 仍合法/也失权各一例 | 只按当下权限读取；无权不暴露历史存在性/旧缓存，owner 通过独立受保护原件查询且不复活 executor。缺可交接原件时明确阻塞，不共享旧私钥或私有 outbox |
| BG06 控制查询限流/离线、正在 long-poll 时 Stop、重启或旧备份恢复 | 读取至多一个在途并按 retry_after 退避；Stop 不等模型/行动预算恢复。只查原件，无无限重试/新模型；已用预算/未知预留/世界占用及当前 fence 不被旧本地代际重置 |

- 必须分别记录 worker 能强制限制的范围、Runtime 内部不可拦截范围、世界 authority 实际扣减和各数据缺项；完整账单审计与实际付费 Runtime 运行另有证据，不由 scripted meter 代签。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="skill-release"></a>
### 场景 skill-release

- 层/环境：Artifact / installation / 两 Runtime。
- 给定：受信发布来源的 Skill、manifest、固定版本 driver 与有同名自定义 Skill 的临时 workspace。
- 执行：核对 manifest/source/hash，逐 Runtime 安装/加载；注入 zip 越界、symlink、篡改说明、API major 不匹配。
- 判定：拒绝不可信/越界/不兼容资产，不覆盖用户文件；安装不自动保活或执行高权脚本，真实首次调用可回读。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="viewer-dual"></a>
### 场景 viewer-dual

- 层/环境：真实 Web + native / S6。
- 给定：同一世界和 Agent 成果，实际浏览器及支持 OS 的原生安装包，记录 build hash。
- 执行：分别进入、缩放定位、选择查看、发布高层目标、关窗口再回访。
- 判定：两者在自己界面有真实交互和同义结果；native 弹系统浏览器或仅截图均失败。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="viewer-explore"></a>
### 场景 viewer-explore

- 层/环境：World projection / visual integration。
- 给定：多区域/Fragment 世界，有可见 Agent/设施/路线和不具备坐标的对象。
- 执行：世界总览→局部→搜索对象→选中→回到任务，模拟局部加载失败。
- 判定：上下文和返回路径连续，抽象位置有标注，未知/失败不画成精确实时世界。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="viewer-guidance"></a>
### 场景 viewer-guidance

- 层/环境：Owner UI / API contract。
- 给定：自己的 Agent、可见但不可控 Agent，pending/committed 与目标新旧版本。
- 执行：从地图修改自己的目标、回答消息/审批，点击其他实体及资源地点。
- 判定：仅高层有效写入；地图点击不发 action:submit，接受/应用/回执分开。
- renderer 组合：按[恢复合同](../../world-simulator/viewer/external-agent-world-view.design.md#view-recovery)分别在 boot 失败、ready 后 context loss/fatal、断连及显式重试时，保持原 pending/committed 和当前目标。核对获准文本的来源、原 Agent/operation、unknown/stale 标记；高层指导、Stop 与原结果查询只按各自当前权限可用。自动恢复最多一次，重建后须真实命中/选择及当前授权投影可用才能 ready；没有新 action、第二份效果或自动重启 Runtime。
- 权限竞态：图形失败期间先换选中 Agent，再换账号/audience 或撤读取权，并让旧 promise 晚到。前者原结果仍归原主体；后者清图形/文本/费用/缓存与旧命中，旧响应不得再显示。可读历史保留与当前无权展示分别判断；不能因 renderer 不可用放松授权。
- 逐端证据：正式 Web 与受支持 native 嵌入容器各自执行上述路径，记录恢复前后文本/交互、实际容器、原请求与权威结果；受控 renderer 故障不代签真实 GPU 正常可用，另一端、外部浏览器跳转和截图不代签本端。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="viewer-primary"></a>
### 场景 viewer-primary

- 层/环境：Viewer / S6 readability。
- 给定：带当前目标、选中对象、关键 blocker 的正式场景，桌面及窄屏。
- 执行：切换详情/图层、密度和任务反馈，观察默认阅读顺序与键盘操作。
- 判定：世界与目标/下一步可找到，诊断次级；不以环境动画代签因果。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="viewer-freshness"></a>
### 场景 viewer-freshness

- 层/环境：Data client / deterministic concurrency。
- 给定：源快照、旧请求回复、新 audience revision、失效 world cursor 与一次分支变化。
- 执行：安排旧响应最后到达、撤销读取权限、重连并更新 world/task 流。
- 判定：旧 generation 不覆盖新视点或权限；不同流不伪装原子状态；显式清缓存/reset 并标时效。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="viewer-scale"></a>
### 场景 viewer-scale

- 层/环境：Projection / load profile。
- 给定：固定 50000 对象测试世界和受限 audience，视点实体超过单次限额。
- 执行：快速平移/缩放、搜索/跟随并解除；测帧耗时、P95 查询和缓存。
- 判定：分页/聚合和 partial 诚实；上限不丢选中/目标，输入与 stop 不被数据洪泛阻塞。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="viewer-native"></a>
### 场景 viewer-native

- 层/环境：Native package / trust boundary。
- 给定：受支持 OS 的真实 WebView host、有效 release build、恶意导航/IPC 输入。
- 执行：在程序内进入世界；尝试任意 URL、Origin:null、token 深链、通用 shell IPC；模拟 optional payload 缺失。
- 判定：native 实际绘制交互；隔离来源与最小 IPC；缺资源明确降级，不开启本机后门。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="viewer-authority"></a>
### 场景 viewer-authority

- 层/环境：Viewer / permissions / no-effect。
- 给定：两个不同所有权的可见对象，真实资源与初始 world root。
- 执行：执行平移、选择、跟随、过滤和地形点击，再对自己 Agent 提交合法目标。
- 判定：前一组无世界副作用；后一组只走 owner 目标控制，世界效果来自后续 Agent 与权威回执。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

## 实现后的建议自动化拆分

parser/schema 与权限 matrix 使用单元/合同测试；context/operation 与消息补送使用临时持久库和 deterministic fault injector；fake driver 测启动/unknown/cancel，不联网调用模型；Viewer 纯数据层测乱序、缓存与权限降级。上述能在普通 CI 验证的项不重复包装成昂贵集成门禁。

只有真实 Runtime 策略和工具执行、实际 browser/native 资产交互，以及真实部署恢复需要对应环境。实际测试代码/fixture 应在实现 PR 回链本手册场景 ID，本次文档不伪造尚不存在的测试函数或可执行命令。

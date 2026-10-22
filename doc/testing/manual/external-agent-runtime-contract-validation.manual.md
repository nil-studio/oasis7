# 外部 Runtime 游戏接口与协作验证手册

- 适用基线：`03fb58784a65de9f02a43dfeafc3e882bb04e5f2` 之后按四份系统设计实现的候选；本手册是验证计划，不是当前运行结果。
- Owner role：`qa_engineer`；日期：2026-10-10。
- 总测试策略：[testing-manual.md](../../../testing-manual.md)；设计入口：[系统设计导航](../../world-simulator/llm/README.md)。

## 执行边界

设计阶段仅运行文档结构、链接、schema 示例和一致性检查；以下 API、driver、native 场景在实现存在后执行。不存在的端点、安装包或 test fixture 必须记为未覆盖，不生成绿色证明。普通 CI 应先覆盖确定性合同和故障注入，缺测试先补普通 CI。付费模型、真实机器持久恢复及正式 Web/native 体验只在获准环境和预算下验证，不让每个普通文档 PR 调用模型。

所有场景记录 source HEAD、测试合并树（若有）、世界/玩法版本、Runtime/driver/Skill/API/Viewer build、平台、许可和测试开始结束状态。保留脱敏请求、操作 ID、world receipt、故障点、控制版本及实际结果；不保存密码、token、私有推理链或其他任务数据。只清理本轮创建的进程/任务/世界目录，不改线上世界或他人会话。

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

<a id="api-wire"></a>
### 场景 api-wire

- 层/环境：HTTP contract / 不调用模型。
- 给定：已实现新 Game API 的服务，普通 HTTP 客户端，不启动任何 Provider Bridge、OpenClaw 或 Codex。
- 执行：按发现→鉴权→task/context→query→action→operation→events 顺序请求，再验证 query-only 后用 context outcomes 进入等待/收口，覆盖非法 JSON、未知字段、超大整数和错误 schema。
- 判定：状态/幂等/版本可消费，accepted 不代签 committed；只读无世界效果；旧 provider DTO 不被当新请求。
- 消费 DTO：验证 active_goal 完整正文与 null 条件，以及 OperationView 的 recovery_required/recovery/retry_advice 能被严格解码；不识别这些必要语义的旧草案客户端应停止，不默认成功或盲重试。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

<a id="api-safety"></a>
### 场景 api-safety

- 层/环境：API / safety / deterministic races。
- 给定：两个主体、一个累计 grant、多个连接与并发动作，构造已提交的撤销和新目标。
- 执行：并发消耗 grant，在 commit 前撤销或转让；重连重试，模拟 auth unavailable、跨 origin cookie 请求和任意 callback URL。
- 判定：累计额度不复制；新受限动作在权威生效后拒绝；CORS/CSRF/route 限制生效，无新写 fallback。
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
| 空集、200/201/256 个同类项 | 分别只放问题、只放申请；200 项可能因字节提前分页，201/256 项不得被 200 的列表上限拒收或截断。最终唯一 ID 集合与 S 上授权集合完全相等，最后页单独不能初始化 |
| 混合集合 | 128 个问题 + 128 个申请，另有未结算操作和当前报告；用 limit=1、200 验证跨集合边界、顺序、total_counts 与原 operation 关联，不能只读问题列表 |
| 最大编码边界 | goal 4 KiB、message/report 8 KiB，包含引号、反斜线、控制字符、多字节字符；另测 64 KiB 当前项/头部及超出一个字节。整个响应不超过 2 MiB，metadata 不能绕过检查，单项不切片 |
| 条数与大小拒绝 | 第 257 个待决项、非法 limit、超大新记录在接受前明确拒绝且无持久副作用；256 个既有项及其正文保持不变。旧草案过大记录保留原数据并 recovery_required，不伪完整 |
| 重试、乱序与断连 | 重复同 cursor 返回同一页；客户端重复页幂等，缺第零页/中间页、另一 snapshot_id 或被改写头部不能提交基线；重试不延长 pin 寿命 |
| 页间新增、回答与批准 | 取得第一页后新增问题、回答旧问题、终结待批申请；其 S 上旧形态仍在原页，变化恰好经 after-S 接续。应用增量后不复活已回答/终结项目，不重放批准或原世界副作用 |
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
- 给定：同一 task 的 Q1 绑定旧目标，Q2 绑定当前目标；Web 和桌面重复回答。
- 执行：回答旧 Q1、重复发送同内容/异内容 message_id，发送“同意”及包含指令的世界文本。
- 判定：关联和去重正确，旧答复为 stale；文字不自动改目标、不生成 grant、不改变工具权限。
- 证据：候选标识、脱敏交互记录、适用控制版本与实际世界结果；失败保留错误原因，不替换为一次重跑的成功截图。

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
- 判定：S 与 after-S 没有静默缺口；过期显式 reset；private 数据不串线，pending 当前项不会因传输日志压缩丢失。完整度和容量联合 [collab-snapshot-pages](#collab-snapshot-pages) 验证，不能只测第一页。
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
| 问题与回答 | 在 G2/E2 下产生 Q2 与 message_id，owner 的回答以 in_reply_to 指回该问题；旧 G1 的问题/答复若仍存在，保持 stale，不替换当前目标；普通“同意”不生成 grant |
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

<a id="adapter-liveness"></a>
### 场景 adapter-liveness

- 层/环境：Worker / process lifecycle。
- 给定：用户明确启动的 worker 和一个打开的 Viewer，另有不属于测试的运行进程。
- 执行：先关 Viewer，再停 worker，再模拟健康检查成功但 driver 未启动及 heartbeat 过期。
- 判定：关 Viewer 不停仍有效任务；停 worker 后不声称继续推理；不杀其他进程，健康与实际执行不同。
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

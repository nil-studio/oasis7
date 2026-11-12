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

#### 准备固定公网 authority 的隔离夹具

`scripts/package-ecs-cold-entry-fixture.py` 的 `fixture` 支持显式 `public_origin`、`rp_id`、`bootstrap_tls_port` 关键字参数。仅用于已经获准的全新隔离 authority 准备；不启动隧道、部署或复用旧 world。先取得并固定可验证的公网 HTTPS origin，再首次 setup；RP 必须精确等于 origin 的 DNS host，不接受路径、userinfo、query、fragment、IP host 或越界端口；默认 HTTPS 端口必须省略，显式 `:443` 与 native canonical origin 约定不符。

```python
# root 必须是新建的 0700 私有目录；bundle/probe 固定到获准候选。
# local_tls_server 由正式 smoke.generate_tls / smoke.forwarder 创建，使用 root.crt。
smoke.variables["origin"] = f"https://localhost:{local_tls_server.server_port}"
setup = fixture.fixture(
    smoke, bundle, probe_binary,
    public_origin="https://authority.example.test",
    rp_id="authority.example.test",
    bootstrap_tls_port=local_tls_server.server_port,
)
```

本地 bootstrap 固定连接 `localhost`，以 `root.crt` 校验证书和主机名，同时传递固定公网 Host；不跟随跳转。身份环境、RP 快照及客户端 endpoint/audience 使用同一公网 origin，公网客户端使用正常系统 TLS 信任，不借用测试 CA 或关闭验证。未传关键字时维持原 localhost 行为。

首次 provision 前以独占创建方式固定 `fixture-authority.json`。已有绑定声明、world、node keys、身份数据库或授权文件时拒绝重新 setup；失败后保留原目录，不能删除声明、改 origin 或 rebind 后重试。后续恢复使用原 argv/env、密钥及 world pin。此处普通装配回归不代签真实公网连接、World 操作或真实 Runtime 冷入场。



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

<a id="api-intent-results"></a>
### 场景 api-intent-results：通用待决、终态与原关系读回

Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405, O7-REQ-78948fd8-29dc-4e9b-8a38-321ed37b7ad6。承接 [AC-DWE-002](../../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-002) / [AC-DWE-003](../../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-003)、[API 目标映射](../../world-simulator/llm/external-agent-game-api.design.md#api-intent-results)及 [Viewer 原成员显示](../../world-simulator/viewer/external-agent-world-view.design.md#view-intent-results)。本方法以真正采用 SR2 原件、持久决策与结果适配的同候选为前提；当前五态 registry、volatile Received、native Chat superseded 或手填 OperationView 不满足该前提。

从 [GWSC PL01–09](../longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-pending-lineage-combination)取得真实受信原件和合法 profile/proof，固定 source/tree/world/window、API schema、具体消费者版本与当前读权。两个主体分别拥有自身原 operation，并设置允许读取/撤销读取的关系对象。独立预期先来自原 domain 规则与实际 canonical decision，再比较 API/Agent/Viewer，不能从被测转换器生成同一预期表。此处只核消费者；world 原子性、持久准入与合法 proof 仍由 GWSC 对应场景验收。

| ID / 执行输入 | 必须读取和断言的结果 |
| --- | --- |
| IR01 逐项真实处置 | 分别提交/取得已知无效果 pending、committed_effect、rejected、expired、replan_required、terminated_without_effect，并另造原提交未知。两种原 operation lookup 返回准确目标状态/原因/next step；committed_effect 需要真实执行 receipt，其余无效果不填 module success receipt；未知只 lookup_original，不用 expired 或普通 failed 收口 |
| IR02 原件关联与冲突 | 使用不同 operation ID 与 intent ID，原同键重复后对照持久映射；混入另一 world/branch、subject、request hash、winner/receipt 或无效 proof。只接受原 ingress→intent→decision 的完整匹配；不按 ID 字符串、当前 Agent、较新 goal 或库存变化补关联；失败不重签或产生第二效果 |
| IR03 恢复与当前权限 | pending/未知/已证终态分别重启读取；到期、Stop、执行失权但保留 receipt 读权，再撤销读取权。合法只读可核原件且不重启模型/行动；失去读权的两种 lookup 在暴露对象存在性前拒绝，旧 cache/迟到响应不显示。缺原件/历史与原请求未发生分开，已证历史不由查询故障改写 |
| IR04 前驱、胜者与独立成员 | 双序 winner 和已知/迟到 loser、无效果 withdrawal、自身 rejected/expired/replan、合法独立 intent 分别查询。后续 winner 出现后再查询/重试已有终态，原成员仍保留原处置/原因/位置，不被改为 loser；直接 predecessor/winner 必须匹配原 domain 的同 lineage/set，不能拼接其他合法前驱。分别制造 related available/not_readable/history_unavailable，受限项不含 ID/hash/状态/receipt；没有关系才省略，不把不可读说成原件取消或不存在 |
| IR05 实际消费者与行动边界 | pure_api、Agent adapter、Web 和 native 对同一受权结果各自读取/返回/重连；Viewer 切选择/身份，Agent 另遇反馈尚不可得。显示/结构化结果保持原关联及业务阶段；查看关系只读，不隐式发撤回、替代、outcome=abandoned、新 nonce 或付费调用；有真实 cognition record 才交付 RF 原输入，SR2 投影不制造 feedback |
| IR06 协商与有界展开 | 新旧 schema/客户端分别读取；未声明或未协商 sr2_intent_results、未知状态、超界字段、长 lineage。只有受支持组合使用目标新态；其余明确能力/协议阻塞并保存原键，不错误折叠。一次只展开原件及至多一个直接 predecessor/winner，进一步查询重新鉴权且有界，无隐式全 lineage 枚举 |

记录真实 source/proof、原 operation/intent/关联、各查询的当前 actor/scope、目标状态与原因、请求次数及世界 effect/receipt 对照；公开材料脱敏，私有原件按原证据权限保留。API bytes、Agent 实际接收、Web/native 文本与真实交互分别判定；只有 API fixture 不代签图形入口或真实 Runtime。required 执行真实适配/持久/权限接缝，full 仍按原候选与环境取得各适用消费者结果；不为文档阶段造端点、启动模型或声称方法已运行。

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

<a id="api-primary-entry-validation"></a>
### 场景 api-primary-entry-validation：逐入口能力与恢复适用性

- Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264；产品 [REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001) / [AC-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#ac-entry-mode-001)：当前入口的唯一 primary mode、阶段、能力及独立证据。
- Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；产品 [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) / [AC-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#ac-entry-mode-002)：前置、身份、provider、模式或后端失败时的真实分类和恢复。
- 系统条款：[实际入口适用性](../../world-simulator/llm/external-agent-game-api.design.md#api-primary-entry-applicability)。本方法补齐入口消费与失败组合；[FS01–07](#api-first-session)、[普通 HTTP](#api-wire)、[原操作幂等](#api-idempotency)及[共同安全边界](#api-safety)继续拥有其原验收范围。

#### 固定候选、实际入口与独立判定

对候选声明提供的每个入口，记录真实客户端及启动方式、transport、world/backend/profile、经验证主体与权限、实际协商能力、primary mode、当前阶段和可执行操作。primary mode 沿用 `viewer` / `pure_api`；observer、blocked 是该入口的能力/可用性结论，SDK、provider、部署方式和管理通道不成为新模式。把协议存在、生产者已实现、消费者已接通、当前实际执行四层分别记录；临时脚本或 mock 只能证明其接缝，不能填作正式消费者。

2026-11-05 固定源码基线为 `c67298dcfc08ce0eaece293bd30c184980ac5327`。以下是该基线的静态接点和后续候选的验证方法，不是运行通过记录；候选改变接线后须按真实源码修正适用范围。

| 已有接点 | 当前可确认的范围与必须保留的缺项 |
| --- | --- |
| [TCP CLI 主程序](../../../crates/oasis7_client_cli/src/bin/oasis7_pure_api_client.rs)与[JSON/终止条件](../../../crates/oasis7_client_cli/src/bin/oasis7_pure_api_client/support.rs) | 已有 snapshot、step/play/pause、chat/chat-retry、gameplay-action、prompt apply/rollback、agency-control 及 register/reconnect/rotate/revoke session。HelloV2 当前只声明 canonical chat 能力。Step 匹配本地 request ID 的 completion ACK；Play/Pause 按收包时窗结束，Pause 不带 request ID；gameplay、Prompt、recovery 的通用终止谓词主要按响应类型，agency-control 的原始 JSON 通道只核响应类型。这些事实不证明全部结果已精确关联或具备持久恢复。 |
| [canonical chat](../../../crates/oasis7_client_cli/src/bin/oasis7_pure_api_client/canonical_chat.rs)及[原件保存](../../../crates/oasis7_client_cli/src/bin/oasis7_pure_api_client/canonical_chat_pending.rs) | 协商 canonical 能力后有发送前原件保存、原签名重送和对应 intent 回执校验；未协商时仍是独立 legacy 分支。此机制只适用于原 chat 合同；不能推广成其他命令的通用 retry，也不证明目标已经执行。 |
| [玩法原结果查询](../../world-simulator/viewer/viewer-control-plane-split-live-playback.design.md#gameplay-original-result-lookup) | Runtime/浏览器已有协商后的只读原请求通道；当前 CLI 没有接入 `LookupGameplayAction` 命令及其恢复消费。该行记为 CLI 当前恢复缺项，不能因协议可解码、另一消费者可查或补跑 snapshot 而记为通过。 |
| [Game API 首局组合](../../world-simulator/llm/external-agent-game-api.design.md#api-first-session-composition)与[SDK 原操作 port](../../world-simulator/llm/agent-game-client-boundary.design.md#original-operation-lifecycle-port) | 目标允许普通 HTTP 客户端独立消费 Game API；当前 `/v1/game` bootstrap 仍仅 GET、`writes_available=false`。底层原生写能力不开放该 HTTP 路由。SDK/adapter 提供窄原件与状态能力，需核实际调用者的存储、授权、transport 和编排。其合同不自动补齐上述旧 CLI，也不单独证明完整 pure_api 首局或两种真实 Runtime。 |

沿“统一准备与判定”使用 A/B 两个合法主体及各自 Agent，在隔离候选里选择该入口实际支持的一个业务请求 R；测试侧先固定其完整参数和预期领域结果。客户端只能用公开且获准的观察，独立判定侧从适用 authority 的状态/日志/receipt 核实原请求身份、阶段、效果次数及权限。比较控制变更、业务提交、世界效果和领域后续完成这几类差量；只读的“零效果”指该请求没有新业务提交或效果，不要求正在运行的世界整体停止变化。

#### 逐能力正常与失败样例

每个实际声明的入口分别执行适用行的正常与负例；没有该能力就记录准确的未提供/受阻范围；完整组合必需的能力不能靠从清单删去来判通过，也不要求每个 proto 变体新增 CLI 命令、预检或权限。下表中的 readback 仅使用当前授权和该领域原有查询能力。

| 样例与实际范围 | 正常操作及独立判定 | 失败、错配与恢复反例 |
| --- | --- | --- |
| PE01 模式、阶段与入口前置 | 在当前支持的客户端组合中完成发现、身份确认及一个可确认结果的操作；对照原声明阶段、能力及证据范围核对真实后端和结果；本地成功不自动升阶或扩大 claim。正式 pure_api 的完整首局继续按 FS01–07；同候选 Viewer 的结论另案记录。 | 分别移除必要凭据、令必需 provider 初始化失败、指定不支持模式、使后端不可达；缺哪个前置就如实阻塞依赖它的能力，仍获准的读取可继续。不得自动换 provider/模式后沿用原成功结论，也不得把链 observer、direct debug 或管理连接当成玩家 observer。恢复配置或身份后重新核当前范围；修复成功不补造故障期间进展。 |
| PE02 snapshot / player_gameplay | 对同 candidate、world、身份及 as-of/cut 的获准投影，与 Viewer 比较 accepted intent、execution status、primary reason、next step，沿 [DES-WR-IA-008](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-008)。核对读取没有业务 submit；自然推进另按其原因归属。 | 注入较旧投影、A→B 身份切换、另一 world/reorg 及缺少原 R receipt；不同 cut 先标 freshness 差异，不能强行比作同一事实。新余额、空 mailbox 或当前 status 不证明 R 已提交或未提交；不得由 snapshot 结束未知原请求或泄露 A 的私有内容。 |
| PE03 step / play / pause | 先按 `HelloAck.control_profile` 和真实 handler 确定适用控制语义。对 CLI 实际发送的 LiveControl Step，核对原 request ID、completion status 与真实进展；Play/Pause 按[新决策调度合同](../../world-simulator/viewer/viewer-control-plane-split-live-playback.design.md#live-control-state-design)核对当前连接/epoch/revision 与调度状态。Playback 若是被测入口，则另按其原 PlaybackControl 合同验证。 | 给 Step 混入另一 ID 的 ACK；给 Play/Pause 注入旧 epoch、较低 revision、迟到确认、断连或未协商 peer。不能把计时收包完成、任意 ACK 或零世界增量显示成动作成功；live Pause 只证明其调度作用域，不证明世界时钟/在途模型/已有任务均停止。当前 CLI 缺精确消费的部分保持未验证，不要求另造 playback 命令。 |
| PE04 chat / chat-retry | canonical 能力下发送有明确意图的 R，在权威提交后丢弃回复并退出客户端；用获准保护的原件文件恢复。逐字节比较原签名 payload、nonce、world/branch/reorg、Agent 与 intent tick/seq；按原合同取得关联的 intent ID/accepted event sequence，独立确认只产生一个原 intent。accepted/blocked 的 intent 回执与后续 applied/领域完成分别判定。 | 在首次发送前使原件持久化失败应不发；恢复时损坏原件、换 endpoint/world/身份或撤权应停留在原身份/真实错误，不换 nonce 消除未知。混入另一 chat ACK/无关 AgentSpoke 不得完成 R。legacy 未协商路径单独登记，不能从 canonical 的原件保存、重送或回执合同推出 legacy 等价。原件含敏感内容，不放入公开测试记录。 |
| PE05 gameplay-action / 已有原结果查询 | 选择当前入口真实支持的 action，固定 authenticated actor、action/target 和原签名 nonce；直接结果按精确原关联及领域 receipt 判定。对已接入 lookup 的消费者，在提交后丢回复、重启并取得当前读取身份，再用原 signed request 只读查回：查询 submit 次数为零，R 的效果总次数为一。结果 committed 与后续产出/交付分开。 | 错 actor、target、nonce、world/branch/reorg 或切换后的迟到回包均不能结束 R。仅声明身份的 ReconnectSync 不建立 gameplay 查询读权；缺 capability、历史不可用、限流或当前无读权保留原件与未知。当前 CLI 调用 builder 时 actor 固定为 None，对要求 actor 的 claim_agent/release_agent_claim 不能宣称已接通；按该真实前置得到拒绝，不代填别人的 Agent。通用 ACK/错误类型和随后 snapshot 不补齐精确原结果查询；不得把重新运行 gameplay-action 产生的新签名当成恢复。若该入口已有 quote，则按[原预检范围](../../world-simulator/viewer/viewer-control-plane-split-live-playback.design.md#preflight-family-applicability)核对同 authority/参数及漂移，不给未提供 quote 的动作补造预检。 |
| PE06 prompt preview / apply / rollback | 在该候选实际协商的 [Prompt 合同](../../world-simulator/viewer/viewer-control-plane-split-live-playback.prd.md#prompt-control-request-identity)内，对同 Agent 与 expected_version 先 Preview、再显式 Apply，随后按原权限显式 Rollback；增强合同逐项核对 request ID、operation digest、authority/session/binding epoch、状态与 scope。以 authority 的 profile/version/ledger 判定 Preview 无配置应用、Apply/rollback 的真实 mutation；本实例应用不升格为持久化或跨入口同步。 | 同基线并发更新分别得到原合同的 applied/stale；撤权优先于版本检查，当前不可见值保持 hidden。原 ID 异内容为 conflict；实例重启后的旧 epoch 按原合同保持 result_unknown，不借 gameplay lookup 恢复 Prompt。Preview 可以消费自己的认证 nonce/有界结果元数据，不能消费另一次 Apply 的 nonce或修改实际 profile。当前 CLI 的 legacy builder/类型终止条件不能代签增强字段、关联或持久恢复已接通。 |
| PE07 agency-control | 对当前 owner 的 signed Inspect 验证授权过滤的投影；对实际支持的本地 profile 用一个有限 grant 做 Install、同原件 exact replay、Revoke，核对原 grant ID/revision、预算与控制结果。沿[owner delegation 原合同](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#owner-delegation-result-replay-contract)区分新 nonce 应用与旧 nonce 只读匹配；Inspect/匹配不安装、不扣世界行动预算，Applied 只表示该控制变更。 | 用 B 身份读 A、旧 owner/source、同 ID 异 terms、错误 revision 或范围不支持分别核原拒绝；chain-linked local mutation 保持 `canonical_agency_control_unavailable`。被撤销的 install 不能借 replay 恢复为成功；不把 install/revoke 的当前历史匹配推广给 memory correction、interrupt/override 等其他命令。只核 JSON type 的 CLI 输出还需与当前 request/Agent/world 和 authority 结果对账。 |
| PE07b 已采纳的记忆纠正与在途控制 | 在该 profile 原本支持的路径上，按[记忆纠正](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-007)提交匹配 memory ID/revision 的 correction，核 correction identity、影响范围和后续真实结果；再对原 pending intent 按[在途控制](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-012)显式 interrupt/override，核原 intent/digest、control ID、当前 owner/grant revision 与控制处置。 | 错 memory revision、已完成 intent、旧 owner、撤销 grant 及实际世界/安全 hard blocker 各按原合同拒绝或报告 stale/ignored；纠正被接受不保证下一动作改变，owner override 不能跨越硬阻断。响应丢失时按各原命令可用的原件核对，不能套 install/revoke 的 matcher 或用新 control ID 消除未知。 |
| PE08 register / reconnect / rotate / revoke 与恢复入口 | Register 按原 signed proof 及适用 registration grant 建立当前 session；核对获准绑定及 epoch。Reconnect 只按其合同读 cursor/reorg/session 信息，不把声明的 player/key 当作新认证。Rotate/Revoke 仅在当前 profile 已授权的管理入口验证：核对原目标 key、实际 epoch/撤销结果与新动作资格；与[会话和 intent 连续性](../../product/player-entry-distribution/prd.md#player-session-authorization-continuity)分开对账。 | 普通玩家连接不得借管理命令取得授权；仅 reconnect、旧 key、过期 grant、错 world/reorg 或身份切换不得恢复旧执行资格。会话变更的响应丢失也不能凭任意 recovery ACK 或 snapshot 断言原命令完成；先用原合同允许的当前状态/结果读取，无精确关联能力则报告原结果未知。已提交 intent/receipt 保留，重新登录不复活过期 Agent 委托，也不把新会话当成已取消原 R。Rollback 等其他 proto 运维变体按原适用入口验证，不因 CLI 的四条 session 命令而宣布全部 recovery 已支持。 |

#### 原件、授权漂移与未知结果的组合

下面各故障固定同一原 R 和独立 authority oracle；在实际提供该能力的消费者执行。没有原件持久化、当前读权、查询端口或精确关联能力的现有入口，记录具体缺项及安全下一步；不得借测试 helper 接通后覆盖该缺项。

| 窗口/对照 | 注入与可核对的结果 |
| --- | --- |
| PR01 可靠未发与可能已发 | 分别在原件持久化失败、持久化后可靠未 dispatch、dispatch 后丢回复时停止。第一项业务 submit 为零；第二项只有该原操作政策允许且当前权限有效时，显式继续同一原件；第三项保持可能已发，优先查询原件。无 dispatch 记录的旧缓存一律不证明未发送。 |
| PR02 提交后回复丢失 | 选有确定资源后果且支持原结果回读的 R，由 authority oracle 确认已提交后阻断回复。进程重启及会话恢复后查回同一结果，再重复查询或该领域允许的 exact replay；资源/费用/效果和适用 receipt 均不重复。不具备回读的 CLI 只获得“原结果未知、恢复受阻”的正确结论，不获得完成验收。 |
| PR03 查询本身失败 | 对同 R 依次注入 timeout、服务不可达、单节点未找到、HistoryUnavailable、限流及读取权限撤销；业务结果不能改为 rejected/未提交/已取消。按真实原因给重新认证、等待/retry_after、查询同一原件或安全停止；无法查询时保留原身份，不承诺登录或刷新必然恢复能力，不自动换 endpoint/new nonce/new operation。 |
| PR04 交叉关联与迟到结果 | 保留 A/R1 的 pending，从独立合法场景取得 A/R2、B/R3 的已有结果；不为构造负例绕过 pending/全 slot/Stop 屏障。把 R2/R3、旧 world/reorg、旧连接/epoch 的结果混入 R1。消费者只能更新匹配原件且当前仍允许读取的事实；合法查明终态不被迟到 pending/unknown 降级。无关消息可留诊断，不能充当完成 R1 的回执。 |
| PR05 权限变化 | 在读取前与返回前分别撤销/切换 session、owner、task/fence 或 grant（按该入口实际字段），并分别检查原已提交结果和未决定的新动作。当前读权仍成立时可确认原历史；读权已失效时不泄露旧私有结果。新的执行权限按当前合同重验，已固定执行决定的续接沿原专业合同，不重新 admission 或改写已确认历史。 |
| PR06 query-only 与显式继续 | 调用 [SDK 原操作 port](../../world-simulator/llm/agent-game-client-boundary.design.md#original-operation-lifecycle-port) 或该入口真实只读查询，计数业务/Context/Action/Outcome 新原件及 submit 均为零；协议必要的读取鉴权签名可存在。Stop 后仅保留当前允许的读，不重送或终止写。另以显式继续为独立对照，仅验证原操作原本允许的首次发送或 exact replay；缺 phase、无结果和超时均不能赋予重投权。 |

最终证据按入口记录正例、代表失败、实际安全下一步与未接通处，并把原件 digest/correlation、authority 结果、效果次数、授权变化及客户端可消费状态对齐。原始签名文件、凭据和私有目标保留在获准存储，公开记录仅含脱敏关联及必要摘要。协议/adapter 故障向量、真实 TCP CLI/HTTP 消费与 Runtime/full 首局分别列结论；前两者不代签真实模型自主游玩，Viewer 图形/窄屏/键盘证据仍由其专用方法取得。本节只定义方法，源码审读与文档检查不改变各运行项的 not_run 或未覆盖状态。

<a id="api-independent-entry-validation"></a>
### 场景 api-independent-entry-validation：便利停止后的正常链与原请求恢复

Trace refs: O7-REQ-32e47f2a-2da3-4ca6-aaf0-ef522be4b898；对应 [REQ-ENTRY-FREE-001](../../product/player-entry-distribution/free-entry-world-progression-and-recognition.prd.md#req-entry-free-001) / [AC-ENTRY-FREE-001](../../product/player-entry-distribution/free-entry-world-progression-and-recognition.prd.md#ac-entry-free-001)，验证[同身份独立入口](../../world-simulator/llm/external-agent-game-api.design.md#api-independent-entry-continuation)。与 [HI01–HI05](hosted-passkey-account.manual.md#hosted-independent-continuation-validation)使用同一候选和基础依赖；原 FS、PE、PR 场景继续拥有各自协议判据。

执行前固定实际基本 Viewer 或已采纳 native gateway/SDK/Runtime 组合、模式/阶段、schema/能力、同 namespace 的原主体、world/genesis、Agent/task/goal/fence、当前资源/授权，以及原受保护请求存储。完整初次入场沿 api-cold-entry；从既有角色继续时保存其合法来源，不能用测试侧预造角色、grant、资源或原件补齐被测客户端。普通协议、真实 Runtime 与真实玩家体验分开取证；以下方法尚未执行。

| 样例 | 正常或故障操作 | 必须核对的结果 |
| --- | --- | --- |
| AI01 正常成长与回访 | 取得健康前态后停用 Hosted 附加便利，在 T1/T2 后从所声明免费基本入口恢复同身份/原角色，读取目标和资源，按 production_only 路线完成实际必需的移动、建设、转运、排产及结果读取，再离开重入。 | 真实 parser、当前授权和执行消费者均成立；原请求关联的 RecipeCompleted 与 owner-bound 产物证明该阶段成果，重入读到原历史。一次 GET、移动、排产 accepted 或 fixture 成功不足以判完整正常链。只有 Passkey 的 Viewer 正例与具备完整原生证明的 SDK 正例分别记录。 |
| AI02 固定基础网关 | 核对 SDK 实际使用原身份 HTTPS discovery/participant/owner/executor 路径及配置的 service_public_key/world，随后令便利通知包含另一 endpoint/issuer 或令所需入口开关缺失。 | 正例仍经固定 gateway 到受信 literal-loopback WorldService；错误信任根和未启用能力明确拒绝，不直连任意节点、不临时开放 private listener、不从 DevLocal 或另一个世界取得替代通过。 |
| AI03 已提交后丢回复 | 对有确定资源后果且具备原结果查询的 R，在 authority 确认提交后阻断回复，结束便利进程并重启合法客户端；从受保护原件查询，再重复查询。 | 原 payload/signature/nonce/correlation、主体和世界关联不变；恢复查询零业务 submit，R 的效果/费用及回执只出现一次。Unknown 不触发新报价、新 context、新注册或新业务键；若原领域允许 exact replay，另按其原向量核对，不推广成通用重送。 |
| AI04 执行方停止与 owner 历史读 | 使用当前 owner 按原权限已合法取得的 E1 已提交原件，按原控制合同停止/替换执行方；owner 用原 LookupExecutor 读取，必要的新 E2 执行由独立合法授权开始。 | 核历史 E1 signer/binding 与当前 canonical owner、完整 task fence；Suspended 不要求重启 E1。E1 失去私有读权时不得读缓存或继续轮询；owner 不自动解密其私有 outbox，缺合法原件的另一例明确受阻。换执行方不复制预算、续 grant 或遗漏待决旧请求。 |
| AI05 原件持有与交接边界 | 分别恢复用户侧 SDK cache 和同 origin/tab 浏览器 capsule；使唯一副本留在已退出便利服务，或换 origin/设备/native 消费者、损坏原件。 | 正例按各原存储/用途/授权读取。负例明确具体缺失的原件或消费者，保留最后已验证关联；不能导出 cookie/长期 key、自动解密另一存储、补签原件或凭新快照猜原结果。无真实受权查回/交接的组合不判独立恢复通过。 |
| AI06 能力及原结果错配 | 使用 GET-only bootstrap、缺 actor/lookup 协商的旧 CLI，或将另一请求/世界/Agent 的回执、旧 fence/迟到响应混入 R。 | 真实缺项保持 blocked/未覆盖，不能删必需动作凑完整组合。终态只由匹配原件与当前合法读取的权威结果成立，ACK 类型、当前余额、空列表及另一入口的成功不代签。 |
| AI07 基础故障与重新进入 | 在原请求未明时使 identity/gateway/world 不可达，耗尽新执行预算，另测读权撤销；恢复支持条件后重新进入。 | 基础不可达沿原新鲜度停止受影响新操作；仍有独立读权者按有界原 lookup 核结果，失读权者不泄露私有内容。恢复只重读原身份/角色/请求，不重置额度、自动恢复写权或把安全停止计入 AI01。 |

独立判定侧对账身份/控制变更、业务提交、世界效果与领域完成的分别差量，并保留自然推进的真实原因；不要求只读时整个世界静止。公开证据只记录安全关联和必要摘要。缺实际消费者、受保护原件、当前读权或生产 profile 的格逐项记缺口；文档和普通协议层通过不使默认关闭入口、P1、完整 HTTP 写路由或真实 Runtime 自动就绪。

<a id="parity-declaration-consumption-validation"></a>
### 场景 parity-declaration-consumption-validation：声明来源与逐入口接收

Trace refs: O7-REQ-21f1c7b1-8be0-438e-8f92-d4d1b34be0e6。对应[声明消费设计](../../world-simulator/llm/provider-agent-experience-parity.design.md#parity-declaration-consumption)，复用原 parity 判据、可信 Skill/driver 发布链及本手册逐入口方法；本节只增加这些合同的组合验证，不建立新认证或改变支持范围。

**前置与层次。** 固定 source/build、受信发行根、实际 manifest/声明 schema、入口/adapter/platform、世界及权限范围。确定性层由受控发行 fixture 提供记录 R1：Runtime/version、model/profile、driver/schema/Skill、mode/surface/lane、场景与权限均有明确值，发布 revision=1，原结论为 experimental，证据引用完整；另备只改一个维度的 R2，以及缺项、坏 hash、撤回和超限原件。这些是验证材料，不是实际 Runtime 获准或模型通过。实现后，必须把同一材料送进真实发布记录 loader、入口组合匹配器和最终呈现/结构化输出消费者，不能只手工造 UI 状态。当前缺接线就记录该层未实施；现有 parity 聚合脚本只作指标层回归，不能替代这些接收者。

full 层使用实际获准发布物及其对应真实 paired/Runtime recovery/QA/producer 材料，逐声明组合执行，不借 fixture 的完整引用宣称真实证据存在。真实模型和外部服务仍按原授权运行；只验证声明消费时不应调用模型或提交世界行动。保留声明原件与验证来源、实际观察、各独立结论及下一步；凭据和私有 Prompt 不进入记录。

| 方法 | 输入与操作 | 独立预期与观察点 |
| --- | --- | --- |
| PD01 精确接收 | 将 R1 及完全匹配的实际组合/合法能力输入 Launcher、Viewer、pure_api 中本次实际声明支持的接收者 | 三端保留相同声明 ID/revision/hash、组合和 experimental 范围；样式可不同，本地启动成功不升级 stage/default/parity。模型调用和业务提交均为 0；当前权限仍由原 authority 决定 |
| PD02 单维漂移 | 分别只改 Runtime version、model、profile revision、adapter/schema、mode、Web/native、平台、lane 或必要能力；再用发布者明确列入的兼容范围作正例 | 未被声明覆盖的变体不得借 R1 通过；输出具体不匹配与真实下一步。原明确兼容范围内的值按原规则匹配，不由测试擅自要求所有版本字节相同；恢复匹配后重新读取确认，不自动开始 |
| PD03 来源与修订 | R1 内容 hash 错、来源非可信发布链、同 ID/revision 冲突、当前读取失败；再发布合法 R2 明确替代/撤回 R1 | 错误记录不进入有效声明；旧缓存只标历史，当前未确认/不适用。未被撤回且仍匹配原固定组合的旧版本不因存在较新版本被任意失效。修复来源后沿同发布链读取；不删除任务或原操作 |
| PD04 证据不足 | 分别删除 paired、Runtime restart/reconnect、QA 或 producer 评分；保留 benchmark passed/latency A；另测原批准为 B 的完整受限记录 | 任一缺项不得显示完整 parity 或默认通过；输出缺项和适用限制。B 只承接原受限许可，A 也不自行产生默认发布；另一 Runtime/场景证据不能补缺 |
| PD05 能力与声明分层 | R1 匹配但 Provider 故障/权限撤销/必要动作不可用；另使用无需 Runtime/Provider 的普通 HTTP 只读组合 | 仅真实依赖失败的组合/动作 blocked，并提供原检查/认证/配置入口；普通 HTTP 不因无模型而整体 blocked。parity 缺证据与当前合法读取可用可同时成立，不凭支持声明授予写权 |
| PD06 完整字节与秘密 | 向真实 loader 输入编码后恰为 65,536 bytes 和 65,537 bytes 的完整记录；再配置更小的实际下层限制；证据引用中夹带秘密或非受信来源 | 只有满足完整 schema、来源和所有更小限制的前者可被消费；后者拒绝，不能截去能力/缺项后通过。原始 trace、Prompt/密钥不作为公开声明正文；没有因为记录刷新增加模型调用或世界费用 |
| PD07 旧请求与刷新 | 固定已提交但结果未知的原操作 O，切换声明/Provider 选择并使声明读取暂时失败；恢复后读取 O 的原结果 | O 身份、签名、digest、原声明 provenance 不变；0 次新业务重发。只在当前读权下查询同 O；真实 committed 仍属 O，声明恢复不能覆盖其结果或替代 PSW 原切换合同 |
| PD08 实际表面和失败恢复 | 在每个被声明的表面分别打开选择/配置、检查、显式开始前摘要及失败详情；读取超时后再修复实际源 | 对应 source/build/adapter 的真实消费者输出来源、版本、范围、限制和可执行下一步。不存在选择能力的表面不被凭空要求新增按钮；safe query 不存在时明确停止，不用另端截图、Hello、SDK 或旧记录补签本端 |

通过判据是以上身份、分类、范围和零额外副作用逐条符合设计；技术 consumer 通过不代表真实 parity 或发行。报告分别标记 fixture 接收、真实发布源、各实际表面和真实 Provider 证据，缺一层保留该层未完成，不能反向改原声明让测试通过。

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

**双表面同任务组合。** Trace refs: O7-REQ-62007da5-b12a-4b8c-98bb-82ab53a35069；接收 [AC-COLLAB-001](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#ac-collab-001) 与 [专业消费合同](../../world-runtime/runtime/player-runtime-collaboration.design.md#collab-shared-surfaces)。本组在实际支持的 Web 与 native 安装包中分别操作；HTTP/RuntimePort fixture 可证明底层分项，不能代签另一表面。记录各自 build/平台、当前登录身份与声明范围，native 弹出系统浏览器不算 native 通过。

同案固定 world/genesis/branch、Agent A、task T、G1/E1，以及合法 owner 的两个独立窗口；另备无权身份、Agent B 和将失去执行/私有读取资格的 E1。由临时世界已确认的 TaskControl/控制提交、当前 audience 和原私有记录预先列出允许集合及期望版本，不从两端相同文案反推正确性。两端 snapshot_id、session 和时点不要求相等；在同一已知控制状态核对内容，有并发变化时按各自切点及 after-S 的可核顺序解释。

| ID / 执行与故障 | 独立判据与必须保留的结果 |
| --- | --- |
| CB01 双端初始化与当前执行方 | 两端以当前读权取得 T 的全部 snapshot 页及 after-S；任务、Agent、激活目标和 executor/fence 与独立控制来源一致，完整待决 ID 集合等于 S 上允许集合。打开或选择对象不新增 task、executor、context 或业务提交；健康、Key 保存或收到报告不能替代当前执行方来源。 |
| CB02 Web→native 与 native→Web 回访 | 依次关闭一个观察端并从另一端或新窗口恢复；清除原受限内存，不读取玩家端持久正文缓存。仍是 T/A，原线程、决定及操作结果保留；内部 Runtime session 变化不创建 Agent/task。回访不重做批准或行动、不续 grant、不发 Stop；独立执行器和世界可按原许可继续，不能要求整个 World root 因观察操作保持静止。 |
| CB03 显式交接与旧执行方 | owner 通过原受保护入口将 E1 替换为 E2，记录 C2 控制提交；重启旧 E1 并送达旧请求。两端恢复后显示来源为 C2 的当前 E2，E1 的新 goal/action 不生效；本例同时撤去 E1 私有读权，读取不泄露正文/计数/指纹。仍有历史读权的其他主体按原 audience 读取，不能把执行失权泛化为删除原历史。 |
| CB04 分页、事件与代际交错 | 复用 collab-snapshot-pages 的 201 项合法当前态，在读页期间激活 G2；原 S 页保持 G1，经 after-S 得到 G2，旧 G1 新 context 被当前权威拒绝。另分别使页号缺失、游标到期、branch/读权版本或适用 fence 改变：按原合同 reset/拒绝，读齐前不伪完整，不拼两轮页面或把旧正文改标新目标；仍获权的原结果查询和 Stop 保留原可用条件。 |
| CB05 错身份、错 Agent 与晚到私有响应 | 分别把另一身份/Agent B/旧 E1 用于 T 的读取、目标写入与行动；不能借选中 A、旧 token 或深链获得权限。另在两端请求发出后切身份/撤读权，旧私有内存和晚响应不展示、不作为新请求输入；比较原 goal/grant/业务写集确认拒绝没有新增效果，401/403 不泄露对象存在性或差异。 |
| CB06 原结果未知与恢复 | 保留 T 的原 operation P，分别在断连、关窗、执行器重启后回访。snapshot 待决项与原键查询关联 P，未知保持未知；若原 P 后续确认 committed，只展示其原效果一次，不换 nonce/context 或自动重提。driver run_unknown、私有写入 recovery_required 与世界 P 状态分别核对；读取可有原鉴权/有限协议元数据，不得成为新业务写入或批准。 |

同候选的 collab-bind 原隔离分项、collab-recovery、collab-events、collab-snapshot-pages、collab-question-delivery 和 api-idempotency 可提供适用的确定性负例；本组仍须核两表面实际消费。保存小型关联 `case_id → Web/native build与身份 → world/Agent/task → control/goal/fence → snapshot切点与完整集合 → 原operation/result`，保留失败、来源时间、当前读权和各端输出，不公开私有正文、可重放游标/签名或可枚举 digest。

缺完整 TaskSnapshot、实际 native/Web 消费者或合法测试资格时对应组合为 blocked/not_run；已实施 task-control/private-goal 子集及 scripted driver 不补齐缺项。本组不新增真实模型调用要求，不代签 AC-COLLAB-007 的两 Runtime 经历；该项继续按 collab-session 及既有 adapter 方法取证。

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

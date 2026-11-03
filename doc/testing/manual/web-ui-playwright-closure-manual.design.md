# oasis7：Web UI Playwright 闭环测试手册（2026-02-28）设计

- 历史/共享设计 companion: `doc/testing/manual/web-ui-agent-browser-closure-manual.prd.md`
- 历史任务追溯: GitHub task issue evidence comments 与 git history
- 当前 Playwright 实跑系列入口: `doc/testing/manual/web-ui-playwright-closure-manual.manual.md`

<a id="1-设计定位"></a>
## 1. 设计定位：目标与范围
定义测试手册专题设计，统一系统性测试、Web UI Playwright 闭环与工程化维护方式。

## 2. 设计结构
- 手册结构层：明确入口、章节组织与适用范围。
- 执行方法层：沉淀测试步骤、命令模板与证据要求。
- 工具链对齐层：把 Playwright、系统测试与门禁口径统一到手册。
- 维护治理层：建立版本更新、引用互链与长期维护约定。

<a id="playwright-source-capture-method"></a>
跨层场景采用可审计的 capture→action→oracle→artifact join：先固定 QA receiver 指定的 source clause、candidate/tree、world/branch/window、profile、真实依赖和 viewport；再通过真实可见 UI 控件完成玩家动作；`__AW_TEST__` 仅读取 readiness/state/history，不执行动作或代替权威 oracle；之后按 before/action/pending-or-outage/fresh-recovery/final 顺序采样并对齐同候选 API/journal/Agent authority refs。语义由 [QA scene contract](../prd.md#qa-infrastructure-scene-families)决定，本设计只定义页面采证的可重复步骤。

### 2.1 需求承接与分配表

本表保留原 QA 采证方法与四个 planned 接收场景，并另接 AC-14C 的 fixture-functional 合同。后者不改 companion 的 Viewer/launcher driver 分流、renderer/S6、F1–F4、named session/owned close、prompt/chat/echo、时限、兼容产物目录与脱敏义务；真实 PWT 与受控功能回归分别记录，不能互相代签。下表 Trace refs 引用既有唯一声明，不改变产品覆盖/实现状态。


| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-d7eec6c8-06c7-4707-8f50-81b19e910c78；[AC-14C](../prd.md#browser-functional-regression) | BROWSER-AC-1–5：真实 UI/状态处理、事件调度、Prompt 试点、隔离与证据。 | [受控浏览器合同](#browser-functional-fixture-contract) | QA runner、Viewer consumer、既有 CI planner；领域 owner 解释预期。 | 产品适用性：N/A；reason=确定性浏览器测试机制；scope=fixture-functional，不改变生产控制或 PWT/full 真实依赖要求。 |
| [professional_acceptance: 采证计划与记录](../prd.md#qa-infrastructure-receiver) | UI evidence joins selected QA scene, candidate, viewport, oracle, archive and tier; visible actions remain distinct from read-only state probes. | [doc/testing/manual/web-ui-playwright-closure-manual.design.md#playwright-source-capture-method](web-ui-playwright-closure-manual.design.md#playwright-source-capture-method) | QA owns verdict/scene; Viewer and Agent own their surfaces; Runtime/P2P/gameplay own authoritative oracles. | Design defines capture only; no browser/provider run, runtime implementation, release decision, or playability claim.  产品层 N/A；reason: 本关系仅定义QA如何绑定source、scene与可审计产物，不独立定义玩家世界结果；scope: 仅原采证方法、可见动作与只读探针区分；下列产品场景及其全部权威oracle仍分别适用；仅免新增独立产品叶分配，不免专业验收。 |
| [professional_acceptance: 采证环境与层级判定](../prd.md#qa-infrastructure-tier-boundaries) | Keep required deterministic/API evidence separate from full real-provider, Agent parity, and external headed desktop+narrow evidence. | [doc/testing/manual/web-ui-playwright-closure-manual.design.md#playwright-full-tier-boundary](web-ui-playwright-closure-manual.design.md#playwright-full-tier-boundary) | QA selects tier; Ops/provider owners supply real dependency provenance; Agent/Viewer owners verify actual consumer paths. | Mock, screenshot-only, local contract, or absent viewport cannot satisfy the full tier.  产品层 N/A；reason: 本关系只定义QA采证环境的选择和证据判定，不能独立授予发布或玩法结论；scope: 仅required/full采证边界；下列SC-9环境子义务和原full完整要求均保持；仅免新增独立产品叶分配，不免专业验收。 |
| Trace refs: O7-REQ-05c84021-ee48-41ef-b94a-8aa9ceb8048c；[product_requirement: SC-8](../../product/world-infrastructure/prd.md#sc-infra-008) | PWT-003 / V-INDUSTRIAL、V-CONSUMER：可见推荐动作的ACK与canonical receipt、同世界state delta、数量和lineage分别采集。 | [采证条款](#playwright-source-capture-method) | Gameplay/Runtime拥有工业oracle；Viewer/Agent拥有消费表面；QA按所选scene判定。 | 仅原可见动作的消费采证；不证明完整工业root、hold、背压或整条流水线。 |
| Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405；[product_requirement: REQ-DWE-002](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-002) | PWT-005 / V-EXECUTION、V-CONSUMER：在选中无效果待决/推进反馈场景时，区分queued、completed_advanced、blocker与正向world delta，不以按钮响应推断世界推进。 | [采证条款](#playwright-source-capture-method) | Runtime/P2P拥有pending/committed权威；Viewer呈现；QA对账。 | 仅待决无效果与真实progression的消费判读；不定义pause权限、pending持久化或恢复协议。 |
| Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[product_requirement: REQ-DCS-005](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005) | PWT-006 / V-RECOVERY、V-CONSUMER：仅当选中服务状态转换场景时，采集当前状态、受影响操作、最后可信边界、blocker与真实下一步。 | [采证条款](#playwright-source-capture-method) | P2P/Runtime拥有service gate；Ops分类实际故障依赖；Viewer/Agent投影；QA判定。 | 普通provider断线不证明service gate；完整DCS-005的receipt 0/1、正向新请求/模块行动续跑与合法新配方仍独立验收。 |
| Trace refs: O7-REQ-2b7cfef9-d917-46ea-a3a9-e3a4fb96d730；[product_requirement: SC-10](../../product/world-infrastructure/prd.md#sc-infra-010) | PWT-007 / V-WORLD-SCOPE、V-CONSUMER：锁定同world、branch、candidate和observation window，采集target scope与receipt/world identity；local证据不呈现为global effect。 | [采证条款](#playwright-source-capture-method) | Runtime拥有world identity；Viewer与pure API拥有消费；QA核对同候选字段。 | 只接收消费者identity和语义对账；不证明全部跨世界拒绝实现，不把no-LLM observer/debug或不同候选当parity。 |
| Trace refs: O7-REQ-4792541c-aaa4-4ac1-9a2d-374df44eba6a；[product_requirement: SC-9](../../product/world-infrastructure/prd.md#sc-infra-009) | 仅SC-9所选full采证环境子义务：同候选真实provider、本地栈、外部headed desktop+narrow截图/console与provider-backed Agent parity。 | [采证条款](#playwright-full-tier-boundary) | QA选择tier与verdict；Ops/provider确认依赖provenance；Agent/Viewer确认实际路径。 | 不把四PWT计划替代八格authority/read×四边界矩阵，不新增第九格；required deterministic/API与full完整义务独立保留。 |
| [professional_acceptance: source-to-scene record](../prd.md#qa-infrastructure-source-scene-method) | 逐记录固定case/action、candidate/world/window、lane、provider provenance、raw artifact locator/byte length/digest、失败signature与exact unmet assertion；保留planned与实际执行状态的区别。 | [证据记录合同](#playwright-evidence-record-contract) | QA拥有记录完整性；domain owner拥有语义oracle，Ops/provider拥有环境事实。 | 产品层 N/A；reason: 本项仅是QA材料身份、字节完整性和失败记录的内部证据合同，不独立定义玩家结果；scope: 仅原记录字段与artifact join；artifact digest只证明字节完整性，不是domain receipt/finality；不免上述产品场景oracle和full要求。 仅免新增产品叶分配，不免专业验收。 |

## 3. 关键接口 / 入口
- 测试手册入口
- 步骤/命令模板
- Playwright / 系统测试链接点
- 手册维护约定

<a id="playwright-evidence-record-contract"></a>
每条 UI 记录绑定 `caseId`、可见 action、候选/world/window 身份、访问模式和执行 lane、关键输入/输出、反馈阶段、provider kind/model/endpoint route/preflight provenance（如适用）、raw screenshot/state/console/request-error/API/journal refs、每份 artifact 的 byte length/digest、mock 禁用/检测结果、失败 signature 与 exact unmet assertion。秘密只由受控运行时持有，不复制进报告或 artifact。PWT-003/005/006/007 按手册矩阵保持 planned，直到相同候选实际执行并保存可读产物。

<a id="browser-functional-fixture-contract"></a>
### 可控时序的真实前端功能回归（目标，未实现）

Trace refs: O7-REQ-d7eec6c8-06c7-4707-8f50-81b19e910c78；对应 [AC-14C / BROWSER-AC-1–5](../prd.md#browser-functional-regression)。当前已有 JS-required、Vitest/jsdom、tooling fixture smoke 与真实 shell 的视觉 fixture；本节补可复用时序控制，未把现有能力描述为缺失。

**装配与替换边界。** 首个 `fixture-functional` profile 复用同候选 Vite 测试构建、真实 `viewer_app.jsx::mountViewerApp`、`legacy_core.js` 及 `viewer_prompt_control_module.js` / `viewer_prompt_feedback_module.js` / `viewer_feedback_module.js`。测试入口位于 Viewer 的 test 侧，只引导初始化与生命周期，不复制 DOM、reducer、请求关联、能力判断或错误映射。初始合成 session/授权材料和无关 bridge/feed 可按既有 fixture 方式装配，报告须逐项列出；Prompt 交互开始后只能经 HTTP/WebSocket 入站消息和真实 UI 更新状态，不能用 `injectSnapshot`、直接写 core state 或 fixture 的预期终态绕过生产接收链。

传输控制器在应用初始化前安装，只为本次场景提供 HTTP 响应与 WebSocket 事件；请求解析、身份比对、timer、状态转换和渲染仍由生产模块执行。每个 socket 有独立连接代次，控制器能保留、释放、重复或拒绝指定消息。它不是 Runtime 模拟实现：只交付版本化场景样本，不在测试 server 中重写应用业务逻辑。当前 visual fixture 的 `connect=0` 快照模式不能自动视为已验证连接/重连；该 profile 的传输接线还需实现。沿既有 release/test 构建隔离，不在 release 安装控制器或测试 API，缺测试能力立即 blocked，不向生产产物注入 bypass。

签名期另有明确测试接缝：真实 `sendPromptControl` 会等待 `signAuthPayload` 中已安装 identity 的 `sign` Promise，HTTP/WebSocket 或 fake clock 不能阻塞这个完成点。测试装配可为已校验的合成 signing identity 提供 deferred signer 适配：仍由真实 payload 构造、身份校验与原 `identity.sign` 生成签名字节，只延迟 Promise 结果交付；`signingEntered(request)` 确认 B 已进入签名期，runner 交付旧 A 后才 `releaseSigning(request)`，再核对 B 实际发送。不得返回固定伪签名、改写请求/feedback/core state 或绕过原 generation/identity 再校验。此适配只存在于测试构建，记录为额外替换边界；它证明异步完成时序处理，不证明真实认证服务、密码学安全或权限正确。

**确定性场景接口。** 场景输入至少含 case ID、schema version、source/build、合成世界/玩家/Agent/连接身份、初始授权 profile、消息样本、独立预期、viewport 与阶段截止。runner 使用 `awaitRequest(label, matcher)` → `release(label, payload)` / `emit(connection, event)` 等有界 barrier 调度；这些是待实现接口，不是当前 CLI。每次先确认真实请求和 UI 状态到达，再交付下一事件；请求 ID/nonce 等关联字段取自实际出站包，但成功状态、版本和预期文案/字段由场景独立定义。显式 fake clock 只推进该场景的已声明 timer，保留真实 DOM 事件与渲染刷新；渲染就绪用条件等待，禁止任意长 sleep、重载或自动重试隐藏失败。

玩家输入通过真实键盘/点击、可见标签和已有 `data-prompt-action` 选择器完成；`__AW_TEST__` 如存在仅作只读观测，不能调用 send/apply 来替代交互。卸载/重挂是 runner 对真实 mount/dispose 的生命周期操作，单独记录。trace 按序保存 UI 动作、出站请求、连接代次、交付消息、时钟推进、DOM 断言与截止；同一场景应可据此重放，不要求真实网络延迟可预测。

<a id="browser-functional-pilot"></a>
**试点：Prompt Overrides 的 Apply/拒绝/恢复。** 预期沿 [PromptControl system contract](../../world-simulator/viewer/viewer-control-plane-split-live-playback.design.md#promptcontrol-system-contract)、[客户端等待与结果相关性](../../world-simulator/viewer/viewer-control-plane-split-live-playback.design.md#viewer-client-request-lifecycle-design)和[产品交互](../../product/agents-world-simulation/agent-conversation-and-prompt-control.design.md#3-关键交互)。现有 `viewer_prompt_control_protocol.test.jsx` 提供低层补充，不能代签以下真实浏览器动作。所有场景目前 planned，表中断言是目标，不是当前已通过结论。

| 场景 | 控制顺序与正例 | 必须检出的反例/边界 |
| --- | --- | --- |
| BFR-P01 loading / 空态 | 延迟 hello、授权绑定与首个 profile；随后分别交付空 Agent 集合和有效绑定，检查明确等待/空态与适用控件可达性 | 未就绪时能发 Prompt 请求、沿用别的 Agent 值、无限 loading 却没有失败原因；空态不伪造世界可用 |
| BFR-P02 成功 | 真实展开高级设置、编辑并 Apply；保留响应观察 pending，交付同请求 accepted，再按合同交付 applied 与授权新 profile | accepted 提前显示 applied；未收到匹配结果便清草稿或提升版本；同一次点击多发请求。applied 的 scope 仍为当前 runtime，fixture 不证明真实应用 |
| BFR-P03 拒绝与过期 | 分别交付 correlated rejected、version-conflict/stale 和 control-lost；展示安全原因及适用下一步，保留未提交草稿 | 本试点没有先写权威值的乐观提交；拒绝仅恢复等待/控件状态，不能先显示 applied 再回滚。control-lost 不泄露最新值/差异，不自动重试或迁移草稿 |
| BFR-P04 旧结果晚到 | 沿 R1 真实发 A rollback(999)；A 终止/超时后，B Apply 用 signingEntered/releaseSigning 固定签名期并释放 A error，另案在 B sent 后释放 A error，最后交付 B 合法结果 | A 清掉 B timer、改 B 请求身份/反馈、阻止 B 真正发送；不同 Agent/会话的响应也不能写当前状态。不为制造并发绕过真实 in-flight 约束，签名期不靠竞速或 sleep |
| BFR-P05 重复与乱序 | 在 B 完成后交付旧 A/旧连接结果、重复 B 结果及较旧授权 profile，核对当前身份/版本和可见反馈 | 重复成功增加一次提交/历史，旧响应覆盖新状态，过期 profile 覆盖新值；不以 fixture 修改出站包掩盖 consumer 缺陷 |
| BFR-P06 断线与恢复 | 挂起请求后断线，交付新连接 hello/会话绑定及授权 profile；旧连接排队事件延后交付 | 未恢复身份便开放提交、自动重放结果未知的副作用、旧连接消耗新等待；真实认证/持久恢复仍需对应领域集成证据 |
| BFR-P07 卸载与重挂 | 调用真实 disposer 后投递消息，再 mount；经一个可见点击提交，核对当前状态和请求次数 | 已卸载 DOM 仍被更新、重复订阅/多次提交。组件卸载不被擅自等同注销，共享 transport 生命周期仍沿其 owner 合同 |

**隔离与拒绝真实服务。** 每 case 使用新的命名私有浏览器 context/session、临时 storage 与合成身份，不导入用户登录态、provider 配置或凭据。静态服务仅绑定 loopback 的系统分配端口；HTTP、WebSocket、worker/service-worker 与跳转都限制到本次明确的本地资源，测试不读取 active LLM 配置。未知 endpoint/请求、生产域名或真实 provider 访问在到达网络前拒绝并使场景失败，不能靠响应超时才发现。跨 case 不共用 localStorage/IndexedDB/cache；重连只发生在本 case 内。复用既有 owned-session lifecycle 清理自有资源，失败/取消后也关闭；不使用全局 close-all。当前共享的测试构建输出先串行构建一次并冻结来源，执行使用独立产物目录；只有确证不共享端口、数据、context、全局模块和可写输出后才并行 case。

**产物与结果。** 每次输出新目录，记录候选/tree、bundle 来源与摘要、实际生产模块、fixture schema/替换集合、浏览器版本/平台、viewport、场景 ID/seed、命令、选择理由与开始/结束。每个适用 case 保存 pending/error/final 的实际截图、结构化重放 trace、DOM/只读状态断言、出站与交付消息计数、console/pageerror/network、结束与清理结果；driver 的原生 browser trace 可一并归档，但结构化场景 trace 不冒充 Playwright trace。预期拒绝/断线错误须逐 case 明确 allowlist，其余 console/network 错误失败；脱敏不删除请求关联与失败原因。产物身份遵循既有 locator/bytes/digest 合同，使用合成数据避免记录真实秘密。

case 结果区分 pass、failed、blocked 与有具体不适用原因的 not_applicable；必要 case 缺浏览器/构建/fixture、超时、零执行、缺截图/trace 或报告失败均非零结束，不能记 skipped 后令 suite 通过。总通过仅覆盖实际执行且全部满足的适用 case 和声明的浏览器/viewport。CI 选择仍由现有 planner/Viewer driver 决定；本节不新增隐式 gate，不将普通视觉 fixture 的绿色结果充当该全矩阵结果。

首个实现片只交付真实装配、网络隔离、barrier、BFR-P01–P03 及失败产物；其余 P04–P07 与选测负例沿 #5387 保留，矩阵只执行一部分时报告部分覆盖。域缺陷分别回到 #5167、#4966、#5116 的原责任边界，不在测试机制里重写反馈/权限规则。真实后端/provider、合法签名/权限、canonical receipt/world delta 与发布验证仍按原 PWT/S6/full 接续，fixture-functional 无需先启动完整 launcher/LLM 栈，也不能代替这些验收。

<a id="4-约束与边界"></a>
## 4. 约束与边界：风险限制
- 手册必须服务真实测试流程而非重复 PRD。
- 命令与步骤需可直接执行或映射。
- 不在本专题扩展新的测试框架。
- 同候选 UI 语义应与 API/journal/Agent 权威 observation 逐字段对齐；本地化文本可不同。按钮 ACK、截图或 debug snapshot 单独不能证明 committed effect、finality 或 world parity。
- `test_tier_full` 的 headed UI 证据要求外部真实 browser、真实 provider-backed path、desktop+narrow 两类 viewport 的截图/console，以及适用的 provider-backed Agent parity。`provider_local_mock` 只作 plumbing 诊断；缺失 full 环境要标 `blocked`/`unverified`。
- 按 [QA tier boundary](../prd.md#qa-infrastructure-tier-boundaries) 区分 shape-only、deterministic contract 与 full environment；此 capture design 不改变 PWT-004 日期状态或把规划用例写成执行结果。
- <a id="playwright-full-tier-boundary"></a>外部 headed desktop+narrow 的 browser/runtime/provider 和 Agent-parity proof 必须来自同一候选及对应窗口；本篇不控制 provider availability，也不将规划矩阵或 stale artifacts 升格为已验证。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-d7eec6c8-06c7-4707-8f50-81b19e910c78；[AC-14C](../prd.md#browser-functional-regression) | [受控浏览器合同](#browser-functional-fixture-contract) | BROWSER-AC-1–5：真实 consumer、确定性时序、隔离、准确结果与原集成边界。 | [手册核验](../../../testing-manual.md#browser-functional-regression-validation)执行 BFR-P01–P07：同候选 fixture-functional、声明浏览器和 desktop/narrow；另用缺工具、零用例、未知网络、缺产物与泄漏订阅负例验证 runner。 | 实施 PR 的生产模块/构建身份、实际 UI 与消息 trace、截图、console/network、原始断言及非零退出。 | planned，未运行；合成 auth/transport 不证明真实安全、Runtime、provider、世界效果或其他平台；部分矩阵不称完整通过。 |
| [professional_acceptance: 采证计划与记录](../prd.md#qa-infrastructure-receiver) | [doc/testing/manual/web-ui-playwright-closure-manual.design.md#playwright-source-capture-method](web-ui-playwright-closure-manual.design.md#playwright-source-capture-method) | PWT-003/005/006/007 capture must bind visible action to the QA scene and same-candidate authoritative API/journal/Agent observation; only applicable when a player-facing browser flow is selected. | Manual contract only, not execution: [PWT planned matrix](web-ui-playwright-closure-manual.manual.md#pwt-planned-matrix) defines PWT-003, PWT-005, PWT-006, PWT-007. On actual future run, freeze candidate/tree/world/window before UI action; browser selection follows S6. | Future same-candidate action/state/API-or-journal refs, each raw screenshot/state/console/request-error artifact with locator, byte length, and digest; current authoring evidence is the planned method only. | These four cases remain planned and unexecuted here; no actual provider, browser, runtime, or gameplay result is claimed. |
| [professional_acceptance: 采证环境与层级判定](../prd.md#qa-infrastructure-tier-boundaries) | [doc/testing/manual/web-ui-playwright-closure-manual.design.md#playwright-full-tier-boundary](web-ui-playwright-closure-manual.design.md#playwright-full-tier-boundary) | The same-candidate real provider, external headed desktop+narrow browser, and provider-backed Agent parity requirements apply only to `test_tier_full`; required deterministic/API evidence does not satisfy them. | [testing-manual.md S6](../../../testing-manual.md#testing-s6-qa-receiver) is the execution selector; PWT manual matrix is the scenario source. No browser/provider command was run in this authoring slice. | If run later, preserve provider kind/model/endpoint route/preflight provenance, actual browser/platform/viewport, same-candidate consumer refs and both viewport capture sets. | Provider availability, active-LLM API parity, external headed environment and Agent parity remain unverified; this design does not establish required/full, release, or playability pass. |
| Trace refs: O7-REQ-05c84021-ee48-41ef-b94a-8aa9ceb8048c；[product_requirement: SC-8](../../product/world-infrastructure/prd.md#sc-infra-008) | [采证条款](#playwright-source-capture-method) | PWT-003 / V-INDUSTRIAL、V-CONSUMER：可见推荐动作的ACK与canonical receipt、同世界state delta、数量和lineage分别采集。 | [PWT planned matrix](web-ui-playwright-closure-manual.manual.md#pwt-planned-matrix)；PWT-003 planned：固定同candidate/tree/world/window和目标，实际可见提交一次，分别保存ACK、receipt、state delta与数量/lineage。 | 未来同候选source/action/state/API或journal refs及raw screenshot/console/request-error，逐artifact保存locator、byte length、digest和exact unmet assertion；当前仅方法定义。 | 仅原可见动作的消费采证；不证明完整工业root、hold、背压或整条流水线。 本次未执行，不构成browser/provider/runtime/recovery或playability通过。 |
| Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405；[product_requirement: REQ-DWE-002](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-002) | [采证条款](#playwright-source-capture-method) | PWT-005 / V-EXECUTION、V-CONSUMER：在选中无效果待决/推进反馈场景时，区分queued、completed_advanced、blocker与正向world delta，不以按钮响应推断世界推进。 | [PWT planned matrix](web-ui-playwright-closure-manual.manual.md#pwt-planned-matrix)；PWT-005 planned：固定formal viewer profile及runtime/provider lane，经可见控件操作，对照前后state、feedback stage及同候选事件/receipt；没有权威world delta不能认推进。 | 未来同候选source/action/state/API或journal refs及raw screenshot/console/request-error，逐artifact保存locator、byte length、digest和exact unmet assertion；当前仅方法定义。 | 仅待决无效果与真实progression的消费判读；不定义pause权限、pending持久化或恢复协议。 本次未执行，不构成browser/provider/runtime/recovery或playability通过。 |
| Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[product_requirement: REQ-DCS-005](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005) | [采证条款](#playwright-source-capture-method) | PWT-006 / V-RECOVERY、V-CONSUMER：仅当选中服务状态转换场景时，采集当前状态、受影响操作、最后可信边界、blocker与真实下一步。 | [PWT planned matrix](web-ui-playwright-closure-manual.manual.md#pwt-planned-matrix)；PWT-006 planned：固定实际依赖与恢复窗口，经授权注入或观察故障，按before/outage/fresh-recovery/final对账状态、旧历史及receipt；不重放非幂等动作。无真实依赖标blocked/unverified。 | 未来同候选source/action/state/API或journal refs及raw screenshot/console/request-error，逐artifact保存locator、byte length、digest和exact unmet assertion；当前仅方法定义。 | 普通provider断线不证明service gate；完整DCS-005的receipt 0/1、正向新请求/模块行动续跑与合法新配方仍独立验收。 本次未执行，不构成browser/provider/runtime/recovery或playability通过。 |
| Trace refs: O7-REQ-2b7cfef9-d917-46ea-a3a9-e3a4fb96d730；[product_requirement: SC-10](../../product/world-infrastructure/prd.md#sc-infra-010) | [采证条款](#playwright-source-capture-method) | PWT-007 / V-WORLD-SCOPE、V-CONSUMER：锁定同world、branch、candidate和observation window，采集target scope与receipt/world identity；local证据不呈现为global effect。 | [PWT planned matrix](web-ui-playwright-closure-manual.manual.md#pwt-planned-matrix)；PWT-007 planned：先执行一项可见动作，再以active-provider pure API读取同一canonical状态，核对status/blocker、receipt/root和next action；保留两侧原始字段及provider身份。 | 未来同候选source/action/state/API或journal refs及raw screenshot/console/request-error，逐artifact保存locator、byte length、digest和exact unmet assertion；当前仅方法定义。 | 只接收消费者identity和语义对账；不证明全部跨世界拒绝实现，不把no-LLM observer/debug或不同候选当parity。 本次未执行，不构成browser/provider/runtime/recovery或playability通过。 |
| Trace refs: O7-REQ-4792541c-aaa4-4ac1-9a2d-374df44eba6a；[product_requirement: SC-9](../../product/world-infrastructure/prd.md#sc-infra-009) | [采证条款](#playwright-full-tier-boundary) | 仅SC-9所选full采证环境子义务：同候选真实provider、本地栈、外部headed desktop+narrow截图/console与provider-backed Agent parity。 | [S6执行选择](../../../testing-manual.md#testing-s6-qa-receiver)；SC-9 full planned：依QA tier与主手册S6选择环境，记录provider kind/model/endpoint route/preflight provenance、实际browser/platform与两viewport capture；任何必需环境缺失保持blocked/unverified。 | 未来同候选source/action/state/API或journal refs及raw screenshot/console/request-error，逐artifact保存locator、byte length、digest和exact unmet assertion；当前仅方法定义。 | 不把四PWT计划替代八格authority/read×四边界矩阵，不新增第九格；required deterministic/API与full完整义务独立保留。 本次未执行，不构成browser/provider/runtime/recovery或playability通过。 |
| [professional_acceptance: source-to-scene record](../prd.md#qa-infrastructure-source-scene-method) | [证据记录合同](#playwright-evidence-record-contract) | 逐记录固定case/action、candidate/world/window、lane、provider provenance、raw artifact locator/byte length/digest、失败signature与exact unmet assertion；保留planned与实际执行状态的区别。 | [PWT planned matrix](web-ui-playwright-closure-manual.manual.md#pwt-planned-matrix)的每次记录逐项核对字段、原始产物可读性和byte length/digest，并将actual action与同候选权威oracle对齐；planned，无实际执行。 | 原始artifact及逐字段record、缺项/冲突原因、准确未满足断言；没有可读产物不能升级verdict。 | artifact digest只证明字节完整性，不是domain receipt/finality；不免上述产品场景oracle和full要求。 未运行，不声称schema/fixture或浏览器通过。 |

## 5. 设计演进计划
- 先固化手册目录与范围。
- 再补步骤模板和工具链对齐。
- 最后沉淀维护约定。

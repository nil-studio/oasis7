# 网页自带模型连接：Agent 客户端侧系统设计

- 生命周期：`active`
- 文档状态：目标合同，未声明实现、部署、真实供应商兼容或安全验收完成。
- Owner role：`agent_engineer`
- 协作：`viewer_engineer`、`runtime_engineer`、`qa_engineer`、`producer_system_designer`。
- 固定审读基线：`nil-studio/oasis7@35961e32b7e7ad3469b7cd7db2e718fb1d6795b1`；2026-10-16。
- 产品主责：[网页自带模型连接 PRD](../../product/agents-world-simulation/self-supplied-model-connection.prd.md)；[配对产品设计](../../product/agents-world-simulation/self-supplied-model-connection.design.md)。
- 系统入口：[LLM/provider](README.md)；既有分层：[Agent Game Client](agent-game-client-boundary.design.md#des-agc-001)。

## 1. 问题、目标与非目标

目标改为网页直接调用供应商并承载第一方 Agent 循环；玩家保持页面打开，Key 持久保存在本站 localStorage，仅在请求时发送到所选供应商，Oasis7 后端不接收 Key。本设计于 2026-10-25 替代网页配置加服务端临时凭证的目标架构；旧实现与历史验收不能证明新路径完成。

首期不提供后台托管、关页续跑、Service Worker 推理、跨设备密钥同步、任意代理或自动付费来源回退。DeepSeek 与 OpenRouter 分别准入；DeepSeek 脚本调用成功不证明真实浏览器或 OpenRouter 已支持。

## 2. 上游约束与相关角色

模型连接属于 `world-simulator/llm` 的第一方客户端侧；主 Web 是交互 surface，Launcher 组织启动/入口，工程指挥面板不持有玩家 Key。产品边界由配对 PRD 拥有；Runtime 继续拥有 task、grant、executor fence、operation 与世界回执。部署方负责页面脚本、TLS、CSP、日志和游戏入口；不再提供接收玩家 Key 的托管模型执行端。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-a95a56b1-22b6-4955-b152-0c672310ddb8；[REQ-BYOK-001](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-001) | 受支持 Web 完成预设选择、贴 Key、检查及开始；无服务时不先收 Key | [DES-BYOK-001](#des-byok-001) | viewer / agent；真实应用入口与浏览器执行适配 | 无游戏接口的静态官网、玩家安装 CLI、任意网关 |
| Trace refs: O7-REQ-26cefe7a-deb5-4b21-845a-a2d4556f7dca；[REQ-BYOK-002](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-002) | 接收方和费用知情、localStorage 保存与账号使用隔离、断开忘记、检查/重试有界 | [DES-BYOK-002](#des-byok-002) | agent / 部署安全 owner；账号认证、供应商账户 | 跨设备同步、全账户账单硬封顶、替用户上游充值 |
| Trace refs: O7-REQ-84294c93-3275-4d19-8faf-389950b87464；[REQ-BYOK-003](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-003) | 模型就绪不授予游戏权限；会话内第一方执行复用统一游戏合同 | [DES-BYOK-003](#des-byok-003) | runtime / agent；有效任务、grant、fence 与客户端接缝 | 第一方特权、全量迁移完成、离线永久托管 |
| Trace refs: O7-REQ-d3d63078-ef30-45fb-8ae6-2f6db1f46c34；[REQ-BYOK-004](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-004) | 停用/切换隔离迟到结果、保留原操作、未知不重放 | [DES-BYOK-004](#des-byok-004) | runtime / agent；Provider Switch 与原操作恢复 | 取消已提交世界效果、保证退还上游调用费 |
| Trace refs: O7-REQ-e0ae0d80-31ca-488b-8ba2-a728bfb9ce8d；[REQ-BYOK-005](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-005) | 两家具体模型/协议分别验收、保留工具语义和可诊断失败 | [DES-BYOK-005](#des-byok-005) | agent / QA；供应商契约和实际测试配置 | 所有模型天然兼容、局部检查代签可玩或发行 |

### 2.2 既有权威的复用

[第一方客户端边界](agent-game-client-boundary.design.md#des-agc-001)拥有依赖方向；[Game API](external-agent-game-api.design.md#api-composition)拥有游戏接口；[Provider Switch](provider-switch-intent-continuity.design.md#des-psw-003)拥有在途意图连续性。浏览器本地模型管理不是 Game API，也不开放 operator/control API；只在玩家自己的受限第一方执行上下文内管理模型连接。

## 3. 当前状态、目标状态与差距

<a id="current-state"></a>

当前 Launcher 的 `/v1/agent-client/*`、临时内存凭证、服务端租约及 Rust provider 是旧托管路径。新浏览器目标不依赖它们进行模型连接、检查和推理；世界认证、任务、签名、权限 fence、Context、回执仍复用现有专业合同。现有网页签名/动作入口不等于已有浏览器 Harness；不得以 legacy 直接动作绕过统一 Game Client 的任务与恢复合同。

迁移尚未实施。上游预检和脚本调用只构成前置证据；发行仍须验证真实页面 CSP、实际响应 CORS、浏览器工具往返和合法游戏结果。临时部署状态与过程证据归实施 PR，不固化为产品保证。

## 4. 边界与结构

<a id="des-byok-001"></a>

### DES-BYOK-001：浏览器连接与唯一游戏客户端

Trace refs: O7-REQ-a95a56b1-22b6-4955-b152-0c672310ddb8；[上游要求](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-001)。

```text
本站 localStorage Key → 浏览器模型适配器 → 固定供应商 HTTPS API
                            ↓ 受限模型建议
                 浏览器 Agent loop / Game Client
                            ↓ 合法身份、任务、Context、签名
                  World / Runtime 准入、效果与回执
```

发行配置固定供应商 origin/path、模型、协议、工具能力、路由和运行限制。入口先检查安全上下文、localStorage 可用性、浏览器执行/签名能力、合法游戏接口及 CSP/CORS 支持；不支持就明确禁用，不上传 Key 探测旧托管端。静态配置只能证明预设存在，实际连接检查须玩家明确同意且有界。模型 Key 不是游戏身份，不进入 World DTO、游戏签名、任务、Context 或回执。

## 5. 数据与状态

逻辑对象由浏览器持有，不要求新增服务端模型管理 API：

| 对象 | 内容/范围 | 恢复边界 |
| --- | --- | --- |
| ProviderPreset | 固定 origin/path、protocol、model、route、revision 与限制 | 变更使原检查失效；不采信模型返回 URL |
| LocalCredential | schema version、游戏账号命名空间、provider、Key、credential revision | localStorage；不跨账号自动使用，不跨设备同步 |
| ProbeResult | 配置/revision、能力、检查时间与固定状态 | 仅代表当时检查；不是运行授权，不持久化响应正文 |
| ActiveBinding | world/owner/Agent/Task/goal reference/executor fence、配置 revision、run generation | 当前页内存；新页面不继承活动执行资格 |
| RecoveryJournal | 原 run/Context/operation/nonce、提交阶段、预算预留/消耗及必要恢复关联 | 提交前持久化；不含 Key、私有推理或完整模型历史；可授权读取才核对 |

本地恢复记录按 world/owner/Agent/Task/fence 隔离；它是恢复索引而非世界事实。损坏、缺失或不可写不能当新 run 自动开始。持久化最小字段、版本迁移和容量必须在实现中验证；世界权威不能从 localStorage 获得或被其覆盖。

## 6. 接口与协议

浏览器以 `fetch` 请求固定供应商端点，使用 `credentials: omit`、`redirect: error`、`referrerPolicy: no-referrer` 与有界超时/AbortController，Key 仅置于 Authorization，不进入 URL。供应商必须允许真实应用 origin、POST、authorization/content-type 的预检与实际响应；失败按网络/CORS 阻塞，不能用 no-cors、扩展、任意代理或公共 Key 绕过。页面 CSP 的 connect-src 精确允许所启用供应商 origin，保留原世界连接 origin，不扩大成通配符。

旧 `/v1/agent-client/capabilities`、model-connections/probe/activate/lease/stop/forget 不是新路径必需 API。浏览器本地管理提供对应用户动作，合法世界接口仍要求原认证、签名和服务端授权；缺失浏览器可用的统一接口时该玩法明确阻塞，不自造 wire 合同或绕权。

<a id="model-management-reconciliation"></a>

### 6.1 本地存储、管理与多标签页

Trace refs: O7-REQ-a95a56b1-22b6-4955-b152-0c672310ddb8；O7-REQ-26cefe7a-deb5-4b21-845a-a2d4556f7dca；O7-REQ-d3d63078-ef30-45fb-8ae6-2f6db1f46c34。承接 [REQ-BYOK-001](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-001)、[REQ-BYOK-002](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-002) 与 [REQ-BYOK-004](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-004)。

- localStorage Key 按受认证游戏账号及供应商命名空间保存；账号变化先停止旧运行，不读取旧账号 Key 作为新账号默认凭证。退出账号清除该账号 Key；Stop 保留，Forget 删除，清站点数据后要求重新输入。共享 origin 的脚本仍能读全部命名空间，这不是安全隔离或加密。
- 写入/读取/删除捕获 SecurityError、配额、损坏及 schema 不支持；保存失败不显示成功，不悄悄改存 sessionStorage 或后端。删除失败也先停止运行，明确残留并提供清站点数据指引。删除 Key 不删除未决操作的非敏感恢复索引。
- 运行前用 Web Locks 对同一 world/owner/Agent 获得唯一活动页锁，锁覆盖模型调用与游戏提交；不支持可靠锁时禁用运行，不用 localStorage 读写模拟 CAS。所有执行页使用同一命名规则；服务端仍以 task/executor fence 裁决，不把浏览器锁当权限或恶意客户端防线。
- 配置保存、probe 准入、预算预留/结算、Stop/Forget 与账号退出使用独立的账号级短时管理 Web Lock；在锁内重读并比较 expected revision，再将该账号版本化控制记录（配置引用、凭证 revision、停止代及预算）一次 setItem 发布。冲突拒绝旧草稿，不以最后写入覆盖；写失败不发送请求。probe 与运行在同一管理锁内预留共享预算，结果按原预留 ID 结算，未知不返还。Forget/退出推进删除 tombstone revision，防止旧草稿重新保存已忘记的 Key。密钥数据与控制记录跨写入失败时先保持停止并报告待清除，不能恢复运行。
- 管理锁只覆盖本地状态临界区，不等待网络、模型响应或长期活动锁；活动页可短暂获取管理锁，管理动作不反向等待活动锁，保证 Stop/Forget 始终可操作。发布停止后所有页面的发送门都检查当前记录，已发送的网络请求仍按在途结果处理；不承诺撤回已经通过发送门的提交。存储事件仅提示重读，不作为互斥机制。管理锁不可用时不允许付费检查或配置提交，不静默降级成 localStorage CAS。
- storage/BroadcastChannel 仅发送 revision、失效和停止通知，禁止发送 raw Key。发送前与模型返回后重新读取凭证 revision、停止标记、当前账号及游戏 fence。隐藏/失去运行许可的页撤销自身 generation，其他页不得自动接管。Stop/Forget 先持久发布失效 revision；休眠页醒来必须检查，不能依赖消息送达。当前页同步处理自身变更，因为 storage 事件不回发源页。
- 本地只读状态与世界原件查询不得触发 probe、模型调用、预算重置或运行恢复。付费检查超时/取消不代表未消费；记录 unknown 并保守保留预算，明确新检查须玩家重新确认，不声称供应商支持按本地检查 ID 幂等重试。

<a id="des-byok-005"></a>

### DES-BYOK-005：按能力适配，而非只替换 base_url

Trace refs: O7-REQ-e0ae0d80-31ca-488b-8ba2-a728bfb9ce8d；[上游要求](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-005)。

预设区分 `provider_id` 与 `protocol`。保留既有 Responses 路径；Chat Completions 仅在其适配器完成工具往返、流式与用量契约后可启用，不能把 `letai_direct` 文本结果直接当作完整 Harness 工具闭环。DeepSeek 官方已提供 Chat Completions/工具调用说明；其文档也列有 Responses 入口，不能将“只支持 Chat”硬编码成永久产品前提。具体协议随候选配置验证，不做盲目协议探测或失败后隐式改协议。

传输允许域名仅为选定预设的 `api.deepseek.com` 或 `openrouter.ai`，实际 HTTPS origin/path 按已核实的官方协议固定。使用 `redirect: error`，禁止接受模型返回的调用 URL、用户自带代理地址或动态额外认证头；依赖浏览器 TLS/网络安全机制，不声称网页能够检查 DNS 解析后的 IP 或配置主机出站防火墙。

模型适配器需保留工具名、schema、call ID、参数增量重组、工具结果顺序、终态及 usage。缺终态、格式损坏、工具未支持或必需输入无法容纳时返回明确阻塞，不把文本 JSON、截断结果或普通回答当动作。推理相关附加字段按具体协议保存必要历史，不能丢掉必需内容，也不能当作面向玩家或世界的事实。

#### 历史归属与输入容量

Harness 与 Provider 必须显式约定历史模式，不能由适配器是否保存消息推断。`ModelProviderHistory::CallerSnapshot` 是默认模式：调用者每轮提供完整授权快照，Provider 不再累计另一份对话。`ProviderConversation` 由 Provider 拥有结构化会话历史；SessionModelProvider 使用该模式。这里的“无状态”是供应商请求协议属性，不改变本地 Provider 对必要历史的归属。

在 `ProviderConversation` 模式，同一 Context 首次决策发送完整当前任务、目标、授权观察和能力；后续决策仅提供继续当前 Context 的小型指示及本轮新增工具结果。工具结果以原 supplier call ID 通道交付一次，不再把累计 transcript 复制进 user prompt。Provider 保留先前 assistant 工具调用、必要推理字段及有序结果；多工具批次中未执行调用也须按原 ID 完成因果链。取得合法的新 Context 后，首轮必须发送完整新上下文，不能用旧 Context 的历史替代当前切点、目标或 fence；换 Context 也不自动清空仍属该模型会话的必要协议历史。

模型输入容量以实际候选请求为准：Provider 的 `preflight(&self, request)` 必须在不修改会话、不发送、不预留物理调用预算的情况下，构造包含现有必要历史与本轮新增输入的完整候选，检查真实编码字节数和保守 token 上界。调用者快照模式默认校验请求本身；结构化会话模式不能只校验新增片段。浏览器 Harness 在持久调用预留前执行等价预检（Rust 对应接缝为 durable host 的 `reserve_model_call`）；浏览器适配器在执行入口再次校验，发送前仍独立复核当前 Stop、页面运行许可与 fence，预检不构成执行许可。

实际候选无法容纳时返回 typed `input_limit`，不静默截断、摘要替换必要历史、丢弃工具结果、抬高限制或降级协议。本地预检拒绝不计物理请求、不制造 unknown usage、不消耗持久物理调用预留；已完成预留后的竞态或发送结果不明仍保守计入原预算，不以该合同自动退款。容量证据只记录非敏感的历史模式、编码大小、配置限额和拒绝阶段，不记录完整 prompt 或凭证。

OpenRouter Responses 按官方无状态合同携带必要历史，使用 `store: false` 且不设置非空 `previous_response_id`。Chat Completions 的 OpenRouter 路由默认锁在明确的允许集合，禁用集合外回退，并要求必要工具参数被支持；相应请求使用 `provider.only`、`allow_fallbacks: false`、`require_parameters: true`。其他协议只有在适配器能证明同等约束时才启用；字段不支持时拒绝该组合，不静默删除约束。默认不跨模型回退，实际模型/路由变化必须可诊断并重新检查适用性。

项目的 session、幂等和 provider invocation 标识保留在本地控制/诊断层，不无条件塞进供应商请求体；上游接受同名字段也不能证明免费重试或计费幂等。费用/用量缺失保留 unknown。

## 7. 关键流程

<a id="des-byok-002"></a>

### DES-BYOK-002：本地凭证、费用与数据流

Trace refs: O7-REQ-26cefe7a-deb5-4b21-845a-a2d4556f7dca；[上游要求](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-002)。

保存前明确说明 Key 持久保存在本站 localStorage，同源脚本/XSS、浏览器扩展或主机访问者可能读取；刷新/关页不清除，不承诺加密保险箱。Key 仅在所选供应商请求中使用，不上传 Oasis7 后端、不写世界、日志、异常原文、APM、分析、分享、导出、Service Worker 缓存或应用备份。清空输入框不等于已删除 localStorage。避免额外副本，移除内存引用不承诺 JS 内存绝对清零。运行日志和错误仅输出固定分类及最小非敏感用量。

检查、重试与运行共用本次有界预算；发送前先持久化次数和最大 token 预留，usage 未知保留预留，不按零返还。Key 替换、刷新、Stop 或切换页不重置同 run 预算；损坏/丢失无法核实时停止，不能宣称跨清站点数据仍能强制账户费用上限。本地次数/token/并发/时间限制仅约束守约客户端，不是供应商账户或世界授权的安全边界。金额仅在定价、路由和全部收费维度已验证时计算，否则显示未知/估算。

Prompt 只含当前合法任务与世界投影，不包含 Key、其他账号材料或管理权限。新供应商/路由的数据接收方变化须确认；模型不能选择任意网络地址、读取存储或扩大工具权限。

<a id="des-byok-003"></a>

### DES-BYOK-003：页面运行与世界许可

Trace refs: O7-REQ-84294c93-3275-4d19-8faf-389950b87464；[上游要求](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-003)。

开始前读取并展示同一 world/owner/Agent/Task、canonical goal reference、实际执行身份与限制，由玩家确认。模型调用前、模型返回后及提交前复核完整 Runtime fence、Context 有效性、账号、配置 revision、活动页锁和 generation；owner、目标或 executor 改变即失效并要求重新确认，不用客户端声明替代服务端 fresh fence。工具严格白名单/schema，走统一 Game Client；不向模型暴露签名器或任意 fetch/存储工具。缺少所需游戏接口就阻塞，不复制特权世界写入。

状态为未配置、已保存待检查、检查中、就绪未运行、运行、暂停待核对、停止及阻塞。加载已保存 Key 不 probe、不自动开始；启动必须重新校验配置与权限。仅可见、联网、锁与授权有效时发送新推理/新效果；hidden、pagehide、offline、冻结/长间隔后暂停并撤销活动 generation，AbortController 尽力取消，返回必须先核对并显式恢复。不能依赖 unload、定时器按时运行或 Abort 成功来保证供应商退款；恢复发送边界必须重新检查当前状态。关页后不存在服务端代跑模型循环，已受理世界作业仍由 Runtime 推进。

## 8. 失败、恢复与一致性

<a id="des-byok-004"></a>

### DES-BYOK-004：停止、切换与原操作恢复

Trace refs: O7-REQ-d3d63078-ef30-45fb-8ae6-2f6db1f46c34；[上游要求](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-004)。

Stop 先撤销运行 generation/持久停止标记再取消待发工作，保留 Key；Forget 同样停止再删除 Key；退出账号停止并删除对应 Key。旧代模型响应永久失去新提交资格，取消不改写世界结果。跨页停止在发布失效标记后才能显示本地已停止，无法确认其他执行方时说明限制；不伪报已经取消原世界作业或撤销供应商 Key。

提交前写入原 run/Context/operation/nonce 与预算预留；写不成就不发送。刷新、重开、换 Key/供应商或锁接管后先以现有合法权限只读核对全部未决原件，不调用模型、不重发效果、不换 nonce、不新建替代 Context、不重置预算。分别呈现已生效但响应丢失、Pending、Rejected、HistoryUnavailable、部分结算的已确认范围；读不到保持未知，无权限 fail closed。连接成功、模型输出、successful_effects、同 owner 的产物或最新快照均不构成本 run 原件证明，引用已有 Action Receipt/成果面，不另建事实源。

新页默认不恢复旧模型会话；只有原件已核对、完整 fence 仍有效、原预算可恢复且当前 Context 所需协议历史可从合法证据重建时，才由玩家明确继续。必要历史缺失/超限则阻塞，不以重启或丢弃工具历史续跑；新 Context 必须遵循现有 Harness/Runtime 过期收口合同，不能为解阻塞自动创建。#4836 的目标映射/未知结果与 #4837 的历史恢复仍需各自实现及验收。未提供原件查询时显示本页能力限制与受支持执行端路径；没有合法路径就明确暂不可用并停止新效果，不画假查询按钮。

## 9. 安全、隐私与可观测性

localStorage 命名空间不能防御同源恶意脚本。部署应采用严格 CSP，限制供应商 connect-src，避免第三方脚本和 session replay，不加载模型返回脚本/HTML；模型结果按不可信数据解析。禁止把 Key 放入 DOM 属性、URL、分享、HAR/截图或诊断导出。既有应用登录、世界 CSRF/Origin、签名和权限校验继续有效；不把模型 Key 转成登录凭证。

供应商请求失败只输出本地固定错误分类（认证、额度、预算、限流、模型/协议不兼容、容量、网络/CORS、存储、活动页冲突）；供应商原始错误体可能含输入，不直接展示。CORS 与网络失败未必可由 JS 区分，文案不得声称已确认具体原因。完整响应大小、流式累积、工具参数和终态须有界；截断或畸形工具不执行，不通过自动重试/抬高预算解阻塞。记录最小 provider/model/revision、计数、耗时与 unknown 用量，不记录完整 prompt/response、私有推理或 Key 指纹。

## 10. 兼容、迁移与部署

目标浏览器路径与旧托管路径不可悄悄互相回退。实施时用明确能力选择新路径，同步旧入口/调用方及运行说明，停止旧服务器模型循环并处理旧未决原件后再由玩家确认新页运行；不能双执行或把旧 Key 从服务器导出到浏览器。旧模式在退出支持前的凭证销毁和已有操作核对责任继续有效，不恢复旧托管模式作为 CORS 失败兜底。

供应商 CORS、页面 CSP、工具协议和浏览器 Game Client 各自是发行前置。初期按实际已验组合启用；不支持 OpenRouter 或浏览器能力时保持明确禁用。页面 CSP 必须允许固定供应商，不能放宽全部脚本/网络来源。回滚禁用新开始并停止所有活动代，删除本地 Key 的失败须可见；保留已授权可读的非敏感原件索引，不回滚世界事实。旧托管验收手册和 runbook 仅证明其原路径，浏览器迁移时应更新适用消费者，不能照旧标记通过。

## 11. 验证与验收

<a id="verification-mapping"></a>

### 11.1 验证映射表

| 上游 requirement（path#fragment） | 本设计条款（path#anchor） | 验证项 / 场景 | 验证方法与测试/手册源 | 证据与判定边界 | 状态 / 未覆盖范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-a95a56b1-22b6-4955-b152-0c672310ddb8；[REQ-BYOK-001](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-001) | [DES-BYOK-001](#des-byok-001) | BYOK-01 浏览器主流程与缺 CORS/CSP/执行适配负例 | [BYOK 验收手册](../../testing/manual/self-supplied-model-connection.manual.md#byok-01)：按该手册对应场景执行确定性/HTTP/网页及获授权真实试玩 | 两家逐一从表单至明确开始；不支持入口不收 Key | 计划验证；功能、真实供应商与网页均未验收 |
| Trace refs: O7-REQ-26cefe7a-deb5-4b21-845a-a2d4556f7dca；[REQ-BYOK-002](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-002) | [DES-BYOK-002](#des-byok-002) | BYOK-02 localStorage 凭证/账号隔离；BYOK-03 并发预算与未知用量 | [BYOK 验收手册](../../testing/manual/self-supplied-model-connection.manual.md#byok-02)：按该手册对应场景执行确定性/HTTP/网页及获授权真实试玩 | canary secret 不泄漏；未知 usage 不按零返还；不承诺账户硬封顶 | 计划验证；功能、真实供应商与网页均未验收 |
| Trace refs: O7-REQ-84294c93-3275-4d19-8faf-389950b87464；[REQ-BYOK-003](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-003) | [DES-BYOK-003](#des-byok-003) | BYOK-04 合法工具/游戏闭环；BYOK-05 隐藏、冻结、刷新与重开 | [BYOK 验收手册](../../testing/manual/self-supplied-model-connection.manual.md#byok-04)：按该手册对应场景执行确定性/HTTP/网页及获授权真实试玩 | 同一 Runtime 准入；暂停/失效后无新调用，世界作业按原规则推进 | 计划验证；功能、真实供应商与网页均未验收 |
| Trace refs: O7-REQ-d3d63078-ef30-45fb-8ae6-2f6db1f46c34；[REQ-BYOK-004](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-004) | [DES-BYOK-004](#des-byok-004) | BYOK-06 换 Key、Stop、双标签页活动锁/失效 revision、迟到结果与原件恢复 | [BYOK 验收手册](../../testing/manual/self-supplied-model-connection.manual.md#byok-06)：按该手册对应场景执行确定性/HTTP/网页及获授权真实试玩 | 旧运行代永久失效；不替换未知原操作，不保证上游退费 | 计划验证；功能、真实供应商与网页均未验收 |
| Trace refs: O7-REQ-e0ae0d80-31ca-488b-8ba2-a728bfb9ce8d；[REQ-BYOK-005](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-005) | [DES-BYOK-005](#des-byok-005) | BYOK-07 双协议工具往返/路由、多轮历史归属与容量负例；BYOK-08 两家授权真实模型闭环 | [BYOK 验收手册](../../testing/manual/self-supplied-model-connection.manual.md#byok-07)：按该手册对应场景执行确定性/HTTP/网页及获授权真实试玩 | 协议可配不等于已支持；分供应商/模型/协议/路由/浏览器取证 | 计划验证；功能、真实供应商与网页均未验收 |

浏览器专项映射（沿用上表 Trace refs 与 DES 条款）：

- DES-BYOK-001/005：真实外部浏览器按 provider/model/protocol 验证 Origin 预检、实际响应、CSP、工具往返、响应大小与错误；服务端脚本和单供应商不能代签。
- DES-BYOK-002：canary Key 保存/刷新/重开、换号/退出、存储拒绝/损坏/配额/删除失败、网络请求与遥测脱敏；仅供应商收到 Key，Oasis7 请求不得包含 Key。模型检查与 unknown 用量预算在刷新前后连续。
- DES-BYOK-002/003/004：双页 probe/运行并发预留、配置草稿与 Forget 冲突、管理写入中断、管理锁不被活动页长期阻塞、双页竞争、Stop/Forget 跨页、旧页冻结后苏醒、关页事件丢失、断网/隐藏/恢复、目标/owner/executor fence 变化、迟到结果与提交前日志写入失败；证明没有预算超额准入、旧凭证复活、自动接管、恢复消费或重复效果。
- DES-BYOK-004：离线无付费 fixture 覆盖已生效但丢响应、Pending、Rejected、HistoryUnavailable、部分结算；原件核对只读，未知不改终态，无权限拒绝；有真实合法入口后再验 Agent/目标/Task 对应和回执归属。

这些均为计划验证，当前浏览器实现、浏览器试玩与安全验收未完成。旧手册在浏览器实现 PR 中按这些映射修订前不能直接签署新路径验收。

本表都是待执行场景，不是新增 CI 状态或流程材料。实现 PR 记录实际测试命令、版本和结果；费用调用另获真实凭证/预算授权。fixture 不使用真实 Key；截图、HAR、日志和回执证据必须先脱敏。普通 CI 可验证确定性安全与协议合同，真实付费模型和真实玩家体验不能由其代签。

## 12. 设计决定、风险与外部依据

决定：采用网页直连、localStorage 保存 Key、可见页面内有界执行。取舍是跨刷新保留配置，接受同源脚本可读与共享设备风险；以告知、严格 CSP、第三方脚本约束、显式忘记及停止策略降低风险，不声称消除风险。关页不托管，模型与世界权限严格分离，原件恢复继续沿用统一合同。本次只修改设计，不声明实现、部署或整体验收完成。

官方资料核对日期：2026-10-16。下面仅支持上游接口事实；不证明 oasis7 的实现或实际部署，模型/价格不冻结在本设计。

- [DeepSeek API 入口](https://api-docs.deepseek.com/)与[工具调用](https://api-docs.deepseek.com/guides/tool_calls/)：官方地址、Key 与工具往返；不由服务端接口推断浏览器 CORS。
- [OpenRouter Responses](https://openrouter.ai/docs/api/reference/responses/overview)：无状态历史及 `store` / `previous_response_id` 限制。
- [OpenRouter 路由](https://openrouter.ai/docs/guides/routing/provider-selection)：允许提供商集合、fallback 和必要参数策略。
- [OpenRouter OAuth PKCE](https://openrouter.ai/docs/guides/overview/auth/oauth)：可作为后续免复制 Key 的连接方式，不是首期前置，也不自动提供安全托管。
- [OWASP HTML5 Security](https://cheatsheetseries.owasp.org/cheatsheets/HTML5_Security_Cheat_Sheet.html)：浏览器敏感数据存储和跨域安全风险。

# 受控预览的玩家支持与服务生命周期内容设计

- 主责：`liveops_community`；所属模块为 [readme](../README.md)，产品承诺由玩家接入与发行拥有。
- 生命周期：`active`；适用受限可玩技术预览的反馈说明、人工分流、事故沟通与轮次暂停/结束，不代表当前有开放邀请或持续客服服务。
- 内容语言：中文；英文公开面使用时须按同一事实、范围和下一步另行翻译核对。
- 实现审读基线：`d21019414c46d7e8ba61ab2c59b1f217b144ba88` 的源码；当前上游要求及准确关系见 §2。文案在本文件交付；界面接入、实际发布、运行与运营验收分别取证。
- 制作与验证方法：[玩家支持操作手册](player-support-and-service-lifecycle.manual.md)。

<a id="support-scope"></a>
## 1. 问题、目标与范围

玩家需要知道问题能送到哪里、哪些内容会公开、眼前结果说明什么，以及入口受限或本轮结束后可以采取什么动作。当前 GitHub、native Launcher、Web Launcher 和渠道评论分别提供不同的入口与结果；把它们统称为“客服已收到”会混淆本地保存、远端受理、人工处置和世界结果。

本设计交付可复用的支持说明、反馈分类与处置文字、事故和轮次结束文案，以及它们的制作和检查方法。读者应能选择真实可用的入口，提供最小信息，正确理解提交结果，并找到已记录的处置理由与复核下一步。数据说明必须反映公开追加记录的实际限制。

范围包含现有 GitHub 反馈、Launcher `Bug / Suggestion` 及已获授权的渠道回流；不新增 UI、端点、反馈类型、私密受理、举报裁决、支付、退款、补偿、自动删除或恢复机制。正式客服、商业服务及全产品停止服务的政策仍须由原产品与专业主责决定。本设计不改变历史轮次的关闭状态，也不把“材料已准备”写成“当前正在招募”。

## 2. 上游要求与约束

根 [README 项目状态](../../../README.md#项目状态)拥有当前公开状态。`liveops_community` 组织内容与渠道同步，`producer_system_designer` 决定产品边界，`qa_engineer` 核对候选证据；技术处置由实际拥有入口、存储、安全或世界恢复的专业 owner 执行。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-ENTRY-PREVIEW-FEEDBACK-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-preview-feedback-001)；Trace refs: O7-REQ-bafbce55-b1f3-414c-98e8-2cb5e70e6d1e | 反馈入口如实说明可用性、实际结果和有依据的下一步 | [入口与结果](#support-entry-results) | Launcher / LiveOps / QA；当前声明入口与专业门控 | 不承诺尚未支持的修复 CTA、必达、人工接单或其他入口成功 |
| [REQ-ENTRY-PREVIEW-FEEDBACK-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-preview-feedback-002)；Trace refs: O7-REQ-9bd3819f-e0fa-489c-9f36-6dd2f415f9ef | 有效信号分桶、owner/处置/下一步及可复现 blocker 回同候选 gate | [分流与复核](#support-triage-review) | LiveOps / QA / 对应专业 owner / producer | 人工方法不等于当前轮次已执行，不新增工单协议或回复时限 |
| [REQ-ENTRY-PREVIEW-FEEDBACK-003](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-preview-feedback-003)；Trace refs: O7-REQ-eb290f7e-b49e-42dc-ba88-016ed9c87808 | 只索取必要信息，按实际渠道说明公开、保留和误传处置边界 | [数据边界](#support-data-boundaries) | Launcher / DistFS / 安全与实际平台 owner；原数据合同 | 不承诺私密收件、全面清洗、自动隐藏或全副本删除 |
| [REQ-ENTRY-CLAIM-001](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-001)；Trace refs: O7-REQ-e20ed8c7-6f8d-400a-a883-57b2e772acff | 支持说明发布前绑定候选、primary mode、入口与同窗证据 | [事实与发布状态](#support-claim-source) | 产品 / QA / LiveOps；根 README 与真实发布记录 | 仅支持内容；不代签全渠道或统一发行 gate |
| [REQ-ENTRY-CLAIM-002](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-002)；Trace refs: O7-REQ-f71ac2e9-1586-44af-89b7-e98ff364e70c | 事实漂移时纠正受影响说明，分开消息撤回与事故恢复 | [事故与纠正](#support-incident) | 技术 owner 确认影响与恢复，QA 复核，产品决定范围 | 不赋予技术回滚、上线或补偿权限 |
| [REQ-ENTRY-CLAIM-003](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-003)；Trace refs: O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385 | 可复用文案和每次实填候选分别保留事实、目标、假设、证据边界 | [事实与发布状态](#support-claim-source) | 产品 / QA / LiveOps 的同窗口审核 | 完成本文不表示任何候选已 publish_ready 或已发布 |
| [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001)；Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1 | 本地包、远端 receipt、Web 代理成功只说明反馈路径结果 | [入口与结果](#support-entry-results) | Launcher 的当前门控与传输合同 | 不覆盖全部会话、配置、世界行动或可玩性 |
| [REQ-ENTRY-FREE-001](../../product/player-entry-distribution/free-entry-world-progression-and-recognition.prd.md#req-entry-free-001)；Trace refs: O7-REQ-32e47f2a-2da3-4ca6-aaf0-ef522be4b898 | 说明便利服务中断不自动撤销世界权利、结果或有效义务 | [暂停与结束](#support-pause-close) | 产品及实际 hosting/storage/support、runtime authority | 仅内容边界；不声明付费服务存在，不定义订单、导出、迁移或退款 |
| [REQ-WR-CC-002](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-002)；Trace refs: O7-REQ-19ea1035-2d0a-43fc-a3a8-b0309030273c | 损失说明区分规则内结果、身份历史与当前真实恢复选择 | [损失与举报分流](#support-competition-reports) | gameplay / runtime / QA 确认规则和证据 | 不认定所有损失合法，不实现或代签战损恢复 |
| [REQ-WR-CC-003](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-003)；Trace refs: O7-REQ-eedaa5cf-e91e-48df-9c33-158774b40168 | 事故说明不把支持回复或临时援助当成已确认恢复效果 | [事故与纠正](#support-incident) | 世界恢复项目与 receipt 的原专业 authority | 不提供新恢复项目、免费资源或选择性补偿 |

### 2.2 反馈与数据的准确依赖

[受控 Limited Preview](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#61-受控-limited-preview)已有的正式反馈可用性、真实提交结果、`Blocking / Opportunity / Idea`、owner/处置/下一步、blocker 回到同候选 gate，以及最小信息义务均继续有效，由下文入口、分流、数据和手册中的对应场景实际承接。该条款明确不承诺公开反馈账本、必达、保存期限、调解、人工支持或逐条答复；底层存在公开账本也不能把它改写成产品承诺。

| 原 authority | 本设计依赖的事实或限制 |
| --- | --- |
| [Launcher 当前合同](../../world-simulator/launcher/game-client-launcher-feedback.prd.md#当前合同)与[结果边界](../../world-simulator/launcher/game-client-launcher-feedback.design.md#feedback-result-boundary) | native Ready 后远端优先、失败本地回落；未就绪不落盘；Web 无本地包；各结果不确认世界成功 |
| [最小自动诊断](../../world-simulator/launcher/game-client-launcher-feedback.prd.md#feedback-diagnostic-minimum) | allowlist 摘要、最近最多 200 行自动日志及清洗目标；手写文本不在清洗保证内，现有未验证项保持可见 |
| [DistFS 接口与数据](../../p2p/distfs/distfs-feedback-ledger-and-replication.prd.md#feedback-interface-data)与[公开读取](../../p2p/distfs/distfs-feedback-ledger-and-replication.design.md#des-feedback-public-read) | 公开读、只追加、tombstone 保历史、IP/public-key 基础限流；不提供分级读取或内容审核 |
| [DistFS outbox](../../p2p/distfs/distfs-feedback-ledger-and-replication.design.md#des-feedback-outbox) | 本地写入、入队、复制与全网交付不同；部分错误可能发生在记录已写入之后 |
| [会话与待决请求连续性](../../product/player-entry-distribution/prd.md#player-session-authorization-continuity) | 停止、断线、重新认证不自动取消或重放已接受请求；未知结果先核原请求，不能从未收到回执推导未生效 |
| [Moltbook 升级矩阵](readme-moltbook-liveops-runbook.md#8-升级矩阵) | 对外承诺归产品、缺陷归 QA 与对应工程 owner；渠道评论回流 GitHub，不形成额外客服入口 |

[历史 invite pack](readme-limited-preview-invite-pack-2026-03-22.md#meta)仅是原轮次资料。复用其中分桶或字段时替换真实候选和当前授权，不恢复旧邀请、旧监测时间、旧 owner push 或旧开放状态。

<a id="support-entry-results"></a>
## 3. 入口与结果

Trace refs: O7-REQ-bafbce55-b1f3-414c-98e8-2cb5e70e6d1e。承接[反馈入口与真实结果](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-preview-feedback-001)；真实可用性、未完成不报成功和有依据的下一步适用于本节每条路径。

Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1。承接[本地 session 结果边界](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001)的反馈子义务。

| 玩家所在入口 | 现有动作与消费者 | 可说明的结果 | 失败或未完成后的内容 |
| --- | --- | --- | --- |
| README / 官网 / 渠道 | README 的 [Issues](https://github.com/nil-studio/oasis7/issues)；官网 CN/EN 安装帮助和反馈 CTA；已有修复可走 PR | 以实际创建的 Issue/PR 链接确认记录，不将打开页面、填表或渠道留言视为已提交 | 链接不可达或表单未完成时明确未提交；保留脱敏草稿，稍后使用已核实可用的原入口 |
| native Launcher | `feedback_window.rs` 的门控及 `feedback_entry.rs` 的远端提交 | Ready 且返回 `feedback_id/event_id` 时为远端受理 | 仅该门控通过后的远端失败会尝试本地包；网络错误不能证明远端绝对未写入 |
| native 本地回落 | `feedback_entry.rs::submit_feedback_report` 返回实际路径 | 只确认该路径保存结果；没有人工接收或再次上传证据 | 写入失败仍是失败；分享前检查包内文字，只提供必要脱敏片段。现行 writer 的不覆盖目标尚不能由本文代签 |
| native 链禁用/未就绪 | `feedback_window.rs::submit_feedback` 在 helper 前拒绝 | 未完成提交；没有本地回落包 | 本说明可指向已有 GitHub 入口；不声称按钮旁已有自动修复或可用恢复 CTA |
| Web Launcher | `feedback_window_web.rs` → `/api/chain/feedback` → `/v1/chain/feedback/submit` | 只按结构化返回说明受理、校验、请求中或代理失败 | 无 native 本地包；不要显示“已本地保存”。失败后在仍可编辑的窗口检查草稿；关闭或刷新后的保留未获保证 |

远端受理不表示人工已阅读、必定处理、所有副本已同步或世界动作成功。本地保存不表示团队已收到；Web 请求失败不表示已自动转投 GitHub。说明只给当前专业入口支持的动作，不要求玩家绕过门控或反复点击。

<a id="support-triage-review"></a>
## 4. 最小反馈、人工分流与复核

Trace refs: O7-REQ-9bd3819f-e0fa-489c-9f36-6dd2f415f9ef。承接[有效反馈的处置与下一步](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-preview-feedback-002)。

<a id="support-feedback-minimum"></a>
### 4.1 玩家内容与内部字段

玩家提交前先核对版本、模式和实际入口，再写最短复现。版本未知就写未知及可核查来源，不猜当前 main、下载包或历史案例之间的关系。

| 玩家需要提供的信息 | 使用目的与边界 |
| --- | --- |
| 候选版本或原下载/源码来源；`viewer` / `pure_api` 或尚未确认 | 避免跨候选、跨模式代签。当前 Launcher 远端 `game_version` 为 `unknown`，不能依赖自动上报得到真实版本 |
| native/Web/官网安装等实际表面，平台及必要环境 | 找到入口 owner；只写复现所需环境，不上传完整设备或账户资料 |
| 简短标题、最少步骤、预期与实际结果 | 复现和判断差异；日志采用必要的脱敏错误片段 |
| 影响、发生频率、是否能继续 | 分辨阻断与摩擦，不要求玩家自行定严重等级 |
| 已有 Issue/feedback/event 定位或公开证据 | 存在且可公开时再提供；不强索取账户标识、私密世界记录或他人内容 |

人工处置在原 Issue/PR 或既有轮次记录补充候选与入口、`Blocking / Opportunity / Idea`、owner、处置及理由、下一步和原记录关联。每条有效信号保持明确的当前 owner；专业转交未确认前，由实际接收该信号的 LiveOps owner 承接下一步，不以“待转交”造成无人负责。字段不是新协议或新台账；不要求玩家复制工程任务管理材料。GitHub 仍使用[现有五类 Issue](../../engineering/workflow/source-of-truth.md#issue-lifecycle)及其表单，建单不等于开工。

### 4.2 类别与责任

| 信号 | 已有玩家类别 | 人工分流与主责 | 必须保留的判断 |
| --- | --- | --- | --- |
| 崩溃、无法进入、可复现路径阻断 | `Bug` | `Blocking`；QA + 对应工程 owner | 已确认 blocker 回到受影响专业域和同候选 gate；不继续沿用旧 pass |
| 难懂说明、安装摩擦、低风险体验问题 | `Bug` 或 `Suggestion`，按实际问题选 | `Opportunity`；LiveOps 或文档/入口 owner | 是否阻断由实际影响决定，不能按“只是文案”自动降级 |
| 新机制、功能方向、合作意向 | `Suggestion`；渠道提问也可人工回流 | `Idea`；producer | 记录需求与下一步，不承诺采纳、时间或合作 |
| 公开状态、版本或已验证范围被误述 | 按所在入口报告文字差异 | LiveOps 纠正，QA 核证据，producer 审核 | 分桶依实际影响；claim drift 不因属于 Opportunity 而免于纠正 |
| 疑似滥用、骚扰、越权或敏感误传 | 仅提交能安全公开的最小摘要 | LiveOps 分流给实际安全/工程或平台权限 owner；QA 核可复现行为 | 不要求公开身份、私聊或秘密；不预先裁定违规、提供私密收件箱或许诺处罚 |

这些人工桶与 `Bug / Suggestion` 是不同层次，不扩展 Launcher payload，也不替代既有 Issue 类型和工作流。

### 4.3 告知与复核

只有实际处理后才能使用相应结论。可用结论是“需要补充最小复现”“与已有记录归并”“已转交对应 owner”“当前证据符合规则内结果”“尚无足够证据判断”或“已按所标版本验证修复”。每次告知保留理由、实际记录定位和下一步；不能只写“已关闭”。

复核沿原记录追加新的脱敏事实，由原 owner 或承接该义务的专业 owner 重查；涉及世界规则或处罚权限时回原 authority。无法提供敏感证据的公开替代时，明确当前无法完成该项核查，不诱导当事人公开秘密。复核不是新的仲裁制度，不保证逐条人工回复或处理期限。

<a id="support-competition-reports"></a>
### 4.4 规则内损失与疑似滥用

Trace refs: O7-REQ-19ea1035-2d0a-43fc-a3a8-b0309030273c。承接[损失与恢复路径](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-002)的解释子义务。

先核对当时规则、授权范围、权威结果与适用版本，再解释资源损失或竞争后果；玩家不满意不自动等于 bug，规则允许竞争也不能证明所有骚扰或越权合法。疑似行为越界单独核查，不能以“这是竞争游戏”直接结案。

已确认的损失说明同时写清损失、保留的身份/历史，以及当时真正可用的 repair、rebuild、pivot、等待、退出或其他专业恢复路径；未实现或未验证的路径不作为可点击承诺。支持回复不授予删历史、改胜负、取消有效义务或补发资产的权力。

<a id="support-data-boundaries"></a>
## 5. 数据边界：用途、公开范围与保留限制

Trace refs: O7-REQ-eb290f7e-b49e-42dc-ba88-016ed9c87808。承接[最小反馈与公开资料保护](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-preview-feedback-003)，具体索取范围见[最小字段](#support-feedback-minimum)；本节规定其用途、可见性、保留限制和误传处理。

### 5.1 按实际数据路径说明

| 数据路径 | 用途与内容 | 访问、保留与删除边界 |
| --- | --- | --- |
| Launcher 手写标题/描述 | 由玩家主动提交的复现或建议；远端发送 category/title/description/platform/game_version | 手写文本不受自动诊断清洗保证。不要输入凭据、个人联系方式、私密日志、他人身份或私聊 |
| native 本地 JSON | 时间、类别、标题/描述、allowlist 配置摘要、最近最多 200 行清洗日志 | 文件在返回的本地路径，由设备权限控制；当前专业清洗与不覆盖目标仍有待验证项。未核实自动清理期限；本地保存不是远端送达，本地删除也不能撤回已提交内容 |
| DistFS 公开读视图 | 内容、作者公钥、类别、平台/版本、追加事件、tombstone 状态和时间；供公开反馈读取 | 无分级读取合同，不适合敏感材料；公开读能力不等于已有公开 HTTP 查询网页；runtime 使用节点派生的反馈 signer，作者公钥不能当作已验证玩家身份 |
| DistFS 原始 root/event、nonce/audit 与复制记录 | 签名/nonce/时间用于验证，提交连接 IP/public-key 用于基础限流及审计；复制保存原记录 | root/event 保留提交连接 IP，复制也保留该字段；公开视图省略 IP 不能证明原记录私密。IP 来自 runtime 连接 peer，可能是本地或代理，不等于最终玩家公网 IP |
| GitHub / 已授权渠道记录 | 公开协作、复现和处置；按实际平台权限访问 | 本设计未核实平台保留、编辑历史或删除能力；不能保证关单后删除、全副本撤回或固定保存期 |
| 运营汇总 | 从已有记录统计有效信号、分桶、重复误解、未结阻断和实际处置 | 只保留必要脱敏摘要与原记录定位，不新增个人档案、隐藏遥测或原始诊断副本；原证据依其既有权限与保留合同管理 |

技术来源为 [native 传输与本地包](../../../crates/oasis7_client_launcher/src/feedback_entry.rs)、[Web 请求](../../../crates/oasis7_client_launcher/src/feedback_window_web.rs)、[runtime 请求构造](../../../crates/oasis7_chain_runtime/src/host/feedback_submit_api.rs)、[节点派生 signer](../../../crates/oasis7_chain_runtime/src/host/signer_derivation.rs)、[提交连接来源](../../../crates/oasis7_chain_runtime/src/host/status_server_support.rs)、[DistFS 记录与公开视图](../../../crates/oasis7_distfs/src/feedback.rs)和[复制入库](../../../crates/oasis7_distfs/src/feedback/replication.rs)。这些是固定输入审读接点，不是本候选已运行的证据。

**只追加记录的撤回限制。** root/event 不物理删除；tombstone 是追加事件。当前 `to_public_view` 在 `tombstoned=true` 时仍返回原 `content` 与 append events，因此标记撤回不能解释为正文已隐藏。runtime 的七天 `FEEDBACK_TTL_MS` 用于签名请求有效期，不是保存期限；到期不会据此证明历史记录或副本删除。复制受原网络权限约束，但没有私密受理、全节点及时一致或全副本擦除保证。

### 5.2 敏感误传与举报材料

提交前先在玩家可控制的草稿中移除不必要内容；本地包在分享前逐项检查，自动清洗不是人工检查的替代。只复制能安全公开的必要摘要。

发现已经误传时，接收者停止**人工再次转发、引用或附加原文**，记录脱敏定位和受影响路径，并交实际技术/平台权限 owner 核对现有可执行操作。不能把这一动作说成停止了全部网络复制。若涉及凭据，由持有人使用其现有账号或 Provider 的凭据处理流程；公开回帖不重复凭据，也不索取更多秘密。

对每项处置分别记录对象、操作者权限、实际结果与仍可能存在的副本；仅在操作真正可用且已执行后才说明隐藏、撤回或其他结果。现有 Launcher 没有由本设计新增的删除/举报按钮，tombstone 也不是内容隐藏方法。没有已核实私密入口时，明确公共反馈不适合接收敏感材料；玩家可只报告“存在需私密核查的问题”及安全的入口/版本摘要，不发送详细材料。owner 先决定是否有获准且可核实的后续受理方式，未确定时保持该项未核查。

<a id="support-claim-source"></a>
## 6. 可发布内容的事实与状态

Trace refs: O7-REQ-e20ed8c7-6f8d-400a-a883-57b2e772acff、O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385。分别承接[同窗 claim](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-001)和[四层分离](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-003)的支持内容范围。

每次实填文案在已有简报或 PR/Task 中分别写当前事实、产品目标、假设、证据边界，再核对候选、模式、入口、更新时间与根 README。只把已确认事实和真实下一步发布给玩家；目标和假设出现时明示身份。`draft / reviewed / publish_ready / published / stale / withdrawn` 沿原沟通合同使用，批准草稿不能当发布证据。

实际发布记录保留渠道、时间、URL/message ID 和 owner；内部证据与可公开来源分开。官网、README、公告或帖子仅处理实际受影响的活跃表面，CN/EN 同一事实必须一起纠正。不能用本文或历史 invite pack 独立批准外发。

<a id="support-incident"></a>
### 6.1 事故、纠正与恢复说明

Trace refs: O7-REQ-f71ac2e9-1586-44af-89b7-e98ff364e70c、O7-REQ-eedaa5cf-e91e-48df-9c33-158774b40168。分别承接[纠正与撤回](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-002)及[有界恢复效果](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-003)的说明边界。

| 对外说明所处阶段 | 需要确认的内容 | 玩家下一步与转换条件 |
| --- | --- | --- |
| 正在调查 | 报告所涉版本/入口及已知现象；未知影响单列 | 暂停受影响的新尝试或使用仍受支持入口；不猜损失、原因或恢复时间 |
| 已确认影响 / 局部受限或暂停 | 技术 owner 确认影响和采取的动作，producer 明确受限范围 | 核原请求结果、等待或安全停止；仅暂停说明不能证明已取消动作 |
| 恢复核验中 | 修复或恢复动作已有实际来源，QA 正核对应候选和入口 | 尚无恢复结论前沿用限制，不用技术动作完成代签可玩 |
| 在已证范围恢复 | 同候选证据、恢复范围、残余限制及权威决定 | 仅给已支持的重新进入/重新验证步骤；未知原请求先核对 |
| 事件收口 | 影响、已完成处置、未结事项与下一步有定位 | 旧错误文案纠正，剩余问题继续有承接；不宣称风险清零 |

每次更新注明信息时间和下一次可信信息更新时间；该时间是一次实际安排，不构成通用回复或恢复 SLA。无法确认时间时直说尚未确认，不填猜测日期。LiveOps 只执行沟通与升级；技术回滚、世界恢复、资产处置由各自 authority 决定。已确认世界因果与历史不能通过支持回复改写，任何恢复贡献仍须权威结果确认其实际效果。

<a id="support-pause-close"></a>
### 6.2 暂停与轮次结束

Trace refs: O7-REQ-32e47f2a-2da3-4ca6-aaf0-ef522be4b898。承接[便利服务中断边界](../../product/player-entry-distribution/free-entry-world-progression-and-recognition.prd.md#req-entry-free-001)的内容子义务。

说明先明确暂停的是渠道活动、候选入口、本地会话还是便利服务。渠道停止收集不等于世界停止运行；停止本地 session 不等于取消已接受的 intent；有独立有效授权的 Agent 是否继续，仍由其原合同决定。

暂停或结束交接在既有记录中保留：候选/模式/入口与生效时间；已停止和仍可用的范围；未结反馈、负责 owner 及去向；待决/未知动作与核对限制；本地材料和公开记录的真实处理方式；下一次信息或现有可用入口。没有可核实的后续入口就明确说明，不给空链接。轮次结束后不继续用原邀请文案招募。

玩家已有身份、权限、已确认资产和历史、仍有效的合同义务不由轮次关闭自动撤销。可提供的备份、读取、等待、独立路径或安全停止仅引用当前专业合同；不承诺全产品导出、服务商迁移、恢复全部资产或商业退款。若未来真的结束一项持续服务，producer 须与专业 owner 先确定其实际范围、数据取得与保留安排及未结义务，不能直接套用“本轮结束”代替服务结束设计。

<a id="support-copy"></a>
## 7. 可复用中文文案

以下是本次交付的正文源。`〈…〉` 只表示每次发布必须由实际记录填入的事实变量；选用者不得保留空变量、示例候选或未核实链接外发。未发生的调查、转交、恢复和关闭不使用相应完成句。用于界面时先按入口与长度重新审校，不表示当前 UI 已接入。

### 提交前：选择入口与公开性

> 当前是受限可玩技术预览。你可以通过项目的 GitHub Issues 报告问题；Launcher 的反馈入口可用时，也可提交 Bug 或建议。请提供所用版本、平台、入口、最短复现步骤，以及预期和实际结果。版本无法确认时请注明未知，不必提交完整配置或日志。
>
> 这些反馈路径不适合提交秘密。Launcher 的远端反馈会进入可公开读取、只追加的记录；标记撤回不删除原文，当前公开读取仍返回原文和追加内容。原始记录还保存用于验证和基础限流的公钥、时间与提交连接 IP 等资料，复制记录也保留连接 IP；它可能是本地或代理地址。请勿发送密钥、密码、个人联系方式、私密聊天或他人的敏感信息。本地保存和远端受理都不表示人工已阅读或承诺逐条回复。

### 远端已受理

> 反馈入口已返回记录标识：〈feedback_id / event_id〉。请保留该定位。它只表示本次反馈路径的受理结果，不证明所有副本已同步、人工已处理或游戏动作已成功。后续补充请关联原记录，避免反复提交相同内容。

### 仅本地保存

> 远端提交未获得成功确认，本次反馈已保存到〈实际返回路径〉。这份本地文件不表示团队已收到，也不能证明远端绝对没有写入。分享前请检查标题、描述和自动诊断，只提供必要的脱敏内容；不要直接公开完整反馈包。

### 提交未完成或入口不可用

> 这次反馈没有获得成功确认，原因是〈实际错误或不可用状态〉。请保留可安全公开的草稿并核对现有记录；不要把重复点击当作确认送达。〈已核实可用时：你也可以使用项目 GitHub Issues，并说明原尝试及已知记录标识。〉浏览器路径没有自动保存 native 反馈包。

### 需要补证、归并与转交

> 当前还缺少〈复现必需的一项信息〉，暂时无法判断〈具体问题〉。只需补充〈安全的最小信息〉，请不要附完整配置、凭据或私密内容。
>
> 这条反馈与〈已有记录〉描述的是同一问题，后续信息将在该记录继续整理。归并保留你的新增事实，不表示问题已经修复。
>
> 已将〈脱敏问题摘要〉交给〈实际 owner 或可公开承接记录〉核查。当前结论是〈已确认事实或尚不能判断〉，下一步为〈真实动作〉。这次转交不构成处理时限或修复承诺；有新证据可在原记录补充并请求复核。

### 规则内损失或疑似滥用

> 我们将分别核对游戏规则允许的结果和你提出的越权或骚扰问题。请只提供能安全公开的版本、入口与行为摘要，不公开个人身份、私聊或凭据。当前可确认的是〈事实与依据〉；〈其余问题〉仍未核实。可用的下一步是〈经专业 authority 确认的路径或补证方式〉。支持回复不能直接改写已确认的世界结果或承诺补发资产。

### 敏感信息可能误传

> 请不要再次转发或引用原始敏感内容。请仅提供不含秘密的记录定位，便于核对实际路径和可执行的处理；若涉及凭据，请通过你现有的账号或服务商流程处理该凭据。当前公开反馈不是私密收件箱，我们不能承诺全部副本可删除。公开账本的撤回标记仍可能显示原文；任何实际处理结果会按其真实范围说明。

### 正在调查或局部暂停

> 截至〈信息时间〉，我们正在核查〈版本 / primary mode / 入口〉的〈已确认现象〉。目前已确认影响为〈范围；尚未确认的部分直说未知〉。当前采取的动作是〈权威确认的处置〉；请〈适用的等待、停止新尝试或其他安全下一步〉。对于已经提交但结果未知的动作，请保留原请求定位，恢复读取后先核对原结果，不要自动重提。下一次信息更新时间为〈实际安排；未确认时明确写尚未确认〉，这不是恢复时间承诺。

### 已恢复到限定范围

> 截至〈信息时间〉，〈候选 / primary mode / 入口〉已按〈可公开核验来源〉完成所述范围的恢复核验。可以〈当前支持的重新进入或重新验证动作〉；仍存在〈实际残余限制〉。先前结果未知的请求仍需按原定位核对，重新进入不等于取消、补发或重做原动作。其他版本和入口不由这次结果代签。

### 纠正旧表述

> 〈旧说明定位〉中的〈具体表述〉已不符合当前证据，现更正为〈已核实的窄范围事实〉。请以根 README 当前状态和〈适用说明〉为准。本次修改纠正公开表述；它不单独证明软件回滚、世界恢复或发行升级。

### 本轮暂停或结束

> 〈候选 / 模式 / 入口范围〉的本轮受控预览检查于〈生效时间〉〈暂停 / 结束〉。调整范围为〈实际停止的活动或入口〉；〈仍可用的已核实范围；没有则明确说明〉。未结反馈由〈实际承接记录〉继续说明处置和下一步；没有已确认安排的事项仍保持未结。
>
> 本轮活动结束不会自动取消已接受的动作、撤销已有世界权利或清除历史。请按当前可用入口核对待决结果；本地材料由你按设备权限保管，已提交公开内容的追加与删除限制仍然适用。后续信息以〈已核实入口或明确无当前入口〉为准，旧邀请不再作为新的参与许可。

<a id="support-delivery"></a>
## 8. 制作、消费与持续成本

本文 §7 是可编辑中文文案源，版本随 Git 提交定位；手册提供实填、检查、发布与交接方法。源文案由项目原有产品和专业合同派生，不引入第三方素材。无需独立导出文件；实际发布文字、渠道位置与使用的文案版本留在原 PR/Task 或发布记录。

消费者是 README/官网帮助说明的维护者、Launcher 内容维护者、LiveOps 回帖与公告执行者。当前源码入口分别为 `site/index.html`、`site/en/index.html`、`crates/oasis7_client_launcher/src/feedback_window.rs` 与 `feedback_window_web.rs`；本文未改动这些文件，不能宣称玩家已经看到新说明。产品目标、数据路径或入口能力变化时，先更新原 authority，再修本文及手册，最后在获准的实际消费面同步并复核。

首次采用时制作一组覆盖入口结果、敏感误传和暂停/恢复/结束的代表性文案，按手册桌面演练；每次真实复用只复核变化的事实、入口和公开范围。CN/EN 配对公开面需逐项核对，不能只更新中文。自动 UI、传输或网络验证沿原专业方法，不以文字审校替代。

持续成本依据已有运营记录中的实际检查窗口、有效信号及分桶、重复误解、未结 blocker、已采取动作、owner 和实际投入判断；无法核实的人时或成本保持未知，不编制虚假产能。producer 根据这些输入决定继续、暂停或重新评估范围。没有执行者或可用窗口时不新增邀请或承诺固定回应频率；历史首轮信号目标和巡检节奏不成为当前 SLA。实际记录使用现有 PR/Task，不再建立个人信息台账。

## 11. 验证与验收

本设计以文案、数据边界和人工方法为交付物。静态源码审读说明方法的输入；文档检查、桌面案例、界面验证、真实受理、复制和实际发布分别记录，不互相替代。内容审校由作者及适用产品/LiveOps/隐私或技术 owner 按风险完成，验证记录留在当前 PR。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-ENTRY-PREVIEW-FEEDBACK-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-preview-feedback-001)；Trace refs: O7-REQ-bafbce55-b1f3-414c-98e8-2cb5e70e6d1e | [入口与结果](#support-entry-results) | 所声明反馈入口的可用性、拒绝、受理与本地结果各自真实 | [入口结果检查](player-support-and-service-lifecycle.manual.md#support-entry-check)：固定候选源码及各路径桌面结果，运行另取证 | 实际 PR 的输入、文案选择与判定 | 不证明实际入口在线、UI 或可靠交付 |
| [REQ-ENTRY-PREVIEW-FEEDBACK-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-preview-feedback-002)；Trace refs: O7-REQ-9bd3819f-e0fa-489c-9f36-6dd2f415f9ef | [分流与复核](#support-triage-review) | 每条有效信号有分类、当前 owner、处置和下一步；blocker 重验 | [分流检查](player-support-and-service-lifecycle.manual.md#support-triage-check)：同候选阻断/摩擦/建议与新证据复核桌面案例 | 实际 PR 的案例处置与 owner 承接判定 | 不证明真实受理、专业修复或 gate 已重跑 |
| [REQ-ENTRY-PREVIEW-FEEDBACK-003](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-preview-feedback-003)；Trace refs: O7-REQ-eb290f7e-b49e-42dc-ba88-016ed9c87808 | [数据边界](#support-data-boundaries) | 最小索取、公开说明和误传处理不虚构清洗/保留/删除能力 | [数据检查](player-support-and-service-lifecycle.manual.md#support-data-check)：实际字段/源码与虚构误传反例逐项对照 | 实际 PR 的字段、文案与反例检查结果 | 不证明隐私算法完善、实际删除或私密受理能力 |
| [REQ-ENTRY-CLAIM-001](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-001)；Trace refs: O7-REQ-e20ed8c7-6f8d-400a-a883-57b2e772acff | [事实与发布状态](#support-claim-source) | 每份支持文案只使用同候选/模式/入口的证据 | [口径检查](player-support-and-service-lifecycle.manual.md#verify-claim-evidence)：固定输入下比较事实漂移、跨候选和单语言负例 | 实际 PR 的实填文案、来源对照与判定 | 不证明已发布、完整发行或全渠道审核 |
| [REQ-ENTRY-CLAIM-002](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-002)；Trace refs: O7-REQ-f71ac2e9-1586-44af-89b7-e98ff364e70c | [事故与纠正](#support-incident) | 事故后旧表述及时失效，纠正不冒充技术恢复 | [事故演练](player-support-and-service-lifecycle.manual.md#verify-incident-communication)：调查、暂停、核验和限定恢复桌面案例 | 实际 PR 的状态文案及未结事项 | 不证明实际事故处置、技术回滚或生产恢复 |
| [REQ-ENTRY-CLAIM-003](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-003)；Trace refs: O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385 | [事实与发布状态](#support-claim-source) | 文案源和真实发布状态分离，目标/假设不写成事实 | [口径检查](player-support-and-service-lifecycle.manual.md#verify-claim-evidence)：四层对照及发布记录缺失负例 | 实际 PR 的可发布范围判定 | 不证明候选 publish_ready 或真实外发 |
| [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001)；Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1 | [入口与结果](#support-entry-results) | 本地、远端和 Web 结果不升级为世界成功 | [入口结果检查](player-support-and-service-lifecycle.manual.md#support-entry-check)：逐路径对照源码与虚构返回 | 实际 PR 的文案/结果对应和未验证项 | 不运行或代签 native/Web 闭环、网络或世界行动 |
| [REQ-ENTRY-FREE-001](../../product/player-entry-distribution/free-entry-world-progression-and-recognition.prd.md#req-entry-free-001)；Trace refs: O7-REQ-32e47f2a-2da3-4ca6-aaf0-ef522be4b898 | [暂停与结束](#support-pause-close) | 停止便利交付不撤销世界权利或产生退款/迁移承诺 | [暂停结束演练](player-support-and-service-lifecycle.manual.md#verify-pause-and-close)：未结反馈、未知动作、本地/公开资料分别交接 | 实际 PR 的结束文案与交接推演 | 不证明付费服务、世界恢复、数据导出或商业政策 |
| [REQ-WR-CC-002](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-002)；Trace refs: O7-REQ-19ea1035-2d0a-43fc-a3a8-b0309030273c | [损失与举报分流](#support-competition-reports) | 保留损失、身份历史与真实恢复边界，不先行判无滥用 | [损失与举报检查](player-support-and-service-lifecycle.manual.md#verify-world-boundaries)：规则内损失与未知越权分开 | 实际 PR 的分类、文案和准确依赖 | 不代签战争/恢复实现或正式举报裁决 |
| [REQ-WR-CC-003](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-003)；Trace refs: O7-REQ-eedaa5cf-e91e-48df-9c33-158774b40168 | [事故与纠正](#support-incident) | 回复、修复或重新进入不等于恢复效果已确认 | [损失与举报检查](player-support-and-service-lifecycle.manual.md#verify-world-boundaries)：缺 receipt 的恢复与补偿负例 | 实际 PR 的结果边界判定 | 不证明有界恢复项目、贡献结算或补偿能力 |

## 12. 决策、风险与未覆盖范围

采用“已有入口 + 最小公开信息 + 原记录处置”的人工方法，复用真实反馈路径而不新增未经确认的私密承诺。原公开账本不适合敏感材料；文字说明也不能替代提交前实际可见的告知、技术保护或实现修复。界面接入仍须由对应消费者完成并验证。

| 尚需决定或核实的范围 | 当前可执行边界 | 决策主责与触发条件 |
| --- | --- | --- |
| 私密举报、敏感材料受理与访问人员 | 公共入口仅收安全摘要；没有验证通过的私密入口就不接收详细秘密 | producer + 安全/实际平台 owner；开展私密收集之前 |
| 正式社区规则、处罚、复核与申诉权威 | 区分规则内竞争和疑似滥用，回原规则及平台权限，不赋予 LiveOps 世界裁判权 | producer + gameplay/安全 owner；要承诺社区裁决或制裁之前 |
| 统一保留期、删除/隐藏能力及副本处理 | 不新增数据副本；准确说明公开追加、tombstone、peer IP 与期限未知；只报告实际执行范围 | 数据/安全/存储 owner + producer；改变数据用途或作保留/删除承诺之前 |
| 正式持续服务及其结束安排 | 本文仅关闭明确轮次/入口；世界结果、既有义务和数据限制保持原合同 | producer + 服务/存储/runtime owner；开始持续服务或决定停止该服务之前 |
| 付费支持、退款、补偿、渠道/地区协议及隐私材料 | 不声明服务已售卖或作法律/商业保证；恢复与资源处置走原权威 | producer 与相应专业主责；选定发行渠道、地区或商业服务之前 |

这些未覆盖项不使当前反馈与事故说明成为空白：本文件已经给出最小提交、真实结果、分流复核、误传处置和暂停结束的完整文字与方法。当前必要内容一旦与实现、入口或证据冲突，先收窄该内容并修正原主责，不以未决政策为由继续传播错误保证。

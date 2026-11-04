# 玩家支持、社区与服务生命周期内容设计

- 主责：`liveops_community`；所属模块为 [readme](../README.md)，产品承诺由玩家接入与发行拥有。
- 生命周期：`active`；承接受限技术预览的反馈说明，以及实际选定发行范围、项目可管理社区、支持资料与可选服务退出的内容合同；不代表当前有开放邀请或持续客服服务。
- 内容语言：中文；英文公开面使用时须按同一事实、范围和下一步另行翻译核对。
- 既有反馈实现审读基线：`d21019414c46d7e8ba61ab2c59b1f217b144ba88`；发行与运营补充的文档/源码输入为 `f4f8e1bb1be6cb9746ea767af3bdf71204e347e3`，准确关系见 §2。文案和人工方法在本文交付；实际采用、界面接入、发布和运营验收分别取证。
- 制作与验证方法：[玩家支持操作手册](player-support-and-service-lifecycle.manual.md)。

<a id="support-scope"></a>
## 1. 问题、目标与范围

玩家需要知道问题能送到哪里、哪些内容会公开、眼前结果说明什么，以及入口受限或本轮结束后可以采取什么动作。当前 GitHub、native Launcher、Web Launcher 和渠道评论分别提供不同的入口与结果；把它们统称为“客服已收到”会混淆本地保存、远端受理、人工处置和世界结果。

本设计交付可复用的支持说明、反馈与社区处置规则、发行材料的适用判定、支持资料保留与请求方法，以及事故、轮次和持续服务结束的内容合同。读者应能选择真实可用的入口，提供最小信息，正确理解提交或处置结果，并找到依据与复核下一步。数据说明必须反映公开追加记录的实际限制。

范围包含现有 GitHub 反馈、Launcher `Bug / Suggestion` 及已获授权的渠道回流；社区规则只作用于项目具有实际管理权限的内容表面。资料政策只处理项目实际控制的运营副本，服务退出只处理决定缩减或终止的具体提供者和服务。各节区分可直接审校的内容与尚待实现、配置或运行核验的消费者。

本文不新增 UI、端点、反馈类型、私密收件、账号或世界处罚、支付/退款/补偿、自动删除或恢复机制；也不选择发行国家、商店或商业定价。具体发行适用结论、持续服务的履行窗口和技术能力由其主责在使用前核实。本设计不改变历史轮次的关闭状态，也不把“材料已准备”写成“当前正在招募”。

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
| [REQ-ENTRY-RELEASE-MATERIALS-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-release-materials-001)；Trace refs: O7-REQ-a0b94d0f-456c-4702-8933-2232fdf0a544 | 实际渠道、受众、地区和数据用途决定材料适用性；缺项限制相应发布范围 | [发行材料](#support-release-materials) | producer / LiveOps / 素材与相应平台、地区专业主责 | 不声明已获分级、平台审核或地区许可 |
| [REQ-ENTRY-CANDIDATE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-candidate-001)；Trace refs: O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b | 发行内容、入口和材料使用同候选/模式/窗口 | [发行材料](#support-release-materials) | 原发行资产、Runtime/P2P 和 QA authority | 本文只承接内容与材料输入，不代签统一 release gate |
| [REQ-ENTRY-COMMUNITY-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-community-001)；Trace refs: O7-REQ-cd283368-58dd-4538-93a8-7dae3cb2de79 | 按行为与实际权限采取最小适当动作，保留理由和复核 | [社区规则](#support-community-rules) | 实际渠道管理者 / 无利益冲突的复核者 / producer | 不授予跨平台或世界处罚权，不虚构敏感举报入口 |
| [REQ-ENTRY-SUPPORT-DATA-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-support-data-001)；Trace refs: O7-REQ-96d24911-2209-4606-999e-545e4cd10fe9 | 按用途与可控制副本执行有限保留、请求核对和结果告知 | [保留与数据请求](#support-retention-requests) | 实际数据/平台权限 owner；原技术数据合同 | 不覆盖不可控历史、凭据或世界状态的删除实现，不代签地区法定义务 |
| [REQ-ENTRY-SERVICE-EXIT-001](../../product/player-entry-distribution/free-entry-world-progression-and-recognition.prd.md#req-entry-service-exit-001)；Trace refs: O7-REQ-c95314c6-feb1-40cd-8c0f-499830f09c3f | 具体持续服务退出前说明窗口、真实下一步和未结义务，普通与紧急停止分别处理 | [持续服务退出](#support-service-exit) | producer / 服务提供者 / 数据、身份、Runtime 和账务 owner | 不证明服务上线、导出/迁移/退款或全产品停止服务已经可执行 |
| [REQ-ENTRY-FREE-001](../../product/player-entry-distribution/free-entry-world-progression-and-recognition.prd.md#req-entry-free-001)；Trace refs: O7-REQ-32e47f2a-2da3-4ca6-aaf0-ef522be4b898 | 具体服务退出不撤销世界身份、控制与有效义务，不从便利状态推导世界结果 | [持续服务退出](#support-service-exit) | 服务提供者 / 原身份与 Runtime authority | 不授予恢复、退款、补偿或世界写权限 |
| [REQ-ENTRY-COMMUNITY-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-community-001)；Trace refs: O7-REQ-cd283368-58dd-4538-93a8-7dae3cb2de79 | 受理与复核安排对应实际可用人力；能力不足时保留待核而非虚报完成 | [运营容量与成本](#support-capacity-cost) | LiveOps / QA / producer / 渠道 owner | 不新增统一回复时限，不以信号数量代替处置质量 |
| [REQ-ENTRY-SERVICE-EXIT-001](../../product/player-entry-distribution/free-entry-world-progression-and-recognition.prd.md#req-entry-service-exit-001)；Trace refs: O7-REQ-c95314c6-feb1-40cd-8c0f-499830f09c3f | 支持与退出承诺有实际执行窗口和可核成本，未结责任不因人手不足消失 | [运营容量与成本](#support-capacity-cost) | producer / 服务提供者 / 各执行 owner | 不定义商业价格、售后 SLA 或全项目制作预算 |

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

<a id="support-community-rules"></a>
### 4.5 项目社区的行为、处置与复核

Trace refs: O7-REQ-cd283368-58dd-4538-93a8-7dae3cb2de79。承接[项目社区处置](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-community-001)。本节是公开规则的可编辑源；采用时说明适用的仓库、讨论或渠道、管理者角色及真实复核入口。文件存在不表示规则已经展示给参与者或平台权限已经就绪。

规则评价可观察行为及其影响，不按是否赞同项目、投入金额、游戏内阵营或输赢作判断。允许批评规则、报告缺陷和要求解释；不得公开他人敏感资料或凭据、实施具体威胁/持续定向骚扰、冒充官方或他人收集秘密、传播钓鱼/恶意下载、重复灌水以阻碍正常讨论。善意误传与恶意散布可以采用同样的即时防扩散动作，但责任结论须分别核查。

| 观察到的行为 | 首选动作与升级条件 | 处置边界与复核 |
| --- | --- | --- |
| 强烈但针对产品的批评、规则内竞争损失、可复现缺陷 | 保留实质内容，说明已知事实；按原反馈/玩法 authority 核查 | 不因负面评价处罚；玩家报告损失仍可同时提出独立滥用事实 |
| 一次偏题、重复提问或无恶意错误 | 指向原记录、解释相关规则，必要时整理或归并 | 保留新增事实；“重复”不等于“已解决”，技术问题仍走原 owner |
| 经提醒后仍持续重复刷屏、无关推广或绕过同一限制 | 在有实际权限的表面先采用范围最小的内容整理、单讨论限制或平台提供的临时参与限制 | 决定写明行为依据、对象、起止或复核时间；不得连带关闭其无关有效缺陷 |
| 具体威胁、持续定向骚扰、冒充收集凭据、恶意链接或敏感信息公开 | 优先减少继续曝光；由有权限者执行相应平台处置，安全 owner 核对凭据/技术风险；必要时交平台 | 紧急保护可先于完整责任判断，但保留最小安全记录并随后说明；没有权限不能声称已移除或已封禁 |
| 指控存在，但证据不足或涉及他方私密内容 | 只索取版本、入口、行为与记录定位的安全摘要，明确未核查；需秘密时先核实受理能力 | 不公开被指控者私密内容，不从举报数量直接推定成立，不虚构私密收件箱 |

处置由实际渠道管理者负责，记录原文的安全定位、适用规则、可核事实、动作范围、执行结果和复核条件；不复制敏感原文来证明已处理。拟执行、平台已受理和当前展示已变化分别说明。限制参与时，优先使用平台支持的有期限措施；平台仅支持人工解除时，在原记录设置明确复核时间和负责人，不能写自动到期。紧急保护的复核先于其他非紧急互动，具体窗口受实际人员约束，不虚构统一响应 SLA。

参与者可通过当时已核实可用的原记录或发布的复核入口提出新增事实；原讨论锁定时，管理者须先给出仍能使用的替代路径，无法提供则如实说明复核尚不可达并安排恢复。复核检查适用规则、证据、权限、必要性、限制范围和新事实。原处置者涉及个人利益、是争议当事人或无法客观复核时，由其他有权限人员处理；没有合适人员时标为待复核，保留必要临时保护，不扩展为永久结论。改变决定时说明更正及已撤回的限制，不删除原判断的安全摘要或伪称从未发生。

**平台动作的实际含义。** GitHub 的隐藏评论是可展开的折叠，不适合用来声称秘密已不可见；正文编辑和编辑历史处置也不同，见 [GitHub 评论管理](https://docs.github.com/en/communities/moderating-comments-and-conversations/managing-disruptive-comments)及[评论编辑历史](https://docs.github.com/en/communities/moderating-comments-and-conversations/tracking-changes-in-a-comment)。这些官方说明只证明平台有相应操作，执行前仍须核对当前仓库、账号权限和结果。其他渠道沿原 runbook 核实，不把 GitHub 能力移植为跨平台保证。

渠道巡检 P1/P2/P3、`Bug / Suggestion` 与 `Blocking / Opportunity / Idea` 保持原定义；行为处置不新增 Issue 类型，也不替代缺陷优先级。社区管理不更改账号身份、Agent 控制、世界资产、声誉规则、合同或战争结果；涉及这些对象时交原专业 authority，不能用删帖或限制评论代替世界裁决。

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

<a id="support-retention-requests"></a>
### 5.3 支持资料的保留与数据请求

Trace refs: O7-REQ-96d24911-2209-4606-999e-545e4cd10fe9。承接[按用途和控制范围保留支持资料](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-support-data-001)。本节新增的是项目可控运营副本的内容政策与人工处置方法，不改变 §5.1 中任何技术记录的权限、保存、复制或删除语义。

**默认不另存原件。** 优先在原记录处理，内部协作只留必要的脱敏摘要和定位。确需保存最小复现片段时，接收 owner 先确认用途、具体字段、实际存储位置、可访问人员及到期规则；缺少受限存储或受理权限时，不让玩家换一个公开渠道补发秘密。资料不用于营销、跨渠道身份拼接、建立个人档案或模型训练。新增用途必须先回到其产品和数据主责，不能从“用于支持”推导任意复用。

| 材料及控制者 | 本设计的保留/处置规则 | 必须继续说明的限制 |
| --- | --- | --- |
| 玩家尚未发送的草稿、本人设备上的反馈包 | 提交前删去不必要内容；本地删除和备份由设备持有人按实际权限决定 | 项目没有因此取得访问或删除其设备的权力；浏览器草稿保留仍无保证 |
| 项目为一次复现/分流临时另存的必要片段或工作副本 | 目的完成即清理；未完成时自接收起最多保留 7 个自然日，确需延长则按下述例外复核 | 这是人工政策目标，非当前自动清理能力；不适用于原专业验收证据或不可控副本 |
| 项目另存的、含必要关联信息的处置/复核摘要 | 只保留规则、事实摘要、动作、结果和原记录定位；事项终结后最多保留 90 个自然日，随后移除不再必要的个人关联，仅保留经核对不再关联个人的改进结论 | 原 Issue/PR 的公开历史不由本行自动删除；仍能间接关联个人的摘要继续受本规则约束，不将原秘密抄进摘要以延长保存 |
| 未结争议、尚需复现或确有适用保留依据的最小材料 | 指定实际保管 owner、保留理由与依据、最小字段、访问者、下一复核日期；每次人工复核起最多延长 30 个自然日，复核后删除、缩减或给出仍必要的依据 | 需要持续保留的依据可以跨多个窗口，但不能用“以后也许有用”自动续期；复核记录自身不得含秘密 |
| GitHub、已授权第三方渠道上的原记录和编辑历史 | 当前可做动作按平台能力与权限核对，采用必要的正文/历史处置并回读实际结果 | 关单、折叠、撤回当前展示不等于历史删除；外部副本不在项目控制内 |
| DistFS root/event、复制记录、世界 receipt、身份/Agent 私密状态及既有专业证据 | 按各自原 authority 的保留、可见性和恢复合同处理；支持记录只保留安全定位 | 不套用 7/30/90 日运营副本目标，不因用户请求或服务结束改写世界义务、他方权限或权威历史 |

7 日限制减少临时复现材料在工作目录长期累积；90 日给已结事项留出有限的再核查窗口；30 日要求仍在保留的例外持续说明必要性。这些是本项目运营副本的设计取舍，不是法定统一期限或当前执行事实。已存在更短的工程临时输出清理义务仍按[开发流程的信息归属与清理](../../engineering/workflow/source-of-truth.md#information-record-placement)执行；原始验收、冻结证据或其他明确保留合同不能被本节擅自缩短。

期限以记录的接收/结案时间起算，到第 7/90 个自然日的同一时刻为上限；例外从实际复核时刻起算，均记录时区和绝对到期时间。上限不是必须存满的时长，有效数据请求仍按下述流程处理。例外应在原期限前复核；到期尚未处置则标为逾期，由实际保管 owner 优先处理并暂停新增同类临时副本，不能回填“自动删除”。需要删除时同时核对项目控制的工作副本、分享导出与备份；备份若有独立有效保留规则，写明其范围与复核点，不能给主副本删除盖上“全部已删”。新增管理人员、存储迁移、用途变化或争议终结均触发重新核对。

#### 数据请求流程

1. **定位对象，少取身份。** 接收者在已有可用入口记录请求类型、材料安全定位及联系所需的最小信息。能通过原提交账号或既有认证通道确认关联时直接使用；公开公钥、反馈 ID、姓名截图或知道一段原文均不自动证明有权操作。不得要求把证件、会话令牌、私钥或他人原文发进公开 Issue。
2. **拆分对象和权限。** 将请求分到本地文件、项目可控运营副本、第三方原记录、公开复制历史、账号/私密状态或世界事实。请求更正一个支持摘要不等于授权删除他人信息、撤销合同或注销世界身份。关联或权限不能确认时，保持待核，仍可按 §5.2 处置可核实的公开敏感曝光。
3. **核对保留例外。** 实际数据 owner 检查对象是否仍用于未结复现、复核或存在具体适用的保留依据；有则说明可公开的理由、最小保留范围和复核日期。公开告知不泄露他方材料，不能为答复请求扩散原秘密。
4. **按控制范围执行。** 可控制且无保留依据的副本清理或更正后核对实际存储与副本；第三方动作先确认能力和权限，再记录已提交请求或已确认结果。无已验证删除能力时说明限制和下一主责，不把能力未知改成已完成。
5. **分别回告结果。** 逐项使用“已更正/已删除的具体副本”“仅当前展示已撤回/折叠”“第三方待处理”“当前无法控制的历史/副本”“依据明确的保留例外”或“待核查”。保留脱敏的处置理由与复核入口；不是一律回复“数据已删除”。

GitHub 评论的敏感内容须分别核对当前正文与编辑历史；当前文件删除也不清除 Git 历史、克隆和其他引用。仓库历史的清理由实际仓库/安全 owner 另行核对授权、影响与恢复，不在普通支持操作中执行重写，见 [GitHub 敏感资料移除说明](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/removing-sensitive-data-from-a-repository)。凭据暴露时先由持有人通过原账户/提供者流程撤销或轮换，删除文字不能证明凭据已失效。

收到公开世界历史或私密 Agent 数据的删除请求时，分别引用[会话和世界结果连续性](../../product/player-entry-distribution/prd.md#player-session-authorization-continuity)与[认知数据权威边界](../../world-runtime/runtime/agent-cognition-lifecycle.design.md#1-boundary-map)，不能让社区管理员获得新的读写权限。请求、转交和已执行各自保留真实状态，没有实际私密通道时不索取详细材料。

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

玩家已有身份、权限、已确认资产和历史、仍有效的合同义务不由轮次关闭自动撤销。可提供的备份、读取、等待、独立路径或安全停止仅引用当前专业合同；不承诺全产品导出、服务商迁移、恢复全部资产或商业退款。持续服务按下节逐项核对，不能直接套用“本轮结束”代替服务结束设计。

<a id="support-service-exit"></a>
### 6.3 具体持续服务的退出

Trace refs: O7-REQ-c95314c6-feb1-40cd-8c0f-499830f09c3f、O7-REQ-32e47f2a-2da3-4ca6-aaf0-ef522be4b898。分别承接[可选服务退出](../../product/player-entry-distribution/free-entry-world-progression-and-recognition.prd.md#req-entry-service-exit-001)和[服务状态与世界权力分离](../../product/player-entry-distribution/free-entry-world-progression-and-recognition.prd.md#req-entry-free-001)。下表规定退出内容及责任交接，不是当前服务清单或已经提供服务的证明。

提供一项持续服务前，提供者必须能说明服务对象、实际支持者、运行/支持窗口、停止新增受理的条件、通知方式、退出时可读取或取得的数据、未结义务、资料政策和退出后可用的联系/复核方式。普通计划退出的通知期与读取/交接期采用该服务实际公布并能履行的条款；尚未选定这些内容时不能对外承诺该持续服务已经就绪。本文不规定所有服务共用的天数，也不替付款、退款或地区专业主责决定条款。

| 退出阶段 | 必须形成的事实与动作 | 转换判据和失败后的下一步 |
| --- | --- | --- |
| 确认对象与准备 | producer 与服务提供者确定缩减/结束对象、原因、受影响候选/用户、仍可用范围、通知与履行窗口；技术/数据 owner 核对真实退出能力 | 对象、责任或关键能力未知时，停止扩大新承诺并保留待核；不能先删入口再推测影响 |
| 通知并停止新增承诺 | 用已授权且实际可用的通知渠道发布生效时间、停止的新请求/续约/支持范围、玩家真实下一步；同窗纠正仍活跃的相关页面 | 发布后回读并记录真实定位；渠道投递失败不记已通知所有人，补充可达方式并评估原窗口是否仍可履行 |
| 保留约定的读取/交接与处理未结事项 | 数据 owner 提供已验证的读取或交接方法；执行 owner 核每个已接受但未知/待结请求、未交付服务承诺与受理中的复核 | 只对实际取得且核验过的结果确认完成；能力失败明确阻塞、责任与补救安排，不用本地备份或工单关闭代替交付 |
| 结束受控服务 | 技术 owner 按对应合同停止该服务的新增执行、回收其控制的凭据/访问并核真实结果；数据 owner 按适用保留规则处置副本 | 必需的交接与未结责任已履行，或有明确有效的继续承接；无替代承接的关键义务保持未完成，不因页面关闭消失 |
| 关闭后说明与复核 | 保留必要的关闭说明、实际仍可用入口、未结事项 owner、资料处置结果与下一复核点；更新已过期的邀请/入口说明 | 关闭公告只描述实际停止范围，后续纠正仍可追溯；没有可用联系入口须明示并由提供者处置，不填虚构地址 |

紧急停止可以因已确认的安全或运行风险先暂停受影响的新操作；说明已知影响、采取的动作、未确认范围和下一次可信信息更新。正常履行窗口无法保持时，提供者说明原因与拟补救安排，由各原主责处理未结义务；不能把紧急停止解释为已经取消所有旧请求或放弃保留/交接责任。风险解除后是恢复、缩小范围还是继续退出，仍须产品与技术/QA 的对应事实支持。

| 必须单独核对的对象 | 关闭服务时的边界 |
| --- | --- |
| 已接受但结果未知的世界请求 | 保留原请求定位，按当前读取权限核结果；不因超时、客服结案或停止提供者而自动重提、取消或退还投入 |
| 独立授权的 Agent、身份与控制权 | 只按原授权期限和撤销合同处置；关闭 Viewer、会话或托管提供者不自动撤销另一有效授权，也不静默延长授权 |
| 账号、钥匙、私密认知与玩家可取得数据 | 取得范围和方法由身份/存储/认知 authority 决定；支持人员不索取私钥，不将运营摘要充当可恢复存档 |
| 世界公开事实与第三方节点 | 保持原权威、权限和副本限制；单个提供者不能宣布 P2P 世界全部停止、删除所有历史或撤回所有副本 |
| hosting/storage/support 的未履行承诺 | 服务提供者列明已交付、待履行、争议和相应责任；实际存在的账务/退款按已采用合同处理，不由世界资产补发替代 |
| 本地包、运营副本和专业原始证据 | 分别按 §5 和原证据合同处理；退出不一律删数据，不以“已备份”代替原来约定的玩家取得能力 |

尤其不能把[身份完整快照导出](../../testing/manual/identity-snapshot-export.manual.md)用于宣传玩家自助迁移：它服务 DevLocal issuer 的身份政策前态核验，不导出凭据、session、token 或私钥，不产生新的 authority。退出中真实可用的玩家导出、替代入口或迁移能力必须从对应服务合同及运行验证取得。缺少必要能力时如实记录阻塞和提供者责任，不让玩家反复尝试不存在的工具。

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

### 社区规则与处置说明

> 欢迎对游戏规则、体验和项目做具体批评，也欢迎报告竞争中发生的损失。请讨论事实，不公开他人的敏感资料或凭据，不实施威胁、持续定向骚扰、冒充、钓鱼或阻碍讨论的重复灌水。规则内游戏结果与社区行为会分别核查。
>
> 本规则用于〈实际由项目管理的表面〉。若需要采取内容整理或参与限制，我们会按实际证据与权限说明原因、范围、已执行的结果及〈真实可用的复核方式〉；紧急防扩散可能先行处理。平台内容处置不会直接改变游戏内资产、身份、控制权或合同结果。

### 支持资料与数据请求说明

> 请只提供复现或核查所需的安全摘要，不上传凭据、私钥、证件、完整配置或他人的私密资料。当前〈实际提交渠道〉的公开与保存范围为〈按 §5.1 核实的路径〉；项目会优先使用原记录，不另存不必要的原件。
>
> 项目确需另存的临时工作副本在用途完成后清理，通常不超过接收后 7 个自然日；含必要关联的另存处置摘要在结案后不超过 90 个自然日。确需继续保留的最小材料有具体理由和不超过 30 日的下一复核点。这些规则不删除平台原记录、已复制历史或世界事实。你可以通过〈已核实可用的请求方式〉提出更正或处置请求；我们会分别说明实际可控制的对象、已执行动作和仍存在的限制，不承诺全网删除。

### 持续服务退出说明

> 〈具体提供者及 hosting/storage/support 服务〉将于〈已确定时间〉〈缩减/结束〉。从〈停止新增时间〉起，不再接受〈明确的新承诺〉；〈已接受事项及仍可用范围〉按〈实际承接记录/条款〉继续处理。你在〈真实读取或交接窗口〉可以〈已验证可做的动作〉；尚不能提供的能力是〈真实缺口、负责者与下一步〉。
>
> 此次调整只影响上述服务。它不会自动取消已接受的世界动作、撤销其他有效授权、删除世界历史或保证第三方副本消失。结果未知的请求请通过当前有权使用的入口核对；未结服务义务及资料处置为〈逐项真实状态〉。后续说明和复核通过〈实际可用入口；没有时明确说明及责任安排〉。紧急停止时另说明已确认原因、受影响范围和下一次可信信息更新。

<a id="support-release-materials"></a>
## 8. 发行材料与适用范围

Trace refs: O7-REQ-a0b94d0f-456c-4702-8933-2232fdf0a544、O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b。分别承接[材料对应真实渠道和受众](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-release-materials-001)及[同候选跨域输入](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-candidate-001)的内容子范围。制作人先写清实际要提供给谁、通过什么入口、使用哪个候选，再选择材料；不能先复制商店模板后倒推项目已支持这些范围。

### 8.1 现有表面与发行选择

| 表面或选择 | 已核对的来源与本设计输入 | 采用前还须确认的内容 |
| --- | --- | --- |
| 开源仓库、README 与 CN/EN 官网 | 根 README 给出受限技术预览、源码与正式 Skill 入口；[官网设计](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-download)要求下载来源、资产与 checksum 同候选 | 当前实际页面、推荐资产、平台/语言及证据窗口；仓库可访问与 MIT 许可不证明品牌、所有素材或所有地区发行均已获许可 |
| 本地 Native/Web 预览与受控邀请 | [访问模式](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#2-玩家访问模式)及[受控预览](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#61-受控-limited-preview)定义已声明入口与有限范围 | 本轮候选、primary mode、支持环境、实际受众与开放授权；历史 invite pack 不自动恢复为新轮次 |
| Moltbook、小红书等已授权传播渠道 | 各自现有 runbook 规定发布、巡检和回流 | 当前账号权限、授权、可见性和可公开材料；帖子、账号或认证不成为游戏下载、账户互通或合作证明 |
| 新商店、新地区、特定年龄受众或收费服务 | 不能从现有公开页面推导已经选择；须有明确产品决定 | 渠道/地区/受众范围、运营主体、适用专业材料与真实平台能力；未选定保持未选定，不填“已支持” |

本表没有替产品选择新的市场。已经公开的表面仍须核对其真实触达、受众和数据用途；“没有选择商店”不能用来忽略现有渠道的适用义务。具体地区或受众不明确时，交 producer 与相应专业主责核实，受影响发布结论保持待核。

数据告知不得只看反馈表单。固定输入的 [官网样式](../../../site/assets/styles.css)和[路演样式](../../../site/deck/deck.css)还声明了 Google Fonts 的远程字体请求，属于网站发布时须核对的外部资源接触；这不是 Viewer 采用本地系统字体栈的同一条路径。此源码事实不证明实际部署的请求结果或第三方保留政策，应在该发布范围的资源/数据清单分别核实，不宣传“所有展示完全本地、无第三方请求”。

### 8.2 最小材料包及缺项处理

在当前候选的原 PR/发行记录维护材料定位、版本/语言、适用范围、实际核定者与未结项；长期正文只维护以下内容合同，不另建永久发行账本。

| 材料 | 需要交付给消费者的内容 | 缺项或变化后的判定 |
| --- | --- | --- |
| 候选与安装说明 | 版本、primary mode、实际支持平台/语言、入口、获取来源、完整性核对、成本前置和真实退出/恢复方法；版本说明列变化和已知限制 | 缺同候选资产、校验或入口证据，停止对应推荐/升级结论；不是从另一平台借一个通过结果 |
| 品牌、截图和宣传素材 | 名称、署名与来源/许可；概念图、原型和实机证据标记各自身份；翻译保持同一事实范围 | 权利或来源不能核定的素材不进入待发布包；替换素材需重核受影响 claim，不能仅删署名 |
| 内容与交互事实清单 | 记录候选实际包含的冲突/暴力表现、玩家或 Agent 生成内容、公开互动、交易/付费及外部链接；每项有内容源或消费者依据 | “未发现”只覆盖已审读范围；玩法设计目标、功能未实现和实际可进入内容分别写，不能把整个游戏标成无风险内容 |
| 适用内容分级与受众材料 | 按实际渠道、地区和年龄范围，由相应专业主责判断需何种问卷、分级说明/标识、受众限制及平台回复 | 未核定不填分级值；同一候选在另一渠道的结果不自动通用，内容/互动/支付变化时重核 |
| 使用与服务条款 | 区分开源软件许可、具体在线/托管/支持服务承诺、可用范围、费用如适用、退出和争议/复核方式；玩家可在接受前取得适用版本 | 没有已采用服务就不造购买/退款条款；确有服务但缺必要条款时，该服务就绪保持阻塞，不能用 MIT 许可替代 |
| 数据告知与请求方式 | 实际数据类别、用途、公开/访问者、控制者、保留规则、第三方/复制限制、真实请求入口；§5 和支持文案只覆盖其准确数据范围 | 新用途、字段、外部接收方或私密能力变化须先更新原数据合同；没有完整服务数据盘点时，不发布“全产品隐私政策已齐备” |
| 支持、社区、事故与退出说明 | 适用规则、真实接收/复核方式、运行/支持窗口、未承诺的时限和退出下一步；引用 §4–§6 的已采用内容 | 没有人员、权限或可用入口时收窄/暂停相关新邀请与持续服务承诺，不能承诺逐条人工答复 |
| 同窗公开稿与发布记录 | 事实、目标、假设、证据边界分离；CN/EN 同事实，根 README 与受影响活跃表面一致 | 审核未通过保留草稿；实际渠道/时间/URL 或 ID 缺失时不记已发布，沿 §6 纠正漂移 |

对每项材料只接受三类有依据的判断：**适用且已准备**（仍区分内容、专业审核与实际采用）、**有明确理由的不适用**（记录具体范围、决定者、依据与重核触发条件）、**待核/缺项**。未选定渠道、信息未知、没有实现或尚未制作不是自动不适用。受众/地区、收费或互动方式、实际数据、素材权利、平台规则或候选内容变化后，只重核受影响材料和组合；某材料齐备不代签整体发行。

内容适用性由本文规定，实际法律/分级结论与平台审核属于相应专业主责。普通静态审校不能给候选背书；它应能指出真实缺口、阻止跨渠道代签，并让准备者知道须补哪份材料、由谁核定。

<a id="support-delivery"></a>
## 9. 制作、消费与持续成本

本文 §7 是可编辑中文文案源，版本随 Git 提交定位；手册提供实填、检查、发布与交接方法。源文案由项目原有产品和专业合同派生，不引入第三方素材。无需独立导出文件；实际发布文字、渠道位置与使用的文案版本留在原 PR/Task 或发布记录。

消费者是 README/官网帮助说明的维护者、Launcher 内容维护者、LiveOps 回帖与公告执行者、实际渠道管理者和具体服务提供者。当前源码入口分别为 `site/index.html`、`site/en/index.html`、`crates/oasis7_client_launcher/src/feedback_window.rs` 与 `feedback_window_web.rs`；本文未改动这些文件或平台设置，不能宣称玩家已经看到新说明。产品目标、数据路径或入口能力变化时，先更新原 authority，再修本文及手册，最后在获准的实际消费面同步并复核。

首次采用时制作一组覆盖入口结果、敏感误传和暂停/恢复/结束的代表性文案，按手册桌面演练；每次真实复用只复核变化的事实、入口和公开范围。CN/EN 配对公开面需逐项核对，不能只更新中文。自动 UI、传输或网络验证沿原专业方法，不以文字审校替代。

<a id="support-capacity-cost"></a>
### 9.1 运营容量、持续成本与调整

Trace refs: O7-REQ-cd283368-58dd-4538-93a8-7dae3cb2de79、O7-REQ-c95314c6-feb1-40cd-8c0f-499830f09c3f。分别承接[真实社区处置与复核](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-community-001)和[服务退出的支持安排](../../product/player-entry-distribution/free-entry-world-progression-and-recognition.prd.md#req-entry-service-exit-001)。本节只核运营投入，内容制作、美术或软件开发预算仍归其制作 authority。

使用实际安排的同一检查窗口，分别统计新的有效信号、重复记录、当前未结项及等待时间、已完成的实际处置和复核、事故/误传、平台不可用、文案纠正及参与者重复误解。按候选、入口、语言和原反馈分类汇总；不记录为追踪个人而新增的身份画像。帖文数、互动量、关闭数和零投诉不能单独表示问题已解决、满意或发行就绪。

| 输入/计算 | 使用方式与限制 |
| --- | --- |
| 可用人时 | 按实际可执行人员和角色记录窗口内可分配小时；同一人的重复角色不能重复计时，已有开发/值守占用先扣除 |
| 处置需求 | 每类信号的实际数量 × 已观测单次投入，加固定巡检/文案同步、未结复核、退出交接与本窗口安排的事故余量；暂无测量时给依据明确的估算区间并标未验证 |
| 角色瓶颈 | 分别比较 LiveOps、QA、技术/平台 owner、无利益冲突复核者的需求与可用窗口；总工时有余不代表缺失角色可被替代 |
| 未结量 | 期末未结 = 期初未结 + 本窗新增有效事项 + 已确认转入 − 实际解决 − 已确认转出；新增事项不包含内部转入，转出须由接收方确认承接，未确认仍留原 owner。父范围汇总抵消内部转入/转出，交接不减少总未结量 |
| 持续成本 | 已投入角色小时 × 已核实的对应成本单价，加本窗口实际可归因的存储/通知/平台服务费用；未核单价或费用保持未知，不按零计算 |
| 质量信号 | 同类误解复发、决定被复核改变、误标修复、数据请求逾期、缺失 owner 与事故继续暴露；逐项关联可改进的内容/入口，不只优化处理数量 |

producer 据此决定维持范围、减少新邀请/新渠道、调整实际责任或暂停新增收集。先处理仍在扩散的敏感暴露、已确认入口阻断和到期处置，再安排非紧急互动与内容改进；同级事项结合影响范围和等待时间，说明调整依据。暂停新收集不抹去已接受义务，未结项保留 owner 与下一步；复核者不可用时明确待复核，不以原处置者再次点击通过代替。

实际人时、费用、窗口和决定留在既有 PR/Task 或运营记录。没有执行者或可信窗口时不新增邀请或承诺固定回应频率；历史首轮信号目标、巡检节奏及下文的计算案例都不成为当前客服 SLA。

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
| [REQ-ENTRY-RELEASE-MATERIALS-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-release-materials-001)；Trace refs: O7-REQ-a0b94d0f-456c-4702-8933-2232fdf0a544 | [发行材料](#support-release-materials) | 以真实渠道/受众/地区/内容/数据判定适用材料，缺项不升级 | [发行材料核对](player-support-and-service-lifecycle.manual.md#verify-release-materials)：RM01–RM05 固定候选的渠道扩张、素材缺权、数据变化及分级未知桌面案例 | 原 PR 中逐项适用判断、材料缺口和发布范围判定 | 不证明实际平台审核、地区法律适用、分级、上线或完整发行 |
| [REQ-ENTRY-CANDIDATE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-candidate-001)；Trace refs: O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b | [发行材料](#support-release-materials) | 选定材料与模式/入口对应同候选；跨渠道和跨版本不得借证 | [发行材料核对](player-support-and-service-lifecycle.manual.md#verify-release-materials)：RM01/RM05 比较同候选与混版反例 | 原 PR 的来源对应、错误输入拒绝与未覆盖范围 | 仅内容组合，不代签 Runtime/P2P、经济、QA 或统一 gate |
| [REQ-ENTRY-COMMUNITY-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-community-001)；Trace refs: O7-REQ-cd283368-58dd-4538-93a8-7dae3cb2de79 | [社区规则](#support-community-rules) | 内容处置有行为依据、实际权限、最小范围和可达复核，世界权威分离 | [社区处置核对](player-support-and-service-lifecycle.manual.md#verify-community-disposition)：CM01–CM07 的正常批评、骚扰、敏感曝光、证据不足、冲突和权限失败 | 原 PR 的规则/动作/理由/下一步对照与负例判定 | 不证明平台已配置、人员可用或真实处置完成 |
| [REQ-ENTRY-SUPPORT-DATA-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-support-data-001)；Trace refs: O7-REQ-96d24911-2209-4606-999e-545e4cd10fe9 | [保留与数据请求](#support-retention-requests) | 不同控制者/副本适用不同保留规则，例外有限复核，实际删除结果分开告知 | [保留与请求核对](player-support-and-service-lifecycle.manual.md#verify-retention-requests)：DR01–DR07 固定日期的正常到期、争议、第三方/历史、身份不足和逾期案例 | 原 PR 的对象/时间/权限/结果分类及改进项 | 不证明真实资料清理、自动能力、私密入口或法定期限成立 |
| [REQ-ENTRY-SERVICE-EXIT-001](../../product/player-entry-distribution/free-entry-world-progression-and-recognition.prd.md#req-entry-service-exit-001)；Trace refs: O7-REQ-c95314c6-feb1-40cd-8c0f-499830f09c3f | [持续服务退出](#support-service-exit) | 普通/紧急退出均逐项承接真实窗口、数据取得和未结义务 | [退出演练](player-support-and-service-lifecycle.manual.md#verify-service-exit)：EX01–EX06 的正常、紧急、通知失败、读取能力缺失和未结责任 | 原 PR 的逐阶段判定、玩家下一步和未结交接 | 不证明存在已售服务、真实停服/迁移/退款或所有提供者结束 |
| [REQ-ENTRY-FREE-001](../../product/player-entry-distribution/free-entry-world-progression-and-recognition.prd.md#req-entry-free-001)；Trace refs: O7-REQ-32e47f2a-2da3-4ca6-aaf0-ef522be4b898 | [持续服务退出](#support-service-exit) | 提供者退出不能取消原世界请求、扩权或抹历史 | [退出演练](player-support-and-service-lifecycle.manual.md#verify-service-exit)：EX02/EX04/EX06 核原请求、身份导出误用和独立授权 | 原 PR 的服务/世界对象分离与拒绝结论 | 不证明身份/Agent/世界恢复实现 |
| [REQ-ENTRY-COMMUNITY-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-community-001)；Trace refs: O7-REQ-cd283368-58dd-4538-93a8-7dae3cb2de79 | [运营容量与成本](#support-capacity-cost) | 真实受理/复核能力不足时如实保留待核并收窄新收集 | [容量核对](player-support-and-service-lifecycle.manual.md#verify-support-capacity)：CP01/CP02 的角色瓶颈、交接与父范围未结量 | 原 PR 的手算、调度理由和真实缺口 | 不证明现有人员、产能、回复 SLA 或满意度 |
| [REQ-ENTRY-SERVICE-EXIT-001](../../product/player-entry-distribution/free-entry-world-progression-and-recognition.prd.md#req-entry-service-exit-001)；Trace refs: O7-REQ-c95314c6-feb1-40cd-8c0f-499830f09c3f | [运营容量与成本](#support-capacity-cost) | 退出支持投入与实际窗口对应，未知成本不记零，未结责任不因预算不足消失 | [容量核对](player-support-and-service-lifecycle.manual.md#verify-support-capacity)：CP01/CP03 的退出窗口与费用缺项 | 原 PR 的容量/成本判定与承接安排 | 不证明商业定价、实际人时费用或服务已可交付 |

## 12. 决策、风险与未覆盖范围

采用已有入口、最小公开信息与原记录处置；对项目可控运营副本设有限保留，对社区行为采用最小适当动作和复核，对服务退出先保留未结责任与真实下一步。这些规则能用于内容制作和人工推演，但其发布采用、权限、技术能力及实际人员仍须逐表面核验。原公开账本不适合敏感材料，文字不能替代提交前告知或技术保护。

| 尚需决定或核实的范围 | 当前可执行边界 | 决策主责与触发条件 |
| --- | --- | --- |
| 私密举报、敏感材料受理与访问人员 | 公共入口仅收安全摘要；没有验证通过的私密入口就不接收详细秘密 | producer + 安全/实际平台 owner；开展私密收集之前 |
| 社区规则的实际采用、处置权限和可达复核 | §4.5 已给行为/处置规则；各渠道仍须核对真实管理者和无利益冲突的复核能力，世界处罚仍外部 | producer + 实际平台/安全 owner；公开规则采用和每次操作前 |
| 项目现有资料库存、实际保留/删除与第三方副本 | §5.3 的有限目标只适用于受控运营副本；尚需核对当前实际对象、权限、例外和到期动作，原专业数据合同不被覆盖 | 数据/安全/存储 owner；采用政策、改变用途或收到请求时 |
| 具体持续服务条款、读取/交接/退款能力与退出窗口 | §6.3 定义退出准备和人工交接；没有具体已采用合同与能力证据时不能声明正式服务退出就绪 | producer + 提供者/身份/存储/Runtime/账务 owner；提供服务前及缩减/结束决定时 |
| 具体渠道、地区、受众、分级、协议和完整服务隐私材料 | §8 已给现有表面、材料与缺项判定；真实选定范围和适用专业结论仍须核定，不声明全球合法性或商店已获准 | producer 与相应专业主责；选择或改变实际发行范围/内容/数据时 |
| 持续支持的实际人员、成本及玩家理解效果 | §9.1 提供可核算方法；实际窗口和单价未知不记零，文字推演不证明人力、实测产能或真实满意度 | LiveOps / QA / producer；新邀请、服务承诺、容量不足和退出交接时 |

本文件交付规则、内容源和可执行的检查方法；未取得的实际采用、专业核定与运行证据保持可见。当前必要内容一旦与实现、入口或证据冲突，先收窄该内容并修正原主责，不以未决参数为由继续传播错误保证。

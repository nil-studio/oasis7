# 玩家接入与发行 PRD

## 文档身份

- 产品模块：玩家接入与发行
- 产品模块 slug：`player-entry-distribution`
- 产品层唯一 PRD：`doc/product/player-entry-distribution/prd.md`
- 产品模块总入口：`doc/product/README.md`
- Product PRD-ID：`PRD-PRODUCT-004`
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：`2026-08-06`
- 后继文档：`无`
- 下层专业域：[`README.md`](../../../README.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/game/prd.md`](../../game/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)

本文只组合玩家从了解产品到进入、安装、验证受支持技术预览的路径。根 `README.md` 是公开当前状态与 claim envelope 的唯一权威；Viewer、Launcher、发行资产与访问模式的实现合同由 `world-simulator` 专业域拥有。

账号接入组合依赖[免邮箱注册与多凭证账号合同](../../p2p/blockchain/hosted-public-join-key-based-onboarding.prd.md)：推荐通行密钥，邮箱保持可选；账号创建、世界准入与运行时绑定分别验收。实现候选与虚拟认证器证据不自动升级公开就绪结论。

### 活跃产品专题

- [`访问模式与发行就绪`](access-modes-and-release-readiness.prd.md)：`viewer` / `pure_api` 模式边界、能力等价、失败恢复、统一候选门禁与公开 claim 升阶。
- [`本地启动会话连续性与恢复`](local-launch-session-continuity-and-recovery.prd.md)：受支持 Launcher 路径中本地会话的真实状态、停止、恢复与配置边界；不把本地运行事实升级为模式或发行 claim。
- [`发行沟通与公开口径`](release-communications-and-public-claims.prd.md)：内部证据到外部 claim 的状态隔离、审核、发布、纠正与撤回合同。
- [`官网发现、玩法展示与持续关注`](public-site-discovery-and-showcase.prd.md)：陌生访客理解、具体决策展示、静态观测与进展、预览分流及小说导读；配对产品体验与 site 系统设计，不代表页面已实现或公开状态升级。
- [`参与和认可边界`](participation-and-recognition-boundaries.prd.md)：有限预览中的可审核贡献、非自动权益和治理执行边界。
- [`免费进入、世界内成长与有界认可`](free-entry-world-progression-and-recognition.prd.md)：免费基础进入、非权力型可选服务，以及对世界成长、情境认可和区域机会主责专题的入口/claim 组合边界；世界内规范性语义由世界规则与玩法专题承接。

## 1. 产品承诺

玩家能从一个不夸大的公开入口理解 oasis7 当前是什么、哪些模式已有证据、如何进入或安装，以及如何验证自己连接的是声明的产品路径。

## 2. 范围与玩家边界

覆盖产品发现、公开说明、Web 访问、平台安装、Launcher 转移、账号/模式入口和首次验证。玩家可以了解当前阶段并选择受支持入口；不应被未发布功能、内部测试模式、过期下载或历史 go 证据诱导。

公开说明中的既有 HTML 路演 deck 由 [SC-11](#sc-entry-011)独立接收，来源为 [PRD-SITE-012](../../site/prd.md#prd-site-012)。这项接收不扩大官网发现专题的首期六项功能或其状态范围，不重做页面。

### 安装与发行体验边界

每个平台应提供一个推荐下载包和主 CTA；下载前应清楚显示支持边界、版本与 checksum，使玩家无需研究 shell、bundle 内容或多个技术资产的差异即可完成受支持的安装或启动。

当前升级只支持重新下载最新主包并手动覆盖安装/替换；用户须先备份 `config.toml`、`.oasis7_launcher_ux_state.json` 与 `output/chain-runtime/<node_id>/reward-runtime-execution-world/`。这不是应用内更新、自动迁移或跨目录状态保留承诺。Windows codesigning trust chain 与 macOS 签名/notarization 未完成前，不得把技术预览资产、单主 CTA 或手动安装路径表述为普通用户广泛发行已就绪。

<a id="download-integrity-and-upgrade"></a>
### 下载完整性失败边界

玩家可以在开始安装前核对推荐资产的版本、平台、发布来源与 checksum；若该核对缺失、无法完成或不匹配，产品只能明确停止该资产的安装/覆盖路径，并让玩家保留或重新取得与当前发布来源绑定的候选。玩家可以查看预期与实际核对结果、选择重新下载、改用同一候选中明确受支持的正确平台资产，或进入支持/恢复路径；不能把“文件已下载”“曾经可用的旧 checksum”“浏览器/操作系统未报警”或第三方镜像的自述当作验证通过，也不能被引导绕过核对继续覆盖既有状态。

完整性失败只裁定本地候选资产不可信，不自动撤销玩家 identity、session、Agent 授权、待决 intent 或已 committed receipt；同样，重新取得并通过本地核对的资产也不自动恢复这些权威能力。这样 world-first 的权威世界不由分发字节决定，persistent 的世界历史不会被本地安装诊断重写；emergence-first 的玩家、Agent 与组织行动不因下载渠道获得额外规则或经济权力；每次失败/恢复都应能以候选版本、平台、来源类别和结果追溯，而不要求公开本地路径、凭据或敏感诊断。未来新增平台、签名或其他校验机制可以扩展实现，但必须保留同一语义：未验证的资产不能被表达为受支持安装，验证通过也只恢复本地候选资格而不代签世界权威。

### 手动升级兼容与安全回退边界

新版本在首次不可逆处理旧配置、Launcher 状态或本地 runtime 数据前，必须形成绑定版本、平台、primary mode 与候选资产的兼容判定，并把结果表达为以下三类之一：

- **兼容并可继续**：新版本已经确认所需状态可读取或可安全迁移，玩家可以继续进入声明的模式；成功只覆盖本地升级结果，不自动证明远端世界状态、session authority 或待决行动已经恢复。
- **失败且确认未改写**：新版本在写入前停止，并能证明原状态未被改写；产品可以指引玩家退出新版本、恢复升级前备份并重新安装先前受支持资产。回退目标必须明确，不能让“下载旧包”代替版本、checksum 与状态来源核对。
- **未知或可能已改写**：无法证明写入边界，或已开始不可逆迁移；产品必须停止继续启动，保留备份与诊断材料，并提供重新安装当前版本、隔离新数据或进入支持/恢复流程的下一步。此时不得建议直接降级、反复覆盖或用旧程序打开可能已升级的状态。

本地软件回退不回滚权威世界、玩家身份、session authority、已 committed receipt 或已经被网络接受的行动；这些状态只能按各自专业域的恢复合同重新核验。备份也不是可移植存档或 authority 凭据。具体兼容版本、迁移格式、写入原子性、诊断字段和恢复命令由 Viewer/Launcher、runtime 与发行专业合同拥有，产品层只约束玩家可见结果、禁止误导的回退路径与跨域验收。

### 玩家访问模式与证据边界

- 正式玩家访问模式只有 `viewer` 与 `pure_api`：`viewer` 是 Web/UI surface，`pure_api` 是无 UI surface。兼容 alias、execution lane、provider、deployment 和 session context 都不会产生新的玩家访问模式。
- `hosted_public_join` 是 `viewer` 下的 deployment/session context，不是独立模式，也不表示更高的可用性或发布就绪。每份 claim 与 evidence 必须绑定一个 primary mode；`viewer` 与 `pure_api` 的证据不得互相代签。
- `pure_api` 只有在专业域规定的可玩性与 parity 前置条件成立且有对应证据时，才能支持正式可玩结论；observer 或 blocked 是该模式下的结果分类，不是第三种入口。具体 Viewer/provider 实现、玩法 parity、hosted session/custody 与测试证据分别由 [`doc/world-simulator/prd.md`](../../world-simulator/prd.md)、[`doc/game/prd.md`](../../game/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md) 与 [`doc/testing/prd.md`](../../testing/prd.md) 维护。

### Hosted 玩家进入与会话边界

- Hosted Web 保持 `viewer` 模式；共享 URL 只提供玩家加入或观察入口，不提供 operator/control 能力。guest observation 不等于可玩身份；进入可玩状态还需要有效的玩家 identity/session，并成功绑定到 runtime。
- 会话过期、撤销或服务重启后，产品必须明确引导玩家 reconnect、re-register 或 re-auth，不得静默恢复旧 authority。玩家不管理长期 signer material；登录恢复的是 identity/session，不是 private key，也不会仅凭登录解锁敏感操作。
- 当前状态是 limited playable technical preview，不代表 universal sharing、production custody 或 broad readiness。Hosted entry、session、authentication 与 signer custody 的专业合同由 [`doc/p2p/prd.md`](../../p2p/prd.md) 维护。

<a id="player-session-authorization-continuity"></a>

### 会话失效、待决 intent 与 Agent 授权连续性

会话过期、撤销、断开或重新认证只改变玩家当前进入与提交新动作的能力；它本身既不撤销、不确认，也不重放已由权威系统接受的 intent。客户端尚未收到 committed receipt，不等于权威系统确认请求尚未生效：超时、断连或回复丢失可能发生在实际提交之后。

| 请求结果的玩家语义 | 判定依据与恢复边界 |
| --- | --- |
| 已确认尚未生效的待决请求 | 必须有适用的权威状态证明，不能从缺少回执推导。尚未决定的请求按当前有效权限、资源、期限及规则重审；已固定执行决定的原请求按专业恢复合同续接，不因重新登录而重新决定或换成新动作。 |
| 提交结果未知、待核对 | 无法证明原请求是否提交时保留该状态和原请求身份。恢复读取资格后先核对原结果；超时、单端未找到、陈旧缓存、查询不可用或无读取权限都不能被解释成确定失败、已取消或可放心重新执行。 |
| 已确认提交及其结果 | 以绑定目标世界与原请求的可验证结果为准。重连、重复查询或专业合同允许的原请求幂等重试，只返回或续接原结果，不再次扣减资源、结算或授予权限。动作提交不自动等于生产、交付等后续业务完成。 |

已结算 receipt 不因会话失效消失，重新认证也不得静默重提旧 intent、复制资源 sink，或把旧会话、界面缓存和 Agent 计划当成新的提交授权。结果仍未知时，安全停止或重新定目标只约束后续决策，不代表已经取消原请求、返还其投入或消除其可能已提交的效果。

Agent 离线工作是否继续，只由其独立且仍有效的授权范围、到期条件和专业合同决定。玩家会话恢复不得延长、扩大或复活已失效的 Agent 授权；授权失效后，Agent 必须停止产生新的越权世界动作，或按专业合同把尚未生效的动作明确置为待决、暂停或拒绝，不能伪装成已完成。若 intent 已由权威系统接受，玩家登录中断也不能把它误报为已取消或从历史中隐藏。

`viewer` 与 `pure_api` 的正式 surface 至少要区分“会话阻塞”“已确认尚未生效的待决请求”“提交结果未知、待核对”“Agent 授权失效或暂停”和“已确认提交及其结果”。会话、读取资格和 Agent 授权是与请求结果正交的状态；登录失败或授权失效不能覆盖已知结果，也不能将未知结果改写为未生效。Viewer 必须以玩家可读形式呈现原请求、当前因果状态与安全下一步；无 UI 的 `pure_api` 必须返回语义等价、可由客户端消费的结构化状态与下一步类别，不要求内建玩家界面。

下一步可以是 reconnect/re-auth、按原请求核对、等待明确的服务恢复、查看 receipt、重新规划，或专业域明确允许的撤回/替代。查询和原请求幂等重试不得换成新的意图；另行发起新动作必须有新的显式授权，并保留与原请求的关系，不能作为恢复流程的隐含副作用。查询仍按当前读取权限与世界/主体绑定验证；重新认证或持有旧请求引用不自动授予读取他人私有结果的权限。

[Runtime 提交与恢复设计](../../world-runtime/design.md#runtime-authority-commit-recovery-target)和[P2P 原候选恢复设计](../../p2p/design.md#des-p2p-original-candidate-recovery-v1)拥有原请求查询、持久去重、既定执行续接与未知结果核对的技术合同。本节组合它们的玩家语义，不新增 wire 状态枚举、查询授权或第二套恢复协议；具体 session token、签名、Agent 授权字段、API schema、transport 和 UI 表达仍由对应专业域拥有。

## 3. 权威与冲突处理

| 产品层拥有 | 公开/专业域权威 |
| --- | --- |
| 发现、访问、安装、验证与发行路径的组合体验 | `README.md` 拥有当前公开状态与 claim；`doc/world-simulator/prd.md` 拥有 Viewer、Launcher、provider、资产与访问实现；`doc/world-runtime/prd.md` 拥有持久 runtime 状态兼容、写入原子性与恢复合同；`doc/game/prd.md` 拥有 pure API 玩法与 parity；`doc/p2p/prd.md` 拥有 hosted entry/session/authentication/signer custody；`doc/testing/prd.md` 拥有证据与门禁合同 |

Site 的静态发布、导航、维护权限与专业验收继续由 [Site PRD](../../site/prd.md#prd-site-012)拥有，现行系统 authority 是 [Site 根设计](../../site/design.md#site-roadshow-deck-receiver)；产品层只接收既有顺序化公开说明结果，不新增发布审批或改变 README 的公开状态权威。

本 PRD 不得独立宣布新版本、新渠道、新下载或发布就绪。公开 claim 变更必须先有专业验证与 QA 结论，再由 `liveops_community` 同步根 README/公开渠道。

## 4. 路线图

1. 唯一发现路径：根 README 说明当前阶段、受支持入口与公开边界。
2. 可验证进入：Web、Launcher 或平台资产带玩家到达声明的真实模式。
3. 发行一致性：版本、资产、说明、可用性与回退路径经同一 release gate 核验。

## 5. Done：成功标准与验收

- SC-1：根 README 只声明当前受证据支持的产品状态、访问方式与公开边界。
- SC-2：每个公开入口都能指向对应的模式、版本、平台和可重复验证路径。
- SC-3：发行资产、Launcher 转移与主 Web 入口不会把内部、回退或假模式宣称为真实发布体验。
- SC-4：公开 claim 变更可追踪到专业 PRD-ID、QA 结论与 LiveOps 同步 owner。
- SC-5：访问结论只绑定 `viewer` 或 `pure_api` 之一，execution/provider/deployment/session context 不被升格为新模式；两种模式的可玩性、parity、observer 或 blocked 结论均有各自证据且不得互相代签。
- SC-6：至少一条发行证据链以同一版本和 primary mode 贯通公开发现、正确的平台/模式选择、下载安装或 Web/pure API 进入、玩家核对版本/backend/mode，以及 unsupported、失败或手动升级时的真实恢复说明；仅对适用的 Viewer/Launcher 路径验证 Launcher 到达声明的真实后端。每个受支持的平台/入口组合分别验证，`viewer` 与 `pure_api` 仍不得互相代签。
- SC-7：产品样例证明免费客户端、账户和基础进入与可选的 hosting/storage/support 便利服务相分离，后者不授予世界权力；成长、认可和区域协作仍以世界内有代价、可审计的资产、行动和有界资格为基础，且长期目标不被误报为当前技术预览可用性或发行就绪。
- SC-8：每个支持手动覆盖升级的平台资产都在不可逆处理前给出兼容判定；兼容时可继续，只有能证明原状态未改写时才提供绑定明确旧版本与备份来源的回退，未知或可能已改写时停止并禁止盲目降级。本地回退不会被表达为权威世界、identity/session authority、committed receipt 或网络已接受行动的回退。
- SC-9：代表性 hosted 与恢复样例证明，会话失效不会把已接受的 intent 或已结算 receipt 静默撤销、隐藏、重放或二次结算；客户端缺少回执不能被当作未生效。玩家能区分已确认未生效、提交结果未知及已确认提交，并在当前读取资格有效后按原请求核对。重新认证不恢复旧 authority，也不扩展 Agent 委托；尚未决定的请求与既定执行的续接分别遵循专业合同。提交成功但回复丢失的样例必须查回原结果且只有一次效果，查询受阻时保持真实未知与安全下一步，见[组合验收](#player-session-unknown-outcome-acceptance)。
- SC-10：每个受支持平台至少有一条推荐资产的完整性失败样例：缺失、无法完成或不匹配的核对会在写入既有状态前停止安装/覆盖，并给出预期/实际结果和重新下载、正确平台资产或支持/恢复中的适用下一步；通过重新取得资产的本地核对不被表述为 identity/session/Agent authority 恢复、pending intent 结算或 world receipt。新增校验机制或平台必须保留这组语义边界。

- SC-11：公开站点保留可直接访问的 HTML 路演 deck，以固定顺序说明项目定位、玩法差异、当前阶段与后续路径；访客可从 docs hub 或首页的清晰入口进入并回到首页 / docs hub，持续辨明当前仅为 limited playable technical preview。该项独立接收既有 PRD-SITE-012 及其既有验收，不把首页改为演示稿，不增加新的公开能力；[唯一身份及专业验收](#sc-entry-011)。

<a id="player-session-unknown-outcome-acceptance"></a>

### 会话恢复与未知提交的组合验收

本节细化 SC-9，复用上文系统设计与现有候选证据，不新建测试平台或并行任务台账。每个实际声明支持的 primary mode 分别成案；同一案例绑定同一候选、世界、合法主体和原请求，不能拼接不同任务或模式的局部成功。以下为验收设计，不是已执行结果。

| 样例 | 操作与故障窗口 | 必须观察到的结果 |
| --- | --- | --- |
| SC-9-U1：提交后回复丢失 | 对合法且有资源后果的原请求，在权威提交后阻断返回，随后让会话失效；重新认证后按原请求查询，并重复查询或执行专业合同允许的同身份幂等重试。 | 客户端先保持结果未知，查明后显示原世界结果及其回执关联；资源效果、里程碑及适用外部副作用不重复，不能生成第二个意图来消除未知。 |
| SC-9-U2：已知无效果与既定续接 | 分别准备权威确认尚未决定的待决请求，以及已有固定执行决定但尚未发布的原请求；恢复前改变适用权限或资源条件。 | 前者按当前条件重审且无旧权限透传；后者核对并续办原持久决定，不重新 admission、重选结果或重复执行已保存的完整 candidate。两者都不能仅凭客户端缺少 receipt 判定阶段。 |
| SC-9-U3：原结果暂时不可查询 | 在 U1 恢复查询时分别注入超时、服务不可用、单端未找到或无当前读取资格。 | 如实呈现服务/会话/读取阻塞，原请求结果保持未知；给出重新认证、原请求复核、等待服务恢复或安全停止中的适用下一步，不将错误、空列表或旧缓存变成未提交证明。 |
| SC-9-U4：Agent 授权与会话分离 | 在提交及重新登录前后使 Agent 委托到期或撤销；分别核对原已提交结果与尚未决定的新动作。 | 已合法提交历史保留；在当前读取权限允许时仍能查明原结果，否则如实限制读取。重新登录不复活委托，不授权新动作，也不把授权错误当作原请求取消。 |
| SC-9-U5：错误归属或结果绑定 | 用其他世界、其他主体或不匹配原请求的结果替代恢复材料。 | 不采纳为原请求结果，不泄露无权读取的私有内容；保持原请求的已知事实与待核对边界，不转投其他世界或借新身份重放。 |

QA 关联会话变化、原请求、实际查询返回、权威结果及其资源变化；Viewer 另保留玩家可见状态与恢复动作，pure API 保留结构化同义结果。只证明提交受理或排程的回执不能被记为铁锭完成；声称首产物完成时仍须按[首局合同](../world-rules-core-gameplay/first-session-and-continuation.prd.md)核对匹配的正量 output ledger 与持久里程碑。

候选记录区分 fixture、真实 Runtime/入口和真人观察，标明实际执行、未执行与不适用的范围；受控故障样例不代签独立故障域容灾或完整邀测放行。公开材料只保留脱敏关联与必要结果，不包含会话令牌、签名密钥或私有请求正文。具体故障注入、协议字段和当前结果仍由专业测试及 PR 证据承接。

### 5.1 验收追踪

| 成功标准 | 专业 owner | 专业域 PRD-ID | 权威文档 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- | --- |
| SC-1 | liveops_community | PRD-WORLD_SIMULATOR-042/043 | `README.md`; `doc/world-simulator/prd.md` | 公开 claim 与当前发布证据对齐审计 | test_tier_required |
| SC-2 | viewer_engineer | PRD-WORLD_SIMULATOR-020-031 | `doc/world-simulator/prd.md` | Web、Launcher、平台入口到声明模式的 smoke | test_tier_required |
| SC-3 | viewer_engineer | PRD-WORLD_SIMULATOR-039/041/046 | `doc/world-simulator/prd.md` | 真实后端/模式与回退边界回归 | test_tier_required |
| SC-4 | qa_engineer | PRD-WORLD_SIMULATOR-042/043 | `README.md`; `doc/world-simulator/prd.md` | release gate、公开文案与 LiveOps owner 记录 | test_tier_required |
| SC-5 | qa_engineer | PRD-WORLD_SIMULATOR-039/041/046 / PRD-GAME-008 / PRD-TESTING-003 | `doc/world-simulator/prd.md`; `doc/game/prd.md`; `doc/testing/prd.md` | primary mode、可玩性/parity 分类与非替代证据审计 | test_tier_required |
| SC-6 | viewer_engineer / qa_engineer / liveops_community | PRD-WORLD_SIMULATOR-020 / PRD-WORLD_SIMULATOR-042 / PRD-WORLD_SIMULATOR-045 / PRD-TESTING-003 | `README.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 同版本、平台和 primary mode 的发现、进入、核验、失败与升级恢复端到端证据，包含适用平台真实资产与完整 release gate | test_tier_full |
| SC-7 | producer_system_designer / gameplay_designer / viewer_engineer / qa_engineer / liveops_community | PRD-GAME-015 / PRD-WORLD_SIMULATOR-042/043/045 / PRD-TESTING-003 | `doc/product/player-entry-distribution/free-entry-world-progression-and-recognition.prd.md`; `doc/game/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 免费基础进入、非权力型可选服务、世界内成长/认可/区域互赖及当前 claim 分离的组合审计 | test_tier_required |
| SC-8 | producer_system_designer / viewer_engineer / runtime_engineer / qa_engineer / liveops_community | PRD-WORLD_SIMULATOR-020 / PRD-WORLD_SIMULATOR-042 / PRD-WORLD_RUNTIME-014 / PRD-TESTING-003 | `doc/world-simulator/prd.md`; `doc/world-runtime/prd.md`; `doc/testing/prd.md` | 真实发行资产覆盖兼容继续、写入前失败且可验证回退、未知或可能写入时阻止降级三类样例，并核对版本、平台、primary mode、候选资产、备份来源与权威状态非回退边界 | test_tier_full |
| SC-9 | producer_system_designer / runtime_engineer / agent_engineer / viewer_engineer / blockchain_ops_engineer / qa_engineer | PRD-P2P-004 / PRD-P2P-023 / PRD-P2P-029 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_RUNTIME-003 / PRD-WORLD_SIMULATOR-039 / PRD-WORLD_SIMULATOR-041 / PRD-WORLD_SIMULATOR-046 / PRD-TESTING-003 | `doc/p2p/prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md`; `doc/product/player-entry-distribution/access-modes-and-release-readiness.prd.md`; `doc/product/world-infrastructure/prd.md`; `doc/product/world-rules-core-gameplay/prd.md` | [SC-9-U1–U5 组合样例](#player-session-unknown-outcome-acceptance)：提交后丢回复、重新认证、原请求查询/幂等重试、查询受阻及错误归属；区分尚未决定与既定执行续接，核对 Agent 授权先于或晚于会话失效时的单次效果、无越权新提交、无自动续权，以及 Viewer 玩家可读状态、`pure_api` 结构化状态与原 receipt 的同义结果 | test_tier_full |
| SC-10 | viewer_engineer / runtime_engineer / qa_engineer | PRD-WORLD_SIMULATOR-039 / PRD-WORLD_RUNTIME-003 / PRD-TESTING-003 | `doc/world-simulator/prd.md`; `doc/world-runtime/prd.md`; `doc/testing/prd.md` | 每个受支持平台上的错误 checksum、缺失/不可完成核对与重新取得正确候选的安装前负例；核对结果、可用恢复路径，以及不改写本地状态、不恢复世界 authority/receipt 的组合核对 | test_tier_required |
| SC-11 | viewer_engineer / game_visual_interaction_designer / qa_engineer；模块维护者维护、发布责任人复核 | PRD-SITE-012 | `doc/site/prd.md`；[既有 deck 合同](../../site/prd.md#prd-site-012)、[AC-15](../../site/prd.md#ac-15)、[AC-16](../../site/prd.md#ac-16)、[AC-17](../../site/prd.md#ac-17) | [原专业验证入口](../../site/prd.md#validation-site-012)：docs hub / deck 入口、静态链接、导航和状态文案核对；双语/hash/键盘触控与页面比较沿原设计，当前未执行 | test_tier_required |

### 5.2 首写与升级的系统设计承接

SC-8 / SC-10 的平台预检、状态清点、首写边界与三类恢复结论由
[手动升级首写设计](../../site/design.md#manual-upgrade-first-write)组合承接，具体
[平台验证映射](../../site/design.md#manual-upgrade-validation)分别覆盖 Windows、macOS 与 Linux 的实际受支持入口。
[包来源与发行安全](../../engineering/workflow/package-source-security.md#final-artifact-freshness)
继续拥有最终资产身份/新鲜度与成功后的版本保留；原状态格式/代际及世界恢复仍由 runtime 主责。
这些链接补齐实施与验证责任，不改变 SC-8 / SC-10 的产品结果，也不表示预检、迁移或平台验收已完成。

### 5.3 根成功标准的逐项状态与语义身份

SC-1～SC-7 复用叶子要求的 Trace refs，只作为组合验收，不重复声明要求。SC-8/SC-9/SC-10 仅为下述根独立义务声明身份，不替代相交叶子或专业 authority。原 §5.1 六列、原 SC 文本与测试层级保持不变。

以下为 2026-10-15 对 `dffd797f` 文档候选的关系复核；实现参照各叶子已注明的固定源码基线。根组合均未取得覆盖全部条件的同候选运行记录，未执行新安装、浏览器、部署或真人验收。官网六叶子既有“已覆盖 / 已实现待验证”不因根组合未知而降级。


<a id="sc-entry-001"></a>
#### SC-1：成功标准与状态

- Trace refs: O7-REQ-e20ed8c7-6f8d-400a-a883-57b2e772acff, O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385。
- 引用定位与组合边界：O7-REQ-e20ed8c7-6f8d-400a-a883-57b2e772acff（[REQ-ENTRY-CLAIM-001](release-communications-and-public-claims.prd.md#req-entry-claim-001)）; O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385（[REQ-ENTRY-CLAIM-003](release-communications-and-public-claims.prd.md#req-entry-claim-003)）。
- 适用范围：§5 的 SC-1 全部组合条件，不能用一项叶子结果代签。
##### 功能状态卡：SC-1

- Trace refs: O7-REQ-e20ed8c7-6f8d-400a-a883-57b2e772acff, O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385。
- 引用定位与组合边界：O7-REQ-e20ed8c7-6f8d-400a-a883-57b2e772acff（[REQ-ENTRY-CLAIM-001](release-communications-and-public-claims.prd.md#req-entry-claim-001)）; O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385（[REQ-ENTRY-CLAIM-003](release-communications-and-public-claims.prd.md#req-entry-claim-003)）。
- 系统设计状态：部分覆盖
- 系统设计依据：[官网来源标识](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-evidence)承接网站来源子范围；认领经济四层模型见叶子卡。
- 实现状态：未核实
- 实现与验证依据：README 当前边界与 source/test 链接；首页固定受控案例 → claim gate → Pages build。 当前 README 不再被泛化为旧七月 review；能定位公开限定文案及构建检查，但 gate 针对特定首页 DOM，不审核 README 每条专业事实。 [README.md:8–33](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/README.md#L8-L33)；[scripts/site-homepage-claim-check.sh:392–442](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/scripts/site-homepage-claim-check.sh#L392-L442)；[.github/workflows/pages.yml:23–39](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/.github/workflows/pages.yml#L23-L39)
- 剩余范围：根 README 每项现行 claim 与专业 authority/同窗 QA 的完整对账记录未核清；保留发布/玩法边界，不能以文案诚实或构建检查代签当前验收。
- 核对日期与基线：2026-10-17；源码 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，本轮固定源码/操作记录只读复核；未运行安装、浏览器、模型或业务验收。


- 本次实现复核：2026-10-17；需求与源码固定基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub固定文本/PR记录审读；覆盖本条原入口/阶段及验收范围，未执行浏览器、安装或业务验收；旧核对基线只保留历史语境。

<a id="sc-entry-002"></a>
#### SC-2：成功标准与状态

- Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264, O7-REQ-d23033f5-0d4d-40b6-8745-1374374af0c3。
- 引用定位与组合边界：O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264（[REQ-ENTRY-MODE-001](access-modes-and-release-readiness.prd.md#req-entry-mode-001)）; O7-REQ-d23033f5-0d4d-40b6-8745-1374374af0c3（[REQ-SITE-DISCOVERY-004](public-site-discovery-and-showcase.prd.md#req-site-discovery-004)）。
- 适用范围：§5 的 SC-2 全部组合条件，不能用一项叶子结果代签。
##### 功能状态卡：SC-2

- Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264, O7-REQ-d23033f5-0d4d-40b6-8745-1374374af0c3。
- 引用定位与组合边界：O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264（[REQ-ENTRY-MODE-001](access-modes-and-release-readiness.prd.md#req-entry-mode-001)）; O7-REQ-d23033f5-0d4d-40b6-8745-1374374af0c3（[REQ-SITE-DISCOVERY-004](public-site-discovery-and-showcase.prd.md#req-site-discovery-004)）。
- 系统设计状态：部分覆盖
- 系统设计依据：[下载同候选展示](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-download)与叶子首次注册/返回设计承接相应入口。
- 实现状态：未核实
- 实现与验证依据：release API 同快照 → 平台 source map → 版本/说明/checksum/安装要求可见。 真实官网公开发现与下载展示子链已闭合到消费者；快照缺少 primary mode/QA verdict，平台切换不生成独立验证路径。 [site/assets/release-candidate.js:3–26](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/site/assets/release-candidate.js#L3-L26)；[site/assets/app.js:616–658](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/site/assets/app.js#L616-L658)；[site/assets/app.js:758–866](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/site/assets/app.js#L758-L866)；[crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher.rs:966–985](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher.rs#L966-L985)
- 剩余范围：所有公开入口的 mode/版本/平台/可重复验证组合仍未核清；本轮定位缺的是页面元数据到实际运行候选的语义连接。
- 核对日期与基线：2026-10-17；源码 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，本轮固定源码/操作记录只读复核；未运行安装、浏览器、模型或业务验收。


- 本次实现复核：2026-10-17；需求与源码固定基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub固定文本/PR记录审读；覆盖本条原入口/阶段及验收范围，未执行浏览器、安装或业务验收；旧核对基线只保留历史语境。

<a id="sc-entry-003"></a>
#### SC-3：成功标准与状态

- Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1, O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270, O7-REQ-d23033f5-0d4d-40b6-8745-1374374af0c3。
- 引用定位与组合边界：O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1（[REQ-ENTRY-LAUNCH-001](local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001)）; O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270（[REQ-ENTRY-MODE-002](access-modes-and-release-readiness.prd.md#req-entry-mode-002)）; O7-REQ-d23033f5-0d4d-40b6-8745-1374374af0c3（[REQ-SITE-DISCOVERY-004](public-site-discovery-and-showcase.prd.md#req-site-discovery-004)）。
- 适用范围：§5 的 SC-3 全部组合条件，不能用一项叶子结果代签。
##### 功能状态卡：SC-3

- Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1, O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270, O7-REQ-d23033f5-0d4d-40b6-8745-1374374af0c3。
- 引用定位与组合边界：O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1（[REQ-ENTRY-LAUNCH-001](local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001)）; O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270（[REQ-ENTRY-MODE-002](access-modes-and-release-readiness.prd.md#req-entry-mode-002)）; O7-REQ-d23033f5-0d4d-40b6-8745-1374374af0c3（[REQ-SITE-DISCOVERY-004](public-site-discovery-and-showcase.prd.md#req-site-discovery-004)）。
- 系统设计状态：部分覆盖
- 系统设计依据：[Launcher 重新验证](../../world-simulator/launcher/game-client-launcher-runtime-session-continuity.design.md#session-revalidation)与[下载状态](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-download)已区分本地探测、最后已知候选和真实结果。
- 实现状态：部分实现
- 实现与验证依据：Launcher viewer URL→WebGL2/Wasm loader→host adapter→fatal unavailable→玩家不可用及 Retry Renderer 的生产链已定位。该 Viewer 子范围不会用不可用 bridge 假装 ready。 [生产 URL 固定 render_mode=viewer，独立 ws 与 hosted_access 参数](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher.rs#L966-L985)；[真实 Wasm bridge 初始化失败返回 unavailable，而非假 ready](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/pixel_world_runtime_loader.js#L108-L149)；[生产 host adapter 消费 runtime loader](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/pixel_world_host_adapter.js#L70-L110)；[fatal→卸载/清除 Rust 渲染状态→unavailable 与真实 runtime metadata](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx#L932-L960)；[玩家图形不可用说明与 Retry Renderer](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx#L1280-L1293)。本轮静态审读；测试定义及历史 PR 结果不替代完整验收。
- 剩余范围：其他平台发行资产、native/Web 全部后端和内部模式切换/回退表示尚未逐入口核清；该链不证明整体真实发布体验。
- 核对日期与基线：2026-10-17；需求/源码 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`，main `bab8990cef3b42fe5a0a5538b5dd0c80ca3b19f5` 仅后继文档调整；未执行本项业务验收。


- 本次实现复核：2026-10-17；需求与源码固定基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub固定文本/PR记录审读；覆盖本条原入口/阶段及验收范围，未执行浏览器、安装或业务验收；旧核对基线只保留历史语境。

<a id="sc-entry-004"></a>
#### SC-4：成功标准与状态

- Trace refs: O7-REQ-e20ed8c7-6f8d-400a-a883-57b2e772acff, O7-REQ-f71ac2e9-1586-44af-89b7-e98ff364e70c。
- 引用定位与组合边界：O7-REQ-e20ed8c7-6f8d-400a-a883-57b2e772acff（[REQ-ENTRY-CLAIM-001](release-communications-and-public-claims.prd.md#req-entry-claim-001)）; O7-REQ-f71ac2e9-1586-44af-89b7-e98ff364e70c（[REQ-ENTRY-CLAIM-002](release-communications-and-public-claims.prd.md#req-entry-claim-002)）。
- 适用范围：§5 的 SC-4 全部组合条件，不能用一项叶子结果代签。
##### 功能状态卡：SC-4

- Trace refs: O7-REQ-e20ed8c7-6f8d-400a-a883-57b2e772acff, O7-REQ-f71ac2e9-1586-44af-89b7-e98ff364e70c。
- 引用定位与组合边界：O7-REQ-e20ed8c7-6f8d-400a-a883-57b2e772acff（[REQ-ENTRY-CLAIM-001](release-communications-and-public-claims.prd.md#req-entry-claim-001)）; O7-REQ-f71ac2e9-1586-44af-89b7-e98ff364e70c（[REQ-ENTRY-CLAIM-002](release-communications-and-public-claims.prd.md#req-entry-claim-002)）。
- 系统设计状态：部分覆盖
- 系统设计依据：[官网编辑/发布/纠正](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-publishing)明确事实核对、同步与部署分离。
- 实现状态：未核实
- 实现与验证依据：Pages workflow → homepage claim check → upload site。 存在真实构建门禁，固定 case/source/window 和双语 disclosure 校验；解析对象为网站文件，输出为退出码。 [scripts/site-homepage-claim-check.sh:392–442](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/scripts/site-homepage-claim-check.sh#L392-L442)；[.github/workflows/pages.yml:23–39](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/.github/workflows/pages.yml#L23-L39)；[README.md:8–33](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/README.md#L8-L33)
- 剩余范围：这条真实 gate 未承接专业 PRD-ID→QA verdict→LiveOps owner 授权。需外部操作记录核对，不由 upload/build 成功推定已发布授权。
- 核对日期与基线：2026-10-17；源码 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，本轮固定源码/操作记录只读复核；未运行安装、浏览器、模型或业务验收。


- 本次实现复核：2026-10-17；需求与源码固定基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub固定文本/PR记录审读；覆盖本条原入口/阶段及验收范围，未执行浏览器、安装或业务验收；旧核对基线只保留历史语境。

<a id="sc-entry-005"></a>
#### SC-5：成功标准与状态

- Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264, O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270, O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b。
- 引用定位与组合边界：O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264（[REQ-ENTRY-MODE-001](access-modes-and-release-readiness.prd.md#req-entry-mode-001)）; O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270（[REQ-ENTRY-MODE-002](access-modes-and-release-readiness.prd.md#req-entry-mode-002)）; O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b（[REQ-ENTRY-CANDIDATE-001](access-modes-and-release-readiness.prd.md#req-entry-candidate-001)）。
- 适用范围：§5 的 SC-5 全部组合条件，不能用一项叶子结果代签。
##### 功能状态卡：SC-5

- Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264, O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270, O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b。
- 引用定位与组合边界：O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264（[REQ-ENTRY-MODE-001](access-modes-and-release-readiness.prd.md#req-entry-mode-001)）; O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270（[REQ-ENTRY-MODE-002](access-modes-and-release-readiness.prd.md#req-entry-mode-002)）; O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b（[REQ-ENTRY-CANDIDATE-001](access-modes-and-release-readiness.prd.md#req-entry-candidate-001)）。
- 系统设计状态：部分覆盖
- 系统设计依据：模式与失败恢复已有叶子所列首次绑定/返回子范围；[官网下载](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-download)只消费模式事实，不定义新模式。
- 实现状态：未核实
- 实现与验证依据：Launcher no-LLM fail-fast → viewer URL；页面唯一 viewer 验证面 → release metadata。 no-LLM guard 的错误清晰且禁止玩法 fallback；render/deployment 与 candidate 的字段边界已识别，仍非 viewer/pure_api 独立 playable/parity 分类。 [crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher.rs:335–344](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher.rs#L335-L344)；[crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher.rs:966–985](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher.rs#L966-L985)；[site/assets/release-candidate.js:3–26](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/site/assets/release-candidate.js#L3-L26)
- 剩余范围：两 mode 的能力/阶段/证据及 observer/blocked 分类完整事实源与消费者未核清；不因一个 viewer 入口 guard 升根组合状态。
- 核对日期与基线：2026-10-17；源码 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，本轮固定源码/操作记录只读复核；未运行安装、浏览器、模型或业务验收。


- 本次实现复核：2026-10-17；需求与源码固定基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub固定文本/PR记录审读；覆盖本条原入口/阶段及验收范围，未执行浏览器、安装或业务验收；旧核对基线只保留历史语境。

<a id="sc-entry-006"></a>
#### SC-6：成功标准与状态

- Trace refs: O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b, O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1, O7-REQ-1d9a996e-dd9d-4d59-9b3b-b79191f17676, O7-REQ-d23033f5-0d4d-40b6-8745-1374374af0c3, O7-REQ-2f44d108-4588-46f4-8c96-63b79983a15f。
- 引用定位与组合边界：O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b（[REQ-ENTRY-CANDIDATE-001](access-modes-and-release-readiness.prd.md#req-entry-candidate-001)）; O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1（[REQ-ENTRY-LAUNCH-001](local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001)）; O7-REQ-1d9a996e-dd9d-4d59-9b3b-b79191f17676（[REQ-ENTRY-LAUNCH-002](local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-002)）; O7-REQ-d23033f5-0d4d-40b6-8745-1374374af0c3（[REQ-SITE-DISCOVERY-004](public-site-discovery-and-showcase.prd.md#req-site-discovery-004)）。；O7-REQ-2f44d108-4588-46f4-8c96-63b79983a15f（[SC-8 首写独立义务](#sc-entry-008)）。
- 适用范围：§5 的 SC-6 全部组合条件，不能用一项叶子结果代签。
##### 功能状态卡：SC-6

- Trace refs: O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b, O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1, O7-REQ-1d9a996e-dd9d-4d59-9b3b-b79191f17676, O7-REQ-d23033f5-0d4d-40b6-8745-1374374af0c3, O7-REQ-2f44d108-4588-46f4-8c96-63b79983a15f。
- 引用定位与组合边界：O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b（[REQ-ENTRY-CANDIDATE-001](access-modes-and-release-readiness.prd.md#req-entry-candidate-001)）; O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1（[REQ-ENTRY-LAUNCH-001](local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001)）; O7-REQ-1d9a996e-dd9d-4d59-9b3b-b79191f17676（[REQ-ENTRY-LAUNCH-002](local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-002)）; O7-REQ-d23033f5-0d4d-40b6-8745-1374374af0c3（[REQ-SITE-DISCOVERY-004](public-site-discovery-and-showcase.prd.md#req-site-discovery-004)）。；O7-REQ-2f44d108-4588-46f4-8c96-63b79983a15f（[SC-8 首写独立义务](#sc-entry-008)）。
- 系统设计状态：部分覆盖
- 系统设计依据：[下载候选](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-download)、[Launcher 重验](../../world-simulator/launcher/game-client-launcher-runtime-session-continuity.design.md#session-revalidation)及[升级首写](../../site/design.md#manual-upgrade-first-write)分别承接各自阶段。
- 实现状态：未核实
- 实现与验证依据：官网发现/平台下载 → Windows bundle 安装 → Launcher viewer URL；各阶段源已定位。 release 页面 tag/资产，NSIS RELEASE_VERSION，Launcher render_mode 是三个不同数据面；页面提供的历史验证随新 tag 收窄。 [site/assets/app.js:616–658](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/site/assets/app.js#L616-L658)；[site/assets/app.js:758–866](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/site/assets/app.js#L758-L866)；[scripts/windows-release-installer.nsi:34–52](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/scripts/windows-release-installer.nsi#L34-L52)；[crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher.rs:966–985](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher.rs#L966-L985)；[site/assets/app.js:1051–1065](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/site/assets/app.js#L1051-L1065)
- 剩余范围：未核清一个生产 DTO/收据把下载 tag/hash、实际安装字节、运行 build/backend/primary mode 关联为同候选；不能将并列代码串成已验证全链。
- 核对日期与基线：2026-10-17；源码 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，本轮固定源码/操作记录只读复核；未运行安装、浏览器、模型或业务验收。


- 本次实现复核：2026-10-17；需求与源码固定基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub固定文本/PR记录审读；覆盖本条原入口/阶段及验收范围，未执行浏览器、安装或业务验收；旧核对基线只保留历史语境。

<a id="sc-entry-007"></a>
#### SC-7：成功标准与状态

- Trace refs: O7-REQ-32e47f2a-2da3-4ca6-aaf0-ef522be4b898, O7-REQ-dc05d050-6c0c-4dbd-bffc-a5314c6f133e, O7-REQ-c980624f-bb15-4fab-bc95-d950f6d83de5, O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385。
- 引用定位与组合边界：O7-REQ-32e47f2a-2da3-4ca6-aaf0-ef522be4b898（[REQ-ENTRY-FREE-001](free-entry-world-progression-and-recognition.prd.md#req-entry-free-001)）; O7-REQ-dc05d050-6c0c-4dbd-bffc-a5314c6f133e（[REQ-ENTRY-FREE-002](free-entry-world-progression-and-recognition.prd.md#req-entry-free-002)）; O7-REQ-c980624f-bb15-4fab-bc95-d950f6d83de5（[REQ-ENTRY-PART-001](participation-and-recognition-boundaries.prd.md#req-entry-part-001)）; O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385（[REQ-ENTRY-CLAIM-003](release-communications-and-public-claims.prd.md#req-entry-claim-003)）。
- 适用范围：§5 的 SC-7 全部组合条件，不能用一项叶子结果代签。
##### 功能状态卡：SC-7

- Trace refs: O7-REQ-32e47f2a-2da3-4ca6-aaf0-ef522be4b898, O7-REQ-dc05d050-6c0c-4dbd-bffc-a5314c6f133e, O7-REQ-c980624f-bb15-4fab-bc95-d950f6d83de5, O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385。
- 引用定位与组合边界：O7-REQ-32e47f2a-2da3-4ca6-aaf0-ef522be4b898（[REQ-ENTRY-FREE-001](free-entry-world-progression-and-recognition.prd.md#req-entry-free-001)）; O7-REQ-dc05d050-6c0c-4dbd-bffc-a5314c6f133e（[REQ-ENTRY-FREE-002](free-entry-world-progression-and-recognition.prd.md#req-entry-free-002)）; O7-REQ-c980624f-bb15-4fab-bc95-d950f6d83de5（[REQ-ENTRY-PART-001](participation-and-recognition-boundaries.prd.md#req-entry-part-001)）; O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385（[REQ-ENTRY-CLAIM-003](release-communications-and-public-claims.prd.md#req-entry-claim-003)）。
- 系统设计状态：未核实
- 系统设计依据：世界内认可/机会主责与入口负边界见 FREE-002；贡献操作状态见 PART-001；尚未确认免费基础路径、可选服务和世界规则的完整系统设计组合。
- 实现状态：未核实
- 实现与验证依据：首局显式资源动作、贡献 draft/approved/pending 链、公开当前/目标限制分别定位。 这些切片没有把可选便利服务取消/恢复、成熟世界机会或当前公开能力连接起来；批准贡献款仍是未关闭履约义务。 [crates/oasis7/src/viewer/gameplay_actions.rs:86–116](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7/src/viewer/gameplay_actions.rs#L86-L116)；[doc/readme/governance/readme-limited-preview-contributor-reward-distribution-closure-round-2026-04-13.md:3–16](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/doc/readme/governance/readme-limited-preview-contributor-reward-distribution-closure-round-2026-04-13.md#L3-L16)；[README.md:8–33](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/README.md#L8-L33)；[site/index.html:263–283](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/site/index.html#L263-L283)
- 剩余范围：要核清的是服务生命周期→权限/历史/独立进入与成熟认可→入口的组合消费，不是再证明普通会话刷新或模板存在。
- 核对日期与基线：2026-10-17；源码 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，本轮固定源码/操作记录只读复核；未运行安装、浏览器、模型或业务验收。


- 本次实现复核：2026-10-17；需求与源码固定基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub固定文本/PR记录审读；覆盖本条原入口/阶段及验收范围，未执行浏览器、安装或业务验收；旧核对基线只保留历史语境。

<a id="sc-entry-008"></a>
#### SC-8：成功标准与状态

- Trace ID: O7-REQ-2f44d108-4588-46f4-8c96-63b79983a15f
- 声明范围：仅声明原状态首次不可逆写入前的兼容三分判定，以及未改写证明/未知写入下安全回退的独立安装义务；不重新声明 LAUNCH-001/002 的本地会话重验。
##### 功能状态卡：SC-8

- Trace refs: O7-REQ-2f44d108-4588-46f4-8c96-63b79983a15f；[SC-8 唯一声明](#sc-entry-008)。
- 系统设计状态：部分覆盖
- 系统设计依据：[首写预检与三类结果](../../site/design.md#manual-upgrade-first-write)、逐平台验证矩阵已明确原状态清点、写入方静止、备份关联、未知首写禁止降级及权威非回退。
- 实现状态：未核实
- 实现与验证依据：实际 Windows install File /r → Launcher UX 读取/保存；公开说明手动覆盖与备份。 Windows install section 在覆盖前没有消费兼容三分结果；UX 解析失败回默认后可直接写新状态。卸载会删全目录是实际代码而非推测。 [scripts/windows-release-installer.nsi:34–52](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/scripts/windows-release-installer.nsi#L34-L52)；[crates/oasis7_client_launcher/src/self_guided_storage.rs:8–56](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_client_launcher/src/self_guided_storage.rs#L8-L56)；[crates/oasis7_client_launcher/src/self_guided.rs:226–261](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_client_launcher/src/self_guided.rs#L226-L261)；[scripts/windows-release-installer.nsi:55–62](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/scripts/windows-release-installer.nsi#L55-L62)；[site/index.html:263–283](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/site/index.html#L263-L283)
- 剩余范围：已经定位 Windows/UX 生产缺口；其他平台、runtime 世界旧格式与首次不可逆写入仍须独立审计。不能把 operator backup/rollback 当玩家入口安全证明。
- 核对日期与基线：2026-10-17；源码 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，本轮固定源码/操作记录只读复核；未运行安装、浏览器、模型或业务验收。


- 本次实现复核：2026-10-17；需求与源码固定基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub固定文本/PR记录审读；覆盖本条原入口/阶段及验收范围，未执行浏览器、安装或业务验收；旧核对基线只保留历史语境。

<a id="sc-entry-009"></a>
#### SC-9：成功标准与状态

- Trace ID: O7-REQ-a15a1b8e-8b7f-4c6a-9cb5-2f949730496c
- 声明范围：仅声明玩家会话、当前读取资格及 Agent 委托与原已接受请求结果正交、重新认证后按原请求核对且不隐式重提的入口组合义务；P2P/runtime 原决定、pending、receipt 和 lineage 继续消费其既有 Trace 与专业合同。
##### 功能状态卡：SC-9

- Trace refs: O7-REQ-a15a1b8e-8b7f-4c6a-9cb5-2f949730496c；[SC-9 唯一声明](#sc-entry-009)。
- 系统设计状态：部分覆盖
- 系统设计依据：[runtime 原决定接收](../../world-runtime/design.md#runtime-controlled-candidate-recovery-receiver)与 [P2P 原 candidate 恢复](../../p2p/design.md#des-p2p-original-candidate-recovery-v1)区分 pending 重审、固定原决定续接和未知结果；SC-9-U1～U5 保留玩家组合判据。
- 实现状态：部分实现
- 实现与验证依据：Viewer canonical Agent goal 子范围：原签名 request pending 重送、身份改变停止、ACK 原 tuple 校验、超时 unknown、当前 owner 的当前有效 goal 签名读取与反馈显示已接到生产消费者。另有 v2 join/refresh/release 绑定会话与 world/key/epoch。 [实际 Agent Chat 注册→当前 owner read→签名→保存原 request→发送](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/legacy_core.js#L2660-L2694)；[pending 不宣布接受；重送同一签名 request；世界/branch/reorg/身份变化或连接不可用时停止重送](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/viewer_canonical_goal_module.js#L1-L30)；[canonical 请求超时明确 unknown，提示刷新当前世界](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/legacy_core.js#L945-L970)；[生产 ACK 消费者验证 runtime identity/原 tuple，pending 不当 accepted](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/legacy_core.js#L3121-L3138)；[当前已注册主体/世界/权限范围、签名请求、响应相关性与 min commit；失败清除可用 owner view](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/viewer_canonical_owner_read_module.js#L26-L120)；[仅当前 owner 的有效 canonical goal；ACK 绑定原 tuple](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/viewer_canonical_goal_module.js#L60-L75)；[玩家 Agent Chat 展示 canonical goal、stage 及反馈详情](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/legacy_core.js#L3975-L3988)；[v2 HTTP world 入口验证账号 session/open_preview 与逐账号世界绑定](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_identity/world.rs#L65-L138)；[重新验证会话，授予短期 player_session，绑定 world/key/epoch；非泛化世界权限](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_identity/world.rs#L338-L394)。本轮静态审读；测试定义及历史 PR 结果不替代完整验收。
- 剩余范围：不能扩大为完整 SC-9：pure_api、已结算经济 receipt、跨重新认证原请求持久查询、Agent 委托到期/撤销与尚未决定/固定执行恢复的全部接线及 U1–U5 同候选案例仍未全核清。 当前 goal 要求 current_intent_id 匹配，不能证明任意历史请求/receipt 可查询。
- 核对日期与基线：2026-10-17；需求/源码 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`，main `bab8990cef3b42fe5a0a5538b5dd0c80ca3b19f5` 仅后继文档调整；未执行本项业务验收。


- 本次实现复核：2026-10-17；需求与源码固定基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub固定文本/PR记录审读；覆盖本条原入口/阶段及验收范围，未执行浏览器、安装或业务验收；旧核对基线只保留历史语境。

<a id="sc-entry-010"></a>
#### SC-10：成功标准与状态

- Trace ID: O7-REQ-2c381c11-8004-4efb-bc82-b04958b7eec5
- 声明范围：仅声明推荐资产在安装/覆盖既有状态前的真实完整性失败阻断和重新取得后的独立重验义务；SITE-DISCOVERY-004 仍独占页面版本/资产展示，不由本条重发身份。
##### 功能状态卡：SC-10

- Trace refs: O7-REQ-2c381c11-8004-4efb-bc82-b04958b7eec5；[SC-10 唯一声明](#sc-entry-010)。
- 系统设计状态：部分覆盖
- 系统设计依据：[资产校验至使用及首写合同](../../site/design.md#manual-upgrade-first-write)、逐平台完整性负例矩阵已规定缺失/不可验/不匹配停止，以及对象替换重验。
- 实现状态：未核实
- 实现与验证依据：候选 JSON URL/checksum存在性 → 页面推荐 → Windows installer复制；Linux .deb 打包只生成文件/元数据。 页面检查仅确认 checksum 资产存在，不读取校验文件内容或下载字节。Windows 写入 section 不核该外部候选校验关系；不是包格式自检/OS策略是否存在的全称断言。 [site/assets/release-candidate.js:3–26](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/site/assets/release-candidate.js#L3-L26)；[scripts/windows-release-installer.nsi:34–52](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/scripts/windows-release-installer.nsi#L34-L52)；[scripts/package-native-installer.sh:185–218](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/scripts/package-native-installer.sh#L185-L218)；[site/index.html:263–283](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/site/index.html#L263-L283)
- 剩余范围：实际安装前的缺失/不可验/不匹配停止、对象替换后独立重验与安全下一步尚未接通于已读 Windows 链；其他平台另核，不宣称全仓无校验。
- 核对日期与基线：2026-10-17；源码 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，本轮固定源码/操作记录只读复核；未运行安装、浏览器、模型或业务验收。


- 本次实现复核：2026-10-17；需求与源码固定基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub固定文本/PR记录审读；覆盖本条原入口/阶段及验收范围，未执行浏览器、安装或业务验收；旧核对基线只保留历史语境。

<a id="sc-entry-011"></a>
### 5.4 SC-11：既有 HTML 路演 deck 的独立接收与状态

Trace ID: O7-REQ-6cbda46f-bba2-423f-8197-2a15a4b1d5a6

声明范围：仅为 §5 SC-11 的既有顺序化公开说明结果登记身份，来源为 [PRD-SITE-012](../../site/prd.md#prd-site-012)、[原功能矩阵](../../site/prd.md#deck-functional-contract)与 [DEC-SITE-012](../../site/prd.md#dec-site-012)。原 [AC-15](../../site/prd.md#ac-15)、[AC-16](../../site/prd.md#ac-16)、[AC-17](../../site/prd.md#ac-17)继续定义可访问页面、静态发布兼容及入口/回链/预览边界；原专业权限和验收真值不迁移。该身份不覆盖 discovery 六叶，也不为它们重复声明。

- 功能点：SC-11；适用原 CN/EN deck route family，不新增语言、页面、内容或布局方案。
- Trace refs: O7-REQ-6cbda46f-bba2-423f-8197-2a15a4b1d5a6；本节唯一声明。
- 系统设计状态：未核实
- 系统设计依据：[Site 有限接收](../../site/design.md#site-roadshow-deck-receiver)与 [deck 页面原状态](../../site/deck/html-roadshow-deck-page-2026-06-19.design.md#required-states)提供准确定位；尚未核实本范围全部系统义务的覆盖，链接可解析不等于已覆盖。
- 实现状态：部分实现
- 实现与验证依据：PR #202交付双语静态Reveal页面和本地vendor；当前源码保留固定顺序、持续preview、回链、hash/touch与移动前后控制。PR #539提供历史页面修改与响应式检查记录，不能替代当前完整验收。 [有限设计接收](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/doc/site/design.md#site-roadshow-deck-receiver)；[双语/回链及持续preview](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/site/deck/index.html#L25-L48)；[Reveal hash/touch及移动导航](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/site/deck/index.html#L380-L420)；[英文对应导航](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/site/deck/en/index.html#L380-L420)；[静态Pages发布配置](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/.github/workflows/pages.yml#L1-L40)。[交付PR #202](https://github.com/nil-studio/oasis7/pull/202)；[交付PR #539](https://github.com/nil-studio/oasis7/pull/539)。本次未执行仓库测试、浏览器、安装、部署或实际业务操作；测试源码仅为定义。 PR #202/#539保留历史记录；未取得本固定候选完整行为证据。
- 剩余范围：系统设计范围与未覆盖边界沿本条原设计依据；实现与验证：现行候选双语直接访问、hash/键盘触控/窄屏密集内容、入口回链、静态发布与实际线上部署分开验证；完整设计覆盖仍未核实。
- 核对日期与基线：2026-10-15；原文基线 `a51afcd46c9e949fd4cfa8af84619ace70eeb5bd`，只读文档关系核对；没有执行浏览器、站点、部署、发布或体验验收。
- 本次实现复核：2026-10-17；需求与源码固定基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub固定文本/PR记录审读；覆盖本条原入口/阶段及验收范围，未执行浏览器、安装或业务验收；旧核对基线只保留历史语境。


## 6. Non-Goals

- 不宣布任何未由当前根 README 与发布证据支持的玩家可用性。
- 不在产品层定义 Launcher、Viewer、登录、打包或渠道实现。
- 不用产品路线图代替发布任务、QA 放行、运营承诺或渠道 runbook。

# oasis7 免邮箱注册与多凭证账号体系：系统设计
> 配套系统设计 · v1.1 · 2026-10-14 · `implementation candidate`
>
> **目标：用标准公钥认证建立和恢复稳定账号，复用既有玩家与世界接入能力；不把注册便利性变成账号丢失、越权绑定或资产接管。**

| 文档身份 | 内容 |
| --- | --- |
| 配对产品文档 | [《免邮箱注册与多凭证账号体系》v1.0](hosted-public-join-key-based-onboarding.prd.md) |
| 产品输入版本 | 以同一 Git / PR 版本的配对 PRD 为准；不把旧附件摘要当作后续变更的放行凭据 |
| 系统设计路径 | `doc/p2p/blockchain/hosted-public-join-key-based-onboarding.design.md` |
| 产品归属 | 玩家接入与发行；系统合同归属 hosted identity / player-session 专业域 |
| 历史代码核对基线 | `eng-cc/oasis7`，`main` 提交 `f69c8a666024fa564e52314965025b019fa42d4c`，2026-10-13 读取；同 PR 实现候选的实际路由见 §11.2 |
| 承接需求 | `PRD-KEYAUTH-001`～`012`、`AC-01`～`16`、`NFR-01`～`12` |
| 设计主责角色 | `producer_system_designer`；实现由 identity / runtime / viewer 工程职责共同承接，未虚构人员分派 |
| 适用阶段 | P0：受控 hosted Web 邀请测试闭环；P1：受保护原生客户端登录密钥 |
| 状态边界 | 本文约束同 PR 的 P0 实现候选；真实云事务、真实设备矩阵、性能与邀请资格尚未验收，不是公开放量凭据；交付事实以 Git、PR、CI 与评审记录为准 |
本文以[配对 PRD](hosted-public-join-key-based-onboarding.prd.md)为唯一产品输入，不维护第二套账号需求。产品文档后续按仓库产品分类规范归入“玩家接入与发行”时，本系统设计仍留在专业域，通过更新配对链接承接，避免复制两份长期权威。
## 0. 设计摘要与阅读入口
采用**模块化单体 + 标准 WebAuthn 验证 + 持久事务存储 + 独立账号会话 + 既有世界会话适配**。P0 不新建身份微服务集群，不引入通用 IAM 平台，不要求 Redis、外部钱包、托管资产服务或链上交易成为注册前置。
| 决策 | 本设计采用的方案 | 主要收益 / 代价 |
| --- | --- | --- |
| 网页公钥认证 | 可发现通行密钥，要求用户验证，服务端保留认证仪式状态 | 不填邮箱或账号 ID；仍需真实设备兼容验证 |
| 账号标识 | 保留 `hosted_account_id`、既有 `player_id`；公钥只是凭证 | 换凭证不换号；需要独立凭证和索引记录 |
| 本地持久化 | `file` 配置语义保留，v2 实现改为本地 SQLite；旧 JSON 经显式迁移保留备份 | 避免自研多文件事务；增加一个嵌入式数据库依赖，不增加服务 |
| 云端持久化 | Tablestore 新 v2 表，P0 同一身份命名空间置于同一分区键，使用本地事务 | 使唯一性、挑战消费和恢复替换可原子提交；不能宣称无限横向扩展 |
| 多设备 | 多个账号登录会话；同一玩家在同一世界保留一个控制会话，切换须明确确认 | 不让第二台设备仅登录就踢掉第一台；不扩展多端同时控制玩法 |
| 撤销 | 持久状态和版本为准；推送加速，30 秒授权新鲜度上限兜底 | 在网络分区中也不无限放行旧凭证；身份服务故障会阻断后续玩家新动作 |
| 邮箱恢复 | 预登记新凭证、固定恢复目标、等待/审批、再次邮箱验证、新凭证持有证明、原子替换 | 防止审批后偷换新密钥；比普通登录多必要步骤 |
| 发布兼容 | 先建立 v2 读写和迁移屏障，再开放新注册；关闭注册不关闭登录 | 新账号一旦产生，邮箱唯一旧版本不再是无损回退目标 |
核心机制见 [存储与事务](#storage)、[认证协议](#webauthn)、[恢复](#recovery)、[世界接入与撤销](#world-sessions)；实施定位见 [代码改动与交付](#delivery)，验收见 [测试矩阵](#validation)。

<a id="scope"></a>
## 1. 目标、边界与不可破坏的不变量
### 1.1 P0 必须完成
无 SMTP 的免邮箱注册和登录、旧邮箱账号保留、多凭证管理、账号会话退出、可选邮箱恢复、注册与世界准入分离、端到端撤销、两种存储后端的一致性、灰度和可访问性保护，构成一个完整交付面。不能仅提交 Passkey 按钮和 happy path 后就开放真实用户。

P1 增加原生 Ed25519 **登录**密钥及项目定义的加密备份格式；不要求 runtime 改成使用 Passkey 算法。钱包连接、资产自托管、节点密钥管理、跨任意网站 SSO、自动账号合并和人工弱证明接管不在范围。
### 1.2 不变量
| ID | 必须始终成立 |
| --- | --- |
| INV-01 | 账号 ID 不等于邮箱、公钥、凭证 ID、昵称或浏览器生成的玩家 ID；替换凭证不改变原玩家映射。 |
| INV-02 | 一个身份域内的一个公钥凭证只能属于一个账号；已撤销公钥凭证不能经注册接口重新激活。 |
| INV-03 | 挑战的用途、身份域、账号、发起上下文与操作目标由服务端绑定；成功只能产生一次授权效果。 |
| INV-04 | 账号已创建、账号已登录、世界准入通过、控制会话生效是不同事实；不能相互代签。 |
| INV-05 | 只凭公钥字符串、旧会话或任意邮箱验证码不能为已有公钥账号追加或替换凭证。 |
| INV-06 | 账号冻结是独立状态；认证成功不得写 `status=active` 自动解冻。 |
| INV-07 | 凭证撤销影响该凭证派生的登录会话及其世界能力；退出单个登录会话不删除共享凭证。 |
| INV-08 | 恢复的新凭证激活、旧公钥失效、安全版本更新和恢复结果落档在一个身份事务内完成。 |
| INV-09 | 登录、恢复、增加凭证不改写 `signer_ref`，不签发资产或 operator/governance 权限。 |
| INV-10 | 网络不确定性不能变成第二次开户、重复消耗邀请、重复占 slot 或重放世界 intent。 |
| INV-11 | 限流、挑战消费、唯一绑定不能靠进程内锁或负载均衡粘性会话在多实例下“近似成立”。 |
| INV-12 | 注册开关、邮箱可用性和世界健康分别治理；停止新注册不得遗弃既有纯公钥账号。 |
<a id="baseline"></a>
## 2. 当前代码核对与必要改造
下表保留变更前固定提交的改造基线；不能把它当作同 PR 实现候选仍然只有邮箱登录的现状。候选实现位于 `hosted_identity/{service,credentials,email,recovery,world}.rs`，实际接口和未完成验收见 §11.2、§18.4；这些核对不代表完成全仓安全审计。生产 custody 目标仍不能视为实现事实。[R7]
| 已核对代码 | 当前行为 | 本次必须处理的衔接 |
| --- | --- | --- |
| `hosted_account_identity.rs` | 初始化依赖 SMTP；只支持 email；`complete_login_with_key` 同时完成账户写入与玩家 session 签发，issue 失败会把 OTP challenge 放回待处理集合 [R1] | 邮件成为可选 adapter；身份认证提交与世界接入分开；已消费认证证明不得因满员重新变成可用。 |
| `hosted_account_store_backend.rs` | 账户带单个 `login_channel`；file 有 `accounts_by_id` / `account_id_by_factor`，Tablestore 以 factor 行保存账户 [R2] | 迁移为独立账号、凭证、索引和会话记录；保持旧 ID，不做按公钥重建。 |
| 同一 store 文件 | file 登录会先改内存再保存；已有账号 touch 会写 `status=active`，Tablestore touch 也会更新状态 [R2] | 提交成功前不发布内存新状态；冻结判断和最后验证时间更新分离。这里是新增冻结能力必须修正的路径，不推断现有线上已有冻结事故。 |
| `hosted_player_session.rs` | 一个 player 映射一个 active release token；issue 时先 release 旧 slot；有 30 秒 pending-registration、120 秒 slot lease [R3] | 将账号登录会话与 world control lease 分开；普通登录不调用重发；切换先验证、后原子生效，不能提前释放旧有效会话。 |
| `runtime_live/session_policy.rs` | 每个 player 一个 active public key，轮换更新 session epoch，旧 key 加入撤销集合 [R4] | 保留单控制会话模型；新增身份授权有效性检查和明确接管，不把多凭证直接映射成多控制 key。 |
| `viewer_hosted_auth_state_module.js` | localStorage 保存恢复令牌/registration grant 等材料；读取时重写白名单，不恢复旧 privateKey [R5] | 保留旧私钥清洗；新账号登录 cookie 不进入 JS，长期恢复令牌逐步退出 localStorage。 |
| 既有 hosted identity/custody 设计 | 四类身份与 public/identity/custody/private control 分离 [R7] | 接入多凭证，不合并签名权限。 |
**结论：这不是仅给 `normalize_login_channel` 增加一个值。** 完整范围是身份事务、凭证生命周期、会话派生关系及 runtime 撤销链；世界玩法、共识规则和资产签名算法不随本次重做。
### 2.1 需求承接与分配表
| 上游需求 | 具体义务与适用条件 | 本设计条款 | 主责与依赖 | 未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264；[REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001) | P0 在声明的 `viewer` 入口使用可发现凭证完成认证；通行密钥是本设计选用的协议，不改变产品的 primary mode。 | [条款](#webauthn) | identity / viewer / runtime | 不证明真实设备兼容、生产 custody 或公开放量。 |
| Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) | 身份认证失败、中断或会话失效时提供真实恢复路径；稳定账号登录与重新认证不得恢复已失效 authority。 | [条款](#auth-flows) | identity / viewer / runtime | 不证明真实邮件送达、真实设备矩阵或 P1 原生密钥。 |
| Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) | 撤销或凭证失效后，入口必须保持阻塞/恢复语义；凭证唯一归属和最后凭证保护属于实现该失败边界的专业规则。 | [条款](#credential-management) | identity / viewer / runtime | 不定义产品层的凭证存储 schema。 |
| 产品 N/A：reason=当前产品要求不定义旧邮箱账户的数据迁移策略；scope=旧 hosted account 与 player 映射的持久化迁移；[PRD-KEYAUTH-004](hosted-public-join-key-based-onboarding.prd.md#prd-keyauth-004) | 保留旧邮箱账号与玩家映射；迁移必须保持既有身份关系且不得按公钥重建。 | [条款](#migration) | identity / viewer / runtime | 产品层不承诺具体迁移格式、存储或回滚机制。 |
| Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264；[REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001) | 账号创建、世界准入与 runtime 绑定分别判定；账号登录态不等于世界名额或控制会话，hosted session 仍属于 `viewer` deployment context。 | [条款](#world-sessions) | identity / viewer / runtime | 不授予额外资产、治理或 operator 权限。 |
| Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) | 恢复仅能沿固定候选、等待和原子替换流程继续；不得以回退或重新认证复活旧会话。 | [条款](#recovery) | identity / viewer / runtime | 不证明真实设备、生产恢复服务或灾备。 |
| 产品 N/A：reason=资产签名和治理权限由专业权限 authority 管理，当前产品入口要求未授予登录以外的专业权限；scope=资产与治理操作的凭证授权；[PRD-KEYAUTH-008](hosted-public-join-key-based-onboarding.prd.md#prd-keyauth-008) | 登录不能扩张资产及治理授权；认证态不能被解释为 signer、治理或资产控制权。 | [条款](#world-sessions) | identity / viewer / runtime 与既有专业 authority | 不定义资产 custody 或治理 signer 的生产托管。 |
| Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) | 超时或回复未知时保持原操作身份并先核对；阶段反馈不得把自动重试、fallback 或局部成功伪装成完成。 | [条款](#viewer) | identity / viewer / runtime | 不证明所有 hosted API 均已完成运行验收。 |
| Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) | 认证存储、迁移、关闭注册及安全回退失败必须保留可解释的 blocked/恢复状态，不得静默启用另一 authority。 | [条款](#storage) | identity / viewer / runtime | 数据库事务、迁移步骤和回退门槛仍由本专业设计定义。 |
| 产品 N/A：reason=P1 原生凭证不是当前受支持 hosted viewer 入口的产品承诺；scope=P1 原生密钥注册、备份与 P0 禁用策略；[PRD-KEYAUTH-011](hosted-public-join-key-based-onboarding.prd.md#prd-keyauth-011) | P1 原生受保护密钥和备份；P0 禁用入口。 | [原生边界](#native) | viewer_engineer | 本次 P0 不开放原生密钥。 |
| 产品 N/A：reason=来源/用途/验证状态元数据属于凭证协议的专业隐私合同，产品层没有对应独立 Trace 要求；scope=credential provenance 与验证元数据；[PRD-KEYAUTH-012](hosted-public-join-key-based-onboarding.prd.md#prd-keyauth-012) | 区分凭证来源、用途、验证状态并最小披露；不能把登录或验证信息暴露给无关 surface。 | [条款](#webauthn) | identity / viewer / runtime | 不宣称已完成隐私审计或真实设备验证。 |
<a id="architecture"></a>
## 3. 总体架构与信任边界
### 3.1 组件关系
以下为目标逻辑关系，不代表新增六个独立部署服务。

```text
玩家浏览器 / P1 原生客户端
  │  WebAuthn 响应 / 邮箱证明 / 原生挑战签名
  ▼
稳定公共入口：同源 Viewer + 身份 API + 世界接入代理
  │
  ├─ HostedIdentityService
  │    ├─ WebAuthnAdapter / EmailAdapter / NativeKeyAdapter(P1)
  │    ├─ CeremonyService / AccountPolicy / RecoveryService
  │    └─ IdentityStoreV2 + AuditOutbox + QuotaStore
  │           ├─ file: 本地 SQLite
  │           └─ tablestore: 同身份分区本地事务
  │
  ├─ AccountSessionService —— 账号登录态，不占世界名额
  │
  └─ HostedWorldSessionAdapter
       ├─ 既有资格 / 邀请 authority
       ├─ 既有玩家映射、slot 与 registration grant
       └─ RuntimeSessionPolicy + 授权新鲜度检查

资产 custody / node / governance / private control
  └─ 独立权限和签名系统；不由上述登录成功自动开放
```

P0 将上述模块放在现有 launcher 的可复用 Rust 模块中。只有确定存在独立部署需求时才抽出进程；不得为了组织代码先引入网络 hop、独立数据库和第二套账号主键。
### 3.2 单一主责
| 事实 | 唯一主责 | 消费方式 |
| --- | --- | --- |
| 凭证归属、账号状态、安全版本 | IdentityStoreV2 | 认证和 runtime 授权检查只读权威结果，不信任浏览器自报 |
| 玩家在某世界的身份 | 既有玩家注册/映射 authority | 本设计建立稳定关联，不重新定义世界身份作用域 |
| 邀请资格与赠送预算 | 既有 admission / 资源 authority | 账号注册与资格授予分别幂等；不新增影子权益账本 |
| 当前玩家控制 key、世界 session epoch | runtime / 既有 session authority | 通过版本化 registration grant 验证并接管 |
| 资产、创作者高风险动作 | 对应 signer / policy authority | 消费认证来源，但不能将其等同于资产签名 |
| UI 阶段状态 | Viewer 的服务端投影 | UI 不创造账号、撤销或世界提交真值 |
### 3.3 公共入口与多节点
P0 用稳定的同源入口承载账号页和认证 API；世界节点在反向代理后切换。`RP ID`、`allowed_origins`、`identity_service_id`、`environment_id` 固定配置，不从 Host、转发头、world URL 或用户参数推导。

示例域名均为不可直接部署的占位：`https://play.example.invalid`。正式值必须在开放第一个真实 Passkey 前固定。不同环境使用不同 RP ID 和存储命名空间，不将 production RP 设为所有 staging 子域都能借用的宽泛父域。

WebAuthn 的凭证有 RP/来源作用域；本方案仅在标准边界内使用，并不承诺任意世界节点网址都可直接认证。[W1] P0 不做跨站自动登录跳转；独立站点接入须另行采用经过验证的授权码交接方案，不把完整 cookie、私钥或长期游戏令牌放进 URL。

<a id="model"></a>
## 4. 身份、凭证与数据模型
### 4.1 标识与命名空间
`IdentityNamespace = (identity_service_id, environment_id)`；RP 配置及版本保存在该命名空间的元数据中。数据库发现身份域或 RP 不匹配时拒绝启动认证写入，不自动初始化一个空库。

保留原 `hosted_account_id` 与 `player_id` 的格式和数值。新账号由服务端分配新的不可混淆标识；若继续旧序列，必须在事务中分配。WebAuthn 使用独立随机 UUID 的 16 字节值作为 `userHandle`，不由邮箱、昵称或公钥派生；该 UUID 为账号长期字段，凭证轮换不变。

临时注册 attempt 可预分配账号 ID / userHandle，但只有注册事务提交才产生有效账号。空洞序列不是错误，不能为了补号复用被放弃的标识。
### 4.2 核心记录
以下是实现合同字段，不是向浏览器全量开放的 JSON。
| 记录 | 必要字段 | 约束 |
| --- | --- | --- |
| `AccountRecordV2` | `account_id`、`user_handle`、`status`、`row_version`、`policy_version`、`security_epoch`、`has_ever_public_key`、`created_at`、`last_authenticated_at`、`last_recovery_id` | `has_ever_public_key` 只可由 false 变 true；认证不能解冻；security epoch 只能递增。 |
| `PlayerBinding` | `account_id`、`world_identity_scope`、`player_id`、`source`、`binding_version` | 导入旧映射时必须保留作用域；不根据当前选中的 world 猜旧 player 归属。 |
| `CredentialRecordV2` | `credential_record_id`、`account_id`、`type`、`state`、`version`、`label`、`created_at`、`last_used_at`、`public_material`、`registered_via` | public_material 为类型联合体；最多 10 个 active 公钥凭证；撤销为终态。 |
| `CredentialLookup` | `scope`、`key_digest`、`canonical_key`、`credential_record_id`、`account_id`、`state` | 唯一行键 + 事务内不存在条件；digest 相同仍比较原始 canonical bytes。 |
| `AccountSession` | `session_id`、`token_digest`、`account_id`、`origin_credential_id`、`origin_credential_version`、`issued_security_epoch`、`auth_method`、`auth_time`、`expires_at`、`idle_expires_at`、`revoked_at`、`recovery_context` | 登出一条 session 不改 CredentialRecord；不能通过更新 auth_method 洗掉原始派生关系。 |
| `CeremonyRecord` | `ceremony_id`、`kind`、`namespace`、`rp_config_version`、`owner_context_digest`、`target_account_id?`、`target_digest?`、`expires_at`、`failed_attempts`、`status`、`server_state`、`result_ref?` | server_state 只在服务端；32 字节随机挑战、120 秒 TTL、最多 5 次无效响应。 |
| `SensitiveAction` | `action_id`、`account_id`、`session_id`、`action_type`、`target_digest`、`account_policy_version`、`proof_credential_id`、`proof_time`、`expires_at`、`state` | 新鲜证明绑定具体动作；5 分钟窗口不是通用全权限令牌。 |
| `RecoveryRequest` | `recovery_id`、`account_id`、`email_credential_id/version`、`candidate_credential_id/digest`、`account_policy_version_at_start`、`security_epoch_at_start`、`created_at`、`not_before`、`expires_at`、`state`、`approved_by?`、`result_ref?` | 同账号最多一个活跃申请；候选 key、邮箱版本与目标账号不可偷换。 |
| `OperationReceipt` | `operation_id`、`kind`、`owner_context_digest`、`request_digest`、`state`、`result_ids`、`committed_at` | 只存非秘密结果；幂等查询不重新签发 session token。 |
| `OutboxEvent` | `event_id`、`account_id`、`event_type`、`state_version`、`safe_payload`、`attempts`、`delivery_state` | 与安全变更同事务写入；重复投递由 event_id 去重。 |
安全中心列表使用 `credential_record_id`，不得拿 WebAuthn raw credential ID 当公开路由标识。凭证内部引用也不公开跨账号可枚举的集合。
### 4.3 凭证类型联合体
```text
CredentialPublicMaterial =
  WebAuthn {
    rp_id, credential_id_bytes, user_handle,
    cose_public_key, cose_algorithm,
    sign_count, backup_eligible, backup_state,
    transports_hint, library_record_version
  }
  | Email {
    normalized_address_encrypted,
    lookup_hmac, normalization_version,
    verified_at, bound_at,
    recovery_enabled, recovery_eligible_since
  }
  | NativeEd25519(P1) {
    public_key_bytes, key_fingerprint,
    identity_service_id, environment_id, protocol_version
  }
```

Passkey 不限定为 Ed25519，不能转换后强塞进现有游戏消息验签器。`transports_hint`、凭证标签、User-Agent 都是展示或选择提示，不是额外身份真实性证明。库导出的序列化记录必须带 schema / library-format 版本，升级先做旧记录解析回归。

WebAuthn lookup 以 `(namespace, rp_id, credential_id_bytes)` 唯一；原生凭证以 `(namespace, algorithm, public_key_bytes)` 唯一。新 Passkey 不根据同一个 `userHandle` 就跳过 credential ID 和公钥核对。
### 4.4 邮箱规范化与释放
迁移必须沿用现有规范化结果，不在同批变更擅自删除 `+tag`、合并点号或改变历史邮箱等价规则。新增规则需有独立版本和冲突分析。邮箱索引用服务端带版本的 HMAC，原地址加密保存；普通摘要 hash 不能被宣传为阻止邮箱字典枚举。

首版一个账号最多一个 active 邮箱因子。替换时验证旧账号权限与新邮箱，事务内停用旧绑定并建立新绑定；旧恢复申请失效，新邮箱的 24 小时成熟期重新计时。移除的邮箱可以在新的 OTP 验证和唯一性检查后重新登记，但不能凭旧 OTP 或旧 lookup 直接接回原账号。

公钥撤销墓碑与邮箱可重新验证绑定的生命周期不同，不能用统一“删除索引即可复用”实现两者。

没有绑定邮箱的账号向下游输出 `email=null` 或不包含该字段，不生成伪造邮箱补齐旧接口。昵称、模型服务关联和玩家映射以稳定账号标识对接；下游可选服务若仍强制邮箱，须修正其合同或明确局部不可用，不能反向阻断已承诺的普通试玩与公钥登录。

<a id="storage"></a>
## 5. 存储、事务与幂等
### 5.1 统一原子接口
IdentityCore 只依赖类型化的 `IdentityStoreV2`。事务接口至少支持：按键读取、读取版本校验、条件创建、条件更新、删除、范围分页，以及**单个身份事务的全成全败**。事务返回 `Committed / Conflict / Rejected / OutcomeUnknown`，不把网络超时统一映射为失败。

关键原子单位：
| 事务 | 必须一起提交的内容 |
| --- | --- |
| `RegisterAccount` | 挑战消费、账号、userHandle 索引、首凭证、唯一 lookup、首个账号 session、注册计数消费、operation receipt、审计事件 |
| `AuthenticateAccount` | 挑战消费、账号/凭证最新状态核对、计数/最后使用信息、一次新账号 session、receipt |
| `BindCredential` | action proof 消费、账号版本核对、新凭证、唯一 lookup、`has_ever_public_key=true`、receipt/outbox |
| `RevokeCredential` | 保留凭证证明与版本核对、目标 credential 撤销/version 增加、action 消费、receipt/outbox |
| `ReplaceEmail` | 新邮箱证明消费、旧绑定撤销、新绑定与 lookup、恢复申请失效、action 消费、receipt/outbox |
| `CompleteRecovery` | 恢复条件复核、最终 OTP 和新 key 证明消费、新凭证激活、旧公钥撤销、security_epoch 增加、申请完成、新恢复后 session、receipt/outbox |
| `RevokeAccountSession` | 目标 session 标记撤销、授权证明消费（适用时）、receipt/outbox |
这些事务不包含 SMTP 网络发送、WebAuthn 密码学计算、runtime RPC、节点调用或世界提交。先做有界的协议验证，再在短事务内重新读取所有安全条件并提交；不能用事务外读取的“active”状态覆盖刚完成的撤销。
### 5.2 file v2：SQLite，而不是手写多文件事务
保留 `OASIS7_HOSTED_ACCOUNT_STORE_BACKEND=file` 作为“本地身份存储”的部署选择，新增显式 v2 数据库路径。旧 JSON 路径只作为迁移输入，不把同名 `.json` 静默改写成 SQLite 字节。

采用 `rusqlite` 或仓库最终统一的 SQLite adapter，独立有界 blocking worker 执行；不在 async executor 上直接做同步磁盘 IO。推荐 `WAL + synchronous=FULL`，提交以数据库 COMMIT 成功为准，不先更新进程缓存。SQLite WAL 的多连接共享要求同主机，不能把文件放到 NFS 并据此宣称多机共享身份。[W5][W6]

可使用以下最小记录表统一映射两种后端；领域验证仍由类型化 Rust 代码负责，不向外开放任意 JSON 写入：

```sql
CREATE TABLE identity_records (
    namespace   TEXT NOT NULL,
    record_key  TEXT NOT NULL,
    kind        TEXT NOT NULL,
    account_id  TEXT,
    row_version INTEGER NOT NULL,
    expires_at  INTEGER,
    payload     BLOB NOT NULL,
    PRIMARY KEY (namespace, record_key)
);
CREATE INDEX identity_by_account
    ON identity_records(namespace, account_id, kind, record_key);
CREATE INDEX identity_by_expiry
    ON identity_records(namespace, expires_at)
    WHERE expires_at IS NOT NULL;
```

写事务用 `BEGIN IMMEDIATE`，读取目标版本后条件变更；忙冲突有总截止时间，不无限排队。P0 的 file 部署只支持一个逻辑身份 authority，多个世界通过它访问，不支持每个节点复制一份可写 SQLite。

备份使用 SQLite 一致性备份能力或停写后的完整快照，不能在运行中只复制主 `.sqlite` 而漏掉 WAL。上线锁定的 SQLite 必须包含官方已发布的 WAL-reset 修复（官方列出 3.51.3 及相关回补版本）；不能只根据 Rust wrapper 版本认为底层已修复。[W5]
### 5.3 Tablestore v2：明确事务能力，不假定 BatchWrite 原子
新表使用 `namespace` 作为第一主键、`record_key` 作为第二主键。P0 一个身份命名空间的账号、lookup、challenge、recovery、session、限流和 outbox 记录放在**同一分区键值**，事务涉及的行必须全部属于该分区。

Tablestore 官方本地事务限定同一分区键值，且需要表具备相应能力，不能假设现有表自动支持；自动递增主键能力也不能与本地事务混用。[W4] 本设计不把普通 BatchWrite 或多个条件 PutRow 当作事务。

实现需补齐 `StartLocalTransaction`、带 transaction_id 的读写、`CommitTransaction`、`AbortTransaction` 适配，并以隔离环境真实调用验证。现有 `aliyun_tablestore_rs` 代码只核对到普通读写路径，本次没有证明该 Rust SDK 已封装全部本地事务接口；缺失时在该依赖适配层补齐官方协议调用，不引入“先写账号、再写凭证、失败以后补”的不安全替代。[R2][W4]

使用新表显式建档并探测事务能力。认证服务启动时遇到无事务能力、错误 namespace、过旧 schema 或缺失安全配置，应拒绝开放相关写接口，保留明确健康诊断；不得自动切到 file 或另一张空表。

**容量边界：** 同一身份分区写事务存在串行化压力，这是以受控测试的简单正确性换取扩展边界。本文只以 PRD 的 10,000 账号、30,000 凭证、5 次/秒、20 并发目标验证，不声明无限多节点身份写扩展。超出范围须先验证或另做分片唯一性设计，不能直接把 `namespace` 随机拆分。
### 5.4 提交结果未知的处理
每个变更携带服务端签发/认可的 `operation_id`，并把 `OperationReceipt` 与业务写入放在同一事务。遇到提交响应丢失：

1. 保留原操作 ID，不开始新开户、不重用旧签名创建新 session。
2. 在同身份命名空间建立新的串行化事务，读取 receipt。前一事务尚未决时允许返回 `operation_pending`，不把一次“未找到”当成未提交证明。
3. receipt 已存在且请求摘要相同：返回同一非秘密结果；摘要不同：返回 `idempotency_conflict`。
4. 只有确认前一事务已终止且未提交，才可在原挑战仍有效时继续同一操作；过期则重新认证。

`Set-Cookie` 中的新 session token 只在首次成功认证响应发送。响应丢失且浏览器没有拿到 cookie 时，状态查询仅能确认“账号已建立/操作已完成”，随后用新鲜凭证登录；不保存一份长期可重复领取的明文 session token 作为幂等回执。
### 5.5 后端约束与清理
两种后端运行同一 store contract suite，不能只测试内存 fake。未完成挑战过期即失效，数据清理延迟不延长权限；已成功的 operation receipt 建议保留 7 天便于解释重试。WebAuthn server_state 在消费后立即去掉，只留下结果引用与摘要。

公钥撤销索引保留最小墓碑用于拒绝重新激活；其他安全审计依 PRD 保留 90 天，运行诊断 30 天，完整 IP 最多 7 天。账号注销与法定删除的细则不由本次定义，永久保存个人数据不是默认前提。

<a id="webauthn"></a>
## 6. WebAuthn 协议与验证边界
### 6.1 验证库与配置
当前候选已将 `webauthn-rs-core = 0.5.5` 纳入 Cargo.lock，通过 `WebAuthnAdapter` 使用同库的配对注册/认证状态；高层 Passkey API 的 resident 要求是机会性提示，故采用 core builder 明确要求 resident 与 UV，discoverable 登录使用空 allowlist 后注入权威凭证。没有只修改浏览器 JSON、手写密码验证或启用 conditional mediation 的旁路。[W2][W3] 已加入依赖不等于完成真实设备或全部跨平台验收。

`WebAuthnAdapter` 包装库 API，不向业务模块泄露不受版本控制的库内部结构。必须实测默认注册选项是否真正满足可发现凭证目标；不能看到库名包含 Passkey 就认为 resident credential 一定创建成功。若高层 API 无法一致配置并验证 resident 要求，使用同库经过审查的 core 适配能力，不仅修改发给浏览器的 JSON 而让服务端保留另一套状态。
| 参数 | 本设计要求 |
| --- | --- |
| `rp.id` | 固定配置的稳定身份域；精确匹配登记的配置版本 |
| `user.id` | 账号稳定随机 UUID bytes；不是邮箱或 public key hash |
| `user.name/displayName` | 系统分配的非敏感显示名称；不参与身份验证 |
| `residentKey` | `required`；同时为兼容环境设置相应 resident 要求 |
| `userVerification` | `required`；PIN、系统解锁等可满足，不能只认指纹 |
| `authenticatorAttachment` | 不强制 platform；允许满足条件的安全密钥和跨设备认证器 |
| `attestation` | `none`；不以未要求的硬件证明宣称设备真实型号或不可复制 |
| `pubKeyCredParams` | 优先 ES256，兼容允许列表可含 RS256；只接受实际验证库支持且已测试的算法 |
| `excludeCredentials` | 已认证账号登记备用凭证时填入现有凭证；公开建号不泄露其他账号列表 |
| discoverable 登录 | 不让用户填账号；`allowCredentials` 留空，由返回的凭证查找后完整验证 |
| `timeout` | 客户端提示 120 秒；服务端独立执行 120 秒过期，不能由客户端延长 |
浏览器支持检测不能仅靠 `isUserVerifyingPlatformAuthenticatorAvailable()`：返回 false 不代表外部安全密钥或另一台设备一定不可用。P0 由明确点击触发认证，不依赖自动填充或 conditional mediation 才能登录；库的 feature 名不等于必须在 UI 自动唤起。
### 6.2 认证仪式状态
```text
pending ──有效响应+事务提交──> consumed(result_ref)
   ├────无效响应──────────> pending(failed_attempts + 1)
   ├────达到失败上限──────> exhausted
   ├────到期──────────────> expired
   └────服务端确认取消────> cancelled
```

状态转移需条件写/事务保护。验签在事务外可重做，**权限效果不能重做**。事务提交前再次核对 pending、期限、失败上限、目标账号、安全版本及敏感操作目标。并发请求一个提交成功，其他只能读同一结果或被拒绝。

`server_state` 包含库生成的完整配对验证状态，不把服务器期待的 challenge/origin/验证策略交给客户端回传决定。存储层对状态设保留时间和访问权限；不在日志里输出 challenge response 或库 state 的 Debug。

未登录仪式绑定随机 32 字节 pre-auth 浏览器上下文 cookie，服务端只保存其摘要。已登录敏感仪式同时绑定当前 account session。Origin 校验、CSRF 防护、用户意图和 challenge binding 共同防护登录 CSRF；有正确签名并不自动证明它是当前浏览器发起的登录。
### 6.3 必须覆盖的标准验证
完整标准验证委托库完成，集成层负责执行部署和业务约束，不能只拿一段签名调用 Ed25519 verifier。WebAuthn 标准分别定义注册与认证校验，并要求依赖方处理 challenge、来源、RP、用户验证及凭证记录等条件。[W1]
| 类别 | 集成必须证明的条件 |
| --- | --- |
| 解码 | 限定请求大小；二进制字段严格 base64url 解码；拒绝畸形 COSE/CBOR、重复或冲突字段 |
| 注册类型 | `webauthn.create`、正确 challenge/origin/RP、所需用户验证与支持算法 |
| 登录类型 | `webauthn.get`、正确 challenge/origin/RP、所需用户验证与签名 |
| 跨源 | P0 只接受顶层同源路径；拒绝意外 `crossOrigin` / topOrigin 组合，不为内嵌浏览器关闭验证 |
| 凭证归属 | raw ID、存储 credential、userHandle 与账号对应；查到记录不等于认证通过 |
| 生命周期 | 账号未冻结、credential active、版本匹配；提交时重查，避免撤销竞态 |
| counter | 保存库返回的计数与状态更新；异常进入风险处理，不用 counter 代替 challenge 一次性消费 |
| 同步凭证 | 保存可用的 backup flags 以解释风险；不将“已同步”宣称为项目保证可恢复 |
| resident 能力 | 能执行不输入账号的 discoverable 登录；不能只依赖可被客户端伪造的扩展提示证明兼容性 |
普通注册的 `attestation=none` 不提供硬件背书。本设计不把一次无 attestation 的 create 响应夸大成可信硬件证明；账号绑定仍由服务端仪式、现有账号证明与后续真实认证约束。恢复候选另要求最终新凭证 assertion，见 §9。

计数异常可能涉及并发与同步行为。实现应保留库安全检查；不可为了“兼容同步”全局忽略错误。已识别的异常可拒绝本次登录并要求其他凭证/恢复，不能仅因一次计数异常永久冻结整个账号。该策略需在同步凭证回归中核验。

<a id="auth-flows"></a>
## 7. 注册、登录、幂等与中断恢复
### 7.1 新账号注册
```text
浏览器               IdentityService              IdentityStoreV2
  │ capabilities          │                              │
  │ register/options ────>│ 创建配对状态 ─────────────────>│ pending
  │<── options/attempt ───│                              │
  │ 系统 create()         │                              │
  │ register/finish ─────>│ 库验证 + 业务检查              │
  │                       │ RegisterAccount 事务 ────────>│
  │                       │ account/credential/session   │ COMMIT
  │<── account_created ──│ + Set-Cookie                  │
  │                       │                              │
  │ world-session 请求 ──>│ 资格/容量/绑定独立处理         │
```

`register/options` 只建立短期 attempt，不占 world slot，不发免费额度，也不建立可用账号。`register/finish` 在已验证的 server-state 约束下创建账号、凭证和账号 session；响应中的 `world_state=not_requested` 不得写成 `runtime_bound`。

相同 attempt、相同有效响应的重试：只产生一次账号和一次 session。不同 attempt、不同且分别经用户确认的新凭证，不依据 IP、设备指纹或昵称自动合并；产品 UI 应提醒“已有账号请登录”，但不能推断两次注册必属同一真人。

若凭证已在认证器创建而服务端未提交，保留本页内的响应用于有效期内重试；不写入分析事件或持久化任意裸响应。页面刷新导致材料丢失时先查询原 attempt；已提交可重新登录，未提交且无法恢复则重新创建，提示认证器中可能存在未登记条目。不得自动调用系统删除凭证接口删除用户其他凭证。
### 7.2 无用户名登录
`authenticate/options` 建立绑定浏览器上下文的 discoverable 仪式，不返回全站 credential ID 列表。服务端收到响应后，以 userHandle/credential ID 查候选记录，但只在完整库验证及状态复核成功后确定账号并创建新登录会话。

已登录浏览器选择“切换账号”是明确动作；普通添加凭证不能转成新账号登录。跨标签页用前端 generation 防止旧请求覆盖 UI；服务端将 attempt 绑定发起 session/浏览器上下文，涉及账号切换时复核用户显式选择。收到成功响应后重新读取 `/session` 确认当前账号，再展示世界归属，不仅相信过期响应体。

查不到凭证、userHandle 不匹配、已撤销及签名无效，向未认证者返回统一的 `authentication_failed`，内部审计记录具体原因。已认证用户在自己的安全中心可以看到其凭证的撤销原因；不能以“错误可解释”为由公开别人的账号存在性。
### 7.3 普通邮箱登录兼容
邮箱兼容指既有邮箱账号可通过 v2 `/api/public/hosted-account/v2/email/login/options` 与 `/api/public/hosted-account/v2/email/login/complete` 重新认证，内部走 `EmailAdapter → 身份事务 → AccountSession`，不是保留第二套旧登录 authority。启用 v2 时，旧 `/api/public/hosted-account/login/start` 与 `/api/public/hosted-account/login/complete` 返回 `identity_v2_required`；旧接口仅限 v2 未开启且满足 §13.4 安全回退条件的部署，完整路由边界见 §11.2。

OTP 必须绑定 `purpose`（登录、绑定邮箱、恢复开始、恢复完成）、因子/版本、目标账号（适用时）和发起上下文。一个登录 OTP 不可拿去完成恢复或改变邮箱。OTP 存储采用带秘密的验证摘要，不能对低熵验证码仅做公开 hash 后声称抗离线猜测；发送与验证材料不入日志。

邮箱账号不存在且允许邮箱注册时，验证后走统一开户事务；关闭注册但已有邮箱账号时仍可登录。外部未验证响应不暴露该地址是否建号。邮件不可用时 capabilities 标明 email unavailable；WebAuthn 不依赖 SMTP 初始化成功。

**关键兼容修正：** 世界满员或 session 签发失败不再把已通过的 OTP 放回待处理集合；返回“已登录，尚未进入世界”，后续依账号 session 重试准入。[R1]
### 7.4 认证结果查询不等于授权重放
`operation/status` 仅允许原 pre-auth 上下文或目标账号持有人读取，返回阶段、公开错误、已完成结果引用与安全下一步，不返回 session token、完整邮箱或原证明。跨浏览器持有 attempt ID 不足以领取结果。

无论 WebAuthn、邮箱还是原生登录，旧证明都不能在新浏览器、其他 session 或另一个目标世界换取全新认证。需要新权限时必须执行新的有效仪式。

<a id="credential-management"></a>
## 8. 凭证管理、重新认证与邮箱绑定
### 8.1 操作授权而非一个“刚登录过”布尔值
敏感动作先创建 `SensitiveAction`，绑定当前账号、session、动作类型、目标摘要、账号版本和到期时间，再用已有凭证证明授权。即使当前 session 的 auth_time 在 5 分钟内，也不能自动把它变成任意目标的修改权。

**P0 每个敏感动作都要求一个绑定该动作的新断言**；5 分钟窗口只允许完成已经明确授权的同一个动作，不允许从旧登录证明衍生新的修改目标。提交时仍检查用户确认、CSRF、目标摘要和安全策略版本。后续减少重复弹窗属于单独优化，未经安全验证不复用通用“刚认证过”状态。

授权产生与消费是服务端事实。`action_token` 若作为客户端引用，必须是高熵不可猜值，绑定当前 session、短期且一次性；不能作为 URL 查询参数、写入日志或当跨设备万能恢复链接。
### 8.2 允许路径
| 情况 | 允许证明 | 额外检查 |
| --- | --- | --- |
| 旧邮箱账号首次添加公钥 | 新近验证的原邮箱 | `has_ever_public_key=false` 在提交时仍成立 |
| 已有公钥账号添加另一个公钥 | 已有 active 公钥新鲜认证 | 新凭证标准登记、唯一性、active 数量 < 10 |
| 撤销公钥 | 另一个将保留的 active 公钥新鲜认证 | proof credential 不等于删除目标；提交后至少一个 active 公钥 |
| 添加/替换/解绑邮箱 | 已有公钥新鲜认证；纯邮箱旧账号按其受支持路径 | 新邮箱验证码、唯一性、取消依赖旧邮箱的恢复申请 |
| 退出当前账号 session | 当前 session | 只影响该 session 及其 world 会话 |
| 退出其他账号 session | 公钥新鲜认证；从未有公钥的旧账号可用邮箱 | 保留本次明确选择的 session，不误删凭证 |
| 公钥全丢、用邮箱替换 | §9 完整恢复 | 不能借“active 公钥数为 0”回到旧邮箱首次升级分支 |
两个标签页同时撤销不同凭证时，保留凭证证明和最终数量必须在同一事务重新核对；禁止各自根据事务外的“两把 key”判断都可删除。限制计数由 store 中 active 记录判定，不信任浏览器上传数量。
### 8.3 变更与取消竞态
凭证添加、邮箱变更、恢复和撤销都绑定预期 `account.policy_version` 以及相关 credential version。安全状态冲突返回 `state_changed`，重新读取后由用户决定下一步，不在后台悄悄将证明重定向到新的目标。

`row_version` 是数据库并发写版本，`policy_version` 是安全设置版本。普通登录的最后使用时间、计数等更新不能仅因 row_version 变化使等待中的恢复申请失效；提交事务重新读取并合并这类观测字段。凭证、邮箱、恢复策略变更会递增 policy_version，并使依赖旧安全状态的 action 或恢复申请失效；恢复流程本身的合法状态转换按申请版本更新，不自我取消。普通凭证变更不必自动撤销所有正常账号 session；恢复完成和紧急冻结另递增 `security_epoch`，形成账号级失效屏障。

邮箱绑定页面明确区分通知信息与登录/恢复能力。此 PRD 选择的是“启用邮箱登录与恢复”，只有完成明确授权和验证才激活；任意资料表单填入地址不能顺带增加登录权限。

<a id="recovery"></a>
## 9. 邮箱恢复与原子凭证替换
### 9.1 信任边界与产品澄清
预先启用邮箱恢复意味着信任邮箱可在受限流程后替换登录凭证。等待与通知降低部分风险，不能阻止掌握邮箱且未被发现的攻击者最终完成恢复；邮箱登录本身也能进入普通 gameplay，这是配对 PRD 的选择，而非等价于原公钥持有证明。

为使“批准恢复”对应确定的新 key，系统在**恢复申请建立前预登记候选凭证**，待最后提交才激活。配对 PRD 的 Flow-KEYAUTH-006 已区分候选预登记、最终持有证明与激活；两份文档共同约束“不先审批不确定目标，也不在最后任意换 key”。

候选登记只产生 `recovery_candidate`，不能登录、取得 gameplay 或修改账号。每个账号只允许一个活跃恢复候选，不计入 10 个 active 公钥上限，但受恢复频率/大小保护。
### 9.2 流程
| 阶段 | 服务端动作 | 用户可见含义 |
| --- | --- | --- |
| `eligibility` | 以已登记邮箱重新验证；检查 bound/eligible_since 满 24 小时、未冻结、邮箱版本匹配 | 确认具备申请资格，尚未替换账号 |
| `candidate_prepare` | 给原 userHandle 创建 `recovery_candidate` 仪式；验证新凭证登记响应并固定 candidate digest | 系统已保存将来要使用的新凭证，但它尚不能登录 |
| `waiting` | 事务创建申请、唯一活跃申请索引、通知 outbox；记录 created/not_before/expires | 显示申请编号、候选标签/指纹、最早可完成时间 |
| `approved` | 原有效公钥持有人对特定 recovery_id/candidate_digest 新鲜授权 | 可以提前结束等待，但不代表已完成最终验证 |
| `cancelled` | 原有效公钥持有人明确取消，或已授权申请人放弃；候选失效 | 已取消申请不能重新到期生效 |
| `finish_verification` | 再验同一邮箱；为已固定候选发起新的 WebAuthn assertion，验证实际持有新 key | 重新确认邮箱和将接管账号的凭证 |
| `completed` | 单事务替换凭证、递增安全版本、记录恢复来源、失效旧会话、创建恢复后 session | 仍是同一账号；旧登录凭证已失效 |
| `expired/blocked` | 超过创建后 7 天，或冻结/争议/邮箱/账号版本失配 | 无自动接管，需要明确重新处理 |
默认等待不少于 24 小时；首版采用 `not_before=max(created_at+24h, notification_accepted_at+24h)`，除非原有效公钥明确批准。这里的 SMTP accepted 只表示发送服务接受，不代表已送达或真正持有人已阅读。通知长期失败时不走未批准的自动完成路径；7 天总期限不因重试被延长。

申请建立后，不允许更换 candidate、目标邮箱或目标账号。需要变更时取消旧申请并重建，重新满足成熟期/等待期等规则。申请人可以重新验证同一邮箱恢复申请上下文，但不能因此更换候选；丢失候选 key 则取消重来。
### 9.3 完成事务的判定顺序
1. 读取申请及账号，核对 active、namespace、状态、created/expires/not_before 和服务器时间可信状态。
2. 校验恢复邮箱仍是同一 credential/version、恢复已启用且成熟期有效；新旧绑定变动不能被原申请忽略。
3. 校验批准（如用来提前完成）明确绑定相同候选和申请版本；批准者当时证明有效且申请未被取消。
4. 校验新的 `recovery_complete` OTP、候选 key assertion、新鲜挑战、申请归属及安全版本。
5. 在同一事务中原子消费最终证明，激活候选，撤销旧 active 公钥并增加其版本，`security_epoch += 1`，标记申请 completed，写 receipt/outbox。
6. 所有旧账号 session 因旧 security_epoch 逻辑失效，无须逐个同步扫全表才能建立安全屏障；索引状态可后续整理。
7. 可以在同事务建立新 epoch 下的恢复后 session，`auth_method=webauthn`，但保留 `recovery_context=email_recovery:<id>`；不修改资产 signer。

候选 key 已预留的唯一 lookup 只能由同 recovery_id 激活；一个被取消或过期的候选不能直接登录。取消申请与完成申请同时到达，必须只有一个终态，后来的请求读到已取消或已完成；不能先返成功再异步撤销旧 key。

恢复后发送 outbox 通知是提交后重试任务；发送失败不回滚已经完成的 credential replacement，也不恢复旧 session。UI 和审计能区分“恢复已完成，通知待重试”。
### 9.4 防止时间与历史状态被绕过
恢复时间由可信服务器 UTC 时钟判断，受时钟健康探测约束；不能用客户端时间或倒计时结束事件授权。时钟大幅跳变、节点间偏差超出配置上限时暂停自动完成和过期敏感判断，保持需人工/有效旧凭证确认的阻断状态。

导入历史邮箱时若没有可信 bound_at，不将账号 created_at 猜成恢复成熟时间；以导入时刻开始恢复成熟期，不影响其即时普通邮箱登录或纯邮箱账号添加第一把公钥。

冻结不由成功 OTP 解开。管理人员没有“拿到截图就换 key”的接口；账号保护性冻结和后续处置继续依现有受限运营权限，不能由公钥登录新建 private-control 能力。

<a id="world-sessions"></a>
## 10. 账号会话、世界会话、准入与撤销传播
### 10.1 两层 session，不扩张世界控制语义
新增 `AccountSession` 表示“这个浏览器已登录账号”，不占 world slot。既有 `device_session_id` 继续表示一条 world registration/control 会话；额外记录它的 `parent_account_session_id`、来源 credential 与安全版本。不能把两个 ID 重新取名后仍当成同一个对象。

```text
Account A
  ├─ Credential P：同步 Passkey
  │   ├─ AccountSession S1：电脑
  │   │    └─ WorldSession W1：world-X 当前控制者
  │   └─ AccountSession S2：手机（已登录，尚未接管控制）
  └─ Credential Q：备用安全密钥
      └─ AccountSession S3
```

退出 S2 不影响 P，也不终止 W1；撤销 P 会使 S1/S2 及 W1 失效，但不撤销 Q/S3。恢复完成则通过账号 security_epoch 同时失效所有旧 session。原生备份复制同一 key 不产生新独立凭证，也不能独立撤销其某个副本。
### 10.2 浏览器 session 载体
P0 账号登录使用随机 32 字节 opaque token，放入 `__Host-oasis7_sid` cookie：`Secure; HttpOnly; Path=/; SameSite=Lax`，不设 Domain。服务端保存带 key-id 的 token 摘要；日志只记内部 session_id。cookie 和 CSRF token 的机制不能被误表述为消灭 XSS 风险，HttpOnly 主要阻止脚本直接读取 token。[W7][W8]

本设计新增建议值：账号 session 绝对最长 30 天、空闲最长 7 天；敏感操作新鲜认证窗口仍为 PRD 的 5 分钟。安全变更、恢复和重新登录需要旋转 token；普通活动可以更新有界的空闲时间，但不能超过绝对期限或撤销状态。

UI 不持久化账号 bearer token；刷新通过 cookie 请求 `/session`。WebSocket 连接必须校验 Origin；world 会话用短期一次性、绑定本页临时 key 的登记授权建立。不得为 WebSocket 方便把账号 token 塞进 query string。

旧 `release_token` 如仍为底层兼容接口所需，由同源服务端会话适配层持有/关联，不在新 UI 的 localStorage 暴露。旧数据导入或旧浏览器刷新不能自动升级成账号管理权限；见 §13。
### 10.3 准入与 slot 的独立操作
`JoinWorldOperation(account_id, world_scope, parent_session, join_attempt_id)` 经服务端认证后，依次读取玩家映射、资格、容量和当前控制会话。玩家 ID 从权威映射取得，不能接受浏览器传来的任意 player_id 替换。

邀请消费在已有资格 authority 内以 `(invitation_id, account_id)` 或其既有等价键幂等提交。身份事务与世界/权益事务不做跨系统假原子：邀请已兑换且绑定失败，用户仍保留资格，重试不会再扣一次；未确认兑换结果时保持同操作查询，不拿新邀请码强行补偿。

slot 是可过期占用，不是账号属性。发出的 pending 会话仍按既有 30 秒 pending-registration 与 120 秒 slot lease 基线处理，不以这些 slot TTL 冒充身份撤销有效期。[R3]
### 10.4 多设备接管与顺序修正
已有控制会话属于另一账号 session 时，第二台设备登录只得到 `authenticated + control_elsewhere`，可以管理安全设置，或明确选择“在此设备继续”。接管是普通世界控制切换，不等于注销其他设备的账号 session。

接管先验证当前登录、资格、目标、期望 world/session epoch、候选临时 key；再通过 world authority 的 compare-and-swap/既有原子提交点完成 key 轮换。**不能复用“先 release 旧 slot、再检查新准入”的顺序。** 新控制建立失败前保持旧控制可用；成功后旧控制被明确标为 replaced，账号 session 仍可登录。[R3][R4]

接管要求用户显式确认和新近有效登录因子；邮箱在本 PRD 中可用于普通游戏登录，因此不能偷偷将“邮箱用户恢复普通游玩”变成必须原公钥。账号管理、资产与跨设备账号 session 撤销仍执行其更强规则。

同一账号 session 的多个标签页仍不能并发创建互相覆盖的临时控制 key：保留原 join attempt；需要换 key 时按受控重连/明确切换处理。迟到的旧 refresh/release 请求必须核对 `device_session_id + world_epoch`，不能释放新接管者的 slot。
### 10.5 版本化身份授权上下文
向 world adapter/runtime 传递以下**新设计**上下文，不能假设现有 grant 已含这些字段：

```text
HostedAuthContextV2 {
  version, identity_service_id, environment_id,
  account_id, account_session_id,
  credential_record_id, credential_version,
  account_security_epoch,
  auth_method, auth_time, recovery_context,
  world_identity_scope, world_id, player_id,
  device_session_id, session_public_key,
  world_session_epoch, capabilities,
  checked_at, issued_at, expires_at, nonce, issuer_key_id
}
```

以服务端专用登记/授权 issuer 签名，使用版本和用途分离的签名字节；验签 allowlist 不从请求指定。绑定 namespace、world、player、短期公钥和 session，防止一个世界的 grant 到另一个世界或另一个 key 使用。

注册 grant 的一次性 nonce ledger 与身份挑战的消费 ledger 仍分别存在：前者防重用世界登记授权，后者防重用登录证明。不能因为有一个 ledger 就删除另一个；它们对应不同 authority 和消费动作。
### 10.6 撤销的端到端时限
身份服务签发基于权威快照的短期授权，最长 30 秒；`expires_at` 由**成功权威读取开始时刻/快照新鲜度**约束，不以本次缓存命中时间重新开始计时。运行节点允许时钟误差最多 5 秒，周期检查最多 5 秒；设计上限约 40 秒，给 PRD 的 60 秒目标留余量。

撤销推送/outbox 只用于加速；断连和漏推也必须因授权新鲜度超期拒绝后续新玩家操作。不能只在 WebSocket 握手验一次，更不能让一次登录得到长期离线 JWT 后等自然过期才撤销。
| 动作 | 权威变更 | Runtime 必须拒绝的后续行为 |
| --- | --- | --- |
| 撤销 credential P | P.state=revoked、version 增加 | 来源为 P 的所有账号 session 及其派生 world session 的新操作和续期 |
| 退出 AccountSession S | S.revoked_at 设置 | S 派生的 world session；不影响其他 session 使用同凭证 |
| 恢复完成 / 账号冻结 | security_epoch 增加，或 status frozen | 全部旧 epoch 会话；不得通过旧 refresh 复活 |
| 单世界控制接管 | world session epoch/key 更新 | 旧控制者在该世界的新控制输入；不等于全账号退出 |
续期必须读取账号、来源凭证和 session 的当前有效状态；action proof 失效不能被旧签名重新生成。IdentityStore 故障时不能借缓存持续续发授权；runtime 在新鲜度耗尽后停止新的受认证玩家动作，允许符合既有权限的 guest observation 和恢复提示。

**不回滚世界历史：** 已接受 intent、已 committed receipt、独立 Agent 授权仍按各自 authority 处理。账号 session 失效不自动撤销离线 Agent 委托，重新登录也不延长它；原结果未知时按原请求核对，不重发新世界动作。

<a id="api"></a>
## 11. HTTP / 内部接口合同
### 11.1 验证映射表
| 上游需求 | 本设计条款 | 独立义务与适用条件 | 准确验证方法 / test / manual source | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264；[REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001) | [条款](#webauthn) | P0 可发现凭证仅作为声明 viewer 入口的认证方式。 | [验收手册](../../testing/manual/hosted-passkey-account.manual.md#contract-suite)；候选 hosted v2 / 隔离测试环境。 | 同候选测试记录 | 真实设备、真实云事务、性能与外部灾备须另验。 |
| Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) | [条款](#auth-flows) | 身份认证失败、中断或会话失效时须沿可解释恢复路径重新认证。 | [验收手册](../../testing/manual/hosted-passkey-account.manual.md#contract-suite)；候选 hosted v2 / 隔离测试环境。 | 同候选测试记录 | 真实设备、真实邮件送达、生产恢复服务须另验。 |
| Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) | [条款](#credential-management) | 撤销或凭证失效后保持 blocked/恢复语义，不复活旧认证 authority。 | [验收手册](../../testing/manual/hosted-passkey-account.manual.md#contract-suite)；候选 hosted v2 / 隔离测试环境。 | 同候选测试记录 | 真实设备、云事务与性能须另验。 |
| [PRD-KEYAUTH-004](hosted-public-join-key-based-onboarding.prd.md#prd-keyauth-004)；产品 N/A：reason=当前产品要求不定义旧邮箱账户的数据迁移策略；scope=旧 hosted account 与 player 映射的持久化迁移。 | [条款](#migration) | 迁移必须保留旧账号/玩家映射且不得按公钥重建。 | [验收手册](../../testing/manual/hosted-passkey-account.manual.md#contract-suite)；候选 hosted v2 / 隔离测试环境。 | 同候选测试记录 | 不代表具体迁移格式、回滚或云端事务已验收。 |
| Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264；[REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001) | [条款](#world-sessions) | 账号创建、世界准入、runtime 绑定分别判定；登录态和 hosted session 不产生新访问模式。 | [验收手册](../../testing/manual/hosted-passkey-account.manual.md#contract-suite)；候选 hosted v2 / 隔离测试环境。 | 同候选测试记录 | 不证明真实设备、真实准入资格或额外控制权限。 |
| Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) | [条款](#recovery) | 恢复只沿固定候选、等待与原子替换流程进行，不恢复已失效 authority。 | [验收手册](../../testing/manual/hosted-passkey-account.manual.md#contract-suite)；候选 hosted v2 / 隔离测试环境。 | 同候选测试记录 | 真实设备、生产恢复服务与外部灾备须另验。 |
| 产品 N/A：reason=资产签名和治理权限由专业权限 authority 管理，当前产品入口要求未授予登录以外的专业权限；scope=资产与治理操作的凭证授权；[PRD-KEYAUTH-008](hosted-public-join-key-based-onboarding.prd.md#prd-keyauth-008) | [条款](#world-sessions) | 登录不能扩张资产及治理授权；认证态不能被解释为 signer、治理或资产控制权。 | N/A; reason=当前候选缺少对登录态执行资产/治理操作的匹配负例; scope=credential-to-asset/governance authorization; owner_role=identity_engineer; evidence_ref=doc/p2p/blockchain/hosted-public-join-key-based-onboarding.design.md#world-sessions; re-evaluate=加入并运行对应权限边界回归时。 | 对应实现任务的 Issue evidence 与冻结候选回归结果。 | 不证明资产 custody、治理 signer 或生产权限隔离。 |
| Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) | [条款](#viewer) | 超时、断连或结果未知时保持原请求身份并先核对，不用 fallback/自动重试伪装完成。 | [契约测试](../../../crates/oasis7_viewer/software_safe_src/viewer_passkey_account_module.test.js)；候选 hosted v2 / 隔离测试环境。 | 同候选测试记录 | 真实设备、服务端运行与外部灾备须另验。 |
| Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) | [条款](#storage) | 认证存储、迁移或回退失败须保留可解释的 blocked/恢复状态，不静默切换 authority。 | [验收手册](../../testing/manual/hosted-passkey-account.manual.md#contract-suite)；候选 hosted v2 / 隔离测试环境。 | 同候选测试记录 | 数据库、云端事务、性能和外部灾备须另验。 |
| 产品 N/A：reason=P1 原生凭证不是当前受支持 hosted viewer 入口的产品承诺；scope=P1 原生密钥注册、备份与 P0 禁用策略；[PRD-KEYAUTH-011](hosted-public-join-key-based-onboarding.prd.md#prd-keyauth-011) | [原生边界](#native) | P1 原生备份不属于 P0 发布。 | N/A: reason=P1原生密钥明确不在P0实现范围; scope=首版WebPasskey; owner_role=viewer_engineer; evidence_ref=doc/p2p/blockchain/hosted-public-join-key-based-onboarding.prd.md#prd-keyauth-011; re-evaluate=启用原生密钥入口时。 | P1 专项验收 | 不证明原生安全存储和备份。 |
| 产品 N/A：reason=凭证来源、用途和验证元数据属于专业隐私合同，当前产品层无对应独立要求；scope=credential provenance 与验证元数据；[PRD-KEYAUTH-012](hosted-public-join-key-based-onboarding.prd.md#prd-keyauth-012) | [条款](#webauthn) | 区分凭证来源、用途、验证状态并最小披露。 | N/A; reason=仓内没有直接覆盖凭证来源/用途/验证元数据隐私边界的精确回归; scope=credential provenance and verification metadata; owner_role=identity_engineer; evidence_ref=doc/p2p/blockchain/hosted-public-join-key-based-onboarding.design.md#webauthn; re-evaluate=增加并运行对应隐私回归时。 | 对应实现任务的 Issue evidence 与冻结候选隐私回归结果。 | 不宣称已完成隐私审计或真实设备验证。 |
### 11.2 路由与版本策略
同 PR P0 候选已将下表账号 API 实现在 `/api/public/hosted-account/v2`。这些是实现接口，仍受 v2 配置、固定 RP/origin、事务后端、邮箱与世界开关约束；存在接口不表示通过真实环境放量验收。启用 v2 时，旧 email `login/start|complete` 及旧 player-session 发行/续期/释放入口拒绝请求并返回 `identity_v2_required`，不提供第二套兼容登录 authority；关闭 v2 的旧部署才保留原接口。

所有写接口默认要求同源、CSRF token、JSON 内容类型、大小限制和目标上下文核对。凭证材料只进入专门认证接口，不通过 URL 传递。GET 仅查询，不消费挑战、取消恢复或修改账号。
| 方法 / 相对路径 | 输入要点 | 输出 / 安全要求 |
| --- | --- | --- |
| `GET /capabilities` | 无认证必填项 | API version、CSRF token、注册开关、Passkey/邮箱/邮箱恢复可用性、native key 关闭；不返回 secrets 或内部存储地址 |
| `POST /passkeys/registration/options` | `intent=new_account|bind`；bind 携带 action_id | ceremony、options、expires；目标账号从受验证上下文取得，不信任自报 account_id |
| `POST /passkeys/registration/complete` | ceremony、标准 registration response、服务端 operation_id | 新账号或绑定凭证结果；恢复候选走独立 recovery 路由 |
| `POST /passkeys/authentication/options` | 显式登录/切换意图 | discoverable options；不要求邮箱，不枚举账号 |
| `POST /passkeys/authentication/complete` | ceremony、标准 assertion、operation_id | 首次提交设置新 cookie；重复提交只返回安全结果 |
| `POST /actions` | action_type、目标内部引用、expected_version | 服务端建立特定敏感动作，不直接执行 |
| `POST /actions/{id}/reauth/options` | 当前账号 session | 绑定该 action 的旧凭证验证选项 |
| `POST /actions/{id}/reauth/complete` | ceremony、标准 assertion、operation_id | 一次性已授权 action；仅指定操作可消费 |
| `POST /actions/{id}/email/options|complete` | 当前账号 session；complete 携带专用 OTP | 只允许从未拥有公钥的旧邮箱账号首次升级；已有/曾有公钥账号不得走弱证明旁路 |
| `POST /ceremonies/{id}/cancel` | 原尝试上下文或归属账号 session | 原子取消未完成仪式；已消费状态如实返回，不能撤销已提交账号或凭证 |
| `GET /session` | cookie | 当前账号摘要、认证方式与 CSRF token；不返回 bearer token，不代签世界控制状态 |
| `POST /logout` | 当前 session | 撤销自身 session 并清 cookie，重复调用幂等 |
| `GET /credentials` | 当前 session、分页 cursor | 本账号登录方式摘要；不返回私钥、完整原始认证记录 |
| `PATCH /credentials/{id}` | label、expected_version | 仅修改标签；不能通过 PATCH 换 key、改 owner 或改 active |
| `POST /credentials/{id}/revoke` | action_id、保留凭证证明、expected_version、operation_id | 撤销结果、affected session 提示、生效上限 |
| `GET /sessions` | 当前 session、分页 cursor | 本账号会话摘要，不把同步凭证当设备；未知设备如实标记 |
| `POST /sessions/{id}/revoke` | action_id（非自身时）、operation_id | 定向退出；不修改 credential |
| `POST /email/login/options|complete` | options 携带 email；complete 携带 otp_id、otp_code、原 operation_id | 可选邮箱登录/注册通过同一 v2 身份事务；邮箱投递未验收不影响 Passkey 通道 |
| `POST /email/binding/options` | action_id、新邮箱、intent | 专用 OTP challenge；发码不等于绑定 |
| `POST /email/binding/complete` | action_id、OTP、operation_id | 原子绑定/替换；恢复资格重新计时 |
| `POST /email/unbind` | credential_id、action_id、expected_version、operation_id=action_id | 保留所需可用公钥后解绑；旧恢复申请失效 |
| `POST /recovery/eligibility/options` | 已绑定邮箱、可选原 recovery_id | 专用重新验证 OTP；发码不授予账号接管权限 |
| `POST /recovery/eligibility` | 专用邮箱重新验证证明 | 受限恢复上下文；不允许新提交邮箱接管旧账号 |
| `POST /recovery/candidate/options|complete` | HttpOnly 恢复上下文；标准 create 响应 | 预登记固定候选；complete 返回后续申请用的 operation_id，不激活登录凭证 |
| `POST /recovery` | 恢复上下文、候选完成返回的 operation_id | 固定目标申请、最早完成时间、过期时间；未知提交继续核对原操作 |
| `GET /recovery/{id}` | 已认证 owner 或有效恢复上下文 | 当前状态；ID 本身不足以读取 |
| `POST /recovery/{id}/approve` | 原有效公钥 action、expected_version、operation_id=action_id | 对固定候选批准；不自动完成 |
| `POST /recovery/{id}/cancel` | 原公钥 action 的 operation_id，或 eligibility 签发的 cancel_operation_id | 原子取消固定候选；无法取消已 committed 恢复 |
| `POST /recovery/{id}/completion/options` | 合法恢复上下文 | 最终专用 OTP 与候选 assertion 的绑定上下文 |
| `POST /recovery/{id}/complete` | 最终 OTP、候选 assertion、operation_id | 原子完成；旧 session 无效；新 session 保留恢复来源 |
| `POST /account/freeze` | 当前有效公钥新鲜证明、明确风险确认、operation_id | 保护性冻结与 epoch 屏障；无凭证者走已授权受限支持路径，不提供弱证明重置 |
| `GET /operations/{id}` | 原尝试上下文或账号 session | 非秘密进度/结果；不凭旧签名领取新 token |
| `POST /world/join` | public_key、服务端格式 join_attempt_id、takeover、可选 expected_epoch | v2 账号映射到世界玩家，显式接管；默认拒绝未配置准入，仅 `open_preview` 可走受控预览 |
| `POST /world/refresh|release` | public_key、device_session_id、expected_epoch | 重查账号/凭证/session 与世界 epoch；迟到释放不能释放新控制者 |
v2 world adapter 使用以上 `/world/*` 路由及版本化身份授权；既有公开 `/api/public/player-session/*` 不承担 v2 兼容授权。`open_preview` 不是正式邀请兑换或资格 authority，正式准入、真实世界首会话与撤销链路仍须独立验收。登录界面和安全中心不直接调用私有 operator 接口。

新增公共路径须逐条登记到 hosted public allowlist；不能为省事放开 `/api/*` 或把所有 account 子路径自动认作匿名可访问。匿名、已登录、action proof 和恢复 proof 在 handler 前分别分类校验。
### 11.3 公共响应格式
```json
{
  "api_version": 2,
  "ok": true,
  "operation_id": "opaque-operation-id",
  "phase": "account_registered",
  "account": {
    "account_id": "opaque-account-id",
    "display_name": "玩家 7K2M"
  },
  "authentication": { "state": "authenticated", "method": "webauthn" },
  "world": { "state": "not_requested" },
  "next_action": "request_world_admission",
  "diagnostic_id": "opaque-diagnostic-id"
}
```

以上只是字段示意，示例标识不是有效 token。不得在响应中加一个笼统 `can_control=true` 覆盖 world、Agent、资产和账号管理所有授权。

失败示例：

```json
{
  "api_version": 2,
  "ok": false,
  "phase": "world_admission",
  "error_code": "world_full",
  "retryable": true,
  "next_action": "retry_world_admission",
  "authentication": { "state": "authenticated" },
  "account_retained": true,
  "diagnostic_id": "opaque-diagnostic-id"
}
```

`retryable=true` 表示允许指定的安全重试，不表示重新发送全部历史操作。未知事务结果只能给 `check_original_operation`，不能返回“重新注册”。
### 11.4 错误码与 HTTP 语义
| HTTP / 错误码 | 适用范围 | 安全下一步 |
| --- | --- | --- |
| 400 `invalid_request` | 格式、长度或无法解析的参数 | 修正输入，不暴露反序列化内部栈 |
| 401 `authentication_failed` | 未认证调用的未知/无效/撤销凭证 | 新鲜认证或使用其他已登记方式；不自动开户 |
| 401 `session_expired` / `session_revoked` | 已知 session 上下文 | 重新认证；不以旧 refresh 换新权限 |
| 403 `reauth_required` | 已登录但无指定 action 的有效证明 | 执行相应重新认证 |
| 403 `account_frozen` | 已有充分身份上下文的被冻结账号 | 显示受限支持/恢复入口；认证不解冻 |
| 409 `credential_conflict` | 已认证绑定冲突 | 保留当前账号；不透露另一 owner |
| 409 `last_credential_protected` | 删除最后有效公钥/唯一邮箱 | 先增加和验证保留凭证 |
| 409 `state_changed` | 账号/凭证/恢复版本变化 | 刷新状态并重新确认，不自动改目标 |
| 409 `control_elsewhere` | 世界已有其他控制者 | 明确选择接管或保持现状 |
| 409 `idempotency_conflict` | 同 operation_id 不同请求 | 停止自动重试，核对原操作 |
| 410 `ceremony_expired` / `recovery_expired` | 原上下文有读取资格 | 获取新挑战或重新建立申请 |
| 429 `rate_limited` | 注册/认证/恢复各自额度 | 指定 Retry-After，区分受影响通道 |
| 503 `identity_store_unavailable` | 存储/事务不可用 | 保持安全失败，不造临时账号 |
| 503 `email_unavailable` | 邮箱相关动作 | 只暂停该通道，公钥登录独立 |
| 202 `operation_pending` | 提交结果仍未确定 | 用原操作 ID 查询；不是成功也不是失败 |
| 403/409 `recovery_not_ready` | 已验证恢复上下文 | 展示 not_before/阻断原因，不自动完成 |
异常详情通过 diagnostic_id 关联受限日志，不回传数据库 endpoint、AK、文件绝对路径、凭证原文或完整邮箱。

<a id="viewer"></a>
## 12. Viewer 状态、刷新与安全存储
### 12.1 分离状态机
前端至少维护四个正交对象：`registrationState`、`accountSessionState`、`worldAdmissionState`、`controlSessionState`。恢复是独立流程，不占用一个通用 `loginStatus` 状态槽。
| 状态组 | 枚举示意 | 不能误读成 |
| --- | --- | --- |
| 注册 | idle / user_prompt / verifying / registered / outcome_unknown / cancelled / failed | registered 不等于已经进入世界 |
| 账号 | guest / authenticated / expired / revoked / frozen | authenticated 不等于新鲜公钥证明 |
| 准入 | not_requested / checking / denied / waiting / admitted | admitted 不等于 runtime 绑定成功 |
| 控制 | not_bound / registering / bound / replaced / revoked / recovery_needed | 页面打开或 WS 已连接不等于可控制 |
账号/会话失效优先阻止新的认证操作，但不抹掉已有原请求结果；世界回执和历史意图视图继续按其授权及已知事实呈现。不同错误不得互相覆盖成“注册失败”。
账号登录以 `/session` 为依据，`credentials` / `sessions` 管理列表独立加载并单独提供失败提示和重试；列表不可用不能阻止认证回调或世界入口。列表报告 session 失效时必须清除账号、列表与页内签名器，并通知失效；不把未知网络故障解释成已退出。每次失败只展示本地阶段、固定接口路径、HTTP 状态、安全错误码及格式受限的诊断编号，不展示响应原文或浏览器异常详情。

原生密钥 options 成功后、调用本地签名前即保存原 ceremony/operation 引用。本地签名失败先由原上下文取消：只有服务端确认 cancelled/expired/exhausted 才释放；已提交返回安全回执并重新核对 session；取消响应丢失或结果未知时保留原引用，重试先核对原操作，不能盲目建立第二个挑战。原生挑战的存储清理期限为证明到期后再保留 10 分钟，允许原上下文查询 expired/cancelled；证明有效期仍为 120 秒，保留不延长授权。上下文丢失或超出保留窗口时如实提示无法确认原结果，不把 authentication_failed 推断成取消成功；超过保留窗口仍无法查询时明确提示刷新页面先确认当前 session、必要时执行新鲜认证，并保留原结果未知的事实，不在当前页面自动分配新挑战。

### 12.2 跨标签页与重新进入
同源多标签页通过 BroadcastChannel 或等价本地通知同步“session_changed / revoked”，但通知不是授权依据；每个标签页收到事件后向服务端核对。前端请求携带本地 generation，旧结果不能覆盖新账号 UI；generation 只是 UI 防竞态，不替代服务端 action/session 绑定。

刷新后先调用 `/session`，再查询目标世界原 join attempt/既有绑定状态。先核对已有结果，必要时注册新页内临时 key；绝不恢复浏览器长期 player privateKey，也不自动重发原世界 intent。
### 12.3 存储白名单
| 位置 | 允许 | 禁止 |
| --- | --- | --- |
| HttpOnly cookie | 短期 pre-auth 上下文、账号 session token | 被返回到 JS、被加入 URL 或分析埋点 |
| JS 内存 | 系统响应的短暂处理材料、CSRF token、页内临时会话 key、进行中请求引用 | 跨页面持久保存标准响应和长期登录私钥 |
| localStorage | 语言、UI 偏好、非秘密目标 world/operation 引用；继续清理旧 privateKey | 账号 bearer、长期 release token、原生私钥、OTP、恢复完成证明 |
| 系统认证器 | Passkey 的认证器管理材料 | 产品不得声称自己可任意导出、备份或删除其私钥 |
| 原生安全存储（P1） | 明确隔离的登录 key，按本地保护合同访问 | 节点、治理或资产 key 冒充登录身份 |
CSRF/Origin 防护与 Cookie 安全属性必须同时存在。对来自受信任反向代理的请求才能采纳转发来源，不能信任公网随便提供的 `X-Forwarded-For` 来绕过限流。[W7][W8]
### 12.4 兼容与可访问性
真实支持矩阵包含 PRD 指定的桌面/手机浏览器及一个满足用户验证要求的硬件安全密钥。内嵌环境提供打开系统浏览器入口；认证器缺失显示可行替代，不降低 userVerification。

区分用户取消、设备不支持、服务器不可达与明确验证失败。某些浏览器异常可能不能精确区分“用户取消”和“未找到凭证”，UI 应使用不超出可观测信息的表述，不假装读取到了系统内部原因。

按钮需键盘可达，焦点从系统弹窗返回后落在可理解位置；状态变化可由读屏感知；复制诊断只包含脱敏编号。首次操作 300 毫秒内反馈，不以无限 spinner 隐藏失败。

<a id="migration"></a>
## 13. 数据迁移、旧会话与兼容回退
### 13.1 升级不是两套账号长期双写
采用 **expand → 停写迁移 → 校验 → 切读写 → 开放能力**。P0 接受有维护提示的短暂身份写入窗口，不为本次功能构建复杂 CDC 双写。世界运行是否受影响由独立 authority 判断，不能声称身份维护自动无感。

迁移目标是一个 v2 权威；旧 JSON/Tablestore 因子表只读保留，不能一部分新用户写 v2、旧用户继续写 v1，制造可恢复/不可恢复的两套真值。
### 13.2 迁移步骤及完成条件
| 步骤 | 必须执行 | 失败边界 |
| --- | --- | --- |
| 1. 预检 | 读取 schema、身份 namespace、所有旧账号/因子、player 映射和历史序列；统计重复与冲突 | 任何一份旧因子指向互相矛盾身份时停止，不能按最后读取覆盖 |
| 2. 建立写入屏障 | 停止所有旧版本身份 writer；在部署和存储权限层阻止旧实例回写 | 仅在新库写 migrated=true 不能约束不认识该字段的旧程序 |
| 3. 一致性备份 | 记录来源摘要、快照时间、schema、namespace，保留可验证备份 | 不使用仍变化的散乱行读当一致性证明 |
| 4. 导入 v2 | 保留 account/player ID；分配稳定 userHandle；生成 email credential、lookup、binding；标记 provenance | 批量部分成功可以重跑，但每行基于源 ID/摘要幂等，不对外开放半成品 |
| 5. 历史字段处理 | `has_ever_public_key=false` 仅用于确实从旧邮箱专属 schema 导入的账号；未知 bound_at 以迁移时刻起计恢复资格 | 不从“现在 active 公钥为 0”推断从未有公钥 |
| 6. 对账 | 数量、每条 ID 映射、唯一因子、无孤立凭证、无错误合并；所有来源有结果或明确拒绝 | 不能只比较总数相等就通过 |
| 7. 切换 | v2 schema marker/配置和路由切换；实际注册开关仍关闭；旧程序无写权限 | 配置错误不能自动回到空 file 或旧因子表 |
| 8. 验证并开放 | 原邮箱登录、首公钥绑定、无 SMTP 公钥注册、账号与 world 恢复、两后端回归 | 发生越权/映射错误则停止放量，保护已写 v2 数据 |
旧 email-only ID 格式保持兼容；新字段缺失不能让序列化把 `has_ever_public_key`、status、security_epoch 误默认为一个更宽松状态。任何权限敏感字段缺失应显式归入旧 schema 的受限迁移分支，不能通用 `serde(default)` 自动放行。
### 13.3 旧浏览器会话
现有浏览器恢复材料缺少本设计完整的 credential/session 派生关系。[R5] 不能拿一份旧 release_token 直接换成具有凭证管理能力的账号 session。

受控切换时要求旧浏览器用已有邮箱重新认证；页面显示“登录方式升级，请重新登录”，保留非秘密目标世界和原请求引用。既有 runtime 会话必须在切换边界结束或按受限兼容方案取得可验证派生信息，不能以无限 legacy-grace 绕过 60 秒撤销承诺。

trusted-local bootstrap 与本地 QA test-login 保持原部署边界，不通过它们迁移为正式 hosted 玩家账号。迁移过程不将 host/node key 注入浏览器。
### 13.4 回退矩阵
| 发布状态 | 安全回退 | 禁止操作 |
| --- | --- | --- |
| v2 尚未开放写入 | 退出迁移、恢复原只读备份，确认无新事务后重启旧 writer | 在不知道是否已写新账号时直接覆盖 |
| 已有 v2 凭证和新账号 | 保留 v2 IdentityService；可回退 Viewer 非关键界面或关闭新注册 | 回到 email-only 身份服务，要求纯公钥用户补邮箱 |
| 邮件服务故障 | 关闭 email capabilities，维持 Passkey 登录和备用凭证恢复 | 公钥认证也初始化失败，或放松 OTP 校验 |
| 某类认证器存在安全缺陷 | 有意暂停受影响 credential class，说明恢复方案，按策略撤销派生会话 | 把有损安全阻断称为无损降级 |
| 身份数据库可能回滚 | 隔离恢复实例，完成账本/撤销对账并提升外部灾备 epoch 后放行 | 从旧快照上线，复活已撤销 key 或旧恢复申请 |
### 13.5 灾备恢复与防止撤销复活
事务持久化保护进程崩溃，不等于陈旧备份回滚安全。身份库灾备恢复时必须进入 maintenance fence；从独立保留的安全审计/备份增量重建撤销和恢复终态。恢复部署使用新的 `identity_instance_epoch` 或等价外部版本屏障，令所有旧 session/grant/未决挑战失效，客户端重新认证。

外部版本屏障只能防旧 session/grant 被重新接受，**不能单独证明旧 credential 没有被撤销**。如果无法补齐撤销/恢复历史，不得自动重新开放这些账号的旧公钥登录；保持受限状态并走可验证处置。不能因为拿到一份可读快照就宣称账号控制权已经恢复正确。

RP ID 变更是另一个账号可访问性迁移，不由数据库备份恢复解决。已生成 Passkey 的旧 RP 服务必须在迁移期间保留有效入口；跨 RP 迁移另行建立用户再次验证与新凭证绑定路径。

<a id="deployment"></a>
## 14. 部署、配置和安全基线
### 14.1 配置合同
以下是当前候选的配置合同；身份配置经类型化解析和启动校验，SMTP adapter 可选，存储失败不得自动降级。更详细的操作命令与实证仍以同版本部署/测试记录为准。
| 配置示意 | 默认 / 必要条件 | 影响范围 |
| --- | --- | --- |
| `OASIS7_HOSTED_IDENTITY_V2_ENABLED` | 显式 `1` 开启；未开启保留旧部署入口 | 开启后拒绝旧账号和玩家发行/续期/释放接口 |
| `OASIS7_HOSTED_IDENTITY_SERVICE_ID` | hosted 写入时必填，与 store 元数据一致 | 身份 namespace，不随节点重启变化 |
| `OASIS7_HOSTED_IDENTITY_ENVIRONMENT` | 显式 dev/staging/production | 环境隔离 |
| `OASIS7_HOSTED_WEBAUTHN_RP_ID` | 开放 Passkey 前必填 | 不能从请求 Host 推导 |
| `OASIS7_HOSTED_WEBAUTHN_ORIGINS` | 精确 HTTPS origins；本地开发特例显式隔离 | CORS/Origin/RP 配置 |
| `OASIS7_HOSTED_ACCOUNT_REGISTRATION_ENABLED` | 初始 false，验收后启用 | 新账号创建；不影响既有登录 |
| `OASIS7_HOSTED_EMAIL_ENABLED` | 未配置 SMTP 时 false | 邮箱发码与普通邮箱登录 |
| `OASIS7_HOSTED_EMAIL_RECOVERY_ENABLED` | 依赖邮箱与完整恢复能力 | 新恢复申请/完成的局部开关 |
| `OASIS7_HOSTED_NATIVE_KEY_ENABLED` | P0 固定 false | P1 入口，不能靠配置绕过平台验收 |
| `OASIS7_HOSTED_IDENTITY_SQLITE_PATH` | file v2 必填 | 新 SQLite，不覆盖旧 JSON |
| `OASIS7_HOSTED_IDENTITY_TABLE_V2` | Tablestore v2 明确表名 | 禁止失败后自动回旧表 |
| `OASIS7_HOSTED_IDENTITY_SESSION_KEY` / `OASIS7_HOSTED_IDENTITY_INSTANCE_EPOCH` | 独立 32 字节 secret 的 hex / 必填有效 epoch | cookie 摘要与实例恢复屏障；不能因重启无意更换 |
| `OASIS7_HOSTED_IDENTITY_TRUST_LOOPBACK_PROXY` | 默认关闭，仅 `1` 且直接 peer 是 loopback 才信指定来源头 | 不信任任意公网 X-Forwarded-For |
| `OASIS7_HOSTED_WORLD_IDENTITY_SCOPE` | v2 必填稳定 world scope | 玩家映射与授权域隔离 |
| session 摘要/HMAC、邮件加密、issuer 密钥配置 | 受限 secret 引用或已验收的部署注入 | 不提交仓库、不发送浏览器、不使用 node/governance key |
当前 `/capabilities` 返回 `registration_enabled` 及各方式的 `available`，不包含完整分层 readiness 或每通道 `reason` 合同；这些诊断与真实可用性仍须在上线门禁核对。邮件未配置不使公钥认证进程退出。`OASIS7_HOSTED_PASSKEY_LOGIN_ENABLED` / `OASIS7_HOSTED_IDENTITY_SCHEMA_MIN` 并非当前可用开关，不应从旧设计示意复制进部署配置。

已启用后的关键 RP 配置不允许热更新为任意新域。旧/新配置版本共存必须有明确迁移白名单，不从数据库里自动信任任意来源字符串。
### 14.2 服务边界
公网代理终止 TLS 后，内部仍采用已批准的可信网络/mTLS 通道。只有 identity 模块可接收账号 cookie；代理转发到世界节点时剥离该 cookie，不能让任意世界后端获得完整账号认证材料。

禁用不必要的 credential CORS；状态变更要求 CSRF token、Origin 精确匹配，配合 Fetch Metadata 和 JSON-only 防护。身份页面使用受控脚本来源、CSP、`frame-ancestors` 限制及 `Cache-Control: no-store`；Referer 不包含 secrets。[W7][W8]

冻结、恢复、注销会话等公共用户自服务与 private-control 运维动作分离。内部 introspection 只对被授权的 world adapter 暴露字段最小化结果，不提供“任意 account_id 查询所有凭证”的公网接口。
### 14.3 时钟、轮换和过期
所有有效期以服务端判定，采用可注入 `Clock` 便于测试。实例启动和运行中监控时钟偏差；超出 5 秒新鲜度预算的实例不再签发/验证新授权。不能把时钟容差设置为几分钟后仍声称 60 秒撤销上限。

issuer rotation 用 `key_id` 与明确 not-before/not-after；旧验证 key 只保留覆盖未过期授权的窗口。变更 session 摘要/HMAC key 时保留受限过渡 lookup 或强制重新认证，不能把“查不到摘要”当新账号。加密 key 轮换由版本化 ciphertext 接收，不以一次性解密所有明文落盘迁移。

<a id="native"></a>
## 15. P1 原生本地密钥与加密备份
### 15.1 密钥生成与本地保护
原生客户端生成 32 字节高质量随机种子并导出 Ed25519 公钥，使用独立的 `purpose=login_identity` 存储条目。优先使用操作系统凭据库/用户解锁保护；具体平台必须实测。**将 Ed25519 种子放入 Keychain/凭据库不等于证明它是硬件不可导出的 Secure Enclave/TPM 密钥。**

为支持备份，本方案是“受本地保护、用户主动解锁后可导出的登录 key”，不同时承诺硬件不可导出。后续如引入不可导出模式，必须使用另一个 credential class 和恢复合同。

客户端缺少已验收的保护机制时拒绝启用 native key；不能 fallback 为明文配置文件或普通网页 localStorage。内存种子用专用 secret 类型，避免 Debug、异常报告和无必要复制；清零是尽力防护，不能保证对已完全控制终端的攻击者保密。
### 15.2 原生挑战协议
原生签名使用新协议 `oasis7.identity.native.v1`，不能签任意字符串后被当成登录。服务端返回结构化摘要及对应唯一签名字节；客户端只对受信任身份服务和允许用途签名。

```text
signed_bytes = domain_separator
             || version
             || len(identity_service_id) || identity_service_id
             || len(environment_id)      || environment_id
             || len(audience)            || audience
             || len(purpose)             || purpose
             || len(account_context)     || account_context
             || len(operation_id)        || operation_id
             || public_key[32]
             || nonce[32]
             || issued_at_u64_be
             || expires_at_u64_be
```

字符串使用 UTF-8，长度为固定 u32 大端且有上限，时间为 u64 大端；协议实现必须有跨语言 golden vectors，禁止临时拼接 JSON 字符串导致不同序列化签不同语义。`purpose` 严格列举 register/login/bind/reauth/recovery_candidate，不接受 wildcard。

服务器验证签名、公钥类型、服务域、环境、用途、目标、期限和原子 nonce 消费，再接入相同身份事务。原生持有证明不自带 WebAuthn 的浏览器来源保护，也不证明终端已做硬件用户验证；客户端必须验证 TLS/目标服务并展示操作，不把该方式标成与 Passkey 完全相同的抗钓鱼等级。
<a id="imported-key-web"></a>

### 15.2.1 网页会话内导入

Trace refs: O7-REQ-45c9a6e2-d624-4a57-bced-b96b6aa78f69，承接 [Flow-KEYAUTH-008](hosted-public-join-key-based-onboarding.prd.md#imported-key-login)。

Viewer 将密钥导入放入原生 `details` / `summary` 折叠区，默认关闭；展开后保留格式、内存生命周期及防钓鱼告知，提交与签名合同不变。首屏状态使用玩家可理解的中文，协议标识留在诊断中。

`native_key.available` 表示有限 `import_only` 能力；不代表受保护客户端生成或加密备份已发布。仅接受 Ed25519、32 字节公钥和种子；Web Crypto 导入 non-extractable 签名对象，原始输入不进缓存、storage、网络或日志。浏览器先验证公私钥配对。页面关闭/退出释放对象，服务端只存 `NativeKey { public_key }`。

`POST /native-key/options` 与 `/native-key/complete` 支持 `login`、`bind`、`reauth`，不支持新建账号。签名字节沿用 §15.2 格式：域分隔符为 UTF-8 `oasis7.identity.native.v1\0`，version 为单字节 1；audience 是经 Origin 白名单验证的精确来源；account_context 是绑定/重新认证账号 ID，登录为空；operation_id 是 ceremony_id，nonce 为独立随机 32 字节。全部字符串以 u32 大端长度编码；时间为毫秒 u64 大端。客户端按 capabilities 中的 namespace、当前 location.origin、用途、公钥、操作 ID 和期限重建并对比字节后才签名，不接受任意服务器字符串签名。

挑战持久化保存 owner、namespace、RP version、用途、公钥、action 和签名字节；120 秒、最多 5 次失败。完成时重新检查 owner、namespace、RP version、账号/凭证/action 有效性与一次消费。绑定须现有敏感 `bind` action；reauth 只授权同账号同 session 的 pending action。登录/绑定共用来源及全局挑战配额。绑定唯一 lookup、凭证、账号 policy_version、action 消费、审计和 receipt 同事务；重试同 operation 返回已提交结果，不重新发行 session。登录复用 cookie/session/world 权限边界；`HostedAuthContextV2.auth_method=native_key` 仍仅允许原有 player_session/release 能力，不能升级到资产或 operator 权限。恢复完成时撤销全部旧公钥类型；原生凭证不能冒充 WebAuthn 恢复候选。

部署先升级所有世界授权 verifier 以识别 `native_key`（仍只接受玩家会话能力），再升级身份服务和 Viewer 开放入口；旧 verifier 会安全拒绝该 auth_method。首次绑定后身份库存有新的 credential union 分支，旧身份 binary 不具备读取兼容性，不能原地降级或删除新凭证来回退。回退版本必须继续识别并服务已绑定导入凭证，备份/迁移仍按现有存储合同执行。

### 15.3 项目登录身份备份格式
采用项目专属 `oasis7-login-identity-backup` v1。备份不包含 node key、wallet seed 或资产 signer。推荐 Argon2id 派生加密 key、XChaCha20-Poly1305 做带附加数据的认证加密；两者能力分别参考 RFC 与 libsodium 官方文档。[W9][W10]

建议初始参数为 **64 MiB、3 次、parallelism=1、16 字节随机 salt、32 字节派生 key**；XChaCha20-Poly1305 使用 24 字节随机 nonce。这里是本设计的待实测参数，不是引用标准的强制默认。低性能设备不能静默降低保护强度，需在受支持参数档位内明确选择。

外层 JSON 只包含 `format`、`version`、`header_b64`、`ciphertext_b64`。header bytes 为严格 UTF-8 JSON，包含服务/环境、credential fingerprint、公钥、KDF/AEAD 参数及 salt/nonce。解析拒绝重复字段和不受支持算法；认证附加数据直接使用 `"oasis7.login-backup.v1\0" || header_bytes`，避免解密时重新序列化产生不一致。

导入先做大小和资源上限检查，建议文件 ≤64 KiB、KDF 内存 ≤256 MiB、迭代 ≤6、parallelism ≤4，再本地解密。错误服务/环境、错误口令、认证标签失败、派生公钥与 header 不同均拒绝；不能因导入失败自动创建新身份。

导入成功只表示恢复同一私钥，仍须在线挑战确认该 credential 未被撤销。备份被复制后，同一 key 的所有副本共享撤销状态；不能承诺“只撤销被盗的那个文件”。遗失口令且无其他凭据时不提供不可实现的解密找回。

P1 发布前必须提供密码学测试向量、篡改/截断/参数膨胀测试、错误服务测试、真实安全存储验证、导出告知和撤销演练。受保护客户端生成及加密备份仍未开放；§15.2.1 的会话内导入独立于该能力。

<a id="operations"></a>
## 16. 限流、可观测性、性能和运行故障
### 16.1 独立的保护预算
注册挑战、登录挑战、OTP 投递、敏感 action、恢复申请、身份建号、世界准入和免费模型额度分别限流。禁止因为新注册额度耗尽而让老用户不能登录，也禁止增加一个实例就把整个部署的限额翻倍。

P0 沿用 PRD 的基线：注册挑战每来源每分钟 60、部署每分钟 300；新账号每来源每小时 10、环境每天 100；挑战有效期 120 秒、最多 5 次错误；每账号最多 10 个 active 公钥凭证。最终建号事务同时消费环境/来源建号额度，失败或重复提交不能重复收费，已确认提交不能因响应丢失返还额度再创建第二个账号。

当前候选另有硬编码保护默认值：登录挑战每来源每分钟 60、部署每分钟 300；敏感 action 每账号每分钟 30；公钥重新认证挑战每账号每分钟 60；HTTP 同时处理连接上限 32。身份通道计数进入 namespace 事务记录，连接上限是单 HTTP 进程保护，不能当作部署级分布式额度。邮箱 OTP 每来源每分钟 5、每地址每分钟 1、部署每分钟 300；恢复申请每来源每天 5、每账号每天 2。它们是当前实现默认值，不声明已有运营热配置或负载验收证据。

采用固定窗口及服务器时间，counter key 含 namespace、通道、来源摘要、窗口；临界边界的可接受突发是公开的限流语义，不把它说成滑动窗口绝对上限。需要滑动窗口时另行实现并测试，不在文档和代码各用一种算法。

网络来源从受信代理链解析；IPv4/IPv6 归一化有明确配置，不把任意 X-Forwarded-For 当真。共享 NAT 的受控例外由运营配置和审计承接，不永久封禁多人同网用户。公钥数量不能证明独立真人，邀请与资源预算仍是独立主责。

未认证连接受请求体和并发边界保护，防止还未创建 challenge 就耗尽数据库；重复查询和同恢复申请幂等重试不计新申请，不改变一账号仅一个活跃申请的要求。限额调整须随代码或受审查配置变更并验证，不以设计建议替代当前每账号每天 2 次的实际默认值。
### 16.2 指标与审计
| 指标 | 标签边界 | 解释 |
| --- | --- | --- |
| `identity_ceremony_total` | kind、result、credential_type、environment | 区分取消、过期、验证失败和存储故障 |
| `identity_commit_latency_ms` | backend、operation、outcome | 不包含用户系统确认和世界排队 |
| `identity_transaction_unknown_total` | backend、operation | 结果未知，不能算确定失败 |
| `identity_recovery_total` | started/approved/cancelled/completed/expired/blocked | 不记录邮箱、候选原始 key 或 OTP |
| `identity_revocation_enforcement_ms` | credential/session/recovery、runtime lane | 从权威提交到最后一个可接受新操作的实际窗口 |
| `identity_outbox_lag_seconds` | event_class、backend | 通知/撤销投递积压，超时应告警 |
| `account_to_world_total` | registered/authenticated/admitted/runtime_bound | 分母分开，不用注册成功冒充开始游戏 |
| `identity_legacy_access_total` | schema、path、result | 检查迁移期间旧版本是否仍在回写或绕路 |
账号、凭证、operation ID 不作为高基数指标标签；需要追溯时用受限审计字段。禁止采集完整请求体、attestation/assertion、cookie、registration grant、OTP、原生私钥或备份口令。邮件发送失败日志也不能带完整地址和服务凭据。

安全变更必须先持久化审计/outbox，再返回成功；外部日志平台不可用不阻断已经有持久记录的事务。身份库写满导致不能保留安全记录时应失败，不转为仅控制台打印后放行。
### 16.3 性能预算与验证方法
PRD 的 NFR-01 为 10,000 账号、30,000 公钥凭证、20 并发客户端、每秒 5 次有效认证请求持续 10 分钟，认证/登记服务端 P95 ≤1 秒，非预期服务错误率 <1%。这里保留为**待测验收目标**，不写成现有结果。

验证应分别覆盖 SQLite 和 Tablestore。预置账号和凭证走隔离数据生成，压测使用明确的测试 namespace/容量 profile；不得在公开环境关闭限流凑吞吐。若测试覆盖大量新注册，记录与产品默认 100/日的保护配置差异，功能限流测试仍按默认值独立运行。

建议将单请求解析/库验证、存储排队、事务读取、提交、响应分段打点，定位真实耗时；不要把多个子事务相加前的各自 P95 当作端到端 P95。目标未达标时先减少事务内网络往返和不必要全量读写，不能取消持久 commit、唯一性或审计要求。

Tablestore adapter 应复用连接并提供异步 IO；SQLite 使用有界线程池。当前每个 Tablestore backend 自建运行时的结构需要通过适配重整，不能在任意 async handler 持锁调用嵌套 runtime 并以线程阻塞维持表面正确。[R2]
### 16.4 故障处理矩阵
| 故障 | 系统动作 | 不允许的兜底 |
| --- | --- | --- |
| SMTP 未配置或超时 | 邮箱 unavailable；新 Passkey 注册和已有登录继续；恢复按真实条件阻断 | 自动接受测试 OTP、让 email 失败导致整个 identity 退出 |
| Tablestore 不支持本地事务 | 该 v2 writer readiness 失败；明确配置缺口 | 退化为多个顺序写、自动切到 file |
| SQLite/Tablestore commit 超时 | `OutcomeUnknown`，用原 operation 查询和串行化判定 | 直接换 operation_id 重试新建号 |
| 认证后 world 满员 | 保留账号 session、显示 waiting | 放回已消费 challenge、生成新 player |
| 撤销推送丢失 | 授权新鲜度上限兜底拒绝旧能力 | 认为推送订阅成功就永久信任 |
| identity 与 runtime 网络分区 | 既有短期授权耗尽后停止新受认证玩家动作 | 用陈旧 cache 无限延期 |
| 恢复通知失败 | 重试 outbox；未获原 key 批准的等待从通知接受时约束 | 到点无验证接管、恢复已提交后复活旧 key |
| SDK/schema 升级无法读库 state | 未决仪式安全失效，要求新挑战；长期 credential 读取故障阻断升级 | 丢掉公钥记录或把解析失败当新账号 |
| session cookie 响应丢失 | 确认账号/操作结果，再新鲜登录 | 凭旧断言反复领取新 session |
| 恢复旧快照 | maintenance fence、对账、外部 epoch、重新认证 | 把可读快照直接当完整授权恢复 |
<a id="validation"></a>
## 17. 验收追踪、测试层级与安全反例
### 17.1 PRD → 系统设计 → 验收
下表保留配对 PRD 的全部需求 ID，不发明已存在的 Issue、CI 结果或已实施测试函数。
| PRD-ID | 本设计主要承接 | 对应 AC | 必须证明的系统结果 |
| --- | --- | --- | --- |
| PRD-KEYAUTH-001 | [§6](#webauthn)、[§7](#auth-flows)、[§14](#deployment) | AC-01、AC-16 | 无邮箱字段、无 SMTP 的 register/login；真实 discoverable 能力 |
| PRD-KEYAUTH-002 | [§4](#model)、[§7](#auth-flows)、[§13](#migration) | AC-02 | 退出/刷新/换设备后同账号和原 player；登录失败不开户 |
| PRD-KEYAUTH-003 | [§5](#storage)、[§8](#credential-management)、[§10](#world-sessions) | AC-03、AC-04、AC-05 | 多凭证唯一绑定、保留 key 证明、原子撤销与最后凭证保护 |
| PRD-KEYAUTH-004 | [§7.3](#auth-flows)、[§8](#credential-management)、[§13](#migration) | AC-06、AC-07 | 旧邮箱升级不换号、邮箱可选、既有 key 账号不走首次升级旁路 |
| PRD-KEYAUTH-005 | [§10](#world-sessions)、[§12](#viewer) | AC-04、AC-08 | 多账号 session 与单世界控制独立；退出不误删同步 Passkey |
| PRD-KEYAUTH-006 | [§9](#recovery)、[§5](#storage) | AC-05、AC-07、AC-09 | 备用 key、邮箱成熟/等待/批准/取消、最终证明、原子替换 |
| PRD-KEYAUTH-007 | [§10.3](#world-sessions)、[§16.1](#operations) | AC-10 | 资格/slot/额度独立，邀请原子消费与重复请求安全 |
| PRD-KEYAUTH-008 | [§1](#scope)、[§3](#architecture)、[§10.5](#world-sessions) | AC-11 | 所有新认证/恢复方式均不能开放资产和 operator 权限 |
| PRD-KEYAUTH-009 | [§7](#auth-flows)、[§11](#api)、[§12](#viewer) | AC-12 | 取消、超时、未知结果、重试、跨标签页正确反馈 |
| PRD-KEYAUTH-010 | [§5](#storage)、[§13](#migration)、[§14](#deployment) | AC-13、AC-16 | 两后端迁移，关闭注册不关闭登录，回退不遗弃新账号 |
| PRD-KEYAUTH-011 | [§15](#native) | AC-14 | 原生 key 受保护生成、签名、备份/导入、篡改拒绝；仅 P1 |
| PRD-KEYAUTH-012 | [§6](#webauthn)、[§11](#api)、[§16](#operations) | AC-11、AC-15 | 跨域/跨用途拒绝、隐私最小化、日志不能重放认证 |
### 17.2 NFR 承接
| NFR | 系统设计和判定 |
| --- | --- |
| NFR-01 | §16.3 两后端真实负载，保留 PRD 的样本量、5 次/秒与 P95/错误率目标 |
| NFR-02 | §12 用户点击 300ms 内反馈；不以服务端计时替代 UI 反馈证据 |
| NFR-03 | §10.6 30 秒新鲜度 + 5 秒时钟 + 5 秒检查余量；端到端必须 ≤60 秒 |
| NFR-04 | §6 challenge 32 字节、120 秒、5 次错误、持久原子消费 |
| NFR-05 | §16.1 来源/部署挑战配额、来源/环境新账号配额与独立登录预算 |
| NFR-06 | §5 原子事务；至少 20 路竞争同凭证、同尝试、同邀请 |
| NFR-07 | §12.4 真实平台/浏览器/硬件密钥矩阵，记录发布时版本 |
| NFR-08 | §12 键盘、读屏、非指纹验证和受限环境替代入口 |
| NFR-09 | §12.3、§16.2 网络/日志/错误报告检查；P1 加本地导入导出 |
| NFR-10 | §5.4、§13.5 重启、提交未知与灾备边界；不丢已提交映射 |
| NFR-11 | §14 capabilities、分通道开关，无 SMTP 和关闭注册分别验证 |
| NFR-12 | §4、§13 稳定 ID、类型联合体、schema/旧邮箱迁移和新方式扩展 |
### 17.3 Required：可重复的单元/契约/集成测试
所有固定时间参数通过注入 Clock 测试，不让 CI 实等 24 小时或 7 天。目标测试组如下，名称仅为分组建议：
| 测试组 | 正向与必须覆盖的反例 |
| --- | --- |
| `ceremony_contract` | 正确注册/登录；用途错误、origin/RP 错误、UV 缺失、过期、5 次耗尽、重启后重放 |
| `store_contract` | SQLite、Tablestore 同一测试语义；条件创建、版本冲突、唯一 lookup、失败不发布内存新状态 |
| `commit_ambiguity` | 持久提交前/中/后崩溃；COMMIT 已成功但响应丢失；结果未知不制造第二账号/session |
| `credential_binding` | 20 路同 credential 竞争；同 ID 异公钥；已有账号被提交新公钥；错误 userHandle；上限 10 |
| `last_credential_race` | 两把 key 被两个并发请求分别删除；proof key 同时撤销；必须至少一个操作拒绝 |
| `email_compatibility` | 纯邮箱旧账号首次升级；`has_ever_public_key=true` 且 active 为 0 时不能绕路；邮箱冲突无错误合并 |
| `freeze_integrity` | frozen 账号收到正确 OTP/Passkey 仍不能自动写 active；状态只由授权解冻流程更改 |
| `recovery_state_machine` | 不满成熟期/等待期；预登记不能登录；批准后换 candidate；过期、邮箱更换、同时取消/完成 |
| `recovery_atomicity` | 新 key 生效、旧 key 撤销、epoch 和 receipt 必须同提交；无法产生双活或全丢中间态 |
| `session_lineage` | 一把同步 Passkey 两个 session；登出一个不删凭证；撤销凭证后派生 session 全部失效 |
| `runtime_revocation` | 已打开 WS、漏推、短期授权过期、陈旧缓存续期、跨节点切换、旧 release 请求均受阻 |
| `join_idempotency` | 账号成功但 world_full；邀请兑换后 RPC 丢失；控制接管失败不预先踢掉旧控制者 |
| `authority_separation` | email / Passkey / recovered session 不能覆盖资产、高风险 prompt、node/governance 门禁 |
| `web_request_security` | 登录 CSRF、action CSRF、伪造转发 IP、越权 ID、过大/畸形响应、跨环境 grant |
| `migration_contract` | 原 ID 对账、冲突停止、无 bound_at 的保守导入、旧 writer 屏障、schema 解析失败拒绝 |
| `feature_isolation` | 关闭新注册仍能登录；无 SMTP 仍能 Passkey；test-login 不可经公网访问 |
现有 Tablestore 实现的部分代码被 `#[cfg(not(test))]` 包围。[R2] 新测试结构必须让真实 adapter 可由集成 harness 编译和调用，不能以所有测试都走 file/fake 证明云端事务正确。外部存储调用放在隔离集成环境，并只使用测试凭据。
### 17.4 Full：真实环境与用户体验证据
真实 Safari/Chrome/Edge 与手机浏览器完成创建、登录、取消、第二凭证、跨设备、撤销、恢复及清除 cookie 后返回。虚拟认证器用于自动化回归，不替代系统凭证管理器同步、硬件 PIN、设备丢失和内嵌浏览器证据。

执行 PRD 的至少 20 名首次用户观察，报告无协助 60 秒内到达账号创建结果的比例，目标至少 80%；如实保留取消、受限环境、系统交互等样本。注册完成和 world 排队分开计时。

两存储后端分别进行 5 次/秒的性能验收、提交响应丢失故障注入、关闭 SMTP、暂停注册、重启、恢复旧快照的安全演练。多实例 Tablestore 验证全局计数与一次消费；SQLite 不以多机副本测试冒充共享存储。
### 17.5 证明范围与完成定义
P0 放量必须同时满足全部 P0 AC、NFR 安全不变量、真实支持矩阵、旧账号迁移、关闭注册仍可登录及恢复演练。P1 的 AC-14 尚未实现不阻断 P0，受保护客户端生成及加密备份必须保持关闭；`import_only` 仅按 §15.2.1 开放会话内导入。

测试报告须绑定候选提交、构建、模式 `viewer`、hosted deployment、存储后端、RP/环境、浏览器/认证器版本和证据窗口。文档链接、单元测试通过、虚拟认证器成功都不能证明生产 custody、多节点最终性或真人持续回访成立。

本候选已运行身份契约、签名世界授权、loopback Tablestore 协议和 Chromium 虚拟认证器回归；这些证据不替代真实 SMTP/Tablestore、真实设备支持矩阵、性能及首次用户验收。新注册默认关闭，完整 P0 放量仍以本节验收为门禁。

<a id="delivery"></a>
## 18. 代码落点、交付顺序与设计交接
### 18.1 代码落点
表中“现有”路径已在基线或本次输入中核对；“建议新增”是待实施结构，不是仓库现存文件的声明。
| 位置 | 类型 | 改造 |
| --- | --- | --- |
| `crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_account_identity.rs` | 现有 | legacy email façade 仅在 v2 未开启且满足 §13.4 时保留；v2 使用 §11.2 的 EmailAdapter / AccountSession 路由，拒绝旧登录和 player-session 发行/续期/释放入口 |
| `.../hosted_account_store_backend.rs` | 现有 | 分离旧格式 reader / v2 store adapter；不再 login touch 解冻；事务先于内存可见 |
| `.../hosted_player_session.rs` | 现有 | 账号 session 与控制 lease 关联，重排接管顺序，旧 release 绑定 epoch |
| `crates/oasis7_viewer_runtime/src/viewer/runtime_live/session_policy.rs` | 现有 | 保留单控制 key/epoch；增加身份上下文版本和短期新鲜度校验 |
| `crates/oasis7_viewer/software_safe_src/viewer_hosted_auth_state_module.js` | 现有 | 新 cookie 登录态、旧私钥/敏感恢复材料清理、正交状态投影 |
| `.../viewer_hosted_login_state_module.js`、`.../viewer_hosted_login_registration_bridge.js` | 输入中已核对的既有模块 | 注册/登录/准入分别反馈，不让旧 promise 或重试换号 |
| `hosted_identity/{model,service,webauthn,email,policy,recovery,store,session,audit}.rs` | 建议新增模块组 | 模块化封装；是否提取为库模块由编译边界决定，不新建独立部署服务 |
| `hosted_identity/store/{sqlite,tablestore,migrate}.rs` | 建议新增 | 统一事务合同、两 adapter、显式 migration |
| Viewer Passkey adapter / 安全中心模块 | 建议新增 | 标准编码转换、capabilities、凭证/会话/恢复 UI；不复制服务端权限判断 |
| identity schema migration / 契约测试 / 真实环境 smoke | 建议新增验证入口 | 明确候选与后端；不能伪装为已经存在的测试脚本 |
`hosted_access` 公共路由白名单、registration grant 结构/验签入口以及恢复令牌处理也须按调用链同步更新。路径级具体函数与 feature 编译组合在实施时确定，不能只修改文档列出的几个文件而漏掉绕行入口。
### 18.2 交付顺序
推荐以一个 P0 功能 PR 包含垂直闭环，native P1 独立交付；顺序在 PR 内以可审查提交组织，不把每个数据字段拆成前置 PR。

**第一段：身份基础与事务。** 建立模型、统一 store contract、两后端事务与迁移；修正冻结状态，加入 challenge 一次消费和 receipt。功能开关关闭，旧账户读取保持受控。

**第二段：注册与会话。** 接入 WebAuthn、邮箱兼容、HttpOnly 账号 session、正交 UI；增加无 SMTP 路径。身份与 world 准入拆开，完成多设备账号登录但单世界控制的适配。

**第三段：安全闭环。** 凭证管理、邮箱绑定、预登记恢复、最后凭证保护、完整 runtime 撤销链和限流；旧会话升级与安全回退同步实现，不能作为“未来优化”。

**第四段：证据与开放。** 两后端集成、真实设备、用户观察、迁移与回退演练通过后，才开启公开注册。保持文档目标、实现状态和公开 claims 分开。

这是减少无意义 PR 分裂的组织建议，不意味着大型改造可跳过评审或在安全链未闭合时部分上线。若 Tablestore 事务 SDK 适配必须独立上游提交，可作为依赖交付，但不能用不原子的临时后端替代 P0 安全结果。
### 18.3 P0 采用后的产品与系统边界
| 澄清项 | 必须同步的语义 | 是否改变核心产品方向 |
| --- | --- | --- |
| 账号 session 与 world device session | 多设备登录不等于多端同时控制；控制接管须明确确认 | 否，落实现有 runtime 边界，需进入 UI 流程 |
| 恢复候选预登记 | 申请审批绑定具体候选 key；等待结束才激活并再次证明持有 | 否，细化恢复步骤，不能留空目标 |
| file v2 存储 | 本地存储仍是 file 部署方式，但数据从 JSON 显式迁入 SQLite | 否，属于系统选型；部署/备份文档必须更新 |
| 旧浏览器认证 | 无完整派生关系的旧 release_token 不自动换成账号管理权限；切换可要求重登 | 否，不丢身份，但不是“全程免操作”升级 |
| 邮箱成熟时间缺失 | 不伪造历史 bound_at；恢复资格从可信时间起算 | 否，不影响旧邮箱普通登录 |
| 新鲜 action、默认 session TTL、恢复频率 | 本文新增的技术默认值需在配置与用户提示中一致 | 是具体参数补充，不应被误当作现有产品实测或既有正式值 |
托管身份 PRD 的 SC-1 / AC-2 和“玩家接入与发行”根产品已采用 P0 的网页推荐 Passkey、邮箱可选与多凭证账号接入合同；本节明确其产品与系统分工，不将 P1 原生密钥升级为已发布能力。资产/custody 原有边界、受控技术预览的公开阶段及 §18.4 尚待验证条件继续保留。
### 18.4 尚待实施验证的明确条件
P0 代码候选已有 SQLite 事务、标准 WebAuthn、可选邮箱与恢复、v2 账号/世界会话接口及受控预览适配；这不等于已完成全部 P0 放量条件。以下是仍需实施或取得真实证据的门禁，不要求用户重新作已讨论过的产品选择：

- Tablestore 目标实例/新表确实支持本地事务，Rust SDK 适配可编译并真实提交/回滚；否则该 backend 不得开放 v2 写入。
- 当前适配固定 `webauthn-rs-core 0.5.5`，配对状态要求 resident/UV、无 attachment 强制、attestation none；虚拟认证器证明不能替代真实浏览器/硬件支持矩阵或系统同步恢复验证。
- 稳定正式 RP/origin、身份服务 namespace、旧 player 作用域、真实部署 secrets 由现有配置和部署 owner 落实；本文不杜撰域名或读取私密配置。
- 30 秒新鲜度检查必须覆盖所有 hosted 新操作和 refresh 路径；未覆盖的旁路是 P0 缺陷，不是文档免责项。
- 默认世界准入仍安全拒绝；`open_preview` 不实现正式邀请兑换/资格 authority。真实世界进入、控制接管、失效传播与完整玩家首会话不得由本地身份测试代签。
- 两后端 P95、20 名用户观察、恢复通知及灾备演练有真实证据；设计目标本身不能替代这些结果。

<a id="references"></a>
## 19. 依据、版本与设计决策性质
所有产品参数以配对 PRD 为基础；本文新增选型、接口、表结构、30 秒新鲜度、SQLite 和预登记恢复等是**设计决策**，不是对当前代码能力的描述。外部标准用于核验机制边界，不代表项目已通过相应标准认证。
### 19.1 仓库与产品依据
| 标识 | 来源及核对范围 | 支持内容 |
| --- | --- | --- |
| P | 同目录配对 PRD v1.0，以同一 Git / PR 版本为准 | 12 条需求、16 个 AC、12 个 NFR，以及邮箱恢复和 P1 范围 |
| R1 | 固定提交的 `hosted_account_identity.rs`，初始化与 275–540 行登录完成路径 | email/SMTP、OTP 与玩家 issue 耦合 |
| R2 | 固定提交的 `hosted_account_store_backend.rs`，账户结构及 165–470 行读写路径 | file/Tablestore 存储、状态 touch、同步 runtime 与测试编译边界 |
| R3 | 固定提交的 `hosted_player_session.rs`，1–255 与 360–640 行 | session 结构、lease TTL、一个 active token 和重发顺序 |
| R4 | 固定提交的 `runtime_live/session_policy.rs`，1–240 行 | 单 active key、rotation/revoke 与 session epoch |
| R5 | 固定提交的 `viewer_hosted_auth_state_module.js`，读取/持久化方法 | 浏览器恢复材料及旧 privateKey 不恢复 |
| R6 | “玩家接入与发行”根产品文档 | 账号/会话恢复归属及与世界/Agent 权限的边界 |
| R7 | 托管身份与 custody 系统设计 | 四类身份、服务平面分离、尚未交付的 custody 目标 |
### 19.2 外部一手资料
| 标识 | 资料 | 本文使用范围 |
| --- | --- | --- |
| W1 | W3C WebAuthn Level 3 | RP/来源、注册/认证、credential record、用户验证 |
| W2 | webauthn-rs 官方 API 文档 | 配对 state、Passkey / discoverable 接口、服务器保存状态要求 |
| W3 | webauthn-rs 官方 feature 文档 | feature 开关与版本核对，不代表已经加入项目依赖 |
| W4 | 阿里云 Tablestore 本地事务 API | 同分区原子性、新表能力、事务接口约束 |
| W5 | SQLite 官方 WAL 文档 | 单机文件边界、WAL 备份和已公开修复注意事项 |
| W6 | SQLite 官方 PRAGMA 文档 | 持久性和同步配置 |
| W7 | OWASP Session Management Cheat Sheet | cookie/session 安全原则 |
| W8 | OWASP CSRF Prevention Cheat Sheet | Origin、CSRF token 与跨站请求约束 |
| W9 | RFC 9106 | Argon2id 密码派生背景；本文参数为设计建议 |
| W10 | libsodium XChaCha20-Poly1305 文档 | 认证加密与附加数据机制 |
[R1]: https://github.com/eng-cc/oasis7/blob/f69c8a666024fa564e52314965025b019fa42d4c/crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_account_identity.rs
[R2]: https://github.com/eng-cc/oasis7/blob/f69c8a666024fa564e52314965025b019fa42d4c/crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_account_store_backend.rs
[R3]: https://github.com/eng-cc/oasis7/blob/f69c8a666024fa564e52314965025b019fa42d4c/crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_player_session.rs
[R4]: https://github.com/eng-cc/oasis7/blob/f69c8a666024fa564e52314965025b019fa42d4c/crates/oasis7/src/viewer/runtime_live/session_policy.rs
[R5]: https://github.com/eng-cc/oasis7/blob/f69c8a666024fa564e52314965025b019fa42d4c/crates/oasis7_viewer/software_safe_src/viewer_hosted_auth_state_module.js
[R6]: https://github.com/eng-cc/oasis7/blob/f69c8a666024fa564e52314965025b019fa42d4c/doc/product/player-entry-distribution/prd.md
[R7]: https://github.com/eng-cc/oasis7/blob/f69c8a666024fa564e52314965025b019fa42d4c/doc/p2p/blockchain/hosted-public-join-managed-identity-custody.design.md
[W1]: https://www.w3.org/TR/webauthn-3/
[W2]: https://docs.rs/webauthn-rs/0.5.5/webauthn_rs/struct.Webauthn.html
[W3]: https://docs.rs/crate/webauthn-rs/0.5.5/features
[W4]: https://www.alibabacloud.com/help/en/tablestore/table-store-service/developer-reference/startlocaltransaction
[W5]: https://www.sqlite.org/wal.html
[W6]: https://www.sqlite.org/pragma.html#pragma_synchronous
[W7]: https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html
[W8]: https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html
[W9]: https://www.rfc-editor.org/rfc/rfc9106.html
[W10]: https://doc.libsodium.org/secret-key_cryptography/aead/chacha20-poly1305/xchacha20-poly1305_construction

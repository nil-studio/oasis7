# 托管公网接入 — 部署安全基线、原生密钥备份与运行故障（分册）

- 主设计文档：[`hosted-public-join-key-based-onboarding.design.md`](hosted-public-join-key-based-onboarding.design.md)
- 本分册按 #6507 原样承接原第 14–16 节（部署安全基线、P1 原生本地密钥与加密备份、限流与运行故障），不改变既有安全边界、Trace 归属或验收义务。

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

共享字节编码入口为 `oasis7_agent_api::identity::NativeIdentityChallengeV1::signing_bytes`，现有身份服务复用它；`tests/fixtures/native_identity_v1.json` 由 Rust 合同测试与 Viewer 原生签名测试共同消费，覆盖用途和 UTF-8 字节长度。codec 只编码显式字段，不验证服务信任、账号上下文、期限、凭据有效性或原子消费，不是可直接调用的 signer。现有网页入口仍严格限于下节三种用途；机器注册按[独立事务入口](#machine-registration)的双开关发布实际能力，不能凭 codec 将 discovery 标成可注册或可完整入场。
<a id="imported-key-web"></a>

### 15.2.1 网页会话内导入

Trace refs: O7-REQ-45c9a6e2-d624-4a57-bced-b96b6aa78f69，承接 [Flow-KEYAUTH-008](hosted-public-join-key-based-onboarding.prd.md#imported-key-login)。

Viewer 将密钥导入放入原生 `details` / `summary` 折叠区，默认关闭；展开后保留格式、内存生命周期及防钓鱼告知，提交与签名合同不变。首屏状态使用玩家可理解的中文，协议标识留在诊断中。

`native_key.available` 表示有限 `import_only` 能力；不代表受保护客户端生成或加密备份已发布。仅接受 Ed25519、32 字节公钥和种子；Web Crypto 导入 non-extractable 签名对象，原始输入不进缓存、storage、网络或日志。浏览器先验证公私钥配对。页面关闭/退出释放对象，服务端只存 `NativeKey { public_key }`。

`POST /native-key/options` 与 `/native-key/complete` 支持 `login`、`bind`、`reauth`，不支持新建账号。签名字节沿用 §15.2 格式：域分隔符为 UTF-8 `oasis7.identity.native.v1\0`，version 为单字节 1；audience 是经 Origin 白名单验证的精确来源；account_context 是绑定/重新认证账号 ID，登录为空；operation_id 是 ceremony_id，nonce 为独立随机 32 字节。全部字符串以 u32 大端长度编码；时间为毫秒 u64 大端。客户端按 capabilities 中的 namespace、当前 location.origin、用途、公钥、操作 ID 和期限重建并对比字节后才签名，不接受任意服务器字符串签名。

挑战持久化保存 owner、namespace、RP version、用途、公钥、action 和签名字节；120 秒、最多 5 次失败。完成时重新检查 owner、namespace、RP version、账号/凭证/action 有效性与一次消费。绑定须现有敏感 `bind` action；reauth 只授权同账号同 session 的 pending action。登录/绑定共用来源及全局挑战配额。

绑定唯一 lookup、凭证、账号 policy_version、action 消费、审计和 receipt 同事务；重试同 operation 返回已提交结果，不重新发行 session。登录复用 cookie/session/world 权限边界；`HostedAuthContextV2.auth_method=native_key` 仍仅允许原有 player_session/release 能力，不能升级到资产或 operator 权限。恢复完成时撤销全部旧公钥类型；原生凭证不能冒充 WebAuthn 恢复候选。

部署先升级所有世界授权 verifier 以识别 `native_key`（仍只接受玩家会话能力），再升级身份服务和 Viewer 开放入口；旧 verifier 会安全拒绝该 auth_method。首次绑定后身份库存有新的 credential union 分支，旧身份 binary 不具备读取兼容性，不能原地降级或删除新凭证来回退。回退版本必须继续识别并服务已绑定导入凭证，备份/迁移仍按现有存储合同执行。

<a id="machine-registration"></a>

### 15.2.2 机器首次注册事务

当前代码提供默认关闭的身份事务与原件恢复；它不交付世界首次绑定、公开游戏写入口或完整冷入场。

Trace refs: O7-REQ-126d54f5-6d4a-4406-8d75-915c04d5d196；承接[合法冷入场](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#req-ext-018)。机器注册使用同一身份 authority 和 `NativeKey` 公钥类型，不新增世界角色或 executor 权限。新增 `/machine-registration/options` 与 `/machine-registration/complete`，仍由现有 HTTPS、Origin/CSRF/JSON、请求上限和事务存储保护。客户端先读取可信 `/capabilities`，保存 preauth cookie/CSRF 于受保护本地存储；它们不进入模型上下文。

注册必须同时满足已有注册总开关及默认关闭的 `OASIS7_HOSTED_MACHINE_REGISTRATION_ENABLED=1`，两者在挑战创建和完成时均重验。配置缺失为关闭，非法值拒绝启动。关闭注册后，已有账号仍可经原 `native-key` 登录、撤销、恢复及查询原结果；网页 `import_only` 入口不接收 register intent。机器 profile 不绕过站点配额或额外人类确认，部署尚未批准时保持关闭。

Unix SDK 的 `IdentityClient` 固定 HTTPS origin、identity service/environment 与 audience，禁用重定向和环境代理；每次响应最多 64 KiB。注册 options/complete 与登录 complete 原件先加密持久化再发送。登录回包丢失后重放原 complete 只对账；缺原 session cookie 时显示 `action_required/explicit_fresh_login`，由调用者显式启动新登录操作。历史进度不代替当前 session 验证。

客户端在 options 前持久保存随机 32 字节 base64url `operation_id`、公钥和显示名；相同 preauth/operation/内容返回原挑战，异内容拒绝。options 分配原账号引用与一次性 nonce，使用 `purpose=register`、空 account_context 的共享 v1 codec。显示名保存在该不可变 operation 的服务端状态中，不由 complete 再提供。完成请求仅含原 operation/ceremony ID 和确切字节的签名；客户端在发送前持久保存原请求，不能超时后更换键或 nonce。

账号、唯一公钥 lookup、凭据、挑战消费、配额、审计与永久原操作 receipt 在同一事务提交。注册仅返回账号及 receipt，不签发会话，也不进入世界；后续登录是独立用途的挑战。

complete 原件重放只返回原 receipt，不重复建号或签发 session；原挑战到期不允许新提交，但已提交原件仍可恢复 receipt。原件丢失时使用受支持的新登录挑战查回同一公钥主体，不能删除或替换账号以绕过未知结果。

永久公钥 lookup（包括撤销 tombstone）阻止并发、重启和历史请求重复创建主体。失败、耗尽、取消与 proof 过期也持续保留机器注册原 operation 的内容绑定，cleanup 不解除该身份。公开响应不能把此身份 receipt 表示为准入、owner、starter 或任务生效。

Unix 密码解锁 profile 复用 `oasis7_secure_envelope` 的固定 Argon2id（64 MiB、3 次、1 lane）与 XChaCha20-Poly1305，使用独立 `login-identity-v1` kind。登录种子与随机 outbox 密钥只在加密 key envelope 内持久保存；请求原件按独立密钥、服务/操作/阶段 AAD 保护，不含长期私钥。目录/文件沿用无符号链接、当前用户所有、0700/0600、fsync 和不可变原件发布合同；缺失或损坏不自动换钥。解锁仅经可信本机控制面的私有 stdin/API；不从 argv/env/聊天获取，不把身份存储交给默认 executor 工具。不支持的平台拒绝打开，密码 profile 不声明硬件不可导出或 OS 凭据库验收。


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

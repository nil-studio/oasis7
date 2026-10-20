# Hosted 多凭证账号验收手册

本手册只验证 `viewer / hosted_public_join / identity v2`。虚拟认证器、loopback 云协议和账号登录均不代签真实硬件、真实云事务、邀请资格、资产 custody 或完整首局可玩性。

## 候选与配置

记录候选 Git OID、构建来源、RP ID、精确 HTTPS origin、身份 namespace、外部 instance epoch、世界 ID、存储后端和浏览器版本。新注册默认关闭；受保护客户端生成和加密备份保持未发布，网页会话内导入按文末流程验证。生产 RP 不从 Host、转发头或世界 URL 推导。

`OASIS7_HOSTED_IDENTITY_V2_ENABLED` 只接受精确值：`1` 启用，未配置、空值、`0` 或 `false` 关闭。其他值（含 `true`、大小写变体或空白包裹的 `1`）必须在启动时拒绝；runtime 对非法配置继续拒绝 legacy 授权。关闭时已有 v2 存储的回退保护仍适用。

启动 v2 至少配置：

- `OASIS7_HOSTED_IDENTITY_V2_ENABLED=1`、`OASIS7_HOSTED_IDENTITY_SERVICE_ID`、`OASIS7_HOSTED_IDENTITY_ENVIRONMENT`。
- `OASIS7_HOSTED_WEBAUTHN_RP_ID`、`OASIS7_HOSTED_WEBAUTHN_ORIGINS`、`OASIS7_HOSTED_IDENTITY_INSTANCE_EPOCH`。
- 独立的 `OASIS7_HOSTED_IDENTITY_SESSION_KEY` 和授权 issuer；私钥只进入身份服务，运行节点只接收授权公钥。
- file 后端使用新 `.sqlite` 路径；Tablestore 使用显式 v2 表、instance、AK 或 STS 配置，并确认本地事务能力。不得自动回旧表、file 或空库。
- 启用邮箱或导入历史邮箱时配置稳定的 `OASIS7_HOSTED_IDENTITY_EMAIL_KEY` 与 key ID；SMTP 缺失只关闭邮件通道。
- `OASIS7_HOSTED_IDENTITY_TRUST_LOOPBACK_PROXY=1` 仅适用于受控 loopback 代理；代理覆盖 `X-Real-IP`，向世界 WebSocket 转发时清除账号 Cookie。直连请求不信任转发来源。
- `OASIS7_HOSTED_WORLD_ADMISSION_MODE=open_preview` 明确限定技术预览；不能据此宣称邀请码已原子消费。正式邀请资格仍须其独立 authority。

所有 secret 通过受限部署注入；不贴到日志、截图、PR、命令行参数或浏览器。数据库和备份使用受限权限。关闭 v2 后不得回到仅邮箱 writer 服务已创建的纯公钥账号。

<a id="contract-suite"></a>
## 契约与协议回归

首次编译准备共享 target。正式记录 canonical 命令和完整结果：

```bash
./scripts/cargo-dev.sh --prepare-target
env -u RUSTC_WRAPPER cargo test -p oasis7_launcher_backend --bin oasis7_game_launcher --no-default-features hosted_identity:: -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7 --lib --no-default-features hosted_authorization -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_launcher_backend --bin oasis7_game_launcher --no-default-features identity_request_security_tests
```

对应源码：身份 `hosted_identity/{store/tests.rs,session_tests.rs,credential_contract_tests.rs,email.rs,recovery.rs,webauthn.rs,world_tests.rs}`、签名授权 `viewer/auth/hosted_authorization_tests.rs`。型别化领域 fixture 只证明事务和权限状态机，不证明未调用的认证器或云服务。

必须观察：旧 ID 和 player 映射保留；失败不发布部分写入；挑战五次失败耗尽；冻结不被认证解开；共享凭证下单独退出会话不删凭证；撤销凭证失效全部派生会话；安全 epoch 和外部 epoch 阻止旧 session；最后凭证保护及策略版本重新核对；第 101 条历史记录仍可分页和定向撤销。

Tablestore loopback 测试证明签名请求、事务 ID、回滚、未知 commit receipt 和分页协议。上线声明该后端前，必须在隔离真实表运行同一事务场景，确认真实 Start/Commit/Abort、分区锁、故障及表能力。没有实测就保留 readiness 阻断。

<a id="virtual-authenticator"></a>
## Chromium 虚拟认证器

本地 disposable HTTPS fixture 使用 RP `localhost`、origin `https://localhost:8449`，只监听 loopback。测试使用临时证书和隔离数据库。测试信任临时证书仅限本地 fixture，公开站点必须通过正常 TLS 校验。

```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_launcher_backend --bin oasis7_game_launcher --no-default-features hosted_passkey_virtual_authenticator_fixture -- --ignored --nocapture
node scripts/hosted-passkey-virtual-authenticator-smoke.mjs --url https://localhost:8449/
```

第二条命令要求受控本地 TLS 代理转发到 fixture 的 `127.0.0.1:4179`。fixture 最长运行十分钟，停止文件位于系统临时目录的 `oasis7-passkey-virtual-fixture/stop`。不得将 fixture 端口或测试认证器公开。

脚本通过真实 `navigator.credentials.create/get` 和两个独立虚拟认证器验证 resident/UV required、注册、清 Cookie 后原账号登录、响应重试不重发 Cookie、备用凭证、撤销后旧凭证签名被拒、CSRF 和退出登录。测试不保存或打印私钥、Cookie、OTP 或完整标准响应。UI 取消和未知提交须保留原尝试；只有服务器确认取消、耗尽或未提交后才新建仪式。

## 真实诊断 runtime 授权合同

同候选构建 `oasis7_viewer_live` 与 launcher 测试 binary 后，可运行隔离协议验证：

```bash
node scripts/hosted-passkey-world-runtime-smoke.mjs \
  --runtime-bin <same-candidate-viewer-live> \
  --fixture-bin <same-candidate-launcher-test-binary> \
  --url https://localhost:8449/
```

脚本先确认 4179/5023 空闲，启动只监听 loopback 的 `minimal --no-llm` 诊断 runtime 与真实身份 HTTP handler。沿用上一节受控 TLS 代理；测试密钥、数据库和日志仅留在新建的 0700 临时目录，环境密钥文件为 0600。结果记录实际 binary SHA256；不得把旧 binary 的记录改成新候选通过。

必须观察真实注册 ack、超过 35 秒的续期、停止续期后的授权过期、第二设备仅登录不接管、明确接管隔离旧 key、旧 release 不影响新控制者、当前 release 提升 epoch 并撤销 runtime 控制。脚本有界关闭自己启动的进程；可选 `--hold-ms` 最多 180000，只用于浏览器采样。

此诊断世界关闭玩法，不使用付费模型，也不证明 DevLocal 链集成或正式 Skill 自主游玩。完整 Agent 游戏验收仍按 [自测手册](agent-runtime-self-playtest.manual.md) 选择场景、独立派工并核对权威世界结果。

<a id="real-device"></a>
## 真实设备、可访问性与邮箱

按配对 PRD 支持矩阵逐项记录 macOS Safari/Chrome、Windows Edge/Chrome、iOS Safari、Android Chrome 和一个支持用户验证的硬件安全密钥。验证主动点击、取消、PIN、第二凭证、清 Cookie、跨设备、撤销、受限环境替代入口、键盘和读屏状态。虚拟认证器不替代该矩阵。

使用操作者掌控的邮箱直接在 UI 输入验证码；不在聊天、日志或截图传播验证码。分别关闭 SMTP、新注册、恢复通道，验证已有 Passkey 登录继续可用。邮箱登录只建立账号 session，不把世界满员时的 OTP 放回待用状态。旧邮箱账号首次绑定保持原 ID；曾有公钥的账号不能用邮箱首次升级旁路。

恢复验证须绑定同一个预先登记候选，等待从 SMTP accepted 时刻计时，未成熟邮箱不能恢复；批准不跳过最终同邮箱 OTP 与候选断言。并发取消/完成只产生一个终态，旧凭证、旧 epoch 和旧会话不能复活。默认 clock health 为阻断；启用自动完成前确认可信 UTC，并验证本地墙钟跳变保护。

<a id="world-control"></a>
## 世界控制与撤销时限

两台设备分别登录同一账号，第二台只登录不得占 slot 或替换第一台。明确接管须提交用户确认和刚读取的 runtime epoch；候选注册失败保留旧控制，结果未知查询原尝试。旧 device/epoch 的 refresh/release 必须拒绝，重复已完成 release 保持幂等。

撤销凭证、退出账号 session、完成恢复和冻结后，持续打开的 WebSocket 仍必须在不超过 60 秒内拒绝新受认证操作。关闭身份服务、漏推和陈旧缓存不得延长期限；签名授权最长 30 秒，来源读取时刻约束其期限。纯登录和这些恢复动作均不得修改资产 signer 或开放 operator/node/governance 权限。

<a id="release-gates"></a>
## 迁移、性能与放量

已有 v2 数据后，暂停注册须保留 `OASIS7_HOSTED_IDENTITY_V2_ENABLED=1`，关闭 `OASIS7_HOSTED_ACCOUNT_REGISTRATION_ENABLED`。关闭或遗漏 v2 开关时，当前 launcher 对已存在的显式 SQLite 路径（含无法检查路径的错误）或已配置的 v2 表拒绝启动，不能降回旧邮箱 broker。清除存储配置或运行旧 binary 不受这项启动检查保护，部署仍须隔离旧 writer。

旧 writer 必须实际停止并撤去写权限，生成一致性来源快照，再显式导入和逐条核对账号、因子、player 映射。operator 的 fenced/history 声明不等于外部撤销历史已经恢复。灾备恢复先隔离，对账撤销/恢复终态，再单调提升外部 epoch；旧 RP、旧 signer 或缺历史的 credential 不能因可读备份自动恢复。

两种对外宣称支持的后端分别完成 10,000 账号、30,000 凭证、20 并发、5 次/秒持续十分钟的认证/登记负载，并记录硬件、P95、错误率及隔离限流 profile。不能用 store-only 或 loopback 协议耗时代替完整有效认证。首次用户观察至少 20 人，账号创建和世界等待分开计时。

在真实设备、真实后端、撤销时限、迁移、性能和恢复证据完成前，保留未验收边界。`open_preview`、开发构建成功或虚拟认证器成功均不升级根 README 的公开 claim。

## 网页导入密钥登录

反向代理上线前核对两条精确 POST 路由 `/api/public/hosted-account/v2/native-key/options` 与 `/api/public/hosted-account/v2/native-key/complete` 均转发到身份服务，沿用既有同源、CSRF、请求方法和来源头限制；不得为补路径而放开全部 `/api/`。`capabilities.native_key.available=true` 只说明后端能力，不能证明公网反代已开放路径。

使用同源浏览器上下文及新读取的 CSRF token 提交无凭据的无效 `{}` 请求，两条路径均应返回身份服务的 JSON `invalid_request`；这只验证路由可达，不证明认证成功。若公网返回 Nginx HTML 404，而 loopback 身份服务返回 JSON，先修复反代配置，再做下述专用无资产账号绑定、退出、导入登录与 session 核对。`world/join` 的 `registration_pending` 仍不是世界控制成功，继续核对同一会话的续期结果为 `world.state=bound`，不得自动接管其他设备。

`native_key.mode=import_only` 时，账号页提供“导入密钥登录”。它仅接受 Ed25519 公钥和 32 字节私钥种子，两者各为 64 位十六进制。私钥在本页内存中导入为不可导出的签名对象，不发送给服务器，也不保存到浏览器存储；提交后清空输入，关闭页面或退出后重新导入。此入口不提供密钥生成或加密备份，不能导入 Passkey，也不要使用钱包或节点私钥。

部署时先升级世界授权 verifier，再升级身份服务和 Viewer。首次绑定后，旧身份 binary 无法读取新增凭证类型；回退须保留导入凭证读取和登录能力，不能直接切回旧 binary。

首次使用先通过现有方式登录，在“账号与安全”中输入公私钥并点击“绑定这把密钥到当前账号”，使用现有凭证重新认证以批准绑定。之后退出，在登录页输入同一对密钥并点击“使用导入密钥登录”。未绑定公钥不会创建新账号。已登录且刷新页面后，可“加载已绑定密钥用于安全确认”；加载只保留本地对象，每次敏感操作仍需服务器验证新的签名挑战，错误账号的密钥不能获得授权。

验证覆盖：真实签名登录、原账号 ID 保持、无私钥网络字段/日志/storage、输入清空、未知 operation 查询及重试不增发 session、错误配对/用途/来源/环境/过期/重放拒绝、绑定需重新认证、全局唯一绑定、最后公钥保护、撤销后原生登录和派生会话失效、恢复时原生旧凭证一并撤销。浏览器不支持 Ed25519 Web Crypto 时使用已有登录方式；该导入入口没有 Passkey 的浏览器来源绑定和硬件用户验证保证。

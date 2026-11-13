# 开发测试账号 Workers 服务

本 crate 实现新的 `dev-test-accounts-v1` Worker/SQLite Durable Object 适配，承接 #5776、#5076 与[设计 §12.1](../../doc/p2p/blockchain/hosted-public-join-managed-identity-custody.design.md#dev-test-accounts-workers)。它不使用旧四角色 custody/TestSession，也不把业务转发给 native SDK 代理。身份／世界仍由固定 HTTPS authority 承担；签名、原件和工具预算由本服务持有。

## 构建与检查

安装固定 `worker-build 0.7.5` 以及 `wasm32-unknown-unknown` target；使用仓库固定 Rust 工具链和 Node 22+。在本目录执行 `./scripts/verify.sh`，依次运行 host 合同、Wasm Clippy、固定 npm lock 安装、真实 Wasm 打包、workerd/SQLite 回归和 Wrangler dry-run。打包产物在忽略的 `build/` 中。`npm test` 需要先构建，不能用空入口代替实际 Wasm 测试。

生产 Rust 模块同时接受 host 与 Wasm 类型检查；host 构建没有文件系统、native SDK 或 HTTP 服务 fallback。实际运行入口仍为 Worker/DO，host 单元测试只调用纯合同逻辑。

本地组件证据不代表真实 authority、Cloudflare 发布或真实外部 Agent 验收。云端业务部署与验收由 #5777 继续记录。

## 部署配置与资源

`wrangler.toml` 仅声明新应用 `oasis7-dev-test-5076` 和一个 SQLite DO namespace；使用 `workers.dev`，不创建自定义域、付费资源或日志收集，不修改账户已有 Worker。普通 fetch 入口只路由 `/mcp` 到单个 actor；业务在 DO 内执行。

必须先以私有、本地文件准备 `DEV_TEST_POLICY` binding，匹配实际发布 origin、固定 identity origin、当前合法账号／世界／Executor fence 及独立 service key。配置只引用 Secrets 名称，禁止把 cookie、私钥、模型认证、宿主路径写入该文件。`Resource` 的序列化摘要必须等于 grant 的 `executor_config_digest`；无匹配材料时服务拒绝启动业务。

每个 caller 使用独立 `DEV_TEST_*` Bearer Secret（32 字节小写 hex），每个 resource 使用独立材料 Secret JSON，字段仅为 `executor_seed` 和 `identity_cookie`。`DEV_TEST_STATE_KEK` 为独立随机 32 字节小写 hex 的 AES-GCM 包装密钥。材料应来自显式新建的隔离测试主体；不复用模型供应商认证。禁止把 Secrets 写入终端输出、Issue、PR、普通变量或测试归档。仅固定 public key、匿名引用和不可逆摘要可进入报告。

未配置、错误或缺失的 Secret、HTTPS 校验失败、身份或完整 fence 不一致均拒绝业务；不自动登录、换 key、重建世界或扩大权限。

Cloudflare 单个变量／Secret 上限为 5KB。双账号完整 fence 可能超出该值，不能只依据本地 60KiB 配置上限上传单个 Secret。可信 operator 使用 `operator_policy <公共policy文件> --bindings` 生成可合并到 `wrangler secret bulk` 输入的公共配置绑定映射：小配置仍使用 `DEV_TEST_POLICY`；较大配置使用 `DEV_TEST_POLICY_MANIFEST` 和最多16个 `DEV_TEST_POLICY_PART_01`～`_16`，每片最多4096 UTF-8字节，总计最多60KiB。manifest固定版本1、片数及完整文本BLAKE3，Worker完整重组、验摘要后才解析和使用配置。缺片、损坏、混合单片和manifest均拒绝，不回退。输出中的null用于在同一次bulk更新中删除旧表示／旧片；不要过滤这些null或单独覆盖某片。先部署支持分片的代码，再提交绑定更新；核对材料、凭据、历史残留与配置合计不超过Free的64个变量。原始配置输出与现有单片调用保持兼容；分片不轮换服务授权、不清零账本，也不改变会话合同。

## 持久化与恢复边界

DO 中的 policy tombstone、初始化锚和每个 grant/caller 的账本通过同步 SQLite CAS 更新。业务准入和额度必须落账后才发 native 请求。metadata 不消耗业务额度，close 按合同不扣额；失败／未知请求不返还已经保留的额度。

业务原件使用独立 KEK 加密，AAD 绑定完整 resource scope、kind 和原操作 ID；先持久化再发送。重放及查询只读取原 Context/Move，不重新签名或再次提交。响应丢失、缓存丢失、损坏、错误 KEK、缺 marker、scope 冲突均保留 unknown，不能当作未开始。

平台休眠或同授权实例替换从 SQLite 恢复尚未关闭的不可变 session，仍按当前身份、完整 grant 绑定与期限重验；缺失／损坏不补建，关闭使用永久墓碑。逻辑服务重启或备份恢复必须轮换 service_grant_id，旧 session 失效；账本和原件保留。策略 generation 不可回退，同版本不能改变授权合同；删除再添加 grant 必须提升 authorization version。正式恢复必须发布更高 generation、全新 `service_grant_id` 和显式更高版本授权，原 grant 永久退休。需要查历史时显式配置 `history_grants`；只有已退休且同 caller、同完整 scope 的原件可供 query-only lookup，不能借历史 Context 新发 Move。不得回滚 native world/key/binding 或将旧 SQL 备份当作当前权限。

目前没有向模型公开的管理、导出、清空或恢复路由。备份、恢复和 Secret 轮换必须由可信 operator 离线准备并独立核对；没有真实云端恢复结果前不能声称已验证完整恢复。

## 有限验收

先固定源码、构建、公开 origin、authority trust、账号 A/B、完整 fence、到期时间、有限 max_calls、允许坐标及每一步操作 ID。外部 Agent 仅接收受限 MCP 工具和正式游戏 Skill；connector 保管 Bearer。独立核对 canonical 原操作结果及实际世界效果，不以 HTTP 200、accepted 或 fixture 替代业务成功。对跨响应丢失／重启的同原件恢复、错误 scope、异正文、过期／停用／关闭及余额连续性分别保留证据。

## 受信本地材料交接

隔离账号已通过正式 SDK 完成登录与合法 Executor 绑定后，可用 `cargo run -p oasis7_client_storage --example dev_test_worker_material --locked -- --identity-config <私有配置> --login-operation <原登录ID> --executor-directory <原vault> --unlock-file <私有unlock> --expected-account <已核账号> --expected-executor <已核fence公钥> --output <新私有文件>` 生成一次性材料文件。该 operator 工具只读原 SDK vault／session，核对账号、Executor 与独立 identity key，不生成／替换 key，不发网络请求；结果仅写新建0600文件，拒绝覆盖、symlink、FIFO／设备、宽权限或错误 scope，不打印秘密。

材料文件须由可信 operator 直接导入指定 Worker Secret，不能经过模型消息、普通配置或日志。导入后按运行终点保留或清理私有交接文件；Worker 仍须在线重验账号／权限与完整 fence，离线导出不代表当前可用。它不属于 Worker 的生产依赖或模型工具入口，也不提供备份恢复或权限重授权能力。

## 明确传输别名

`Resource.endpoint` 固定实际 HTTPS 传输目的地；可选 `identity_audience` 仅指定既有身份 authority 的 Origin，省略时等于 endpoint。显式别名也纳入资源摘要与 grant pin，不能由工具参数指定；原 world、RP、身份 scope、服务公钥和完整 fence 不变。别名必须由可信 operator 验证，不能用它绕过 TLS 或重绑已有 SDK 配置。

`cargo run -p oasis7_dev_test_worker --example operator_policy --locked -- <公共policy文件>` 只接受不超过60KiB的普通文件，完成闭合配置验证后补齐空的 resource pin；已有错误 pin 会被拒绝。该离线工具无网络或 Secret 导出能力，不属于 MCP 入口。


## 独立通用账号服务

`POST /accounts/mcp` 使用标准 MCP 初始化、发现与 `account_service_call`。它与旧 `/mcp` 游戏工具采用不同 DO actor 名称；不需要任何 `DEV_TEST_*` 绑定、本机进程、游戏配置或外部身份后端。工具仅允许 `accounts`、`open`、`describe`、`close`，没有游戏动作、秘密读取或远程管理方法。

可信管理员通过版本化 Secrets 发布 `ACCOUNT_SERVICE_POLICY`（完整 JSON 不超过5120 UTF-8字节）和引用的 Secrets。示例仅含公开引用：

```json
{"version":1,"generation":1,"service_id":"example-run","origin":"https://YOUR-WORKER.workers.dev","expires_at_unix":2000000000,"max_calls":100,"credentials":[{"caller_id":"consumer","token_secret":"ACCOUNT_SERVICE_CONSUMER_TOKEN","expires_at_unix":2000000000,"revoked":false}],"accounts":{"A":{"alias":"Development account","account_id":"account-A","provider":"example","environment":"test","credential_secret":"ACCOUNT_SERVICE_A_CREDENTIAL_V1","credential_version":1,"disabled":false}},"grants":[{"grant_ref":"consumer-A","caller_id":"consumer","account_ref":"A","authorization_version":1,"expires_at_unix":2000000000,"enabled":true}]}
```

示例时间仅用于展示结构，实际测试须提供短期有限期限。consumer token 是独立随机的64位小写十六进制 Secret；账号 credential 是独立的非空 Secret（不超过5120字节），不能把 policy 或消费者 token 当作账号 credential。服务核对 Secret 存在性并保管引用，不验证凭据能否登录第三方服务。公开账号字段不得放入密码、Cookie或私密目标。原始凭据、Secret名称和token不进入工具回复。

工具调用示例：

```json
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"account_service_call","arguments":{"method":"accounts"}}}
```

从目录选择 grant_ref 后，用 `open` 携带绝对 Unix 秒 `expires_at_unix`（不超过服务、调用者及 grant 的最早到期时间），保留 session_ref；`describe` 和 `close` 携带 session_ref。每个成功 accounts/open/describe 调用扣同 service/caller 的一次额度，结果 `value.usage` 返回累计用量和上限。拒绝不扣额；close不扣额，额度耗尽时仍能关闭。服务不设置全局“当前账号”，并行会话各自固定账号。

别名变化、新增账号及普通 Worker 实例替换保留既有会话及累计用量。轮换账号 credential Secret 引用/版本、改变provider/environment、停用账号/授权或改变调用者凭据时，提高受影响 grant 的 authorization_version；旧会话随安全合同变化拒绝，不能用旧版本恢复移除的grant。每次配置更新提高generation，同 generation 不同正文及generation回退拒绝。同service_id不得提高max_calls或延长期限；新的显式有限授权使用新service_id，旧ID退休且不能重用，历史计数不删除。策略的可信高水位先持久化，即使所有消费者已被撤销，也不能通过之后回退配置复活旧授权。

边界：每份policy最多16个调用者、32个账号、64个grant；DO保留最多64个会话引用、128个grant高水位和128个累计用量条目、32个退休service ID，完整状态不超过64KiB。容量不足或持久记录缺失/损坏拒绝，不静默清零或淘汰安全记录。当前不提供管理后台、明文凭据导出、第三方登录自动续期或平台备份回滚API；这些不由目录/会话验收代签。可信管理员做备份恢复时先发布新的service_id和有限授权，旧授权退休。

验证：`cargo test -p oasis7_dev_test_worker --lib --locked`；构建实际Wasm后 `node --test tests/account-service.test.mjs` 验证没有任何游戏绑定、outbound调用为0，以及账号切换、授权隔离、轮换、停用、额度与SQLite实例重建。实际Cloudflare部署和版本替换单独记录，不用Miniflare代签云端。


### 策略发布与生效核对

Cloudflare活跃DO默认可能延迟采用新版本300秒；实测普通 `secret bulk` 成功后的下一次请求仍读到旧别名。本应用把普通部署的DO更新策略设为immediate。策略/凭据更新使用以下两步（在包目录运行，Secret文件仅可信运营者可读）：

```sh
node node_modules/wrangler/bin/wrangler.js versions secret bulk /PRIVATE/PATH/bindings.json
node node_modules/wrangler/bin/wrangler.js versions deploy CREATED_VERSION_ID@100% --yes --durable-objects-code-update-mode immediate
```

版本ID取第一步真实返回值，不能复用旧版本或输出秘密文件内容。普通 `wrangler secret bulk` 走独立API且不携带DO策略，不能作为停用/轮换已完成的证明。发布后回读业务响应中的 `value.policy_generation`、原session及累计用量，再判断新策略生效；保留旧版本传播期间的失败记录。立即更新可能中断在途请求，消费者按未知结果处理，不以重置存储或重发游戏动作恢复。

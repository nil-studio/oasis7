# NewAPI Bridge Service Operator Runbook

- 适用范围: `oasis7_newapi_bridge_service` 的最小 operator 部署。
- 目标: 持有 LetAI 管理员 key，在 `bind` 时动态创建/确保 `platform user + project + token_key`，并把结果写入共享 `bridge-state.json`，供 remote provider bridge 自动映射读取。

## 目标端点

- `GET /health`
- `GET /v1/bridge/health`
- `POST /v1/bridge/bind`
- `POST /v1/bridge/deposit-route`
- `POST /v1/bridge/reconcile`

当前 runtime 自动映射的最低要求是 `POST /v1/bridge/bind` 能成功把 `token_key` 写进 state。

## 仓库资产

- 启动脚本: `scripts/newapi-bridge-service/start-newapi-bridge-service.sh`
- 环境变量模板: `scripts/newapi-bridge-service/newapi-bridge-service.env.example`
- systemd 模板: `scripts/newapi-bridge-service/oasis7-newapi-bridge.service`

## ECS 目录建议

```text
/opt/oasis7/newapi-bridge/
  current/
  releases/<release-id>/
/etc/oasis7/newapi-bridge-service.env
/etc/oasis7/newapi-bridge/bridge-state.json
```

## 当前环境矩阵

本矩阵只描述 NewAPI bridge / remote provider bridge，不改变 hosted-login、testnet、mainnet 的环境命名。

| Lane | Host | NewAPI bridge | State | 当前验证 |
| --- | --- | --- | --- | --- |
| 测试环境 | `39.104.204.172` | `oasis7-newapi-bridge.service`，`active + enabled`，监听 `127.0.0.1:5852` | 独立测试 state: `/etc/oasis7/newapi-bridge/bridge-state.json`；可按 runbook 重置 | `GET /v1/bridge/health` 返回 `ok=true`；已通过本机 provider bridge 调用 `letai/gpt-5.4` smoke |
| 正式环境 | `39.104.205.67` | `oasis7-newapi-bridge.service`，`active + enabled`，监听 `127.0.0.1:5852` | 正式 state: `/etc/oasis7/newapi-bridge/bridge-state.json`；保留既有 active binding / project binding | `GET /v1/bridge/health` 返回 `ok=true`；已通过本机 provider bridge 调用 `letai/gpt-5.4` smoke |

两套环境当前都部署自 CI artifact `newapi-bridge-service-linux-x86_64-f628b2ab0fbb88a392d44a9d92c6b5147eaeda4f`，release 目录为 `/opt/oasis7/newapi-bridge/releases/20260529-205947-f628b2a`。

## 必需输入

- `OASIS7_NEWAPI_BRIDGE_LETAI_BASE_URL`
- `OASIS7_NEWAPI_BRIDGE_LETAI_PLATFORM_KEY`
- `OASIS7_NEWAPI_BRIDGE_STATE_PATH`

建议同时明确:

- `OASIS7_NEWAPI_BRIDGE_LETAI_PARENT_CHANNEL_ID`
- `OASIS7_NEWAPI_BRIDGE_BIND_ADDR`
- `OASIS7_NEWAPI_BRIDGE_ROUTE_TTL_SECONDS`

- `OASIS7_NEWAPI_BRIDGE_LETAI_PARENT_CHANNEL_ID` 为空时，bridge-service 不向 LetAI project upsert 请求传 `parent_channel_id`，由平台侧默认渠道策略决定 project token 归属。
- 只有 operator 明确需要将生成的 project token 绑定到指定 LetAI channel 时，才设置 `OASIS7_NEWAPI_BRIDGE_LETAI_PARENT_CHANNEL_ID`。

## 启动后最小验证

```bash
curl -sS http://127.0.0.1:5852/v1/bridge/health

curl -sS -X POST http://127.0.0.1:5852/v1/bridge/bind \
  -H 'Content-Type: application/json' \
  -d '{"newapi_user_ref":"user-1","oasis_sender_account_id":"oc:pk:user-1"}'
```

成功后，`/etc/oasis7/newapi-bridge/bridge-state.json` 至少应出现:

- `bindings[].newapi_user_ref`
- `bindings[].platform_user_id`
- `project_bindings[].platform_project_id`
- `project_bindings[].token_key`

## 全链路测试用户模式

早期测试环境可使用假邮箱作为 hosted login / QA persona 标签。假邮箱不要求真实收信，但必须清楚标记为非生产，并避免与真实用户冲突。

推荐映射：

```text
email=oasis7-e2e-001@test.invalid
external_user_name=Oasis7 E2E User 001
project_name=oasis7-e2e-001
oasis_sender_account_id=oc:pk:<public-key-or-account-id>
newapi_user_ref=pk_<public-key-fingerprint>
provider_auth_token=newapi_user_ref:pk_<public-key-fingerprint>
```

约束：

- 私钥由测试环境安全随机生成，不能从邮箱派生。
- 私钥只写入 `~/Documents/keys/test_keys.txt` 或受控 secret store，不写入 repo、bridge state、provider auth token、CI log。
- `newapi_user_ref` 使用公钥或公钥 fingerprint，避免把邮箱作为 provider bearer 主选择器。
- `email` 可传给 `POST /v1/bridge/bind`，用于 LetAI platform user metadata / QA 审计。
- bind response 不能暴露 `platform_user_id`、`platform_project_id` 或 raw `token_key`；这些只允许存在于受控 bridge state。

示例 bind:

```bash
curl -sS -X POST http://127.0.0.1:5852/v1/bridge/bind \
  -H 'Content-Type: application/json' \
  -d '{
    "newapi_user_ref": "pk_<public-key-fingerprint>",
    "oasis_sender_account_id": "oc:pk:<public-key-or-account-id>",
    "external_user_name": "Oasis7 E2E User 001",
    "email": "oasis7-e2e-001@test.invalid",
    "project_name": "oasis7-e2e-001",
    "project_metadata": {
      "purpose": "bridge-full-chain-e2e",
      "email_login_env": "test",
      "identity_root": "fake-email-plus-chain-public-key"
    }
  }'
```

## 与 Remote Provider Bridge 串接

remote provider bridge 可读取同一受控 state，但自动映射的 selector 仅用于路由，
不是调用者认证。先按 [Remote Provider Bridge 认证与授权前提](provider-remote-https-bridge-operator-runbook.md#认证与授权前提)
核实独立认证与 caller → selector 授权绑定，或严格可信调用边界；未核实前不得将
下面配置用于公网入口。全局 bridge auth token 在自动映射分支不构成额外校验，
公钥 fingerprint、TLS、限流及 state 中的 active binding 也不替代该前提。

配置示例:

```bash
OASIS7_PROVIDER_AUTH_ROUTE_FROM_BEARER=true
OASIS7_REMOTE_LLM_NEWAPI_BRIDGE_STATE_PATH=/etc/oasis7/newapi-bridge/bridge-state.json
```

此时 client 直接传:

```bash
--agent-provider-auth-token newapi_user_ref:<newapi_user_ref>
```

provider bridge 会自动解析出对应的 `token_key`。

## 测试步骤入口

完整的测试环境全链路步骤以 `doc/p2p/token/mainchain-token-newapi-quota-bridge.runbook.md#67-全链路测试步骤矩阵` 为准，覆盖：

- 从 `~/Documents/keys/test_keys.txt` 选择 fake-email + chain keypair persona。
- `POST /v1/bridge/bind` 建立 `newapi_user_ref -> bridge_user_id -> LetAI project/token_key` 绑定。
- `POST /v1/bridge/deposit-route` 创建充值路由。
- 通过已签名的 chain transfer submit / faucet / operator 工具完成 testnet 入账。
- `POST /v1/bridge/reconcile` 推进 happy path 或 manual-review path。
- 用 `newapi_user_ref:<ref>` bearer 调 provider smoke，确认额度消耗归属到同一个测试用户。

注意：测试步骤中可以记录 `newapi_user_ref`、`bridge_user_id`、`route_id` 和 tx/action id；不得记录 raw `token_key` 或链上私钥。
完整 happy path 需要先确认测试 lane 有可执行的签名转账工具或 operator/faucet 等价入账路径；否则只能完成环境、bind 与 deposit-route rehearsal。

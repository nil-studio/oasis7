# Remote HTTPS Provider Bridge Operator Runbook

- 适用范围: `agent_provider_transport=remote_https` 的 repo-owned 参考装配。
- 当前参考实现: `oasis7_provider_local_bridge` + `nginx` HTTPS 反向代理。

## 目标端点

target lane 对外必须暴露以下 4 个 provider contract 端点:

- `GET /v1/provider/info`
- `GET /v1/provider/health`
- `POST /v1/world-simulator/decision-context`
- `POST /v1/world-simulator/feedback-context`

bridge 还可保留以下 bare 路由用于 legacy compatibility-only 输入：

- `POST /v1/world-simulator/decision`（旧 `DecisionRequest` / `DecisionResponse` DTO）
- `POST /v1/world-simulator/feedback`（旧 `FeedbackEnvelope` DTO）

bare 路由不承载 `ContinuousAgentRequestContextV1`、`ContinuousAgentResponseContextV1` 或
`FeedbackEnvelopeV1`，不得作为 target cognition、receipt 或 production async 证明。当前
HTTP body 没有 `compatibility_lane` 字段；路由/adapter 配置是 lane 选择依据。

repo-owned 参考装配中，这些端点由 `oasis7_provider_local_bridge` 提供，公网 `https://` 入口由 `nginx` 代理到本机 `127.0.0.1:5841`。参考 nginx 模板已包含按 IP 与 `Authorization` 的限流，以及连接数限制，用来降低单个 bearer 快速耗尽上游配额的风险。

## 仓库资产

- Per-user route manager: `scripts/provider-remote-https/manage-user-route.py`
- Bridge 启动脚本: `scripts/provider-remote-https/start-remote-provider-bridge.sh`
- 环境变量模板: `scripts/provider-remote-https/remote-provider-bridge.env.example`
- systemd 模板: `scripts/provider-remote-https/oasis7-remote-provider-bridge.service`
- nginx 模板: `scripts/provider-remote-https/t2t.oasis7.tech.nginx.conf`

systemd 模板默认使用低权限专用用户:

- remote provider bridge: `oasis7-provider`
- newapi bridge: `oasis7-newapi`

部署时须先创建对应 user/group，并把 env/state 文件权限授权给这些用户。

## ECS 目录建议

```text
/opt/oasis7/remote-provider-bridge/
  current/                      # 指向当前发布目录
  releases/<release-id>/        # 每次上传一份独立 release
/etc/oasis7/remote-provider-bridge.env
/etc/nginx/ssl/t2t.oasis7.tech.pem
/etc/nginx/ssl/t2t.oasis7.tech.key
```

## 必需输入

- `OASIS7_REMOTE_LLM_API_KEY`
- `OASIS7_REMOTE_LLM_MODEL`
- `OASIS7_PROVIDER_BRIDGE_AUTH_TOKEN`

推荐同时明确:

- `OASIS7_REMOTE_LLM_BASE_URL`
- `OASIS7_REMOTE_LLM_HEALTH_URL`
- `OASIS7_REMOTE_LLM_TIMEOUT_MS`
- `OASIS7_REMOTE_LLM_MAX_OUTPUT_TOKENS`
- `OASIS7_REMOTE_LLM_USER_AGENT`

`OASIS7_PROVIDER_BRIDGE_AUTH_TOKEN` 建议至少用:

```bash
openssl rand -hex 32
```

## Per-User Token Key Mode

当前参考装配可实现“每个用户一个 `token_key`”：按 `bridge bearer token -> route label -> upstream token_key/model/base_url` 路由。

还支持自动映射模式：`bridge bearer token -> newapi bridge state -> token_key`。此处 Authorization bearer 字段承载路由 selector，不是调用者拥有该用户的证明；满足下述安全前提后，才可省去单独维护的 auth-routes / llm-routes JSON。

需要两份 JSON:

- `/etc/oasis7/remote-provider-auth-routes.json`
  - `bridge bearer token -> route label`
- `/etc/oasis7/remote-provider-llm-routes.json`
  - `route label -> { api_key, model, base_url }`

示例模板已预铺在 ECS:

- `/etc/oasis7/remote-provider-auth-routes.json.example`
- `/etc/oasis7/remote-provider-llm-routes.json.example`

启用 per-user mode 时，正式 env 需要至少新增:

```bash
OASIS7_PROVIDER_AUTH_ROUTE_MAP_PATH=/etc/oasis7/remote-provider-auth-routes.json
OASIS7_REMOTE_LLM_ROUTES_PATH=/etc/oasis7/remote-provider-llm-routes.json
```

此时可不再使用单一 `OASIS7_PROVIDER_BRIDGE_AUTH_TOKEN` / `OASIS7_REMOTE_LLM_API_KEY` 作为唯一真值。

### 自动映射模式

#### 认证与授权前提

`newapi_user_ref`、`bridge_user_id` 及公钥 fingerprint 都是路由标识，不是秘密，
也不证明调用者拥有该用户或有权使用其额度。TLS 和按 IP/Authorization 限流不能
替代调用者认证与 caller → selector 授权绑定。

当前 `http_bridge_support.rs::authorize_request` 先使用非空静态 auth route map；
仅在该 map 为空时进入自动映射分支。该分支解析 selector 并查询 state，不再叠加
`OASIS7_PROVIDER_BRIDGE_AUTH_TOKEN` 的比较。配置该全局 token 不能被当作自动映射
的第二层认证；state 中的 active binding / token_key 也只证明路由可用。

推荐不要手改 JSON，直接用脚本给用户签发 bridge token:

```bash
python3 scripts/provider-remote-https/manage-user-route.py upsert-user \
  --auth-routes /etc/oasis7/remote-provider-auth-routes.json \
  --llm-routes /etc/oasis7/remote-provider-llm-routes.json \
  --user alice \
  --api-key <alice-provider-api-key> \
  --model gpt-5.4
```

脚本行为:

- 若用户没有 bridge token，脚本会生成随机 bearer token。
- 若用户已有 bridge token，默认沿用原 token，只更新上游 `api_key/model/base_url`。
- 需要轮换 bridge token 时，加 `--rotate-bridge-token`。
- 运行成功后会输出该用户应使用的 bridge bearer token 和一条 `curl` 样例。

## 本机验证

在 ECS 上准备好环境变量文件后，启动 remote provider bridge:

```bash
/opt/oasis7/remote-provider-bridge/current/scripts/provider-remote-https/start-remote-provider-bridge.sh
```

另开终端验证 provider 合同:

```bash
curl -sS -H "Authorization: Bearer $OASIS7_PROVIDER_BRIDGE_AUTH_TOKEN" \
  http://127.0.0.1:5841/v1/provider/info

curl -sS -H "Authorization: Bearer $OASIS7_PROVIDER_BRIDGE_AUTH_TOKEN" \
  http://127.0.0.1:5841/v1/provider/health
```

per-user mode 下，可先签发一个测试用户，再用脚本输出的 bearer token 验证:

```bash
python3 /opt/oasis7/remote-provider-bridge/current/scripts/provider-remote-https/manage-user-route.py upsert-user \
  --auth-routes /etc/oasis7/remote-provider-auth-routes.json \
  --llm-routes /etc/oasis7/remote-provider-llm-routes.json \
  --user alice \
  --api-key <alice-provider-api-key> \
  --model gpt-5.4

curl -sS -H "Authorization: Bearer <alice-bridge-token>" \
  http://127.0.0.1:5841/v1/provider/info
```

## nginx 验证

真实环境门禁优先使用 live gate（不打印 raw key、密码或私钥）。

```bash
./scripts/provider-remote-https/provider-bridge-live-gate.sh \
  --target-context-payload-file <runtime-issued-context-pairs.json> \
  --decision-count 1
./scripts/provider-remote-https/provider-bridge-live-gate.sh \
  --target-context-payload-file <runtime-issued-context-pairs.json> \
  --decision-count 1 --loopback-target 204 --skip-public
./scripts/provider-remote-https/provider-bridge-live-gate.sh \
  --target-context-payload-file <runtime-issued-context-pairs.json> \
  --decision-count 1 --loopback-target 205 --skip-public
./scripts/provider-remote-https/provider-bridge-live-gate.sh \
  --target-context-payload-file <runtime-issued-context-pairs.json> \
  --decision-count 1 --skip-loopback
```

如需仅核对 bare 兼容路径，必须显式使用
`--legacy-compatibility-only`；该模式输出只属于 legacy evidence，不能作为 target
cognition、Runtime receipt 或 production async 证明。

长跑额度消耗/低额度验证也必须走真实环境，使用 dedicated lowquota persona 后再执行:

```bash
./scripts/provider-remote-https/provider-bridge-live-gate.sh \
  --target-context-payload-file <runtime-issued-context-pairs.json> \
  --skip-public \
  --loopback-target 204 \
  --lowquota-target 204 \
  --lowquota-decision-count 20

./scripts/provider-remote-https/provider-bridge-live-gate.sh \
  --target-context-payload-file <runtime-issued-context-pairs.json> \
  --skip-loopback \
  --lowquota-target public205 \
  --lowquota-decision-count 20
```

默认 CI 只跑无副作用的 provider contract 回归；真实环境门禁需要凭据和公网可达性，
并且必须提供 Runtime-issued pair artifact；用
`OASIS7_PROVIDER_LIVE_TARGET_CONTEXT_PAYLOAD_FILE=/secure/path/runtime-issued-context-pairs.json
OASIS7_CI_RUN_PROVIDER_LIVE_GATE=true ./scripts/ci-tests.sh --required` 显式启用。
mock 测试只用于保护 harness 解析和失败判定，不作为环境通过证据。

公网入口验证:

```bash
curl -sS -H "Authorization: Bearer <token>" https://t2t.oasis7.tech/v1/provider/info
curl -sS -H "Authorization: Bearer <token>" https://t2t.oasis7.tech/v1/provider/health
```

自动化合同 smoke:

```bash
./scripts/provider-remote-https/provider-bridge-contract-smoke.sh \
  --base-url https://t2t.oasis7.tech \
  --auth-token <token> \
  --target-context-payload-file <runtime-issued-context-pairs.json> \
  --decision-count 1 \
  --min-successes 1
```

该脚本同时验证 `/v1/provider/info`、`/v1/provider/health`、target
`POST /v1/world-simulator/decision-context` 与配对的
`POST /v1/world-simulator/feedback-context`。`<runtime-issued-context-pairs.json>`
必须是 Runtime 产生的 JSON wrapper artifact，根对象为
`{"requests":[{"decision_context":<ContinuousAgentRequestContextV1>,
"feedback_context":<FeedbackEnvelopeV1>}]}`；脚本会在发出请求前拒绝 bare DTO、缺失
identity/capability context、非 canonical digest、错配 feedback 或不足的 pair 数量。
provider bridge 仍会执行其自身的 canonical hash、Runtime binding 与 feedback 合同校验；
本地脚本不把自造 payload 或脚本输出升级为 Runtime attestation。默认允许 health 返回
`degraded`，因为上游 health URL 可能返回 `HTTP 401`；target cognition proof 仍要求两个
target POST 都成功。若需要把 health 也作为硬门禁，追加 `--require-health-ok`。

如需单独审计旧兼容入口，必须显式标记为 legacy，且该结果不能替代 target cognition proof：

```bash
./scripts/provider-remote-https/provider-bridge-contract-smoke.sh \
  --base-url https://t2t.oasis7.tech \
  --auth-token <token> \
  --legacy-compatibility-only \
  --decision-count 1 \
  --min-successes 1
```

若上游对默认 User-Agent 做了额外拦截，可在 env 里显式覆写:

```bash
OASIS7_REMOTE_LLM_USER_AGENT=curl/8.5.0
```

## Runtime 接入

```bash
env -u RUSTC_WRAPPER cargo run -p oasis7_launcher_backend --bin oasis7_game_launcher -- \
  --scenario llm_bootstrap \
  --with-llm \
  --agent-provider-mode provider_backed \
  --agent-provider-backend provider_local_bridge \
  --agent-provider-contract worldsim_provider_v1 \
  --agent-provider-transport remote_https \
  --agent-provider-url https://t2t.oasis7.tech \
  --agent-provider-auth-token <token> \
  --agent-provider-connect-timeout-ms 15000 \
  --agent-provider-profile oasis7_p0_low_freq_npc \
  --agent-execution-lane player_parity
```

## 当前已知边界

- 该参考装配复用 `oasis7_provider_local_bridge` 的 provider contract 和 prompt/decision parser，因此 `provider_id` 仍会表现为 `provider_local_bridge`。
- `provider_health` 默认通过 `OASIS7_REMOTE_LLM_HEALTH_URL` 做 upstream 探针；若上游兼容服务不支持该路径，需要显式改成可用的健康检查地址。
- 在未提供真实上游 `api_key/model` 前，ECS 端只能部署模板和二进制，不能完成最终 smoke。

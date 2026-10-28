# Remote HTTPS Provider Bridge Operator Runbook

- 适用范围: `agent_provider_transport=remote_https` 的 repo-owned 参考装配。
- 当前参考实现: `oasis7_provider_local_bridge` + `scripts/provider-remote-https/letai_provider_cli.py` + `nginx` HTTPS 反向代理。
- 本 runbook 不覆盖 `oasis7_newapi_bridge_service`；后者是 `OC -> LetAI quota/token_key` 额度桥，不是 runtime decision provider。

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

repo-owned 参考装配中，这些端点由 `oasis7_provider_local_bridge` 提供，公网 `https://` 入口由 `nginx` 代理到本机 `127.0.0.1:5841`。参考 nginx 模板已包含按 IP 与 `Authorization` 的限流，以及连接数限制，用来降低单个 bearer 快速打空 LetAI quota 的风险。

<a id="当前环境矩阵"></a>
## 历史环境观察（2026-06-04）

本矩阵保留 2026-06-04 的 remote provider bridge 观察，不代表当前部署配置或认证边界已复核；NewAPI bridge 自身是额度/token state 服务。

| Lane | Host | Provider bridge | 对外入口 | State 来源 | 历史 live gate |
| --- | --- | --- | --- | --- | --- |
| 测试环境 | `39.104.204.172` | `oasis7-remote-provider-bridge.service`，监听 `127.0.0.1:5841` | 暂无公网域名；本机/内网验证优先 | 本机 `/etc/oasis7/newapi-bridge/bridge-state.json` | 2026-06-04 live gate: state realigned to the same NewAPI project/token as 205; bare legacy decision smoke passes with `provider_version=letai/gpt-5.4` |
| 正式环境 | `39.104.205.67` | `oasis7-remote-provider-bridge.service`，监听 `127.0.0.1:5841` | `https://t2t.oasis7.tech` | 本机 `/etc/oasis7/newapi-bridge/bridge-state.json` | 2026-06-04 live gate: bare legacy decision smoke passes with `provider_version=letai/gpt-5.4`; current Codex control host to public TLS is reset before nginx, while 204-to-205 public TLS/HTTP succeeds; this is not target cognition proof |

该历史窗口记录两套 provider bridge 启用自动映射模式：client 传 `newapi_user_ref:<user_ref>` 或 `bridge_user_id:<id>`，provider bridge 从本机 NewAPI bridge state 解析对应 `token_key`。`GET /v1/provider/health` 可能因上游 health URL 返回 `HTTP 401` 而显示 `degraded`；是否能调用模型以 target `/v1/world-simulator/decision-context` smoke 为准。若使用仓库现有的 bare-route contract smoke，结果只能证明 legacy compatibility 路径可达，不能替代 target cognition 验证。

## 仓库资产

- Wrapper: `scripts/provider-remote-https/letai_provider_cli.py`
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

部署 owner 必须先核实：入口已独立认证调用主体并约束其可选 selector，或整个入口
严格隔离于仅获准操作这些用户额度的可信调用方。仅监听 loopback 不足以证明经反向
代理后的入口仍满足该前提。未核实前不得把下面的自动映射示例作为公网安全配置。
本节不宣称任何现网已暴露、发生费用损失或完成安全修复；实现、部署变更及真实调用
另行验证和授权。格式解析与认证授权分别验收，禁止裸格式也不能替代授权检查。

满足上述安全前提，且 `oasis7_newapi_bridge_service` 已在持久化 state 里维护 `newapi_user_ref -> bridge_user_id -> token_key` 后，自动映射配置示例为:

```bash
OASIS7_PROVIDER_AUTH_ROUTE_FROM_BEARER=true
OASIS7_REMOTE_LLM_NEWAPI_BRIDGE_STATE_PATH=/path/to/newapi-bridge/bridge-state.json
```

此时:

- Client 传 `--agent-provider-auth-token newapi_user_ref:<user_ref>`，bridge 会自动按 `newapi_user_ref` 找 `token_key`
- 或传 `--agent-provider-auth-token bridge_user_id:<id>`，bridge 会按 `bridge_user_id` 查找
- 不再需要 `/etc/oasis7/remote-provider-auth-routes.json`
- 不再需要 `/etc/oasis7/remote-provider-llm-routes.json`

适用前提:

- `oasis7_newapi_bridge_service` 的 state 文件能被 remote provider bridge 读取
- state 内对应 binding 为 `active`
- 对应 `project_bindings` 已写入真实 `token_key`
- 对于全链路测试用户，`newapi_user_ref` 推荐使用链上公钥 fingerprint，例如 `pk_<public-key-fingerprint>`；client bearer 使用 `newapi_user_ref:pk_<public-key-fingerprint>`。测试邮箱只作为 QA / hosted-login persona 标签，不作为 provider bearer 主选择器。

推荐不要手改 JSON，直接用脚本给用户签发 bridge token:

```bash
python3 scripts/provider-remote-https/manage-user-route.py upsert-user \
  --auth-routes /etc/oasis7/remote-provider-auth-routes.json \
  --llm-routes /etc/oasis7/remote-provider-llm-routes.json \
  --user alice \
  --api-key <alice-letai-token-key> \
  --model gpt-5.4
```

脚本行为:

- 若用户没有 bridge token，脚本会生成随机 bearer token。
- 若用户已有 bridge token，默认沿用原 token，只更新上游 `api_key/model/base_url`。
- 需要轮换 bridge token 时，加 `--rotate-bridge-token`。
- 运行成功后会输出该用户应使用的 bridge bearer token 和一条 `curl` 样例。

## 本机验证

在 ECS 上准备好环境变量文件后，至少执行:

```bash
python3 /opt/oasis7/remote-provider-bridge/current/scripts/provider-remote-https/letai_provider_cli.py \
  agent --agent letai --message '{"decision":"wait"}' --timeout 15 --json

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
  --api-key <alice-letai-token-key> \
  --model gpt-5.4

curl -sS -H "Authorization: Bearer <alice-bridge-token>" \
  http://127.0.0.1:5841/v1/provider/info
```

## nginx 验证

真实环境门禁优先使用 live gate。该脚本会通过 SSH 读取 204/205 真实
`/etc/oasis7/newapi-bridge/bridge-state.json`，派生 active
`newapi_user_ref:<ref>` selector，然后分别打真实 ECS loopback provider 和 205
公网 nginx ingress。它不打印 raw `token_key`、密码或私钥。

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

若 LetAI 上游对默认 `oasis7-letai-provider-cli/1.0` User-Agent 做了额外拦截，可在 env 里显式覆写:

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
- `provider_health` 默认通过 `OASIS7_REMOTE_LLM_HEALTH_URL` 做 upstream 探针；若 LetAI/兼容服务不支持该路径，需要显式改成可用的健康检查地址。
- 在未提供真实 LetAI `api_key/model` 前，ECS 端只能部署模板和二进制，不能完成最终 smoke。
- 自动映射示例使用显式 `newapi_user_ref:<user_ref>` / `bridge_user_id:<id>`。当前
  `auth_support.rs::parse_newapi_bridge_bearer_selector` 仍兼容不含冒号且长度至少
  `MIN_BRIDGE_AUTH_TOKEN_LEN`（24 字节）的裸值，并将其同时作为两类标识候选查找；
  显式前缀要求非空值，未知前缀不接受。解析成功不等于 binding 存在或已授权。
  这描述当前格式兼容行为，不推荐把裸值当作认证凭据；若要移除兼容，须另行修改实现与回归。

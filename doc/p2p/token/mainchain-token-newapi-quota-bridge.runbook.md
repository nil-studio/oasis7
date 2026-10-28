# oasis7 主链 Token 到 LetAI Run OpenAPI 额度桥接 Operator Runbook

- 对应需求文档: `doc/p2p/token/mainchain-token-newapi-quota-bridge.prd.md`
- 对应设计文档: `doc/p2p/token/mainchain-token-newapi-quota-bridge.design.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

审计轮次: 1

## Meta
- Owner Role: `runtime_engineer`
- Review Role: `qa_engineer`
- Scope: `bridge-service` 独立部署、配置注入、首次演练、日常 reconcile、manual review 与回滚边界
- Current Verdict: `operator_runbook_drafted`

## 1. 适用范围
- 本 runbook 只覆盖 `one-way OC -> LetAI Run OpenAPI quota` bridge 的 operator 侧执行。
- 本 runbook 不覆盖：
  - 浏览器下发 `token_key`
  - `OC <- quota/token_key` 兑回
  - 公开兑换所 / AMM / orderbook
  - richer operator dashboard
- 当前 bridge 仍属于 `limited preview operator-managed service-credit bridge`。

## 2. 开始前输入
每次部署或演练前，必须先固定：

- bridge-service 二进制来源
  - 当前入口: `crates/oasis7_newapi_bridge_service/src/bin/oasis7_newapi_bridge_service.rs`
- 状态文件路径
  - 建议单独目录，例如 `output/newapi-bridge/bridge-state.json`
- 链侧只读输入
  - `--chain-base-url`
  - `--chain-confirmations-required`
- LetAI 管理面输入
  - `--letai-base-url`
  - `--letai-platform-key`
  - `--letai-parent-channel-id`
- 定价输入
  - 至少一条 `--pricing-rule <pricing_version:oc_amount:credit_units:bonus_units>`
  - 当前示例真值只保留在 `scripts/newapi-bridge-service/pricing-rules.example.env`
  - `newapi-bridge-service.env.example` 只保存该文件路径，不重复保存定价值
- 运维输入
  - 监听地址 `--bind-addr`
  - 自动 reconcile 间隔 `--reconcile-interval-seconds`
  - route TTL `--route-ttl-seconds`
- target cognition/live gate 输入（仅在执行 target provider smoke/live gate 时需要）
  - `OASIS7_PROVIDER_LIVE_TARGET_CONTEXT_PAYLOAD_FILE=<private-path>/runtime-issued-context-pairs.json`
  - 文件必须是 Runtime-issued 的 `decision_context`/`feedback_context` 配对 wrapper artifact；缺失、不可读或自行伪造时，target lane 必须失败关闭。
- provider-local feedback continuity（可选）
  - `OASIS7_PROVIDER_FEEDBACK_STATE_PATH=<private-state-dir>/provider-feedback-state.json`
  - 配置后可跨 provider bridge 进程重启保留有界的 request/feedback lineage；这是 provider-local persistence，不是 Runtime journal/recovery certification，也不构成 release 或 default enablement 证明。
- 值班 owner
  - `runtime_engineer`
  - `qa_engineer`

## 3. 硬阻断条件
- 缺任一 LetAI 凭证输入：不得启动自动 bridge。
- `--chain-base-url` 未配置：不得执行 reconcile。
- 未冻结任何 `--pricing-rule`：不得对外发放 deposit route。
- 状态文件路径位于公共静态目录、HTML 产物目录或浏览器可下载路径：直接阻断。
- bridge-service 与 public web/player plane 混部且没有最小访问隔离：直接阻断。
- operator 无法证明 `token_key` 不会进入公共日志、公共 API 或浏览器 bootstrap：直接阻断。

## 4. 推荐启动命令
示例仅作为 operator 参考，实际值必须来自受控环境：

```bash
pricing_rules="$(
  sed -n 's/^OASIS7_NEWAPI_BRIDGE_PRICING_RULES=\"\(.*\)\"$/\1/p' \
    scripts/newapi-bridge-service/pricing-rules.example.env
)"

env -u RUSTC_WRAPPER cargo run -p oasis7_newapi_bridge_service --bin oasis7_newapi_bridge_service -- \
  --bind-addr 127.0.0.1:5852 \
  --state-path output/newapi-bridge/bridge-state.json \
  --route-ttl-seconds 900 \
  --deposit-account-prefix oc:bridge: \
  --chain-base-url http://127.0.0.1:5010 \
  --chain-confirmations-required 1 \
  --letai-base-url https://api.letai.run \
  --letai-platform-key "$LETAI_PLATFORM_KEY" \
  --letai-parent-channel-id "$LETAI_PARENT_CHANNEL_ID" \
  $(printf '%s' "$pricing_rules" | sed 's/,[[:space:]]*/\n/g' | sed '/^$/d; s/^/--pricing-rule /') \
  --reconcile-interval-seconds 15
```

约束：
- `LETAI_PLATFORM_KEY` 只能来自受控服务端环境变量或等价 secret store。
- 示例 pricing rule 真值只在 `scripts/newapi-bridge-service/pricing-rules.example.env` 维护；`newapi-bridge-service.env.example` 只负责指向该文件；runbook 通过读取该文件展示启动命令。
- 不要把真实 `platform key` / `parent channel id` 写进 repo、脚本默认值或 public CI log。
- 若只做手工演练，可把 `--reconcile-interval-seconds` 设为 `0`，改用手工 `POST /v1/bridge/reconcile`。

## 5. 首次部署检查
1. 先跑定向测试：
   - `env -u RUSTC_WRAPPER cargo test -p oasis7_newapi_bridge_service --bin oasis7_newapi_bridge_service -- --nocapture`
2. 再确认文档/行数门禁：
   - `./scripts/check-rust-file-size.sh`
   - `./scripts/doc-governance-check.sh`
3. 启动 bridge-service 后，先检查健康接口：
   - `GET /health`
   - `GET /v1/bridge/health`
4. 确认 health 输出中：
   - `ok=true`
   - `binding_count/project_binding_count/route_count/ledger_count` 初值符合预期
5. 确认状态文件已落到 operator 指定路径，而不是默认 public 目录。

## 6. 首次演练流程

### 6.1 准备全链路测试用户
- 早期演练可以先使用假邮箱，因为邮箱登录服务也有测试环境；假邮箱只作为测试 persona 标签，不要求真实投递。
- 推荐使用保留域名，避免误发给真实用户：
  - `oasis7-e2e-001@test.invalid`
  - `oasis7-e2e-lowquota-001@test.invalid`
  - `oasis7-e2e-manualreview-001@test.invalid`
- 每个测试邮箱必须对应一套受控生成的链上 keypair：
  - 私钥只放在 operator/QA 本地 secret 文件或等价 secret store，不进 repo、CI log、bridge state、provider request。
  - 公钥或公钥 fingerprint 用作 `newapi_user_ref`，例如 `pk_<fingerprint>`。
  - `oasis_sender_account_id` 使用同一公钥/account id，作为链侧转账来源。
- 不要从邮箱确定性派生私钥；邮箱只是 human/audit root，链上私钥必须由安全随机源生成。
- 本地测试密钥材料模板记录在 `~/Documents/keys/test_keys.txt`；公开文档只记录字段形状和假邮箱，不记录真实私钥。

建议字段形状：

```text
email=oasis7-e2e-001@test.invalid
external_user_name=Oasis7 E2E User 001
project_name=oasis7-e2e-001
newapi_user_ref=pk_<public-key-fingerprint>
oasis_sender_account_id=oc:pk:<public-key-or-account-id>
agent_provider_auth_token=newapi_user_ref:pk_<public-key-fingerprint>
```

### 6.2 建立用户绑定
- 调用 `POST /v1/bridge/bind`
- 输入：
  - `newapi_user_ref`
  - `oasis_sender_account_id`
  - 可选 `external_user_name/email/project_name/project_metadata`
- 期望：
  - 返回 `bridge_user_id`
  - 不返回 `platform_user_id/platform_project_id/token_key`

示例：

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

### 6.3 分配充值路由
- 调用 `POST /v1/bridge/deposit-route`
- 输入：
  - `bridge_user_id`
  - `pricing_version` 或 `topup_plan_id`
- 期望：
  - 返回唯一 `deposit_account_id`
  - 返回短 `deposit_token`，用于链上 transfer `memo`
  - `route_status=issued`

### 6.4 链上入账
- 让绑定用户向 `deposit_account_id` 转入与 `pricing_version` 对应的 `OC`，并把 `deposit_token` 放入链上 transfer `memo`
- 期望：
  - chain watcher 观察到 route 对应入账
  - 不匹配金额、重复 route、过期 route、缺失或不一致的 `deposit_token` 默认进入 `manual_review`

### 6.5 reconcile
- 手工触发：
  - `POST /v1/bridge/reconcile`
- 或等待后台 interval
- 期望顺序：
  - `confirmed -> provisioning_user -> provisioning_project -> crediting -> credited -> reconciled`
- 审计真值至少包括：
  - `platform_user_id`
  - `platform_project_id`
  - `token_key`
  - `external_order_id`
  - `user_snapshot/project_snapshot/topup_log_snapshot`

### 6.6 provider / gameplay 消费验证

先核实 [Remote Provider Bridge 认证与授权前提](../../world-runtime/runtime/provider-remote-https-bridge-operator-runbook.md#认证与授权前提)：
`newapi_user_ref` / 公钥 fingerprint 是路由 selector，不是调用者持有私钥或拥有
对应额度的证明。自动映射必须置于独立主体认证与 selector 授权绑定之后，或严格
隔离于获准操作这些用户额度的可信调用方。TLS、限流和全局 bridge auth token
不补足自动映射分支的该项前提；只验证映射/usage 成功不等于验证调用者授权。
此处只维护操作前提，不确认现网配置、实际风险发生或安全问题已经关闭。

- 使用同一个 `newapi_user_ref` 作为 provider bearer selector：
  - `newapi_user_ref:pk_<public-key-fingerprint>`
- remote provider bridge 必须从受控 `bridge-state.json` 自动解析到对应 project `token_key`。
- 游戏 runtime / launcher 只接收 bearer selector，不接收 raw `token_key`。
- 完成一次 `play/step`、`agent_chat` 或 provider decision smoke 后，验证 trace 中出现同一用户路径下的 token usage / provider decision 证据。

#### target feedback 的当前桥接边界

`/v1/world-simulator/feedback-context` 是 target lane 的 feedback route，必须与
`/v1/world-simulator/decision-context` 的同一对 Runtime-issued context 配对。当前
bridge 只验证已接受 decision response 的 `agent_subject`、session/turn/request id
和 `request_digest`，只接受 bounded diagnostic feedback：

- `pending` 只能不带 reason，或使用 `recovery_pending`、`retry_scheduled`、
  `scheduler_backpressure`。
- `rejected` 必须使用已登记的拒绝 reason；`failed` 必须使用
  `failed_provider`、`failed_persist`、`cognition_failed` 或
  `provider_unavailable`。
- 这些记录仅表示 provider-local diagnostic，不能创建 Runtime authority、committed
  action、receipt、world projection，也不能据此声称本地/remote 异步恢复或发布就绪。

当前 bridge 没有 Runtime receipt/projection verifier，因此以下输入必须显式失败关闭：

- `status=committed`，或任意 status 携带 `candidate_action_id` / `runtime_receipt_id`：
  HTTP `409`，`error_code=feedback_disposition_unverifiable`。
- 携带当前 `FeedbackEnvelopeV1` 未定义的 `projection` 或其他 outer field：HTTP
  `400`，`error_code=unknown_context_field`；不得把它当成已验证投影。
- 未见过对应 decision、session/turn 不一致或 digest 不一致：HTTP `409`，分别为
  `unknown_feedback`、`feedback_correlation_mismatch` 或 `feedback_digest_mismatch`。
- reason 缺失、超界或不在当前 status allowlist：HTTP `409`，
  `error_code=feedback_disposition_reason_invalid`。

只有同一 request lineage 的 bounded diagnostic 返回 `{"ok":true}` 才能记为
feedback accepted；该结果仍不是 Runtime committed receipt。上述错误码应原样写入
测试报告，不能改写成“反馈已提交”或“动作已执行”。

### 6.7 全链路测试步骤矩阵

测试前从 `~/Documents/keys/test_keys.txt` 选择一组 persona，把对应字段临时导出到 operator shell。不要把该文件内容贴进 issue、PR、聊天、CI log 或 public terminal recording。

```bash
export BRIDGE_BASE_URL=http://127.0.0.1:5852
export PROVIDER_BASE_URL=http://127.0.0.1:5841
export CHAIN_BASE_URL=http://127.0.0.1:5010
export PRICING_VERSION=pv-1

export TEST_EMAIL=oasis7-e2e-001@test.invalid
export TEST_EXTERNAL_USER_NAME="Oasis7 E2E User 001"
export TEST_PROJECT_NAME=oasis7-e2e-001
export TEST_NEWAPI_USER_REF=pk_<public-key-fingerprint>
export TEST_OASIS_SENDER_ACCOUNT_ID=oc:pk:<public-key-hex>
export TEST_CHAIN_PUBLIC_KEY_HEX=<public-key-hex>
export TEST_PROVIDER_AUTH_TOKEN=newapi_user_ref:${TEST_NEWAPI_USER_REF}
```

不要把 `chain_private_key_hex` 导出到 shell 环境变量。签名工具必须从本地 secret 文件或等价 secret store 读取私钥，并且不得把私钥打印到 terminal、CI log 或证据文件。

| Persona | 目标 | 入账金额 | 期望 |
| --- | --- | --- | --- |
| `happy_path` | 正常发额度并完成一次 provider smoke | 等于 `PRICING_VERSION` 对应 OC | `reconciled_credit_count=1`，provider decision 成功 |
| `lowquota_exhaustion` | 小额度消耗到不足 | 等于最小测试档，随后连续 provider smoke | 最后一次调用出现额度不足或上游 quota 失败，且不串到其他用户 |
| `manual_review_underpay` | 少付 | 小于 `PRICING_VERSION` 对应 OC | `manual_review_count` 增加，不能自动 credited |
| `manual_review_overpay` | 多付 | 大于 `PRICING_VERSION` 对应 OC | `manual_review_count` 增加，不能静默错充 |
| `expired_route` | route 过期后入账 | 等于或接近 `PRICING_VERSION` 对应 OC，但在 TTL 后转入 | 进入 expired/manual review 路径，不发正常额度 |

本轮 `happy_path` 必须先满足以下 execution gates；任一不满足时，只允许做环境预检、`bind` 和 `deposit-route` rehearsal，不得声称全链路通过：

- 可通过受信 SSH / tunnel / 本机启动访问 `BRIDGE_BASE_URL`、`PROVIDER_BASE_URL`、`CHAIN_BASE_URL`。
- bridge-service 已配置 `--chain-base-url`、`--letai-base-url`、`--letai-platform-key`，且测试 lane 的 `--pricing-rule` 包含当前 `PRICING_VERSION`。
- remote provider bridge 已启用 `OASIS7_PROVIDER_AUTH_ROUTE_FROM_BEARER=true`，且可读取同一份 NewAPI bridge state。
- 已确认 `oasis7_chain_transfer_submit_client` 可从本地 secret 文件读取当前 persona 的 `chain_private_key_hex`、`chain_public_key_hex`、`oasis_sender_account_id`，并在传入 `--memo` 时产出 `octransferauth:v2:<signature-hex>`。
- 测试 sender 账户有足够 OC，或测试网 faucet/operator 已准备好等价入账路径。
- 证据采集能按本次 `route_id` / `bridge_deposit_id` / chain action id 定位 ledger row；不得只看全局计数。

扩展负向覆盖：

- `duplicate_route_deposit`、`binding_not_found`、`project_binding_not_found`、LetAI topup/query mismatch、LetAI timeout/decode failure 属于 phase-2 negative coverage；不是 `happy_path` 首轮阻断，但正式放行前应单独补证据。
- 当前 bridge reconcile 记录 `from_account_id`，但不得把 `from_account_id == oasis_sender_account_id` 当作已强制执行的安全不变量；若需要强约束，另开 runtime/security 修复任务。测试证据仍应记录实际 `from_account_id`，并增加“不同 sender 向同一 deposit account 入账”的负向检查。

#### Step A: 环境健康检查

```bash
curl -sS "${BRIDGE_BASE_URL}/v1/bridge/health"
curl -sS "${PROVIDER_BASE_URL}/v1/provider/info" \
  -H "Authorization: Bearer ${TEST_PROVIDER_AUTH_TOKEN}"
```

期望：
- bridge health 返回 `ok=true`。
- 在 bind/reconcile 之前，provider 自动映射还没有 active binding / project `token_key`，用 `newapi_user_ref:<ref>` 访问 `GET /v1/provider/info` 预期可以是 `401 Unauthorized`。
- bind 且成功写入 project `token_key` 后，provider info / decision 才应返回成功；任何阶段都不能要求 client 传 raw `token_key`。

#### Step B: 建立 bind

```bash
BIND_RESPONSE="$(
  curl -sS -X POST "${BRIDGE_BASE_URL}/v1/bridge/bind" \
    -H 'Content-Type: application/json' \
    -d "{
      \"newapi_user_ref\": \"${TEST_NEWAPI_USER_REF}\",
      \"oasis_sender_account_id\": \"${TEST_OASIS_SENDER_ACCOUNT_ID}\",
      \"external_user_name\": \"${TEST_EXTERNAL_USER_NAME}\",
      \"email\": \"${TEST_EMAIL}\",
      \"project_name\": \"${TEST_PROJECT_NAME}\",
      \"metadata\": {
        \"purpose\": \"bridge-full-chain-e2e\",
        \"persona\": \"${TEST_PROJECT_NAME}\",
        \"email_login_env\": \"test\",
        \"identity_root\": \"fake-email-plus-chain-public-key\"
      },
      \"project_metadata\": {
        \"purpose\": \"bridge-full-chain-e2e\",
        \"newapi_user_ref\": \"${TEST_NEWAPI_USER_REF}\"
      }
    }"
)"
printf '%s\n' "${BIND_RESPONSE}"
export TEST_BRIDGE_USER_ID="$(printf '%s' "${BIND_RESPONSE}" | jq -r '.bridge_user_id')"
```

期望：
- `ok=true`。
- `bridge_user_id` 非空。
- response 不包含 raw `token_key`。
- `bridge-state.json` 中可以由 operator 查到该 `newapi_user_ref` 对应 active binding 与 project binding。

#### Step C: 创建 deposit route

```bash
ROUTE_RESPONSE="$(
  curl -sS -X POST "${BRIDGE_BASE_URL}/v1/bridge/deposit-route" \
    -H 'Content-Type: application/json' \
    -d "{
      \"bridge_user_id\": \"${TEST_BRIDGE_USER_ID}\",
      \"pricing_version\": \"${PRICING_VERSION}\",
      \"topup_plan_id\": null
    }"
)"
printf '%s\n' "${ROUTE_RESPONSE}"
export TEST_DEPOSIT_ROUTE_ID="$(printf '%s' "${ROUTE_RESPONSE}" | jq -r '.route_id')"
export TEST_DEPOSIT_ACCOUNT_ID="$(printf '%s' "${ROUTE_RESPONSE}" | jq -r '.deposit_account_id')"
export TEST_DEPOSIT_TOKEN="$(printf '%s' "${ROUTE_RESPONSE}" | jq -r '.deposit_token')"
export TEST_ROUTE_EXPIRES_AT_UNIX_MS="$(printf '%s' "${ROUTE_RESPONSE}" | jq -r '.expires_at_unix_ms')"
```

期望：
- `ok=true`。
- `route_status=issued`。
- `deposit_account_id` 非空且只用于本 route。
- `deposit_token` 非空，形如 `bridge:deposit:<route_id>`；不得包含 raw `token_key`、私钥或完整业务载荷。

#### Step D: 链上签名转账

公开 transfer submit API 的请求体必须包含签名字段，不要用未签名 `curl` 伪造链上入账。默认使用 repo-owned signed transfer client：

```bash
env -u RUSTC_WRAPPER cargo run -p oasis7_protocol_tools --bin oasis7_chain_transfer_submit_client -- \
  submit \
  --keys-file ~/Documents/keys/test_keys.txt \
  --persona happy_path \
  --chain-base-url "${CHAIN_BASE_URL}" \
  --to-account-id "${TEST_DEPOSIT_ACCOUNT_ID}" \
  --amount 100 \
  --nonce 1 \
  --memo "${TEST_DEPOSIT_TOKEN}"
```

约束：

- `--persona` 必须选择本轮正在跑的 test persona。
- `--amount` 必须替换成当前 `PRICING_VERSION` 对应的 OC 金额；少付/多付 case 用同一命令改金额制造。
- `--nonce` 必须使用该 sender 在测试 lane 中未用过的 nonce。
- `--memo` 必须使用 Step C 返回的 `TEST_DEPOSIT_TOKEN`；缺失或不一致应进入 bridge `manual_review`，不得正常 credit。
- client 从 `--keys-file` 的当前 persona 段读取 `chain_private_key_hex`、`chain_public_key_hex`、`oasis_sender_account_id`，不会把私钥打印到 stdout。
- 若本 lane 由 faucet/operator 工具托管测试入账，也可以用等价工具把同一 `TEST_OASIS_SENDER_ACCOUNT_ID -> TEST_DEPOSIT_ACCOUNT_ID` 的金额提交到链上，并保留 tx/action id。

transfer submit 字段形状：

```json
{
  "from_account_id": "oc:pk:<public-key-hex>",
  "to_account_id": "<TEST_DEPOSIT_ACCOUNT_ID>",
  "amount": 100,
  "nonce": 1,
  "memo": "<TEST_DEPOSIT_TOKEN>",
  "tx_version": 2,
  "tx_type": "asset_transfer",
  "public_key": "<public-key-hex>",
  "signature": "octransferauth:v2:<signature-hex>"
}
```

期望：
- submit response 返回 `ok=true` 和 `action_id`。
- `GET /v1/chain/transfer/status?action_id=<action_id>` 或链侧 explorer 证据最终显示 confirmed。
- explorer 证据中的 `memo` 必须等于 `TEST_DEPOSIT_TOKEN`。
- 对少付/多付/过期 route，用对应 persona 的金额或等待策略制造失败条件，并保留同样的 tx/action id。

#### Step E: reconcile

```bash
RECONCILE_RESPONSE="$(
  curl -sS -X POST "${BRIDGE_BASE_URL}/v1/bridge/reconcile"
)"
printf '%s\n' "${RECONCILE_RESPONSE}"
```

happy path 期望：
- 至少一次 reconcile 后，本次 `route_id` / `bridge_deposit_id` 对应 ledger row 推进到 `reconciled`。
- `reconciled_credit_count` 可作为本次处理摘要，但不能单独作为通过标准；若 state 内已有旧记录，全局计数可能误导。
- state 内同一 binding/project binding 有 `platform_user_id`、`platform_project_id`、`token_key`、`external_order_id` 和 LetAI topup/query 快照。

失败 path 期望：
- underpay / overpay / expired route 不得 credited。
- 本次 `route_id` / `bridge_deposit_id` 对应 ledger row 进入 `manual_review` 并记录 review reason；`manual_review_count` 只作辅助摘要。
- 后续 operator review 只能显式 `mark_resolved` 或 `close`，不能口头判成功。

#### Step F: provider compatibility audit、target smoke 与消耗证据

以下第一条请求使用旧 `DecisionRequest` DTO，只能作为显式的 legacy compatibility-only
审计，不计入 target readiness 或完整链路通过。它不能证明 Continuous Harness target
cognition、Runtime receipt 或 production async 闭环。target lane 必须改用
`/v1/world-simulator/decision-context` 的
`ContinuousAgentRequestContextV1`，并以 `/v1/world-simulator/feedback-context` 的
`FeedbackEnvelopeV1` 完成配对验证；当前 HTTP body 不定义 `compatibility_lane` 字段，路由
本身是 lane 选择依据。

```bash
# Explicit legacy compatibility audit only; this route is not target cognition proof.
curl -sS -X POST "${PROVIDER_BASE_URL}/v1/world-simulator/decision" \
  -H "Authorization: Bearer ${TEST_PROVIDER_AUTH_TOKEN}" \
  -H 'Content-Type: application/json' \
  -d '{
    "observation": {
      "agent_id": "agent-1",
      "world_time": 1,
      "mode": "headless_agent",
      "observation_schema_version": "oc_dual_obs_v1",
      "action_schema_version": "oc_dual_act_v1",
      "environment_class": "bridge-full-chain-e2e",
      "observation": {
        "self_state": {
          "location_ref": "loc-1",
          "pose_hint": "grid_pose=(0, 0, 0)",
          "status_flags": [],
          "resource_summary": {}
        },
        "mission_context": {
          "goal_summary": "return a minimal wait decision for bridge quota smoke"
        },
        "nearby_entities": [],
        "recent_events": [],
        "local_navigation_graph": [],
        "hazard_summary": [],
        "interaction_targets": []
      },
      "recent_event_summary": [],
      "memory_summary": "bridge quota smoke",
      "action_catalog": [
        {
          "action_ref": "wait",
          "summary": "do nothing this tick"
        }
      ],
      "timeout_budget_ms": 7000
    },
    "provider_config_ref": "provider://local-bridge",
    "agent_profile": "oasis7_p0_low_freq_npc",
    "fixture_id": "bridge-full-chain-e2e",
    "timeout_budget_ms": 7000
}'
```

可重复执行的自动化 smoke:

```bash
# Required for the target lane; this file must be a Runtime-issued pair artifact.
# Do not fabricate it. The same variable is consumed by the live-gate wrapper.
export OASIS7_PROVIDER_LIVE_TARGET_CONTEXT_PAYLOAD_FILE="/secure/path/runtime-issued-context-pairs.json"
TARGET_CONTEXT_PAYLOAD_FILE="${OASIS7_PROVIDER_LIVE_TARGET_CONTEXT_PAYLOAD_FILE:?set a Runtime-issued target context pair artifact}"
./scripts/provider-remote-https/provider-bridge-contract-smoke.sh \
  --base-url "${PROVIDER_BASE_URL}" \
  --auth-token "${TEST_PROVIDER_AUTH_TOKEN}" \
  --target-context-payload-file "${TARGET_CONTEXT_PAYLOAD_FILE}" \
  --decision-count 1 \
  --min-successes 1
```

正式环境/测试环境验收不得只看 mock 或 legacy compatibility smoke。需要发现真实 ECS、nginx ingress、LetAI
额度和 state 映射问题时，必须跑 live gate。live gate 会从真实 204/205 ECS
读取 active `newapi_user_ref:<ref>` selector，不打印 raw `token_key`；target lane 必须使用上面
的 `OASIS7_PROVIDER_LIVE_TARGET_CONTEXT_PAYLOAD_FILE`，不能用 `--legacy-compatibility-only`
替代：

```bash
./scripts/provider-remote-https/provider-bridge-live-gate.sh \
  --target-context-payload-file "${TARGET_CONTEXT_PAYLOAD_FILE}" \
  --decision-count 1
./scripts/provider-remote-https/provider-bridge-live-gate.sh \
  --target-context-payload-file "${TARGET_CONTEXT_PAYLOAD_FILE}" \
  --decision-count 1 --loopback-target 204 --skip-public
./scripts/provider-remote-https/provider-bridge-live-gate.sh \
  --target-context-payload-file "${TARGET_CONTEXT_PAYLOAD_FILE}" \
  --decision-count 1 --loopback-target 205 --skip-public
./scripts/provider-remote-https/provider-bridge-live-gate.sh \
  --target-context-payload-file "${TARGET_CONTEXT_PAYLOAD_FILE}" \
  --decision-count 1 --skip-loopback
```

CI 默认只运行无副作用 harness 回归；真实环境门禁需显式启用：

```bash
OASIS7_CI_RUN_PROVIDER_LIVE_GATE=true ./scripts/ci-tests.sh --required
```

`lowquota_exhaustion` persona 需要连续 decision 直到出现额度不足或上游 quota
失败。自动化判定示例：

```bash
./scripts/provider-remote-https/provider-bridge-contract-smoke.sh \
  --base-url "${PROVIDER_BASE_URL}" \
  --auth-token "${TEST_PROVIDER_AUTH_TOKEN}" \
  --target-context-payload-file "${TARGET_CONTEXT_PAYLOAD_FILE}" \
  --decision-count 20 \
  --min-successes 1 \
  --expect-provider-error-code-substr quota
```

真实环境低额度验收使用 dedicated lowquota persona 后执行：

```bash
./scripts/provider-remote-https/provider-bridge-live-gate.sh \
  --target-context-payload-file "${TARGET_CONTEXT_PAYLOAD_FILE}" \
  --skip-public \
  --loopback-target 204 \
  --lowquota-target 204 \
  --lowquota-decision-count 20

./scripts/provider-remote-https/provider-bridge-live-gate.sh \
  --target-context-payload-file "${TARGET_CONTEXT_PAYLOAD_FILE}" \
  --skip-loopback \
  --lowquota-target public205 \
  --lowquota-decision-count 20
```

通过标准：至少一次 decision 成功，并且后续 response 中出现 `provider_error.code`
包含 `quota` 的记录；不得把 raw `token_key`、私钥或完整 bridge state 写入证据。

期望：
- happy path 返回 `provider_error=null` 或等价成功字段。
- provider trace / bridge log 能证明该调用通过 `newapi_user_ref:<ref>` 映射到对应 project `token_key`。
- LetAI / NewAPI 侧能看到该 project 的 token usage 或余额消耗。
- lowquota persona 连续调用直到额度耗尽时，失败必须归属于同一 `newapi_user_ref`，不得 fallback 到全局 token。

#### Step G: 证据回写

每个 persona 跑完后，必须记录到 GitHub task issue evidence comments，并为正式 QA closeout 增补一个 sanitized evidence 文件，例如 `doc/testing/evidence/oc-letai-bridge-full-chain-e2e-<date>.md`：

- persona slug，不含私钥。
- `newapi_user_ref`、`bridge_user_id`、`route_id`、`deposit_account_id`、`deposit_token`。
- chain `action_id` / tx id / explorer link，以及实际 `from_account_id`。
- reconcile response 摘要和本次 route/ledger row 状态。
- provider smoke response 摘要。
- `lane`：`target_cognition` 或 `legacy_compatibility_only`。
- `decision_route`：实际使用的决策路由；target 必须为
  `/v1/world-simulator/decision-context`，legacy 才能为 `/v1/world-simulator/decision`。
- `feedback_route`：实际使用的反馈路由；target 必须为
  `/v1/world-simulator/feedback-context`，legacy compatibility audit 记为 `null` / `not_run`。
- target feedback 的 HTTP status、`ok`、`feedback_id`/request correlation 摘要及
  `error_code`（若失败）；禁止把 `feedback_disposition_unverifiable`、
  `unknown_context_field` 或 correlation error 记为 Runtime receipt。
- LetAI/NewAPI usage 摘要。
- 对失败 path，记录 `manual_review` reason 和 operator review resolution。

## 7. 日常巡检
- health 接口关注：
  - `manual_review_count`
  - `failed_credit_count`
  - `pending_confirmation_count`
  - `reconciled_count`
- 状态文件巡检关注：
  - 是否存在长期停留在 `Failed` 的记录
  - 是否存在 `ManualReview` 且无 `operator_note`
  - 同一 `bridge_deposit_id` 是否出现多条业务 order
  - `token_key` 是否只出现在受控状态文件，不出现在 public logs / API 回包

## 8. 异常收口

### 8.1 常见异常
- `underpay` / `overpay`
- `expired_route_deposit`
- `duplicate_route_deposit`
- `binding_not_found`
- `project_binding_not_found`
- `letai_topup_log_mismatch`
- `letai_project_summary_mismatch`
- LetAI 5xx / timeout / decode failed

### 8.2 处理原则
- 单条异常不得阻断整轮 reconcile。
- 缺 binding / project binding 时，当前实现会把该 ledger row 直接落到 `manual_review`。
- LetAI topup/query 异常优先保留稳定 `external_order_id`；不得重新造第二个业务 order。
- `token_key` 缺失或 query verification mismatch，不得人工口头判成成功。

### 8.3 operator review
- 当前最小接口：
  - `POST /v1/bridge/operator/review/{bridge_deposit_id}`
- 当前 resolution 只支持：
  - `mark_resolved`
  - `close`
- 若需要“调额度后重发”“换 project 后重试”等 richer action，必须另开任务；本轮 runbook 不声称支持。

## 9. 回滚边界
- 当前 bridge 是独立服务，回滚优先级：
  1. 停止新的 route 发放
  2. 停止自动 reconcile
  3. 保留状态文件与链上证据
  4. 回退 bridge-service 二进制到上一个已验证版本
- 禁止做法：
  - 手工删除 `bridge-state.json` 再“重建”
  - 清掉 `external_order_id` 强行重发
  - 把失败记录从 ledger 中直接抹掉
- 可接受做法：
  - 保留 ledger 原始记录
  - 通过 operator review 显式关闭不再处理的异常单
  - 在新的二进制版本上继续读取同一状态文件恢复处理

## 10. 证据回写
每次正式部署、首次演练或异常收口后，至少回写：
- GitHub task issue evidence comments
- GitHub Issue / GitHub Project task evidence
- 必要时附：
  - health 快照
  - 定向测试命令与结果
  - `bridge_deposit_id -> external_order_id -> review_reason/resolution` 证据

provider target evidence 还必须保留 `lane`、`decision_route`、`feedback_route` 三个
字段。contract smoke 当前输出中的 `decision_path` / `feedback_path` 应在归档报告中
规范化为上述 route 字段；legacy lane 的 feedback route 不得留空后被解释成 target
feedback 已通过。

## 11. 当前缺口
- richer operator runbook automation 还未脚本化。
- dashboard / replay / re-credit UI 仍未实现。
- 当前 runbook 仍默认 operator 手工持有 deployment 输入，不含 secret rotation / KMS / HSM 闭环。
- `OASIS7_PROVIDER_FEEDBACK_STATE_PATH` 持久化仍是可选的 provider-local continuity；
  即使配置，也不构成 Runtime journal/recovery、remote recovery、release 或 default
  enablement 证明。managed/packaged/remote/systemd wiring 延后到另行授权的运维切片，
  本 runbook 不新增其拓扑或部署要求。

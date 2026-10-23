# 本地启动 public_testnet 测试环境 + LetAI Runbook

- Owner Role: `qa_engineer`
- Supporting Roles: `blockchain_ops_engineer`, `runtime_engineer`, `agent_engineer`
- Scope: 本机启动一个接入 formal `public_testnet` rehearsal world state 的 test 环境入口，并启动本地 viewer/API、NewAPI quota bridge 与 LetAI provider bridge，供环境 readiness、OC -> NewAPI/LetAI 充值链路、agent/LLM 决策路径测试使用。
- Status: operator manual
- Last Verified Example: 2026-06-23, local node `oasis7.testnet.fourth`

## 1. 口径边界

可以说：
- 本地启动了 test 环境入口，并接入 test 环境的大世界，前提是本 runbook 的 manifest、world/chain/network、同步、viewer/API、provider 证据都通过。
- 本地启动了完整充值链路测试环境，前提是 `oasis7_newapi_bridge_service` 已启动、已分配 deposit route、已提交 signed testnet OC transfer、reconcile 已把 confirmed tx 兑换成 NewAPI/LetAI quota，并且 provider 使用同一个 `newapi_user_ref` 消费成功。
- 余额不足、预扣费失败和充值传播延迟可以作为充值/额度功能测试现象记录，但不能替代 OC -> NewAPI quota bridge 的 bind/deposit/reconcile 证据。

不可以说：
- 这是 `ready_for_live_candidate` 的 public testnet 结论。
- 这是 mainnet、生产 OC 结算、公开 faucet 或公开 validator onboarding。
- 只因页面能打开，就说已接入 test 环境大世界。
- 把纯本地测试或 local-only playtest 说成“本地启动 test 环境”。
- 把 `127.0.0.1:5841` 的 LetAI provider bridge auto-topup 当成 testnet OC 充值链路；真实 OC -> NewAPI 充值必须经过 `oasis7_newapi_bridge_service`。
- 把发到本机 observer 的 pending gameplay action 当成“已广播链上交易”。玩家认领/玩法 action 必须广播到 submit-capable public_testnet endpoint，并随后从本机 observer 的 committed snapshot 同步回来，才算类似主流 DApp 的完整提交路径。
- 把手工复制 checkpoint、手工拷 validator `data/`、或从一台 validator 覆盖另一台 validator 的状态当成“testnet 已同步”。testnet 节点恢复只能来自自动 replication/head exchange 追平，或按 governed bootstrap runbook 从当前 deployment truth 从零重建。

## 2. 与其他 manual 的分工

本 runbook 是“本地启动 test 环境并接入 public_testnet world state”的搭建与 readiness 证明入口，不是纯本地测试入口。

- `doc/testing/manual/web-ui-agent-browser-closure-manual.manual.md` 是通用 Viewer 页面采样/截图/状态留证入口。环境按本 runbook 启动后，可以复用该手册做页面证据采样。
- `doc/testing/manual/web-ui-playwright-closure-manual.manual.md` 是玩家可见 UI 操作的 Playwright 实跑入口。环境按本 runbook 启动后，Playwright 用例应通过 `--url` 复用本 runbook 的 URL。
- `./scripts/run-local-letai-game-test.sh` 是纯本地真实 LetAI playtest 栈入口。它会统一启动 bridge + launcher/runtime/viewer，但默认是 local-only playtest，不等价于本 runbook 的 public_testnet attach-existing-node 证明路径。
- `./scripts/run-viewer-web.sh` 或单独静态 server 只负责 Web 静态入口，不负责 provider bridge、launcher bootstrap 或 public_testnet 证明。

术语约定：
- `纯本地测试` / `local-only playtest`: 本机启动 runtime、viewer、provider，验证本地玩法/LLM/UI 链路；不要求 world state 来自 formal public_testnet。
- `本地启动 test 环境`: 本机入口、viewer/API、NewAPI quota bridge 和 provider bridge 在本地跑，但 world state 必须来自 formal public_testnet manifest 对应的已同步 testnet 节点。若本轮目标包含充值，必须走 `oasis7_newapi_bridge_service`。

## 3. 当前本地拓扑

| 组件 | 默认地址 | 作用 |
| --- | --- | --- |
| local public_testnet node | `127.0.0.1:19083` | 本机已同步 testnet 节点 status/API |
| public_testnet submit endpoint | operator-provided | 玩家认领/玩法 action 的链上广播入口；不能用 observer-only endpoint 代替 |
| NewAPI quota bridge | `127.0.0.1:5852` | OC -> NewAPI/LetAI quota bind、deposit route、reconcile |
| LetAI provider bridge | `127.0.0.1:5841` | 本地 LLM provider endpoint |
| viewer live WebSocket | `127.0.0.1:5011` | viewer 连接的 live bridge |
| viewer live API | `127.0.0.1:5023` | viewer live side API |
| static viewer | `127.0.0.1:4173` | 本地 `viewer.html` 入口；`software_safe.html` 仅为兼容 alias |

本地入口 URL：

```text
http://127.0.0.1:4173/viewer.html?ws=ws://127.0.0.1:5011&test_api=1&locale=zh
```

## 4. 配套启动脚本

优先使用配套脚本完成环境编排：

```bash
rtk ./scripts/run-local-public-testnet-letai-test-environment.sh \
  --manifest "$OASIS7_TESTNET_MANIFEST" \
  --letai-config "$LETAI_TOKEN_FILE" \
  --letai-platform-env "$LETAI_PLATFORM_ENV"
```

脚本会：
- 验证 local public_testnet node identity/readiness。
- 启动 `oasis7_newapi_bridge_service` 到 `127.0.0.1:5852`。
- 启动 LetAI provider bridge 到 `127.0.0.1:5841`，并设置同一份 `OASIS7_REMOTE_LLM_NEWAPI_BRIDGE_STATE_PATH`。
- 启动 `oasis7_viewer_live` 与 static viewer；`--chain-status-bind` 读本机 observer，`--chain-submit-bind` 写 submit-capable endpoint。
- 打印后续 bind/deposit/signed transfer/reconcile/provider smoke 的 operator command 模板；其中自动打印的 provider smoke 仍是 legacy compatibility audit，按本手册第 10 节的 target command 才能做 target readiness 验证。

脚本不会自动提交 signed OC transfer。真实充值交易仍必须由 operator 显式选择 persona、nonce、amount 和 memo 后执行。

`--chain-submit-bind` 的来源顺序是：
- 显式 `OASIS7_PUBLIC_TESTNET_CHAIN_SUBMIT_BIND` / `--chain-submit-bind`
- 显式 `OASIS7_PUBLIC_TESTNET_CHAIN_SUBMIT_BASE_URL` / `--chain-submit-base-url`
- formal manifest 的 `endpoint_policy.rpc_ref`
- 本机 `--node-base-url` fallback

如果脚本退回本机 `--node-base-url` fallback，会打印 warning。该 fallback 只适合本机节点本身就是 submit-capable 的诊断；如果本机节点是 observer，认领可能只进入本地 pending queue，不能算已广播到 public_testnet 共识。

仅做环境/端口/节点检查，不启动服务：

```bash
rtk ./scripts/run-local-public-testnet-letai-test-environment.sh \
  --preflight-only \
  --manifest "$OASIS7_TESTNET_MANIFEST" \
  --letai-config "$LETAI_TOKEN_FILE"
```

## 5. 前置条件

1. 本机已有 public_testnet local node，例如 launchd label `oasis7.testnet.fourth`。
2. 节点 manifest 存在。用本地环境变量指向实际安装位置，例如：

```text
OASIS7_TESTNET_NODE_ROOT=<local-public-testnet-node-root>
OASIS7_TESTNET_MANIFEST=$OASIS7_TESTNET_NODE_ROOT/manifest.json
```

3. LetAI token 文件存在。用本地环境变量指向 secret 文件，例如：

```text
LETAI_TOKEN_FILE=<local-secret-dir>/letai-token-local.txt
```

4. 若 token 文件只包含 `Key:` / `base_url:`，充值/auto-topup 测试还需要平台字段来源，例如旧 local testnet env：

```text
LETAI_PLATFORM_ENV=<local-public-testnet-node-root>/playtest-stack-<timestamp>/letai-local-token.env
```

不要把 token、platform key、user id、project id 写入仓库文档、task log 或终端摘要。

5. 若本轮目标包含 OC -> NewAPI/LetAI 充值，必须额外准备：
   - `oasis7_newapi_bridge_service` 可执行文件或可本地构建源码。
   - LetAI platform key，供 NewAPI bridge 给用户/project/token 发额度。
   - 受控测试 persona 的 chain key 文件，例如 `OASIS7_TEST_KEYS_FILE=<local-secret-dir>/test_keys.txt`。不要把 `chain_private_key_hex` 导出到 shell、写进报告或贴到聊天里。
   - 明确的 pricing rule，例如 `scripts/newapi-bridge-service/pricing-rules.example.env` 中的 `pv-1:100:100000:0`。

6. 若本轮目标包含玩家认领/玩法 action 的链上提交，必须能解析出 submit-capable public_testnet endpoint。优先使用 formal manifest 的 `endpoint_policy.rpc_ref`；如果要覆盖，显式设置：

```text
OASIS7_PUBLIC_TESTNET_CHAIN_SUBMIT_BASE_URL=http://<public-testnet-submit-host>:<port>
# 或
OASIS7_PUBLIC_TESTNET_CHAIN_SUBMIT_BIND=<public-testnet-submit-host>:<port>
```

当前 `oasis7_viewer_live --chain-submit-bind` 是裸 HTTP socket bind，不是 HTTPS URL client。若只有 HTTPS public RPC，需要先提供本地 HTTP relay/proxy，或扩展 viewer live 的链提交客户端支持 full URL + TLS。

## 6. 验证 public_testnet 节点

先确认端口和健康：

```bash
rtk lsof -nP -iTCP:19083 -sTCP:LISTEN
rtk curl -sS http://127.0.0.1:19083/healthz
```

核对同一份 formal manifest 与 world identity：

```bash
rtk jq '{
  schema_version,
  tier,
  status,
  network_id,
  chain_id,
  runtime_refs
}' "$OASIS7_TESTNET_MANIFEST"

rtk curl -sS http://127.0.0.1:19083/v1/chain/status | jq '{
  node_id,
  world_id,
  tier: .network_tier.tier,
  status: .network_tier.status,
  chain_id: .network_tier.chain_id,
  network_id: .network_tier.network_id,
  source_path: .network_tier.source_path,
  readiness: .readiness.status,
  failed_gates: .readiness.failed_gates,
  committed_height: .consensus.committed_height,
  network_committed_height: .consensus.network_committed_height,
  network_height_lag: .observability.network_height_lag,
  connected_peer_count: .observability.connected_peer_count,
  bootstrap_peer_count: .network_tier.bootstrap_peer_count
}'
```

最低通过条件：
- `tier=public_testnet`
- `world_id == chain_id == network_id`
- `source_path` 指向本机 formal manifest
- `readiness=ready`
- `failed_gates=[]`
- `committed_height == network_committed_height`
- `network_height_lag=0`
- `connected_peer_count >= 1`
- manifest 有 `runtime_refs.genesis_ref` 和 `runtime_refs.bootstrap_peer_ref`

若 public_testnet submit endpoint 或本机 observer 不是 `ready`，不要通过手工 checkpoint/data copy “修同步”。允许的恢复判断只有两种：

1. 等待或修复自动恢复路径：manifest bootstrap peers、runtime 进程、端口、provider discovery、replication/head exchange 正常后，让节点自己追平。
2. 若自动恢复被 deployment truth 漂移或本地状态污染阻断，按 `doc/p2p/blockchain/public-testnet-governed-bootstrap.runbook.md` 从当前 deployment truth 从零重建 validator pair。

在这两种路径之外得到的状态，不能作为本 runbook 的 readiness 证据。

## 7. 启动 NewAPI quota bridge

如果本轮只验证环境 readiness，可以跳过本节；如果本轮要做“完整链路测试”或“充值/额度功能测试”，本节是必需步骤。`127.0.0.1:5841` 的 provider bridge 不会提交 OC 转账，也不会替代这里的 bind/deposit/reconcile。

先构建服务和签名转账客户端：

```bash
rtk ./scripts/cargo-dev.sh build \
  -p oasis7_protocol_tools -p oasis7_newapi_bridge_service \
  --bin oasis7_newapi_bridge_service \
  --bin oasis7_chain_transfer_submit_client
```

准备本地状态路径和 pricing rule。真实 platform key 只能来自本机 secret store 或 operator shell，不要写入仓库：

```bash
export OASIS7_NEWAPI_BRIDGE_STATE_PATH=/tmp/oasis7-newapi-bridge-state.json
export OASIS7_PROVIDER_FEEDBACK_STATE_PATH=/tmp/oasis7-provider-feedback-state.json
export OASIS7_NEWAPI_BRIDGE_CHAIN_BASE_URL=http://127.0.0.1:19083
export OASIS7_NEWAPI_BRIDGE_LETAI_BASE_URL=https://api.letai.run
export OASIS7_NEWAPI_BRIDGE_PRICING_VERSION=pv-1
export OASIS7_NEWAPI_BRIDGE_PRICING_RULES="$(
  sed -n 's/^OASIS7_NEWAPI_BRIDGE_PRICING_RULES="\(.*\)"$/\1/p' \
    scripts/newapi-bridge-service/pricing-rules.example.env
)"
```

`OASIS7_PROVIDER_FEEDBACK_STATE_PATH` 是可选的 provider-local persistence 配置；配置后可在
provider bridge 进程重启后保留有界的 request/feedback lineage。它不等价于 Runtime
journal/recovery certification，也不构成 release 或 default enablement 证明。
本手册不把 packaged/managed/remote/systemd wiring 作为当前前置条件，也不为这些部署形态
增加拓扑或上线认证要求；它们留待另行授权的运维切片。

本地报告必须明确记录 `lane`、`decision_route`、`feedback_route`。target cognition
只能记录 `target_cognition`、`/v1/world-simulator/decision-context`、
`/v1/world-simulator/feedback-context`；`legacy_compatibility_only` 只能记录
`/v1/world-simulator/decision`，feedback route 记为 `null` / `not_run`，不能把 bare
route 成功算作 target feedback 或完整决策通过。

target cognition smoke/live gate 还必须使用 Runtime-issued 的配对 wrapper artifact。先在受控路径
设置同一个变量；不要自造 payload，也不要把它与 `--legacy-compatibility-only` 混用：

```bash
export OASIS7_PROVIDER_LIVE_TARGET_CONTEXT_PAYLOAD_FILE="<local-secure-dir>/runtime-issued-context-pairs.json"
```

`OASIS7_NEWAPI_BRIDGE_CHAIN_BASE_URL` 是 bridge reconcile 读取 confirmed tx 的本机 observer/explorer 入口，不是 signed transfer 的广播入口。

启动 `oasis7_newapi_bridge_service`。下面命令只传 secret 环境变量名，不把 platform key 放进长驻进程 argv；`LETAI_PLATFORM_KEY` 必须已经在当前 operator shell 中设置：

```bash
rtk bash -lc 'target_dir=$(./scripts/cargo-dev.sh --print-target-dir); \
IFS="," read -r -a pricing_rules <<< "$OASIS7_NEWAPI_BRIDGE_PRICING_RULES"; \
cmd=("$target_dir/debug/oasis7_newapi_bridge_service" \
  --bind-addr 127.0.0.1:5852 \
  --state-path "$OASIS7_NEWAPI_BRIDGE_STATE_PATH" \
  --route-ttl-seconds 900 \
  --deposit-account-prefix oc:bridge: \
  --chain-base-url "$OASIS7_NEWAPI_BRIDGE_CHAIN_BASE_URL" \
  --chain-confirmations-required 1 \
  --letai-base-url "$OASIS7_NEWAPI_BRIDGE_LETAI_BASE_URL" \
  --letai-platform-key-env LETAI_PLATFORM_KEY \
  --reconcile-interval-seconds 0); \
for rule in "${pricing_rules[@]}"; do \
  rule="${rule//[[:space:]]/}"; \
  [[ -n "$rule" ]] && cmd+=(--pricing-rule "$rule"); \
done; \
exec "${cmd[@]}"'
```

验证 5852：

```bash
rtk curl -sS http://127.0.0.1:5852/v1/bridge/health | jq '{
  ok,
  binding_count,
  project_binding_count,
  route_count,
  ledger_count
}'
```

## 8. 执行 OC -> NewAPI/LetAI 充值链路

本节会触发 testnet 状态变化。执行前必须由 operator 明确选择测试 persona、金额、pricing version 和 memo；不要把私钥内容打印出来。

从本地 secret 文件选择一组 persona 后，只导出非私钥字段：

```bash
export BRIDGE_BASE_URL=http://127.0.0.1:5852
export CHAIN_SUBMIT_BASE_URL=<submit-capable-public-testnet-rpc-origin>
export PRICING_VERSION=pv-1
export TEST_NEWAPI_USER_REF=pk_<public-key-fingerprint>
export TEST_OASIS_SENDER_ACCOUNT_ID=oc:pk:<public-key-hex>
export TEST_EXTERNAL_USER_NAME="Oasis7 E2E User"
export TEST_PROJECT_NAME=oasis7-e2e
```

建立用户绑定：

```bash
rtk curl -sS -X POST "$BRIDGE_BASE_URL/v1/bridge/bind" \
  -H 'Content-Type: application/json' \
  -d "{
    \"newapi_user_ref\": \"$TEST_NEWAPI_USER_REF\",
    \"oasis_sender_account_id\": \"$TEST_OASIS_SENDER_ACCOUNT_ID\",
    \"external_user_name\": \"$TEST_EXTERNAL_USER_NAME\",
    \"project_name\": \"$TEST_PROJECT_NAME\",
    \"project_metadata\": {
      \"purpose\": \"local-public-testnet-full-bridge-e2e\"
    }
  }" | tee /tmp/oasis7-newapi-bind-response.json
```

分配 deposit route：

```bash
export TEST_BRIDGE_USER_ID="$(
  jq -r '.bridge_user_id' /tmp/oasis7-newapi-bind-response.json
)"

rtk curl -sS -X POST "$BRIDGE_BASE_URL/v1/bridge/deposit-route" \
  -H 'Content-Type: application/json' \
  -d "{
    \"bridge_user_id\": \"$TEST_BRIDGE_USER_ID\",
    \"pricing_version\": \"$PRICING_VERSION\"
  }" | tee /tmp/oasis7-newapi-deposit-route-response.json
```

提交 signed testnet OC transfer。金额必须等于 pricing rule 中该版本的 OC amount；`pv-1:100:100000:0` 对应 `--amount 100`。`--memo` 必须使用 route 返回的 `deposit_token`。

`CHAIN_SUBMIT_BASE_URL` 必须指向 submit-capable public_testnet endpoint，例如 formal manifest 的 `endpoint_policy.rpc_ref` 去掉 `/v1/chain/status` 后的 origin。不要把本机 observer-only `127.0.0.1:19083` 当成广播入口。

```bash
export TEST_DEPOSIT_ACCOUNT_ID="$(
  jq -r '.deposit_account_id' /tmp/oasis7-newapi-deposit-route-response.json
)"
export TEST_DEPOSIT_TOKEN="$(
  jq -r '.deposit_token' /tmp/oasis7-newapi-deposit-route-response.json
)"

rtk bash -lc 'target_dir=$(./scripts/cargo-dev.sh --print-target-dir); \
"$target_dir/debug/oasis7_chain_transfer_submit_client" submit \
  --keys-file "$OASIS7_TEST_KEYS_FILE" \
  --persona happy_path \
  --to-account-id "$TEST_DEPOSIT_ACCOUNT_ID" \
  --amount 100 \
  --nonce <operator-selected-next-nonce> \
  --memo "$TEST_DEPOSIT_TOKEN" \
  --chain-base-url "$CHAIN_SUBMIT_BASE_URL"'
```

转账 confirmed 后触发 reconcile：

```bash
rtk curl -sS -X POST "$BRIDGE_BASE_URL/v1/bridge/reconcile" | jq '{
  ok,
  observed_transfer_count,
  credited_count,
  manual_review_count,
  errors
}'
```

通过条件：
- chain explorer 能查到转入 `TEST_DEPOSIT_ACCOUNT_ID` 的 confirmed transfer。
- transfer `memo == TEST_DEPOSIT_TOKEN`。
- reconcile 进入 `credited` / `reconciled`，而不是 `manual_review`。
- bridge state 中出现同一 `bridge_user_id` 的 project binding 和 `token_key`。

## 9. 准备 LetAI provider bridge 配置

### 9.1 配置、代理与预检排障顺序

先区分字段用途：`token_key` / `api_key` 明确是 project inference token，供
`/v1/responses` 或 `/v1/chat/completions` 使用；未标注的旧 `Key` 是 ambiguous legacy
字段，可能是 inference token，也可能是 platform key；`platform_key` 是管理
user/project 和 topup 的平台凭据，不能直接作为 inference token；`platform_user_id`
用于 direct auto-topup，`platform_project_id` 仅保留映射/审计关联。`Doc` 不是 API
endpoint。`with-letai-llm-config.sh` 要求 inference token，所以第一次调用 wrapper
前必须先识别/规范化来源文件；旧 `Key` 会按长度启发式分类（至少 50 个字符视为
platform key），新配置应显式写 `token_key` 或 `platform_key`。

macOS `scutil --proxy` 的 GUI 设置不会自动进入当前进程。若同一凭据直连返回 HTML
Cloudflare `403`，而经本机代理返回 JSON API 错误，先把 operator 确认的代理显式传给
同一个 shell，再判断凭据：

```bash
rtk scutil --proxy | sed -n '1,80p'
export http_proxy="${OASIS7_PROXY_URL:?set approved HTTP proxy URL}"
export HTTP_PROXY="$http_proxy"
export https_proxy="$http_proxy"
export HTTPS_PROXY="$https_proxy"
if [[ -n "${OASIS7_SOCKS_PROXY_URL:-}" ]]; then
  export all_proxy="$OASIS7_SOCKS_PROXY_URL"
  export ALL_PROXY="$all_proxy"
else
  unset all_proxy ALL_PROXY
fi
export no_proxy="127.0.0.1,localhost,::1${no_proxy:+,$no_proxy}"
export NO_PROXY="$no_proxy"
```

PWT-004/hosted strong-auth runner 注意：LetAI provider probe 与 launcher/runtime（包括
`scripts/viewer-prompt-control-regression.sh`）必须从同一 shell 启动，并显式继承 operator-approved
的 `http_proxy`/`HTTP_PROXY`、`https_proxy`/`HTTPS_PROXY` 与 `all_proxy`/`ALL_PROXY`；`NO_PROXY`
（以及 `no_proxy`）必须含 `127.0.0.1,localhost,::1`，以保护本地 launcher/viewer/runtime。直连
Cloudflare `403` 后 SDK 出现的 `decode_error` 只是代理遗漏的二次症状：先在同一 shell 经代理重试，
不能先充值、换 token/model 或 API path。代理地址和凭据只来自 operator 私有环境，不写入文档。

然后按一次性、低成本顺序操作，不要扫遍历史模型或无限重试付费请求：

1. 先把来源文件规范化到私有 run 目录；已有 inference token 只会被安全复制，平台 key 或 ambiguous `Key` 则按 helper 规则生成/保留 project token：

   ```bash
   LETAI_TOKEN_SOURCE_FILE="$LETAI_TOKEN_FILE"
   export LETAI_TOKEN_FILE="$RUN_DIR/letai-local-token.env"
   rtk ./scripts/ensure-letai-local-token-config.sh \
     --config "$LETAI_TOKEN_SOURCE_FILE" \
     --out "$LETAI_TOKEN_FILE" \
     --model "$MODEL"
   ```

   后续命令必须继续使用已显式赋值的 `LETAI_TOKEN_FILE`，避免把 platform key 当作 inference token。

   如果本轮已经有由同一 W3 provisioning/topup 流程产生的权威 project mapping，必须把它
   作为显式输入传给规范化 helper；不要再从另一个本地 token 文件猜测 project：

   ```bash
   export LETAI_AUTHORITATIVE_MAPPING_FILE="$RUN_DIR/authoritative-project-mapping.env"
   rtk ./scripts/ensure-letai-local-token-config.sh \
     --config "$LETAI_TOKEN_SOURCE_FILE" \
     --authoritative-mapping "$LETAI_AUTHORITATIVE_MAPPING_FILE" \
     --out "$LETAI_TOKEN_FILE" \
     --model "$MODEL"
   ```

   mapping 文件必须显式包含 `token_key`/`api_key` 和 `platform_project_id`，可选包含
   `platform_user_id`。helper 会在任何平台管理请求前比较所选 project token 与 mapping；
   不一致时以 `authoritative project token mapping mismatch` 失败关闭，不输出 token，也不应
   继续 topup 或 completion。匹配时只输出字段存在性、匹配结果和短 fingerprint，生成文件仍
   必须留在私有 `0600` run 目录。该 mapping 是本轮 project 的唯一 authority；后续 wrapper、
   native QA 和 provider retry 都必须继续使用同一个 `LETAI_TOKEN_FILE`。
2. 运行 `rtk ./scripts/with-letai-llm-config.sh --config "$LETAI_TOKEN_FILE" --print-config`，只看 host、模型和字段存在性；不要输出 key。
3. 在同一个 wrapper 环境内请求 `/v1/models`，只输出当前 model id；`200` 只证明可见性，不证明该模型能预扣费或完成 tool-call。
4. Builtin Hosted W3 使用 `rtk ./scripts/check-active-llm-provider.sh --pretty` 验证 Responses 文本和 tool-call；local bridge 使用 `rtk ./scripts/check-letai-chat-completions.sh` 验证 chat-completions。两条协议的成功不能互相替代。

安全的模型列表请求可通过环境变量承载 token，避免 token 出现在命令参数或输出中：

```bash
rtk ./scripts/with-letai-llm-config.sh --config "$LETAI_TOKEN_FILE" -- python3 - <<'PY'
import json, os, ssl, urllib.error, urllib.request
base = os.environ["OASIS7_LLM_BASE_URL"].rstrip("/")
request = urllib.request.Request(base + "/models", headers={"Authorization": "Bearer " + os.environ["OASIS7_LLM_API_KEY"]})
try:
    with urllib.request.urlopen(request, timeout=30, context=ssl.create_default_context()) as response:
        payload = json.load(response)
except urllib.error.HTTPError as error:
    print(f"models_http_status={error.code}")
    raise SystemExit(1)
except (urllib.error.URLError, json.JSONDecodeError) as error:
    print(f"models_request_error={type(error).__name__}")
    raise SystemExit(1)
models = payload.get("data") if isinstance(payload, dict) else []
print(json.dumps({"models": sorted(str(item["id"]) for item in models if isinstance(item, dict) and item.get("id"))}, ensure_ascii=False))
PY
```

### 9.2 错误分类

| 签名 | 分类 | 下一步 |
| --- | --- | --- |
| 直连 HTML/Cloudflare `403`、`decode_error` | `edge403`，常见原因是进程没有继承代理 | 对同一 host/凭据显式代理重试一次；不要先改 API path |
| 代理后 `401 Invalid token`，或 `/models` 为 `401` | `API401`，inference token 无效、过期或字段取错 | 重新核对 project token；平台 key 只能用于管理 API |
| `404/503 model_not_found`、`no available channel` | `model_not_found`，当前 credential/channel 不支持该模型 | 重新读当前 `/models`，选当时可见模型；模型列表不是永久清单 |
| `403 insufficient_user_quota` 或包含 `余额` | `quota403`，请求已到额度检查但余额不足 | 见 9.3 的单次受控 topup；先降低输出上限 |

`/v1/provider/health` 的 degraded 不能单独判定失败，因为 `/models` 探测可能被 provider
以 `401` 拒绝；应结合选定 lane 的实际 completion/decision 结果。反过来，模型列表
返回 `200` 也不能替代实际 provider preflight。

### 9.3 受控 auto-topup 与不确定响应

`5841` provider auto-topup 只补 LetAI provider quota，不提交 OC transfer，也不替代
`5852` bridge 的 bind/deposit/reconcile。现有 provider CLI 只有在 completion 明确返回
`insufficient_user_quota`/`余额`，并同时设置正数的
`OASIS7_REMOTE_LLM_AUTO_TOPUP_USD`（或 `LETAI_AUTO_TOPUP_USD`）、
`OASIS7_REMOTE_LLM_PLATFORM_KEY` 和 `OASIS7_REMOTE_LLM_PLATFORM_USER_ID` 时才会尝试
管理 API topup；platform project id 不是 direct topup 的必需字段。

`run-local-letai-game-test.sh --auto-topup-usd <amount>` 与
`run-local-letai-provider-bridge.sh --auto-topup-usd <amount>` 设置
`OASIS7_LETAI_AUTO_TOPUP_USD`，bridge 再映射到 remote 变量。当前 local helper 默认
`$0.10`；它只是本地默认金额，不是生产或 OC 充值规则。topup 后 completion 最多默认
重试 3 次、间隔 1000ms，可用 `OASIS7_REMOTE_LLM_AUTO_TOPUP_RETRY_COUNT` 和
`OASIS7_REMOTE_LLM_AUTO_TOPUP_RETRY_DELAY_MS` 在本轮内缩小或保持有界。

管理 API 的 endpoint 是 `/api/platform/open/users/<platform_user_id>/topups`，helper
使用 `oasis7-local-auto-topup-<unix-seconds>` 作为 `external_order_id`；平台必须按该
外部订单号幂等处理。成功事件只在 POST 返回后写入 stderr；如果响应超时、连接断开或
无法确定：停止，不要盲目重跑；保留私有时间、user mapping 和脱敏错误，交由
provider/QA owner 通过订单号或平台后台确认“已创建 / 明确未创建”后再继续。当前
helper 没有 pending-order journal 或订单查询命令，无法自行证明 ambiguous POST 没有
扣款；任何不确定订单必须保持 pending。

因此必须分别记录 `topup`（未启用/明确成功/明确失败/状态未知）、`provider_preflight`
和 real Hosted W3/browser acceptance。topup 成功不等于 preflight 成功，preflight 成功
也不等于 Hosted W3 或 OC -> NewAPI/LetAI 充值链路完成。

通用成功判据是：平台 topup 请求返回 `2xx` 且不是 `success=false`，随后同一请求的
有界 completion retry 成功，并由 provider/QA 的权威订单回执确认已创建/结算，才可将
本轮 provider topup 记为“明确成功”；若缺少回执，或只有 HTTP 响应、helper trace 或
一次 `/models` `200`，仍只能记为“已接受/未确证结算”。
`provider_preflight` 还必须在选定协议中得到结构化 completion/decision；Hosted W3
和 OC 充值则分别满足各自 lane 的真实 browser、reconcile 与 receipt 条件。

**已退役的历史诊断示例，不用于当前配置生成或充值操作。** 下列片段仅保留旧排障方法的来源：它无条件将 `Key:` 解释为 `token_key`，并拼接旧平台字段，不能证明 inference token 类型或当前权威 user/project mapping；字段齐全也不代表身份正确。当前配置只按 §9.1 的既有 helper 和 authoritative mapping 生成、核验，并沿用该节显式设置的 `LETAI_TOKEN_FILE`。不得执行下列旧片段来生成当前配置，也不得将其临时输出交给当前 bridge。

```bash
rtk python3 - <<'PY'
from pathlib import Path

import os

merged = Path("/tmp/oasis7-letai-merged-local-bridge.env")
old = Path(os.environ["LETAI_PLATFORM_ENV"])
new = Path(os.environ["LETAI_TOKEN_FILE"])
values = {}

for line in new.read_text(errors="replace").splitlines():
    raw = line.strip()
    if not raw or raw.startswith("#"):
        continue
    sep = "=" if "=" in raw else ":" if ":" in raw else None
    if not sep:
        continue
    key, value = raw.split(sep, 1)
    key = key.strip().lower()
    value = value.strip().strip('"').strip("'")
    if key == "key":
        values["token_key"] = value
    elif key == "base_url":
        values["base_url"] = value

for line in old.read_text(errors="replace").splitlines():
    raw = line.strip()
    if not raw or raw.startswith("#"):
        continue
    sep = "=" if "=" in raw else ":" if ":" in raw else None
    if not sep:
        continue
    key, value = raw.split(sep, 1)
    key = key.strip()
    value = value.strip().strip('"').strip("'")
    if key in {"platform_key", "platform_user_id", "platform_project_id"}:
        values[key] = value

values["model"] = "gpt-5.4"
required = [
    "token_key",
    "base_url",
    "platform_key",
    "platform_user_id",
    "platform_project_id",
    "model",
]
missing = [key for key in required if not values.get(key)]
if missing:
    raise SystemExit("missing keys: " + ",".join(missing))

merged.write_text("".join(f"{key}={values[key]}\n" for key in required))
merged.chmod(0o600)
print({"path": str(merged), "keys": required, "value_lengths": {key: len(values[key]) for key in required}})
PY
```

使用 §9.1 已生成并核验的当前配置做 sanitized config 验证，不回显密钥；以下命令不使用上面的退役示例输出：

```bash
rtk ./scripts/run-local-letai-provider-bridge.sh \
  --config "$LETAI_TOKEN_FILE" \
  --model gpt-5.4 \
  --print-config
```

## 10. 启动 LetAI provider bridge

低余额或充值传播测试建议先用较小输出上限，避免预扣费额度过高掩盖真实链路。若本轮目标包含 OC -> NewAPI/LetAI 充值，provider 必须读取第 7 节的 bridge state，并通过 `newapi_user_ref:<ref>` bearer selector 消费刚发出的 project token：

```bash
rtk env \
  http_proxy=http://127.0.0.1:7897 \
  https_proxy=http://127.0.0.1:7897 \
  all_proxy=socks5://127.0.0.1:7897 \
  OASIS7_REMOTE_LLM_NEWAPI_BRIDGE_STATE_PATH=/tmp/oasis7-newapi-bridge-state.json \
  OASIS7_PROVIDER_FEEDBACK_STATE_PATH=/tmp/oasis7-provider-feedback-state.json \
  OASIS7_REMOTE_LLM_MAX_OUTPUT_TOKENS=64 \
  ./scripts/run-local-letai-provider-bridge.sh \
    --config "$LETAI_TOKEN_FILE" \
    --model gpt-5.4 \
    --bind 127.0.0.1:5841 \
    --provider-agent letai-local-token-file
```

验证 bridge 基本信息：

```bash
rtk curl -sS http://127.0.0.1:5841/v1/provider/info | jq '{
  provider_id,
  capabilities
}'
```

provider decision smoke 分为两个互斥 lane：

下面的 legacy compatibility-only 命令是可选的旧 DTO 可达性审计，只能产生 legacy evidence，
不计入环境 readiness、target cognition 或完整 LLM 决策通过。需要 target 证明时，必须使用
Runtime-issued `decision_context`/`feedback_context` pair artifact，并调用 provider contract
smoke 的 `--target-context-payload-file`。

```bash
rtk ./scripts/provider-remote-https/provider-bridge-contract-smoke.sh \
  --base-url http://127.0.0.1:5841 \
  --auth-token "newapi_user_ref:$TEST_NEWAPI_USER_REF" \
  --timeout-ms 90000 \
  --legacy-compatibility-only \
  --decision-count 1 \
  --min-successes 1
```

target feedback 的当前 bridge 合同是 diagnostic-only。`feedback-context` 必须携带与
本次 `decision-context` 完全一致的 agent/session/turn/request identity 和
`request_digest`，并先在 bridge 中找到已接受的 decision response。只允许 bounded
`pending`/`rejected`/`failed` diagnostics（reason 必须在当前 status allowlist）；这类
反馈只是 provider-local diagnostic，不产生 Runtime action、committed receipt 或
world projection，也不构成本地/remote recovery 证明。

以下结果必须明确记为拒绝，不能写成“已提交”或“已执行”：

- `status=committed`，或带 `candidate_action_id` / `runtime_receipt_id`：HTTP `409`
  `feedback_disposition_unverifiable`。
- 带当前 `FeedbackEnvelopeV1` 未定义的 `projection` 或其他 outer field：HTTP `400`
  `unknown_context_field`。
- 未知 request、session/turn 不匹配、request digest 不匹配：HTTP `409`，分别为
  `unknown_feedback`、`feedback_correlation_mismatch`、`feedback_digest_mismatch`。
- reason 缺失、超界或不在 status allowlist：HTTP `409`
  `feedback_disposition_reason_invalid`。

只有同一 request lineage 的 bounded diagnostic 返回 `{"ok":true}` 才能记为
feedback accepted；这不等于 Runtime authority 或恢复/发布证明。若 contract smoke
输出使用 `decision_path` / `feedback_path`，归档时分别规范化为
`decision_route` / `feedback_route`。

```bash
rtk ./scripts/provider-remote-https/provider-bridge-contract-smoke.sh \
  --base-url http://127.0.0.1:5841 \
  --auth-token "newapi_user_ref:$TEST_NEWAPI_USER_REF" \
  --timeout-ms 90000 \
  --target-context-payload-file "${OASIS7_PROVIDER_LIVE_TARGET_CONTEXT_PAYLOAD_FILE:?set Runtime-issued target context pair artifact}" \
  --decision-count 1 \
  --min-successes 1
```

若本轮是在测试充值/额度功能，`insufficient_user_quota` 不自动等于环境失败，但必须先区分来源：
- `5852` reconcile 前余额不足：这是充值前状态。
- `5852` reconcile 已 `credited/reconciled` 后仍余额不足：这是充值传播、token/project 绑定或 provider bearer selector 问题。
- 只看到 `5841` provider auto-topup trace，不能算 OC -> NewAPI 充值链路通过。

如果测试目标是非充值的 target cognition / 完整 LLM 决策流程，则必须执行上面的 target lane，
并满足 `decision_successes >= 1`；legacy compatibility-only 成功不计入该通过条件。

## 11. 启动 viewer live 并绑定同一 testnet 节点

先构建 viewer live：

```bash
rtk ./scripts/cargo-dev.sh build -p oasis7_viewer_runtime --bin oasis7_viewer_live
```

启动 live 服务，显式绑定已同步节点：

```bash
rtk bash -lc 'target_dir=$(./scripts/cargo-dev.sh --print-target-dir); chain_submit_bind="${OASIS7_PUBLIC_TESTNET_CHAIN_SUBMIT_BIND:?set submit-capable host:port first}"; env \
  OASIS7_AGENT_DECISION_SOURCE=provider_backed \
  OASIS7_AGENT_PROVIDER_BACKEND=provider_local_bridge \
  OASIS7_AGENT_PROVIDER_CONTRACT=worldsim_provider_v1 \
  OASIS7_AGENT_PROVIDER_TRANSPORT=loopback_http \
  OASIS7_AGENT_PROVIDER_URL=http://127.0.0.1:5841 \
  OASIS7_AGENT_PROVIDER_CONNECT_TIMEOUT_MS=90000 \
  OASIS7_AGENT_PROVIDER_DECISION_TIMEOUT_MS=90000 \
  OASIS7_AGENT_PROVIDER_PROFILE=oasis7_p0_low_freq_npc \
  OASIS7_AGENT_EXECUTION_LANE=headless_agent \
  "$target_dir/debug/oasis7_viewer_live" \
    --bind 127.0.0.1:5023 \
    --web-bind 127.0.0.1:5011 \
    --deployment-mode trusted_local_only \
    --chain-status-bind 127.0.0.1:19083 \
    --chain-execution-world-dir "$CHAIN_EXECUTION_WORLD_DIR" \
    --chain-submit-bind "$chain_submit_bind" \
    --chain-link-policy enforcing \
    --llm'
```

启动前由操作者把 `CHAIN_EXECUTION_WORLD_DIR` 设为本机节点实际发布 execution checkpoint 的 root；使用脚本时传 `--chain-execution-world-dir` 或 `OASIS7_CHAIN_EXECUTION_WORLD_DIR`。不从 status 响应推导该路径。缺可验证 generation 时等待 writer；`RecoveryRequired` 由 writer 恢复，`ResourceLimited` 拒收并保留旧展示。

本 runbook 使用 direct `oasis7_viewer_live`，不是 `run-launcher-stack.sh`。原因是当前 wrapper 没有完整暴露 formal manifest / attach-existing-node 路径；直接绑定 `--chain-status-bind 127.0.0.1:19083` 能更清楚证明 viewer/API 从同一个 public_testnet world state 读取 committed snapshot。`--chain-submit-bind` 必须单独指向 submit-capable endpoint；如果它也指向 observer-only 的 `127.0.0.1:19083`，玩家 action 可能只会进入 observer pending queue。

## 12. 启动静态 viewer 入口

如果已有 `4173` 静态服务，可复用。否则启动：

```bash
rtk python3 -m http.server 4173 \
  --bind 127.0.0.1 \
  --directory crates/oasis7_viewer/dist
```

若 `crates/oasis7_viewer/dist/viewer.html` 不存在，不要 fallback 到 `crates/oasis7_viewer` 源目录；那会重新触发 pixel-world bindgen stub 路径。先构建/同步 dist 后再启动静态入口；`dist/software_safe.html` 只用于兼容 alias 复核。

确认入口可访问：

```bash
rtk curl -sSI 'http://127.0.0.1:4173/viewer.html?ws=ws://127.0.0.1:5011&test_api=1&locale=zh' \
  | sed -n '1,5p'
```

## 13. 最终复核

端口：

```bash
rtk lsof -nP \
  -iTCP:19083 \
  -iTCP:5852 \
  -iTCP:5841 \
  -iTCP:5011 \
  -iTCP:5023 \
  -iTCP:4173 \
  -sTCP:LISTEN
```

节点状态：

```bash
rtk curl -sS http://127.0.0.1:19083/v1/chain/status | jq '{
  world_id,
  tier: .network_tier.tier,
  chain_id: .network_tier.chain_id,
  network_id: .network_tier.network_id,
  readiness: .readiness.status,
  failed_gates: .readiness.failed_gates,
  h: .consensus.committed_height,
  nh: .consensus.network_committed_height,
  lag: .observability.network_height_lag,
  peers: .observability.connected_peer_count
}'
```

provider：

```bash
rtk curl -sS http://127.0.0.1:5852/v1/bridge/health | jq '{
  ok,
  binding_count,
  project_binding_count,
  route_count,
  ledger_count
}'

rtk curl -sS http://127.0.0.1:5841/v1/provider/info | jq '{
  provider_id,
  capabilities
}'

rtk ./scripts/provider-remote-https/provider-bridge-contract-smoke.sh \
  --base-url http://127.0.0.1:5841 \
  --auth-token "newapi_user_ref:$TEST_NEWAPI_USER_REF" \
  --timeout-ms 90000 \
  --target-context-payload-file "${OASIS7_PROVIDER_LIVE_TARGET_CONTEXT_PAYLOAD_FILE:?set Runtime-issued target context pair artifact}" \
  --decision-count 1 \
  --min-successes 1
```

入口：

```bash
rtk curl -sSI 'http://127.0.0.1:4173/viewer.html?ws=ws://127.0.0.1:5011&test_api=1&locale=zh' \
  | sed -n '1,5p'
```

## 14. 判定矩阵

| 测试目标 | 必须通过 | 可接受的充值分支 |
| --- | --- | --- |
| 本地启动 test 环境 | manifest/world/chain/network、节点 ready/synced、viewer/API 绑定同一节点、页面 200 | 不涉及 LetAI 充值 |
| LLM-backed agent 决策 | provider info、target decision/feedback smoke `decision_successes >= 1` | legacy compatibility-only 成功不计入；不接受 `insufficient_user_quota` 作为通过 |
| OC -> NewAPI/LetAI 充值功能 | `5852` health、bind、deposit route、signed testnet OC transfer、reconcile `credited/reconciled`、provider 用 `newapi_user_ref:<ref>` 消费成功 | `insufficient_user_quota` 是有效测试现象，但只有在 reconcile 前或明确记录为传播/绑定问题时才可接受 |
| 环境 readiness | testnet 大世界入口 + target provider decision/feedback 通过；若声称完整链路，还必须包含 `5852` 充值证据 | 需要在测试报告中明确当前是环境 readiness、充值链路，还是稳定 LLM 决策绿灯；本表不构成 release/default enablement 证明 |
| 玩家 UI 端到端测试 | 本 runbook 环境 ready 后，再运行 Playwright/agent-browser 用例并产出 UI 操作证据 | 充值分支只能说明 provider/额度路径，不替代玩家 UI 操作通过 |

## 15. 常见问题

### `/v1/provider/health` 是 `degraded`

当前 LetAI `/models` health 探测可能返回 401。不要只看 `/health` 判定 provider 失败；以 `provider-bridge-contract-smoke.sh` 的 decision 结果或充值分支证据为准。

### `insufficient_user_quota`

这是充值/额度测试的核心现象之一。若目标是完整充值链路，先看 `5852` 是否完成 bind、deposit route、signed transfer 和 reconcile；reconcile 前余额不足是正常前置状态，reconcile 后仍不足才进入传播/绑定/token 诊断。若目标是 target cognition / 普通 LLM 决策，必须先执行配对的 decision/feedback target smoke；legacy compatibility-only 成功不能替代它。降低 `OASIS7_REMOTE_LLM_MAX_OUTPUT_TOKENS` 后 target lane 仍失败才应阻断。

### `auto_topup_skipped` / `platform_key_missing`

说明 token 文件只有 API key，没有平台充值字段。它只解释 provider direct auto-topup 失败，不等价于 OC -> NewAPI bridge 失败。完整充值链路应先启动 `oasis7_newapi_bridge_service` 并用它完成 reconcile。

### 5852 没有监听

不能宣称“完整链路测试环境已就绪”。这时最多只能说本地 public_testnet world state + provider consumer 路径部分可用；OC -> NewAPI/LetAI 充值链路尚未启动。

### public_testnet 节点 `readiness=not_ready`

不要用手工 checkpoint/data copy 修本地或远端 testnet 同步。先判断是否能通过自动 replication/head exchange 恢复；若不能，按 governed bootstrap runbook 重新生成 deployment truth 并从零重建。只有最终节点通过自动同步或重建后返回 `readiness=ready`、`failed_gates=[]`，才能继续本 runbook 的 viewer/API/provider/充值验证。

### `run-launcher-stack.sh` 能不能替代本 runbook

当前不能作为本 runbook 的等价证明路径。它适合常规本地启动，但这里需要证明 viewer/API 指向已有 formal `public_testnet` local node；在 wrapper 暴露 attach-existing-node/formal manifest 参数前，继续用 direct `oasis7_viewer_live`。

## 16. 记录模板

每次搭建或复核，在 PR、实际 CI 与评审记录（Issue 按需） 或测试报告中至少记录：

```text
- manifest path:
- world_id / chain_id / network_id:
- genesis_ref:
- bootstrap_peer_ref:
- node readiness / failed_gates:
- committed_height / network_committed_height / lag:
- connected_peer_count:
- viewer URL:
- viewer live ws/api bind:
- newapi bridge URL / state path:
- bind bridge_user_id:
- deposit_account_id / deposit_token present:
- signed transfer tx id / height / memo match:
- reconcile result:
- provider bridge URL:
- provider auth selector:
- target context payload path (Runtime-issued, sanitized):
- provider feedback state path (optional, sanitized):
- provider model / max_output_tokens:
- lane (`target_cognition` / `legacy_compatibility_only`):
- decision_route:
- feedback_route:
- provider decision smoke result:
- feedback accepted/rejected summary and `error_code`:
- recharge branch result, if any:
- caveats:
```

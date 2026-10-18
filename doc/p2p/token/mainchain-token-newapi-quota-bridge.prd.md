# oasis7 主链 Token 到 LetAI Run OpenAPI 额度桥接方案

- 对应设计文档: `doc/p2p/token/mainchain-token-newapi-quota-bridge.design.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

审计轮次: 2

## 1. Executive Summary
- Problem Statement（原设计基线）: 本专题最初提出时，`oasis7` 已经有 `OC`、`oc:pk:` 与签名化 transfer submit，但从链上 `OC` 到 LetAI Run 大模型可调用 `token_key` 的路径只做到了“入账后向一个 generic credit endpoint 发 POST”。这无法覆盖 LetAI Run OpenAPI 的真实对象模型，也无法保证每个用户/项目的 `token_key`、充值和审计查询闭环。
- 当前源码范围：固定 main `b5476a616dd3d4dbc29f332ee42ed848903c1f60` 的 [service_letai.rs](https://github.com/nil-studio/oasis7/blob/b5476a616dd3d4dbc29f332ee42ed848903c1f60/crates/oasis7_newapi_bridge_service/src/bin/oasis7_newapi_bridge_service/service_letai.rs) 已包含 user upsert、project/token 确保与持久化、topup、user summary/project token summary/log 查询及 reconciled 写入编排；generic-only 不再是当前源码事实。源码存在不等于完整业务或部署验收。配对设计的 [首次绑定恢复](mainchain-token-newapi-quota-bridge.design.md#bridge-identity-recovery)、[扫描完整性](mainchain-token-newapi-quota-bridge.design.md#bridge-scan-completeness)、[有效处理权](mainchain-token-newapi-quota-bridge.design.md#bridge-reconcile-ownership)与[持久业务阶段](mainchain-token-newapi-quota-bridge.design.md#bridge-recovery-state)四项恢复验证仍为 planned，不因已有对象编排而关闭。
- Proposed Solution: 把桥接能力收口为一条 operator-managed 的 `OC -> LetAI Run OpenAPI quota` bridge。bridge-service 在确认 `OC` 入账后，按 LetAI Run OpenAPI 的真实链路执行 `创建/获取平台用户 -> 创建/获取项目并返回 token_key -> 给平台用户充值额度 -> 查询额度/日志做验证`，并持久化 `platform_user_id/platform_project_id/token_key/external_order_id` 等审计字段。
- Success Criteria:
  - SC-1: 自动 bridge 必须明确支持 `parent channel(platform key)` + 动态创建的 per-user project + per-project `token_key`；不得继续停留在单一 generic credit POST 语义。
  - <a id="tbridge-sc-2"></a> SC-2: 每一笔成功 topup 都必须可追溯到 `bridge_deposit_id -> chain tx -> platform_user_id -> platform_project_id -> token_key -> external_order_id -> LetAI topup / query receipt`，幂等对账完整率 `100%`。
  - SC-3: 模型调用语义必须冻结为“项目 `token_key` 用于实际调用模型接口，平台 Key 只用于开放管理接口”，不得混用。
  - <a id="tbridge-sc-4"></a> SC-4: 同一平台用户下可存在多个项目；bridge 默认必须为每个 bridge user 动态创建或复用一个独立 project，并把该 project 的 `token_key` 持久化为唯一真值。
  - SC-5: 对外 claim 必须继续明确为 `limited preview operator-managed service-credit bridge`；不支持 `OC <- quota/token_key` 兑回，不承诺公开兑换所、AMM、浏览器热钱包充值或自动提现。
  - SC-6: 收款策略必须冻结为“每个 newapi 接入方维护一组受控 OC 收款账号，route 负责把单次充值映射到该账号池内的某个可用账号”；当前默认推荐小池化（例如 5 个账号）而不是全平台单账号或每单现生地址。

## 2. User Experience & Functionality
- User Personas:
  - `producer_system_designer`：需要冻结“这是一条 LetAI Run 服务额度桥，不是交易所/钱包产品”的边界。
  - `runtime_engineer`：需要维护已有 LetAI OpenAPI 对象编排，并补齐已记录的可恢复/可对账义务。
  - bridge operator：需要配置父级渠道（platform key / parent channel），运维独立 bridge-service、watcher、对账和异常收口。
  - LetAI Run operator：需要让 bridge 走官方 OpenAPI，而不是在 repo 内写死内部私有 credit 逻辑。
  - invited user / contributor：需要把 `OC` 充值为自己项目可用的模型调用 `token_key` 和对应额度，而不是理解成资产双向兑换。
  - `qa_engineer` / `liveops_community`：需要验证重复 topup、错绑用户、项目复用、`token_key` 漂移和对外口径边界。
- User Scenarios & Frequency:
  - invited user top-up：每次用户需要补充 LetAI Run 调用额度时触发。
  - first-time bridge onboarding：每个新用户第一次把 `OC` 充值到 LetAI Run 时触发一次 user/project/token_key 初始化。
  - operator reconciliation：每日或每批次核对 `OC` 入账、LetAI topup 记录、额度概览和日志。
  - pricing / audit review：每次调整 `OC -> quota` 定价表、补贴策略或审计口径时触发。
- User Stories:
  - PRD-P2P-TBRIDGE-001: As a `producer_system_designer`, I want one canonical one-way bridge boundary, so that `OC` 的服务额度充值不会被误解成公开交易所或双向兑回承诺。
  - <a id="tbridge-002"></a> PRD-P2P-TBRIDGE-002: As a bridge operator, I want every confirmed deposit to drive a stable LetAI OpenAPI lifecycle, so that同一笔 `OC` 入账总能唯一映射到一个平台用户、一个项目、一个 `token_key` 和一个 topup order。 局部恢复设计：[首次绑定恢复](mainchain-token-newapi-quota-bridge.design.md#bridge-identity-recovery)、[扫描完整性](mainchain-token-newapi-quota-bridge.design.md#bridge-scan-completeness)、[有效处理权](mainchain-token-newapi-quota-bridge.design.md#bridge-reconcile-ownership)；仍为目标合同，非已交付证明。
  - PRD-P2P-TBRIDGE-003: As a LetAI Run operator, I want the bridge to call official OpenAPI endpoints for user/project/token/topup/query, so that repo truth matches actual product semantics instead of a generic adapter placeholder.
  - <a id="tbridge-004"></a> PRD-P2P-TBRIDGE-004: As a `qa_engineer`, I want stable anomaly states and verification snapshots, so that user/project create drift、topup retry、query mismatch 和 `token_key` 缺失都不会静默变成错误额度。 局部恢复设计：[持久业务阶段](mainchain-token-newapi-quota-bridge.design.md#bridge-recovery-state)；仍为目标合同，非已交付证明。
- Critical User Flows:
  1. Flow-TBRIDGE-001: `用户先绑定 bridge user -> operator 已提供 parent channel / platform key 与接入方收款账号池 -> bridge-service 分配唯一 deposit route -> route 选择池内一个可用收款账号 -> 用户向该账号转入 OC`
  2. Flow-TBRIDGE-002: `chain watcher 观察到 confirmed deposit -> bridge_ledger 命中定价表 -> bridge-service 通过 POST /api/platform/open/users/upsert 确保平台用户存在`
  3. Flow-TBRIDGE-003: `bridge-service 根据 bridge user 动态创建或复用用户专属 project -> 获取该 project 的 token_key -> 持久化 platform_project_id/token_key`
  4. Flow-TBRIDGE-004: `bridge-service 调用 POST /api/platform/open/users/:platform_user_id/topups，写入 external_order_id/quota/amount/currency -> 使用额度概览和日志接口做验证 -> ledger 转为 reconciled`
  5. <a id="tbridge-flow-005"></a> Flow-TBRIDGE-005: `若 user/project/token/topup/query 任一步失败或返回不一致 -> 保留同一 external_order_id / idempotency key -> 重试或转 manual_review`
  6. Flow-TBRIDGE-006: `外部提出“既然有 token_key，是否等于用户直接买到资产并可兑回” -> 对照 PRD 检查 one-way service-credit 边界 -> 直接驳回`
- Functional Specification Matrix:
| 功能点 | 字段定义 | 动作行为 | 状态转换 | 计算/判定规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 用户绑定 | `bridge_user_id`、`newapi_user_ref`、`oasis_sender_account_id`、`letai_external_user_id`、`platform_user_id`、`status` | 用户完成 bridge 绑定；reconcile 前会以 `external_user_id` upsert LetAI 平台用户 | `unbound -> active`，或 `active -> disabled` | `external_user_id` 默认必须稳定映射到一个 bridge user；后续查询只用 `platform_user_id` | 仅 bridge user 自己可发起；operator 可停用 |
| 动态项目 | `letai_external_project_id`、`platform_project_id`、`project_name`、`token_key`、`token_status` | confirmed deposit 前后确保每个 bridge user 存在一个专属 LetAI project，并获取项目 `token_key` | `missing -> provisioning -> active`，或 `active -> stale/manual_review` | 一个项目对应一个 `token_key`；同一平台用户下多个项目必须各用各自 `token_key` | 仅 bridge-service 服务账号可创建/刷新 |
| 入账路由 | `route_id`、`provider_id`、`deposit_account_id`、`pricing_version`、`topup_plan_id`、`expires_at`、optional `deposit_token` | bridge-service 分配唯一 deposit route，并从接入方收款账号池选择入账账号 | `draft -> issued -> settled/expired` | 自动 topup 前必须唯一映射 beneficiary 与 pricing context；在没有可验证的链上归因 token 前，单个收款账号同一时刻只允许绑定一个活跃 route；若后续链上转账支持 `message/memo` 且 bridge 已能解析必填 `deposit_token`，才允许一个账号承载多个活跃 route | 仅 bridge-service 生成；operator 可作废 |
| 链上充值检测 | `chain_tx_id`、`amount_oc`、`confirmations`、`required_confirmations`、`block_height` | watcher 轮询 explorer 并写入 `bridge_ledger` | `detected -> pending_confirmations -> confirmed/rejected` | 未达到确认窗口不得进入 LetAI OpenAPI 调用 | 只读链上 |
| LetAI 用户 upsert | `external_user_id`、`external_user_name`、`email`、`metadata`、`platform_user_id` | 调用 `POST /api/platform/open/users/upsert` 创建或获取平台用户 | `pending_user -> user_ready/manual_review` | 用户创建按 `external_user_id` 幂等；一旦获得 `platform_user_id`，后续 query/topup 必须用内部 ID | 仅 bridge-service 服务账号可调用 |
| LetAI 项目/Token | `external_project_id`、`platform_project_id`、`token_key` | 调用“创建或获取项目并返回 Token”接口，为用户 project 返回 `token_key` | `pending_project -> project_ready/manual_review` | 项目创建按 `external_project_id` 幂等；`token_key` 缺失视为失败 | 仅 bridge-service 服务账号可调用 |
| LetAI topup | `external_order_id`、`quota`、`amount`、`currency`、`topup_receipt` | 调用 `POST /api/platform/open/users/:platform_user_id/topups` 充值额度 | `confirmed -> crediting -> credited/reconciled`，或 `failed/manual_review` | `external_order_id` 必须稳定且可重试；`quota` 为最小单位，若需 USD 展示按 `quota / 500000` 仅用于审计展示 | 仅 bridge-service 服务账号可调用 |
| LetAI query 验证 | `balance_snapshot`、`project_summary`、`topup_log_snapshot` | 调用用户额度概览、项目 Token 汇总和用户日志接口做验证 | `credited -> reconciled`，或 `credited -> manual_review` | topup 成功不能只看 2xx；至少需要一份 query snapshot 回写 | 仅 bridge-service 服务账号可调用 |
| 手工审查 | `review_reason`、`operator_note`、`resolution` | 处理 underpay / overpay / project create drift / topup mismatch / query mismatch | `manual_review -> resolved/closed` | 未经 resolution 不得标成 reconciled | 仅 operator 可处理 |
- Acceptance Criteria:
  - AC-1: 专题必须明确本轮目标态是 `one-way OC -> LetAI Run OpenAPI quota`，并把 `OC <- quota/token_key` 兑回、自动提现、AMM、公开交易所全部列为 non-goal。
  - AC-2: bridge-service 必须按 LetAI OpenAPI 真实对象模型定义 `platform user -> project -> token_key -> topup -> query`，不得继续把“generic HTTP credit adapter”写成当前真值。
  - AC-3: 专题必须冻结“parent channel/platform key 由 operator 提供；每个用户的 project 和 token_key 动态创建”这一业务前提。
  - <a id="tbridge-ac-4"></a> AC-4: `bridge_ledger` 必须补齐 `platform_user_id/platform_project_id/token_key/external_order_id/query snapshots` 等最小审计字段，覆盖 user/project/topup/query 全链路。
  - AC-5: 专题必须明确 `token_key` 只用于模型实际调用，平台 Key 只用于开放管理接口，不得混用。
  - <a id="tbridge-ac-6"></a> AC-6: topup 成功判定必须至少包含一次验证查询回写；不得把单次 HTTP `2xx` 直接当成 reconciled。
  - AC-7: 对外文案必须继续明确这是 `limited preview operator-managed service-credit bridge`；“公开兑换所”“浏览器热钱包充值”“自动提现回 OC”仍在 denylist。
  - AC-8: 专题必须给出 LetAI OpenAPI-specific implementation 任务拆解、验证层级与 owner role，而不是停留在概念讨论。
  - AC-9: 收款账号策略必须明确区分三层边界：`provider-level receiving account pool`、`route-level beneficiary/pricing truth`、`optional transfer message/memo token`；不得把“账号池”误写成“无需 route 真值”。
  - AC-10: 在 bridge 还不能从链上可靠解析唯一归因 token 前，收款账号池不得让多个活跃 route 共享同一个 `deposit_account_id`；若未来启用池化复用，必须把“唯一归因 token 已 mandatory 且 bridge 可验证解析”写成显式前置条件。
- Non-Goals:
  - 不实现 `OC <- LetAI quota/token_key` 自动兑回。
  - 不实现链上 AMM、order book、公开做市或价格发现。
  - 不把 bridge-service 做进 hosted public player plane 或浏览器 HTML/bootstrap。
  - 不在本轮实现 production-grade keystore、HSM/KMS、法币支付、税务或合规结算。
  - 不把 `token_key` 直接暴露为浏览器自动下发的公共凭证分发系统。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: bridge-service 需要对接 LetAI Run OpenAPI：平台用户 upsert、项目创建/Token 返回、用户额度概览、用户日志、充值接口与项目 Token 汇总接口。模型实际调用接口仍是 `POST /v1/chat/completions` + `Authorization: Bearer <token_key>`，但该调用不在本轮 bridge-service 直接执行。
- Evaluation Strategy: 以 topup 正确率、重复 topup 为 0、user/project/token provisioning 成功率、query verification 覆盖率、manual review 比例和 claim 边界正确率评估。

## 4. Technical Specifications
- Architecture Overview: 架构采用独立 bridge-service。它包含 `binding API`、`deposit route allocator`、`chain watcher`、`bridge_ledger store`、`pricing engine`、`LetAI OpenAPI adapter` 与 `reconciliation worker`。`oasis7` 侧继续负责链上资产真值；LetAI Run 侧继续负责用户、项目、`token_key` 与额度消费；bridge-service 只做“已确认 `OC` 入账 -> LetAI OpenAPI user/project/token/topup/query”映射。
- Integration Points:
  - `crates/oasis7_runtime/src/main_token.rs`
  - `crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/transfer_submit_api.rs`
  - `crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/server.rs`
  - `doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md`
  - `doc/p2p/blockchain/p2p-mainnet-security-governance-readiness.prd.md`
  - `doc/p2p/blockchain/p2p-mainnet-security-governance-readiness.prd.md`
  - LetAI Run OpenAPI `Baseurl: https://api.letai.run`
  - `POST /api/platform/open/users/upsert`
  - `POST /api/platform/open/users/:platform_user_id/topups`
  - “创建或获取项目并返回 Token”接口
  - “查询用户额度概览”接口
  - “查询用户消耗明细”接口
  - “查询项目 Token 汇总”接口
- Edge Cases & Error Handling:
  - 在没有链上可验证归因 token 前，不允许多个活跃 route 复用同一收款账号池成员；否则 watcher 只按 `deposit_account_id` 扫描时会把同一笔链上转账错误归给多个 route。
  - 若未来启用“同一收款账号承载多个活跃 route”，则必须先满足两条前置条件：链上转账支持 mandatory `message/memo` 或等价字段；bridge watcher/explorer 已把该 token 作为唯一归因输入参与解析和去重。
  - 若 bridge user 未先完成绑定，就直接向 deposit route 充值，系统必须进入 `manual_review`，不得猜测用户。
  - 若 `users/upsert` 未返回稳定 `platform_user_id`，后续 project/topup/query 必须阻断并进入 `manual_review`。
  - 若项目创建成功但未返回 `token_key`，必须标记为 `project_token_missing`，不得继续 topup。
  - 若同一平台用户下已有多个项目，bridge-service 必须始终只认自己持久化绑定的 `platform_project_id/token_key`，不得“取最新 project”做猜测。
  - 若 topup 接口超时或 5xx，bridge-service 必须保留同一 `external_order_id` 重试，不得重新生成新订单。
  - 若 topup 返回 2xx 但查询额度/日志看不到对应变更，系统必须进入 `manual_review`，不得标记 reconciled。
  - 若 underpay、overpay、expired route deposit、duplicate route deposit、`topup_plan_id` 自动折算、query mismatch 出现，默认进入 `manual_review`。
  - 若 `token_key` 状态显示异常或失效，不得自动替用户刷新为新 project，除非文档明确允许同一 external project id 幂等刷新。
- Non-Functional Requirements:
  - NFR-TBRIDGE-1: `bridge_ledger` 审计字段完整率 `100%`，至少包含 `bridge_deposit_id/chain_tx_id/platform_user_id/platform_project_id/token_key/external_order_id/pricing_version/quota/state/operator_note`。
  - NFR-TBRIDGE-2: 对同一链上入账，LetAI topup 次数必须恒为 `<= 1` 个业务 order；重复 watcher 事件、worker 重试或 query retry 不得导致双发。
  - NFR-TBRIDGE-3: bridge-service 必须支持配置化确认窗口，并把该值写入审计输出；未达到确认阈值的入账不得触发 LetAI OpenAPI。
  - NFR-TBRIDGE-4: 同一 bridge user 的 LetAI project/token 映射必须持久化；服务重启后不能依赖重新猜测 project。
  - NFR-TBRIDGE-5: bridge-service 的 platform key / parent channel credential 只允许留在受控服务端环境，不得进入 HTML/bootstrap/public JS。
  - NFR-TBRIDGE-6: `quota` 单位必须按 LetAI 文档真值保存；若对外要显示 USD，仅允许以 `quota / 500000` 作为审计换算展示，不能回写为业务真值。
  - NFR-TBRIDGE-7: implementation 前后都不得在 repo 内写死平台外部账号或渠道私密配置；parent channel 只通过 operator CLI/config 注入。
  - NFR-TBRIDGE-8: 收款账号池必须由 operator 控制真实公私钥或等价 custody 权限；`deposit_account_id` 不能长期只停留在逻辑命名而没有真实收款控制面。
  - NFR-TBRIDGE-9: 若后续引入链上 `message/memo`，bridge 默认只接受短 `deposit_token/route token`，不把完整加密业务载荷直接作为链上对账真值。
- Security & Privacy: bridge-service 持有 LetAI 平台级 Key、链侧 watcher 配置和本地审计状态，必须与 runtime、公网页面和浏览器完全隔离。`token_key` 属于用户项目凭证，虽然 bridge 需要持久化它，但不得经 public API 明文向未授权调用方广播。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP: operator-managed `one-way OC -> LetAI Run quota` bridge，支持用户绑定、动态项目/`token_key` 初始化、确认窗口、`bridge_ledger`、OpenAPI topup 与 query verification。
  - v1.1: 增加 operator dashboard、`token_key` 生命周期管理、pricing version 管理、批量 reconciliation 报表与 richer manual-review action。
  - v2.0: 在 custody、wallet、public claims 和 LetAI OpenAPI 合同都更稳定后，再评估更强自助化能力；仍不默认承诺双向提现。
- Technical Risks:
  - 风险-1: LetAI OpenAPI 文档当前可见内容较多依赖飞书页面展示，若项目/Token 接口字段后续变更，adapter 需要同步升级。
  - 风险-2: 若没有持久化 `platform_project_id/token_key`，服务重启后容易把用户已有多个项目的场景映射错。
  - 风险-3: 若只看 topup 2xx 不做查询验证，容易把上游暂时成功/幂等重放误判成最终成功。
  - 风险-4: `token_key` 属于可直接调用模型的凭证，若暴露到错误日志或 public API，会放大安全风险。
  - 风险-5: 若对外把该能力称为“代币兑换所”或“用户拿到 token_key 就等于自由资产”，会引入不必要的合规和信任预期压力。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-P2P-TBRIDGE-001 | BRIDGE-0/1 | `test_tier_required` | 专题 PRD/design/project 建档、模块入口回写、one-way bridge claim 与 non-goal 冻结 | 资产口径、对外 claim 与 owner 边界 |
| PRD-P2P-TBRIDGE-002 | BRIDGE-2/3 | `test_tier_required` | `bridge_ledger` 状态机、LetAI user/project/token/topup/query 链路、`external_order_id` 幂等与 anomaly 流程评审 | 账本正确性、重复 topup 风险与 operator 对账 |
| PRD-P2P-TBRIDGE-003 | BRIDGE-3/4 | `test_tier_required` | `oasis7_newapi_bridge_service` 定向测试：首次 user/project create、token_key 持久化、topup retry、query verification、manual review fallback | LetAI OpenAPI 合同与 bridge 实现真值 |
| PRD-P2P-TBRIDGE-004 | BRIDGE-4/5 | `test_tier_required` | underpay/overpay/project drift/token_key missing/query mismatch 手工审查路径与 runbook | liveops / QA 异常收口与审计证据 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-TBRIDGE-001 | 采用 `one-way OC -> LetAI Run OpenAPI quota` | 继续把 bridge 写成 generic credit POST | LetAI Run 已有明确 user/project/token/topup/query 模型；generic adapter 无法表达 `token_key` 真值。 |
| DEC-TBRIDGE-002 | 平台 Key 只用于开放管理接口，项目 `token_key` 只用于模型调用 | 混用平台 Key 和项目 `token_key` | 飞书文档已明确两者职责不同，混用会把权限和审计边界打乱。 |
| DEC-TBRIDGE-003 | parent channel 由 operator 提供，每个 bridge user 动态创建/复用独立 project | 一个父级渠道下所有用户共享同一个项目/`token_key` | 文档已明确“一项目一 token_key”，同用户多项目也要各自独立；共享项目会破坏用户隔离和审计。 |
| DEC-TBRIDGE-004 | topup 成功必须附带 query verification snapshot | 仅按 topup 接口 2xx 判定成功 | 充值幂等和异步一致性要求决定了 2xx 不能代表最终额度已生效。 |
| DEC-TBRIDGE-005 | 继续冻结 one-way service-credit bridge + denylist claim | 对外包装成“用户买到模型 token，可自由兑回” | 当前能力只覆盖受控服务额度充值，不是公开交易所产品。 |
| DEC-TBRIDGE-006 | 收款面默认采用“每个 newapi 接入方一组受控 OC 收款账号 + 每次充值保留独立 route 真值” | 全平台单账号，或每笔充值现生成新地址 | 接入方级账号池能先提供资金控制与轮换边界，同时避免把订单级唯一性完全压到链地址生成上。 |
| DEC-TBRIDGE-007 | 若链上转账后续支持 `message/memo`，默认只承载短 `deposit_token` 作为辅助归因 | 直接把整包加密对账信息塞进 `message` | 短 token 更稳，更适合长度、兼容和重放控制；完整业务真值仍应保留在 bridge state / ledger。 |

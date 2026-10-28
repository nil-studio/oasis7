# oasis7 主链 Token 到 LetAI Run OpenAPI 额度桥接设计

- 对应需求文档: `doc/p2p/token/mainchain-token-newapi-quota-bridge.prd.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

审计轮次: 2

**开发 hold：** 本设计对应的 LetAI 专属后续开发目前暂停，不作为主要开发路径，范围与恢复条件以[主责开发状态](mainchain-token-newapi-quota-bridge.prd.md#letai-development-hold)为准。下述目标合同与安全/资金边界保留，不构成当前实施排期；包括安全错误投影在内的待实现项仍不得写成已修复。

## 1. 设计目标
- 把 `OC` 到 LetAI Run 调用额度的路径定义成一条受控、独立部署、可对账的 OpenAPI bridge。
- 让 bridge-service 直接表达 LetAI 的真实对象模型：`platform user -> project -> token_key -> topup -> query verification`。
- 保持 `oasis7` runtime 只提供链上资产真值，不把 LetAI 用户/项目/Token 逻辑塞回链上或浏览器。

## 2. 架构拆分
- `bridge-portal`
  - 负责用户绑定、查看 deposit route、查看 project/token/topup 状态。
  - 不持有链上私钥；只展示 route / beneficiary / audit status。
  - 默认不明文展示或导出 `token_key`。
- `bridge-core`
  - 负责 route 分配、pricing 解析、`bridge_ledger` 状态机、LetAI 资源映射、幂等重试与 operator action。
- `chain-watcher`
  - 负责从 `oasis7` 链侧观察 deposit truth。
  - 只读，不写链。
- `receiving-account-manager`
  - 负责维护每个 newapi 接入方的 OC 收款账号池、账号状态、轮换和 route 分配视图。
  - 当前推荐按接入方维护小池化账号组，而不是每笔充值现生成新地址。
- `letai-openapi-adapter`
  - 负责 `users/upsert`、项目创建/Token 返回、用户 topup、额度概览、日志查询与项目 Token 汇总。
  - 必须支持幂等键、query verification 与审计回写。
- `bridge-db`
  - 保存 bridge binding、route、pricing、`bridge_ledger`、LetAI user/project/token 映射与 operator 审计日志。

### 2.1 需求承接与分配表

| 上游要求 | 承接与分配 | 本设计条款 | 主责与消费方 | 边界 |
| --- | --- | --- | --- | --- |
| [PRD-P2P-TBRIDGE-002：稳定身份](mainchain-token-newapi-quota-bridge.prd.md#tbridge-002) | 在外部副作用前持久保存恢复身份，未知结果沿用原身份 | [首次绑定恢复](#bridge-identity-recovery) | runtime_engineer；bridge-core/store/provider | 保留身份不等于映射就绪，不假设跨服务事务 |
| [PRD-P2P-TBRIDGE-002：完整发现与审计](mainchain-token-newapi-quota-bridge.prd.md#tbridge-002) | 完整消费相关分页，去重并区分未完成扫描 | [扫描完整性](#bridge-scan-completeness) | runtime_engineer；chain-watcher/Explorer | 不用头页计数证明完整，不启用账号池并发复用 |
| [PRD-P2P-TBRIDGE-002：唯一订单与幂等对账](mainchain-token-newapi-quota-bridge.prd.md#tbridge-002) | 串行提交或 fencing 保护文件、内存和人工终态 | [有效处理权](#bridge-reconcile-ownership) | runtime_engineer；reconcile/store/operator | 并发处理权不代替跨重启恢复 |
| [PRD-P2P-TBRIDGE-004：异常诊断](mainchain-token-newapi-quota-bridge.prd.md#tbridge-004) | 全部分支仅输出固定本地错误与受控元数据 | [安全错误投影](#letai-safe-error-projection) | adapter owner；local/newapi bridge | 不改变资金状态、重试或 receipt，不清理历史错误 |
| [PRD-P2P-TBRIDGE-004：异常与验证快照](mainchain-token-newapi-quota-bridge.prd.md#tbridge-004) | 持久成功事实与尝试分离，receipt 驱动 verification-only | [持久业务阶段](#bridge-recovery-state) | runtime_engineer；ledger/reconcile/provider | 未知外部结果按原订单核对，不无条件跳过 topup |

## 3. 最小数据模型
- `bridge_user_binding`
  - `provider_id`
  - `bridge_user_id`
  - `newapi_user_ref`
  - `oasis_sender_account_id`
  - `letai_external_user_id`
  - `platform_user_id`
  - `letai_external_user_name`
  - `email`
  - `metadata`
  - `status`
  - `created_at`
  - `updated_at`
- `letai_project_binding`
  - `bridge_user_id`
  - `letai_external_project_id`
  - `platform_project_id`
  - `project_name`
  - `token_key`
  - `token_status`
  - `created_at`
  - `updated_at`
- `deposit_route`
  - `route_id`
  - `provider_id`
  - `bridge_user_id`
  - `beneficiary_ref`
  - `deposit_account_id`
  - `deposit_token`
  - `route_type`
  - `pricing_version`
  - `topup_plan_id`
  - `expires_at`
  - `status`
- `provider_receiving_account`
  - `provider_id`
  - `account_id`
  - `account_label`
  - `pool_slot`
  - `custody_mode`
  - `status`
  - `rotation_epoch`
  - `last_assigned_at`
  - `created_at`
  - `updated_at`
- `bridge_ledger`
  - `bridge_deposit_id`
  - `route_id`
  - `bridge_user_id`
  - `chain_tx_id`
  - `chain_action_id`
  - `amount_oc`
  - `expected_amount_oc`
  - `pricing_version`
  - `credit_units`
  - `bonus_units`
  - `total_credit_units`
  - `platform_user_id`
  - `platform_project_id`
  - `token_key`
  - `external_order_id`
  - `quota`
  - `amount_audit`
  - `currency`
  - `topup_receipt`
  - `user_snapshot`
  - `project_snapshot`
  - `topup_log_snapshot`
  - `idempotency_key`
  - `state`
  - `review_reason`
  - `operator_note`
  - `credit_attempt_count`
  - `last_error_code`
  - `last_error`
  - `observed_at`
  - `updated_at`
- `pricing_schedule`
  - `pricing_version`
  - `oc_amount`
  - `quota_units`
  - `bonus_quota_units`
  - `status`
  - 示例配置真值入口: `scripts/newapi-bridge-service/pricing-rules.example.env`

## 4. 状态机
- `bridge_user_binding`
  - `active_local_only -> user_ready`
  - `user_ready -> project_ready`
  - `project_ready -> project_token_ready`
  - 任意状态可进入 `manual_review`
- `deposit_route`
  - `draft -> issued -> settled`
  - `draft -> issued -> expired`
  - `issued -> disabled`
- `provider_receiving_account`
  - `provisioning -> active`
  - `active -> draining`
  - `draining -> retired`
  - `active -> disabled`
- `bridge_ledger`
  - `detected -> pending_confirmations -> confirmed`
  - `confirmed -> provisioning_user -> provisioning_project -> crediting -> credited -> verifying -> reconciled`
  - `confirmed -> provisioning_user -> manual_review`
  - `confirmed -> provisioning_project -> manual_review`
  - `crediting -> failed -> crediting`：仅用于没有持久成功 topup 事实的记录；外部结果未知时保留原订单并先核对，不能推断未入账。
  - `credited -> verifying -> manual_review`
  - `manual_review -> resolved -> reconciled`
  - `manual_review -> closed`

<a id="bridge-recovery-state"></a>
### 4.1 持久业务阶段与处理尝试分离

主责：runtime_engineer；承接 [PRD-P2P-TBRIDGE-004](mainchain-token-newapi-quota-bridge.prd.md#tbridge-004)，相关条款 [AC-4](mainchain-token-newapi-quota-bridge.prd.md#tbridge-ac-4)、[查询成功判定 AC-6](mainchain-token-newapi-quota-bridge.prd.md#tbridge-ac-6)。产品适用性：N/A；reason=本项仅细化 operator-managed 单向服务额度桥的内部恢复，不定义玩家或 Agent 模型可用性承诺；scope=成功 topup 事实与后续查询恢复。不覆盖公开兑换、custody、账号池复用或供给。

本节细化配对 PRD 的 PRD-P2P-TBRIDGE-002/004、Flow-TBRIDGE-005 和 AC-4/6，是目标恢复合同，不表示四项实现已交付。业务阶段记录已经成立的事实；一次 reconcile 的尝试、失败和临时处理权不能覆盖这些事实。

| 已有持久事实 | 允许的后续处理 | 禁止的退回 |
| --- | --- | --- |
| 已分配本地 binding/project 身份，远端结果未知 | 用原外部身份查询或幂等续跑；映射保持未就绪 | 删除后分配新 `bridge_user_id`/`external_project_id` |
| topup 尚未确认成功 | 保留原订单，核对未知结果或按原幂等合同重试 | 换订单、默认成功或无条件跳过 topup |
| 已持久记录成功 topup/receipt | 仅恢复验证所需上下文并继续 query verification | 因 user/project 查询失败退回未入账阶段 |
| 已 reconciled 或处于已裁定人工终态 | 返回原结果；人工动作遵守既有权限和状态转换 | 被过期的成功/失败回调或较旧文件快照覆盖 |

`credited` 只说明已有成功入账事实；`reconciled` 还要求适用查询证据。当前查询暂不可用时应保留“已入账、待核对”，不误报为未充值或已核对完成。状态重载必须保留这一区别；诊断、快照和最后错误不是可以覆盖成功 receipt 的更强真值。

## 5. 关键接口契约
- `POST /v1/bridge/bind`
  - 输入: `provider_id`, `newapi_user_ref`, `oasis_sender_account_id`, optional `external_user_name`, `email`, `metadata`
  - 输出: `bridge_user_id`, local binding status
  - 约束: 只建立受控 bridge binding，不隐式 topup；`provider_id` 是 route 选择收款账号池的上游真值。已建立本地身份不等于项目/Token 已就绪，provider 只能消费已验证就绪的映射。
- `POST /v1/bridge/deposit-route`
  - 输入: `bridge_user_id`, `pricing_version` 或 `topup_plan_id`
  - 当前代码输出: `route_id`, `deposit_account_id`, `deposit_token`, `expires_at`
  - 目标态扩展输出: `route_id`, `provider_id`, `deposit_account_id`, `deposit_token`, `expires_at`
  - 约束: 每个活跃 route 必须可唯一映射 beneficiary；目标态里 `deposit_account_id` 默认从 `bridge_user_binding.provider_id` 对应的接入方收款账号池选择，不要求 route 自己拥有独立链地址；但在 bridge 尚不能验证链上唯一归因 token 前，同一个 `deposit_account_id` 同一时刻只允许分配给一个活跃 route。
- `POST /v1/bridge/reconcile`
  - 输入: 空 body；由 operator 或后台 interval 触发一次 `scan routes -> observe deposits -> ensure user/project/token -> topup -> verify`，具体恢复起点由持久业务阶段决定，不对所有记录无条件重放整条链。
  - 输出: `latest_committed_height`、`observed_new_deposit_count`、`reconciled_credit_count`、`manual_review_count`
  - 约束: 若 `chain-base-url`、LetAI base URL 或 platform key 未配置，返回 operator-facing error。扫描完整性与已观测条目的处理结果分别表达，已有计数不能独自证明本轮全部历史已扫描；兼容输出如何扩展由实现明确版本/消费者适配。
- `POST /v1/bridge/operator/review/{bridge_deposit_id}`
  - 输入: `resolution`, `operator_note`
  - 输出: `next_state`
  - 约束: 当前最小实现仍只支持 `mark_resolved|close`。
- `LetAI OpenAPI`
  - `POST /api/platform/open/users/upsert`
  - `POST /api/platform/open/users/:platform_user_id/topups`
  - `GET /api/platform/open/users/:platform_user_id/...`
  - “创建或获取项目并返回 Token”
  - “查询项目 Token 汇总”
  - 约束:
    - `Authorization: Bearer <platform-key>`
    - `token_key` 只用于实际模型调用，不用于管理接口
    - `external_user_id` / `external_project_id` / `external_order_id` 必须稳定幂等

<a id="letai-safe-error-projection"></a>
### 5.1 LetAI adapter 的安全错误投影

Trace ID: O7-REQ-7ed340db-8f3f-4543-9f6d-d00184875607

本节是 adapter 错误出口的唯一工程合同，主责 `oasis7_letai_credit_adapter`；local bridge、newapi bridge 只消费安全结果，不各自猜测上游文本是否可信。细化 [PRD-P2P-TBRIDGE-004](mainchain-token-newapi-quota-bridge.prd.md#tbridge-004) 的异常诊断；产品适用性：N/A；reason=只约束受控额度桥的内部诊断数据，不新增玩家金融功能；scope=错误投影，不改变资金、价格或额度语义。目标要求由 [#5102](https://github.com/nil-studio/oasis7/issues/5102) 跟踪，不表示已实现或发生真实泄漏。

- 所有失败出口均只返回本地固定 code、由该 code 选择的固定说明及有界 allowlist 元数据。保留现有 `letai_request_failed`、`letai_response_read_failed`、`letai_response_not_success`、`letai_response_decode_failed`、`letai_response_not_ok` 与字段缺失分类；业务分类和可重试性不从展示字符串反推。
- 元数据仅允许本地操作枚举、HTTP 数字状态和有界数字计数；不保留供应商任意 `error_code`。将来确需供应商分类时，必须显式映射有限 allowlist 到本地枚举，未知值归固定 unknown，不携带原值。网络库/JSON decoder 的 `Display`、URL、header、响应 body 或嵌套错误文本同样不是安全说明，不能直接复制。
- HTTP 成功但 `ok/success=false` 与非 2xx、传输/读取/解码/字段校验失败使用同一出口约束。`message/error/msg/detail/error_code` 均不进入 `LetaiAdapterError` 的诊断字段、Debug/Display、stderr/trace 或新持久化 `last_error`；先截断再传播不算脱敏。原始响应不能以 debug 附件或旁路日志保留。
- 成功响应仍按既有结构化摘要/receipt 合同消费；本节不删除有效业务字段，也不把自由文本诊断提升为 receipt。`external_order_id`、幂等身份、reconcile 状态、人工复核、retry 分类与实际 topup 调用次数保持不变；错误投影不得新增调用或把 unknown 改为未入账。已有历史错误按既有隐私/保留合同处理，不自动改写、清空或重放账项。

静态基线 `b9bc4b2a6df858e27f94754d227b0fdc23ae3639`：[adapter](../../../crates/oasis7_letai_credit_adapter/src/lib.rs) 的 semantic-error 分支仍拼接上游自由文本；[local diagnostic](../../../crates/oasis7_provider_local_bridge/src/bin/oasis7_provider_local_bridge/letai_direct_topup.rs) 仅截断；[reconcile](../../../crates/oasis7_newapi_bridge_service/src/bin/oasis7_newapi_bridge_service/service_letai.rs) 仍消费 message。现有非 2xx redaction 不能证明全部出口安全。

验证要求：离线 mock 在上述五个上游字段逐个放入短敏感 canary，并覆盖 2xx semantic failure、非 2xx、坏 JSON、读取/传输失败与缺字段；断言 adapter、stderr/trace、新写入和重载 `last_error` 都无 canary，固定 code 与安全说明仍可区分。保留正常成功摘要、原订单/receipt、重试/人工复核及调用计数的前后对照。新增回归尚待实现执行，静态数据流不是泄漏或修复完成证据。

## 6. 编排顺序
1. 当前代码生成 route 并返回 `deposit_account_id` 与短 `deposit_token`；目标态扩展为 route allocator 按 `bridge_user_binding.provider_id` 选择一个 active 收款账号池成员。
2. 新 route 的 `deposit_token` 已由 explorer `message/memo` 透传并在 bridge reconcile 强校验；legacy route 缺少 token 时仍保留旧行为。账号池并发混单仍需后续 custody/allocator 任务明确唯一归因和去重策略。
3. watcher 观察 route 对应链上 confirmed deposit，并保留本轮完整或未完成的扫描结果。
4. pricing engine 把 `OC` 金额折算成 LetAI `quota`。
5. `users/upsert` 确保 `platform_user_id` 存在，并回写 binding；恢复使用原身份。
6. `ensure project + token_key` 确保 bridge user 绑定独立 project，回写 `platform_project_id/token_key`；不确定结果保留原 project 身份，未就绪映射不能交给 provider。
7. 仅对没有持久成功 topup 事实的记录进入入账处理，始终使用原 `external_order_id`；结果未知先核对，不分配新订单。已有成功 receipt 的记录跳过此步。
8. 调用用户概览 / 日志 / 项目汇总做验证；失败保留已有成功 topup 事实。
9. 在当前有效处理权及最新业务阶段仍允许时回写 ledger snapshots，标记 `reconciled` 或 `manual_review`；过期尝试不能覆盖终态或重复增加成功计数。

<a id="bridge-identity-recovery"></a>
### 6.1 首次绑定的不确定结果

主责：runtime_engineer；承接 [PRD-P2P-TBRIDGE-002](mainchain-token-newapi-quota-bridge.prd.md#tbridge-002)，相关条款 [SC-4](mainchain-token-newapi-quota-bridge.prd.md#tbridge-sc-4)。产品适用性：N/A；reason=本项仅细化 operator-managed 单向服务额度桥的内部恢复，不定义玩家或 Agent 模型可用性承诺；scope=首次 binding/project 不确定结果恢复。不覆盖公开兑换、custody、账号池复用或供给。

在第一次可能产生外部副作用的调用之前，持久保存恢复所需的本地 binding/project 身份和由其派生的稳定 external ID。持久保存失败时不能先调用远端；远端创建成功但回复丢失、无法解码或本地结果落盘失败时，重启/重试沿用同一身份进行查询或幂等 upsert。

只有能够确定未产生外部副作用、且没有其他业务引用的失败，才能按显式清理规则删除本地预留记录；不能把所有异常都作为回滚身份的依据。保留身份不等于标为 ready，项目/Token 准备完成并通过原有检查前 provider 仍被阻塞。本节不创建跨服务事务或宣称远端无条件 exactly-once。

<a id="bridge-scan-completeness"></a>
### 6.2 watcher 分页和扫描完整性

主责：runtime_engineer；承接 [PRD-P2P-TBRIDGE-002](mainchain-token-newapi-quota-bridge.prd.md#tbridge-002)，相关条款 [SC-2](mainchain-token-newapi-quota-bridge.prd.md#tbridge-sc-2)。产品适用性：N/A；reason=本项仅细化 operator-managed 单向服务额度桥的内部恢复，不定义玩家或 Agent 模型可用性承诺；scope=watcher 相关分页发现与去重。不覆盖公开兑换、custody、账号池复用或供给。

消费 Explorer 返回的全部相关分页，而不是将第一批 200 条当成完整历史；账户匹配包含转入和转出，必须在读取后按现有合同归因。每页结果按原交易/action 身份去重，重复扫描不重复建账。

后页错误、重复/倒退 cursor、达到有界扫描预算但尚有后页时，返回失败或明确未完成，保留合法已观测条目，不能发布“无遗漏”的扫描结论。扫描预算限制一次工作量，不允许永久丢弃尚未扫描的历史。

优先使用有稳定顺序及边界的分页合同；若当前 Explorer 只提供会随头部新增而漂移的 offset，必须使用可验证的重叠扫描/边界重核并按原身份去重，不能把裸 offset 作为永久完成水位。持续新增时无法证明某次扫描完整，应如实保留未完成并继续有界处理。该设计限于 watcher，不以此启用收款账号池并发复用。

<a id="bridge-reconcile-ownership"></a>
### 6.3 并发处理权与跨重启阶段

主责：runtime_engineer；承接 [PRD-P2P-TBRIDGE-002](mainchain-token-newapi-quota-bridge.prd.md#tbridge-002)，相关条款 [Flow-TBRIDGE-005](mainchain-token-newapi-quota-bridge.prd.md#tbridge-flow-005)。产品适用性：N/A；reason=本项仅细化 operator-managed 单向服务额度桥的内部恢复，不定义玩家或 Agent 模型可用性承诺；scope=同一稳定订单的并发提交及人工终态保护。不覆盖公开兑换、custody、账号池复用或供给。

对同一稳定订单串行化业务提交，或在存储提交边界以有效的 attempt generation/版本条件拒绝过期结果。具体互斥/fencing 实现可替换，但必须同时保护持久文件和内存投影；单纯在请求入口加锁而允许旧回调覆盖文件不满足合同。

网络等待期间不能持有导致全服务永久阻塞的不可恢复处理权。异常退出后处理权能被安全释放或恢复，旧尝试恢复后仍无法写入新阶段。单进程互斥的证明不能外推为跨进程协调；部署支持的并发范围必须明确。

A 成功提交 Reconciled 后 B 的延迟失败只能成为该过期尝试的诊断，不能回退业务状态、增加失败计数或 `credit_attempt_count`；并发双成功只计一次业务成功。人工裁定通过同一业务状态保护，后台回调不能改写人工终态。

互斥/fencing 不能代替跨重启恢复：持久成功 topup 标记、receipt 和原订单必须在后续 user/project/query 失败、再次退出和状态保存失败时保持可恢复。重载后从 verification-only 继续；若外部 topup 成功但成功标记尚未持久化，只能以原订单核对未知结果，而不能声称已知道未入账。

## 7. 收款策略
- 当前推荐:
  - 每个 newapi 接入方维护一组受控 OC 收款账号。
  - 初始规模可用小池化，例如 5 个账号。
  - 每次充值仍生成独立 route，并从账号池选择当前可用账号，而不是现生成新地址。
  - 在没有 mandatory `deposit_token` 前，账号池成员不得同时服务多个活跃 route；小池化的含义是“一个接入方有多个候选账号可轮转”，不是“一个账号同时混多笔活跃订单”。
  - `provider_id` 必须在 bind 阶段就固定到 `bridge_user_binding`，避免 route 分配时临时猜测“属于哪个接入方”。
- 归因顺序:
  1. `route_id` / `provider_id`
  2. `deposit_account_id`
  3. `expires_at` 有效期
  4. `pricing_version` / `topup_plan_id`
  5. `from_account_id`
  6. `deposit_token`（新 route 必填；legacy route 可为空）
- 为什么不默认全平台单账号:
  - 接入方之间缺少资金隔离。
  - operator 轮换和风控粒度太粗。
  - manual review 时容易把接入方账混在一起。
- 为什么不默认每单现生成新地址:
  - 需要完整密钥派生、托管和归集系统。
  - 当前 bridge 更缺 route/ledger/custody 的稳定闭环，而不是地址数量。
- `message/memo` 边界:
  - 链上转账 `message/memo` 默认只放短 `deposit_token`。
  - `deposit_token` 只做辅助归因，不替代 bridge state / ledger。
  - 新 route 观测到缺失或不一致的 memo 必须进入 `manual_review`，不能正常 credit。
  - 不建议把完整加密对账信息直接塞进链上消息体。
  - 只有当 `deposit_token` 变成 mandatory 且 watcher/explorer 已把它纳入唯一归因和去重输入后，才允许同一个收款账号同时承载多个活跃 route。

## 8. 推荐实现顺序
1. 先扩展 `model.rs` / `store.rs`，补齐 `provider_id`、收款账号池、LetAI user/project/token/topup/query 字段。
2. 替换 generic `credit_adapter.rs` 为 LetAI OpenAPI adapter，支持多接口。
3. 重写 `service.rs` route 分配与 reconcile 编排，从 `Confirmed` 推进到 `Reconciled`。
4. 扩展 `deposit_token` 验证后的账号池 allocator 与 provider receiving account 并发归因策略。
5. 重写测试桩，覆盖账号池复用、user/project/token/topup/query 的 happy path、retry 和 manual review。
6. 最后再补 richer operator dashboard / portal。

## 9. 风险门禁
- 没有 `platform_user_id` 时，不允许继续 project/topup。
- 没有 `platform_project_id/token_key` 时，不允许继续 topup。
- topup 没有 query verification snapshot 时，不允许标记 reconciled。
- `external_order_id` 不稳定时，不允许自动重试。
- 当前 exact-match pricing rule 之外的充值形态一律进入 `manual_review`。
- `token_key` 若需要进入浏览器或 public bootstrap，提案直接失败。
- 收款账号池若没有真实 custody 能力，只靠逻辑命名，提案直接失败。
- 若多个活跃 route 共用同一收款账号但缺少足够归因字段，提案直接失败。

## 10. 生命周期验收与交付边界

| 上游要求/独立责任 | 故障输入 | 必须观察的结果 |
| --- | --- | --- |
| PRD-P2P-TBRIDGE-002/004：稳定身份 | 远端已创建但回复丢失；成功回复后本地保存失败；重启重试 | 原 `bridge_user_id/external_project_id` 不变，mock 远端仅一个项目；未就绪映射不可消费 |
| watcher 完整发现 | 201/401 条、转出占头页、目标在后页、扫描中新增头部交易 | 相关转入进入正常或人工流程；重复扫描不重复建账；异常 cursor/后页失败明确未完成 |
| reconcile 有效处理权 | barrier 固定 A 成功后 B 延迟失败；并发双成功；处理者异常 | 文件和重载均保留终态，成功只计一次，过期尝试不改业务计数；处理权可恢复 |
| AC-4/6：入账与核对分阶段 | Credited 后恢复 user/project 临时失败、下一轮成功；期间再次重启/保存失败 | receipt/订单与成功事实保留，topup 总次数仍为一；真正未入账记录仍按原订单处理 |

四项分别验收；并发测试不能代替重启测试，外部幂等不能代替本地身份稳定，查询成功不能证明分页完整。可使用有真实分页和持久故障语义的离线 mock，不调用真实付费服务、不改账号、汇率、额度、凭据或链上资金。现有实现、mock 验收和真实 operator-managed 部署结果分别记录；本设计不声称已发生重复扣费、资金损失或凭据泄漏，也不以文档合入关闭实现待办。

## 11. 验证

### 11.1 验证映射表

| 上游要求 | 本设计条款 | 机制与场景 | 验证来源 | 执行证据 | 未覆盖与状态 |
| --- | --- | --- | --- | --- | --- |
| [PRD-P2P-TBRIDGE-002：稳定身份](mainchain-token-newapi-quota-bridge.prd.md#tbridge-002) | [首次绑定恢复](#bridge-identity-recovery) | §10 的丢回复、保存失败与重启；mock 远端项目数及未就绪消费分别断言 | [测试手册](../../../testing-manual.md)；下述 Bridge binary suite 的身份故障用例 | source、外部 ID、项目数、本地重载及命令退出码 | planned；现有创建前失败用例不能代签未知副作用 |
| [PRD-P2P-TBRIDGE-002：完整发现与审计](mainchain-token-newapi-quota-bridge.prd.md#tbridge-002) | [扫描完整性](#bridge-scan-completeness) | §10 的 201/401 条、后页、转出占头页、新增头部、坏 cursor 与后页错误 | [测试手册](../../../testing-manual.md)；下述 Bridge binary suite 的真实分页 mock | source、请求 cursor、完整/未完成结果、去重账项和退出码 | planned；返回所有交易的单页 mock 不能代签分页 |
| [PRD-P2P-TBRIDGE-002：唯一订单与幂等对账](mainchain-token-newapi-quota-bridge.prd.md#tbridge-002) | [有效处理权](#bridge-reconcile-ownership) | §10 的 barrier 固定双尝试、延迟失败、双成功与处理者退出 | [测试手册](../../../testing-manual.md)；下述 Bridge binary suite 的并发故障用例 | source、受控时序、内存/文件/重载终态、计数和退出码 | planned；部署支持的并发范围单独说明 |
| [PRD-P2P-TBRIDGE-004：异常诊断](mainchain-token-newapi-quota-bridge.prd.md#tbridge-004) | [安全错误投影](#letai-safe-error-projection) | 所有错误出口短 canary 与固定分类；保持订单、receipt 和调用次数 | [回归方法](../../../testing-manual.md#regression-oracle)；adapter 离线测试入口 `../../../crates/oasis7_letai_credit_adapter/src/lib.rs`及待补 bridge 回归 | 同候选诊断、stderr/trace 与持久化负向断言，不保存真实凭据 | planned；未执行，不代签真实上游隐私或资金验收 |
| [PRD-P2P-TBRIDGE-004：异常与验证快照](mainchain-token-newapi-quota-bridge.prd.md#tbridge-004) | [持久业务阶段](#bridge-recovery-state) | §10 的 Credited 后查询失败、重启与再次保存失败；保留真正未入账正例 | [测试手册](../../../testing-manual.md)；下述 Bridge binary suite 的持久阶段故障用例 | source、receipt/order、重载阶段、topup 调用数和退出码 | planned；并发回归不能代签跨重启恢复 |

现有测试入口为 `crates/oasis7_newapi_bridge_service/src/bin/oasis7_newapi_bridge_service/tests.rs`，分页替身在同目录 `tests_support.rs`；在这些已有入口补充 §10 的故障语义，按 `env -u RUSTC_WRAPPER cargo test -p oasis7_newapi_bridge_service --bin oasis7_newapi_bridge_service` 执行并记录实际结果。手册/测试入口存在不表示新增场景已实现或执行；本次文档不运行真实 topup、模型调用或 operator 部署，也不以测试计划代替四项独立验收。

### 11.2 当前源码差距（静态核对）

本节区分目标恢复合同与当前代码，主责 runtime_engineer / qa_engineer；源码固定基线 `c1ae79da79c2cb7962a5e87bb3cee1b60eaf623e`，未执行行为测试或外部服务。

- `service.rs::bind_user` 先保存本地身份，但 ensure 失败仍对新 binding 调用 `rollback_new_binding`；创建前失败测试期望 binding/project 为空，不能证明远端已创建后丢回复的身份恢复。
- `chain_client.rs::fetch_confirmed_account_txs` 仅请求 `limit=200` 并消费返回 items，尚不能证明 §6.2 的相关分页完整发现。
- `store.rs` 局部 mutex 与原子文件保存不能证明整次 reconcile 的有效处理权；§6.3 的延迟失败、双成功和人工终态场景仍需独立证明。
- `service_letai.rs` 依据原 state 为 `Credited` 跳过 topup；此前 user/project 失败会进入 `record_retryable_failure`，该路径可能写为 Failed/ManualReview。已有 restart 测试人工设置 Credited 后在健康路径恢复，不能证明 Credited 后前置连续失败、再次重启/保存失败仍保留成功事实。

这些缺口不改变上文 planned/目标状态，不引入付费调用或状态修复结论；共享 runtime/CI 的在途 owner 工作不作为本设计已实现的依据。

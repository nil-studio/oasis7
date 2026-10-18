# Agent Claim Restricted Grant LiveOps Runbook（2026-03-29）

Lifecycle: current controlled operator procedure; the date is its establishment provenance, not a grant round or evidence window. It does not promise a public reward, transferable value, general eligibility, public launch, or automatic entitlement; every execution remains subject to its current authority, funding, permission, review, expiry and revoke gates.

审计轮次: 6

## Meta
- Owner Role: `liveops_community`
- Review Role: `producer_system_designer`
- Document Type: `How-to / Runbook`
- Scope: `TASK-GAME-050` / `TASK-GAMEPLAY-AGC-012`
- Related PRD: `doc/game/gameplay/gameplay-agent-claim-economy-contract.prd.md`
- Product boundary: [`Agent 所有权与持续经营`](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-001)；本 runbook 保留受限 grant 的运营执行权威，只有在该产品义务被完整吸收且无 live reference 后才可删除。
- Related gameplay authority: `doc/game/gameplay/gameplay-agent-claim-economy-contract.prd.md`; mutable execution history is in the GitHub task issue evidence.
- Runtime Dependencies:
  - `crates/oasis7_runtime/src/world/event_processing/action_to_event_core.rs`
  - `crates/oasis7_runtime/src/world/event_processing/action_to_event_policy_contract.rs`
  - `crates/oasis7_runtime/src/state/apply_domain_event_main_token.rs`
  - `crates/oasis7_runtime/src/world/governance.rs`
  - `crates/oasis7_runtime/src/tests/agent_claims.rs`

<a id="public-claim-boundary"></a>
## Public claim boundary

本 runbook 是受控 restricted grant 的运营执行权威，不是公开发行公告，也不把一次成功执行变成普遍可用性或公开 entitlement。它必须遵守玩家接入与发行模块的 [`REQ-ENTRY-CLAIM-003`](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-003) 与 [`四层口径分离`](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#public-claim-four-layer-model)。

- 当前事实（current fact）：本 runbook 只能记录某个具体窗口、账户、权限和 world-state 检查实际得到的结果；runbook 本身不证明已执行、已发布或所有玩家均可获得 grant。
- 产品目标（product goal）：在受控窗口中让运营可以用正式 runtime 真值发放、检查、到期和撤销 restricted grant，并保持资金用途、权限、过期和回收可审计。
- 假设（assumption）：执行前的 admin registry、signer policy、专用池、beneficiary 状态和证据窗口均已通过当前 authority 复核；任一条件不满足时，动作必须停止或记录 blocker。
- 证据边界（evidence limit）：单次 issue/status/revoke 结果只覆盖该账户、该 world-state、该时间窗口和该 issuance reason；它不证明 public launch、开放资格、可转账资产、未来排期或其他账户可用性。
- 可发布公开 claim（releasable public claim）：只有相同窗口的执行记录、产品 PRD、根 [`README.md`](../../../README.md) 和相关专业/QA authority 一致，并且产品审核将候选标记为 `publish_ready` 时，才可把 runbook 结果用于公开说明；否则只保留为内部 evidence 或 `draft/blocked`。

## 1. 目的
- 这份 runbook 用于把 `restricted starter claim grant` 的发放、到期、撤销和事故回退固定成一套运营可执行 SOP。
- 它服务于 `PRD-GAME-011` 的受控启动金链路，不替代 runtime 真值，也不替代 QA lifecycle / audit matrix。
- 它只覆盖 `slot-1` 专用、不可转账、可过期可撤销的 restricted grant，不把该余额扩写成通用补贴或公开空投。

## 2. Runtime 真值边界
- 发放动作只能走 `IssueRestrictedStarterClaimGrant`，撤销动作只能走 `RevokeRestrictedStarterClaimGrant`；不要再用手工余额注入去替代正式发放。
- runtime 当前把 daily restricted grant 的 source bucket 固定为 `MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL`，`spend_scope` 固定为 `slot-1 claim + slot-1 upkeep`。
- `MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL` 不是自由运营余额，也不是 `liveops` 账户余额；它只能通过 `TopUpRestrictedStarterClaimLiveopsPool` 从 `ecosystem_pool` 划拨资金，且该 top-up 仍必须由当前 `ecosystem_pool` treasury controller slot 绑定的 controller account 按 signer allowlist / threshold policy 签名批准。
- runtime 现在要求 `issuer_id` 先命中 `governance_main_token_controller_registry.restricted_starter_claim_admin_account_ids`；若 registry 缺失、admin allowlist 为空或 `issuer_id` 未登记，action 会在进入 grant 状态机前直接被拒绝。
- `issuer_id=liveops` 对应的 controller signer policy 可以作为低权限运营槽位显式使用 `1-of-2`，前提是 operator-local `public_manifest.json` 为 `liveops` 记录写出 `threshold=1`，并通过 `oasis7_governance_registry_import/audit` 导入当前 world-state；该特例不会自动放宽 `ecosystem_pool` 等 treasury/controller 主槽位，它们仍默认按 `2-of-3` 运维。
- `oasis7_governance_registry_import` 当前把 `public_manifest.json` 视为整份治理 registry 快照，而不是“只补一个 slot”的 patch；重新导入时会重写 finality/controller signer registry，并把 `restricted_starter_claim_admin_account_ids` 重新置空。
- 因此不要把只含 `liveops` 的 slot-only manifest 直接导入 production world；若 custody 把 `liveops` 单独存成 `1-of-2` bundle，必须先把它并入 operator-local 的全量治理 manifest，再执行 import/audit。
- 若 `liveops` 尚未在 admin registry 中，必须先由当前 `ecosystem_pool` treasury controller slot 绑定的 controller account 提交 `UpdateRestrictedStarterClaimAdminRegistry` 把 `liveops` 加入 allowlist；runbook 本身不能替代这一步正式治理动作。
- grant 的必要字段是 `issuer_id`、`beneficiary_account_id`、`amount`、`issuance_reason`、`expires_at_epoch`；其中 `issuer_id`、`issuance_reason` 不能为空，`expires_at_epoch` 必须严格大于当前 epoch。
- 同一 beneficiary 同时只能存在 1 条可用 grant；已有 active grant、已有原始 restricted 余额、或仍有 locked restricted bond 时，runtime 会拒绝重发。
- 撤销必须由同一 `issuer_id` 发起；如果发放时 `issuer_id` 写错，后续只能用同一个错误值去 revoke，因此发放前必须双人复核字段。
- grant 到期或撤销后，账户剩余 restricted spendable 会退回 `restricted_starter_claim_liveops_pool`；之后 claim release / forced reclaim 产生的 restricted bond refund 也会直接回同一专用池，不再返还给 beneficiary。

## 3. v1 运营冻结口径
### 3.1 Canonical issuer_id
- v1 统一使用 `issuer_id = liveops`。
- 这样可以保证撤销 owner 单一，避免因为多个 `issuer_id` 并行发放导致 revoke 权限碎片化。
- `qa_engineer` 可以发起 `qa_seed` 请求，但正式 issue / revoke 仍由 `liveops_community` 以 `issuer_id = liveops` 执行。

### 3.1A Recommended operator entry
- 日常推荐入口是 `./scripts/oasis7-liveops-grant.sh`；正式 CLI `oasis7_liveops_grant_cli` 继续保留给需要更细粒度调试的工程/运营同学，不要再让运营同事手工拼接 runtime action JSON。
- 推荐命令：
  - `./scripts/oasis7-liveops-grant.sh status --world-dir <world_dir>`
  - `./scripts/oasis7-liveops-grant.sh status <account> --world-dir <world_dir>`
  - `./scripts/oasis7-liveops-grant.sh issue <account> <amount> preview_allowlist <epoch> --world-dir <world_dir>`
  - `./scripts/oasis7-liveops-grant.sh revoke <account> qa_window_closed --world-dir <world_dir>`
- 若当前 shell 已固定 world 目录，先执行 `export OASIS7_WORLD_DIR=<world_dir>`，之后可省略每条命令里的 `--world-dir`。
- wrapper 默认 `issuer_id=liveops`，支持 `--dry-run`、`--json`、`--print-cmd`；底层仍只调用 `oasis7_liveops_grant_cli`，不提供 admin roster 直改命令。admin 轮换仍必须走 controller-governed `UpdateRestrictedStarterClaimAdminRegistry`。
- 若 `liveops` signer policy 采用 `1-of-2`，建议把两把 signer 都保留在离线 custody，但日常 restricted grant issue/revoke 只要求其中一把完成 `liveops` slot 签名；这属于低权限例外，不适用于 `msig.ecosystem_governance.v1` 等高风险 treasury/controller 槽位。

### 3.1B One-time bootstrap / post-import recovery
首次启用 `liveops`，或任何一次 governance registry 重导入之后，按下面顺序恢复运营可用态：

1. 准备 operator-local 的全量 `public_manifest.json`，不要只拿 `liveops` 两条记录单独导入。
2. 确认该全量 manifest 中，`liveops` 的每条记录都显式带 `"threshold": 1`；最小示例：

```json
[
  {
    "slot_id": "liveops",
    "threshold": 1,
    "signer_id": "signer01",
    "scheme": "ed25519",
    "public_key_hex": "<liveops_signer01_public_key_hex>"
  },
  {
    "slot_id": "liveops",
    "threshold": 1,
    "signer_id": "signer02",
    "scheme": "ed25519",
    "public_key_hex": "<liveops_signer02_public_key_hex>"
  }
]
```

3. 导入全量治理 manifest：
   `env -u RUSTC_WRAPPER cargo run -p oasis7_ops_tools --bin oasis7_governance_registry_import -- --world-dir <world_dir> --public-manifest <operator_full_public_manifest.json>`
4. 立刻审计导入结果，不要跳过：
   `env -u RUSTC_WRAPPER cargo run -p oasis7_ops_tools --bin oasis7_governance_registry_audit -- --world-dir <world_dir> --public-manifest <operator_full_public_manifest.json> --strict-manifest-match --require-single-failure-tolerance`
5. 让当前 `ecosystem_pool` treasury controller slot 对应的治理操作者提交一次 `UpdateRestrictedStarterClaimAdminRegistry`，把 `liveops` 放回 admin allowlist。payload 最少应包含：

```json
{
  "type": "UpdateRestrictedStarterClaimAdminRegistry",
  "data": {
    "controller_account_id": "<ecosystem_pool_controller_account_id>",
    "next_admin_account_ids": ["liveops"]
  }
}
```

6. 若 `restricted_starter_claim_liveops_pool` 余额不足，再由当前 `ecosystem_pool` controller account 提交一次 `TopUpRestrictedStarterClaimLiveopsPool`，最少 payload 形态如下：

```json
{
  "type": "TopUpRestrictedStarterClaimLiveopsPool",
  "data": {
    "controller_account_id": "<ecosystem_pool_controller_account_id>",
    "top_up_id": "liveops-topup-<YYYYMMDD>-<seq>",
    "amount": <u64>
  }
}
```

7. 再执行 `./scripts/oasis7-liveops-grant.sh status --world-dir <world_dir>`，只有以下 4 个条件同时成立，才算恢复完成：
   - `admin_registry_configured: true`
   - `issuer_is_allowlisted_admin: true`
   - `issuer_has_signer_policy: true`
   - `restricted_claim_liveops_treasury_balance > 0`

- 若第 4 步 audit 失败，不要继续做第 5/6 步。
- 若第 7 步仍看到任一 `false`，或专用池余额仍为 `0`，不要继续发 grant；先回到 manifest / admin roster / controller slot 绑定 / top-up 记录检查。
- 任何一次 signer rotation、manifest 重导入或 world 恢复，只要重新跑了 `oasis7_governance_registry_import`，都要把这 7 步再走一遍。

### 3.2 Allowed issuance_reason
仅允许以下 3 个 `issuance_reason`：

| issuance_reason | 使用场景 | 谁可申请 | 谁执行发放 | 默认策略 |
| --- | --- | --- | --- | --- |
| `preview_allowlist` | limited preview / allowlist 首次进入中循环 | `liveops_community` | `liveops_community` | 用于受控名单账号的首个 claim 启动 |
| `qa_seed` | QA 复现、回归、手工验证 | `qa_engineer` | `liveops_community` | 用于测试账号，不得混入真实 campaign |
| `liveops_campaign` | 已批准的活动批次、定向回流、人工扶梯 | `liveops_community` | `liveops_community` | 必须能回链到具体 campaign / cohort |

- 不允许使用自由文本 reason，例如 `seed1`、`grant-test`、`promo`、`ops-temp`。
- 如果确实需要新增第 4 类 `issuance_reason`，必须先回流 `producer_system_designer`，并更新本 runbook 后再执行。

### 3.3 Amount policy
- 默认发放金额以“当前 `slot-1` canonical 启动成本”为基线，即至少覆盖 `activation fee + claim bond + first upkeep`。
- 若要提供额外续航，只能增加“经批准的 upkeep buffer”；不要为了省事直接发明显超出 `slot-1` 需求的大额 grant。
- `qa_seed` 可以带更宽的调试 buffer，但必须在请求记录里写明原因，例如 `multi-epoch repro`、`manual release/revoke validation`。

## 4. Expiry 策略
### 4.1 通用规则
- 不发永久 grant。每一笔 grant 都必须显式填写 `expires_at_epoch`。
- 不发“短到需要中途续期”的 grant。runtime 当前不支持原地延长有效期；active claim 若仍锁着 restricted bond，就不能直接重发。
- 设定 expiry 时，要覆盖完整目标窗口，并预留 `release cooldown + 人工收尾` buffer。

### 4.2 按 reason 的默认策略
| issuance_reason | 默认 expiry 策略 | 不应使用的做法 |
| --- | --- | --- |
| `preview_allowlist` | 设到当前 preview wave 结束 epoch，再额外留 `2` 个 epoch buffer | 不要只给 `+1` 或 `+2` epoch 的短 grant，避免玩家刚 claim 就触发 expiry |
| `qa_seed` | 设到该轮 QA 验证窗口结束 epoch，再额外留 `1` 个完整回收窗口 | 不要把长期手工回归账号做成“永不过期”；超窗仍需重申请求 |
| `liveops_campaign` | 设到活动关闭 epoch，再额外留 `2` 个 epoch buffer | 不要让 campaign grant 跨到下一轮 campaign，避免 cohort 边界混乱 |

- 如果当前无法可靠判断活动结束 epoch，就先不要发；先补齐 campaign 窗口定义。

## 5. Revoke 条件
以下情况应优先 `revoke`，而不是等待自然到期：

| 触发条件 | 推荐 `revoke_reason` | 说明 |
| --- | --- | --- |
| 发错账号 / beneficiary 绑定错误 | `misissued_account` | 立刻撤销，避免错误账户先消耗 restricted 额度 |
| QA 测试结束且不需要继续保留 | `qa_window_closed` | 不把 QA grant 继续留给长期账号 |
| campaign 已提前终止或名额收回 | `campaign_closed` | 比自然到期更快收敛余额 |
| 发现异常占用、转手、代持、可疑滥用 | `abuse_suspected` | 先冻结授予关系，再回流 QA / producer 判定 |
| 账号替换、重建、迁移到新 beneficiary | `account_replaced` | 旧账号先 revoke，再给新账号重新 issue |

- 若没有清晰 `revoke_reason`，先不要动手；先补齐 incident 记录。
- revoke 之后如果 beneficiary 还持有 active slot-1 claim，后续 restricted bond refund 会自动回 treasury。这是预期行为，不需要手工补第二次退款。

## 6. 发放前检查
每次 issue 前按下面顺序检查：

1. 确认申请属于 `preview_allowlist`、`qa_seed`、`liveops_campaign` 三类之一。
2. 确认本次发放统一使用 `issuer_id = liveops`。
3. 先执行 `./scripts/oasis7-liveops-grant.sh status --world-dir <world_dir>`，确认 runtime 当前 world-state 已把 `liveops` 放进 restricted grant admin registry；不要只看 runbook 文案就直接提交。
   最低通过线是同时看到 `admin_registry_configured: true`、`issuer_is_allowlisted_admin: true`、`issuer_has_signer_policy: true`。
   若未登记，先走 controller-governed `UpdateRestrictedStarterClaimAdminRegistry`，不要回退到离线 import、手工改 world 文件或给运营临时放开旁路。
4. 确认 beneficiary 没有仍在生效的 grant，也没有遗留 raw restricted balance。
5. 确认本次金额只覆盖批准用途，没有把 unrestricted 补贴混进来。
6. 确认 `expires_at_epoch` 覆盖完整窗口，不会在正常使用中途提前终态。
7. 确认 `restricted_claim_liveops_treasury_balance` 足够覆盖本次发放，若不足则先走 controller-governed `TopUpRestrictedStarterClaimLiveopsPool`，不要临时改成直接从 `ecosystem_pool` 发 grant。
8. 确认 source bucket 仍是 `MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL`，并记录本次占用额度。
9. 双人复核 `beneficiary_account_id / issuer_id / issuance_reason / amount / expires_at_epoch` 后再提交动作。

## 7. 发放执行
最小执行记录必须包含：
- `beneficiary_account_id`
- `issuer_id=liveops`
- `issuance_reason`
- `amount`
- `expires_at_epoch`
- `requester`
- `operator`
- `approval note / cohort note`

发放后必须核对：
- journal 中存在 `RestrictedStarterClaimGrantIssued`
- event 内 `source_treasury_bucket_id = MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL`
- beneficiary 的 restricted balance 与 issue amount 一致
- `restricted_claim_liveops_treasury_balance` 按同额扣减，`ecosystem_treasury_balance` 不应因为 daily issue/revoke 直接变化
- 若本次先做过 admin registry 热更新，再额外确认 journal 中已存在 `RestrictedStarterClaimAdminRegistryUpdated` governance event，且 `next_admin_account_ids` 含 `liveops`
- 若本次先做过 top-up，再额外确认 journal 中已存在 `RestrictedStarterClaimLiveopsPoolToppedUp`，且 `top_up_id`、`amount` 与批次记录一致

### 7.1 Recommended command sequence
- 发放前先看总状态：
  `./scripts/oasis7-liveops-grant.sh status --world-dir <world_dir>`
- 发放前看单个 beneficiary：
  `./scripts/oasis7-liveops-grant.sh status <beneficiary_account_id> --world-dir <world_dir>`
- 先 dry-run，再正式 issue：
  `./scripts/oasis7-liveops-grant.sh issue <beneficiary_account_id> <amount> <issuance_reason> <expires_at_epoch> --world-dir <world_dir> --dry-run`
  `./scripts/oasis7-liveops-grant.sh issue <beneficiary_account_id> <amount> <issuance_reason> <expires_at_epoch> --world-dir <world_dir>`
- 如需撤销，也先 dry-run，再正式 revoke：
  `./scripts/oasis7-liveops-grant.sh revoke <beneficiary_account_id> <revoke_reason> --world-dir <world_dir> --dry-run`
  `./scripts/oasis7-liveops-grant.sh revoke <beneficiary_account_id> <revoke_reason> --world-dir <world_dir>`

- `status` 里若指定 beneficiary，应重点看 `beneficiary_grant_status`、`beneficiary_grant_amount`、`beneficiary_grant_expires_at_epoch`、`beneficiary_grant_issuance_reason`。
- `status` 至少要同时看 `ecosystem_treasury_balance` 和 `restricted_claim_liveops_treasury_balance`；前者用于判断是否还能继续 top-up，后者才是 daily issue/revoke 真正会消耗的专用池余额。
- `issue` 成功后应看到 `issue ok`，并附带 `restricted_claim_liveops_treasury_balance_after`、`beneficiary_restricted_balance_after` 与 `event.restricted_grant_issued`。
- `revoke` 成功后应看到 `revoke ok`，并附带 `event.restricted_grant_revoked`。
- 若命令返回 `issue rejected` 或 `revoke rejected`，不要手工修 world；先按 `rejection:` 文本回到本 runbook 的第 6 节和第 9 节排查。

## 8. 到期与巡检
- `preview_allowlist` 与 `liveops_campaign` 至少在到期前 `1` 个运营检查窗口做一次预检。
- 预检时确认 beneficiary 是否仍有 active slot-1 claim、是否还在有效 cohort、是否需要按规则自然到期还是提前 revoke。
- 若决定让 grant 自然到期，不需要人工补“退款动作”；runtime 会把剩余 restricted spendable 退回 treasury，并把后续 restricted bond refund 也重定向回 treasury。
- 到期后应在运营记录中补记：
  - `expired_at_epoch`
  - `beneficiary_account_id`
  - `issued_amount`
  - `expired_amount`
  - 是否存在后续 claim release / reclaim 待观察

## 9. Incident Fallback
### 9.1 发放字段写错
- 若 `beneficiary_account_id`、`amount`、`issuance_reason`、`expires_at_epoch` 任一写错，第一优先是确认 journal 中是否已经 `Issued`。
- 如果 issue 已成功，立即评估是否满足 revoke 条件；不要再补一笔“抵消 grant”去掩盖问题。
- 若 `issuer_id` 写错，必须使用同一个错误 `issuer_id` 执行 revoke；因此 incident 记录里必须保留原始字段。

### 9.2 运行时 reject
- 若 runtime 拒绝 issue，优先按 reject note 排查以下几类：
  - beneficiary 已有 active grant
  - beneficiary 已有 raw restricted balance
  - treasury bucket 余额不足
  - `expires_at_epoch <= current_epoch`
- reject 不允许通过手工改余额绕过。要么修正输入后重试，要么升级 owner。

### 9.3 账号异常占用
- 若账号出现异常囤位、代持、共享使用或其它可疑滥用，先以 `abuse_suspected` revoke。
- 然后把 beneficiary、claim 状态、相关 epoch、证据链接回流给 `qa_engineer` 与 `producer_system_designer`。
- 对外只说明“受控 grant 已回收”，不要公开扩写具体风控细节。

### 9.4 到期策略判断失误
- 如果发现 grant 还在合法使用窗口内就将到期，先不要等待自动 expiry。
- 立即升级给 `producer_system_designer` 判断是否接受“本轮自然收回”，还是先人工 revoke 并安排 claim 收尾。
- 当前 runtime 不支持原地延长 active grant；因此“延长期限”的补救只能通过 claim 收尾后重新 issue。

## 10. 回流与日志
- 所有 issue / revoke / natural expiry incident 仍须在本次获准的 incident 记录或相关 PR 中记录；Issue 按跟踪需要使用。不再新增 daily devlog，不恢复 `.pm` 或 Project 准入。下列审计字段、重复 incident 升级、权限/资金/expiry/revoke 边界全部保留。
- 至少记录：
  - 时间
  - 角色：`liveops_community`
  - beneficiary / issuance_reason / amount / expires_at_epoch
  - 执行动作：`issued / revoked / reviewed_for_expiry / incident`
  - 遗留事项和 owner
- 若同一 `issuance_reason` 在一周内重复触发 incident，必须升级给 `producer_system_designer` 评估是否调整 policy。

## 11. Out of Scope
- 本 runbook 不定义 QA 自动化矩阵；那是 `TASK-GAME-051 / TASK-GAMEPLAY-AGC-013`。
- 本 runbook 不改变 runtime 的 canonical 成本、slot 限制、`ecosystem_pool -> restricted_starter_claim_liveops_pool` top-up 审批边界或 restricted refund sink 真值。
- 本 runbook 不引入第 4 类 issuer/reason，也不放宽 `slot-1 only / non-transferable / provenance-preserving` 边界。

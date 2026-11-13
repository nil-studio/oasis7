# oasis7 hosted_public_join 托管身份、托管密钥与受限执行（设计文档）

- 对应需求文档: `doc/p2p/blockchain/hosted-public-join-managed-identity-custody.prd.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

审计轮次: 1

本文 §1～11 承接 hosted player 的生产身份与资产托管；[§12](#dev-test-accounts-design)定义供外部 Agent 使用的开发测试账号工具。开发工具复用游戏客户端与世界权威，按当前能力与验收实施；历史扩张要求已退役，部署和实际接入仍待验证。

账号认证现由[多凭证系统设计](hosted-public-join-key-based-onboarding.design.md)承接：标准 Passkey、可选邮箱、HttpOnly 账号会话与独立世界控制授权；本设计仍拥有资产 custody、强认证与权限分层，不把登录成功视为资产出签。

## 1. 设计目标
- 把 `hosted_public_join` 从“preview player-session + browser local key”推进到“普通玩家可登录、服务端可托管、后续可自托管升级”的正式产品架构。
- 让 runtime、viewer、custody backend、LiveOps 和后续 bridge/asset 系统围绕同一组身份主键工作，而不是继续把 `player_id`、浏览器密钥和 signer 语义混在一起。
- 保持现有 hosted access 平面边界不回退：public player plane 继续公开，private control plane 继续私有，托管密钥进入新的 custody plane，而不是回流到浏览器。

## 2. 当前代码真值
| 维度 | 当前状态 | 设计结论 |
| --- | --- | --- |
| Hosted session issue | `crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_player_session.rs` 已管理 `player_id/device_session_id/release_token`、slot lease、refresh/release，并支持稳定 `player_id` 的复用发放 | public player session 与 device-session recovery 基线已落地，但 `signer_ref`/custody sign lane 仍未进入正式 contract |
| Hosted strong auth | `crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_strong_auth.rs` 通过 `OASIS7_HOSTED_STRONG_AUTH_*` + `approval_code` 给特定 `action_id` 出 preview grant | 已有 backend reauth 前置，但不是正式 custody sign lane |
| Hosted account persistence backend | `crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_account_store_backend.rs` 现已把 `hosted_account_id -> player_id` 持久化抽成 `HostedAccountStoreBackend`，支持 `file` 与 `tablestore` 双 backend，并以 `OASIS7_HOSTED_ACCOUNT_STORE_BACKEND=auto|file|tablestore`、`OASIS7_HOSTED_ACCOUNT_TABLESTORE_*` / `ALIYUN_OTS_*` env 决定 hosted 部署行为 | hosted account registry 已不再绑死单机 JSON；生产托管部署可把身份映射落到 Aliyun Tablestore，本地开发仍可保留文件 fallback |
| Legacy bootstrap off for hosted | `crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/oasis7_game_launcher_tests.rs` 已断言 `hosted_public_join` 不再解析 viewer auth bootstrap | hosted 模式已停止从 `config.toml` 或 env 直注 host key 到浏览器 |
| Browser local persistence | `crates/oasis7_viewer/software_safe_src/viewer_hosted_auth_state_module.js` 现仅持久化 `hostedAccountId/playerId/deviceSessionId/releaseToken/sessionEpoch/issuedAtUnixMs`，旧版 `privateKey` 残留会在读取时清洗掉 | hosted 浏览器已不再把长期私钥写入 `localStorage`；当前剩余缺口是邮件投递与 custody sign，而不是浏览器长期材料 debt |
| Viewer auth bootstrap implementation | `crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/viewer_auth_bootstrap.rs` 仍保留从 env / `config.toml` 读取 `node.private_key` 的 trusted-local 路径 | 该能力继续只属于 trusted-local preview，不得回流到 hosted product 默认路径 |

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264；[REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001) | 保持 `hosted_account_id`、`player_id`、`device_session_id` 与 `signer_ref` 的身份职责分离；账号登录保留稳定玩家入口，邮箱仅为可选登录凭证。 | [目标身份模型](#4-目标身份模型) | identity broker 与 runtime/account registry | 不代表 managed signer、production 邮件投递或真实跨设备恢复已经实现。 |
| Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264；[REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001) | hosted 浏览器仅持久化设备会话恢复材料，不持久化长期托管 signer。 | [浏览器存储策略](#6-浏览器存储策略) | viewer auth-state module | 当前验证范围不含历史 `privateKey` 缓存清洗。 |
| Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264；[REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001) | 保留旧 `localStorage privateKey` 残留须在读取时清洗的现有要求，并把本设计中的已实现状态与其验证缺口分开。 | [当前代码真值](#2-当前代码真值) | viewer auth-state module；回归测试仍待补齐 | 本文不声称该清洗行为已有精确回归覆盖。 |
| Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264；[REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001) | hosted player 的 public、identity、custody 与 private control planes 保持边界；不得据此扩展 node、validator 或 governance signer 的托管范围。 | [目标平面拆分](#3-目标平面拆分) | runtime 与既有 custody/governance authority | 不定义 node、validator 或 governance signer 的生产 custody。 |
| Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264；[REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001) | `prompt_control_apply`、`prompt_control_rollback` 与 `main_token_transfer` 等高风险动作目标态需经过 step-up、风险判定和 custody sign。 | [风险分级与出签策略](#7-风险分级与出签策略) | runtime 与未来 custody-service delivery | managed custody sign lane 当前未实现；本表不把目标态写成现有能力。 |
| Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) | 邮箱 challenge 应拒绝无效凭据并施加 resend/burst 限流；有效账户可恢复既有 player identity。 | [推荐组件](#5-推荐组件) | hosted account identity broker | 当前单测不覆盖 production 邮件送达、重复账户合并或完整风控冻结流程。 |
| Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) | 已签发的 device session 可刷新、撤销或在恢复后重新签发；runtime 仍是 player/entity 绑定真值。 | [Runtime / Viewer 对接原则](#hosted-runtime-viewer-boundary) | runtime session authority 与 viewer | 本地 issuer 单测不证明 hosted 部署或完整设备丢失/账户恢复 runbook。 |
| 产品 N/A：reason=external-wallet bind、cooldown 与 transfer-out 是尚未交付的专业资产 custody 路径，当前产品要求不承诺该升级功能；scope=self-custody migration、cooldown 与 transfer-out；[托管身份专业要求](hosted-public-join-managed-identity-custody.prd.md#hosted-custody-user-experience) | 自托管升级必须通过显式 external-wallet bind 或 transfer-out 路径，并避免把托管私钥回传浏览器。 | [实现顺序](#10-实现顺序) | runtime/account 与资产转移 owners，具体交付 owner 待任务绑定 | self-custody bind、cooldown 与 transfer-out 仍属后续阶段。 |
| Trace refs: O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b；[REQ-ENTRY-CANDIDATE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-candidate-001) | 对外阶段必须继续表述为 `limited playable technical preview`，不能将设计目标当作已投产 hosted wallet。 | [当前阶段口径](#11-当前阶段口径) | producer/system review 与 claims review | 本设计不授权 production launch 或更改当前阶段。 |
| 产品 N/A：reason=分层 SMTP/store/signer/approval secret 隔离是部署与运维证据，不构成单独玩家体验承诺；scope=dev、staging、production 环境间的 hosted identity/custody 配置与数据隔离；[Hosted custody 技术要求](hosted-public-join-managed-identity-custody.prd.md#3-technical-requirements) | dev、staging、production 仍须隔离 SMTP、account store、signer/approval secrets、风控阈值和对外 claims；当前阶段记录不能替代分层部署证据。 | [当前阶段口径](#11-当前阶段口径) | hosted identity/custody deployment owner，须由分层交付任务绑定 | 本文没有 staging/production 隔离的 live evidence，也不据此声称环境边界已验证。 |
| Trace refs: O7-REQ-56c3a553-8216-4e44-86dc-bd368f3b53cb；[REQ-DEV-TEST-ACCOUNTS-001](hosted-public-join-managed-identity-custody.prd.md#req-dev-test-accounts-001) 产品 N/A：reason=开发测试账号工具的专业要求，不新增玩家或发行承诺；scope=dev-test-accounts-v1； | 单个可信服务保管测试凭据，明确客户端、世界与测试编排职责。 | [开发工具条款](#dev-test-accounts-boundaries) | runtime_engineer／blockchain_ops_engineer／qa_engineer | 账号工具待实现；部署与实际调用仍待验证。 |
| Trace refs: O7-REQ-8b95aab7-037d-45cc-8d02-e320d1ea4570；[REQ-DEV-TEST-ACCOUNTS-002](hosted-public-join-managed-identity-custody.prd.md#req-dev-test-accounts-002) 产品 N/A：reason=开发测试账号工具的专业要求，不新增玩家或发行承诺；scope=dev-test-accounts-v1； | 账号目录和不可变会话支持多账号选择及并行隔离，配置变化不重置旧用量。 | [开发工具条款](#dev-test-accounts-sessions) | runtime_engineer／blockchain_ops_engineer／qa_engineer | 账号工具待实现；部署与实际调用仍待验证。 |
| Trace refs: O7-REQ-9dfc3e3f-250d-4e90-a246-42a7fcdc2ef5；[REQ-DEV-TEST-ACCOUNTS-003](hosted-public-join-managed-identity-custody.prd.md#req-dev-test-accounts-003) 产品 N/A：reason=开发测试账号工具的专业要求，不新增玩家或发行承诺；scope=dev-test-accounts-v1； | 复用正式工具及原操作；停用与恢复不复活旧写权，准确报告世界结果。 | [开发工具条款](#dev-test-accounts-operations) | runtime_engineer／blockchain_ops_engineer／qa_engineer | 账号工具待实现；部署与实际调用仍待验证。 |
| Trace refs: O7-REQ-7405a6cf-1d2a-4518-8521-013b64ae33c7；[REQ-DEV-TEST-ACCOUNTS-004](hosted-public-join-managed-identity-custody.prd.md#req-dev-test-accounts-004) 产品 N/A：reason=开发测试账号工具的专业要求，不新增玩家或发行承诺；scope=dev-test-accounts-v1； | 先验证实际接入，本地与远程外部 Agent 分别完成真实双账号验收。 | [开发工具条款](#dev-test-accounts-adapters) | runtime_engineer／blockchain_ops_engineer／qa_engineer | 账号工具待实现；部署与实际调用仍待验证。 |

## 3. 目标平面拆分
- `public player plane`
  - 对外可见: 静态网页、世界只读快照、guest/player session 入口、低风险 gameplay 输入
  - 不可见: operator control API、长期 signer、custody backend
- `identity plane`
  - 负责邮箱登录、OTP、device session、account recovery、rate limit
  - 输出: `hosted_account_id`, `device_session_id`, `player_session`
- `custody plane`
  - 负责 `signer_ref`、托管签名、step-up 授权、风险策略、审计日志
  - 对 public browser 永远只输出签名结果或 challenge state，不输出托管私钥
- `private control plane`
  - 继续承载世界启停、事故处理、运营控制、operator-only GUI actions

## 4. 目标身份模型
| 身份层 | 主键 | 生命周期 | 存储位置 | 用途 |
| --- | --- | --- | --- | --- |
| Hosted account | `hosted_account_id` | 长期 | identity store | 登录与恢复 |
| Player identity | `player_id` | 世界范围长期 | runtime/account registry | 玩家实体和世界内归属 |
| Device session | `device_session_id` | 短期 | browser + identity plane | 当前设备登录态 |
| Managed signer | `signer_ref` | 长期 | custody plane | 托管签名能力 |
| External wallet binding | `external_account_id` | 长期 | account registry | 自托管升级 |

设计规则:
- `hosted_account_id` 不直接等于 `player_id`，以便后续支持跨世界、跨设备与运营账户合并。
- `player_id` 不直接承载托管密钥引用，统一通过账户或 signer binding 间接关联。
- `device_session_id` 可以失效、轮换、冻结；它只服务当前设备，不等于账户长期身份。
- `signer_ref` 是 runtime/asset/custody 的唯一长期签名引用，底层后端可替换，但 API 语义不能漂移。

## 5. 推荐组件
- `hosted-account-service`
  - `start_login(channel, handle)`
  - `complete_login(challenge_id, code_or_link, device_info)`
  - `recover_account(handle, recovery_proof)`
- `player-session-broker`
  - `issue_guest_session(world_id)`
  - `exchange_account_for_player_session(hosted_account_id, device_pubkey)`
  - `refresh_device_session(device_session_id)`
  - `revoke_device_session(device_session_id)`
- `managed-custody-service`
  - `provision_signer(account_id, policy_profile)`
  - `prepare_sign(signer_ref, action_id, payload_digest)`
  - `approve_sign(authz_id, step_up_proof)`
  - `finalize_sign(authz_id)`
- `wallet-transition-service`
  - `bind_external_wallet(account_id, external_account_id, proof)`
  - `request_transfer_out(account_id, target_account_id, scope)`
  - `complete_transfer_out(request_id)`

## 6. 浏览器存储策略
- 允许:
  - `device_session_id`
  - 非导出的 device key handle 或等价短期浏览器材料
  - UI locale、非敏感 feature flags
- 禁止:
  - 托管 signer 明文私钥
  - node signer / governance signer / host key
  - 原始 OTP、长期 step-up token
- 兼容过渡:
  - hosted 浏览器当前只保留设备会话材料与页内临时 key；旧版 `localStorage privateKey` 残留会在读取时被清洗。
  - 后续切到真实邮件 provider / custody sign lane 时，仍应保留旧缓存迁移与提示逻辑，避免历史浏览器状态漂移回长期密钥路径。

## 7. 风险分级与出签策略
| 动作类 | 示例 | 浏览器本地可完成 | 需要 step-up | 需要 custody sign |
| --- | --- | --- | --- | --- |
| `guest_read` | 观战、读状态 | 是 | 否 | 否 |
| `player_gameplay` | 移动、普通玩法输入、低风险 chat | 是 | 否 | 否 |
| `creator_control_preview` | `prompt_control_preview` | 否 | 可选 | 视策略而定 |
| `creator_control_high_risk` | `prompt_control_apply` / `rollback` | 否 | 是 | 是 |
| `asset_transfer` | `main_token_transfer` | 否 | 是 | 是 |
| `governance_admin` | 治理或 treasury 相关动作 | 否 | 是 | 是，但不建议复用 player custody |

规则:
- gameplay 输入继续尽量留在 player session / device session 层，避免每个动作都经过 custody service。
- `main_token_transfer` 的 hosted 目标态不再是永久 `blocked`，而是进入 `step-up + managed custody sign` lane。
- governance/admin 如需浏览器入口，优先走独立更高等级 plane，不与普通 player custody 混用。

## 8. KMS / custody backend 边界

本节约束生产 player custody 目标；[开发测试账号工具](#dev-test-accounts-design)只服务隔离测试环境，不构成本节生产托管能力的证明。
- 上层契约固定为 `signer_ref + sign API + audit trail`。
- 后端可以有两类实现:
  - `KMS/HSM direct key`
    - 适用于算法、吞吐与成本满足时
    - 优点: 私钥不可导出、托管语义清晰
  - `KMS-wrapped sealed key backend`
    - 适用于运行时算法或吞吐不适合直接落到 KMS key API 时
    - 优点: 可以保留上层 trust boundary，同时降低厂商耦合
- 不允许的实现:
  - 浏览器直接拿托管私钥
  - HTML/bootstrap 注入长期 signer
  - 只靠 `approval_code` + env signer 的长期生产运行

<a id="hosted-runtime-viewer-boundary"></a>
## 9. Runtime / Viewer 对接原则
- Runtime
  - 只校验 session、capability、签名证明和 `signer_ref` 绑定
  - 不直接关心邮箱明文
  - `player_id -> entity` 绑定继续由 runtime 真值维护
- Viewer
  - UI 上显示 `Oasis ID`、登录状态、custody mode、设备状态
  - 默认不显示公钥输入框；“外部钱包绑定”单独作为高级入口
  - 对旧 preview 路径要明确提示“这是 trusted-local preview，不是 hosted public join 正式模式”

## 10. 实现顺序
1. `hosted-managed-identity-doc-freeze`
   - 冻结产品和 trust boundary
2. `hosted-account-identity-broker`
   - 增加 hosted account 与登录因子
3. `device-session-and-runtime-binding`
   - 替换浏览器 `privateKey` 持久化，改成设备会话模型
4. `managed-custody-sign-api`
   - 新增 `signer_ref` 和 sign API
5. `step-up-auth-and-risk-policy`
   - 让高风险动作进入可审计的 step-up 体系
6. `external-wallet-bind-and-transfer-out`
   - 提供托管退出与自托管升级
7. `qa-abuse-and-liveops-runbook`
   - 形成运营和风控收口

## 11. 当前阶段口径
- 当前已成立:
  - `hosted_public_join` 已禁止 legacy host key bootstrap 直接进入 hosted mode
  - public player session / preview strong-auth contract 已存在
  - hosted account 邮箱登录 broker 已落地，viewer 正式入口已切到 hosted account login
  - hosted account registry 已支持 `file/tablestore` 双 backend；默认 `auto` 模式下无 OTS 配置走本地文件，有 `OASIS7_HOSTED_ACCOUNT_TABLESTORE_*` 或 `ALIYUN_OTS_*` 时自动切到 Aliyun Tablestore，并支持自动建表
  - hosted 浏览器已切到 `device_session + in-memory ephemeral Ed25519` 恢复模型，不再持久化 hosted player 私钥
- 当前未成立:
  - managed custody sign lane
  - self-custody bind / transfer-out 正式能力
  - 风控冻结与恢复 runbook
- 结论:
  - 当前仍是 `limited playable technical preview`
  - 本文描述的托管身份方向已有第一版实现，但距离生产级 hosted login / managed custody 仍有明显缺口

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264；[REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001) | [目标身份模型](#4-目标身份模型) | hosted viewer entry 保留稳定 hosted account/player identity/session 关联；凭证方式不改变 primary mode。 | [测试分层手册](../../../testing-manual.md)规定 required-tier 选择；现有 ID：`crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_account_identity_tests.rs::hosted_account_login_start_rejects_phone_channel`、`::hosted_account_login_complete_reuses_stable_player_id`；本地 Rust 单元层，`test_tier_required`。 | 对应实现任务的 Issue evidence 与冻结候选单测结果。 | 不证明 production 邮件送达、跨世界账户映射或 managed signer 绑定。 |
| Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264；[REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001) | [浏览器存储策略](#6-浏览器存储策略) | viewer 持久化 hosted browser session 时只保留身份和设备/session 恢复字段。 | [viewer hosted auth-state tests](../../../crates/oasis7_viewer/software_safe_src/viewer_hosted_auth_state_module.test.js)：`recovers snake_case hosted player session storage and migrates it to the canonical key shape`；本地 Vitest 层，`test_tier_required`。 | 对应实现任务的 Issue evidence 与冻结候选 Vitest 结果。 | 该测试检查 canonical payload；它不直接断言旧 `privateKey` 缓存清洗。 |
| Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264；[REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001) | [当前代码真值](#2-当前代码真值) | 旧 `privateKey` 缓存读取时清洗是设计记录的现状声明；需有专门负例才能独立验证。 | N/A; reason=仓内没有直接覆盖历史 privateKey 残留清洗的精确回归测试; scope=旧 viewer localStorage cache migration; owner_role=viewer_engineer; evidence_ref=hosted-public-join-managed-identity-custody.design.md#2-当前代码真值; re-evaluate=补充并运行对应旧缓存回归测试时 | 当前设计状态声明；未来对应实现任务 Issue evidence 与负例结果。 | 不能从 canonical session payload 测试推导旧缓存清洗已验证。 |
| Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264；[REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001) | [目标平面拆分](#3-目标平面拆分) | hosted public join 不应解析 trusted-local viewer auth bootstrap。 | [测试分层手册](../../../testing-manual.md)规定 required-tier 选择；现有 ID：`crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/launcher_viewer_auth_bootstrap_tests.rs::hosted_public_join_disables_viewer_auth_bootstrap_resolution`；本地 Rust 单元层，`test_tier_required`。 | 对应实现任务的 Issue evidence 与冻结候选单测结果。 | 不验证 KMS/HSM custody、node/governance signer 隔离或 hosted production planes。 |
| Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264；[REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001) | [风险分级与出签策略](#7-风险分级与出签策略) | 高风险资产/控制动作目标态应经过 step-up、policy 与 custody sign；当前设计应继续区分 preview strong-auth 与正式 sign lane。 | N/A; reason=正式 managed custody sign lane 尚未实现且仓内没有该行为的匹配测试; scope=prompt control 高风险出签、main_token_transfer 与生产 custody backend; owner_role=runtime_engineer; evidence_ref=hosted-public-join-managed-identity-custody.design.md#11-当前阶段口径; re-evaluate=managed custody sign lane 首次实现时 | 当前阶段声明；未来实现任务 Issue evidence、单测及集成验证结果。 | 现有 preview strong-auth 不能证明正式 signer_ref 出签或生产风险策略。 |
| Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) | [推荐组件](#5-推荐组件) | email challenge 应拒绝错误 OTP 并执行 resend/burst 限流；有效凭据应复用稳定账户/player identity。 | [测试分层手册](../../../testing-manual.md)规定 required-tier 选择；现有 ID：`crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_account_identity_tests.rs::hosted_account_login_complete_rejects_wrong_otp`、`::hosted_account_login_start_enforces_burst_rate_limit`；本地 Rust 单元层，`test_tier_required`。 | 对应实现任务的 Issue evidence 与冻结候选单测结果。 | 不证明重复账户合并、冻结复核或运营恢复 runbook。 |
| Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) | [Runtime / Viewer 对接原则](#hosted-runtime-viewer-boundary) | device session refresh/release 与撤销状态必须维持 player-session authority；runtime 继续维护 player/entity binding。 | [测试分层手册](../../../testing-manual.md)规定 required-tier 选择；现有 ID：`crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_player_session_tests.rs::hosted_player_session_refresh_rotates_registration_grant_for_new_browser_key`、`::hosted_player_session_ledger_recovers_lease_rate_and_revocation_after_restart`；本地 Rust 单元层，`test_tier_required`。 | 对应实现任务的 Issue evidence 与冻结候选单测结果。 | 不证明真实 hosted runtime 的账户恢复流程或 LiveOps device-loss 演练。 |
| 产品 N/A：reason=external-wallet bind、cooldown 与 transfer-out 是尚未交付的专业资产 custody 路径，当前产品要求不承诺该升级功能；scope=self-custody migration、cooldown 与 transfer-out；[托管身份专业要求](hosted-public-join-managed-identity-custody.prd.md#hosted-custody-user-experience) | [实现顺序](#10-实现顺序) | transfer-out/self-custody 仅在有 external wallet bind、冷却与审计回归后可视为验证。 | N/A; reason=external wallet bind 与 transfer-out 当前未实现且没有匹配测试或操作手册; scope=self-custody migration、cooldown 与 transfer audit; owner_role=runtime_engineer; evidence_ref=hosted-public-join-managed-identity-custody.design.md#11-当前阶段口径; re-evaluate=external wallet bind 或 transfer-out 首次实现时 | 当前阶段声明；未来对应实现任务 Issue evidence 与测试/审计工件。 | 不证明外部账户绑定、资产迁移或托管退出已可用。 |
| Trace refs: O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b；[REQ-ENTRY-CANDIDATE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-candidate-001) | [当前阶段口径](#11-当前阶段口径) | 文档与对外 claims 应保留当前 preview 阶段语义，不将目标架构描述成已发布能力。 | N/A; reason=阶段与 claims 是本设计的专业文档判定且没有匹配的自动测试或操作手册; scope=仓内及对外 hosted wallet/custody claims; owner_role=producer_system_designer; evidence_ref=hosted-public-join-managed-identity-custody.design.md#11-当前阶段口径; re-evaluate=阶段或 claims 发生变化并进入独立 review 时 | 本设计阶段记录与对应任务的专业 review evidence。 | 文档结构检查不证明真实服务部署或 release readiness。 |
| 产品 N/A：reason=分层 SMTP/store/signer/approval secret 隔离是部署与运维证据，不构成单独玩家体验承诺；scope=dev、staging、production 环境间的 hosted identity/custody 配置与数据隔离；[Hosted custody 技术要求](hosted-public-join-managed-identity-custody.prd.md#3-technical-requirements) | [当前阶段口径](#11-当前阶段口径) | dev、staging、production 之间的 SMTP、account store、signer/approval secret、风控阈值和 claims 隔离必须由分层环境证据证明。 | N/A; reason=仓内没有覆盖三层 hosted identity/custody 服务隔离的精确测试或手册; scope=staging/production data plane 与独立环境配置; owner_role=blockchain_ops_engineer; evidence_ref=hosted-public-join-managed-identity-custody.design.md#11-当前阶段口径; re-evaluate=分层部署任务提供独立环境验证证据时 | 未来分层部署任务 Issue evidence 与 staging/prod smoke artifact。 | 当前本地单测和文档检查不证明真实环境隔离。 |
| Trace refs: O7-REQ-56c3a553-8216-4e44-86dc-bd368f3b53cb；[REQ-DEV-TEST-ACCOUNTS-001](hosted-public-join-managed-identity-custody.prd.md#req-dev-test-accounts-001) 产品 N/A：reason=开发测试账号工具的专业要求，不新增玩家或发行承诺；scope=dev-test-accounts-v1； | [开发工具条款](#dev-test-accounts-boundaries) | 单个可信服务保管测试凭据，明确客户端、世界与测试编排职责。 | [开发账号工具验收](../../testing/manual/agent-runtime-self-playtest.manual.md#dev-test-accounts-validation) | 后续实现 PR 的合同／故障回归和实际 Agent 双账号运行结果。 | 当前仅设计，未执行新 profile 或真实环境验收。 |
| Trace refs: O7-REQ-8b95aab7-037d-45cc-8d02-e320d1ea4570；[REQ-DEV-TEST-ACCOUNTS-002](hosted-public-join-managed-identity-custody.prd.md#req-dev-test-accounts-002) 产品 N/A：reason=开发测试账号工具的专业要求，不新增玩家或发行承诺；scope=dev-test-accounts-v1； | [开发工具条款](#dev-test-accounts-sessions) | 账号目录和不可变会话支持多账号选择及并行隔离，配置变化不重置旧用量。 | [开发账号工具验收](../../testing/manual/agent-runtime-self-playtest.manual.md#dev-test-accounts-validation) | 后续实现 PR 的合同／故障回归和实际 Agent 双账号运行结果。 | 当前仅设计，未执行新 profile 或真实环境验收。 |
| Trace refs: O7-REQ-9dfc3e3f-250d-4e90-a246-42a7fcdc2ef5；[REQ-DEV-TEST-ACCOUNTS-003](hosted-public-join-managed-identity-custody.prd.md#req-dev-test-accounts-003) 产品 N/A：reason=开发测试账号工具的专业要求，不新增玩家或发行承诺；scope=dev-test-accounts-v1； | [开发工具条款](#dev-test-accounts-operations) | 复用正式工具及原操作；停用与恢复不复活旧写权，准确报告世界结果。 | [开发账号工具验收](../../testing/manual/agent-runtime-self-playtest.manual.md#dev-test-accounts-validation) | 后续实现 PR 的合同／故障回归和实际 Agent 双账号运行结果。 | 当前仅设计，未执行新 profile 或真实环境验收。 |
| Trace refs: O7-REQ-7405a6cf-1d2a-4518-8521-013b64ae33c7；[REQ-DEV-TEST-ACCOUNTS-004](hosted-public-join-managed-identity-custody.prd.md#req-dev-test-accounts-004) 产品 N/A：reason=开发测试账号工具的专业要求，不新增玩家或发行承诺；scope=dev-test-accounts-v1； | [开发工具条款](#dev-test-accounts-adapters) | 先验证实际接入，本地与远程外部 Agent 分别完成真实双账号验收。 | [开发账号工具验收](../../testing/manual/agent-runtime-self-playtest.manual.md#dev-test-accounts-validation) | 后续实现 PR 的合同／故障回归和实际 Agent 双账号运行结果。 | 当前仅设计，未执行新 profile 或真实环境验收。 |

<a id="dev-test-accounts-design"></a>
## 12. 外部 Agent 开发测试账号工具职责与接口

本节承接 [PRD §6](hosted-public-join-managed-identity-custody.prd.md#dev-test-accounts)，定义独立 `dev-test-accounts-v1`。当前为设计目标；旧扩张要求已从有效设计移除，历史定位见 [PRD 退役记录](hosted-public-join-managed-identity-custody.prd.md#retired-cloud-custody-requirements)。专业主责为 runtime_engineer，凭据与部署边界由 blockchain_ops_engineer 审读，真实接入及双账号验证由 qa_engineer 承接。执行安排只维护在 [#5076](https://github.com/nil-studio/oasis7/issues/5076)。

<a id="dev-test-accounts-boundaries"></a>
### 12.1 职责、信任与部署

Trace refs: O7-REQ-56c3a553-8216-4e44-86dc-bd368f3b53cb；承接 [REQ-DEV-TEST-ACCOUNTS-001](hosted-public-join-managed-identity-custody.prd.md#req-dev-test-accounts-001)。

| 层 | 拥有的内容 | 下游与边界 |
| --- | --- | --- |
| 账号工具 | 非秘密账号目录、测试授权、会话绑定、准入／用量、操作引用、停用和诊断 | 只接受登记账号与固定工具，调用既有客户端；不复制世界权限与业务状态机 |
| 可信凭据保管 | 测试账号私钥、原生登录材料、解锁材料与受限签名调用 | 复用现有加密存储和角色用途；模型无秘密读取、raw sign、任意路径或 URL 入口 |
| Game Client | 登录、Owner／Executor 类型、canonical 编码、签名、发送和原结果查询 | 继续消费现有身份／世界 authority；合并部署不合并协议角色或共用不同用途的 key |
| 身份／世界服务 | 当前主体、授权、资源预算与业务结果 | 工具的目录、会话或 accepted 不能替代权威校验及真实结果 |
| 测试编排与 Runtime | setup、测试选择和效果核对由编排承担；Runtime 使用获准工具 | 世界 Stop／Revoke／清理仍调用既有 Owner 能力，工具服务不自动获得这些权限 |

一个可信服务包含账号、凭据和调用模块。服务运营者和可部署该服务的主体属于可信边界；此方案不抵御该主体控制全部测试凭据的风险。模型与服务采用不同受限安全主体或独立远程服务，不能以同 UID 的文件权限声称密钥不可读取。首版本工具的管理入口为可信运营者本地 CLI，以操作系统安全主体及私有管理目录所有权和访问权限认证、授权：目录由运营者拥有且仅该主体可访问，模型采用不同 UID，不能进入该目录或执行管理入口。该 CLI 可登记账号、修改测试授权；不建设远程管理 API，模型工具鉴权不能进入管理面。原生 `/session` 只核对登记账号当前身份，不授予工具管理员权限。

Rust 优先复用 `oasis7_client_storage`、`oasis7_game_client` 及既有协议类型。本地／普通服务进程可复用 native broker；其 Unix socket、文件与子进程不能直接搬入 Worker。若使用 Cloudflare，仅复用可在该平台运行的 custody DTO、密码与持久适配，不把整个 native 客户端链接进 Wasm。实施时选择一条可实际运行的部署路径即可，平台迁移不成为首版前置。

<a id="dev-test-accounts-workers"></a>
#### Workers 部署适配

在 `dev-test-accounts-v1` 内增加 Workers 适配时，九方法 DTO、不可变会话、原件 Scope、累计用量及拒绝/恢复语义沿用本节，不恢复退役的四角色发布或 TestSession 建设。共享纯合同由 `oasis7_custody_core::dev_test` 声明，native 与 Worker 消费同一类型及状态转换；协议签名、canonical 编码、完整 task fence 和世界回执继续复用 `oasis7_client_api`。纯合同不包含当前身份认证、可信时钟或持久化成功证明，反序列化历史记录不产生新权限。

| 适配职责 | Native | Workers |
| --- | --- | --- |
| 可信准入、扣额与原件关联 | 文件锁内原子持久化 | 同一 SQLite Durable Object 事务内持久化；按同一 grant/caller 汇总 A/B，不按连接或 session 清零 |
| SDK 原件与私密材料 | 原加密 vault/state 及受限 UID | 可信 Worker 的私有配置、加密存储及平台适配；模型不获得 Cookie、密钥、KEK、配置路径或任意签名接口 |
| 当前身份、权限与世界结果 | 正式 native 客户端 | 固定 HTTPS authority 的 fetch/crypto/store 适配，逐次核对身份及完整 fence；不得把本地代理响应冒充 Worker 执行 |
| 会话与重启 | 启动 epoch 使旧 session 无效，账本保留 | 平台休眠／同授权的实例替换从 SQLite 读取尚未关闭的不可变会话，每次按当前身份、授权版本、完整绑定与期限重验；服务授权轮换或关闭不能复活旧 session、权限或额度 |
| 恢复 | 旧授权退休，历史 query-only，新明确有限授权 | 相同合同；平台回滚与 DO 历史恢复不能恢复写权、旧余额或自动重发未知原件 |

Worker 本身不承载世界共识；Native 身份和世界 authority 的固定 HTTPS 地址、world/genesis/service key 以及合法测试身份是部署输入。Worker 业务执行链必须覆盖 Context 准备、签名原件先持久化、唯一首发与原件查询，不允许将模型 DTO 转换成任意 URL 或 raw sign。凭据、原件和预算必须在网络发送前满足持久化合同，跨异步等待后重验当前期限与授权；未知保持未知，不重签或更换原 ID。

部署采用独立 staging 应用并固定 Free 套餐的真实包大小、CPU、存储与调用限制，不以账户请求额度替代运行时可行性验证。管理与模型工具分离，Secrets 及部署权限仅由可信运营者持有；是否采用多个 Worker/DO 由状态原子性及安全边界决定，不能为复用旧配置增加本工具无关义务。生产/云端状态只在实际构建、部署、身份与 A/B 业务及恢复验收后声明。实现与剩余义务由 #5775、#5776、#5777 承接，本段不代表平台适配或云端部署已完成。

Cloudflare 的[单变量上限为 5KB，Free 的变量总数为 64](https://developers.cloudflare.com/workers/platform/limits/#environment-variables)，不能把合法的多账号配置假定为一个可部署 Secret。配置保留单片 `DEV_TEST_POLICY` 兼容入口；较大配置由 `DEV_TEST_POLICY_MANIFEST` 指定版本、分片数量和完整文本的 BLAKE3，按序读取 `DEV_TEST_POLICY_PART_01` 至 `_16`。每片不超过 4096 UTF-8 字节，总文本仍不超过 60KiB；缺片、空片、超限、摘要不符、未知 manifest 字段或同时配置单片与 manifest 均在鉴权、扣额及网络调用前拒绝，不降级回旧配置。未被 manifest 引用的旧片不参与配置。可信离线工具生成可供一次 bulk 更新的绑定映射，显式清除另一表示及多余旧片；与材料、凭据和其他绑定合计仍须满足平台变量数量上限。分片只改变部署表示，拼接后的完整策略文本及其摘要仍是原有 generation、会话与累计账本的依据，不产生新授权或重置用量。

Workers 的平台实例可能在请求间被替换，不能把易失内存当作会话存储。SQLite 保存无凭据的会话引用、不可变 grant/caller/绑定／动作范围、签发与到期时间及关闭墓碑；返回引用前必须提交成功。每次调用复用共享会话合同核对当前授权，session_ref 不提供认证。记录缺失、损坏或已关闭时拒绝，不补建；不从原操作推导新会话。逻辑服务重启／备份恢复须轮换 service_grant_id，旧会话因此失效；普通平台休眠不轮换授权、不延长期限或清零额度。Native 的启动 epoch 合同保持。

<a id="dev-test-accounts-sessions"></a>
### 12.2 账号目录、测试授权与会话

Trace refs: O7-REQ-8b95aab7-037d-45cc-8d02-e320d1ea4570；承接 [REQ-DEV-TEST-ACCOUNTS-002](hosted-public-join-managed-identity-custody.prd.md#req-dev-test-accounts-002)。

| 对象 | 必需内容与稳定关系 |
| --- | --- |
| 测试账号 | 稳定 account_ref、真实 account_id／公开身份、environment、别名、状态、可信内部 credential_ref；列表只返回获准非秘密字段 |
| 测试授权 | 当前调用主体、允许账号／世界与资源集合、动作范围、绝对到期、累计调用上限和并发限制；默认无权限，无无限额度 |
| 工具会话 | 不可变真实账号、environment／world、resource、授权版本、动作范围和不越父授权的期限；session_ref 只作引用，不能单独作为认证 |
| 操作关联 | 账号／资源作用域、稳定工具 request ID、正文摘要、既有客户端原 operation 引用和最后已核实状态；不随会话或服务重启更换操作身份 |

管理员通过原生合法开户／登录准备测试账号，然后登记可信凭据引用。首版支持已准备账号和世界，凭据引用由管理员配置；工具调用不能改变注册权限或自动导入其他用途的凭据。重复登记以真实账号和环境判断，别名不作为权限主体。

模型入口的最小合同为账号列表、打开指定账号会话、调用固定工具、查询原操作、关闭自身会话。CLI 与远程适配消费同一合同，名称与 DTO 在实现时统一导出；这些是待实现接口，不是现有命令。调用必须经过当前认证、会话绑定与账号状态校验；请求携带另一个账号、环境或资源时拒绝，不能用列表展示值覆盖服务记录。

每个会话固定目标。需要切换时打开另一个获准会话，A／B 并行互不改写；普通调用不接受全局 current_account。同一测试授权的累计用量持久保存，打开新会话、别名变更或服务重启均不清零。新增账号不修改既有会话和原操作；权限收缩或账号停用立即影响后续准入，扩大范围须由管理员更新授权并新建会话。每账号的写操作沿用客户端可支持的串行／并发边界，首版可串行写入而支持不同账号并行。

<a id="dev-test-accounts-operations"></a>
### 12.3 调用、停用与恢复

Trace refs: O7-REQ-9dfc3e3f-250d-4e90-a246-42a7fcdc2ef5；承接 [REQ-DEV-TEST-ACCOUNTS-003](hosted-public-join-managed-identity-custody.prd.md#req-dev-test-accounts-003)。

工具层只包装既有 Game Client 的类型化操作和白名单参数，不接受模型传入凭据路径、服务 origin、签名原文或任意 shell。授权按实际读取／写入范围判断；游戏协议继续核对原生身份、Owner／Executor、fence 与业务预算。

首次发送前持久保存账号／资源、request ID、正文摘要和原生操作关联；准入与累计工具用量原子登记。同 ID 同正文查回原状态，同 ID 异正文返回冲突。发生超时、断线或重启后，根据已保存的原生 operation 查询；只有原客户端协议明确允许的同原请求恢复才可发送，不生成替代 nonce、任务或写操作。工具额度控制调用总量，内部请求受常规限流与超时控制。

停用账号／授权或关闭会话与后续准入使用同一串行化边界。完成该操作后不再接受新的执行；已经准入并可能发出的操作保留在途或 unknown，不承诺撤回已发生效果。关闭会话可报告已拒绝新调用；只有取得对应世界权威结果才报告 Stop、Revoke 或 Closed。结果查询仍需当前有效读权，关闭或过期本身不创造新的读取授权。

账号、累计用量和原操作持久化；Native 工具会话的活性只属于当前服务启动实例，重启后旧 session_ref 不能继续执行；Workers 平台实例替换按 §12.1 的持久会话边界处理，逻辑服务重启仍轮换授权并使旧引用失效。普通进程重启保留当前持久化授权及累计用量；客户端在该测试授权仍有效时重新认证并建立新会话，查询原账号／资源的旧操作，原操作身份保持不变。备份恢复后，恢复的旧测试授权保持失效且不能重新启用；管理员核对实际账号、权限、已有效果和未决操作后，须明确建立新的测试授权。缺失或无法核实的用量、原操作保留 unknown，不把备份值声明为最新余额，不据此自动重发旧写入，也不能从备份 active 标志恢复写权。此方案信任恢复操作者，不新增对抗恶意部署者的独立恢复 authority。

恢复管理入口须在同一配置控制锁与累计账本锁下生成工具快照，包含目录、授权和原操作关联；不回滚原生世界、身份密钥或SDK当前原件缓存。后者仍作为当前权限和原件证明的外部依赖，缺失时unknown。新私有恢复目标先持久化restore_pending，完成后保留旧grant ID/ref和凭据退休记录；恢复配置明确要求恢复记录，标记缺失不能降级普通启动。旧授权不能经apply/enable复活。可信管理员完成当前身份／权限、已有效果及未决原件核对后，以不同grant/credential明确授予新的有限预算；该新预算不是备份用量的当前余额。历史关联只供当前同scope读权下的Lookup，调用仍扣新授权额度；不能将历史Context导入新写入上下文或重签原nonce。


错误至少区分未认证／无权限、账号或环境不匹配、会话过期／停用、额度不足、原请求冲突、原生身份失效、下游拒绝和结果未知。输出含非秘密账号、session／operation 引用与可执行下一步；日志记录准入、拒绝、调用和恢复，排除密钥、Cookie、token 和完整私有正文。没有原结果或核对权限时保留 unknown／blocked，不把失败转为成功。

<a id="dev-test-accounts-adapters"></a>
### 12.4 外部 Agent 接入

Trace refs: O7-REQ-7405a6cf-1d2a-4518-8521-013b64ae33c7；承接 [REQ-DEV-TEST-ACCOUNTS-004](hosted-public-join-managed-identity-custody.prd.md#req-dev-test-accounts-004)。

先用实际外部 Agent 验证认证与只读工具调用，再固定适配方式。本地入口复用受限 socket／CLI；远程入口采用实际支持的 HTTPS／MCP 认证，凭据由可信连接器或宿主配置保管。若实际 connector 无法保管认证秘密或没有可用工具入口，记录具体缺口，不以添加 Runtime 名称或 curl 可达视为支持。

远程服务固定 HTTPS origin、认证 issuer／audience 或专用服务凭据、调用者与测试授权的映射。未认证请求不能枚举账号；有效认证也不能超出账号、动作和期限。可采用有限 Bearer 认证；泄露的有效 token 在其授权范围内可能被使用，通过短期／有限 scope、撤销、日志脱敏和服务端校验控制影响。

外部 Agent 共享账号与调用合同，传输和可信认证由入口适配；具体客户端名称、版本、认证方式和工具集合写入实际验收记录。本地与远程两类入口各选择一个真实消费者验证，分别记录结果；一条链受阻时其他就绪实现可继续，整体完成仍需两类入口的实际验收。

<a id="dev-test-accounts-reuse"></a>
### 12.5 当前复用依据与缺口

源码核对基线为 `3bd96334c7ce3ad09111ed25d586afd1e20bab64`；下表证明复用位置，不证明新 profile 已运行。

| 位置 | 可复用能力 | 实施需处理的差距 |
| --- | --- | --- |
| `oasis7_client_storage/src/identity_store.rs`；`oasis7_game_client/src/identity_cli.rs` | 加密身份／角色存储、原生创建与登录入口 | 账号台账、一次配置／解锁体验、凭据引用和错误提示 |
| `runtime_broker.rs`；`device_client.rs` | resource 到独立 identity／vault／配置的映射及工具白名单 | 当前仅静态最多 16 项、一个启动 unlock，预算 fingerprint 包含整个配置；增改账号须与旧授权／用量解耦 |
| `executor_gateway.rs`；`identity_client.rs` | 真实账号会话核对、正式 API、类型与原操作恢复 | 新账号会话到既有调用链的实际接线及双账号验证 |
| `custody_tool_host.rs` 与旧 Worker 组件 | 私有交接、固定 HTTPS、加密和 DTO 等已实现部分 | 当前 Host 仅 Control；不得当作业务工具、远程外部 Agent 接入或本工具已完成 |

<a id="dev-test-accounts-compatibility"></a>
### 12.6 现有组件与凭据边界

本工具使用明确的认证 audience／入口版本和独立授权记录，只接受按当前账号工具合同建立的授权；其他用途或版本的凭据、批准和预算不能被转换为新授权。未知版本拒绝，认证失败不回退到较弱入口。账号登记由管理员核对真实身份及当前权限，未决操作始终按原账号与原操作查询。

现有组件的 wire、签名用途、权限校验及真实已发生操作仍由其实际实现解释；文档退役不放宽代码行为，也不产生自动迁移。需要追溯时查阅[旧设计固定版本](https://github.com/nil-studio/oasis7/blob/3bd96334c7ce3ad09111ed25d586afd1e20bab64/doc/p2p/blockchain/hosted-public-join-managed-identity-custody.design.md#cloud-test-custody-design)。其未实现的扩张目标已退出本工具计划，不继续维护为另一套待交付设计。

<a id="dev-test-accounts-verification"></a>
### 12.7 验证与交付判定

使用[开发测试账号工具验收方法](../../testing/manual/agent-runtime-self-playtest.manual.md#dev-test-accounts-validation)逐项覆盖 AC-DEV-01～06。先验证实际接入和两账号最小读取／写入路径，再覆盖并发隔离、丢响应、重启、停用、错误账号／环境及旧凭据拒绝。普通合同／故障回归与真实 Agent 验收分别记录；不以组件通过、控制请求 accepted 或文档完成代替真实使用结果。

实现提供有限可配置的会话期限、累计调用／并发上限、请求大小和超时，并在所选部署验证拒绝行为及正常测试可完成；不足时由管理员明确建立新的授权配置。优先验证远程外部 Agent 接入、静态 broker 配置解耦和秘密隔离，再扩展工具集合。

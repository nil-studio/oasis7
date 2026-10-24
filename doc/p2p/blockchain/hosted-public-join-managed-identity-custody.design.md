# oasis7 hosted_public_join 托管身份、托管密钥与受限执行（设计文档）

- 对应需求文档: `doc/p2p/blockchain/hosted-public-join-managed-identity-custody.prd.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

审计轮次: 1

本设计的[Cloudflare 测试账号托管](#cloud-test-custody-design)是待实现的受控测试 lane；仅集中托管控制面，不迁移世界权威，不把 Workers Secrets 或 WebCrypto 视为 HSM。

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
| Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) | 已签发的 device session 可刷新、撤销或在恢复后重新签发；runtime 仍是 player/entity 绑定真值。 | [Runtime / Viewer 对接原则](#9-runtime-viewer-对接原则) | runtime session authority 与 viewer | 本地 issuer 单测不证明 hosted 部署或完整设备丢失/账户恢复 runbook。 |
| 产品 N/A：reason=external-wallet bind、cooldown 与 transfer-out 是尚未交付的专业资产 custody 路径，当前产品要求不承诺该升级功能；scope=self-custody migration、cooldown 与 transfer-out；[托管身份专业要求](hosted-public-join-managed-identity-custody.prd.md#2-user-experience-functionality) | 自托管升级必须通过显式 external-wallet bind 或 transfer-out 路径，并避免把托管私钥回传浏览器。 | [实现顺序](#10-实现顺序) | runtime/account 与资产转移 owners，具体交付 owner 待任务绑定 | self-custody bind、cooldown 与 transfer-out 仍属后续阶段。 |
| Trace refs: O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b；[REQ-ENTRY-CANDIDATE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-candidate-001) | 对外阶段必须继续表述为 `limited playable technical preview`，不能将设计目标当作已投产 hosted wallet。 | [当前阶段口径](#11-当前阶段口径) | producer/system review 与 claims review | 本设计不授权 production launch 或更改当前阶段。 |
| 产品 N/A：reason=分层 SMTP/store/signer/approval secret 隔离是部署与运维证据，不构成单独玩家体验承诺；scope=dev、staging、production 环境间的 hosted identity/custody 配置与数据隔离；[Hosted custody 技术要求](hosted-public-join-managed-identity-custody.prd.md#3-technical-requirements) | dev、staging、production 仍须隔离 SMTP、account store、signer/approval secrets、风控阈值和对外 claims；当前阶段记录不能替代分层部署证据。 | [当前阶段口径](#11-当前阶段口径) | hosted identity/custody deployment owner，须由分层交付任务绑定 | 本文没有 staging/production 隔离的 live evidence，也不据此声称环境边界已验证。 |
| Trace refs: O7-REQ-844f4b30-b7d5-4651-941e-17c21f37b4ac；[REQ-CLOUD-CUSTODY-001](hosted-public-join-managed-identity-custody.prd.md#req-cloud-custody-001)  产品 N/A：reason=本节是受控测试账号运维服务的专业要求，不新增玩家功能或公开发行承诺；scope=Cloudflare 测试账号托管与受限执行；| 多账号台账只含非秘密引用，精确账号隔离且 projection 不作授权源。 | [Cloudflare 条款](#cloud-custody-data) | blockchain_ops_engineer / runtime_engineer | 测试服务待实现；不证明部署、生产资产或治理托管。 |
| Trace refs: O7-REQ-b799d81b-eafb-44e8-a0df-fda202d5d133；[REQ-CLOUD-CUSTODY-002](hosted-public-join-managed-identity-custody.prd.md#req-cloud-custody-002)  产品 N/A：reason=本节是受控测试账号运维服务的专业要求，不新增玩家功能或公开发行承诺；scope=Cloudflare 测试账号托管与受限执行；| 管理登录、owner 批准与 executor 执行分用途、分部署与秘密；不扩为生产 custody。 | [Cloudflare 条款](#cloud-custody-boundaries) | blockchain_ops_engineer / runtime_engineer | 测试服务待实现；不证明部署、生产资产或治理托管。 |
| Trace refs: O7-REQ-f837065c-4da5-4e6f-8a53-f4e5900cf26c；[REQ-CLOUD-CUSTODY-003](hosted-public-join-managed-identity-custody.prd.md#req-cloud-custody-003)  产品 N/A：reason=本节是受控测试账号运维服务的专业要求，不新增玩家功能或公开发行承诺；scope=Cloudflare 测试账号托管与受限执行；| 短期能力受范围/期限/共享预算约束，类型化 API 与现有 Ed25519 canonical wire 兼容。 | [Cloudflare 条款](#cloud-custody-api) | blockchain_ops_engineer / runtime_engineer | 测试服务待实现；不证明部署、生产资产或治理托管。 |
| Trace refs: O7-REQ-26945890-45d8-4be4-81af-5fe82c85aede；[REQ-CLOUD-CUSTODY-004](hosted-public-join-managed-identity-custody.prd.md#req-cloud-custody-004)  产品 N/A：reason=本节是受控测试账号运维服务的专业要求，不新增玩家功能或公开发行承诺；scope=Cloudflare 测试账号托管与受限执行；| 持久原操作、unknown 恢复以及 quiesce/Stop/Revoke 分开判定，父撤销阻断派生。 | [Cloudflare 条款](#cloud-custody-state) | blockchain_ops_engineer / runtime_engineer | 测试服务待实现；不证明部署、生产资产或治理托管。 |
| Trace refs: O7-REQ-b67f1215-00f3-4fbd-b8df-1c2ead4b3945；[REQ-CLOUD-CUSTODY-005](hosted-public-join-managed-identity-custody.prd.md#req-cloud-custody-005)  产品 N/A：reason=本节是受控测试账号运维服务的专业要求，不新增玩家功能或公开发行承诺；scope=Cloudflare 测试账号托管与受限执行；| 恢复先冻结并核对安全代、撤销和原操作；轮换与回退不复活旧授权。 | [Cloudflare 条款](#cloud-custody-operations) | blockchain_ops_engineer / runtime_engineer | 测试服务待实现；不证明部署、生产资产或治理托管。 |

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

本节约束生产 player custody 目标；[Cloudflare 测试 lane](#cloud-test-custody-design)只在无生产资产/治理的明确范围内使用 Worker 可信持钥，不构成本节生产托管能力的证明。
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
| Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) | [Runtime / Viewer 对接原则](#9-runtime-viewer-对接原则) | device session refresh/release 与撤销状态必须维持 player-session authority；runtime 继续维护 player/entity binding。 | [测试分层手册](../../../testing-manual.md)规定 required-tier 选择；现有 ID：`crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_player_session_tests.rs::hosted_player_session_refresh_rotates_registration_grant_for_new_browser_key`、`::hosted_player_session_ledger_recovers_lease_rate_and_revocation_after_restart`；本地 Rust 单元层，`test_tier_required`。 | 对应实现任务的 Issue evidence 与冻结候选单测结果。 | 不证明真实 hosted runtime 的账户恢复流程或 LiveOps device-loss 演练。 |
| 产品 N/A：reason=external-wallet bind、cooldown 与 transfer-out 是尚未交付的专业资产 custody 路径，当前产品要求不承诺该升级功能；scope=self-custody migration、cooldown 与 transfer-out；[托管身份专业要求](hosted-public-join-managed-identity-custody.prd.md#2-user-experience-functionality) | [实现顺序](#10-实现顺序) | transfer-out/self-custody 仅在有 external wallet bind、冷却与审计回归后可视为验证。 | N/A; reason=external wallet bind 与 transfer-out 当前未实现且没有匹配测试或操作手册; scope=self-custody migration、cooldown 与 transfer audit; owner_role=runtime_engineer; evidence_ref=hosted-public-join-managed-identity-custody.design.md#11-当前阶段口径; re-evaluate=external wallet bind 或 transfer-out 首次实现时 | 当前阶段声明；未来对应实现任务 Issue evidence 与测试/审计工件。 | 不证明外部账户绑定、资产迁移或托管退出已可用。 |
| Trace refs: O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b；[REQ-ENTRY-CANDIDATE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-candidate-001) | [当前阶段口径](#11-当前阶段口径) | 文档与对外 claims 应保留当前 preview 阶段语义，不将目标架构描述成已发布能力。 | N/A; reason=阶段与 claims 是本设计的专业文档判定且没有匹配的自动测试或操作手册; scope=仓内及对外 hosted wallet/custody claims; owner_role=producer_system_designer; evidence_ref=hosted-public-join-managed-identity-custody.design.md#11-当前阶段口径; re-evaluate=阶段或 claims 发生变化并进入独立 review 时 | 本设计阶段记录与对应任务的专业 review evidence。 | 文档结构检查不证明真实服务部署或 release readiness。 |
| 产品 N/A：reason=分层 SMTP/store/signer/approval secret 隔离是部署与运维证据，不构成单独玩家体验承诺；scope=dev、staging、production 环境间的 hosted identity/custody 配置与数据隔离；[Hosted custody 技术要求](hosted-public-join-managed-identity-custody.prd.md#3-technical-requirements) | [当前阶段口径](#11-当前阶段口径) | dev、staging、production 之间的 SMTP、account store、signer/approval secret、风控阈值和 claims 隔离必须由分层环境证据证明。 | N/A; reason=仓内没有覆盖三层 hosted identity/custody 服务隔离的精确测试或手册; scope=staging/production data plane 与独立环境配置; owner_role=blockchain_ops_engineer; evidence_ref=hosted-public-join-managed-identity-custody.design.md#11-当前阶段口径; re-evaluate=分层部署任务提供独立环境验证证据时 | 未来分层部署任务 Issue evidence 与 staging/prod smoke artifact。 | 当前本地单测和文档检查不证明真实环境隔离。 |
| Trace refs: O7-REQ-844f4b30-b7d5-4651-941e-17c21f37b4ac；[REQ-CLOUD-CUSTODY-001](hosted-public-join-managed-identity-custody.prd.md#req-cloud-custody-001)  产品 N/A：reason=本节是受控测试账号运维服务的专业要求，不新增玩家功能或公开发行承诺；scope=Cloudflare 测试账号托管与受限执行；| [Cloudflare 条款](#cloud-custody-data) | 多账号台账只含非秘密引用，精确账号隔离且 projection 不作授权源。 | N/A; reason=Cloudflare 测试服务尚未实现，没有匹配的 Worker 测试或运维手册; scope=REQ-CLOUD-CUSTODY-001 的 CT-001 正反例/故障验收; owner_role=blockchain_ops_engineer; evidence_ref=hosted-public-join-managed-identity-custody.design.md#cloud-custody-validation; re-evaluate=首次实现并执行 CT-001 时 | 实现 PR 的测试输出、原请求权威回执和安全审读。 | 本次只提交设计，不证明测试通过或生产可用。 |
| Trace refs: O7-REQ-b799d81b-eafb-44e8-a0df-fda202d5d133；[REQ-CLOUD-CUSTODY-002](hosted-public-join-managed-identity-custody.prd.md#req-cloud-custody-002)  产品 N/A：reason=本节是受控测试账号运维服务的专业要求，不新增玩家功能或公开发行承诺；scope=Cloudflare 测试账号托管与受限执行；| [Cloudflare 条款](#cloud-custody-boundaries) | 管理登录、owner 批准与 executor 执行分用途、分部署与秘密；不扩为生产 custody。 | N/A; reason=Cloudflare 测试服务尚未实现，没有匹配的 Worker 测试或运维手册; scope=REQ-CLOUD-CUSTODY-002 的 CT-002 正反例/故障验收; owner_role=blockchain_ops_engineer; evidence_ref=hosted-public-join-managed-identity-custody.design.md#cloud-custody-validation; re-evaluate=首次实现并执行 CT-002 时 | 实现 PR 的测试输出、原请求权威回执和安全审读。 | 本次只提交设计，不证明测试通过或生产可用。 |
| Trace refs: O7-REQ-f837065c-4da5-4e6f-8a53-f4e5900cf26c；[REQ-CLOUD-CUSTODY-003](hosted-public-join-managed-identity-custody.prd.md#req-cloud-custody-003)  产品 N/A：reason=本节是受控测试账号运维服务的专业要求，不新增玩家功能或公开发行承诺；scope=Cloudflare 测试账号托管与受限执行；| [Cloudflare 条款](#cloud-custody-api) | 短期能力受范围/期限/共享预算约束，类型化 API 与现有 Ed25519 canonical wire 兼容。 | N/A; reason=Cloudflare 测试服务尚未实现，没有匹配的 Worker 测试或运维手册; scope=REQ-CLOUD-CUSTODY-003 的 CT-003 正反例/故障验收; owner_role=blockchain_ops_engineer; evidence_ref=hosted-public-join-managed-identity-custody.design.md#cloud-custody-validation; re-evaluate=首次实现并执行 CT-003 时 | 实现 PR 的测试输出、原请求权威回执和安全审读。 | 本次只提交设计，不证明测试通过或生产可用。 |
| Trace refs: O7-REQ-26945890-45d8-4be4-81af-5fe82c85aede；[REQ-CLOUD-CUSTODY-004](hosted-public-join-managed-identity-custody.prd.md#req-cloud-custody-004)  产品 N/A：reason=本节是受控测试账号运维服务的专业要求，不新增玩家功能或公开发行承诺；scope=Cloudflare 测试账号托管与受限执行；| [Cloudflare 条款](#cloud-custody-state) | 持久原操作、unknown 恢复以及 quiesce/Stop/Revoke 分开判定，父撤销阻断派生。 | N/A; reason=Cloudflare 测试服务尚未实现，没有匹配的 Worker 测试或运维手册; scope=REQ-CLOUD-CUSTODY-004 的 CT-004 正反例/故障验收; owner_role=blockchain_ops_engineer; evidence_ref=hosted-public-join-managed-identity-custody.design.md#cloud-custody-validation; re-evaluate=首次实现并执行 CT-004 时 | 实现 PR 的测试输出、原请求权威回执和安全审读。 | 本次只提交设计，不证明测试通过或生产可用。 |
| Trace refs: O7-REQ-b67f1215-00f3-4fbd-b8df-1c2ead4b3945；[REQ-CLOUD-CUSTODY-005](hosted-public-join-managed-identity-custody.prd.md#req-cloud-custody-005)  产品 N/A：reason=本节是受控测试账号运维服务的专业要求，不新增玩家功能或公开发行承诺；scope=Cloudflare 测试账号托管与受限执行；| [Cloudflare 条款](#cloud-custody-operations) | 恢复先冻结并核对安全代、撤销和原操作；轮换与回退不复活旧授权。 | N/A; reason=Cloudflare 测试服务尚未实现，没有匹配的 Worker 测试或运维手册; scope=REQ-CLOUD-CUSTODY-005 的 CT-005 正反例/故障验收; owner_role=blockchain_ops_engineer; evidence_ref=hosted-public-join-managed-identity-custody.design.md#cloud-custody-validation; re-evaluate=首次实现并执行 CT-005 时 | 实现 PR 的测试输出、原请求权威回执和安全审读。 | 本次只提交设计，不证明测试通过或生产可用。 |


<a id="cloud-test-custody-design"></a>
## 12. Cloudflare 测试账号托管与受限执行系统设计

### 12.1 问题、目标与非目标

本节承接[测试账号托管 PRD](hosted-public-join-managed-identity-custody.prd.md#cloud-test-custody)，将多账号元数据、登录认证、owner 控制意图与受限 executor 执行分开，使助手无需接触长期私钥仍能完成获准测试动作。owner roles：`blockchain_ops_engineer`（custody/运维）、`runtime_engineer`（协议/权威结果），`producer_system_designer`（体验/claims）。

首版是待实现设计，默认关闭。仅用于明确批准的受控测试账号/世界；不支持主网资产、validator、node、governance/treasury、任意 URL/raw signing、生产钱包承诺或无限自治。Cloudflare 只承载身份托管与动作控制面，不能成为 world state、owner 归属、游戏资源、共识或 finality 的第二 authority。引用既有协议而不另造世界 operation 状态机。

### 12.2 上游约束与相关角色

需求逐项映射在 §2.1 与 §11.1；本节的本地运行状态仅解释托管侧处理。下游合同继续由以下来源持有：

- [多凭证身份](hosted-public-join-key-based-onboarding.design.md#webauthn)与[机器注册](hosted-public-join-key-based-onboarding.design.md#machine-registration)：账户登录、明确注册能力、认证证明和账户恢复。
- [Game API](../../world-simulator/llm/external-agent-game-api.design.md)：owner/executor fence、原操作、持久查重与受限恢复；[Runtime adapter](../../world-simulator/llm/external-agent-runtime-adapter.design.md)与[可信入口](../../../skills/oasis7-play/references/identity-entry.md)：模型只见受限工具，不见 unlock、配置或 vault。
- [正式网络层级](formal-network-tiers-testnet-mechanism.runbook.md)：环境/网络层级与公开 claims，不由 Worker 配置推导主网准入。
- Cloudflare 官方：[WebCrypto](https://developers.cloudflare.com/workers/runtime-apis/web-crypto/)、[SQLite-backed DO storage](https://developers.cloudflare.com/durable-objects/api/sqlite-storage-api/)、[DO 并发规则](https://developers.cloudflare.com/durable-objects/best-practices/rules-of-durable-objects/)、[Worker secrets](https://developers.cloudflare.com/workers/configuration/secrets/)。这些 API 能力是实现约束，不替代本服务隔离、签名或恢复验收。

<a id="cloud-custody-baseline"></a>
### 12.3 当前状态、目标状态与差距

固定审读基线为 `nil-studio/oasis7@73398d454082eb2106b2f1075a57bc9f377cd222`（2026-10-24）；只核对仓内代码与合同，没有云端部署、账号接入、secret 读取或实际运行。

| 对象 | 当前基线 | 目标/差距 |
| --- | --- | --- |
| 模型工具 broker | `crates/oasis7_game_client/src/runtime_broker.rs` / `runtime_broker_cli.rs` 是 Unix socket，Runtime 白名单为 codex/openclaw，私有解锁留可信本地进程 | 不能直接在 Worker 中启动 Unix broker、CLI 或本地 vault；新 HTTP 类型化 adapter 待实现，现有两个 Runtime 支持范围不扩张 |
| Login/owner/executor | `identity_store.rs`、`entry_owner_client.rs`、`executor_gateway.rs` 已隔离用途；owner 请求先保存原意图后联网 | 云端分 Worker 密钥、批准、预算及 outbox 待实现；不自动搬运现有 vault |
| 世界协议 | `oasis7_client_api` / `oasis7_agent_api` 已定义签名请求、完整 task fence、委托和回执 | Workers 必须复用同一字节与语义，不能把 JSON 可解析当 wire 兼容 |
| Hosted custody | 本文 §11 明确正式 managed custody sign、self-custody 与冻结恢复缺口 | 新 Cloudflare 测试 lane 不补写为已有生产能力 |
| 实现/验收 | 无本节服务、云端签名向量、故障恢复及真实受控账号证据 | PRD 五项功能均未实现；本次检查仅证明文档合同/关系 |

<a id="cloud-custody-boundaries"></a>
### 12.4 边界与结构

三类 Worker 必须分别部署、独立 ingress/service binding、发布身份与 secret 绑定。不能仅把一个含全部秘密的 Worker 拆成不同 URL，就宣称隔离完成。

| 组件 | 可持有/可做 | 不可做与信任边界 |
| --- | --- | --- |
| Management UI + Identity Worker | Passkey challenge/登录、凭证安全版本与管理会话；仅自己的 login key wrapping secret 与身份协议适配 | 不持 owner/executor wrapping secret，不把登录 session 当协议 owner 签名；不从模型参数决定账户 ACL |
| Owner Control Worker | 新鲜操作批准、owner typed intent/sign/send；按账号托管 owner key；管理 grant/Stop/Revoke | 不暴露通用签名或私钥；不能从 executor capability 推导 owner 批准 |
| AccountAuthority DO（Owner 服务私有 namespace） | 每账号唯一权威：账号管理 ACL/批准消费、授权树/撤销版本、预算、请求/outbox 状态、账户冻结及安全代；受限 RPC 为 executor 做原子 admission/claim/settle | D1、缓存和调用者不能覆盖它；executor binding 只暴露 typed admission/claim/result，不能调用 owner sign/grant/恢复方法 |
| Executor Worker | 仅持 executor wrapping secret 和获准账号的 executor key ciphertext；签类型化执行请求、经固定 adapter 发送/回读 | 没有 owner/login secret、grant 创建/续期权限或任意网络代理；不跳过世界当前委托/fence |
| Recovery Fence（独立 namespace/发布权限） | 持久 service epoch、执行禁用及恢复 tombstone，防止账号快照恢复旧服务代 | 仅增加拒绝条件，不授予账号权限/预算；不得与账号 DO 一起回滚 |
| D1 / 按需 R2 | D1 可重建列表/检索投影；R2 受限归档和加密备份 | 均不判断 active grant、撤销、剩余预算或原请求可重发；不存明文密钥/token |
| oasis7 identity/world service | 校验独立协议身份、current owner/executor、完整 fence、业务预算及 canonical 结果 | 不信任托管页面的成功状态、D1 投影或模型意图 |

每次出站写都需要 Account DO 最新 admission 与当前 Recovery Fence；二者不可用则拒绝派发。跨 Worker binding 只授予必要窄接口，并校验服务角色、account、intent 类型与版本；网络可达或持有 binding 不等于能够 owner 出签。权限/部署测试必须证明 Executor 身份无 Owner/Identity 发布权和秘密读权。

Worker 平台、被授权发布者及在运行时处理私钥的代码仍属于可信计算基。Workers Secrets 是 secret 绑定，不是不可导出密钥设备；WebCrypto `extractable=false` 不消除导入前明文、恶意部署、同服务代码或平台风险。分账号密钥缩小正常误操作范围，无法抵御所有共享平台管理员攻击。不能对外宣称“私钥永不离开 HSM”或“云端不可见”。

### 12.5 关键运行流程

1. **建档/接入**：管理者先登录，只读发现账号/世界能力。明确批准新的测试账号创建或已有账号接入，选择环境与用途后，可信服务内部生成三套独立 key。身份注册、owner 绑定、executor 委托分别按现行协议执行并核对原回执。未获准注册、重复绑定、缺前置或结果未知时保持冻结，不换账号绕过。
2. **Owner 批准**：固定 account、world/genesis/branch/reorg、Agent/task、类型化动作摘要、预算与期限，取得绑定该摘要的新鲜 Passkey 证明，Identity authority 在自己的事务中重验会话、凭证安全版本并消费挑战，签发短时一次性批准证明；Account DO 在自己的事务中重读账号管理 ACL、安全代与账户状态并消费该证明。两者不是一个事务，交接及撤销竞态按下文处理。Owner Worker 生成并持久化真实 owner 签名原件。Passkey assertion 不被当成 Ed25519 owner 签名。
3. **发能力**：仅在下游 owner/executor 绑定已确认且本服务 grant 已持久化后发 capability。客户端凭据经私有通道交付受限客户端，模型/日志只见 capability handle；token 丢失重新认证，不从审计回传 token。
4. **动作执行**：输入 schema/目标/参数初检；DO 原子核权、占用预算、写 request/outbox；适配器生成既有协议原件并保存；派发前再核 grant/epoch/fence/期限，按同一原件发送；响应必须与原操作、固定 service key 和世界身份匹配后落 receipt。只接收 HTTP 2xx 不表示世界提交。
5. **失败/取消**：未派发且确定没有副作用可取消并释放业务预留；处理请求次数仍计费于请求预算。发送可能发生则标记 `unknown`、保留预留并核对原操作，不换 nonce/operation ID。父撤销与冻结阻断排队项，已经 claim 的项即使尚未实际 fetch，也一律按 in-flight/unknown 保留待核对。
6. **停止/撤销**：先 `quiesce` 本服务新派发并撤销相应 capability；Owner Worker 分别按既有 `GameExecutorOperationV1::Stop` 与 `AgentSignerDelegationChangeV1 { revoked: true }` 生成原操作。Stop 与 Revoke 可分别失败/未知，只有匹配的权威回执/当前 fence 或 delegation 确认才升级各自状态。只读恢复走既有 owner `LookupExecutor` 等受限路径，不恢复 executor 写权限来查询。

**认证到账号授权的交接：** Identity authority 拥有 Passkey/会话/凭证撤销，Account DO 拥有账号 ACL、custody security epoch 与批准消费。批准证明由固定 Identity service key 签发，绑定 audience=该 Account DO、管理者、credential ID/security version、account/environment、typed intent digest、一次 nonce、签发/到期（最长 30 秒）及要求的 Account DO policy/security version。Owner Worker 在事务外完成验签/最终在线核验；Account DO 事务只消费经认证窄 RPC 交付的、完整摘要绑定的证明，不读取网络、不信任客户端自报版本或 D1 缓存。证明/在线核验失败、过期、重复、版本不符或来源不可用均拒绝。

Identity 撤销的线性化点只阻止新证明签发，不能使另一个 DO 已消费的批准自动回滚；撤销与已签发证明存在至多 30 秒的授权交接窗口。管理端须将 identity revoked 与 custody authorization closed 分开显示：撤销时必须向所有关联 Account DO 发冻结/升安全代，撤销该管理者/凭证来源的全部 root/children、阻断未 claim 请求并取得确认，再核对已 claim 在途原操作，才显示后者。不能以批准证明过期替代已有根授权/能力撤销；即使暂时没有在途请求，也必须完成账号侧撤销。Identity 撤销后不再签发新批准证明；尚未确认冻结的账号仍可能消费已签发证明或使用已有 grant，所以保持 custody pending/unknown，不能宣称全局立即阻断。已消费/已claim 请求按原操作核对，不能宣称立刻追回。若实现无法证明这一边界、关联账号全集或最终核验，禁用 owner 批准而不是只靠短TTL声称安全。

<a id="cloud-custody-api"></a>
### 12.6 接口、能力与签名合同

下列 `/v1` 是新托管服务的目标接口，不是现有游戏 API。输入/输出均类型化、拒绝未知字段，固定 schema revision；适配器只可调用版本化允许列表，不接受调用者 URL、header、host、path、method 或 actor 覆盖。

| 接口 | 调用方 → 服务 | 身份、顺序与结果 |
| --- | --- | --- |
| `GET /v1/accounts`、`GET /v1/accounts/{id}` | 管理会话 → Identity/Owner | 从主体 ACL 筛选非秘密投影；标记 projection version/stale，不返回 key/token；读详情重核权 |
| `POST /v1/accounts/{id}/owner-intents` | 管理端 → Owner | 新鲜 Passkey 证明绑定完整 typed intent 与 expected policy/security version；固定 idempotency key；返回本地受理或权威结果引用 |
| `POST /v1/accounts/{id}/grants` | 管理端 → Owner | 一次批准、独立 executor 与已核对世界绑定；创建有界根授权，不消费 executor token；增权/延时需新批准 |
| `POST /v1/actions` | 受限客户端 → Executor | capability + client proof + request ID + typed action；DO 绑定精确账号和原摘要；202 仅表示受理 |
| `GET /v1/operations/{id}` | 当前获准主体 → 对应服务 | 仅回读原操作非秘密状态/结果；读权限独立核对，旧写能力不能重新发送 |
| `POST /v1/accounts/{id}/quiesce`、`/stop`、`/revoke` | 管理端 → Owner | 本地停派发、canonical Stop、delegation Revoke 分端点/operation ID/状态；不以一个动作的结果代替另一个 |

capability 为不可猜测 opaque 凭据，其 digest 与客户端公钥绑定，原值不落 D1/日志；客户端通过标准化 proof-of-possession 绑定请求方法、规范路径、正文摘要、服务器挑战及有效期，重放挑战一次消费。凭据在助手 Prompt 中出现不应成为默认接线；若运行端不能保密/完成持有证明，保持不支持而不是改发 owner key。该客户端证明协议、测试向量及会话接线必须在实现 PR 冻结后才可开放，不能宣称现有 dot 插件已支持。

根 grant 必含：`grant_id/root_grant_id/parent_id`、`origin_manager_id/origin_credential_id/origin_credential_security_version`、`account_id/environment/service_epoch`、`client_key_id`、完整 world identity、`agent_id/task_id`、owner/delegation generation 与 task/executor fence、action 枚举、参数约束、allowed resource refs、累计请求/业务预算、`not_before/expires_at/root_absolute_expires_at`、`policy_version/revocation_version`。首版不允许客户端转授；如服务生成内部子能力，其深度最多一层、权限/预算只能缩小且共享根预算，绝对期限不得超父/根。无 refresh 自续期，无创建下一根 grant 的 capability。

下游签名保持当前代码合同：`crates/oasis7_client_api/src/world_service/authority.rs` 的 canonical CBOR tuple `("oasis7.world-service.v1", domain, value)` 与 BLAKE3 request digest；各 typed request 继续使用其自己的 domain，不能换成 JSON stringify 或新 HTTP token 签名。`GameExecutorControlV1`/task fence 与 `AgentSignerDelegationChangeV1` 等请求的字段、整数、nonce/sequence、generation、world/genesis/branch/reorg 均按原 schema。

Workers 使用标准 `Ed25519` WebCrypto，不使用 legacy `NODE-ED25519`。以固定兼容日期/运行时和仓库 Rust 版本验证：32-byte seed/PKCS#8 导入、公钥 raw 编码、64-byte signature、原 canonical bytes/digest 双向完全一致；所有 u64 必须无损，明确区分 CBOR unsigned integer 与 `DecimalU64` 字符串，禁止经 JS Number 丢失高位。除正常向量，必须覆盖大于 2^53、u64 最大值/溢出、Unicode、map 顺序、空/缺字段、错误签名域、错误 key/weak key、错误 fence 及过期/重放。适配层若不能产生完全相同 bytes 就阻断该动作，不新造兼容 fallback。

服务无 `get_secret`、`export_key`、`sign(rawBytes)`、`fetch(url)`、任意命令/文件 API。Owner 意图采用单独枚举及审批策略；executor 允许列表只包含当前协议和完整 fence 明确允许的测试动作，不自动开放资产/治理/账号恢复操作。外部目标通过环境固定 route ID 映射到 HTTPS origin/service key，拒绝重定向、跨 origin、未登记服务和 SSRF 私网/metadata 目标；主动测试网内网 endpoint 只能是部署侧逐一批准的精确 pin，不能由模型传入。

<a id="cloud-custody-data"></a>
### 12.7 数据、事务与持久化

每个 `(environment, account_id)` 映射唯一 AccountAuthority DO，使用 SQLite-backed storage。账号 DO 是授权、撤销版本、预算与本服务请求的唯一权威；Cloudflare 的对象串行执行并不阻止在外部 `await fetch` 期间发生交错，因此所有安全判断与状态转移仍须事务内条件更新，不依赖进程内布尔量。[官方存储与并发约束](https://developers.cloudflare.com/durable-objects/api/sqlite-storage-api/)

| 记录 | 最小字段/持有方 | 原子性与生命周期 |
| --- | --- | --- |
| `AccountRecord` | account/environment、用途标签、公开 login/owner/executor key refs、world/Agent/task refs、status/security_epoch/policy_version | DO 权威；非秘密投影可发 D1；删除标签/投影不删除撤销事实 |
| `KeyEnvelope` | role/key_id/key_version、algorithm、ciphertext、nonce、AAD、wrapping_key_version | 按角色 namespace；AES-GCM 新随机 nonce，AAD 绑定环境/账号/角色/key version；用途 Worker secret 才可解密，禁止交换角色 |
| `OwnerApproval` | 管理者/credential security version、intent digest、challenge、expiry、consumed operation | 一次消费；不可复用于另一账号、动作、参数或授权范围 |
| `Grant` / `Budget` | 上述 capability 字段、token digest、revoked_at、reserved/consumed、version | 同一账号 DO 内共享 root 预算；issue/admit/revoke/settle 条件事务，拒绝欠额/溢出 |
| `Operation` / `Outbox` | request ID、request digest、typed body、原签名/nonce/correlation、service/world pin、state、attempt/lease/epoch、receipt | 原件先持久化再联网；相同 ID 不同 digest 冲突；同 request 重试不重记业务扣费，不换 actor/nonce |
| `AuditEvent` / `ProjectionOutbox` | account sequence、actor/action、非秘密摘要、policy/key/security version、结果引用 | 与授权/预算/请求提交同事务；异步投影按 account sequence 幂等，缺序不伪造完整 |
| `RecoveryFence` | environment service epoch、disabled、恢复事件与墓碑 | 独立于账号备份；只收紧执行，不保存可恢复的 active grant 或预算 |

私钥只在对应 Worker 生成/解密/签名的短生命周期内使用，不返回 dot、UI 或通用日志。秘密台账与元数据可在同一账号逻辑关联，但只能经不同的用途访问面读取。生产 key 不允许进入此服务；测试 key 创建/导入是独立授权动作。本版不自动导入旧 vault。密钥备份仅限经授权的加密恢复材料，wrapping secret 不与密文一起归档；否则不得声称加密备份形成独立保护。

D1 以 DO 事件流重建列表，延迟/丢失不影响授权结果；页面展示 stale 并回 DO 取当前状态，不以投影恢复 active。R2 可按需保存加密归档/完整性摘要，无归档需求时不引入。队列/alarm 只唤醒持久 outbox；重复投递按记录消费，不以一次 alarm 当唯一执行保证。

<a id="cloud-custody-state"></a>
#### 本服务请求状态与提交点

| 本地状态 | 含义与允许转移 | 禁止推论 |
| --- | --- | --- |
| `received` | 尚未 admission；可拒绝 | 不表示授权或世界受理 |
| `admitted` / `prepared` | 权限检查、预算预留、原件/outbox 已落盘；可在最终重验后 claim | 不表示已发送或世界提交 |
| `dispatching` | 事务 claim 绑定 epoch/attempt/原件；发送开始或可能开始 | 崩溃/lease 到期不能当未发送 |
| `unknown` / `reconciling` | 查询原请求和权威状态；保留预留，暂停有依赖的后续写 | 不能换新 nonce/ID、释放预算或恢复旧 executor |
| `confirmed` | 验证过的原权威回执；业务 accepted/rejected/effect 分字段保留 | world committed 不等于业务成功、交付或未来持续能力 |
| `rejected` / `cancelled_before_dispatch` | 明确 admission 失败或确定未派发；释放适用业务预留 | 不覆盖发送可能发生的未知请求 |
| `recovery_required` | 缺权威历史、pin/fence 冲突或安全代不明；仅受限核对 | 不能自动重启任务或重建原请求 |

事务 A 同时完成最新 grant/父撤销/时间/账户核验、幂等冲突判断、预算预留、Operation、outbox 和审计；事务中不调用网络或密码学异步 API。必要签名在事务外有界计算，事务 B 重新核验状态与签名原件摘要后保存为 prepared。最终 claim 再检查父/根版本、security/service epoch、期限、最新已验证 task fence 与预留，保存 dispatching。非原子阶段的任何版本变化都使计算结果失效，不能覆盖撤销。

下游 HTTP 不与 DO 事务原子提交。恢复必须复用 Game API 既有原请求恢复合同：先查 durable original-operation receipt，确认 digest、原 actor、service/world pin、结果关联；只有下游明确保证该操作持久查重、恢复仍保留同一去重历史且当前授权允许时，才可发送原 bytes。单次 not-found、502、超时或 Worker lease 过期不是未执行证明。协议不提供可靠原请求查询/去重时，此动作在首版禁用或停在人工 recovery_required，不宣称“恰好一次”。

父/根撤销在 DO 事务增加版本并使全部派生 capability 和未 claim outbox 失效，不要求先逐行扫描所有子记录才能阻断。已经 claim 的请求仍在网络交接竞态内，本服务无法撤回已离开的 bytes；应立即 quiesce 并提交 canonical Stop/Revoke，由世界当前 fence/委托最终拒绝后续执行。Stop 与 Revoke 分别保留 operation ID、submitted/unknown/confirmed 和权威状态；Revoke 按既有合同严格递增 generation/nonce，同一原件重复仍按原操作处理。停止任务不自动撤销全部签名委托，撤销能力也不回滚过去世界结果。

预算是两层：本服务请求次数/资源上限与世界业务资源预算。DO 持久预留保证本服务并发不超支，世界权威仍独立核业务预算；不同账号没有隐含共享原子总额。未知请求保留最坏业务预留，只有权威拒绝/未执行或明确结算才释放/转消耗。HTTP 请求次数、重试成本与业务动作成本分开记录；budget 耗尽不能通过换 token/Worker 重置。

<a id="cloud-custody-operations"></a>
### 12.8 部署、安全与运维失败恢复

dev/staging 使用独立 Cloudflare Worker、DO/D1/R2 namespace、身份域、secret 和世界 pin；首版没有 production 配置。跨 Worker 发布权最小化，执行服务不能部署 Owner/Identity 或改 Recovery Fence。发布固定源码/构建/兼容日期，管理面与执行面不同 origin/路由；精确 Origin、Secure/HttpOnly/SameSite Cookie、CSRF 检查、WebAuthn RP 与一次性 challenge 沿现有身份合同。仅允许固定 typed CORS/绑定入口，不能允许任意网站带用户 session 调 owner 操作。

| 威胁 | 强制约束 | 剩余风险/失败动作 |
| --- | --- | --- |
| 模型被提示注入、工具参数恶意 | 类型化 allowlist、参数/actor/world 绑定、无任意 URL/raw sign；模型无长期秘密 | 合法范围内错误动作仍可能发生，使用小预算/短期限并审计 |
| 管理登录被盗、CSRF、假 Passkey | RP/origin/UV/challenge 校验，新鲜 intent-bound owner 批准，session/security version 重验 | 被盗已解锁设备仍有风险，冻结与撤销恢复不绕过 owner 审查 |
| Executor Worker/凭据泄露 | 独立 secret/部署权限、PoP、最小 grant、DO 预算/父撤销，世界 fence 再验 | 攻陷执行服务仍可使用有效 executor key，需世界 Revoke 与轮换，不能只删 HTTP token |
| Owner/平台/部署链攻陷 | 明确可信持钥、发布权限分离、密钥分用途、审计与紧急禁用 | 本设计不能消除平台或 owner 发布者提取/滥签风险，禁止生产资产 |
| D1 滞后/污染、重复队列、DO 重入 | DO 权威事务/条件状态、投影序号、稳定 outbox 原件 | 存储/查询故障时暂停，不降级读投影放行 |
| 旧备份/旧部署复活权限 | 独立 Recovery Fence、不回退 service epoch、默认冻结恢复、世界撤销/原件核对 | 无法证明 epoch/历史完整时永久保持 frozen/recovery_required，人工处理 |
| 日志/遥测/备份泄密 | 结构化字段 allowlist；禁请求体、Cookie/token、Passkey assertion、私钥及第三方会话录制 | 必要私密目标仅受控加密存储，审计摘要也受账号 ACL |
| 外部超时/重放/伪回执 | 原请求查重回读、固定 service key/世界 pin、签名结果关联 | 没有权威结果就保留 unknown，不凭 transport 成功升级 |

运行失败处理：

- **DO/Identity 不可用**：拒绝新授权/派发；可显示带新鲜度的投影，不报告账号可执行。D1/R2 失败只影响搜索/归档，不能扩权；审计 outbox 逼近上限时停止新写。
- **外部服务故障**：熔断指定 route/account、保留原请求并退避查询；每次查询也受预算/总截止限制，持续未知转人工核对，不静默重建账号或 task。
- **账户恢复/备份**：先禁用旧执行部署并增加独立 Recovery Fence 的 service epoch，终止旧写 lease；恢复到隔离 namespace，所有账号 frozen、旧 grants 不可用。读取当前身份安全版本、世界 owner/delegation/task 状态和每条非终态原操作；确认撤销历史与预算后才允许新 owner 批准新根 grant。epoch 来源/撤销历史丢失时保持冻结。恢复前还须隔离旧部署、绑定与发布凭据，冻结全部签名，并确认世界侧 canonical 撤销/轮换既有 key；无法确认旧授权或原请求时仍阻塞。service epoch 只是守约服务的自我约束，不能在密码学上使旧 Worker 已持有的 key 或已签请求失效；恶意旧代码/平台管理员仍属明确残余风险。账号 PITR 本身不能恢复当前授权正确性，Recovery Fence 不随账号 PITR 回滚。
- **并发旧写者**：新 epoch 禁旧服务再 claim，但无法撤回已经发送的 bytes；先完成下游 Stop/Revoke/fence 变化并回读，未知原件核完后才可接续。不能仅等待租约到期就认定旧写者没有副作用。
- **轮换**：分别轮换 login、owner、executor 与 wrapping key。重包密文不改协议公钥；协议密钥轮换必须走对应身份/owner/delegation 正式流程并确认世界状态。保留旧 key version 仅用于必要原件验证/受限查询，绝不以旧密钥新签写请求。轮换响应未知仍查原请求。
- **回退**：代码回退只能回到理解当前 schema/security epoch 的已验证版本；不降 schema、不回退预算/撤销、不恢复旧 secrets 或 grant。不能理解新状态时维持只读冻结，已提交世界效果不可回滚。

**冻结下的恢复控制通道：** 冻结禁止普通 login/owner 业务签名、executor 写入与创建/续期 grant，不关闭独立人工批准的最小恢复通道。该通道只允许固定账号/世界的 canonical Stop、delegation Revoke、协议明确允许的密钥轮换及必要原结果只读核对，重新做 intent-bound 身份验证、审计和当前权威检查；无玩法、转账、governance、通用签名或解冻自授权。恢复操作使用独立有界应急预算，不被已耗尽的执行预算锁死，也不允许无限重试。owner key/身份本身不可恢复或已不受信时保持阻塞，按既有世界/身份恢复权威处理，不能临时发明另一个 owner。

安全审计建议保留 90 天，运行诊断 30 天；账号退役冻结所有写后，保留最小撤销 tombstone、operation digest/原结果引用，直到下游持久查重/回读保留合同足以阻断重放。需要删除完整私密原件前，先核完 unknown 并销毁相关写能力；不得因 TTL 清理而把旧 ID 当全新可执行。任何具体保留例外与删除责任在实施运维合同中明确，R2 不是默认永久保存个人资料。

<a id="cloud-custody-quality"></a>
### 12.9 质量、容量与可观测性

以下是首版受控测试的设计上限/验收目标，非 Cloudflare 平台极限或实测承诺；阈值由实现任务在成本预算内验证，超限拒绝/冻结，不自动扩容或续费。

| 场景 | 环境、刺激/规模 | 预期响应与指标 | 验证/当前范围 |
| --- | --- | --- | --- |
| 多账号 | 单测试环境最多 100 账号；每账号 4 个 active root grant、1 个待确认 owner intent、8 个未决动作 | 列表分页且每账号强隔离；未决上限包含 unknown，达到上限拒绝新写 | CT-001/005，待实现 |
| 受限执行 | capability TTL 默认 5 分钟、根绝对期限最多 30 分钟；每根最多 100 次工具请求；可进一步缩小 | 到期判定使用服务时间并校验下游协议时限；无续期，无并发穿透 | CT-003，待实现；这些值不改变世界 lease/业务数值 |
| 请求/传输 | 请求至多 32 KiB；每账号最多 1 个写请求 in-flight；下游单次超时 10 秒，整体核对截止 2 分钟 | 大小超限拒绝；超时进入 unknown，截止后 recovery_required，后续人工可继续只读核对 | CT-004，待实现；当前 broker response 上限不被静默改写 |
| 控制面延迟 | staging 100 账号/20 并发请求、持续 10 分钟；不含 Passkey 用户耗时与世界提交 | admission p95 目标 <500 ms；单账号暂停提交后下一次 admission 拒绝；无成功吞错 | 实施 PR 提供分位数、地区/版本/样本，未测 |
| 审计/隔离 | malformed、跨账号、重复、超额、撤销/恢复故障注入 | 每安全转移有 account sequence；secret 扫描零原始秘密；失败原因可分类 | CT-001~005，待实现 |

监控记录 admission/deny、queued/dispatching/unknown 数、预算 reserved/consumed、回读延迟、projection lag、key/service/security version、最后成功下游核对；告警区分服务健康与世界效果。持续 unknown、审计堵塞、epoch 不一致或拒绝突增触发账户/route 冻结与 operator 提示；日志不得包含完整私有目标或 bearer 材料。

### 12.10 兼容、迁移与回滚

本节添加部署适配，不修改既有 protocol/wire、运行时支持名单、默认注册开关或 localhost broker 行为。当前 Unix broker 继续服务 codex/openclaw；Cloudflare 版本作为明确独立配置，不能把 Unix socket 路径配置成远端 URL，不能在缺配置时回退到本地 vault/全局 env key。

首版只接新建或明确逐账号授权接入的测试身份。先只读发现、固定 world/service/identity pin 和 schema，再验证 Workers/Rust 双向向量、只读回执与 owner 控制；最后在受控世界单账号、小预算启用 executor。所有阶段显式开关默认关闭。没有现有账号/世界证据时记录缺前置，不读取旧秘密寻找捷径。

现有本地身份不会因建云端台账改变 custody；迁移需先冻结原执行者、核对原 outbox、分别轮换/绑定新角色 key、撤销旧 delegation 并确认，再启用新的 grant。不得双端保留同一 executor key 自动活跃；旧私钥不通过 dot/chat/工单传输。自托管退出仍归既有 wallet-transition authority，本测试版不提供明文导出。

协议版本、canonical bytes、fixture 或世界 fence 改变时，先拒绝未支持组合并补适配/负例。D1 可删后重建，但 DO 操作记录/预算/撤销不通过 D1 回灌。兼容回退按 §12.8 保留当前安全代和世界已提交事实。

<a id="cloud-custody-validation"></a>
### 12.11 验证设计与可追溯性

以下 CT 是本设计规定的未来验收场景，不是已存在或已运行的测试名称。对应 PRD [AC-CLOUD-01~08](hosted-public-join-managed-identity-custody.prd.md#cloud-custody-acceptance)；当前 PR 仅执行文档治理、链接/Trace 关系与独立安全审读。实现 PR 必须固定 Rust/Workers 源版本、Cloudflare compatibility date、schema、真实 world/service pin、环境和观测窗口，并区分 fixture、本地模拟、Workers staging 与世界权威回执。

| 场景 ID | 正例、负例与故障切点 | 要核对的实际结果 | 覆盖 AC |
| --- | --- | --- | --- |
| CT-001 | 2 管理者/多账号切换、跨账号 ID 替换、D1 延迟/污染、日志/备份检查 | 只能读取/操作自身账号；D1 丢失可重建但不改变授权；无长期秘密泄漏 | AC-CLOUD-01 |
| CT-002 | Passkey 正常/错误 RP/origin/UV、stale approval、executor token 调 owner route、错误角色 key/发布身份；证明已消费发出 grant、当前无在途，再撤销凭证但未收齐 DO 确认 | login 不等 owner；一次批准绑定精确摘要；Executor 无 owner secret/发布/调用能力；等待证明过期不能显示 custody closed，账号侧撤销后旧 capability 不能新 claim | AC-CLOUD-02 |
| CT-003 | 并发使用同根预算、父撤销、过期、递归派生/续期、跨 world/Agent/fence；Rust→Worker 与 Worker→Rust 全量 golden vectors | 权限只能收缩且不超支；canonical CBOR/digest/signature 精确一致；不支持组合 fail closed | AC-CLOUD-03/04 |
| CT-004 | admission、保存原件、claim、下游提交、回执落盘各切点杀进程/断网；同 ID 同/异 body；Stop/Revoke 分别丢响应 | 原操作只有可证明的原结果；unknown 不换 nonce 重发；quiesce/Stop/Revoke 三态独立；新 executor 只读原件不提交旧 actor 请求 | AC-CLOUD-05/06 |
| CT-005 | 恢复撤销前备份、旧 Worker 尚存、Recovery Fence 不可用、wrapping/protocol key 轮换、代码回退、预算/审计耗尽 | 恢复默认冻结、旧 epoch/grant 不能派发、世界撤销核对与新批准后才接续；全链受控测试，不冒称生产 | AC-CLOUD-07/08 |

可复用的基线回归包括 `hosted_identity/entry_executor_control_tests.rs` 的 `executor_dispatch_denies_owner_intents_and_checks_current_delegation_and_complete_fence`、`rotated_current_executor_can_query_verified_old_original_but_cannot_submit_it`、`stopped_current_owner_can_query_original_without_reenabling_executor`；`oasis7_game_client/src/entry_owner_client_tests.rs` 的 `restart_retains_original_fence_and_wire_before_any_network_and_isolates_vaults`、`owner_signed_history_unavailable_blocks_and_preserves_original_without_submit`。这些已有 Rust 测试只证明自身层，不能代替新增 Worker adapter、网络重放、角色发布权限或灾备演练。本设计没有运行它们，不记录虚假 PASS。

### 12.12 决策、长期风险与未决问题

| 决策 | 采用理由与代价 | 失效/复核条件与 owner |
| --- | --- | --- |
| 每账号 DO 为执行控制权威，D1 只投影 | 同账号授权/预算/请求事务简单；单账号写串行且有容量上限 | 跨账号原子预算或更高吞吐需求出现时由 runtime_engineer 重新设计，不能直接双写 |
| Owner/Identity/Executor 分部署，测试版 Worker 可信持钥 | 降低助手持钥与误接线风险；仍信任平台和发布者，不等于 HSM | 任何真实资产/治理或更强不可提取需求出现即停止扩面，由 blockchain_ops_engineer 另审生产 custody |
| 持久 outbox + 原请求查重/回读 | 保存未知状态和原副作用身份，避免“重试一次”产生重复动作 | 下游持久去重/查询合同缺失或保留期改变时禁用相应写，由 runtime_engineer 解决 |
| 恢复单独 epoch 与冻结，重新批准 | 防旧权限复活，牺牲自动恢复可用性 | 变更 restore 路径或 fence 发布权限时由 blockchain_ops_engineer 复核，不能靠旧备份自行解冻 |

实施前仍需在具体任务冻结：capability 客户端 PoP 的标准协议/向量与 dot 接线支持、Passkey 验证库与现有 identity 服务的部署复用方式、每类 typed action 的下游持久去重/回读/保留能力、真实 Cloudflare 套餐预算和地区容量。各缺口分别阻断对应能力开放，不阻断文档评审，也不通过伪造默认值或新增流程台账掩盖。长期风险仍包括平台/发布链攻陷、账户恢复误授、撤销与已发请求竞态、未知操作造成预算冻结；这些风险必须在测试验收和管理界面中可见。

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
| Trace refs: O7-REQ-17af77d2-a02e-41ea-8185-cbc0650e69e3；[REQ-CLOUD-CUSTODY-006](hosted-public-join-managed-identity-custody.prd.md#req-cloud-custody-006) 产品 N/A：reason=受控测试托管服务的工程实现边界，不新增玩家功能或发行承诺；scope=Rust 包组织、平台适配与实现复用； | 两个 Rust crate、四个独立角色工件、共享协议与窄平台适配；外部实现不改变授权与秘密边界。 | [Rust 实现](#cloud-custody-rust) | runtime_engineer / blockchain_ops_engineer | 包、构建、UI/API 生成与真实平台适配均待实现；不是已有 Wasm 构建证据。 |
| Trace refs: O7-REQ-f98670c4-c1fd-4610-8884-8eb2cf89c1eb；[REQ-CLOUD-CUSTODY-007](hosted-public-join-managed-identity-custody.prd.md#req-cloud-custody-007) 产品 N/A：reason=受控测试 Agent 的专业使用闭环，不新增玩家或公开发行承诺；scope=可信客户端、原生会话、工具、TestSession 与真实 Agent 验收； | 实际 Agent 能消费工具，自主核对效果、接续原件并完成有界收尾，凭据不进入模型。 | [Agent 自测闭环](#cloud-custody-selftest) | runtime_engineer / blockchain_ops_engineer / qa_engineer | 全链待实现；targeted 不代签 dot、cold-entry、其他 Runtime 或完整首局。 |

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
| Trace refs: O7-REQ-17af77d2-a02e-41ea-8185-cbc0650e69e3；[REQ-CLOUD-CUSTODY-006](hosted-public-join-managed-identity-custody.prd.md#req-cloud-custody-006) 产品 N/A：reason=受控测试托管服务的工程实现边界，不新增玩家功能或发行承诺；scope=Rust 包组织、平台适配与实现复用； | [Rust 实现](#cloud-custody-rust) | 两个包保持单向依赖，四种角色产物独立；共享 Rust 协议与实际 HTTP 消费者一致，FFI/存储无整数失真。 | N/A; reason=新增 Rust custody 包及 Workers adapter 尚未实现; scope=CT-006 和 AC-CLOUD-09; owner_role=runtime_engineer; evidence_ref=hosted-public-join-managed-identity-custody.design.md#cloud-custody-validation; re-evaluate=首次实现并执行 CT-006 时 | 实现 PR 的实际构建、依赖/exports 清单、workerd 故障测试与消费者验证。 | 文档、源码参考和已有 native 测试均不证明新 Worker 已构建或部署。 |
| Trace refs: O7-REQ-f98670c4-c1fd-4610-8884-8eb2cf89c1eb；[REQ-CLOUD-CUSTODY-007](hosted-public-join-managed-identity-custody.prd.md#req-cloud-custody-007) 产品 N/A：reason=受控测试 Agent 的专业使用闭环，不新增玩家或公开发行承诺；scope=可信客户端、原生会话、工具、TestSession 与真实 Agent 验收； | [Agent 自测闭环](#cloud-custody-selftest) | 可信客户端/原生会话可连接，工具与原件可恢复，执行、核对和安全收尾有独立边界。 | N/A; reason=客户端新 profile、TestSession 与云端服务尚未实现; scope=CT-007/008、AC-CLOUD-08/10/11 与 ST-02/03/04/06 的所选项; owner_role=runtime_engineer; evidence_ref=hosted-public-join-managed-identity-custody.design.md#cloud-custody-selftest; re-evaluate=实现并完成实际客户端合同及真实 Agent 验收时 | 实现 PR 的 handler/客户端故障测试，以及现有 RunInput/TesterInput/RunResult、世界原件/效果和收尾证据。 | 当前文档不证明真实 Agent 已接通；未运行的工具/Runtime/启动模式保留 gap。 |

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
| 模型工具 broker | `crates/oasis7_game_client/src/runtime_broker.rs` / `runtime_broker_cli.rs` 是 Unix socket，Runtime 白名单为 codex/openclaw，私有解锁留可信本地进程 | 原生包内新增可信 custody tool-host/profile；dot 的 MCP/宿主签名扩展另须实现验收。不能在 Worker 中启动 Unix broker，也不从现有名单推导云端支持 |
| Login/owner/executor | `identity_store.rs`、`entry_owner_client.rs`、`executor_gateway.rs` 已隔离用途；owner 请求先保存原意图后联网 | 云端分 Worker 密钥、批准、预算及 outbox 待实现；不自动搬运现有 vault |
| 世界协议 | `oasis7_client_api` / `oasis7_agent_api` 已定义签名请求、完整 task fence、委托和回执 | Workers 必须复用同一字节与语义，不能把 JSON 可解析当 wire 兼容 |
| Hosted custody | 本文 §11 明确正式 managed custody sign、self-custody 与冻结恢复缺口 | 新 Cloudflare 测试 lane 不补写为已有生产能力 |
| 实现/验收 | 无本节服务、云端签名向量、故障恢复及真实受控账号证据 | PRD 七项要求均未实现；本次检查仅证明文档合同/关系 |


当前实现边界：已有两包、四角色工件、纯状态原语、原生 Passkey 批准/来源核验/只读会话状态及独立 frozen fence 存储。以上不改变表中历史审读基线，也不表示七项要求整体验收完成；切片交付与后续验收进度见 [Issue #5076](https://github.com/nil-studio/oasis7/issues/5076)。

Owner 批准 handoff 组件的范围：Rust 按固定公钥和 canonical bytes 验签，绑定 Owner 生成的核验 nonce，在同步 SQLite 事务中重新核对当前上下文、fence 与期限，一次保存消费历史及 audit，重试返回同一原件。签名检查同时经过严格 Ed25519 与实际 Worker WebCrypto，异步验签结果只允许按完整检查材料回用于事务。该组件没有生产 fetch/RPC 调用；专用 `approval-harness` 工件只供本地测试，无部署配置。消费历史不产生 grant、密钥操作或世界 dispatch。完整管理者关联目录、撤销确认和原生 authority 转发未接通前，Owner 业务批准继续关闭（§12.5）。

CT-008 与 ST-02/03/04/06 尚无真实闭环验收证据。正式游戏 Skill 和现有 ordinary Unix broker 不能替代 custody host/transport、配对及 TestSession 收尾接线；组件 fixture、构建或历史 instruction-only 实验不能代签真实世界效果与收尾验收。具体实现缺口和实玩前置集中更新原 Issue。

<a id="cloud-custody-boundaries"></a>
### 12.4 边界与结构

Identity、Owner、Executor、Recovery Fence 四个 Worker 角色必须分别部署、独立 ingress/service binding、发布身份与 secret 绑定。不能仅把一个含全部秘密的 Worker 拆成不同 URL，就宣称隔离完成。

| 组件 | 可持有/可做 | 不可做与信任边界 |
| --- | --- | --- |
| Management UI + Identity Worker | 管理 Passkey 的受限适配，复用原生 identity authority；角色私有存储持托管账号 login key、原登录请求、当前 Cookie/CSRF；按角色窄接口转发已获 claim 的原件 | 不建立第二份凭证权威；不持 owner/executor wrapping secret，不把管理者 session 当托管账号 session；不从模型参数决定 ACL |
| Owner Control Worker | 新鲜操作批准、owner typed intent/sign/send；托管 owner key；管理 grant 与预批准 TestSession 的固定 Stop/Revoke 收尾 | 不暴露通用签名或私钥；不能从 executor capability 推导新 owner 批准，不能把收尾策略扩为玩法/续期 |
| AccountAuthority DO（Owner 服务私有 namespace） | 每账号唯一权威：账号管理 ACL/批准消费、授权树/撤销版本、预算、请求/outbox 状态、账户冻结及安全代；受限 RPC 为 executor 做原子 admission/claim/settle | D1、缓存和调用者不能覆盖它；executor binding 只暴露 typed admission/claim/result，不能调用 owner sign/grant/恢复方法 |
| Executor Worker | 仅持 executor wrapping secret 和获准账号的 executor key ciphertext；签类型化执行请求、经固定 adapter 发送/回读 | 没有 owner/login secret、grant 创建/续期权限或任意网络代理；不跳过世界当前委托/fence |
| Recovery Fence（独立 namespace/发布权限） | 持久 service epoch、执行禁用及恢复 tombstone，防止账号快照恢复旧服务代 | 仅增加拒绝条件，不授予账号权限/预算；不得与账号 DO 一起回滚 |
| 原生 Rust tool-host（既有 Game Client 包） | 在可信测试主机保管独立客户端 PoP key/capability/只读凭据、请求日志，向模型提供固定工具；可承载经验证的远程 MCP profile | 是额外可信客户端进程，不是第五个 Worker 或世界 authority；无 login/owner/executor 协议私钥，不接受任意 shell/URL，不向模型泄密 |
| D1 / 按需 R2 | D1 可重建列表/检索投影；R2 受限归档和加密备份 | 均不判断 active grant、撤销、剩余预算或原请求可重发；不存明文密钥/token |
| oasis7 identity/world service | 校验独立协议身份、current owner/executor、完整 fence、业务预算及 canonical 结果 | 不信任托管页面的成功状态、D1 投影或模型意图 |

每次出站写都需要 Account DO 最新 admission 与当前 Recovery Fence；二者不可用则拒绝派发。跨 Worker binding 只授予必要窄接口，并校验服务角色、account、intent 类型与版本；网络可达或持有 binding 不等于能够 owner 出签。权限/部署测试必须证明 Executor 身份无 Owner/Identity 发布权和秘密读权。

Worker 平台、被授权发布者及在运行时处理私钥的代码仍属于可信计算基。Workers Secrets 是 secret 绑定，不是不可导出密钥设备；WebCrypto `extractable=false` 不消除导入前明文、恶意部署、同服务代码或平台风险。分账号密钥缩小正常误操作范围，无法抵御所有共享平台管理员攻击。不能对外宣称“私钥永不离开 HSM”或“云端不可见”。

### 12.5 关键运行流程

1. **建档/准备**：仅经部署登记的管理者可批准新测试账号；先记录稳定 provisioning ID、draft ACL 和关联目录，再分别生成角色 key、注册并核对原回执，详见[开户与原生会话](#cloud-custody-provision)。首版关闭已有账号迁移。可信 setup 准备 `prepared_world` 的固定 world/Agent/task/目标/资源，任一步未知保持原件，不重建身份绕过。
2. **Owner 批准**：固定 account、world/genesis/branch/reorg、Agent/task、类型化动作摘要、预算与期限，取得绑定该摘要的新鲜 Passkey 证明，Identity authority 在自己的事务中重验会话、凭证安全版本并消费挑战，签发短时一次性批准证明；Account DO 在自己的事务中重读账号管理 ACL、安全代与账户状态并消费该证明。两者不是一个事务，交接及撤销竞态按下文处理。Owner Worker 生成并持久化真实 owner 签名原件。Passkey assertion 不被当成 Ed25519 owner 签名。
3. **发能力**：按 [TestSession](#cloud-custody-selftest) 一次批准执行、原件读取和固定安全收尾三个目的；各有独立范围/预算/截止。仅在下游绑定、客户端公钥及 grant 已持久化后，按[私有凭据交接](#cloud-custody-client)交给指定宿主。模型只见引用；丢失凭据先吊销旧交接，不能从审计回传或换 token 延长期限。
4. **动作执行**：输入 schema/目标/参数初检；DO 原子核权、占用预算、写 request/outbox；适配器生成既有协议原件并保存；派发前再核 grant/epoch/fence/期限，按同一原件发送；响应必须与原操作、固定 service key 和世界身份匹配后落 receipt。只接收 HTTP 2xx 不表示世界提交。
5. **失败/取消**：未派发且确定没有副作用可取消并释放业务预留；处理请求次数仍计费于请求预算。发送可能发生则标记 `unknown`、保留预留并核对原操作，不换 nonce/operation ID。父撤销与冻结阻断排队项，已经 claim 的项即使尚未实际 fetch，也一律按 in-flight/unknown 保留待核对。
6. **停止/撤销**：正常结束、到限、失联或显式停止先 `quiesce`；Owner 按预批准收尾策略或新的人工批准分别生成既有 `GameExecutorOperationV1::Stop` 和 `AgentSignerDelegationChangeV1 { revoked: true }` 原件。预批准不是重用 Passkey 证明或授予 Agent owner 权限。Stop/Revoke 分别以权威回执及当前状态确认，原件读取走受限 owner `LookupExecutor`，不恢复 executor 写权限。身份失效或超出预批准收尾范围时阻断并请求既有管理恢复。

**认证到账号授权的交接：** Identity authority 拥有 Passkey/会话/凭证撤销，Account DO 拥有账号 ACL、custody security epoch 与批准消费。批准证明由固定 Identity service key 签发，绑定 audience=该 Account DO、管理者、credential ID/security version、account/environment、typed intent digest、一次 nonce、签发/到期（最长 30 秒）及要求的 Account DO policy/security version。Owner Worker 在事务外完成验签/最终在线核验；Account DO 事务只消费经认证窄 RPC 交付的、完整摘要绑定的证明，不读取网络、不信任客户端自报版本或 D1 缓存。证明/在线核验失败、过期、重复、版本不符或来源不可用均拒绝。

Identity 撤销的线性化点只阻止新证明签发，不能使另一个 DO 已消费的批准自动回滚；撤销与已签发证明存在至多 30 秒的授权交接窗口。管理端须将 identity revoked 与 custody authorization closed 分开显示：撤销时必须向所有关联 Account DO 发冻结/升安全代，撤销该管理者/凭证来源的全部 root/children、阻断未 claim 请求并取得确认，再核对已 claim 在途原操作，才显示后者。不能以批准证明过期替代已有根授权/能力撤销；即使暂时没有在途请求，也必须完成账号侧撤销。Identity 撤销后不再签发新批准证明；尚未确认冻结的账号仍可能消费已签发证明或使用已有 grant，所以保持 custody pending/unknown，不能宣称全局立即阻断。已消费/已claim 请求按原操作核对，不能宣称立刻追回。若实现无法证明这一边界、关联账号全集或最终核验，禁用 owner 批准而不是只靠短TTL声称安全。

<a id="cloud-custody-api"></a>
### 12.6 接口、能力与签名合同

下列 `/v1` 是新托管服务的目标接口，不是现有游戏 API。输入/输出均类型化、拒绝未知字段，固定 schema revision；适配器只可调用版本化允许列表，不接受调用者 URL、header、host、path、method 或 actor 覆盖。

| 接口 | 调用方 → 服务 | 身份、顺序与结果 |
| --- | --- | --- |
| `GET /v1/accounts`、`GET /v1/accounts/{id}` | 管理会话 → Identity/Owner | 从主体 ACL 筛选非秘密投影；标记 projection version/stale，不返回 key/token；读详情重核权 |
| `POST /v1/account-provisions`、`GET /v1/account-provisions/{id}` | 已登记管理者 → Owner | 按稳定客户端请求 ID 创建/读取 draft ACL、目录及原计划；本步不生成 key 或注册身份 |
| `POST /v1/account-provisions/{id}/approve` | 已登记管理者 → Owner | 新鲜开户批准绑定原计划摘要/范围/截止；一次消费后才生成角色密钥并注册，同 ID 恢复原开户 |
| `POST /v1/accounts/{id}/owner-intents` | 管理端 → Owner | 新鲜 Passkey 证明绑定完整 typed intent 与 expected policy/security version；固定 idempotency key；返回本地受理或权威结果引用 |
| `POST /v1/accounts/{id}/test-sessions` | 管理端 → Owner | 首版自测创建入口；一次批准原子保存执行/只读/cleanup 授权和客户端绑定，不让工具自行创建根 grant |
| `POST /v1/client-enrollments`、`/v1/client-enrollments/{id}/prove`、`/v1/client-enrollments/{id}/claim` | 可信宿主 → Owner | 登记公钥并证明持有，经管理者精确配对批准后私领凭据；注册本身无账号权限 |
| `POST /v1/proof-challenges` | 已绑定客户端 → 对应服务 | 签发绑定 token digest/client/route/期限的一次挑战；有界、无业务副作用，不作为权限证明 |
| `POST /v1/actions` | 受限客户端 → Executor | capability + client proof + request ID + typed action；DO 绑定精确账号和原摘要；202 仅表示受理 |
| `GET /v1/operations/{id}` | 当前获准主体 → 对应服务 | 仅回读原操作非秘密状态/结果；读权限独立核对，旧写能力不能重新发送 |
| `GET /v1/test-sessions/{id}`、`POST /v1/test-sessions/{id}/heartbeat` | 已绑定客户端 → Owner 窄会话入口 | 读取非秘密状态；heartbeat 仅维持原批准内的 90 秒宿主 lease，不延长绝对截止、不复活 closing |
| `POST /v1/test-sessions/{id}/finish`、`/abort` | 已绑定客户端 → Owner 窄关闭入口 | 仅标记结束/提前缩权并触发已批准收尾；不能传 owner intent、目标、序号或新预算 |
| `POST /v1/accounts/{id}/quiesce`、`/stop`、`/revoke` | 管理端 → Owner | 本地停派发、canonical Stop、delegation Revoke 分端点/operation ID/状态；不以一个动作的结果代替另一个 |

首版不单独开放 `/grants` 创建或通用 root 发行；三个 purpose 只随获批准的 TestSession 一起创建，任何工具动作都必须属于该 session。capability 为不可猜测 opaque 凭据，其 digest 与客户端公钥绑定，原值不落 D1/日志。客户端采用[固定 HTTP Message Signatures profile](#cloud-custody-client)，覆盖方法、URI、原正文摘要、凭据摘要与一次挑战；不再把 PoP 协议留作实施选型。模型只消费类型化工具，凭据留可信宿主；dot 的额外宿主签名接线仍须真实实现验收，不能把普通 MCP Bearer 自动当成端到端 PoP。

根 grant 必含：`grant_id/root_grant_id/parent_id`、`origin_manager_id/origin_credential_id/origin_credential_security_version`、`account_id/environment/service_epoch`、`client_key_id`、完整 world identity、`agent_id/task_id`、owner/delegation generation 与 task/executor fence、action 枚举、参数约束、allowed resource refs、累计请求/业务预算、`not_before/expires_at/root_absolute_expires_at`、`policy_version/revocation_version`。首版不允许客户端转授；如服务生成内部子能力，其深度最多一层、权限/预算只能缩小且共享根预算，绝对期限不得超父/根。无 refresh 自续期，无创建下一根 grant 的 capability。

下游签名保持当前代码合同：`crates/oasis7_client_api/src/world_service/authority.rs` 的 canonical CBOR tuple `("oasis7.world-service.v1", domain, value)` 与 BLAKE3 request digest；各 typed request 继续使用其自己的 domain，不能换成 JSON stringify 或新 HTTP token 签名。`GameExecutorControlV1`/task fence 与 `AgentSignerDelegationChangeV1` 等请求的字段、整数、nonce/sequence、generation、world/genesis/branch/reorg 均按原 schema。

Workers 使用标准 `Ed25519` WebCrypto，不使用 legacy `NODE-ED25519`。以固定兼容日期/运行时和仓库 Rust 版本验证：32-byte seed/PKCS#8 导入、公钥 raw 编码、64-byte signature、原 canonical bytes/digest 双向完全一致；所有 u64 必须无损，明确区分 CBOR unsigned integer 与 `DecimalU64` 字符串，禁止经 JS Number 丢失高位。除正常向量，必须覆盖大于 2^53、u64 最大值/溢出、Unicode、map 顺序、空/缺字段、错误签名域、错误 key/weak key、错误 fence 及过期/重放。适配层若不能产生完全相同 bytes 就阻断该动作，不新造兼容 fallback。

服务无 `get_secret`、`export_key`、`sign(rawBytes)`、`fetch(url)`、任意命令/文件 API。Owner 意图采用单独枚举及审批策略；executor 首版固定[工具集合与 native authority mode](#cloud-custody-tools)，不自动开放资产/治理/账号恢复操作。外部目标通过环境固定 route ID 映射到 HTTPS origin/service key，拒绝重定向、跨 origin、未登记服务和 SSRF 私网/metadata 目标；主动测试网内网 endpoint 只能是部署侧逐一批准的精确 pin，不能由模型传入。

<a id="cloud-custody-data"></a>
### 12.7 数据、事务与持久化

每个 `(environment, account_id)` 映射唯一 AccountAuthority DO，使用 SQLite-backed storage。账号 DO 是授权、撤销版本、预算与本服务请求的唯一权威；Cloudflare 的对象串行执行并不阻止在外部 `await fetch` 期间发生交错，因此所有安全判断与状态转移仍须事务内条件更新，不依赖进程内布尔量。[官方存储与并发约束](https://developers.cloudflare.com/durable-objects/api/sqlite-storage-api/)

| 记录 | 最小字段/持有方 | 原子性与生命周期 |
| --- | --- | --- |
| `AccountRecord` | account/environment、用途标签、公开 login/owner/executor key refs、world/Agent/task refs、status/security_epoch/policy_version | DO 权威；非秘密投影可发 D1；删除标签/投影不删除撤销事实 |
| `Provisioning` / `ManagerAccountIndex` | 稳定 custody/provision ID、manager→account 关联、三角色公开 key refs、native account 绑定与分阶段原回执 | Owner 私有权威目录先登记后准入；用于完整撤销枚举，D1 不替代；部分成功只恢复原开户 |
| `NativeSession` / 角色秘密存储 | Identity 私有 DO 持 login 原件、加密 session/CSRF/security version；Executor 私有 DO 持自身密钥；按 custody account/environment 定位 | 仅是秘密/传输状态；ACL/grant/预算/操作权威仍为 Account DO；cookie 不向 Owner/Executor/模型导出 |
| `KeyEnvelope` | schema、environment/account、role/key_id/key_version、algorithm、ciphertext/value_nonce、wrapped_dek/wrap_nonce、wrapping_key_version | 每协议 key version 独立随机 DEK，角色 KEK 包装 DEK；两层 AES-GCM 使用独立随机 nonce 和分域 AAD，绑定环境/账号/角色/key ID/version/schema；重包仅替换 DEK 包装，不改变协议公钥 |
| `OwnerApproval` | 管理者/credential security version、intent digest、challenge、expiry、consumed operation | 一次消费；不可复用于另一账号、动作、参数或授权范围 |
| `Grant` / `Budget` | 上述 capability 字段、token digest、revoked_at、reserved/consumed、version | 同一账号 DO 内共享 root 预算；issue/admit/revoke/settle 条件事务，拒绝欠额/溢出 |
| `TestSession` / `ClientBinding` | 批准摘要、prepared fixture refs、工具/transport profile、PoP key、三个 purpose 的截止/额度、heartbeat lease、cleanup 原件 refs、close 状态 | 执行/读取/收尾在同一初始批准中创建；彼此不隐含扩权；管理者/账号撤销统一覆盖，关闭不删除 unknown |
| `Operation` / `Outbox` | request ID、request digest、typed body、原签名/nonce/correlation、service/world pin、state、attempt/lease/epoch、receipt | 原件先持久化再联网；相同 ID 不同 digest 冲突；同 request 重试不重记业务扣费，不换 actor/nonce |
| `DelegationSequence` | account/environment/world/Agent、owner/history 范围、generation/nonce 分配水位、原 operation 引用与撤销 barrier | 同一 canonical delegation 资源共用，不按 executor key 分开；签名前事务预留，撤销覆盖全部已保留/可能在途授权，已分配值不回收；历史缺失/溢出冻结 |
| `AuditEvent` / `ProjectionOutbox` | account sequence、actor/action、非秘密摘要、policy/key/security version、结果引用 | 与授权/预算/请求提交同事务；异步投影按 account sequence 幂等，缺序不伪造完整 |
| `RecoveryFence` | environment service epoch、disabled、恢复事件与墓碑 | 独立于账号备份；只收紧执行，不保存可恢复的 active grant 或预算 |

私钥只在对应 Worker 生成/解密/签名的短生命周期内使用，不返回 dot、UI 或通用日志。秘密台账与元数据可在同一账号逻辑关联，但只能经不同的用途访问面读取。生产 key 不允许进入此服务；测试 key 创建/导入是独立授权动作。本版不自动导入旧 vault。密钥备份仅限经授权的加密恢复材料，wrapping secret 不与密文一起归档；否则不得声称加密备份形成独立保护。

新托管 HTTP DTO、需经 JS 值传递的 FFI/RPC 以及 DO SQL/D1 投影，对全宽 u64（nonce、generation、epoch、sequence 和预算）使用规范十进制字符串/TEXT 或固定长度 bytes，Rust 做 checked arithmetic，不经 JS Number/f64 或 SQL 数字往返，也不用字符串字典序比较数值。既有 World Service JSON/CBOR schema 保持不变：原 schema 的数值型 u64 由 Rust 从原始 bytes 直接序列化/解析，不能改成字符串，也不经过 JS JSON.parse/stringify 重编码；既有 DecimalU64 仍按其字符串合同。[Rust 平台约束](#cloud-custody-rust-platform)覆盖这些不同边界的往返验收。

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

父/根撤销在 DO 事务增加版本并使全部派生 capability 和未 claim outbox 失效，不要求先逐行扫描所有子记录才能阻断。已经 claim 的请求仍在网络交接竞态内，本服务无法撤回已离开的 bytes；应立即 quiesce 并通过独立安全控制容量提交 canonical Stop/Revoke，由世界当前 fence/委托最终拒绝后续执行。Stop 与 Revoke 分别保留 operation ID、submitted/unknown/confirmed 和权威状态；Revoke 按既有合同严格递增 generation/nonce，同一原件重复仍按原操作处理。停止任务不自动撤销全部签名委托，撤销能力也不回滚过去世界结果。

**应急准入和派发容量：** 普通 executor 在途写上限、8 个普通未决动作上限与 1 个普通 owner intent 上限均不占用安全控制槽。每账号分别保留 1 个 Stop 槽和 1 个 Revoke 槽；槽位在 Account DO 条件事务中分配，相同动作/摘要复用其原 operation，不允许靠不同 ID 反复占位。同类操作仍 unknown 时先查原件；Stop unknown 不阻断独立 Revoke，反向亦然。二者各自读取/核验当前合法 owner 与 fence；Revoke 序号还必须覆盖下述本服务全部已分配授权水位，不只读取当前世界值，不替换已经签好的原件。独立应急预算、有限次数/截止和同类去重继续生效；普通配额已满或执行预算耗尽不阻塞安全控制。quiesce 是本地拒绝条件更新，不等待远端写槽。轮换只有在相关 Stop/Revoke、旧 writer 及原件已按恢复合同核对后才进入普通 owner 串行槽；它不能挤占停止/撤销容量。任何旧 unknown 及其业务预留都继续保留，不伪造终态或清空记录来腾槽。

**委托撤销的序号屏障：** [Runtime 委托校验](../../../crates/oasis7_runtime/src/service_admission/agent_authority.rs) 按 Agent 保存当前 delegation，接受 generation 和 nonce 都严格递增的变更。Account DO 按同一 canonical delegation 资源（account/environment/world/Agent 及合法 owner/history 范围）维护持久分配水位，不按 executor 公钥分别计数。任何授权原件在签名前先事务保留 generation/nonce 与 operation 关联；取消或未派发也不回收号码。受理 Revoke 时冻结该资源新的授权签发，取分别严格大于“当前 canonical 值”和“所有已保留/已签/prepared/claimed/unknown 授权原件”的 generation 与 nonce，原子登记后再出签。旧分配即使签名尚未返回也在水位内，事务 B 仍重验撤销版本；只有撤销确认并取得新鲜 owner 批准才可再授权。恢复必须保全/核对这一水位和全部可能在途原件；历史不全、其他 owner 写者未隔离或任一计数溢出时保持冻结，不能猜测序号或宣称撤销完成。这样 Revoke committed 后迟到的旧授权会被递增规则拒绝；Stop 仍使用原完整 task fence，不套用委托序号替代其协议。

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

**冻结下的恢复控制通道：** 冻结禁止普通 login/owner 业务签名、executor 写入与创建/续期 grant。仅因测试结束、执行期限/预算或宿主失联而停写时，仍可在[预批准 cleanup](#cloud-custody-selftest)自己的范围/期限内执行固定 Stop/Revoke；这不是绕过管理来源撤销、身份失效或灾备冻结。后者以及密钥轮换、超出原计划的恢复须新的 intent-bound 人工批准、审计与当前权威检查；仍无玩法、转账、governance、通用签名或解冻自授权。恢复使用独立有界应急预算和上述 Stop/Revoke 槽，不被普通在途写或已耗尽执行预算锁死；二者可各自未知但不互阻首次独立派发，不无限重试。owner key/身份不可恢复或已不受信时保持阻塞，按既有世界/身份恢复权威处理，不能自动重登或临时发明另一个 owner。

安全审计建议保留 90 天，运行诊断 30 天；账号退役冻结所有写后，保留最小撤销 tombstone、operation digest/原结果引用，直到下游持久查重/回读保留合同足以阻断重放。需要删除完整私密原件前，先核完 unknown 并销毁相关写能力；不得因 TTL 清理而把旧 ID 当全新可执行。任何具体保留例外与删除责任在实施运维合同中明确，R2 不是默认永久保存个人资料。

<a id="cloud-custody-quality"></a>
### 12.9 质量、容量与可观测性

以下是首版受控测试的设计上限/验收目标，非 Cloudflare 平台极限或实测承诺；阈值由实现任务在成本预算内验证，超限拒绝/冻结，不自动扩容或续费。

| 场景 | 环境、刺激/规模 | 预期响应与指标 | 验证/当前范围 |
| --- | --- | --- | --- |
| 多账号 | 单测试环境最多 100 账号；每账号至多 4 个 active purpose root、1 个未关闭 TestSession、1 个普通待确认 owner intent、8 个普通未决动作 | 一个测试的 execute/read/cleanup 占 3 个 root；同 Agent/task 不并发测试。unknown 计入普通未决，满额拒绝普通新写；Stop/Revoke 各有 1 独立槽 | CT-001/004/005/008，待实现 |
| 受限执行 | 默认执行窗口 5 分钟，明确批准可至 30 分钟；capability/root 截止等于本次 write_deadline；最多 100 次普通工具调用、400 次普通下游传输 | 需要较长测试时首次批准该窗口，不先发 5 分钟再自动续期。无客户端 refresh；execute/read/cleanup 各按 §12.18 独立已批准截止与预算，时间均由服务核验 | CT-003/007/008，待实现；这些值不改变世界 lease/业务数值 |
| 请求/传输 | 请求至多 32 KiB；最多 1 个普通 executor 写 in-flight；单次下游超时 10 秒，每原件自动恢复最多 2 分钟且不越其 purpose 截止 | 超时 unknown，恢复窗口或预算到限转 recovery_required；保留原件/业务预留，独立读取/Stop/Revoke 仍依其授权推进，不能反复重置 2 分钟窗口 | CT-004/007，待实现；响应上限和完整 context 按 §12.17，不静默改现有 broker |
| 控制面延迟 | staging 100 账号/20 并发请求、持续 10 分钟；不含 Passkey 用户耗时与世界提交 | admission p95 目标 <500 ms；单账号暂停提交后下一次 admission 拒绝；无成功吞错 | 实施 PR 提供分位数、地区/版本/样本，未测 |
| 审计/隔离 | malformed、跨账号、重复、超额、撤销/恢复故障注入 | 每安全转移有 account sequence；secret 扫描零原始秘密；失败原因可分类 | CT-001~005，待实现 |

监控记录 admission/deny、queued/dispatching/unknown 数、预算 reserved/consumed、回读延迟、projection lag、key/service/security version、最后成功下游核对；告警区分服务健康与世界效果。持续 unknown、审计堵塞、epoch 不一致或拒绝突增触发账户/route 冻结与 operator 提示；日志不得包含完整私有目标或 bearer 材料。

### 12.10 兼容、迁移与回滚

本次文档不修改既有 protocol/wire、运行时支持名单、默认注册开关或 localhost broker 行为。未来在既有 Game Client 增加独立 custody profile，保留现有 native/local profile；dot 的远程 MCP/宿主签名接线单独交付验证。不能把 Unix socket 路径配置成远端 URL，不能在缺配置时回退到本地 vault/全局 env key。

首版只接新建并明确逐账号批准的测试身份；已有账号迁移保持关闭。先固定 world/service/identity pin 和 schema，完成开户原件、Workers/Rust 向量、当前原生会话和 owner 控制；可信 setup 准备固定世界任务，再批准小预算 TestSession。所有开关默认关闭；缺账号/世界/客户端前置时记录 blocked，不读取旧秘密寻找捷径。

现有本地身份不会因建云端台账改变 custody。未来若开放迁移，须另行补齐旧账号/owner 权威证明，冻结原执行者、核对原 outbox、轮换/绑定新角色 key、确认旧 delegation 撤销，再启用新 grant；本版不提供该入口。不得双端保留同一 executor key 自动活跃；旧私钥不通过 dot/chat/工单传输。自托管退出仍归既有 wallet-transition authority，本测试版不提供明文导出。

协议版本、canonical bytes、fixture 或世界 fence 改变时，先拒绝未支持组合并补适配/负例。D1 可删后重建，但 DO 操作记录/预算/撤销不通过 D1 回灌。兼容回退按 §12.8 保留当前安全代和世界已提交事实。

<a id="cloud-custody-validation"></a>
### 12.11 验证设计与可追溯性

以下 CT 是本设计规定的未来验收场景，不是已存在或已运行的测试名称。对应 PRD [AC-CLOUD-01~11](hosted-public-join-managed-identity-custody.prd.md#cloud-custody-acceptance)；当前 PR 仅执行文档治理、链接/Trace 关系与独立安全审读。实现 PR 必须固定 Rust/Workers 源版本、Cloudflare compatibility date、schema、实际客户端/profile、真实 world/service pin、环境和窗口，区分 fixture、本地模拟、Workers staging、真实 Agent 与世界权威回执。

| 场景 ID | 正例、负例与故障切点 | 要核对的实际结果 | 覆盖 AC |
| --- | --- | --- | --- |
| CT-001 | 2 管理者/多账号、跨账号 ID、未登记者自封管理员、D1 污染；三角色 key/注册/登录/participant 每阶段断点、撤销与迟到开户并发 | 仅合法 ACL 可读写；同开户恢复同 key/身份/原件，部分成功不伪造 prepared；权威关联目录完整，迟到初始化不覆盖撤销，D1 不影响授权；无秘密泄漏 | AC-CLOUD-01 |
| CT-002 | Passkey 错误 RP/origin/UV/stale proof、executor 调 owner/login route；管理者与托管账号 cookie/CSRF 互换、session 过期/撤销、批准响应丢失；证明已消费发 grant 后撤销而 DO 未收齐确认 | 原生账号会话/角色/批准分离；原 proof 一次消费，无自动重登；未知保持，不能以短 proof TTL 显示 custody closed；账号侧撤销后旧 capability 不能新 claim | AC-CLOUD-02 |
| CT-003 | 并发使用同根预算、父撤销、过期、递归派生/续期、跨 world/Agent/fence；Rust→Worker 与 Worker→Rust 全量 golden vectors | 权限只能收缩且不超支；canonical CBOR/digest/signature 精确一致；不支持组合 fail closed | AC-CLOUD-03/04 |
| CT-004 | admission/保存原件/claim/提交/回执切点故障；同 ID 同/异 body；普通配额满且 executor unknown 后发 Stop/Revoke；Stop unknown 后独立 Revoke，反向亦然；高序号授权原件延迟到 Revoke committed 后 | 原件/预留保留，unknown 不换 nonce；安全控制槽独立且有界、同类去重；Revoke 覆盖全部签发水位，迟到旧授权被拒绝；三态分别确认；新 executor 只读旧原件不重提旧 actor | AC-CLOUD-05/06 |
| CT-005 | 恢复撤销前备份、旧 Worker 尚存、Recovery Fence 不可用、wrapping/protocol key 轮换、代码回退、预算/审计耗尽 | 恢复默认冻结、旧 epoch/grant 不能派发、世界撤销核对与新批准后才接续；全链受控测试，不冒称生产 | AC-CLOUD-07/08 |
| CT-006 | 两包 host/Wasm；四个真实角色构建与无/多角色负例；SDK 事务中途失败；u64 全链往返；真实 HTTP DTO/TS 消费者；跨角色路由/DO export/发布身份尝试 | 单向依赖、四个隔离工件；事务完整回滚、无整数失真；旧协议 fixture 不变、新 HTTP 合同同源；Executor 无 Owner/Identity/Fence 权限；参考加密的 AAD/换包/轮换负例成立 | AC-CLOUD-09，并补强 AC-CLOUD-02/03/04/07 |
| CT-007 | 真实 handler/tool-host 的 PoP/配对/丢交接/重启负例；全部工具与 native mode、多阶段原件；同 ID 异 body、丢 202、竞争写、过期 context；Prepare 已保存但丢响应后到限/Stop/Revoke、DO 完整证明无 Prefix 的关闭；三目的及控制预算到限 | 无秘密泄漏、token 单独不可用；完整 context 才可行动；原 Action/Outcome 不替换、无重复效果；canonical 原件持续核对，私有准备/纯读取可按无世界效果证据关闭且不冒称未发生；全部工具合同和独立读/收尾成立 | AC-CLOUD-03/04/05/06/09/10/11；ST-02/04/06 contract |
| CT-008 | 实际受支持 Runtime/profile、固定 prepared_world 中独立 Agent 读 Skill、自选行动并验真；注入已保存/可能派发请求的响应丢失与重连；到期/预算耗尽/失联及 Owner 收尾 | 按 §12.18 的 RunInput/RunResult 留真实工具与世界效果证据；旧原件恢复无重复效果，到限无新执行；quiesce/Stop/Revoke/pending/cleanup 分别确认或阻断，不把脚本/其他 profile 当 Agent 通过 | AC-CLOUD-08/10/11，并补强 AC-CLOUD-03/05/06；ST-02/03/04/06 live 所选项 |

可复用的基线回归包括 `hosted_identity/entry_executor_control_tests.rs` 的 `executor_dispatch_denies_owner_intents_and_checks_current_delegation_and_complete_fence`、`rotated_current_executor_can_query_verified_old_original_but_cannot_submit_it`、`stopped_current_owner_can_query_original_without_reenabling_executor`；`oasis7_game_client/src/entry_owner_client_tests.rs` 的 `restart_retains_original_fence_and_wire_before_any_network_and_isolates_vaults`、`owner_signed_history_unavailable_blocks_and_preserves_original_without_submit`。这些已有 Rust 测试只证明自身层，不能代替新增 Worker adapter、网络重放、角色发布权限或灾备演练。本设计没有运行它们，不记录虚假 PASS。

### 12.12 决策、长期风险与未决问题

| 决策 | 采用理由与代价 | 失效/复核条件与 owner |
| --- | --- | --- |
| 每账号 DO 为执行控制权威，D1 只投影 | 同账号授权/预算/请求事务简单；单账号写串行且有容量上限 | 跨账号原子预算或更高吞吐需求出现时由 runtime_engineer 重新设计，不能直接双写 |
| Owner/Identity/Executor 分部署，测试版 Worker 可信持钥 | 降低助手持钥与误接线风险；仍信任平台和发布者，不等于 HSM | 任何真实资产/治理或更强不可提取需求出现即停止扩面，由 blockchain_ops_engineer 另审生产 custody |
| 持久 outbox + 原请求查重/回读 | 保存未知状态和原副作用身份，避免“重试一次”产生重复动作 | 下游持久去重/查询合同缺失或保留期改变时禁用相应写，由 runtime_engineer 解决 |
| 恢复单独 epoch 与冻结，重新批准 | 防旧权限复活，牺牲自动恢复可用性 | 变更 restore 路径或 fence 发布权限时由 blockchain_ops_engineer 复核，不能靠旧备份自行解冻 |
| [Rust 核心 + Workers 适配两包](#cloud-custody-rust)，四角色分别构建 | 复用现有 Rust 协议、集中业务判定，兼顾宿主测试与部署隔离 | wasm target、SDK/FFI 或角色工件不能满足边界时由 runtime_engineer 先修适配；不扩为第二套协议或多角色动态通用 Worker |
| [agentic-keyvault 的有界参考](#cloud-custody-reference) | 借鉴封装加密、版本化和薄客户端，不引入其 secret 读取及响应缓存语义 | 采用外部实质代码或上游版本变化时重核来源、许可、AAD 与权限合同 |

本设计已固定客户端/PoP、原生身份适配、开户、工具和 TestSession 合同，实施者不再二次选择这些边界。实施任务仍须产出具体版本/配置、签名互操作向量、各原生阶段的持久查重/回读保留证据、dot 宿主扩展的实际兼容结果，以及 Cloudflare 套餐/地区容量实测；能力验证失败则保持对应 profile 关闭。长期风险包括平台/发布链攻陷、账户恢复误授、撤销与已发请求竞态、未知操作冻结预算及身份失效导致 cleanup 阻断；不能以自动重登、通用代理或虚假成功掩盖。

<a id="cloud-custody-rust"></a>
### 12.13 Rust 技术栈、包组织与实现边界

本节承接 [REQ-CLOUD-CUSTODY-006](hosted-public-join-managed-identity-custody.prd.md#req-cloud-custody-006)。Rust 是首版服务业务和协议逻辑的实现语言；Cloudflare 入口目标为 `wasm32-unknown-unknown`，使用官方 [workers-rs](https://developers.cloudflare.com/workers/languages/rust/)。管理 UI 的浏览器 WebAuthn 调用与展示可用 TypeScript。以下目录与职责规定完整目标。当前[基础实现](../../../crates/oasis7_custody_worker/README.md)已建立两包、纯状态原语、四角色 Wasm 构建与禁用业务的 HTTP metadata/error 合同；原生身份批准/来源核验/会话 metadata、幂等开户纯状态与独立 frozen fence 存储已有实现；云端身份转发、事务授权、秘密适配、可信客户端与游戏执行仍待接通。基础工件不代表可用 custody 服务或部署/实玩验收。

#### 包与依赖方向

| 拟新增 package / 位置 | 持有的职责 | 依赖边界 |
| --- | --- | --- |
| `crates/oasis7_custody_core` | 托管 HTTP DTO、批准摘要、grant/预算与原请求/outbox 状态变换、恢复判定、key envelope/AAD 格式；同步纯逻辑供 host/Wasm 测试 | 复用 `oasis7_client_api` / `oasis7_agent_api` 的类型/验证和 `oasis7_wasm_abi` 的 canonical 编码；无 Workers SDK、JS、真实时钟/随机、HTTP/文件/进程或世界执行器依赖 |
| `crates/oasis7_custody_worker` | Rust fetch/DO/alarm 入口，SQLite 事务、WebCrypto、Fetch/窄 service binding、D1 投影与可选 R2；只在本层出现 `JsValue` 和平台 I/O | 依赖 custody core 和锁定的 `worker`/Wasm bindings；不依赖整个 `oasis7_game_client`、Client Storage、local signer、launcher/backend 或 world runtime/service |
| `oasis7_custody_worker/deploy/{identity,owner,executor,recovery-fence}/` | 各角色 Wrangler 配置、migration、bindings、构建和无秘密运维说明 | 四套独立发布身份/secret/namespace；Recovery Fence 不并入 Owner 部署或账号恢复快照 |
| `oasis7_custody_worker/ui/` | 管理确认页、浏览器 Passkey 调用、非秘密状态；消费 Rust HTTP 合同产物 | 不持协议私钥，不实现授权、预算、canonical 编码或世界操作签名；无额外 TS/Go custody 服务 |

新增 package 数为两个；`policy`、`budget`、`operations`、`recovery`、`envelope` 是 core 内部模块，DO/crypto/transport 是 worker 内部适配模块。依赖只从 worker 指向 core、再指向既有 API/codec；现有 API 不反向依赖 custody，core 不回拉世界执行层。通过显式上下文/端口注入当前时间、安全随机源、签名器、持久事务与网络，不让核心自行读取环境或访问操作系统。

`oasis7_custody_worker` 使用 `cdylib`（测试所需可同时 `rlib`），按 `identity`、`owner`、`executor`、`recovery-fence` 四种 feature 分别构建。Wasm 发布构建恰好启用一个角色，无角色/多角色均拒绝；host 检查不导出可部署服务。各角色只编入自己的 handler/DO exports：Owner 持有 AccountAuthority，Recovery 持有独立 Fence。四份输出保存在不同角色目录、核对各自产物摘要；共享源码不等于共享运行时秘密。禁止用一个含全部能力的动态 role/env 参数程序替代隔离，也不能用 `--all-features` 代替四个真实构建组合。

#### 既有 Rust 的复用与窄提取

固定源码基线中，[Client API manifest](../../../crates/oasis7_client_api/Cargo.toml) 与 [Agent API manifest](../../../crates/oasis7_agent_api/Cargo.toml) 已持有协议类型/验证依赖；直接调用 [authority.rs](../../../crates/oasis7_client_api/src/world_service/authority.rs) 的 `signing_bytes`/`request_digest`、现有 typed payload、correlation 和 fence 校验。Rust canonical bytes 以字节数组交给 WebCrypto，禁止先变成 JS object/Number 后重新构造签名原像。HTTP 新 DTO 属 custody core，不复制现有 World Service 类型。

[Game Client manifest](../../../crates/oasis7_game_client/Cargo.toml) 含 `reqwest::blocking`/rustls、Client Storage、原生随机源；`runtime_broker` 和 identity/owner/executor 客户端还依赖 Unix vault、文件和进程行为，不能整包链接到 Worker。如需要复用 `entry_owner_client` / `executor_gateway` 中的原 actor、历史/当前 fence、原请求/响应关联验证，先把纯验证函数窄提取到既有 Client API 合适模块，再由原生和云端调用同一实现；不复制验证器、不新增通用 client-core crate。原生调用方与已有负例同步回归，Unix vault/transport 保持原职责。

Client API 中的文件读取、`SystemTime` 等便利 API 也不因 crate 可编译就变成 Worker 可调用；实现需登记纯协议入口，核验实际 target/feature 依赖。首个构建切片验证两新包及其传递依赖，必要时在原 API 包内隔离宿主辅助方法，保留当前协议字节和原生消费者；当前文档不声称已通过 Wasm 编译。

<a id="cloud-custody-rust-platform"></a>
#### 平台、身份与密码学适配

- **DO 事务**：以 [Rust Storage API](https://docs.rs/worker/0.8.7/worker/durable/struct.Storage.html) 和 [SQLite DO 合同](https://developers.cloudflare.com/durable-objects/api/sqlite-storage-api/) 为实现参考。优先使用 SDK `Storage::transaction()`，回调内只执行同步 SQL/完整消费 cursor 与纯 Rust 状态计算，无网络、WebCrypto、计时器或其他异步 I/O；外层等待事务完成后再返回可派发状态。回调错误须使全部修改回滚，不能用手写 SQL `BEGIN/SAVEPOINT` 或进程内锁代替。必须在 workerd 验证中途报错、并发 admission 与崩溃切点；若选 `transactionSync`，其窄 FFI 及异常回滚语义单独验证，不假设 Rust SDK 已有同名高层方法。
- **整数**：[Rust SQL value](https://docs.rs/worker/0.8.7/worker/enum.SqlStorageValue.html) 的整数桥接仍经 JS 数值，Rust 类型不能自动保护全宽 u64。仅在 §12.7 指定的新 DTO/JS 值桥接/存储边界使用字符串/TEXT/bytes；既有协议数值字段仍由 Rust 直接编码/解析原始 JSON/CBOR bytes。在事务内以 Rust 解析、比较和 checked arithmetic 更新；覆盖 `2^53+1`、u64 最大值、溢出及审计/投影回读，禁止以 SQL 数字运算或字符串排序重新解释。
- **角色接口**：优先采用经验证的窄 Rust binding；[Rust RPC](https://docs.rs/worker/0.8.7/worker/#rpc-support) 的实验性支持不等于已有完整复杂类型生成。确需命名 `WorkerEntrypoint` 时，薄 JS/TS 胶水仅暴露本设计明确列出的角色方法：Executor→AccountAuthority 为 `admit/claim/settle/read_result`，Identity forwarding、许可核验及角色 ensure-key 各用 §12.16 的独立 entrypoint。胶水只把静态服务身份和有界 bytes 交给 Rust，不承载业务判断、签名编码或秘密。Executor 不持 AccountAuthority 直接 namespace，也不获全部 Owner/Identity routes 的通用 binding。角色不能由请求字段/header 自报；验证最终 exports、binding 与发布权限。
- **密码学**：WebCrypto Ed25519/AES-GCM 经 Rust `web-sys`/`wasm-bindgen` 调用；CSPRNG 和时间来自 worker 适配。需要 `getrandom` 时，unknown-Wasm 的 `wasm_js` 支持只在 worker 叶子依赖启用。核心不使用全局 env secret 或 native fallback。Rust/native、Rust/Wasm、WebCrypto 仍执行 §12.6 的双向向量和拒绝矩阵；签名器只消费内部类型化构造得到的 bytes，平台持钥残余风险不变。
- **Passkey 与登录会话**：当前 [Cargo.lock](../../../Cargo.lock) 的 `webauthn-rs-core 0.5.5` 带 OpenSSL 依赖；首版不把现有验证器直接编到 unknown-Wasm。Identity Worker 通过固定 HTTPS origin/service pin 的窄适配复用既有原生 Rust `hosted_identity` authority；Passkey ceremony、凭证/会话安全版本、撤销和恢复继续由这一份权威判定。批准证明在该权威校验并原子消费后签发，Owner DO 仍按 §12.5 单独消费，不由代理自报验证成功。需增加的批准证明接口在现有身份模块实现，不另建身份 crate/凭证数据库。域名、RP/origin、Cookie/CSRF 和服务 audience 必须显式匹配，不能透明代理后放宽来源校验。托管 login key 与下游 session 按账号/环境隔离并留可信适配层，Owner/Executor 的 gateway 调用仍须当前合法会话与角色证明；不能因已有托管 capability 就绕过登录撤销。后续迁移 Passkey verifier 到边缘须另审标准库支持与撤销/恢复等价性。

#### HTTP 产物、构建与现有工程入口

托管 HTTP 接口的真实 Rust DTO 是唯一类型声明，路由/auth/error/limits 使用独立 custody profile。当前 [协议生成器](../../../crates/oasis7_agent_api/protocol_codegen/README.md) 只支持固定 `signed-read-bootstrap` 五个 GET roots，不生成通用 Rust DTO 或 JS 签名 codec。实施时先增加真实 Rust DTO 到 schema/components 的导出入口，再扩展独立 profile、POST/requestBody/auth/error 支持及负例，导出管理 UI 所用 JSON Schema/TypeScript，验证真实 Rust handler fixture 与 TS 消费者；不能只改 profile 就宣称支持，不建立脱离实际 handler 的镜像 DTO。保留旧 bootstrap profile 和其既有产物。生成器仅属开发工具，不进入 Worker normal/build 依赖，不在 `build.rs` 或请求路径运行。

实施在根 `Cargo.toml` 注册两个成员并继承 workspace edition/license/lints，更新同一 `Cargo.lock`；依赖版本遵循现有 workspace 策略。为新增包在 [cargo-dev-architecture.json](../../../scripts/cargo-dev-architecture.json) 登记指向本节的 host/Wasm/test 与四角色 profiles，禁止传递回拉 native client/storage、launcher 与世界执行层；core 另禁止 worker/JS 依赖。包内角色隔离不能仅靠 Cargo 图证明，仍做 exports/bindings 负例。

同步 [ci-required-scope.json](../../../scripts/ci-required-scope.json) 的 package/input 归属、[SUPPORT_PROFILES](../../../scripts/plan-rust-required-scope.py)、[ci-tests.sh](../../../scripts/ci-tests.sh) 及现有选择器测试，覆盖部署配置、schema/profile、UI/生成产物及 Rust 依赖变化。普通文档改动继续只选适用文档检查；不为本设计预建空 CI profile 或新增通用审批。

实现必须固定 Rust toolchain、worker/worker-build、wasm-bindgen、Wrangler、features 和 compatibility date；以所选版本实际生成并经 dry-run 验证的入口为准，不沿用未经核对的历史 shim 路径。Rust/Cargo 管理服务依赖；Node 工具仅用于 Wrangler、必要 glue 和 UI/平台测试，并锁定版本。首批验证包括 core host tests、`cargo check -p oasis7_custody_core --target wasm32-unknown-unknown --locked`、worker 四角色分别构建和 workerd 故障测试；所有命令均是未来包存在后的验收入口，当前未执行。

实施按可验证切片推进：①两包 host/Wasm、四角色产物和窄 bindings；②幂等开户、原生批准与当前 session forwarding；③可信 Rust tool-host 的配对/PoP/只读凭据、TestSession 与独立收尾；④共享 native profile、多阶段原件及 CT-001~007；⑤管理 UI/正式 Skill 更新、受控 prepared-world 的 CT-008。dot 的 MCP 宿主扩展在第三切片接通并单独验收，未接通不阻断原生 profile 的真实范围交付，也不宣称 dot 已支持。每片保留实际构建/合同证据；部署及凭据动作继续按既有授权，不自动建新 Issue 或开放服务。

<a id="cloud-custody-reference"></a>
### 12.14 agentic-keyvault 的实现参考与采用边界

参考固定为 [Hitesh-Sisara/agentic-keyvault@f39c1cf859e79a30b684291f1a2a599cc8527a96](https://github.com/Hitesh-Sisara/agentic-keyvault/tree/f39c1cf859e79a30b684291f1a2a599cc8527a96)。它是 TS Worker/D1 的秘密存取服务；以下结论来自实际源码，不作为 oasis7 协议签名、权限隔离或真实部署验收证据。本次仅采用设计方法，不引入上游服务、Go/TS backend、运行依赖或源代码副本。

| 已核对来源 | 可借鉴的具体实现 | 本项目采用方式与限制 |
| --- | --- | --- |
| [crypto.ts](https://github.com/Hitesh-Sisara/agentic-keyvault/blob/f39c1cf859e79a30b684291f1a2a599cc8527a96/packages/worker/src/crypto.ts)：`sealValue/openValue/rewrapDek` | 每版本随机 DEK、AES-GCM 封装、分版本 KEK、只重包 DEK | Rust core 定义 envelope/AAD，worker 执行 WebCrypto；两层 AAD 分域并绑定环境/账号/角色/key ID/version/schema，不照搬仅 secretName/固定常量的绑定；重包不改变协议公钥 |
| [ids.ts](https://github.com/Hitesh-Sisara/agentic-keyvault/blob/f39c1cf859e79a30b684291f1a2a599cc8527a96/packages/worker/src/ids.ts) 与 [store/tokens.ts](https://github.com/Hitesh-Sisara/agentic-keyvault/blob/f39c1cf859e79a30b684291f1a2a599cc8527a96/packages/worker/src/store/tokens.ts) | 随机 opaque token、digest 落盘、非秘密列表 | 保留本服务 PoP、精确资源、root budget、绝对 TTL 和撤销代；不采用 bearer-only、可空 expires_at 或通用 admin token |
| [shared/client.ts](https://github.com/Hitesh-Sisara/agentic-keyvault/blob/f39c1cf859e79a30b684291f1a2a599cc8527a96/packages/shared/src/client.ts) 与 [secret-service.ts](https://github.com/Hitesh-Sisara/agentic-keyvault/blob/f39c1cf859e79a30b684291f1a2a599cc8527a96/packages/worker/src/secret-service.ts) | 薄 typed transport、crypto/service/store 分层、不可变版本 | 在两个 Rust 包内以模块/端口复用；授权/预算/原件/审计仍在 Account DO 同事务，D1 仅投影；上游手写 TS types 不替代本项目真实 DTO 产物 |
| [crypto.test.ts](https://github.com/Hitesh-Sisara/agentic-keyvault/blob/f39c1cf859e79a30b684291f1a2a599cc8527a96/packages/worker/test/crypto.test.ts) | 随机往返、Unicode、错误 AAD/KEK、篡改拒绝、重包连续性 | 转为 Rust/native/Wasm 测试类别，再补跨账号/角色/环境换包、协议签名与恢复负例；不把上游测试当作本项目已运行证据 |

明确不采用 [MCP 的 get_secret/get_secret_version](https://github.com/Hitesh-Sisara/agentic-keyvault/blob/f39c1cf859e79a30b684291f1a2a599cc8527a96/packages/mcp/src/index.ts) 及批量明文导出/命令环境注入方式：本项目模型只获类型化动作、opaque 引用和脱敏结果。也不采用其 [idempotency middleware](https://github.com/Hitesh-Sisara/agentic-keyvault/blob/f39c1cf859e79a30b684291f1a2a599cc8527a96/packages/worker/src/idempotency.ts) 的“查询缓存→执行 handler→waitUntil 保存响应”：该步骤不能提供副作用前的持久 claim、正文绑定及故障原件恢复，不能替代 §12.7 的 DO 原子状态/outbox。完整响应缓存也不能保存新发 capability 等原始秘密。

上游 [LICENSE](https://github.com/Hitesh-Sisara/agentic-keyvault/blob/f39c1cf859e79a30b684291f1a2a599cc8527a96/LICENSE) 为 MIT，署名 Hitesh Sisara。将来复制实质代码时随代码保留版权/许可通知并记录来源；本设计的概念参考不引入许可或安全性的整体背书。

<a id="cloud-custody-client"></a>
### 12.15 可信客户端、私有交接与请求持有证明

在现有 `oasis7_game_client` 增加原生 Rust `custody_tool_host` / `custody_transport` 模块和窄启动入口，可依赖 custody core 的纯 DTO/profile；新增 crate 仍只有两个。tool-host 保存独立客户端 PoP key、执行/只读凭据及请求日志，不持 login/owner/executor 协议 key。Worker 不能反向依赖 native client、MCP/OAuth、socket、OS vault 或 native TLS。

首个消费者固定 `native-broker-custody-v1` transport + `custody-native-v1` 工具 profile，由受信 launcher 启动 Game Client，模型用现有 `broker-call` 的 socket/resource_id/typed args。首批只登记实际验收的 codex 组合；openclaw 须单独通过 custody transport 验收。受信宿主须隔离模型进程对私密目录、keyring、环境/argv 和内存的访问；同 UID 的 0600 文件或“不要读取”提示不构成隔离。

#### 宿主部署声明与实际隔离前置

宿主登记使用闭合 `HostDeploymentDescriptor`：environment、transport/tool profile、host instance ID、客户端公钥、构建和正式 Skill/tool manifest digest、固定 HTTPS origin、受限 runtime identity、隔离机制及验收记录引用。它只描述待配对对象，不是密码学隔离证明。enrollment 公钥持有证明和管理者 Passkey 批准不能把自报 descriptor、profile 名称或 `approvedHost` 布尔值提升为可信宿主。已验宿主集合由既有部署环境的固定配置限定，配置仅含公开绑定与验收引用；尚无实际隔离验收的环境保持空集合或 disabled，不在此流程新增运维签名权根或自动创建凭据。

TestSession basis 的 runtime_ref、skill_ref、source_ref 必须从实际命中部署 allowlist 的 exact descriptor 唯一派生，分别为 restricted runtime identity、正式 Skill manifest digest、该版本 build digest；tool manifest 与全部字段另由完整 descriptor digest 绑定。管理者或模型不能仅回显任意字符串替代来源核验。匹配自报 descriptor 的字段值本身不能使空 allowlist 变为可信。

可接受的隔离必须实际约束模型运行时：宿主与模型采用不同 OS 安全主体/强制沙箱，或模型只通过固定 HTTPS 闭工具面调用独立远程宿主。只有宿主安全主体可读 client key、purpose credential 和持久 inbox。真实验收覆盖文件、keyring、env/argv、debug/process memory、继承句柄、挂载和网络逃逸；同 UID 文件权限、容器名称或 AppSandbox 配置文本不代签这些结果。宿主公钥、instance、descriptor digest、runtime/Skill manifest 或 origin 改变必须重新管理配对，不能用同 key credential replacement 隐式迁移宿主。生产配置默认不启用；合成 fixture 只验证协议实现，不能填实际隔离验收的 PASS。

#### dot 的固定远程接线

`dot-mcp-custody-v1` 使用 Oasis 测试运营者额外部署的原生 Rust tool-host，提供固定 HTTPS `/mcp`。采用 [MCP 2025-11-25 Streamable HTTP](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)，请求采用 JSON 响应，不提供主动 SSE，GET 返回 405，不使用 `MCP-Session-Id`；合法 notification/response 按规范回 202 空正文。业务状态由持久 TestSession/operation 承接。只提供 initialize、必要通知、tools/list 和本节固定 tools/call，不提供任意资源读取、sampling、shell 或递归派工。

dot 宿主必须实现专用可信 connector adapter，在模型之外持 OAuth token 与自己的 Ed25519 connector key，逐请求加下面的 HTTP 签名。OAuth 按[固定 MCP 授权规范](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization)使用精确 resource/audience、预登记 client_id/redirect URI、授权码与 PKCE S256。OAuth 授权/交换是现有原生 identity authority 的待新增 custody 适配，使用同一管理账号/撤销权威，不是现成能力或平行身份库；新授权仅针对已批准 TestSession，issuer/签名公钥/端点由环境配置固定。

发行方必须把 token 绑定 `manager/credential/security version + connector public key + host instance/custody client key + TestSession`，scope 只允许该测试工具/结果/请求关闭；无账号枚举、owner、login 或 grant 创建。其绝对到期不越本次 read_until，不提供 refresh；每次业务调用仍核验当前来源状态、对应 purpose 的更短截止与剩余额度，长于 write_deadline 的 OAuth 认证不延长执行权。连接器不可私密持钥或不能注入签名 header 时，该 profile 保持 blocked；普通 OAuth Bearer 不降级准入。

原生 OAuth issuer 的 authorize/code/token exchange 必须经 Identity 的固定只读 binding-query 适配读取 Owner 已持久化的 TestSession/ClientBinding，核对管理者、两端 key、host、scope、截止与撤销代；不能相信 OAuth 参数自报已批准。native→Identity 使用单独、仅允许此查询的 service credential 与固定 HTTPS origin，Identity→Owner 使用专用窄 binding；它与下文 Identity→native 的 approval-verify credential 分开。查询失败或记录未完成即拒绝发行；token 的 source/绑定核验也不能靠长期缓存越过 Account DO 的撤销。

握手顺序为：限速的 metadata/enrollment/challenge → 两端公钥登记与管理者精确配对批准 → 宿主私有 OAuth code/token exchange → 带已批准 TestSession 引用和签名的 initialize。adapter 从首次 initialize 起显式发送 `MCP-Protocol-Version: 2025-11-25`；无需先有 MCP session ID。tool-host 校验 OAuth issuer/audience/client/subject/scope/key 与测试绑定，之后用自己的独立 custody client key/capability 对第二跳重新签名；上游 token 不透传。两跳各有 challenge/replay 状态，不把第二跳签名宣传成同一签名端到端证明。

原生 host、OAuth 适配、dot connector 的部署/隔离和兼容性是明确的实施成本；Cloudflare Worker 部署不自动提供它们。只有真实 dot 宿主/adapter/MCP/host/Workers 组合通过 CT-008，才可标 dot 支持；原生 profile 可先独立交付。

#### 固定 HTTP 签名 profile

采用 [RFC 9421](https://www.rfc-editor.org/rfc/rfc9421.html) 的 HTTP Message Signatures 与 [RFC 9530](https://www.rfc-editor.org/rfc/rfc9530.html) 的 Content-Digest。以下取值、授权与重试是 Oasis 应用合同，标准本身不替服务保存 nonce 或作准入判断；不改变 World Service canonical bytes。

| 项目 | 固定合同 |
| --- | --- |
| 凭据 | MCP 跳为 `Authorization: Bearer <token>` 且必须同时验签；custody 跳为 `Oasis7-Capability: <opaque>`。token 为受限 ASCII，不进 URL、模型、日志或错误反射。`Oasis7-Token-Digest` 是 Structured Field Byte Sequence，值为 SHA-256(本跳 token 原 ASCII bytes)，服务端重算并常量时间比较。 |
| 正文 | `Content-Digest: sha-256=:<标准 Base64>:`，仅此算法；POST `Content-Type: application/json`，拒绝 Content-Encoding/trailer digest。对实际完整 octets 验摘要后 typed parse，不重排 JSON。GET 无正文，digest 为 SHA-256(empty)。 |
| 绑定 | `Oasis7-Test-Session`、`Oasis7-Request-Id` 强制；前者对应批准 session，后者是宿主发送前持久保存的业务 request ID（读/控制请求也有稳定 ID）。header、path、typed body 的对应字段必须一致。 |
| 覆盖组件 | POST 顺序为 `("@method" "@target-uri" "content-type" "content-digest" "oasis7-token-digest" "oasis7-test-session" "oasis7-request-id")`；GET 去掉 content-type。MCP 第一跳追加 `"mcp-protocol-version"`，包括 initialize。 |
| 签名参数 | 唯一 label `oasis7`，`alg="ed25519"`，keyid 为已登记 opaque key ID，不能是远端取钥 URL；tag 分别 `oasis7-mcp-client-v1` / `oasis7-custody-client-v1`；必含 created/expires/nonce。Ed25519 直接签 RFC signature base，不额外预哈希。 |
| 时钟与挑战 | Unix 整秒，created ≤ server_now+5s，expires>created，expires-created≤30s，server_now<expires 且不越 token/purpose 截止。nonce 为服务生成 32 random bytes 的无 padding base64url，30s 内有效，绑定 hop/key/token digest/session/purpose/method/URI/request ID/body digest。 |

HTTP 签名时间使用独立 Unix 秒类型，批准、Worker 时钟和三 purpose 截止使用独立 Unix 毫秒类型。边界转换必须 checked，秒级 expires 对毫秒截止向下取整，不能通过向上舍入延长权限；小于完整一秒的剩余窗口不能新发 proof。旧 `SessionLifecycle` 的秒值不能直接接收批准里的毫秒值，也不能将 RFC created/expires 当作毫秒存入同一字段。固定 signature base 保留实际正文 bytes；单次 header 总量、每字段长度及正文仍受已有闭工具/32 KiB 请求上限限制，解析前先拒绝超限。

具有原始字段实例的 native 入口按不区分大小写的实例拒绝重复。Cloudflare [Headers](https://developers.cloudflare.com/workers/runtime-apis/headers/) 暴露逗号合并后的表示，不能声称取得原始重复数或原始 header 字节总量。Core 明确区分 RawInstances 与 WorkerMergedSingletons；后者所有签名、认证、正文和目标相关字段采用不含逗号的单值语法，出现逗号直接拒绝而不 split 重建。注册 keyid、nonce、session/request ID 也限定该语法；新 capability 仅无 padding base64url，Bearer 仅允许无逗号的既定 token 语法。若 Host/Content-Length 可观察则要求规范单值；Content-Encoding、Trailer、Transfer-Encoding 拒绝。未知 Accept 等非授权列表可忽略，合并后 8 KiB/32 fields 上限只约束 Worker 可观察表示，原始连接上限由实际入口承担。必须在真实入口验证大小写重复和空值在前/后/双方的行为；若平台丢掉原始空实例，只能报告其固定值语义，不得宣称原始重复拒绝验收通过，相关部署保持待核实，不用客户端自报 raw 信息补证。

严格拒绝多 label、重复/多义 header 或参数、缺失覆盖项、未知算法/key/tag、错误摘要和重放。签名验证按可信入口的固定 public HTTPS origin 与实际请求目标，不信任客户端 Forwarded 或自行改写 URI；禁止重定向，非规范路径/未知 query 拒绝。内部窄转发携带已核验绑定和原 bytes，不重新解释签名目标。

挑战入口 `POST /v1/proof-challenges`（MCP host 同名入口）只发绑定挑战，不授予权限。已配对客户端须先用登记 key 签 challenge-only 请求：沿上表 POST 组件和 30 秒时间条件，tag 分别为 `oasis7-custody-challenge-v1` / `oasis7-mcp-challenge-v1`，不含 server nonce；正文固定所需 purpose/方法/URI/业务 request ID/body digest。服务验签、核当前绑定后，以挑战请求自己的稳定 request ID 原子保存/返回同一挑战，重放不再占槽或扣账，同 ID 异正文拒绝。这避免“取挑战先要挑战”的循环，也不能仅凭公开 session/key 或被盗 token 耗尽他人的额度。

验证持钥之后才适用每 key/session 最多 8 个未消费挑战、每秒 2 次及对应传输预算。尚未领 token 的已批准 claim 以空 token digest、登记 key 与原 enrollment/session 绑定认证；该例外只限 claim 挑战。未配对 enrollment 仅按 origin/IP 限速，最多保留 10 分钟，不能触碰任何已批准账号/测试预算；metadata 公开但无账号列表。密码学验签在事务外完成，业务 nonce 一次消费、当前 key/credential/purpose/epoch 核验、request→digest 绑定及相应预算/admission 在同一权威事务提交。custody 端使用 Account DO；MCP host 用持久 inbox 保存已验证主体和稳定第二跳请求，不用内存 cache，也不作为授权或业务预算真值。

丢响应后先查发送前保存的 operation ID；必要的同原请求重试取得新 challenge/HTTP proof，保持原 typed body/业务 ID。已消费 nonce 不复用；同 ID 同 digest 返回原状态，不重复业务预留，同 ID 异 digest 冲突。HTTP proof nonce、custody operation ID 与世界 nonce 相互独立。MCP JSON-RPC id/新连接不重置业务身份；宿主日志保存 runtime tool-call→operation 对应及尚未收到的结果。

#### 公钥登记、一次领取与凭据替换

##### 定向邀请与唯一 Account 配对

管理会话经既有 Origin/CSRF 与固定 Account 私有 POST 入口建立 `PairingInviteTargetRef`，这不是纯读取；同一稳定 request ID 精确重试只返回原邀请，同 ID 异正文拒绝。邀请是非授权 metadata，不需要另一次 MFA，也不扣业务、Execution、Read 或 Termination 预算。Account 独立持久限制最多八个未结邀请；总历史容量最多 256，满额只阻止新邀请，不阻止关闭、撤销和旧原件读取。未消费且到期的 metadata 可按明确规则回收；已消费邀请及 Enrollment/TestSession 唯一绑定不能当过期缓存丢弃或重置，库存不明保持 pending。

邀请原件包含独立 domain/schema、environment/service authority、目标 custody account/provision、随机 invite ID、issued/expires（最多十分钟）与允许的 transport/tool profile；`invite_digest` 覆盖完整原件，不只是短码哈希。Host 的 Create/Prove 原件必须精确签入此 target ref、Enrollment ID、host descriptor、公钥和服务器挑战/截止。仅凭裸 Account ID、公开 handle 或自报已批准信息不能建立邀请或领取权限。定向 target 不能在 prove 后更换，因此另一合法管理账号也不能抢先占用看到的公开 Enrollment。

Enrollment DO 只保存不可变的定向持钥证明，不分配 grant，也不做跨 Account 的可变认领。真正授权只有目标 Account 一个线性化点：从固定私有 namespace、由 Enrollment ID 确定的 actor 读取实际原件及 State.id/realm，校验目标等于本物理 Account/当前 provision、原 invite 与部署允许的 descriptor；final SQL 再核时钟、未取消、Invite/Prove/MFA 均未过期，原子消费 invite、唯一 Enrollment→TestSession 关联、全 Account 唯一 MFA proof 和三个 sibling purpose roots。一个未关闭 TestSession、现有 root 上限及预算约束不变。不能让客户端提供 receipt、URL、Forwarded、任意 actor 名或 `verified` 布尔值，也不新增运维签名权根代替固定内部能力。

独立 fresh-MFA 的 TestSession 原件签入闭合 `ClientPairingBinding`，包含完整 target/enrollment digest、host instance/descriptor digest、custody client 公钥以及 dot 的 connector 公钥（适用时）、Prepared 资源与所有 purpose 期限/预算。旧仅有 client key 的 TestSession 结构不构成新配对授权。Enrollment 的十分钟 TTL 只限制新 prove/pairing；已经证明或绑定的不可变历史仍可只读核对，不因过期删除，也不因此允许再次配对、续期或重领 raw token。配对 final commit 所核 Prove 截止是原 Enrollment/邀请的十分钟窗口；RFC proof 的三十秒窗口在实际 proved_at 验证并固定为历史事实，不要求用户完成 MFA 时仍落在旧 HTTP 签名的三十秒内，也不能重新 prove 延长登记窗口。原件证明及时而配对 final commit 已过期仍拒绝。后续业务只由 Account 当前 ClientBinding、来源、lease 与 purpose 决定，不把 Enrollment 事实当 live Current。生产 host allowlist 为空或隔离未验收时始终不发 grant。

可信 launcher/host 用现有受保护 vault/OS secret store 生成并保存 custody client key，unlock 仅走可信 stdin/秘密通道。`POST /v1/client-enrollments` 登记公钥、host instance 和有限 enrollment intent，服务器返回 registration ID 与 challenge；调用者经 `/{id}/prove` 证明持钥。prove 采用 RFC 9421 的同类 POST 签名，session 组件替换为 `oasis7-enrollment`，tag=`oasis7-custody-enrollment-proof-v1`，token digest 为空值摘要；keyid 对应该登记公钥，nonce 来自绑定本次登记/正文的服务器挑战。proof 一次消费，重复只回原登记状态。未批准记录无账号/工具权限。管理页面核对 host、两跳 key fingerprint、prepared 资源及 TestSession 摘要后 fresh Passkey 批准，Account DO 将唯一 enrollment/session/key 关联与三个 purpose 原子保存；公开短码/handle 本身不能领取权限。

已批准的原 key 经 `POST /v1/client-enrollments/{id}/claim` 私领 execute/read 凭据，cleanup 只留 Owner 内部。领取使用独立 `oasis7-custody-enroll-v1` tag、绑定 enrollment/session/purpose/credential generation 的新挑战和请求 ID；没有旧 token 时 token digest 为 SHA-256(empty)，只在已批准的 claim 入口有效，不适用于 actions。未绑定 session 的初始 enrollment 使用其独立 enrollment ID/固定 schema 与服务器挑战，不混用业务签名 profile。两种签名基均由同一 core 构造并有独立向量。

raw token 只返回宿主私密通道，DO 保存 digest/generation/期限；浏览器、CLI stdout、MCP 结果和审计不能回领它。领取响应丢失时，同 key 以显式 replace=true、预期旧 generation 和新一次挑战重领，DO 原子吊销旧 generation，每 purpose 最多 3 次替换且计入控制预算。重复同 claim ID 只回领取状态；无存储 raw token 可重放时须另一次明确 replace，不能借重试隐式反复轮换。新凭据继承原根范围、余额和截止；已 admission 原件保持原 root/operation 身份，旧凭据不得新调用。丢失私钥、换 host/connector/key 或被撤销须新鲜管理批准，不能凭公开 handle 恢复。

每个可领取 purpose 永久保留本 TestSession 内的完整 credential history（初领加最多三次替换，最多四条），不随撤销、过期或当前代替换删除；TerminationPlan 无客户端凭据及该历史。当前凭据必须等于末条，generation 从 1 连续递增、替换计数与历史长度一致，领取时间不早于批准且不晚于原 purpose 截止。任何新 allocation 的 digest 不得与任何 purpose 的当前或历史 digest 重复，包括本 purpose 已退休代，避免旧 raw token 再次变为有效当前凭据。每条历史须反向绑定完整已登记 claim 原件及其 body digest；同一 claim 同时领取 Execute 与 Read 可共享 request ID，但每个 purpose 不重复该 ID，跨 purpose 只能对应同一原件的目的集合、replace 与 expected generation。旧 claim ID 重试只返回其历史 metadata，不重发秘密、不恢复旧 current、不添加历史或重扣预算；同 ID 异正文冲突。严格恢复核对上述关系，当前凭据存在但历史缺失不能默认成空或重置代数。

<a id="cloud-custody-provision"></a>
### 12.16 原生身份会话、批准与幂等开户

#### 身份、秘密与持有方

管理者 principal 是原生 identity authority 认证的 Passkey 账号；托管测试账号是由独立 login key 登录的另一 native account。管理者可以管理多个托管账号，两种 account/session 不合并。AccountAuthority 持久保存 environment、custody_account_id、manager_principal、native_identity_namespace、native_account_id、login/owner/executor 的公开 key refs；native_account_id 在原注册回执确认后才绑定，之后不可由请求覆盖。

管理浏览器的 Passkey cookie 只用于管理认证和批准，不能拿来提交被托管账号的 participant/owner 请求。Identity Worker 独占每个托管账号的 native session cookie、CSRF、pre-auth 上下文、login key envelope 与原注册/登录请求；Owner/Executor 不得到这些原值。它们不能通过 capability、claim、日志或操作结果回传给模型、UI 或执行客户端。

Identity 的这些秘密与原件存放在该角色私有的持久 DO 记录，按 environment/custody account 隔离并加密；AAD 区分 login-key、preauth、native-session 和 original-request，绑定 identity namespace、账号和记录版本。Executor 的 key envelope 同样由其角色私有持久记录持有；Owner key 可由 AccountAuthority 的私有记录持有。这里增加的是同一 Worker 包中的角色适配与 DO 类，不增加服务、crate 或第二份身份/凭证权威。原生 identity authority 继续决定账号、凭证、session、撤销与恢复；AccountAuthority 继续唯一决定 custody ACL/grant/预算/原操作。

AccountAuthority 保存非秘密 login_generation/session 状态引用，不能仅凭该投影判断 native session 有效。角色私有 DO 的备份/恢复也受同一账号的冻结、service epoch 与原件核对约束，不能恢复旧 session/key 记录后自行恢复派发。

#### 固定角色 forwarding 接口

Identity 向 Owner 和 Executor 分别提供独立的命名 service entrypoint，只有固定 typed forward 方法，没有可选 URL、HTTP method、header 或 cookie 参数。管理浏览器的认证代理路由、账号 login/provision 内部流程与下表接口分开；给 Executor 的 binding 不能调用 Owner forwarding、native login、注册或管理认证方法。

| 静态调用角色 | 固定 route enum / native 路径 | 允许条件 |
| --- | --- | --- |
| Owner | ParticipantAuthorize / /api/public/hosted-account/v2/participant/authorize | 已批准的同一 provision operation；提交既有 owner 签名的 GameParticipantRegisterV1 |
| Owner | ParticipantPolicyOriginal / 同前缀的 participant/policy-original | 只读固定 World policy；Account 先登记唯一 observation_nonce，Identity 使用自己的托管 session 转送；返回原 SignedReadRequest 与 SignedServiceResponse，Owner 按固定 World service pin、request digest、nonce 和 world 核验，不信 discovery payload |
| Owner | ParticipantSubmit、ParticipantLookup / 同前缀的 participant/submit、participant/lookup | 原 participant 请求及关联；submit 需要有效派发 claim，lookup 不重建注册 |
| Owner | ParticipantOwnerView / 同前缀的 participant/owner-view | 同一账号/世界的有界核对许可；返回供 Owner 验证的原服务响应 |
| Owner | OwnerControl / 同前缀的 entry-owner/control | 仅既有 EntryOwnerControlRequestV1 与当前 profile 允许的 owner 操作；必须匹配本服务批准/claim 或受限恢复查询许可 |
| Executor | ExecutorControl / 同前缀的 entry-executor/control | 仅既有 EntryExecutorControlRequestV1 与首版 executor profile；必须匹配原 operation、grant 与派发/读取许可 |

首版没有通用 forwarding，也不因为下游已存在某条路由就自动加入允许列表。机器注册、native-key login 的 options/complete、capabilities 和 session 查询由 Identity 的固定内部状态流程调用，不开放给 Executor 选择。

ForwardV1 最小字段：schema_revision、environment、custody_account_id、native_account_id、identity_namespace、provision_id 或 operation_id、静态 route enum、typed payload 的原 bytes 和 digest、AccountAuthority 签发的许可引用、预期 account security/service/login generation。模型不能自报有效版本或构造许可。Owner 和 Executor 的 entrypoint 将固定服务角色写入可信调用上下文；不相信请求字段/header 中的 role。

Identity 通过专用窄 binding 向 AccountAuthority 核对许可引用与其已落盘的账号、调用角色、route、payload digest、操作/attempt、期限和安全代；不是检查一个客户端传来的布尔值。对于写请求，原协议 bytes 已在账户 outbox 持久保存，并且已完成最终 claim。读取与 provisioning 许可也有固定用途、请求预算和截止，不能用于普通新写。直接让 Identity 持有 Owner 的全部管理入口不是实现方式。

每次经上述角色 forwarding 派发 participant/owner/executor 调用前，Identity 使用该托管账号自己的当前 session 核验 /session 返回的 account_id，再按现有合同取得/使用 CSRF；验证当前 AccountAuthority 许可及 Recovery Fence，固定 HTTPS origin、identity namespace/audience 和对应 native 路径，拒绝重定向。Cookie/Origin/CSRF 由适配器构造，payload 由 Rust 从原 bytes 解析校验或原样发送，不经过 JS Number 或 stringify 重编码。Native gateway 的账号、session、owner/executor 与 world fence 检查继续生效。初次机器注册/native-key login 则走既有 preauth/challenge 合同及有效 provision/managed_login 许可，不要求尚未建立的托管 session，也不把该例外开放给角色 forwarding。

policy 原件只证明固定 World 对该 nonce 关联请求的策略观察，不是托管账号或 session 存活的跨 authority 签名，也不构成 participant admission、canonical receipt 或执行许可。Account 必须将原件绑定到自己预先登记并计费的唯一 nonce、当前 native→custody 关联和观察窗口；Owner 仍须单独检查当前来源、独立 Fence、真实 session 转发及后续 admission/receipt/current owner，不能把 native issuer 对只读请求的签名当成管理者批准或账号存活证明。

session 核验、Account DO、Recovery Fence 与 native HTTP 不组成分布式事务。claim 后可能发送的请求仍按在途处理；冻结/撤销不能追回已离开 bytes。Identity 的 HTTP 成功不结算业务状态：它只返回有界原响应及传输状态，Owner/Executor 验证 service signature、world pin、correlation 与原请求后，才调用 AccountAuthority settle。传输中断保留原 operation/预算，后续按原查询/查重合同核对。

#### 登录、失效与重新登录

初次 provisioning 的明确批准可以包含一次为新托管账号建立 native session 的独立 login operation；注册本身不被当成登录。注册回执确认后，Identity 才使用同一已持久化 login key 发起原生 native-key login，用途、audience、account/context 和挑战字节均按既有标准合同验证。

session 过期、被撤销、账号不匹配、Cookie 丢失或最终在线校验失败时，标为 explicit_login_required/session_unknown，阻断需要该 session 的新出站写。读取当前有效 session 不授予新登录，也不延长 native session、custody grant、TestSession 或原请求截止。

ready 预检还需确认正常 session 到期不会早于 write_deadline/cleanup_deadline。现有 GET /session 只返回账号/认证状态，没有 expiry；因此本 lane 新增受当前托管 session 认证的固定只读 GET /api/public/hosted-account/v2/custody/session-status，返回 identity namespace、account_id、非秘密 session/credential ref 与安全版本、server_time、expires_at、idle_expires_at。Identity 核对主体，并把可用窗口限制为 min(expires_at, idle_expires_at)；不足则在批准前缩短测试窗口或先由管理端明确完成新登录，不自动重登补足。该元数据不能保证未来不撤销，每次实际调用仍在线验证；不能从当前底层 30 天/7 天常量或 Cookie Max-Age 推算权威窗口。时间字段沿新 custody DTO 的无损字符串合同。

不允许因为 capability 仍有效、客户端重新连接、outbox 重试、自动收尾或计划 Stop/Revoke，就自动发起新登录。需要新登录时，管理者在管理端用新鲜 Passkey 批准独立 managed_login intent，绑定 custody/native account、当前 login key ref、identity namespace、安全版本、原 blocked operation 引用与截止；AccountAuthority 消费批准后，Identity 才创建新的 login operation。它只建立登录态，不重启任务、不发 grant，也不替换未知业务原件。原生凭证/账号已撤销或冻结时仍须原 authority 拒绝；不能生成新 key、重新注册或换账号绕过。

登录 complete 响应丢失时先核对原登录；已完成但未拿到原 cookie，只能回报需要明确新登录，不能重放 complete 来重新领取 cookie。新登录成功后还须重新核验 Account DO、service epoch、当前 owner/delegation/task fence 和原操作，不能恢复旧 claim 或旧 grant。

普通执行被冻结或写 lease 到期时，已批准收尾计划不取得新的身份权限。它只能在自己的短 cleanup 截止内、已有当前合法 native session、仍有效的管理来源安全版本与 Account DO ACL 下使用固定 Stop/Revoke。身份撤销、账号恢复冻结或 session 失效使条件不满足时记录 cleanup_blocked/unknown；只能转入既有独立人工批准的恢复通道，不自动重登或延长授权。恢复通道也不能绕过原生账号冻结、失效凭证或缺失 owner。

#### 原生批准与当前来源核验接口

管理批准是原生 identity authority 新增的窄业务能力，不能把现有普通 reauth 成功文本当成已存在的 custody proof。固定以下 target API；原生 Rust handler 已使用 custody core 的真实 DTO 实现并默认关闭，实际配置和验证入口见[实现说明](../../../crates/oasis7_custody_worker/README.md#native-identity-authority-configuration)。Identity/Owner 的 Cloudflare 适配与完整自测仍待接通。它们不修改 World Service wire，也不建立新凭证数据库。

共同前缀为 /api/public/hosted-account/v2/custody/approvals；管理 options/complete 仍须同一原生管理 session、精确 Origin/CSRF、大小限制、标准 WebAuthn UV/挑战校验与限流。管理 proof 与托管账号 session 分别存取/验证，不能用托管账号 native-key 登录来完成管理 Passkey 批准。

| target API | 请求与结果 | 提交/重试语义 |
| --- | --- | --- |
| POST /options | approval_request_id、闭集 CustodyIntentV1、目标 environment/custody account、配置内的 AccountAuthority audience、expected policy/security/service epoch。CustodyIntentV1 包含准确动作与参数/资源/world/预算/期限；管理者 principal 从当前 session 得出。native 用共享 Rust 编码计算 intent_digest，返回 approval_id、ceremony_id/operation_id、标准 Passkey options、intent_digest、audience 和 ceremony_expires_at。 | 按管理 principal + audience + approval_request_id 条件保存 intent digest 与同一个 ceremony。相同 ID 同 body 回原 pending options，不重新生成挑战；不同 body 冲突；已完成/过期/取消分别回原状态，不用重试暗中开启新批准。ceremony 沿现有 120 秒与有界失败策略，不能把其 TTL 当 proof TTL。 |
| POST /complete | approval_id、原 ceremony_id/operation_id、标准 WebAuthn assertion 与预期 intent_digest。验证 assertion 后，事务内重读同一管理 session/credential/account 安全版本、原 intent/ceremony 和期限；一次消费并保存 CustodyApprovalProofV1 的精确原 bytes，然后返回 proof。 | proof 使用独立 nonce，绑定 §12.5 全部字段，另绑定原管理 session ref 与 approval_request_id；issued_at/expires_at 的窗口最多 30 秒。签名与证明原件先保存再响应。同一 complete operation ID 不同 body 冲突；同一原 complete 重试只能取得同一 nonce/issued_at/bytes，不能延长 30 秒或另发 proof；已过期/撤销只返回 expired/revoked/原完成状态，后续须显式新批准。 |
| POST /verify | 仅允许 Identity 的专用 native service adapter；提交完整原 proof、其 digest、expected audience/intent digest、purpose=consume_approval 或 inspect_origin，以及 Owner 这次核验的 verify_nonce。native 核对 proof 签名/保存的原件及当前管理 account/session/credential，返回签名的 ApprovalVerificationV1，绑定 request purpose、proof_digest、approval_request_id、audience、intent_digest、verify_nonce、current security versions、observed_at、proof_expires_at、source_session_expires_at/idle_expires_at，并分别报告 proof_state 与 origin_state。 | consume_approval 必须 proof 未过期且 origin 当前有效；Owner 核对 native 签名、自己产生并登记的 verify_nonce、完整绑定及 proof 截止，立即按 §12.5 在 DO 消费。inspect_origin 只报告原批准来源当前是否 active，不因 proof 已过 30 秒就使已消费的有界计划自动失效；它不延长 proof，不允许重消费，也不发 session/grant。结果不能被 Identity 改成无出处的 verified=true，不能跨 intent/账号/核验purpose复用。 |

CustodyApprovalProofV1 的签名由原生 identity authority 持有的专用 approval signing key 产生，采用 core 唯一 canonical 编码入口和独立域 oasis7.custody.approval.v1；ApprovalVerificationV1 使用独立域 oasis7.custody.approval-verification.v1。版本、key_id、所有字段和 domain 均在 golden vectors 中冻结。Owner 的 native public-key pins 按环境配置，不从 proof 携带的任意 URL/JWKS 取信任；轮换保留明确的在途验证窗口，不接受未知 key_id。

/verify 使用 Identity 独占的最小 native-adapter service credential 经固定 TLS origin 认证，凭据只允许该只读核验接口，不获得账号登录、注册、owner 或 executor 权限；它是角色配置中的独立运维 secret，不是协议私钥，也不发给 Owner/Executor/客户端。native 对这个特定非浏览器接口使用明确的 service-auth 合同，不能通过放宽已有 Cookie/Origin/CSRF 路由来实现；options/complete 仍沿浏览器原有防护。该 credential 与 native approval signing key 分环境部署/轮换，四角色配置及权限负例须列出。

inspect_origin 仅可供 AccountAuthority 已登记的一次批准来源查询使用。授权仍来自该 DO 中未过期、未撤销的 Provision/TestSession/收尾或只读计划，不能凭 origin_state=active 创建新计划、增权或延长截止。native 保留原 approval 来源及安全版本记录，至少覆盖这版合同允许的最长计划/只读期限与原操作核对窗口；缺历史就返回 unavailable，Account DO 停止对应动作。ready 的已知到期上限同时取管理来源 session 与被托管 native session 的最早 expires_at/idle_expires_at；这些是当前观测，不保证之后不撤销，运行期间仍逐次检查当前状态。

complete 丢响应时，客户端保留原 approval_request_id、ceremony 和 complete 原件并核对/重放同一 complete。native 已提交且 proof 尚有效时只回同一个 proof；proof 已过期时不重新签发，先核对 Owner 是否已消费并受理原 intent，未受理才允许明确新批准。Owner 受理响应丢失时查同一 custody operation；不能因为缺响应再创建一个不同 id 的 owner 动作。原生原子消费与 Account DO 消费不是同一事务，已签 proof 仍受 §12.5 最多 30 秒交接窗口及账号侧冻结/撤销确认规则约束。

上述命名 entrypoint 是目标权限合同，不假设 worker 0.8.7 已自动生成这些 RPC。优先用经过 workerd 验证的 Rust/wasm-bindgen bindings；需要命名 WorkerEntrypoint 时，可用最小 JS/TS class 暴露固定方法并注入静态角色，再把有界 bytes 交给 Rust。JS 不做授权、业务编解码或 session 持久化。不得改成一个向 Executor 暴露 Identity 默认 fetch 全路由的 binding 来凑通调用。必须验证实际 exports、角色 bindings、default entrypoint 和部署权限的负例；不能满足则该角色接线保持关闭。

#### 新托管测试账号的幂等开户

##### 授权根与 HTTP 行为

首版只创建新的受控测试身份；既有 native 账号接管、旧 vault 导入、owner 迁移和跨账号合并默认关闭。它们需要另行冻结旧权威证明、当前 owner 轮换和历史原件核对方案，不能凭知道 account ID、公钥或拥有一个 Passkey 登录就接入。

环境配置显式列出可创建测试账号的 bootstrap manager principals，每项包含可信 native identity namespace、manager account ID 与允许环境；不使用首次登录者、邮箱字符串、模型参数或浏览器 label 建立管理员身份。该 allowlist 只允许准备本人的新账号创建意图，不绕过新鲜批准、Account DO ACL 或世界准入；后续账号管理仍以 Account DO 为权威。首版不提供管理 ACL 转让/增员入口。

以下为固定的新 custody 接口（不是已有 native API）：

| 接口 | 最小合同 |
| --- | --- |
| GET /v1/management/session-status | 同原生 RP 的固定 HTTPS origin，经原生 Passkey management_session 返回当前非秘密元数据；不续期，不以 native-key session 建管理者。 |
| POST /v1/account-provisions | 当前合法管理会话、client_request_id、environment、固定 world/identity route refs、用途及有界 provision 计划；校验 bootstrap allowlist。按 environment + verified manager principal + client_request_id 稳定定位同一个 custody account/provision，Account DO 条件创建 draft ACL/原 intent/digest；相同 ID 同 body 返回原 draft，不同 body 冲突。该步骤不生成 key、不创建 native account、不派发世界写。 |
| POST /v1/manager-revocations | 当前管理者主动冻结自己的整个 custody 关联目录；只接受 schema version 与稳定 operation_id，沿原生 POST Origin/CSRF 管理认证。先关闭关联，再持久记录全部账号目标；每次最多确认八个，原请求重试接续，未确认或在途原件未核清保持 pending。该动作不撤销原生 identity credential，也不代替原生撤销事件的自动传播。 |
| POST /v1/account-provisions/{id}/approve | 对原 intent digest、custody account、环境、完整步骤范围/预算/截止及 policy/security version 的新鲜 Passkey proof；Account DO 同事务消费批准并把 draft 转为 provisioning，登记原 outbox 和审计，重复同批准不产生第二套步骤。 |
| GET /v1/account-provisions/{id} | 当前仍有 ACL 的管理主体读取非秘密阶段、公开 refs、原 operation/receipt 引用和 blocked/unknown；不返回 seed、cookie、完整 assertion 或重新领取秘密。 |

Provision 批准原件必须显式签入 `purpose` 与原 draft 的完整 `plan_digest`，并逐项绑定 provision/account、environment、bootstrap version、固定 identity/world routes 与完整 world pins、native identity namespace、注册/登录 request IDs、deadline 和 transport budget。`intent_digest` 与 `plan_digest` 使用不同分域，不能直接视为相等或仅凭相似字段推断授权。缺少新增必需绑定的旧批准原件拒绝消费，不以默认用途或空摘要补齐。 新 Account draft 的目标 authority security epoch 沿现有初始化合同为 0；仅上述完整绑定的 Provision bootstrap 批准允许该目标代，Owner 消费时仍须匹配真实 pristine draft 的当前代。此例外不适用于其他 intent，也不放宽管理者来源、原生凭证/会话或 service epoch 的验证。Account DO 在同一个持久事务内消费已核验 proof、将同一 draft 的 ProvisionRecord 转为 approved，并写入三个固定角色 key 准备阶段的有界原 outbox 和审计；异步验签后必须在提交点重验当前来源、ACL/配置版本、冻结状态和时钟。该提交本身不生成 key、不创建身份、不派发任何阶段；取消或冻结先关闭阶段准入并保留这些原件。

批准入口单独由显式 `CUSTODY_PROVISION_APPROVAL_ENABLED=true` 开启；缺省不启用，单有该开关也不能绕过其他权威。Identity 完成同源管理认证后，只将闭集批准请求与当前管理元数据送往 `OwnerManagement.approveProvision`。Owner 从已登记目录核对同一管理者、provision/account、完整且未冻结的关联库存、初始化状态及当前配置；Account 根据真实 draft 与固定配置建立目标 authority，不信任请求自报的当前版本。Account 先登记本次 native verify nonce，再调用独立 `IdentityNativeApproval.verifyApproval`，最后重读目录与独立 Recovery Fence，核对签名验证原件后在同一 Account SQL 事务消费。目录与 Fence 观察保留版本及观测时间，提交时最多接受五秒前的观察并重验本地冻结、配置和时钟；这些跨 DO/原生读取不构成分布式原子事务，原生撤销到全部账号关闭之间仍按既有 pending 屏障处理。

Recovery Fence 的 `RecoveryFenceReader.read` 是固定命名的只读 binding，读取该独立 Worker 的固定持久命名空间，不接受调用者指定 epoch 或 DO 地址，默认 HTTP ingress 不转发它。新命名空间仍初始化为冻结；普通批准不能启用或解除它。首次启用与恢复必须由独立、受控的恢复合同完成，在该合同未完成或 Fence 仍冻结时，批准正向路径必须拒绝。隔离测试可明确提供测试 authority 证明其余调用链，但不能据此宣称真实 Fence 已启用或生产批准可用。成功批准仅返回非秘密 operation/阶段引用，不回传原生 proof、session 或内部来源上下文。

**独立 RF 激活合同：** 激活不是 `tighten` 的反向调用，也不接受普通 Owner proof 或请求中的 enabled bool。部署侧固定 environment、独立 authority ID、实际 RF Durable Object ID 和离线恢复公钥集合；集合包含 2 至 5 个不同 key ID 与公钥，固定 quorum 至少为 2。发布配置只安装公钥，不安装恢复私钥；普通 Identity/Owner/Executor 不绑定控制入口。首次启用和新代恢复均须对闭合 `RecoveryActivationManifestV1` 的唯一 canonical CBOR 域取得 quorum 个不同固定公钥的严格 Ed25519 签名。manifest 绑定固定 scope、policy digest、当前完整 RF state digest、原 epoch、严格加一的新 epoch、operation ID、最多五分钟的新鲜期限和非空 evidence digest。evidence digest 是独立恢复操作者对旧 writer 隔离、Stop/Revoke/原件与库存核对记录的批准引用，不冒充代码已自动验证这些外部事实；无法完成核对时不得签发 manifest。签名只允许推进服务代，不授予任何账号 grant、预算、SID 或业务写能力。

RF 的条件 SQL 事务必须在异步验签之后重读 policy/scope、完整库存摘要、旧 epoch、时钟与原件，并原子保留已消费 manifest、旧代冻结墓碑、新代 active fence 及审计；响应丢失只回读同一原件，不重复推进、续期或以旧回执解冻当前代。历史容量有界，容量满仅阻止新的激活，不能阻止本地收紧冻结。审计必须逐 revision 从保留的原件和冻结转换重放，核对归属、连续序号、闭集 payload 和对应 state digest；仅核对行数不足以证明完整性。配置/namespace/policy 不符、未知或损坏/缺失历史保持 disabled/pending，不能把旧 active 记录当作新 namespace bootstrap。旧代墓碑不可删除；本合同不提供恢复根集合轮换或 RF 自身回滚的捷径。同一物理 DO 的整库一致 PITR 或全量丢失无法由同 DO 内的 ledger 自证，属于运维禁区，不在部分库存损坏检测的保证内；恢复不能自动复用旧 manifest，应隔离旧服务并按独立恢复权威重新核对 namespace 和下游状态。既有 `RecoveryFence.tighten` 永远不能 reenable，独立控制入口缺省关闭且不由默认 HTTP 转发。开启新服务代后，旧账号状态仍须冻结并按现有恢复流程取得新批准；manifest 不重写旧 proof、原件、预算或 grant 的 service epoch。实际部署、离线签发与外部核对须另行授权，单元/隔离平台测试不代签这些义务。

稳定定位映射由服务端固定版本计算/存储；调用者不能以路径参数选择并初始化一个已存在的任意 Account DO。空 draft 的 ACL 仅限创建者读取/批准该草稿；native account 尚未建立，不能据此发 grant。allowlist 配置变化与当前管理凭证有效性在批准/派发前重验。

管理认证适配复用原生固定 GET/POST `/api/public/hosted-account/v2/custody/management-session-status` 与 `CustodySessionStatusV1`。GET 只读；POST 空正文仍经过现有 Origin/CSRF 与 `management_session`。Identity 只转发当前唯一 SID 和原始精确 Origin/CSRF；Worker 须位于同一 HTTPS RP origin，不伪造跨域身份。Owner 的 `ManagerAssociations` DO 只协调关联与冻结，`AccountAuthority` 仍是账号权威。管理请求的 Identity→Owner binding 只配置 `OwnerManagement` 的固定命名方法；原生 proof verify 使用单独 `IdentityNativeApproval`。实际 builder 会把 Rust free function 挂到默认 entrypoint，因此部署使用只暴露 fetch 与固定 scheduled 回调、没有生成 RPC 的 wrapper，不能把 raw 产物作为部署入口而额外开放默认 RPC。

Owner 保留权威的 manager→custody account 关联目录，包括 pending provisioning。关联登记须先于任何 key 生成、native/world 副作用或可用 grant，并与稳定 provision identity 一起持久化；D1 只投影，不拿异步列表当全集。目录登记与 Account DO 初始化可分阶段幂等完成，不能假定跨 DO 原子提交。撤销处理先关闭该管理来源的新关联/批准，再对已登记全部 Account DO 执行冻结/升代并收齐确认。冻结 tombstone 可以先于账号初始化，迟到 provisioning 不得覆盖它。

参与范围以原生 authority 持久登记的完整 registry 为准，不能将 namespace 中所有账号默认视为 custody 参与者。首批由独立运维配置显式声明至多 256 个当前 namespace 中已存在且唯一的管理账号，首次启动持久登记，此后配置必须与登记集合一致；不提供隐式增删或把丢失 registry 当空集合的恢复路径。非参与者的正常登录、logout、吊销和身份维护沿既有合同，不产生 custody 事件，也不要求 Owner 关闭确认。所有 custody options、complete 和 verify 都必须检查持久参与资格；首次 overflow 后永久拒绝 custody 批准，ACK 和 fresh native login 均不能解除该 fence。托管账号到 custody 的反向登记仍须随开户流程另行接通，不能将其自报为管理者。

自动原生撤销采用独立、窄权限的持久事件通道，不能要求已撤销的管理会话继续发请求，也不能扩大批准 `/verify` 的只读权限。首批事件桥仅支持真实 SQLite identity store；在 Tablestore 上启用必须拒绝，不能借通用 store 接口声称已有同等保证。持久注册后移除事件桥配置也必须拒绝启动。启用该通道时，原生账号状态/安全版本、凭证状态/版本和会话撤销或安全期限缩短的变更，在同一个 identity 事务中保存固定签名原件；普通使用时间或延长 idle 期限不构成撤销。事件原件按固定 namespace/environment/身份代、manager 和来源记录绑定；独立私有 poll/ack 凭据不得与批准核验、登录或签名秘密共用。托管历史容量不能阻止原生 logout、凭证吊销、账号失效等安全事实提交。完整签名事件原件最多保留 4096 份（含已确认历史），不自动删除；达到上限后，在同一原生事务为该参与者设置永久 custody fence，并在独立预留记录保存不可替换的首个 overflow trigger 与原生签名 checkpoint。后续安全变更继续提交，只更新有界 checkpoint 的非递减水位、前 checkpoint 摘要、最新事件摘要及摘要链；即使计数达到表示上限，也不能重新阻断原生失效。checkpoint 是明确的压缩边界，不保存此后每个事件全文，也不冒充这些事件逐条取得 Owner 确认。原先完整原件和首次 trigger 仍须保留。Owner 每个管理者最多保存 256 份来源确认，容量不足只使 custody pending，不能回滚原生身份失效。

Owner 使用独立命名撤销入口验证固定原生签名，持久关闭目录并接续有界 fan-out。多个来源事件分别绑定到同一永久 manager-wide 冻结，保留第一份不可替换的撤销 notice；后续事件不能因 operation ID 不同而丢失，也不能重开目录。无现存目录时也保存冻结 tombstone，阻止迟到初始化，但未知库存保持 pending；没有独立全集证据时不能把新建空目录当作零关联的证明。旧快照缺少本版本的库存完整标记时同样保持 pending，不能自动升级为已关闭；当前保守实现不提供 fresh login 自动解冻或新建关联。只有目录全集和全部账号确认、在途原件均可核清，才产生固定 Owner key 签发、绑定精确 source event digest 与实际 closure/inventory digest 的 closed 回执；原生 ack 必须核验该回执。未确认始终为 custody pending，identity revoked 单独成立；未知库存不能签 closed。离线身份代推进还须验证原代登记参与者的全部完整事件、永久 checkpoint、精确 trigger 的 Owner 关闭回执以及 writer fence；不扩大到普通非参与账号。checkpoint 签名绑定 registry 摘要、namespace/epoch、参与者、首 trigger、最新事件摘要、水位和前 checkpoint 摘要。首次 overflow 与永久 fence 同事务，Owner 对该精确 trigger 的永久 manager-wide 关闭证明可确认授权入口已关闭；后续 checkpoint 仅记录原生变更连续性，不能据旧 ACK 自动解冻或声称逐条下游确认。缺少这些证据拒绝推进。该通道不获得世界 Stop/Revoke、管理批准、密钥生成或重新登录权限。

目录缺失、版本不明或未收齐确认时不宣称 custody authorization closed，也不继续新增授权。该目录是 Owner 内的路由/撤销协调记录，不另设 grant/预算 authority。

##### 原生托管反向绑定与首次会话

反向绑定使用固定服务 pin 验证的签名事实，不采纳自报 native account、公钥或 manager registry。
当前 native HTTP 持有 IdentityService/issuer 锁并同步处理请求；不能在锁内回调 Identity→Account 再进入 native 来源检查。
采用分阶段 challenge/statement，native 只执行本地 crypto/SQL，Identity 在两次 native 请求之间调用固定 Owner RPC。
这仍使用原 Account authority，不增加另一份 custody ACL/grant authority。

1. Identity 先加密保存原 preauth Cookie/CSRF，再使用原计划派生的 registration operation 完成既有 machine registration。
   native 从已提交的 machine_register receipt、ceremony、credential 和原 owner-context 推导账号/公钥/摘要，
   保存 30 秒一次性 binding nonce，并以固定 pin 的 native authority 签发 SignedNativeBindingChallengeV1。
   闭合字段为 api_version、challenge_id、nonce、identity_namespace、identity_instance_epoch、registration_operation_id、
   registration_request_digest、registration_receipt_digest、native_account_id、native_credential_ref、login_public_key、
   owner_context_ref、issued_at、expires_at。owner_context_ref 是 native 生成的摘要，不是 Cookie；调用者不能指定目标账号/key。
   nonce 为 32 字节的 canonical lowercase hex；签名采用既有无 padding base64url 的 64 字节格式。
   签名 domain 固定为 oasis7-custody-native-binding-challenge-v1，使用 Rust canonical bytes。
   普通注册 JSON 回执没有因此获得跨 authority 签名权威。
   options 只接原 registration_operation_id 与 Account 本次已登记的 binding_operation_id，后者成为 challenge_id。
   同 attempt 返回原 nonce且不续期；新 attempt 可以为同账号/key 创建新 nonce，每原注册最多永久保留 16 个挑战。
   达限只拒新增挑战，不能阻止普通 logout/revoke。固定只读 lookup 以原 preauth 核对并返回原 signed receipt 或 404，
   即使原 statement/nonce 已过期也不丢原事实；Owner 只能接受与自己已登记 statement 匹配的原回执，不能据 lookup 新授权。
   registration_request_digest 使用 namespace/epoch、原 operation、公钥、display_name 和原 audience 的共享 canonical public projection；
   native 从原 ceremony 重构，Owner 从自己的原登记请求及固定配置计算，不使用 native 私有 HMAC request digest。
   bootstrap v1 的 display_name 固定为 Oasis7 managed account，不接受模型/HTTP 请求改名；该常量由原 bootstrap version 绑定。
   原 registration/login operation 从 environment、custody/provision、namespace/epoch、原 request ID 及 Identity 公钥稳定派生，
   不直接使用调用者 ID，也不因未知结果重新派生身份或操作。
   registration_receipt_digest 是 native 签名的原 receipt fact；Owner 验挑战后登记，不假称普通 JSON 可重建 native 内部回执。
2. Account 仅从已批准 Provision、Identity role result、原注册请求及已验 native 签名挑战生成 OwnerNativeBindingStatementV1。
   statement 嵌入完整 challenge，再绑定 environment、custody_account_id、provision_id、approved_plan_digest、login_key_ref、
   binding_operation_id、permit_id、source_revision、account_authority_revision、service_epoch、service_identity、signer_key_id、
   authority_observed_at 和自身 expires_at。Account key ref/请求摘要必须与 native 挑战完全匹配，不能签请求回显字段。
   先持久预留/claim，核对真实来源、目录、独立 Fence 和最终时钟，保存原 bytes/digest/nonce/attempt 后才签发。
   使用专用服务 key 和独立 domain oasis7-custody-native-binding-statement-v1，不扩大现 approval-verify secret。
   最早真实来源/目录/Fence 观察为 authority_observed_at；截止最多其后 5 秒，且不越 challenge/plan/source 截止。
   转发或重读不能续期；这仍是有界观察，不宣称分布式事务。
3. native 只信独立配置并持久固定的 Owner 服务 key ID/public key、environment、service identity/epoch。
   不信 statement 自带公钥、每账号 Owner role key 或普通管理者 key；pin 轮换单调推进，旧 pin 仅供历史核对。
   接线缺省关闭，不创建/安装运维 key，不解除 Recovery Fence。
   一个 native SQL 事务重验 nonce、注册 receipt/credential/key/owner-context、账号状态、namespace/epoch、签名和时钟，
   complete 是闭合 NativeBindingCompleteV1，包含 signed statement 与 login_key_signature；fresh 持钥签名使用
   oasis7-custody-native-binding-possession-v1 并绑定完整 statement digest，只能由该原 login key 生成，不提供任意字节签名。
   消费 nonce并保存不可替换的 managed-native→custody/provision/login-key binding、receipt 和 audit。
   native 以独立 domain oasis7-custody-native-binding-receipt-v1 签署回执，Account 精确验收后才推进。
   冲突绑定拒绝；丢响应不换身份或覆盖原件；历史同 operation/body 只恢复原事实，不重新授权。

Owner pin 的持久水位按 native namespace 固定，不能只随部署环境变量变更。
首次仅在该 namespace 没有任何 managed challenge/binding/outcome/失效库存时，
允许把显式配置的当前 pin 与 native root key/instance epoch 同 SQL 固定；
旧 managed 库存缺水位时属于 unknown/pending，不能自动补为可用。
同一 service epoch 只能精确使用原 pin；降代、同代换 key、重复启用已退役 key 均拒绝新准入。
更高 epoch 需要独立 OwnerPinRotationManifestV1：绑定原 ledger digest、native namespace/instance epoch、
原/新 pin、operation_id、issued_at/expires_at（最多五分钟），同时带已固定 native root 的专用域签名
与新 Owner key 对完整 manifest digest 的持钥签名。manifest 只从固定运维配置读取，
不接受普通管理批准、公开请求 bool 或 statement 自报的签名 key。
同 SQL 验证并推进水位、保存旧 pin 指纹墓碑；旧 key 只可核对已存历史事实，不能再签发新准入。
初版最多保留 256 次轮换墓碑，达到容量只拒绝后续轮换，不影响原生 logout/revoke。
native root 或 instance epoch 变化不由该 key 轮换协议隐式激活，仍 pending，等待独立恢复合同。
业务开关关闭时保留水位；重新加载同一当前 pin 不得清除任何既有 frozen/pending 或旧 key 墓碑。
历史 receipt 查询不改 pin 或水位；pin 不匹配时不能签 custody active session status 或按托管登录原件发新 SID。
服务 pin 轮换不等同于吊销底层 native credential；普通 native-key 登录仍遵循原有持钥协议，
其 SID 不能据此恢复退役 binding 的 custody 活性或 grant。
普通原生账号的独立 logout/revoke 不依赖该 pin 水位成功。

managed binding 库存与 manager registry 分离。托管账号/credential/session 失效按可信 binding 关闭相应 custody 入口，
不得路由成 manager-wide 目录，不影响普通非参与者。缺失、legacy、未知库存仍 pending；容量满只拒绝新增 custody 尝试，
不能阻止原生 logout、撤销和失效事实持久化。业务关闭还需完整库存/在途原件及独立 Stop/Revoke，不能凭 binding 签 closed。

注册、binding、初次 login 各用 Account 先登记的阶段原件，每次预留 64 transport，包含来源预检、固定角色许可/签名、
native 往返和最终核对；真实接线不得越该上界。同原件不恢复发送权，unknown 不退款，新尝试另扣并恢复同一账号/key。
Provision 总上限仍 400，旧批准不自动提高；不足保持 pending/recovery_required。
Owner 只见非秘密引用/摘要/回执；preauth、CSRF、原请求和 session 由 Identity 私有 DO 加密，AAD 区分
login-key/preauth/native-session/original-request，并绑定 environment、namespace/epoch、custody account、provision、record ID/version。

首次 login 只消费原 Provision 明示的一次 operation；原 SID 先加密持久化，再核托管账号自己的 session-status，
才上报非秘密 generation/status。complete 已提交但未保存原 Cookie 时只记 explicit_login_required，
禁止重放领 Cookie或用旧计划新建登录；fresh Passkey ManagedLogin 批准须原子消费并登记新登录原件。
恢复/heartbeat 不能扩大截止或预算；native/session-ready 不等同于 participant-bound 或世界可执行。
status 的 generation 是 native 持久 binding 失效水位；Identity 私有 SID 槽代独立记录，不能混用或由请求自报。
托管登录的原生 options 另带显式 managed_operation_id（规范的 32-byte base64url ID），
作为已批准逻辑登录与 native ceremony 的同一永久操作标识；不把随机响应 ID 当作批准目标。
该扩展仅在托管适配开启时用于 login，普通 native-key 客户端不带该字段仍由服务随机分配 ID，
不改变 bind/reauth 或既有 operation_id 字段的行为。
在 native pending 原件保留期内，同一托管 ID 的重试必须精确匹配原 preauth owner、Origin、
namespace/instance epoch、public key 与用途，只返回原 challenge，不换 nonce、不延长截止；已过期即拒绝。
未持钥证明的 pending 原件按既有 TTL 清理后，native 只能回答该 ID 当前 unknown，不能证明从未出现过。
Account/Identity 仍永久保留原逻辑 ID、请求原件、截止与 claim，不能借 native 的 unknown
重获旧 attempt 发送权、替换已存 options/complete 或视为已成功；新的 native challenge 本身不是 custody 授权。
成功 complete 必须同 SQL 留永久已提交原件或墓碑，清理 ceremony 后也不得按该 ID 再发起登录。
该提交原件不保存或重放 SID Cookie；same-complete 或 options 重放只能返回已提交事实/拒绝，
不能领取旧 Cookie。未持钥证明的 pending options 保留既有期限和配额，不能据此制造永久成功墓碑。
status/invalidation 的 service_epoch 指该绑定原 Owner statement 中的 custody service epoch，
不使用 native RP 的 instance epoch 代填；后者继续单独核对 namespace、原账号、credential 与会话。

Identity manifest 已存 complete 事实但 Account 未收到提交时，公开 continue 必须能够调用固定、纯只读的历史通道。
该通道仅按 immutable environment/account/provision、原阶段与 logical operation、首次产生结果的 result_permit_id
返回原 typed Root fact 和已存 SID 的非秘密 attestation；不发 GetPermit、不读 KEK/Cookie、不签名、不发 native 请求，
也不初始化或修改 manifest。缺原件返回 unknown；旧记录无法唯一确定结果所属 attempt 时仍 pending。
Account 独立强验 Root 签名、原 namespace/key/pin、已登记请求及已知 B1 statement，永久归档该原事实与原观察/截止。
归档本身不推进 NativeSessionReady、不激活 epoch、不清除 frozen/cancel、不产生预算或恢复发送权。
若原结果与原批准仍满足当下全部新鲜度/来源/目录/Fence/时钟约束，可经原最终 SQL 准入提交；
不能拿原 claim 时间重验已过期的 RootStatus 并宣布当前可用。

已存 SID 的当前状态恢复必须另有有界 SessionStatusRecovery 原件：只允许同一已登记 login 与该私有 SID generation，
用独立 nonce、固定只读 native session-status 路径和新鲜来源/目录/Fence；不调用 options/complete、不再签登录。
每个恢复原件先预扣 64 transport 和最多八次 oracle 读，不能续旧 permit；
首次登录恢复从仍有效的原 Provision 预算扣除，ManagedLogin 恢复只从该次 fresh 批准尚余的预算扣除。
原截止、当前权限或余额不足时只保留历史 pending/frozen，不自动加额或借旧 grant，须取得另一个适用的有效批准。
SID 确实未保存时只记录 explicit_login_required；恢复或历史查询不能改为重新领取 Cookie。
若该收紧结果到 Account 的提交丢失，固定 Identity history named capability 可另外返回 typed missing-session attestation，明确区别于 Root 签名事实。它只能来自已完成且持久保存 explicit_login_required 的 manifest，绑定原 scope、phase、native operation、原 permit/result permit 与稳定 intent digest；当前仅允许初始或 ManagedLogin 的原 permit，不猜测 legacy/recovery 归属。Account 仅对自己已 claimed 且尚无成功结果的精确原件记录 explicit_login_required，允许在到期、冻结或取消后收紧，但不得改 stage、epoch、预算、nonce、期限或解冻。该通道不能成为状态恢复或 Root 验签的替代，也不能覆盖已有成功会话事实。未知、歧义或损坏库存继续 pending。
新 nonce 的 RootStatus 经真实验签和最终 SQL 当前准入后才可声明 NativeSessionReady；
与归档的历史状态及独立 native binding 失效水位分开保存，失败不能复活旧状态或旧发送权。

原生阶段必须先在 Account 本地预留固定核验原件、nonce 与预算，才可执行来源检查 I/O。
首次 inspect 的目录预检查只读取已关联管理者的当前目录、冻结与独立 Fence；
它可以定位已验签原批准保存的 immutable source refs，但不能把旧 server_time 改成新鲜活性证明。
该只读历史预检查只能 claim 已预留的 inspect 原件，不能签发阶段许可、签名、密钥或业务发送权。
inspect 返回当次 nonce 绑定的真实 native 签名后，必须再走正常新鲜管理会话检查、目录/Fence 与最终 SQL 重验，
才可 claim 阶段或返回 oracle 许可。所有 oracle 均先本地耗用有界读信用，再执行远端检查。

初始 Provision 的 target epoch 0 仅用于完整 bootstrap 原任务，不能扩展给 ManagedLogin 等其它 intent。
绑定回执经真实验签及当前准入后，Account 同 SQL 保存 native target/key 关联与一次性 bootstrap 激活记录，
将实际 custody target authority 从 0 单调推进到 1；这不是借用 native account 的 security epoch，两者独立。
原 Provision 的来源核对仍验证已消费 proof 的完整原 context，但每次派发另外精确核对当前阶段的实际 target epoch。
仅该原绑定结果允许记录 0→1 接续；撤销/冻结或其它升代都会使旧阶段许可失效，不能拿旧 proof 恢复旧代。
新的 ManagedLogin 须对当前正数 target epoch 作 fresh 批准，精确绑定 native account、login key、namespace、
blocked operation 和有界期限/预算；批准消费与新登录原件同事务，不复用原 Provision nonce或旧操作发送权。
ManagedLogin intent 必签新的 login_request_id 和 transport_limit（64..400）；kind 本身限定 purpose，
旧 proof 缺这些字段即拒绝，不从旧 Provision 借额度，不根据剩余 deadline 猜出额外预算。
同一个 Account 对 ManagedLogin 批准的原 native nonce/proof 保留全局唯一消费关联，
覆盖 pending 与 consumed 原件及恢复校验；更换未签署的 operation_id 不能重复消费同一批准或再次获得预算。
相同 operation 的精确历史重试只返回原结果，不签发新操作，也不清除该唯一关联。
管理 HTTP 使用相同 Origin/CSRF/Passkey 管理会话前置：
`POST /v1/account-provisions/{id}/managed-logins/approve` 只消费本次 ManagedLogin 批准并登记新原件；
`POST /v1/account-provisions/{id}/managed-logins/continue` 只选择该已批准 operation 执行一次登录。
后者沿用只含 api_version/operation_id 的 continuation body，不允许传 session、key、native endpoint 或 current authority。
原 Provision 的 continue 只按 Account 已存阶段接续开户、绑定和首次登录；
Account 已提交结果而 RPC 丢响应时以原事实推进，unknown 同 attempt 不重发，不能创建第二次首次登录。
HTTP 入口仍要求 Identity 本次真实管理会话观察不超过五秒；该观察用于鉴别这次调用，不能充当长流程后来阶段的 grant 证据。
长 provisioning 循环与其 participant 接续只用该已认证来源的精确 account/session/credential/version/security/policy 引用和绝对 expiry 核对原批准，
不改写旧 server_time 或签名。每个新阶段仍先登记独立 nonce，通过真实 native inspect、当前 Directory/Fence 和最终 SQL 才授予许可；
跨阶段超过五秒本身不撤销已提交事实，也不免除下一阶段自己的五秒 live gate。独立 ManagedLogin continue 的入口仍核新鲜管理观察。
这些原生业务入口另受默认关闭的 native provisioning 配置控制，不改变普通 metadata/默认 RPC 的关闭边界。
Trace refs: O7-REQ-844f4b30-b7d5-4651-941e-17c21f37b4ac; O7-REQ-b67f1215-00f3-4fbd-b8df-1c2ead4b3945; O7-REQ-17af77d2-a02e-41ea-8185-cbc0650e69e3.

##### 分阶段提交与恢复

AccountAuthority 持久保存 ProvisionRecord：provision_id、client_request_id/body digest、environment、manager principal/credential source、bootstrap config version、custody account ID、identity/world pins、批准 nonce/intent digest/截止/调用预算、各角色 key refs、native account ID、各阶段原 operation/correlation/receipt 与状态。它可以映射为现有 AccountRecord/Operation/Outbox/KeyEnvelope 记录，不强制为每个阶段新建表或 crate。

| 阶段 | 持有方与提交点 | 失败/重试规则 |
| --- | --- | --- |
| draft | Owner Account DO 先保存稳定 intent、临时受限 ACL 和关联目录；无外部副作用 | 重复返回原记录；不因刷新 UI 创建新账号 |
| approved | 验证新鲜 proof 后，DO 事务重验 allowlist/ACL/版本，消费 proof 并写有界计划 | 证明过期/撤销/摘要变更拒绝；证明消费与 plan/outbox 同事务 |
| role_keys_ready | Owner 通过角色专用 ensure-key 方法，要求 Identity/Owner/Executor 各自为同一 provision/role/version 生成并持久化自身 key/envelope，只返回公开 key ref | 先保存 envelope 与 role/version 槽，再公布公钥。重复同槽返回同一 key ref，不接受导入 seed、改 role 或替换已有 key。响应丢失读取原槽；部分成功不删除/重新生成已持久材料。三类公钥必须不同。 |
| native_registered | Identity 先保存原 machine-registration options/complete 及必要受保护上下文，按当前 capability/注册双开关提交；确认原 receipt 后把 native account ID 与 login public key 绑定 | 只恢复同一注册原件；registration_closed/unknown 不换 key/账号。关闭注册不代表已提交原 receipt 消失。 |
| native_session_ready | 使用已批准的初次 login operation，独立完成 native-key login；session 留 Identity 私有存储，仅上报非秘密 generation/status | 缺原 cookie 按本节登录恢复要求明确新登录；不能从注册成功推断已登录 |
| participant_bound | Owner 使用独立 owner key 和已确认 native account 构造既有 GameParticipantRegisterV1，先存原件；经 Identity fixed forwarding 取得 admission、提交/lookup并核验原 canonical receipt 与当前 owner | 任何未知保留原 operation/签名/账号；native/world namespace、owner/history 或 service pin 不符冻结；不以 HTTP 200 视为绑定完成 |
| ready_for_setup | 账号与当前 canonical owner 已确认，仅允许管理端继续已授权 setup | 这不是可执行 grant，也不表示 task/goal/resources/executor 已就绪 |
| prepared | 受信 setup 通过另外明确的 owner intents 完成 task/goal、必要资源与 executor delegation，并由云端核对当前完整 fence | 这些世界动作不隐含包含在“创建账号”批准中。unknown 保留，不能换一个 world/task 补成成功。 |

每一步派发前都重验原 plan 截止/调用预算、管理来源安全版本、Account DO 状态与 Recovery Fence。30 秒是批准证明的消费窗口；已经消费的 provision 计划仍只能在其明确的动作范围/截止内推进，不构成无限授权。provision 使用普通 owner 工作容量，不占用 Stop/Revoke 的独立安全控制槽。

开户计划的 `transport_limit` 首版显式限制为 1–400，完整签入批准；它与 TestSession 的 execute/read/cleanup 预算分开，不能互相借用。旧已批准计划不自动扩大额度。派发前对固定调用图保守预留：一次来源检查原件 claim 扣 8 个传输额度，一次角色密钥派发 claim 扣 32 个，覆盖该步骤的目录、Fence、native 核验或角色/oracle/DO 路由等有限调用；未发出或 unknown 的已 claim 尝试不自动退回。该上界不能作为无限内部重试许可，每个角色 permit 最多允许三次当前 oracle 核验（factory、角色存储前、加密后），恢复使用同一 task/key/version/vault 身份，新传输 attempt 另行预留。三角色首轮至少需要 104 个额度，额度不足明确拒绝或保留 pending，不能静默提升原计划或换新身份；后续注册、登录和 participant 阶段仍须从同一开户余额预留各自固定调用图，不挪用独立收尾槽。

首批 participant 冷路径使用单一、预先持久登记的 104 额度 attempt，最多 8 次付费当前 oracle 读取，不能直接复用会为每个 route 重解 Cookie 的普通 driver。固定调用图保守按来源检查 8、8 次角色到 Owner/Account 与目录/Fence 核对 40、角色独立 Fence 读取 16、5 个固定 native 动作各自的 session/native/World 转发 15、policy/admission/final 三次当前原件回调 15、私有 actor 读取 4、Owner signer/vault 2、固定入口 3，合计 103；余 1 是容量余量，不是重试许可。Identity 在一个有界流程内私读原 SID/CSRF，不重复创建或写入秘密；每次出站仍须重新核对当前 permit、scope、截止和真实 session，所有异步间隔后的 5 秒当前观察要求不变。实际调用图超界时明确拒绝，不能删掉核验来迁就预算。

第 8 次 oracle 的固定 final-read 许可同时登记一次 lookup 和一次 owner-view 的原件/只读 claim。真实 World submit 可能先返回 pending；此时仅允许在原截止内作一次至多 250 毫秒的有界等待，再查询这份已登记的 AuthenticatedLookup，不能循环或重发 submit。每个实际 native 读取前仍取得并强验新的固定 Root session 状态。只有对应 Submit 或 Lookup 签名域、原 request digest 与同一 correlation 均匹配的 committed 结果，才继续已登记的 owner-view；lookup 仍 pending 或传输未知就保留 pending。已在同 attempt 内预留的这一次 lookup 属于上述 5 个动作；后续新的查询尝试仍须另行预付，不能把此例外变成无限恢复或新的发送许可。

Owner signer 的最后一次 oracle 核对同时提交固定 bundle 候选：先在本地生成双签名，再消费该次读取信用，由 Account 重读当前来源/目录/Fence、严格验双签名并原子登记完整原件；Vault 只在该精确提交成功后返回同一原件。这一次属于上述 8 次读取池，不另设未计费的第四原件回调；不因签名已本地生成就允许出站。Identity 的两个秘密可以在一次有界私读过程中解密，持久 envelope 在前后各读核对，中间每个 await 后核验原 scope/时钟，全部解密后再完成真实当前 oracle/Fence 核对，未通过前不输出秘密或派发。

仅此 participant 冷路径的每次 native 动作前置 session 核验使用固定 `/custody/managed-bindings/session-status`：Account 为该步骤登记独立 nonce，Identity 以原 SID/CSRF 请求，严格验证固定 Root key、当前绑定/账号/credential、nonce、service epoch、5 秒时限以及 active 且非 frozen/pending。该 route 自身在 native 同一事务核对真实 session 和绑定，不接受调用者自报活性；它替代该次普通 `/session` 读取而不增加权限。policy/admission/final 回调携带这份签名状态与既有非秘密 SID generation 引用，由 Account 独立核验，不能把绑定 generation 与 Identity 私有 SID generation 混用。其他 forwarding 仍遵循原 `/session` 前置合同，不扩大此例外。

Account 先登记 policy nonce、owner-view nonce、精确 native 绑定/SID 非秘密引用、固定 World service pin、原 native 账号与三个角色 public reference；当前目标 security epoch 为真实绑定后的 1，原 Provision source proof 的 0 不改写。策略原件强验后，只允许 Owner vault 生成一个固定 bundle：既有 GameParticipantRegisterV1 域的注册原件，以及 VIEW_JSON_PATH 域、OwnerControlV1 profile 所用的只读请求原件。二者分别使用已登记 nonce/operation，并由 Account 固定角色、固定域、精确 payload 核准；不接受原始 bytes 通用签名。此冷路径仅支持新建 native 账号的首次 fresh binding；只读 agent 固定为既有 World account-key 规则派生的 participant agent，owner generation 必须为 1。imported、已有不同 agent/owner 或不一致 receipt 只保留 pending，不能根据响应换 reader 或重签注册。

Account 在任何 native/World 派发前持久保留完整 owner/admission/submit 原件及一次性 send claim；policy、admission、canonical result 和当前 owner 的回调均强验固定签名、nonce、业务摘要、世界/namespace/branch/reorg 与原关联，再在同一 SQL 事务重读当前来源、独立 Fence、时钟、冻结、库存、预算与原件。只有 canonical receipt 与独立当前 OwnerControlV1 都匹配同一新 agent/owner，才推进 participant_bound；HTTP 200、native admission 或缓存阶段均不能代签。响应未知或丢失不重发 submit，不释放或退回原 attempt；后续仅可用另行预付的 lookup-only attempt 查询同一原件。余额不足或原授权失效保持 pending，不能借 ManagedLogin/TestSession 的余额，也不能以新 operation 重建注册绕过未知结果。明确的新批准和 lookup 恢复合同未满足前，不宣称该 pending 已恢复。

来源检查只能针对 Account 已登记并消费的原批准，在任何远端预检前本地预留额度并持久保存独立 `inspect_origin` nonce，真实目录/Fence 预检后才原子登记原请求 bytes 和发送 claim；重复已预留原件不再次取得预检或发送权。nonce 原件最多 pending 30 秒且不越原计划/来源截止，独立于签名观察的五秒新鲜度。原 proof 的 30 秒消费窗口过期后可报告来源，但当前 origin、原来源版本、计划截止和 service fence 仍须有效，签名响应必须绑定本次 nonce/purpose/原 proof digest 且观察不超过五秒；角色 permit 截止还须限制在该 native 观察时刻加五秒内，不能按后续 oracle 响应时间续期。角色许可从已保存的精确 task、typed 请求原 bytes/digest、attempt、已认证来源检查和安全代计算，角色通过自己的窄静态 Account oracle 核对，不接受请求自报的 active 状态或摘要回显；oracle 先本地耗用有界读信用，再执行远端核验，预留信用本身不返回任何授权。加密前后重读许可/Fence，最终本地 SQL 重验完整绑定与时钟；无远端 timestamp 时，新鲜度从 RPC 发出前保守计时，慢响应不能更新观察年龄。跨角色已发布密钥保持不可替换；unknown 收据保持原件和预算占用，不因撤销删除或重造密钥。

跨角色 DO、native authority 与世界不构成一个原子事务；协调依据是稳定 provision ID、每角色不可替换的 key ref 与每个副作用的原件/回执。未在任何对外请求中使用的临时密码学计算可丢弃；已经持久发布的 role key 或可能已注册的原件不可因重试换新。取消 provisioning 先停新阶段，保留已产生身份/绑定与原件的真实状态，不假装全局回滚或默认删除 native account。已有世界写能力的清理沿原 Stop/Revoke/冻结合同。

只有 prepared 状态和当前世界/身份核验均通过，且另一次明确的 grant/测试会话批准被消费后，才发行执行能力。native 注册成功、列表可见、key 已生成或 participant_bound 均不是 Agent 已获执行权。

上述接口复用现有 [identity client](../../../crates/oasis7_game_client/src/identity_client.rs)、[HTTP 会话适配](../../../crates/oasis7_game_client/src/identity_http.rs)和[Owner gateway](../../../crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_identity/entry_owner_control.rs)的账号/会话/CSRF 前置，不是跳过它们的 cloud 后门。`inspect_origin` 的核验结果绑定 Owner 当次 nonce，最多新鲜 5 秒；最终 claim 重验本地版本及有效期，跨权威仍非原子，撤销 fan-out 与在途原件继续按 §12.5 处理。

<a id="cloud-custody-tools"></a>
##### 独立批准的 Owner setup 与原件 outbox

Trace refs: O7-REQ-f837065c-4da5-4e6f-8a53-f4e5900cf26c.

Owner setup 的世界结果分别记录传输、原件和业务效果：只有严格成功的 canonical Committed 才允许后续阶段；实际 World 原件中的 Rejected 保存为已知失败，不推进 Prepared。Expired、HistoryUnavailable、Unknown、Received 或 Pending 均保留原件与预留，不能改成成功、确认已清理或换 ID 重发。公开进度只给安全的分类和原 operation 引用，不回显私有错误正文。

Canonical 写的公开签名响应在 live Result 回调前先进入窄 fact-only archive：每阶段 Submit/Lookup 各一个 immutable 槽，只以固定部署 World key、该 Account 已登记的确切原件/correlation/域做严格 Rust 与 WebCrypto 核验，并在同 SQL 重验库存后保存原 bytes。它允许在冻结或权限到期后保留晚到事实，不需要 Owner seed/KEK，不改阶段、epoch、预算或任何发送权；即使归档了 Committed 也不能凭历史事实推进下一阶段。原生响应在 await 后超出 live 五秒边界时仍保留这份公开证据，之后的正常 Result 必须同时满足其独立当前授权。该 lane 不处理私有 PrepareGoal 正文，不为取回丢失私有准备响应重新发送；私有准备未知仍按原件和无 canonical 写的关闭合同处理。

独立 OwnerSetup 可以从 `ParticipantBound` 登记、消费新 MFA 并准备下述只读候选；旧 participant 回执不等于现在的 owner 确认。只有首个 CreateTask 的 Admission 强验当前 World/Root、原 owner 和确实不存在 task/delegation 后，才能在同 SQL 保存 owner-confirmation 原件，将阶段推进为 `ReadyForSetup` 并 claim 首个已批准 effect。后续阶段仍要求这份确认及精确前序原件；不能由普通 phase setter 或单凭批准消费提前推进。

participant 绑定不批准创建 task、发布 goal 或授权 executor。首版另设闭合 `OwnerSetupPlanV1`，只支持同一已绑定 Agent 的固定流程：CreateTask、私有 PrepareGoal、PublishGoal、Delegation、Bind，最后独立读取当前 OwnerControl、CurrentGoal 和 delegation 后核对 Prepared。四个 canonical 写和一个私有准备各有固定 operation ID；不含 starter、资源发放、任意世界 payload 或任意签名字节入口。资源只引用管理者明确批准并由当前世界证据确认的预置集合，缺资源保留未准备，不自动补发。

管理者在当前正数 Account security epoch 下进行新鲜 Passkey 批准。`CustodyIntentV1::OwnerSetup` 完整签入 setup/provision/account、原 participant receipt digest、唯一固定 world identity、Owner/Executor key refs、完整私有 plan digest、bootstrap version、deadline 和独立 request/transport/write 上限；不得消费已用 Provision proof 或借开户/TestSession 余额。计划原件包含完整 goal content、四个 canonical operation ID、private prepare ID/随机 nonce、executor ID/public key/代、delegation nonce 与预置资源引用；Owner 从批准原件及已验前序结果唯一推导请求，不接受调用者替换 task、goal、key 或 fence。私有 goal 及其可枚举摘要不进入公共 progress、审计正文或模型输出。

管理端先通过现有 Origin/CSRF/Passkey 管理会话调用固定 Owner setup basis/prepare 入口。basis 只读实际 Account 的 participant result digest、唯一 World、Owner/Executor 公钥引用、native namespace、bootstrap version 和当前正数 security/service epoch；不含 Cookie、SID、goal 或旧批准 token。prepare 接完整私有 plan 与 approval request ID，在真实 Rust current 下强验 basis 并由 Core 唯一 canonical codec 构造 ApprovalOptionsRequestV1，供 Native 新 MFA；不保存业务候选、不预留预算、不签名、不消费批准或推进阶段。摘要只在该管理者私有批准流传递，不作为模型工具或公开进度。Node/管理 UI 不另算协议摘要。

每账号至多一个未结清普通 setup。批准时同 SQL 保存精确计划、消费全 Account 唯一 proof nonce 并预留独立预算；首版至多六个逻辑阶段、四个 canonical 写、独立 584–600 transport；此新 OwnerSetup 的显式批准上限为 600，不改变已有 Provision 或 TestSession 的 400 上限。来源检查原件预留 8，四个写阶段、私有准备、最终核验各预留 96，完整初次路径合计 584。每阶段最多五次付费当前 oracle（Identity 私有解密前后各一次，Owner factory、固定角色 vault、发布前各一次），另有 Admission 与 Result 两个各一次的当前原件回调。阶段的独立来源检查不能复用早于五秒的初次观察；每阶段 source nonce 先持久登记、有效期最多三十秒且不越批准的计划截止。96 包含固定初次 lookup 一次，保守图为 source 8、五 oracle 25、角色独立 Fence 10、native Root status/转发/World 的预检与 effect/lookup 最多 11、两回调 10、私有 actor 4、signer/vault 2、固定入口 3、一次 lookup 当前准入 5、本地 unknown 收紧 1 与 Submit/Lookup 各一次事实归档共 4，合计 83；native Owner gateway 内部的额外 owner-view World 读取已计入。发布、Admission 与 lookup 返回的新当前 permit 在 Identity 使用前各独立重读 Fence（最多三次，共 6 跳）；旧私有解密阶段的 Fence 不随新 permit 静默延时。连同下述刷新 access 的 6 跳，完整固定图为 95/96。余量不是无限重试许可。固定图超界拒绝而不减少安全核验；unknown 不退已 claim 额度。固定初查后仍 unknown 或 canonical 已归档而当前结果准入丢失时，按下述独立 MFA paid reconcile 合同恢复；原阶段预留不返还，不能靠新管理 request ID 重建同一 setup。

旧 participant admission 的 tick 有效期不能由后续 setup 延长或绕过。每个 setup 阶段在 96 预留中登记 policy observation、policy Root session 和 authorize Root session 三个独立 nonce。Identity 第二次私有解密后当前读取在同 SQL 一次 claim policy/authorize 固定对；以当前托管 SID/CSRF 取得固定 World 的 policy 原件，再用同一已存 Owner register 原件重新取得 native admission，不再次注册 Agent。第三次付费 Sign oracle 携闭合 access evidence（policy 原件、两份独立 Root session status 和新 admission），在严格 Rust/WebCrypto 与同 SQL 当前核验后固定保存该阶段 access；第四次 vault 与第五次 publish 只使用并重验这份原件，不接受更换 owner 或静默换票。policy 必须绑定已登记 nonce、唯一 World pin 和原 issuer；admission 必须绑定原 register business digest/account/key/namespace、当前 policy generation 与有效 tick 窗口；两份 Root 状态分别验证实际会话引用、私有 SID 代、binding invalidation 代与五秒边界。首版要求原 register、新 policy 和 admission 的 namespace epoch 精确一致；底层 Native 支持的旧 epoch 恢复不自动扩展为本 setup 合同。Native authorize 内部会再次读取 policy，两份 policy 的 generation、namespace、issuer、branch/reorg 或窗口发生变化时保持 pending，不续票。fresh admission 只证明准入，不能代替每阶段独立当前 canonical OwnerControl。固定对 claim 后任一响应丢失保持 unknown，不换 nonce 重领。旧 participant receipt 和 admission 均不改写。新增两个 native 读取的 Root/native/World 跳共 6，计入本阶段上限，保守图合计 95/96；失效或 unknown 保留 pending，不循环续发 admission。

每阶段先从已批准计划和已验前序 canonical 原件唯一派生固定候选与该阶段独立 nonce 的 OwnerSetup 读取请求；此时它们是历史预期，不自称当前 World。五次 oracle 仅依据新鲜来源、真实 Directory/Fence 和批准范围核验固定候选，同 SQL 永久保存完整签名字节、correlation、lookup 和只读请求，不能从 RPC 暴露给模型，也不授予 effect 发送权。这一只读预签边界避免“需要当前 World 才能签获得当前 World 的 read”循环。Identity 只从真实 Owner/Account 固定 RPC 取得 Rust 当前来源，在解密前后分别重读；不能把调用者附带的 permit 当 Current。随后 Identity 私有转发预检读取；Admission 回调强验该请求对应的 World 原件、Root session status 与独立 nonce，并核对当前实际状态等于固定候选的已登记前置，才在同 SQL claim effect 发送。前置改变保持 pending，不重新签不同 fence 或目标。

外部 await 后重新核对 scope、原件库存和不超过五秒的当前观察；Identity 只用原私有 managed SID/CSRF向固定 native Owner 控制路由转发，每次 native 发送先取得该阶段独立 nonce 的 Root session status。预检/effect/lookup 的 nonce 分别登记；FinalObservation 的只读发送也由该阶段新鲜来源授权，其结果不授予任何新写。Owner vault 仅签上述固定阶段域及已登记读取请求，不提供通用 raw sign。新增 setup lane 的 native/World 响应先做完整 Value→typed→Value 相等校验，拒绝被泛型 DTO 静默丢弃的未知 wrapper 或字段，不改变旧客户端的泛型 wire。World request/response 和 Owner、Root 签名均经严格 Rust 与 WebCrypto 验证；Result 回调最终同 SQL 重读时钟、authority、原件和预算后提交结果。HTTP 200、Received 或服务响应本身不代表 canonical committed。

CreateTask 的 task ID沿用 `game-task-` 加 `request_digest("game-task-id", correlation.key)`。PrepareGoal 只签由该 task、已批准 content 和固定 nonce 组成的原提案；原 PreparedGoalReceipt 通过现有 `validate_against` 后，才能用既有 publication helper 构造公开 PublishGoal。真实 TaskCreate 的控制状态为 Suspended；GoalPublish 只更新 task/goal revision 与 reference，保留 Suspended，不提前推断 Active。Delegation 前置和 Bind 的完整 fence 必须保留这份实际状态；只有 Bind 的 canonical ExecutorReceipt 才能证明 Active，最终 Prepared 仍严格要求 Active。Delegation 是与 Bind 分开的第四类世界写：持久分配的 delegation generation/nonce 必须覆盖本服务已分配水位；未知旧授权不能被忽略。Bind 仅使用已验当前完整 task fence、同 Executor key 和已 committed delegation generation。每阶段 unknown 只恢复原 request，不换 nonce、不重新 prepare goal；私有准备丢回包也不能伪造未执行，无法合法恢复时保留 pending。

只有最终当前证据同时证明原 task/agent/owner、精确 goal reference/content、Active task、完整 branch/reorg/fence、Executor key/ID/epoch、未撤销的 delegation generation 及已批准资源都一致，Account 才可由 setup 专用强验路径将 `ReadyForSetup` 推为 `Prepared`。通用 phase setter、旧 participant receipt 或结构正确的快照不能代签。冻结、到期、旧 pin、身份撤销或未知库存只收紧/保持 pending，不授予 TestSession，也不能为结束测试自行发 Stop/Revoke。setup 默认关闭，部署不配新运维凭据；真实启用和实玩仍是独立验收。

setup 当前观察使用显式 `OwnerSetupV1` response profile，不改变已有 `OwnerControlV1` 的 wire。World 在同一 pinned commit 返回原 owner/task control 和该 Agent 的完整当前 signer-delegation record（含 revoked/nonce/generation），或明确的 None；整个 `OwnerSetupObservationV1` 由固定 World service 对原请求、profile 和 observation nonce 签名。只有这个完整已验 profile 的 None 能证明当时无 delegation，不能从旧 control 未返回字段推断。首版仅支持 fresh setup、无既有或未知 delegation 分配库存、当前 World record 为 None 的起点，固定初代 generation/nonce 为 1，并在出签前持久分配；资源集合首版必须为空，后续带资源的 setup 需要另补独立原件合同，不以传入引用当已验。

##### 独立 MFA 的 setup 原件核对与迟到结果准入

阶段来源的五秒时限是真实授权边界；即使 World 已处理请求，过期也只能归档原件事实，不能用旧 claim 时间完成当前准入。原 Submit/Lookup 每类只保存一条 immutable 原件，因此 Received/Pending 不能覆盖为后来 Committed。后续恢复使用独立 `OwnerSetupReconcile` 正数 fresh-MFA 批准及独立账本，不延长原 setup，不重发写请求，不借 Provision、ManagedLogin 或 TestSession 余额。

Account 存储适配器可在单个 store 实例内复用一份已完整严格恢复的历史快照，但每次读取和 SQL 回调仍须读取实际当前原始字节；只有整串字节完全相同时才能克隆已验快照，任何字节变化都重新执行完整恢复校验。该复用只省去相同持久化输入的重复解析与历史校验，不复用当前授权、时钟、Fence、外部验签或 transition 结果；SQL 库存校验、当前检查、状态变更校验和最终原子提交照常执行。快照不跨 store 实例共享，不以 audit sequence 或摘要代替原始字节相等。


批准闭合绑定 reconcile ID、setup/provision、唯一阶段、原 setup plan digest、原请求与 correlation digest、固定 World、Owner/Executor key refs、原 deadline、此次 deadline（不晚于原 deadline）、attempt limit 1..3 与独立 transport limit。来源先预留 8，每 attempt 在任何外部 I/O 前另预留 104；上限仍为 400，固定最多 3 次（预算分别为 112、216、320），首次至少 112。每 attempt 独立 nonce、五票角色矩阵与一次 lookup claim；失败或 unknown 不返还。原 setup 已预留阶段不重新扣费或 refund，未预留后续写阶段仍只能使用原 setup 的剩余预算及原期限，新 reconcile 余额不资助后续写。

每 attempt 重建当前来源检查、Directory/Fence、私有会话及 fresh policy/admission，Owner 只能签已登记的固定只读 observation。实际 lookup 必须发送原阶段已存 authenticated lookup bytes，不能重新编码替换 correlation。新的签名 lookup response 存入该 attempt 自己的 immutable 槽，不覆盖原 Submit/Lookup。冷调用图从阶段 95/96 图去掉 effect submit，保留来源、五 oracle、私有 actor、fresh access、预检 Owner 读取、一次 lookup、事实归档及当前结果回调，基础保守 91；新增 ClaimResult 的独立当前核验与 Account 往返共 5 跳，不能用旧 permit 替代；归档后重新读取 post-state 的独立 Root status/native/World 共 3 跳，Result permit 返回后独立 Fence 共 2 跳，完整保守图为 101/104，三跳余量不构成重试许可。Result 回调容量在 post-state 网络读取前本地一次 claim，返回已绑定独立 post-state nonce 的新只读 permit；lookup 前状态与 post-state 状态使用不同 nonce，Completion 同时携带并分别强验两份 Root 状态，不以同 nonce 重新观察续期。

恢复当前结果不能复用正常 Admission 的 predecessor 期望：World 已 committed 后应严格核对由原 canonical receipt 推导的 post-state，包括 Create 的 Suspended rev1、Publish 保留 Suspended 的新 goal/revision、Bind 后 Active 及精确 executor/delegation。强验新 Root session、原 signed canonical correlation/domain/固定 World pin、新 OwnerSetup observation 及同 SQL current/clock/inventory 后，只填原阶段结果及 `Reconcile{reconcile_id,attempt_id}` provenance，不改原 source、original、claim、deadline 或预算。strict restore 按该 provenance 重验，不伪装原五秒 grant 仍 fresh。已归档 Committed 也必须经过这次新当前核验；Pending/Received/Unknown 不推进，Rejected 已是终态，不重开后续阶段。

原计划到期、冻结、撤销或库存未知只允许事实归档并保持 pending，不恢复 active/Prepared。私有 PrepareGoal 未知不在 canonical lookup 白名单内，不重复 prepare 或改 body；只有已持久保存的真实 receipt 可在独立当前准入下恢复。FinalObservation 没有写副作用，可在新批准中重新签固定 OwnerSetup/CurrentGoal 只读 pair，但仍必须强验原 Bind、同 goal 与 delegation，不能借此签写请求。UI 明示此次额外读取预算、原计划不可延长与未结清原件；普通 continue 不自动创建或消费新批准。

### 12.17 首版原生工具与多阶段原件

首版固定 `custody-native-v1` 工具 profile；下游私有任务请求显式采用 `TaskContextAuthorityModeV1::NativeExecutorV1`（wire `native_executor_v1`，相关 schema revision 按既有 Rust helper），不省略该模式而落入旧 Provider authority。通过当前受认证的 `EntryExecutorOperationV1` 接续原生任务读写，不等待或假定 `/v1/game` 已具备写接口。该 profile 是新增托管适配的目标合同，现有 Unix broker 不因本段声明而自动支持它。

一个调用只选择一个已批准 TestSession/resource handle；其 account/environment/world/Agent/task、actor、公钥与完整 fence 由服务保存的绑定确定。模型不得传入 signed payload、任意 `Action` JSON、actor、签名域或网关地址。下表是首版封闭工具集合；每根 grant 可以进一步缩小动作、参数和资源，不能扩大此集合。管理者的 Owner 操作仍使用独立入口，不进入游戏工具。

| 工具与输入 | 既有协议/实现映射 | 结果与限制 |
| --- | --- | --- |
| `task`、`capabilities`、`industry`，无目标覆盖参数 | `EntryExecutorOperationV1::ReadView`；复用 `device_client::task_fence`、`capabilities`、`industry` 的验签和有界投影语义 | 返回当前读版本/tick、任务完整 fence、native 业务 grant 投影与实际工业账本、线路、工厂和 pending jobs；保留集合总量/截断标记。对外可调用 proposal forms 取本 profile、当前 custody grant 与现有原生工具支持的交集；module catalog 与业务 grant 分别标明，不把目录或当前 grant 当作动作必然可受理的证明。 |
| `context`，首次使用稳定 `request_id` | `PrepareContext(TaskContextPrepareV1)` → 保存并 `Submit(SchedulerOperationV1::TaskProviderPrefix)` → canonical committed → `ReadContext(TaskContextGetV1)` | 服务按现有 prepare/prefix/read 流程保留全部原件。只有已验证 prefix committed、当前权限/fence 与 lease 仍有效，才返回 `usable_context=true`、`context_ref`、完整目标/观察/native authority、capture/read commit。完整目标留在获准模型上下文；nonce、私有 proof、salt、签名秘密不进入模型/日志。输出超限或客户端不能完整接收时明确阻塞，不截断目标伪装完整 context。 |
| `move`，`context_ref`、三轴 i64 厘米坐标 | `Action::MoveAgent` → 既有 `task_context::cognition_intent` → `Submit` / `Lookup` | actor 由绑定注入；坐标必须在 grant 允许范围及真实世界约束内。canonical cognition receipt 与新授权观察中的位置共同证明效果。 |
| `transfer`，`context_ref`、from/to ledger、material、amount、distance_km、priority、route_ids 或显式 direct | `Action::TransferMaterial` → 同一 Cognition/Submit/Lookup 流程；沿用 `commands::transfer_action` 的类型和校验 | 双方账本不同且逐一位于 grant allowlist；材料、正 i64 数量、非负 i64 距离、standard/urgent、累计资源上限均受批准约束。有序 route 至多 16 项且不重复；direct 需显式允许，`auto_reroute=false`。世界继续核所有权、库存、路线和成本；transit 启动回执不等于到账，需关联原转运和目标账本变化。 |
| `quote-build`、`build`，`context_ref`、site、factory、内置 factory kind | `FactoryQuote(TaskFactoryQuoteRequestV1)`；`build` 另由已验签报价构造原 Cognition/Submit，复用 `device_client_build` | site/factory 精确落在批准资源内，factory kind 仅当前支持的内置类型；参数不接受调用者自造 spec/成本/报价。quote 只读、不预留。build 已有原 Action/Outcome 时先恢复原件，不能重新报价替换它。FactoryBuildStarted 与最终 FactoryBuilt 分开观察。 |
| `quote-schedule`、`schedule`，`context_ref`、factory、内置 recipe、正 u32 batches | `RecipeQuote(TaskRecipeQuoteRequestV1)`；`schedule` 另由已验签报价构造原 Cognition/Submit，复用 `device_client::schedule` | factory/recipe/batches 受 grant 范围和累计业务预算约束，不接受自造 recipe plan；已有原 Action/Outcome 时先恢复原件。RecipeStarted 不等于首产物，需对应 RecipeCompleted 和输入/输出账本证据。 |
| `lookup`，`operation_ref` | `GET /v1/operations/{id}` 定位 DO 原件；已保存的 canonical Prefix/Action/Outcome 在当前执行授权下用 `EntryExecutorOperationV1::Lookup`，到期/停止后由 Owner 按独立读取授权用 `LookupExecutor` | 返回各阶段已知状态、非秘密结果与关联；不让客户端上传替代原件。Prepare/ReadContext 不能经 Owner LookupExecutor 回读：只在当前 executor/fence/lease 允许时用原生恢复，关闭后按下述无世界效果判据处理。缺历史或不能证明时保持未知，不恢复写权限查询。 |
| `outcome`，`context_ref`、固定 reason | `TaskProviderFailure` 的 Scheduler 原件 → Submit/Lookup；复用 `device_client_outcome` | reason 仅 `executor_aborted/provider_failed/transport_exhausted/context_expired`；是当前 grant 必须明确允许的原生写，不是只读查询，也不是到期后例外签名。存在原 Action/Outcome 时不得生成竞争写；过期 context 的特别条件见下文。 |

首版不开放 `world-rule`/`world-rule-key`、`schedule-essential`、任意 module action、治理、自由配方、账号/owner 控制或任意脚本。`Describe` 等协议发现可以由可信 setup/适配器内部调用固定目标；不是授予模型通用路由访问。transfer/build/schedule 的加入不构成完整 starter chain、持续生产或全首局声明；实际测试 scope 可以缩小，未运行的工具保留未验证状态。

capabilities 的空业务 grant 投影继续使用现有 `needs_native_context_confirmation` 语义；完整已验签 native context 才确认 `default_action_validation` 或 `grant_scoped`。不把空列表直接当作零费用、自动授权或所有动作均禁止，实际业务 admission 仍由世界决定。

#### 托管 DTO、关联与失败语义

首版工具 envelope 固定 `schema_revision`、`profile_id`、`test_session_id`、`request_id`、`tool` 和该 tool 的唯一 typed arguments；capability 与 PoP 只由可信客户端附于认证层。各 enum variant 均拒绝未知字段，不允许通用 `args: Value` 穿透下游。u64 沿本设计十进制字符串合同；新 HTTP 中 i64 坐标/数量/距离也使用规范有符号十进制字符串并由 Rust checked parse，避免经过 JS Number 损失精度；u32 batches 是正整数并检查上限。账本/资源 ID 沿现有 parser 校验，不能将任意字符串解释为路径或 URL。

客户端在首次发送前持久保存随机、稳定的 `request_id`。托管公共 `operation_id` 使用该相同 ID，在当前 TestSession 和账号范围内寻址；认证层决定范围，不能仅凭 ID 获取他人结果。DO 将 `(account/environment, test_session_id, request_id)` 与首份 typed body digest、origin grant/security version 原子绑定；同 ID 异正文返回冲突，同正文重复返回原 operation。digest 不包含会变化的 PoP challenge/transport attempt，更新传输证明不创建第二个业务操作。client binding/credential generation 是权限检查项而不是幂等 namespace；合法凭据替换仍定位原操作。重新批准的 grant 不能重置旧 session/request 的身份、预算或派发条件。

`context_ref` 是 context 请求的 operation 引用；后续 action/outcome 各有自己的 request ID，并在同一个 DO 事务中关联该 context 及其原件类别。查询 context operation 返回已保存 prepare/prefix 状态和已关联 action/outcome operation refs；查询 action/outcome 返回其确切原件结果。首次 202 丢失后，客户端仍可凭发送前保存的 request ID 使用独立读权限查询，不需要重复创建 POST 或先取得一个只存在于丢失响应中的服务器 ID。HTTP/tool 错误必须保留 operation/context 引用，不让助手通过换 ID 消除未知状态。

新接口固定可机器处理的错误类别：`invalid_request/unsupported_profile`（改正输入，未受理）、`permission_denied`（停止，不重试授权）、`idempotency_conflict`（保留原件）、`budget_exhausted`（停止新调用）、`stale_fence/context_expired`（先核对原件；不得改绑旧 payload）、`upstream_unavailable`（按已记录状态作有界原件核对）、`recovery_required/history_unavailable`（停止新效果并保留现场）。202、received、unknown 和 canonical pending 都不是成功；不存在的本地原件不是“下游没有执行”的证明。世界明确的业务 rejection/next_step 作为原结果保留，不扁平化为可随意重试的 HTTP 500。输出不回显原始上游错误正文、cookie、证明或私有存储路径。

普通 task/capabilities/industry/quote/context/action/outcome 工具受其当前执行/读取范围和工具请求预算控制；查询不是免费绕过总调用上限。写 capability 到期、撤销或测试 deadline 后，只允许既有独立读凭据在其更窄范围/预算/期限内查询原结果；不能创建 context、生成 outcome、重新报价或以 GET 触发原件重新提交。清理中的 Stop/Revoke 归预先获准的 owner 控制通道。

`describe-session`、`finish-session`、`abort-session` 是同一工具宿主的有限会话工具，分别映射非秘密 session 状态与 §12.18 的关闭入口，不属于 executor 世界动作。首版业务工具集合不随 MCP tools/list 或既有 native CLI 新增命令自动扩大。

响应上限按实际层取小值：native gateway 已有 4 MiB 原响应上限；新 custody HTTP 含封装至多 4 MiB+64 KiB，客户端及模型消费预算可更小。目标/context 必须完整传递并标明完整性，超限返回 `context_too_large` 且不准入依赖动作，不能静默截断。工业集合允许保留 count/truncated 的有界投影。只读恢复结果只含授权的原回执/状态/关联，不重新暴露过期私有 context 或把原签名写 bytes 返回模型。

#### 原生 turn 的原件迁移

托管实现复用 `oasis7_client_api::world_service::client_state::{ClientStateKind, ClientStateV1}` 的 prepare/prefix/action/outcome 原件类别和纯验证，以及 `task_context` 的 public_prefix/cognition/outcome helpers。把现有 game client 的持久化和网络 I/O 移到 DO/outbox、Workers 平台端口；不把原生 Unix vault/store 整包拉进 Worker，也不改变原生 turn 状态机或签名域。

| 原件类别 | DO 必须保存/关联的现有内容 | 可前进的条件与恢复 |
| --- | --- | --- |
| Prepare | 首次 `TaskContextPrepareV1`、固定 native mode、完整 fence、client operation ID、原 nonce/签名、固定 world/service pin；成功后保存经验证的 preparation 关联 | 先保存再请求 PrepareContext；超时仅按现行幂等准备合同使用相同原证明恢复，不能换 nonce。Prepare 的返回不证明 canonical prefix 或可行动 turn 已存在。 |
| Prefix | 从已验证 preparation 用现有 public_prefix helper 生成的完整 Scheduler SubmitIntent、原 correlation/digest/签名及 receipt | 当前 helper 生成 `task_prefix_binding_revision=1`，其捕获/租期校验继续由原协议决定；保存后 claim/提交，unknown/pending 只核对原 prefix，canonical committed 后才 ReadContext。 |
| Context read | prepare/prefix 与完整 public context identity 的关联、既有 read proof、已验证 read/capture commit 和有效期 | 获准模型获得完整授权目标/观察；ReadContext 失败或 lease 过期不令 prefix 失效，也不能隐式创建新 context 清掉活动 turn。只继承现有有界读取/quote drift 重试，不通过重签 Action 延期。 |
| Action | 由当前合法 context（必要时当前已验签 quote）生成的确切 Cognition SubmitIntent；关联公开 action request ID、context_ref、body digest、预算预留与原 correlation | 每个 context 的 Action/Outcome 写入选择在 DO 条件事务中互斥；保存任一 Action 后，同 context 的不同 action/body 返回冲突。重试先读原 Action，不重新报价、改 base、改 nonce 或替换动作；action committed 后仍分别核对业务 accepted/rejected/effect。 |
| Outcome | 依照原 native helper 构造的确切 Scheduler TaskProviderFailure 原件和固定 reason；与 context/独立请求 ID 关联 | 已有 Action 不创建 Outcome；已有 Outcome 只能恢复同原件。context_expired 首次创建须原 prefix 已 committed、无竞争写原件、当前 Active 任务完整 fence 一致且当前已验签 tick 严格大于原 valid_until_tick；使用原公共 prefix 与当前签名 base，不读取过期私有正文。只有该原 outcome committed 后才允许下一个 context。 |

这张表细化同一个 Operation/Outbox 合同：一个 context 创建请求可以顺序保存不同阶段原件，各阶段有自己的状态、原件摘要和关联，不把最后一次 HTTP 响应当作整个 turn 的唯一记录。外部 await 之间的 action/outcome 选择、授权和预算均由 Account DO 条件事务保护；不因账号一次只有一个普通在途写就假定不会发生竞争。上下文已到期或 grant 已到期时，不能为保持自动化补发未获授权的 cleanup outcome；保留待核对，使用仍合法的原件读取与独立 owner Stop/Revoke。

**私有准备与纯读取的关闭判据：** [原生 Prepare](../../../crates/oasis7_chain_runtime/src/host/world_service_private_task_context.rs) 的重复调用仍要求当前 executor/fence 和原 lease；Owner `LookupExecutor` 只查 canonical 原件，不打开私有 context。若 Prepare 已保存但响应丢失，在关闭该 session 的原件分配/签名/派发并隔离旧 writer 后，完整 DO/outbox 必须能证明从未保留、签发或派发相应 Prefix，才可将该阶段记为 `private_preparation_unavailable_no_canonical_submit` 并结束收尾依赖。它只证明没有本服务产生的世界提交，不宣称 Prepare 没发生，不删除私有准备/审计引用；仍付清传输成本，释放仅与未发生 canonical 动作相关的预留。已有或可能存在 Prefix、历史/分配屏障不完整时继续核对/阻断。ReadContext/quote/普通纯读同样可记录 `read_result_unavailable`，不为取回读取结果恢复写权限；若已有 canonical 前置，前置仍必须独立核清。这些分类属于 Operation 的完成原因，不改变世界状态机。

本服务工具请求预算、每次下游传输/恢复请求预算和世界业务预留分别记账；同一 action 的恢复不重复扣业务效果预算，但每次实际调用仍消耗其有界请求额度。prepare/prefix 的阶段推进不得重新生成根 grant 或扩大批准范围；恢复 deadline 到达后只保留原件并报告 recovery_required，不重建 context/账号继续。

源码复用入口：[EntryExecutor 枚举](../../../crates/oasis7_client_api/src/world_service/entry_executor.rs)、[native authority mode](../../../crates/oasis7_agent_api/src/game/executor.rs)、[ClientState 原件](../../../crates/oasis7_client_api/src/world_service/client_state.rs)、[context/任务 helpers](../../../crates/oasis7_client_api/src/world_service/task_context.rs)、[原生工具](../../../crates/oasis7_game_client/src/commands.rs)、[建厂恢复](../../../crates/oasis7_game_client/src/device_client_build.rs)和[过期 outcome](../../../crates/oasis7_game_client/src/device_client_outcome.rs)。引用只说明既有语义；DO 适配及全部 CT-007 正/负/故障验证仍待交付。

<a id="cloud-custody-selftest"></a>
### 12.18 TestSession 生命周期与真实 Agent 自测

新增持久 `TestSession`，并在同一次 `TestSessionApprovalV1` 中生成三个目的明确的授权记录：ExecutionGrant、ReconcileReadGrant、TerminationPlan。后两者是初次批准的 sibling authority，不是过期 execution grant 下新建更长 child。它们共享 session/account/manager/credential/security version/revocation lineage 和已有预算真值；任意 session/account/identity 撤销均能失效关联权限。保持普通 execution root 的绝对期限规则，不把 read/cleanup 的目的期限算成游戏执行延期。

Approval 绑定：manager/credential/version、账户与 native principal、环境和完整 world identity、prepared Agent/task/goal/executor fences、客户端/connector keys 与 approved runtime/Skill/source refs、typed action/参数/资源 allowlist、累计 request/business 总预算、固定 write_deadline、固定 read_until、固定 cleanup_until，以及精确 termination plan digest。初版只接受已核实 prepared_world 的固定资源；不借此模糊批准未来任意账号创建/目标/资产。

资源 allowlist 精确反映实际 Prepared 证据，不能为满足非空校验填造资源引用。闭合的 `Move` 原件只含已批准坐标且不引用资源；仅当所有动作均为精确 `Move` 且 business_limit 为零时，可批准空资源 allowlist，仍强验同一 Prepared Agent/task/Owner/Executor fence 和每次当前准入。Transfer、Build、Schedule 必须逐一核验其真实固定 World 资源事实并匹配非空 allowlist；这些资源证据尚未接通时保持 pending，不借 Move 通道扩大动作或伪造 basis。无资源动作子集的实现不代表其余动作及整体验收已完成。

| 记录 | 初次批准的上限及权限 |
| --- | --- |
| ExecutionGrant | 默认批准5min、可明确批至 `write_deadline <= approved_at + 30min`；capability `exp = write_deadline`，至多 100 次普通工具调用/400 次普通下游传输，业务资源按初批预留/结算。 |
| ReconcileReadGrant | `read_until <= write_deadline + 30min`，最多100次原件读取/300次transport；仅本 session 原 operation 查询与固定 close 状态，非全账号枚举/任意历史数据。执行过期后不新增context、quote或industry查询，不得write、delegate、rotate世界key。 |
| TerminationPlan | `cleanup_until <= write_deadline + 5min`；只允许本 session 已批准目标的 quiesce、Stop、Revoke、与其原操作查询；最多2份世界写原件（Stop1/Revoke1）及24次transport。仍使用独立槽位和 delegation high-water 屏障。 |

TestSession 的 native 自然存活期证据使用独立固定 `POST /custody/managed-bindings/session-lifetime` 与 `SignedManagedNativeSessionLifetimeV1` 原件，不改变旧 status 路由及其 5 秒 observation `expires_at` 语义。新原件在独立 canonical domain 内嵌完整现 status，另签入明确的毫秒 `session_expires_at`、`session_idle_expires_at`；Native 同 SQL 从实际 AccountSession 读取，按当前 SID/CSRF、receipt、credential/account/security、namespace、Owner pin/退役墓碑及 pending 重核。它不返回 SID、不发 Set-Cookie、不刷新 idle 或续 session。只有当前 active 且两自然期限覆盖所批准 purpose 截止时才可 ready；缺字段或期限不足保持 pending，不从 observation 期限或配置常量推断自然期限。新路由仍限固定 Native origin 和 managed-session 私有 transport，不扩 approval verify-secret 权限。

TerminationPlan 必须保存闭合的 typed 原件，不能接收调用者仅回显的 `termination_plan_digest`。管理 basis 从 Account 已验 Prepared 结果派生版本 1 原件：session/account/provision、Prepared result digest、固定 World/branch/reorg、agent/task、Owner key ref 与 owner binding/identity generation、Executor key ref/ID/public key、完整初始 task fence、原 delegation public key/generation、稳定 Stop/Revoke operation ID、cleanup 截止与 2 写/24 transport 上限。operation ID 由各自独立 domain、session ID 和 Prepared digest 确定，不能由模型选择或换 ID 重建。唯一 trigger policy 是本 session 的持久 closing/finish/abort/deadline；唯一 nonce policy 是 `AccountAllocatedHighWaterV1`。Core 用同一 canonical codec 求 digest，fresh-MFA TestSession 原件必须签入该 digest，最终同 SQL 对比实际 typed 原件。

公开管理 basis 所携 TerminationPlan 的 `original_delegation` 使用 custody 专用闭合元数据适配：`agent_identity_generation`、`generation`、`nonce` 在 JSON 和该新 custody canonical 域中均为规范十进制字符串。Rust 字段仍保留完整 SDK typed 原件，解码严格恢复三个 u64；number、前导零、符号和溢出拒绝。此表示只属于 custody 计划，不改变 SDK 原件自己的 JSON、World domain/canonical bytes 或下游签名编码；发送或签署 World 原件时仍直接调用原 SDK codec，不经 JavaScript Number 重建。公开 basis digest 的计算必须在大于 2^53 的字段和 u64 上限下保持精确，不能用显示层已舍入的数字产生另一摘要。

生成 Stop 时只允许既有 `GameExecutorControlV1::Stop`，其 fence 必须来自已验当前 Owner observation，且精确保持本批准的 World、Agent/task、Owner/identity generation、goal 和 Executor 绑定；不能借模板停止后来更换 Owner、目标或 executor 的任务。Revoke 只允许既有 `AgentSignerDelegationChangeV1` 且 `revoked=true`、仍是本批准的 delegate key/owner lineage；generation/nonce 在出签前同 SQL 按实际 World 已验水位与本 Account 所有已分配水位取最大值后 checked 加一，并永久保留分配事实。未知或不完整库存、溢出或观察到不同 scope/delegate 时只记录 cleanup_blocked，不从零推断、不降水位或换 key。已经生成的 Stop/Revoke 原件只能按原 operation/correlation 查询，不重复分配 nonce。

Stop 与 Revoke 使用各自独立的一次 claim、原件和恢复状态；一个 unknown 不占死另一个的安全槽。Stop 已确认后，Revoke 仍按原授权目标和已记录 Stop receipt 所能证明的后状态核验，不能把自己的 Stop 引起的变化误判为换目标，也不能把任意外部 fence 变化吞掉。24 transport 是原批准的硬上限，不因当前接线增加 hop 静默提额；实际固定调用图、每次实际出站前预扣和超限拒绝必须由集成测试证明。缺证、预算或合法当前来源时保持 cleanup_blocked/unknown，不把预先保存模板或自然 token 到期当作 Stop/Revoke 已 canonical 或 session 已 closed。

**原签查询、事实归档与当前准入分离：** Stop 已离开本服务、实际提交而回包丢失时，World 可能已处于 Suspended，但 Account 中 Stop 仍为 unknown。读取这一已登记原件不能要求 World 仍满足发送前的初始 Active fence，否则会同时阻断 Stop lookup 和后续独立 Revoke。只读 lookup 使用原已保存的完整 signed lookup request、operation/correlation、固定 World pin 与 domain；它不重新签名、不替换原件、不改变任何 fence，也不授予 Stop/Revoke 新写入或延长 purpose 期限。派发 lookup 仍须满足该预批准读取本身的 scope、当前来源、期限和独立预算条件，不把取消初始 Active 检查扩大为绕过身份/管理来源失效或灾备冻结。首次 Revoke 槽仍独立于 Stop 的 unknown 状态；仅有 unknown 不能占死该槽，无法证明 Revoke 自身合法当前范围时则如实 cleanup_blocked。

对已收到的 canonical Submit/Lookup 响应提供独立的事实归档：按原件类型和固定 World key 强验完整签名、request digest、domain、correlation、branch/reorg 与原 typed payload 所对应的 receipt，分别保存至不可覆盖的两个槽。同一原件的精确重试只读返回原事实；不同原件不能覆盖旧槽。归档本身不要求初始 Active fence 或仍新鲜的业务 Current，因为它只记录已发生的权威事实；它不分配/返还预算、不确认当前活性、不修改 delegation 水位、不清除冻结/unknown，也不推进新的写权限。迟到事实只能在新的合法 Current、原 purpose 未过期、管理/Native 来源及独立 Fence 均通过后，按同一已登记原件进入结果准入。Stop 的强 committed receipt 可据此建立它实际造成的 Suspended 后状态，Revoke 再按该后状态和自身高水位合同核验；不能借归档吞掉任意外部 task/owner/executor 变化。Rejected 是已知阻断结果而非成功；Pending/Received/Unknown/Expired/HistoryUnavailable 不证明未执行，也不能伪造终态、重发 submit 或换 operation ID。

**Cleanup 24 的完整调用图准入：** 当前独立 cleanup dispatcher、读原件签名与迟到事实准入尚未完成，24 transport 内的完整冷路径没有闭合证明，实际派发保持 disabled/cleanup_blocked。不能直接复用 Owner setup 的 96 transport 阶段、用 8 个观察信用代替实际出站计数、提高 24 上限，或从 Execution、ReconcileRead、控制/native 预算借额度。所有所需的 Source、Directory/Fence、SID/CSRF 私读前后观察、Native lifetime/当前 session、fresh policy/admission、固定 Owner 角色签名、World submit/lookup/回读、状态回传均须列入本 purpose 的实际逐 port 图；每次真实出站先持久预扣，失败/unknown 不退款，内部同 admission 回调的幂等不能把新网络请求变成免费重试。

**本地到期与 alarm 一致性：** Account Authority 必须从实际持久状态派生最近的本地截止：未消费批准的 30 秒 proof 窗口、write_deadline、已激活写 lease 及 cleanup_until 分别按其阶段适用。alarm 只触发同 SQL 的本地 expire/quiesce/closing 与重新排程，不调用 Native、不消费 24 transport、不证明世界 Stop/Revoke 已执行。当前仅实现本地到期维护、尚未接通 cleanup dispatcher，因此排程 getter 排除全部已 closing 会话，不为它们伪挂 cleanup wake；后续实际接通 dispatcher 时才按独立合同登记仍在未来的一次 cleanup wake。cleanup 已过期而原件仍未知时保留 pending/cleanup_blocked，不重排过去的时间形成 busy loop，也不标记 Closed。早醒必须重读实际时钟和持久状态，尚未到期则重排，不提前关闭或重置任何绝对期限。

DO SQL 事务与 storage.setAlarm 不是同一原子事务。交付任何首次开放或续租成功前，先持久一个不晚于当前实际最早应唤醒时刻的 alarm，再提交/核对开放或 lease 状态；SQL 随后失败留下额外早醒是安全的，早醒只重读重排。alarm 持久化失败时不得向调用者交付开放/续租成功。构造恢复（例如 blockConcurrencyWhile）以及 fetch/alarm 入口必须重读状态、补处理已经到期的本地事件并恢复该保守排程；重启、延迟 wake 或 setAlarm/SQL 任一阶段失败都不能产生新的执行授权。该机制仅补齐本地到期收紧，不将独立 cleanup 派发缺口或 24 次完整调用图宣称为已实现。

fetch/构造恢复中的维护若因严格库存读取或恢复失败而不能完成，应明确保留 maintenance_pending/failed，不把未知库存当空、到期处理当成功；可先保守登记 60 秒后的有限重试，再继续原 handler 自有的时钟、strict restore/库存、冻结及权限检查，不将维护失败另行变成所有既有只读/收紧/业务路由的全局新拒绝条件，也不得吞掉原 handler 的授权失败。首次开放或续租仍须满足前述实际保守 alarm 已持久化的交付条件；原 handler 不能借维护降级发出新的权限或跳过其自身库存检查。alarm handler 自身处理失败则返回错误并保守 backoff/retry，不报告已完成 expiry；重试仍不是授权来源，不能无限立即重排。

下面仅列复用既有 Owner 签名调用模板时的已知子图，用于说明不能直接宣称“24 次足够”，不是已完成的 cleanup 调用图或集成验收：

| 复用模板的已知子图 | 静态 hop 数 | 未包含的必要工作 |
| --- | --- | --- |
| 三次固定签名 oracle，各含固定服务转发及实际 Directory/Fence 检查 | 3 × 5 = 15 | 本 cleanup 的新鲜管理来源与 Native/session 检查 |
| 三次角色阶段的独立 Fence 读取 | 3 × 2 = 6 | 不能以 oracle 内的观察代签独立角色检查 |
| 固定签名 factory 与 Vault actor 交接 | 2 | World fresh access/read、Stop/Revoke submit、unknown lookup、结果及最终状态回读 |
| 以上未完整子图 | 23 | 仍不是两个独立写原件的完整冷路径；任何实际图必须重新逐 port 计数 |

可继续设计仅限本 TerminationPlan 的固定双原件通道，但在证明保留全部强签/当前来源/Fence/私读隔离/前置预扣、精确原件与独立恢复语义且完整图不超过 24 之前，不能将“合并一次读取/签名”当成已获授权的优化，也不能宣称该候选方案成立。若无法在原批准额度内完成，保留原模板、所有已 claim/unknown、事实槽与分配水位并报告阻断；Closing Ack、自然 session 到期或其中一个操作的 committed 都不等于 Closed。最终关闭必须分别证明 Stop 与 Revoke 的成功权威结果和所需当前 World 后状态，不能用普通 CI 或组件状态代签。

所有期限和三目的预算在首次批准时可见、固定且计入初批总账。明示批准30min就发至同一 write_deadline 的凭据，无每5min重批或周期续领；只有私密领取丢响应/重启时允许原登记key在有效权限内rotate credential generation，deadline、预算不变。ExecutionGrant 到期后只可领取预批 ReconcileReadGrant 的凭据，不发新执行凭据；read_until 到期后停止客户端查询。增加预算、延期、换key/scope或恢复被撤销会话必须fresh approval，新批不能覆盖旧unknown reservation。首版每账号仅1个未关闭TestSession，3个目的占现有4个root的上限，不新增不计数的旁路根。

在正常连续运行的前提下，ready/preflight 要保证当前 native account session 的已知自然有效期覆盖本次 write/cleanup 窗口；不足时缩短本次窗口或由管理端先执行独立批准的 fresh login。当前 GET /session 没有返回 expiry，需在同一 native identity authority 增补受认证 session metadata：server_time、expires_at、idle_expires_at、安全代；取两个 expiry 的较小值检查覆盖，不能根据底层当前30天/7天常量假定外部合同。撤销仍可随时提前终止权限。不能用“已经批准 30min”推导 native session 自动延长。

#### finish、expire、abort 与身份边界

`finish-session` / `abort-session` 是模型可调用的“请求关闭自身 TestSession”typed 工具，只收 session_handle 与 stable request_id，不接受 Stop/Revoke payload、nonce、任意 target 或 owner capability。host/DO 校验仍由该 session 登记客户端请求后，原子关闭新写入，并触发既定 TerminationPlan。预算耗尽、write_deadline 到达或管理员 abort 也产生同一持久关闭事件；DO alarm 是触发机制而非授权来源，重启后补处理到期事件，任何 action 准入/claim 都独立检查时钟，不依赖 alarm 准时。

初次 fresh Passkey 审批对象是类型化 `TestSessionApprovalV1` 计划。计划预授权的是固定资源与确定的停止语义，不能复用已消费的 Passkey proof 来批准后来任意 intent。Owner 在执行每个子操作前生成并保存其精确 typed digest、canonical payload、原 operation/correlation，并验证其是已批准计划的确定实例：Stop 只对应已批准的 Agent/task/executor fence；Revoke 只对应本 session 的 signer delegation，generation/nonce 由已定义的持久 high-water 规则分配。发现外部 owner/task/fence 改变而不满足模板前置时，记录 cleanup_blocked，不猜目标或扩权限。

实际 canonical Stop/Revoke 只能由可信 Owner 组件出签和发送，模型、dot adapter 和 Rust tool-host 都没有 Owner 世界私钥。关闭执行入口后，Stop unknown 不妨碍已独立预授权的 Revoke，反之亦然；每类恢复仍查原操作，不换 nonce。执行 capability 自然到期不等于世界 delegation 已撤销，状态分别显示 write_closed、stop_status、revoke_status、pending_operations、evidence_status，不能仅凭 token expired 显示 cleaned/closed。

TerminationPlan 允许在 execution root 到期后执行，是因为初次已经明确批准该独立目的和截止时间；它不是 frozen recovery 特权。每次发送仍必须经 Identity 持有并验证当前合法 native account session/CSRF、当前 manager credential security version、DO ACL 和 recovery fence。若 native session 过期、credential 撤销、账号冻结或身份 authority 不可确认，则禁止透过预批计划或 executor grant 重登、续 session 或绕过校验；状态为 cleanup_blocked/identity_reauth_required，保留原未知操作和预算，等待独立管理 fresh login/恢复/当前授权的 Stop/Revoke。若 cleanup_until 也已经过去，fresh login 本身不能复活旧 TerminationPlan，新的 owner Stop/Revoke 必须另获当前 fresh intent approval；过期的 read lease 同理。

首次批准不承诺身份撤销后仍可自动完成世界收尾。若该需求以后成立，须另有明确的独立 emergency principal/批准合同，不能隐含加入此实现。普通正常到期的清理和身份冻结后的恢复必须在文档中分开。

断线/host 重启用保留的私密 key、TestSession handle 和原 operation_id 恢复：写窗口和 host lease 均未失效且 session 未 closing 时，才可接续相同根；写窗口结束则只领取 read token、读取原结果；均不重开任务、不清零账目。显式撤销客户端/manager/账户后不可借 read lease 残余自动重连，只能由当前合法独立管理授权查询。

工具宿主另有仅维持当前测试的 `POST /v1/test-sessions/{id}/heartbeat`，每 30 秒一次、lease 最多 90 秒且不越 write_deadline；每 session 至多 64 次，不能恢复已 closing 的测试。同 heartbeat ID 的重试返回原确认，不再次延长 lease。DO 的 admission/claim 和 alarm 都检查 lease；宿主失联到限触发同一关闭事件，活着但无进展的宿主仍受绝对 deadline。重连超过 lease 后只能核对/关闭，不能把 heartbeat 当续权。

首次 TestSession 的当前 World/Native 预检发生在三个可用 purpose roots 发出之前。Account 先以本地强验的完整 fresh-MFA 原件登记 `preflight_reserved`：同 SQL 全 Account 唯一占用 proof nonce/原 approval request、stable approval operation ID 与完整计划 digest，保存初批 control 账、绝对截止与一次预检 claim；此状态只授权有界只读预检，不是 approved 或业务权限。它占用一个未关闭 session 槽及三个 root 预留名额，和仍有效的旧 roots 一起受四 root 上限约束。相同 operation/digest 只读原状态、不重新派发或扣账；不同 digest 冲突，新管理请求 ID 不能重建同 proof。取消、到期或 unknown 保留消费、预算与预留事实，不退回额度自动重试。

预检属于初批 80 中一个稳定逻辑控制请求；每实际 HTTP 尝试预扣对应 hop 的 240 账，Identity→native 的实际来源核验、lifetime、policy、authorize 分别预扣 160 账。FreshAccessPair 的固定调用图包含 policy 前的 Root session-status、policy 原件、authorize 前的独立 Root session-status、authorize 四次实际 HTTP；必须在一次 once-only claim 同 SQL 预扣 4 次，而非只扣两个业务端点。余额不足 4 时整次 claim 拒绝且不得出站，中途失败不退款，重启不得重发该 pair。两个 Root status 仍分别绑定原登记 nonce，不复用一次观察来省略核验。World 与内部 DO/binding 调用受固定 profile 的独立内部调用上限和逐出站 claim 约束，不能混称 native HTTP 或借 Execution/Read/Cleanup 预算。尚未实测闭合固定调用图时该 profile 保持 disabled/blocked，不能静默提高初批上限。最早管理 session 认证属于原有有界管理 metadata 入口，不伪称由尚不存在的 session 账预扣。最终发出三 roots 前仍需真实 native `consume_approval` 对同 proof、Account 登记 verify nonce 的 positive 原件（消费时 proof 在 30 秒窗口内），及当前来源、DF/RF、独立 lifetime、固定 World 强证据和最终同 SQL 复验；本地签名或到期后的 inspect_origin 不能把未消费的预检状态转为授权。三个 roots、配对绑定及预算同 SQL 一次提交，截止始终取初批绝对值，不从提交时重新起钟。

challenge 与后续调用按登记的 stable business request ID/精确控制摘要共享一个逻辑 reservation，各自实际 HTTP 尝试仍独立扣 transport，不能把一次 heartbeat 的 challenge 与调用算成两个逻辑请求。首次私领凭据使用独立 ClaimChallenge purpose：先验登记 key 的 challenge-only PoP，再核实际 enrollment 与已批准 session/purpose，才原子扣控制 HTTP 尝试并占用有界挑战槽；空 token 仅限 enrollment 或该已批准 claim，不能进入 action。未配对 enrollment new/prove 使用独立 metadata origin/IP 限流，不扣不存在的 TestSession 账，也不发业务根。

Enrollment metadata 入口按可信 provider 提供的实际 IP 与固定 public origin 分桶，初版每 60 秒 create 最多 8 次、prove 最多 32 次；另有每 realm 每 60 秒 create 256/prove 1024 的总上限。计数由同一固定 namespace 的独立 bucket actor 原子保存，时钟回退拒绝，窗口历史有界清理不删除 Enrollment 原件。生产不能把请求正文、Forwarded 或普通客户端可替换 header 当真实 IP；缺可信 provider context 时该入口保持 disabled。测试 provider 来源只能由显式测试入口静态注入。上述滥用限额不替代登记 key 的 PoP，也不能据此分配 TestSession challenge 槽或任何业务预算。

初次批准另列同一 TestSession 内的控制预算：配对后 claim/替换、MCP initialize/tools-list、heartbeat 和客户端请求关闭共用最多 80 次不同逻辑控制请求；每个已批准 transport hop 各最多 240 次控制 HTTP 尝试，包含 challenge、调用、失败及同 ID 重试。原生为 host→custody 一份；dot 为 connector→host、host→custody 各一份。Identity 的控制来源核验另有最多 160 次 native HTTP 尝试；内部 DO/binding 调用每逻辑步骤有固定上限，不隐含无界重试。预算是三目的之外的传输/连接成本，不发行第四种业务权限；DO 保存初批总额，host inbox 保存第一跳计数并向 DO 对账。未认证流量仅受入口滥用限额，不凭伪造 ID 扣合法 session 预算；通过认证后的失败/重试计尝试成本。控制额度耗尽触发 closing，但本地拒绝写的状态更新和已预留 Owner cleanup 不依赖该额度；不会把 80 个逻辑请求误算成只够 40 次 challenge+heartbeat。执行/原件读取的挑战分别计入对应 transport 上限，所有额度可缩小、不可由模型提高。

首版客户端控制调用图对 challenge 与需要 live 授权的后续调用分别取得新的来源观察和独立 Root lifetime，每个此类 HTTP 请求实际消耗 2 次 Identity→native HTTP，同一 logical 的 challenge+调用共消耗 4 次；两次请求共用 logical reservation，但各自预扣 HTTP 与 native 尝试，失败不退款。预检已消耗 8 次后，剩余 152 次 native 预算最多支持 38 个此类完整控制往返；80 是独立 logical 硬上限，不是可完成次数承诺。不能借其他目的预算、提高 160 上限，或通过客户端回显旧观察减少核验。若后续减少调用次数，必须先定义可信持久观察槽的真实来源、独立 Current/DFRF 和原 5 秒期限复验，不能缓存授权或延长观察寿命。 Finish/Abort 的第二次调用沿既有纯收紧分支：仍须真实 PoP、原 challenge 和逐 HTTP 尝试扣费，但不依赖新的 native 来源或 lifetime，避免 native 不可用阻塞关闭；它不能恢复权限或延长截止。其 challenge 仍要求 live 授权。每个真实入口 HTTP 尝试包含相同签名重放都计入 240；不得只按签名摘要去重来免除重放成本。内部同一次 admission 的 SQL 回调重试不能重复扣费，入口尝试身份必须由可信服务生成并绑定已认证原件，不能采用客户端选择的 request ID。 通过 PoP 的 Host HTTP 尝试费用必须先独立持久提交，再以同一已付 admission 原件预留 Source/native 成本；后一阶段余额不足、库存限制或准入失败不能回滚前者，失败不得出站，也不得退款。内部回调复用同 admission 幂等而非重新收费。 原生客户端入口固定以 `/v1/account-provisions/{provision_id}/accounts/{custody_account_id}/client/` 为前缀，后接既有闭合路由去除 `/v1/` 后的 tail（如 `proof-challenges`、`client-enrollments/{enrollment_id}/claim`、`test-sessions/{id}/heartbeat`）。两个 selector 只决定有界路由，不授权；完整实际 URI 含 selector 必须被 RFC 9421 `@target-uri` 签名覆盖，Account 必须以实际物理 actor、持久 Draft 和 session 的原 pairing target 复验，不重写 URI 后验旧签名、不扫描全目录、不初始化未知 Account。入口另按可信 provider IP/固定 Origin 限流，未认证请求不能扣合法 session 预算。 此入口独立限流每 60 秒可信 provider IP 最多 32 次、realm 最多 1024 次所有客户端 HTTP；采用单窗口有界 metadata bucket，时钟回退拒绝，与 Enrollment Create/Prove 的 bucket 及各会话业务账分开。缺可信 provider context 时保持 disabled，不信任 Forwarded 或客户端自报 IP。原生连接元数据在该前缀下使用固定 POST `test-sessions/{id}/initialize` 与 `test-sessions/{id}/tools-list`，分别只接受对应闭合 SessionControlRequest kind；不得复用 actions 路由或接受通用 RPC 方法。它们同 heartbeat 一样必须 live、使用 Control 资金，并受原会话绝对期限约束。MCP/dot 的独立入口、OAuth 与双 PoP 仍须单独实现和验收。

`GET /v1/test-sessions/{id}` 返回当前获准主体的非秘密 purpose 截止/余额、write_closed、Stop/Revoke 原件状态和 pending 引用。执行写期限后，登记客户端只能用独立 read 凭据做本次原件/close 查询，以及只会缩权的 finish/abort；普通工具包括 task/industry/context/quote/outcome 全部停止，不新读私有目标。只读 root 的读取上限是 100 次/300 次传输，Owner 查询不扣 execute 预算；预批 cleanup 原件由内部计划生成，客户端读凭据不能请求任意 Owner 签名。

一次关闭依次记录 `closing`、`write_closed`、各自 stop_status/revoke_status、pending 原件核对和自有资源回收；不是第二套世界状态机。仅当 Stop/Revoke 已按当前权威证明、可能派发的 canonical 原件已核清、私有准备/纯读取按 §12.17 明确分类、业务预留已结算/有明确终态、客户端及本轮自有资源回收已记录，才标 `closed`。可能有世界效果而未核清的原件保持 `closing/recovery_required/cleanup_blocked`，保留原件/预留并报告下一步；不能要求 Owner 读取其协议不允许的私有准备结果才能关闭。未关闭测试阻止同账号再次起测；人工处理可以继续独立获准核对/收尾，不能清空记录开新会话。正常 closed 可保留剩余期限内的只读证据访问；显式全会话/账号/身份撤销则连读取也关闭。

#### prepared_world targeted Agent 自测

实施验收复用[开发自测手册](../../testing/manual/agent-runtime-self-playtest.manual.md)、[RunInput/TesterInput/RunResult](../../testing/governance/agent-runtime-self-playtest.design.md#6-接口与数据合同)和既有[试玩任务](../../testing/templates/agent-runtime-playtester-task.template.md)/[报告](../../testing/templates/agent-runtime-playtest-report.template.md)格式，不创建第二套 run schema 或测试专用游戏 Skill。首个实际验收固定 `scope=targeted`、启动模式 `prepared_world`，选一个已真实接通 custody-native-v1 的客户端/runtime（首个为经确认的 codex 原生 profile）；交付的正式 `oasis7-play` Skill 增加该真实客户端的调用方式、原件引用与失败下一步，工具缺失时报告 blocked，不能让试玩者改用本地持钥 CLI 绕过云端接入。

RunInput 固定 source/base/tested-tree、build/正式 Skill hash、Rust/Workers compatibility/API/tool profile 版本、实际客户端/Runtime/model 和执行模式、合法测试账号/world/genesis/service pin/branch/Agent/task/goal/fence 初始证据、setup 完成的预置步骤、允许工具/资源、总工具/模型/业务预算、有效授权、deadline、故障注入点、停止条件和 artifact root。以现有已验证执行模式记录实际 `attached_codex_subagent` 或 `supervised_runtime`；不能用一个模型模拟另一个 runtime，也不因本设计增加额外付费执行。没有支持的派工/客户端、授权或费用额度时记录实际 blocked/inconclusive。

Setup 在开始前合法准备并确认 task/完整目标/独立 executor 委托与绑定、可见库存、设施/site/位置、电力及业务 grants；转运资源按真实双方账本和路由声明。准备过程保持原 owner 与世界权威，不在试玩中临时发资源、跳 tick、改目标判据或直接写世界解决阻塞。独立试玩者只收到 TesterInput：正式 Skill/公开帮助、非秘密账号/任务引用、受限工具、一个不泄露解法的高层目标、允许资源/预算/停止条件和报告位置。不给父开发对话、源码/隐藏 oracle、任意 shell/数据库、Owner 凭据或递归派工权。

最小 live 项目在同一候选与同一世界任务历史内完成，逐项保留证据：

1. **ST-02 接入：** 实际独立子会话读取正式 Skill，使用真实托管工具发现 task/capabilities/industry，取得完整且可用的 native context；只读管理 UI 或脚本 POST 成功不能代签。
2. **ST-03 的定向目标和结果验真：** 给出明确工业高层目标，试玩者自己决定合规参数、查询/行动/等待，并报告阻塞。选中的场景至少产生一个可归因业务效果；例如转运后排产，核对原 transfer 的目标账本到账、原 schedule/action/job、RecipeCompleted 与前后物料/电力变化；如选建厂，另核 FactoryBuilt。不能以 cognition committed、RecipeStarted、动作数或无关 ambient 库存变化代替成果。单纯移动只证明移动子项，不能代签生产。
3. **ST-04 的响应丢失/重连：** Setup 在实际已保存且可能已派发的请求上丢响应；试玩者按正式 Skill 重新连接，凭同一 request/operation/context 引用回读同一任务的原件，再在剩余授权内继续。世界原 correlation/业务事件数量和资源变化证明无重复效果；恢复落在 host lease 内，超出 lease 应转只读/收尾；不能注入新原件或重置世界使其通过。该项不宣称实际 Runtime 进程重启已测；若实施涉及进程恢复则另保留该选中子项的实际重启证据。
4. **ST-06 到限与控制：** 在预声明的 capability/test deadline 到达后，实际验证工具停止新 task/industry/context/quote/action/outcome，仍获准的独立读通道只能核对既有原结果。随后由预先获准 owner 控制完成 quiesce、canonical Stop、delegation Revoke，分别记录 confirmed/unknown；核对所有 pending/预算预留和仍需人工恢复的原件。回收本轮自有客户端/worker/browser/subagent/测试资源；不把子会话返回、HTTP 2xx 或进程退出当作委托已撤销。

主 Agent/核验器回读 ArtifactRef 的 locator/byte_length/digest，并按原 operation、service/world 身份、task/fence、read/commit 与业务 action/job/result 做关联核验。RunResult 保留每个选中 scene/check_kind 的 execution_state、tier/evidence_level、verdict、实际 Skill 读取、客户端调用轨迹、完整/截断观察、原 operation/receipt、目标效果、已知/未知费用、停止与清理、retained_gap；错误与策略/环境阻塞分别分类。源/build 改变后用新 attempt，不覆盖原失败。

CT-007 属确定性合同验证，不能代签 CT-008；CT-008 的 live 记录使用现有 `test_tier_full`，不是本次文档 PR 新增的付费 required check。targeted 只证明实际所选工具、客户端和生命周期项；保留未执行工具、其他 Runtime、cold_entry、完整 starter chain、持续目标/回访/真实进程重启、Web/native UI 与完整 AC-EXT-010 的缺口。缺真正 Agent/工具/世界结果时不能报告闭环已通过；不要求现在运行，也不要求所有 Runtime parity。

CT-007 还须覆盖双跳错 key/tag/token/URI/body、nonce 重放、同 key 凭据替换的期限/预算不变、MCP 初始化前后与持久 inbox 重启；CT-008 单独声明实际消费的 transport profile。dot 必须有真实宿主 adapter、OAuth/签名接线、受限工具、重连和收尾证据，native、mock MCP 或 curl 不能代签；缺宿主能力记录 dot blocked。

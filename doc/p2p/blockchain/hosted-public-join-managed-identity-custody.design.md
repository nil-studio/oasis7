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

首个消费者固定 `native-broker-custody-v1` transport + `custody-native-v1` 工具 profile，由受信 launcher 启动 Game Client，模型用显式 `custody-control-call` 的固定 socket/resource_id/typed args，不依据 sidecar 存在与否回退普通 broker。首批只登记实际验收的 codex 组合；openclaw 须单独通过 custody transport 验收。受信宿主须隔离模型进程对私密目录、keyring、环境/argv 和内存的访问；同 UID 的 0600 文件或“不要读取”提示不构成隔离。

#### 宿主部署声明与实际隔离前置

宿主登记使用闭合 `HostDeploymentDescriptor`：environment、transport/tool profile、host instance ID、客户端公钥、构建和正式 Skill/tool manifest digest、固定 HTTPS origin、受限 runtime identity、隔离机制及验收记录引用。它只描述待配对对象，不是密码学隔离证明。enrollment 公钥持有证明和管理者 Passkey 批准不能把自报 descriptor、profile 名称或 `approvedHost` 布尔值提升为可信宿主。已验宿主集合由既有部署环境的固定配置限定，配置仅含公开绑定与验收引用；尚无实际隔离验收的环境保持空集合或 disabled，不在此流程新增运维签名权根或自动创建凭据。

TestSession basis 的 runtime_ref、skill_ref、source_ref 必须从实际命中部署 allowlist 的 exact descriptor 唯一派生，分别为 restricted runtime identity、正式 Skill manifest digest、该版本 build digest；tool manifest 与全部字段另由完整 descriptor digest 绑定。管理者或模型不能仅回显任意字符串替代来源核验。匹配自报 descriptor 的字段值本身不能使空 allowlist 变为可信。

可接受的隔离必须实际约束模型运行时：宿主与模型采用不同 OS 安全主体/强制沙箱，或模型只通过固定 HTTPS 闭工具面调用独立远程宿主。只有宿主安全主体可读 client key、purpose credential 和持久 inbox。真实验收覆盖文件、keyring、env/argv、debug/process memory、继承句柄、挂载和网络逃逸；同 UID 文件权限、容器名称或 AppSandbox 配置文本不代签这些结果。宿主公钥、instance、descriptor digest、runtime/Skill manifest 或 origin 改变必须重新管理配对，不能用同 key credential replacement 隐式迁移宿主。生产配置默认不启用；合成 fixture 只验证协议实现，不能填实际隔离验收的 PASS。

#### 原生 Control 宿主的窄 IPC 与私密引导

`custody-native-v1` 首个原生宿主只接实际七个 Control POST 路由，不因通用 HTTP profile 的枚举存在而开放 Actions、operation/session GET、Dot 或任意签名。模型方法只接受固定资源 handle 和闭合 Initialize、ToolsList、Heartbeat、RequestFinish、RequestAbort 参数；Finish/Abort 只报告已请求 Closing，不报告 World Closed。模型不能填写 URL、header、purpose token、密钥、解锁材料、descriptor 或运行角色。

Unix 本地纯 UID 配置须把实际 Host 主体 H、受限模型 runtime 主体 M，以及本地 Native/管理主体 N 两两分开。宿主以实际 effective UID 为 H，仅让 H 读取 vault、PoP seed、purpose credential 和 inbox；启动拒绝任一主体 UID 为 0，以及 H=M、H=N 或 M=N。Native 确实远端时不比较跨机器 UID 数字，仍固定远端 authority/origin 并保持本机 H≠M。此代码检查只是必要边界，实际部署仍须通过前述全环境隔离验收，默认 allowlist 保持空。

模型 Unix socket 位于独立、由 H 所有且 M 不能替换的目录，连接权限由受信部署预先配置，不放宽私密 vault 的 0700/0600。服务从 kernel peer credentials 读取 M，拒绝请求中自报 UID/role 和其他 peer；客户端核对固定公开 binding、实际 socket owner 和宿主 identity。原普通 Executor broker 的同 UID/0600 路径不改；`custody-control-call` 只走独立 custody 窄分支，公开 binding 缺失或异常时拒绝，禁止回退普通 `broker-call`，不进入 ExecutorControlStore 或 `--executor-config`。

在没有可信模型 launcher 的首个本地组件中，另以固定 OS 根配置约束同一 M 当前唯一的接线：Linux/BSD 从 `/var/lib/oasis7-custody/model-bindings/<M>.json` 读取，macOS 从无 `/var` 别名的 `/private/var/lib/oasis7-custody/model-bindings/<M>.json` 读取。程序只读此固定路径，逐父目录和文件拒绝 symlink、非 root owner、组/其他可写替换及多链接文件，记录 exact 绑定 H/M/N、identity/descriptor/session/resource/runtime；请求不能选择路径或传 `verified`。缺失、异常或不匹配时 serve、模型调用和自动 heartbeat fail closed，不创建系统文件或 Root 运维签名权根。

该 root-owned 记录只约束 M **当下**的唯一 binding；它不证明模型加载了正式 Skill，也不能阻止 operator 改绑后旧同 UID 进程访问新 binding。可信 launcher、旧进程退役、M 生命周期不可重用及全环境隔离仍须实际验收，未具备时保持 deployment disabled；不得把该记录、artifact hash 或配置文本当作这些项目的 PASS。operator 不应在旧会话/进程未可靠退役时改绑 M。

私密引导 CLI 只由受信 operator 以 H 执行，通过私有 stdin 交接公开邀请/批准会话 metadata，不从模型 argv/env/stdio 取 vault 解锁、密钥或 credential。它生成独立 Host PoP/inbox key，执行实际 Create/Prove 与私密 Claim，完整校验并持久保存回复后才允许模型 Control。若另增引导 socket，kernel-peer operator 集合必须与 M 不交；首版不新增该 RPC。custody service origin 来自固定部署配置，不从 descriptor 的 HTTPS origin 猜测。

Host 私密 session binding 可保留原 V1 plan 或精确 V2 plan；V1 序列化原件不变，不投影或升级为 V2。V2 必须完整校验原 plan（含 mandatory-nullable model budget 和终止元数据），并把其 termination account/provision/session 与固定邀请和 session binding 逐项比较。Host 的本地绑定只限制接线与保守费用/期限，不能替代 Account 已保存的实际 V2 Book、批准、Source、三 Roots、credential history 与当前准入；同一闭合 Control HTTP/RFC 原件仍由受信 Account 唯一 map 决定版本，不接受模型选择版本或切换会话。M 侧五方法和固定 socket/resource scope 不增加任意参数，Execution、Dot 和模型推理资格不由此打开。

Claim 的历史回复只有 purpose/generation，不返回 token。丢失首次 claimed 回复保持 unknown，不能自动重建 session、重领或向模型导出秘密；仅受信 operator 显式指定期望 generation，按既有同 key replacement 合同恢复。每次真实 HTTP 发送先持久登记新 attempt，包括 challenge、失败及重发；无自动 retry、redirect 或退款，重启不得清空账与原件。

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

##### OwnerSetup V1 管理页面与目录元数据

本切片复用上述 custody options、complete、批准原件和未知结果恢复合同；普通 Passkey ceremony 不能替代 custody 批准。首个页面只准备和批准 OwnerSetup V1：`resource_refs` 必须为空，仅支持精确 Move business 0 的既有边界；不发行 Host、TestSession 或 TaskSession，不启用 Execution、自动 continue 或世界写。下述页面与目录 DTO 是待实施合同，不能由独立组件或本地夹具宣称生产可用。

**同源入口与能力。** 页面挂在现有 `mountPasskeyAccount` 账户管理界面，通过相对路径使用同一受控 HTTPS RP origin 的 Native `/api/public/hosted-account/v2/...` 和 Worker `/v1/...`，不增加跨域 Cookie 代理、用户可选服务地址或 CORS 凭证通道。Native `/capabilities` 可新增闭集、非秘密的 custody 管理能力元数据，仅给出版本、是否已配置、固定 audience/environment 和固定相对路径；这不是当前来源、ACL、RF 或授权成功证明。`/session` 的实际认证方式须保留到页面状态；native-key 会话显示须使用 Passkey 重新登录，不能代签。Native 仍逐次检查真实 WebAuthn management session、参与资格、同源 Origin/CSRF 和配置，Worker 仍经固定 management-session-status 验证唯一 SID；配置缺失保持不可用。

**新的有界父项投影。** 新增 `POST /v1/management/parents/query`，Rust 闭集 `ManagementParentsQueryV1` 只接受 `api_version=1`、`limit`（1 至 16）、可空 `after_custody_account_id` 和与非空游标同时必需的 `directory_revision`；不接受 manager、DO、World、Account 路由或查询参数。身份只从实际 Native management source 取得。响应 `ManagementParentsPageV1` 只含本人目录中的 custody account/provision 引用、environment/world 公共标识、目录 revision、冻结/库存完整标记、有界 items 和 next 游标；最多 16 项、编码最多 32 KiB，目录总量沿现有 256 项上限。游标只在同一真实 manager/environment/identity epoch 的目录内定位，revision 不同拒绝重新选择；不返回 private plan、goal、Cookie、assertion、proof、seed 或 key material。Native 能力发布由独立 `OASIS7_CUSTODY_MANAGEMENT_UI_ENABLED=true` 控制，Worker/Owner 查询由独立 `CUSTODY_MANAGEMENT_METADATA_ENABLED=true` 控制，两者缺省均关闭；后者还要求有界非空 `CUSTODY_MANAGEMENT_METADATA_REALM`、精确相等的固定 `CUSTODY_PUBLIC_ORIGIN` 与 `CUSTODY_NATIVE_ORIGIN` 及已有完整 Native/Directory pins。能力只能报告配置，不能假设另一个服务已开启。metadata limiter 在既有 `CLIENT_ENROLLMENTS` 物理 binding 内使用独立 actor 分域与独立必需计数状态，不要求启用 Enrollment，也不复用其限额。

该投影只读取 ManagerAssociations 现有 index/draft 的公共关联，不扇出 AccountAuthority、Native 托管账号或 World，不读取资源、grant、quote，也不把目录登记、label、库存完整或旧阶段当作当前可批准资格。缺目录不初始化业务目录，不把旧库存缺少完整标记升格为空全集。选择父项后，用户明确触发该 provision 的既有 OwnerSetup basis，再以实际 Rust prepare 响应展示并批准；Account ACL、当前来源、Directory/RF 和提交检查不因列表省略。页面不后台轮询列表、basis、prepare 或业务进度。

**独立元数据收费与物理 I/O。** 现有 Native options 的 source 60/min、manager 10/min 及 complete 的 source 60/min 不覆盖新查询。新查询须先以可信 provider ingress 和实际来源 IP，在独立 `management_metadata` 类别内依次持久扣 provider 32/min、realm 1024/min，再发 Native management 认证。复用既有 provider→realm 有界 Durable Object 收费机制，但使用独立分域键和计数，不借 Enrollment、ClientHttp、Control、Host、Native、Execution、Read 或 Cleanup 预算；失败不退款。provider 拒绝后不消耗 realm；缺可信 ingress、时钟回退或状态不明拒绝，不接受普通自报 IP header。认证后在对应目录的元数据事务持久扣该 manager 16/min，再读投影；计数不是权限，不写业务目录/Account、不推进阶段、不生成 grant。重试与翻页都重新计费，不能通过缓存或批次隐藏出站。

一次合法查询的外向服务调用上限为：两个前置收费 DO HTTP、一个固定 Native management-session-status HTTP、一个固定 Identity→OwnerManagement binding RPC、一个 Owner→ManagerAssociations DO HTTP，共五次；目录本地元数据收费与投影在同一有界事务，不另跨 actor 查询。拒绝路径只允许已付费前缀。浏览器到 Worker 请求、独立 `/capabilities` CSRF 获取和 Native 本地数据库操作须分别计数，不能混同这五次或称零成本；写请求若沿现有 helper 再取 capabilities，也须真实记录。这里增加管理元数据防滥用费用，不提高会话业务预算。构建入口必须提供可信收费 context；DTO 大小、目录满量、拒绝前缀、真实 bridge counters、限额隔离未通过时，该目录查询与页面能力保持关闭，不能以本地路由可达代替证明。

**专用批准客户端与 RAM 生命周期。** 页面只复用 WebAuthn 字节转换和实际 assertion 序列化 helper，不调用普通 ceremony 的 complete、cancel 或 operation-status。用户手势前冻结同一父项的 Rust basis/prepare 请求、原 plan、稳定 IDs、完整 Rust intent/audience/environment/World pins、范围/预算/截止和 policy/security 版本；JS 不计算协议 digest，不要求手输 JSON，不从历史 Captured quote 派生当前权限。以安全文本展示完整批准摘要和 V1 资源限制；明确确认后调用 custody options，再使用实际 options 调用 `navigator.credentials.get`。complete 只发送返回的精确 `approval_id`、`ceremony_id`、`operation_id`、`intent_digest` 和实际 assertion response；不假定三个 ID 相等、不补 fallback、不展开原 options 请求。

plan、assertion、proof 和待重试 complete 正文仅存该挂载实例 RAM，不进入 URL、持久存储、DOM debug、日志或分析事件。CSRF 取实际同源值，proof 只交同一冻结 OwnerSetup approve 原件。双击复用同一在途任务。complete unknown 保留确切正文，可显式重送同一 complete，不重生 challenge、request ID 或 assertion；Owner approve unknown 查询原 provision/operation 实际状态，不用新 proof 覆盖未核清结果。proof 过期只按既有核清合同提示重新 MFA，不自动批准。120 秒 challenge、至多 30 秒 proof 和全部五秒 Current/Directory/Fence 检查不变。

取消 WebAuthn 只终止本地交互，不宣称撤销 Native challenge 或 Owner 操作；Native 没有 custody cancel/status API，不调用普通 ceremony 路由。未消费同一原件仅在既有期限内显式重试或等待到期。logout、账户切换与 unmount 递增实例 generation、清除私有 RAM，迟到回调不得对新账户提交或显示成功；已发服务端操作仍须原账户以后经既有状态入口核清，不能把清内存当取消。报错使用闭集阶段/状态，不渲染原始私有正文或内部错误。

**实施与本地证据。** Rust Core 持有新 metadata/capability DTO、校验和 schema 导出；Native 发布非秘密能力且保持批准权威；Worker/Owner 实现可信预收费、ACL 目录投影和真实计数；现有账户模块实现专用客户端与实际 mount。浏览器不新增授权服务。隔离本地 HTTPS 单 origin 组合真实 Native、Worker/Owner、一次性 SQLite 与受控 ACL，沿实际页面以浏览器 CDP virtual UV authenticator 完成 options→get→complete→同一 OwnerSetup approve；不以测试签名 helper 或普通登录 ceremony 代签。覆盖 exact IDs/CSRF、native-key 拒绝、取消、unknown 同正文重送、账户切换迟到结果、双击、目录满量/越界/revision/他人父项、收费拒绝前缀及真实调用计数，并在桌面/窄屏检查键盘操作和批准摘要。软件 authenticator 只证明本地集成，不代硬件、部署或生产测试；后续步骤按用户限制暂停。复用既有 CLOUD003、CLOUD006、CLOUD007 要求追踪，不新增任务身份，不宣称整体 #5076 完成。


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

#### Execution：World 公开派生证明与私密正文隔离

本组件只提供 Native task context 的 World producer、SDK 强验与 Native 固定传输；后续真实 V2 批准/激活及 paid caller 独立接线，不能将 V1 Prepared/Session 静默升级，不以此代替 Transfer/Build/Schedule。World 的真实 `PrivateGoalStore` 持有并读取 PreparedGoal/PreparedTaskContext 明文，既有 snapshot/load_captured 路径从实际 pinned World 构造 Native authority、actor observation 和 base binding，既有 fresh read 在初始及投递前检查 task、prefix、lease 和完整 CommitRef。Identity 独占 custody 侧 Context AEAD KEK。Account 与 Executor 均不得接收两个私密 slot 的正文，包括瞬时 RPC；“不持久化”不是例外。新增证明由实际 World 服务签名 key 生产，明确证明 World 已完成完整私密 SDK 图的验证。Account 验证新 domain 的强签和精确公开 tuple，不声称独立验证它不可见的私密正文及其嵌套签名。裸 digest、Identity/Native 自报 verified、普通 bool 或序列化成功标记不能替代该证明。

旧 Prepare/Get/Live 请求、响应、签名 domain 和 preimage 保持不变；只新增闭合 V2 DTO/route，不向旧 deny_unknown_fields DTO 添字段，不迁移 Owner/Executor/Native/Host key 或 approval secret。新的 observation_nonce 必须是64位小写十六进制，只是观察挑战，不新增权限；后续 custody caller 必须在实际出站前由服务生成、登记并一次绑定尝试，不把 World read 服务不消费 nonce 当作可复用旧观察。World 仍完整验证原 signed Prepare/Get、Native 模式、当前 task/Owner/Executor/lineage、prefix 和 lease；domain 的强签绑定整个新请求。

| 新请求/固定路径 | 新投递与公开证明 |
| --- | --- |
| `TaskContextPrepareAttestationRequestV2 { original: SignedReadRequest<TaskContextPrepareV1>, observation_nonce }`；`/v1/world/private-task/context-prepare-attested-v2` | 外层新域强签 `TaskContextPreparedDeliveryV2` 同时携旧域完整 signed Prepared（只给 Identity）和独立强签 `TaskContextPreparedProjectionV2`（给 Account）。内层 domain 为 `/v1/world/private-task/prepared-projection-v2`。公开证明含原 signed Prepare、精确 ContextIdentity/full fence、实际 read/activation CommitRef、captured base binding 与整个旧 signed 私密响应的 canonical-CBOR SHA256/长度。不含 private goal/text、reference salt 或 actor observation。 |
| `TaskContextGetAttestationRequestV2 { original: SignedReadRequest<TaskContextGetV1>, observation_nonce, prepared_projection }`；`/v1/world/private-task/context-read-attested-v2` | World 强验前一公开证明，并由真实已存 record 重算 Prepared commitment，再执行实际 fresh Captured read。外层 `TaskContextCapturedDeliveryV2` 携旧域 signed Captured（只给 Identity）和新内层 `/v1/world/private-task/captured-projection-v2` 强签 `TaskContextCapturedProjectionV2`。公开字段含原 Get、前一证明 digest、精确 ContextIdentity、capture/read CommitRef、整个旧 signed Captured canonical-body commitment 与原 canonical Prefix/correlation commitment；不夹带正文。 |
| `TaskContextMoveProjectionRequestV2 { original: SignedReadRequest<TaskContextGetV1>, observation_nonce, prepared_projection, captured_projection }`；`/v1/world/private-task/context-move-projection-v2` | World 强验精确公开证明链、原 Get/Prepare 和 body commitments，再从真实 store/load_captured 取得当前完整数据、执行全部 SDK 验证和精确 delivery pin 检查。返回仅公开的 `TaskContextMoveProjectionV2`：ContextIdentity、前驱证明 digests、真实 fresh read/capture CommitRef、fresh normalized base binding、实际 observation digest 和 Native capability/authority hashes。不接受 caller Action/Cognition，不返回 observation raw/text；Account 仅从批准的 Move 取 actor/坐标并按真实 SDK 算法重建 Cognition。 |

所有公开 DTO 固定版本/profile、拒未知字段、canonical decimal 与有界完整编码；仅 Native profile 可达。原件、完整 ContextIdentity/Owner/Executor binding、delegation generation、World/genesis/branch/reorg/服务 key 和独立 nonce 必须逐项相等，缺适用证据拒绝，不把缺 floor 推为零。body commitment 覆盖整个旧 signed 私密响应（含旧 request digest、signature 与 typed payload），不是未签 subset 或 JSON 投影。Account 只在独立 World 强签 domain 内使用 commitment 作精确关联，不能裸回显 hash 授权。

后续 custody 接线只有 Identity 可接收私密 delivery。它必须先强验外层、两个真实 typed 内层原件、公开 projection 及整个 canonical-body commitment，再用原独立 Context KEK/DEK/AAD 封装；两个完整 slot 仍各64KiB、wire 仍256KiB。超限明确阻塞，不截断、不在 unknown 时重 seal/reput，不接受 caller reference。该接线必须使用显式 FullSignedResponseV2/AttestedSlotRefV2 新格式：64KiB 上限覆盖整个旧 signed response canonical bytes；旧 payload-only 格式不得跨用。Account 的新 attested reference 只能存 configured signed public projection、固定 scope、commitment、实际 actor metadata fact 和 immutable envelope digest/wire length；从不收 body/envelope。旧 ContextRef 保持旧语义，不能静默升级。ref-only restore 强验公开证明链及闭合关系，但不生成 usable context 或新 grant；实际 actor 读取、当前 Source/LF/private SID generation/Directory/RF/inventory 仍独立核验。

本协议是 World service assertion，不是新共识授权；Account 不能将投影存在、unsigned candidate、HTTP200、cognition committed 或目标坐标相等解释为实际动作成功。旧 V1 写通道与结果语义不改。后续原件 outbox、业务效果/usage、paid Current、实际 Actor/KEK bridge 与完整 Execution/ReconcileRead graph 均须独立闭合。

实际 Native 固定操作的已知三条物理边为 Identity→Native、Native→World VIEW、Native→一个上述固定 World route；双 part delivery 是同一次真实响应中的两份强签，不是批签未来操作，也不额外抓第二份 projection 后隐去费用。其余 private Actor、factory/signer、协调 RPC、Root/LF/Directory/RF/Source 与 poststate/usage 的每次实际出站另计并预付；失败不退款、不自动重试。保原 Source 五秒、票/nonce和绝对 deadline，不隐式刷新/续 grant，不借400/300/160，不宣称全图已闭合。无真实 paid caller 的组件不能开公网 Actions/TerminationTool，也不能充部署/实玩验收。

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

<a id="cloud-custody-resource-basis"></a>
##### 资源事实的独立 setup V2 原件

Trace refs: O7-REQ-f837065c-4da5-4e6f-8a53-f4e5900cf26c；O7-REQ-f98670c4-c1fd-4610-8884-8eb2cf89c1eb。

以下是带资源候选的闭合目标合同，不能据此宣称实现或预算图已经验收。已有 `OwnerSetupPlanV1`、`OwnerSetupV1`、普通 CurrentGoal 与其签名 wire/恢复语义保持不变；V1 资源集合仍为空，只能接续精确 Move 且 business_limit 为零的 TestSession。另设独立 canonical domain 的 `OwnerSetupPlanV2`，只从原有 fresh setup 起点创建，使用新的独立 fresh-MFA 原件，不升级已消费的 V1 计划、不改旧 task/goal，也不绕过既有或未知 delegation 分配库存。

V2 完整签入一个闭合 `ResourceBasisSelectionV1`，首个候选至多一个精确 Transfer、Build 或 Schedule 动作及全部固定参数。selection 是管理者批准读取哪个候选的选择，不是 `resource_refs`、资源 grant 或业务权限；actor、Owner、Agent、World、task/goal/Executor 和完整 fence 均由实际 participant 及原 Publish/Bind 原件确定，不能由选择器指定。ID 仅来自管理者已知的预置配置；不新增发现/枚举、预批准免费 probe 或从尚未批准的 Execution400/Native160 借费。V2 仍是原六阶段、四个 canonical 写和一个私有 PrepareGoal，不安装 AgentDelegationGrant、不发资源、不预留 World 资产；缺少真实适用 grant 时保持 pending，而不是增加第五写或把 signer delegation 当资源 grant。

FinalObservation 保留原先付费的 preflight OwnerSetup 观察与 Admission，但资源 basis 不能取自这份较早观察。新增闭合 `OwnerSetupFinalResourceReadV2` 原件和 `EntryOwnerOperationV1::ReadResourceGoal` variant；新原件绑定 setup/provision、V2 plan digest、实际 World/Agent/task、原 goal reference、完整预期 Owner/Executor fence、精确 selection 和本阶段独立 nonce，由已付费 Owner factory 从保存原件唯一派生并签名。Native 仍在原 `/entry-owner/control` 路径先取得并核验当前 owner-view、真实 SID/CSRF/Account、World pin、任务与 Owner，以及 await 后的当前会话；随后仅把原 `ReadGoal` 的那一次 World HTTP 替换为固定新 domain `/v1/world/private-goal/current-resource-basis-v2`，不再追加资源或 Captured quote HTTP。

World 从同一个实际 pinned World 构造 `OwnerSetupFinalResourceObservationV2`：包含完整 OwnerSetup control/signer-delegation 语义、CurrentGoal 的实际 activation/private PreparedGoal 证明与 `ResourceBasisFactV1`。复用原 current-goal 的 active publication、历史 activation pin 和私有 store 校验，并复用纯 SDK payload/嵌套 Owner proof 验证；新外层服务签名绑定整个新请求与全部 payload，不能伪造旧 CurrentGoal domain 签名或只换 wrapper。原 before-delivery fresh pin 必须保留；V2 还要求完整 CommitRef 与最初 pin 精确相等，grant、材料或任何 World commit 改变即拒绝，不能把新的 quote 拼入旧 goal、自动重试、刷新 Source 或放宽五秒。Account 最终 Result 强验该最终响应本身的同 pin owner/task/goal/Executor/delegation/资源，并与已验 Publish/Bind、preflight 原件及独立 Current/SQL 精确关联；最初 owner-view 成功不代替此最终证明。

资源事实只从该 pin 的真实状态提取。Transfer 必须复用实际 Agent 因果决定、delegation admission 与 source ownership，再计算精确 from/to/material/amount/route 的物流 quote；单独物流 quote 或任意历史 grant row 不证明授权，foreign Agent/Site/Factory 或 World source 不因持有 grant 而放行。Build/Recipe 复用真实 private-clone factory/recipe quote 的 owner、grant source/revision/window/allowance、site/location、输入输出 ledger、材料、电力与条件拒绝。grant 必须真实匹配且非 revoked/expired/ambiguous/exhausted；不存在 ledger 不能用默认零余额伪装存在。Build 的拟建 factory ID 仅为候选，不标成已存在工厂的 owner 事实。纯 quote 不修改原 World、grant spent/decision、材料、tick 或任务，不接受 signed Captured HTTP quote 替代批准前证据，也不构造 fake Prepared fence。

V2 Prepared 保存独立 `PreparedResourceBasisV1` provenance：原 setup plan/最终读取摘要、完整强签事实关联、typed selection、as-of commit/tick 与从这些 typed 事实唯一派生的 refs；它不改 V1 plan.resource_refs。严格恢复重验原件、签名、同 pin、前序与 refs 派生关系，拒绝调用者的 verified 标记。refs 只是 allowlist/展示中的集合，授权仍须完整 typed selection 和事实；同名字符串不能跨 ledger、site、factory 或 material 类型扩大动作。历史 final quote、Prepared/Captured/ContextRef、存储成功或可恢复 provenance 都不能恢复当前资源权限或派发权；后续每次 live 使用仍独立验证 fresh Source/Root/DFRF/当前 World、原期限、原批准动作与真实业务 admission。

本候选只替换原物理路径，不把 HTTP 内部工作隐藏成一跳：FinalObservation 受改局部仍包括四次 Identity→Native HTTP（preflight Root、owner-view、effect Root、OwnerControl）与三次 Native→World HTTP（preflight owner-view、OwnerControl 内 owner-view、最终 Goal+Resource）。资源 evaluator 仅访问已 pin 本地 World；原私有 goal store/历史 pin 仍实际执行，新增 clone/编码/验签成本也须测量。完整阶段 95/96 与 setup 584–600 账不因此自动成立。请求、完整响应及 wrapper 保留各原边界（World owner signed 子响应至多60KiB、已有64KiB传输边界及各入口原限制），不分块、不裁证据、不提高 cap。真实 SDK/Native/World bridge 逐物理 HTTP counters、先扣费/once、原五秒窗口、最坏合法正文和 CPU/存储成本未验证前，此 profile 保持 disabled/blocked，不开放执行或以组件通过代整体验收。

Resource final 的结果回调使用独立 V2 outbox 的 `result_callback` 原件，不能把旧 `common.claimed_at` 或 `common.callback_claimed_at` 填成已派发/已回调；两者在本 profile 继续必须为空。该字段是严格必填的 nullable：新阶段初始显式 `null`，实际 `reserve_owner_setup_resource_result_callback_v2` 才以同一 Account 事务写入唯一 claim。claim 绑定完整实际 dispatch record、已签 Resource original、原 Source record 摘要、实际 claim 时刻和原 dispatch 绝对截止，不接受 caller 传入 claimed/verified 标记，也不刷新 Source、签名、原件或期限。这只是既有第六阶段 96 固定图内的 result callback 预留；原一次 `RecordResourceResult` Actor RPC 及其已列账的 Directory/Fence 两次实际读取不增加、合并或借费。claim 必须在任何这两次外部读取之前持久化；其后失败或响应未知仍消费，重复 claim 在外部读取前拒绝，不凭空增加恢复 RPC 或成功缓存。

最终完整 Node/Native 结果只有在该专用 callback claim 存在且与实际 dispatch/original/Source 完整匹配、claim→完成顺序和原五秒/实际更窄窗口成立后，才可继续执行独立 fresh Current、固定外部 Root/Node 强签和最终 SQL/Prepared 工厂。严格恢复重算 claim 关联和原截止，拒绝缺失 `result_callback` 字段、孤立 claim、重复/错配、倒序、越期以及没有 claim 的已完成结果；旧实验性 completed-without-callback 或未知回调记录不得被默认值、推测时间、旧 common 字段或重放幂等 backfill 升级。V1/非资源 outbox 与原字节/恢复语义不变；本组件不证明实际 Directory/Fence 调用者、World floor/once、完整 96/584 图或可执行资源动作已经接通，未闭合仍 disabled。

Resource final 的正式 Native producer 仅接受实际新 `OwnerSetupV2` 资源 selection 的完整已消费 `ApprovalAdmissionInput` 与原第六阶段 96 预扣派生的 `PaidResourceReadProofV1`（一 Native、两 World 固定子端口），不另造批准、Book、summary 许可或预算。新 Native body 是闭合的 `{api_version:2,input,participant,request}`：`participant` 必须由真实已接受 Participant descriptor 与该 OwnerSetup engine 保存的 ParticipantSessionRef 唯一派生，`request` 保留强 typed access 与 `ReadResourceGoal`，全体绑定进原 `native_request_digest`；proof 仍只经下述固定认证 header 传递，不能塞进 body 或允许普通新 variant 绕过。Native options 在建立这种新 Resource 记录前显式进入已有受保护 restore lineage；complete、注销及撤销保持同一 lineage，记录不可回填的 producer/provenance。旧无 provenance 的完整 proof 与旧 Consumed/phase=None 均不得因开关、部署或幂等 commit 升级为新 dispatch。Native 只核其实际本地 SID/session-ref、binding、credential、generation 与 Root；Cloud private_sid_generation 仍由真实 Identity actor 在自身私有 header before/after 核验，Native 不自报该世代，Host 不接收原 SID、Seed 或完整私有原件。Native→World 仅转实际 access/resource 原件和同一付款 header，不把完整 Native MFA input 或 Cloud 私有会话 metadata 传播到 World。World 接收端仍须独立 fixed issuer/floor/once、实际 Participant Admission/Owner、同 pin grant/ledger/quote 和 delivery-current；缺任一真实 consumer 时生产继续 disabled，不能把 Native 组件代签全链。



##### V2 批准、付费读取凭证与零 Root 准入

Trace refs: O7-REQ-f837065c-4da5-4e6f-8a53-f4e5900cf26c；O7-REQ-f98670c4-c1fd-4610-8884-8eb2cf89c1eb。

本段冻结后续接线目标，不表示该链或收费凭证已经实现。两个批准原件独立：`OwnerSetupPlanV2` 由新的专用 custody fresh-MFA 精确批准 selection、目标与原六阶段预算，消费后才允许其先付费 Owner factory 派生 `OwnerSetupFinalResourceReadV2`；最后既有付费读取返回同 pin 的历史 `PreparedResourceBasisV1`。随后 `TestSessionPlanV2` 与新的专用 custody fresh-MFA 精确绑定该 basis digest、同一 typed selection、完整账户/World/Owner/Executor/配对与 Runtime/Skill/source、实际动作限制、原期限和各独立预算。第二次 consume 只建立控制 preflight 权威，仍为零 Roots；原 Source、FreshAccessPair、LF 前序成功后，把原 OwnerRead 的一次 Native HTTP/一次 World HTTP 替换为同 selection 的当前资源读取，独立完成 full Current/账号/Source/World/实际 grant 与最终 SQL 准入才发行 Roots。历史 basis 是用户批准选择与来源的证据，不是当前价格、权限或派发承诺。若要求第二次 MFA 前展示另一份最新 quote，必须另行闭合事先批准的管理读取与费用，不能从尚未消费的会话借权或免费提前 probe；本 profile 不增加该读取。

V1 的计划、原 domain、字节、恢复与 resource_refs 空集合/精确 Move0 守卫全部不变。V2 是独立 closed DTO/domain：`OwnerSetupPlanV2` 包含原固定 setup/provision/目标/goal/五个操作 ID、独立 nonce、Executor 与完整费用/期限字段及单一 `selection`，不含可自行授予的资源 refs；`PreparedResourceBasisV1` 必含 V2 plan digest、完整最终读取原件及强签响应关联、typed selection、as-of 完整 CommitRef/tick、唯一派生的 typed refs；`TestSessionPlanV2` 必含该 prepared digest、同 selection、完整批准 scope 与各费用/期限。三者使用独立 canonical domain，拒未知字段、非规范 decimal、缺必填关系、旧批准替用与任意 verified 标记。完整 quote/Prepared/Captured/ContextRef 或历史结果均不能替代本次当前资源准入。四个 canonical 写不安装资源 grant，缺真实 grant 继续 blocked。

V2 凭据 challenge/claim 继续使用既有 `/v1/account-provisions/{provision}/accounts/{account}/client/…` 精确路径、原 closed body、原始 RFC 签名 target 和正文，不新造 `/v2` 兼容 URL，也不由客户端 header 声称版本。首次已有 Account Admit 在任何 V1 专属状态重验之前，按完整账号/环境/provision/session namespace 检查可信持久 V1/V2 两表；必须恰有一份对应记录，两表冲突或均无即拒绝，不能在任一版本验证失败后 fallback。V1 原返回结构和签名字节保持；V2 首 Admit 仅对 CredentialClaim challenge/claim 返回独立 closed routing-version 元数据，精确绑定原 ingress、操作、原正文及批准输入摘要。该内部版本响应使用独立类型/domain，只决定后续固定 callee，不是 Current、verified 标记或新授权。Identity 必须延续已经完成的同一次 Admit，不能为选择版本再发 Admit 或隐藏额外 Actor/RPC；其后每个 Account 阶段仍重验同一 raw PoP、版本/批准输入及持久 paid binding。两表创建、恢复及 claim 关联必须拒绝 namespace 碰撞；旧 V1 记录不能投影升级为 V2。任何新调用边按原 Control/Host/Native 账计费，缺实际先付费消费者时保持关闭。

Resource 的正式 World reader 不能把现有 `current_goal_history → pin_at_height` 当本地免费步骤，也不能删去真实 activation pin。采用 Node 私有的 qualified activation archive 作为该历史 pin 的本地物化证据，旧 Resource 请求/响应、旧 V1 GoalPublish 原件/域、World 根编码和 canonical 写入数均不改变。archive 只保存已经发生的 GoalPublish 事实，不是 Current、Source、Owner read 或付费授权；缺证据的旧任务或启动时已有目标保持 Unavailable，不以当前 CommitRef 回填历史，也不发 P/R Head/CAS 或 Native 补查询。Trace refs: `O7-REQ-f837065c-4da5-4e6f-8a53-f4e5900cf26c`、`O7-REQ-f98670c4-c1fd-4610-8884-8eb2cf89c1eb`。

archive 的唯一 producer 是实际 Formal writer 的 verified publication 接缝：在相邻的 before/after `World + VerifiedFormalReadAnchorV1` 上分别核实完整 snapshot hash、World/chain/genesis、governing/projection 根，核实 after 的 `parent_record_id` 等于 before 的实际 record ID、after.before_state_root 等于 before.after_state_root；仅为这次确实新出现且成功的完整 typed GameGoalPublish canonical result 建立记录，保留完整原件、correlation/result、对应真实 after qualified anchor/public CommitRef 和该 activation snapshot 的本地内容寻址引用。不能仅凭 `committed_height == World.time` 判断新提交，更不能把 Formal position 与 Runtime height 作数值比较。首次 publication、缺失相邻 cut、publication gap、非匹配结果或 archive 缺项都不能补写当前 anchor；离线修复也只能消费独立验证过的真实历史 qualified cut。`world_service_results` 虽从授权 root 投影排除，仍由完整 execution snapshot/journal 提交，不能把该投影规则误称 World clone 会清除历史。

archive 是显式版本化的私有记录。持久材料必须足以在普通冷恢复时重新核验实际 qualified anchor 的现有证明链及其完整 activation snapshot hash、完整 typed canonical result/Owner 签名与该 snapshot 中的精确结果；不能只存摘要、自报 verified 标记或当前根。activation snapshot 可以按内容摘要共享，必须实际保留在 Node 私有本地存储，禁止把 remote CAS 引用当已持有字节、禁止缺块时在线补取，禁止向读者导出整份私有 snapshot。索引、原件、证明、引用和版本/mandatory 字段反向校验；篡改、缺材料、根不符、分支变化或不兼容版本均拒绝。需要的事实仍被当前活动 publication 引用时不得被普通裁剪删除；额外本地 CPU/存储开销须实测，不声称因无 HTTP 就无成本。

publisher 不能在 Node/Viewer 共享锁里做 archive 文件 I/O、fsync、历史解析或等待 worker。仅移交有界、不可由 wire 构造的 owned qualified material，由锁外单一持久消费者完成严格验证/提交，再发布只读 archive 可用状态；压力、故障或缺材料只使对应新 Resource 观察不可用，不把旧 V1 查询改成假成功，不把未完成写入当 ready。HTTP 消费还必须从当前实际 qualified cut 重验 active publication 与 archive 中的完整 canonical result 精确相同，并执行原 private goal store、当前 Owner/participant/admission、task/goal/fence/delegation、真实资源 owner/grant/ledger/material/quote 检查；archive 命中不能代替任何当前权限。最终 delivery 再取独立 fresh qualified cut、完整 CommitRef 精确相等并检查原绝对 until，才签发原强 typed 响应。

World 端两次实际 HTTP 必须分别消费现有 Account 第六阶段 96 预扣原件所签出的固定 World2 子端口，而不是调用者的 paid/verified bool，也不是 Native160、Control、Exec、Read 或新增可扩充 cap。复用同一 `PaidResourceReadProofV1` canonical codec、配置固定的 issuer/stage/World/audience/epoch 和原五秒窗口；preflight Owner 与 final Resource 各严格匹配自己的 route/完整 request digest，后者另核完整 typed selection/fence/signer 关联。每个实际 HTTP 在 pin、archive/private store/quote 或任何出站之前，先在 World 自己的持久事务内核验 issuer/floor/原件关联并消费唯一 `(issuer namespace/service/environment, World audience, paid_attempt_id, closed substep)`，两子端口互不替代；失败、unknown、重复或轮换不得退款、清占用、刷新原件或返回成功 cache 使新 HTTP 免费。返回的只应是不可 Clone/Serde 的进程内一次性消费 token；其构造只来自真实持久提交，不接受 caller bool。World floor/once 独立于 Native 的 guard；普通 SQLite/备份恢复与完整本地盘/VM 回滚的受测/未覆盖边界仍按前述正式要求保留，不能用自带 pin、同一数据库或本机 marker 自证整体防回滚。实际 archive producer、World floor/once、锁外接线、ordinary route 零 fallback、正常/过期/重复/篡改/恢复/交付漂移以及每条真实边未共同验证前，profile 继续 disabled。

**新付费 Resource 回包不得降级为 ordinary 观察。** 旧 ParticipantOwnerView 无付费 header 时继续原 V1/Move0 ordinary 语义，不能泛封旧路由；但新 Resource 消费者不得仅凭该旧路由相同 Owner 请求及合法 Node 签名，把旧观察视为本次 World 子端口已经消费。新 World2 profile 使用独立闭合 `resource_paid_world_reply_v1` 回包，固定签名域 `oasis7.world.resource-paid-reply.v1`，其 Node 强签覆盖完整原 `PaidResourceReadProofV1` canonical digest、固定 stage、唯一闭合子步骤 `owner_preflight` 或 `resource_final`、实际固定 route、完整 typed HTTP body digest及原 typed 观察。proof digest 包含实际 issuer 签名与 paid attempt/record，不能仅取不含 paid 原件的旧 request digest。该回包只能由前述 World 持久消费 token 构造；Node 固定 key 与完整 Current/delivery/floor/原绝对期限检查仍必需，回包不是新的 Owner/Current 权限，也不增 RPC、费用或预算。

Native 新 Resource gateway 必须先以本次保存的 paid frame 重建上述完整预期 tuple、强核固定 Node 的新域签名并拒未知字段/版本/子步骤，再交既有 typed Owner/Resource verifier 核原观察；ordinary `SignedServiceResponse` 或旧 Resource 结果即使 Node 签名、Owner 原件和 payload 均正确也必须拒绝，不能只剥外层或重算旧 digest 后接受。header 只选择新 handler，不授 authority；剥 header、缺 proof、改 attempt/子步骤/route/body 或回放另一强签 paid 原件的回包均不得推进 Native completed/Resource callback/Prepared。旧普通消费者和旧 wire 不升级。有效负例须先证明同一旧 ordinary 回包的原 Node/Owner 验证通过，再证明新 paid 消费目标拒绝且状态不变；实际 World ledger 未消费不得声称 prepaid success。新 SDK/Native/World 三方回包生产及消费、真实 HTTP 剥 header 与交叉子步骤测试未共同验证前，profile 仍 disabled。

两次付费 Node 外层原件必须随本次真实 Native result callback 保留到 Account 的独立 Resource final proof，而不能在线验过后只存旧 inner 观察。Native 新 Resource 完成返回显式闭合的 paid OwnerSetup envelope，包含本次 Native 实际收到的完整 signed PaidResourceReadProofV1 及 preflight 与 final 两个上述完整强签原件；旧 generic NativeResourceEnvelopeV1 及其 ordinary wire 保持不变，新 Resource result bridge 不接受它冒充新 paid 完成。Core 的新完成记录持久保留该实际 signed paid proof 和两 Node 原件，字段真正 required-nullable（缺字段不自动 None），并在 strong result admission、Prepared 工厂与每次 cold restore 先由实际 dispatch/Consumed Book/Source/Owner 完整原件重建 expected paid payload、用 dispatch 已记录固定 issuer 强核该收到并持久保存的原 signed paid proof，再重建两套固定 binding，外部 configured Node pin 逐个强验，final inner 必须与归档观察精确相等。None、缺失、旧 capture、只复制新的字段存在性或任一无对应真实 paid dispatch 的材料均不能发新 Prepared/读取或发送资格，不因环境启用回填、迁移或称已付款；旧实验完成缺新证据保持不合格，不静默升级。真正旧 V1/Move0 ordinary 消费不改。完整新的 Native envelope 连同两原件按现64KiB上限实际序列化计量，不能只量 inner、假预留或把额外证据当免费附带；超限失败保已消费 once，不刷新/退款。本保留不增加 HTTP、Book、MFA 或预算。

**Resource 观察的两种提交坐标必须显式区分。** 既有 `CurrentGoalResponse`、V1 及 Resource 旧 `None` 表达保持原字节和 Runtime 高度核验；不能把它们的 `activation_commit.position == active_goal_reference.activation_committed_height` 规则用于真实 Formal cut。`FormalReadPublication` 的原始 `CommitRef.position` 是 Formal 日志位置，而 `active_goal_reference.activation_committed_height` 是 Runtime 高度；两者即使某 fixture 数值碰巧相等也不是同域，禁止重写原 CommitRef、用当前 tip 替原激活、跨域数值比较或伪造父时间。

仅 Resource V2 的 `ResourceCurrentGoalV2` 增加 skip-None 的可选闭合事实字段 `activation_profile`；`None` 继续按既有 Runtime 高度模式验证，唯一新 `Some` 为 `formal_qualified_v2 { committed_runtime_height }`。该选择是观察的坐标编码，不是 Owner、Current、付款或读取许可。新 producer 只能从前述真实完整 qualified activation archive 同一次转移取得原 Formal CommitRef、实际 canonical Goal result 的 Runtime committed height 和对应 control 原激活引用；不能由 HTTP 请求、自报 checkpoint 或任意 current snapshot 拼接。原始 `activation_commit` 保持真实 Formal World/binding/record ID/root/position，新增 Runtime 高度只与 canonical result/control 引用在 Runtime 域精确比较，激活与当前的 Formal 位置只在同一已证明的 Formal 链内比较。完整节点签名覆盖该闭合字段与原响应；Node 固定 pin、完整 Owner 请求及 task/delegation/scope、历史 Owner proof/私有 Goal receipt、selection/grant/ledger/material/quote 和第二次 Current/floor/until 校验均不得减少。

SDK Resource 专用 verifier 必须按上述显式模式验证全部既有 Goal/Owner 原件与两套坐标，不得把新 `Some` 转成旧 `CurrentGoalResponse` 后调用旧坐标比较，也不得全局放宽旧 verifier。Core 的强结果归档、冷恢复和 Prepared Resource 工厂复用同一 Resource 专用 verifier；字段缺失或未知闭合模式不能因环境启用而补成新模式，旧保存结果不回填。新 codec/真实 Formal cut 正控及坐标失败测试、强签消费、冷恢复和实际 receiver 未共同通过前，正式路径仍关闭；本编码不增加 RPC、预算、Book/MFA 或任何 World 写入。


新增只由实际 Account 批准/预扣权威经唯一 Account service issuer 的固定 typed 工厂签发的 `PaidResourceReadProofV1`，不是模型工具参数、Native 签名代理或可恢复 token。闭合签名 payload 字段固定为：`api_version=1`、`issuer_namespace`、`issuer_service_id`、`issuer_environment`、`issuer_key_id`、`issuer_epoch`、`issuer_config_generation`、`native_audience_origin`、`world_audience`、`native_path`、`world_path`、`stage`、`custody_account_id`、`manager_native_account_id`、`participant_native_account_id`、`provision_id`、`setup_id`、必填 nullable `test_session_id`、`approval_operation_id`、`approval_input_digest`、`plan_digest`、必填 nullable `prepared_resource_basis_digest`、`selection_digest`、`owner_public_key`、`owner_binding`、完整 `task_fence_digest`、完整 `signer_record_digest`、`native_request_digest`、`world_request_digest`、必填 nullable `preflight_world_request_digest`、`source_operation_id`、`source_record_digest`、`source_security_epoch`、`service_epoch`、`directory_generation`、`resource_flag_generation`、`paid_attempt_id`、`paid_record_digest`、`native_http_units`、`world_http_units`、`issued_at`、`expires_at`。`stage` 仅 `owner_setup_final` 或 `test_session_owner_read`；nullable 字段按阶段必须精确 Some/None，不能省略。金额/世代/时间使用规范十进制字符串；digest 均为完整原件的独立 domain digest。外层仅 `payload/signature_hex`，domain 固定 `oasis7.custody.paid-resource-read-proof.v1`，用既有规范签名 codec 对完整 payload 签名；不得从调用者公钥建立信任。

Producer 在同一持久事务中核实对应 V2 custody 批准已经精确消费、当前账号/Source/管理 scope/配置与原期限有效、原件及 selection 匹配，先按原阶段账扣足全部物理端口并分配不可复用的 paid_attempt_id/记录，再允许签发该唯一原件的凭证。OwnerSetup final 此凭证的 units 精确为 Native1/World2，绑定原已有 OwnerControl 请求及其两次 Native→World（access 中已保存 owner-view 与最终资源读取）；preflight_world_request_digest 精确为 access 原件摘要。该凭证不是声称完整阶段全部只有一次 Native/两次 World；对应其它 Identity→Native 及 Root/preflight 仍按原阶段完整预扣。TestSession OwnerRead 的 units 精确为 Native1/World1，preflight_world_request_digest 必须为 None；原其他 Root/Source/LF 端口按既有账先扣，不由本凭证覆盖。`expires_at` 不晚于 `issued_at.checked_add(5000ms)（溢出拒绝）`、原 Source 保守有效截止、Native session 截止、批准/阶段/会话原期限及发行 key 有效期的最小值。签发时间不能刷新 Source；不重试签发以延长旧原件窗口。未知签发/发送保留原件、记录和已扣费用，不退款、不互借预算。Core 持久恢复必重验 stage、完整批准/原件/来源/paid record 对应、计数、冲突与所有 mandatory 字段，不能 default 补旧未知状态。

Issuer trust 由运营配置中的显式 pin 决定：`namespace/service_id/environment/key_id/public_key/issuer_epoch/issuer_config_generation/not_before/not_after/native_audience_origin/world_audience/allowed_stage`，并与原 bootstrap config version/World pin 绑定。这是服务签名专用 key，不复用 Host purpose key、Owner/Executor seed、普通 WebAuthn credential 或 Native participant admission issuer。Native 与 World 只接受当前配置已启用的 exact key/epoch/audience/stage；缺 pin、过期、换环境或撤 key 均本地拒绝。Native 与 World 各自持久保存已接受的 issuer_epoch/config_generation 单调 floor，配置版本必须不低于该 floor，更新 floor 与 key/config 的准入必须原子失败关闭；不能仅用当前 pin 文件宣称防回滚。floor 的可靠性必须来自不能随该服务完整本地盘一起回滚的受保护单调存储或独立已授权外部可信 floor。若只持有可随盘快照一起还原的本地文件/数据库，就无法自证没有整体回滚，本 profile 对该风险保持 blocked，不通过免费在线查询补洞，不以普通重启恢复为防回滚证明。轮换须先使旧 issuer 停止签发、结束其最大五秒在途窗口，再由受保护部署更新 exact pins；若需立即撤销，由独立获准运营停止该 profile/服务，不新增免费在线 key/revocation callback。凭证只证明已扣费原件的有界读取资格，不是持续 current 或业务授权；Worker 仍在读取后做独立当前强验与最终 SQL，issuer 签发不能豁免 epoch/freeze/配置变化。

复用 Native guard 的已有实际验证范围仅是 Native SQLite/backup 数据回滚，数据库外的本机 marker 不等于上述独立外部 floor，不能据此声称完整本地盘/包含 marker 的 VM 回滚已覆盖或 qualified。这一受测范围说明不放宽前述正式要求，不为组件编译偷偷增加未批准 RPC/硬件前置；新 Resource receiver、World 独立 floor/once、真实发送者及每条物理边仍须分别采证，生产保持关闭。


凭证随已有那次服务器到 Native HTTP 的认证层传输，固定 header `Oasis7-Custody-Resource-Read` 为无 padding base64url 的完整规范签名 JSON，保留原真实 SID/CSRF、同源/Account 与 after-await session 检查，不接受模型提供的替代 header；header 与完整请求仍须满足入口原有大小边界，超界直接拒绝，不提高 cap 或切块。Native 在任何 World I/O 前本地强验 proof、当前实际 SID/account/namespace、exact 请求/路径/World/selection/Owner 与 approved scope，并把同一 proof 随原 World 请求认证层转交；World 用其本地同 issuer pin 复核 exact World 请求、原实际 participant Admission/Owner 权限、stage/scope 与当前本地 World，然后才执行原固定读取。这里不追加付款、Current、key 或 scope 回调 HTTP。裸管理 SID、旧 V1 空 refs 批准、旧 flag/envbool、普通 ceremony、调用者 verifiedbool 或仅签名正确的新 OwnerControl variant 均不能开启该分支。

Native 与 World 分别在各自可信持久存储先原子占用 `(issuer namespace/epoch, custody account, paid_attempt_id, fixed_substep, native_request_digest/world_request_digest)` 的一次派发槽，fixed_substep 仅 OwnerSetup 的 native_owner_control/world_owner_preflight/world_resource_final 或 TestSession 的 native_owner_resource/world_resource_final，并与该 stage 的固定 units/原件摘要逐项相等，再执行其被批准的固定物理读取；同 attempt 异原件冲突，重复同原件也必须拒绝，不返回成功 cache；合法 caller 一旦持久 claimed 就不得再用同 proof/substep 发第二次实际 HTTP、重新派发或刷新 quote。首份响应丢失即保留 unknown；即使存有精确强签历史响应，本 profile 也没有获准的独立已付费历史读取 API，不能把额外 HTTP 免费包装成幂等成功。上述防重记录是有界读取协议账，不是 canonical World 写；其 CPU/存储成本也必须测量，不能藏成“同一 HTTP 免费”。本切片不提供重发或另分配 attempt 的能力，不能以剩余预算宣称该恢复已闭合。未来若提供成功 cache/历史响应或重试，须先另行设计稳定 operation 与每个实际 HTTP 的独立 transport_attempt/new-prepay proof，并以原预算计每次物理传输；不能复用当前 proof、扩 cap 或绕过 once slot。canonical 四写继续保存完整 signed original 与 authenticated known lookup；非 canonical OwnerResource 当前没有独立 lookup-result API，本段不增加该免费 API。本地已存只读原件可留存为历史证据，但不新增对外成功 cache/读取接口；缺结果保持 unknown/blocked，不能把另一次 Current 读取冒同一事实。

全部 producer/verifier、强签/恢复、实际逐跳 counters、预扣失败零出站、同 attempt 原件冲突/重放、五秒与 Source/角色前序、完整 envelope/子响应界限、CPU/存储成本和最终零 Roots 故障矩阵验收之前，正式 V2 Native/World catalog 继续零出站拒绝。此接线也不自动开放 Execution，不能把 resource scope 当 onceSubmit、原件核对、权威 usage 结算或完整 cleanup24 已完成。

首版工具 envelope 固定 `schema_revision`、`profile_id`、`test_session_id`、`request_id`、`tool` 和该 tool 的唯一 typed arguments；capability 与 PoP 只由可信客户端附于认证层。各 enum variant 均拒绝未知字段，不允许通用 `args: Value` 穿透下游。u64 沿本设计十进制字符串合同；新 HTTP 中 i64 坐标/数量/距离也使用规范有符号十进制字符串并由 Rust checked parse，避免经过 JS Number 损失精度；u32 batches 是正整数并检查上限。账本/资源 ID 沿现有 parser 校验，不能将任意字符串解释为路径或 URL。

客户端在首次发送前持久保存随机、稳定的 `request_id`。托管公共 `operation_id` 使用该相同 ID，在当前 TestSession 和账号范围内寻址；认证层决定范围，不能仅凭 ID 获取他人结果。DO 将 `(account/environment, test_session_id, request_id)` 与首份 typed body digest、origin grant/security version 原子绑定；同 ID 异正文返回冲突，同正文重复返回原 operation。digest 不包含会变化的 PoP challenge/transport attempt，更新传输证明不创建第二个业务操作。client binding/credential generation 是权限检查项而不是幂等 namespace；合法凭据替换仍定位原操作。重新批准的 grant 不能重置旧 session/request 的身份、预算或派发条件。

`context_ref` 是 context 请求的 operation 引用；后续 action/outcome 各有自己的 request ID，并在同一个 DO 事务中关联该 context 及其原件类别。查询 context operation 返回已保存 prepare/prefix 状态和已关联 action/outcome operation refs；查询 action/outcome 返回其确切原件结果。首次 202 丢失后，客户端仍可凭发送前保存的 request ID 使用独立读权限查询，不需要重复创建 POST 或先取得一个只存在于丢失响应中的服务器 ID。HTTP/tool 错误必须保留 operation/context 引用，不让助手通过换 ID 消除未知状态。

新接口固定可机器处理的错误类别：`invalid_request/unsupported_profile`（改正输入，未受理）、`permission_denied`（停止，不重试授权）、`idempotency_conflict`（保留原件）、`budget_exhausted`（停止新调用）、`stale_fence/context_expired`（先核对原件；不得改绑旧 payload）、`upstream_unavailable`（按已记录状态作有界原件核对）、`recovery_required/history_unavailable`（停止新效果并保留现场）。202、received、unknown 和 canonical pending 都不是成功；不存在的本地原件不是“下游没有执行”的证明。世界明确的业务 rejection/next_step 作为原结果保留，不扁平化为可随意重试的 HTTP 500。输出不回显原始上游错误正文、cookie、证明或私有存储路径。

普通 task/capabilities/industry/quote/context/action/outcome 工具受其当前执行/读取范围和工具请求预算控制；查询不是免费绕过总调用上限。写 capability 到期、撤销或测试 deadline 后，只允许既有独立读凭据在其更窄范围/预算/期限内查询原结果；不能创建 context、生成 outcome、重新报价或以 GET 触发原件重新提交。清理中的 Stop/Revoke 归预先获准的 owner 控制通道。

`describe-session`、`finish-session`、`abort-session` 是同一工具宿主的有限会话工具，分别映射非秘密 session 状态与 §12.18 的关闭入口，不属于 executor 世界动作。首版业务工具集合不随 MCP tools/list 或既有 native CLI 新增命令自动扩大。

响应上限按实际层取小值：native gateway 已有 4 MiB 原响应上限；新 custody HTTP 含封装至多 4 MiB+64 KiB，客户端及模型消费预算可更小。目标/context 必须完整传递并标明完整性，超限返回 `context_too_large` 且不准入依赖动作，不能静默截断。工业集合允许保留 count/truncated 的有界投影。只读恢复结果只含授权的原回执/状态/关联，不重新暴露过期私有 context 或把原签名写 bytes 返回模型。

**首个单 Move 子集的私密 context 存储合同：** 在完整工具 profile 尚未接通期间，首个候选进一步收窄为每 TestSession 至多一个 context 创建 operation，保留 `Prepared` 与 `Captured` 两个不可覆盖私密正文槽。每槽以仓库 canonical CBOR codec 保存完整 SDK typed artifact 的确定序列化 bytes，至多 64 KiB，解码后重编码必须等同原 bytes；不得分块、裁剪目标/证明或改成摘要后声称完整。上限在派生写原件之前检查，超限返回 `context_too_large`、保留原 operation/已付账与已知前置，并拒绝依赖的 Prefix/Cognition。这个服务端收窄不改变 Native 4 MiB wire 或 SDK 自身合同，不构成其余工具或下一 context 已实现；不能绕单槽上限换 request ID 清除原件。以后扩大容量/数量须另行设计、核对 Account 和真实调用成本后交付，不由客户端或模型调整。

正文使用独立的 Identity `ContextBody` 持久 actor/namespace，不能借私密 SID actor 或其 32 KiB wire 隐藏正文。专用完整记录 wire 上限为 256 KiB，须以最大合法 scope、64 KiB 正文、完整 envelope/AAD 的真实编码验证，超限在出站前拒绝；不能仅比较明文长度。复用现有 EnvelopeCrypto/独立随机 DEK/Zeroizing，但使用单独预配置的 context-body KEK 与包装版本及独立 AAD 域，不使用 Native session/SID、Owner/Executor seed 或客户端可选 KEK。固定配置为 `CUSTODY_CONTEXT_BODY_ENABLED=true`、独立 secret `CUSTODY_CONTEXT_BODY_KEK`（32 字节小写 hex）和非零十进制 `CUSTODY_CONTEXT_BODY_WRAPPING_KEY_VERSION`，并核对实际 `CUSTODY_ROLE_KEY_ENVIRONMENT`。未配置或未启用时 fail closed，不在首个请求自动创建生产 KEK。默认 HTTP 不提供该 actor 的正文接口，固定 binding 只由真实 Identity 组件调用；不能信任请求中的 role/verified、actor 地址或任意存储路径。

Account 只保存两个有界、非公开的 `ContextRef` 及其原阶段/账/派发状态，沿用整体 2 MiB 持久上限，不能把正文或无界历史塞入引用。公开 `context_ref` 仍是发送前已有的 context operation ID；私有 actor 地址、正文/密文摘要、AAD、nonce/proof 不进入模型或日志。actor scope 由实际 Account/环境/World pin/TestSession/client binding/原 context operation/固定槽及版本派生；在同一 Account 条件事务中将原阶段原件、完整正文长度/摘要、对应已付 transport 和一次写 claim 绑定，不能接受调用者自报 hash/ref 作为权威。Prepared 只能来自 SDK 对原 Prepare 的真实验证；Captured 还须 exact Prefix committed、原 ReadContext 与完整 capture/read commit 验证。返回的 actor 内容必须按完整 scope/AAD、原阶段与 SDK 纯验证重核，不以 actor 回显、哈希相等或存储成功代签 World/Native 事实。

`ContextRef` 的闭合证据至少保留原 `SignedReadRequest<TaskContextPrepareV1>`、固定 `NativeExecutorV1`、固定 World service key/pin 与 `TaskContextIdentityV1`，以及真实 `SignedServiceResponse<PreparedTaskContextV1>` 的 request digest/服务签名验证材料；Captured 还关联原 `SignedReadRequest<TaskContextGetV1>`、不可替换的 signed Prefix/correlation、选中强签 committed canonical fact/capture commit 和对应完整私有响应验证材料。正文在私密槽内，不重复存入 Account。Identity 从实际原请求/固定响应构造不可反序列化、不可由调用者布尔生成的短命 SDK 验证对象；Account 最终 SQL 核对精确原件/阶段/费用绑定。ref-only strict restore 只核可独立验证的原件、结构、签名材料、两槽关联和账，不声称凭摘要重验了缺失正文的完整 World 响应签名，不恢复 usable token。每次 live 使用仍须固定读取原正文，重建完整响应并执行 `verify_private_reply` 与原 Prepare/Get 的 `validate_against`，再独立核 fresh Source/Root/DFRF/当前 World 和原期限；不能反序列化历史 `verified/usable` 标记来签 Prefix/Cognition。

**依赖动作之前的新鲜读取：** 已存 Captured 及原 Get 的服务签名证明历史结果，不证明本次读取。新增闭合 `TaskContextLiveReadV1 { original, observation_nonce }`：`original` 是原有 `SignedReadRequest<TaskContextGetV1>`，`observation_nonce` 是本次读取的 32 字节小写 hex 挑战。原 Get 的签名字节、权限、目标与 lease 不变。专用域/路径为 `TASK_CONTEXT_LIVE_READ_PATH = /v1/world/private-task/context-read-live`，Native 固定操作为 `ReadLiveContext`；World 对完整 wrapper 的新域 request digest 和完整 Captured payload 签名，SDK 必须同时核新域、精确 wrapper/nonce、原 Get 语义与现有 Prepared/Prefix/capture/read commit。不能将旧 `context-read` 回包、nonce A 回包或同 nonce 的异 Get 回包作为此次挑战的成功结果。

World 新路径必须实际重新读取当前 task/root/prefix 与 `current_public_context`，复用现有 `load_captured`、初始及交付前 live/权限/lease/fence 核对和 delivery drift 拒绝；不能仅给旧 Captured 缓存换 nonce 后重签。Native 继续执行实际 SID/CSRF、账号及资源额度/audit、固定 World pin/目标、当前 Active executor/fence、原 Get signer 与 gateway actor 一致、初始和交付前 session 核对、4 MiB 响应上限。新挑战不是授权，也不替代原有 gateway access 签名或当前准入。旧完整 Native wrapper 的签名或 HTTP 刚返回均不能单独证明本次新鲜度；任何消去签名/端口的优化须另行证明等价，不能以这项协议默认免除费用。

托管调用的挑战由服务生成，须在出站前与原 context/Get、独立服务 attempt ID、用途及已付端口一次绑定；不接受模型提交替代挑战或自报 verified。完整新响应只有在 SDK 强验和 await 后独立 fresh Source/Root/DFRF/当前 World/原期限核对通过后，才能作为该次 Cognition 的短命输入；不覆盖两个 ContextRef 私密槽，不续期原 Get/context，不因历史核对成功恢复 usable。晚到或丢回复按原 attempt 归档已验证历史事实，不隐式重复 Native/World 读取；新尝试必须独立付费并登记新挑战。Native、World access/read、SID 四次私密读取、协调/Directory/Fence/signer、Source/LF 和恢复均按实际物理端口另计，不能把该 nonce 当作免费池或预设完整图已小于 400/300。协议与强签 fixture 验证不代签真实托管 caller、已付 Current、权威 usage、完整预算或真实 Agent 验收；未闭合部分继续保持业务执行关闭。

每个新 ContextBody 端口须在既有 purpose transport 账项内持久带闭合 Context 类别，绑定原 context operation、slot/port、purpose、source、首次 claim 与精确 scope/reference 承诺，并与两槽/attempt 双向核对。旧通用一费记录保持原字符串 wire 与原恢复语义，不能推定为 Context 费用；新类型对象缺字段/类别不能回退成旧字符串。即使请求账与 transport 数学平衡，Context 缺失而其 typed 费用仍在也必须拒绝恢复；不能只另加一个可同时删空的可选 link map，再把留下的 Context 费用当普通传输，丢失单 context/once/unknown 历史。

两个槽各仅允许首次不可变写；无 KEK 的 opaque actor 仅比较完整原 scope/envelope bytes，精确重复幂等、不同 envelope 冲突，不依据调用者自报明文摘要比较“相同正文”。Identity seal 后、出站前在 Account 同 SQL 保存首份候选 envelope 的实际摘要/wire 长度与一次写 claim，不保正文或完整 envelope。写已提交而回复丢失时保留原 pending/write claim，只以显式付费的原槽核对恢复；在合法 live 条件下由 Identity 解密、对原 SDK artifact 重新验证并复用原 envelope，不重加密替换、不重新写、不重新 Prepare/GetContext 或签新 Prefix。首次写/核对与 Account 引用确认可能分属事务，actor 有记录而 Account 尚未确认时必须显示 pending；原槽不存在也不自动证明 Native 未执行或释放 context。未知或未关联正文不能被删除来伪造未派发。

actor 另有固定 fact-only metadata 读取，只返回预期 scope/version、已存在与首份 envelope 的摘要/长度等有界存储事实，不返回 ciphertext/body、不加载 KEK/解密、不产生 usable context；每次实际恢复读取另列付费，不藏入正常六端口。到期/冻结后只能在仍合法的独立核对授权下记录该事实，不能重新暴露过期私有 context、清 unknown 或复活依赖写；缺合法核对授权时保留引用与未知证据。

正常 Write/ReadBefore/ReadAfter 每槽分别 once；显式恢复使用新的服务 attempt ID，逐次付费、逐次 once，不隐式重发。RecoverBody 仍要求未到原 write_deadline 的完整 Execution 来源与当前检查。RecoverMetadata 只查询已存在的该 session 原 ContextRef，使用初批独立 ReconcileRead root、其 read_until/当前凭据/来源/Root lifetime/DFRF 与 Read transport，不取得正文或进入普通工具；这是原事实读取，不把正常执行费用转给 Read。恢复记录沿既有 Execution400/Read300 及 Account 总字节上限有界，不新增十次免费池或未声明的每类一次恢复限制；已合法派发的晚到存储事实至多归档，不恢复权限、阶段或新派发。

正常单 Move 的两个槽实际增加两次写（Prepared、Captured）和四次读（每次构造 Prefix/Cognition 前各自解密前后读取同一不可变记录），共六个 `ContextBody` 出站端口。必须在对应阶段有界图中先持久预扣 Execution transport，再 once claim 出站；失败/unknown 不退款，不借 Control/Native/Read 或隐含免费批处理。依赖的 Native/Root、Directory/Fence、Owner/Executor signer、Account named/DO 往返及恢复尝试仍逐 port 另计。若引用确认不能在已有阶段最终 SQL 真实合并，其新增协调 RPC 也必须按完整物理图计费；不能将两个本地 SQL 冒作两次 RPC，也不能隐藏实际新增 RPC。六端口只是存储增量，不证明完整 Execution400/Read300 图已闭合；完整真实图、强验和故障验收完成前，业务 Actions 继续关闭。

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
Executor Prepare V2 固定持久 signer（CT-004/007）：原 RoleKeySlot schema1 的字节与 32 KiB 上限保持。实际部署仍使用原固定 `EXECUTOR_PROVISION_KEYS` service binding，再由 Executor Worker 进入 `ROLE_KEYS` Actor；InitializeSigner 与 SignPrepare 因而是四条独立物理边（每 operation 的 Account→Executor service RPC、Executor service→Actor fetch），不得把两 operation 标签说成两 HTTP、复用旧 Provision 账或不存在的 direct namespace binding。Account 同一成功 SQL 必须额外从 Execution400 预扣四边、保存各自固定 operation/hop/proof digest 的闭合 typed leaf 及 ordinal，然后仅首次返回不可 Serde/Clone 的两个 operation dispatch token；每 token 携其 service/Actor 两叶。精确历史只能读 proof，不再构造 token、重新调用 initialization 或免费查询。任何 service/Actor/解析/签名/回复失败都保留四叶与费用；未发边也不退款，不能换 nonce 重试或借 Control/Native160。四叶只是固定 Prepare 签名子图，不代已有 factory1、Current3、后续 World/Native/Identity 及完整400图。

固定四边首次成功预付恰耗尽 Execution transport 时，只允许原 `PermitsRecorded` ordinal、完整 proof 与四叶对应的不可重建 dispatch 继续原 init/sign；这不是一般 Closing 豁免。首次记录保存并冷恢复反核该唯一 ordinal；history 不返回 dispatch。Account 在 init 前及 init await 返回后、sign 发出前，必须在同一 AccountDO 本地重新进入，核实际现存状态、原 Source/LF 与私有 Current3 观察的全部短窗/最窄 until。只有原四边事件自身导致的 BudgetExhausted 可继续；后来实际 Finish/Abort 的 causal journal 即使与预算事件同毫秒、closing 首因仍为 BudgetExhausted，也阻断尚未发出的 sign。Deadline、lease、freeze、epoch、root revoke 或其它 Closing 同样阻断；不增加 self-fetch、免费 callback、新 Source 或重新签名。只剩三费时整次四边预付必须保持 state 不变，已经发出的异步结果仅可成为原件历史，不能替代后续 fresh admit。

RKV sign 已发出的完整公开 SDK 原件返回后，Account 在同DO本地 SQL 强验 expected bundle、实际 Executor 公钥和全部原签名，保存完整有界 canonical 原件及唯一归档事件；不增加物理 RPC 或费用、不刷新原签名/Source时钟。失败或回复 unknown 不创建原件，原四边 once 与占用仍保留。实际原件即使迟于 freeze、Closing 或 deadline 到达，也仅可归入 immutable history；原件、事件和首次记录时间冷恢复须双向完整，篡改签名、删一边或重排原因事件拒绝。历史 getter、精确重放及反序列化均不得产生 dispatch/Current token；将原件发送 Native/World 仍须独立完整物理预付、一次提交和新的 admission，不因本地归档完成而开放。

实际 mounted Prepare caller 使用新闭合 `owner_service_and_account_v2` factory profile：Identity→固定 Owner named method、Owner→同一实际 Account Actor 是两条物理边，第一成功 Account SQL 必须固定结算两叶并永久绑定同一可信 ingress、原 RFC 尝试、action/session/input/Source 与 allocation；不得接受 caller units、自由 route 或目的 bool，不初始化未知 Account，未认证原件不扣合法 session。这个结算如实承认两条入站路线已经发生，不称事前已付；认证/余额不足时拒绝后续出站，不伪称整请求0 I/O。旧 `identity_to_account_prepare` 一费 record、构造入口、账式和历史字节只保原 component/history 语义，不改成两费、不借作新 profile 的证明。新 producer 只在此新 profile 同事务保存固定 service/Account 两个 typed leaves 后返回新一次句柄；冷恢复双向反算两叶、两费、原件与唯一事件。缺任一叶、改为 Generic/旧一费类、orphan、同或跨目的 ID/nonce 碰撞均拒；不因旧一费存在自动升级，也不把旧一费再加两费而重复收三费。Current3、RKV4及先前真实 Source/LF 已付记录须按原件精确引用，不再重复扣。

其后的新 `native_execution_prepare_v2` profile 只接受上述实际两费 factory 原件、真实执行凭据与已强验 Book/Source/LF/private Current3、完整固定 SDK/RKV 原件；public wire、反序列化 book、metadata 或 caller verified/ready 都不产生权限。此新 profile 的现有部署 binding 图固定十条独立物理边：Account→IdentityTestSessions service、service→NativeSession manifest Actor、manifest Actor→SID cipher Actor before/after 两次、manifest Actor→CSRF cipher Actor before/after 两次、IdentityActor→Native、Native→World current VIEW、Native→World attested Prepare、IdentityActor→ContextBody Actor immutable PUT。manifest 才持有真实 SecretRef record/version；SID/CSRF 密文分属两个既有 Actor，完整 read/open/read 精确比较保留，不迁移记录、合并 Actor 或省掉 after-read 来压计数。Identity dedicated KEK 与 WebCrypto 本地执行，不另造 Crypto RPC；缺任一既有记录/版本立即拒绝，不偷偷补 Capabilities、登录或重新生成 SID/CSRF。最终只沿原 Account await 回传公开 strong projection/ref，Account 的本地 archive SQL 不增加 callback。故本 Prepare 已知局部至少是 factory2+Current3+RKV4+Native10=19；public HTTP/abuse limiters、原 Source/LF、其它回调/动作与实际额外查询仍独立列账，19不是完整400图。

Prepare 先完成已付 Current3，取得真实配置 Native Root pin 与 Directory/RF 观察，再取得私密 session Current；后者使用独立 `execution_private_current_v2` 已付前缀，不能从 Book 保存的 LF、原 `private_sid_generation` 或反序列化 Current 补造。Account 父 handler 在同 SQL 先从 Execution400 永久扣两费并保存两个不同 closed leaf/once：Account→既有 `TEST_SESSION_PRIVATE_HISTORY` binding 的固定新 `attestExecutionV2` Identity service method，及 service→既有 NativeSession manifest Actor。随后只调用这个固定真实 callee：它核外部配置 Native Root 的完整 LF 强签、实际 completed manifest/history、session_ref 与 Native binding generation、当前 private SID generation 和完整 scope，并返回公开 session_ref/private_generation/LF digest；不解密、不取 key、不出 Native/World HTTP。Account 验返回原件与已保存的本次 session/LF/Source、paid prefix、operation/input/factory/nonce 精确对应，才建立不可 Clone/Serde 的私观察句柄，并只在原父 handler 内持有至后续 RKV4、Native10 reentry；Current3 的原观察时间及 Source 窗口不重置。跨 await 只保留原句柄及原观察时间/窗口，不重新用保存的 scalar 或历史响应构造句柄，也不发免费 callback。

新执行用途不能把普通只读 `attest` 的历史响应升级为已付证据。Account 的唯一 issuer 仅从真实 Core first-commit 私观察 token 产生独立 domain/purpose 的 `SignedAccountExecutionPrivateCurrentV2`：完整绑定 compiled Identity recipient/固定 Actor route、真实 session 与 Identity scope、批准 input/action/stage、factory 原件、Source、Native LF 原件及 digest、原 ordinal/两个 distinct physical leaf/原 until。Identity 从部署配置取固定 Account issuer pin/floor，不从 packet 取 key；接收 Actor 独立版本化 once 先提交，再运行原真实 manifest/current-generation/LF metadata kernel，失败不释放 claim。原 `attest` DTO/路由/只读字节与原 Native manifest schema1/32KiB 不变；新 once 使用独立有界 SQL rows，不往旧槽位塞新业务状态或放宽容量。RPC 回包只含 exact proof digest 与公开 session_ref/private_generation/LF digest，不含 SID、CSRF、key、私密 context body 或 grant/floor。Owner 只从这个实际固定调用的回包构造同 handler 私观察，不能把调用者 JSON 当结果，也不从 cold history 恢复句柄。

该私观察两费与 Current3 的 Directory1+RF2、Native10 内再次实际发生的 service/manifest/cipher 叶互不替代；同一个物理请求只扣一次，两个不同 RPC 不能合记为一费。其新记录、ordinal、两个 leaf 及强 LF/完整 request digest 必须双向恢复核对；first commit 后才可返回唯一出站句柄，错误/unknown 不退款、不重派，历史查询不恢复句柄。源 observed/expires、LF 实际较窄 status/natural until、原 lease/批准 deadline 与各 issuer/dispatch until 均保持原交集，不刷新 Source 或延长五秒。最后恰好预付耗尽只允许原 accepted ordinal 下尚未发出的既付 exact edges；Finish/Abort、Deadline、Lease、freeze、epoch/config 变化仍拒未发边。按这个具体 caller，已知局部为原19加独立私观察2，即至少21；最初 Source/LF 的其它实际物理边、public ingress/限流及其它 stage 继续另列，21不是完整400图，不据此开放公网 Actions/Tool/N floor。

Native10 分布式逐边派发（CT-004/007）：上面的十叶预付仅建立 reservation，不代表十次 send 已开始。每个业务物理叶须分别经历 Reserved→DispatchClaimed，AccountDO 的实际成功 SQL 是该叶与 Finish/Abort 的线性化点。SQL 核原 accepted ordinal、完整 proof/leaf/body digest、实际 session/input/Source、原窗口交集和当时 Account 中的 causal Closing/root/freeze/epoch/config；只有该 reservation 自身造成的 BudgetExhausted 可以继续。任何后来 Finish/Abort（包括同毫秒）、Deadline、Lease 或其它收紧先于 claim 时，未 claim 叶必须拒绝。不能一次 claim 整个子树再把尚未发出的叶称为已发。成功 claim 后恰好该叶可以在原 until 内首次发送；与 send 并发但发生在 claim 之后的收紧不能取消已经线性化的该叶，其迟到结果只能进入历史，不能让下一个叶自动可发。发送前仍核原绝对 until；过期、发送失败或 unknown 都永久保留费用及 claim，不重新取得 token、不重发、不退款。

此 Account gate 是明确计费的远程回入，不通过 Rust closure、保存的 bool 或免费 Current callback 跨 Role 同步。Account→Identity service 第一叶可在原父 AccountDO handler 中本地 claim；它不产生新增 HTTP。Identity service/manifest Actor 的其余七个业务叶（service→manifest、SID before/after、CSRF before/after、Actor→Native、Actor→Context PUT），每叶发送前沿固定 Owner named service→同 AccountActor 两跳回入，各有独立两费、两个 physical attempt 与 body 绑定。不得把 service 层和 Actor 层合计一费；七次回入增加十四叶。

Native 的两个 World 叶不能借 Cloudflare binding、Cookie、H PoP 或 cleanup/world3 票回入 Account。新增独立 closed NativeRoot dispatch-claim public route，只接真实 NativeRoot 对原 Exec proof/业务 leaf/body/原 until/独立 callback attempt 的强签；部署配置固定此 Identity HTTPS origin 与原外部 NativeRoot pin，不能用 packet key、Native audience origin 或 caller URL 代替。此 route 沿正式入口保留 provider/realm 两次 abuse Actor 限流，再进入固定 Owner service→AccountActor。因此每次 Native 回入实际五叶：Native→Identity HTTPS、两个 abuse Actor fetch、Identity→Owner service、Owner→AccountActor；两个 World 叶增加十叶。每次调用的固定五叶都预先列入同 Execution400 图、逐票 once；不省 limiter、不借 Native160/Control、不增加 cap。未知结果不重新查 claim；若以后添加事实 lookup，必须先另列其真实付费叶和原件，不复用本图免费查询。

| Native10 业务叶 | 派发前 Account gate 的真实额外物理叶 |
| --- | --- |
| Account→Identity service | 同父 AccountDO 本地 SQL，0 |
| service→manifest、SID before/after、CSRF before/after、Actor→Native、Actor→Context PUT | 每叶 Owner service+AccountActor 2，共14 |
| Native→World current、Native→World attested Prepare | 每叶 Identity HTTPS+abuse Provider Actor+abuse Realm Actor+Owner service+AccountActor 5，共10 |

故逐边 gate 新增24，已知 Prepare 局部21变为45；原 Source/LF、最初 public ingress/limiter、其它 stage 与显式 lookup 仍另列，45不是完整400图或性能资格。先完整已知图预付后才可出站；不足、未知图或 missing receiver/fixed pin 时后续出站0。不能把 callback 自身又当任意业务叶递归申请 gate：它只消费事先固定的观测回入票，请求 Account 收紧/claim 唯一业务叶，不拥有 World/私密读取权限。回入票一次消费后 unknown 仍占用，不能重构业务 dispatch。

Account 的回入只核它当下实际本地 authority/causal journal 和原短窗，不能从串行化字段恢复原父 handler 的私有 Current3/私观察句柄，也不把本地 gate 说成新 Directory/RF/Native Current。原父 handler 持有的私观察、Source/LF/C3 原 until 由独立 typed grant 完整承诺，接收端固定配置强验，时间绝不刷新；Native 仍运行自己的 managed/human/config protected Current 并在每个 World 发送前后核对。若某 stage 合同需要新的外部 Directory/RF/Native 事实，须增加真实已付观测叶，不能由此回入替代。每个业务 dispatch claim 的强签 receipt 绑定原 reservation ordinal、唯一业务 leaf、callback attempts、Account 实际 claim ordinal 与原 until；Identity/Native 的本地 durable physical once 只能消费这一叶，missing/changed/duplicate receipt 在发送前拒。Account claim 之后才发生的异步失败可归档历史，不能恢复新的 claim 句柄或后续 Current。旧 Native10 proof/receiver 没有这套逐边 receipt 时保持 unsupported，不把旧预付 reservation 升级成 dispatched。

#### Execution 专用 pending-start 与同一 Execution400 总账（CT-004/007）

##### 已证启动环

正式 execute_prepare_context_v2 调用 factory charge；factory charge 要求 execution_book Some。Worker 没有调用 claim/archive/admit_test_session_v2_execution_lifetime 的生产入口，而正式 browser activation 明确产生 Book None。因此不能用 fixture seed Book 或旧 credential Control Source/LF 路径绕环。

##### 唯一账与首次接受

新增闭合 ExecutionPendingStartV2，位于现有 SessionV2 内部，不能反序列化成 dispatch 权限。首个正式 Host→Identity 入口仍执行现有限流：Host1、Provider/Realm abuse Actors2、OwnerService1、AccountActor1，合计5。前3没有通过真实 PoP 认证时只记录入口事实，不从任意 session 扣费；认证成功后，Account SQL1 从既有 Execution root.budgets.transport 独立永久扣已发生的首5，保存实际 H attempt 与四个内部叶；该提交先于 RNG、后续预付或任何 Source/LF 观察。Account SQL2 再强验同原件、原 nonce/generation/期限和当前本地状态，才从同一 root 预付余下134并建立 accepted pending/逐叶 once，合计139。余款不足或 SQL2 失败不能回滚 SQL1；Source/LF 后续出站为0，失败全状态等式以 SQL1 已提交快照为基准，不把真实首5退掉。purpose_http_admissions 与新 pending record 只索引同一实际扣账，不创建第二个400预算。失败不退款。

旧 GenericOne、旧 Factory2、旧 Book 历史数学不改。新 start 使用显式独立 closed kind，保存原 H RFC attempt、完整 action/input/session、原 ToolMetadata challenge、每叶ID、接受ordinal及绝对until。pending 的固定 until 还须不晚于实际 H PoP header 签名截止、未消费 challenge 的原 expires_at、accepted_at+5000、原批准/lease/Execution deadline；后续任何新观察只能收紧不能延后它。当前实际 Move0 先支持；其余三动作缺真实 ResourceBasis/public refs 时 Unsupported，不能空 refs 扩权。新 start 的首次 OwnerService/AccountActor2 替代该新调用原 factory2 两叶，不是旧 factory2 再扣一次。后续 factory 从精确 pending 已付两叶派生，只能同一实际 handler 首返 opaque；历史无重建 token。

本专用启动还须先补正式 Worker ToolMetadata nonce issuer：同固定 scoped ProofChallenges path、原 closed request/RFC target 与实际 V2 map 分流，在真实 Host/Provider-abuse/Realm-abuse/OwnerService/AccountActor 五边发生后独立结算5。新增明确 closed 五叶 admission class/once/reverse，旧 ToolNonceThree 和旧数学/字节不变；不能仅把旧 charge(3) 改5，不能跳 limiter。原 challenge 只承诺 session/purpose/logical request/操作 digest/当前凭据 generation，不授 Tool/Book/World 权；其实际30秒截止不重置。nonce五边与后续139独立，连续新请求已知局部为5+139=144，实际Actor counter与 Root400同账要同时证明。 新 nonce 请求5与 start 的首5是两次真实不同 HTTP，各须收费；start139自己的首5和剩134则不得再另外加5。未认证前缀不给任意session扣账；不足5没有新nonce，start首5已付后余款不足134（总额不足139）仍保前费且无Source/LF出站。

##### Source/LF 固定逐物理清单

以下是新增 Execution 专用 caller 的明确固定流程，不声称旧 credential helper 已符合。每个 C3 都是 Account→MANAGER_ASSOCIATIONS Actor1、RF service1、RF Actor1；Identity RF 也是 service1+Actor1。任何具体绑定改变必须重算、不能把 service 调用折成单 Actor。

|阶段|实际边|数量|
|---|---|---:|
|Source inspect 前|Account 当前 C3|3|
|Source inspect|Account→Identity service；Identity→Native verify|2|
|Source 强事实归档前|Account 当前 C3|3|
|LF入口|Account→Identity service；Identity→NativeSession manifest Actor|2|
|PreDecrypt/PostDecrypt/ClaimLF 三次|每次 Identity→Owner service→Account Actor2，Account C3，Identity RF2|21|
|解密|SID manifest/blob、CSRF manifest/blob，实际四 Actor fetch|4|
|LF|Identity→Native SessionLifetime|1|
|完成|Identity→Owner service→Account Actor2；Account→Identity attestation service→manifest Actor2；Account C3|7|
|合计||43|

三次 Source oracle read 对应真实三次观察，不手填 read count。Complete 的私有 metadata attestation 为独立实际读取，不能把旧私有 generation 或历史 LF 当当前。Source nonce 保留已核 Native 强签绑定 input/session/sourceID/ingress/actual RFC attempt/action/独立salt；Native LF nonce 分域，不复用 token。Native verify 使用现 Identity-only broker 配置，Account 不加载 Identity secret。

##### 未发远端叶的 F/A 回入

Account 自己出站的叶由该 DO 本地 SQL 在出站前逐叶 claim；Account 的 Directory/RF C3 同样逐叶本地 claim。已经到达 Account 的 Observe/Complete 是固定控制回入，SQL 在完整原件核验后才返回下一叶的资格，不为该回入递归生成回入。

远端内部新增20个具体业务叶各必须额外 Account service+Actor2 回入：Native verify1；LF manifest Actor1；SID/CSRF四读取4；三次 Identity RF 的 service/Actor共6；Native LF1；Complete attestation manifest Actor1；上述六次 Account C3 各自的 RF service 内部 Actor child6。共40额外物理边。Account 能在本地 SQL claim 自己直连的 RF service，却不能因此预先 claim 远端 service 尚未发送的 Actor；六个 child 必须真实逐次回入，不能藏进原 C3 三费。这里 RF service/Actor 是两个独立授权/持久 once 消费点，不能一次批 claim 后把未发送 Actor 当已发送；须新增 Execution 专用固定 profile 的 RF receiver，旧普通 RF 请求不接受新 dispatch 权限。此处必须实现实际 service/Actor counter，而非把现普通 readRecoveryFence 方法参数添 caller bool。

此外，原 Prepare prefix 的 Current3 另有 RF Actor child1、private2 另有 manifest Actor child1、RKV init/sign 各有 ROLE_KEYS Actor child1；新专用 handler 对这四个未发业务 child 也各加真实 Account service+Actor2，共8，不能拿原两费/四费 component 证明已逐叶 claim。旧 profile 与旧记录保持原数学，不自动升级为本新 profile。

每个 Account 回入强核原 start/full body tuple、预付叶ID、causal closing、当前冻结/epoch、绝对until后永久 claim 唯一叶并签固定 purpose/domain receipt。Identity/Native receiver 持久消费对应 receipt；业务失败/回复 unknown 不释放。Native Root 不离开锁；Native 每 World 仍真实当前→短锁签 query→锁外回入→短锁强验/当前→锁外 World。

##### 控制回入与实际 Current 的权威边界

AccountGate 仅是已命名未发业务叶与 Account Finish/Abort 的 SQL 线性化点；它核本地 causal journal/root/lease/closing 及原已封存完整 tuple/绝对 until，不自称新的 Native、Directory、RF 或 global-config 观察。回入自身 service/AccountActor 是固定控制传输，只能把那个具体业务 leaf 从 Reserved 变为 DispatchClaimed；不授另一个 leaf、World 写或 Current。它不调用 C3/RF/Native，故没有递归生成额外 gate；不能用这个有限性免掉任何业务 child 的独立回入。

|业务接收者|必须另核的实际权威与时序|
|---|---|
|RF service / FENCES Actor|固定实际部署配置/命名，收到对应强签 grant；Actor 自己先 durable once，再读自身真实 RF row；不能由 service 代 claim。父 handler 随后用该真实返回值重做完整 Directory/Source/Native tuple 校验；gate 不提供 RF 值。|
|Directory Actor|本 DO 实际 manager session/epoch/自然期限；本地 Account 逐叶 claim 后才请求；返回只作本次原观察，不因 gate 更新时间。|
|Identity NativeSession Actor|真实 saved record/version、当前 Cloud private generation、原 completed history/固定 Native pin；SID/CSRF 只在该角色解密，before/after actual read 与 generation checks 均保留。|
|Native verify / lifetime / World work|实际 guarded provenance、protected human/managed SID、Native generation/revoke/freeze/config/floor；每次业务 IO 前后自身 Current，Native Root key 留原锁内。|
|RKV service / ROLE_KEYS Actor|fixed issuer/recipient/role scope、真实 slot/HWM/once、原 sealed Current grant 和全部原 until；service 不能重建/续期私观察，Actor 前仍需独立回入。|
|Account 原父 handler|全程持有实际私有观察 opaque，后续强核 Source/LF/C3/私观察所有原期限；新 gate、冷恢复和历史响应不能补造 opaque。|

每个业务 receiver 的强核失败仍永久保费/once，返回仅错误或历史；没有新 Current/下一叶 token。F/A/revoked/freeze/generation 反例须分别在 gate 前、gate 后但业务 sender 当前核前、以及业务 IO 后执行，区分未 claim 拒绝、已 claim 可完成历史、后续未 claim 必拒三种实际结果。

##### 图的边界与总数

原45是 factory2+C3(3)+private2+RKV4+Native10+callback24，原图从未包含新 Source/LF，也未包含 Host/abuse3。新首5代原 factory2，Source/LF43+其逐叶回入40 为新增83，原 prefix child 的新回入另8。故本专用启动图已知局部总数为45−2+5+43+40+8=139；不是完整400图，ToolMetadata challenge 自身正式首次请求及历史/unknown lookup 没有藏入139。任何额外 lookup 都须先列具体实际叶并预付，不沿用已消费139免费查询。已付 prefix 精确引用原 receipt，不重复收费。139只有真实逐叶 receiver 与 counter 全闭合后才成为验收结果。

##### 接受与时限

pending SQL 先验证实际原 ToolMetadata challenge.expires_at 与真实 raw H PoP（现 metadata producer 为 min(created_at+30000,root.deadline)，不能混用 credential challenge 的期限）；不得以开始为理由延长未消费 nonce。后续 Book 创建只能消费同 pending 首次 opaque 与原挑战，原子消费一次；若过期，保费用/unknown，无 Book/grant。原 Current/Source/LF 更窄窗全部取交集，不能取后来的 observed_at 刷新首个 accepted until。预算刚好耗尽只允许该 exact accepted ordinal 的已付原图继续，所有 Finish/Abort/Deadline/Lease/freeze/epoch 改变阻断未 claim 叶；已 claim 只可完成历史，不能获得新鲜 grant。

##### 恢复与验收

mandatory pending collection、显式 closed event kinds 与所有叶反向库存；首5 fee 与134 graph reservation 为两个独立成功 ordinal/原件，图必须完整反查前者；新模式的既付 child records 只引用唯一 master paid leaf，不再独立 charge同一叶。静态账与 journal 都按精确物理 leaf 集合去重/强反向，不用默认0费用、caller verified、另一份400或 Book 出生重新初始化来凑等式；legacy None 仍保原收费数学。每条 paid admission→pending/Book 精确双向关系；原预算400重放包含 Book 前费用、不能在 Book 出生时清零或另建账。Source/Nonce/raw leaf IDs 全命名空间互斥、原件哈希反算；缺/孤儿/重复/降级/换 signed attempt 拒绝。冷恢复只验证历史，不能构造首次 opaque。

必须 genuine normal-clock activation Book None→真实 Exec PoP/nonce→同 root400 扣账→实际 Source/LF→Book Some→Current3/private2/RKV4/archive/Native45；counter 逐叶，不把 Core 强签 fixture 冒 HTTP。另测 start 入站前剩139可完成预付、只剩138时首5已提交而134预付拒绝/余133且出站0；nonce5为另一个请求，另保各自费不足场景。再测每叶 F/A claim 前后顺序、冷恢复、unknown 无重发、实际 TLS固定Origin/根pin、>32KiB request/reply 分开。实现尚未资格化；必须保留 pending-start/Source/LF/逐叶回入各 actual counter 与失败证据，不能把本设计数字当作139已经实测通过。




十叶在首次出站前由独立 Execution400 同 SQL 预扣并保存；不得借 Control240、Control-native160、Provision、World3/TermCurrent 票或旧 factory1，也不以 Source5 once ordinal 当付款。独立 issuer purpose/domain 的证明只从真实 Core opaque 与私有 Current 交集产生，强绑完整 public SDK 原件、原 source/input/session/action/stage、实际 Native binding/session/guarded Ready identity、固定收件端和原 absolute until。原 SID/CSRF及任何 private body 都不进入 Account/Executor/H；机器注册只作已实际 Ready 的 protected Current lineage/SID 定位，不把 Cleanup/World3/Active metadata 当 Execution 或 World 权。旧未 guard FullMFA、普通 cookie、旧库或旧 proof 不自动升级；Native 自己仍核真实 protected row、当前 pin/floor/config、managed SID 与 human-origin current，再凭完整原 executor 签名和 World current/fence/规则执行，不能信 packet 自带 key/bool。旧 Prepare/View signed preimage、旧 Native DTO、RoleKeySlot schema1/32KiB保持不变；新 Native 请求完整 wrapper 仍受64KiB边界，超限拒绝，不裁证据或抬旧 cap。

Identity service 固定 pin 核新证明后定位现有 manifest Actor；Actor 在任何 cipher/Native/PUT前持久消费各自 exact paid physical attempt，失败/unknown 不释放、不免费重读。Native 对其实际 Native/View/Prepare 三叶在独立 guarded SQL 先 commit once 后才做 fallible Current/HTTP；新目的不得挪用原 Preflight receipt/domain。World HTTP在全局 Native mutex外执行，前后回锁重核完整实际 security/config/naturalUntil tuple、approved World/Nodepin与原窗口。私密完整 signed response 仅 Identity 实际强SDK验、封装；必须先在Identity持久原 sealed envelope、公开ref及PUT once，再发原 immutable PUT。失败查询若有额外HTTP须独立列明确边、先付费再一次发送；未知只能查原件，不重seal、PUT、Native send或签名。原 Source observed/expires不刷新；crypto后的检查仍受原私有 Current观察及最窄 until，不冒新远端Current。晚到结果只可fact archive，不成为fresh admit、业务成功或N Captured/current authority。最后十费恰耗尽时，只原accepted ordinal/immutable graph/subphase的尚未发送叶可继续；Finish/Abort/Deadline/lease/freeze/epoch及其它Closing阻断新dispatch，不采用一般Closing豁免。实际producer/callee、独立once、冷恢复、最后N/N−1及逐物理counter未验证前，完整Actions/Tool图继续关闭。

Account issuer 唯一 typed child 只从真实 Core opaque 与 Account 私有 Current3 观察交集生成 `ExecutionPrepareSignerV2` 独立域 proof，强绑固定 Executor role/route/slot/public key、session/input/action/stage/source、RFC 原件、原观察时钟和所有最窄 until；闭合 `executor_service_and_actor_v2` profile 明确新四边，旧缺该字段 proof 不自动升级。原 Source5 是 once ordinal 而不是付款。不能跨 RPC 传 Rust Current closure、调用者 bool 或私有 slot body：接收方必须按固定部署 issuer pin/floor 强验整个 sealed grant 与精确 SDK candidate，才由该 grant 产生不可反序列化的本地签名许可。RKV 在 key open/publication 前后重新核固定 pin/floor、实际 immutable slot、完整原件和原 expires_at；这一许可仅证明原 Account 已观察短窗内的该签名操作，不声称此后新读过 Native/World/Directory/RF，也不是后续 World use、fresh admit 或业务成功。若要求新的 RKV→Account/Oracle Current 读取，必须另外列边、预付和一次消费，不能隐藏在本四边或无限延长原窗口。

RKV 在独立版本化 SQLite 表保存 signer state、sign once、public original 与 initialization once；每一 initialization 原件也先实际永久消费，重复只拒绝，不把旧 metadata 成功当免费新调用。sign 与 initialization 库分别至多1024条、每条 metadata 至多4096 bytes，其上界来自同一 Account/provision 现有最多256本 V2 Book 的四个固定 stage；公开 signed original 至多64 KiB，不增加原 slot cap。所有记录的数量、精确 ID、连续序号/hash 链、scope/slot 与时钟 floor 冷恢复反查；新模式缺表、缺必填格式版本或部分状态拒绝，不能退旧 signer/隐式 bootstrap/淘汰 unknown。sign 的实际 SQL once 必须在读取 KEK、解密或任何签名之前独立提交；后续失败只保留 unknown。真实 SDK VIEW/Prepare 两签与 public-only 原件必须完全校验后不可覆盖保存；lookup 是另需真实付费的历史事实流程，不复生 token 或重签。该 SQLite floor 不是 Native 外部 rollback floor，也不是 World delegation generation/HWM。私有 body/envelope 始终仅 Identity；此固定子图、historical fixture 或签名成功都不打开 Actions/Tool catalog 或替代完整 paid context/四业务动作验收。

### 12.18 TestSession 生命周期与真实 Agent 自测

新 TestSessionV2 的完整 `plan + scope`（包括永久 World Closed 终止条款）必须在同一次 Native custody fresh-MFA 中批准，即使 `model_budget` 为 null 也不能使用旧 opaque V2 记录。Native 仅为受支持的新 UI 合同创建 server-owned full-session summary contract，并将其与完整 top-level intent digest、配置目标和 scope 一起签入原 custody proof；原 Book 仍是唯一批准内核，不新增独立批准。浏览器在实际 get 前以用户可读摘要明确显示精确 Move0/真实资源限制、各独立预算、绝对期限与永久 Closed，model_budget Some 时还完整显示 N/preset/price/graph/whole-session 上限。server-owned contract 表示固定协议支持与已签摘要内容，不是自报 rendered bool，也不声称远程证明 DOM 或模型实际加载 Skill。

合同采用闭合 versioned typed variant：原 OwnerV2 model-budget summary 的 wire/语义保持；新 TestSessionV2 使用独立 full-session V2 variant，仍保预算专属形状与价格上界强验。旧 V1 可选值 None 省略，签名字节不变；旧记录 None 在启用新服务配置后仍返回原 None，不静默重写或升级。缺新 full-session contract、错 scope/term/预算摘要或取消确认均在 usable V2 激活/能力产生前拒绝，unknown 重送仍用原正文和原 ceremony，不发新批准以掩盖未知结果。生产支持默认关闭，未完成真实消费者与 paid graph 资格时不签可用 N cap。


新增持久 `TestSession`，并在同一次 `TestSessionApprovalV1` 中生成三个目的明确的授权记录：ExecutionGrant、ReconcileReadGrant、TerminationPlan。后两者是初次批准的 sibling authority，不是过期 execution grant 下新建更长 child。它们共享 session/account/manager/credential/security version/revocation lineage 和已有预算真值；任意 session/account/identity 撤销均能失效关联权限。保持普通 execution root 的绝对期限规则，不把 read/cleanup 的目的期限算成游戏执行延期。

Approval 绑定：manager/credential/version、账户与 native principal、环境和完整 world identity、prepared Agent/task/goal/executor fences、客户端/connector keys 与 approved runtime/Skill/source refs、typed action/参数/资源 allowlist、累计 request/business 总预算、固定 write_deadline、固定 read_until、固定 cleanup_until，以及精确 termination plan digest。初版只接受已核实 prepared_world 的固定资源；不借此模糊批准未来任意账号创建/目标/资产。

资源 allowlist 精确反映实际 Prepared 证据，不能为满足非空校验填造资源引用。闭合的 `Move` 原件只含已批准坐标且不引用资源；仅当所有动作均为精确 `Move` 且 business_limit 为零时，可批准空资源 allowlist，仍强验同一 Prepared Agent/task/Owner/Executor fence 和每次当前准入。Transfer、Build、Schedule 必须逐一核验其真实固定 World 资源事实并匹配非空 allowlist；这些资源证据尚未接通时保持 pending，不借 Move 通道扩大动作或伪造 basis。无资源动作子集的实现不代表其余动作及整体验收已完成。

资源 V2 的 TestSession 使用显式 `TestSessionBasisV2` 与新版本批准原件，签入上述 PreparedResourceBasis provenance、精确唯一 selection、派生 refs、typed action、业务/传输账与原目的期限；管理者 Native fresh MFA 批准的是这份完整固定计划。该 basis 是已强验的历史 Prepared 证据，不能在管理读取时冒充当前 owner/grant/余额。旧 V1 bytes 与恢复不默认补成已验资源；V2 action 必须与所签单一 selection 精确匹配，只能收窄额度，不换参数/资源，也不借普通 Move 通道支持资源动作。

三 roots 发行前的付费 OwnerRead 为 V2 独立重验当前同 selection 资源：新增固定 Native `ParticipantOwnerResourceView` 与 World `/v1/world/participant-owner-resource-view-v1` 原件/domain，绑定 session/approval operation、PreparedResourceBasis provenance、完整 prepared fence、World、selection 和独立 nonce。它替换原 direct ParticipantOwnerView，不经会先多读一次 World 的 OwnerControl：Account 在原 once OwnerRead SQL 中先登记 exact 原件/route/profile、扣原一次 Native transport，并登记固定内部 World 出站成本；Native 以实际 SID/CSRF/Account、原 participant register/admission 和 Owner 签名核对全范围后，恰好一次转发该固定 World resource-view，不先调旧 owner-view 或 executor quote，await 后仍核对真实会话。World 在同 pin 返回实际 owner/task/goal reference/Executor/signer record 及资源事实，before-delivery fresh pin 要求完整 CommitRef 相等。最终 Account Activate 对照原批准的历史 provenance/selection/refs，强验新强签事实、Current/DFRF、私有 SID 代、Source 五秒、Native lifetime、同 proof 的 fresh consume 和最终 SQL 后才三 roots；setup 后 grant/owner/material 改变保持 pending，不替换历史事实或复活旧权限。

V2 OwnerRead 的候选物理边为 Identity→Native 一次及 Native→World 一次，分别沿原 Native160 与固定内部 profile 成本，不借 Execution400/Read300/Control240。实际新路由必须以 bridge counters 证明没有隐藏旧 owner-view、第二次资源/quote HTTP 或免费 retry；完整预检预算、正文/五秒/CPU边界及恢复尚未验证时仍 disabled。历史 final quote 或 Prepared/Captured provenance 只供批准范围和原件关联，不能作为此次 fresh 读取、当前资源准入、Cognition/typed outbox/once 或权威 usage 结算的替代。


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

<a id="cloud-custody-world-closed-v2"></a>
**V2 的持久 World Task Closed：** Trace refs: O7-REQ-26945890-45d8-4be4-81af-5fe82c85aede；O7-REQ-f98670c4-c1fd-4610-8884-8eb2cf89c1eb。

以下为尚未实现的闭合目标，不改变前述 legacy TerminationPlanV1、GameTaskControlV1/Stop/Delegation 请求、domain、canonical bytes 或既有恢复意义。旧 Stop 仍只使任务 Suspended；旧 Revoke 仍只撤销 signer delegation，不能被展示为实际 World Task Closed。另设独立 domain `oasis7.custody.termination-plan.v2` 的 `TerminationPlanV2`，由独立 TestSession V2 custody fresh-MFA 精确批准：原 session/account/provision/Prepared/world/Owner/task/goal/Executor 初始 fence 与原 signer lineage、第二原件稳定 RevokeAndClose operation ID、已实际分配的 Stop 意图（如有）、原 cleanup_until、2 request/24 transport、ClosingOnly 与 AccountAllocatedHighWater，以及永久 World Task Closed policy。不把旧 V1 批准升级为此不可逆效果，不在 activation 时预签第二原件。

第一份原件仍为原 `GameExecutorControlV1::Stop`，保其 Suspended/no executor binding 语义。第二份采用独立 `GameTaskRevokeAndCloseV2`，签名 domain `game_task_cleanup/v2`：绑定原批准目标与 plan digest、完整 initial Active fence、exact expected signer/HWM、稳定 operation ID、原 cleanup_until，以及下文原子条件节规定的 `GameTaskCloseConditionsV2` 允许集合。`own_stop_intent` 是必须出现的 nullable 字段；null 不要求 Stop 已分配或签出，Some 只绑定已持久化 typed Stop 意图的 operation ID 与 intent digest，不要求 Account 先持有 Stop 回执。它是一份 canonical 原件、两个明确原子状态效果：更新该 signer 为 revoked 并 detach executor，且把精确 Task 持久置为 Closed；不暗改旧 Revoke 副作用、不追加第三份写、不接收通用 batch 或任意任务列表。World 事务先验证全部条件，任何一步失败都不提交只撤销未关闭或只关闭未撤销的半状态。

第二安全槽独立于 Stop unknown/响应丢失/HistoryUnavailable。出签和派发前仍必须取得新的实际非 World Current、管理 Source/Native SID/CSRF/账号/Directory/Fence/Owner 权限；同一 World 的 current 谓词由下文 [World Closed V2 的原子条件与交付当前性](#cloud-custody-world-closed-atomic-v2) 规定，在安装效果的事务中精确验证 initialActive 或自己的 Stop 所形成的 OwnStopSuspended。World 自实际 authenticated history 解析完整强签 Stop 原件/correlation/result，重算请求允许集合内的 intent tuple，验证 before→after fence、Owner/goal/任务/原 executor lineage；不能只看 Suspended/None，也不以调用者持有 result digest、post fence 或先取得额外 Current DTO 为硬前置。结果中的 `GameTaskClosePredicateV2::OwnStopSuspended` 另保实际 signed-original digest、canonical result digest 与 derived post fence，永久 `TaskTerminalRecordV2` 保留可独立严格重验的完整 predecessor witness；其余外部变更全部拒绝。

Active 分支成功时只能如实记录“第二原件撤销并关闭成功”，不能假称第一份 Stop 已成功；原 Stop 的 unknown/历史缺失仍单独保留，后续只按原 signed lookup 核对。已 Closed 后迟到的未执行 Stop/Bind/GoalPublish/Executor action 或其他旧写均拒绝；此前已 committed 的原件仍可按独立获准且付费的原件查询返回历史事实，不重新发生效果、不恢复权限、不把新 HTTP 免费包装为幂等成功。第二写的 unknown 同样保留 exact original/nonce/预留，只能依原预算内明确获准的 authenticated lookup 核对，不重签或重分配水位。

RevokeAndClose 出签前，Account 在同一权威 SQL 对当前已验 World generation/nonce、所有版本 setup、所有 session、轮换/替换及所有已分配未发送/claimed/unknown 第二原件取 max，再分别 checked_add(1)；任一 MAX、库存不完整、冲突或无法证明来源即 blocked，不能 wrap、从零猜或只统计成功行。每份分配永久保存，过期、撤销、终止或 unknown 不清水位。World 逐字段核 expected signer 与本地当前状态、新 revoked 记录的同 key/Owner lineage 与递增关系；它不能冒称看到了 Account 尚未提交的私有库存，完整库存由可信 Account/Owner 出签前强验负责，不能用一枚 caller bool/digest 单独代替该过程。

引入明确的 `GameTaskStateV2::Closed(GameTaskClosedV2)` 与权威 `TaskTerminalRecordV2`/`TaskTerminalIndexV2`，保旧 V1 wire 不新增偷换的 status 字符串。记录保完整强签关闭原件、实际 committed height/UTC、初始与实际前 fence、所选 predicate、exact revoked signer/永久 HWM，OwnStop 分支另保完整 authenticated predecessor witness；纳入实际 World authorization state root、snapshot 和 versioned terminal event。终态通过永久 task ID index 表达，不伪造 V1 task revision/executor epoch 递增；原 signer 新水位仍必须 checked max+1，MAX 不 wrap。旧 V1 task/control 可作为已核历史前态保留，但所有实际 current/prepare/context/reader/cognition/action 内核必须先检查永久 terminal index，不能把旧 Active 记录当活任务。V2 读取显式 Closed；旧 V1 无法表达时明确拒绝 unsupported_closed_task，不能投影成 Suspended、空任务或可恢复状态。同一 task ID 永久不可重新创建/恢复；原 V1 已允许的完整新 Owner/history scope 下新 task ID 不因旧 scope tombstone 被额外禁止，同 scope 第二任务的旧限制不在本切片重设计。正常恢复、裁剪和 replay 必须保留终态/HWM；present V2 index 的 schema/records 为 mandatory，不以 serde(default) 猜空，缺失或签名/root/witness 不一致失败关闭。合法旧快照仍按旧 root codec 读取；删除 terminal 并同时重算全部本地 root 的整库回滚，必须由外部已认证 state/finality anchor 检出，不能宣称本地自证防回滚。branch/reorg 改变不能复活旧 TestSession 权限，无法证明当前权威终态时保持 blocked。

关闭事实以完整 `TaskTerminalRecordV2` 与独立 V2 强签 receipt/delivery-current 合同传递；后者的正式 catalog/paid caller 仍须按下文原子条件节单独接线，不把 Runtime 记录自身称为已交付 Node 强签回执。事实关联完整原件/domain/operation、实际 commit、真实前态与 Closed、Stop 来源分支和 exact revoked signer 两效果。事实归档独立验固定 World key/domain/原件/结果；当前准入与最终 SQL 再核实际 Closed/撤销和完整来源，不能用 Core roots closed、Closing Ack、HTTP 到期或一个 revoked 字段代 World 终态。该命令不授资源 grant、不发物资/电力、不用业务 quote 包装授权；每个实际 Host/Actor RPC、Source/Directory/Fence、Native/World、lookup/回读和 CPU/存储成本仍须完整归 Cleanup24，不因同一个 HTTP 或原子双效果隐藏内部费用。

新 TerminationPlanV2/Native restricted proof/Owner factory/World 命令、所有当前读/报价/执行消费者、强 receipt、Snapshot/retention/replay、迟到原件与故障矩阵尚未完成。完整物理图仍必须逐 port 证明原24内可行；现冷签单原件23、两次至少46且尚缺派发/核对，不构成已获优化授权。新的 orchestrator 或持久观察槽不能缓存旧授权、合并实际调用计数、提高 cap、借其他 lane 或预签第二写。缺安全调用图/当前强证据/预算/库存任何一项，执行与 cleanup dispatcher 继续 disabled/cleanup_blocked，不把本目标合同写成已实现。

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

初次批准另列同一 TestSession 内的控制预算：配对后 claim/替换、MCP initialize/tools-list、heartbeat 和客户端请求关闭共用最多 80 次不同逻辑控制请求；每个已批准 transport hop 各最多 240 次控制 HTTP 尝试，包含 challenge、调用、失败及同 ID 重试。原生为 host→custody 一份；dot 为 connector→host、host→custody 各一份。Identity 的控制来源核验另有最多 160 次 native HTTP 尝试；内部 DO/binding 调用每逻辑步骤有固定上限，不隐含无界重试。预算是三目的之外的传输/连接成本，不发行第四种业务权限；DO 保存初批总额，host inbox 保存第一跳计数并向 DO 对账。未认证流量仅受入口滥用限额，不凭伪造 ID 扣合法 session 预算；通过认证后的失败/重试计尝试成本。控制额度耗尽触发 closing，但本地拒绝写的状态更新和已预留 Owner cleanup 不依赖该额度；不会把 80 个逻辑请求误算成只够 40 次 challenge+heartbeat。执行/原件读取的挑战分别计入对应 transport 上限，所有额度可缩小、不可由模型提高。 Execution 与 ReconcileRead 的每次已认证真实 HTTP 入口（含同一签名原件再次到达）分别扣对应 400/300 transport，使用可信服务生成的 ingress_attempt_id；签名摘要、客户端 request ID 不能代替入口身份。同次内部 SQL 回调只按同一 ingress_attempt_id 幂等，新的入口即使签名摘要相同也另计一次；同一入口身份若换 purpose、Host hop 或已认证原件摘要必须整次拒绝且不改账。Core 持久保存独立、必需的目的 HTTP admission 映射，绑定 ingress 身份、Execution/ReconcileRead purpose、固定 Host hop、完整签名尝试摘要与首次扣费时间；每目的受原 400/300 上限约束，合计最多 700 条。严格恢复必须核对每条与对应目的实际已扣 transport、原批准期限及固定 Host hop 一致，拒绝缺失字段、孤立记录、计数不符或与 Control/内部传输身份碰撞，不能用内部出站 transport 记录替代 HTTP admission。后续 require-paid 只接受该精确入口、purpose、Host 与原件摘要组合。此独立账不占用或借用 Control 240/Identity native 160，不提高 cap、不延期、不退款，也不开放 Termination 的公共 HTTP 或尚未实现的工具入口。

**Execution/Read 的闭合 nonce metadata：** 为后续 typed Tool 调用设置独立、仅原生客户端可用的 `ToolNonce` 服务操作，复用 `SessionChallengeRequestV1` 的 Tool kind、session、稳定 nonce-request ID、business request ID 与精确操作 body digest；不接受客户端填写 generation、费用、任意 target URL 或授权状态。metadata 请求仍以原 literal RFC 9421/9530 原件签固定 scoped `client/proof-challenges`，使用登记 client key 与当前 purpose bearer credential；不是两个签名公钥。服务从 Account 的实际 Draft、批准 pairing 与已保存 Enrollment 的 public origin 推导固定 scoped `client/actions` 目标及当前 credential generation。新入口须完整核对持久 Enrollment descriptor 与已签 TestSession client binding 都是 NativeBroker，登记 client key 相同且 pairing 没有 connector key；单凭 HTTP profile/tag 不能把合法 Dot 状态当成原生客户端。有新 nonce 事实的严格恢复同样核对此限制；旧 Dot 无新事实保持原恢复语义。旧 Control/Claim challenge 路径仍拒绝 Tool，MCP/Dot 不借此假称 OAuth 或双 PoP 已实现。

此操作只证明登记客户端对 metadata 原件的持有，不证明 Native 活性、当前 World、Source、Directory/Fence 或执行权限。它只检查本地已批准 session、inventory/freeze/security epoch、目的期限与当前凭据；Execution 还受既有 closing/lease 收紧。不得调用 Native、Root、Directory/Fence、私密 SID/KEK、World 或模型，不创建 Source、action/business reservation、Owner 原件或额外 root。后续真实 Tool 入口仍须独立付费、验证原 nonce/请求/当前凭据以及完整 fresh Native/World/DFRF 与 once outbox；本 metadata 能力不开放该尚未完成的入口，也不改变七个既有 Control 调用图。

**唯一 nonce 入口计费合同：** 最小固定图为 Host→Identity HTTP、Provider abuse DO、Realm abuse DO、Identity→固定 Owner named method、Owner→同一实际 Account，共五个真实入口/远端调用。Provider 先于 Realm，沿用可信 IP 每60秒32与 realm1024的独立 metadata 限流。最初两个服务路由发生在 Account 读取当前 key/token并强验之前，只能作为该限流保护下的有限预认证 metadata 路由：闭方法/闭schema/有界正文、物理 selector 核对、只定位既有 Account，不初始化未知业务账号，不返回私有状态或权限。不能称这两跳已从尚未认证的受害 session 预扣。

Account 完成实际 PoP及当前 key/token/完整 URI/body/目的绑定后，第一独立 SQL 按可信服务 ingress ID **固定结算三次 purpose transport**（公开 HTTP1＋上述已发生服务路由2）；两项 abuse DO 属既有独立入口限流，另列而不隐藏。这里是有界预认证路由后的结算，不称所有 routing 已预付；不得继续只记 Host1却宣称覆盖全图。费用由同一 Execution400/Read300承担，不借 Control240/Native160、不给 metadata 新增免费预算；最多可付次数因固定三次费用而下降，剩余额度不足三次即拒绝、不发 nonce。费用把 Execution 或 ReconcileRead 自身 transport 消耗到零时，必须在同一费用 SQL 持久首次 `BudgetExhausted` Closing，不覆盖已有关闭原因/时间；维护入口与到期排程补处理历史零余额。该目的不再产生新 nonce，已付同 ingress 仅可精确回读原费用事实，不重扣、不退款、不恢复执行或续 nonce；Execution 关闭不清除 Read 自有剩余额度，不把合法 business_limit0 或独立 Cleanup24 误当作该耗尽。每实际新入口同 proof 重放另结三次；同 ingress 的内部回调 exact tuple 幂等，换 purpose/原件/固定 scope 冲突时整次不改账。账与严格恢复须显式关联同一次 Host admission及两个固定内部路由费用，精确重算，不把未知旧记录推定为已付三次。

同一次 Account RPC 内第二独立 SQL 再验该已付原件，只创建或回读有界 nonce；本地两个 SQL 不是两个 RPC，也不隐藏任何额外出站。如果未来增加一次协调/确认 RPC，须另计真实往返后再准入。第二 SQL 的槽满、过期、冲突或其他失败不得回滚首笔费用：新 ingress 复用 nonce-request ID 但换 body 时，保留其已付费用，nonce issuer 相对付费后的快照不变。nonce 为服务随机32字节 canonical base64url，复用每 session 总1024、同时活跃8、每秒2的共享槽限制，不另开池；截止不越首次创建加30秒和原 purpose deadline。精确重试返回同一 nonce/首次截止，不换随机值、不续期；过期只拒绝或回原事实，不能重建。

nonce 原件须与首次 creating ingress、实际已付三次记录、完整原请求/原签尝试摘要、purpose、固定 operation target、创建时 generation、nonce 与首次时间形成不可覆盖关联。这里的原请求只保存闭合 typed body 与原签尝试摘要，不持久 HTTP Authorization/bearer、原始 headers、SID/CSRF 或可复用 raw proof；凭据不进入历史。严格恢复核对槽、账、generation和派生 URI及原截止，拒绝 orphan/collision/费用不符。正常 rotate 后旧 nonce/历史可保留但不能被旧 token继续使用；不能因历史 generation 小于当前值就破坏整个 Account 恢复。旧 Control/Claim及旧 live Tool 槽不猜作 metadata nonce，缺少新证明不得准入新 Tool 路径。完整真实 Worker 调用图、独立 fee commit/失败不退款、双验与恢复测试完成前，该新公开 metadata 分支保持关闭；Core fixture 不代实际物理计数，更不证明完整400/300执行图已闭合。

首版客户端控制调用图对 challenge 与需要 live 授权的后续调用分别取得新的来源观察和独立 Root lifetime，每个此类 HTTP 请求实际消耗 2 次 Identity→native HTTP，同一 logical 的 challenge+调用共消耗 4 次；两次请求共用 logical reservation，但各自预扣 HTTP 与 native 尝试，失败不退款。预检已消耗 8 次后，剩余 152 次 native 预算最多支持 38 个此类完整控制往返；80 是独立 logical 硬上限，不是可完成次数承诺。不能借其他目的预算、提高 160 上限，或通过客户端回显旧观察减少核验。若后续减少调用次数，必须先定义可信持久观察槽的真实来源、独立 Current/DFRF 和原 5 秒期限复验，不能缓存授权或延长观察寿命。 Finish/Abort 的第二次调用沿既有纯收紧分支：仍须真实 PoP、原 challenge 和逐 HTTP 尝试扣费，但不依赖新的 native 来源或 lifetime，避免 native 不可用阻塞关闭；它不能恢复权限或延长截止。其 challenge 仍要求 live 授权。每个真实入口 HTTP 尝试包含相同签名重放都计入 240；不得只按签名摘要去重来免除重放成本。内部同一次 admission 的 SQL 回调重试不能重复扣费，入口尝试身份必须由可信服务生成并绑定已认证原件，不能采用客户端选择的 request ID。 通过 PoP 的 Host HTTP 尝试费用必须先独立持久提交，再以同一已付 admission 原件预留 Source/native 成本；后一阶段余额不足、库存限制或准入失败不能回滚前者，失败不得出站，也不得退款。内部回调复用同 admission 幂等而非重新收费。 原生客户端入口固定以 `/v1/account-provisions/{provision_id}/accounts/{custody_account_id}/client/` 为前缀，后接既有闭合路由去除 `/v1/` 后的 tail（如 `proof-challenges`、`client-enrollments/{enrollment_id}/claim`、`test-sessions/{id}/heartbeat`）。两个 selector 只决定有界路由，不授权；完整实际 URI 含 selector 必须被 RFC 9421 `@target-uri` 签名覆盖，Account 必须以实际物理 actor、持久 Draft 和 session 的原 pairing target 复验，不重写 URI 后验旧签名、不扫描全目录、不初始化未知 Account。入口另按可信 provider IP/固定 Origin 限流，未认证请求不能扣合法 session 预算。 此入口独立限流每 60 秒可信 provider IP 最多 32 次、realm 最多 1024 次所有客户端 HTTP；采用单窗口有界 metadata bucket，时钟回退拒绝，与 Enrollment Create/Prove 的 bucket 及各会话业务账分开。缺可信 provider context 时保持 disabled，不信任 Forwarded 或客户端自报 IP。原生连接元数据在该前缀下使用固定 POST `test-sessions/{id}/initialize` 与 `test-sessions/{id}/tools-list`，分别只接受对应闭合 SessionControlRequest kind；不得复用 actions 路由或接受通用 RPC 方法。它们同 heartbeat 一样必须 live、使用 Control 资金，并受原会话绝对期限约束。MCP/dot 的独立入口、OAuth 与双 PoP 仍须单独实现和验收。

**V2 会话内部顺序与恢复：** 沿用本节 O7-REQ-26945890-45d8-4be4-81af-5fe82c85aede、O7-REQ-f98670c4-c1fd-4610-8884-8eb2cf89c1eb，新增仅属于同一 `TestSessionAuthorityV2` 的 mandatory ordinal journal；它是原 Book、账与 typed 原件的恢复索引，不是第二批准服务、能力或任意 Account 审计。V1、外部 Plan/签名/HTTP wire 和原预算上限不变，新 V2 缺 journal 不以 default 空值升级。只有实际 Account 状态事务可在成功改变原记录时以 checked ordinal 追加；同 ingress/同原件内部幂等回读不追加，失败不追加效果，但先前独立提交的费用/unknown once claim 保留原事件。事件只引用既有 typed 原件或付费记录的实际 key 和必要的 Account 事务时间，不保存 token、SID/CSRF、raw proof、私有目标或可派发签名副本。

会话 journal 覆盖实际 preflight 初始 logical reservation、Native verify claim、Source reserve、preflight once step、三 roots 激活、Host HTTP 费用、Credential/Control 各自 LF once claim、共享 challenge 创建与 credential claim/Control Ack，以及首次 deadline/lease 收紧。首次 Finish/Abort 由其 Ack 事件决定，预算耗尽由真实费用或 logical reservation 事件执行后的账决定；不得用单独自报 Closing bool 选择原因。新增业务/目的费用若会改变本会话阶段或消费共享槽，必须接入同一事务顺序后才开放，不能旁路 journal；这不开放当前 disabled 的 Actions、N inference 或清理图。现 `control_records.sequence` 可引用同一全局 ordinal，不能另行重排一个仅 Control 的平行历史。长度由原 once step、Source/paid records、共享1024挑战、最多8份 credential claim、最多80份 Control Ack 和首次收紧等既有有界库存做 checked 求和推导，不能新设任意放大 cap、借费、删除旧事实或无限追加重试事件。

strict restore 从实际完整 V2 input/Book 与原预算重放这一唯一顺序：每个事件与原 map 双向、唯一、完整关联，重建原预扣/未确认费用、共享 credential＋Control 同时活跃8槽、每秒2创建、首次 nonce 截止、当前代际历史、lease 及首次 Closing；终态必须与原账和状态逐项相等。事件 ordinal 决定 Account 提交先后，不能按 Native `server_time`、异步 started/observed 时间或 request ID 猜顺序。事件时间只能取该实际 Account mutator 的可信 `now`，供其原本的 expiry/phase 校验；外部观察保持自己的原时间、五秒和截止，不刷新、不强行要求全部异步观察时间全局单调。后到的旧观察不能借较早时间绕过已经提交的 Closing，已关闭 Execution 不因回包或 replay 重新打开。

重放保留因果边界：最后一个合法 logical challenge 可以先占槽、持久原件，再由同一事件真实耗尽预算进入 Closing；不能因此把该最后额度拒掉。相同毫秒中真实先 issue 再 Finish 的合法顺序必须可恢复，先 Finish 后 issue 的 Execution 则拒；原8槽已满而只有 Finish 才释放槽时，不能猜把新 Execution 放在 Finish 后。ReconcileRead 在独立期限/余额内仍可按实际顺序核对/缩权。过期 challenge 历史不占活跃槽，unknown 费用与 once claim 不退款；journal/原件/阶段或费用缺一、错序、错 key、溢出或数量不符均 fail closed，不清空后重建“可用”会话。该日志只证明所保存状态的同一批准事务与恢复一致性，不声称抵抗完整 VM＋内存快照回滚或替代真实 Native/World/OS 与 Agent 验收。

`GET /v1/test-sessions/{id}` 返回当前获准主体的非秘密 purpose 截止/余额、write_closed、Stop/Revoke 原件状态和 pending 引用。执行写期限后，登记客户端只能用独立 read 凭据做本次原件/close 查询，以及只会缩权的 finish/abort；普通工具包括 task/industry/context/quote/outcome 全部停止，不新读私有目标。只读 root 的读取上限是 100 次/300 次传输，Owner 查询不扣 execute 预算；预批 cleanup 原件由内部计划生成，客户端读凭据不能请求任意 Owner 签名。

一次关闭依次记录 `closing`、`write_closed`、各自 stop_status/revoke_status、pending 原件核对和自有资源回收；不是第二套世界状态机。仅当 Stop/Revoke 已按当前权威证明（完整 V2 收尾另须按本节证明实际 World Task Closed 与原子撤销）、可能派发的 canonical 原件已核清、私有准备/纯读取按 §12.17 明确分类、业务预留已结算/有明确终态、客户端及本轮自有资源回收已记录，才标 `closed`。可能有世界效果而未核清的原件保持 `closing/recovery_required/cleanup_blocked`，保留原件/预留并报告下一步；不能要求 Owner 读取其协议不允许的私有准备结果才能关闭。未关闭测试阻止同账号再次起测；人工处理可以继续独立获准核对/收尾，不能清空记录开新会话。正常 closed 可保留剩余期限内的只读证据访问；显式全会话/账号/身份撤销则连读取也关闭。

<a id="cloud-custody-world-closed-atomic-v2"></a>
#### World Closed V2 的原子条件与交付当前性

本节补充 REQ-CLOUD-CUSTODY-004/007，不更改旧 V1 Stop→Suspended、Revoke 的签名域、字节或副作用。新 TerminationPlanV2 显式批准第二原件 RevokeAndClose 的两个原子效果：撤销 exact signer lineage 并 detach executor，且持久写入 literal Task Closed。它不是 Core Closed 标记或两个读取谓词的组合；原 task ID 永久不得 Create/Bind/Resume/Publish/执行。两份 canonical 写上限不变，RevokeAndClose 是第二份而非额外第三份。

新 World Owner 强签原件须绑定完整 World/genesis/branch/reorg、account/approved plan digest、Agent/task/Owner binding+generation+pubkey、原 signer lineage、initial task/goal/executor fence、稳定 operation/correlation、cleanup 截止，以及 exact checked high-water。请求签入固定允许集合：exact initialActive fence 与必需、可显式为 null 的 own_stop_intent。null 仅允许 exactInitialActive，缺字段严格拒绝；Stop 尚未分配或尚无 typed 意图时，不为关闭而构造虚假 Stop/nonce/签名。有已持久化 Stop 意图时，own_stop_intent 精确包含稳定 operation ID 与新字段 intent_digest，定义为 canonical hash(旧 Stop 签名 domain、Owner 公钥、完整 typed Stop request)，不含 signature bytes，不静默复用旧 signed-original digest 语义；因此已分配未签也可构造独立关闭。Stop 实际 dispatch 前必须先持久化同一意图和原件，故 lost-reply 可提供 exact intent，不要求调用者持有 Stop result digest 或 post fence。World 自实际 authenticated history 判断允许的 exact initialActive，或本计划自己的 Stop 已 commit 后的 exact OwnStopSuspended；World 在安装效果的同一原子事务中比较实际 canonical 当前状态。OwnStopSuspended 必须由 World 自实际原 Stop 的完整签名/correlation、原任务链和 before→after fence 推导，结果另记录实际 selected predicate、真实 signed-original digest、derived result digest/post fence，不能只凭 Suspended/None 或调用者 receipt。initialActive 分支不假称 Stop 成功，Stop unknown/HU 不阻塞已独立批准第二原件。外部 Stop、Owner/goal/lineage/fence/branch/reorg 改变全部拒绝。

第二原件仅在触发后的独立 fresh 非 World 准入后分配并签署，不在 activation 预签。Account 对所有已 allocated、已知及 unknown nonce/generation 库存求 checked max+1，并永久保存 HWM；缺库存、溢出或 World 并发已提升值则 blocked，不 wrap、不自动重签。World 强验实际 signer 当前值及 exact 预期值，不接受 caller verified 标志。World 原子条件替代的是该同一 World 的独立预读谓词；Source 五秒、Native Root/LF、私有 generation、manager/Directory/RF、inventory/freeze/security/deadline 等非 World 强验全部保留，不宽 V1 current checks。

新强 receipt 绑定完整原件/domain/correlation、请求与观察 challenge、实际 before/after、所选 predicate、signer revocation、detach、Closed tombstone 及 canonical CommitRef。单纯 commit receipt 是历史事实；若要代替最终 World-current 读取，另须 V2 delivery-current envelope：既有服务 key 强签新注册 nonce 与实际交付时 branch/reorg/Owner/task/Closed/signer 状态，并在交付前核对完整 CommitRef。漂移返回 typed drift/unknown，不把旧 receipt 套新 nonce 当当前成功。archive 不需当前授权但不推进 Closed/current admission。

Closed 必须进入现 canonical task history/retained chain 的严格重建和 state commitment，不能通过 serde(default) 丢弃 tombstone。新 V2 task-state/receipt 显式表示 Closed；旧 V1 读取不具备该枚举时明确 unsupported，不能假报 Suspended 或 None。严格 snapshot、cold retention、重放、prune/恢复均须保留终端事实；旧快照只能以显式版本迁移并从已认证 canonical history 恢复，缺终端历史不得复活任务。

此组件不启用 Native/Account dispatcher：现有非 World 一次 Source/private-session 获取子图已至少 29 个实际调用，超过 Cleanup24，且尚未计 LF/签名/World；原冷签名单原件 23 的另一成本问题也未解决。不得把本地调用合为一个 HTTP、隐藏 batch/缓存旧授权、删非 World 强验或提高 cap 宣称闭图。新的 Account orchestration/Native 限定 purpose/Identity 私有代证明/Vault exact once 协议及每个物理 hop 必须另行闭合并实测，完整图未满足 24 时执行仍 disabled。

**Closed 的 Formal commitment。** 实际关闭后，Formal writer 不能因 legacy task current 必须拒绝 Closed 而失去构造下一权威 publication 的能力。永久 terminal index 已严格验证且当前完整 Owner/history scope 精确选择该 Closed task 时，原私有 audience leaf 使用明确版本化的 `ClosedTaskFormalProjectionV2 { api_revision: 2, closed: GameTaskClosedV2 }`；它只证明该 audience 的终态事实，不含或复活 task context、quote、grant、executor 或工具权限。public 与所有 Open/合法新 scope 的 V1 projection/leaf/tree codec 和 bytes 不变。旧 V1 current/read formatter 仍明确 unsupported_closed_task，绝不把终态投影成 Suspended/None；新 typed payload 不通过旧 V1 HTTP response 返回。新观察消费者按 exact audience/typed payload 核对实际 qualified anchor 的 projection inclusion，不能只验 packet 自带 root。实现必须以 Active publication 正控→实际 Runtime Close→V1 unsupported 仍成立→新 terminal commitment/qualified local publication 成功的有效回归验证；此组件证据不替代实际 Formal writer producer、Native/Account 付款、全部 BFT profile 或 Cleanup24 闭合。

#### 独立 Native CleanupV2 machine-auth 与 GuardedStopV2（目标合同，尚未实施）

本 profile复用同一完整 TestSessionV2 fresh MFA、Account Book、两条Termination原件及原2/24预算，不升级V1、不额外批准第二Book、不提前出签。Native ApprovalRecord中已完成的人类MFA原件不能独自证明Account consume/active，也不等于目标managed Participant SID。固定配置注册的Account dispatch-authority须分别签不同purpose/domain的 `AccountConsumedPendingV2`（实际Book已consume、roots尚未出生）和 `AccountActiveThreeRootsV2`（实际三roots持久出生，引用pending证书和Native ack）；同证书不能声称两个阶段。Native从实际已存request/proof重建exact AdmissionInput并重算full top-level intent/input/plan/term digest，不接受调用者 consumed/active布尔或仅传入的digest。证书及本地linkage精确绑定获批native_account_id、custody/provision、World/Owner/task/signer lineage、目标managed SessionRef/Native binding和epoch；manager_session_ref不是目标SID。

注册使用明确的新literal/type/domain：`/api/public/hosted-account/v2/entry/read-and-register-test-session-v2`、closed `NativeReadAndRegisterRequestV2`/`NativeReadAndRegisterReplyV2`；request digest 为 `oasis7.custody.native-read-and-register-request.v2`，Account Pending/Active 及 Native registration ack 分别使用 `oasis7.custody.account-consumed-pending.v2` / `oasis7.custody.account-active-three-roots.v2` / `oasis7.custody.native-registration-ack.v2` 签名 domain；不在旧readonly endpoint暗加登记副作用，旧V1 bytes不变。request含ConsumedPending证书、原FullMFA引用、目标managed session ref、实际OwnerVault窄lane签出的精确Owner read及各实际预付证据；ack含opaque registration ref、pending/input digest、实际Node read结果及Native服务事实，状态只能ConsumedPending。Native只保存受限、版本化session-ref→Native内部storage-key索引和linkage；rawSID/cookie/CSRF digest/CloudOwner或Executor seed不迁Native/Host，opaqueRef不是bearer。三roots未出生时pending不能签发业务权限；只由另一个ActiveThreeRoots证书触发一次单调Active转换。Native await World不持全局服务锁，完成前在实际本地事务重核managed SID/pin/epoch/freeze。每个新操作及OwnerVault/World/Actor认证前置都按真实HTTP计Native160/激活图，不能因替换一个逻辑OwnerRead就声明物理计数不变。回复丢失为unknown，不重派原操作；若实现独立 `NativeRegistrationKnownLookupV2`，须独立purpose/新nonce/实际付费，只返回精确原件历史linkage/ack，不能把缓存ack当当前授权；否则保unknown。

浏览器原Origin/CSRF/专用custody ceremony路径不变；ReadAndRegister 仍要求原 Cookie、Origin 与 CSRF，只有下述 current-v2、term-current-v2、activate-v2 独立 machine 路由不以 Cookie 授权、无 Cookie fallback，仅接受固定注册Account key对 exact method/origin/catalog path/body/instance/nonce/实际Current时限/完整scope的强PoP。billing proof仍只证明具体已预扣HTTP，不静默提升为此权限。Native每请求只从实际registry检查它自己的managed SID/Native binding generation/credential/namespace/pending/revocation/TTL/Owner pin；Cloud Identity全局private generation不是Native authority，必须由独立付费的实际Identity attestation进入Account固定配置强工厂，核验后绑定fresh dispatch证书，不能把旧注册generation或Native本地字段当Current。OwnerStop/Close只能由实际Cloud OwnerVault受限窄lane签出，或Native接收其已签原件后强验转发；NativeRoot/计费key不得代签，CloudOwner seed不搬入Native。所有actual OwnerVault/RF/Source/Identity/Actor RPC仍逐HTTP计费。实际nonce/发送claim与原件持久once，已消费proof重复拒、无success cache/自动重派；旧注册/lookup事实不授予新权限。

**GuardedStop必须有自己的Owner签名。** 新 `GameTaskStopV2` 使用 `game_task_stop/v2` domain，字段为 `api_revision:2/client_operation_id/custody_account_id/termination_plan_digest/initial_fence/target_executor_public_key/target_executor_generation/target_executor_nonce/execute_until_ms`。不得嵌套或暴露可剥离后独立提交的合法legacy Stop签名；到期外套不满足合同。实际OwnerVault签出的截止不超过批准cleanup_until、原Source5秒及原proof/session截止、实际Native/LF覆盖的最小值，之后不重签/刷新/延长。Runtime在实际已验证、private begun committed context的UTC及全部真实World current/Owner/delegation/fence核验通过后，才进入受限内部Stop mutation，一次clone事务/replace生成显式versioned canonical Stop record，效果仍Suspended；不伪造legacy签名、不用caller bool跨过旧校验。旧V1 wire/domain/Stop语义保持，旧HTTP入队前outer deadline不冒充新profile的canonical执行截止。

同一TerminationPlanV2要求mandatory `stop_policy:guarded_committed_utc_stop_v2`，close_policy仍literal persistentClosed。新GameOwnStopIntentV2/result reference显式携带 `profile:guarded_task_stop_v2`，intent是新domain、Owner公钥及完整unsigned新Stop的规范hash；NoStop显式null、缺field拒，已分配未签可Some且不强迫预签/receipt。World可独立选择exactInitialActive；OwnStopSuspended只从actual authenticated versioned GuardedStop history核强签并推导postfence，不能把legacy结果标成新时限保护。缺mandatory字段旧draft V2不默认升级，旧V1不变。新Stop事件/索引/root投影与canonical result/delivery envelope均显式版本化，旧None路径root bytes不变；final FullCommitRef在真实finalize后产生，不能进入自身state-root preimage形成循环。Close永久记录须保完整GuardedStop predecessor witness，不靠可evict的V1结果；实际任务reducer/Owner reader/Cognition/Resume按canonical顺序处理，外部Resume/新fence使旧OwnStop失配，Closed后late Stop拒。mixed/BFT/profile不支持时prequeue typed拒，不重排V1。

外部floor是强制消费者依赖：同一Native registry/pin/once恢复域不能自证Native DB防回滚。须有独立于Native DB恢复的可信配置与实际Account/recovery authority floor/cursor，绑定当前Native incarnation/nonce并核支付/签发/激活单调状态；缺失、倒退、身份不符拒且零Owner签/零World出站。若获取/核验需要RPC，逐真实HTTP计入激活/cleanup图。普通DB备份恢复须实际证明独立floor仍保留并拒旧DB复活；本组件不自证完整VM+内存任意回滚，也不把同DB保存pin充当外部floor。

首次 consent 前的防回滚边界同样须实际反例：旧无 guard 的完整强签 FullMFA/Book consume 后发生真实 revoke，再恢复到 revoke 前的旧 Native DB，首次注册不能新建一个 guard 后把旧状态当 Current。新消费者必须因缺新 producer/provenance 拒绝，零 World 出站。正向新 guarded options→真实 complete→真实 revoke→恢复同 lineage 旧 DB，必须在独立 floor 核对处拒，且不能回填 tag、重签或重发来“恢复”。保普通 V1 的真实正控；这些测试不把整 VM rollback 当已证明。


上述独立认证若实际闭合，World原子exactInitial/OwnStop/UTC可使新profile不需要普通EntryExecutor VIEW来授权同一conditional Close；不能直接删旧check或默认省去OwnerSigner/RF/Source/Native/read/descriptor。旧non-World scaffold保守下界29仍超24；新caller必须逐实际Host/Actor/Directory/RF/Identity私代际/Source/LF/descriptor/OwnerVault/Native/World Submit或Lookup/最终freshCurrent admission列账，注册及丢Stop回复恢复的新Current均计费。Native/World两个paid indices只证明具体两请求，不是全图上界。实际caller、独立floor/once、GuardedStop消费者及真实<=24图完成并独立评审/验证前，仍defaultoff；不提高cap、不缓存旧authority、不以组件通过宣称全链cleanup或生产就绪。


此注册路径沿实际 Native `PREFIX=/api/public/hosted-account/v2` 挂接，仍经过原人类 Cookie、Origin 与 CSRF 校验；不是普通 ceremony、只读方法副作用或旧 endpoint 的 Cookie 豁免。后续显式 machine 操作使用固定 `/api/public/hosted-account/v2/custody/cleanup/current-v2` 与 `/api/public/hosted-account/v2/custody/cleanup/activate-v2`，仅这些新 literal 使用独立 Account dispatcher PoP，旧 entry-owner/control、managed lifetime 与所有普通浏览器路由保持原认证。current-v2 只读取 Native 实际 managed SID/credential/binding/current 并返回事实，不持有或替代 CloudOwnerVault 的 Owner seed/签名权限。

外部 floor 可在已有 mandatory Account finalCurrent 的同一次实际认证绑定 RPC 内核对，但必须使用该 Account/recovery authority 自己持久化的 floor/cursor，并把 Native 本地当前证明与 exact registered incarnation、输入/会话、operation、实际 transport attempt、nonce 和单调 cursor 原子核对；不能把 Native 自报 floor、同库 pin、旧成功回复或账本 proof 当认证。发生责任迁移前须有生产者/消费者与普通 DB restore 的实际反例。若实际 floor 在另一 Actor/HTTP，内部这一跳仍逐次计费；不能只因有同一外层 RPC 就算零。完整成本须分别列正常、StopUnknown 直接条件 Close、CloseUnknown 原 signed knownLookup→fact archive→独立 freshCurrent admit，以及新 Source/Native/Fence 失败分支；24 已用满时不能把确认查询算免费或自动提高 cap。

Account 发起型新 lane 的 floor 消费者必须是持久 floor 所在的 Account authority：Account 发出已实际预付的新 Native current 请求，Native 只返回新 domain 事实；Account 在同一个 mandatory finalCurrent handler 中，以自己持久的 floor/cursor 和真实独立 Identity/RF/Directory 当前材料核对该事实，再允许 CloudOwnerVault 使用。Native 不持 Owner seed、不签 Owner 原件，也不发行可直接调用 World 的 descriptor，因此这一路不要求 Native 再回调 Account 取同一 floor。若 floor 位于另一实际 Actor，仍须逐跳计费；此责任选择不是把内部 RPC 隐去，也不放松 Native 本地 SID/credential/binding 检查或现有配置的恢复 floor。

该新 lane 的 Source/LF nonce 与普通 V2 路径显式分域；Account 在派发前持久标记 machine Source ID、完整请求摘要和实际 attempt。普通 Source complete/live API 必须拒这些 ID，不允许抽出新 fact 的内层旧格式 InspectOrigin/LF 来绕过 external floor。新 fact 仍可真实记录 `Expired` MFA 与 `OriginActive`：已消费会话的截止由完整已批准 plan/root 决定，不把过期 ceremony 重新声明为 Active、不重新消费或刷新 MFA；Source 原 5 秒与自然会话期限仍逐次核验。

首个 ReadAndRegister V2 接线限定已批准的精确 Move0 OwnerView profile；不把普通 OwnerRead、旧 V1 空 refs 或资源 profile 的未接 consumer 升级。新 closed `RecordLifetimeAndClaimMachineRead` RPC 替换旧 `RecordLifetime`：沿旧 handler 已实际付费取得的同一 Current/Source ticket，在一个 Account SQL 转移中保存完整强签 LF、预扣原 `OwnerRead` once claim，并由真实 Core producer 生成 Pending 原件供专用 Account issuer 签名；同一响应返回 exact ReadAndRegister 请求。Identity 不再调用旧后续 `ClaimStep(OwnerRead)` Actor RPC，随后用新请求替换原那一次 Identity→Native OwnerRead；不是在纯 `ClaimStep` 中补免费 Current、不是额外 probe 或新增 World 读。certificate 除完整已消费批准/plan/input/session digest 外，还必须签入该实际 OwnerRead claim 的 Source/record digest、exact 已由 CloudOwnerVault 签出的 OwnerView 原件 digest、两端实际 attempt ID、claimed_at 及固定 Native1/World1 profile。Native1 属原 Native160；World1 属已声明的内部固定 preflight profile（全 preflight 的 PolicyOriginal1、Authorize 内 policy1、OwnerRead1 共 World3，各自先持久 claim），不借 Execution400/Read300/Control240，不把其中 World 调用混成 native HTTP。

新 request 可用 closed `AccountConsumedPendingReferenceV2` 传递证书的紧凑引用：仅省去 Native 自己已经持久保存的完整 request/proof，Native 必须从 exact completed FullTestSessionV2 ApprovalRecord 重建同一个 `ApprovalAdmissionInput`，展开为原完整证书的同一 canonical signing bytes/digest，再用固定 Account issuer 验原签名；不是新 authority、不是摘要替权限、不能从普通 ceremony 或缺失记录补造 consent。引用中的 approval/session/input/intent/plan 与完整已付 read claim 均严格匹配。该压缩不提高原请求上限；实际 final request serializer 超限时出站前拒，不裁剪字段。Native ack 还签入 exact World response digest；Account 在原 Activate callback 中同时强验固定 World key 的完整 Node reply 和固定 Native root 的 ack/本次 request，之后才提交三 roots。Native 与 World 两项 prepaid/claimed/observed 分开，回复丢失保 unknown，不把已预留等同已发生 I/O。

ReadAndRegister 的网络接缝必须从 Native 全局 service mutex 外执行：锁内核配置/真实 Cookie+Origin+CSRF、已完成 consent、actual managed SID/credential/binding 和 prepaid once，持久 claim 后生成不可反序列化的 owned read token；释放锁后仅发固定那一次 World OwnerView，重入锁再比较全部配置代、Native session/credential/account/Owner pin/namespace 与原 deadline，才写 Pending registry 和 ack。不得跨 await 持总锁，不复制 Native/Owner seed到token；只移出固定信任材料和只读 HTTP client。独立 restore journal 必须先于新 TermWide guarded-consent 的 options 记录创建，而不是等首次 ReadAndRegister 才建立基线。该专用 options 在实际 Cookie/Origin/CSRF 与新 profile/固定配置验证后，显式建立或核对独立 guard；不让 ordinary open、普通只读路由或旧 V1 自动初始化。options、complete 与此后的 session/credential/account/revoke 写入都必须参与同一 guard incarnation 的事务，DB marker 与外部 journal 双阶段。新 ApprovalRecord 保存不可回填的 producer 版本、guard incarnation、options creation head、完整原 request digest 与 machine 配置摘要 provenance；完成后仍只保原 provenance，不因配置启用或新消费者存在而补 tag。ReadAndRegister 只接受具有该 provenance、在同一仍受保护 lineage 内真正完成的 FullMFA 原件，并在任何 World 出站前强核它与当前 guard/head/配置/完整保存请求的绑定。旧 V1/旧 FullV2 元数据 consent 缺 tag 时仍是旧语义，不升级为新 machine consent；缺外部 guard、倒退或不确定时拒新 profile。完整 VM/文件系统回滚仍非本组件资格。注册、Active确认及其 unknown 只读恢复的每个新增实际 Native/Actor边须另外列入既有激活预算，不说同一响应/同一逻辑步骤就没有物理成本。

新 machine producer 必须显式分阶段，不能直接借用实验 `PaidCleanupTransportV1`：旧 codec 在 Native 派发前要求实际已签原件与最终 World body digest，且仅描述旧固定 Native forward 路径及 Close Submit/Lookup；新 `current-v2` 要先取得真实 LF/Origin/Source 才能收紧原件截止，不能为满足旧字段而预签或猜未来 bytes。旧 V1 codec/domain/path 保持不变，新 lane 用同一个 Termination ledger 的 versioned paid-edge records，不新增 Book、额度或可执行 authority。

第一阶段只为 exact Native current HTTP 预扣并一次 claim：固定 Account issuer/epoch/config floor、完整已批准 input/plan/term digest、session、Stop/Close slot 与稳定 operation ID、Source ID、Native instance/固定 path、实际 native attempt/nonce/body digest、原批准截止及本次短期限。`observation_intent_digest` 是新独立 domain 的观察意图，不能冒充已有 Stop signed-original digest；它绑定批准范围、slot/operation/Source/attempt，不要求尚不存在的 Owner 签名或未知最终 World body。已分配原件时还必须绑定 exact original digest（mandatory-nullable，只有确实未分配才可 null）。Native 仅持久 claim 后返回真实当前事实，不据该第一阶段记录发送 World 或发行 Owner 签名。

第二阶段由实际 Account factory 在原预扣记录内严格消费 Native fact，核配置固定 Native root、自己的 external floor、实际已消费 V2 Book/三 roots/Closing、DF/RF、独立 Identity globalgen 与原 Source 5 秒，才分配或复用完全相同的原件，并交 CloudOwnerVault 真实签名。之后另 seal 不可变 exact World request/body digest、实际 World peer/path/attempt/paid ordinal 与同一批准/slot/original；World edge 的 proof 仅在这一步产生，发送前仍按实际图重核必要当前 fence/RF，World committed UTC 最终拒过期原件。Native-only、pending/unknown、未核完整 fact、尚未 sealed World body 的持久阶段都不能恢复为 World-ready。每个实际 HTTP 独立预扣/once，重复已 claim proof/substep 拒绝而不成功缓存；任何 reply-lost 原件保留 unknown，不重派、不改签、不刷新 until。原已签 lookup 只读取原件事实，仍需本次真实已付观察 edge 和独立当前准入，不能把历史事实当新执行 grant。

费用资格分支必须分开。新 Account 发起方案的 `10` 仅是 handler 内部候选小计：Directory 1、RF service+DO 2、Identity service+NativeSessions DO 2、Native current 1、同部署 RoleKeyVault DO 1、World 前 RF 2、World HTTP 1；独立 floor 回调两 HTTP 仅在上款实际 Account 消费合同完成后可以省去。实际 Host→Owner frontend→Account DO 的触发另有 2 HTTP，不能因是入口而漏算；Stop、Close 各独立触发时正常候选图仍为 `12+12=24`。StopUnknown 可直接独立条件 Close，但不据此伪造 Stop 成功事实；CloseUnknown 若新增独立触发并执行完整 freshCurrent 查询原件，仅省 OwnerVault 重签仍需内部 9 加入口 2，最坏累计 35，不能在 24 内自动恢复。若提议同一 Account orchestrator 在一个已明确预付、持久 once 的图中推进两槽，须实际证明没有第二入口 RPC、内部回调或隐藏额外 Native/RF/Identity I/O，不能先假定触发免费。只有逐项证明同一尚有效、同请求 scope 的事实可以合法复用且每个新 HTTP 仍独立预扣/once，才可另给有界图；不能把缓存 authority、未核当前的 Identity/globalgen 或未列出的 Actor 调用当作省费条件。完整最坏图未实际闭合前 dispatcher 保持关闭。

#### Term-wide 5 秒观察与 Account owned invocation（新独立 profile，尚未实施或费用合格）

Trace refs: O7-REQ-f837065c-4da5-4e6f-8a53-f4e5900cf26c；O7-REQ-f98670c4-c1fd-4610-8884-8eb2cf89c1eb。

本 profile 不能由上款 slot-bound `NativeCurrentEnvelopeV2` 隐式升级。完整 TestSessionV2 原件必须签入新的必填 closed `cleanup_transport_policy`，明确选择 `TermWideFiveSecondOwnedV2`；旧 V1 字节、旧 V2 实验原件和普通 Source/Native routes 不升级。仍只有同一 Book、Stop/Close 两槽、2 次 canonical writes、24 transport、原 cleanup_until。缺字段、未实际 consume/三根/Closing、未装新消费者或费用图未验收一律关闭。

新固定 Native literal 为 `/api/public/hosted-account/v2/custody/cleanup/term-current-v2`；旧 slot-bound `current-v2` 不变。新的 `NativeTermObservationRequestV2` / `NativeTermCurrentFactV2` 分别使用 `oasis7.custody.native-term-observation.v2` / `oasis7.custody.native-term-current-fact.v2`。请求与 Account 已付证明绑定 full approved-input/plan/term digest、session/owned-invocation/source ID、固定 Native issuer/实例/epoch、真实 Active 证书及 ack、唯一 native attempt、三个独立新域 nonce，以及精确 Stop/Close operation IDs 的封闭集合。它不绑定未来尚不存在的 signed-original digest，不授 World write、scope 或 signer 权限；每次实际 HTTP 的 paid attempt 仍另行不可重用。Native 只核本地实际 completed FullMFA、managed SID/credential/binding/Root/epoch/pending/freeze 和当前 Origin/LF，不自报 Cloud private generation，不签 Owner 原件。

Account 用真实已认证的 Directory 1、RF service+DO 2、Identity service+NativeSessions DO 2 与 Native 1 共六条物理边构造非 Deserialize/非 Clone 的当前观察 token。固定配置工厂强验所有实际回包、Native Root、Account 自己的 durable external Native floor、Cloud globalgen/private history 与 full actual Book/三根/Closing；floor 与观察事实在同一 Account 事务消费，不再发 Native→Account floor 回调。token 的上界取原 Source 截止及六边最早可信开始时间+5秒、Native/Origin/LF自然期限、原计划截止的最小值，不以最后回包延长；持久保存事实不是恢复 token 的授权。已知 tuple/配置/generation/epoch/freeze 改变立即拒；每次分配、签名或发送前重新运行所有本地 full-current 谓词，只有 token 本身仍满足同一完整 tuple 与原 5 秒才可使用。同一 token 可服务本 Term 的两个固定槽及 exact-original lookup，不可借给其他 session/operation/domain。普通 Source 接口拒该域，不可剥出旧内部 LF/Origin 当普通 Source。

World 的 owner/task/delegation/branch 与 UTC 最终谓词仍在每次实际 commit 原子核对，不缓存旧 World authority。每次 World HTTP 前独立支付并实际重读 RF service+DO 两边；RF 对象不得从六边观察中复用。实际 CloudOwnerVault 独立支付一边，只接受新域 exact Owner body/current/seal 证明且使用原私有 Owner seed；Native Root、Account billing/issuer key 或公开原件都不能代签。Stop/Close 原件实际 until<=min(原批准截止、该次原 Source 5秒、实际 Native/LF覆盖)，分配后原 bytes/nonce/until 不刷新、不重签。Stop 未构造、仅 unsigned allocation、已签 unknown 和已 commit/lost reply 分别保留；Close 的 World InitialActive/OwnGuardedStop 条件选择不要求调用者先拿到 Stop receipt，也不据 unknown 伪造 Stop 成功。

同一 durable Account 记录建立 owned invocation，不由 public caller 传入 trusted/current/paid 布尔。已有 Finish/Abort 仍只写 Closing，并可持久登记调度意图；实际 cleanup 在独立内部任务中推进，不把该 Control ACK 当 World Closed。一个 invocation 一次获取观察后可顺序推进两槽；CF await/reentry 后必须重取并逐字段比对最新 Account record/配置/原件/全局 HWM/floor，不能持锁做网络。若实际使用 alarm 生命周期继续，须沿现真实 `OwnerAccount::alarm -> repair` 入口新增封闭消费者，明确平台生命周期 delivery 与应用 `fetch` 的边界，并以 workerd/HTTP 探针证明没有第二 Host/Owner frontend/Account fetch、内部回调或隐式重试；不能仅凭“同一个 Account”宣称入口免费。所有实际平台/Actor HTTP 若存在，照实加入图，不适合下表即拒本 profile。

新 profile 明确由真实 Account durable alarm/owned continuation 发起，而不是第二套 public kickoff：既有 `OwnerAccount::alarm` 没有 HTTP Request 参数，现仅直接调用同对象 `owner_test_session_alarm::repair`。新调用者必须在该同对象内部直接推进持久 job，不调用 Owner frontend/Account namespace.fetch 来调用自己；每次 CF reentry 后仍全量重核同一实际记录。平台 alarm 生命周期 delivery 必须在证据中单列，不能声称测量了 Cloudflare 基础设施内部网络；本 transport 账按实际应用 HTTP/Actor binding.fetch 计，任何新增应用 fetch/callback 都必须另扣24，不能套“内部调用”豁免。如果实际 deployment/runner 需要 Host→Owner→Account 两条新的应用入口，本 profile 不合格，回到外部图25并关闭。调度本身不出签、不发World，也不预签未来 Revoke；实际 until 总在当前检查后收紧。

GuardedStopV2 的 Runtime 合同只允许省去不必要的 Stop lookup，不是 RF 或当前检查：current exactInitial 时选择 InitialActive；否则从永久 root-covered `task_stops_v2.records` 按新 GuardedStop domain+Owner subject+完整typed Stop request意图找唯一exact own Stop，验证原始 Owner/Executor lineage、delegation、branch/HWM/post-fence并保留完整 witness。调用者不用先取得 Stop receipt。故 StopUnknown 直接使用独立已批准的 Close 槽；Stop 原件/nonce/HWM/unknown照存，不能改成None或宣Stop成功。Close的真实强回执记录所选分支和完整前件；若World为InitialActive分支，晚到Stop永久拒；若OwnStop分支，查询Close原件即可取得已认证完整前件事实，不再发Stop查询。

**严格 Stop 回执顺序的直连组合观察（独立新闭合 profile，尚未实现或资格验证）。** 沿本 TermWide 已签入的 exact-original known-lookup 与非秘密控制观察范围，新增固定 World `POST /v1/world/task-stop-known-control-v2`，只由实际 Account owned invocation 的受限 producer 构造；不新增 Book、MFA 或预算。只有固定配置允许 Account 直连该 World origin/path、配置 Node pin 且所有出站已预扣时才支持，不迁 managed SID/CSRF、Native Root 或 CloudOwner seed。旧 Stop、旧普通 lookup、Formal read 和旧 Native route 不取得此 profile；尤其不能从 FormalAuthenticatedLookupV1 删除新的 Owner read 签名后冒作同一协议。新请求只携在原同一次 CloudOwnerVault 签名内明确含 combined observation grant 的完整新域 GuardedStop Owner 原件、exact Term/input/session/invocation/Source 与已付 attempt 关联及新的独立观察 nonce；目标从原件唯一导出，不接受调用者选择其他 task、Agent、scope、字段、分页或 batch。新 nonce 只是已批准 scope 内的观察关联，不代替 Owner scope 或 Account 当前准入。

Owner 原件采用最窄同域扩展：`GameTaskStopV2.observation_grant: Option<StopObservationGrantV2>`，None 缺字段与 None 序列化均省略，保既有 GuardedStop-only canonical bytes；只有 Some 为 closed `known_stop_control_v2` 模式，字段为 `target_digest`、`approved_cleanup_until_ms`、`historical_read_until_ms`、`control_read_until_ms`，不提供任意字段集合或通用 read scope。`target_digest` 精确取 canonical hash `("oasis7.game-task-stop-observation-scope.v2", custody_account_id, termination_plan_digest, client_operation_id, initial_fence, target_executor_public_key, target_executor_generation, target_executor_nonce)`；scope preimage 不含 grant 自身，避免自引用。Owner 签名仍沿实际 `world_service::signing_bytes("game_task_stop/v2", &完整GameTaskStopV2含Somegrant)`，所以原 Stop effect 与此窄观察权限在原那一次 Vault 调用共同签入，不新增 MFA/RPC/预算。None 可执行原 GuardedStop 业务，但不能调用 combined endpoint；旧签名不能通过添加/删除/更换 grant 或外层字段升级。

业务 `execute_until_ms`、历史查询期限与 current 查询期限分别明确：`0 < control_read_until_ms <= historical_read_until_ms <= approved_cleanup_until_ms`，业务 Stop execute_until 另须不超过同 approved cleanup deadline。原受限 Vault/Account producer必须对 actual consumed full plan 核 `approved_cleanup_until_ms == termination.cleanup_until` 与完整原目标/intent digest；Node 不把 caller 自报 Term 或 Account bill 当授权，只接受原 Owner 真签的 closed grant。Node 的实际接收/交付 UTC 与原固定 World 配置用于检查读期限，不能取 caller now 或仅历史 committed timestamp；接收已过 historical deadline 不新查，交付已过 control deadline只允许返回仍获准的历史 fact、不生 Current。Account 当前准入还必须取原 Source 五秒、Native/LF 自然期限和此 control deadline的最小值，不由 Node 回包重置任何 until。已实际取得的 Node 强签 fact 可在读期限后独立本地归档，但不能由此重开 World 读取或恢复 Current。Some grant 必须进入 Runtime 的完整原件/永久 Stop record/结果 commitment、Close OwnStop intent hash与完整 witness；不从副表或后来配置补 grant，普通 V1 和既有 None bytes均不变。

同一有界 World HTTP 返回两个明确独立 Node 强签域 `oasis7.world.stop-known-fact.v2` 与 `oasis7.world.stop-control-current.v2`。前者绑定完整查询、原 Stop、真实 canonical Suspended record 与历史 CommitRef，可独立晚归档；后者另绑定相同完整查询与本次 Source nonce、实际完整当前 CommitRef、该原目标的 Owner lineage、GameTaskFenceV1、同 Agent 的 signer delegation/generation/revoked/HWM，并经交付前独立 pin 比较。当前投影不含目标正文、industry/resource/quote、ContextRef 私密正文或任意 World snapshot；goal 仅保同原件已批准的引用/版本。Owner、目标、goal 或 Executor lineage 漂移只返回 closed Drift，不披露新目标信息；历史 fact 不因 Current 漂移而丢弃，历史签名也不能替 Current。World 必须先按原件 Owner 签名内的显式 grant 与当前 exact Owner/目标关系核定这项窄读；仅公开 Stop 签名、Account paid 关联、owned-invocation 标签或新 nonce 均不足以授权 combined 观察。无 grant 的旧 GuardedStop-only 必须拒绝新接口，不作为静默兼容分支；普通 Formal/OwnerView 的独立签名校验完全保留。Account 在独立 post-Stop fresh 六边、完整实际 Book/Closing/floor/local reentry 与原五秒仍有效时才可将两证据共同 admit，然后才分配和签 Revoke；不得预签 Revoke、刷新 Source 或把保存的投影恢复成当前 token。

此查询只读一次已限定的原件和当前控制状态；HistoryUnavailable、Pending/Unknown、Drift、不同 delivery tip、缺强签或过期均不证明 Suspended 当前准入，不自动补页、重发或查询另一原件。Node 必须具备实际 qualified current/history/delivery pin 与新两域 signer/HTTP adapter；本地有界读取不是额外 HTTP 的豁免，实际远程 CAS、签名服务或回调须另列费，缺消费者保持 unsupported。原 §12.18 的无 Stop receipt 条件 Close 分支仍是不同合同，不能拿其 14/23 小计证明本严格顺序。

严格 lost-Stop-response 候选为首观察六边、Stop Submit 四边、独立 post-Stop fresh 六边、组合 known/current 查询的 fresh RF 两边加 World 一边、Revoke Submit 四边，即 `6+4+6+3+4=23`；每个 World HTTP 前的 RF 两边独立发生，不复用六边观察中的 RF。若实际必须 Native 转发查询，则新增 Native 一边，候选 24，该分支在实证前 unsupported；新 World 一边不能藏入 Native current 一边。此余量不允许自动 retry：Revoke 也 Unknown 再查询至少三边成为 26，观察过期需新六边、额外入口/lookup/Actor 均可能超额，必须 blocked 并保原件/HWM/已付费用与 Closing/unknown，不伪 Closed，不保证任意双 unknown 恢复。Native archive-only 仅可能读取此前已持久保存的 Node fact，不发 World、不 mint Current，且其缓存形成与读取各实际边另计，不作为本直连 23 图的省费条件。只有实际 mounted caller、逐边付款/once、强签与丢回复/期限矩阵证明后，才可称 Cleanup24 该有界分支合格。

以下全为实际应用 HTTP/Actor fetch，未实现/未验收，不能先开启：

| 边 | 次数 | 付款与once |
| --- | ---: | --- |
| Directory实际principal/policy读取 | 1 | 首观察或一次刷新各自新paid Source port；不缓存authority |
| RF service→RF DO读取 | 2 | 首观察/刷新各2，且每个World HTTP前另2 |
| Identity service→NativeSessions DO globalgen/history | 2 | 实际独立Cloud权威；Native不自报 |
| 新Native term-current HTTP（本地LF+Origin） | 1 | 新body/domain、固定Account issuer、Native本地once/外部journal |
| CloudOwnerVault受限Owner sign | 每Submit 1 | 实际持原Owner seed，只签exact新Owner域；无Native/账单key代签 |
| World Submit或KnownLookup | 每次1 | 每actualHTTP新sealed paid proof；重复已claim只拒，无成功cache |
| 同Account SQL/floor消费/原件归档/内部函数 | 0 | 不是HTTP；不免除任何跨Actor fetch；不得远程回调偷列0 |

首观察六边；每个Submit=`Vault1+freshRF2+World1=4`；KnownLookup=`freshRF2+World1=3`。正常Stop+Close=`6+4+4=14`。StopUnknown不查Stop，另fresh观察6后独立Close=`6+4+6+4=20`；若Close也Unknown，保留这份仍有效的观察，最终只查Close一次3，完整双unknown候选=`6+4+6+4+3=23`。Stop已知而CloseUnknown同样至多`14+6+3=23`。一条额外真实应用Actor边若确属必要必须实数+1才24，不能凭空新增或用作省略两边外部入口。任何实际Host外部kickoff2、二次fresh观察6、第二次Close查询3、隐藏Native/descriptor/RF/Identity调用会超24，禁止启用该未闭合实现。

当前观察仍须在最终Close事实独立admit时有效；fact-only archive不推进Root Closed。即使最多一次fresh观察与一次Close查询的port图可在23内，第二次观察过期、更多lostreply/重启或额外入口仍可能不够；此时拒新dispatch并保Closing/unknown，不延5秒、不刷新旧until、不重签原件、不退款/借Read300或Control/Native160。不能为了报告成功在lookup回包后省freshCurrent。资格测试至少覆盖实际World Stop/Close双回包丢失、真正一次freshCurrent后CloseKnownLookup、完整强签archive与独立current-admit最终literal World Closed及全部allocatedunknown checkedmax+1。对刻意拖至第二次过期的负例须wholeState保原件/HWM并blocked，不能把负例当正常完成。

Guarded Stop 的持久兼容只保历史，不恢复旧发送权限。新增独立 `guarded_stop` outbox 必须保存完整实际 `GameTaskStopV2`（包括同一次 Owner 签名内的 Some observation grant）；新 allocator/Close own-stop intent 只承认 `game_task_stop/v2` 的完整原文与闭合 Guarded profile，绝不把旧 `GameExecutorControlV1` hash 改标签升级。旧已保存 legacy Stop 的原 payload、Owner 签名、记录 digest、Source/LF 原件、费用与 unknown 状态不改。新 Guarded 批准下，旧 publish-original 和 submit 写入口即使命中缓存原件也必须拒绝且 whole-state 不变；只读历史取得、已付 known-original lookup 与强签事实归档沿各自原有授权/预算/once 保留，不能以历史可读推导重新签名或重发。

新增字段缺失的唯一迁移边界是：Raw 恢复确认已存在 **signed legacy Stop Some**，其完整原件、固定 Owner 强签、Source/LF 绑定、全部账本/库存及原恢复谓词全部通过后，才可将缺失的新 `guarded_stop` 显式映射为 None，并立即落在上述永久禁止 legacy publish/submit 的历史状态。此规则不适用于无 legacy Stop、unsigned legacy、歧义的双 outbox、任意其它缺失字段、签名/原件/账不符；这些情况必须拒绝，不能用通用 `serde(default)`、补签/补费/改原件或 caller migration 标志使其通过。迁移不构造新 Guarded 原件/Some grant、first-dispatch handle、Current 或新预算，不刷新原 Source 五秒、Term 截止或已有未知状态；旧 V1 wire 与恢复合同不变。

Term 的直连 Owner Vault1 只支持显式新安装的闭合 signer profile，不能把普通 OwnerSetup signer 的 Account/RF 回调算作本地。唯一 Account service issuer 的专用 typed child 只能消费实际 Core 已付 claim 首次返回的非 Clone/Serde token，并为一个固定 Owner slot、完整批准 input/Term/Guarded 原文、独立 paid attempt/原件 digest 与原 absolute-until 签固定 `AccountMachinePurposeV2::TermGuardedStopSignerV2` / `oasis7.custody.term-guarded-stop-signer.v2` 许可，recipient 明确绑定 `Role::Owner` 与实际 `RoleKeySlotScopeV1`（既有 issuer pin 的 Native origin 仅保原信任 tuple，不冒 Owner Actor recipient）；Native Root、Owner key、普通 SDK BindingAuthority 或 caller JSON 不能替代该 Account 发行权。Actor 只按固定配置公 pin/floor、确切 Owner role/scope/key ref+public key、完整 typed 原文/用途、实际付款 child 和原窗口验许可，不能选 key、改域或调用普通授权回调。它先在独立持久事务消费该物理 attempt/原件 once，再读取 KEK、解密并签完整新 Stop 原文；SQL、解密、签名、回包 unknown 都保占用/费用，不重派、不签成功 cache、不延原 Source/Term 窗口。

该新 Actor profile 的 once/floor 状态必须与 **新 Owner slot 的真实首次付费创建** 在同一原子事务初始化，复用该次既有 Actor 请求而不增加隐形请求；这只安装存储边界，不发行 Term 许可。既有 Owner slot 缺新 profile state、部分状态、错误 schema/config/slot 绑定一律 Unsupported，不能在首次 Term sign 时自动 `CREATE IF NOT EXISTS` 空账、从当前配置补 floor 或由请求补 marker。若今后要为既有 slot 单独初始化，必须另列真实 Actor 请求及其授权、付款与独立验收；未计入以前不得称 cleanup 候选23仍成立。永久 once 库存受实际既有批准/原件上界派生的 checked 长度约束，不另加可借用预算或任意新 cap。普通备份/冷恢复须保完整新 profile 状态并重验；同一 DO/本地盘中的 once/floor 不能自证整 DO 或整盘回滚防护，未有独立保护与实证的整体回滚边界必须明确不覆盖。该 profile 在实际 paid 初始化、Core claim/issuer、Actor 先持久后密钥借用和真实物理一次计数共同验证前保持关闭；它不包含后续 RF2/World1，也不单独证明 Term6/cleanup23。

Revoke/Close 的 Owner Vault1 是独立的第二个物理请求与独立原件，不能复用 Stop signer 许可、第一次 once 或一次签两份。后继闭合 profile 使用独立 `TermGuardedCloseSignerV2` / `oasis7.custody.term-guarded-close-signer.v2`，只允许固定 Owner Actor Close route 与完整实际 `GameTaskRevokeAndCloseV2`，Owner 最终仍按既有 `game_task_cleanup/v2` 签名；唯一 Account issuer child 只消费实际 Close allocation、独立 transport1 已持久提交后首次返回的非 Clone/Serde handle。完整许可另外绑定既有 Close allocation digest、所有 allocated/unknown 的 generation 与 nonce checked-max+1 HWM、两种精确允许前件、同一 session/input/Term/Owner slot、原 Source/Current 及 absolute-until；OwnStopSuspended 只能由 World 实际解析同一 GuardedStop 原件的 canonical 强事实，不用 unsigned intent 或 unknown 自报 Suspended，InitialActive 仍只匹配原批准 initial fence；新的 Current 只能收紧本次签名/发送许可，不能修改或延长任一已经分配原件自身的 bytes/nonce/until；Actor 不获取新的 World 状态、不推断 Stop 已成功、不修改分支或预签激活期 Close，World 仍用原始独立 Close 原件在实际提交时核条件。Stop unknown 不构成 Close 的 fresh Current 或成功事实。历史 Stop 的 `execute_until_ms` 只限制它当时的真实提交，Owner grant 的 historical/control-read 截止只限制该窄外部读取；已经 canonical commit 的 Stop 强事实按其实际 committed UTC 与原当时窗口验真，不要求后来的 Close 分配/签名/提交仍早于 Stop 的旧截止，也不把旧 read grant 当 Close 时的 Current。尚未分配的新 Close 使用其独立合格 Current/Source 与原 Term cleanup 截止计算自己的窗口，可以晚于历史 Stop 截止；Current 依既有同 invocation 合同可复用仍鲜 token，或从实际另付六边取得，但不得延 Term、刷新/重签已存在的 Stop 或 Close。World 的 OwnStop 分支仍必须解析同一历史 Stop 的实际 canonical 事实，不能因它已过期而重派 Stop；旧 read grant 已过期时也不得调用其 combined 读冒 fresh Current。

新安装若要同时支持 Stop 与 Close，必须在真实新 slot 的同一首次付费创建事务中保存明确的闭合双用途模式；旧 Stop-only 或缺/部分 profile 不自动扩用途。两类原件均完整绑定自己的固定用途/route；同一 issuer/paid attempt 在完整联合库存内不得跨用途再次消费，不因 route 不同形成两个可用 once 槽；每一实际 Session 至多各一份；上界依据为既有 `test_session_v2_restore.rs::validate_test_session_v2_persistence` 对同一 Account `test_sessions_v2.len() <= 256` 的强恢复规则，本次仅将原 literal 提取为 `MAX_TEST_SESSION_AUTHORITIES_V2`，不改变它。该来源只在同一 OwnerSlot 绑定的同一 Account/provision 持久 lineage、所有历史 Session 保留且无删除/重置后复用时成立；生产必须从实际批准上下文强核这一约束，不能仅因配置填写 256 而宣称生命周期受限。成立时以此常量 checked-multiply 两个闭合用途得到永久库存界 512，拒溢出；若实际生命周期不能证明该约束则 profile Unsupported，不另造 cap、不 GC 永久 once、不扩大预算。两类签名各自先永久 once 提交、再借 KEK/签名，unknown 不返回成功缓存、不重建 handle、不重试或隐式刷新；同 slot/盘整体回滚未覆盖的边界不变。该后继在真实两个 paid claims/issuer/Actor、独立原件、Close 时独立合格 Current 与实际历史 Stop 条件的顺序，以及物理两次计数共同验证前保持关闭；单独 Stop Vault1 组件不能声明已完成第二次签名或 cleanup23。

Account 不能仅凭当前 Env flag 或既有 `PublicRoleKey` 就断言该 slot 已安装新 Term profile。首个最窄 mounted caller 采用显式额外一条 `OwnerSlotQualificationReadV2`：在同一 owned invocation 内，先从原 Term24 独立持久预扣/claim 一个固定 Owner Actor fact-only 读取，再以实际已存 participant receipt 推导的唯一 Owner scope/key/provision 和配置固定的 Actor binding 发一次请求。该请求不初始化/修补旧 slot、不加载 KEK/解密/签名、不读 Current、不发其它服务；仅返回闭合安装元数据（实际 slot scope/public key、schema/双用途模式、配置 pin/floor digest 与永久库存边界），绝不返回 ciphertext、seed、原私有签名许可或任意 SQL。Account 只从真实固定 namespace/Actor 的已付返回构造本 invocation 的非 Clone/Serde 安装资格 token，完整比较上述实际 tuple；caller JSON、配置 bool、历史元数据或旧 PublicRoleKey 都不能构造该 token。资格只证明安装边界，不保证 ciphertext 可解密、Current 或 World；后续 signer 每次仍独立重核全部模式/config/slot/floor/原期限。

该额外 Actor1 必须实数列账，不能藏入 Vault1 或 Source6：原限定23分支成为24；若还需 Native 转发或其它真实边则超过24，继续 unsupported。缺双用途安装、旧/部分状态、错误配置或 unknown 仍保这一次已付读取/占用，并在 Source6、密钥借用和 World 出站前 blocked；不得免费重查、自动安装旧 slot 或把 unavailable 当通过。接收端该读取使用独立固定 purpose/path/原件与独立永久 once 库存，和两类签名做全库存 paid-attempt 交叉冲突检查；安装仅在新 slot 原首次付费创建的同一事务完成。读取库存上界仍仅在前述实际固定 Account/provision/slot 无删除/重置 lineage 可证明时，来自既有256个 Session 各最多一次；签名库存的独立 checked512界保持原含义，不把第三类读取塞进两签512。普通 cold/整盘回滚未覆盖边界不变。实际首付读取、旧 slot 拒绝、真实无 KEK/外出站计数、首次 invocation 与后续六边/两写完整图未共同验证前保持 disabled。

激活成本独立列账而不是藏进cleanup：ReadAndRegister替换原已付Native OwnerRead1+其World1，不增加旧read；Pending cert随替换原 `RecordLifetime` 的 `RecordLifetimeAndClaimMachineRead` 响应返回；原后续 `ClaimStep(OwnerRead)` Actor RPC 被删除，不产生额外领取 RPC，也不在纯 SQL ClaimStep 中新增观测。新的固定 profile 由拥有实际 Account SQL 和 Env 的同一 AccountAuthority 直接发送 Native Active 请求：Account 在本地事务预扣原 Native160 的恰一次实际 Native HTTP，返回仅进程内可消费的 exact-request token；事务外向固定 Native `activate-v2` 发一次 PoP-authenticated HTTPS POST，无重定向、challenge、retry 或 caller 选择目标，然后重取同一 Account 记录处理 Ack。该新机器请求不用 managed Cookie/SID；注册的人类 Cookie/Origin/CSRF 路径不变，Account 不取得 Identity 的 managed SID/CSRF，也不迁 Native Root 或 CloudOwner seed。Account 不把 cert 先交 Identity 再回 Owner frontend/Account；Ack 在同对象内保存，因而不新增取 cert RPC、self fetch 或两条 Ack Actor 回调。若实际部署适配确需这些应用 HTTP，必须另列真实 funding/once 并保持本 profile 不合格，不能默认为免费。

Active3Roots 证书只从已真实持久登记的三个固定 Root 及完整 top-level 批准原件派生，不将未落库候选、ConsumedPending 或 caller 字段签作 Active。新 profile 明确区分三根内部登记与对外可用：保存三根的同一 Account 事务必须保存 `AwaitNativeActivation` 门，所有执行/read 凭据领取、Host descriptor/Tool 发行及新 machine 当前许可在该门未完成时拒绝；旧 V1 不受这条新门影响。Actual Active cert 绑定这三根的精确 root-birth digest、原 Pending/Native registration Ack 和新的唯一 Active HTTP attempt。它只证明内部三根登记和固定激活动作的预付，不把历史 cert 当新的 World/Source 授权。

Native 成功 Ack 由原固定 Native Root 强签，Account 在唯一发送后的重新进入中同时核 actual 原请求、Active cert/root-birth、registration ref、nonce/attempt、外部 Native floor、最新 SQL record/version/冻结/完整批准/Source 原五秒及自然期限/config-generation。任何已知 tuple、epoch/generation 或配置变化拒绝发布；时钟不能取第二阶段重新开始的五秒。固定配置工厂和实际 Native Root 验证必须来自受信配置，不能从 Ack 取公钥或相信布尔。只有这些当前前提及同一实际原件仍满足时，本地 SQL 原子保存 Ack/floor 并把门转为 Ready，随后既有回复才可公开可领取状态。Native Active 回包丢失、Ack 拒绝、reentry 变化或超时均保持 AwaitNativeActivation/unknown 和原件/已扣费用，不能再次发送同一 proof、偷偷重新签 cert、领取凭据或把三根当已对外可用；未设计独立付费历史查询前只保 unknown。该一次额外 Native1 可明确从160预扣，其余 Native内部World3仍是独立固定 profile 成本/once，不挪入160，也不借 Execution400/Read300/Control/Host/Cleanup 预算。


新明确签入的 `CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2` 激活 profile 对内部 World3 的付款与 once 单列，不把 Native160 的外层8或新增 Active1当内部端口：既有 `ClaimStep(FreshAccessPair)` 在原 Account 事务中保存外层四次 Native 预扣，并另保存已由该新完整批准 profile 授权的固定两项 World 子操作记录：PolicyOriginal 的 `/v1/world/participant-policy` 一次，Authorize 内部同 path 一次。它们是不可互借、不可退款、不可转让的精确阶段端口，不是新的可自由支配 HTTP cap，也不从 Execution/Read/Cleanup/Control/Host 取款。旧 V1、旧 `TermWideFiveSecondOwnedV2` 及没有该新 profile 的 V2 不因启用环境开关、替换 wrapper 或以后添加 consumer 而取得此授权。新 closed variant 必须进入同一个 TestSessionPlanV2.termination、完整 intent/plan digest 与新鲜 Native FullMFA 签名原文；旧签 fullproof 不因重新计算外层摘要而升级。现有 server basis producer 固定旧 profile，不能声称加入 enum 已可完成新批准；须在既有 `TestSessionBasisRequestV2` 增加显式 optional、None 省略序列化的 closed `cleanup_transport_policy` 选择器。None 逐字节保留旧请求/旧 basis，只有显式 Some(World3) 才由同一次原 metadata RPC 产生新 term/scope/basis digest；它不消费 MFA、不安装 grant/端口、不发行证书。prepare 与 admission 必须按实际完整签入 plan 重建该选择与 server basis，拒绝旧保存 Book/Fullproof 被外层请求或环境配置升级；新 profile 仍须一次真正新的完整 fresh-MFA，并沿原消费事务进入零 Roots。不能要求 Execution Book 来满足这个管理读取。原 OwnerRead 的第三项 World 仍由 RecordLifetimeAndClaimMachineRead 的独立原件与端口覆盖，不重复计算。

此处的 Book 是人类 fresh-MFA 的 ApprovalAdmissionBook，不是 Execution 的 `execution_book`；FreshAccess setup 阶段后者可以仍为 None，不能要求尚未建立的执行 Book 造成循环。

同一既有 ClaimStep 回复可返回新的 closed `SignedAccountPreflightAccessV2`，而不增加一次 Actor/Native RPC。签发前核实际 `TestSessionAuthorityV2.book.original(session_id)` 的 `ApprovalAdmissionStage::Consumed` 记录、该记录中的实际完整 `ApprovalAdmissionInput` 与 top-level intent/plan 原件，以及原 FreshAccessPair 四 Native 预扣、同一 Source 强签与原五秒、准确 ParticipantSession/原 Native binding/批准 input/plan/TermWide policy、配置 issuer/purpose/floor 与期限；不能把尚未发生的 OwnerRead/LF 伪造为 Pending。证书以新独立 `oasis7.custody.native-preflight-access.v2` 域和 `NativePreflightAccessV2` 目的绑定两项稳定子操作 ID、各实际请求 body 摘要、原 Policy/Authorize nonce、相同账户/会话/Source/完整 input 与原绝对期限。每子项只允许一次真实 World HTTP；原件重放仅拒，不返回成功 cache 使另一请求免费。费用记录与 Native claim 不能在后续观察失败时回滚。

Identity 保持原 SessionStatus 两次和 Cookie/Origin/CSRF；仅以两个显式新固定 route `/api/public/hosted-account/v2/participant/test-session-v2/policy-original` 与 `/api/public/hosted-account/v2/participant/test-session-v2/authorize` 替换相应旧请求，实际外层 Native HTTP 仍四次，不追加 probe。新 Native consumer 必须先从同一受保护 lineage 的已完成 FullMFA 原件重建完整批准，并按配置固定 Account issuer pin/floor 强验证；真实 managed Cookie/CSRF、当前凭据/账号/绑定与原 kernel 权限不变。先在外部 guard 保护的本地事务中持久消费精确子操作的 physical-attempt，再调用原 policy/authorize 内核，网络在 shared mutex/SQL 事务外且只一次；回锁后重核完整安全 tuple/期限，再由配置固定 Native Root 签发新域 physical receipt；保留原 typed policy/admission 原件，并绑定完整证书摘要、子操作/attempt/body 摘要与实际结果摘要，不能只返回可与旧普通路由混用的原格式结果。Identity 在既有 GetPermit Actor RPC 内携带两份实际 receipt，Account 同事务强核 Native 固定 Root、精确子操作与原件一致并归档到原阶段记录后才允许 OwnerRead 签名；Pending producer 同样要求这两份真实 receipt。未消费新 profile 的旧普通响应不能填充此字段，不新增一次回执 RPC。只消费后的一次观察失败、replylost、重启或重复均不重发/退款；任何未实现的消费/预扣/floor 接缝保持新路由零 World 出站拒绝，不把现普通路由的 quota 当新 profile 已付款证据。


可实现顺序：先使新 Native 注册/current、Account cert producer/external-floor、OwnerVault fixed receiver 和每出站 once 记录成为实际消费者；再用 mock HTTP/真正 workerd alarm 探针核定内部 lifecycle 是否有额外物理边及 paid 归属。每一步强拒过期/重复/回滚，不为凑数改变 RF 或 scope。新23图以真实同对象alarm消费者和不额外queryStop为前提，必须实际源/HTTP计数、双unknown真World Closed、cold restore/current-admit矩阵全部通过；任何额外边/付款缺口未闭合前 dispatcher 关闭，不把此候选写成整体完成。

<a id="cloud-custody-n-inference-budget"></a>
#### N 受限推理预算与实际费用图

Trace refs: O7-REQ-26945890-45d8-4be4-81af-5fe82c85aede；O7-REQ-f98670c4-c1fd-4610-8884-8eb2cf89c1eb。

**原件、摘要与 null**

TestSessionPlanV2 和 OwnerSetupPlanV2 增加显式必填 nullable `model_budget`。Rust Option 必须使用 required_option 反序列化，缺字段拒绝，不能仅无 default（serde 的 Option 缺失仍会变 None）。null 只表示没有推理授权，任何 V1 或旧 V2 缺字段都不能升级。Some 必须通过 plan.validate/digest、实际 prepare→Native custody options/get/complete→Account approval 验证，Native 人可读批准摘要显示 N 身份、固定供应商/model、调用/token/价格上限、串行并发、绝对期限以及两次探测计入同预算；只把 opaque digest 塞入 JSON 不算完整摘要。

同一既有 V2 的 action selection 明确分两类：精确 `Move { position_cm }` 的 business 0 分支，或原真实 ResourceBasis Transfer/Build/Schedule 分支。不是新增宽泛 ControlOnly 授权，也不将 Move 塞成 ResourceBasisSelection/空 Transfer。仅精确 Move0 要求 allowed_resource_refs=[]、business_limit=0、唯一 action 逐字段等于批准坐标，prepared_resource_basis_digest 显式 null；Resource 分支保持原非空 refs、真实 owner/grant/ledger/quote/basis 约束且该 digest 必须为真实 Some。missing nullable 字段拒绝，不以默认值升级既有 V1。

两类均须真实 V2 OwnerSetup 四阶段原件/当前强验、独立 fresh-MFA、TestSession consume/preflight/三 sibling roots 激活。不能复制 V1 Prepared 状态当 V2；Move0 仅免除不存在的资源 business basis，不免当前 owner/goal/delegation/fence/Source/Native enrollment。模型在该获批 Move0 会话内只获得独立批准的推理/读取，不能扩 actions 或由推理预算获得执行权限。执行 Context prepared/captured projection 是 roots 激活后的付费阶段，不能循环用它当激活前提或免费创建 context。

`ModelInferenceBudgetV1` 内的 mandatory `approval_contract=native_custody_model_budget_v1` 表示本版真实 UI 摘要与 Native custody UV 合同，并纳入签名原件；旧 opaque Some 记录缺失该标识时失败关闭，不能后补 UI 后静默升级。资格不是 caller rendered/verified bool。预算原件的版本标识本身也不构成 UI 资格：可用 N cap 还必须匹配 Native 独立签名 proof 中的 `model_budget_summary_contract`（固定版本与 exact intent/context/budget 的摘要）。该字段只能由 Native 已实现的固定摘要合同生成并持久在本次 ApprovalRecord；旧记录 None 重送不补，后续 UI 完成也不能静默升级。本版 Native 固定 UI 合同支持开关关闭时仍仅记录 setup 批准事实，Native proof 的该项为 None，不能升级为 Ncap。两次 probe 与每次 inference 的物理 HTTP 上界（各含 H→Account reserve 与所有授权边）也必须显式签入、实际匹配已知完整图；图摘要或用户输入数值本身不证明计费正确。真实 client 在 credentials.get 前必须呈现 target/固定 preset/N/预算/价格/图成本/绝对期限并取得显式确认，缺摘要/错绑定/账号代际变化不能调用 get/complete/approve。

Native 的 `OASIS7_CUSTODY_MODEL_BUDGET_SUMMARY_V1_ENABLED` 默认关闭，仅声明该部署实现固定 Native/UI 摘要合同，不是请求方 `rendered=true` 或远端 DOM 遥测证明。仅新 ApprovalRecord 可固化精确 typed summary contract；旧 None 记录重送不补。实际账户 surface 提供显式 V2 原件入口，仍使用同源 CSRF、固定 endpoints、Rust 返回原件；预算摘要须与原件逐字段相同，显示 target/N/preset/calls/tokens/价格/全图物理 HTTP 上界/期限并显式确认之后才 `credentials.get`。取消、账号切换、摘要缺失/错配或确认时已过期在 get 前拒绝；complete/approve unknown 始终保原正文。该 UI 不自动构建/升级 V1 Move0 plan，不执行 continue，不产生 N capability；完整 graph/cap/activation 另行强验。

`ModelInferenceBudgetV1` 最窄字段：api_version=1；独立 N Ed25519 public key、Native account binding、N process instance public key；preset id/revision/model；ledger_id；runtime/Skill/tool/context policy digests；physical_graph_digest；max_calls（包含两次真实 probe）、max_input_tokens、max_output_tokens、max_total_tokens；max_concurrent=1（其他明确 Unsupported，不能静默下调）；deadline<=write_deadline；price_policy。全数值采用 DecimalU64，checked 算术，禁止浮点。价格候选 `price_policy` 固定 currency、input/output 每百万 token 的最大整数微单位、max_total_micro_units、quote_digest/valid_until；该原件为用户批准的价格上界，不声称是真实账单。具体 preset 当前价格必须由可信 N 预设/运维固定报价产生，M 不得提供；实际服务若不能提供可验证 usage/单价映射，保守保留最大金额且不能宣权威货币结算。不同缓存价/工具费/批处理费等未枚举收费模型不支持；不能套通用 token 单价偷偷启用。预算的 all-in 供应商价若无法精确界定，该 profile 不出站。

**N→H→Account 原子预付与远端 floor**

新增 N 专用独立 IPC，不扩 M 五工具。serve 的 trusted H 配置固定 N peer、本次唯一 approved session/budget/ledger/preset，实际 kernel N UID 非 root 且与 H/M 互异；远端 N 使用独立固定认证协议而非比较跨机 uid。N 独立签名域 `oasis7.custody.model-graph-reservation.v1`，payload 固定 approval/plan/budget digest、N instance key、ledger id、strict next sequence、attempt id、phase(probe1/probe2/infer)、immutable request digest、graph digest、最大 token/价格预留及绝对 deadline；不接受 raw body、URL、H token/key 或 caller verified bool。

H 核实际 N peer+签名和 root registry 后，用自己的 Host possession 调 Account 专用 reserve 路由。Account 验 H 现有权限及 N 签名、真实已批准 V2预算，原子扣本次整个已知图（包括此 H→Account reserve HTTP）到原 Exec400，同时扣 whole-session call/token/price 最大额，记录不可回退 sequence/floor 和 immutable original digest。相同 attempt+完全相同 payload 返回同已保存事实；不能因此再授一次 dispatch。不同 payload 冲突 whole-state 拒绝。floor 由 Account durable state 决定，N 新路径/request id/重启不能重置，H/M 无预算初始化权。

Account 返回固定服务签名图收据（必须通过既有真实服务签名链核验，不另造布尔授权）；N 必须拥有 exact graph/receipt/floor 且 persist 本地预留后才能创建不可 Clone、不可 Deserialize 的 ActiveAttempt。本地 missing/corrupt/below floor/claimedunknown 不重建句柄；N process instance key 是附加防重启绑定，不是 VM rollback 锚。已在途 reserve 回复丢失只恢复原事实，不自动 provider dispatch；未知保留最大 call/token/price/并发占额。没有 finish(None) 释放外层额度。禁止以 N 自报 usage 退 token/金额。

每 edge 在 actual dispatch 前本地 durable once claim，记录 exact canonical request digest/phase/ordinal/issuer/target/pins/deadline；reserved、claimed、observed、authenticated-result 分开。Account 图预付不是 actual HTTP observed。重复 edge/图外 target/不同 body/过期在出站前拒绝。无 retry/redirect/fallback/未知图出站；失败已扣不退、原期限不延。Account floor 与不可反序列化 ActiveAttempt 保障正常重启/N 文件系统回滚失败关闭；完整 VM 加内存快照回滚不在已 qualified 范围，不以 fsync 自证。若增加 recipient once API，其额外 HTTP 必须纳入图，不能递归隐藏。

**真实费用图与实现前置**

不能把 normal SessionModelProvider 五次 BindingAuthority + provider 简化成固定六。candidate/active_preset 在 provider send 之前已有 authorize，因此外层 gate 必须早于 candidate、connection create 和 probe。普通 Executors / SDK devices 不能作为新 custody authority。

现 Core test_session_live_context.rs 是 paid component：IdentityNative、NativeWorldView、NativeWorldLive 三类 port；Descriptor 明确 unsigned/not dispatch permit，实际 trusted RNG/network consumer 尚缺。实际 Native `EntryExecutorOperationV1::ReadLiveContext` 已有接收验证与 World gateway bridge，但这不是 Worker paid NativeContext 的 mounted consumer。新的 CustodyCurrentAuthority 要实际接 Identity paid context source/history/lifetime/Current → Native gateway/cipher → World view/live → 回包验签/归档/准入；每实际服务 HTTP/网关/cipher RPC 都有唯一 edge。需先精确查此完整分支及 Account reservation issuer 的实际签名出口，不可先手填 graph digest 或把 incomplete profile 支持为 true。

**Some 的真实 N 注册与固定 proxy 消费者**

现 Native full-session/model summary 是用户同意原件，不是实际 N instance、preset、报价或完整网络图的资格证明。可用 proxy 前，Native 必须从服务端已有用户专属 provider key 连接和固定 preset 配置产生独立签名 basis：绑定 actual Native account、当前 process instance public key、固定 preset id/revision/model/HTTPS endpoint policy、当前 all-in 价格上界、runtime/Skill/tools/context policy 与完整已知物理 graph。Native basis 不返回 provider key、SID 或原始私密配置；连接或报价/图不满足原合同则不产生可用 basis。首版 basis 由现有 Native custody options 的本地固定 producer 内联产生并持久到本次 ApprovalRecord，不新增免费 basis HTTP，不访问 provider、不建立连接、不探测。UI 只从现有 capability/options 合同取得公开 N instance/preset/报价 metadata，provider key 的用户专属解析留在 N；若后续新增 metadata HTTP，必须先明确其实际已有费用归属，不能偷塞原执行图。用户批准后的 whole-session header 必须精确匹配该实际 basis；旧 Some 或旧 summary 不能在补齐服务端 basis 后静默变成新 cap，必须具有本次实际 basis 的原件关联与 full-session fresh-MFA。

现有 `ModelBudgetApprovalContractV1` 保留 `NativeCustodyModelBudgetV1` 为旧摘要 consent-only，新增闭合 `NativeCustodyModelProxyV2` 才可申请本节 proxy；不是新增 Plan 版本、budget lane 或平行 capability。Native options 对后者必须先从 actual N 本地服务端 producer 建立 immutable proxy basis，再创建本次 ceremony。basis 以 exact full intent digest、完整 budget digest、ledger_id、Native account/instance public key 和 actual Native 私有连接 generation/ref digest 关联本次 ApprovalRecord；不是请求方传入的 basis digest、ready 或 verified bool。Native complete 在签名之前重核该同一持久 basis、本次实际 session/account、N instance 和连接代际/报价/graph 仍相符；任一缺失或错配不得签出 proxy 版本证明。旧记录不存在 basis 就保持不存在，重送不补。Account 可用 N 预付 producer 必须同时核 actual activated V2 header、该闭合新 contract、对应 Native full-session proof/完整 budget 摘要和已知 graph；NativeCustodyModelBudgetV1 的旧 Some 永不变成 Ncap，即使后来 preset/catalog 恰好匹配。None 仍没有推理权，Mrun 仍不启动。

Native proof 对新 producer 使用闭合 `FullTestSessionProxyV2` summary variant，承载原 `NativeFullSessionSummaryContractV2` base 和 actual `NativeModelProxyBasisV2`/basis digest；不是在 prepare 之后改写用户 header。Account 的 summary 语义检查先逐字匹配原 full-session base，再核 basis 的 intent/context/budget/ledger、N account/instance、固定 preset/model/price/graph 和本次时间绑定，并用原固定 Native Root pin 强验完整 proof。`expected_model_budget_summary_contract()` 仍是纯 header 内容基准，不能直接声称产生实际 basis；旧 full-session variant 不等价于新 proxy variant。provider key 属主按实际 Native provider 连接的 account/session/generation 核定，不能把目标 managed Participant 的 scope.native_account_id、SID 或经理 session ref 自动当作该连接的拥有者。私有连接引用仅以不可逆 commitment 留在 basis，key/SID 原件不出 N。

首版 Local N 的自动启动只以真实 Native 注册 `Active` 首次 commit 作为本地调度信号，不把 Cleanup/Active 证书、Ready metadata 或历史登记当作 N 权限。信号仅从该真实 Active row 所存完整 pending input 与 active input digest/session 反查，并复用 Native 已完成 ApprovalRecord/full-input 验证；None、旧 Some 或不是本次闭合 Proxy 合同不排队。原 activation SQL 已成功 commit 后才能通知有界本地调度器；原 Native handler 持锁期间不等 socket、不读文件、不做网络，不增加 Native HTTP 或普通 SDK callback。调度线程只持 Weak Identity，锁外等预配 H Local socket（其建立要求实际 Ready/Execution claim），随后才由实际 current owner/basis 工厂生成并永久提交首个随机 N query，再沿专用 H IPC 发送。query 工厂仍须先满足本节 N 独立批准、真实 provider owner/instance/ref/generation、已安装且能完整枚举实际收费边的固定图 driver 与合格价格；图的 wire 描述或 Ready metadata 均不证明该 driver 已资格。固定 Probe 正文可由本地既有 encoder 实际产生，故首 query 可先绑定该已知正文以请求原图预付，再在其原期限内取得强 Current/Captured/floor；不得把这些后置结果倒置为首 query 的前置，也不得为未知 inference 正文填未来 digest。H Execution PoP、Account 实际消费/原图预扣、后续 Current/Captured/floor 仍各自独立强验，调度信号不替代其中任何一项。缺 query 生成前置时保持0 query/provider IO；后置 Current/Captured/floor 未完成时保持0 floor/provider IO，不因 Active 成功启动 M 或跳过两次 probe。每 N 进程只绑定一个本地 whole-session input；重复通知不造第二线程/nonce，不同 input 不自动重绑。进程退出、线程失败或历史 Active 重读不补建 dispatch handle；原远端 floor、Native lineage与旧实例约束继续适用。

首个 N proxy 只支持批准的 max_concurrent=1 与固定 closed profile。M 只获得一个绑定本次 session/runtime 的受限 inference socket/call，不获得 provider key、Native SID、H key/token 或可选择 endpoint/preset/价格/预算的参数；H 的五个 Control 方法不新增 inference 方法。远端 N 路径必须用独立固定认证和 scope 绑定，不能把本机 socket UID 数字拿去比较远端，也不能因为 M 在本机就给它 Native 登录态。trusted launcher 的本次模型生命周期结束、原 Session Closing/绝对期限或配置代际变化均停止新 dispatch；已收到的晚结果只作历史事实，不能继续授权工具或新模型调用。

实现复用现 ModelProvider 的 ProviderPreset、请求编码、响应/usage 解码与 ModelTransport HTTPS-only/no-proxy/no-redirect/no-retry/remaining deadline 原语。普通 SessionModelProvider/ConnectionManager 的 memory budget、SDK BindingAuthority、普通 activate/probe/finish 语义不改，也不能给 custody 伪造一个 SDK BindingTarget。新的 Native custody adapter 在 candidate、连接创建或 probe 之前消费前述 H→Account 全图原子预付；所有 network-authority 调用须来自已枚举的 paid Current/Context/LiveRead graph，不能由普通五次 authorize 隐式新增免费 HTTP。底层 transport 仅借用 N 内部 key，actual dispatch 的不可反序列化 ActiveAttempt 必须已持久核销相应 edge，重复 key-borrow/local check 本身不制造新的网络授权回调。

同一 Account durable floor 统管整个获批 ledger，而不是每个 N 路径或 request ID 一个新预算。两次真实 probe 和推理共用原 calls/tokens/价格与 Exec400，首次 H→Account reserve HTTP 本身也在 graph 内。N 本地缺账、低于远端 floor、重启后 instance 不匹配、claimed unknown 或 reserve 回复丢失均不重建可派发 handle；只可恢复原事实，不能自动重试 provider。未知保持最大 call/token/金额/并发占额；已知真实 TLS response 的 usage 由 N 固定 adapter 绑定 attempt/request/response digest 与 approved price policy 后归档，不能接收 M totals。首版可保守保持已扣最大预算并另记已认证 usage，不凭 usage 退 call/token/金额，不声称该价格上界是真实供应商账单。


首个 reserve 的 H possession 使用独立闭合 `NativeModelCustodyV2` RFC profile，固定 tag `oasis7-custody-native-model-v2` 与固定同部署 public-origin scoped `/v1/account-provisions/{provision_id}/accounts/{custody_account_id}/client/test-sessions/{session_id}/model-reservations` target；该内部 H/N 通道不新增 M 的五个 Control 工具，也不扩普通 broker。旧 Custody/Control/Claim/Actions profile 不接受该 route，新 profile 也不接受其它 route。它仍使用本次实际 Execution credential 和 H 登记 key，不能用空 token。首次 reserve 的 nonce 来自本次获批 N instance 的真实新随机值，并由 N 独立服务签名连同 exact session/input/budget/basis、固定 phase/graph、稳定 reservation ID 和五秒 admit_until 一起提交；Account 先以已批准 Native basis 中的 N instance key 强验该 typed query，再将此 nonce 用于 H RFC 验证。这里的 N 请求只绑定本地固定已知 graph/合格价格及原批准范围，不证明 Current 或授予 provider dispatch。N 本地 peer 与 H 私密请求入口必须先固定 binding；M 不提供此 nonce、请求正文或 reservation 参数。Account 持久拒重复 nonce/变正文，精确已登记请求只能读取原事实；不得为此先调用普通 ToolMetadata 或 Actions nonce HTTP，也不得把 M 自报 nonce 当服务端 challenge。N/H 本地 producer 缺少实际 server-owned N request、N 新签名或 H Execution PoP 时不得发出首次 reserve 请求；已进入正式 HTTP 的无效请求只拒后续 Source/Current/RKV/provider 出站，并如实记录其已经发生的入口前缀，不能宣整个请求 0 I/O。

本机 N→H 首版固定为独立 `native-model-custody-v2` Unix IPC profile，只接受完整 `SignedNativeModelReservationQueryV2`，不复用 M 的 Control socket/parser。H 的 native serve 命令仍由可信 H 通过私有 stdin 解锁本机 vault；N 的固定 reserve-call 客户端只传公开已签 query、接收公开 Account pending/原件，不接 wrapping key、Host token/key、Native SID、provider key或任意 URL。H 校 kernel N UID 非零且与 H/M 互异、当前 runtime pins 和固定 session/Some 新 contract；还须以本次 plan 的 N instance key 核 query 强签、固定 graph/budget/期限，再由自己的 Execution token+登记 key发原固定 public route。H 的公开 plan 不替 Account 所存完整 input/Book/Native basis；最终授权与预算仍由 Account 原强验承担。本机客户端不得用于 Remote placement；远端独立认证 profile 尚缺时明确不支持，不比较远端 UID。

本机 N profile 使用单独受 H 持有目录/inode约束的 socket及公开 sidecar，M 描述符不返回该路径；内核 peer 验证不可用 sidecar 字段或文件0600替代。每次请求和空闲轮询同时强验原固定 root-owned M binding及新增固定 `/var/lib/oasis7-custody/native-bindings/{native_uid}.json`（macOS 对应 `/private/var/lib/...`）的 N binding，两者均 exact identity/descriptor/session/resource/restricted-runtime/isolation tuple，N record固定新 profile；缺失、改绑或期限结束停止新请求并退休本 socket。记录仍只是当前 OS 路由约束，不证明 Skill、旧同 UID 进程已退役或模型/完整命名空间隔离；可信 launcher须在重绑前真正退出旧 M/N 进程，另作真实 OS 验收。此 IPC 不增加 HTTP graph叶；真正 H→Identity 后仍走正式 limit与单 Owner/Account 的五边结算，不扩 M 五工具、不接普通 Executor broker，也不绕过 Native认证。

该首请求 admit_until 仅限首次预付准入；它不是 provider dispatch_until，也不能代替随后真实 Source。Account 预付后才按已付 closed graph 一次收集 Source/Current，原 Source observed/expires 与原批准绝对期限贯穿全部后续阶段，不重取 Source 延长五秒。预付返回值只有固定 paid-pending 图与 once claim 元数据；没有已强验完整 Captured/current/供应商价格资格就不签可派发 floor。事件和全图费用分别引用真实服务端 ingress、独立 N query 原件及各已付物理边，不把 caller 填写的 ports/units/ready 作为收费或权限真值。

N 原子接受预付与一般 Closing 的关系按会话内 ordinal 精确区分。只有真实 Account 同 SQL 接受某个稳定 reservation、已将其完整已知固定图和 whole-session 最大 call/token/金额占额永久保存，且恰由该事件耗尽 Execution transport 时，才可保存该 reservation 的 BudgetExhausted completion 边界；不得凭相同毫秒、调用方 reason、普通 paid receipt 或历史签名推断。后续只允许这一原事件之后尚未派发过的已预付 exact edges，并须逐一核原 reservation/edge/nonce/完整原件、原 Source 五秒/Root/Native/current/供应商期限和独立 once；它不允许新 reserve 或改变已冻结 graph；仅该 accepted reservation 既付图内按固定 typed subphase once 创建的首次 Source 与首个不可克隆/不可反序列化 opaque attempt 可以继续。首次 Source/attempt 的创建必须永久引用原 accepted journal ordinal、完整 immutable graph commitment、确切首个 subphase/已付 edge 和当次完整 fresh checks；它们不是 replacement 或续权。原件缺失、已创建过、claimed-unknown 或重启丢失句柄均不补建。禁止 replacement/refresh Source、第二 handle、renew/reconstruct/retry，不退款或延钟。首次 Source 的原 observed/expires 一旦保存便固定，后续严格沿原五秒界；整个图还受 accepted 时冻结的原 absolute until，不能在首次或后续 phase 重新起钟。任何 Finish/Abort、Deadline、LeaseExpired、撤销、freeze 或其它 Closing 原因均禁止新 dispatch；先前已发 HTTP 的晚结果仍仅为历史。预算耗尽之外的 Closing 不可被该 completion 边界覆盖，实际 Current/Captured/价格资格仍缺失时也不能出站。

该边界必须由真实最后 N 个可用 transport 的正控、N−1 whole-state/cost 不变拒绝、同 reservation 重放与变原件拒绝、BudgetExhausted 后新 reserve 拒绝、Finish/Abort/Deadline/Lease 后原操作剩余 edge 拒绝证明；严格恢复重放同唯一 journal ordinal 和对应 typed records，不能以当前剩余额度或时间排序制造许可。若该已接受操作边界尚未完整实现，该 profile 保持 disabled，不采用一般 Closing 豁免。请求正文承诺按闭合阶段区分：固定 probe 正文在 N 已知，首 query 可携 `EncodedRequestV2` 实际 ModelProvider 编码 digest；inference 只有已具备实际已付且强验 Captured prefix 时才能同样编码。若 reserve 的目的正是预付尚未取得的 Captured/privateContext，则 query 必须为 `AwaitingCapturedV2`，仅绑定原已批准公开 context-policy/graph commitment，不填占位 hash、未来正文 digest 或调用方 ready。该 pending 阶段不作正文授权、不产生 floor；只有原既付 graph 收集了实际 Captured 并在 N 真正编码后，才用独立已付 `AdmitEncodedV2` query 把实际编码 digest 强签并绑定进 Account floor。Admit 所经固定五边入口（下文定义）及其它实际边均须在原冻结图中另列并预付，不能免费 callback、改 graph 或刷新原 Source 五秒。N 在 once claim 与 key loan 前用同编码器重核 floor 的实际 digest；仅 commitment 出 N，provider 请求正文、私密 context、key/SID 均不入 Account。缺 actual Captured producer 时当前 inference 不产生 query 或任何出站。


首 reserve 复用既有 ToolMetadata/Prepare 的“真实入口先结算、后续出站先预付”合同，不能把已发生入口称为事前预付。保留正式客户端入口的可信 provider IP 与 realm 滥用限流；`test_session_client_entrypoints::handle` 先调用 `limit_client_http`，其 `abuse` 对 provider 与 realm 各向固定 CLIENT_ENROLLMENTS Actor 发出一次请求。因此成功到达 Account 首结算的固定路径实际为五条边：Host→Identity HTTP、Identity→provider abuse Actor、Identity→realm abuse Actor、Identity→Owner service、Owner→Account Actor；不得跳过原限流、隐藏其两次 fetch 或借其它预算。提前被限流、正文或认证拒绝的请求只报告其真实已发生前缀，不伪称已完成五边；未认证请求不能扣合法 session 预算。Account 不初始化未知账号、不做 Native/Current/Source/provider 出站，不返回权限；先只以本地已存 actual V2 input/Book/三根/当前 Execution key-token 强验 N typed query 与 H RFC 原件。成功后第一独立 SQL 以可信服务 ingress 身份固定结算这五条已发生边并永久保存五份 exact typed route/原件关联，不能接受请求方 units、route 或 caller bool。同 ingress 幂等，新 ingress 即使签名/业务原件相同也独立计费；后续图预付失败、unknown 或 current 拒绝均不退款。五边必须由实际 mounted 公开入口、两个 abuse Actor fetch 和两条 named service/Account Actor 路线的 spy 证明，不是把父 RPC 标量伪作整图。

第二事务才原子占 whole-session call/token/金额最大额并预付冻结图中尚未支付的 Source/Current/RKV/provider 等实际边；首五条及原已付 Captured/Book prefix 按原记录精确引用，不再重复扣，也不能把 reserved 当 observed HTTP。N−1 的 whole-state/cost 拒绝正负例以首五费已成功 commit 后的实际 state 为基线，只证明剩余图/最大占额的原子性；不得把 HTTP 之前 snapshot 与失败后相同当作“已发生5边免费”的正确结果。另需真实首五费余额不足的目标：扣费事务原子拒绝、绝不补负账或提高 cap、无任何后续 Source/Current/RKV/provider 出站；五条前置路线仍如实列为已发生且该请求失败，不能宣整个请求0I/O。旧 ToolNonceThree 名称与三费常量不能充当新入口五边的证据；必须按实际新入口分别记账。现有事务分离实现参考 `test_session_v2_tool_nonce.rs` 的 ToolNonceThree 首费与固定内部两 route marker，以及 `test_session_execution_prepare_current_store_v2.rs` / Prepare 工厂的首独立SQL与 fallible Current 分离；新 profile 必须有独立 typed records/journal/reverse，不能借旧 Actions nonce 或盗用其免费 admission。

首支持图冻结 probe1、probe2、infer 三个 phase，各图具有明确 schema/边列表/有界可选分支，unknown branch 不发送。两次真实 probe 在同预算中预付 call/token/price+Exec400，再执行 create/probe/candidate；生产不能 skipprobe、mark ready 或 fixture ready=true。供应商固定 HTTPS origin/path/model/max output/no proxy/private DNS/no redirect/no retry，clip remaining deadline；截止禁止新 dispatch、迟到结果仅 archive，不承诺远端立即停止计费。


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

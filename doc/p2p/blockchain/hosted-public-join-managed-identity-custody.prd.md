# oasis7 hosted_public_join 托管身份、托管密钥与受限执行

- 对应设计文档: `doc/p2p/blockchain/hosted-public-join-managed-identity-custody.design.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

审计轮次: 1
建档日期: 2026-05-18

## 目标
- 为 `hosted_public_join` 建立一套正式的托管身份 / 托管密钥产品规格，解决“普通玩家如何登录并长期使用”而不是继续停留在 preview 会话与浏览器本地私钥。
- 统一邮箱登录、托管 player signer、step-up auth 与自托管升级路径的规格，供 runtime / viewer / QA / LiveOps 继续实现。

## 范围
- 覆盖 hosted player 的账户身份、设备会话、托管签名、风险分级、自托管升级与相关 trust boundary。
- 不覆盖 node / validator / governance signer 的生产托管；这些继续由既有 custody / governance 专题负责。

## 接口 / 数据
- 主文档: `doc/p2p/blockchain/hosted-public-join-managed-identity-custody.prd.md`
- 设计文档: `doc/p2p/blockchain/hosted-public-join-managed-identity-custody.design.md`
- GitHub Issue/Project task truth: GitHub Issue / GitHub Project
- 关键主键: `hosted_account_id`、`player_id`、`device_session_id`、`signer_ref`

## 里程碑
- M1 (2026-05-18): 专题 PRD / design；任务追溯写入 GitHub 建档，冻结 hosted account、邮箱登录、device session、托管签名与自托管升级边界。
- M2: 落地 hosted account 与 device session，清退浏览器 `localStorage privateKey` preview debt。
- M3: 落地 custody sign API、step-up auth 与 external wallet bind / transfer-out。

## 风险
- 若把 hosted account 与高风险出签混为一步，玩家体验和风控都可能失衡。
- 若继续让浏览器持有长期 player signer，任何缓存泄露都会放大为账户接管风险。
- 若把 player custody 与 node/governance signer 共用一套 trust domain，会让产品问题和协议级 custody 问题互相绑死。

本文 §1～5 定义 hosted player 的生产身份与资产托管目标；[§6](#dev-test-accounts)定义供外部 Agent 使用的开发测试账号工具，对应 [#5076](https://github.com/nil-studio/oasis7/issues/5076)。两种用途分别交付，开发工具只采用本节的能力与验收要求。

账号接入的推荐方式、多凭证管理和邮箱受限恢复已由[免邮箱多凭证 PRD](hosted-public-join-key-based-onboarding.prd.md)承接；本专题继续拥有资产 signer / custody / step-up 与自托管升级合同。多凭证实现候选不升级当前技术预览或生产 custody 口径。

## 1. Executive Summary
- Problem Statement: `hosted_public_join` 已支持 hosted account 邮箱登录 broker、`device_session + in-memory session key` 恢复链路，以及 env-configured SMTP 邮件投递；普通玩家不再需要手抄浏览器本地私钥，也不必依赖 repo-owned preview code 才能回到同一 `player_id`。但这套实现整体仍处于 `limited playable technical preview` 边界：`crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_strong_auth.rs` 依然依赖 `OASIS7_HOSTED_STRONG_AUTH_*` + `approval_code`；`signer_ref`、managed custody sign API、冻结/恢复策略和 self-custody upgrade 还没有进入正式后端 contract。这些缺口补齐前，`public join` 仍不能被宣称为生产级 hosted wallet / custody 方案。
- Proposed Solution: 为 `hosted_public_join` 正式冻结一套 producer-owned 的“托管身份 + 托管密钥 + 自托管升级”目标态。默认玩家路径改为 `guest -> hosted account -> managed player signer -> optional self-custody bind/transfer-out`：用户通过通行密钥或已登记邮箱证明登录，浏览器只拿 `player_session` 与设备级短期密钥；长期玩家 signer 留在服务端 custody plane，生产目标由 KMS/HSM 或 KMS-wrapped sealed-key backend 保护，并通过 step-up auth + policy engine 出签。开发测试账号工具服务隔离测试环境，不代表上述生产托管目标已达成。
- Success Criteria:
  - SC-1: `hosted_public_join` 必须提供“不输入原始公钥私钥也能开始玩”的正式登录路径，默认推荐通行密钥，邮箱为可选兼容因子；账号与凭证生命周期由[免邮箱多凭证合同](hosted-public-join-key-based-onboarding.prd.md)承接。
  - SC-2: 浏览器在 `hosted_public_join` 下不得再持有长期资产 signer、node signer 或可长期复用的明文私钥；旧 `localStorage privateKey` 残留必须被清洗而不是恢复到运行态。
  - SC-3: 玩家身份必须拆成 `hosted_account_id`、`player_id`、`device_session_id`、`signer_ref` 四类真值，不再把 `player_id` 直接当成完整账户体系。
  - SC-4: 高风险动作必须支持 `managed custody sign` lane，并由 step-up auth、风险策略和审计日志共同放行；`main_token_transfer` 不得再停留在“blocked，但没有目标方案”的状态。
  - SC-5: 玩家默认不需要保存或输入原始公私钥；如需自托管，必须走显式的 `bind external wallet` 或 `transfer-out to self-custody` 流程，而不是把托管私钥直接回流到浏览器。
  - SC-6: 本专题必须与 `PRD-P2P-023 hosted player access session auth` 和 `PRD-P2P-017 signer custody` 形成清晰边界：只覆盖 hosted player identity/custody，不覆盖 node、validator、governance signer 的生产托管。
  - SC-7: `doc/p2p/prd.md`、GitHub Issue / GitHub Project 与本专题三件套完成映射，后续实现任务可直接挂到统一真值。
  - SC-8: hosted account 中心化服务必须冻结至少 `dev/staging/production` 三层环境边界；登录投递、账户存储、强鉴权 signer、风控阈值、对外 claims 与验证命令不得跨层混用。

## 2. User Experience & Functionality
- User Personas:
  - `producer_system_designer`: 需要将“邮箱登录游戏”和“链上身份/签名”整合为不损害安全边界的正式产品方案。
  - 普通玩家: 希望点开公开 URL 就能试玩，升级到正式玩家时只做常见登录，不维护裸私钥。
  - `runtime_engineer`: 需要知道 runtime 应该信任什么身份断言、什么 sign API、什么 step-up 结果。
  - `viewer_engineer`: 需要知道浏览器本地还能存什么、登录/重连/升级如何表达。
  - `qa_engineer` / `liveops_community`: 需要知道托管账户、托管签名、撤销、恢复和事故处置的 pass/block 标准。
- User Scenarios & Frequency:
  - 新用户第一次打开公开 join URL 时，需要先以 guest 方式进入，再升级为 hosted account player。
  - 老用户换设备或清缓存时，需要通过可用的已登记通行密钥或邮箱重新登录并恢复玩家身份，而不是找回私钥文件。
  - 玩家第一次执行高风险动作时，需要通过 step-up auth 触发托管签名，而不是暴露托管私钥。
  - 玩家后续若要脱离托管模式，需要显式绑定外部钱包或转移到自托管账户。
- User Stories:
  - PRD-P2P-029-A: As a `producer_system_designer`, I want `hosted_public_join` to have a formal hosted account model, so that public onboarding does not depend on users handling raw keys.
  - PRD-P2P-029-B: As a player, I want email login to recover my game identity, so that I can re-enter the world without copying a private key around.
  - PRD-P2P-029-C: As a `runtime_engineer`, I want a managed signer reference and sign API boundary, so that runtime can accept hosted-custody signatures without trusting the browser.
  - PRD-P2P-029-D: As a `viewer_engineer`, I want device-session and step-up auth states to be explicit, so that login, reconnect and sensitive-action UX are predictable.
  - PRD-P2P-029-E: As a `qa_engineer` / `liveops_community`, I want abuse, recovery, revocation and transfer-out rules frozen, so that hosted login can be operated safely.
  - PRD-P2P-029-F: As a player, I want an optional path to bind an external wallet or move to self-custody later, so that managed onboarding does not lock me into browser-only trust.
- Critical User Flows:
  1. Flow-P2P-029-001: `访客打开 hosted_public_join URL -> 先拿 guest session -> 浏览世界 -> 点击开始游玩`
  2. Flow-P2P-029-002: `用户选择推荐的通行密钥或可选邮箱登录 -> 按多凭证合同完成认证 -> 生成或恢复 hosted_account_id；世界准入独立检查`
  3. Flow-P2P-029-003: `account service 为用户恢复 signer_ref 和 player_id -> session broker 签发 player_session + device_session -> runtime 完成 entity bind`
  4. Flow-P2P-029-004: `玩家执行普通玩法输入 -> 浏览器只用 device session key 或 session token -> runtime 校验 capability，不触发 custody sign`
  5. Flow-P2P-029-005: `玩家执行 prompt_control_apply / main_token_transfer 等高风险动作 -> step-up auth -> policy engine 评估 -> custody service 出签 -> runtime 验证并执行`
  6. Flow-P2P-029-006: `玩家要退出托管 -> 绑定外部钱包或发起 transfer-out -> 进入 cooldown / 审计 -> 迁移完成后把 custody_mode 切到 self_custody`
  7. Flow-P2P-029-007: `邮箱失效、设备丢失、账号被盗用或风控命中 -> operator/recovery runbook -> 撤销 device_session 或冻结 managed signer -> 用户重新认证`
- Functional Specification Matrix:
| 功能点 | 字段定义 | 动作行为 | 状态转换 | 计算/判定规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| Hosted account | `hosted_account_id/player_id/status`；具体凭证字段归多凭证合同 | 推荐通行密钥、可选邮箱，验证凭证并恢复稳定账户 | `guest -> challenge_issued -> verified -> active/frozen` | 同一登录因子可映射多个设备 session，但只能映射一个当前主账户真值 | `identity plane` 负责 |
| Device session | `device_session_id/player_session_id/device_pubkey/expires_at/risk_level` | 浏览器建立/刷新/撤销短期设备会话 | `issued -> registered -> refreshed -> expired/revoked` | 设备会话必须有限 TTL；浏览器只允许存 `session handle + device key handle`，不存长期 signer 明文私钥 | `public player plane` + `identity plane` |
| Managed signer reference | `signer_ref/custody_mode/algorithm/backend_class/account_binding` | 服务端创建、恢复、停用或迁移托管 signer | `provisioning -> active -> step_up_required -> frozen/migrated` | 运行时只认 `signer_ref` 及签名证明，不认浏览器自报托管私钥 | `custody plane` 负责 |
| Sign authorization | `action_id/policy_class/step_up_result/audit_id/signature_proof` | 高风险动作先授权再出签 | `requested -> challenged -> approved -> signed -> consumed/denied` | `main_token_transfer`、治理与高风险 prompt/control 必须经过 step-up 和风控；失败时返回结构化错误 | `policy engine` + `custody plane` |
| Self-custody upgrade | `custody_mode/target_account_id/transfer_out_request_id/cooldown_until` | 绑定外部钱包或发起迁移 | `managed -> export_pending -> transferred -> self_custody` | MVP 不要求导出原始私钥；至少支持把资产和身份迁到外部账户 | 需 step-up + operator / runtime 复核 |
| Audit / recovery | `risk_event_id/revoke_reason/recovered_by/evidence_ref` | 撤销设备、冻结账户、恢复访问 | `healthy -> challenged -> suspended -> recovered/closed` | 任一异常登录、设备丢失、滥用或风控命中都必须可审计 | `qa_engineer` / `liveops_community` 可追踪 |
- Acceptance Criteria:
  - AC-1: 专题必须明确 `hosted_account_id/player_id/device_session_id/signer_ref` 的职责分离，不再把 `player_id` 既当登录账户又当 signer。
  - AC-2: 专题必须明确通行密钥是推荐入口、邮箱为可选兼容方式，而“输入公钥/私钥”不是默认 UX；凭证注册、撤销与恢复遵循[多凭证合同](hosted-public-join-key-based-onboarding.prd.md)。
  - AC-3: 专题必须明确 hosted 浏览器已经清退 `localStorage privateKey` 持久化，并改用 `device_session + in-memory ephemeral Ed25519` 恢复路径；旧缓存残留必须在读取时被清洗。
  - AC-4: 专题必须明确 `managed custody sign` lane 的动作范围，至少覆盖 `prompt_control_apply` / `prompt_control_rollback` / `main_token_transfer`。
  - AC-5: 专题必须明确托管身份只适用于 player plane；node / validator / governance signer 继续由独立 custody/governance 文档约束。
  - AC-6: 专题必须定义至少一个不回传托管私钥的 self-custody 升级路径。
  - AC-7: 本专题必须显式定义 `identity plane / custody plane / public player plane / private control plane` 的边界。
  - AC-8: 本专题必须包含 recovery、revocation、device loss、rate limit、duplicate account bind、step-up failure 的处理规则。
  - AC-9: 本专题必须在 `doc/p2p/prd.md`、GitHub Issue / GitHub Project 和 `doc/p2p/prd.index.md` 建立映射。
  - AC-10: 本专题必须保持当前对外阶段口径仍为 `limited playable technical preview`；文档不得把该方案误写成“已经实现”。

## 3. Technical Requirements
- Architecture Overview: 最合适的 hosted_public_join 目标态不是“让浏览器代持真正的玩家私钥”，也不是“强迫用户第一次进游戏就接外部钱包”，而是把登录、设备会话、托管签名和后续自托管升级拆成四层。
  - `identity broker`: 按[多凭证设计](hosted-public-join-key-based-onboarding.design.md)处理标准 WebAuthn、可选邮箱 OTP、稳定账号、独立账号会话与风险控制；普通登录不占世界 slot。
  - `account registry`: 保存 `hosted_account_id -> player_id/signer_ref/device bindings` 映射；当前 hosted account 第一版实现允许 `file` 与 `Aliyun Tablestore` 双 backend，并约定 hosted 部署默认应优先落到服务端托管表存储而不是单机 JSON。
  - `session broker`: 用已验证账户换发 `player_session` 与设备短期 key challenge，服务于 runtime register/reconnect。
  - `custody service`: 只暴露 `prepare_sign/approve_sign/finalize_sign` 或等价 sign API；底层可以是 KMS/HSM 直管密钥，也可以是 KMS-wrapped sealed-key backend，但浏览器不能直接拿到长期 signer。
  - `policy engine`: 对 action class、设备风险、step-up 结果、冷却期与风控状态做统一判定。
  - `wallet bind / transfer-out gateway`: 处理外部钱包绑定、自托管迁移和托管退出。
- Data Model:
  - `hosted_account`: `hosted_account_id`, `login_channel`, `normalized_login_hint`, `status`, `created_at`, `last_verified_at`
  - `account_factor`: `factor_id`, `account_id`, `factor_type`, `masked_handle`, `verified_at`, `revoked_at`
  - `player_identity_binding`: `account_id`, `player_id`, `world_scope`, `entity_binding_mode`, `status`
  - `managed_signer`: `signer_ref`, `account_id`, `algorithm`, `backend_class`, `custody_mode`, `state`, `created_at`
  - `device_session`: `device_session_id`, `account_id`, `player_session_id`, `device_pubkey`, `issued_at`, `expires_at`, `risk_level`
  - `sign_authorization`: `authz_id`, `signer_ref`, `action_id`, `step_up_method`, `approved_at`, `expires_at`, `audit_id`
  - `custody_audit_log`: `audit_id`, `account_id`, `event_type`, `actor`, `reason`, `evidence_ref`, `created_at`
- Integration Points:
  - `crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_account_store_backend.rs`
  - `crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_player_session.rs`
  - `crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_strong_auth.rs`
  - `crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/viewer_auth_bootstrap.rs`
  - `crates/oasis7_viewer/software_safe_src/viewer_hosted_auth_state_module.js`
  - `doc/p2p/prd.md` 与 `doc/p2p/blockchain/hosted-player-access-operator-runbook.md`
  - `doc/p2p/blockchain/p2p-mainnet-security-governance-readiness.prd.md`
  - `doc/p2p/token/mainchain-token-newapi-quota-bridge.prd.md`
  - `testing-manual.md`
- Environment Tiering Contract:
  - `dev`:
    - 目标: 本地开发、结构验证、单机 smoke。
    - 允许: `OASIS7_HOSTED_ACCOUNT_STORE_BACKEND=file` 或无 OTS 配置下的 `auto` fallback，临时测试邮箱，最小手工 smoke。
    - 禁止: 对外宣称为真实玩家入口；复用 staging/production 的 SMTP、Tablestore、strong-auth signer 或 approval code。
  - `staging`:
    - 目标: 受控测试用户试用、回归、演练、上线前 smoke。
    - 允许: 独立的 SMTP、独立的 Tablestore/file store、独立 signer/approval 配置、受控测试账号与 operator 演练。
    - 要求: 至少完成“同一邮箱跨重启保持同一 `hosted_account_id/player_id`”和“真实 OTP 可送达”两条验证；对外口径仍是 preview/受控试用。
    - 禁止: 与 production 共用邮箱投递、账户表、signer、风控阈值或 incident channel。
  - `production`:
    - 目标: 面向真实外部玩家的正式 hosted account 服务面。
    - 要求: 必须使用独立真实 SMTP、独立托管账户存储、独立强鉴权 signer/custody 后端、正式告警与审计。
    - 要求: 上线前必须先通过 staging fresh smoke、operator runbook 演练与 claims review；production 只能共享“已批准的发布工件”，不能共享 staging 数据面。
    - 禁止: 复用 staging/dev 的邮箱、OTP、account store、approval code、tablestore table 或对外 claims。
- Edge Cases & Error Handling:
  - 若邮箱重复绑定到多个 hosted account，必须提供 canonical merge / reject 规则，不能静默创建分叉账户。
  - 若浏览器丢失 device session，但账户因子仍有效，必须允许重新登录恢复，而不是要求用户回忆旧私钥。
  - 若 hosted 部署配置了 `OASIS7_HOSTED_ACCOUNT_STORE_BACKEND=tablestore`，或 `auto` 模式下检测到 `OASIS7_HOSTED_ACCOUNT_TABLESTORE_*` / `ALIYUN_OTS_*`，但 endpoint/AK/table 条件不完整，server 必须在启动期显式报错，而不是静默回退到错误 backend。
  - 若 Tablestore 表不存在，服务端可以在 `auto_create=true` 时自动建表；若关闭自动建表，则必须以明确错误阻断启动，避免 hosted account 持久化隐式退化为 best-effort。
  - 若 KMS/HSM 不直接支持运行时签名算法，custody backend 可以退化为 KMS-wrapped sealed key，但对上层仍必须保持“只暴露 sign API，不暴露长期私钥”的契约。
  - 若风控判定高风险设备登录，必须把账户状态切到 `step_up_required` 或 `frozen_review`，不得继续自动签发高权限 session。
  - 若托管到自托管迁移尚未完成，managed 与 self-custody 不得并行对同一高风险动作出签，必须有单一 source of truth。
  - 若 hosted account 已存在但 `player_id` 对应实体槽已被 runtime 占用，必须先走 rebind / operator arbitration，不能靠重复创建 signer 绕过 ownership。
- Non-Functional Requirements:
  - NFR-P2P-029-1: 任何 hosted login、step-up、sign authorization、revoke、freeze 与 transfer-out 事件都必须有审计记录。
  - NFR-P2P-029-2: 浏览器端不得持久化托管 signer 明文私钥；hosted 浏览器当前只允许落盘 `device_session` 级恢复材料，旧 `privateKey` 残留必须在读取时被清洗而不是继续恢复到运行态。
  - NFR-P2P-029-3: `identity broker` 与 `custody service` 必须是独立信任面；身份验证通过不等于自动拥有高风险出签能力。
  - NFR-P2P-029-4: `managed custody` 默认适用于 hosted player surface，不得被误扩展为 node/governance/validator signer 方案。
  - NFR-P2P-029-5: 用户默认看到的是 `Oasis ID` / 账户状态 / custody mode，而不是原始公钥命名。
  - NFR-P2P-029-6: 本专题完成前，仓内与对外口径不得声称“任意新用户已经默认拥有安全托管钱包并可直接进行资产动作”。
  - NFR-P2P-029-7: `dev/staging/production` 必须是独立环境面；至少隔离 SMTP、account store/table、signer/approval secret、风控阈值和对外 claims，不得只靠“不同 URL”假装分环境。
- Security & Privacy: 邮箱、设备标识和签名授权记录都属于敏感身份数据。本专题允许记录账户 ID、factor 类型、掩码后的联系方式、signer 引用、公钥摘要、step-up 与审计事件；禁止把真实 OTP、原始私钥、seed 或长期签名材料写入仓库、前端 bootstrap、日志与测试证据。

## 4. Risks & Roadmap
- Non-Goals:
  - 不在本专题里为 node / validator / governance signer 设计生产托管体系。
  - 不要求 MVP 支持“导出托管私钥原文”；自托管升级可以先以 `bind external wallet` 或 `transfer-out` 完成。
  - 不把 hosted identity 直接等同于 bridge-service、充值或法币支付体系。
- Phased Rollout:
  - MVP: 冻结 hosted account、device session、managed signer、step-up auth、transfer-out 的正式合同。
  - v1.1: 落地邮箱登录、设备恢复与 runtime 账户绑定。
  - v1.2: 落地 custody sign API、`main_token_transfer` 托管签名 lane 与审计。
  - v2.0: 落地 external wallet bind / self-custody transfer-out，与桥接、资产与高级 creator 能力衔接。
- Technical Risks:
  - 风险-1: 如果把“邮箱登录”和“高风险动作出签”绑死在同一步，玩家体验和风控都会失衡。
  - 风险-2: 如果继续把浏览器本地 `privateKey` 当作 hosted player 正式方案，任何缓存泄露都可能扩大为账户接管。
  - 风险-3: 如果 KMS 选型过早限定为单一厂商 API，后续算法、成本或跨环境迁移问题会更难解决；因此上层必须先冻结 `signer_ref + sign API` 契约。

## 5. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-P2P-029-A | hosted-managed-identity-doc-freeze / hosted-account-identity-broker | `test_tier_required` | 专题 PRD/design/project 建档、模块入口映射、登录与 hosted account contract 冻结 | hosted onboarding、账户真值 |
| PRD-P2P-029-B | hosted-account-identity-broker / device-session-and-runtime-binding | `test_tier_required` + `test_tier_full` | 邮箱登录、guest->player upgrade、恢复与 rebind 回归 | player login、session 生命周期 |
| PRD-P2P-029-C | managed-custody-sign-api | `test_tier_required` + `test_tier_full` | signer_ref、custody backend、sign authorization 与 runtime 验签回归 | high-risk action、资产面签名 |
| PRD-P2P-029-D | device-session-and-runtime-binding / step-up-auth-and-risk-policy | `test_tier_required` | viewer UX、设备会话存储、step-up 状态与错误反馈回归 | 前端登录/重连/敏感动作文案 |
| PRD-P2P-029-E | qa-abuse-and-liveops-runbook | `test_tier_required` + `test_tier_full` | 盗号、设备丢失、重复绑定、风控冻结、撤销与恢复 runbook | 运营事故、QA 阻断 |
| PRD-P2P-029-F | external-wallet-bind-and-transfer-out | `test_tier_required` + `test_tier_full` | self-custody 升级、wallet bind、transfer-out 冷却与审计回归 | 账户迁移、custody exit |
| PRD-P2P-029-G | hosted-account-env-tiering | `test_tier_required` | 文档冻结 `dev/staging/production` 边界；`rg -n "staging|production|tablestore|smtp|claims"` 覆盖 PRD/project/runbook；`./scripts/doc-governance-check.sh` + `git diff --check` | 环境隔离、试用/正式口径、operator 上线清单 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-P2P-029-001 | `hosted_public_join` 默认推荐通行密钥，邮箱为可选兼容方式；不要求用户手填公私钥 | 让新用户自己保存或输入公私钥 | 原邮箱默认决策已由[多凭证合同](hosted-public-join-key-based-onboarding.prd.md)及本页 SC-1 / AC-2 承接；保留稳定身份、恢复、风控和 custody 独立边界。 |
| DEC-P2P-029-002 | 浏览器只持有设备级短期会话材料，长期玩家 signer 留在 custody plane | 继续把 `privateKey` 持久化到浏览器 localStorage 作为正式方案 | 这与 hosted access “浏览器不持有长期 signer” 的既有边界冲突。 |
| DEC-P2P-029-003 | 上层冻结 `signer_ref + sign API` 契约，底层后端允许 `KMS/HSM` 或 `KMS-wrapped sealed-key backend` | 直接把产品方案写死为单一云 KMS 的具体 key API | 产品首先需要稳定的 trust boundary，而不是把运行时算法/成本约束提前硬编码进 PRD。 |
| DEC-P2P-029-004 | 自托管升级先支持 `bind external wallet` / `transfer-out`，不要求 MVP 导出托管私钥原文 | 把“导出原始托管私钥”当成 MVP 必备功能 | 迁移能力重要，但“把私钥直接吐给浏览器”不是 hosted onboarding 的最优安全默认值。 |
| DEC-P2P-029-005 | 本专题只覆盖 hosted player identity/custody，不复用到 node/governance signer | 让一个统一 KMS 同时接管 player、node、governance 全部 signer | trust domain、审计要求与风险级别不同，混用会把 hosted player 产品问题和协议级 custody 问题绑死。 |


<a id="dev-test-accounts"></a>
## 6. 外部 Agent 开发测试账号工具

### 6.1 用途与职责

开发者一次配置专用测试账号后，外部 Agent 能查看获准账号、选择身份、使用现有游戏工具并查询结果，无需反复复制私钥。首版采用独立的 `dev-test-accounts-v1`，服务可重建的开发／隔离测试账号和已准备的世界；不承担生产资产托管、完整游戏测试编排或世界任务终止协议的建设。

本节是本工具唯一的有效需求。后续实现按 [#5076](https://github.com/nil-studio/oasis7/issues/5076) 拆解；已放弃的托管扩张要求见[退役记录](#retired-cloud-custody-requirements)，不再构成实施或验收义务。

| 主体 | 主责与交接结果 |
| --- | --- |
| 测试账号工具 | 账号目录、凭据保管、账号会话、工具准入、用量与操作关联、停用新调用；提供可识别的身份和错误 |
| 现有 Game Client | 原生登录、Owner／Executor 协议、类型化签名、正式 API 调用与原操作查询 |
| 身份／世界 authority | 实际账号身份、当前权限、任务与资源状态、业务预算和最终结果 |
| 测试编排 | 准备账号和世界、选择测试目标、判断效果、按既有权限执行所需 Stop／Revoke／资源清理 |
| 外部 Agent | 阅读正式使用说明，选择获准账号与工具，报告实际结果；接入凭据由其可信连接器或宿主管理 |

正常流程：管理员登记账号和测试授权 → Agent 查看账号 → 为指定账号打开会话 → 调用现有工具／查询原结果 → 关闭会话。账号切换通过选择另一会话完成，既有会话保持原账号绑定。管理员可在同一有期限的测试授权内允许重复测试；增加账号、环境或权限时才修改授权，原生游戏操作所需批准仍由原协议决定。

<a id="req-dev-test-accounts-001"></a>
### REQ-DEV-TEST-ACCOUNTS-001：工具职责与可信服务

Trace ID: O7-REQ-56c3a553-8216-4e44-86dc-bd368f3b53cb

采用一个可信账号服务或宿主保管专用测试凭据，模型只获得账号／会话／操作引用和受限工具。服务配置固定测试环境与目的地，管理接口与模型工具接口区分权限。长期私钥、解锁材料和原生登录秘密不进入模型可读文件、参数、输出或日志；实际隔离由独立安全主体或远程服务实现。Rust 优先，复用现有协议、存储和客户端，选择一条实际可运行的服务部署路径。

- 系统设计状态：已覆盖；依据：[职责与部署](hosted-public-join-managed-identity-custody.design.md#dev-test-accounts-boundaries)。
- 实现状态：未实现；已有身份存储、broker 和 custody 组件可复用，尚无本 profile 的交付证据。
- 实现与验证依据：[验证方法](../../testing/manual/agent-runtime-self-playtest.manual.md#dev-test-accounts-validation)；核对基线：2026-11-06，`3bd96334c7ce3ad09111ed25d586afd1e20bab64`。
- 剩余范围：实际服务封装、凭据管理和所选部署的隔离验证。

<a id="req-dev-test-accounts-002"></a>
### REQ-DEV-TEST-ACCOUNTS-002：账号目录与会话选择

Trace ID: O7-REQ-8b95aab7-037d-45cc-8d02-e320d1ea4570

账号目录显示别名、实际 account ID、公开身份、环境和状态。管理员登记已合法准备的测试账号，创建／登录复用原有入口和注册权限。会话固定实际账号、环境／世界、已获准资源、动作范围、期限和调用上限；每次调用核对该绑定。多个会话并行时不共享可变的全局当前账号。别名修改、新增账号和重新连接不改变旧会话的主体，不清零同一测试授权的累计用量或丢失未决操作。

- 系统设计状态：已覆盖；依据：[账号与会话](hosted-public-join-managed-identity-custody.design.md#dev-test-accounts-sessions)。
- 实现状态：未实现；现有静态多资源 broker 不等于完整账号管理流程。
- 实现与验证依据：[验证方法](../../testing/manual/agent-runtime-self-playtest.manual.md#dev-test-accounts-validation)；核对基线：2026-11-06，`3bd96334c7ce3ad09111ed25d586afd1e20bab64`。
- 剩余范围：账号台账、配置／解锁流程、会话映射和真实双账号隔离。

<a id="req-dev-test-accounts-003"></a>
### REQ-DEV-TEST-ACCOUNTS-003：受限调用、恢复与停用

Trace ID: O7-REQ-9dfc3e3f-250d-4e90-a246-42a7fcdc2ef5

工具复用正式 Game Client，不提供任意 URL、任意签名、秘密读取或绕过原生权限的入口。工具层限制期限、累计调用和并发，业务资源预算继续由世界协议执行；内部每个网络调用不另建一套授权计费规则。操作先绑定真实账号与稳定 ID，丢响应后查同一原件，同 ID 不同正文拒绝。

关闭／停用只承诺拒绝新的工具执行；在途操作、结果未知、世界 Stop、delegation Revoke 和 Task Closed 分别报告。世界级控制与资源清理由测试编排调用既有授权入口。账号和操作记录可持久恢复；Native 服务重启或恢复备份后旧工具会话失效，经当前授权建立新会话后核对旧操作，不自动重发未知写入或恢复旧写权。Workers 的普通平台休眠／同授权实例替换保持尚未关闭的不可变工具会话及原期限，每次调用仍核对当前身份、授权版本和完整作用域；逻辑服务重启或恢复备份必须轮换测试授权使旧会话失效，关闭记录不能复活。

- 系统设计状态：已覆盖；依据：[执行、停用与恢复](hosted-public-join-managed-identity-custody.design.md#dev-test-accounts-operations)。
- 实现状态：未实现；现有原操作与游戏权限检查只证明可复用基础。
- 实现与验证依据：[验证方法](../../testing/manual/agent-runtime-self-playtest.manual.md#dev-test-accounts-validation)；核对基线：2026-11-06，`3bd96334c7ce3ad09111ed25d586afd1e20bab64`。
- 剩余范围：账号工具到真实客户端的调用、用量连续性、故障恢复及准确状态展示。

<a id="req-dev-test-accounts-004"></a>
### REQ-DEV-TEST-ACCOUNTS-004：外部 Agent 接入与真实验收

Trace ID: O7-REQ-7405a6cf-1d2a-4518-8521-013b64ae33c7

外部 Agent 经统一账号与工具合同接入，适配方式按其实际 Runtime、认证和传输能力确定。首版覆盖本地与远程两类入口，先验证真实认证及只读调用。远程入口采用可信连接器支持的标准 HTTPS／MCP 认证，秘密由连接器／服务保管，服务核对调用者与账号授权。有限 Bearer 凭据泄露后仍可能在有效范围内被使用，须限制 scope、期限并支持撤销。

两类入口各选一个实际外部 Agent 完成账号 A → B → A、正式读取和一个获准写操作，并独立核对身份、结果、隔离和恢复。具体客户端与版本记录在验收计划，不进入产品或接口命名。某个入口缺能力时记录 blocked 及解除条件，不能用另一个入口、curl 或 mock 的结果替代。仅声明实际验证的接入组合。

- 系统设计状态：已覆盖；依据：[接入与验证](hosted-public-join-managed-identity-custody.design.md#dev-test-accounts-adapters)。
- 实现状态：未实现；当前 broker 的固定 Runtime 名单和仅控制能力的 Host 不证明新工具已接通外部 Agent。
- 实现与验证依据：[验证方法](../../testing/manual/agent-runtime-self-playtest.manual.md#dev-test-accounts-validation)；核对基线：2026-11-06，`3bd96334c7ce3ad09111ed25d586afd1e20bab64`。
- 剩余范围：接入能力验证、CLI／远程适配、正式使用说明和两个真实 Agent 组合的验收。

<a id="dev-test-accounts-acceptance"></a>
### 6.2 首版验收与范围

| 验收 | 对应要求 | 可观察结果 |
| --- | --- | --- |
| AC-DEV-01 | 001／002 | 一次配置后能列出真实测试账号 A／B，公开身份、环境和状态准确；模型接触不到长期密钥和原生登录秘密 |
| AC-DEV-02 | 002 | A → B → A 及并行会话不串号；跨账号／环境引用拒绝；账号元数据更新不改变已有会话与累计用量 |
| AC-DEV-03 | 001／003 | 两账号各经正式客户端完成读取和一个获准写入；原生权限、目标和真实结果可核对 |
| AC-DEV-04 | 003 | 丢响应和服务重启后仍可定位同一原操作；同 ID 异正文拒绝，未知操作不产生第二次业务效果 |
| AC-DEV-05 | 003 | 关闭、停用、过期与恢复后旧会话拒绝新执行；在途／unknown 与世界停止／撤销分别展示；其他用途或版本的凭据不能被当作本工具授权 |
| AC-DEV-06 | 004 | 本地与远程两类实际外部 Agent 入口分别完成 AC-DEV-01～05 的适用组合，给出真实结果和未覆盖范围 |

验证复用[现有自测手册](../../testing/manual/agent-runtime-self-playtest.manual.md#dev-test-accounts-validation)。本节只定义开发工具的能力与验收；真实模型、部署和世界操作仍按已有授权执行。游戏玩法和世界控制由其现有接口负责，工具验收无需建设新的世界生命周期协议。

<a id="retired-cloud-custody-requirements"></a>
## 7. 已退役要求的定位

旧 Cloudflare 托管建设要求已退役：四角色发布、独立 Recovery Fence、双跳 PoP、逐网络 hop 授权账本、完整 TestSession、世界永久 Closed 及工业闭环均退出本工具的需求和开发计划。旧 AC／CT 不再作为本工具验收。下表仅保留不可复用的旧 Trace ID 与历史原文定位；现行条款不继承旧要求的全部义务，也不设另一套增强版待办。

| 已退役条款（历史原文） | Trace ID | 现行相关条款 |
| --- | --- | --- |
| [REQ-CLOUD-CUSTODY-001](https://github.com/nil-studio/oasis7/blob/3bd96334c7ce3ad09111ed25d586afd1e20bab64/doc/p2p/blockchain/hosted-public-join-managed-identity-custody.prd.md#req-cloud-custody-001) | O7-REQ-844f4b30-b7d5-4651-941e-17c21f37b4ac | [凭据边界](#req-dev-test-accounts-001)、[账号目录](#req-dev-test-accounts-002) |
| [REQ-CLOUD-CUSTODY-002](https://github.com/nil-studio/oasis7/blob/3bd96334c7ce3ad09111ed25d586afd1e20bab64/doc/p2p/blockchain/hosted-public-join-managed-identity-custody.prd.md#req-cloud-custody-002) | O7-REQ-b799d81b-eafb-44e8-a0df-fda202d5d133 | [既有客户端职责](#req-dev-test-accounts-001)、[账号会话](#req-dev-test-accounts-002) |
| [REQ-CLOUD-CUSTODY-003](https://github.com/nil-studio/oasis7/blob/3bd96334c7ce3ad09111ed25d586afd1e20bab64/doc/p2p/blockchain/hosted-public-join-managed-identity-custody.prd.md#req-cloud-custody-003) | O7-REQ-f837065c-4da5-4e6f-8a53-f4e5900cf26c | [有限授权](#req-dev-test-accounts-002)、[工具调用](#req-dev-test-accounts-003) |
| [REQ-CLOUD-CUSTODY-004](https://github.com/nil-studio/oasis7/blob/3bd96334c7ce3ad09111ed25d586afd1e20bab64/doc/p2p/blockchain/hosted-public-join-managed-identity-custody.prd.md#req-cloud-custody-004) | O7-REQ-26945890-45d8-4be4-81af-5fe82c85aede | [原操作查询与停用](#req-dev-test-accounts-003) |
| [REQ-CLOUD-CUSTODY-005](https://github.com/nil-studio/oasis7/blob/3bd96334c7ce3ad09111ed25d586afd1e20bab64/doc/p2p/blockchain/hosted-public-join-managed-identity-custody.prd.md#req-cloud-custody-005) | O7-REQ-b67f1215-00f3-4fbd-b8df-1c2ead4b3945 | [当前授权下的恢复](#req-dev-test-accounts-003) |
| [REQ-CLOUD-CUSTODY-006](https://github.com/nil-studio/oasis7/blob/3bd96334c7ce3ad09111ed25d586afd1e20bab64/doc/p2p/blockchain/hosted-public-join-managed-identity-custody.prd.md#req-cloud-custody-006) | O7-REQ-17af77d2-a02e-41ea-8185-cbc0650e69e3 | [复用现有存储、协议与客户端](#req-dev-test-accounts-001) |
| [REQ-CLOUD-CUSTODY-007](https://github.com/nil-studio/oasis7/blob/3bd96334c7ce3ad09111ed25d586afd1e20bab64/doc/p2p/blockchain/hosted-public-join-managed-identity-custody.prd.md#req-cloud-custody-007) | O7-REQ-f98670c4-c1fd-4610-8884-8eb2cf89c1eb | [外部 Agent 实际使用验收](#req-dev-test-accounts-004) |

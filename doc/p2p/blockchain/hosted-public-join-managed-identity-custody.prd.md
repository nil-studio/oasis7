# oasis7 hosted_public_join 托管身份、托管密钥与受限执行

- 对应设计文档: `doc/p2p/blockchain/hosted-public-join-managed-identity-custody.design.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

审计轮次: 1
建档日期: 2026-05-18

## 目标
- 为 `hosted_public_join` 建立一套正式的托管身份 / 托管密钥产品规格，解决“普通玩家如何登录并长期使用”而不是继续停留在 preview 会话与浏览器本地私钥。
- 把邮箱登录、托管 player signer、step-up auth 与自托管升级路径收成统一真值，供 runtime / viewer / QA / LiveOps 继续实现。

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

本专题同时定义[Cloudflare 测试账号托管与受限执行](#cloud-test-custody)。该首版是待实现设计，不是生产资产 custody，也不改变已有世界权威。

账号接入的推荐方式、多凭证管理和邮箱受限恢复已由[免邮箱多凭证 PRD](hosted-public-join-key-based-onboarding.prd.md)承接；本专题继续拥有资产 signer / custody / step-up 与自托管升级合同。多凭证实现候选不升级当前技术预览或生产 custody 口径。

## 1. Executive Summary
- Problem Statement: `hosted_public_join` 现在已经补上了 hosted account 邮箱登录 broker、`device_session + in-memory session key` 恢复链路，以及 env-configured SMTP 邮件投递；普通玩家不再需要手抄浏览器本地私钥，也不必继续依赖 repo-owned preview code 才能回到同一 `player_id`。但这套实现整体仍停留在 `limited playable technical preview` 边界：`crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_strong_auth.rs` 依然依赖 `OASIS7_HOSTED_STRONG_AUTH_*` + `approval_code`；`signer_ref`、managed custody sign API、冻结/恢复策略和 self-custody upgrade 还没有进入正式后端 contract。如果不把这些缺口继续收完，`public join` 仍然不能被宣称为生产级 hosted wallet / custody 方案。
- Proposed Solution: 为 `hosted_public_join` 正式冻结一套 producer-owned 的“托管身份 + 托管密钥 + 自托管升级”目标态。默认玩家路径改为 `guest -> hosted account -> managed player signer -> optional self-custody bind/transfer-out`：用户通过通行密钥或已登记邮箱证明登录，浏览器只拿 `player_session` 与设备级短期密钥；长期玩家 signer 留在服务端 custody plane，生产目标由 KMS/HSM 或 KMS-wrapped sealed-key backend 保护，并通过 step-up auth + policy engine 出签。下述 Cloudflare 测试版使用独立、明确标注的 Worker 可信持钥 lane，不代表上述生产目标已达成。
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
  - `producer_system_designer`: 需要把“邮箱登录游戏”和“链上身份/签名”收成一个不会误伤安全边界的正式产品方案。
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
  - 若 Tablestore 表不存在，服务端可以在 `auto_create=true` 时自动建表；若关闭自动建表，则必须以明确错误阻断启动，避免把 hosted account 持久化退化成隐式 best-effort。
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
  - 风险-3: 如果 KMS 选型过早写死到单一厂商 API，后续算法、成本或跨环境迁移会很难收口；因此上层必须先冻结 `signer_ref + sign API` 契约。

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


<a id="cloud-test-custody"></a>
## 6. Cloudflare 测试账号托管与受限执行

### 6.1 使用情境、目标与非目标

测试负责人需要管理多个 oasis7 测试账号，并让 dot 或其他已授权助手完成特定世界、Agent 的有限操作。负责人不应反复复制私钥，助手也不应因为能看到账号或登录管理页就获得 owner 控制权。负责人能够看清每个账号用途、授予哪些动作和预算、何时到期、怎样停止与撤销，以及一次请求到底只是被接收还是已经产生世界结果。

首版采用 Cloudflare Workers 承载托管控制面；世界状态、owner 归属、executor delegation、业务预算和共识仍由现有 oasis7 authority 决定。控制面只管理测试账号、保管分用途密钥、消费已授权动作并回读原结果。服务不可用时宁可暂停，不退回把密钥交给助手。文档状态为设计候选，以下功能全部待实现，不声明已部署或完成真实账号验收。

范围：多账号非秘密台账、Passkey 管理登录、独立 owner 批准、受限 executor API、可追踪请求、停止/撤销、密钥轮换与冻结恢复，以及 REQ-CLOUD-CUSTODY-007 的 Agent 自测闭环。服务采用 Rust 优先的实现与共享协议复用方案，包边界、平台适配和管理 UI 的范围按 REQ-CLOUD-CUSTODY-006 执行。首版仅新建经批准的测试身份，已有账号迁移保持关闭；开户必须遵守现有注册能力和开关，不能借托管服务开放注册。

首个可交付场景固定为 `prepared_world + targeted`：可信 setup 先准备并核对一个账号、一个世界、一个 Agent/task、目标和有限资源，真实 Agent 读取正式 Skill 后自行观察、选择获准动作并核对结果。工具合同覆盖 context、工业观察、移动、转移、建厂和排产所需的原生闭环；它不证明 cold-entry、全部玩法或全部 Runtime 支持。原生可信工具宿主可先交付；dot 云端接入须单独实现并通过发送者约束的 MCP profile，不能从 HTTP 服务可用或普通 MCP Bearer 登录推导 dot 已可自测。

非目标：生产资产、主网转账、validator/node/governance/treasury 密钥；修改游戏规则或共识；任意 HTTP 代理、通用签名服务、secret 读取器；替助手保存云平台管理员凭据；托管模型推理或无限后台运行；自动接管现有本地 vault、静默迁移账号、自助导出私钥。现有生产 custody 与 self-custody 目标保留，不由测试版替代。

### 6.2 核心体验与权限决策

1. 部署侧已登记的负责人以 Passkey 登录管理端，看见自己有权管理的非秘密账号列表、环境、world/Agent、公开身份、状态及最近核验时间。首个账号 ACL 来自明确的开户批准，不接受调用者自报 owner 或“第一个登录者即管理员”。列表隐藏不代表撤销；旧列表也不证明账号可写。
2. 负责人选择已准备账号，核对 world/Agent/task、工具及参数、业务预算、执行与核对截止，并以新鲜 Passkey 证明批准一个有界 TestSession。批准包括该测试的固定 Stop/Revoke 收尾策略；后续无人值守收尾只可缩权，不能延时、换目标或重新委托。协议 owner 签名仍由独立 owner custody lane 产生。
3. 指定 Rust 工具宿主在模型之外生成客户端证明密钥，并经已批准的私有交接领取 capability 和独立结果读取凭据；模型只见 session/resource/operation 引用和受限工具。管理者会话与托管账号的原生登录会话分开，后者只留在 Identity 角色，均不交给 Agent。
4. 助手读取正式 Skill，提交类型化动作，自行回读并判断目标是否完成。宿主在发起前保存稳定请求 ID；界面先显示本服务接收与排队，后显示原操作权威结果。超时或宿主重启先查询同一原件，不能引导重复创建新操作。
5. 负责人可以先暂停本服务派发，再提交世界 canonical Stop 或 delegation Revoke。普通请求占满配额或保持 unknown 时，应急停止/撤销仍有独立有界处理容量；Stop 未确认也不能阻塞独立 Revoke。界面分别显示本地暂停、世界停止确认、委托撤销确认，不能用一个绿色“已停”掩盖未确认项。
6. 正常结束、到期、预算耗尽、取消或宿主失联触发本地停写与已预授权的有界收尾。HTTP capability 到期不等于世界 delegation 已撤销；Stop/Revoke、未决原件及清理结果分别呈现。写期限后，助手仅可在独立读取凭据的范围、预算与截止内核对本次原件；身份撤销、历史缺失或无法收尾时显示阻断，不能静默重登。再次执行需要负责人重新批准。

管理界面须支持键盘操作、可读的错误文本和不依赖颜色的状态；确认页不预选所有账号或无限范围。账号切换必须清除尚未提交的其他账号草稿，提交时重验目标。负责人可选择不给权限、缩小动作/预算、先只读观察或退出；这些选项不得被自动降级流程绕过。

<a id="req-cloud-custody-001"></a>
### REQ-CLOUD-CUSTODY-001：账号台账与秘密分离

Trace ID: O7-REQ-844f4b30-b7d5-4651-941e-17c21f37b4ac

台账按账号与环境展示标签、用途、公开身份、world/Agent/task 绑定、密钥用途和版本引用、冻结状态，不存私钥、密码、恢复材料或 bearer token。同一负责人可管理多个账号，不能把标签或列表索引当权限。每次调用从已认证主体和批准关系核对精确 account/resource，跨账号替换必须拒绝。开户先持久化稳定 provisioning ID、管理关系和角色密钥原件，再调用原生注册；三角色部分成功或响应未知时只恢复原开户，不重新生成身份或密钥。所有必要绑定及原回执确认后才允许测试授权。

- 系统设计状态：已覆盖
- 实现状态：未实现
- 系统设计依据：[存储与数据](hosted-public-join-managed-identity-custody.design.md#cloud-custody-data)
- 实现与验证依据：[实现边界](hosted-public-join-managed-identity-custody.design.md#cloud-custody-baseline)；当前仅有 Unix 本地 broker，不含本节 Cloudflare 服务，尚无服务实现/部署验收任务结果。
- 核对日期与基线：2026-10-24，源码 `73398d454082eb2106b2f1075a57bc9f377cd222`，仅仓库代码/合同核对。
- 剩余范围：完整实现、跨账号隔离与脱敏验收。

<a id="req-cloud-custody-002"></a>
### REQ-CLOUD-CUSTODY-002：登录、Owner 与执行权分离

Trace ID: O7-REQ-b799d81b-eafb-44e8-a0df-fda202d5d133

Passkey 仅认证管理者；获得管理会话不自动批准开户、owner 绑定、任务控制或 executor 委托。login、owner、executor 使用不同密钥、签名用途、部署权限和秘密绑定。管理者身份与被托管账号身份建立明确映射，不能用管理者 cookie 替代被托管账号的原生会话；Identity 通过受限角色接口补齐当前合法 Cookie/CSRF，原生 gateway 继续核对账号主体。owner 操作必须与明确账号、世界、动作摘要和有效期绑定；executor 不能调用 owner 管理入口、创建新授权或提升自己的权限。Worker 运行时仍属可信持钥域，不能声称采用了 HSM 或对平台/部署管理员不可提取。

- 系统设计状态：已覆盖
- 实现状态：未实现
- 系统设计依据：[信任边界](hosted-public-join-managed-identity-custody.design.md#cloud-custody-boundaries)
- 实现与验证依据：[实现边界](hosted-public-join-managed-identity-custody.design.md#cloud-custody-baseline)；本地三类密钥隔离不证明云端隔离。
- 核对日期与基线：2026-10-24，源码 `73398d454082eb2106b2f1075a57bc9f377cd222`，无云端运行证据。
- 剩余范围：管理认证、分部署权限、owner 批准与密钥隔离实现及安全验收。

<a id="req-cloud-custody-003"></a>
### REQ-CLOUD-CUSTODY-003：有限授权与协议兼容

Trace ID: O7-REQ-f837065c-4da5-4e6f-8a53-f4e5900cf26c

每份 capability 固定账号、世界身份、Agent/task、执行客户端、允许动作、参数范围、请求/业务预算、绝对期限和父授权。禁止递归转授与通过 refresh 自行续期；同一授权并发调用共享预算。客户端证明与凭据交接按固定 profile 实施，不把 opaque handle 或 Bearer token 本身当作持有证明。首版原生工具明确使用 `authority_mode=native_executor_v1`，缺失或不支持时拒绝，不能退回 Provider 身份。托管服务不能签任意 bytes、调用任意 URL、改变 actor 或覆盖既有协议签名域。Cloudflare Ed25519 与 Rust 的同一类型化 payload 必须字节级互操作，通过正反向签名测试及拒绝矩阵后才可启用。

- 系统设计状态：已覆盖
- 实现状态：未实现
- 系统设计依据：[接口与签名](hosted-public-join-managed-identity-custody.design.md#cloud-custody-api)
- 实现与验证依据：[实现边界](hosted-public-join-managed-identity-custody.design.md#cloud-custody-baseline)；已有 Rust 适配不能证明 Workers 互操作。
- 核对日期与基线：2026-10-24，源码 `73398d454082eb2106b2f1075a57bc9f377cd222`，无云端签名测试结果。
- 剩余范围：受限适配、共享预算、签名向量及真实下游兼容验收。

<a id="req-cloud-custody-004"></a>
### REQ-CLOUD-CUSTODY-004：请求结果、停止与撤销可区分

Trace ID: O7-REQ-26945890-45d8-4be4-81af-5fe82c85aede

每个请求保留稳定 operation ID、原正文/签名摘要、原下游请求和结果。重试不得变成新的业务动作；同 ID 不同正文必须拒绝。结果未知不能显示成功或默认重发。暂停本服务派发、世界 canonical Stop、delegation Revoke 是三个不同结果，后两者必须用原请求权威回执和当前 fence/委托状态证明。父授权撤销在账号权威提交后阻断本服务全部派生 capability 与尚未取得派发 claim 的请求；已取得 claim 即按在途请求处理，即使尚未实际发送，仍需核对，不能承诺撤销过去的世界效果。Stop/Revoke 的安全控制容量独立于普通执行和普通 owner intent 的配额；旧动作或 Stop 结果未知，不得阻塞独立 Revoke，也不得为腾出容量而删除未知原件或释放其业务预留。应急 Revoke 必须覆盖已经签发但尚未确认的授权序号，防止其迟到后恢复委托；序号历史不全时保持冻结。

- 系统设计状态：已覆盖
- 实现状态：未实现
- 系统设计依据：[事务、状态与恢复](hosted-public-join-managed-identity-custody.design.md#cloud-custody-state)
- 实现与验证依据：[实现边界](hosted-public-join-managed-identity-custody.design.md#cloud-custody-baseline)；本地原请求恢复不证明云端故障恢复。
- 核对日期与基线：2026-10-24，源码 `73398d454082eb2106b2f1075a57bc9f377cd222`，无 Cloudflare outbox/停止演练。
- 剩余范围：持久 outbox、下游查重回读、并发撤销与失联演练。

<a id="req-cloud-custody-005"></a>
### REQ-CLOUD-CUSTODY-005：安全恢复与真实阶段边界

Trace ID: O7-REQ-b67f1215-00f3-4fbd-b8df-1c2ead4b3945

备份恢复、密钥轮换和部署回退必须先冻结执行，核对未回退的安全代、最新撤销事实与原操作结果，再由负责人重新批准。不得从旧备份复活权限、预算或已发送请求。系统披露 Worker 及部署管理员的剩余信任风险、单账号暂停边界、审计保留与数据删除限制。只通过文档/fixture/本地测试不得宣称云端托管、世界结果或生产安全已验收。

- 系统设计状态：已覆盖
- 实现状态：未实现
- 系统设计依据：[运维与恢复](hosted-public-join-managed-identity-custody.design.md#cloud-custody-operations)
- 实现与验证依据：[实现边界](hosted-public-join-managed-identity-custody.design.md#cloud-custody-baseline)；生产 custody、恢复与云端部署不在当前能力内。
- 核对日期与基线：2026-10-24，源码 `73398d454082eb2106b2f1075a57bc9f377cd222`，仅静态核对。
- 剩余范围：灾备隔离、轮换、观测、真实恢复与受控账号端到端验收。

<a id="req-cloud-custody-006"></a>
### REQ-CLOUD-CUSTODY-006：Rust 实现与可核对的包边界

Trace ID: O7-REQ-17af77d2-a02e-41ea-8185-cbc0650e69e3

托管业务、协议编码与校验优先使用 Rust，首版规划 `oasis7_custody_core` 与 `oasis7_custody_worker` 两个新 crate；现有协议类型和 canonical 编码保持单一声明。平台 I/O、原生身份服务、浏览器 UI 和部署角色的边界必须明确，不能把 Unix broker 整包移入 Workers，或为每个内部模块另造 crate。四个 Worker 角色分别构建和部署，外部参考代码不得改变既有秘密不出模型、DO 授权权威与原操作恢复合同。

- 系统设计状态：已覆盖
- 实现状态：未实现
- 系统设计依据：[Rust 包组织与平台适配](hosted-public-join-managed-identity-custody.design.md#cloud-custody-rust)、[外部实现参考边界](hosted-public-join-managed-identity-custody.design.md#cloud-custody-reference)
- 实现与验证依据：[实现边界](hosted-public-join-managed-identity-custody.design.md#cloud-custody-baseline)；新 crate、Workers 构建组合及协议生成支持均待实现，本次不创建占位包。
- 核对日期与基线：2026-10-25，源码 `73398d454082eb2106b2f1075a57bc9f377cd222` 与本设计引用的固定外部来源，仅静态核对。
- 剩余范围：包与角色工件实现、真实 Wasm 构建、平台适配、API 消费者一致性及架构边界验收。

<a id="req-cloud-custody-007"></a>
### REQ-CLOUD-CUSTODY-007：Agent 自测、接续与有界收尾

Trace ID: O7-REQ-f98670c4-c1fd-4610-8884-8eb2cf89c1eb

实现必须交付实际模型可消费的可信工具宿主与版本化工具 profile，打通开户/原生会话、准备环境、一次批准、上下文、动作、原件查询、效果核对和收尾。执行窗口、只读核对窗口与 cleanup 权限分别有界；到期不能阻止已批准的安全收尾，也不能触发无限续期或自动新登录。自动 Stop/Revoke 只执行预批准策略并核验当前 owner/fence，未知结果保持可见。验收复用既有 Agent 自测 RunInput/TesterInput/RunResult 与真实 Runtime 证据，必须有真实 Agent 读取 Skill、选择动作、查询权威结果及故障后的接续记录；脚本或 UI smoke 不能代签。dot、其他 Runtime 与 cold-entry 分别按实际通过范围声明。

- 系统设计状态：已覆盖
- 实现状态：未实现
- 系统设计依据：[客户端与持有证明](hosted-public-join-managed-identity-custody.design.md#cloud-custody-client)、[原生会话与开户](hosted-public-join-managed-identity-custody.design.md#cloud-custody-provision)、[工具合同](hosted-public-join-managed-identity-custody.design.md#cloud-custody-tools)、[TestSession 与真实 Agent 验收](hosted-public-join-managed-identity-custody.design.md#cloud-custody-selftest)
- 实现与验证依据：[实现边界](hosted-public-join-managed-identity-custody.design.md#cloud-custody-baseline)；工具宿主的新 profile、云端接线、TestSession 与 CT-007/008 均待实现，不把当前 Skill/native broker 当作云端消费者证据。
- 核对日期与基线：2026-10-25，源码 `73398d454082eb2106b2f1075a57bc9f377cd222` 与本设计固定的协议来源，仅静态核对。
- 剩余范围：可信客户端/连接器、原生会话适配、全部工具合同与故障测试、独立真实 Agent 的有界端到端验收。

<a id="cloud-custody-acceptance"></a>
### 6.3 验收场景与结果边界

| 验收 ID | 覆盖要求 | 使用者情境与可判定结果 | 不可替代的失败检查 |
| --- | --- | --- | --- |
| AC-CLOUD-01 | REQ-CLOUD-CUSTODY-001 | 管理多个测试账号，列表只显示非秘密元数据；开户故障后恢复同一账号、角色 key 和原操作 | 自封首个管理员、跨账号/环境替换、部分开户后重建身份、日志与错误泄密必须拒绝/消除 |
| AC-CLOUD-02 | REQ-CLOUD-CUSTODY-002 | 登录后先保持只读；Owner/Executor 使用正确托管账号的当前原生会话及角色证明 | 管理者 cookie 替代托管账号 cookie、跨账号 cookie/CSRF、撤销后自动重登、executor 调 owner 路由、共用三类 key 都失败 |
| AC-CLOUD-03 | REQ-CLOUD-CUSTODY-003 | 委托只在固定世界/Agent、动作参数、预算和期限内生效 | 并发超支、过期、跨世界、重放、续期、转授、任意 URL/raw sign 全部拒绝 |
| AC-CLOUD-04 | REQ-CLOUD-CUSTODY-003 | 同一 Ed25519 测试向量在 Rust/Workers 双向生成和验签一致 | 域、字节、版本、整数编码、world/actor/fence 篡改与弱公钥必须拒绝 |
| AC-CLOUD-05 | REQ-CLOUD-CUSTODY-004 | 超时/重启后回到同一原操作结果，不产生第二次业务效果 | 在提交前后各断网、并发同 ID、不同摘要、下游结果暂缺时不盲重发 |
| AC-CLOUD-06 | REQ-CLOUD-CUSTODY-004 | 页面分别呈现本地暂停、世界 Stop、委托 Revoke 的完成或未知 | 父撤销后派生权限和未 claim 请求失效；原动作 unknown/普通配额已满时仍可 Stop/Revoke，Stop unknown 不阻塞 Revoke；迟到的旧授权不能越过已确认撤销；保留旧原件/预留，未核对不得显示全部已撤销 |
| AC-CLOUD-07 | REQ-CLOUD-CUSTODY-005 | 恢复旧快照仍冻结；核对当前安全代、撤销和原结果后重新批准 | 旧 capability、旧 nonce、旧预算、旧 Worker 版本不能复活写权限 |
| AC-CLOUD-08 | REQ-CLOUD-CUSTODY-005/007 | 在固定候选、prepared-world 和已声明 Runtime/profile 下，真实 Agent 读 Skill、自主完成目标并核对世界效果，生成现有 RunResult 与收尾证据 | 固定脚本、人工代填工具结果、UI smoke 或 cloud accepted 不代替 Agent 行为/world committed；未实测 dot、cold-entry 或其他工具不标通过 |
| AC-CLOUD-09 | REQ-CLOUD-CUSTODY-006 | 两个 Rust crate 职责可核对，四种角色分别构建/部署；原生/Wasm 共用协议，管理 UI 消费实际 HTTP 合同 | 禁止 native 依赖回流、混角色产物、手写第二套签名编码；全宽整数经 FFI/SQL 回读不失真；无模型 secret 读取或响应缓存替代原操作 |
| AC-CLOUD-10 | REQ-CLOUD-CUSTODY-003/007 | 已登记工具宿主完成私有凭据交接、PoP、重启后的原件查询及首版工具映射 | 仅偷 token、改正文/URI、重复挑战、换客户端/会话、丢失 202 后重做动作、遗漏 native authority mode 均不能执行；dot 缺宿主证明扩展时阻断 |
| AC-CLOUD-11 | REQ-CLOUD-CUSTODY-004/007 | 结束/到期/预算耗尽/失联后停新写，独立读取预算核对旧结果，Owner 按预批准策略有界 Stop/Revoke | 不能把 HTTP 到期当 canonical Revoke，不能恢复写权查询；身份撤销或下游未知保持 cleanup blocked/unknown，不能假报 closed |

### 6.4 风险、成本与推进边界

- 集中托管减少助手接触秘密，但集中 Worker 代码、平台账号与部署链的信任；账号分片不自动抵御平台管理员或供应链攻陷。
- 暂停和结果核对优先于可用性。负责人承担重新认证、重新授权与处理未知请求的操作成本，不承诺断网期间持续执行。
- 首版限制见[容量与预算](hosted-public-join-managed-identity-custody.design.md#cloud-custody-quality)，是待测上限而非容量实测。超限拒绝，不自动增加费用或购买资源。
- 推进顺序为文档/独立安全审读、实现及离线向量/故障测试、隔离测试环境、真实受控账号验收。部署、凭据创建、数据导入与生产扩面分别需要明确授权；本次文档本身不授权这些动作。
- 专业 owner 为 `blockchain_ops_engineer` 与 `runtime_engineer`，体验/claims 由 `producer_system_designer` 核对；具体实现排期及交付事实留在相应 PR/Task，不在本文维护第二份任务台账。

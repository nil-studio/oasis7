# oasis7 Runtime：玩家发布制成品的 WASM 模块与 Profile 治理闭环

- 对应设计文档: `doc/world-runtime/module/player-published-entities.design.md`
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history。

审计轮次: 4


## 1. Executive Summary
- Problem Statement: 玩家无法在可治理、可审计、可回放的前提下发布“新制成品”，WASM 模块发布与产品/配方/工厂 profile 治理脱节，导致经济系统无法稳定接入新内容。
- Proposed Solution: 在现有模块发布链路上增加“Profile 变更闭环”，让玩家在发布 WASM 模块时同步提交产品/配方/工厂 profile，经过 shadow + 多角色审批后自动落账，并以统一审计事件回放。
- Success Criteria:
  - SC-1: 三节点测试场景中，`ModuleReleaseSubmit -> Apply` 完成审批与落账 `p95 <= 60s`（含 profile 变更落账）。
  - SC-2: 发布完成后，`ProductProfileV1/RecipeProfileV1/FactoryProfileV1` 在 runtime 状态中可被查询，且与发布单中的 payload 一致。
  - SC-3: 任意发布/回滚过程均生成可审计事件序列（release + profile governed），回放一致性 100%。
  - SC-4: 违规发布（缺签名、缺角色审批、profile 覆盖提交）必定在 `Shadow/Apply` 被拒绝并记录 `ActionRejected` 原因。

## 2. User Experience & Functionality
- User Personas:
  - 玩家创作者：希望发布新的制成品/配方/工厂并在世界中可用。
  - 经济治理运营：需要控制内容上链与经济平衡风险。
  - 安全评审者：需要确保模块发布与 profile 变更可审计、可回放。
  - 运行时维护者：需要确定性与性能边界不被破坏。
- User Scenarios & Frequency:
  - 新制成品发布：玩家每周/每日提交新内容，要求 1 分钟内完成审批链路。
  - 版本升级与修正：运营定期升级模块或调整 profile（每周/双周）。
  - 事故回滚：发布后发现异常时触发回滚（低频但必须可靠）。
- User Stories:
  - PRD-WORLD_RUNTIME-010: As a 玩家创作者, I want to submit a WASM module with product/recipe/factory profiles in one release request, so that a new crafted item becomes usable after approval.
  - PRD-WORLD_RUNTIME-011: As a 治理审批者, I want role-based approvals to complete within 60 seconds in 3-node tests, so that publishing does not stall gameplay loops.
  - PRD-WORLD_RUNTIME-012: As a 运行时维护者, I want deterministic, audited release + profile events with strict validation, so that replay and security boundaries stay intact.
- Critical User Flows:
  1. Flow-PP-001（发布新制成品）:
     `Compile/Deploy wasm -> ModuleReleaseSubmit(包含 profile_changes) -> Shadow -> 多角色 Approve -> Apply -> ModuleEvent + ProfileGoverned 事件落账 -> 产物可被经济模块调用`。
  2. Flow-PP-002（升级或修正）:
     `Deploy 新 wasm -> ModuleReleaseSubmit(升级 + 新 ID profile admission) -> Shadow/Approve -> Apply -> 旧版本记录保留 -> 新 profile 生效`。
  3. Flow-PP-003（异常回滚）:
     `Release Apply 后发现异常 -> RollbackModuleInstance -> 旧版本激活 -> 保留 committed profile、资产与历史 -> 显式冻结被回滚发布单的新增发布/激活准入`。
- Functional Specification Matrix:
| 功能点 | 字段定义 | 按钮/动作行为 | 状态转换 | 排序/计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 发布单提交 | `ModuleReleaseSubmit { manifest, activate, install_target, required_roles, profile_changes }` | 校验 artifact/identity/owner，创建发布单 | `Requested` | 计算 `shadow_manifest_hash` 覆盖当前 manifest + module_changes + 完整 profile_changes；格式兼容见设计文档 | 仅 artifact owner 可提交 |
| Shadow 校验 | `ModuleReleaseShadow { request_id }` | 校验 wasm_hash/ABI/limits/profile 冲突与覆盖提交 | `Requested -> Shadowed` | `shadow_manifest_hash` 必须可复现 | 由当前已登记 agent 执行；审批 role binding 独立校验 |
| 角色审批 | `ModuleReleaseApproveRole { request_id, role }` | 记录角色审批 | `Shadowed -> PartiallyApproved/Approved` | 角色集合归一化，去重 | 审批人必须绑定该 role |
| 应用发布 | `ModuleReleaseApply { request_id }` | 复核冲突后应用 module_changes + profile_changes | `Approved -> Applied` | module_changes 先于 profile_changes，按 module_id/product_id/recipe_id 排序 | 须重验当前 owner/角色绑定与全部 required_roles，旧批准不预留权限 |
| 产品 profile 治理 | `GovernProductProfile { proposal_id, profile }` | 校验字段白名单与覆盖冲突 | `pending -> governed` | `product_id` 作为唯一键；若 state 已存在同 ID 则拒绝（即使内容一致） | 必须引用 Applied 的 proposal_id |
| 配方 profile 治理 | `GovernRecipeProfile { proposal_id, profile }` | 校验字段白名单与覆盖冲突 | `pending -> governed` | `recipe_id` 作为唯一键；若 state 已存在同 ID 则拒绝（即使内容一致） | 必须引用 Applied 的 proposal_id |
| 工厂 profile 治理 | `GovernFactoryProfile { proposal_id, profile }` | 校验字段白名单与 tier/slots/覆盖冲突 | `pending -> governed` | `factory_id` 作为唯一键；若 state 已存在同 ID 则拒绝（即使内容一致） | 必须引用 Applied 的 proposal_id |
| 发布拒绝 | `ActionRejected::RuleDenied` | 输出拒绝原因与审计事件 | `Requested/Shadowed/Approved -> Rejected` | 记录拒绝原因列表 | 任何校验失败即拒绝 |
<a id="ppe-release-acceptance"></a>
- Acceptance Criteria:
  - AC-1 (PRD-WORLD_RUNTIME-010): 发布单支持携带 `profile_changes`（产品/配方/工厂），Apply 后 profile 与 payload 一致。
  - AC-2 (PRD-WORLD_RUNTIME-010): 发布单 Apply 后，新的制成品可在 `ValidateProductWithModule` 与 `ScheduleRecipeWithModule` 流程中被识别。
  - AC-3 (PRD-WORLD_RUNTIME-011): 三节点测试场景中，提交到 Apply 的 `p95 <= 60s`，超时记录为发布失败并可诊断。
  - AC-4 (PRD-WORLD_RUNTIME-012): 缺少 artifact identity、角色审批不足、profile 覆盖提交均触发 `ActionRejected`，并记录审计事件。
  - AC-5 (PRD-WORLD_RUNTIME-012): 事件回放后 module registry + profile maps 与发布时一致。
  - AC-7 (PRD-WORLD_RUNTIME-012): Shadow/Approve 后改变任一 profile payload，Apply 必须原子拒绝；未发生 module/profile 部分生效。
  - AC-8 (PRD-WORLD_RUNTIME-012): Apply 前当前 owner/审批角色绑定失效时，旧批准不允许落账；拒绝不产生发布世界效果。更丰富的授权期限、范围缩减与原子替代模型仍为后续目标合同，不据此宣称当前实现。
  - AC-9 (PRD-WORLD_RUNTIME-012): Rollback 原子恢复旧模块并记录对被回滚发布单的新增发布/激活冻结；保留已 committed profile、独立资产、来源与历史 receipt。失败不产生部分恢复，重复/恢复/replay 不复制效果；旧批准或 receipt 不自动解冻。
  - AC-6 (PRD-WORLD_RUNTIME-012): 对 state 中已存在的 `product_id/recipe_id/factory_id`，发布单在 `Shadow` 与 `Apply` 均拒绝；即使 payload 与现有 profile 完全一致也拒绝，并给出明确拒绝原因。
- Non-Goals:
  - 不提供游戏内 IDE/脚本编辑器。
  - 不支持非确定性 I/O 或外部网络访问。
  - 不构建资源美术资产（图标/模型）分发管线。
  - 不改动既有 `wasm-1` ABI 版本。
  - 不提供同 ID profile patch、完整授权到期/替代框架、既有能力受控退出或任意经济消费冻结。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用。
- Evaluation Strategy: 不适用。

## 4. Technical Specifications
- Architecture Overview:
  - 玩家提交合法 wasm binary 工件（生产源码构建由外部受控的同一 Docker builder 完成；runtime 宿主直编只允许显式 gated 的 dev/test 路径，见 [SC-5](../wasm/wasm-deterministic-build-pipeline.prd.md#sr2-obligation-sc-5)、[SC-7](../wasm/wasm-deterministic-build-pipeline.prd.md#sr2-obligation-sc-7) 与 [AC-8](../wasm/wasm-deterministic-build-pipeline.prd.md#sr2-obligation-ac-8)）-> module artifact 写入 store -> ModuleReleaseSubmit(包含 profile_changes) -> Shadow 校验(artifact/ABI/limits/profile冲突与覆盖提交) -> 多角色审批 -> Apply 二次复核冲突后落账 ModuleEvent + ProfileGoverned 事件 -> world state 更新 module registry + profile maps -> 经济系统路由新制成品。
- Integration Points:
  - `doc/world-runtime/wasm/wasm-interface.md`（WASM ABI）
  - `doc/world-runtime/module/module-lifecycle.md`（发布/治理事件）
  - `doc/world-runtime/prd.md`（模块存储）
  - `doc/world-runtime/runtime/runtime-integration.md`（执行/路由）
  - `crates/oasis7_wasm_abi`（`ModuleManifest` / `ProductProfileV1` / `RecipeProfileV1` / `FactoryProfileV1`）
- Edge Cases & Error Handling:
  - artifact hash 不一致或 signature 校验失败 -> `ActionRejected::RuleDenied` + 审计记录。
  - profile 覆盖提交（同 `product_id/recipe_id/factory_id` 已存在，无论内容是否一致）-> `Shadow` 与 `Apply` 均拒绝并输出明确原因。
  - 角色审批重复或越权 -> 忽略重复、越权拒绝并记录。
  - 并发发布同一 module_id -> 按 proposal_id 序列化处理，后续发布需基于最新 manifest。
  - Apply 过程中 profile 落账失败 -> Apply 回滚为 Rejected 并记录失败原因。
  - 模块执行返回非法输出 -> 触发 `ModuleCallFailed`，不影响已发布 profile。
- Non-Functional Requirements:
  - NFR-1: 三节点测试场景下发布审批 `p95 <= 60s`。
  - NFR-2: `wasm-1` ABI 兼容性保持向后兼容。
  - NFR-3: 单次发布支持 `<= 50` 个 profile 变更（产品+配方+工厂合计）。
  - NFR-4: 单个 wasm artifact `<= 10MB`，发布时校验输出大小与模块 limits。
  - NFR-5: 发布与 profile 落账事件回放一致率 100%。
- Security & Privacy:
  - 仅 artifact owner 可提交发布单；所有发布动作写入审计事件。
  - `artifact_identity` 的生产校验遵守[线上发布 AC-3](online-module-release-legality-closure-2026-03-08.prd.md#ac-3)：禁用 `identity_hash_v1` 宽松回退，仅允许 `ed25519` 或后续明确版本的签名方案；identity_hash 兼容只在[SC-7](../wasm/wasm-deterministic-build-pipeline.prd.md#sr2-obligation-sc-7)及其 release policy 明确允许的 dev/test 配置中适用。此处只澄清既有环境边界，不定义 canonical hash，也不宣称所有实现入口已落实。
  - 多角色审批必须满足 `required_roles`（默认 `security/economy/runtime`）。
  - 受限准入资格不等于发布成功；只有权威提交的 release + profile governed 事件确认世界效果，技术构建、请求送达、Approved 或本地缓存不能替代 receipt。
  - 回滚采用产品允许的“仅停止新的发布/激活”结果；既有能力及已结算世界事实保持原有因果，后续动作仍须满足各自当前专业合同。
  - 模块仅可生成 `EffectIntent`，禁止直接 I/O。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP (2026-03): 发布单支持 `profile_changes` + Apply 自动落账，三节点 60s SLA 验证。
  - v1.1: 后续目标：同 ID profile patch、授权期限/范围/替代及既有能力受控退出；当前闭环只采用 append-only admission 与回滚后新增准入冻结。
  - v2.0: 发布门禁自动化策略（基于风险评分/策略模型）。
- Technical Risks:
  - 发布审批延迟超过 60s（角色审批链路阻塞）。
  - profile 冲突与模块升级并发导致回放差异。
  - artifact identity 管理不完善导致安全边界松动。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-WORLD_RUNTIME-010 | TASK-WORLD_RUNTIME-010/011 | `test_tier_required` | 新增发布单 + profile 变更单测、`runtime::tests::module_action_loop` 回归 | 模块发布、profile 落账 |
| PRD-WORLD_RUNTIME-011 | TASK-WORLD_RUNTIME-012 | `test_tier_full` | 三节点发布 SLA 测试：`crates/oasis7/tests/module_release_sla_triad.rs` 输出 `output/world-runtime/module_release_sla_triad.json` | 发布链路时延 |
| PRD-WORLD_RUNTIME-012 | TASK-WORLD_RUNTIME-010/011/013/014/015 | `test_tier_required` | 角色审批、签名校验、冲突拒绝路径测试（`crates/oasis7_runtime/src/tests/module_action_loop_regressions.rs` + `crates/oasis7_runtime/src/world/module_release_publication_transaction_regressions.rs` 覆盖 release shadow/approve/apply 场景） | 安全与治理门禁 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-PPE-001 | 发布单携带 `profile_changes` 并在 Apply 时落账 | profile 单独治理提案 | 减少审批链路与延迟，满足 60s SLA。 |
| DEC-PPE-002 | 新增 `FactoryProfileV1` 作为治理数据 | 仅复用 `FactoryModuleSpec` | 需要独立治理版本与回放追踪。 |
| DEC-PPE-003 | 继续使用 `required_roles` 多角色审批 | 自动审批 | 保持安全边界与治理审计要求。 |
| DEC-PPE-004 | profile 发布禁止覆盖既有 ID（Shadow/Apply 双重拒绝） | 允许同 ID 覆盖（仅差异拒绝或幂等放行） | 防止隐式覆盖导致回放歧义与治理绕过，拒绝策略更可审计。 |

### 产品授权与验收来源
- 产品承诺由 [`受治理的区域能力与扩展`](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md) §2.1/§2.2、REQ-WR-GR-002/003 与 AC-WR-GR-002/003 提供；本合同只细化玩家发布实体的专业执行边界。
- `test_tier_required`：AC-7/8/9 的 payload 漂移、当前角色漂移、回滚成功/失败与历史保全；`test_tier_full`：持久化恢复/replay、跨节点与入口 receipt 一致性以及既有三节点 SLA。要求不等于已取得通过证据，当前结论以 task issue 中 fresh QA/runtime evidence 为准。

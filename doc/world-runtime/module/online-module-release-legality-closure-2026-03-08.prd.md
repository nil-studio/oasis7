# oasis7 Runtime：线上模块发布合法性闭环补齐（2026-03-08）

- 对应设计文档: `doc/world-runtime/module/online-module-release-legality-closure-2026-03-08.design.md`
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history。

审计轮次: 1


## 1. Executive Summary
- Problem Statement: 当前实现已具备模块身份校验、治理提案与多 runner 对账能力，但仍存在“内置清单写死、身份签名弱化回退、本地可自签最终性证书”等断点，且生产发布路径仍未与 CI 完全解耦，无法满足去中心化场景下“所有节点仅执行合法模块”的目标态。
- Proposed Solution: 建立“线上发布清单 + 外部多签最终性 + 运行时强校验 + 节点侧可审计发布流水”四段闭环，把合法性锚点从本地构建结果迁移到治理确认的发布清单与签名证据，并明确生产发布不依赖 CI。
- Success Criteria:
  - SC-1: 生产路径下，运行时只接受“治理确认的 manifest hash + 合法 artifact 签名”模块，非法模块拦截率 100%。
  - SC-2: 生产路径下禁用本地派生 finality 证书自动 apply；`apply_proposal_with_finality` 仅接受外部阈值签名证书。
  - SC-3: builtin 模块加载主路径从仓库 `include_str!` 迁移到线上发布清单源，支持信任根版本化与轮换。
  - SC-4: 建立去中心化发布流水（提案、复构建证明、阈值签名、清单激活），生产路径不依赖 CI 服务可用性。
  - SC-5: 多平台构建差异在发布阶段可收敛为“同平台可复现 + 清单签名确认”，上线前不可复现构建必须阻断。
  - SC-6: 主 CI 不承载生产发布写入与激活；CI 仅做开发回归，生产发布由发布节点流水完成且可审计。
  - SC-7: `proof_cid` 必须指向一份可归档、可重放的 attestation proof payload；发布节点要能在 node-side 直接提交 `ModuleReleaseSubmitAttestation`，而不是依赖 CI workflow 代替生产提交流水。

## 2. User Experience & Functionality
- User Personas:
  - 链上验证者签名集（按 epoch 快照）：对模块合法性与发布生效进行阈值签名确认。
  - 节点运营者：依赖线上清单拉取合法模块，拒绝非授权模块。
  - 协议维护者：维护信任根、公钥轮换、证书格式与兼容窗口。
  - 发布节点运营者：维护可审计的提案、复构建证明与签名聚合流水。
- User Scenarios & Frequency:
  - 新模块发布：每次模块升级/新增均执行一次（高频）。
  - 紧急冻结与撤销：安全事件触发时执行（低频高优先级）。
  - 信任根轮换：按季度/半年度执行（中频）。
  - 构建漂移排查：门禁失败时执行（中频）。
- User Stories:
  - PRD-WORLD_RUNTIME-016: As a 节点运营者, I want nodes to load modules only from governance-approved online manifests, so that arbitrary local build outputs cannot enter execution path.
  - PRD-WORLD_RUNTIME-017: As a 安全评审者, I want artifact identity to require cryptographic signatures in production, so that `identity_hash` fallback cannot bypass trust policy.
  - PRD-WORLD_RUNTIME-018: As a 发布节点运营者, I want decentralized release attestations and threshold-signature finalization with full audit trail, so that production release does not depend on centralized CI services.
- Critical User Flows:
  1. Flow-OMR-001（标准发布）:
     `发布提案提交 -> 多节点同平台重建 -> 生成发布清单/identity/签名 -> 验证者签名集阈值确认（建议 >=2/3 stake 且满足最小独立节点数） -> finality 证书上链 -> 节点按 manifest hash 拉取并验证执行（主 CI 不参与签名与激活）`。
  2. Flow-OMR-002（发布阻断）:
     `任一平台构建不可复现或签名证明不足 -> 证书阈值不达标 -> 不生成发布证书 -> 运行时保持旧版本`。
  3. Flow-OMR-003（紧急撤销）:
     `发现已发布模块风险 -> 验证者签名集超阈值触发 emergency veto/brake（或由治理合约规则触发） -> 发布清单版本冻结/撤销 -> 节点拒绝目标模块`。
  4. Flow-OMR-004（信任根轮换）:
     `治理提案更新 signer set 与阈值 -> 证书验证策略切换 -> 兼容窗口结束后禁用旧 key`。
- Functional Specification Matrix:
| 功能点 | 字段定义 | 按钮/动作行为 | 状态转换 | 排序/计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 线上发布清单 | `release_id`、`manifest_hash`、`module_id`、`platform_hash_tokens`、`identity_refs`、`created_at` | 发布节点提案并广播清单 | `draft -> signed -> finalized -> active/revoked` | `module_id` 字典序稳定；platform token 按 canonical 平台序输出 | 任意节点可提案，激活需 epoch 快照内验证者阈值签名（建议 `>=2/3 stake` 且满足最小独立节点数） |
| 发布信任根快照 | `epoch_id`、`validator_set_hash`、`stake_root`、`threshold_bps`、`min_unique_signers`、`effective_height` | 治理更新并广播快照 | `draft -> pending -> effective -> retired` | stake 以 canonical 编码聚合并计算总权重；同 epoch 仅允许一个 effective 快照 | 仅治理变更可写；节点按生效高度切换 |
| artifact 身份签名 | `wasm_hash`、`source_hash`、`build_manifest_hash`、`signer_node_id`、`signature_scheme`、`artifact_signature` | 生成并校验签名，不允许生产回退 | `unsigned -> signed -> verified/rejected` | payload 固定 `modsig:ed25519:v1` | signer 必须在信任根中且达到阈值策略 |
| finality 证书 | `proposal_id`、`manifest_hash`、`consensus_height`、`epoch_id`、`threshold_bps`、`min_unique_signers`、`signatures` | 外部提交证书后执行 apply | `approved -> queued -> finalized -> applied` | 校验 `stake_signed_bps >= threshold_bps` 且 `unique_signers >= min_unique_signers`；签名集合去重 | 仅 epoch 快照内验证者签名计入阈值并可签发 |
| 运行时加载策略 | `trust_root_version`、`active_manifest_hash`、`allow_local_fallback`（prod=false） | 节点启动/热更新时校验并拉取模块 | `bootstrap -> sync -> enforce` | 先验签再加载，失败不降级到未授权字节 | 生产节点禁止本地未授权 fallback |
| 去中心化发布证据流程 | `proposal_tx`、`request_id`、`build_attestations{signer_node_id,platform,build_manifest_hash,source_hash,wasm_hash,proof_cid}`、`finality_cert`、`audit_log_cid` | 节点提交复构建证明并聚合签名；证明必须显式绑定 `request_id` 与 `wasm_hash` | `proposed -> attested -> threshold_reached -> finalized` | 证明按 `signer_node_id + platform` 去重聚合，冲突提交拒绝并保留首条审计证据 | 仅信任根内 signer 且已绑定 node identity 的证明计入阈值 |
| attestation proof payload | `proof_schema_version`、`request_id`、`signer_node_id`、`platform`、`evidence_summary`、`attached_files[]`、`payload_sha256` | 节点先打包 proof payload，再以其内容地址作为 `proof_cid` 提交 attestation | `evidence_collected -> packaged -> submitted -> archived` | `payload_sha256` 必须稳定对应 payload 内容；同一 payload 重放不得改变 `proof_cid` | payload 可由任意合规节点生成，但提交 attestation 仍需受信 node identity |
| 兼容状态机映射 | `module_release_request_id`、`release_id`、`shadow_manifest_hash`、`applied_manifest_hash`、`applied_proposal_id` | `ModuleRelease*` 事件驱动写入发布清单映射 | `requested/shadowed/approved/applied -> draft/finalized/active` | 映射关系 append-only；同 `request_id` 不可重写 | 仅治理 apply 与发布流水写入 |
- Acceptance Criteria:
  - <a id="ac-1"></a>AC-1 (PRD-WORLD_RUNTIME-016): 生产路径移除 builtin 模块清单 `include_str!` 作为主真源，改为线上发布清单驱动；本地内置仅允许作为应急 bootstrap 且默认关闭。
  - <a id="ac-2"></a>AC-2 (PRD-WORLD_RUNTIME-016): 节点在加载模块前必须同时校验 `manifest_hash`、artifact signature、signer trust root，任一失败即拒绝执行。
  - <a id="ac-3"></a>AC-3 (PRD-WORLD_RUNTIME-017): 生产配置下禁用 `identity_hash_v1` 宽松回退；仅允许 `ed25519`（或后续明确版本）签名方案。
  - <a id="ac-4"></a>AC-4 (PRD-WORLD_RUNTIME-017): `apply_proposal()` 在生产配置下不得本地生成 finality 证书；必须显式传入外部阈值签名证书。
  - <a id="ac-5"></a>AC-5 (PRD-WORLD_RUNTIME-018): 新增去中心化发布证据链路，支持提案、复构建证明、阈值签名与证据归档，且可由任意合规节点触发。
  - <a id="ac-6"></a>AC-6 (PRD-WORLD_RUNTIME-018): CI 在生产流程中不作为发布必要条件，仅承担开发期回归与兼容性校验，不拥有生产发布判定权。
  - <a id="ac-7"></a>AC-7 (PRD-WORLD_RUNTIME-018): 发布端各节点构建只要求“同平台可复现”；跨平台不同 hash 由 keyed token 表达，最终以签名清单合法性为准。
  - <a id="ac-8"></a>AC-8 (PRD-WORLD_RUNTIME-017): finality 证书校验必须绑定 `epoch_id + validator_set_hash + stake_root + threshold_bps + min_unique_signers`；仅“签名数量阈值”判定视为不达标。
  - <a id="ac-9"></a>AC-9 (PRD-WORLD_RUNTIME-017): 安装/升级/回滚/发布应用等生产路径不得调用本地自签 `apply_proposal()`；统一改为外部证书路径。
  - <a id="ac-10"></a>AC-10 (PRD-WORLD_RUNTIME-018): 现有 `ModuleReleaseRequested/Shadowed/RoleApproved/Applied` 状态机需输出到 release manifest 映射，保证迁移期可回放且可追溯。
  - <a id="ac-11"></a>AC-11 (PRD-WORLD_RUNTIME-018): 从主 CI 移除生产发布写入与激活职责（含默认模块发布写入）；主 CI 只保留 `--check` 类回归校验。
  - <a id="ac-12"></a>AC-12 (PRD-WORLD_RUNTIME-018): 为阈值验签与发布收敛提供可执行基准：`stake/epoch` 校验耗时与“2 epoch 内收敛”均需有固定测试入口（`scripts/oasis7-runtime-finality-baseline.sh`）与归档产物（`summary.md`/`summary.json`）。
  - <a id="ac-13"></a>AC-13 (PRD-WORLD_RUNTIME-018): `proposal -> attestation` 证明落盘必须包含 `signer_node_id/platform/build_manifest_hash/source_hash/wasm_hash/proof_cid`，并强校验 `wasm_hash == release request manifest.wasm_hash`，冲突重复证明拒绝。
  - <a id="ac-14"></a>AC-14 (PRD-WORLD_RUNTIME-018): `ModuleReleaseApply` 必须按当前 epoch 快照 signer 集做 attestation 阈值聚合，且仅快照内 signer 计入阈值；阈值不足不得激活 release manifest。
  - <a id="ac-15"></a>AC-15 (PRD-WORLD_RUNTIME-018): 发布运行手册必须定义并可执行分诊“证明冲突、attestation 阈值不足、manifest 不可达/回滚/漂移”三类阻断场景，并明确主 CI 仅做开发回归与对账，不承担生产发布写入/激活。
  - <a id="ac-16"></a>AC-16 (PRD-WORLD_RUNTIME-018): `oasis7_chain_runtime` 必须提供 node-side `ModuleReleaseSubmitAttestation` 提交入口，使发布节点能够直接提交 attestation action 到共识队列，而不是依赖 CI 或手工改状态。
  - <a id="ac-17"></a>AC-17 (PRD-WORLD_RUNTIME-018): proof payload 打包脚本必须能输出可归档目录与稳定 `proof_cid`，并把 `request_id/signer_node_id/platform/build_manifest_hash/source_hash/wasm_hash/builder_image_digest/container_platform/canonicalizer_version` 与 release evidence 摘要绑定到同一 payload。
- Non-Goals:
  - 不在本期引入全新加密算法（继续以 ed25519 为主）。
  - 不在本期重构模块业务 ABI 或 gameplay 逻辑。
  - 不在本期建设中心化单点构建服务；仍保持多节点提案与复核。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用（本专题为运行时/治理/发布工程链路，不涉及模型推理策略）。
- Evaluation Strategy: 不适用。

## 4. Technical Specifications
- Architecture Overview:
  - 信任平面：`trust_root(epoch snapshot + validator stake + threshold_bps + min_unique_signers + version)` 由治理维护并版本化。
  - 发布平面：发布节点在 canonical 平台复构建并提交证明，聚合后组装 release manifest。
  - 共识平面：治理提案绑定 `manifest_hash`，由 epoch 快照内验证者从外部提交 finality certificate。
  - 执行平面：节点按 `active_manifest_hash` 拉取 artifact，做签名与身份校验后加载执行。
  - 兼容平面：现有 `ModuleRelease*` 流程保留为迁移接口，但必须写入 release manifest 映射并逐步切换到统一发布证据面。
- Integration Points:
  - `crates/oasis7_runtime/src/world/governance.rs`
  - `crates/oasis7_runtime/src/world/base_layer.rs`
  - `crates/oasis7_runtime/src/world/module_actions_impl_part1.rs`
  - `crates/oasis7_runtime/src/world/module_actions_impl_part2.rs`
  - `crates/oasis7_runtime/src/events.rs`
  - `crates/oasis7_runtime/src/state.rs`
  - `crates/oasis7_runtime/src/world/release_manifest.rs`
  - `crates/oasis7_runtime/src/builtin_wasm_identity_manifest.rs`
  - `crates/oasis7_runtime/src/{m1,m4,m5}_builtin_wasm_artifact.rs`
  - `crates/oasis7_runtime/src/builtin_wasm_materializer.rs`
  - `crates/oasis7_distfs/src/bin/sync_builtin_wasm_identity.rs`
  - `doc/p2p/consensus/builtin-wasm-identity-consensus.prd.md`
  - `doc/p2p/prd.md`（PoS 与 committed execution 当前合同）
- Edge Cases & Error Handling:
  - 线上 manifest 不可达：节点保持上一版 `active_manifest_hash`，拒绝切换到未验证新版本。
  - 证书阈值不足或签名重复：`GovernanceFinalityInvalid` 并阻断 apply。
  - 证书 signer 不在当前 epoch 快照验证者集合：`GovernanceFinalityInvalid` 并输出 signer/epoch 诊断信息。
  - 证书 `validator_set_hash`/`stake_root` 与当前 epoch 快照不一致：拒绝 apply，并输出快照期望值与证书值。
  - 证书 `threshold_bps`/`min_unique_signers` 与当前 epoch 快照不一致：拒绝 apply，并保留阈值错配审计信息。
  - 证书 `epoch_id` 与本地信任根快照不一致：拒绝 apply，并记录快照版本冲突审计事件。
  - key 轮换窗口中旧证书：在 grace period 内可验证，窗口结束后统一拒绝。
  - 复构建节点对同平台 hash 结论不一致：标记为 release fault，禁止产生活跃清单。
  - 同 `signer_node_id + platform` 的复构建证明出现不同 hash/cid：拒绝冲突写入并保留首条证明作为审计锚点。
  - attestation signer 不在当前 epoch 快照 signer 集：该证明不计入阈值聚合，若聚合不足则拒绝 `ModuleReleaseApply`。
  - 证据只停留在 CI artifact、无法形成 node-side `proof_cid`：不得视为正式发布证明，必须补 proof payload 打包与 attestation 提交。
  - 清单回滚：仅允许通过治理撤销事件推进，不允许节点本地手工回滚。
  - 生产节点误开本地 fallback：启动即告警并拒绝进入 `enforce` 状态。
  - 生产路径误调用 `apply_proposal()`：立即拒绝并输出“本地自签路径禁用”错误，禁止 silent fallback。
  - 生产路径沿用 legacy `Install/Upgrade/Rollback/ModuleReleaseApply`（未携带外部证书）必须拒绝；仅 `*WithFinality` 动作变体允许进入 apply。
  - `ModuleRelease*` 与 release manifest 映射缺失：拒绝激活并要求补齐映射证据。
  - 故障签名标准化：线上不可达/缺失回滚/identity 漂移分别输出 `builtin_release_manifest_unreachable`、`builtin_release_manifest_missing_or_rolled_back`、`builtin_release_manifest_identity_drift`，用于 runbook 与告警分诊。
  - 运行手册锚点：`testing-manual.md` 的 `S11` 必须保持与上述三类阻断信号同口径，作为值守分诊与验收入口。
  - 主 CI 误触发非 `--check` builtin manifest 写入：必须被脚本门禁拒绝（CI write-disabled），避免 CI 成为生产发布写入路径。
- Non-Functional Requirements:
  - NFR-OMR-1: 节点模块校验（manifest + identity + signature）单模块验证耗时 `p95 <= 200ms`（本地缓存命中）。
  - NFR-OMR-6: node-side attestation submit API 必须返回结构化成功/失败响应，并在无效请求、payload 编码失败、共识队列提交失败三类路径上给出可分诊错误码。
  - NFR-OMR-2: 发布证据（清单、证明签名、证书、链上高度）可追溯完整率 100%。
  - NFR-OMR-3: 信任根变更具备版本号与生效高度，所有节点在 `<= 2` 治理 epoch 内收敛。
  - NFR-OMR-4: 生产环境下未签名/伪签名模块误接纳率 0。
  - NFR-OMR-5: 生产发布路径在 CI 不可用时仍可完成，不降低阈值签名与审计约束。
  - NFR-OMR-6: `stake/epoch` finality 校验单证书耗时 `p95 <= 50ms`（100 signer 快照，热缓存命中）。
  - NFR-OMR-7: 提供固定基准入口与产物（命令、日志、报告路径），可重复验证“校验耗时”与“2 epoch 收敛”。
- Security & Privacy:
  - 强制最小权限：发布节点仅持有自身签名密钥，不共享其他验证者私钥或阈值签名分片。
  - 签名私钥不得进入仓库或构建日志；日志仅记录 signer ID 与签名摘要。
  - 节点仅信任治理下发公钥集合，未知 signer 一律拒绝。
  - 本地 deterministic seed finality signer 仅允许开发/测试环境；生产环境必须禁用。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP: 引入线上发布清单与外部 finality 证书强制校验，生产禁用本地自签 apply。
  - v1.1: 完成 artifact_identity 生产强制签名化与 `identity_hash_v1` 兼容窗口下线。
  - v2.0: 完成 builtin 清单主真源去仓库化、信任根轮换自动化与全链路审计报表。
- Technical Risks:
  - 风险-1: 历史清单兼容窗口过短可能影响存量节点升级。
  - 风险-2: signer 运营流程不完善会导致发布延迟。
  - 风险-3: 多节点证明与签名聚合流程复杂度上升，需补足运行手册与告警。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-WORLD_RUNTIME-016 | TASK-WORLD_RUNTIME-017/018/019/027 | `test_tier_required` | 运行时仅接受线上 manifest + 签名身份用例；`ModuleRelease* -> release manifest` 映射回放一致性校验 | 模块加载与执行合法性 |
| PRD-WORLD_RUNTIME-017 | TASK-WORLD_RUNTIME-020/021/025/026 | `test_tier_required` + `test_tier_full` | finality 证书外部化、`stake/epoch` 阈值验签、key 轮换验证；本地自签路径禁用测试 | 治理 apply 安全边界 |
| PRD-WORLD_RUNTIME-018 | TASK-WORLD_RUNTIME-022/023/024/028/029 | `test_tier_required` | 去中心化提案/复构建证明/阈值签名闭环校验 + 发布 runbook/告警策略 + 节点侧固定验收脚本（`scripts/module-release-node-acceptance.sh`）+ finality 固定基准脚本（`scripts/oasis7-runtime-finality-baseline.sh`）+ 主 CI 去发布写入 + 基准指标产物校验 | 发布门禁与工程流程 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-OMR-001 | 线上签名清单作为合法性锚点 | 继续以仓库内置清单为主真源 | 仓库基线无法覆盖线上动态发布与跨节点一致执行需求。 |
| DEC-OMR-002 | 生产禁用 `identity_hash_v1` 回退 | 长期保留回退作为默认路径 | 回退路径无法提供可追责签名保证。 |
| DEC-OMR-003 | 外部 finality 证书强制化 | 运行时本地派生证书自动 apply | 本地可伪造路径与分布式治理目标冲突。 |
| DEC-OMR-004 | 生产发布完全去 CI 依赖 | 由 CI bot 充当唯一发布入口 | 去中心化运行目标要求生产发布在无中心服务下仍可达成。 |

## SR2 目标承接与实施证据边界

本专题 owner runtime_engineer；source baseline `9c41d57b4436f71f0cf9b481043d882cfdc551ed`；2026-09-26 作者内容审读，独立 review pending。原 SC-1..7、AC-1..17、PRD-WORLD_RUNTIME-016..018、NFR、DEC 与 TASK 历史身份及全部阈值仍有效。本次文档变更采纳 concrete target documentation；实现状态、历史 proof 和当前候选 proof 单独记，不由文档 active 或合入推断。

<a id="sr2-release-acceptance"></a>
### 发布、finality 与实际 execution 的独立接受关系

| 原专业接受关系 | 具体 target receiver | 实施与证明状态 | 独立验证要求 |
| --- | --- | --- | --- |
| SC-1/2、AC-2/3/4/8/9、PRD-WORLD_RUNTIME-017 | [release trust](online-module-release-legality-closure-2026-03-08.design.md#sr2-release-trust) | current production action gates 有 partial locators；prototype finality 非 BFT；full target external trust未证明 | actual signature bytes、epoch/set/stake/threshold/minsigners、certificate、negative local self-sign/unsigned/unknown proof，不以 CI/provider/localEffectReceipt 替代 |
| SC-3、AC-1、PRD-WORLD_RUNTIME-016 | [release trust](online-module-release-legality-closure-2026-03-08.design.md#sr2-release-trust) | online action authorization 不关闭 current builtin materializer source fallback；materializer 无 identity/receipt/policy input仍是能力差距 | verify fetched artifact exact bytes before load/cache；production hardening须实际关闭fallback且negative proof，不能从action tests推断 |
| SC-4/5/6/7、AC-5/6/7/10/11/13/14/15/16/17、PRD-WORLD_RUNTIME-018 | [release proof sequence](online-module-release-legality-closure-2026-03-08.design.md#sr2-release-trust) | current node submit/packaging 是 narrower entrypoint；submit ACK不是 finalized/applied；external worker availability未证明 | proof_cid archive/readback/hash/body、same-platform reproducibility、signer/platform去重/冲突、node-side submit与CI不可用实际flow、S11 runbook faults |
| AC-12、NFR-OMR-1..7（两条原 NFR-OMR-6 分别 submit 响应/finality latency） | [release validation](online-module-release-legality-closure-2026-03-08.design.md#sr2-release-validation) | 历史 TASK保留历史，不是该 target 的 fresh benchmark | p95<=200ms模块校验、100 signer热cache p95<=50ms、<=2epoch收敛、trace100%、forged acceptance0；fixed script与summary产物逐项证明 |
| new target activation/migration dependency of AC-2/8/9/10 | [VersionActivationV1](module-lifecycle.md#sr2-activation-contract) + [migration publication](module-lifecycle.md#sr2-migration-publication) | target/unimplemented；原 governed proposal seam只是partial | canonical inclusive height selects version，exact migration declarations，all-object atomic failure、old bytes/schedule retained，ACT-01/MIG-01..03 |
| external proof dependency of actual effect acceptance | [ExternalEffectReceiptV1](module-lifecycle.md#sr2-external-contract) + [host publication](../wasm/wasm-executor.design.md#sr2-host-publication) | provider immutable result与canonical verification分离，local unsigned/signable EffectReceipt不是productiontrust | RCP-01/02；raw provider response0 committed effect，verified successful module receipt才计1；wrong scope/request/root/epoch/proof拒绝，不承诺provider exactly-once |

该表承接专业接受关系，不增加产品模块或新的经济规则。Target receipt唯一 successful status为 committed_success；rejected/expired/replan_required/terminated_without_effect 是 IntentDecision，不算executionreceipt或lineagewinner。Missing/invalid governing manifest/head/finality 与非 verified_serviceable 的受影响 NEW intents均0新receipt/0effect/无charge/无state publication；历史receipt不可改。Service recovery重审权利、资源、expiry、preconditions；accepted/requestACK不是effect。

<a id="sr2-release-node-proof"></a>
### SC-9 / DC-5 组合证明保持独立

[GWSC actual DC5 receiver](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-dc5-attachment) 必须消费 execution+state-sync 同 candidate/world/canonical window 的真实附件，schema valid不等于pass。SC-9 两 outage classes × stage_finish/transit/buffer_admission/terminal_settlement 八完整 cells、fixed baseline identities/quantities/state/window/lease/receipt/W/progression/next action/recheck 均保留；required deterministic matrix + active LLM/provider pure API parity，full真实localstack/provider external headed desktop+narrow S6/Playwright/screenshots/console + provider Agent parity（decision source/backend/contract/transport），provider_local_mock不能替代。只有 gameplay canonical interruption可reset unfinished W一次；read-plane outage仅stale/unknown/reconcile，不能改世界/W。Release/receipt目标合同只提供依赖，不把genericWASM场景当这些cells完成证据。

## 既有发布结果的产品接收与身份定位

本节只将既有结果精确接入权威世界基础设施根；原 SC-1..7、AC-1..17、PRD-WORLD_RUNTIME-016..018、NFR（两条原 NFR-OMR-6）、DEC、TASK、阈值、profile 适用条件与上述全部原文不变。产品原文声明唯一身份，本专业条款与 AC 只 refs；source、字段、算法、签名、加载策略、测试参数和验收仍由本专业及其引用的 P2P/runtime/QA authority 拥有。文档接收不表示实现、正式环境或公开能力通过。

<a id="sr2-product-source-and-trust"></a>
### 生产来源、身份与外部授权的有限产品接收

- Trace refs: O7-REQ-7a3866ed-cd66-48ea-9cc7-bb793575c957；[生产模块来源受治理清单约束](../../product/world-infrastructure/prd.md#req-infra-release-source)；原 SC-3、AC-1、PRD-WORLD_RUNTIME-016、DEC-OMR-001，独立结果与条件以原文为准。
- Trace refs: O7-REQ-d01cf80e-1172-4963-8bc0-877dc9de8e00；[生产模块身份与签名合法性](../../product/world-infrastructure/prd.md#req-infra-release-identity)；原 SC-1、AC-2/3、NFR-OMR-4、DEC-OMR-002，独立结果与条件以原文为准。
- Trace refs: O7-REQ-1440a1a4-3240-4644-8ada-bf9a34771465；[发布批准必须来自合法外部证书](../../product/world-infrastructure/prd.md#req-infra-release-certificate)；原 SC-2、AC-4/8/9、PRD-WORLD_RUNTIME-017、DEC-OMR-003，独立结果与条件以原文为准。

该组不以 action authorization 关闭 materializer source fallback gap；原 snapshot/threshold/minsigners/生产禁自签和默认 bootstrap 限制全部保留。exact artifact 可验证性只是 DWE-004 的局部依赖，不由 DWE 身份代签本组生产来源/外部授权结果。

<a id="sr2-product-node-release"></a>
### 节点发布、平台构建与证据的有限产品接收

- Trace refs: O7-REQ-b89862c6-6316-4511-afdc-1e2d1e9a5e8e；[发布证明聚合满足当前授权门槛](../../product/world-infrastructure/prd.md#req-infra-release-attestation-authorization)；原 AC-14，独立结果与条件以原文为准。
- Trace refs: O7-REQ-a5a2d539-b2c8-42dd-83e0-a6108b5ab288；[发布阻断故障可分诊](../../product/world-infrastructure/prd.md#req-infra-release-release-triage)；原 AC-15、Technical Specifications 原 Edge Cases / S11，独立结果与条件以原文为准。
- Trace refs: O7-REQ-c9c94faf-43bb-474c-8cf9-7ed1441e5494；[生产发布不依赖 CI](../../product/world-infrastructure/prd.md#req-infra-release-ci)；原 SC-4/6、AC-6/11、NFR-OMR-5、DEC-OMR-004，独立结果与条件以原文为准。
- Trace refs: O7-REQ-c1427344-798f-47f1-9dab-790fd3693cf7；[合规节点可触发并提交发布证据](../../product/world-infrastructure/prd.md#req-infra-release-participation)；原 SC-4/7、AC-5/16、PRD-WORLD_RUNTIME-018，独立结果与条件以原文为准。
- Trace refs: O7-REQ-f25099a9-5f30-466e-939a-48ac57cbead2；[发布构建按平台可复现](../../product/world-infrastructure/prd.md#req-infra-release-reproducibility)；原 SC-5、AC-7，独立结果与条件以原文为准。
- Trace refs: O7-REQ-80d9506a-b1e5-4ac2-8d68-eb73ecfccd51；[发布证据完整可追溯与可回放](../../product/world-infrastructure/prd.md#req-infra-release-evidence)；原 SC-7、AC-5/10/13/17、AC-14/15 的证明绑定与审计子范围、NFR-OMR-2，独立结果与条件以原文为准。

任意合规节点触发与 CI 非必要分别判定；平台可复现和证据完整也不互相代签。request/proof/hash/body、字段绑定、signer/platform 去重/冲突及归档是实现手段，仍须证明它们支持的上述结果；node ACK 不证明 finality/activation。AC-14 的当前 epoch signer 聚合仍须原阈值证据；AC-15 的三类 runbook 阻断场景保持独立。

<a id="sr2-product-release-quality"></a>
### 既有质量目标与验证机制的有限产品接收

- Trace refs: O7-REQ-02b10123-7422-4509-9d04-ea43444890c6；[信任根版本在既有窗口收敛](../../product/world-infrastructure/prd.md#req-infra-release-convergence)；原 NFR-OMR-3、AC-12，独立结果与条件以原文为准。
- Trace refs: O7-REQ-c2a3ca44-d9b3-4ae0-8245-d8f496f38673；[缓存命中时模块验证耗时](../../product/world-infrastructure/prd.md#req-infra-release-module-latency)；原 NFR-OMR-1、AC-12，独立结果与条件以原文为准。
- Trace refs: O7-REQ-8d0fe4a1-7c99-42d2-8b5d-7c4990591993；[既有快照规模下证书验证耗时](../../product/world-infrastructure/prd.md#req-infra-release-certificate-latency)；原 NFR-OMR-6（finality latency）、AC-12，独立结果与条件以原文为准。
- Trace refs: O7-REQ-0ed26c5c-b1a1-4e51-a462-56bd4712e295；[节点证明提交结果可分诊](../../product/world-infrastructure/prd.md#req-infra-release-diagnostics)；原 NFR-OMR-6（submit 响应）、AC-16，独立结果与条件以原文为准。
- Trace refs: O7-REQ-d01cf80e-1172-4963-8bc0-877dc9de8e00；[生产模块身份与签名合法性](../../product/world-infrastructure/prd.md#req-infra-release-identity)；原 SC-1、AC-2/3、NFR-OMR-4、DEC-OMR-002，独立结果与条件以原文为准。
- Trace refs: O7-REQ-c9c94faf-43bb-474c-8cf9-7ed1441e5494；[生产发布不依赖 CI](../../product/world-infrastructure/prd.md#req-infra-release-ci)；原 SC-4/6、AC-6/11、NFR-OMR-5、DEC-OMR-004，独立结果与条件以原文为准。
- Trace refs: O7-REQ-80d9506a-b1e5-4ac2-8d68-eb73ecfccd51；[发布证据完整可追溯与可回放](../../product/world-infrastructure/prd.md#req-infra-release-evidence)；原 SC-7、AC-5/10/13/17、AC-14/15 的证明绑定与审计子范围、NFR-OMR-2，独立结果与条件以原文为准。

原 AC-12 与 NFR-OMR-7 的固定脚本、命令、日志、summary.md/summary.json 和可重复测量是验证方法，仍为必需的专业验收，不能因为方法不新建产品功能而省略。两条原 NFR-OMR-6 按 submit 响应与 finality latency 分别保留；100 signer 热缓存/本地缓存、200ms/50ms、2epoch、trace100%、forged0、CI-off 都不得互相代签。上述 root refs 仅接收各独立结果，不将整组归为 product N/A。

<a id="sr2-product-version-dependency"></a>
### 版本激活与迁移的有限产品依赖

Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；[REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004) 的原 canonical block governing version、inclusive activation boundary、原 manifest/artifact/schedule 历史 replay 与缺证 fail-closed 依赖，经 [activation authority](module-lifecycle.md#sr2-activation-contract) 与 [migration publication](module-lifecycle.md#sr2-migration-publication) 承接。本组仍是原 AC-2/8/9/10 的 target dependency，不重发 DWE 身份，不涵盖 CI/任意节点可用性或完整 BFT。schema/corehash、all-object staging 和单一 durable commit 是履行既有原子激活/失败无 partial publication 的专业手段，不新建产品行为；该结果与原 ACT-01/MIG-01..03 验收不被豁免。所有 target/unimplemented 与 partial 状态仍有效。

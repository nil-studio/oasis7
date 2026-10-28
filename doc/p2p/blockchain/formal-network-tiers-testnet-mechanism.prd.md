# oasis7 正式网络分层与 testnet 机制

- 对应设计文档: `doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.design.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

审计轮次: 2
建档日期: 2026-05-14

## 当前 schema 与目标解耦边界

v1 `network_tier_manifest.rs` 保留 `local_devnet=preview+ephemeral`、`public_testnet=testnet+resettable`、`mainnet=production+frozen` 的兼容校验；下文旧 schema、tier 表和组合规则适用于 v1，不代表世界保留承诺的唯一合法组合，也不证明持久单权威已实现。

目标在既有 manifest 中分别表达网络环境/发行阶段、世界生命周期/保留承诺、合法提交 authority profile/激活版本，以及资产价值/faucet/结算资格。

受限 preview 的正式世界可承诺持久保留合法身份、设施、材料、资格及来源；生产结算资格由经济规则与发行条件独立决定，修改 tier 不升值测试奖励，也不能因此清空已承诺世界。

`frozen + preview` 只是需求组合；`token_policy.reset_policy` 不足以代表全世界保留，字段归属、兼容迁移、示例与 readiness 消费者须随实现共同闭合，不能仅放宽枚举后宣称保证成立。

固定 `world_id`、`chain_id`、`genesis_hash` 与可演进软件/runtime manifest/authority 分开；后者只能按同世界历史合法升级。已有保留承诺时沿既有身份接续，隔离 local/dev 不并入长期世界。正式提交 profile、持久性和交接验收见 [P2P 合同](../prd.md#p2p-authority-profiles)；当前公开状态仍以根 README 和同候选任务证据为准。v2 的计划配置声明不授予运行权威。

<a id="p2p-tier-planned-world-policy"></a>
### PRD-P2P-TIER-002：v2 计划配置与运行支持边界

v2 在同一 manifest 增加 `release_policy.stage`、`world_policy.world_id/retention/reset_policy` 与 `authority_policy.profile/profile_version/activation`。首个支持的声明组合为 `limited_preview`、`preview` 价值、`persistent/frozen` 世界保留及 `controlled_single_authority` version `1`、`planned` 激活；环境 tier 独立声明。v2 的 token policy 不再拥有世界 reset；同时声明两处 reset 或缺少新 policy 均拒绝。v1 不接受这些 v2 policy，避免反序列化忽略字段后进入旧协议。

`AC-P2P-TIER-PLANNED-001`：Rust 与脚本使用同一正负 fixture 验证版本及组合；v1 结果保持兼容。v2 只接受 planned 且不得 `status=live`，生产结算与 mainnet/live claims 保持禁止。world_id 是计划的目标身份，须与 genesis_ref 中的 world_id、chain_id 对齐；genesis_ref 仍是工件引用，此校验不证明固定 genesis_hash、既有世界、独立副本或保留承诺已经成立。可复用[计划配置模板](../../testing/templates/network-tier-persistent-preview-planned.example.json)，其中引用的身份工件须由目标计划提供。

`AC-P2P-TIER-PLANNED-002`：schema-valid 与 runtime-supported 分开。chain runtime 与 launcher 在任何导入/执行状态写入前拒绝 planned v2；readiness/exit review 即使旧 lanes 全 pass 也不能给出 ready/live。旧 BFT/threshold verifier 与 light-client 证据入口不得替 v2 controlled profile 放行。正式 active 的历史激活证明、提交日志和 fault-domain 条件由后续 [Issue #4363](https://github.com/eng-cc/oasis7/issues/4363) 实施，不由配置文件激活。

## 1. Executive Summary
- Problem Statement: oasis7 现在需要把“本地 / test / 正式”三套环境讲清楚；历史 `shared_devnet/staging/canary` 容易被误读成目标 test 环境，而 `mainnet` 又容易被误读成“等 mainnet gates 文档齐了就能直接上线”。
- Proposed Solution: 冻结一份 producer-owned 的正式网络分层 PRD，明确 `local_devnet -> public_testnet -> mainnet` 的 operator/runtime tier、各层 network manifest 真值、faucet/reset/validator/claims policy 边界，以及 repo-owned script/config skeleton；`shared_devnet` 只作为 legacy/rehearsal evidence 追溯，不再作为目标环境入口。Network tier 是统一持久大世界的运行/验证载体分层，不是玩家可见的多个世界模型。
- Success Criteria:
  - SC-1: 明确冻结 operator/runtime network-tier 模型：`local_devnet`、`public_testnet`、`mainnet`，并写清各层目标、访问方式、价值语义与允许 claims；`shared_devnet` 仅保留 legacy/rehearsal 语义，且这些 tier 不作为玩家世界名。
  - SC-2: `public_testnet` 必须拥有一套正式 manifest 字段集合，至少覆盖 `network_id/chain_id/genesis_ref/release_candidate_bundle_ref/bootstrap_peer_ref/public_rpc/explorer/faucet/reset_policy/validator_admission`。
  - SC-3: `mainnet` 必须明确绑定 `no faucet + no reset + frozen genesis + governance registry + MAINNET readiness gates`，不得与 `public_testnet` 共用“可随时重置”的语义。
  - SC-4: 仓库内必须落地 repo-owned skeleton：network-tier manifest create/validate 脚本、smoke、example manifests 与 `testing-manual` 入口。
  - SC-5: 本专题必须明确 current verdict 由 companion project/runbook 与 readiness 脚本汇总；早期 `specified_skeleton_only` 只能作为 skeleton 阶段历史状态，任何 governed-bootstrap rehearsal、lane evidence 或脚本骨架都不等于已存在 live `public_testnet` 或 live `mainnet`。
  - SC-6: 本专题必须补一份 repo-owned `public_testnet` live-candidate companion checklist/runbook，把 required-lane readiness gate、owner、最小 evidence、canonical 命令与 claim boundary 冻结成可执行入口，避免“还差什么”只停留在聊天结论。

## 2. User Experience & Functionality
- User Personas:
  - `producer_system_designer`：需要固定正式口径，明确旧 shared_devnet rehearsal 字段何时仅表示历史共享预演，以及何时才能称为 public_testnet / mainnet。
  - `runtime_engineer`：需要知道一个网络 tier 至少要固定哪些字段，后续 runtime/config 才能围绕单一 manifest 接线。
  - `qa_engineer`：需要知道 testnet/mainnet 的 promotion gate、reset/faucet 边界和 current verdict，避免把 rehearsal 当上线。
  - `liveops_community`：需要知道哪些 tier 可以公开给外部访问，哪些还只能作为团队内部共享轨道。
  - 外部测试者 / validator 候选：需要知道 public testnet 能做什么，不能把 testnet 资产、faucet 或节点准入误认为 mainnet 承诺。
- User Scenarios & Frequency:
  - 每次准备对外开放 `public_testnet` 或让 `mainnet` 进入正式 gate 前执行一次。
  - 每次准备冻结公共 RPC、explorer、faucet 或 testnet reset policy 时执行一次。
  - 每次出现“这个环境算 testnet 还是 mainnet”类争议时复用。
- User Stories:
  - PRD-P2P-TIER-001: As a `producer_system_designer`, I want one formal network-tier model, so that shared release-train and mainnet readiness no longer float as disconnected concepts.
  - PRD-P2P-TIER-002: As a `runtime_engineer`, I want one machine-readable network-tier manifest schema, so that environment bootstrap and release pinning can wire to one source of truth.
  - PRD-P2P-TIER-003: As a `qa_engineer`, I want explicit public-testnet and mainnet gates, so that promotion decisions can block on missing faucet/reset/governance evidence.
  - PRD-P2P-TIER-004: As a `liveops_community`, I want claims boundaries tied to tier state, so that external communication stays behind execution truth.
- Critical User Flows:
  1. Flow-P2P-TIER-001: `producer` 先为某一网络层生成 `network_tier_manifest`，固定 tier、network id、genesis、candidate bundle、endpoint policy 与 claims boundary。
  2. Flow-P2P-TIER-002: `runtime_engineer` 以 pinned bundle、genesis、bootstrap peers 和 governed-bootstrap rehearsal 证据为输入，提出 `public_testnet` manifest，声明 public RPC/explorer/faucet/reset/validator admission；legacy `shared_devnet` evidence 可辅助追溯，但不再作为必需 promotion gate。
  3. Flow-P2P-TIER-003: `qa_engineer` 对照 manifest 与 gate 结论决定该 tier 是 `planned/specified_skeleton_only/rehearsal/live` 中哪一档，并决定是否允许 public claims。
  4. Flow-P2P-TIER-004: `mainnet` 申请时，系统必须同时检查 `public_testnet exit review + MAINNET-1~4 + frozen genesis + no-reset commitment`；任一缺失即阻断。
- Functional Specification Matrix:
| 功能点 | 字段定义 | 动作行为 | 状态转换 | 计算/判定规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| Network tier manifest | `tier/status/network_id/chain_id/release_candidate_bundle_ref/genesis_ref/bootstrap_peer_ref/endpoint_policy/validator_policy/token_policy/claims_policy/promotion_policy` | 为一个网络层冻结机器可读真值 | `draft -> validated -> referenced` | 字段缺失或 tier 语义冲突时不得通过 validate | `producer_system_designer` 冻结语义，`runtime_engineer` 负责技术接线 |
| `shared_devnet` boundary | `visibility=shared_operator`、`value_semantics=preview`、`faucet_mode=none|operator_grant` | 只保留 legacy/rehearsal evidence 语义，不再作为目标 test 环境 | `legacy -> referenced` | 即便已有 shared access，也不自动等于 public RPC/faucet/testnet claim | `runtime_engineer`/`qa_engineer` 维护 |
| `public_testnet` policy | `visibility=public`、`value_semantics=testnet`、`faucet_mode=guarded_testnet_faucet`、`reset_policy=resettable`、`validator_admission=allowlist_or_governed_candidate` | 对外开放 public testnet，并明确它仍可重置、资产无 mainnet 价值承诺 | `planned -> specified_skeleton_only -> rehearsal -> live` | 没有 public RPC/explorer/faucet/reset policy 时不得叫 public testnet | `liveops_community` 执行公开面，producer 审批 |
| `mainnet` policy | `visibility=public`、`value_semantics=production`、`faucet_mode=none`、`reset_policy=frozen`、`validator_admission=governance_registry_only` | 宣告正式价值网络 | `planned -> gated -> live` | 只有 `MAINNET-1~4`、frozen genesis、no-reset commitment 全部具备时才允许 | `producer_system_designer` 审批，`qa_engineer` 阻断 |
| Claims gate | `allowed_claims/denied_claims/required_gates` | 根据 tier 决定允许哪些公开口径 | `draft -> enforced` | legacy `shared_devnet` 与 `public_testnet` 默认 deny `mainnet-grade live network` | `liveops_community` 执行，producer 审批 |
- Acceptance Criteria:
  - AC-1: 本专题必须落地 PRD / design；任务追溯写入 GitHub，并接入 `doc/p2p/prd.md`、GitHub Issue / GitHub Project、`doc/p2p/prd.index.md` 与 `testing-manual.md`。
<a id="tier-model-acceptance"></a>
### 网络分层验收

  - AC-2: 本专题必须明确 `local_devnet -> public_testnet -> mainnet` 三层 operator/runtime network-tier 模型，且明确 `shared_devnet` 只作为 legacy/rehearsal evidence，不是目标 test 环境，也不是玩家世界模型。

Trace refs: O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385；[REQ-ENTRY-CLAIM-003](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-003)。适用子义务：网络 tier 是运行/验证载体而不是多个玩家世界；本组为公开说明提供该专业适用范围，不把目标分层作为当前可用性。原验收和专业 owner 保持有效，不代表该产品要求完整实现或验证。
<a id="tier-manifest-fields-acceptance"></a>
### Manifest 字段验收

  - AC-3: `public_testnet` 的 manifest 必须至少包含 `network_id`、`chain_id`、`release_candidate_bundle_ref`、`genesis_ref`、`bootstrap_peer_ref`、`rpc_ref`、`explorer_ref`、`faucet_ref`、`reset_policy`、`validator_admission`、`allowed_claims` 与 `denied_claims`。
  - AC-4: `mainnet` 的 manifest 必须至少包含 `network_id`、`chain_id`、`release_candidate_bundle_ref`、`genesis_ref`、`bootstrap_peer_ref`、`rpc_ref`、`reset_policy=frozen`、`faucet_mode=none`、`validator_admission=governance_registry_only`，并把 `MAINNET-1~4` 写入 required gates。

Trace refs: O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385；[REQ-ENTRY-CLAIM-003](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-003)。适用子义务：各 tier 的字段和 reset/faucet/准入/gates 差异是公开表述的专业输入；字段齐备不等于运行、readiness 或发布。原验收和专业 owner 保持有效，不代表该产品要求完整实现或验证。
<a id="tier-skeleton-manual-acceptance"></a>
### Skeleton 与手册验收

  - AC-5: 仓库内必须新增 repo-owned `scripts/network-tier-manifest.sh` 与 smoke，并提供 `public_testnet` rehearsal、`public_testnet`、`mainnet` example manifests；旧 shared-devnet 模板路径不再作为正常 validation artifact。
  - AC-6: `testing-manual.md` 必须新增正式 network-tier skeleton 入口，并明确当前仍无 live `public_testnet` / `mainnet`。

产品适用性：N/A；理由：本组只要求 repo-owned create/validate/smoke/example 和测试手册入口，属于专业开发与验证工具，不新增玩家能力或公开服务承诺；范围：AC-5/6 的脚本、样例和手册交付；本组原验收及测试责任不豁免。
<a id="tier-test-value-acceptance"></a>
### 测试价值声明验收

  - AC-7: 本专题必须明确 `public_testnet` 的资产与 faucet 只用于 rehearsal/test surface，不得被写成 `OC` 的 mainnet 价值承诺。

Trace refs: O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385；[REQ-ENTRY-CLAIM-003](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-003)。适用子义务：测试资产与 faucet 的专业语义不得升级成 mainnet 价值事实；本组不规定新的资产价格或经济资格。原验收和专业 owner 保持有效，不代表该产品要求完整实现或验证。
<a id="tier-verdict-evidence-acceptance"></a>
### 候选证据与公开边界验收

  - AC-8: 本专题必须明确 current verdict 以后续 companion project/runbook 为准；在 formal all-pass lane TSV / readiness verdict 出现前，不提升 public claims，不宣称 live `public_testnet` 或 mainnet ready。
  - AC-9: 必须新增 companion runbook，至少列出 `public_rpc_ready/explorer_public_ready/faucet_guard_ready/reset_policy_announced/runtime_bootstrap/world_resource_provenance_ready/provider_resource_provenance_ready/resource_delta_replay_ready/api_viewer_projection_ready/same_world_hosted_entry_ready/claims_boundary_review` required lanes 的 owner、最小 evidence、阻断条件、建议执行顺序与 canonical 命令，并明确在真实证据补齐前仍只能维持 rehearsal/skeleton 口径。

Trace refs: O7-REQ-e20ed8c7-6f8d-400a-a883-57b2e772acff；[REQ-ENTRY-CLAIM-001](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-001)。适用子义务：current verdict 与 required lanes 必须绑定候选证据；runbook/checklist 或历史全绿不单独证明当前 live 状态或允许发布。原验收和专业 owner 保持有效，不代表该产品要求完整实现或验证。
### 专题非目标

- Non-Goals:
  - 不在本专题内直接部署 live `public_testnet` 或 live `mainnet`。
  - 不在本专题内落 runtime 的多 network profile 切换实现。
  - 不在本专题内修改共识、经济模型或交易协议。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用。
- Evaluation Strategy: 不适用。

## 4. Technical Specifications
- Architecture Overview: 正式网络分层收束为 operator/runtime 目标三层：本地 `local_devnet`、测试 `public_testnet`、正式 `mainnet`。这些 tier 是统一持久大世界的运行/验证载体，不是多个玩家世界。历史 `shared_devnet/staging/canary` 继续可作为内部 legacy/rehearsal evidence 追溯，但不再作为目标 test 环境；`public_testnet` 是可公开访问、可 reset、带 faucet 的 rehearsal 网络；`mainnet` 是 frozen genesis、no-reset、no-faucet、受治理准入约束的正式价值网络。各层通过同一 `network_tier_manifest` schema 固定 tier 语义。
- Integration Points:
  - `doc/testing/benchmarks/mainstream-public-chain-testing-benchmark.prd.md`
  - `doc/testing/evidence/legacy-shared-devnet-provenance-2026-07-26.md`（仅 legacy rehearsal provenance）
- `doc/p2p/blockchain/p2p-mainnet-security-governance-readiness.prd.md`
  - `README.md`
  - `doc/product/player-entry-distribution/release-communications-and-public-claims.prd.md`
  - `doc/p2p/network/mainnet-private-reachability-architecture.prd.md`
  - `doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.runbook.md`
  - `testing-manual.md`
  - `scripts/release-candidate-bundle.sh`
  - `scripts/shared-network-track-gate.sh`
- Edge Cases & Error Handling:
  - 若一个环境只有 shared operator access、没有 public RPC/explorer/faucet，则最多仍是 `shared_devnet`，不能叫 `public_testnet`。
  - 若 `public_testnet` 没有 reset policy 或默认宣称“永不重置”，则必须判定 schema 语义冲突，避免与 mainnet 边界重叠。
  - 若 `mainnet` manifest 仍带 `faucet_mode != none`、`reset_policy != frozen` 或缺 `MAINNET-*` gates，则 validate 必须失败。
  - 若 `public_testnet` 允许 claim phrase 包含 `mainnet live`、`production OC settlement` 之类语句，则必须判为 policy 越界。
  - 若 tier manifest 引用的 candidate bundle、genesis 或 bootstrap ref 为空，则该 tier 只能停留在 `draft`，不得进入 release review。
- Non-Functional Requirements：以下精确分组保留原五项专业要求，不改变 v1 兼容与目标 schema 解耦边界。

<a id="tier-manifest-encoding-and-public-fields"></a>
### Manifest 编码与公开字段专业要求

  - NFR-P2P-TIER-1: network tier manifest 必须保持 ASCII JSON，可在 repo 中追踪并可由脚本 create/validate。
  - NFR-P2P-TIER-5: tier manifest 中只允许记录 public refs、bundle refs 与政策字段，不得落私钥、助记词或 operator 私密登录信息。

产品适用性：N/A；理由：本组仅约束专业 manifest 的 ASCII 序列化、可追踪性及禁止写入凭据，不定义玩家能力或对外可用性承诺；范围：NFR-P2P-TIER-1/5 的配置格式与信息卫生。原 schema 校验、人工检查和安全义务全部保留。

<a id="tier-policy-semantic-validation"></a>
### Tier policy 语义校验与声明边界

  - NFR-P2P-TIER-2: `public_testnet` 与 `mainnet` 的 validate 必须显式检查 tier 语义冲突，不能只做字段存在性校验。
  - NFR-P2P-TIER-3: 任何 `mainnet` manifest 都不得允许 `resettable`、`guarded_testnet_faucet` 或 `value_semantics=testnet`。

Trace refs: O7-REQ-f9218cff-4a0e-4a40-8a2f-133d9c824385；[REQ-ENTRY-CLAIM-003](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-003)。本组仅提供该产品要求的专业 tier 语义输入：拒绝互相矛盾的测试/正式配置，不能把 schema-valid 或目标配置当作当前正式能力。它不更改既有 v1 reset/faucet 规则，不替代完整公开审核、当前证据或运行激活。

<a id="tier-public-availability-evidence-boundary"></a>
### Public availability 的证据边界

  - NFR-P2P-TIER-4: 在 live `public_testnet` 尚未建立前，对外文档不得把示例 manifest、skeleton script 或 `shared_devnet` rehearsal 当成 public availability 证据。

Trace refs: O7-REQ-e20ed8c7-6f8d-400a-a883-57b2e772acff；[REQ-ENTRY-CLAIM-001](../../product/player-entry-distribution/release-communications-and-public-claims.prd.md#req-entry-claim-001)。本组承接同一候选/版本与证据窗口的公开事实边界，具体禁止以示例、skeleton 或历史 rehearsal 代签当前 public availability；不声称已执行渠道发布或全量证据审核。


### 安全与隐私边界

- Security & Privacy: 本专题只冻结网络层级真值与验证约束，不新增密钥管理方案。凡涉及 signer、custody、genesis freeze 的敏感部分，继续由主网安全、治理与创世就绪度专题约束，禁止把任何私密凭据写入 manifest 示例或测试文档。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP: 冻结三层 operator/runtime network-tier 模型、manifest schema、claims boundary 与 repo-owned skeleton；该分层不作为玩家世界模型。
  - v1.1: 把 `public_testnet` governed-bootstrap rehearsal、public endpoint、faucet/reset/claims evidence 与 runbook 正式接线，避免把 legacy shared-devnet evidence 误读成目标 test 环境。
  - v1.2: 落第一轮 public testnet rehearsal，补齐 public RPC/explorer/faucet/reset evidence。
  - v2.0: 在 `public_testnet` exit review 与 `MAINNET-1~4` 全绿后，再讨论 mainnet manifest 激活。
- Technical Risks:
  - 风险-1: 如果继续把 legacy `shared_devnet` 和 `public_testnet` 混用，团队会在 claims、faucet、reset 承诺上持续越界。
  - 风险-2: 如果 `mainnet` 没有单独 schema 约束，后续很容易把 preview/testnet 运行习惯直接带进正式网络。
  - 风险-3: 如果只有文档没有 manifest skeleton，后续 runtime/liveops 会再次把 tier 真值散落到各自脚本里。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-P2P-TIER-001 | TIER-0 | `test_tier_required` | 专题 PRD/design/project 建档并回写模块主追踪 | p2p 模块正式网络口径 |
| PRD-P2P-TIER-002 | TIER-1 | `test_tier_required` | `network-tier-manifest.sh` create/validate + example manifests + smoke | network tier config skeleton |
| PRD-P2P-TIER-003 | TIER-2/3 | `test_tier_required` | public testnet / mainnet gate 字段、claims boundary 与 `testing-manual` 入口冻结 | QA / liveops promotion 边界 |
| PRD-P2P-TIER-004 | TIER-3/4 | `test_tier_required` | current verdict、promotion prerequisites 与 deny claims 回写 | 对外口径与后续 mainnet 进入条件 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-P2P-TIER-001 | 目标环境收束为 `local_devnet`、`public_testnet`、`mainnet`，`shared_devnet` 仅作 legacy/rehearsal evidence | 继续把 `shared_devnet` 作为目标 test 子环境 | 用户已确认 `shared_devnet` 不要了；`shared_devnet` 不能替代公共 testnet。 |
| DEC-P2P-TIER-002 | 先冻结 repo-owned manifest schema 和示例，再推进 runtime/ops 接线 | 先做 live public testnet，之后再补 schema | 没有 manifest 真值，后续 public tier 很容易继续靠口头描述推进。 |
| DEC-P2P-TIER-003 | `mainnet` 必须单独校验 `no faucet + frozen reset + MAINNET-* gates` | 让 mainnet 复用 public testnet schema，只靠额外注释区分 | 正式价值网络不能依赖“大家知道这是 mainnet”的口头约定。 |

# core PRD

审计轮次: 6

本页承载跨模块专业治理与导航，不是第五个产品模块。通用开发与交付流程以[开发流程规范](../engineering/workflow/source-of-truth.md)为准；历史 ROUND、Task 和审计标记不构成当前任务准入材料。

本文的阶段收口、发布评审、角色矩阵和计量目标仅适用于明确开展的跨模块阶段/发布工作，不是普通 PR 的固定角色组合、额外审批材料或另建台账要求。普通修改按受影响行为选择验证；真实发布阻断、已确认历史/权限/数据安全风险和有效暂停仍须满足，历史模板不能豁免它们。

## 目标
- 作为项目级总 PRD，提供 oasis7 的全局设计入口。
- 统一跨模块边界、关键链路、术语口径与验收基线。
- 确保各模块改动可追溯到适用要求、实际 PR 与测试证据；Issue 和 Project 按需使用。

## 范围
- 覆盖全项目模块地图、端到端链路、关键分册导航与治理基线。
- 覆盖适用 PRD-ID 到实际 PR、按需 Issue 与验证记录的关系。
- 不覆盖各模块实现细节正文（由模块 PRD 与专题分册承载）。

## 接口 / 数据
- 项目级 PRD 入口: `doc/core/prd.md`
- 可变项目管理真值: PR、实际 CI 与评审记录（Issue 按需）
- 文件级索引: `doc/core/prd.index.md`
- core 局部条目标识: `PRD-CORE-xxx`；新增或实质修改的跨层要求按[要求追踪规范](../engineering/doc-governance/cross-layer-requirements-traceability.design.md)复用同一全局 Trace ID，局部标识继续保留。
- 模块入口总览: `doc/README.md`
- 测试与发布参考: `testing-manual.md`

## 里程碑
- M1 (2026-03-03): 完成模块 PRD 体系重构并建立项目级总览入口。
- M2: 固化跨模块变更影响检查清单（设计/代码/测试/发布）。
- M3: 建立 PRD-ID -> Task -> Test 追踪报表。
- M4 (2026-03-10): 建立阶段收口优先级与跨角色执行口径，统一玩法 / runtime / testing / playability / headless 的发布前闭环目标。
- M5 (2026-03-19): core 记录 `viewer / pure_api` 玩家访问模式与 `execution lane` 的分层；模式定义以[玩家访问模式产品要求](../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001)及其[验收](../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#ac-entry-mode-001)为准，core 保留跨模块验收与证据追踪。

## 风险
- 模块并行演进过快时，全局总览可能滞后于真实实现。
- 模块间术语不统一会造成评审误判与接口漂移。

## 1. Executive Summary
- Problem Statement: 项目已拆分为多个模块 PRD，但缺少一个“只读一份文档即可掌握整体设计”的全局总入口。
- Proposed Solution: 在 core PRD 中固化项目全局模块地图、关键端到端链路、关键分册导航和统一治理口径，使其成为仓库级设计总览。
- Success Criteria:
  - SC-1: `doc/README.md` 将 `doc/core/prd.md` 作为推荐阅读第一入口。
  - SC-2: core PRD 明确列出全部模块职责、关键链路与关键分册。
  - SC-3: 跨模块改动评审可基于 core PRD 完成影响面识别。
  - SC-4: 新增模块级需求可映射到对应模块 PRD 与 core 基线。
  - SC-5: 当前阶段优先级（P0/P1/P2）在 core PRD 中有唯一口径，并能映射到对应模块任务与角色 owner。
  - SC-6: 首次开放前，当前路径的持久恢复、工业闭环与消费者控制须有同候选证据；真人观察在开放条件成立后单独进行。
  - SC-7: 专业工作按是否阻断当前路径排序，专题优先级不自动成为全局 P0。
  - SC-8: core 跨模块评审以[玩家访问模式产品要求](../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001)及其[验收](../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#ac-entry-mode-001)作为 `viewer / pure_api` primary mode 的规范依据；每项模式相关结论均映射到对应产品/专业 authority 和模式适用证据，core 保留跨模块影响、验收与追踪责任，不复制 alias、provider、execution lane 或公开 claim taxonomy。
  - SC-9: core 活跃专题标题、Viewer 活跃手册与实际窗口/Web 标题对齐 `oasis7` 品牌；内部旧品牌兼容命名仅以实现说明形式保留，不得继续冒充公开标题。
  - SC-10: `engineering`、`scripts`、`world-runtime` 的历史专题标题在不改动内部实现标识的前提下完成 `oasis7` 品牌收口，减少 active/historical 入口里的旧品牌混用。

## 2. User Experience & Functionality
- User Personas:
  - 架构负责人：需要一份文档快速把握全局设计与边界。
  - 模块维护者：需要明确自己模块在全局链路中的位置与依赖。
  - 发布负责人：需要统一口径判定跨模块风险与放行条件。
  - `producer_system_designer`：需要统一当前阶段优先级、owner 分工与完成定义，避免团队继续平均发力。
- User Scenarios & Frequency:
  - 架构评审：每次跨模块需求评审前至少 1 次，核对影响边界与依赖。
  - 模块联调：每周多次，按链路检查上游/下游耦合点是否一致。
  - 发布评估：每个版本候选至少 1 次，基于统一门禁做 go/no-go 判定。
  - 新成员入项：入项首日使用，快速建立项目全局认知。
  - 阶段收口评审：每轮版本收口前至少 1 次，明确 P0/P1/P2 优先级、owner、依赖与阻断项。
- User Stories:
  - PRD-CORE-001: As an 架构负责人, I want a project-wide blueprint, so that I can reason about cross-module impact quickly.
  - PRD-CORE-002: As a 模块维护者, I want one place to see end-to-end chains, so that I can design compatible changes.
  - PRD-CORE-003: As a 发布负责人, I want unified release/test governance, so that go/no-go decisions are auditable.
  - PRD-CORE-004: As a `producer_system_designer`, I want a stage-closure source of truth, so that the team aligns on what must ship first, who owns it, and what evidence is required before release.
  - PRD-CORE-005: As a `producer_system_designer`, I want a ranked next-round priority slate, so that the team starts the new cycle from one agreed execution path instead of diffusing effort.
- PRD-CORE-006: As a `producer_system_designer`, I want a formal version-candidate go/no-go entry after readiness reaches `ready`, so that release approval, residual risks, and role handoff are explicit and auditable.
- PRD-CORE-007: As a 新协作者, I want `doc/README.md` to include the current public-preview reading path, so that I start from the right entry points.
- PRD-CORE-008: As a `producer_system_designer`, I want the global docs hub synced with repo/site posture, so that navigation stays consistent.
- Critical User Flows:
  1. Flow-CORE-001: `读取模块地图 -> 识别改动所属模块 -> 定位上下游依赖 -> 形成影响面清单`
  2. Flow-CORE-002: `读取关键链路 -> 映射到模块 PRD-ID -> 对照测试分层 -> 输出发布风险判断`
  3. Flow-CORE-003: `发现口径冲突 -> 回溯分册来源 -> 在 core 基线中统一术语与边界 -> 回写模块文档`
  4. Flow-CORE-004: `评估当前项目状态 -> 划分 P0/P1/P2 -> 指定跨角色 owner / 输入 / 输出 / Done -> 在当前 PR 或按需 Issue 记录执行范围`
  5. Flow-CORE-005: `收集玩法 / runtime / testing / playability / headless 证据 -> 对照阶段收口门禁 -> 形成 go / no-go 结论`
  6. Flow-CORE-006: `确认本轮 completed -> 汇总候选缺口 -> 划分 P0/P1/P2 -> 选定下一条执行主路径`
  7. Flow-CORE-007: `读取玩家访问模式产品 REQ/AC -> 按其 primary mode 约束映射产品/专业 authority 与适用 evidence -> 保留 core PRD-ID 和跨模块验收追踪 -> 输出不越界的跨模块结论`
- Functional Specification Matrix:
| 功能点 | 字段定义 | 按钮/动作行为 | 状态转换 | 排序/计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 模块地图导航 | 模块名、职责、关键载体、入口路径 | 进入模块 PRD/design 与 PR 与实际验证记录 | `draft -> reviewed -> published` | 默认按模块分层顺序展示 | 所有贡献者可读，维护者可改 |
| 关键链路追踪 | 链路名称、上游、下游、测试门禁 | 依据链路定位依赖变更与测试范围 | `identified -> validated -> archived` | 高风险链路优先检查 | 发布负责人具备最终裁定权 |
| 术语与口径统一 | 术语名、定义、引用文档、更新时间 | 发现冲突后统一定义并回写引用 | `conflict -> resolved -> synced` | 按条款内容与 scope 回到对应 authority；core 词典只汇总 | 相关专业 owner 裁定本域语义，core 同步跨模块引用 |
| 阶段优先级台账 | 优先级层级、目标、owner、输入、输出、验收标准、阻断条件 | 评审后确认 `P0/P1/P2` 与 owner 映射，并记录在实际 PR 或按需 Issue | `candidate -> aligned -> executing -> gated -> released` | `P0 > P1 > P2`；P0 未完成时不得提升 P1/P2 为发布结论主路径 | `producer_system_designer` 拥有排序权；模块 owner 负责承接执行 |
| 跨角色证据矩阵 | 发起角色、接收角色、输入、产出物、回写位置、验证方式 | 发起方在当前 PR/Task 记录实际交接与 CI/评审证据，接收方确认责任边界；owner 仅将受影响的长期要求、验收边界及有来源的状态摘要同步到正式 PRD，详细过程留在 PR/Task | `requested -> reviewed -> accepted -> verified` | 先按当前 工作说明或按需 Issue owner 排序，再按发布风险高低排序 | 记录实际负责人及对应专业职责；需要独立评审时遵循 workflow，不要求额外固定角色卡或文件链 |
| 发布收口门禁 | P0/P1/P2 状态、证据路径、阻断结论、例外说明、复审时间 | 汇总证据并输出 `go/no-go/conditional-go` | `not_ready -> conditionally_ready -> ready -> released` | 缺任一 P0 证据时强制 `not_ready` | 发布负责人给出结论，core owner 负责口径一致性 |
| 下一轮优先级清单 | 优先级、主题、owner、输入、输出、进入条件 | 收口后排序并选定下一条执行主路径 | `candidate -> ranked -> selected -> planned` | 先看发布影响，再看闭环依赖，再看 owner 就绪度 | `producer_system_designer` 排序，`qa_engineer` 复核 |
| 玩家访问模式与跨模块验收 | 产品模式 REQ/AC 引用、PRD-ID、对应模块 owner、模式适用 evidence | 评审时引用[玩家访问模式产品要求](../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001)及其[验收](../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#ac-entry-mode-001)，再核对相关产品/专业 authority 的输入与 core 验收映射 | `unclassified -> authority-mapped -> evidenced -> reviewed` | 按产品 REQ/AC 确定 primary mode 与产品边界；provider、execution lane 和其他专业约束按各自 authority 核验；core 不维护第二份模式 taxonomy | `producer_system_designer` 负责跨模块验收与追踪，产品/专业 owner 维护各自规则并参与联审 |
- Acceptance Criteria:
  - AC-1: core PRD 包含模块职责矩阵。
  - AC-2: core PRD 包含至少 4 条关键端到端链路描述。
  - AC-3: core PRD 给出关键分册导航并可从 `doc/README.md` 到达。
  - AC-4: 实际 PR 或按需 Issue 中的相关修改与适用 PRD-CORE-ID 可映射，不强制另建任务。
  - AC-5: 文档级 `审计轮次` 保留对应历史 ROUND 的来源语义，不因日常修订上调，也不作为当前复审或验收通过证明；现行工作不要求新建 ROUND 启动文件或台账。
  - AC-6: core PRD 明确列出当前阶段 `P0/P1/P2` 收口项、对应 owner、输入、输出、验收标准与阻断条件。
  - AC-7: 全局 P0 按当前路径、连续性和必要验证缺口准入，首次开放按当前里程碑退出条件验收。
  - AC-8: 工作包 明确启动条件、依赖与退出条件，后续扩展不自动阻断受控试玩。
  - AC-9: `P2` 仅包含不阻塞发布的体验 polish 与治理补完，不得与 P0/P1 混淆。
  - AC-10: `PRD-CORE-004` 可映射到实际 PR 或按需 Issue 及适用的 `test_tier_required` 验证方法。
  - AC-11: `PRD-CORE-005` 必须明确下一轮第一优先级、对应 owner role、输入/输出与进入条件。
  - AC-13: core 活跃专题、Viewer 活跃手册与 Viewer 用户可见标题必须统一使用 `oasis7` 品牌；若为兼容保留旧内部实现名，必须明确标注为 internal compatibility naming。
  - AC-14: `engineering`、`scripts`、`world-runtime` 下仍可读的历史/治理/运行时专题标题必须改为 `oasis7` 品牌；仅实现标识、环境变量、脚本参数与历史证据正文可继续保留旧内部命名。
- Non-Goals:
  - 不在 core PRD 中替代模块详细技术分册。
  - 不在 core PRD 中维护逐版本实现变更流水（该信息在实际 PR 与按需 Issue 的时间线）。
  - 不在本 PRD 中重写各模块的实现细节或替代 Git、PR 与实际 CI 记录 中的执行计划。
  - 不把 launcher / explorer 体验新增功能作为当前阶段的主发布驱动。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 文档治理检查脚本、`rg` 检索、测试手册与 CI 脚本用于核验跨模块一致性。
- Evaluation Strategy: 以跨模块评审返工率、口径冲突数、发布前补文档次数评估全局 PRD 有效性。
  对 `PRD-CORE-004`，额外以 P0 闭环完成率、证据包完整率、go/no-go 评审一次通过率评估阶段收口质量。

## 4. Technical Specifications
- Architecture Overview: core 是全局设计总览层，描述全局结构、统一约束和跨模块链路，不承载业务实现代码。

<a id="当前阶段收口优先级stage-closure-backlog"></a>

### 当前阶段交付目标与全局 P0

`PRD-CORE-004` / `PRD-CORE-005` 当前目标：让受控真实玩家在同一个持久世界中，通过有界授权的 Agent 完成真实首产物，理解可恢复的生产阻塞，保留成果，并在再次进入后继续原有目标。

全局 P0 只接受：阻断所选首局—持续能力—回访路径的真实缺陷；可能破坏身份、已确认历史、合法资产、授权或幂等连续性的缺陷；使这些结果无法实际验证或被玩家正确理解的必要缺口。专题 Phase 0/P0 仅表达专题启动后的顺序，须说明启动条件。完整制度试点、复杂物流、战争、分片和完整节点自治不自动成为首批试玩前置；制度分类与兼容由[世界规则产品根](../product/world-rules-core-gameplay/prd.md)拥有。

分别判断世界可信且可恢复、工业闭环真实成立、玩家能理解并继续。模块任务说明服务哪个结果；宏观治理和测试建设只有直接阻断当前路径时才进入全局 P0。

| 里程碑 / 工作包 | 主责 | 依赖与产出 | 退出条件 |
| --- | --- | --- | --- |
| 顶层收敛与指标 | 产品 / 架构 / game / Viewer / QA | 对齐目标、工业链、authority 合同、指标与汇总 | 文档一致、语义回归通过、fixture/live/真人来源明确；不升级公开阶段 |
| 持久单权威 | P2P / runtime / 运营 | 顶层合同后补 manifest、证明、唯一追加权、独立故障域持久确认和恢复 | 单主机或单存储故障域失效保留已确认效果；旧 writer 被隔离；未知提交按原请求核对 |
| 代表性体验 | game / Agent / Viewer / runtime | 与持久底座并行；核验 Smelter 铁锭结算、授权纠偏、恢复与回访 | 同候选真实入口、output ledger 与持久 milestone 一致；首产物、持续能力、交付分别判定 |
| 受控真人验证 | 产品 / QA | 持久底座和体验满足首次开放条件后，两次会话观察 | 原始人数、独立/提示/代操作/未完成/未覆盖、后果选择与接续表现；形成继续、修复或收窄结论 |
| BFT 与交接 | P2P / runtime / 运营 / 消费者 | 顶层合同后可并行研发；先加入只读验证节点 | 独立验证者协议与 H/H+1 交接通过才转移写权；同身份、历史、资产连续 |

首次开放核验合法身份/profile、原子效果与幂等、独立故障域恢复、工业结算、Agency/回访和服务状态。具体合同由[基础设施](../product/world-infrastructure/prd.md)、[首局合同](../product/world-rules-core-gameplay/first-session-and-continuation.prd.md)和专业模块拥有。文档通过不能替代运行开放，BFT 未验收不阻止已验收的受控单权威阶段。

技术正确性与授权连续性是硬约束；操作量、动作族、内容量、时长与等待是诊断；样例因果、选择、恢复和继续理由只证明结构，真人动机与自发回访依赖真人记录。状态、阻断、候选 OID 与结果由 Git、PR、实际 CI 和评审记录承载，Issue/Project 按需使用，core 不建立并行台账。

当前多里程碑的需求与验收依据由[基础设施当前阶段](../product/world-infrastructure/prd.md#world-infrastructure-current-stage)、[首局合同](../product/world-rules-core-gameplay/first-session-and-continuation.prd.md)和[会话恢复组合验收](../product/player-entry-distribution/prd.md#player-session-unknown-outcome-acceptance)承接；正式激活、原请求恢复及写权接管分别进入[P2P 系统设计](../p2p/design.md#des-p2p-formal-controlled-authority-v1)与[Runtime 提交和恢复设计](../world-runtime/design.md#runtime-authority-commit-recovery-target)。专业 owner 与退出条件仍以上表及对应规范为准，不因停止某个 Issue 的跟踪而撤销。

[Issue #4363 的文档承接后关闭决定](https://github.com/eng-cc/oasis7/issues/4363#issuecomment-6126245617)仅作历史来源，已替代“继续由该 Issue 独立跟踪”的旧组织安排；不自动重开或创建替代 Issue。文档合入、Issue 关闭不表示实现完成、运行验收通过或可以邀测，也不解除已有部署限制；正式世界开放、独立故障域恢复和真人观察仍须分别取得适用授权与实际证据。

### 项目模块地图（Design Map）
产品信息架构由 `doc/product/README.md` 统一导航，固定为“世界规则与玩法系统 / 权威世界基础设施 / 智能体、世界模拟与交互 / 玩家接入与发行”四个入口。`core` 仍是项目级设计总览与跨模块治理基线，不是第五个产品模块；下表继续表达工程实现与治理模块地图。

| 模块 | 主职责 | 关键实现载体 |
| --- | --- | --- |
| core | 全局设计总览、跨模块治理基线 | `doc/core/*` |
| engineering | 工程规范、文件约束、质量门禁 | `doc/engineering/*`, `scripts/*`, CI workflows |
| game | 玩法循环、治理/经济/战争规则设计 | `doc/game/*`, `crates/oasis7` (gameplay相关) |
| world-runtime | 世界内核、事件溯源、WASM执行与治理 | `doc/world-runtime/*`, `crates/oasis7`, `crates/oasis7_wasm_*` |
| world-simulator | 场景系统、Viewer/Launcher、LLM交互链路 | `doc/world-simulator/*`, `crates/oasis7_viewer`, `crates/oasis7_client_launcher` |
| p2p | 网络、共识、DistFS、多节点运行 | `doc/p2p/*`, `crates/oasis7_net`, `crates/oasis7_consensus`, `crates/oasis7_distfs`, `crates/oasis7_node` |
| headless-runtime | 无界面运行链路、鉴权、长稳运维能力 | `doc/headless-runtime/*`, `crates/oasis7_chain_runtime/src/bin/*`, `crates/oasis7_viewer_runtime/src/bin/*`, `crates/oasis7_ops_tools/src/bin/*` |
| testing | 分层测试体系与发布门禁 | `doc/testing/*`, `testing-manual.md`, `scripts/ci-tests.sh` |
| scripts | 自动化脚本能力与执行约束 | `scripts/*`, `doc/scripts/*` |
| site | 站点信息架构、发布内容、SEO | `site/*`, `doc/site/*` |
| readme | 对外文档入口口径统一 | `README.md`, `doc/readme/*` |
| playability_test_result | 可玩性反馈证据与发布引用 | `doc/playability_test_result/*`, `doc/playability_test_result/game-test.prd.md` |

### 关键端到端链路（E2E Chains）
1. 玩家交互链路:
`Launcher/Viewer -> oasis7_viewer_live/oasis7_chain_runtime -> world-runtime -> event/journal -> UI反馈`
2. 世界执行链路:
`Action/Intent -> Rule Validation -> Resource/State Transition -> Event -> Snapshot/Replay`
3. 模块扩展链路:
`Rust Source -> WASM Artifact -> Register/Install -> Runtime Sandbox Execution -> Governance/Audit`
4. 分布式一致性链路:
`合法 authority profile -> 排序执行与持久提交 -> 同世界复制/恢复 -> Runtime 状态 -> Viewer Observe`
5. 发布验证链路:
`PRD-ID Task -> core治理(test_tier_required) -> 模块专项(required/full) -> Web闭环/长跑 -> Evidence Bundle -> Release Decision`

### 关键分册导航（只读总览后优先下钻）
- 运行时内核: `doc/world-runtime/runtime/runtime-integration.md`
- WASM 接口与执行: `doc/world-runtime/wasm/wasm-interface.md`, `doc/world-runtime/wasm/wasm-executor.prd.md`
- 场景矩阵: `doc/world-simulator/scenario/scenario-files.prd.md`
- Web 闭环测试策略: `doc/world-simulator/viewer/viewer-web-entry-compatibility.prd.md`
- 玩家访问模式产品契约: `doc/product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001` 与 `#ac-entry-mode-001`；模块导航见 `doc/product/player-entry-distribution/prd.md`
- 分布式路线与历史阶段: `doc/p2p/blockchain/p2p-blockchain-p2pfs-hardening.prd.md`
- 系统性测试手册: `testing-manual.md`

### 全局术语（Glossary）
- PRD-ID: 文档或模块内的既有条目标识，保留其需求、验收与证据定位职责；不替代跨层要求的全局 Trace ID。
- Trace ID: 同一稳定要求跨产品或专业声明、适用设计与 Task 共用的全局键；只在要求原文声明一次，其他位置引用，具体格式及增量范围以[要求追踪规范](../engineering/doc-governance/cross-layer-requirements-traceability.design.md)为准。关联不表示设计、实现或验收完成。
- 统一持久大世界 / unified persistent world: oasis7 默认玩家和产品世界模型；环境、network tier 与 `world_id` 是研发/运行时维度，不得被包装成多个玩家世界。
- required/full: 分层测试的两级核心门禁。
- Web-first 闭环: 默认 UI 验证路径（agent-browser 优先）。
- Effect/Receipt: 运行时外部副作用与回执审计机制。
- Snapshot/Replay: 世界状态持久化与可重放能力。

- Integration Points:
  - `AGENTS.md`
  - `doc/README.md`
  - `testing-manual.md`
  - 各模块 `doc/<module>/prd.md`、design/evidence 与 README / `prd.index.md`
  - PR、实际 CI 与评审记录（Issue 按需）
  - 按现行开发流程进行的适用评审记录（不要求额外通用放行材料）
- Edge Cases & Error Handling:
  - 模块入口失效：若目标路径迁移，core 必须同步更新导航并保留可追溯说明。
  - 信息缺失：若缺失内容影响本次发布判断，标记“口径待同步”并阻断该发布结论；无依赖的已授权分析、修复与测试继续。
  - 版本漂移：先按争议内容和声明 scope 确定 authority，再在同一 authority 内比较审定版本；较新日期不产生跨域覆盖权。范围不明或跨域冲突由对应责任人显式决定，并同步受影响条款，见 [权威裁决](../engineering/doc-governance/doc-structure-standard.design.md#doc-authority-resolution)。
  - 依赖冲突：同一链路被多个模块修改时，需合并影响面并重跑 required 级验证。
  - 测试证据缺口：无证据不得判定链路通过，必须补齐最小 required 证据。
  - 术语冲突：先按争议内容与 scope 定位专业或产品 authority，core 词典同步其定义；跨域矛盾在当前 PR/评审中显式裁定，不以 core 总览覆盖专业合同。
  - owner 冲突：多个模块同时声称同一项为 `P0` 且 owner 不一致时，按正式专题的职责范围与当前明确负责人协调；core 记录跨域决定，PR/评审及按需 Issue 记录执行事实，不依赖退役 `.pm` task owner 或额外角色回执。
  - Git、PR 与实际 CI 记录 缺承接：若 P0 项在 PRD 已定义但对应任务未承接，状态只能记为 `candidate`，不得进入发布结论。
  - 证据缺口：不能核对本次候选、适用范围或真实结果时，不得宣称 fully ready；沿用适用专业证据合同，不因未套用统一模板而另增普通 PR 门禁。
  - 资源抢占：若 launcher / explorer 新需求与 P0 资源冲突，默认降级到 P2，除非能直接服务玩法闭环或发布门禁。
- Non-Functional Requirements:
  - NFR-CORE-1: 核心入口文档链接可用率 100%。
  - NFR-CORE-2: 跨模块评审时，影响面识别耗时 <= 30 分钟。
  - NFR-CORE-3: 发布评审前，PRD-ID 到测试证据映射完整率 100%。
  - NFR-CORE-4: 所有核心术语变更需在 1 个工作日内同步到相关入口文档。
  - NFR-CORE-5: core 文档长度仅作编辑参考；按语义与维护成本决定是否拆分，不因超过 1000 行阻断或要求长度豁免。
  - NFR-CORE-6: P0 项的 owner / 输入 / 输出 / 验收标准 / 阻断条件覆盖率 100%。
  - NFR-CORE-7: 发布评审时 P0 证据缺失数必须为 0；P1 可存在未完成项，但必须附带风险与缓解方案。
  - NFR-CORE-8: 明确开展的跨模块阶段/发布工作，其适用证据交接须可完整追溯到真实要求、实际负责人、候选版本和 PR/Task 中的 CI/评审与原始证据；PRD 保留长期要求及有来源的状态摘要，不复制交接过程，不要求独立 pre-PR role review 文件链。`doc/devlog` 仅作为历史归档入口。
  - NFR-CORE-9: 一轮模块主项目全部收口后 1 个工作日内必须形成下一轮优先级清单。
- Security & Privacy: core 仅维护结构与治理口径；涉及密钥、签名、隐私数据的要求由对应模块 PRD 细化并执行。
  发布收口文档仅记录工程与玩法证据，不引入额外敏感数据；若引用线上/远程环境信息，需与对应模块 owner 联审后落档。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP (2026-03-03): core PRD 成为项目级总览入口。
  - v1.1: 建立跨模块变更影响检查清单模板。
  - v2.0: 建立 PRD-ID 到测试证据的自动化追踪报表。
  - v2.1 (2026-03-06): 启动 ROUND-005，专项收敛文档状态时效字段、完成态字段、命名一致性与索引覆盖规则。
  - v2.2 (historical, completed): 已建立阶段收口优先级、跨角色交付矩阵与发布前 P0 必备闭环。
  - v2.3 (historical, completed): 已完成下一轮优先级排序及其首个 release-readiness 专题承接；当前不再维护独立 dated priority slate。
  - v2.4 (historical, 2026-03): 版本级 readiness 与 go/no-go 裁决已落为 Git history 审计留痕；不再保留根目录 release-candidate 三件套作为当前首读入口。
- Technical Risks:
  - 风险-1: 模块新增能力未及时回填全局链路。
  - 风险-2: 总览与分册的口径同步依赖人工流程。
  - 风险-3: 若局部修订直接上调历史 `审计轮次`，会混淆旧 ROUND 的 reviewed-files / progress-log 对账语义与当前验证事实；历史记录不能代替当前 PR 的实际检查与评审。
  - 风险-4: 若继续把 launcher / explorer 体验扩展排在玩法与发布治理前，项目会强化“能展示”而非“能稳定发布”的错配。
  - 风险-5: 若 runtime / testing / playability 的证据标准不同步，发布评审会退化为口头判断。
- 风险-6: 若当前排序与下一步未同步到 Git、PR 与实际 CI 记录，团队会重新回到平均发力与隐式尾注推进。

## 6. Validation & Decision Record
- Test Plan & Traceability:

旧 TASK-CORE 与 ROUND-005/006 的建档、补齐、清零记录保留为历史来源，不构成当前任务准入、审计轮次完成或发布结论。没有唯一原 Task 时不迁入任意治理 Issue；可复用跨模块追踪与验证方法仍保留。
| PRD-ID | 历史来源或验证入口 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-CORE-001 | TASK-CORE-001/002/006/007 | `test_tier_required` | 入口完整性扫描、模块地图与导航可达检查 | 全项目入口与架构总览一致性 |
| PRD-CORE-002 | TASK-CORE-002/003/004/007 | `test_tier_required` | 关键链路映射核验、跨模块依赖抽样复核 | 跨模块设计兼容性与发布评审效率 |
| PRD-CORE-003 | TASK-CORE-004/005/007 | `test_tier_required` | 发布门禁证据映射校验、轮次一致性审查记录检查（含文档级审计轮次标记，缺省按0） | 发布决策可审计性与长期治理稳定性 |
| PRD-CORE-003 | TASK-CORE-008 | `test_tier_required` | `审计轮次 > 5` 漂移扫描、ROUND-005 基线回写、devlog 与 git 证据核对 | 审计标记口径恢复为正式台账语义 |
| PRD-CORE-003 | TASK-CORE-009 | `test_tier_required` | 全仓 `审计轮次 > 5` 扫描清零、缺失标记补齐为 5、devlog 与 git 证据核对 | 审计标记口径对齐到“全仓不高于 ROUND-005 基线” |
| PRD-CORE-004 | TASK-CORE-011/012/013/014 | `test_tier_required` | 阶段收口优先级、owner 分工、交付矩阵、go/no-go 模板与实际 PR / 按需 Issue 来源抽样核验 | 当前阶段发布前闭环目标与责任边界一致性 |
| PRD-CORE-005 | TASK-CORE-016/017/018/019/020/021 | `test_tier_required` | 下一轮优先级清单、候选级入口、版本级扩展与 runtime 联合证据抽样核验 | 新一轮跨模块执行一致性 |
| PRD-CORE-006 | TASK-CORE-022 | `test_tier_required` | 正式版本候选 go/no-go 记录、风险附注与角色交接抽样核验 | 版本候选正式裁决一致性 |
| PRD-CORE-007 | TASK-CORE-023 | `test_tier_required` | `doc/README.md` 含根 README / site 阅读入口 | 全局导航准确性 |
| PRD-CORE-008 | TASK-CORE-023 | `test_tier_required` | 更新时间与新阅读顺序存在 | 公开口径同步性 |
| PRD-CORE-009 | TASK-CORE-028/049/050/051/052/053/055 | `test_tier_required` | 核对[玩家访问模式产品 REQ/AC](../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001)与 core 主入口/索引链接、PRD-ID/任务映射及文档治理；provider、execution lane、alias 与 claim 的定义由对应产品/专业 authority 维护，core 仅验证其跨模块输入、验收与 required-tier 证据追踪 | 产品模式要求与 core 跨模块验收、formal gameplay 分工及 PRD-ID/测试证据映射一致性 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-CORE-001 | 将 core 固化为项目全局唯一总览入口 | 各模块独立维护无全局入口 | 降低跨模块认知成本并提升评审效率。 |
| DEC-CORE-002 | 保留既有 PRD-ID 的局部条目与历史追溯职责；新增或实质修改的跨层要求共用规范定义的全局 Trace ID | 使用任务编号作为唯一要求主键，或用局部 PRD-ID 代替全局 Trace ID | 保留原决定中要求身份独立于任务周期的理由；全局声明、引用及增量迁移边界以要求追踪规范为准，不为未触达存量重新发号。 |
| DEC-CORE-003 | core 文档治理任务默认绑定 `test_tier_required`；跨模块发布结论引用 testing 定义的 required/full 证据 | core 任务直接强制 required/full | 区分治理层与专项回归层，保持口径一致且可执行。 |
| DEC-CORE-004 | 在 ROUND-006 正式台账落档前，统一将脱离台账的高位 `审计轮次` 回写到 5 | 保留局部 `审计轮次: 6` 作为“专题修订痕迹” | `审计轮次` 的定义是“最近完成的正式审计轮次”，局部修订应通过 `最近更新` 和 devlog 追踪，而不是抬高正式轮次字段。 |
| DEC-CORE-005 | 将“阶段收口优先级”纳入 core 主 PRD 统一管理，而不是散落在多个模块 project 的状态说明里 | 仅在各模块 project 中维护各自优先级 | 阶段优先级本质是跨模块发布策略，需要由 `producer_system_designer` 在 core 层统一裁剪与仲裁。 |
| DEC-CORE-006 | 新一轮先冻结优先级清单，再启动第一优先级专题 | 主项目收口后直接随机挑模块继续推进 | 先统一排序，才能避免重新扩散资源。 |
| DEC-CORE-007 | readiness 达到 `ready` 后必须再落正式 go/no-go 记录 | 将 readiness board 直接作为最终放行记录 | readiness 与正式裁决是两个层级，必须分开留痕。 |
| DEC-CORE-008 | core 消费并链接玩家访问模式产品 REQ/AC，保留跨模块验收、PRD-ID 与测试证据追踪；provider、execution lane、alias 和公开 claim 细节由相应产品/专业 authority 维护 | 在 core 复制产品模式 taxonomy 或实现/执行维度规则 | 玩家入口产品承诺、专业执行约束与 core 跨模块验收属于不同 authority 层；明确引用关系可避免重复定义并保持可审计追踪。 |

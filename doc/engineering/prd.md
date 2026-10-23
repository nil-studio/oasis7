# engineering PRD

审计轮次: 8

## 目标

Trace refs: O7-REQ-276983b3-3187-405e-8453-e2785f01c5e6；本页的现行流程与跨层引用承接[全局要求追踪](doc-governance/cross-layer-requirements-traceability.design.md#global-requirement-trace)，Task 可以直接是 PR。
- 建立 engineering 模块设计主文档，统一需求边界、技术方案与验收标准。
- 确保 engineering 模块后续改动可追溯到 PRD-ID、任务和测试。

## 范围
- 覆盖 engineering 模块当前能力设计、接口边界、测试口径与演进路线。
- 覆盖产品或专业要求、适用设计与 PR 任务的关联，保留局部 PRD-ID。
- 不覆盖实现代码逐行说明与历史过程记录。

## 接口 / 数据
- PRD 主入口: `doc/engineering/prd.md`
- 项目执行记录: PR、实际 CI 与评审记录；Issue/Project 按需使用
- 文件级索引: `doc/engineering/prd.index.md`
- 专业条款标识: `PRD-ENGINEERING-xxx`；跨层要求的稳定键按全局 Trace ID 规范维护
- 测试与发布参考: `testing-manual.md`

## 里程碑
- M1 (2026-03-03): 完成模块设计 PRD 主体重写与任务改造。
- M2: 补齐模块设计验收清单与关键指标。
- M3: 建立 PRD-ID -> Task -> Test 的长期追踪闭环。

## 风险
- 模块边界演进快，文档同步可能滞后。
- 指标口径不稳定会降低验收一致性。
## 1. Executive Summary
- Problem Statement: 工程规范分散在多个专题文档，导致文件体量控制、提交门禁、脚本治理与代码质量标准不够统一。
- Proposed Solution: 将 engineering 模块定义为工程治理主文档，统一维护规范、质量门禁、改造节奏与验收口径。
- Success Criteria:
  - SC-1: Rust 单文件超 1200 行新增违规数为 0。
  - SC-2: Markdown 长度只作编辑提示，不因超过 1000 行阻断验证或要求豁免；缺失引用、结构错误等真实检查继续生效，Rust 体量规则不变。
  - SC-3: `scripts/doc-governance-check.sh` 在 required gate 连续通过。
  - SC-4: 新增或实质修改的稳定工程要求按全局 Trace ID 关联原文、适用设计和任务；保留专业 PRD-ID，不为简单编辑新建要求或任务身份。
  - SC-5: `doc/` 根目录与模块根目录平铺文档新增违规数为 0（allowlist 冻结机制）。
  - SC-6: 重点模块的当前主责入口和活跃引用保持可达、语义一致；已停止的历史平铺专题迁移不作为当前开发或交付前置。
  - SC-8: 治理按当前明确缺口有界交付，不恢复固定人数、冻结迁移批次或每日燃尽台账；历史执行安排仅供追溯。
  - SC-9: 活跃文档 `doc/...*.md` 依赖路径断链数为 0。
  - SC-12: 文档-代码偏差在同批次回写闭环率 100%。
  - SC-14: 专业职责按需参考 `.agents/roles/*.md`；根 `AGENTS.md` 保留必要规则与导航，不规定固定角色数量。
  - SC-15: 角色协作按需参考 `.agents/roles/templates/`，交接说明问题、上下文、允许范围和期望结果，不强制模板或回执。
  - SC-16: `AGENTS.md` 与开发流程规范一致，负责人按实际问题选用专业角色和 subagent，不要求固定角色组合。
  - SC-20: `engineering` 模块治理专题标题对外统一使用 `oasis7` 品牌，不再在活跃/历史治理入口中混用 `oasis7` 标题。
  - SC-24D: 同一 PR 内的 review comment follow-up 必须存在 repo-owned helper，用于统一盘点 unresolved review threads、执行显式 resolve，并在每轮操作后分别报告 `reviewDecision`、`mergeStateStatus` 与 unresolved thread 数，避免把“thread 已关”误报成“PR 可合并”。
  - SC-24G（历史）：旧 `.pm/**` 冲突分类与 canonical task/memory/stage 模型仅保留历史含义。当前冲突按 Git 真实改动处置，保留用户资料，不恢复退休文件或任务身份；main 前进本身不要求 rebase 或重测。
  - SC-29: 文档体量治理必须具备正式专题口径，明确 `活跃真值 / 审计留痕 / 历史归档 / 兼容跳转` 四层消费模型；高密度模块的默认阅读面只保留“what / where / next / risk”所需入口，不再把审计与归档材料直接暴露为主入口。
  - SC-29A: 当文档治理债已从“默认阅读面混乱”转向“文档存量维护成本”时，engineering 必须存在一份正式 follow-up 专题，提供可复算的库存报告入口，并明确 `历史压缩 / 路径级治理 / 近限文件拆分 / 季度复核` 四类后续动作。
  - SC-29B: 当某个热点子域进入后仍缺少 canonical 首读入口时，engineering 必须允许继续拆出路径级治理 follow-up；首条子域治理需至少为该热点路径补齐一个子目录 landing page，并把上游模块入口改成先命中该 landing page，而不是继续让 operator 手册或完整索引单独承担全部分流职责。

## 2. User Experience & Functionality
- User Personas:
  - 工程维护者：需要稳定规则来控制技术债。
  - 贡献开发者：需要清晰区分普通提交与 authoritative verification gates。
  - 评审者：需要可量化判断变更是否合规。
- User Scenarios & Frequency:
  - 日常提交：先做最小有效局部验证，再由 CI 执行必要检查；不要求额外 PR-ready 身份或完成回执。
  - CI 失败排查：每个异常流水线触发后执行，定位脚本与规则来源。
  - 规范迭代评审：每周至少 1 次，评估误报率和治理收益。
  - 季度治理复盘：每季度 1 次，回看违规趋势与修复效率。
- User Stories:
  - PRD-ENGINEERING-001: As an 工程维护者, I want enforceable file-size and structure limits, so that maintenance cost stays bounded.
  - PRD-ENGINEERING-003: As a 评审者, I want auditable governance evidence, so that review decisions are defensible.
  - PRD-ENGINEERING-005 (historical): legacy migration collaboration principles were completed and retired; current doc topology rules live in `doc/engineering/doc-governance/doc-structure-standard.design.md`.
  - PRD-ENGINEERING-007 (historical): legacy migration closure review was completed and retired; current workflow gates live in `doc/engineering/workflow/source-of-truth.md`.
  - PRD-ENGINEERING-008: As a 文档维护者, I want per-module file-level PRD indexes, so that active docs are reachable from the root doc tree.
  - PRD-ENGINEERING-009: As a 治理维护者, I want accurate requirement/design links and PR verification records, so that traceability does not require a separate project ledger.
  - PRD-ENGINEERING-010: As a 评审者, I want the selected tests, actual version, results and uncovered scope recorded in the PR, so that verification can be reviewed without extra task fields.
  - PRD-ENGINEERING-011: As a 文档维护者, I want doc path references validated in gate, so that migration-induced broken links are blocked before merge.
  - PRD-ENGINEERING-012（历史）: 2026-03 全量 PRD 审读清单保留为历史覆盖证据，不作为当前修改的必需台账。
  - PRD-ENGINEERING-013: As a 模块负责人, I want implementation facts checked against the applicable product/design authority, so that documentation distinguishes actual behavior, adopted decisions, and implementation gaps.
  - PRD-ENGINEERING-014: As a 评审者, I want duplicate and upstream/downstream alignment checks, so that the PRD tree stays clear and non-conflicting.
  - PRD-ENGINEERING-015: As a 文档作者/评审者, I want one canonical document topology and role split, so that I can place new docs without guessing and keep detailed design discoverable.
  - PRD-ENGINEERING-022: As a `producer_system_designer`, I want external memory and reflection patterns benchmarked against our file-native governance model, so that future self-evolution upgrades borrow structure without replacing repo-native truth.
  - PRD-ENGINEERING-024: As a 项目经理/模块 owner, I want doc surface area governance formalized, so that I can distinguish active reading surfaces from audit/archive material and keep the default reading path usable even when `doc/` keeps growing.
  - PRD-ENGINEERING-025: As a 项目经理/模块 owner, I want doc corpus maintenance cost formalized, so that I can see when doc governance has shifted from reading-surface clutter to path-level maintenance debt and open the right follow-up task.
  - PRD-ENGINEERING-026: As a 项目经理/文档治理评审者, I want a canonical `doc/devlog` archive entrypoint, so that I can navigate historical daily logs by month and hotspot instead of scanning day files blindly.
  - PRD-ENGINEERING-027: As a 项目经理/Viewer owner, I want a canonical `doc/world-simulator/viewer/` path entrypoint, so that I can navigate the largest hotspot path by intent instead of scanning nearly 300 files blindly.
  - PRD-ENGINEERING-028: As a 项目经理/P2P owner, I want a canonical `doc/p2p/node/` path entrypoint, so that I can navigate the densest `p2p` hotspot path by intent instead of scanning nearly 70 files blindly.
  - PRD-ENGINEERING-029: As a 项目经理/Testing owner, I want a canonical `doc/testing/evidence/` path entrypoint, so that I can navigate the densest testing hotspot path by intent instead of scanning nearly 50 evidence files blindly.
  - PRD-ENGINEERING-030: As a 项目经理/README owner, I want a canonical `doc/readme/governance/` path entrypoint, so that I can navigate the densest readme hotspot path by intent instead of scanning nearly 100 governance docs blindly.
  - PRD-ENGINEERING-033: As a 项目经理/文档治理评审者, I want follow-up canonical entrypoints for newly re-identified hotspot paths, so that `doc/world-simulator/launcher/` and `doc/game/gameplay/` can be navigated by intent without expanding a full-corpus rewrite round.
  - PRD-ENGINEERING-031: As a `producer_system_designer`, I want external agent workflow patterns benchmarked against oasis7's repo-native execution chain, so that we only adopt the parts that strengthen evidence, workflow verification, or UI ideation while preserving actual Git/PR/CI facts and applicable professional review, without restoring retired `.pm` task authority.
  - PRD-ENGINEERING-032: As a repo workflow owner, I want the current local skill inventory frozen into keep/replace/retire/defer buckets, so that role cards and engineering docs no longer recommend low-value or workflow-conflicting skill surfaces.
- Critical User Flows:
  1. Flow-ENG-001: 明确问题与验收 → 独立分支实现 → 局部验证 → 同一目的 PR → 实际 CI 与按风险评审 → 修复阻断问题 → 绑定预期 HEAD 正常合入。
  2. Flow-ENG-002: CI 失败 → 复现和定位根因 → 最小修复 → 重测受影响内容。
  3. Flow-ENG-003: 文档变化 → 选择对应领域与职责 → 同步消费者与索引 → 适用文档检查。
  4. Flow-ENG-008: 需要协作 → 给出问题、上下文、允许范围和返回结果 → 按需专业分析或并行实现 → 负责人整合实际结果。
  5. Flow-ENG-009: 获得高价值反馈 → 核对事实、影响及用户范围 → 修复当前问题或记录可读建议；无需自动转任务、memory 或 stage。
  6. Flow-ENG-011: 复杂工作按需使用 Issue checklist 或 Project → 展示依赖及剩余义务 → 验收实际完成后更新；看板故障不停止开发。
  7. Flow-ENG-014: 独立分支或按需 worktree → 保留共享 Cargo 缓存和运行隔离 → 连续开发验证 → 正常 PR 与合入 → 独立安全清理。
  8. Flow-ENG-016: 只读查询或分析 → 直接提供事实或按需专业判断；不创建流程身份、工作区或远程记录。
  9. Flow-ENG-017: subagent 按需使用 → 给出清楚写入范围和上下文 → 返回结论、验证及风险；配置和模型观测不构成 PR 准入。
- Functional Specification Matrix:
| 功能点 | 字段定义 | 按钮/动作行为 | 状态转换 | 排序/计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 文档治理检查 | allowlist、模块根目录规则、根目录规则 | 执行 `doc-governance-check.sh` | `pass/fail` | 按违规严重度输出 | 所有人可执行，治理维护者可更新基线 |
| 工程趋势统计 | 违规率、修复时长、回归率 | 周期性生成报表并复盘 | `collecting -> reported -> actioned` | 按模块与时间排序 | 评审者与维护者可读写 |
| 有界并行治理 | 明确负责人、当前缺口、实际写入范围和验证结果 | 围绕当前有效缺口协作，不恢复已退役迁移排程 | 按 PR 和实际验证记录进展 | 写入范围避免重叠，必要上下文按需交接 | 负责人协调，适用评审复核 |
| PRD 文件级索引 | 模块名、专题 PRD/design 路径、相关 PR 与验证记录 | 生成/更新模块索引并回写入口引用 | `missing -> indexed -> verified` | 活跃文档优先，按路径稳定排序 | 维护者可更新，所有贡献者可读 |
| 依赖路径可达门禁 | 引用文档路径、引用来源、豁免列表 | 校验 `doc/...*.md` 引用目标是否存在 | `pass/fail` | 默认全量校验，通配符/模板与白名单文件豁免 | 维护者维护豁免，提交者必须修复断链 |
| 文档分工与组织规范 | 对象层级（模块/专题/分册）、职责后缀（PRD/Design/Runbook/Manual） | 为新主题选择落点并按规则建档 | `unclassified -> classified -> indexed -> reviewed` | 目录按领域/专题，文件按职责，PRD/design 按长期主题配对，任务直接使用 PR 或按需 Issue | 作者可建档，评审者可裁定例外 |
| 文档体量治理 | 文档总量、模块/子目录密度、消费层类型（活跃真值/审计留痕/历史归档/兼容跳转）、默认入口面 | 先冻结默认阅读面，再决定哪些文档只保留可检索性与定向引用，不再进入根入口/模块入口的主列表 | `unbounded -> classified -> reduced -> monitored` | 默认优先压缩阅读面而不是立即迁移文件；高密度模块优先按 `world-simulator -> p2p -> testing -> readme/core` 排查 | `producer_system_designer` 裁定消费层，模块 owner 回写入口与索引，评审者复核 |
| 历史 PRD 审读留痕 | 文档路径、阅读时刻、代码一致性、重复性、上下游状态、处理动作 | 历史 round logs / deleted snapshot prose 仅用于追溯；当前偏差回写到模块 truth | `historical -> traced -> current_truth_updated` | 入口优先、风险优先 | 维护者与评审者可追溯，当前 truth 由模块 owner 维护 |
| 角色职责卡 | 角色名、使命、owner 范围、输入、输出、决策边界、完成定义、推荐技能、检查清单 | 更新 `.agents/roles/*.md` 并在根 `AGENTS.md` 维护入口映射 | `draft -> aligned -> adopted` | 12 张专业关注点供按需参考 | 全体贡献者可读，角色 owner 与治理维护者可改 |
| 角色交接模板 | 交接标题、来源角色、目标角色、目标、上下文、输入、输出、截止、风险、阻断、验证、回写位置 | 从 `.agents/roles/templates/*.md` 复制填写并随任务流转 | `draft -> sent -> acknowledged -> delivered` | 模板按实际需要使用，不要求传递协议 | 发起方负责填写，接收方负责确认，维护者可演进模板 |
- Acceptance Criteria:
  - AC-1: engineering PRD 明确文件约束、脚本约束、测试分层约束。
  - AC-2: 任务范围、实际状态和剩余义务由当前 PR 描述、实际 CI 与评审承载；Issue/Project 按需使用，不创建 engineering project 台账或本地状态副本。
  - AC-3: 与 `doc/scripts/precommit/pre-commit.prd.md`、`testing-manual.md` 的口径一致。
  - AC-5: 新增工程治理专题若引入运行态治理层，必须明确与正式文档层的分工边界，并在当前 PR 或按需 Issue 记录本次范围；复杂协调按需使用 Issue/Project，不作为普通 PR 准入。
  - AC-6: 文档治理脚本校验 `doc/.governance/*-allowlist.txt`，可拦截 `doc/*.md` 与 `doc/<module>/*.md` 的非预期新增。
  - AC-8: 每次迁移任务需附“原文关键约束点 -> 新文档章节”对照，确保内容不丢失。
  - AC-9: 并行治理按实际修改范围避免重叠并保留必要交接；旧迁移分工表、冻结批次和每日燃尽不作为当前交付条件。
  - AC-11: 专题需求、设计、当前导航与真实要求引用应可达且互相追溯；不要求独立 project 文档，不恢复 PRD/project 双向互链门禁。适用检查以现行文档治理入口为准，现有检查只证明其实际能力。
  - AC-13: 文档治理门禁必须校验活跃文档 `doc/...*.md` 引用路径可达；断链必须阻断并修复。
  - AC-14: 2026-03 全量 PRD 审读留痕必须可通过 historical round logs / deleted snapshot prose 追溯；当前不再要求存在旧 checklist 目录活跃清单面。
  - AC-21: `doc/engineering/**` 仍可读治理专题标题统一使用 `oasis7` 品牌；旧 `oasis7` 仅允许出现在历史正文引用或实现兼容说明中。
  - AC-26: 仓库若显式提交 `.codex/config.toml` 作为 repo-local Codex 默认执行配置，则该文件必须可被 Git 追踪，且默认 `sandbox_mode` 需以仓库工作流要求的显式值落盘，不得依赖用户本机全局配置隐式兜底。
  - AC-27: engineering 模块需存在一份正式“文档体量治理”专题，冻结 `活跃真值 / 审计留痕 / 历史归档 / 兼容跳转` 的判定标准、默认入口面收敛规则、密度触发条件和首批高风险模块优先级。
  - AC-27A: engineering 必须存在 `doc-corpus-maintenance-governance` 正式专题与 `scripts/doc-inventory-report.sh` 报告入口，能够复算 `doc/` 总量、模块密度、热点子目录、`doc/devlog` backlog 与非归档 near-limit 文件，并把文档债从“入口混乱”与“存量维护成本”两阶段明确区分。
  - AC-27B: `doc/devlog` 必须存在一个 canonical archive entrypoint，用于按月份和高体量热点导航历史日文件；该入口只能承担历史归档职责，不得重新成为运行态真值。
  - AC-27C: 对已进入路径级治理的热点子域，必须存在一个子目录级 canonical landing page；至少 `doc/world-simulator/viewer/` 需要通过 `viewer/README.md` 把 `manual`、`software_safe`、runtime live 与定向检索区分开来，避免继续由 `viewer-manual.manual.md` 或 `prd.index.md` 单独承担全部首读分流。
  - AC-27D: `doc/p2p/node/` 必须存在一个 canonical 子目录入口 `node/README.md`，能把节点奖励/资产、复制链路、PoS 时间、身份引导与 WASM 编译护栏分开导航，避免继续让 `p2p` 模块 README 或 `prd.index.md` 单独承担整个热点子域的首读分流。
  - AC-27E: `doc/testing/evidence/` 必须存在一个 canonical 子目录入口 `evidence/README.md`，能把 release gate、hosted access / browser、p2p / legacy network-rehearsal evidence、governance drill、claim/audit matrix 与定向验证 evidence 分开导航，避免继续让 `testing` 模块 README 或 `prd.index.md` 单独承担整个热点子域的首读分流。
  - AC-27F: `doc/readme/governance/` 必须存在一个 canonical 子目录入口 `governance/README.md`，能把治理控制、release communication、Moltbook、limited preview/reward、小红书与公开定位入口分开导航，避免继续让 `readme` 模块 README 或 `prd.index.md` 单独承担整个热点子域的首读分流。
  - AC-27G: 后续 inventory 重新暴露的热点子域应优先补子目录级 landing page，而不是新增 full-corpus rewrite round；首批 aftercare 覆盖 `doc/world-simulator/launcher/README.md` 与 `doc/game/gameplay/README.md`，并同步模块 README / `prd.index.md` 的首读分流。
- 当前开发流程、协作与验收以 [开发流程规范](workflow/source-of-truth.md) 为准；角色卡是专业参考，历史 PM 要求已退出。
- Non-Goals:
  - 不定义 gameplay/p2p/runtime 业务规则。
  - 不替代模块内部测试策略。

<a id="PRD-ENGINEERING-034"></a>

### PRD-ENGINEERING-034：项目开发指挥客户端

为项目负责人提供云端指挥中心与本地辅助端，围绕“看清重点与变化—进入一项工作—处理真实问题—回看结果”接续已有开发现场。ChatGPT Sites 网页承载跨设备的四个主视图、统一工作详情、已提交规范与云端个人空间；macOS 本地辅助端承载工作树、未提交资料及原生工具。首页直接呈现目标、成果、下一步和待处理，同一工作关联适用规范及理由、来源版本、环境、PR 和原工具入口。

- 产品范围、四视图、主要失败恢复和验收条件由 [指挥客户端产品设计](command-center/command-center.prd.md) 主责；技术选型、组件、数据和连接合同由 [系统设计](command-center/command-center.design.md) 承接。
- 该客户端属于 engineering 开发工具，沿用现有文档、Git 与交付来源的事实边界；不扩展游戏产品模块，不新增通用任务登记或开发审批。
- 云端读取授权范围内的已提交文档、GitHub PR 和有界 CI 事实，持久保存用户的个人问题、决定、显式关联与阅读偏好；本地保存设备绑定、私有资料和工具操作状态。来源事实、云端个人记录与本机数据分别拥有唯一权威，导航不授予远程执行或资料上传权限。具体能力与验收以配对主责文档和真实交付证据为准；文档采纳、代码交付与发布分别判断。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 文档治理脚本、CI 测试脚本、静态检查脚本。
- Evaluation Strategy: 通过 required/full gate 成功率、违规项统计、回归修复时长衡量工程治理有效性。

## 4. Technical Specifications
- Architecture Overview: engineering 模块聚焦工程流程与规范，不承载业务逻辑；通过脚本与门禁把规范落地到提交链路。
- Integration Points:
  - `scripts/doc-governance-check.sh`
  - `doc/scripts/precommit/pre-commit.prd.md`
  - `doc/.governance/doc-root-md-allowlist.txt`
  - `doc/.governance/module-root-md-allowlist.txt`
  - `doc/engineering/doc-governance/doc-structure-standard.design.md`
  - `doc/engineering/workflow/source-of-truth.md`
  - historical engineering full-PRD review triplet（后续已删除；历史审读证据见 Git history logs，当前追踪入口见 `doc/engineering/prd.index.md` 与模块入口）
  - historical world-simulator PRD review checklist snapshot（后续已删除；当前 world-simulator truth 见 `doc/world-simulator/README.md`、`doc/world-simulator/prd.index.md`、`doc/world-simulator/prd.md` 与 `doc/world-simulator/prd.md`）
  - historical self-evolution file-based PM background: `doc/engineering/self-evolution/file-based-self-evolution-management-2026-03-30.design.md`; current task truth / evidence rules live in `doc/engineering/workflow/source-of-truth.md`
  - current self-evolution, memory, reflection and external-workflow boundaries: `doc/engineering/workflow/source-of-truth.md`; pending scope is retained in `doc/engineering/prd.md`, and default-vs-library skill reachability is in `.agents/skills/README.md`
  - current document organization and consumption-layer rules: `doc/engineering/doc-governance/doc-structure-standard.design.md`; inventory and maintenance-cost follow-up routing: `doc/engineering/governance/README.md`
  - `doc/testing/evidence/README.md`（当前 testing evidence 子域分流与维护边界；一次性路径落位专题已退役）
  - `doc/devlog/README.md`
  - `doc/world-simulator/viewer/README.md`
  - `scripts/doc-inventory-report.sh`
  - `scripts/doc-governance-check.sh`
  - `doc/*/README.md`
  - `testing-manual.md`
  - `.codex/config.toml`
  - `.github/workflows/*`
- Edge Cases & Error Handling:
  - allowlist 漂移：检测到未登记新增时直接失败并提示最小修复路径。
  - repo-local Codex 配置被忽略：若根 `.gitignore` 使用通配 `config.toml` 规则，必须为 `.codex/config.toml` 增加最小 unignore，避免仓库默认执行配置无法进入版本管理。
  - 误报场景：规则误伤时保留失败证据并通过任务流程修订规则，不直接绕过。
  - 本地/CI 不一致：本地通过但 CI 失败时以 CI 结果为准并补环境对齐说明。
  - 脚本不可执行：缺依赖时给出明确安装建议与最小复现命令。
  - 并发修改冲突：同一规则发生实际冲突或相关集成风险时，更新受影响内容并验证真实组合；main 前进本身不要求重测。
  - 新旧格式并存：迁移中允许 legacy 与 strict 共存，但每个迁移批次必须标注边界并回写追踪状态。
  - 批量迁移回归风险：结构改写可能造成引用断链，需附带路径扫描与脚本复核。
  - 索引覆盖不足：专题文档未被入口索引时，必须在当批修复并补回链路。
  - 引用缺失：修复实际要求、设计与验证记录的链接；尚未形成的层级如实说明，不为补链新建 project 台账。
  - 历史迁移快照：包含旧路径清单的迁移快照文档需通过白名单豁免，避免误判为断链。
  - 审读范围漂移：在已有 PR 或评审记录中注明实际审阅范围和版本，不把未读内容写为已审；历史清单不重签。
  - 事实与规范冲突：正式文档拥有相应要求，实际交付状态以 Git、PR、CI 和评审记录核实；历史 `.pm/` 资料不再定义任务或阶段真值。原 code-first 仅用于核实实现事实及已采纳设计决定，不使现有代码覆盖产品/设计 authority。偏差按[设计检查与修复](workflow/source-of-truth.md#root-cause-design-check)区分设计缺陷、实现缺陷和文档失实，不为迁就错误实现改写正确要求。
- Non-Functional Requirements:
  - NFR-ENG-1: required 门禁平均执行时长 <= 10 分钟。
  - NFR-ENG-2: 文档治理误报率 <= 5%（按周统计）。
  - NFR-ENG-3: 新增或实质修改的相关要求按全局 Trace ID 维护本次跨层关系，不要求未触达存量回填。
  - NFR-ENG-4: 工程治理脚本在 Windows、Linux/macOS 环境均可执行。
  - NFR-ENG-5: 规则变更需附带可追溯说明与回归证据。
  - NFR-ENG-6: 活跃文档语义迁移在当前 PR 或已有记录说明原条款去向、未接收语义与实际验证结果；复用现有内容，不另建三件套证明链。
  - NFR-ENG-7: 治理收益按当前规则冲突、失效入口和实际消费者缺口的消除判定；不规定固定人数或每日迁移篇数。
  - NFR-ENG-8: 全部模块文件级索引应在 1 次 `doc-governance-check` 执行内完成可达性校验。
  - NFR-ENG-9: 活跃专题的真实要求、设计与 authority 链接保持可达，不要求 project ledger。
  - NFR-ENG-10: PR 验证记录能说明所选测试、实际版本、结果与未覆盖范围，不新增模块主项目字段。
  - NFR-ENG-11: 活跃文档 `doc/...*.md` 引用路径可达性覆盖率 100%。
  - NFR-ENG-13: 根 `AGENTS.md` 与 `.agents/roles/*.md` 的角色映射一致率 100%，不得出现无入口角色或悬空引用。
  - NFR-ENG-14: 角色交接模板字段命名稳定，默认模板在 5 分钟内可完成填写并可被他人直接执行。
  - NFR-ENG-15: 开发工作流规则在单人执行与多角色协作两种场景下都应自洽，不得出现相互冲突的提交/回写要求。
  - NFR-ENG-16: 单日日志应同时支持时间线回放与角色维度检索，不得因角色拆分导致当日过程碎片化。
  - NFR-ENG-17: 角色名校验应零配置跟随 `.agents/roles/` 目录变化，不依赖重复维护的手写名单。
  - NFR-ENG-18: 协作执行语义应与当前 Codex/CLI 运行模式兼容，允许单一执行主体通过角色视角切换完成多角色闭环；GitHub PR review 的仓库默认流程、角色协作语义与 no-commit 收口流程之间不得互相冲突。

- Security & Privacy: 仅涉及工程流程元信息；涉及凭据的自动化流程必须遵守最小暴露原则并避免日志泄漏。
## 5. Risks & Roadmap
- Phased Rollout:
  - MVP (2026-03-03): 固化工程规范与门禁指标。
  - Phase-1 进展（2026-03-03）: Owner-B 已完成 `doc/p2p/**` 115 篇待迁移文档的逐篇重写迁移。
  - v1.1: 补齐高频违规的自动修复建议与脚本化诊断。
  - v2.0: 建立工程规范趋势看板（违规率、修复时长、回归率）。
- Technical Risks:
  - 风险-1: 规范过严导致迭代效率下降。
  - 风险-2: 新脚本引入误报造成 CI 噪声。
  - 风险-3: 老文档迁移批次过大导致评审负担与引用回归风险提升。
  - 风险-4: 多人并行对同一目录写入造成冲突与重复迁移。

## 6. Validation & Decision Record

下表保留历史任务与当时验证、决策的追溯关系；旧 TASK 编号、project 三件套、pm-lint、固定角色与旧门禁决定不构成当前执行要求。现行开发规则以 workflow source of truth 为准，未执行的验证不得因保留旧表而宣称通过。
- Test Plan & Traceability:

旧 TASK-ENGINEERING 编号、Owner-B 115 篇与旧迁移/aftercare 记录只作原版本来源，不是当前队列或完成证明；未定位原 Task 的过程保留在固定 Git 历史与本表，不批量代配到总体治理 Task。当前验证仍按有效方法列及 workflow source of truth 选择。
| PRD-ID | 历史来源或验证入口 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-ENGINEERING-001 | TASK-ENGINEERING-001/005/006/007 | `test_tier_required` | 文档结构检查、平铺治理脚本执行 | 文档组织一致性、工程可维护性 |
| PRD-ENGINEERING-002 | TASK-ENGINEERING-002/003/007 | `test_tier_required` + `test_tier_full` | `commit` baseline 与 required/full CI 门禁联动校验 | 提交流程稳定性、回归拦截能力 |
| PRD-ENGINEERING-003 | TASK-ENGINEERING-003/004/007 | `test_tier_required` | 趋势统计与审查模板抽样检查 | 工程治理可审计性与长期演进 |
| PRD-ENGINEERING-004 | TASK-ENGINEERING-008/009 | `test_tier_required` | 原文约束点对照、迁移后治理脚本与引用扫描 | 文档格式一致性与内容保真 |
| PRD-ENGINEERING-005 | TASK-ENGINEERING-010 | historical-only | 迁移协作入口已退役；保留完成态 trace | legacy migration closure |
| PRD-ENGINEERING-006 | TASK-ENGINEERING-011/012/013/013A/013B/013C/013D/014 | historical-only | 迁移批次已退役；保留完成态 trace | legacy migration closure |
| PRD-ENGINEERING-007 | TASK-ENGINEERING-015 | historical-only | 全量迁移收尾复核已退役；保留完成态 trace | legacy migration closure |
| PRD-ENGINEERING-008 | TASK-ENGINEERING-016 | `test_tier_required` | 12 模块文件级索引覆盖扫描、入口可达性检查 | 文档树可达性与导航一致性 |
| PRD-ENGINEERING-009 | TASK-ENGINEERING-017 | `test_tier_required` | 现行 `doc-governance-check` 的入口/引用核验，不恢复 project 投影 | PRD/project 追溯完整性 |
| PRD-ENGINEERING-010 | TASK-ENGINEERING-018 | `test_tier_required` | 适用 PR 的实际测试范围、tier 和 CI 结果核对；Issue 按需 | 任务到测试分层可审计性 |
| PRD-ENGINEERING-011 | TASK-ENGINEERING-019 | `test_tier_required` | 活跃文档引用路径可达性门禁与断链修复验证 | 文档树引用完整性与迁移稳定性 |
| PRD-ENGINEERING-012 | TASK-ENGINEERING-020/024 | `test_tier_required` | 历史审读留痕与入口文档已读率检查 | PRD 审读可追溯性 |
| PRD-ENGINEERING-013 | TASK-ENGINEERING-021/022 | `test_tier_required` | 代码一致性抽样与偏差回写核验 | 文档行为与实现一致性 |
| PRD-ENGINEERING-014 | TASK-ENGINEERING-022/023/024 | `test_tier_required` + `test_tier_full` | 重复治理记录与上下游链路可达性检查 | PRD 体系清晰度与跨模块对齐 |
| PRD-ENGINEERING-015 | TASK-ENGINEERING-025 | `test_tier_required` | 规范正文结构检查、模块入口回写、索引可达性检查 | 新增文档可发现性与详细设计落位一致性 |
| PRD-ENGINEERING-016 | TASK-ENGINEERING-030/036 | `test_tier_required` | 角色职责卡存在性、字段完整性、推荐技能区段与根 `AGENTS.md` 入口映射检查 | 人机协作分工清晰度与执行一致性 |
| PRD-ENGINEERING-024 | TASK-ENGINEERING-106/107/108/109/110/111/112/114 | `test_tier_required` | 文档体量治理专题三件套、engineering 根入口/主项目/索引回写、`world-simulator` / `p2p` / `testing` / `readme` / `core` / `world-runtime` / `game` / `site` 模块 `README.md` / `prd.index.md` 的默认阅读面收紧、module-root allowlist 更新与 `doc-governance-check` 通过；人工核对默认阅读面不再把 `doc/devlog` / round reviews / evidence 直接提升为主入口，也不再把高密度模块近期专题长名单平铺在模块 README 首屏 | 仓库文档消费层、项目经理视角导航效率与后续减重批次规划 |
| PRD-ENGINEERING-025 | `doc-corpus-maintenance-governance` | `test_tier_required` | 存量维护成本专题三件套、`scripts/doc-inventory-report.sh` 输出当前仓库体量快照、engineering 根入口/主项目/索引与 `doc-surface-area-governance` handoff 回写、`doc-governance-check` 通过 | 文档治理阶段判断、路径级治理优先级、`doc/devlog` backlog 处理与季度治理输入 |
| PRD-ENGINEERING-026 | `devlog-history-compaction` | historical-only | 一次性专题三件套已退役删除；当前验证入口为 `doc/devlog/README.md` compact archive summary、engineering 根入口/主项目/索引回写与 `doc-governance-check` | `doc/devlog` 历史入口、`PRD-ENGINEERING-025` 第一条 follow-up 收口 |
| PRD-ENGINEERING-027 | historical `world-simulator-viewer-path-governance`（已退役） | historical-only | 当前检查 `doc/world-simulator/viewer/README.md` 首读分流、维护触发器、`doc/world-simulator/README.md` / `doc/world-simulator/prd.index.md` 上游可达性与 `doc-governance-check`；原三件套从 git history 与 PR 与实际验证记录 追溯 | `world-simulator/viewer` 热点子域入口与 `PRD-ENGINEERING-025` 第二条 follow-up 的长期承接 |
| PRD-ENGINEERING-028 | historical `p2p-node-path-governance`（已退役） | `test_tier_required` | 当前检查 `doc/p2p/node/README.md` 首读分流、维护触发器、`doc/p2p/README.md` / `doc/p2p/prd.index.md` 上游可达性与 `doc-governance-check`；原三件套从 git history 与 PR 与实际验证记录 追溯 | `p2p/node` 热点子域入口与 `PRD-ENGINEERING-025` 第三条 follow-up 的长期承接 |
| PRD-ENGINEERING-029 | `testing-evidence-path-governance` | historical-only | 一次性专题三件套已退役删除；当前验证入口为 `doc/testing/evidence/README.md` 首读分流、`doc/testing/README.md` / `doc/testing/prd.index.md` / engineering 根入口回写与 `doc-governance-check` | `testing/evidence` 热点子域入口、`PRD-ENGINEERING-025` 第四条 follow-up 收口 |
| PRD-ENGINEERING-030 | historical `readme-governance-path-governance`（已退役） | historical-only | 当前检查 `doc/readme/governance/README.md` 首读分流、维护触发器、`doc/readme/README.md` / `doc/readme/prd.index.md` 上游可达性与 `doc-governance-check`；原三件套从 git history 与 PR 与实际验证记录 追溯 | `readme/governance` 热点子域入口与 `PRD-ENGINEERING-025` 第五条 follow-up 的长期承接 |
| PRD-ENGINEERING-033 | `doc-hotspot-path-aftercare` | `test_tier_required` | `doc/world-simulator/launcher/README.md` 与 `doc/game/gameplay/README.md` 首读分流、模块 `README.md` / `prd.index.md` 回写、`bash scripts/doc-inventory-report.sh`、`doc-governance-check` 与 `git diff --check` 通过 | inventory 重新暴露的热点路径 aftercare，避免全仓机械治理并继续压低默认阅读面成本 |
| PRD-ENGINEERING-032 | `skill-replacement-rationalization` | `test_tier_required` | skill rationalization 专题三件套、低耦合 skill 删除、角色卡/活跃文档引用清理、`doc-governance-check`、`pm-lint` 与 `git diff --check` 通过 | `.agents/skills` 本地维护面、角色卡推荐 skill 真实性与 engineering/self-evolution 治理边界 |

- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-ENG-001 | 以脚本门禁落实规范 | 仅依赖人工评审 | 自动化一致性更高且可复现。 |
| DEC-ENG-002 | 保留 allowlist 冻结机制 | 完全开放文档新增 | 可控制结构漂移和历史债扩散。 |
| DEC-ENG-003 | required/full 分层验证 | 单层测试策略 | 兼顾效率与风险覆盖。 |
| DEC-ENG-004 | 老格式文档按批次渐进迁移并采用逐篇人工重写 | 一次性全量改写或自动脚本改写 | 人工重写更利于保留语义细节并控制内容质量。 |
| DEC-ENG-005 | 采用四人并行、目录前缀互斥分工推进大规模迁移 | 单人串行推进或随机切片 | 可兼顾迁移速度、冲突控制与审阅可追溯性。 |
| DEC-ENG-006 | Owner-D 先完成非根入口 60 篇，再单独收口 3 份根入口 redirect project 文档 | 在同一批次混合推进所有 63 篇 | 可减少根入口语义争议导致的回退频次，同时保持可追溯燃尽。 |
| DEC-ENG-007 | D2 完成后保留根入口 `.prd` redirect 并统一引用到新命名 | 恢复旧命名入口或删除 root redirect | 兼顾迁移收口一致性与历史入口兼容性。 |
| DEC-ENG-008 | 为全部模块增加文件级索引并纳入入口链路 | 仅保留目录级导航 | 文件级索引可显著降低“文档存在但不可达”问题。 |
| DEC-ENG-009 | 双向互链作为门禁硬规则 | 仅人工评审追溯关系 | 自动阻断可避免追溯链路长期漂移。 |
| DEC-ENG-010 | 模块任务项显式标注 `test_tier_required/full` | 仅在 PRD 总表声明 tier | 任务级标注更直接支撑评审与执行。 |
| DEC-ENG-011 | 将活跃文档引用路径可达性纳入门禁并维护最小豁免白名单 | 仅靠人工抽查断链 | 迁移后断链可自动阻断，减少隐性导航故障。 |
| DEC-ENG-012 | 2026-03 阶段采用全量逐篇审读留痕；当前该旧清单面已退役为历史留痕 | 仅维护模块级进度百分比 | 当期逐篇留痕提供了审计与遗漏定位；当前 truth 已收敛到模块入口和 round logs。 |
| DEC-ENG-013（历史） | 当期审读先按代码核实事实描述 | 仅凭历史文档推定当前实现 | 保留当期事实回填理由；现行要求与设计 authority 不被代码覆盖，按上述偏差分流处理。 |
| DEC-ENG-014 | 重复与上下游对齐问题在同批次完成修复与回填 | 跨批次累积处理 | 同批次闭环可避免问题扩散到下一轮审读。 |
| DEC-ENG-015 | 根 `AGENTS.md` 仅保留 7 个组合角色入口，详细职责下沉到 `.agents/roles/*.md` | 在根 `AGENTS.md` 内持续堆叠所有角色长描述 | 入口更短、更稳，且更便于按角色独立演进职责卡。 |
| DEC-ENG-016 | 为角色协作提供统一 handoff 模板，并放在 `.agents/roles/templates/` | 继续依赖自由格式口头/临时文本交接 | 统一模板能显著降低跨角色遗漏、返工和上下文漂移。 |
| DEC-ENG-017 | 将 `AGENTS.md` 工作流升级为角色协作版，并显式写入 handoff / QA / LiveOps / GitHub PR review / no-commit 例外 | 继续保留单线程开发表述，或让额外本地 review 替代 owner role / GitHub PR 默认评审边界 | 当前仓库已引入角色职责卡与交接模板；需要继续由角色视角承接 owner 责任，并把 GitHub PR 的 required checks、requested changes/comment closeout、mergeability 与 merge path 作为默认评审/合流边界；`REVIEW_REQUIRED` 仅回报不阻塞。 |
| DEC-ENG-018 | 自我进化项目管理首期采用仓库内文件化运行层 | 直接将外部 PM/SaaS 作为主真值 | 当前仓库已具备 Git/worktree/正式文档治理链，先在 repo 内闭环更符合现有工程约束。 |
| DEC-ENG-020 | 角色名通过 `.agents/roles/*.md` 自动生成白名单并由门禁校验 | 允许自由填写角色名或维护独立手写名单 | 自动从单一事实源派生，最不容易漂移。 |
| DEC-ENG-021 | 在每张角色职责卡内补充“推荐技能”区段，并明确“角色定 owner，技能定方法” | 仅在对话中临时口头说明角色与技能关系 | 关系落盘后更利于新人自助选择方法，也能降低角色/技能混用带来的协作歧义。 |
| DEC-ENG-021 | 将“需要其他伙伴协作”的默认执行语义收敛为“切换到标准角色视角并加载职责卡” | 保留“可开启 sub agent”表述 | 角色视角切换已被现有职责卡、handoff 模板与工作流规则完整支持，且不依赖额外运行时能力，执行口径更稳定。 |
| DEC-ENG-023 | 先冻结“阅读面分层”而不是立刻重组目录树；`活跃真值 / 审计留痕 / 历史归档 / 兼容跳转` 的差异先体现在 root README、模块 README、`prd.index.md` 与专题互链上 | 立即重引入 `archive/` 树或直接批量移动 `reviews/governance/evidence` 文件 | 当前问题首先是默认阅读面过宽，而不是路径本身不可存放；先压缩入口和消费层，风险最小，也不破坏现有引用稳定性。 |

<a id="PRD-ENGINEERING-035"></a>

## PRD-ENGINEERING-035 私有资料管理工具

机器、环境与操作说明保存在独立私有 Markdown 库，支持条件更新、本地历史与加密新目录恢复。需求与验收见 [私有资料产品需求](private-context/private-context.prd.md)，实现合同见 [系统设计](private-context/private-context.design.md)。

<a id="PRD-ENGINEERING-036"></a>

## PRD-ENGINEERING-036 Rust 工程契约

Workspace 共性配置、现行架构边界的可执行检查与协议产物的可重现生成，由 [Rust 工程契约需求](rust-governance/rust-engineering-contracts.prd.md) 拥有稳定要求与验收，[系统设计](rust-governance/rust-engineering-contracts.design.md) 承接配置等价、合法依赖范围、服务 profile、迁移与验证。三项要求在专业原文分别声明 Trace ID；此入口不重复声明。

本专题先形成目标设计，当前实现与剩余范围在专业文档分别说明；文档采纳不代表自动检查已经启用。现有文件体量规则、required/full 门禁与通用开发流程保持各自主责，实际交付事实见 PR、CI 与评审记录。

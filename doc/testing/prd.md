# testing PRD

审计轮次: 9

## 目标
- 建立 testing 模块设计主文档，统一需求边界、技术方案与验收标准。
- 确保 testing 模块后续改动可追溯到 PRD-ID、任务和测试。

## 范围
- 覆盖 testing 模块当前能力设计、接口边界、测试口径与演进路线。
- 覆盖 PRD-ID 到 PR、实际 CI 与评审记录（Issue 按需） 的任务映射。
- 不覆盖实现代码逐行说明与历史过程记录。

## 接口 / 数据
- PRD 主入口: `doc/testing/prd.md`
- 可变任务状态与历史: PR、实际 CI 与评审记录（Issue 按需）
- 文件级索引: `doc/testing/prd.index.md`
- 追踪主键: `PRD-TESTING-xxx`
- 测试与发布参考: `testing-manual.md`

## 里程碑
- M1 (2026-03-03): 完成模块设计 PRD 主体重写与任务改造。
- M2: 补齐模块设计验收清单与关键指标。
- M3: 建立 PRD-ID -> Task -> Test 的长期追踪闭环。

## 风险
- 模块边界演进快，文档同步可能滞后。
- 指标口径不稳定会降低验收一致性。
## 1. Executive Summary
- Problem Statement: 测试套件覆盖范围广（required/full、Web 闭环、长跑、分布式），但目标与触发矩阵若不集中维护，容易出现“通过 CI 但缺少真实风险覆盖”。
- Proposed Solution: 以 testing PRD 统一定义分层测试体系、触发条件、证据标准与发布门禁，并对齐 `testing-manual.md`。
- Success Criteria:
  - [SC-1：测试层级映射](#testing-sc-1)。完整声明见对应精确条款。
  - [SC-2：required/full 门禁](#testing-sc-2)。完整声明见对应精确条款。
  - SC-3: Web UI 闭环与分布式长跑在发布流程中有可追溯证据，且明确区分 `Viewer(agent-browser)` 与 `launcher(GUI Agent first)` 两条驱动链路。
    - SC-3A: `release-gate-web` 在 `renderMode=viewer` 的主 Web 入口上，必须接受 `play/pause` 先返回 `queued` 的 live-control 契约，并以后续 `step` 收到 `completed_advanced` 且产出正向 world delta 作为 formal progress 判据，不再要求 `play` 立刻推进 tick 或强制选中 Agent；`renderMode=software_safe` 仅作为兼容 alias 覆盖。
    - SC-3B: 正式 gameplay evidence packet 必须显式区分 `player leverage` 与 `ambient world activity`，并回答“玩家做了什么、世界因此变了什么、这是否打开下一步决策”。
    - SC-3C: `viewer` 与 `pure_api` 这两个 formal 玩家 surface 必须能用同一份 `snapshot.player_gameplay` 事实源回答同一组核心问题：当前阶段、当前目标、进度、阻塞、下一步建议，以及最近一次关键世界变化；其中 `software_safe` 只作为 `viewer` 兼容 alias，`pure_api` 的 `parity_verified` 只适用于 active LLM access，no-LLM 只能记为 blocked/observer-debug。
  - SC-4: 测试范围可追溯到实际适用需求与专业验收；PRD-TESTING-ID 保留作本模块定位，不要求每次变更新建任务身份。
  - SC-5: 活跃 testing 专题文档按批次完成人工迁移到 strict schema，并统一 `*.prd.md` / `*.design.md` / `*.manual.md` 的文档职责；执行状态由实际 PR、CI 与评审记录承接，Issue 按需。
  - SC-6: builtin wasm（m1/m4/m5）hash 发布链路具备 changed-path scope planner、跨 runner 对账、required check 保护与本地只读校验策略。
  - SC-7: 主链 Token 创世前具备一份 QA 审计清单，覆盖分配比例、custody/treasury 语义、个人上限、创世流通与首年释放上限，避免带着错误经济配置进入执行。
  - SC-8: testing root authorities define the `L1/L2/L3/L4A/L4B/L5` evidence boundaries and forbid writing an automation pass as proof that the game is fun.
  - SC-9: the durable authority defines standard-role review inputs, outputs, trigger matrix and escalation boundary for repeatable internal review.
  - SC-10: the durable authority defines a simulated-player persona panel as an internal hypothesis input to standard-role review, not a formal `player` role.
  - SC-11: the durable authority separates `L4A synthetic internal playability review` from `L4B embodied agent playtest`; internal human play is only optional L4B calibration and never L5.
  - [SC-12：离线 L4 scaffold](#testing-sc-12)。完整声明见对应精确条款。
  - [SC-13：真实 L4B runner](#testing-sc-13)。完整声明见对应精确条款。
  - SC-14: testing 模块具备一份 canonical 模型视觉评审 SOP，使截图/布局/遮挡/层级/可读性等常规视觉 review 默认由模型完成，并明确何时必须升级人工 owner。
  - [SC-15：source-to-scene 方法](#testing-sc-15)。完整声明见对应精确条款。

<a id="testing-sc-1"></a>
### SC-1：测试层级映射

  - SC-1: 关键改动路径均可映射到明确测试层级（S0~S10）。

本条继续覆盖所有关键改动的 S0~S10 选择，不缩为自主试玩。仅在[自测反馈闭环](governance/agent-runtime-self-playtest.design.md#selftest-feedback-loop)所选失败→普通回归→修复→新候选独立复测子链中，承接 [REQ-WR-PE-002](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-002) 的证据层级不可代签和 [REQ-WR-PE-003](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-003) 的同一候选/明确变更范围比较边界；不把层级映射、CI 工具或整个测试体系等同于产品能力，也不声称这两项产品要求覆盖全部测试触发义务。

<a id="testing-sc-2"></a>
### SC-2：required/full 门禁持续可用

  - SC-2: required/full 门禁持续可用且与手册口径一致；其中 PR `required-gate` 允许在保持稳定 check context 的前提下按 changed paths 剪裁无关重型组件，并在命中 `oasis7_client_launcher` / launcher shared runtime 路径时补跑 launcher Web `trunk build`；`full-support` 必须直接触达 workspace 内稳定可跑的 support crate 测试入口。

required/full、稳定 check context、changed-path 剪裁、launcher Web 构建和 support crate 入口仍是本条完整的 testing 工程义务。仅其[自主试玩反馈与复测](governance/agent-runtime-self-playtest.design.md#selftest-feedback-loop)子链承接 [REQ-WR-PE-003](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-003) 的当前候选/变更范围证据比较，以及 [REQ-WR-PE-002](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-002) 的普通回归不能代签真实自主试玩或 L5；该有限关联不把整个 required/full 门禁声明为产品验收，也不改变原命令、触发或必跑范围。

## 2. User Experience & Functionality
- User Personas:
  - 测试维护者：需要统一分层模型与执行标准。
  - 功能开发者：需要明确改动后最小必跑集合。
  - 发布负责人：需要审计级测试证据判断放行。
  - 制作人与经济配置维护者：需要一份可审计的创世配置检查表，避免只靠聊天结论发币。
  - 制作人与玩法 owner：需要一套分层证据栈，区分“没坏”“世界在动”“玩家真的想继续玩”。
  - 内部玩法评审 owner：需要固定的 simulated personas，避免每次靠临时脑补多个“可能的玩家”。
  - UI / Viewer owner：需要用截图和模型视觉判断替代绝大部分人工视觉扫图，只把低置信或对外 claim 交给人。
- User Scenarios & Frequency:
  - 开发分支回归：每次核心改动后触发一次 required 路径。
  - 发布候选验证：每个候选版本执行 required + full 组合。
  - 专项长跑：高风险链路按周执行并沉淀趋势结果。
  - 失效复盘：出现逃逸缺陷后补齐回归与触发矩阵。
  - 前期工业体验回归：影响 `首个制成品 / 停机恢复 / 首座工厂单元` 时，补跑 required-tier 手动卡组。
  - 创世配置冻结前审计：每次准备冻结 Token 分配表时执行一次 required-tier 配置审计。
  - 信任门 / 留存证据复核：每次宣称“玩家值得继续玩”前，必须先检查该样本是否只是世界在自己运转。
  - 玩法质量争议复盘：每次出现“自动化通过但体验仍差”的争议时，必须先定位缺的是哪一层证据。
  - UI / 视觉 PR 收口：每次改动触达可视化相关代码、样式、资源或可见输出时，优先产出截图并执行模型视觉评审；若 owner 判断不影响可见 surface，必须在任务日志或 PR evidence 中写明豁免理由。
- User Stories:
  - PRD-TESTING-001: As a 测试维护者, I want one canonical testing strategy, so that suite evolution stays coherent.
  - PRD-TESTING-002: As a 开发者, I want clear trigger matrices, so that I can run the right tests efficiently.
  - PRD-TESTING-003: As a 发布负责人, I want auditable evidence bundles, so that release decisions are defensible.
  - PRD-TESTING-004: As a 文档维护者, I want each legacy testing topic doc manually migrated with content-preserving rewrite, so that historical intent remains accurate after format upgrade.
  - PRD-TESTING-005: As a 发布工程维护者, I want builtin wasm hash chain hardened end-to-end, so that hash drift can be blocked and traced before release.
  - PRD-TESTING-006: As a `qa_engineer`, I want a token genesis allocation audit checklist, so that producer/runtime can freeze mint configuration without hidden control or circulation risk.
  - PRD-TESTING-007: As a `producer_system_designer`, I want a canonical playability evidence stack, so that I can judge gameplay fun without conflating automation, world activity, and real player motivation.
  - PRD-TESTING-008: As a workflow owner, I want a designed system for role-based playability review subagents, so that internal multi-role review can run as a standard operating path instead of ad hoc coordination.
  - PRD-TESTING-009: As a playability reviewer, I want a designed panel of simulated player personas, so that internal review can compare multiple player mindsets without inventing a new formal role taxonomy.
  - PRD-TESTING-010: As a stage owner, I want `L4A/L4B/L5` boundaries written explicitly, so that synthetic、agentic、real-human playability claims stop collapsing into one undifferentiated `L4`.
  - PRD-TESTING-011: As a UI / Viewer owner, I want screenshot-based model visual review to be the default routine visual reviewer, so that most manual visual review is replaced by reproducible evidence and humans only handle escalations.
  - PRD-TESTING-012: As a QA reviewer, I want an auditable source-to-scene receiver and per-object evidence-retention map, so that deterministic, integration, and full-environment evidence cannot be conflated or silently dropped.
- Critical User Flows:
  1. Flow-TST-001: `识别改动类型 -> 匹配 S0~S10 -> 日常提交先执行 commit baseline，再按风险升级到 required/full -> 输出结果`
  2. Flow-TST-002: `发布前执行 full 套件 -> 按 Viewer/launcher 选择正确驱动链路 -> 汇总命令/日志/截图 -> 生成证据包`
  3. Flow-TST-003: `线上问题复盘 -> 回填触发矩阵 -> 增加回归用例 -> 验证闭环`
  4. Flow-TST-004: `逐篇阅读 legacy 专题文档 -> 按 strict schema 人工重写 -> 按需求/设计/手册职责命名 -> 回归校验`
  5. Flow-TST-005: `触发 wasm hash 校验 -> 跨 runner 对账 -> required check 放行/阻断 -> 发布链路收口`
  6. Flow-TST-006: `识别工业引导体验改动 -> 运行自动化前置 -> 执行 playability 卡组 -> 回写 QA 阻断结论`
  7. Flow-TST-007: `读取 token 创世参数表 -> 逐项核对比例/recipient/vesting/流通边界 -> 输出 QA verdict -> 回流 producer 决策`
  8. Flow-TST-008: `汇总 trust/playability 样本 -> 回答玩家动作与玩家导致的世界变化 -> 标记 world_activity_only -> 再给 pass/watch/block 结论`
  9. Flow-TST-009: `按 evidence stack 标记当前只到 L1/L2/L3/L4/L5 哪一层 -> 明确缺口 -> 再决定是否能声称“值得继续玩”`
  10. Flow-TST-010: `组装 review packet -> 按 changed surface 拉起标准角色 subagent -> 回收 review card -> producer/qa 汇总内部结论`
  11. Flow-TST-011: `若体验争议来自玩家风格差异 -> 选择 simulated personas -> 生成 persona cards -> 回流标准角色 review`
  12. Flow-TST-012: `若先做高强度内部模拟 -> 形成 L4A；若需要 agent 实际进游戏操作且尽量逼近真人评审效果的结论 -> 升级到 L4B；若仍需真实人类 / 真实环境结论 -> 再升级到 L5；内部真人试玩只作为 L4B 可选校准`
  13. Flow-TST-013: `识别可视化相关改动 -> 采集 desktop/mobile 截图 -> 附自动化摘要与视觉 contract -> 模型输出 visual review card -> verdict=pass 且 confidence=high 替代 routine 人工视觉 review，verdict=watch/block/human_escalation 或 confidence=medium/low 升级人工`
<a id="visual-review-product-contract"></a>

视觉验收以[模型视觉评审判定表](manual/model-visual-review-sop-2026-05-29.manual.md#visual-review-decision-contract)为唯一 verdict/confidence 口径：仅 `pass + high` 且无独立升级条件可替代 routine review，首次 `watch` 与 `medium/low` 都升级；必需截图不能被 DOM 或 smoke 文本替代。该规则不替代 GitHub required review、发布 owner 放行、视觉方向裁决、对外 claim 审批或 L5。

- Durable Playability Evidence Governance:
  - `L1` automation proves reproducibility and regressions; `L2` controlled probe proves reachability, pacing or response; `L3` telemetry/experiment compares variants or exit points. None proves fun alone. `L4A` is standard-role/persona/surface-based synthetic review, `L4B` requires an agent to execute a real minimum play loop with session evidence, and `L5` is real-player or controlled external evidence. `L4A != L4B != L5`.
  - Every playability packet records formal surface, `player_action`, `world_change_due_to_player`, `player_leverage_score` and `world_activity_only`. A world-moving sample with no player-caused change is `world_activity_only=yes` and cannot support meaningful participation or a continue claim.
  - `producer_system_designer` and `qa_engineer` review every packet first; surface-specific roles follow. Each card distinguishes direct evidence, inference, what it does not prove, and follow-ups; use `insufficient_input` or `secondary_review_only` when applicable. Personas (`new_player_confused`, `impatient_action_player`, `systems_optimizer`, `narrative_curiosity_player`, `chaos_tester`) are L4A inputs only and must hand off to a standard role.
  - `block` applies to unstable formal surfaces or absent player leverage; `hold` to failed synthetic continuation or high-value L4B contrary evidence; `watch` to missing/thin/conflicting L4B; `go` only within a producer-defined claim envelope when required L1-L4B evidence has no high-value contrary evidence and L5 has no new contrary evidence. QA guards the evidence boundary.
- Functional Specification Matrix:
| 功能点 | 字段定义 | 按钮/动作行为 | 状态转换 | 排序/计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 分层测试触发 | 改动类型、测试层级、命令集合、changed-path scope | 依据矩阵选择最小必跑集合；PR `required-gate` 先规划 `minimal / targeted / full` 再执行命中的重型组件，必要时追加 launcher Web `trunk build` | `planned -> scoped -> running -> passed/failed` | 默认先 `commit`，按风险升级到 `required`，发布加跑 `full`；docs-only / 无关元数据 PR 允许在 stable context 下退化为 governance/fmt-only；`oasis7_client_launcher` / launcher shared runtime 命中时 required-gate 追加 launcher Web 构建覆盖 | 开发者可执行，发布者可放行 |
| Web UI 驱动分流 | `surface_type`、`driver`、`evidence_mode` | Viewer 页面通用闭环默认走 `agent-browser`；真实本地栈 + 玩家 UI 操作流程回归走 Playwright 实跑系列；`oasis7_web_launcher` 产品动作默认走 GUI Agent，页面仅做状态/字段校验 | `selected -> driven -> verified` | 先按 surface 分流，再决定是否补充 Canvas/页面采样；Playwright 实跑用例按 `PWT-###` 编号管理 | QA/发布与产品 owner 共同遵循 |
| 模型视觉评审 | `visual_change_trigger`、`screenshot_set`、`expected_visual_contract`、`automated_evidence`、`verdict`、`confidence`、`human_escalation_needed` | 任何可视化相关代码、样式、资源或可见输出改动默认触发截图评审；对截图执行固定 rubric，输出 visual review card；`verdict=pass` 且 `confidence=high` 可替代 routine 人工视觉 review；`verdict=watch/block/human_escalation` 或 `confidence=medium/low` 必须升级人类 owner，并写明 owner action | `visual_change_detected -> captured -> model_reviewed -> pass/watch/block/escalated` | 先跑确定性测试，再让模型判断视觉；只有明确不影响任何可见 surface 的改动才能豁免且需记录理由；模型只替代截图/布局/可读性类人工 review，不替代 GitHub required review、发布放行、L5 真实玩家验证或对外 claim 审批 | `producer_system_designer` / `qa_engineer` 守边界，命中的 UI owner 处理 findings |
| Viewer release 语义门禁 | `renderMode`、`lastControlFeedback.stage`、`deltaLogicalTime`、`deltaEventSeq` | `viewer` 下允许 `play/pause` 先返回 `queued`；formal progress 以后续 `step` 的 `completed_advanced` + 正向 world delta 判定；`software_safe` 仅作为 compat alias 覆盖 | `queued -> completed_advanced` 或 `queued -> blocked` | 主 Web 入口不再要求 `play` 立刻涨 tick，也不再强制 `selectedKind=agent`；若 `llm_required` 显式阻断则按 blocker 合约留痕 | QA/发布维护者维护 |
| 证据包归档 | 命令、日志、截图、结论、责任人、`player_action`、`world_change_due_to_player`、`player_leverage_score`、`world_activity_only` | 执行后归档并建立索引 | `collecting -> archived -> reviewed` | 按版本与模块分层索引；若 `world_activity_only=yes`，则该样本不能直接支撑玩法放行 | 测试维护者负责最终校验 |
| 缺陷回归闭环 | 缺陷ID、触发条件、修复提交、复测结论 | 缺陷关闭前必须绑定回归记录 | `opened -> fixed -> regressed -> closed` | 高风险缺陷优先回归 | QA/维护者可更新状态 |
| 文档格式迁移 | 旧文档路径、约束点清单、目标命名 | 人工重写并更名，补全映射与验证证据 | `inventory -> migrated -> validated` | 先迁移活跃文档、后迁移归档文档 | 维护者审批迁移质量，贡献者执行 |
| Builtin wasm hash 治理 | 模块集、canonical token、runner 摘要、required check context、release evidence、scope planner | 执行 Docker canonical `sync --check`、按 changed paths 规划 scope、摘要导出与证据对账、分支保护同步 | `check-only -> planned -> reconciled -> protected` | 发布清单仅允许 `linux-x86_64` canonical token，identity 输入使用 receipt + 白名单；无关 PR 保持 stable required-context no-op | 本地默认只读校验，写路径限定非 CI 的显式授权 |
| Release 资产预构建复用 | web dist artifact、cargo cache key、bundle build command set | 同一 release workflow 先产出 viewer/launcher 静态包并复用 warm cache；后续打包不得重复 bootstrap 相同 Web 产物 | `bootstrapped -> reused -> packaged` | 先复用同轮 artifact / cache，再允许脚本 fallback；原生 bundle 构建优先单次 cargo 调用 | QA / 发布维护者维护 release 时延口径 |
| Windows 路径兼容校验 | tracked path、invalid segment、gate command、release runner | 在 required gate 早期扫描 git tracked paths，阻断 `windows-2022` 无法 checkout 的文件名进入 release/package-native | `scanned -> pass/block` | 默认按 git tracked path 全量扫描；发现 Windows 非法字符、保留名、尾随空格/点即直接 fail | QA / 发布维护者维护跨平台 release 可达性 |
| Release 脚本执行位校验 | tracked path、tracked mode、worktree execute bit、gate command | 在 commit/required gate 早期扫描 release 关键 shell 脚本的 tracked mode 与 worktree 执行位，阻断会让 `release-gate-web` / `package-native` 只在 runner 上暴露 `Permission denied` 的回归 | `scanned -> pass/block` | 默认覆盖 release gate、web 回归、bundle/package 关键 `.sh`；tracked mode 必须为 `100755`，worktree 也必须可执行 | QA / 发布维护者维护 release 可执行性 |
| Runtime gate 分片执行 | full-suite shard、sync check、runner capability、日志 artifact | 将 release runtime gate 拆成 core/support/sync 并行 job；聚合 gate 统一裁决是否放行 | `planned -> sharded -> aggregated` | 重型 `oasis7` full-tier 优先单独成 shard，其余 support / sync 独立并行；最终必须全部成功 | QA / 发布维护者维护 runtime 关键路径 |
| Token 创世配置审计 | `bucket_id`、`ratio_bps`、`recipient`、`cliff_epochs`、`linear_unlock_epochs`、`genesis_liquid`、`founder_cap_bps`、`year1_external_release_cap_bps` | 逐项核对参数表与经济口径，输出 `pass/block` 审计结论 | `draft -> audited -> pass/block` | `sum=10000 bps`；项目战略控制 `5000 bps`；协议长期储备 `3500 bps`；`genesis_liquid=0`；个人上限 `<=1500 bps` | `qa_engineer` 独立出具结论，producer 决定是否冻结 |
| 好玩性证据栈 | `evidence_layer`、`formal_surface`、`player_leverage_verdict`、`world_activity_only`、`synthetic_playability_verdict`、`agentic_playtest_verdict`、`optional_internal_human_corroboration`、`external_signal_status` | 把玩法结论分层标记为 L1 自动化、L2 probe、L3 遥测/实验、L4A synthetic、L4B agent、L5 外部信号，再输出 `go/watch/hold/block` | `collected -> layered -> decided` | 低层证据不能替代高层证据；`L4A` 不能自动等于 `L4B`，`L4B` 也不能自动等于 `L5`；自动化 pass 只能证明“没坏/可回归”，不能单独证明“好玩” | `producer_system_designer` 终判，`qa_engineer` 守门 |
| 好玩性 subagent 评审系统 | `review_packet`、`requested_roles`、`role_review_card`、`aggregated_review_summary` | 按 changed surface 拉起标准角色 subagent，收集 review card，并汇总成内部结论 | `packet_ready -> parallel_review -> aggregated -> escalated/closed` | 默认必开 `producer + qa`；其余按 surface 触发；缺 L5 时不得越权宣称真实外部验证完成 | `producer_system_designer` 编排，`qa_engineer` 守门 |
| 模拟玩家 persona 面板 | `selected_personas`、`persona_card`、`persona_divergence_summary`、`handoff_recommended_to` | 按主观体验风险选择多个 simulated personas，生成风格化体验假设，再回流标准角色 review | `selected -> simulated -> handed_off -> absorbed` | 不新增正式 `player` 角色；persona 只能补内部假设，不能替代 agent 实操试玩、真人试玩或外部验证 | `producer_system_designer` 决定是否开启，命中的标准角色负责收口 |
| L4 synthetic/agent 分层 | `synthetic_playability_verdict`、`agentic_playtest_verdict`、`optional_internal_human_corroboration`、`calibration_status` | 先区分当前结论属于 `L4A` 还是 `L4B`，再决定是否可以升级 claim | `synthetic_ready -> agent_ready -> external_ready` | `L4A` 不得冒充 `L4B`；内部真人佐证只可作为 `L4B` 校准，不得伪装成 `L5`；无 calibration 时不得宣称低层已替代高层 | `producer_system_designer` 定义边界，`qa_engineer` 守门 |

<a id="testing-sc-12"></a>
### SC-12：离线 L4 scaffold

  - SC-12: 仓库必须提供一个 repo-local `L4` scaffold 入口，能够在单个 worktree 内稳定生成 `L4A` review packet、role/persona cards、`L4B` agent 卡副本、可选内部真人佐证 notes、最终 summary 与推荐命令，不再依赖临时手写 packet/card 文件名。

本条只要求生成离线评审材料与推荐命令，具体输出与目录保护保持 [L4 scaffold 输出](#l4-scaffold-output)及 L4-C06 的原合同。产物存在不表示角色评审、真实子会话、游戏操作或 L4A/L4B 已执行；真实进入、执行、结果与体验判断由独立的 [SC-13](#testing-sc-13)保留，不能随 scaffold 工程范围一并豁免。

<a id="testing-sc-13"></a>
### SC-13：真实 L4B 进入、执行与结果

  - SC-13: 仓库必须提供一个 repo-local `L4B` embodied-agent runner，按 [L4 构建与驱动](#l4-build-driver)选择产物，在[实际 hosted 操作会话](#l4-hosted-session)完成合法登录后执行真实操作，并把成功、失败或阻断的 [RunResult](#l4-execution-result) 留在同一 artifact；自动采集事实与[继续游玩/玩家杠杆评审](#l4-evidence-judgment)分开。

产品关联仅承接 [REQ-WR-PE-002](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-002) 的真实操作与证据层级边界、[REQ-WR-PE-003](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-003) 的同候选/入口/证据窗口比较；在本自主试玩设计已选择的范围内，分别落实 [AC-EXT-011](../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-011) 的正式 Skill 接入和 [AC-EXT-010](../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-010) 的两 Runtime 首局生命周期，按各自原适用条件采证。此关联不替代下列 build、hosted session、RunResult、自动事实/人工评审和入口兼容的完整专业合同，不声称覆盖全部玩家价值或产品要求；缺真实入口、执行或结果必须保留 blocked/inconclusive，不能按离线 scaffold 记为产品不适用。

### L4 工具链合同（SC-12/13、AC-20/21）

本节规定 L4 工具链的目标行为和验收范围。引入本合同的文档修复不表示旧 `run-playability-l4b-agent.sh` 的 build/session 编排、评分预填或 scaffold 输出已修复；这些实现与执行义务来自 [#4438](https://github.com/eng-cc/oasis7/issues/4438)，已由 [PR #4641](https://github.com/eng-cc/oasis7/pull/4641) 承接为现行文档合同；按[关闭决定](https://github.com/eng-cc/oasis7/issues/4438#issuecomment-6127544475)，原 Issue 已停止独立跟踪，关闭不表示驱动修复或行为验收完成。后续实现仍须逐项落实下列合同并保留对应执行证据，不恢复原 Issue 必须持续开放的安排。系统承接见 [testing design](design.md)，各项验证见[总手册 L4-C01..06](../../testing-manual.md#l4-toolchain-validation)。

<a id="l4-build-driver"></a>
#### L4 构建与驱动

产品承接范围：[REQ-WR-PE-002](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-002) 的低层证据不得代签真实入口体验，以及 [REQ-WR-PE-003](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-003) 的候选与适用环境比较边界。本条仅规定 L4B 构建/驱动与该边界的对应；具体 build profile、API opt-in 和能力诊断是 testing/Viewer 专业合同，不据此声称产品全部验收已覆盖。

默认 L4B 目标采用 release 产物与正式 UI 操作，证明该候选真实玩家入口的行为。`visual-test` 必须显式选择、隔离产物，并记录实际构建与驱动；其测试 API 自动化只证明声明的受控范围，不能代签 release UI 或自主试玩。release 不安装 `__AW_TEST__`，追加 `test_api=1` 不能改变构建 profile；要求测试 API 的驱动遇到缺 API 时必须在登录/玩法等待前明确报告构建能力不匹配。对应验证：L4-C01。

<a id="l4-hosted-session"></a>
#### L4 hosted 会话

产品承接范围：[REQ-ENTRY-MODE-002](../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) 中身份/会话前置失败、真实恢复与禁止 fallback 冒充成功的边界；[REQ-WR-PE-002](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-002) 中 L4B 必须来自真实操作的证据边界。本条只消费既有 Hosted 登录/恢复能力；账号、认证、注册、授权和 owned cleanup 仍按专业合同，不能据此宣称 provider/模式/后端等所有失败路径已覆盖。

默认 `hosted_public_join` 的干净操作会话必须先识别 guest，再经受支持的登录/注册成为 authenticated，确认同一执行 session 的 player 身份和 gameplay readiness 后才能提交动作。已认证恢复须重新核对当前 session/player；另一个 headed 窗口的登录状态不能代签。缺账号配置、登录失败和身份不一致分别记录 blocker；不得通过 Hosted 自动签发或静默切换 `trusted_local_only` 通过。对应验证：L4-C02。

<a id="l4-execution-result"></a>
#### L4 执行结果与当前尝试

产品承接范围：[REQ-WR-PE-003](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-003) 的当前候选、入口与证据窗口可回链，以及 [REQ-WR-PE-002](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-002) 的缺失/阻断证据不得升级结论。本条用逐次尝试记录保留该判断依据；RunResult 字段、状态枚举和清理机制属于专业实现合同，记录存在不证明产品要求通过。

沿用 [RunResult 与逐项 verdict](governance/agent-runtime-self-playtest.design.md#6-接口与数据合同)及[来源到场景记录](governance/agent-runtime-self-playtest.design.md#selftest-source-scene)，记录真实 `execution_mode`、`current_attempt`、候选/实际 build、执行 session、已到阶段、停止原因、证据、清理结果与未覆盖项。运行阶段沿用 `preparing -> ready -> playing -> verifying -> reported -> cleanup`；任一阶段的阻断或取消都要保留当前尝试的结果，不依赖成功路径才写 summary。缺前置记 `blocked`，证据或结果未知记 `inconclusive`，只有已执行且违反约定才记 `fail`；未执行不写玩法失败或成功。

每次尝试独立保留，source/build/Skill 变化另建 attempt；汇总必须指向 `current_attempt`，旧成功不能遮蔽新阻断。阶段、逐项执行 verdict 和体验判断分别记录；这只复用 L4 artifact 的运行记录，不新增通用准入或合入回执。对应验证：L4-C03。

<a id="l4-evidence-judgment"></a>
#### L4 自动事实与体验判断

产品承接范围：[REQ-WR-PE-001](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-001) 的玩家可归因行动与下一选择、[REQ-WR-PE-002](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-002) 的证据层级不可代签，以及 [REQ-WR-PE-003](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-003) 的同一比较单位。本条约束采样与评审如何保留这些边界，不负责实现玩法因果链，也不证明真实玩家愿意继续。

runner 只自动填写实际动作、状态/进度变化、返回结果、时间与证据路径。时间推进、progress 增长、ACK 或 player 一致性不能直接生成 `agent_continue_observed`、玩家杠杆满分或 `world_activity_only=no`。继续结论和[玩家杠杆五分项](../playability_test_result/playability_test_card.md#30-玩家杠杆汇总)由 `producer_system_designer` / `qa_engineer` 逐项核对动作、可归因变化、结果解释、新决策与继续理由后填写；缺观察记未知，不能自动填 0 或 5。固定脚本路线保留实际 `execution_mode`，不能改名为自主规划证据。

模板和旧 runner 导入的产物默认为 `unreviewed`；缺实际试玩证据时 L4B 保持 `missing`，证据不足时不推断“agent 不愿继续”。自动事实追加到本次 attempt，不自动覆盖已经评审的最终 verdict；复核须绑定当前候选和尝试，保留旧结论。对应验证：L4-C04。

<a id="l4-entry-compatibility"></a>
#### L4 入口兼容与执行说明

产品承接范围：[REQ-ENTRY-MODE-001](../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001) 的 primary mode 证据独立性，以及 [REQ-WR-PE-003](../product/world-rules-core-gameplay/playability-evidence-and-claim-boundaries.prd.md#req-wr-pe-003) 中适用环境影响体验时的比较边界。本条只约束 L4 所选 Viewer canonical/compat 构建与入口；alias 不增加玩家模式，未覆盖 pure_api 的完整独立验收。具体 alias anchors、dist copy 和 test API 保留专业定义。

[Viewer entry compatibility PRD](../world-simulator/viewer/viewer-web-entry-compatibility.prd.md)、[配套 design](../world-simulator/viewer/viewer-web-entry-compatibility.design.md)、[game-test design](../playability_test_result/game-test.design.md)和[总手册](../../testing-manual.md#l4-toolchain-validation)必须使用同一构建/驱动边界。保留 canonical/compat alias 与原有 anchors；API 一致性仅适用于显式测试产物。浏览器 URL/session 取自本次 harness/producer 输出，不能用固定端口或另一会话替代。锁定 game-test 原文的适用范围由其配套 design 明确，不以旧 `test_api=1` 示例恢复 release 测试接口。对应验证：L4-C05。

<a id="l4-scaffold-output"></a>
#### L4 scaffold 输出

专业范围：本条的 JSON 序列化、路径传递和已有目录保护只保证离线评审材料生成工具的输出可用性，不定义玩家入口、玩法行为或产品结论。其验证仍完整保留 L4-C06；“不代表 L4A/L4B 已执行”是产物使用限制，不能把生成成功当作产品证据。

scaffold 必须支持普通、含空格和含单引号的合法输出目录。离线 `--json` 成功时 stdout 是一个可解析 JSON 对象；`run_id`、`output_dir`、`packet_path`、`summary_path` 与写出的 manifest 一致，`manifest_path` 指向该 manifest，命令退出 0。诊断不混入 JSON。保留已存在目录的拒绝和原文件；生成后发生错误时明确报告已生成范围，不能静默覆写重试。scaffold 成功只证明离线产物合同，不代表 L4A/L4B 已执行。对应验证：L4-C06。

### QA infrastructure receiver

This receiver turns accepted product and professional clauses into a reproducible verification plan and an auditable result. It is a QA evidence contract, not a product-rule authority, runtime implementation, or release approval. Product obligations start at [world-infrastructure Done](../product/world-infrastructure/prd.md#world-infrastructure-done); the P2P and deterministic-execution clauses remain in their professional PRDs and designs. The combined state-sync target inventory is the [GWSC design](longrun/game-world-state-sync-commit-closure-2026-06-26.design.md), including its exact 17 negative cases, 10 runtime groups, and 8 SC-9 cells. Those inventories define planned coverage; their presence here does not say that the cases were implemented or run.

<a id="testing-sc-15"></a>
#### SC-15：source-to-scene 方法

  - SC-15: QA infrastructure verification has a finite, reusable source-to-scene method that binds each accepted product and professional clause to its environment, oracle, artifact, evidence tier, and retained gap; it preserves the distinction between designed coverage and executed proof.

The exact record and evaluation contract remains [Source-to-scene record and evaluation](#qa-infrastructure-source-scene-method), with its separate [scene families](#qa-infrastructure-scene-families) and [tier boundaries](#qa-infrastructure-tier-boundaries). Product links identify only each selected source obligation: [world-infrastructure SC-9](../product/world-infrastructure/prd.md#sc-infra-009) for outage cells, [SC-10](../product/world-infrastructure/prd.md#sc-infra-010) for world identity, [REQ-DWE-001](../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001) and [REQ-DWE-004](../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004) for deterministic execution/version selection, and [AC-DCS-005](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-005) for consumer state transitions. These clauses retain their own authorities, identities, inventories and acceptance conditions; this QA method neither replaces them nor claims their complete coverage.

For the self-playtest receiver, bind only the selected [AC-EXT-010](../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-010), [AC-EXT-011](../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-011), and [AC-COLLAB-007](../product/agents-world-simulation/player-runtime-collaboration.prd.md#ac-collab-007) obligations to their actual scene/cell/specimen/phase/profile/entry/tier and preserve unselected gaps. Source links and record validity do not prove their execution; SC-15 is not a shared product identity for all QA, and a self-playtest record does not stand in for the infrastructure scene inventory.

<a id="qa-infrastructure-source-scene-method"></a>
#### Source-to-scene record and evaluation

For each selected scene, cell, specimen, and tier, retain one record that binds: exact source path/fragment/identifier plus source OID and digest; candidate source/base/tested-tree identity; scenario and applicability owner; execution state (`planned`, `located`, `implemented`, or `executed`); world/branch/root/revision/window and configuration; access mode and execution lane; baseline and ordered phase observations; dependency/topology and provider identity; raw artifact locator, byte length, and digest; command/result or visible action; oracle assertions; tier verdict, failure signature or blocking reason; and the unproved boundary. Do not record tokens, private keys, or secret payloads. A content digest is an artifact-integrity check, not a domain receipt or finality proof.

Evaluate in this order: (1) pin the current accepted source clauses and candidate/tree/world/window; reject stale or conflicting source identity; (2) enumerate the mandatory scene/case/cell and phase specimens, recording non-applicability only with the responsible domain owner's basis; (3) capture an unchanged baseline and classify the real dependency before an authorized execution; (4) verify artifact bytes, locator, length, and digest, then run structural, canonical, domain, trust, identity, and join oracles as separate checks; (5) compare before, interruption, fresh-read, recovery, retry, and replay observations for quantities, roots, receipts, windows, world progression, and consumer results; (6) independently verify the required provider/browser/Agent environment; (7) classify missing mandatory evidence as `blocked`/`unverified`, an observed invariant violation as `failed` with a reproducible signature, and shape-only evidence as `shape_verified`; use `passed` only when every assertion and the selected tier's environment are satisfied; (8) preserve success and failure artifacts append-only and report the exact scope and limitations. `planned`/`located`/`implemented`/`executed` are orthogonal to verdict; source links, fixtures, schema validity, and old green evidence never establish execution or current-candidate pass.

<a id="qa-infrastructure-scene-families"></a>
#### Finite source and scene families

| Scene | Source clauses and accepted coverage | Required oracle / evidence boundary |
| --- | --- | --- |
| `V-FINALITY` | Product SC-1/2; DCS-001/003/004 and DC-1/3/4; current topology, authority roles, proof, and ordering. | P2P owns certificate, signer-set, round, and storage oracles; runtime binds execution; Ops establishes actual topology. Reject wrong signature, threshold, set, round, non-authoritative write, and role expansion. A single-node fixture is not full finality proof. |
| `V-RECOVERY` | SC-3/6/7; DCS-002/005, DC-2/5, DE-3; GWSC DC5-01..05 and SR2-RT-01/03/09, including the separate [J1/J2 continuation obligations](longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-journal-suffix-recovery). | Join genesis/manifest/certificate/snapshot/log/root and same-candidate service state. Missing/conflicting proof stays isolated or blocked; preserve confirmed history; a closed commit gate yields zero new receipt, while restored pending work is rejudged under current conditions and may take effect at most once. Under fixed valid authority, manifest, permissions, resources, and ordered inputs, compare a finalized checkpoint C<T plus a real nonempty WorldJournal suffix with uninterrupted control at T and after the same future actions/ticks. Independently verify J1 fresh-request completion and J2 successful-tick consumption/wake/suspend continuity; at least one valid new recipe must complete once. Root equality or a receipt upper bound alone cannot pass this positive result. Retain actual blocker and next step. |
| `V-EXECUTION` | SC-4/5; DWE-001..004 and DE-1..4; all 17 QA negative cases and 10 runtime groups in the GWSC design. | Runtime/WASM owns determinism, ordering, migration, meter, and activation oracles; P2P owns canonical inclusion. Join both request orders, duplicates, crashes, replay, version selection, canonical bytes, and no-partial-mutation results. A local fixture does not prove canonical inclusion or recovery. |
| `V-INDUSTRIAL` | SC-8 plus §4 industrial narrative and §5.2 allocation in the world-infrastructure PRD; gameplay SC-20/21/28/31 and stable-line clauses in [`world-rules-core-gameplay`](../product/world-rules-core-gameplay/prd.md). | Gameplay owns recipe, quantity, capacity, reservation, handoff, sink, and W/window meaning; runtime supplies journal/root facts. Distinguish accepted from held, produced, or delivered; prove conservation, once-only release/effect, and retained residuals. Keep each consumer's actual clause and evidence owner. |
| `V-OUTAGE` | SC-9 outage narrative and exactly the eight named cells in GWSC `#sr2-sc9-eight-cells`: authority/read × stage_finish/transit/buffer_admission/terminal_settlement. | Fix one same-world baseline and retain before/outage/fresh/recovery/retry/replay values. Every authority cell includes root-absent pre-finality and root-present post-commit/pre-child specimens within that cell; do not add a ninth cell. Gameplay owns W/window disposition; Ops classifies the actual dependency; read outages never mutate world state. |
| `V-WORLD-SCOPE` | SC-10; runtime world-scope design; consumer world identity. | Compare distinct global and local/development `world_id` through pre-submit target, intent, committed receipt, and result. Missing, mismatched, reused, or replayed identity fails closed and has zero global effect; labels alone are not identity proof. |
| `V-CONSUMER` | SC-5/7/8/9/10; Agent PC-4 in [`provider-agent-experience-continuity`](../product/agents-world-simulation/provider-agent-experience-continuity.prd.md); cross-module FE-6/FE-7 in [`free-entry-world-progression-and-recognition`](../product/player-entry-distribution/free-entry-world-progression-and-recognition.prd.md); Agent harness §13 and Viewer control/readback contracts. | API, Viewer, and Agent observations must refer to the same candidate and agree on canonical state, status/blocker, root/lineage, quantities/window, receipt, progression, and next action. Prompt acknowledgement, provider prose, debug state, or a goal/memory entry is not world authority. Require actual surface/provenance evidence at the selected tier. |

The source receiver preserves distinct upstream axes: product SC clauses and narratives, DCS/DWE REQ/AC, P2P/runtime DC/DE, and the GWSC QA cases are mapped independently. The complete path-level retention disposition is in [the 89-object consumption matrix](design.md#qa-infrastructure-89-consumption-matrix); it records retained source/evidence, actual consumer clause, domain owner, and remaining proof gap for every frozen object.

For J1/J2, consume the [runtime journal-suffix contract](../world-runtime/prd.md#runtime-journal-suffix-recovery) and [validation design](../world-runtime/design.md#runtime-journal-suffix-validation). Full-snapshot roundtrip, canonical event-suffix replay, and ordered committed-input re-execution are separate evidence lanes; one cannot replace another. Journal replay must use recorded facts without rerunning historical WASM or charging again. These are planned deterministic runtime obligations within `V-RECOVERY` and SR2-RT-03; they add no provider or model-playtest requirement. Existing full service, topology, and consumer evidence remains independently required for those claims, and document checks do not prove runtime behavior.

<a id="qa-infrastructure-tier-boundaries"></a>
#### Tier boundary and claim limits

`test_tier_required` covers the 17 deterministic QA negatives, 10 runtime groups, and all 8 SC-9 cells, plus the active-provider `pure_api` consumer check required by the GWSC design. `test_tier_full` adds the same-candidate real-provider integration, external headed S6 capture at both desktop and narrow viewports, and provider-backed Agent parity. A mock provider, schema/fixture pass, screenshot alone, or local API response cannot substitute for those full requirements. Record provider kind, model, endpoint route, and preflight evidence/provenance as distinct fields while redacting credentials. Missing any mandatory cell, phase, archive readback, current verifier, or applicable environment keeps that scope blocked/unverified. These tiers do not by themselves establish playability, public-network readiness, or release approval.

PWT-001 remains the documented bounded local provider-backed UI case. PWT-003/005/006/007 remain planned receiver cases until their actions and matching evidence are executed. Preserve PWT-004's dated 2026-09-17 provider-deferred record: HTTP 403 `bad_response_status_code`, no hello/tool-call, live headed status blocked. Local contract/build checks do not promote it to provider or gameplay pass.

<a id="qa-infrastructure-equal-version-negative"></a>
#### Equal-version migration negative

The receiver includes the GWSC `MIG-01` specimen with structurally valid declared `PositiveU64` fields and `from_schema_version=to_schema_version="1"`. Shape validation is expected to accept this input shape. The semantic receiver must reject it before authoritative mutation, preserve the prior version/state/artifact bytes, and produce no partial charge, event, or world effect, following the existing [GWSC migration oracle](longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-dc5-semantic-validator). This is a planned expected result, not a test execution or a schema rule; no runtime/WASM/schema implementation or proof is claimed here.
- Acceptance Criteria:
  - AC-1: testing PRD 覆盖分层模型、触发矩阵、证据规范。
  - AC-2: 分层测试的稳定策略由本 PRD 和配套设计维护，演进任务的动态状态由实际 PR、CI 与评审记录承接（Issue 按需）。
  - AC-3: 与 `testing-manual.md` 保持一致且互相引用。
  - AC-4: 新增测试流程需标注 `test_tier_required` 或 `test_tier_full`。
  - AC-5: 每个迁移批次必须提供“原文约束点 -> 新章节映射”并通过文档治理检查。
  - AC-6: builtin wasm 发布链路治理（Docker canonical build + single canonical token + wasm-determinism-gate + required check + identity/release evidence 输入收敛）具备独立专题与任务追踪。
  - AC-7: `oasis7_web_launcher` / launcher Web 控制面必须显式标注 GUI Agent 优先，`agent-browser` 仅作为状态、字段与页面加载校验补充。
  - AC-8: 对前期工业引导体验的改动，必须能从 `testing-manual.md` 直接跳转到对应 required-tier 手动卡组。
  - AC-9: 同一 release workflow 内，Web release gate 与 `build-web-dist` 必须复用同一组 wasm/cargo cache，bundle 原生二进制构建默认收敛为单次 cargo 调用，避免重复 bootstrap。
  - AC-9B: `release-gate-web` 的 `viewer` 分支必须按当前 live-control 契约验收：`play/pause` 允许 `queued`，后续 `step` 必须收口为 `completed_advanced` 且 `deltaLogicalTime > 0` 或 `deltaEventSeq > 0`；`software_safe` 只作为 compat alias 复核；缺失 world delta 时要给出显式失败签名，而不是把“未即时涨 tick / 未选中 Agent”误判成回归。
  - AC-9A: release/package-native 触发前必须由 Linux required gate 显式扫描 tracked paths 的 Windows 兼容性，阻断会让 `windows-2022` checkout 直接失败的路径名。
  - AC-9C: release/package-native 触发前必须由 commit/required gate 显式扫描 release 关键 shell 脚本的 tracked mode 与 worktree 执行位，阻断会让 Linux runner 只在 `release-gate-web` / `package-native` 阶段暴露的 `Permission denied` 回归。
  - AC-9D: release gate 的任何显式 `--skip-*` 都必须提供非空 skip reason，并在 summary 中输出对应 claim boundary；带 skip 的结果不得支撑被跳过层级的完整 release coverage claim。
  - AC-10: `release-gate-runtime` 必须允许将 `ci-tests.sh full` 拆为至少两个并行 shard，并与 builtin wasm sync 检查独立聚合，保证放行语义不变。
  - AC-11: runtime shard 划分必须按关键路径持续重平衡；`oasis7 --lib --bins` 等中重量级套件不应长期挤占最重 shard。
  - AC-12: `doc/testing/**` 仍可读历史专题的首行标题必须统一使用 `oasis7` / `oasis7 Runtime` 品牌；旧 `oasis7*` 标题仅允许保留在正文历史上下文与证据原文中。
  - AC-13: `token-genesis-allocation-audit-checklist` 专题文档与执行模板落盘，保留 `TASK-TESTING-062` 为原始专题来源，明确创世参数审计项、阻断条件、证据字段与 verdict 口径。
  - AC-14: `required-gate` 必须在命中 `crates/oasis7_client_launcher/**`、`crates/oasis7_launcher_ui/**`、`crates/oasis7_proto/**`、`crates/oasis7_wasm_abi/**` 或 `crates/oasis7/**` 的 launcher shared runtime 改动时按需执行 launcher Web `trunk build`，避免仅在 release `build-web-dist` 才暴露 wasm 编译错误。
  - AC-14A: 仓库必须提供轻量 Web/UI automation smoke，允许 `qa_engineer` 在不启动完整 runtime 栈的前提下，用 fixture 页面复用真 `agent-browser` 验证 `viewer-software-safe-step-regression.sh` 的最小浏览器链路与 summary/state 产物契约；该 smoke 只用于 tooling 预检，不替代正式 S6 证据。
  - AC-14B: 仓库必须维护 Playwright 实跑测试系列入口，记录 `PWT-###` 用例矩阵、真实本地栈/真实 provider/真实 UI 输入的覆盖边界、mock 禁用规则、证据产物要求和新增用例规则；首个用例为 `PWT-001 Real Agent Chat`，后续玩家操作流程必须从该入口扩展。
  - AC-15: 正式 gameplay/trust evidence 至少要有 1 条代表性样本明确记录 `player_action`、`world_change_due_to_player`、`player_leverage_score` 与 `world_activity_only`，否则不得宣称“玩家已有 meaningful participation”。
  - AC-16: durable root authority defines `L1/L2/L3/L4A/L4B/L5`, combination rules, current anchors, and that automation cannot prove fun alone.
  - AC-17: durable root authority defines standard roles, review packet/card contract, triggers, sequencing and stop conditions.
  - AC-18: durable root authority defines the fixed persona catalog, packet/card, role handoff, and non-role/non-human-validation boundary.
  - AC-19: durable root authority defines L4A/L4B/L5, operator entrypoints, claim boundary and non-substitution commitment.
  - AC-20: `scripts/prepare-playability-l4-review.sh` 与 `doc/testing/templates/playability-l4-*.md` 必须能在当前 worktree 下生成完整 `L4` scaffold，保留 review packet、role/persona cards、summary、`L4B` agent 卡副本、可选内部真人佐证 notes、manifest 和推荐命令；按 [L4 scaffold 输出](#l4-scaffold-output)验收 JSON、合法路径和目录重试保护，生成文件不构成试玩完成。
  - AC-21: `scripts/run-playability-l4b-agent.sh` 消费 `manifest.json` 或 artifact 目录时，必须满足 [build/driver](#l4-build-driver)、[actual hosted session](#l4-hosted-session)、[执行结果](#l4-execution-result)、[事实与判断分离](#l4-evidence-judgment)及[入口一致性](#l4-entry-compatibility)；落盘本次 summary、已采集 state/截图/日志路径，copied card 与总 summary 只自动预填事实。各层是否完成按实际执行和评审证据判定，缺前置亦保留阻断结果。验证入口为[总手册 L4-C01..05](../../testing-manual.md#l4-toolchain-validation)。
  - AC-22: `model-visual-review-sop-2026-05-29.manual.md` 与 `model-visual-review-card-template.md` 必须定义截图输入、rubric、verdict、confidence、人类升级条件与正式 sink，使模型视觉评审能替代 routine 人工视觉 review。
  - AC-23: `scripts/ci-tests.sh full-support` 必须直接执行 workspace support crates 的 Rust 测试入口，至少覆盖 `oasis7_client_launcher`、`oasis7_launcher_ui`、`oasis7_proto`、`oasis7_wasm_abi`、`oasis7_wasm_build`、`oasis7_wasm_executor`、`oasis7_wasm_router`、`oasis7_wasm_sdk`、`oasis7_wasm_store` 与 `pixel_world_bridge`，避免这些 crate 只被间接编译或 release 阶段暴露回归。
- Non-Goals:
  - 不在本 PRD 中替代业务模块的功能设计。
  - 不承诺所有测试都进入 CI 默认路径。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: `scripts/ci-tests.sh`、agent-browser 闭环工具、Playwright 实跑脚本、`oasis7_web_launcher` GUI Agent 接口、长跑脚本、结果汇总工具、`scripts/prepare-playability-l4-review.sh`。
- Evaluation Strategy: 通过门禁通过率、缺陷逃逸率、回归定位时长、证据完整度衡量测试体系质量。

## 4. Technical Specifications
- Architecture Overview: testing 模块是仓库级验证层，负责连接代码改动、测试触发、证据产物与发布门禁。
- Integration Points:
  - `testing-manual.md`
  - `doc/testing/manual/web-ui-agent-browser-closure-manual.manual.md`
  - `doc/testing/manual/web-ui-playwright-closure-manual.manual.md`
  - `doc/testing/manual/web-ui-agent-browser-closure-manual.prd.md`
  - `doc/testing/templates/playability-l4-review-packet-template.md`
  - `doc/testing/templates/playability-l4-role-review-card-template.md`
  - `doc/testing/templates/playability-l4-persona-card-template.md`
  - `doc/testing/templates/playability-l4-summary-template.md`
  - `doc/testing/performance/performance-coverage-gap-matrix-2026-06-09.md`（当前跨 surface 性能覆盖、runtime/LLM observability 与 Viewer Web gate authority）
  - `doc/testing/governance/token-genesis-allocation-audit-checklist.prd.md`
  - `doc/playability_test_result/topics/industrial-onboarding-required-tier-cards-2026-03-15.md`
  - `doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md`
  - `doc/testing/ci/ci-builtin-wasm-docker-canonical-gate.prd.md`
  - `scripts/check-windows-paths.sh`
  - `scripts/ci-tests.sh`
  - `scripts/prepare-playability-l4-review.sh`
  - `scripts/run-playability-l4b-agent.sh`
  - `scripts/viewer-software-safe-step-regression-smoke.sh`
  - `scripts/sync-m1-builtin-wasm-artifacts.sh`
  - `scripts/ci-m1-wasm-summary.sh`
  - `scripts/ci-verify-m1-wasm-summaries.py`
  - `.github/workflows/*`
- Edge Cases & Error Handling:
  - 网络波动：外部依赖失败时记录失败签名并支持重试，不静默跳过。
  - 空产物：测试通过但缺证据产物视为不通过。
  - 活跃世界误报：若样本只证明 autonomous simulation 存在，但没有玩家导致的世界变化证据，必须显式标 `world_activity_only` 并阻止玩法放行结论升级。
  - 权限不足：CI 环境权限不足时标记阻塞并输出最小修复建议。
  - 超时：长跑套件超时需产出中间状态，防止误判为无结果。
  - 并发冲突：同一产物路径并发写入时强制分目录隔离。
  - launcher Web 漏检：若 shared runtime 改动未触发 launcher Web build，错误会延后到 release `build-web-dist` 暴露；required-gate planner 必须在相关路径命中时提升到 targeted 并安装 `trunk`。
  - 数据异常：日志格式破损时保留原始文件并标记解析失败。
  - Windows 非法路径：若 tracked path 含 Windows 非法字符、保留名或尾随空格/点，必须在 Linux required gate 先阻断，不允许等到 Windows runner checkout 才暴露。
  - release 脚本丢执行位：若 release 关键 `.sh` 的 tracked mode 漂移为 `100644` 或 worktree 不可执行，必须在 commit/required gate 先阻断，不允许等到 `release-gate-web` 或 `package-native` runner 才暴露。
  - release Web runner 依赖漂移：若 `viewer-software-safe-step-regression.sh` 未实际使用 `rg` 等额外 CLI，则不得把它们声明为硬依赖，避免 `release-gate-web` 因 runner 预装工具差异报假阴性。
  - release Web node-only config 污染：若 `viewer-primary-web-entry-regression.sh` 在 root 原本没有 canonical `config.toml` 的 runner/worktree 内启动 launcher/chain runtime，则自动生成的 `[node]` keypair `config.toml` 不得遗留并污染同一 workflow 后续 `run-game-test`；`release-gate-web-strict.sh` 的 software-safe phase 也必须显式跳过 active LLM provider preflight，只验证 stack bootstrap 后的 UI/blocker contract，避免在无本机/secret LLM config 的 release runner 上把 `missing env variable: OASIS7_LLM_MODEL` 误判成 Web 回归。
  - release Web 冷构建消耗 fixture 期限：先完成测试 Web 产物、fixture executable 和 source launcher stack 的完整构建，再启动有界 signed fixture；后续 stack 仅复用该次成功构建的同候选 binaries。构建失败不启动 fixture，不通过延长运行期限或复用未知来源 binary 放行。
  - release Web 串行回归互踩：`release-gate-web-strict.sh` 在同一 run 内串行执行 primary-entry 与 software-safe-step 时，后者不得复用前者的 launcher/web/viewer 端口；若 headed live-control 在 software-safe step 中长期停留 `queued` 且无 world delta，脚本必须用独立端口并允许 headless + `play` fallback 收口，避免把残留 listener 或 headed 控制停滞误判成 release 回归。
  - release Web 入口断言：`viewer-primary-web-entry-regression.sh` 按当前 Player shell 合同先核验未登录 Hosted 入口的登录门，再经隔离 loopback 测试环境显式启用的可见测试登录按钮取得后端签发的 QA session，核验可见决策区、单一可达的 Next Move 主动作、Objective / Player Leverage 上下文与 Action Receipt；不得以遮挡后的 DOM 文案代签可用流程，也不依赖已移除的介绍标题。该 QA 路径不证明正式账号登录。`agent-browser get text body` 的文案校验须忽略 CSS `text-transform` 带来的大小写差异；结构缺失、内容为空或主动作不可达仍应失败，不能仅凭连接成功放行。
  - release soak chaos 恢复采样：`p2p-longrun-soak.sh` 在 restart/pause chaos 后，必须先等目标节点恢复到 `healthz/status/balances` 可读、`running=true` 且 `last_error/load_error` 清空，再把后续样本记入 gate；恢复窗口要计入 `chaos_exempt_secs`，避免把预期瞬态误记成 `last_error_samples` / `http_failure_samples`。
  - 迁移断链：文档改名后若引用未同步，需在同批次修复并复测。
  - 创世语义误读：若把 `protocol:*` custody account 误当成已初始化 treasury bucket，QA 必须直接阻断。
  - 流通口径漂移：若创世参数表未显式声明 `genesis_liquid=0` 或首年外部释放上限，视为配置不完整。
  - 自动化、agent 试玩与真人试玩结论冲突：必须先记为“低层 pass / 高层 hold”，不能把高层体验问题降格成脚本未覆盖。
  - 模拟 persona 全部正面：只能说明内部假设面板没有发现高价值断点，不能替代 `L4B` agent 实操或外部真实 session。
  - `L4A` 全正面、`L4B` 未执行：只能写 `synthetic_ready`，不能写 `agent_ready`。
  - `L4B` 全正面、`L5` 未执行：只能写 `agent_ready`，不能写 `external_ready`。
- Non-Functional Requirements:
  - NFR-TST-1: required 套件变更前后执行时间波动 <= 20%。
  - NFR-TST-2: 发布证据包字段完整率 100%。
  - NFR-TST-3: 关键链路缺陷逃逸率持续下降（按月跟踪）。
  - NFR-TST-4: 测试手册与脚本口径冲突数为 0。
  - NFR-TST-5: 测试执行结果可在 30 分钟内完成追溯定位。
  - NFR-TST-6: 文档迁移批次在不降低治理质量的前提下保持可审阅粒度（按独立目的和可审阅范围组织，不强制每篇文档单独建任务）。
  - NFR-TST-7: builtin wasm hash 校验在多 runner 下可复现且差异可定位到模块与平台维度。
  - NFR-TST-8: Token 创世 QA 审计模板字段完整率必须为 `100%`，缺任何一项关键字段都不能给 `pass`。
  - NFR-TST-9: 正式 gameplay evidence 审查者必须能在 30 秒内看出“玩家是否真的改变了世界”，无需从长日志里二次拼装。
  - NFR-TST-10: 正式玩法结论必须能在 60 秒内回答“当前只证明到哪一层，还缺哪一层”。
  - NFR-TST-11: review orchestrator 必须能在 5 分钟内决定本次改动应开哪些标准角色 subagent。
  - NFR-TST-12: simulated persona panel 的使用者必须能在 5 分钟内决定该开哪几个 persona，以及它们的结论最终归谁收口。
  - NFR-TST-13: 任何正式玩法结论都必须能在 30 秒内回答“这是 `L4A`、`L4B` 还是 `L5` 前的内部校准，以及为什么”。
- Security & Privacy: 测试日志与产物需避免泄露凭据；外部 API 测试使用最小化数据并执行脱敏。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP (2026-03-03): 固化 testing 分层模型与证据标准。
  - v1.1: 补齐高风险路径的触发矩阵自动检查。
  - v2.0: 建立跨版本测试质量趋势分析与发布建议。
- Technical Risks:
  - 风险-1: 套件增长导致执行成本上升。
  - 风险-2: 手册与脚本不一致导致执行偏差。
  - 风险-3: hash 校验策略分散会导致 m4/m5 漂移长期难以收敛。
  - 风险-4: release 工作流若让 Web 构建缓存与 bundle 编译链路碎片化，会把耗时重新堆回关键路径。
  - 风险-5: runtime gate 若继续串行堆叠 full-tier 与 sync 检查，会长期锁死发布关键路径。
  - 风险-6: runtime shard 若长期失衡，即使已并行化，也会因为最长 shard 过重而回吐大部分收益。

<a name="6-validation-decision-record"></a>

## 6. Validation & Decision Record

下表 `TASK-TESTING-*`、旧里程碑与迁移编号保留历史来源和专业定位，不代表这些任务仍开放，也不要求恢复旧 Project、另建任务身份或通用回执。当前改动仍须追溯实际适用条款并完成相应验证；历史 ID 与旧绿色结果不能代替本轮 requirement Trace 或执行证据。当前任务组织遵循[开发流程规范](../engineering/workflow/source-of-truth.md)。
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-TESTING-001 | TASK-TESTING-001/002/005/006 | `test_tier_required` | S0~S10 触发矩阵核验、手册一致性检查 | 分层测试入口与执行标准 |
| PRD-TESTING-002 | TASK-TESTING-002/003/006/053/054/055/056/release-windows-invalid-path-blocker/release-script-executable-mode-gate/release-web-entry-text-casefold/rust-required-gate-ondemand-scope/required-gate-ondemand-launcher-web-build | `test_tier_required` + `test_tier_full` | 证据模板抽样、发布前必填字段检查、release workflow 复用链路核验、runtime gate shard 聚合验证、required-gate changed-path planner 回归、launcher Web build 命中/未命中验证，以及 Windows checkout 兼容路径扫描、release 关键脚本执行位扫描与 Web 入口文案大小写漂移回归 | 发布链路可信性与可复现性 |
| PRD-TESTING-003 | TASK-TESTING-003/004/006/053/054/055/056/release-windows-invalid-path-blocker/release-script-executable-mode-gate/release-web-entry-text-casefold/rust-required-gate-ondemand-scope/required-gate-ondemand-launcher-web-build/playability-player-leverage-evidence-rubric/shared-player-gameplay-contract-parity | `test_tier_full` | 趋势指标回顾、缺陷逃逸复盘、release 关键路径对比，以及 required-gate scope 剪裁后的长期时延观察、launcher Web build 逃逸缺陷回归、Windows checkout 失败签名回归、release 脚本执行位失败签名回归、Web 入口文案大小写漂移回归，以及 gameplay evidence 的 `player leverage` / `world_activity_only` 抽样审查与 Web/`pure_api` 共享 `snapshot.player_gameplay` contract 的 QA 复核 | 长期质量治理与发布风险控制 |
| PRD-TESTING-004 | TASK-TESTING-007/008/009/010/011/012/013/014/015/016/017/018/019/020/021/022/023/024/025/026/027/028/029/030/031/032/033/034/035/036/059/060/061 | `test_tier_required` | 原文约束点映射审查、命名与引用回归检查、历史专题标题零残留校验、活跃专题当前真值命名回归检查 | 专题文档可维护性与追溯一致性 |
| PRD-TESTING-005 | TASK-TESTING-037/038/039/040/wasm-determinism-gate-ondemand-scope | `test_tier_required` | keyed manifest/strict policy/changed-path scope planner/多 runner required checks/identity 输入收敛回归 | builtin wasm 发布链路稳定性 |
| PRD-TESTING-006 | TASK-TESTING-062 | `test_tier_required` | token 创世参数表审计清单、执行模板、p2p/testing 模块追踪回写 | 主链 Token 创世冻结与经济配置门禁 |
| PRD-TESTING-007 | durable playability evidence governance | `test_tier_required` | L1-L5 boundary, current anchors, L4 scaffold and combination-rule sampling | 玩法质量 claim 与放行边界 |
| PRD-TESTING-008 | durable role-based playability review | `test_tier_required` | role/card contract, scaffold, triggers and stop-condition sampling | 多角色内部评审系统设计 |
| PRD-TESTING-009 | durable simulated-player persona panel | `test_tier_required` | catalog, card schema, handoff and L4/L5-boundary sampling | 多风格内部玩家视角治理 |
| PRD-TESTING-010 | durable L4 synthetic/agent split | `test_tier_required` | L4A/L4B/L5 boundary, manual entry and L4B runner sampling | synthetic/agent/real-human 玩法证据治理 |
| PRD-TESTING-011 | model-visual-review-sop | `test_tier_required` | 模型视觉评审 SOP、输出模板、testing 根入口和 S6 手册互链、doc governance 检查 | routine 人工视觉 review 替代路径 |
| PRD-TESTING-012 | QA infrastructure receiver | `test_tier_required` + `test_tier_full` | 逐条核对 source→scene→environment→oracle→artifact→tier 映射、17/10/8 coverage identity、89 个原始对象各自的消费/保留/缺口记录及当前候选实际证据；对 equal-version MIG-01 检查 shape 与 semantic verdict 分层 | 可审计的组合验证计划、证据留存与不得升级的明确边界 |

### 已退役的系统测试手册工程化标识

`PRD-TESTING-MANUAL-001..003` 与 `TMAN-1..5` 是 2026-02-26 手册工程化收口的 completed / historical 标识，不再作为活跃专题：

| 历史标识 | 当前承接 |
| --- | --- |
| `PRD-TESTING-MANUAL-001` / `TMAN-1` | `testing-manual.md` 是唯一 canonical operator manual。 |
| `PRD-TESTING-MANUAL-002` / `TMAN-2` | `testing-manual.md` 的 L0–L5、S0–S10 与 required/full 章节拥有当前测试分层、套件和触发矩阵。 |
| `PRD-TESTING-MANUAL-003` / `TMAN-3/4` | `testing-manual.md` 的证据规则、`doc/testing/templates/release-evidence-bundle-template.md`、`doc/testing/evidence/README.md` 以及 Web UI 手册拥有当前证据、fail-fast、GPU/headed 与维护口径。 |
| `TMAN-5` | 活跃的 `PRD-TESTING-004` 与其任务映射拥有逐篇语义迁移、命名/引用回归和历史专题追溯治理。 |

历史 `DEC-TMAN-001..004` 已分别由当前的主手册/分册结构、required/full 策略、发布证据硬门禁与逐篇语义迁移治理吸收；原任务实现过程从 Git history 与 PR 与实际验证记录 追溯。若当前手册、脚本或 CI 漂移，应修改现行权威和新任务证据，不恢复已退役专题。

- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-TST-001 | 采用 required/full 分层验证 | 全量套件每次必跑 | 保持效率与覆盖平衡。 |
| DEC-TST-002 | 证据包作为发布必备输入 | 只记录口头结论 | 审计与追溯能力不足风险更高。 |
| DEC-TST-003 | 以手册驱动触发矩阵统一口径 | 各模块自行定义测试口径 | 可减少跨模块冲突和遗漏。 |
| DEC-TST-004 | legacy 专题文档采用逐篇人工迁移并统一 `.prd` 命名 | 自动脚本批量改写 | 可确保内容语义与约束不丢失。 |
| DEC-TST-005 | Token 创世前增加 QA 审计清单与阻断 verdict | 仅由 producer/runtime 自审 | 经济配置错误一旦进入创世，后续修复成本极高。 |
| DEC-TST-006 | 在正式 gameplay evidence 中单列 `player leverage` 审查层 | 继续只看 world delta / activity / 总体有趣度 | `#166` 暴露的是“玩家是否参与有效”而不是“世界有没有动起来”，需要单独防误报。 |
| DEC-TST-007 | 为“是否好玩”建立分层 evidence stack，并明确自动化不能单独保证好玩 | 继续把自动化 pass、世界活跃和真实玩家继续动机混写 | 这些信号的证明强度不同，混写会持续污染 release/stage 结论。 |
| DEC-TST-008 | 进一步把多角色内部评审设计成标准角色 subagent 系统 | 继续临时决定这次要不要找哪些角色来看 | 临时协调很难规模化，也无法稳定复用 review 输出。 |
| DEC-TST-009 | 用 simulated player persona panel 补多风格玩家视角，但不新增正式 `player` 角色 | 直接把 `player` 升格成新的标准角色 | 会破坏仓库角色治理，并混淆内部模拟与外部真人验证。 |
| DEC-TST-010 | 把 `L4` 正式收口为 `L4A synthetic` 与 `L4B agent`，并把内部真人试玩降为 `L4B` 可选校准、把真实人类验证留在 `L5` | 继续把 agent 角色扮演、agent 实操、内部真人试玩与真实人类继续游玩意愿共用一个 `L4` 标签 | 这些信号的证明强度不同，混写会持续污染 stage/release 结论。 |
| DEC-TST-011 | routine 视觉 review 默认采用截图加模型视觉评审，人工只处理升级项 | 所有视觉截图继续默认人工扫图 | 人工 review 容易不可追溯、成本高；固定截图输入和 rubric 后，模型足以覆盖大部分布局/遮挡/可读性判断。 |

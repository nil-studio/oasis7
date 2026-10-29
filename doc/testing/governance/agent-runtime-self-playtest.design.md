# 外部 Runtime 自主试玩与开发自测闭环

- 状态：target execution contract；本次交付系统设计、操作手册和可复用测试模板，不宣称自动 runner 或真实试玩已完成。
- Owner role：qa_engineer；协作：agent_engineer、runtime_engineer、viewer_engineer；日期：2026-10-10。
- 固定审读基线：`eng-cc/oasis7@e524debf4584e5e6f2ae5baf9d877e78c9d7bf7d`；产品 #4493、系统设计 #4521 已合入。
- 上游：[testing PRD](../prd.md#1-executive-summary)、[外部 Runtime 产品](../../product/agents-world-simulation/external-agent-runtime-play.prd.md)、[协作产品](../../product/agents-world-simulation/player-runtime-collaboration.prd.md)。
- 入口：[testing/governance](README.md)；唯一操作入口：[自测手册](../manual/agent-runtime-self-playtest.manual.md)；配套格式：[试玩任务模板](../templates/agent-runtime-playtester-task.template.md)、[报告模板](../templates/agent-runtime-playtest-report.template.md)。

## 1. 问题、目标与非目标

开发 Codex 不能只读源码和跑固定请求就宣称“Agent 可以玩”。目标是：游戏 Skill/API 及受支持 Runtime 可用后，开发主 Agent 派出独立试玩 subagent。试玩者读取同一份玩家 Skill，启动或连接隔离候选，实际自主游玩并记录失败与体验反馈；主 Agent 再复现、修复、增加普通回归，并针对新候选复测。

本设计承接既有 L4B embodied-agent 自测，不新增证据等级、正式 player 角色、通用任务平台或所有 PR 必跑的模型门禁。脚本探针、L4A 静态/模拟评审、真实自主试玩、真实 Web/native 操作和 L5 外部玩家证据不能互相代签。试玩通过不证明游戏有趣、安全完备或可发行。仓库合入规则仍由[开发流程](../../engineering/workflow/source-of-truth.md)拥有。

文档按职责维护：[开发流程规范 §3.1](../../engineering/workflow/source-of-truth.md#agent-gameplay-selftest)拥有 Agent 游戏功能自测的触发、完成与阻断边界；本设计拥有自测技术合同；操作手册集中维护预检、启动、派工、验真、修复复测和清理步骤；`doc/testing/templates/` 只维护可复用派工与反馈格式。主 Agent 读取手册组织测试，试玩 subagent 读取正式游戏 Skill 和最小任务包，不接收整份开发手册或内部解法。游戏 Skill 是被测玩家能力入口，不再另设开发自测 Skill。

## 2. 上游约束与相关角色

默认一个开发负责人、一个实际试玩 subagent；世界结果验真优先用确定性程序或可信只读查询，不必再开模型。仅当需要验证人类玩家指导或图形体验时，增加一个独立 owner/observer 测试任务。并行单人场景使用不同世界或 Agent；协作场景才显式共享同一任务且分配不同权限。

### 2.1 需求承接与分配表

| 上游条款 | 具体 obligation 与适用条件 | 本设计条款 | 外部 owner / dependency | 排除与未覆盖范围 |
| --- | --- | --- | --- | --- |
| [testing SC-12](../prd.md#testing-sc-12) | 准备离线 L4 packet、卡片、summary 与推荐命令；真实派工/操作/结果另归 SC-13 | [编排](#selftest-orchestration) | qa_engineer；现有 L4 scaffold | 产品 N/A；reason=只生成离线评审材料并验证 JSON/path/已有目录保护，不执行玩家入口或玩法；scope=SC-12 的离线 scaffold 输出；不改变 required-gate，不将生成成功当作 L4A/L4B 已执行 |
| [testing SC-13](../prd.md#testing-sc-13) | 将已有真实 L4B 构建、hosted 会话、操作、RunResult 与独立体验判断落实为开发主 Agent 派工、真实试玩和回收结果 | [编排](#selftest-orchestration) | qa_engineer；现有 L4 runner 和本轮候选/真实子会话 | 不改变 required-gate，不把脚本探针改标成自主试玩；真实进入、执行与结果不能套用 SC-12 的离线 N/A |
| [AC-EXT-011](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-011) | 试玩者读取真实玩家 Skill，在缺少源码答案的情况下接入 | [Skill 接入](#selftest-skill-entry) | agent_engineer；已实现的 Skill/API/driver | 缺依赖记 blocked，不用旧 Bridge 替代新路径 |
| [AC-EXT-010](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-010) | 两个真实 Runtime 各自保留同一任务的首产物、真实阻塞及继续、目标应用、回访、断连和进程重启续接 | [真实游玩](#selftest-gameplay) | gameplay/runtime/agent owner；首局与恢复合同 | 局部首产物不满足整条 AC；不证明稳定生产、另一 profile 或入口 |
| [AC-COLLAB-007](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#ac-collab-007) | 分别验证目标更新、问答、明确授权、离线和执行器恢复 | [协作](#selftest-collaboration) | agent/runtime owner；隔离 owner 与 executor | 测试 owner 预授权不代替真实用户审批体验 |
| [AC-EXT-015](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-015) | Web 与原生客户端各自实际交互并关联游戏结果 | [视觉表面](#selftest-surfaces) | viewer_engineer；实际 GUI/浏览器工具和候选资产 | API pass、截图存在不代签完整 UI/native |
| [AC-EXT-009](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-009) | 全轮共用有限预算，隔离数据并说明未知费用 | [隔离与预算](#selftest-isolation-budget) | qa/agent owner；受控私有测试世界和模型许可 | 不自动充值，不保证无法计量的下游硬上限 |
| [testing SC-1](../prd.md#testing-sc-1) | 为有证据的失败选择适用层级，转成普通回归、修复和新候选独立复测 | [反馈闭环](#selftest-feedback-loop) | 开发负责人；现有 S0~S10 与本次实际影响范围 | 只承接自主试玩反馈子链，不缩减全部关键改动的测试层级义务，不以一次模型成功覆盖此前失败或独立评审 |
| [testing SC-2](../prd.md#testing-sc-2) | 在既有 required/full 与手册边界中运行适用回归，保留旧失败与新候选复测的对应关系 | [反馈闭环](#selftest-feedback-loop) | 开发负责人；现有测试入口、稳定 check context 和 PR | 不改变门禁剪裁、launcher Web 构建或 full-support 要求，不以普通 CI 代签自主试玩或全部回归 |
| [testing SC-15：source-to-scene](../prd.md#testing-sc-15) | 每项所选条款绑定固定来源、被测树、适用性/执行状态、tier、可回读完整证据和保留缺口 | [来源到场景](#selftest-source-scene) | qa_engineer；既有 QA receiver 与领域 oracle | 只承接本轮选择范围；不宣称覆盖完整基础设施场景库存或以格式正确代签执行 |

## 3. 当前状态、目标状态与差距

| 对象 | 基线可核对事实 | 目标与限制 |
| --- | --- | --- |
| L4 工具 | `scripts/prepare-playability-l4-review.sh`、`scripts/run-playability-l4b-agent.sh` 已存在；后者默认指定 `build_factory_smelter_mk1` | 复用启动、artifact 和卡片；固定动作不能证明新 Skill/外部 Runtime 自主策略 |
| Game API 与 worker | #4521 是 target 系统设计，已有 World Service 不等于所有 `/v1/game` 端点和 worker 就绪 | 每轮预检实际能力、发行资产和底层权威；不得从设计文档生成假的健康或完成结果 |
| 玩家 Skill | 基线读取 `skills/oasis7-play/SKILL.md` 未找到；旧 `site/skills/oasis7.md` 是另一条 Bridge 路径 | 指定真实交付的玩家 Skill 及 hash；未交付先记 blocked，不能临时写“测试专用玩家 Skill”自证成功 |
| 编排能力 | Codex 是否能创建 subagent、隔离环境和操作 GUI 取决于实际客户端和配置 | 记录 runtime/profile/工具能力；没有 subagent 时只能明确降级为人工/主 Agent 自测，不称独立 subagent 通过 |

当前 runner 的继续意愿/杠杆评分仍是保留的实现缺口：`scripts/run-playability-l4b-agent.sh` 从 step/progress/ACK/player match 推导 `agent_continue_observed`、杠杆 5 分及 `world_activity_only=no`，尚不满足 [L4 自动事实与体验判断](../prd.md#l4-evidence-judgment)要求的独立逐项评审。关系规范化不修复或验收该行为；固定路线、自动评分和文档检查不能变成自主试玩/玩家价值证据。

## 4. 边界与结构

<a id="selftest-orchestration"></a>
开发主 Agent → 候选构建与隔离环境 → 最小试玩任务包 → 独立试玩 subagent → 游戏 Skill / Game API → 权威世界。

试玩报告与世界证据 → 只读结果验真 → 主 Agent 分诊与修复 → 普通回归 → 新候选、独立复测。箭头表示派工/调用或证据返回，不授予世界写权。

| 执行方 | 允许行为 | 禁止行为 |
| --- | --- | --- |
| 开发主 Agent / 显式 setup 子任务 | 读源码、构建固定候选、准备私有 fixture、收集服务日志、修复与复测 | 测试进行中修改候选却沿用旧版本结论；给黑盒试玩者传内部答案 |
| 试玩 subagent（qa 任务模式） | 读玩家 Skill/公开帮助，通过普通玩家协议规划、查询、行动、询问和报告 | 写仓库源码、读隐藏 fixture/数据库、直接造资源/跳 tick、批准自己、递归派出更多 Agent |
| owner/observer（按需） | 用独立测试账号在 Web/native 设置高层目标、检查世界、按有限授权决定 | 与 executor 共用 owner 凭据；无条件替每项行动批准 |
| 结果核验器 | 受权回读原 operation、世界绑定、生产/授权回执及前后状态 | 用模型自报、进程退出码或截图像素替代权威结果 |

独立上下文不自动等于安全隔离：实际 runner 应只给试玩子进程挂载玩家文档、受限工具和本轮输出目录，屏蔽源码、Git diff、服务日志、fixture seed 的答案、owner/model secrets。无法隔离时记录 `isolation=instruction_only` 或 `source_visible`，不得声称 source-blind 或安全隔离。父对话、隐藏判据和修复结论不应整段继承；给可理解的目标，不给固定动作解法。

## 5. 关键运行流程

<a id="selftest-skill-entry"></a>
### 5.1 选择、启动与派工

命中[规范触发范围](../../engineering/workflow/source-of-truth.md#agent-gameplay-selftest)后必须按手册组织自测：先普通 CI，再选择覆盖实际影响的自主试玩/协作/可视化场景。仅设计、排版或已核实无相关行为影响的重构说明不适用；正式游戏 Skill 的使用指令变化不能按文档豁免。长跑与压力按实际需要及既有授权执行，不把所有场景或平台变成每次必跑。

主 Agent 先执行适用的便宜回归，再固定 source HEAD、未提交补丁 hash（若有）和实际 build/artifact hash。选择 local/private disposable 世界，校验 world_id、branch、可信服务身份和初始状态；“localhost”本身不能证明它未转发生产。只启动已检查的仓库入口或固定 release 的真实命令，分配独占端口/存储/进程，不复用用户日常世界。L4 runner 的现有参数不作不存在的新 autonomous 选项使用。

两种启动测试须分开：`cold_entry` 让试玩者仅按已交付玩家 Skill，经预批准的受限启动/连接工具自行进入；`prepared_world` 由 setup 先准备世界再交给试玩者。后者可以证明游戏行为，不能代签自助启动。如果玩家发行入口不能安全选择测试世界，cold_entry 保持 blocked，不能给 subagent 任意 shell 权限解决。

派工前固定场景、目标、预算、角色/权限、证据路径和停止条件；使用实际 Codex 提供的 subagent 能力创建任务，记录真实子会话引用。不能把脚本、另一段主 Agent 文本或没有返回记录的“已派工”当作独立子任务。采用[试玩任务模板](../templates/agent-runtime-playtester-task.template.md)，显式让子任务先读取玩家 Skill。

<a id="selftest-gameplay"></a>
### 5.2 自主游玩与可信判定

默认 `attached_codex_subagent`：试玩子会话本身作为候选明确支持的 attached Runtime，读取 Skill、取得有效 executor 绑定、经 Game API 自主规划并行动；不再套一层额外 Codex worker 以人为增加费用。该 profile 必须先通过 #4521 的 attached 合同，不能把 Codex 子会话能力推定为 supervised app-server profile 已通过。

`supervised_runtime` 场景则由明确的 worker 驱动 OpenClaw 或 Codex；试玩/observer 子任务通过对应玩家入口交互，不接管其 executor。同一 Agent 仅一个当前决策执行方；验证 OpenClaw 需实际启动被授权 OpenClaw，而不是由 Codex 模仿它。两条路径的实际模型费用分别进入同一总账。

给试玩者高层目标“取得当前首局规定的首产物并说明下一步”，而非内部配方答案或动作序列。只允许正常工具调用；被卡住时可读公开帮助、在既有授权内调整策略、提问。主 Agent 不实时教其过关；任何人工提示、debug 注入或后台帮助记为 `assisted`，不覆盖原无帮助尝试。

结果核验器复用首局 completion evaluator 与原 operation/receipt 关系，证明候选版本、角色、任务、资源和因果一致。模型宣布完成、世界自然推进、一次 build/schedule accepted 或 inventory 巧合变化均不够。原操作 `recovery_required` 时保留关联、查询原结果并冻结关联新副作用；不将未知写成 failed 再重做。

**AC-EXT-010 是组合义务，不等于 ST-03 首产物单项通过。** 完整声明必须按[首局生命周期清单](../manual/agent-runtime-self-playtest.manual.md#first-session-lifecycle)联合 ST-02 的接入、ST-03 的生产/真实阻塞及继续、ST-04 的目标应用/回访/断连/进程重启证据。OpenClaw、Codex 各自固定实际 profile、候选和一个连续的 world/Agent/task 历史；不同 Runtime 使用同一玩法版本及可比较初始条件，不能互相代签或拼接不同世界/任务的阶段。重启可更换私有 session 或合法 executor epoch，但必须延续原世界任务且不重复已结算效果。

只有局部改动时可选择 `scope=targeted`，但仅报告所选 obligation 的结果，未覆盖阶段及 Runtime 保留为整条 AC 的 gap，不能将整条 AC 标为通过。选择 `scope=ac_ext_010_full` 时两 Runtime 和所有生命周期项均为必需；预算/依赖不足保持 blocked/inconclusive，不在测试后缩小分母。图形探索的 ST-05 独立按实际影响触发；纯 API 的观察断开/重新读取不能证明 Web/native 交互。

<a id="selftest-collaboration"></a>
### 5.3 协作、故障与恢复

复用已有验证手册的 `collab-adoption/questions/approval/events/executor` 场景。测试 owner 的高层目标、拒绝/批准和撤销按独立脚本或 observer 明确执行，事先固定条件、额度与有效期；不会因为 tester 说“需要更多钱”就扩大许可。

覆盖：executor 离线发布目标；两客户端旧草稿竞态；读取/采纳/上下文版本；一次澄清与一次明确授权决定；关闭观战、重启执行器、事件游标缺口、未知世界提交与冷启动完整目标正文。故障注入只由 setup 在显式隔离 fixture 中实施，并记录时间/操作；不能让试玩者 kill 任意进程或改世界文件。恢复优先快照和权威回执，不重演旧目标、旧批准或已结算行动。

<a id="selftest-surfaces"></a>
### 5.4 Web 与桌面可视化

API 自主试玩与视觉自测是两个维度。Web 按现有 S6/agent-browser 手册，实际定位 Agent、平移/缩放、选择工厂、读任务/阻塞/回执并作高层指导；渲染截图与交互轨迹关联同一世界事件。原生客户端必须在实际安装包窗口中操作，打开浏览器、截图文件存在或无 GUI 环境不能代签 native。

观察者不调用 executor-only 的 action API 来冒充人类地图操作。Browser/GUI 不可用时该维度 `blocked`，不影响已经有证据的 API 结果，但整体覆盖必须标出缺项。模型对易懂、卡顿、困惑的评价是带步骤/截图的体验观察，不是 L5 或“好玩已证明”。

<a id="selftest-feedback-loop"></a>
### 5.5 反馈、修复和有界复测

子任务返回报告，主 Agent 先验真，再分类：confirmed_bug、suspected_bug、agent_strategy_failure、expected_rejection、environment_blocker、harness_fault、usability_observation。缺资源的合法拒绝或策略失败不是自动代码 bug；源码问题的猜测须在诊断时核实。

确认缺陷 → 提取最小步骤及确定性协议/状态回放 → 在普通单元/合同/集成测试中先证明能检出原问题 → 修复 → 重跑该回归 → 构建新候选 → 用新试玩子会话复测受影响路径。反馈同时分流到代码、正式玩家 Skill、API 可发现性、UI 或测试环境的实际主责；修正玩家 Skill 必须改同一发行源、更新版本/hash 并重新做无提示接入，不在测试提示词里偷偷添加内部解法。复现不了保留样本、条件与不确定性，不为了绿灯改目标、换 fixture、抬预算或删除失败。

默认最多两轮修复后试玩，具体限额在运行前明确且只能更严格于用户总授权；同类阻断反复出现、预算/时间耗尽、权限或操作结果未知时收口反馈。不得把“继续优化”解释成无限付费循环。子任务不自行 push、建 Issue、合入或部署；主 Agent 按原任务授权汇总到 PR，Issue 按跟踪需要使用。

## 6. 接口与数据合同

所有记录复用现有 L4 artifact 目录；以下是本轮测试数据，不是新的任务准入 schema、服务数据库或必交放行回执。字段完整时可自然语言或 JSON 保存，普通开发不必先创建新任务标识。

`RunInput`：run_id、[CandidateIdentity 与所选 SourceSceneRecord](#selftest-source-scene)、build_digest、游戏/Runtime/driver/Skill 版本和 hash、实际 profile、目标世界/信任/初始状态、`scope=targeted/ac_ext_010_full`、选中条款及必需阶段、启动模式、访问模式/执行 lane/表面、隔离等级、总预算、有效授权引用、deadline、预声明停止条件和 artifact root。

`TesterInput` 是 RunInput 的最小投影：目标、普通玩家 Skill、公开帮助、受限连接/启动工具句柄、本轮角色、允许操作、预算和输出位置。隐藏 oracle、源码/diff、内部答案、原始 token/私钥与生产 endpoint 不交给试玩者。

`RunResult`：候选与子会话引用、实际读到的 Skill、ready 证据、逐条 SourceSceneRecord 的 execution_state/test_tier/evidence_level/tier_verdict、生命周期项结果、完整 ArtifactRef 及 oracle 检查、保留 gap、问题列表、已知/估计/未知消耗、停止原因、清理结果和复测关联。详见[报告字段](../templates/agent-runtime-playtest-report.template.md)。

每项 verdict 为 `pass / fail / blocked / inconclusive / not_applicable`；缺授权、缺依赖不写 pass，未执行不写 fail。pass 要有适用 oracle；fail 要有“已执行且违反约定”的观察。若所有选中项均已验真通过且无缺失证据，才汇总 scope pass；不同 Runtime/surface 的未知保留，不能被平均分抹去。

证据引用指向本轮保存的脱敏 action/operation/receipt、受权快照、截图/交互或服务记录，带采集方和时间；验真器重新校验来源与权限。任意 URL、模型粘贴的哈希或自制成功 JSON 不算证据。相同 run/case/event 标识异内容是冲突；单个 artifact 路径不能越出根目录或沿 symlink 逃逸。

<a id="selftest-source-scene"></a>
### 6.1 来源、场景、执行和证据的同一记录

复用 [testing SC-15 的 QA receiver](../prd.md#qa-infrastructure-source-scene-method)，每个选中条款 × 场景/子项/阶段样本 × Runtime/profile/入口 × test tier 保留一条记录；共享来源和候选可按引用复用，不强迫另建报告文件或服务。主 Agent/核验器维护下列字段，给试玩者的最小输入不含隐藏 oracle 或内部源码答案。

| 字段组 | 必须保留的值和判定 |
| --- | --- |
| SourceClauseRef | repository、path、fragment、clause_id、不可变 source_oid、blob_oid、content_sha256；digest 对应该 OID 的原文件字节，fragment/条款标识定位具体原文，不能用 main/latest 或当前工作区内容替代已采纳来源 |
| CandidateIdentity | source_head、base_oid、execution_target（source_head/merge_tree/dirty_worktree）、实际 tested_tree_oid、event_commit_oid（适用时）、dirty_patch_digest（有则必记）、build_digest；记录真正构建/执行的源树，不能用 source commit OID 冒充 tree OID，也不能声称测过未执行的合并组合 |
| 场景与适用性 | scenario_id、check_kind（contract/live）、obligation_id、cell/specimen_id、phase、applicability、applicability_owner 与 basis；不适用需领域 owner 的依据，缺依赖是 blocked 而非不适用；未选范围仍保留 gap |
| 执行与环境 | execution_state=planned/located/implemented/executed；world/branch/root/revision/window 与配置、基线和有序阶段观察、依赖/拓扑、Runtime/driver/provider kind/model/脱敏 endpoint route 与预检来源；access_mode、execution_lane、surface 分开 |
| 层级与结果 | test_tier_required 或 test_tier_full、独立的 evidence_level（沿用 L1/L2/L3/L4A/L4B）、tier_verdict、failure_signature/blocking_reason、retained_gap、gap_owner、next_step；缺失执行证据不能标 passed，本方法不产生 L5 |
| ArtifactRef 与 oracle | 原始采集证据的受控 locator、实际 byte_length、digest_algorithm/digest、collector/collected_at、integrity_check；命令/结果或实际用户操作；结构、canonical、领域、信任、身份、关联 join 各项断言及结果，不适用的 oracle 说明依据 |

执行顺序是固定来源/候选 → 枚举义务和样本 → 保存未变基线与真实依赖 → 执行 → 回读 artifact 字节校验 → 分项 oracle → 对比基线、阻塞/中断、fresh-read、恢复、retry/replay → 逐 tier 判定并保留缺口。恢复会改变 root/revision，但必须有同一世界历史的顺序和 lineage，不能要求各阶段 root 相等或从另一任务补证据。正文、截图、回执导出等分别校验；原始记录含秘密时仅保存在获准受限位置或停止采集，对外脱敏副本单独标识和校验，不能假称与原始字节相同。文件 digest 仅证明字节完整，不证明世界最终性。

execution_state 与 verdict 正交：fixture 存在只是 located/implemented，命令确实运行才是 executed；只完成结构检查，对被测产品仅能 shape_verified。操作报告 pass/fail 对应 QA passed/failed，blocked 保持 blocked，inconclusive 对应 unverified；not_applicable 要保留 owner/basis 且不计成功。不满足当前 tier 的环境、来源一致性或证据完整性，整条所选 obligation 保留 blocked/unverified。确定性记录检查通过，只证明记录检查自身，不把样例升级为真实试玩证据。

## 7. 状态、事务与持久化

运行阶段 `preparing → ready → playing → verifying → reported → cleanup`；任何阶段允许以 blocked/aborted 收口。阶段不是世界执行状态，也不授予权限。写报告采用临时文件和原子替换；每次尝试单独保留，不覆盖失败文件。source/build/Skill 任一变化必须新建 attempt 关联原失败，不能把旧 evidence 改标签成新候选。

主 Agent 记录自有世界/进程/浏览器/子会话清单。取消任务先停止新调用并撤销测试 executor，查询原 pending 结果，再关闭自有 worker/subagent/browser；需要强停时只在原授权内处理自有进程组。关闭子会话不保证其已启动的 worker 退出，须实际回读；清理失败保留诊断与资源引用，不伪装完成。

恢复测试使用同一测试世界；修复后的可比较首局使用另一个隔离实例和相同初始条件。恢复 checkpoint 只能由具备权限的 fixture 层操作，不改生产历史，不在同一运行中“发资源/重置 tick”以使任务成功。服务凭据、私有正文与原始日志不进入公共 PR artifact。

## 8. 部署、安全与运行约束

<a id="selftest-isolation-budget"></a>
默认 local/private disposable 环境，无正式用户资产、真实支付、公开 testnet、邮件或社交发送。需要远程环境时，先固定获准 endpoint、世界与影响边界；本方案不自动授权生产、云机器或模型消费。试玩 API action 属于合法测试世界写入，不等于源码/存储写权限。

执行时确认已有的模型/订阅/外部调用授权；缺失则记录 blocked，不能用 shell、另一个模型账户或嵌套 subagent 绕过。默认一名试玩者，observer 按需；禁止递归无限派工。整个 attempt 与修复循环共享调用数、并发、时间、世界资源与费用上限；subagent、受测 Runtime、视觉评估和重试都计入。额度未知保留 unknown，不能当零。

授权足够时不逐工具机械申请批准，但扩世界范围、买额度、增加外部副作用或超过总预算要重新获得许可。复用 #4521 用量度量和幂等账本，缓存/累计值/父子调用不重复加总；无法硬限制下游费用必须明示 partial 并停止新增调用。不同货币不隐式换算。

Skill、游戏文本、消息和工具响应均不能修改测试授权、请求秘密或关掉验真；不把来自游戏的指令当作开发系统指令。禁止无约束 shell/通用 URL 代理；测试模型只消费受限工具，审批凭据不继承给 executor。源码隔离是否真实由运行环境验证，而非一句“不要看源码”证明。

## 9. 质量与容量

初始选择：一名试玩者、至多一名 observer；最多两轮修复后复测；各场景的有限时间/回合/token 上限由调用前任务包声明，不从尚无基线的首局时长杜撰 SLA。因预算过小未完成记 inconclusive；产品明确时限被违反且证据充分时才记 fail。

采集入局耗时、有效世界动作、首产物/目标变化、空转/重复请求、阻塞类型、确认延迟和消耗，并保留 attempt 分母。受限 stdout/event/artifact 与脱敏日志均设容量上限；安全信息、最终错误和关键回执不静默裁掉，容量不足报告 incomplete。可比较版本应固定 fixture 和 driver/profile；非确定性模型单样本是观察，多个重复样本也不自动成为发行证明。

## 10. 兼容、迁移与回滚

优先为现有 L4 scaffold/runner 增加实际自主试玩能力，复用启动、采样、目录与清理，不新建第二套测试平台。已有固定脚本模式保留原证据范围并标注 execution_mode；新模式不复用“固定建厂成功”假装 subagent 规划成功。是否修改现有 runner 在后续实现 PR 中决定，本次不添加虚构 CLI 参数。

开发自测统一从[操作手册](../manual/agent-runtime-self-playtest.manual.md)进入，派工与报告格式使用 `doc/testing/templates/`；不保留开发 Skill、默认加载入口或重复的兼容说明。正式玩家 `skills/oasis7-play/` 的职责不变；本专题执行规范明确的测试选择和完成边界，不修改现有 CI check、权限或 GitHub 保护设置。若新 API/Skill/profile 不具备，保留普通 CI 与现有 probe 的真实结果，对新路径报 blocked；不静默切旧 Provider 或直接模型回退。

## 11. 验证设计与可追溯性

确定性的输入裁剪、预算、证据引用、去重、报告状态和清理由普通 CI fake driver/临时持久 fixture 验证；实际 subagent 自主行为、Skill 上手和 GUI 再按授权实测。不能仅因使用 subagent 就强制“严格集成”，也不能把缺普通测试的合同丢给模型试玩代测。

各 ST 场景的 `contract` 与 `live` 子项分别归 `test_tier_required` 与 `test_tier_full`，具体范围见[手册分层矩阵](../manual/agent-runtime-self-playtest.manual.md#selftest-test-tiers)。同一报告不能以一项双层标签模糊已跑范围；每条 SourceSceneRecord 只归一个 tier。标注 tier 不新增 GitHub check，也不授权模型消费，实际选择仍服从开发规范及更严格的上游合同。

### 11.1 验证映射表

| 上游条款 | 本设计条款 | 独立 obligation 与适用条件 | 准确验证方法与场景 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [testing SC-12](../prd.md#testing-sc-12) | [编排](#selftest-orchestration) | 离线材料与推荐命令输出，不包含真实子会话或玩法执行 | `test_tier_required`：[总手册 L4-C06](../../../testing-manual.md#l4-toolchain-validation)的 JSON/path/已有目录保护与错误范围 | 实际生成 packet/cards/summary/manifest 的一致性、目录保护与错误范围 | 产品 N/A；reason=仅离线评审材料工具输出；scope=SC-12 scaffold；不证明 L4A/L4B 已执行 |
| [testing SC-13](../prd.md#testing-sc-13) | [编排](#selftest-orchestration) | 可重复地派出真实试玩者，使用正确构建/hosted 会话执行，并回收当前结果及独立评审；同时保留 [总手册 L4-C01..05](../../../testing-manual.md#l4-toolchain-validation)的工具链验证 | `test_tier_required`：对应 contract 子项；`test_tier_full`：对应 live 子项。[ST-01](../manual/agent-runtime-self-playtest.manual.md#st-01)；测试工具缺失、真实子会话、候选变化和派工隔离 | source/build/session 对应的子会话及本轮 RunResult、实际操作和判断来源 | 静态工具探测不证明已试玩；缺真实进入、执行与结果保留 blocked/inconclusive，不能按 SC-12 豁免 |
| [AC-EXT-011](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-011) | [Skill 接入](#selftest-skill-entry) | 同一正式 Skill 自助入局与失败可解释 | `test_tier_required`：对应 contract 子项；`test_tier_full`：对应 live 子项。[ST-02](../manual/agent-runtime-self-playtest.manual.md#st-02)；cold/prepared、缺失/过期 Skill、普通权限 | Skill hash、读取/启动/连接轨迹 | prepared_world 不代签 cold_entry |
| [AC-EXT-010](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-010) | [真实游玩](#selftest-gameplay) | 两 Runtime 各自连续任务上的首产物、真实阻塞及继续、目标应用、回访、断连和重启 | `test_tier_full`：[完整首局生命周期联合验证](../manual/agent-runtime-self-playtest.manual.md#first-session-lifecycle)，必须联合 ST-02、ST-03、ST-04 逐项取证；`test_tier_required` 仅做 oracle/身份关联负例 | 每 Runtime/profile/声明入口的同候选 world/Agent/task lineage、全阶段 operation/receipt 与判定 | 任一阶段/Runtime 缺失不通过整条 AC；局部 targeted pass 不代签完整首局或稳定生产 |
| [AC-COLLAB-007](../../product/agents-world-simulation/player-runtime-collaboration.prd.md#ac-collab-007) | [协作](#selftest-collaboration) | 跨角色目标/问答/审批及断连恢复 | `test_tier_required`：对应 contract 子项；`test_tier_full`：对应 live 子项。[ST-04](../manual/agent-runtime-self-playtest.manual.md#st-04)；复用既有协作场景和预算负例 | 目标/fence/审批版本、消息和回执 | scripted owner 不等于真人体验 |
| [AC-EXT-015](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-015) | [视觉表面](#selftest-surfaces) | 网页和客户端各自实际操作 | `test_tier_required`：对应 contract 子项；`test_tier_full`：对应 live 子项。[ST-05](../manual/agent-runtime-self-playtest.manual.md#st-05)；S6 与 native 窗口分开；无 GUI 负例 | build、截图、真实输入与同世界结果 | API/浏览器不代签 native |
| [AC-EXT-009](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-009) | [隔离与预算](#selftest-isolation-budget) | 无越权和无限付费，清理自有资源 | `test_tier_required`：对应 contract 子项；`test_tier_full`：对应 live 子项。[ST-06](../manual/agent-runtime-self-playtest.manual.md#st-06)；fake 费用、停止、孤儿 worker 与误选世界 | 有效授权、预算分账、清理回读 | 不证明 provider 未报告的费用 |
| [testing SC-1](../prd.md#testing-sc-1) | [反馈闭环](#selftest-feedback-loop) | 失败归因后选择适用测试层级，形成回归、修复与新候选独立复测 | `test_tier_required`：对应 contract 子项；`test_tier_full`：对应 live 子项。[ST-07](../manual/agent-runtime-self-playtest.manual.md#st-07)；旧失败、新回归、新 build 和未复现样本 | 失败归因、适用层级、regression test/PR 与新 attempt | 不证明全部测试层级均覆盖，也不以一次模型成功抵消旧失败或代签 L5 |
| [testing SC-2](../prd.md#testing-sc-2) | [反馈闭环](#selftest-feedback-loop) | 保持既有门禁/手册边界，核对普通回归与当前候选复测的独立结果 | `test_tier_required`：对应 contract 子项；`test_tier_full`：对应 live 子项。[ST-07](../manual/agent-runtime-self-playtest.manual.md#st-07)；先检出旧失败、修复后回归、新候选独立复测 | 实际适用 required/full 命令与结果、regression test/PR 和新 attempt 对应 | 不证明全部 CI 门禁持续可用，不改变 changed-path/launcher/full-support 义务，不以普通 CI 代签真实试玩 |
| [testing SC-15：source-to-scene](../prd.md#testing-sc-15) | [来源到场景](#selftest-source-scene) | 所选条款、实际树、适用性、执行状态、tier、artifact 与保留 gap 一致 | `test_tier_required`：[source-scene-check](../manual/agent-runtime-self-playtest.manual.md#source-scene-check)，旧来源/错树/缺字段/坏证据负例；`test_tier_full`：在所选 live 场景实际填充和核对 | 固定 SourceClauseRef、CandidateIdentity、逐样本记录和证据字节回读 | 格式通过不证明运行；不代签其他领域库存或另一 tier |

## 12. 决策、长期风险与未决问题

选择“现有测试体系 + 自测操作手册与模板 + 真实玩家 Skill + 受权验真”，拒绝仅让模型读日志评价，也拒绝每次改文档都跑多模型长局。首期优先一条小闭环，再按实际缺陷增加测试；不为追求 Agent 数量建立复杂角色审批链。

agent/qa owner 负责在实现中锁定可用的 Codex subagent 与 attached/supervised profile、隔离能力及受限启动工具；Runtime owner 负责可靠结果读回；Viewer owner 负责真实 Web/native 驱动。任一缺失明确影响哪些场景；不能默认偷偷安装软件、改用户配置或调用付费模型。

官方参考：[Codex subagents](https://developers.openai.com/codex/subagents)、[Codex skills](https://developers.openai.com/codex/skills)（2026-10-10 核对）。使用宿主实际提供的派工接口，不发明通用 `codex subagent` 命令；文档说明不能替代目标版本能力探测。持久技术真值沿 #4521 的 API/协作/adapter/world-view，日常结果在 PR 和实际 artifact，不回写成本/进度到本长期设计。

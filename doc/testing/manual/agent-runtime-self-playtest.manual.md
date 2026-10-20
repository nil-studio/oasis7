# 开发 Agent 自测操作手册：独立试玩与修复闭环

- Owner role：qa_engineer；固定设计基线：`eng-cc/oasis7@e524debf4584e5e6f2ae5baf9d877e78c9d7bf7d`。
- 合同：[自主试玩系统设计](../governance/agent-runtime-self-playtest.design.md)；策略：[testing PRD](../prd.md)、[总测试手册](../../../testing-manual.md)。
- 本手册是开发自测的唯一操作入口；配套格式：[试玩任务模板](../templates/agent-runtime-playtester-task.template.md)、[报告模板](../templates/agent-runtime-playtest-report.template.md)。试玩 subagent 仍使用实际交付的正式游戏 Skill。

这是实施后执行的手册，不是一次已运行记录。当前 PR 不增加 CLI runner、启动游戏或执行付费模型；缺失玩家 Skill、API、隔离环境、真实子任务/GUI 能力或授权时，应保存 blocked 及原因，不伪造测试结果。

## 规范触发与完成边界

是否必须执行、哪些缺口阻断本次功能交付，统一按[开发流程规范 §3.1](../../engineering/workflow/source-of-truth.md#agent-gameplay-selftest)判断。命中范围后由负责人主动按本手册操作，无需用户再次点名；本手册负责步骤与场景，不另设流程准入或合入规则。先做普通回归和预检，再完成所选真实试玩；缺依赖/工具/预算时报告实际未完成范围，不能用降级脚本或主 Agent 自测代签独立试玩。

## 职责、阅读范围与执行前确认

开发主 Agent 先读仓库 `AGENTS.md`、[开发流程规范](../../engineering/workflow/source-of-truth.md)、总测试手册和本手册，再结合当前任务、diff、候选及已有授权选择测试。不重复询问已明确的信息；本手册不另设审批或所有 PR 必跑的模型门禁，适用验证义务以规范为准；只读规划和预检可以先做，实际模型、服务及测试世界操作仍须处于既有环境、账户、资源和预算授权内。

| 使用者 | 阅读与职责 | 不应获得的内容或权限 |
| --- | --- | --- |
| 开发主 Agent / setup | 本手册和系统设计；固定候选、准备环境、派工、核验、修复与清理 | 不能用内部解法帮助黑盒试玩过关后仍称无提示通过 |
| 试玩 subagent | 本候选正式游戏 Skill、公开帮助、已填写的最小试玩任务及报告字段 | 不传整份开发手册、父对话、源码/diff、隐藏判据、owner/model secrets；不批准自己或递归派工 |
| owner/observer（按需） | 独立的有界观察/协作任务、测试账号及适用 Web/native 操作说明 | 不与 executor 共用 owner 凭据，不用 action API 代替图形界面操作 |

默认一名试玩者；UI/协作需要时至多增加一名独立 owner/observer。全部 subagent、受测 Runtime、视觉评估和重试共享总许可；最多两轮修复后试玩，同类阻断反复出现、预算/deadline 到限或权限/操作结果未知时停止新增调用并收口。不得自动充值、提高预算或无限重跑。

`attached_codex_subagent` 仅在候选已验证该 profile 时由子会话自身担任游戏执行方，不再无条件套一层付费 Codex。测试 `supervised_runtime` 时实际启动获准的 OpenClaw/Codex worker/profile，试玩子任务作为玩家/观察者且不接管 executor；不能用 Codex 模仿 OpenClaw。一个游戏 Agent 只有一个当前执行方。

## 最小使用方式

命中规范时开发主 Agent 应主动执行；需要明确派工意图时，可在开发 Codex 中给出以下提示（不是触发本流程的额外前置）：

```text
按 doc/testing/manual/agent-runtime-self-playtest.manual.md，
为当前修改做一轮有界自测。
先跑适用的普通回归；使用已批准的私有测试环境和本轮预算。
创建一个独立试玩 subagent，让它读取本候选真正交付的玩家 Skill，
自行启动/接入、规划和玩游戏，不读实现代码或使用调试世界写入。
保留真实操作/回执、失败步骤及未覆盖范围；必要时增加独立 Web/native 观察任务。
汇总反馈，复现确认的问题、补普通回归、修复，并在剩余预算内用新候选复测。
缺依赖、额度或必要工具时停在该边界，不能换旧 Bridge 或另一个模型冒充通过。
```

此提示是派工意图，不意味着任何 Codex 版本都具备相同 subagent/GUI 接口；主 Agent 先核对当前工具。显式派工不自动允许新增付费服务或使用用户日常世界。

## 执行顺序

1. **范围**：读 diff 和相关 AC，固定 targeted 或 ac_ext_010_full 声明及必需 obligation，选用下面的最小场景；沿用当前开发任务/PR 的授权，不创建通用流程登记。选择 `attached_codex_subagent` 或 `supervised_runtime`，记录真正受测 Runtime，避免双重启动 Codex。
2. **便宜前置**：按现有 `testing-manual.md` 执行受影响包的单元/合同测试；不跳过普通 CI，也不为了文档变更跑模型。候选准备可用当前仓库真实的 bundle/playtest 入口，必须先读其说明和实现，确认可指向本轮私有 fixture。不发明通用 spawn 命令或未实现的 autonomous runner 参数。
3. **固定候选和许可**：按[来源到场景核对](#source-scene-check)保存精确条款来源、source/base/实际 tested-tree、未提交 patch hash（有则必记）、build/Skill hash、API/driver 版本、世界信任/branch/root/revision/初始条件、执行模式、test tier、evidence level、总预算和 deadline；报告不能只写 main/latest。测试期间源码或 build 改变则结束当前尝试并新建 attempt。
4. **预检与环境**：确认玩家 Skill 是被测版本交付的同一份；旧 `site/skills/oasis7.md` 不自动替代新路径。验证真实 API/权限、world identity、独占进程/端口/存储、适用执行器能力和最小合法身份/准入前置；既有委托场景验证有效 executor，AC-EXT-018 冷入场则不得预先创建 Agent/task/goal/执行 grant，executor 的合法绑定必须由待测流程完成。测试目录不是生产目录的 symlink。所有模型、GUI 与下游工具均已获准。localhost 可能转发远程生产，须验证实际世界身份；不复用用户日常世界，先记录本轮自有进程/会话。
5. **启动场景**：`cold_entry` 由 subagent 按玩家 Skill 使用受限、预批准入口自己启动/连接；`prepared_world` 由主 Agent/setup 启动再交接。记录由谁完成每个步骤及 ready 的依据。准备好的世界不得代签冷启动体验。
6. **真实派工**：使用宿主实际派工工具创建独立子会话；按[试玩任务模板](../templates/agent-runtime-playtester-task.template.md)发送最小输入，不附开发对话、答案或服务日志。验证子会话实际读取了指定 Skill。源码隔离应由环境权限实现；仅靠指令或仍能读源码时，记录 `isolation=instruction_only/source_visible`，不声称 source-blind。
7. **游玩和观察**：子任务只走合法 Skill/API。主 Agent 不边看边给通关提示；需要提示时标为 assisted。UI 场景用独立 owner/observer 身份；故障注入由 setup 执行，不能赋予玩家原始存储写权限。
8. **验真**：回读证据 locator/byte_length/digest，核对实际执行状态、原 operation、authority binding、生产/授权回执和有序阶段状态；逐 obligation/tier 记录 oracle、判定和未覆盖 gap；回查 unknown/recovery_required。不信任模型的成功标签。收集已知、估计与未知费用，汇总所有子任务和受测 Runtime 的消耗。
9. **反馈与修复**：按[报告模板](../templates/agent-runtime-playtest-report.template.md)分诊。主 Agent 复现确认缺陷，添加能在旧版本检出问题的最小普通回归，修复后新 build、新 attempt、必要时新试玩子会话；保持原失败。不能为绿色结论改判据或无限重跑。
10. **退出**：停止新调用、撤销测试委托、核对 pending、关闭自有 worker/browser/subagent、核查清理。保留脱敏 artifact，不提交私钥/模型原始思维/用户消息到 PR。关闭子会话不代表 worker 已退出，须实际核对；不杀其他进程、删除未知工作区或改变线上世界，清理失败明确报告。最后报告执行了什么、发现什么、修了什么和哪些未证明。没有真实 subagent/执行就如实说明；不自动建 Issue、push、合入或发布，仅在原任务已授权时由主 Agent 执行。

## 按改动选场景

以下矩阵承接规范的触发范围；行为相关场景不能因文件位于文档目录而豁免。只选择本次受影响的 Runtime/profile 和表面；不因一个组合失败扩大到无关平台，也不把未跑的受影响组合删除。

| 改动 | 适用的最小组合 | 不可替代的证据 |
| --- | --- | --- |
| 仅分析/设计、文档排版或已核实不影响相关行为的重构 | 普通静态/单元检查；必要时 ST-01 dry preflight | 说明不适用理由；改变正式游戏 Skill 使用方式不在此豁免 |
| Skill、接入、API schema、能力目录 | ST-01/02 + ST-03 + ST-04 的适用生命周期项；合同负例先普通 CI | targeted 仅报所选项；声明 AC-EXT-010 必须全量生命周期、两个真实 Runtime，不得遗漏 ST-04 |
| 目标、审批、fence、恢复 | ST-04 + 受影响普通故障回归 | 当前版本、授权生效、原结果与无重复副作用 |
| 玩法/生产变化 | ST-03 + ST-07；ST-04 按生命周期声明/实际影响选择 | 首产物仅证明 production_only；声明 AC-EXT-010 须联合全量生命周期 |
| Web/native 中与 Agent 游戏相关的可见功能 | ST-05 + 相关 ST-03/04 | 受影响表面各自实操；共同组件覆盖两端，不从 API 推导 UI |
| worker、取消、用量 | ST-06 + 受影响 ST-03/04 | 总预算、未知调用和自有进程退出 |

未选项目标记 not_applicable 并说明与当前修改无关；已选但无法执行的项目标记 blocked/inconclusive，不可静默从分母删除。旧 L4 artifact 可以复用，但此次必须记录实际 `execution_mode`，不把固定 runner 路线改名为自主规划证据。

<a id="first-session-lifecycle"></a>
## 完整首局的联合验收

按 [AC-EXT-010](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-010)，`scope=ac_ext_010_full` 必须联合 ST-02/03/04。**OpenClaw 与 Codex 各自**选定实际受支持组合，在同一玩法候选及可比较初始条件下分别运行；每个组合内部贯穿同一 world/Agent/task 历史，不能拼接重置世界、不同任务或不同候选的成功片段。

| 必需项 | 场景承接 | 每个 Runtime 组合必须保留的实际证据 |
| --- | --- | --- |
| entry | ST-02 | 推荐玩家 Skill 实际读取、合法绑定与高层目标；prepared_world 不能代签需要 cold_entry 的启动步骤 |
| first_product | ST-03 | 首局主责的权威生产成果、原操作和前后资源状态，非累计动作数或模型自报 |
| blocked_continue | ST-03 + ST-04 | 一次真实资源/生产阻塞，明确原因及在原授权内继续推进的后果；只测故意非法参数被拒不算生产阻塞恢复 |
| goal_applied | ST-04 | 同任务持续目标新版本真正用于后续决策和有效动作，不能只验 received/adoption_reported |
| leave_revisit | ST-04 | viewer 关闭/回访，或所声明 pure_api 观察客户端断开/重新读取；期间 Runtime 有效运行的实际成果与可解释状态 |
| disconnect_resume | ST-04 | 实际连接中断后回读同任务、核对未决与已结算结果并续接，无重复效果 |
| runtime_restart_resume | ST-04 | 实际 Runtime 进程重启后同任务继续；核验旧批准/fence 与结果，私有 session ID 可变；不得拿连接中断替代进程重启 |

每项分别保留 phase/specimen、原始证据与 verdict，关联同一 candidate/profile/primary mode/世界任务；断连与进程重启留两份不同故障记录。若声明多个入口，分别补足入口相关证据；纯 API 回访不强制 GUI，也不能代签 Web/native 的 ST-05。必需项缺失为 blocked/unverified；所有指定 Runtime 的所有项均真实满足后才能报告 AC-EXT-010 passed。

`scope=targeted` 可以只验证受影响组合和局部项，但报告必须列出其余生命周期/Runtime 的 retained_gap，明确整条 AC 未验证；不得把局部 scope pass 自动提升为 AC pass。完整验收失败后不能改名 targeted 覆盖原失败。选型不扩大现有付费许可，预算不足按原范围报告未完成。

<a id="selftest-test-tiers"></a>
## 测试层级与子项

遵循 testing PRD 的 AC-4。下表保留 ST-01..07 稳定编号，用 `check_kind=contract/live` 区分可独立执行的子项；每条记录只填一个 test tier。contract 成功不能代签 live，原有上游更严格的分层义务仍有效；不是把其他 required 测试迁到 full，也不让每个 PR 自动付费跑全部 live。

| 场景 | contract：test_tier_required | live：test_tier_full |
| --- | --- | --- |
| ST-01 | 输入裁剪、候选/隔离预检和缺工具负例 | 宿主真实创建独立子会话、读 Skill 并回收报告 |
| ST-02 | Skill 版本/来源/allowlist、缺依赖与错误目标 | 正式 Skill 在受测 Runtime 的 cold_entry/prepared_world 实际入局 |
| ST-03 | 权威 oracle、错误关联、自报成功与不完整生命周期拒收 | 自主首产物、真实生产阻塞及继续；联合 ST-04 验收完整首局 |
| ST-04 | 版本/fence/审批/幂等及持久化故障注入 | 真实 Runtime 同任务的目标应用、回访、断连与进程重启续接 |
| ST-05 | 投影/权限/乱序与交互数据合同 | 实际 Web/native 输入、世界探索及指导，各表面独立 |
| ST-06 | fake driver 用量/预算/取消、临时自有进程和隔离负例 | 实际受测 profile 的用量可见性、停止/未知状态与 worker 清理；不能由 fake 推定 |
| ST-07 | 最小重现、旧失败/新通过回归、反馈与候选关联 | 修复后新 build、新独立试玩者在同等条件下复测 |

<a id="source-scene-check"></a>
## 来源到场景与证据留存核对

主 Agent 按 [SC-15](../prd.md#qa-infrastructure-source-scene-method) 和[设计字段](../governance/agent-runtime-self-playtest.design.md#selftest-source-scene)建立本轮 SourceSceneRecord；只保留所选条款与场景范围，不新增完整基础设施验收或独立流程台账。模板的来源、判据和隐藏 oracle 由负责人维护，试玩者只填获准的观察/操作字段。

1. 预检锁定 path/fragment/clause_id、source_oid/blob_oid/content_sha256，确认对应已采纳原文；记录 source_head/base_oid/execution_target/tested_tree_oid、适用 event commit、dirty patch 与 build。无法确认来源或实际测试树时说明 gap，不能把旧绿结果贴到新候选。
2. 为每个 clause/scene/check_kind/cell/specimen/phase/Runtime/profile/入口/tier 列出 applicability、owner/basis、execution_state；无场景实现是 planned/located，不写 executed。局部声明保留整条上位 AC 未证明的部分，不能删掉完整声明的必需项。
3. 保存基线、配置、依赖/拓扑/provider 身份，按实际先后采集阻塞或中断、fresh-read、恢复、retry/replay 的 world/root/revision/window、数量及操作回执。只保存获准信息，公开 artifact 使用独立标识的脱敏副本。
4. 回读原始证据文件，验证 locator、byte_length、digest_algorithm/digest 与采集者/时间，再分别核对结构/canonical/领域/信任/身份/join oracle；命令结果或真实可见操作缺失不能靠截图标题补造。
5. 填 test_tier、evidence_level、tier_verdict、失败签名/阻断原因与 retained_gap/owner/next_step；执行状态和判定分开。缺必需材料保留 blocked/unverified，shape_verified 不升级为试玩 passed。追加保存失败与新尝试，不原地改写旧证据。

这项记录合同检查属于 `test_tier_required`：用临时非真实报告分别注入缺来源 fragment、过期 OID/错误 digest、source/tested tree 混淆、未执行却填 passed、层级缺失或错标、artifact 字节被改/长度不符/越界、必需阶段缺失和跨 world/task/profile 拼接。均应拒绝对应 coverage claim 或保留明确 gap；完整且身份/字节一致的样例只证明记录检查自身通过。所选 live 实测在 `test_tier_full` 填入真实记录并独立验真；模板/检查通过不制造运行证据。

## 场景与判据

<a id="st-01"></a>
### ST-01：编排与独立试玩身份

- Test tier：`test_tier_required`（contract）；`test_tier_full`（live），按[分层矩阵](#selftest-test-tiers)分别执行和记录，不能相互代签。

- 准备：固定候选、一个 tester、可选 observer 和受限输入；另准备缺派工工具、继承源码上下文和 build 中途变化的负例。
- 执行：先 dry preflight，再在真实支持的 Codex 中创建子任务、读 Skill、返回报告；观察主 Agent 是否传入内部解法。
- 判定：真实子会话/输入/候选可核对；不具备能力或隔离时如实降级，不用主 Agent 文本代签。候选变化后旧报告只归属原版本。
- 层：确定性 contract 先普通 CI；live 必须有真实 subagent 调度证据且在已有授权内执行。

<a id="st-02"></a>
### ST-02：玩家 Skill 的冷启动与上手

- Test tier：`test_tier_required`（contract）；`test_tier_full`（live），按[分层矩阵](#selftest-test-tiers)分别执行和记录，不能相互代签。

- 准备：当前正式玩家 Skill、固定 hash、有效私有世界配置；另有 Skill 缺失、错误 major、旧 Bridge 和非法世界目标。
- 执行：新子会话仅阅读 Skill/公开帮助，使用受限入口启动或连接；分别运行 cold_entry 与 prepared_world。涉及 [AC-EXT-018](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#ac-ext-018) 时，cold_entry 必须从无预置 Agent/task/goal/grant 的主体开始，按 [api-cold-entry](external-agent-runtime-contract-validation.manual.md#api-cold-entry)覆盖身份、准入、角色、owner/starter、task/目标与 executor 的整条链；只启动进程或连接预置角色不足以通过。
- 判定：必要人类确认、凭据仪式及费用批准按原授权停下并记录，不被“自主”取消。记录每阶段实际执行者、初始预置内容与权限来源；setup 不得临时代做角色/task/grant 来掩盖缺口。进入真实受权世界，知道任务/能力与失败下一步；缺依赖 blocked，不能创造假端点或测试专用玩家说明。记录 setup_assisted，两个启动模式不代签。
- 承接：[skill-release](external-agent-runtime-contract-validation.manual.md#skill-release)；此处新增的是独立试玩输入与冷启动体验。

<a id="st-03"></a>
### ST-03：自主首局与结果验真

- Test tier：`test_tier_required`（contract）；`test_tier_full`（live），按[分层矩阵](#selftest-test-tiers)分别执行和记录，不能相互代签。

- 准备：既有首局玩法/初始条件、允许的资源、实际 Runtime profile；另注入自报完成但无 receipt、仅 ambient 库存变化和结果未知等样本。
- 执行：给出不含内部解法的工业高层目标，让 tester 自主查询/行动/等待/求助；在初始 fixture 已声明的合法资源/生产约束下遇到真实阻塞并继续，保留阻塞原因和继续后的权威进展。不能在运行中发资源或直接改世界过关；验真器核对原请求、状态和成果。
- 判定：首产物满足主责合同，真实阻塞及继续与本次 Agent/任务相关；自报、动作数、ambient 变化不算。策略失败、产品故障和合法拒绝分别记录。AC-EXT-010 还必须联合 ST-02/04 的[全生命周期](#first-session-lifecycle)；仅此项通过不得标整条 AC passed。
- 承接：[api-first-session](external-agent-runtime-contract-validation.manual.md#api-first-session)、[api-idempotency](external-agent-runtime-contract-validation.manual.md#api-idempotency)。

<a id="st-04"></a>
### ST-04：异步协作与恢复

- Test tier：`test_tier_required`（contract）；`test_tier_full`（live），按[分层矩阵](#selftest-test-tiers)分别执行和记录，不能相互代签。

- 准备：隔离 owner/executor、有限授权，fixture 允许受控断连/重启；预先定义 owner 会批准与拒绝哪些请求。
- 执行：离线发布目标、旧草稿竞态、问答、批准/拒绝和撤销；与 ST-03 的同一 world/Agent/task 联合验证持续目标真正应用、按声明 viewer/pure_api 离开并回访、实际断连后续接、实际 Runtime 进程重启后续接，另测事件缺口及冷启动完整目标。故障由 setup 注入；断连和重启分别记录，不重置世界或换任务补阶段。
- 判定：读取/采纳/上下文/世界结果区分；无回复不批准；硬撤销不等已读；旧实例、旧批准和未知提交不生成第二次世界效果。scripted owner 标注为脚本协作者。
- 承接：[collab-executor](external-agent-runtime-contract-validation.manual.md#collab-executor)、[collab-recovery](external-agent-runtime-contract-validation.manual.md#collab-recovery)、[collab-approval](external-agent-runtime-contract-validation.manual.md#collab-approval)。

<a id="st-05"></a>
### ST-05：用户实际看世界和指导

- Test tier：`test_tier_required`（contract）；`test_tier_full`（live），按[分层矩阵](#selftest-test-tiers)分别执行和记录，不能相互代签。

- 准备：同一世界的正式 Web 和受支持 native build；分别具备可用输入/截图工具，observer 无 executor 写权限。
- 执行：世界概览、缩放/平移、定位 Agent、选择工厂、查看目标/回执/阻塞并给高层指导；记录真实输入与对应截图。
- 判定：两端各自在自身界面完成；native 仅开浏览器失败，无 GUI 则 blocked。原始截图带候选和时刻，截图可见不等于交互完成。可读性反馈保留具体步骤，不称 L5。
- 操作前读：[Viewer S6](web-ui-agent-browser-closure-manual.manual.md)；native 承接[viewer-native](external-agent-runtime-contract-validation.manual.md#viewer-native)。

<a id="st-06"></a>
### ST-06：预算、停止和隔离

- Test tier：`test_tier_required`（contract）；`test_tier_full`（live），按[分层矩阵](#selftest-test-tiers)分别执行和记录，不能相互代签。

- 准备：fake driver 可报告累计/重复/缺失 usage，测试 worker、browser 与一个不属于本轮的进程；禁止费用权限和生产目标作为负例。
- 执行：并行预算预留、子调用/取消/重启、超时、report 洪泛、恶意消息、越界路径、stop 后孤儿 worker；只在临时环境注入。
- 判定：所有调用共享有限授权，未知不算零；没有自动 top-up、递归派工、默认生产入口或秘密泄露；仅回收自有资源，泄漏明确报告。取消未确认不宣称上游已停止收费。
- 承接：[adapter-budget](external-agent-runtime-contract-validation.manual.md#adapter-budget)、[adapter-liveness](external-agent-runtime-contract-validation.manual.md#adapter-liveness)。

<a id="st-07"></a>
### ST-07：从试玩失败到回归与复测

- Test tier：`test_tier_required`（contract）；`test_tier_full`（live），按[分层矩阵](#selftest-test-tiers)分别执行和记录，不能相互代签。

- 准备：有真实失败操作/回执的旧候选、最小重现和新候选；另包含不能复现、测试框架错误、正常拒绝、预算不足样本。
- 执行：按报告分诊；确认 bug 转普通测试，验证旧版本能失败、新版本通过，然后用新 tester 对同等初始条件复测。
- 判定：原失败与新尝试均保留，候选/Skill/profile 清楚；未复现不伪造已修复，未执行不算 pass，不降低判据或改测试世界过关；达到轮数/预算上限收口反馈。
- 输出：PR 上描述定位、修复、回归命令/结果、实玩结论、费用和仍未覆盖项，不另建放行证书。

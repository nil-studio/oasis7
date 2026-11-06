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

<a id="agency-domain-composition-validation"></a>
## IA-C：间接决策与原领域结果的组合方法

依据：[领域因果接收](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#agency-domain-causal-reception)，承接 IA-001/002/003 原要求和 Trace。以下是 ST-03/04/05 可选择的业务组合，沿用本手册的触发、授权、预算与分层规则，不增加所有 PR 的自动试玩门禁。

**固定输入与独立预期。** 每次锁定候选/实际测试树、world/branch、领域 authority/version、actor/Agent、canonical intent/decision、原 request/operation、完整对象修订/digest、原结果与当前 audience。先由负责人从原签署正文、合法来源、账本、事件和实际窗口独立列出接受、效果、费用、纠正切点及下一步，再与 Agent/Viewer/API 投影比较；不用被测投影、时间相邻或余额变化自证关联。一次意图可合法产生多个领域效果，每个效果保留自己的原件和完成角色，不把“只执行原操作一次”误读为整个业务只能有一个效果。

设计审读使用明确标为构造的输入与权威顺序，只检查合同和独立预期能否闭合，记录 `design_review`，不计 contract/live 运行通过。实际 contract 使用 `test_tier_required`，live 使用 `test_tier_full`，分别按既有矩阵执行和留存；原 producer、入口、权限或环境缺失即保留对应 `blocked/not_run`，不能以全部拒绝、mock 或另一个领域替代正常分支。下表所引原方法继续拥有其完整场景和验收，组合检查不取代它们。

| 场景 | 正向输入与执行 | 独立判据与配对反例 |
| --- | --- | --- |
| IA-C01：记忆与工业行动 | 按 [DES-ADOPT-002](../../game/gameplay/gameplay-decision-delegation-return.design.md#des-adopt-002) 固定意图 I、实际使用的 memory revision M、纠正 C、prepared decision P 和匹配 committed result R；先记录原观察/可读记忆、真实成本和替代，再跟踪最早可用的纠正点与原工业动作 | 被接受意图、理由/证据、stakes/预期后果、真实替代、打断/纠正、最早生效点、实际结果分别取其原来源；C 被接受但尚未进入 P、R 绑定另一 decision/M、纠正后动作未变而没有原因、用预期产物冒充实际产物均不得写“纠正已应用”。合法未采用/尚未生效可保留准确原因，旧结算不重做；不要求所有动作先有长期记忆，也不读取私有推理链 |
| IA-C02：完整合同族 | 联用 [CF-07](agent-mediated-contract-formation-acceptance.manual.md#cf-07-read)、[CF-12](agent-mediated-contract-formation-acceptance.manual.md#cf-12-family-handoff)、[CF-13](agent-mediated-contract-formation-acceptance.manual.md#cf-13-revocation-succession)、[CF-16](agent-mediated-contract-formation-acceptance.manual.md#cf-16-consumers)；逐族关联 intent、agreement revision、terms/render digest、principal/代表/mandate、原接受和领域结果；核真实 D/O/C、成本/替代和纠正后的下一步 | 每个已采纳族分别有正向形成与实际结果，不以即时交换代签原个人供能、持续供能/维护/运输/共同工作、R&D/epoch/份额转移。安排撤回先于激活、激活先于撤回及条款修订后三种顺序：旧接受不能签新版，后撤权不抹旧义务；激活≠服务已履行、arrival≠商业交付、签署≠新贡献或成果收益。原件未知和继受不洗历史责任 |
| IA-C03：普通与保护治理 | 从 [GOV-M10](ordinary-and-protective-governance-acceptance.manual.md#gov-10-bounds-surfaces) 的同 C0、source cut、事项/主体/原操作与当前读者开始，分别取得普通 matter/ballot/审查及保护 case/执行/救济的正常原件；沿实际建议说明成本、替代和最早有效点 | 普通 passed、申诉受理、保护 ready 与实际执行/救济分列；最终化后的 redress 不倒写原 winning option，代表退出不删旧票。按 [GOV-M08 多案前态](ordinary-and-protective-governance-acceptance.manual.md#gov-08-protective-authority) 独立核 `100−10−7+10=93`，不回到 100、不抹另一案限制，重复裁决不再加 10。数值只用于原规则支持的该样例；当前受众失权不泄露旧案私有来源，也不捏造处分权限 |
| IA-C04：冲突、防御与恢复 | 联用原[防御方法](chartered-conflict-recovery-and-soft-season-acceptance.manual.md#conflict-defense-cases)与 [CF-06 恢复方法](chartered-conflict-recovery-and-soft-season-acceptance.manual.md#loss-recovery-cases)，固定 conflict/participant、稳定 defense lineage、hostile effect、资产/损失/原目标及恢复候选；分别安排合法响应与最终 Revoke 的先后顺序，再比较原规则真实支持的修复/重建 | 先撤则不产生后续新响应，先已提交的效果保留；比较本身不修复/扣费，实际新能力须对应原动作与结果。CF-06 的部件、E、时长只在原测试规则支持时采用，不能将样例变成新平衡规则。合法 pivot 须明确接受新目标，不能称原目标成功；结束、退出或季节刷新不抹在途、待付、未知原件和旧责任，无安全路时返回准确缺口与重新决策 |
| IA-C05：身份、原件与当前读取 | 对 IA-C01–04 每一类保留 canonical intent/decision 与原领域 request/result 的完整关联，按当前合法读者从原协议恢复相同原件；独立核来源、world/branch、主体、版本、效果角色与可披露范围 | 分别换成 feedback label、相邻时间的另一动作、错误 world/revision/blob、当前已撤读权和缺页来源；不得猜同链、用旧缓存泄密或把缺页当空集。未知结果查询原操作，不以新 nonce/身份/业务键探测；无当前合法查询时保留最后已证阶段和实际下一步，不伪造“未发生”或完整续接 |
| IA-C06：等待、离开及跨入口回流 | 同一已验证权威事实分别由实际支持的 Viewer/API 读取，核意图接受、推进/主阻塞、最近后果、可执行下一步及适用的七类因果信息；让原等待 trigger 失效，再按原条件重分类，并从真实旧版本/断连状态恢复 | 两入口布局可异、事实一致；弱理由无限等待、失效 trigger 仍称安全、empty/reload 冒充未执行、旧缓存复活失效授权均不通过。缺必要历史关联时给有界历史与准确重新决策；只有某入口、mock、world tick 或字段齐全不能代签另一入口、真实 provider 或玩家理解 |

**记录与读者检查。** 原 PR 留每个实际适用领域的固定输入、独立预期、原件关联矩阵、纠正前后权威顺序、真实差量、当前可用动作及未运行层级。观察玩家能否据当前允许内容说明“接受了什么、实际改变什么、为什么、还能改什么、未知时怎么办”，真实观察与设计文案检查分别记录；表格齐全和模型自评不证明理解、信任、留存或完整产品验收。

<a id="micro-depot-operation-validation"></a>
## MD：有限设施的当前提交与原结果方法

依据：[有限库存生命周期](../../game/gameplay/gameplay-regional-infrastructure-micro-depot-contract.design.md#micro-depot-measured-lifecycle)与[提交及结果接收](../../game/gameplay/gameplay-regional-infrastructure-micro-depot-contract.design.md#micro-depot-current-submission-recovery)，承接 GR-001 原要求和 Trace。此组是 ST-03/04/05 的可选 micro_depot 场景；沿用上述固定候选、独立 oracle、设计审读 / contract / live 分层和既有触发授权。当前内核能力、正式 adapter 与玩家入口分别记录，不能以裸内核成功或通用 API 设计证明设施已正式接通。

| 场景 | 准备与执行 | 独立判据与配对反例 |
| --- | --- | --- |
| MD01：原操作与领域结果 | 固定 world/source、原 owner/Agent、facility/target、完整请求与实际入口；分别跟踪 Install、Repair/Logistics Service、Upkeep、Reclaim 的提交与返回记录，先由原提交链独立列出关联 | 区分 Kernel 的 `ActionEnvelope.id`、`WorldEvent.id`、实际存在的 service/upkeep `receipt_id` 以及正式 adapter 原操作身份。Install/Reclaim 的事件没有通用 receipt 字段，不补造。latest receipt、同名设施、库存变化和对象消失不证明某原请求成功；另一个 Runtime 的维修/运输不由本内核 service 自动完成 |
| MD02：当前重验与原子拒绝 | 从同一合法 quote 前态分别只改变当前 owner/claim、upkeep、range、资源或 module/schema/proposal 条件，再按原同步动作形成当前 evaluator input/proposal 并提交；另取条件仍合法及无关 revision 改变的对照 | 始终按当前规则决定；hash 只证明输入/proposal 身份，不能替权限与前置校验。合法动作按原链执行一次；任一必需条件不成立则设施库存/吞吐与 service world effect 零变化，并有真实 blocker/取舍/下一步。无关 revision 不凭本方法成为新 stale gate；不对未知旧请求改签、塞新报价或增加报价预留 |
| MD03：有限账本与耗尽恢复 | 按原 v2 接受一次安装，独立记录玩家扣 10 Data、sink 2、设施库存 8、throughput 16 / epoch 0；从实际合法 proposal 取得精确 Data debit `d`，分别手算库存和吞吐的 `before−d`，逐次对账到库存耗尽，再验证 upkeep、reclaim 与新安装 | quote 不预留/扣减；`d` 由原 module/proposal 决定，不新定默认 debit、维修比例或物流损耗。零库存 upkeep 拒绝且不收费，reclaim 零退款并销毁余量，新装再付全额 10，不继承旧库存/吞吐。v1 原 wire/hash 与旧回放保持，不补发 v2 库存；当前库存先于吞吐自然耗尽，不以注入 throughput 耗尽证明新玩法，也不测试未采纳的补货或 epoch reset |
| MD04：可能已发、丢回复与当前读权 | 在实际支持该动作的原 adapter 上分别固定“可能已发但回复未知”“已证已执行后迟到错误”“当前读权撤销”“原查询不可用”“另一入口查询同原件”样例；依原 protocol 查询相同操作，保留最后已证阶段 | 仅原协议明确允许且条件满足时才精确重投；Kernel 每次 submit 产生新 action ID，不能用再次 submit 当原件回查。无查询/历史/权限时准确 unknown/受阻，不换 nonce、facility ID、操作键或新报价；已确认结果不被迟到错误倒改。跨入口须是该原协议实际支持且授权的读取，不互换 namespace/签名。未接 adapter 标 blocked，不用静态推演、快照或别的动作补通过 |

原 PR 分别保留 quote 与当前提交条件、原请求/事件关系、独立资源前后值、实际服务效果、恢复与当前可用下一步。当前区域压力引用的 opaque/insufficient 状态照实呈现；本方法不新增压力认证、预测生产者、补货、长期 epoch 循环或其它设施。原 §11–12 的 measured-supply 局部测试与玩家观察仍分别有效，方法存在不表示它们已在本候选执行。

<a id="dev-test-accounts-validation"></a>
## 开发测试账号工具：外部 Agent 接入与双账号验收

依据：[开发工具要求与 AC-DEV-01～06](../../p2p/blockchain/hosted-public-join-managed-identity-custody.prd.md#dev-test-accounts-acceptance)、[职责与验证设计](../../p2p/blockchain/hosted-public-join-managed-identity-custody.design.md#dev-test-accounts-verification)。本组验收 `dev-test-accounts-v1` 的账号选择、秘密隔离、受限调用与恢复。

准备：固定实际 source/build、正式 Skill／工具说明、服务部署、Runtime／connector 版本、认证方式、两个已获准测试账号 A／B、世界／任务／资源及有限预算。本地和远程各选一条真实外部 Agent 接入组合，当前验收计划分别使用 Codex、dot。先通过各自的认证入口调用只读能力信息；缺入口、秘密保管能力或测试环境时记录具体 blocked 条件。此预检可先于完整业务适配，不以 curl、mock 或另一个 Runtime 的成功替代。

普通合同和故障用例先执行。真实外部 Agent 仅获得正式说明、获准账号的非秘密元数据与受限工具；编排者保管 setup／Owner 权限并独立核对身份、原操作和世界结果。选择两个账号各有合法权限、效果可辨别的小操作，例如各自游戏 Agent 的一次合法移动，无需完成整条工业生产流程。

| 场景／验收 | 操作与观察 | 独立判据 |
| --- | --- | --- |
| DA-01／AC-DEV-01 | 管理者登记 A／B，外部 Agent 列账号；核对实际身份、环境与状态，检查模型可访问面及输出／日志 | 公开身份与原生账号一致；模型无法读取服务私有凭据、解锁材料或登录秘密；错误解锁／损坏凭据明确失败且不新建替代身份 |
| DA-02／AC-DEV-02 | A → B → A 打开会话并读取；并行调用两会话；用 B 会话提交 A 的资源／环境；在 A 活跃时修改别名或新增账号 | 每个结果归属正确账号；跨范围拒绝；既有会话不重新绑定，累计用量及未决操作不变 |
| DA-03／AC-DEV-03 | 两账号各经真实工具完成读取和一个获准写操作，另用无权账号调用同目标 | 独立回读实际主体和世界效果；无权调用无新增世界副作用；accepted 或工具自报不代替真实结果 |
| DA-04／AC-DEV-04 | 对可能已发的写请求丢响应，保存原 ID；重启服务，验证旧会话拒绝，当前授权下建新会话查询旧操作 | 同一原生 operation 可恢复且效果仅一次；同 ID 异正文冲突；查询缺历史／权限时准确 unknown，不换 nonce 重新写 |
| DA-05／AC-DEV-05 | 关闭、停用、到期后再调用；恢复含旧 active 标志与较早用量的备份；提交认证 audience 或用途不匹配的凭据 | 新执行被拒绝；恢复的旧授权不能重新启用，旧快照不恢复原额度或写权；缺失用量／原操作保留 unknown，核对后须明确新建授权；不匹配的凭据被拒绝；在途结果与所需世界控制／清理分别报告 |
| DA-06／AC-DEV-06 | 外部 Agent 分别通过真实已交付的本地与远程入口执行以上适用组合 | 分别记录实际 Runtime、认证、传输和结果；任一接入组合未完成都保留缺口，不以另一条链或确定性脚本代签 |

使用现有 RunInput／TesterInput／RunResult 及报告模板记录版本、账号的非秘密身份、操作关联、实际观察和未覆盖项。普通回归、接入预检、真实模型使用、世界效果和运行环境隔离分别说明证明范围。若需要验证网页登录／切换，另选择真实浏览器会话场景；native API 结果不证明 UI 已通过。

收尾先禁用本轮新调用，按实际需要和已获准权限执行既有世界控制／资源清理，保留 unknown。工具验收不把关闭本地会话写成世界已永久关闭。本节提供实施后的方法；仅修改设计或方法本身不产生一次真实运行证据，触发和执行授权仍遵循本手册入口及开发规范。

### 开发测试服务共同入口

Refs #5076、#5531。Trace refs: O7-REQ-56c3a553-8216-4e44-86dc-bd368f3b53cb。

`dev-test-serve --profile <private-config.json>` 启动串行 Unix socket 服务；`dev-test-call --socket <approved-socket>` 从可信宿主 stdin 接受 JSON。模型隔离及实际外部 Agent 接入由 #5532/#5533 承接，本入口不证明该隔离。

配置须由当前用户拥有且权限 0600，socket 父目录 0700。配置字段：profile=`dev-test-accounts-v1`、socket、usage_directory（固定私有目录）、grant_id（稳定授权标识）、max_request_bytes（256～65536）、timeout_ms（1～30000）、max_calls（正整数）、credentials（caller_id、token〔至少32字符〕、expires_at_unix、revoked）。认证材料只由可信宿主管理，不进入模型输入。本能力入口不提供账号管理操作。

请求仅接受 profile、audience=`oasis7-dev-test-tools`、credential、method=`capabilities`。错误版本、用途、过期、撤销与未知方法拒绝，未知字段拒绝。返回 accounts_available=false、writes_available=false。并发固定1；超时断开请求。只有成功能力调用消耗累计上限，拒绝流量不扣。用量按 grant_id 与 caller_id 在 usage_directory 中保存，跨入口共用锁及原子提交，先持久化再回复；重连、重启或更换 socket 均不清零。重启前由可信运营者确认旧进程终止并删除遗留 socket，保留整个用量目录。只有显式新授权可使用新 grant_id；目录不能换成空目录来恢复旧授权。正式账号会话由 #5535 承接。

在 Linux native 环境执行 `env -u RUSTC_WRAPPER cargo build -p oasis7_game_client --locked`，使用生成的 `target/debug/oasis7_game_client` 运行以上命令。服务与可信调用适配器使用同一受信 OS 主体；客户端先核对 socket 路径、所有权、权限及服务 peer UID，再发送认证材料。模型使用的跨主体或远程接入另按对应适配方法配置。

启动前准备私有 socket 父目录及固定 usage_directory，明确有限配置和绝对凭据到期时间。可信调用端通过私有 stdin 输入上述能力请求，检查返回 status；授权到期、撤销、额度耗尽或超时后均须按实际状态处理，不自动更换 grant_id。普通重启前确认旧进程停止，只删除遗留 socket，保留用量目录，重启后再次核对原授权用量连续。请求读取与可信调用传输使用绝对 deadline，持续发送零碎字节不能延长期限。

按 DA-06 单独记录实际外部 Agent 接入结果；服务进程及 CLI 验证不代替外部 Agent 使用验收。

### 可信运营者账号目录准备

Refs #5534。首版本地管理 CLI 使用运营者 OS 主体和 0700 私有目录；不接模型工具认证或远程管理请求。先以既有 identity-init/register/login 准备账号，凭据保留既有加密 IdentityStore，解锁仅通过可信 stdin。

`dev-test-catalog-register --directory <0700目录> --identity-config <0600配置> --operation <登录原操作ID> --alias <别名>` 核验当前 /session 主体并登记；同账号与环境返回稳定 account_ref，重复登记不得替换凭据主体。别名可修改。`dev-test-catalog-list --directory <目录>` 只展示非秘密引用、账号、环境、公开key与别名，不输出凭据路径或原登录引用。列表是登记投影，不能代替当前权限。目录中的 catalog.json 及锁和临时文件只供可信运营者访问；实际凭据保持在既有加密库，不复制私钥或Cookie。

真实A/B隔离原生身份登记已由#5534核验；本项继续以同一有限环境验证账号只读服务。受控身份服务测试或静态目录不能代签合法账号登记，实际证据按对应固定source/build记录。

### 多账号只读会话（#5535）

二进制 `oasis7_dev_test_sessions --serve PRIVATE_CONFIG` 从可信私有stdin读取账号引用到标准Base64编码unlock的JSON映射（opaque bytes，不假定UTF-8）。配置由可信运营者准备，包含共同 service 配置、catalog_directory、grants 和 resources；每项grant有独立grant_ref、共享稳定grant_id、caller_id、不可变binding、绝对expires_at_unix、enabled与只读methods。binding固定account_ref/environment/world/genesis_digest/agent_id/resource/executor_config_digest/authorization_version；resource只由管理员固定account_ref与executor_config路径。模型请求不能提供这些路径或unlock。

`oasis7_dev_test_sessions --call APPROVED_SOCKET` 从可信宿主stdin接收credential、method及可选grant_ref/session_ref/expires_at_unix。method仅有accounts、open、close、describe、task、capabilities。session_ref不能单独认证。打开和业务读取复核目录原生current_account与Executor配置的实际身份、环境和fence，再复用正式Client；不调用context、不写世界。累计用量复用共同grant/caller账本，不随session或别名变化。普通重启清空内存会话，保留账本；管理者确认旧进程停止后清理遗留socket再启动。

固定组合 `2f292a3926da2f171593ea693d5f7475eb526049` 与目录／原生环境前置 `7193eb838ba088c0d0e3e138770c50d5755317a9` 已完成真实A/B读取、A→B→A、重启／累计用量／关闭／停用／到期边界及合法新增C不改变旧A/B会话、授权和用量；相关说明见 PR #5552。该证据来自受控原生CLI消费者，没有以空目录或mock代签。后续真实外部模型使用由#5538/#5539承接，当前仍blocked；本只读前置不声明外部模型或世界写入通过。合入真实main后的本次检查只证明代码整合，不改写既有受测source/build组合。

运行中的策略仅通过可信运营者命令更新：`oasis7_dev_test_sessions --apply PRIVATE_CONFIG CANDIDATE_CONFIG`、`--disable-account PRIVATE_CONFIG ACCOUNT_REF`、`--enable-account PRIVATE_CONFIG ACCOUNT_REF`。命令与只读worker共用控制/准入锁；账号状态持久化到目录，策略及其授权版本先持久化后报告成功。变更授权范围或重新启用递增版本使旧会话失效，新增无关grant不改旧grant版本。启用账号不会自动启用其旧授权。不得绕过这些命令直接编辑运行中的策略或停用状态；普通账号登记/别名变更继续用既有目录入口，目录写入共用catalog.lock。

本服务显式限制同时在途worker为1。每请求有绝对总应答deadline，控制锁等待也受该deadline约束；下游超时报告unknown。已进入原生只读Client的请求不能伪称已取消，它在可信worker内完成或按原生超时结束前仍占用唯一名额并持有控制锁；避免重复新调用及后台worker无界积累。身份复核后、业务准入前再次核对当前时间，禁止在已过期的本次期限后新准入。控制命令等待该已准入边界结束后才报告停用完成，不承诺取消已在途的合法读取。真实有限并发和超时还需在实际A/B世界组合复核。

策略中的grant_generation_high_water由可信管理入口维护并持久保存，包括已移除的grant_ref；不能删掉该历史。移除后重新添加同名授权必须采用更高代数，旧session_ref不会复活。executor_config_digest是可信宿主为执行器配置原始bytes固定的BLAKE3摘要；读取从同一受信fd取得bytes后核对pin，完整配置（含task fence）变化立即拒绝，管理员更新pin时升授权版本并建立新会话。不可把同resource名字仍相同视为原资源合同仍相同。

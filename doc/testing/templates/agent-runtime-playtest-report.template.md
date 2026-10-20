# 自测报告：事实、判定和修复分开

- 使用入口：[开发自测操作手册](../manual/agent-runtime-self-playtest.manual.md)；配套派工：[试玩任务模板](agent-runtime-playtester-task.template.md)。

这是本轮测试 artifact 的字段约定，不是仓库任务准入、进度数据库或新的 PR 放行证书。复用已有 L4 输出目录及当前 PR，格式可用 JSON 或 Markdown；必须包含相同事实，不强制为普通任务另建报告文件链。

## 运行身份与覆盖

- run/attempt 与原失败或复测引用；source HEAD、未提交 patch hash（有则记录）、实际 build/Skill hash、Runtime/driver/profile/API 版本。
- 实际子会话引用、是否真的创建独立 subagent、`attached_codex_subagent / supervised_runtime / scripted_probe / main_agent_manual`。
- `cold_entry / prepared_world`、`autonomous / regression_replay`、`assisted=true/false`、`isolation=enforced / instruction_only / source_visible`，以及为何降级。
- 获准 world/branch/Agent/task、初始条件和信任来源、目标与范围、有效授权引用；不要保存 secret。
- AC-EXT-018 另记录入场前 Agent/task/goal/grant 预置清单、各阶段实际执行者与 operation/receipt、稳定主体/角色/executor 关系、必要确认及恢复停点；无对应证据不得把 prepared-world 标成冷入场。共存场景分别记录自有/受托权限来源、隔离与定域撤销结果。
- 实际表面 `game_api / web / native`、平台/build、选中和未选中的场景、开始/结束/停止原因。

<a id="report-source-scene"></a>
## 来源到场景记录（主 Agent / 核验器维护）

承接 [SC-15](../prd.md#qa-infrastructure-source-scene-method) 与[系统设计字段](../governance/agent-runtime-self-playtest.design.md#selftest-source-scene)。按每个选中条款 × scene/check_kind/cell/specimen/phase × Runtime/profile/入口 × tier 填写，公共字段可引用同一候选记录。试玩者仅收到其观察/操作字段；不将隐藏 oracle、完整开发报告或源码交给黑盒子任务。

| 字段组 | 必填内容 |
| --- | --- |
| source_clause | repository/path/fragment/clause_id、不可变 source_oid、blob_oid、content_sha256（对应原文件字节）；禁止用 main/latest 替代固定来源 |
| candidate | source_head/base_oid/execution_target/tested_tree_oid、适用 event_commit_oid、dirty_patch_digest（有则必填）、实际 build/Skill digest；source commit 与被测 tree 不混用 |
| selection | scope=targeted/ac_ext_010_full、scenario_id、check_kind=contract/live、obligation_id、cell/specimen_id、phase、applicability、applicability_owner/basis；未选范围仍保留 gap |
| execution | execution_state=planned/located/implemented/executed、access_mode、execution_lane、surface、平台；原 world/branch/Agent/task、root/revision/window/配置、基线与有序 phase observations |
| environment | dependency/topology、Runtime/driver/profile、provider kind/model/脱敏 endpoint route、预检来源；不同 profile 和真实/fake 分开 |
| tier_result | 单一 test_tier_required 或 test_tier_full、独立 evidence_level、tier_verdict、failure_signature/blocking_reason、retained_gap/gap_owner/next_step |
| artifacts | 受控原始证据 locator、byte_length、digest_algorithm/digest、collector/collected_at、integrity_check；脱敏副本单独记其真实字节身份 |
| oracle_results | 实际命令/返回或用户可见操作；结构、canonical、领域、信任、身份、join 断言与结果，不适用的断言附 owner/basis |

字段未知写明确缺口，不能伪造摘要或默填通过。执行状态和 verdict 正交；本报告 pass/fail 对应 QA passed/failed，inconclusive 对应 unverified，blocked 保持 blocked；not_applicable 需 owner/basis 且不计成功。只有结构证据时被测产品最多 shape_verified，不是 executed/passed；文件哈希不是世界回执或最终性证明。该方法本身不产生 L5。

<a id="report-first-session"></a>
## AC-EXT-010 生命周期覆盖

按[联合验收清单](../manual/agent-runtime-self-playtest.manual.md#first-session-lifecycle)逐 Runtime/profile/声明入口保留 entry、first_product、blocked_continue、goal_applied、leave_revisit、disconnect_resume、runtime_restart_resume。每项包含 phase/specimen、实际操作、world/Agent/task lineage、goal/fence、operation/receipt、ArtifactRef、oracle 和结果；恢复后的 root 可以变化但须可追踪，同一次完整组合不可混合候选/世界/任务。

`ac_ext_010_full` 须分别收齐 OpenClaw、Codex 的全部项；`targeted` 仅报告所选子项，显式保留整条 AC 未验证。只首产物、少一个 Runtime、把断连当重启、拼接不同任务或以另一个入口代签均不得报告整条 AC passed。scope pass 不能替代 parent AC 的全量判定；原失败不能被局部复测覆盖。

## 每个场景

| 字段 | 要求 |
| --- | --- |
| case_id / requirement | 复用已有场景、obligation/phase 与固定 SourceClauseRef；不只保存可变链接 |
| execution_state / tier | planned/located/implemented/executed 与 verdict 独立；每项单独标 test_tier 和 evidence_level |
| verdict | pass、fail、blocked、inconclusive、not_applicable |
| expected / observed | 预先确定的目标和实际发现；不临时降低判据 |
| steps | 按真实发生的顺序记录，区分玩家操作、Agent 提案和 setup 故障注入 |
| evidence | 关联 ArtifactRef 的实际 locator/byte_length/digest、采集者/时间和对象身份，回读后判完整性；没有证据不写通过 |
| oracle | 谁用哪个权威结果判定；Tester 自报只能是 reported |
| unmet / next | retained_gap/gap_owner/next_step 与失败签名或阻断原因；未覆盖条款/阶段不删除，不用“暂无问题”代替未测试 |

只有已执行且 oracle 证明要求满足才是 pass；已执行且违反要求是 fail；前置缺失是 blocked；预算到限/模型策略未成功/证据不足通常是 inconclusive。合法拒绝若正是负例目标，可以是该负例通过，不代表正常首局已通过。

## 问题条目

每个 finding 包含 `category、severity、scenario、目标版本、复现步骤、预期/实际、发生次数/尝试数、operation/receipt/截图、影响范围、诊断置信度、修复建议`。

category 使用 confirmed_bug、suspected_bug、agent_strategy_failure、expected_rejection、environment_blocker、harness_fault、usability_observation。tester 可以给 suspected_bug；confirmed_bug 需要主 Agent 或核验器的复现/事实依据。模型对“好玩/难懂”的评价保留原场景观察，不转换为玩家留存指标。

业务完成的证据必须和原 world/Agent/task、source/build、请求/回执关联。不能提交其他任务 receipt、手工改造成功 JSON、从本机日志伪造世界状态或用旧截图充新版本。

## 消耗、修复和清理

保留总授权预算、已报告 token/费用及单位、估计/预留与未知部分、预算可强制范围，以及子任务和实际受测 Runtime 的分项。缺失计量不当作零，不跨货币隐式汇总，不重复累加父子调用或累计事件。

修复记录指向原 finding、普通回归源/命令/旧失败与新通过、新 source/build 和复测 attempt；不能用重跑一次成功删除原失败。暂不能复现或未复测分别说明，不能标已闭合。

清理记录区分测试 executor 撤销、原 pending 对账、worker/浏览器/subagent/私有世界的实际退出。失败保留明确资源引用，不记录秘密；不能只因为子线程返回就宣称后台进程已停。

## 总结给开发负责人

```text
候选和受测 Runtime/profile：
实际派工、隔离与启动模式：
所选条款及 source/base/tested-tree：
本轮真实执行、test tier/evidence level 与范围内 verdict：
AC-EXT-010 各 Runtime 生命周期项及保留缺口：
证据字节回读与分项 oracle：
已确认问题与证据：
策略/环境/框架问题及体验观察：
修复、普通回归和独立复测：
消耗（已知/估计/未知）和停止原因：
未执行、未证明或需要其他 surface 验证的部分：
自有资源清理结果：
```

汇总 scope pass 要求所有选中场景都有完成证据且无 blocked/inconclusive；未选项不算通过。API、不同 Runtime、Web/native 和 L4B/L5 各保留自身边界，不用总分消除缺项。

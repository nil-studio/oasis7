# testing/governance 专题入口

本目录收拢 testing 的质量门禁、证据层级、playability 评审与测试治理专题。它只负责按问题分流和声明专题 authority；不要把专题规则、操作步骤或历史结论再复制到模块根入口。

## 首读路径

| 要回答的问题 | Canonical 入口 | Authority 边界 |
| --- | --- | --- |
| 自动化、agent、外部信号各能证明什么；何时可以声称“好玩” | [`../prd.md`](../prd.md) 的 Durable Playability Evidence Governance | 跨层证据栈、`go/watch/hold/block` 组合规则与高层 claim 边界 |
| `L4A synthetic`、`L4B embodied agent`、`L5` 如何区分，以及如何实际执行 | [`../../../testing-manual.md`](../../../testing-manual.md) 的 L4A/L4B/L5 分层 | operator 入口、可选内部真人校准与非替代边界 |
| 开发 Codex 如何派试玩 subagent 读玩家 Skill、实际游玩并形成修复回归 | [自主试玩与开发自测设计](agent-runtime-self-playtest.design.md)；[执行手册](../manual/agent-runtime-self-playtest.manual.md) | 承接已有 L4B；区分固定探针、自主试玩、权威验真与有界修复，不新增通用门禁 |
| 标准角色与 persona 如何组成内部评审 | [`../prd.md`](../prd.md) 的 Durable Playability Evidence Governance | role/persona card contract、触发、hand-off 与 stop conditions；persona 不是正式角色 |
| 质量趋势指标如何定义；如何定位不同窗口的样本与结论 | `testing-quality-trend-tracking-2026-03-11.prd.md` 定义口径；`../evidence/testing-quality-trend-baseline-2026-03-11.md` 仅保存其注明窗口的历史样本 | PRD 负责公式、阈值与采集边界；[当前阈值采用缺口](testing-quality-trend-tracking-2026-03-11.prd.md#current-threshold-status)尚未关闭，历史阈值不自动沿用；新窗口另存原始报告，由对应 PR/Task 定位，不覆盖旧窗口或将历史样本标为当前基线，不另设角色 handoff 文档 |
| release-gate 指标、质量趋势、审计检查或确定性 guard | `doc/testing/prd.index.md` 的 `governance/` 专题表 | 各专项 topic 的精确文件检索；不替代以上 playability authority |

## 与相邻入口的职责

- `doc/testing/README.md`：testing 模块首读与子域选择；涉及 testing governance 时只路由到本页。
- `doc/testing/prd.index.md`：文件级精确检索和三件套可达性；本页不复制其完整长表。
- `doc/testing/prd.md` / PR、实际 CI 与评审记录（Issue 按需）：模块测试门禁基线与当前执行状态；本页不承载模块状态。
- `testing-manual.md` 与 `doc/testing/manual/*.manual.md`：operator 操作步骤；本页只把需要操作的读者导向对应 manual，不重述步骤。

## 维护规则

- 新增或退役 `governance/` 专题时：更新 `doc/testing/prd.index.md` 的文件级可达性；若其改变首读问题或 canonical authority，同时更新本页的一行分流。
- 专题正文、证据和历史留痕仍留在其专业 authority、Git history、PR 和实际验证记录；本页不是新的规则正文或状态汇总面。
- `L4A`、`L4B`、`L5` 的语义、角色越权边界与 release/claim 结论以 `doc/testing/prd.md` 和 `testing-manual.md` 为准，导航文字不得覆盖它们。

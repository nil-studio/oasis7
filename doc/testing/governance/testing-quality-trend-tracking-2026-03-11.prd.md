# oasis7: 测试质量趋势跟踪（2026-03-11）

- 对应设计文档: `doc/testing/governance/testing-quality-trend-tracking-2026-03-11.design.md`
- 可变任务状态与历史: PR、实际 CI 与评审记录（Issue 按需）

审计轮次: 4

本文维护指标与采样方法。`TASK-TESTING-*`、TQT 编号和首份 baseline 仅定位原始专题及样本，不恢复历史任务或强制所有变更补交趋势材料。一次窗口的原始结果保留其身份与结论，后续报告不覆盖旧窗口；当前执行安排由实际 PR/Task 承载。

<a id="current-threshold-status"></a>
## 当前阈值采用状态与未关闭项

固定审读基线 `b5476a616dd3d4dbc29f332ee42ed848903c1f60` 下，本 PRD、配对设计与历史 baseline 尚未给出当前获采用的阈值版本或明确继续采用历史表的决定。SC-2 / AC-1 对阈值的要求保留为未满足项，不能把“由 PRD 负责”理解为当前阈值已齐备。

历史 baseline 的 `80%/50%`、`20%/40%`、`1d/3d` 只解释原窗口；其中百分比分段未说明非整数比例的边界，不能自行补齐后作为当前规则。当前窗口可以按既有公式报告数值和采样边界，但在 QA 明确采纳或修订阈值、记录版本及完整边界前，红黄绿判定仍为 `unknown`，不得继承旧窗口的 `green`。采用决定还须同步配对设计、入口说明与新窗口报告；本次只披露缺口，不选择新阈值或新增发布门禁。

## 1. Executive Summary
- Problem Statement: `TASK-TESTING-002/003` 已经定义了“该跑什么、证据怎么存”，但 testing 模块仍缺一个长期趋势入口来回答“质量是在变好还是变差”。如果没有趋势维度，发布评审只能看单次结果，无法识别首次通过率下滑、缺陷在阶段内后移或修复速度变慢。
- Proposed Solution: 建立 testing 质量趋势专题，统一定义 `首次通过率`、`阶段内逃逸率`、`修复时长` 的口径、采集源、红黄绿阈值与基线记录方式，并沉淀首份 baseline。
- Success Criteria:
  - SC-1: `TASK-TESTING-004` 具备唯一稳定文档入口，并可追溯到 PR、实际 CI 与评审记录（Issue 按需）。
  - SC-2: 三项指标均有明确公式、数据源、统计窗口与红黄绿阈值。
  - SC-3: 首份 baseline 至少覆盖 3 个近期 testing 证据样本，并给出可审计结论。
  - SC-4: 趋势记录可被 `qa_engineer` 周期性续写，不依赖口头解释。

<a name="2-user-experience-functionality"></a>

## 2. User Experience & Functionality
- User Personas:
  - `qa_engineer`：需要一个固定的趋势口径持续判断测试治理是否在变好。
  - `producer_system_designer`：需要快速看懂当前质量是“单次通过”还是“靠返工收口”。
  - 模块 owner：需要知道自己的任务是首次通过不足，还是问题逃逸偏高，还是修复过慢。
- User Scenarios & Frequency:
  - 形成趋势报告时：按明确的统计窗口和采样范围读取已收口任务的实际证据。
  - 阶段收口前：读取近一轮 baseline 判断是否需要补 QA 或降级结论。
  - 缺陷复盘时：按逃逸样本回查问题发现阶段与修复时长。
- User Stories:
  - PRD-TESTING-TREND-001: As a `qa_engineer`, I want a canonical trend metric set, so that I can compare test quality across recent closures.
  - PRD-TESTING-TREND-002: As a `producer_system_designer`, I want a baseline report with traffic-light thresholds, so that I can judge whether current stage quality is improving.
  - PRD-TESTING-TREND-003: As a 模块 owner, I want each trend sample linked back to evidence docs, so that I can reproduce why a metric went red.
- Critical User Flows:
  1. `任务收口 -> 读取 evidence / PR、实际 CI 与评审记录（Issue 按需） -> 归档为一个 trend sample`
  2. `计算首次通过率 / 阶段内逃逸率 / 修复时长 -> 输出红黄绿状态 -> 回写 baseline`
  3. `阶段评审读取 baseline -> 追溯异常样本 -> 决定是否加测或阻断`
- Functional Specification Matrix:
| 功能点 | 字段定义 | 按钮/动作行为 | 状态转换 | 排序/计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| trend sample 归档 | `sample_id`、模块、任务、证据入口、首次结论、最终结论、发现日期、关闭日期 | 对选入统计窗口的样本保留一行记录 | `identified -> measured -> baselined` | 按关闭日期倒序 | `qa_engineer` 写，其他角色只读 |
| 首次通过率 | `first_pass_count / total_samples` | 统计窗口内自动按样本计数 | `unknown -> green/yellow/red` | 仅把 `首次结论=pass` 计为通过 | `qa_engineer` 维护阈值 |
| 阶段内逃逸率 | `escaped_samples / total_samples` | 统计“问题是否在下游 QA/可用性/复验阶段才被发现” | `unknown -> green/yellow/red` | 样本带 `escape_stage` 即计入逃逸 | `qa_engineer` 与 `producer_system_designer` 联审语义 |
| 修复时长 | `closed_at - detected_at` | 记录并计算均值/中位数 | `unknown -> green/yellow/red` | 以自然日计，same-day 记为 `0d` | `qa_engineer` 维护 |
| baseline 报告 | 窗口、样本数、指标值、阈值、样本明细、风险结论 | 形成阶段质量快照 | `draft -> reviewed -> published` | 默认最近 7 日窗口 | `qa_engineer` 发布，`producer_system_designer` 消费 |
- Acceptance Criteria:
  - AC-1: 新专题文档明确三项指标定义、阈值与采集源。
  - AC-2: `doc/testing/evidence/testing-quality-trend-baseline-2026-03-11.md` 给出首份 baseline，至少 3 个样本。
- AC-3: 每个样本都能回溯到已有 evidence 文档、PR、实际 CI 与评审记录（Issue 按需） 或 git history。
- AC-4: 保留 `TASK-TESTING-004` 与专题链接作为原始交付来源；后续统计记录在实际 PR、CI 与评审中（Issue 按需），不要求恢复原任务。
- Non-Goals:
  - 不在本轮实现自动采集脚本或可视化面板。
  - 不把“阶段内逃逸率”直接等同于线上生产逃逸率。
  - 不改动业务模块测试代码。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用。
- Evaluation Strategy: 不适用。

## 4. Technical Specifications
- Architecture Overview: 本专题以现有 `testing` 证据包、专题项目文档、PR、实际 CI 与评审记录（Issue 按需） 与 git history 为数据源；`qa_engineer` 手工整理 trend samples，按固定公式生成 baseline 报告。
- Integration Points:
  - `doc/testing/evidence/release-evidence-bundle-task-game-018-2026-03-10.md`
  - `doc/world-runtime/evidence/runtime-storage-gate-sample-2026-03-10.md`
  - `doc/testing/evidence/testing-quality-trend-baseline-2026-03-11.md`
  - 退役 launcher 可用性样本的 PR、实际 CI、评审记录（Issue 按需）及 Git history
- Edge Cases & Error Handling:
  - 样本只有最终结论、缺失首次结论：标记 `incomplete_sample`，不得纳入首次通过率分子。
  - 问题在同日发现并修复：修复时长记 `0d`，但仍计入逃逸率。
  - 单条任务同时涉及多个模块：以 testing 证据包中的主模块归属统计，并在备注列记录联动模块。
  - 尚无线上数据：阶段内逃逸率仅统计“任务在下游 QA / 复验阶段才暴露的问题”，避免误称生产逃逸。
- Non-Functional Requirements:
  - NFR-TREND-1: 每个样本必须可追溯到仓内可打开的文档路径。
  - NFR-TREND-2: 指标公式必须用一句话复算，不依赖隐式上下文。
  - NFR-TREND-3: baseline 文档长度仅作编辑参考，按统计窗口、语义与维护成本决定是否拆分；不因超过 1000 行阻断或要求长度豁免，不压成长行规避提示。适用文档结构与引用检查仍须通过。
  - NFR-TREND-4: 阈值一旦调整，必须在 baseline 中显式记录。
- Security & Privacy: 仅使用仓内测试证据与任务文档，不新增敏感数据。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP (`TQT-1`): 建立专题 PRD / Design / Project 与指标口径。
  - v1.1 (`TQT-2`): 生成首份 baseline，冻结样本字段与阈值。
  - v2.0 (`TQT-3`): 按选定窗口生成可比较趋势报告，供阶段评审使用；实际频率和安排由当前任务承载。
- Technical Risks:
  - 风险-1: 当前样本数偏少，指标更适合做 baseline 而非长期统计结论。
  - 风险-2: 不同 evidence 文档对“首次失败”的记法不一致，初期仍依赖 QA 人工判定。
  - 风险-3: 若后续新增自动脚本但不回写文档，趋势台账会再次漂移。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-TESTING-TREND-001 | `TASK-TESTING-004` / `TQT-1` | `test_tier_required` | `rg` 检查指标定义、阈值与样本字段 | testing 治理口径一致性 |
| PRD-TESTING-TREND-002 | `TASK-TESTING-004` / `TQT-2` | `test_tier_required` | 检查 baseline 文件与样本数、结论状态 | 阶段评审质量可读性 |
| PRD-TESTING-TREND-003 | `TASK-TESTING-004` / `TQT-2/3` | `test_tier_required` | 抽样验证 baseline 样本路径可达 | evidence 可追溯性 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| `DEC-TREND-001` | 先做手工 baseline 文档 | 直接上自动 dashboard | 当前样本来源已在仓内，先统一口径比先写脚本更稳。 |
| `DEC-TREND-002` | 使用“阶段内逃逸率” | 直接宣称生产逃逸率 | 当前仓内仅有阶段测试/复验证据，不足以代表线上生产。 |
| `DEC-TREND-003` | 同时保留“首次通过率”和“最终收口率” | 只记录最终 pass/fail | 最终收口率会掩盖返工成本，无法体现 QA 下游发现问题的压力。 |

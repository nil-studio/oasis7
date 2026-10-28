# oasis7: README 季度口径审查与修复节奏（2026-03-11）

- 执行模板: `doc/readme/governance/readme-quarterly-review-template-2026-03-11.md`
- 修复模板: `doc/readme/governance/readme-remediation-log-template-2026-03-11.md`
- 历史设计与完成态项目包装已退役；实施追溯使用 Git history、PR 与实际验证记录。

审计轮次: 4

## 1. Executive Summary
- Problem Statement: README 口径治理需要同时具备可复用的人工判断项、最小自动检查，以及“多久复查一次、谁触发、发现问题怎么回收”的固定节奏。
- Proposed Solution: 以季度审查模板承接人工巡检项，以 `scripts/readme-link-check.sh` 承接自动链接检查，并在本专题定义季度触发、角色分工、固定输入输出与修复闭环。
- Success Criteria:
  - SC-1: 季度审查节奏明确 `谁/何时/检查什么/如何回写`。
  - SC-2: 至少包含季度审查模板与修复记录模板。
  - SC-3: 审查与修复结论可从实际 PR 或已有 Task 追溯；模块 PRD 不维护任务完成态。
  - SC-4: 后续 readme 模块可按该节奏持续执行，而不再临时约定。

## 2. User Experience & Functionality
- User Personas:
  - `producer_system_designer`：需要一个稳定节奏保证对外入口口径不过时。
  - `qa_engineer`：需要知道何时复跑清单与自动检查。
  - 文档维护者：需要固定模板记录问题与修复。
- User Scenarios & Frequency:
  - 季度起始：执行一次完整入口审查。
  - 季度中重大口径变化：触发临时加审。
  - 季度结束：复盘修复项是否收口。
- User Stories:
  - PRD-README-CYCLE-001: As a `producer_system_designer`, I want a quarterly review cadence, so that README governance stays proactive.
  - PRD-README-CYCLE-002: As a `qa_engineer`, I want a fixed review template, so that each cycle records comparable evidence.
  - PRD-README-CYCLE-003: As a 文档维护者, I want a remediation template, so that drift findings become actionable tasks.
- Critical User Flows:
  1. `进入季度窗口 -> 执行人工清单 + 自动链接脚本 -> 记录问题`
  2. `发现问题 -> 在实际 PR 或已有 Task 记录修复项 -> 跟踪关闭`
  3. `季度结束 -> 复盘修复结果 -> 形成下季度风险输入`
- Functional Specification Matrix:
| 功能点 | 字段定义 | 按钮/动作行为 | 状态转换 | 排序/计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 季度审查节奏 | 季度编号、触发时间、owner、联审角色、输入 | 到期或重大变更时触发 | `planned -> running -> closed` | 正常按季度执行，重大变化可插队 | owner 负责发起 |
| 审查模板 | 覆盖对象、检查项、结论、风险、回写文件 | 逐项填写 | `draft -> reviewed -> archived` | 高优项先检查 | `producer_system_designer` / `qa_engineer` 共用 |
| 修复模板 | 问题ID、影响范围、责任人、截止时间、关闭证据 | 审查失败后立刻建项 | `open -> fixing -> closed` | 断链 / 状态口径 > 术语优化 | owner 裁定优先级 |
- Acceptance Criteria:
  - AC-1: 新专题明确季度节奏、触发条件与角色分工。
  - AC-2: 至少产出一份季度审查模板与一份修复记录模板。
  - AC-3: 模板内置人工巡检项并引用 `scripts/readme-link-check.sh`。
  - AC-4: 实际 PR 或已有 Task 记录本轮结论、修复项与关闭证据；长期文档只同步受影响的规则和入口。
- Non-Goals:
  - 不在本轮执行真实季度审查。
  - 不引入新的自动化服务。
  - 不统计趋势指标。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 人工清单 + `scripts/readme-link-check.sh`。
- Evaluation Strategy: 不适用。

## 4. Technical Specifications
- Architecture Overview: 本专题把 readme 治理从“单次任务”提升为“周期性审查流程”，复用现有人工清单与自动脚本作为执行输入。
- Integration Points:
  - `scripts/readme-link-check.sh`
  - `doc/readme/governance/readme-quarterly-review-template-2026-03-11.md`
  - `doc/readme/governance/readme-remediation-log-template-2026-03-11.md`
- Edge Cases & Error Handling:
  - 季度中发生重大状态变化：允许触发临时加审，不等待季度末。
  - 自动检查通过但人工口径失败：仍判定为 `fix_required`。
  - 修复项跨模块：发起方在模板中指定 owner，并回写 handoff。
- Non-Functional Requirements:
  - NFR-RQ-1: 审查模板应在 15 分钟内完成一次最小执行。
  - NFR-RQ-2: 修复模板必须支持问题分级与截止时间。
  - NFR-RQ-3: 节奏文档需可被 grep 快速检索。
- Security & Privacy: 模板不记录敏感凭据，仅记录路径、问题和结论。

## 5. Risks & Roadmap
以下阶段与第 6 节的 `TASK-README-004` / `RQ-1` 标识只保留原建档来源，不恢复排期或据此认定完成；当前执行遵循[开发流程规范](../../engineering/workflow/source-of-truth.md#information-record-placement)。

- Phased Rollout:
  - MVP (`RQ-1`): 建立季度节奏与模板。
  - v1.1 (`RQ-2`): 首轮真实季度审查执行。
  - v2.0 (`RQ-3`): 与趋势指标联动。
- Technical Risks:
  - 风险-1: 如果模板过重，会降低执行率。
  - 风险-2: 跨模块修复若无 owner，节奏会停在文档层。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-README-CYCLE-001 | `TASK-README-004` / `RQ-1` | `test_tier_required` | 检查季度节奏字段与角色分工 | readme 治理节奏稳定性 |
| PRD-README-CYCLE-002 | `TASK-README-004` / `RQ-1` | `test_tier_required` | 检查审查模板与输入项引用 | QA / 文档协作一致性 |
| PRD-README-CYCLE-003 | `TASK-README-004` / `RQ-1` | `test_tier_required` | 检查修复模板字段与实际 PR/已有 Task 的关闭证据 | 问题闭环能力 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| `DEC-RQ-001` | 先固定季度模板和节奏 | 直接执行一次临时审查 | 没有模板时，执行结果难复用。 |
| `DEC-RQ-002` | 重大状态变化允许加审 | 严格只按季度执行 | 状态口径问题不能等下个季度。 |
| `DEC-RQ-003` | 修复项单独建模板 | 只在 devlog 记录 | 季度问题需要可追踪的正式闭环。 |

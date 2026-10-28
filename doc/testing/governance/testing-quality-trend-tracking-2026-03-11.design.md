# oasis7: 测试质量趋势跟踪（2026-03-11）设计

- 对应需求文档: `doc/testing/governance/testing-quality-trend-tracking-2026-03-11.prd.md`
- 可变任务状态与历史: PR、实际 CI 与评审记录（Issue 按需）

审计轮次: 4

本文维护指标与采样方法。`TASK-TESTING-*`、TQT 编号和首份 baseline 仅定位原始专题及样本，不恢复历史任务，也不要求所有变更补交趋势材料。一次窗口的原始结果保留其身份与结论，后续报告不覆盖旧窗口；当前执行安排由实际 PR/Task 承载。

## 1. 设计定位
定义 testing 模块的质量趋势统计结构，统一样本字段、计算公式、阈值分级与 baseline 发布方式。

<a id="audit-method"></a>
## 2. 设计结构

产品适用性：N/A；reason=本节定义内部测试质量样本与统计报告，不定义玩家行为、游戏体验或发布结论；scope=三项趋势指标的采样、计算与原始窗口留存，保留专题专业验收。
- 样本层：从 evidence / project / devlog 抽取 `sample_id`、首次结论、最终结论、发现/关闭日期。
- 指标层：计算 `首次通过率`、`阶段内逃逸率`、`平均修复时长`。
- 报告层：输出 baseline Markdown，附窗口、样本明细、风险结论与下一步建议；`producer_system_designer` 等消费角色直接读取该报告，具体阶段评审交接留在 PR、实际 CI 与评审记录（Issue 按需）。

## 3. 关键接口 / 入口
- 专题 PRD：`doc/testing/governance/testing-quality-trend-tracking-2026-03-11.prd.md`
- 专题任务状态与历史：PR、实际 CI 与评审记录（Issue 按需）
- baseline 报告：`doc/testing/evidence/testing-quality-trend-baseline-2026-03-11.md`

## 4. 约束与边界
- 只统计仓内可追溯的测试/复验样本，不补写不可验证的历史数据。
- baseline 允许小样本，但必须显式写出样本数与统计窗口。
- 线上生产逃逸率不在本轮定义范围内。

## 5. 设计演进计划
- 先冻结样本字段与阈值。
- 再发布首份 baseline。
- 按实际需要选择统计窗口生成新的趋势报告；不由本设计建立每周任务或每个 PR 的回写义务。

### 2.1 需求承接与分配表

下表承接本专题既有专业验收，保持方法与实际运行分离；旧任务编号仅用于来源定位。

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [趋势指标与样本专业验收](testing-quality-trend-tracking-2026-03-11.prd.md#2-user-experience-functionality) | 选定窗口的三项指标、阈值、样本来源可复算；原 baseline 保留身份 | [审计/采样结构](#audit-method) | QA；专题 PRD §4/6 指定的原始输入和领域 owner | 不证明所有任务均被采样、线上生产逃逸率或任意发布放行 |

## 11. 验证设计与可追溯性

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [趋势指标与样本专业验收](testing-quality-trend-tracking-2026-03-11.prd.md#2-user-experience-functionality) | [审计/采样结构](#audit-method) | 选定窗口的三项指标、阈值、样本来源可复算；原 baseline 保留身份 | 专题 PRD §6 验证入口；原始结果按[主手册证据规范](../../../testing-manual.md#证据规范)保存。按专题 PRD §6 的 test_tier_required，从所选报告回读每条样本的原 evidence/PR/git history，复算首次通过率、阶段内逃逸率和修复时长，核对窗口/样本数/阈值版本；原首份 baseline 的至少 3 样本要求只验证原报告 | 本次报告、样本来源与复算结果；原 baseline 保留不覆盖 | 不证明所有任务均被采样、线上生产逃逸率或任意发布放行；本次文档审读没有执行该审计/量测 |

## 范围、结构边界与风险

本设计范围是选定窗口的内部质量统计方法，结构由样本、指标和报告三层组成。样本选择偏差、小样本和阈值变化会影响解释；保留采样范围、原始来源与阈值版本，不把首份 baseline 或旧任务状态当作当前质量结论。

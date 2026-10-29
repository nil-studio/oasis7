# 可玩性发布证据包模板

审计轮次: 5

## 目的
- 为 `TASK-PLAYABILITY_TEST_RESULT-004` 提供可直接接入发布门禁的可玩性证据包格式。
- 对齐 `doc/testing/templates/release-evidence-bundle-template.md`，使可玩性证据可直接用于阶段收口 go/no-go 评审。

## 使用说明
- 每个版本候选至少提交 1 份可玩性证据包。
- 证据包必须绑定：版本 / 候选、来源卡片、问题闭环记录、结论标签。
- 若存在 `高优先级阻断` 问题但无豁免记录，结论不得为 `pass`。

## 模板
### Meta
- 发布候选 / 版本号:
- 证据包 ID:
- 日期:
- 汇总人:
- 关联 testing 证据包:
- 关联 core go/no-go 记录:
- 总结论: `pass` / `fail` / `blocked`

### 卡片覆盖范围
| 卡片 ID | 测试场景 | 测试者 | 结论标签 | 证据路径 |
| --- | --- | --- | --- | --- |
|  |  |  |  |  |

### 评分摘要

严格引用[卡片唯一评分合同](../playability_test_card.md#feedback-score-contract)，不要在证据包另定义公式。记录评分规则版本/修订、候选/模式/场景与来源分组、完整已评卡数、未知/排除卡数及原因；保留每张原始值，再展示同组描述性均值。

只对完整卡片求均值不表示缺失卡片已通过，历史卡和人测/AI 来源分开。按未舍入值判档；总评、附加阻断、玩家杠杆和最终发布结论分别填写。

| 维度 | 分数 | 说明 |
| --- | --- | --- |
| 理解度 |  |  |
| 控制感 |  |  |
| 策略体验 |  |  |
| 可理解性 |  |  |
| 节奏与总体体验 |  |  |
| 总评 |  |  |

### 玩家杠杆摘要
| 卡片 ID | `player_leverage_score` | `leverage verdict` | `world_activity_only` | 玩家做了什么 (`player_action`) | 世界因此变了什么 (`world_change_due_to_player`) | 是否打开新决策 (`opened_decision`) | 是否形成继续玩的理由 (`return_hook`) |
| --- | --- | --- | --- | --- | --- | --- | --- |
|  |  |  |  |  |  |  |  |

### 高优问题摘要
| Issue ID | 严重级 | 当前状态 | owner | 是否阻断发布 | 证据路径 |
| --- | --- | --- | --- | --- | --- |
|  |  |  |  |  |  |

### 关联测试 / 运行证据
| 类型 | 路径 | 说明 |
| --- | --- | --- |
| 截图 |  |  |
| 录屏 |  |  |
| console / semantic 结果 |  |  |
| 启动日志 |  |  |
| testing 证据包 |  |  |

### 结论摘要
- 继续可玩的主要依据：
- 玩家杠杆成立的代表性样本：
- 只有 world activity、尚未证明 player leverage 的样本：
- 需观察项：
- 高优先级阻断项：
- 豁免 / 例外：
- 建议结论：`pass` / `fail` / `blocked`

## 对接规则
- `pass`：无未豁免 `高优先级阻断`，且至少 1 条代表性样本明确回答“玩家做了什么、世界因此变了什么”，并给出 `leverage verdict=pass`。
- `fail`：存在未豁免 `高优先级阻断`，或代表性样本复测后仍只能给出 `world_activity_only=yes` / `leverage verdict=block`。
- `blocked`：卡片、Issue、testing 证据包或 `player leverage` 关键字段任一缺失。
- 若 testing 证据包为 `blocked`，则可玩性证据包最高只能为 `blocked`。

## 最小审查清单
- 是否绑定版本、testing 证据包与 core go/no-go 记录。
- 是否至少列出 1 张卡片和 1 个结论标签。
- 是否至少列出 1 条 `player leverage` 代表性样本，而不是只给 world delta / world activity。
- 是否列出所有高优问题及其当前状态。
- `pass/fail/blocked` 是否被卡片、Issue、截图 / 录屏 / console 路径直接支撑。
- 是否已把可变结论回写对应 GitHub task issue evidence comments，并把稳定字段回写本模块 PRD、testing manual 或 evidence 文档。

# 高优先级可玩性问题闭环模板

审计轮次: 5

## 目的
- 为 `TASK-PLAYABILITY_TEST_RESULT-003` 提供统一的高优问题闭环记录格式。
- 在同一张卡中记录发现、归因、责任人、修复、复测和发布影响，便于版本追踪与 go/no-go 引用。

## 使用说明
- 仅用于 `需观察` 或 `高优先级阻断` 中需持续跟进的问题。
- 每个问题必须绑定至少 1 个来源卡片、1 个 owner、1 条复测记录。
- 未完成复测时不得标记为 `closed`。

## 状态定义
| 状态 | 含义 | 允许人 |
| --- | --- | --- |
| `opened` | 已确认问题，待分派 | `qa_engineer` / `producer_system_designer` |
| `triaged` | 已完成分级与归因 | `qa_engineer` |
| `fixing` | owner 已接手修复 | 对应 owner |
| `verified` | 修复后已完成复测，待关闭 | `qa_engineer` |
| `closed` | 风险关闭，可进入版本对比 | `qa_engineer` / `producer_system_designer` |
| `waived` | 不修复但完成风险豁免 | `producer_system_designer` |

## 模板
### Meta
- Issue ID:
- 标题:
- 当前状态: `opened` / `triaged` / `fixing` / `verified` / `closed` / `waived`
- 严重级: `需观察` / `高优先级阻断`
- 来源版本 / 候选:
- 来源卡片:
- 发现日期:
- owner:
- 协作角色:

### 问题描述
- 玩家表征：
- 触发场景：
- 复现步骤：
- 预期行为：
- 实际行为：
- 影响面：

### 归因信息
- 评分维度：`理解度` / `控制感` / `策略体验` / `可理解性` / `节奏与总体体验`
- 归因标签：`system_control_gap` / `player_strategy_gap` / `variance` / `unclear`
- 证据路径：
- 关联日志 / 截图 / 录屏：

### 修复计划
- 修复任务 / PRD-ID:
- 负责人:
- 计划版本:
- 预计完成时间:
- 临时缓解措施:

### 复测记录
| 日期 | 执行人 | 复测命令 / 方法 | 证据路径 | 结果 |
| --- | --- | --- | --- | --- |
|  |  |  |  |  |

### 发布影响
- 是否阻断当前发布：`yes` / `no`
- 若不阻断，豁免单 / 决策记录：
- 下一复审时间：
- 结论说明：

## 最小审查清单
- 是否绑定来源卡片和版本。
- 是否填写 owner、修复任务、归因标签和证据路径。
- `verified/closed` 是否具备复测记录。
- `waived` 是否具备批准人与复审时间。
- 是否已同步回写对应模块 GitHub task issue evidence comments、对应 GitHub task issue evidence comments 与发布证据包。

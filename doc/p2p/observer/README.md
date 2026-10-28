# Observer 子域入口

## 从这里开始
- 想确认 observer 与 P2P/DistFS/consensus/execution 接起来后的整体闭环、测试层级与 claim boundary：先读 `testing-manual.md#s9a链上大世界状态底座自闭环`；本页只负责 Observer 子域入口。
- 想理解 Observer 同步源策略化的基线：先读 `observer-sync-source-mode.prd.md`。
- 想理解 DHT 组合链路差异：读 `observer-sync-source-mode.prd.md` 的 DHT 组合策略。
- 想查同步模式 metrics / runtime bridge / observability：读 `observer-sync-mode-runtime-metrics.prd.md`。

## 阅读面边界
- 本页只做 Observer 子域分流，不复制 `doc/p2p/prd.index.md` 的完整三件套长表。
- Observer 专题的 sync source、metrics、runtime bridge 或 observability green 结果只能作为链上大世界状态底座的观测/恢复证据；模块级闭环以 S9A 的 `module_required / module_full / integration_required / release_full` 分层为准。
- `observer-sync-source-mode` 已吸收 DHT 组合链路差异，是 source-mode 组唯一当前入口。
- `observer-sync-mode-runtime-metrics` 已吸收 runtime bridge 与 observability，是该组唯一当前入口。
- 完整文件级检索仍回到 `doc/p2p/prd.index.md`。

## 主从/增量组
| 组 | 默认入口 | 增量入口 |
| --- | --- | --- |
| Observer sync source mode | `observer-sync-source-mode.prd.md` | 已吸收 DHT 组合策略 |
| Observer sync mode metrics | `observer-sync-mode-runtime-metrics.prd.md` | 已吸收 runtime bridge 与 observability |

## 维护规则
- 新增 Observer 专题时，先判断其是否属于现有同步模式的增量差异；属于增量时只补本页和 `doc/p2p/prd.index.md` 的主从说明。
- 如果增量文档开始定义新的默认同步语义，应先提升为新的主入口，再更新本页分流。

# world-simulator PRD 分册：体验质量趋势跟踪

审计轮次: 6

## 目标与范围

跟踪 Web-first 闭环、LLM 链路和玩法覆盖的连续证据。本文只定义采集、计算和归档，不修改 runner、实现监控平台或增加 CI/发布门禁；正式结果继续受 S6/S8 和各玩法验收合同约束。

## 指标面板（v2）

每类指标取最近 7 次**已登记尝试**；不足 7 次显示实际 n 和 insufficient_data，不补齐旧目录样本。失败、提前退出、取消和证据缺失仍占本类尝试分母；重跑是新尝试，不能覆盖失败。不同代码/配置/运行模式的变化在趋势旁标记。

| 维度 | 指标与计算 | 目标/解释 | 真实来源 |
| --- | --- | --- | --- |
| 启动可用性 | `web_launch_success_rate` = primary 最终退出 0 且本次 `ok=true` 的次数 / primary 尝试数 | 最近 7 次 >=99% | primary wrapper 退出/日志及 `<primary-root>/<run-id>/summary.json` |
| 闭环稳定性 | `web_console_error_count` = 同次浏览器会话原始 console 中 error 数 | 每次为 0；缺日志为 unverified，不能填 0 | 独立 console dump、对应 session/URL/state |
| 推进或阻断诊断 | `web_realtime_or_blocker_pass_rate` = 下述条件通过次数 / software-safe 尝试数 | 最近 7 次 >=95%；显式 blocker 通过仅证明诊断可见 | `<software-root>/<run-id>/software-safe-step-summary.json` 和最终退出/日志 |
| 实际进展 | `web_progress_observed_rate` = `autoProgressObserved=true` 且本次检查成功次数 / software-safe 尝试数 | report-only，同时列 blocker 次数；不代表完整玩法通过 | 同上，辅以 before/after 状态 |
| LLM 稳定性 | `llm_metric_gate_pass_rate` = wrapper 最终退出 0 且要求的 gate 完整执行次数 / S8 尝试数 | 最近 7 次 >=95%；无判定来源不入通过分子 | wrapper stdout/stderr 和退出码；summary 存在不证明通过 |
| LLM 健康度 | `llm_parse_error_count` | 对照本次 `--max-parse-errors` 等实际参数；不虚构 report 中的 rate | `summary.txt: parse_errors`；report 对应 `trace_counts.parse_errors` |
| 玩法覆盖 | `llm_action_kind_count` | 对照本次 release-gate profile | `summary.txt: action_kinds_total/action_kind_counts` |

software-safe 通过条件：最终退出 0、本次 summary `ok=true`，且 `autoProgressObserved=true`，或 `blockerDomVisible=true` 并且 `stageStatus/blockerKind/blockerDetail` 与同次实际/预期阻断状态一致。不能仅把 `ok` 改名为成功率；原字段可包含脚本 step fallback 后的进展，不能据名称声称纯自然推进。

旧 `semantic_step_pass_rate` / `release-qa-summary-*.md` 已无当前 producer，不再作为趋势来源；v2 的推进/阻断指标与旧语义单步指标不拼接同一时间序列。旧 `llm_parse_error_rate` 也不由计数直接冒充；如需比率须另明确实际分母。

## 采集节奏

- 每日至少 1 次 S6，每周至少 1 次 S8；发布前连续 3 天样本且关键指标达标。
- 采样按已有环境、provider 和预算授权执行；本文的周期说明不是新建定时任务或真实 provider 调用许可。
- 每次先在现有趋势记录中登记 attempt ID、时间、候选提交/build、配置、命令和计划证据路径；结束后补退出码与结果。不能只从成功 summary 枚举尝试。

## 采集配方

每次使用唯一的 `run-id`。示例中的 `<run-id>` 必须替换为本次未使用的值，Web 两脚本还会在给定 root 下生成自己的运行子目录：

```bash
./scripts/viewer-primary-web-entry-regression.sh --out-dir output/quality-trend/<run-id>/primary
./scripts/viewer-software-safe-step-regression.sh --out-dir output/quality-trend/<run-id>/software
./scripts/llm-longrun-stress.sh --scenario llm_bootstrap --ticks 240 --release-gate --release-gate-profile hybrid --out-dir .tmp/llm_stress/<run-id>
```

分别保留三个 wrapper 的完整 stdout/stderr 与最终退出码；重定向日志放在趋势 attempt 目录，不放在会被 stress 初始化清理的 out-dir 中。使用管道时须保存 wrapper 的退出码，不能误用 tee 的成功状态。S8 人工执行判定按[证据模板](../acceptance/web-llm-evidence-template.md#模板-bllm-链路证据卡s8)填写。

这两条 Web 脚本不代替 console 采集。按[canonical S6 手册](../../../testing/manual/web-ui-agent-browser-closure-manual.manual.md)在本轮实际浏览器 session 关闭前独立采集：

```bash
agent-browser --session "$AB_SESSION" console > "$CONSOLE_LOG"
```

`AB_SESSION`、URL、状态快照和 `CONSOLE_LOG` 都绑定同次采集；不能在脚本关闭 session 后另开页面，将新会话日志归到旧结果。无法取得该次 console 时如实标 unverified。

## 工件与保留边界

- primary：`<primary-root>/<run-id>/summary.{json,md}`、状态、截图、launcher 日志；software-safe：`<software-root>/<run-id>/software-safe-step-summary.{json,md}`、before/after/final 状态及浏览器/launcher 日志。登记实际打印路径，不假定二者共用旧 `output/playwright/viewer/`。
- S8：`.tmp/llm_stress/<run-id>/{summary.txt,report.json,run.log}`；`run.log` 是 demo 输出，wrapper 阈值失败可能只在单独保留的 stdout/stderr 中。
- stress 默认会清理整个 `--out-dir`；归档树存在后不得再次运行默认 `.tmp/llm_stress` 根目录命令。`--keep-out-dir` 仍会覆盖固定同名产物，不保证多次保留。
- 长期证据按项目现有归档约定保存；临时目录名不保证持久保存。对照运行两侧各有独立目录，见 `testing-manual.md` S8。
- 离线配方验证只检查第二次独立输出不删除第一份、失败与缺样本均可回链，以及通过/已识别 blocker/提前失败/证据缺失四类计算；不冒充浏览器、provider 或发布验收。

## 趋势记录模板

```md
### world-simulator quality trend (YYYY-MM-DD)
- attempt_id / collected_at / candidate / build / mode / configuration:
- exact_commands / exit_codes / wrapper_logs:
- primary / software / console / S8 evidence paths:
- outcomes: passed / identified_blocker / early_failure / evidence_missing
- window: last 7 attempts; observed n:
- web_launch_success_rate / web_console_error_count:
- web_realtime_or_blocker_pass_rate / web_progress_observed_rate / blocker_count:
- llm_metric_gate_pass_rate / llm_parse_error_count / llm_action_kind_count:
- verdict: healthy / warning / fail / insufficient_data / unverified
- failures and later retry references:
```

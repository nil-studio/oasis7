# 开发方法试验的输入与结果

按 [Skill 主体](../SKILL.md) 准备对照。本参考说明离线工具的格式和证明范围，不新增日常开发义务。命令从仓库根执行，需要 Python 3；不安装模型 SDK，也不访问网络、执行 Git 或编译项目。

## 1. 列出与导出

```sh
python3 .agents/skills/evaluating-development-methods/scripts/trial.py list
python3 .agents/skills/evaluating-development-methods/scripts/trial.py prepare \
  --split pilot --repetitions 2 --out /tmp/oasis7-method-trial-pilot
```

`--out` 使用本次专用的新目录。`--split` 可为 `pilot`、`holdout`、`all`，默认 pilot；`--repetitions` 为 1–100 的整数，默认 1。探索先用 pilot，候选冻结后单独准备 holdout；`all` 不使已参与调优的样本重新成为留出样本。

内置套件是由仓库测试和规范提炼的公开合同 fixture，部分输入为合成数据，不是历史 hosted run 的回放或性能记录。holdout 只是预留划分，不具有答案保密性；执行者已读原套件、对应修复或答案时，不能据此声称独立留出验证。需要另一套固定任务时，将全局选项 `--cases FILE` 放在子命令之前，导出与比较使用同一套件。

导出包含 `plan.json`、`tasks/` 下的可见任务，以及 `results.template.json`。任务不包含评估侧的 `expected` 和 `provenance`；不要向执行者提供原套件、评估实现、修复提交或父会话中的答案。

计划字段为 `schema_version`、`suite_sha256`、`split`、`repetitions`、`case_ids`、每任务的 `task_sha256` 及 `plan_sha256`。固定计划后执行双方全部样本与重复次数，比较时不能再改 split 或筛掉失败。工具重建计划并核对本地套件，防止误用不同版本或缩小比较分母；摘要不证明计划在何时创建，也不证明实际执行。

在运行前把计划摘要、任务范围、H0/候选方法版本及预算写入当前 PR 或已有 Task。保存原计划、原结果和实际运行来源供复核；试验记录不成为额外审批或通用合入材料。

## 2. 填写结果

双方分别复制 `results.template.json`，填写真实的 `method_ref` 和 `context`，逐次追加 `runs`。模板的 `runs` 为空，context 中的 `null` 只是待填写占位；生成模板不表示执行过任何样本。结果的 `plan_sha256` 必须指向同一个固定计划，`schema_version` 为 `1`。

| `context` 字段 | 填写内容 |
| --- | --- |
| `model` | 非空文本，记录实际模型/provider 与可识别版本；不要只写“默认模型” |
| `model_settings` | 对象，记录实际推理及采样等设置 |
| `tools` | 非空文本，记录实际工具集合与版本 |
| `environment` | 非空文本，记录固定源码、输入、环境及隔离，包含缓存准备方法 |
| `memory_snapshot` | 非空文本，记录双方相同的隔离会话初始记忆快照或关闭设置 |
| `cache_state` | `cold`、`warm` 或 `not-applicable`，双方相同 |
| `budget` | 单次执行上限；`max_seconds` 为正数，`max_tokens` 为正整数；总试验预算另按实际授权核对 |

比较要求双方 `context` 完全相等；方法差异放在 `method_ref` 指向的固定版本中。声明相等不证明真实环境相等，仍需核对运行材料；改变模型或预算时另做试验，不把收益归因于方法。

| 每次 `runs` 记录 | 含义 |
| --- | --- |
| `case_id`、`attempt` | 固定计划中的样本及从 1 开始的重复序号；每对只记录一次 |
| `status` | `completed`、`failed`、`timeout` 或 `cancelled`，按实际执行填写 |
| `answer` | 完成时的答案对象，结构按导出任务；执行失败时可为 `null` |
| `wall_seconds` | 本次墙钟耗时，含其所覆盖的等待，说明统计边界 |
| `model_tokens` | 本次实际模型 token；多个调用和重试不可漏计 |
| `runner_seconds` | 本次执行资源耗时，不冒充用户等待时长 |
| `cost_usd` | 已核实的本次费用；未知不写成零 |
| `human_interventions` | 本次人工介入次数，双方使用相同口径 |
| `evidence` | 非空真实来源，能定位该次输出、执行记录和度量 |

度量为非负有限数，`model_tokens`、`human_interventions` 使用整数；缺失填 `null`，不推算未取得的 token 或费用。没有执行的 case/attempt 留为缺失，不虚构一条取消或失败记录补齐分母；已经执行的失败、超时、取消及重试必须保留。脚本不回读 `evidence`，填写者负责核对来源与实际版本。

## 3. 比较与解释

```sh
python3 .agents/skills/evaluating-development-methods/scripts/trial.py compare \
  --plan /tmp/oasis7-method-trial-pilot/plan.json \
  --baseline /tmp/oasis7-method-trial-pilot/baseline.json \
  --candidate /tmp/oasis7-method-trial-pilot/candidate.json
```

输出为 JSON，按固定计划中的全部样本和重复次数报告 `planned_pairs`、`coverage`、逐次 `pairs`、`outcomes` 和度量覆盖。按候选相对 H0 的结果判定：

| 配对结果 | 含义 |
| --- | --- |
| `win` / `loss` | 双方均在预算内，仅候选 / 仅 H0 完成且通过指定字段检查 |
| `tie` | 双方均在预算内，均完成且通过指定字段检查 |
| `neither_passed` | 双方均在预算内，均未通过；不算平局 |
| `inconclusive` | 至少一侧缺记录、超预算，或缺 `wall_seconds` / `model_tokens`，无法确认等预算比较 |

`budget` 分别统计 `within`、`exceeded`、`unknown`；`fixture_passes` 是字段通过数，可能包含超预算记录，不能单独当作有效胜出数。`both_pass_timing` 仅统计双方均通过且均在预算内的墙钟差；差值均为候选减 H0。

`metrics` 每项保留 `known_sum`、`observed`、`planned` 与 `total`，已知和包括失败尝试。覆盖不完整时 `total` 为 `null`；任一侧总量不完整时，相应 `candidate_minus_baseline` 也为 `null`，不能用已知和推断全部成本节省。

同时核对原始逐次输出，不只看汇总。质量相同但成本降低、正确性提高但耗时上升、覆盖不足等分别解释；局部比较明确其样本范围，不代替全量分母。不凭少量样本得出稳定泛化或统计显著性结论。

报告固定标记 `evaluation_scope=contract_fixture`、`evidence_verified=false`、`adoption=not_decided`。脚本只检查指定答案字段，不能证明完整开发质量、隔离、授权、证据真实性或经验可迁移；成功退出不等于采用候选。上下文、计划或套件不一致时拒绝比较，先解决不一致再取得所需执行结果。

## 4. 结束与经验

H0 是保留现行方法，候选无明确收益或存在关键退化时可选择 H0。证据不足以形成可迁移经验时写 `NO_ENTRY` 并说明限制；它只表示不新增长期经验，不表示试验程序失败或原开发任务停止。

确定性 fixture 只验证导出、解析、计分和汇总工具。没有真实模型/开发任务对照时，报告“工具验证完成，方法效果未验证”，不能用 fixture 的 win/loss 宣称开发效率提高。原始证据、经验归属和清理遵循 Skill 主体及通用规范。

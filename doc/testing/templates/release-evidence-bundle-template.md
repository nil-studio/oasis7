# 测试发布证据包模板

审计轮次: 4

## 目的
- 提供统一的发布证据包结构；`TASK-TESTING-003` 保留为原模板来源，不要求复制模板时新建任务。
- 将命令、日志、截图、结论、风险与 `PRD-ID / 实际 PR、CI 或评审来源` 映射汇总到同一份模板中。
- 让 `producer_system_designer`、`qa_engineer`、模块 owner 在 go/no-go 评审时可直接使用同一份证据。

## 使用说明
- 每个发布候选、阶段收口评审或高风险专项回归都应复制本模板并回填实际值。
- 本次适用判据所需的身份、范围或必需证据缺失时，总结论不能填 `pass`；未执行/未知写 `blocked`，已观测到的失败仍保留 `fail`，不得被缺字段遮蔽。
- 同一证据包可引用多个模块，按模块列出合同来源、实际 PR/CI/评审、测试层级与证据路径；Issue / Project 按需，不要求额外任务编号。
- 截图、console、summary、日志等产物使用评审者可回读的仓库路径或实际 CI artifact 定位；按适用合同保存原始文件、字节长度与 digest，不把本机临时路径当作已共享证据，不记录 token/密钥。
- 候选/场景/判定沿用[总手册](../../../testing-manual.md)与 [QA source-to-scene](../prd.md#qa-infrastructure-source-scene-method)，不是新增测试选择器：先明确声明范围，再满足该范围的全部必需 gate；每次 smoke 不因此要求所有 full 场景或新审批。

## 目录建议
| 字段 | 推荐路径 |
| --- | --- |
| required / full 日志 | `output/<module>/<candidate>/logs/` |
| Web UI 截图 / console | `output/playwright/<suite>/<candidate>/` |
| 长跑 / soak 摘要 | `.tmp/<suite>/<candidate>/` 或 `output/<suite>/<candidate>/` |
| 汇总 Markdown | `output/release/<candidate>/summary.md` |
| 汇总 JSON | `output/release/<candidate>/summary.json` |

## 结论状态
| 状态 | 含义 |
| --- | --- |
| `pass` | 同候选/构建与声明 scope/tier 的必需断言和证据全部满足；阶段 pass 不自动等于 release go |
| `fail` | 已执行但存在阻断性失败 |
| `blocked` | 证据缺失、环境不足或待确认，无法给出有效结论 |

## 模板
### Meta
- 发布候选 / 阶段:
- source OID / base OID / 实际 tested tree:
- build ID / 产物 digest / 构建来源与 freshness 记录:
- 适用合同精确路径、锚点与版本 / scope / tier / 发布或阶段判据:
- attempt ID / 环境与配置 / 时间窗口 / 入口与 session（脱敏）:
- 玩家访问模式 / execution lane / provider 身份与 mock/real 分类（适用时）:
- 证据包 ID:
- 日期:
- 汇总人:
- 总结论: `pass` / `fail` / `blocked`
- 关联 go/no-go 记录（未评估发布写 not_evaluated；不另建审批）:

### 覆盖范围
| 模块 / 合同 | 实际 PR/CI/评审来源（Issue 按需） | 测试层级 | 选中场景/断言与所需 gate | 负责人 |
| --- | --- | --- | --- | --- |
|  |  |  |  |  |

### 实际覆盖与未证明边界
| 场景 / gate | planned / located / implemented / executed | 原始证据定位与完整性 | oracle 及实际 verdict | 未覆盖 / 未执行 / 不适用依据 |
| --- | --- | --- | --- | --- |
|  |  |  |  |  |

执行状态与 verdict 分开：代码存在、结构校验或旧绿灯不证明 executed/pass。每份输入证据绑定自身候选、构建和 attempt；不同身份的结果不能无说明汇总成一个候选的通过。

### 执行命令
| 套件 / 检查项 | 实际命令 / attempt | 最终退出码与结果 | 日志路径 |
| --- | --- | --- | --- |
| S0 |  |  |  |
| S1 / required |  |  |  |
| S2 / full（如执行） |  |  |  |
| S6 / Web UI（如执行） |  |  |  |
| S9 / S10（如执行） |  |  |  |

### UI / 体验证据
| 类型 | 路径 | 说明 |
| --- | --- | --- |
| 截图 |  |  |
| console / semantic 结果 |  |  |
| 视频（如有） |  |  |
| playability 卡片 / 评分 |  |  |

截图、console、semantic API 可用只证明 surface observation；涉及提交、receipt/root 或世界效果的结论还需同候选权威 source/journal/API oracle。真实 Provider、headed desktop/narrow、L4/L5 等按实际选中层级执行，未覆盖的保持未证明。

### 长跑 / 在线证据
| 套件 | summary 路径 | failures / timeline 路径 | 结论 |
| --- | --- | --- | --- |
| S9 |  |  |  |
| S10 |  |  |  |
| 其他 soak / stress |  |  |  |

S8 使用 [Web/LLM 模板](../../world-simulator/prd/acceptance/web-llm-evidence-template.md)保留 wrapper stdout/stderr、最终退出码、report/summary/run.log 与独立 run/metric verdict；summary 存在不等于 gate 通过，缺退出结果为 unknown。重试保留独立 attempt 和原失败，不改写历史来源。

### 风险与例外
| 风险 ID | 描述 | 当前影响 | 缓解措施 | 是否阻断 | 负责人 |
| --- | --- | --- | --- | --- | --- |
| R-001 |  |  |  |  |  |

### 结论摘要
- 通过项：
- 失败项：
- 缺失证据：
- 未执行 / 未覆盖 / 尚未证明的范围：
- 声明范围的必需 gate 是否全部满足及逐项来源：
- 建议结论：`pass` / `fail` / `blocked`
- 是否满足适用发布判据：`yes` / `no` / `not_evaluated`（实际 go/no-go 记录链接；不能仅由 S6 + S8 卡片推出）

## 最小审查清单
- 是否固定 source/base/tested tree、构建来源、证据包/attempt、环境窗口及声明范围，而不是只写候选昵称或日期。
- 是否逐模块映射精确合同、实际 PR/CI/评审来源、层级、适用场景/gate 和负责人。
- 是否覆盖该范围的全部必需检查，而非仅凭“一条命令与日志”宣称完成。
- 已执行项的原始产物、实际退出/操作与 oracle 是否可回读；提前失败缺产物不伪造，未执行和不适用有明确区分。
- `pass/fail/blocked` 是否由匹配候选、范围与判据的全部适用证据支撑；截图或 summary 单独不能证明发布 go。
- 是否保留未覆盖项、失败与独立重试；阶段或样本通过不扩大为更高层验收。
- 是否已把可变结论回写对应 PR、实际 CI 与评审记录（Issue 按需），并把稳定测试合同或证据字段回写相关 PRD/manual/evidence 文档。

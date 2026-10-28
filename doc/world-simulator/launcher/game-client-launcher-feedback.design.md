# 客户端启动器反馈设计（当前 authority）

> 对应需求: `doc/world-simulator/launcher/game-client-launcher-feedback.prd.md`
> 历史迁移、验证与 task 状态：GitHub task issue evidence。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [F1 CLI 空格](game-client-launcher-feedback.prd.md#feedback-diagnostic-minimum) | 未配置但可识别的空格分隔 CLI 凭据与赋值形式均受保护 | [F1 CLI 空格](game-client-launcher-feedback.design.md#feedback-f1-cli) | Launcher / QA / 独立隐私或 writer 评审 | 产品 N/A；reason: 本行仅可识别 CLI 空格/赋值凭据识别专业机制，无独立叶级算法承诺；scope: 仅本行机制，仍受 §6.1 最小信息/真实结果及本地启动产品 §2.3 诊断边界约束；不免除下列 LAUNCH-001 结果关系 |
| [F2 扫描预算](game-client-launcher-feedback.prd.md#feedback-diagnostic-minimum) | 扫描有界且超预算未检查尾部整体保护 | [F2 扫描预算](game-client-launcher-feedback.design.md#feedback-f2-budget) | Launcher / QA / 独立隐私或 writer 评审 | 产品 N/A；reason: 本行仅有界扫描的未检查尾部保护专业机制，无独立叶级算法承诺；scope: 仅本行机制，仍受 §6.1 最小信息/真实结果及本地启动产品 §2.3 诊断边界约束；不免除下列 LAUNCH-001 结果关系 |
| [F3a 短值](game-client-launcher-feedback.prd.md#feedback-diagnostic-minimum) | 短配置值不逐字符破坏安全诊断/已有脱敏标记 | [F3a 短值](game-client-launcher-feedback.design.md#feedback-f3a-short) | Launcher / QA / 独立隐私或 writer 评审 | 产品 N/A；reason: 本行仅短值匹配与安全文本保留专业机制，无独立叶级算法承诺；scope: 仅本行机制，仍受 §6.1 最小信息/真实结果及本地启动产品 §2.3 诊断边界约束；不免除下列 LAUNCH-001 结果关系 |
| [F3b 引号路径](game-client-launcher-feedback.prd.md#feedback-diagnostic-minimum) | 含空格引号绝对路径按完整值保护 | [F3b 引号路径](game-client-launcher-feedback.design.md#feedback-f3b-path) | Launcher / QA / 独立隐私或 writer 评审 | 产品 N/A；reason: 本行仅引号空格绝对路径的完整范围保护专业机制，无独立叶级算法承诺；scope: 仅本行机制，仍受 §6.1 最小信息/真实结果及本地启动产品 §2.3 诊断边界约束；不免除下列 LAUNCH-001 结果关系 |
| [F4 最小摘要](game-client-launcher-feedback.prd.md#feedback-diagnostic-minimum) | 配置 allowlist；最近日志至多 200 行，原顺序和保护保持 | [F4 最小摘要](game-client-launcher-feedback.design.md#feedback-f4-diagnostics) | Launcher / QA / 独立隐私或 writer 评审 | 产品 N/A；reason: 本行仅allowlist 配置摘要与 200 行有序裁剪专业机制，无独立叶级算法承诺；scope: 仅本行机制，仍受 §6.1 最小信息/真实结果及本地启动产品 §2.3 诊断边界约束；不免除下列 LAUNCH-001 结果关系 |
| [F4 native 门控](game-client-launcher-feedback.prd.md#feedback-diagnostic-minimum) | native Ready 后远端失败才回落；禁用/未就绪无本地保存 | [F4 native 门控](game-client-launcher-feedback.design.md#feedback-f4-native) | Launcher / QA / 独立隐私或 writer 评审 | 产品 N/A；reason: 本行仅既有 native Ready/远端优先/fallback 的专业接线，无独立叶级算法承诺；scope: 仅本行机制，仍受 §6.1 最小信息/真实结果及本地启动产品 §2.3 诊断边界约束；不免除下列 LAUNCH-001 结果关系 |
| [F4 Web 代理](game-client-launcher-feedback.prd.md#feedback-diagnostic-minimum) | Web 门控与代理结果真实；没有 native 文件回落 | [F4 Web 代理](game-client-launcher-feedback.design.md#feedback-f4-web) | Launcher / QA / 独立隐私或 writer 评审 | 产品 N/A；reason: 本行仅既有 Web 请求/代理/无本地回落专业边界，无独立叶级算法承诺；scope: 仅本行机制，仍受 §6.1 最小信息/真实结果及本地启动产品 §2.3 诊断边界约束；不免除下列 LAUNCH-001 结果关系 |
| [F5 独占 writer](game-client-launcher-feedback.prd.md#feedback-local-file-identity) | 固定时钟、预存路径和并发不覆盖，失败不误报、不删旧文件 | [F5 独占 writer](game-client-launcher-feedback.design.md#feedback-local-file-design) | Launcher / QA / 独立隐私或 writer 评审 | 产品 N/A；reason: 本行仅 native 本地文件身份和独占创建专业机制，无独立叶级算法承诺；scope: 仅本行机制，仍受 §6.1 最小信息/真实结果及本地启动产品 §2.3 诊断边界约束；不免除下列 LAUNCH-001 结果关系 |
| [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001)；Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1 | 本地保存/受理只表明反馈路径结果，不代签可玩或世界行动 | [反馈结果边界](game-client-launcher-feedback.design.md#feedback-result-boundary) | Launcher / 产品 owner / QA | 仅此结果子义务；不覆盖全部 session、配置应用、凭据权限或 F1–F5 算法 |

## 结构

1. **交互承载层**：native 以独立反馈窗口承载 `Bug/Suggestion`、标题、描述与输出目录；Web 以对应窗口承载可提交草稿。主面板只负责发现和打开入口。
2. **校验与状态层**：提交前验证必填字段；in-flight 与可用性门控防止无效重复请求；成功、校验失败、代理/远端失败均保留可读状态，不因失败关闭草稿。
3. **native 提交层**：当前玩家路径先以 chain `Ready` 门控；仅在该门控通过后，`submit_feedback_with_fallback` 请求 `/v1/chain/feedback/submit`，再在远端失败时调用本地反馈包写入。链禁用或未就绪时在请求前拒绝提交，不触发本地写入。时间戳与类别保持可识别，字段和日志上限保持兼容；文件创建需满足下述不覆盖合同。
4. **Web 提交层**：Web 请求 `/api/chain/feedback`，由 `oasis7_web_launcher` 代理到 runtime；响应以结构化结果回到窗口。该层没有浏览器文件写入回落。

<a id="feedback-result-boundary"></a>
## 表现与控制边界

Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1。仅承接[REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001)中“本地保存或请求受理不能单独证明可玩或世界行动成功”的子义务：native 本地文件成功、distributed feedback receipt 与 Web proxy 成功分别只说明其反馈路径结果，不确认玩家世界行动、配置应用、身份权限或 session 全面可用。

[受控 Limited Preview §6.1](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#61-受控-limited-preview)的正式反馈可用性、未完成不得计成功及最小信息要求仍有效；不增加独立叶 ID，不以本设计承接 CANDIDATE-001 或 LP-1 整体 release gate。诊断和 writer 专业算法不因此整体绑定 LAUNCH-001；[本地启动 §2.3](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#23-设置与结果边界)的诊断保密和非世界事实边界继续有效。


- 反馈窗口不能改变世界规则、链状态或提交协议；它只编排已有输入、门控和结果。
- native/Web 需复用同一类别、必填和结果语义，但必须明确呈现不同的回落能力，不能把 Web 失败描述为已本地保存。
- 链禁用或未就绪时，当前实现拒绝提交且不触发 native 本地回落；本设计不把该状态解释为已提供可操作修复 CTA。
- 反馈窗口、草稿和结果应保持可自动化定位；任何实际 UI 改动须遵循 `testing-manual.md` S6 的 desktop/mobile 浏览器证据要求。

## 代码与协议接点

- native：`crates/oasis7_client_launcher/src/feedback_entry.rs`、`feedback_window.rs`。
- Web：`crates/oasis7_client_launcher/src/feedback_window_web.rs`、`app_process_web.rs`。
- 控制面/runtime：`/api/chain/feedback` -> `/v1/chain/feedback/submit`。

本设计保留当前结构，不新增字段、CTA、fallback 或端点。以下为既有诊断保护和本地文件保存所缺的实施边界与验收目标，不声明实现已经修复。

<a id="feedback-diagnostic-design"></a>
## 最小自动诊断设计

对应[最小自动诊断合同](game-client-launcher-feedback.prd.md#feedback-diagnostic-minimum)，`crates/oasis7_client_launcher/src/feedback_privacy.rs::capture_feedback_diagnostics` 负责构造独立摘要，不直接序列化完整配置为反馈字段。获准布尔、枚举、有界数值之外默认排除；日志截取最近 200 行后清洗，保留原时间顺序。原配置值仅作本地敏感值匹配输入，不能随摘要输出。

清洗器对已配置敏感值和可识别 CLI/赋值/路径语法分别处理。CLI 凭据既覆盖赋值形式，也覆盖 `--api-key value` 等空格分隔形式；引号包围的私有绝对路径须以完整值为范围，含空格不能只遮住第一段。短值匹配要保留无关单词和已有 `<redacted>` 等标记；无法安全确定值范围时遮蔽该诊断片段或整行，不退回未清洗内容。每行扫描达到既有预算仍须遮蔽未检查部分，避免有界执行以泄露尾部为代价。使用虚构凭据和路径测试，证据不复制真实私有样本。

2026-10-15 固定 main `b57e30eaaa0f3eb48c273eb62363d2ac2ada8981` 静态复核已有 allowlist 和 200 行裁剪；清洗函数仍有逐值 `.replace` 与最多 32 次凭据扫描等具体接点。它们需按 [#4406 F1/F2/F3a/F3b](https://github.com/eng-cc/oasis7/issues/4406) 分别验证适用性，不能仅凭历史评审状态认定已修复或今天仍可复现。扩展或调整保护逻辑时保留独立隐私评审，不把本次文档改动作为安全实现结论。

<a id="feedback-f1-cli"></a>
### F1 CLI 空格

未配置但可识别的空格分隔 CLI 凭据与赋值形式均受保护。验证输入与断言见两表及独立验证目标；这是原合同的独立子义务，不新增保护策略。

<a id="feedback-f2-budget"></a>
### F2 扫描预算

扫描有界且超预算未检查尾部整体保护。验证输入与断言见两表及独立验证目标；这是原合同的独立子义务，不新增保护策略。

<a id="feedback-f3a-short"></a>
### F3a 短值

短配置值不逐字符破坏安全诊断/已有脱敏标记。验证输入与断言见两表及独立验证目标；这是原合同的独立子义务，不新增保护策略。

<a id="feedback-f3b-path"></a>
### F3b 引号路径

含空格引号绝对路径按完整值保护。验证输入与断言见两表及独立验证目标；这是原合同的独立子义务，不新增保护策略。

<a id="feedback-f4-diagnostics"></a>
### F4 最小摘要

配置 allowlist；最近日志至多 200 行，原顺序和保护保持。验证输入与断言见两表及独立验证目标；这是原合同的独立子义务，不新增保护策略。

<a id="feedback-f4-native"></a>
### F4 native 门控

native Ready 后远端失败才回落；禁用/未就绪无本地保存。验证输入与断言见两表及独立验证目标；这是原合同的独立子义务，不新增保护策略。

<a id="feedback-f4-web"></a>
### F4 Web 代理

Web 门控与代理结果真实；没有 native 文件回落。验证输入与断言见两表及独立验证目标；这是原合同的独立子义务，不新增保护策略。

<a id="feedback-local-file-design"></a>
## 本地保存设计

对应[文件身份合同](game-client-launcher-feedback.prd.md#feedback-local-file-identity)，在现有本地回落边界内先完成内容构造，再用独占创建获取本次文件。时间戳用于阅读和排序；冲突时选择新候选名称，在有界次数内重试，不能用覆盖写入解决冲突。成功必须在完整内容写入后返回实际路径，失败保留原错误，不报告完整保存。只可清理本次独占创建但未完成的文件，不碰预存路径。

同一固定 main 的 `crates/oasis7_client_launcher/src/feedback_entry.rs::submit_feedback_report` 使用秒级时间戳加 kind 形成路径并调用 `fs::write`；两次同名写入可能覆盖，因此仅提高时钟精度不能满足并发和预存文件条件。目标实现不改 Ready 门控、远端优先、Web proxy 或字段语义，也不增加 fsync/断电恢复保证。

## 独立验证目标

| 子项 | 目标回归与准确接点 | 需要观察的结果 |
| --- | --- | --- |
| F1 CLI 凭据 | `feedback_privacy.rs` 的清洗测试，未配置的可识别空格分隔 CLI 值及既有赋值形式 | 值受保护，安全上下文仍可读；独立记录当前适用性 |
| F2 扫描上限 | 同文件构造超过 32 个可识别字段和未检查尾部 | 执行有界，尾部不以未清洗内容输出 |
| F3a 短配置值 | 同文件使用短值、无关诊断单词和已有脱敏标记 | 敏感值受保护，无关安全文本和标记不被逐字符破坏 |
| F3b 引号路径 | 同文件使用虚构带引号、含空格的绝对路径 | 完整私有路径受保护，非敏感错误仍可诊断 |
| F4 最小诊断与门控 | `feedback_privacy.rs` allowlist/日志上限；`feedback_window.rs`、`feedback_window_web.rs` 及提交 transport | 不含凭据/provider URL/私有路径；至多 200 行；disabled/not-Ready 无本地文件，Ready 后远端失败本地回落，Web proxy 保持 |
| F5 不覆盖 | `feedback_entry.rs` 的离线 writer/transport fixture：固定时钟两报告、预存同名文件、并发 writer、写入失败 | 路径互异、旧字节不变、成功包可解析；失败无虚假保存且不删旧文件 |

上述目标尚需实现候选的真实结果；文档检查不代替隐私或 writer 回归。原任务资源归档/删除事实与代码和验收独立，本专题不执行历史资源删除。

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 验证方法、test/manual source 与环境 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [F1 CLI 空格](game-client-launcher-feedback.prd.md#feedback-diagnostic-minimum) | [F1 CLI 空格](game-client-launcher-feedback.design.md#feedback-f1-cli) | 未配置但可识别的空格分隔 CLI 凭据与赋值形式均受保护 | [测试手册](../../../testing-manual.md) launcher/S6；`feedback_privacy.rs`；虚构 `--api-key value` 与赋值/escaped 对照；planned | 实现 PR 分项结果：凭据不输出，安全上下文可读 | 本次未运行；不证明未知秘密、手写文本安全、断电持久性或世界成功 |
| [F2 扫描预算](game-client-launcher-feedback.prd.md#feedback-diagnostic-minimum) | [F2 扫描预算](game-client-launcher-feedback.design.md#feedback-f2-budget) | 扫描有界且超预算未检查尾部整体保护 | [测试手册](../../../testing-manual.md) launcher/S6；`feedback_privacy.rs`；超过 32 个可识别字段及尾部；planned | 实现 PR 分项结果：尾部遮蔽或舍弃，不原样写入 | 本次未运行；不证明未知秘密、手写文本安全、断电持久性或世界成功 |
| [F3a 短值](game-client-launcher-feedback.prd.md#feedback-diagnostic-minimum) | [F3a 短值](game-client-launcher-feedback.design.md#feedback-f3a-short) | 短配置值不逐字符破坏安全诊断/已有脱敏标记 | [测试手册](../../../testing-manual.md) launcher/S6；`feedback_privacy.rs`；短值、无关词及已有标记；planned | 实现 PR 分项结果：保护敏感值，保留安全文本 | 本次未运行；不证明未知秘密、手写文本安全、断电持久性或世界成功 |
| [F3b 引号路径](game-client-launcher-feedback.prd.md#feedback-diagnostic-minimum) | [F3b 引号路径](game-client-launcher-feedback.design.md#feedback-f3b-path) | 含空格引号绝对路径按完整值保护 | [测试手册](../../../testing-manual.md) launcher/S6；`feedback_privacy.rs`；虚构带引号空格路径；planned | 实现 PR 分项结果：完整私有路径受保护，安全错误仍可诊断 | 本次未运行；不证明未知秘密、手写文本安全、断电持久性或世界成功 |
| [F4 最小摘要](game-client-launcher-feedback.prd.md#feedback-diagnostic-minimum) | [F4 最小摘要](game-client-launcher-feedback.design.md#feedback-f4-diagnostics) | 配置 allowlist；最近日志至多 200 行，原顺序和保护保持 | [测试手册](../../../testing-manual.md) launcher/S6；`feedback_privacy.rs`、`feedback_entry.rs`；配置摘要及超 200 行日志；planned | 实现 PR 分项结果：无凭据/provider URL/私有路径，裁剪与顺序正确 | 本次未运行；不证明未知秘密、手写文本安全、断电持久性或世界成功 |
| [F4 native 门控](game-client-launcher-feedback.prd.md#feedback-diagnostic-minimum) | [F4 native 门控](game-client-launcher-feedback.design.md#feedback-f4-native) | native Ready 后远端失败才回落；禁用/未就绪无本地保存 | [测试手册](../../../testing-manual.md) launcher/S6；`feedback_window.rs`、`feedback_entry.rs`；disabled/not-Ready、Ready 远端成功/失败、writer 失败；planned | 实现 PR 分项结果：门控前拒绝不落盘，远端失败回落且失败不报保存 | 本次未运行；不证明未知秘密、手写文本安全、断电持久性或世界成功 |
| [F4 Web 代理](game-client-launcher-feedback.prd.md#feedback-diagnostic-minimum) | [F4 Web 代理](game-client-launcher-feedback.design.md#feedback-f4-web) | Web 门控与代理结果真实；没有 native 文件回落 | [测试手册](../../../testing-manual.md) launcher/S6；`feedback_window_web.rs`、`app_process_web.rs` 与控制面；不可用、in-flight、成功及代理失败；planned | 实现 PR 分项结果：结构化结果和草稿可诊断，不报本地保存 | 本次未运行；不证明未知秘密、手写文本安全、断电持久性或世界成功 |
| [F5 独占 writer](game-client-launcher-feedback.prd.md#feedback-local-file-identity) | [F5 独占 writer](game-client-launcher-feedback.design.md#feedback-local-file-design) | 固定时钟、预存路径和并发不覆盖，失败不误报、不删旧文件 | [测试手册](../../../testing-manual.md) launcher/S6；`feedback_entry.rs`；离线 writer/transport，固定时钟两包、预存同名、并发及写失败；planned | 实现 PR 分项结果：路径互异、旧字节不变、成功包可解析；失败不报完整保存 | 本次未运行；不证明未知秘密、手写文本安全、断电持久性或世界成功 |
| [REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001)；Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1 | [反馈结果边界](game-client-launcher-feedback.design.md#feedback-result-boundary) | native 保存、distributed receipt、Web proxy 成功与世界成功分开 | [测试手册](../../../testing-manual.md) launcher/S6；同候选 native/Web 分别注入成功、未完成、远端/代理/写入失败，核反馈消息与未确认世界状态；planned | 实现 PR 的消息、实际路径/receipt、失败及世界结果不被改写的对照 | 本次仅静态核窗口/transport；未运行，不证明全部 LAUNCH-001 或 release gate |

## 风险与未决

- 固定 main 静态接点仍为逐值 `.replace`、凭据扫描 32 次后直接返回、除 Bearer 外要求赋值分隔符、私有路径遇空白结束；F1/F2/F3a/F3b 的上述目标不能写成已修。
- writer 仍以秒级时间戳加 kind 命名并 `fs::write`，不满足 F5 的独占创建目标；本轮不修改实现或增加安全政策。
- 已定义 allowlist、赋值/escaped 凭据、200 行、单包 JSON 写入及远端失败 fallback 测试，仅代表这些局部测试定义；没有运行，也不能替代 CLI 空格、33+ 字段尾部、短值、空格路径及并发负例。
- 窗口 submit 在可用性门控后才调用 native helper；Web 在门控和 in-flight 检查后请求 proxy。helper 的单独调用不证明 UI 门控闭环。真实 native/Web UI、代理异常与世界结果分离仍须同候选验证；无新增禁用态修复 CTA、必达、公开账本、保留期或人工答复保证。

# 客户端启动器跨表面受控动作设计

> 对应需求: `doc/world-simulator/launcher/game-client-launcher-cross-surface-action-parity.prd.md`
> 历史迁移、验证与 task 状态：GitHub task issue evidence。
> **职责与承接边界**：本文承载受控动作的真实输入、接受/失败、配置读回保草稿与当前 action 追踪连续性合同，供无头控制面（`oasis7_launcher_backend`）及活跃消费者复用。

## 目标与范围

保持受控动作的真实输入、接受/失败、读回与恢复语义；不将控制面代理或本地存储适配掩盖为结算或授权声明。本文覆盖设置读回与当前 action 结果连续性，不修改 runtime 结算、provider 鉴权或凭据注入合同。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [LLM 读回结果](game-client-launcher-cross-surface-action-parity.prd.md#llm-settings-reload-result) | native/Web Reload 的最终状态与草稿由真实结果决定 | [结果连续性](#action-result-continuity) | Launcher settings / native 文件与 Web storage / QA | 不改变 provider 鉴权或凭据注入 |
| [当前转账 action](game-client-launcher-cross-surface-action-parity.prd.md#transfer-current-action) | 新 action 受理后，旧状态/迟到响应不能改变当前追踪 | [结果连续性](#action-result-continuity) | Launcher transfer / runtime 查询 / QA | 不改变经济执行、结算或持久账本 |
| Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1；[REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | Reload 成功才替换草稿/发布成功；失败保留草稿并反馈错误，合法默认回退保持原义；只接收本地配置结果及恢复子义务 | [已有设计](#action-result-continuity) | Launcher 控制面/本地存储/QA；session 与世界 authority 独立 | 不证明 provider 鉴权、runtime 凭据注入、完整 session 或世界结果 |
| Trace refs: O7-REQ-1d9a996e-dd9d-4d59-9b3b-b79191f17676；[REQ-ENTRY-LAUNCH-002](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-002) | Reload 成功才替换草稿/发布成功；失败保留草稿并反馈错误，合法默认回退保持原义；只接收本地配置结果及恢复子义务 | [已有设计](#action-result-continuity) | Launcher 控制面/本地存储/QA；session 与世界 authority 独立 | 不证明 provider 鉴权、runtime 凭据注入、完整 session 或世界结果 |

## 设计分层

1. **共享语义层**：为配置问题、动作前置、提交中、结构化结果和恢复建议定义同源含义。
2. **平台适配层**：native 与 Web 分别决定可见字段、输入方式、存储与 transport；适配差异不得被 UI 隐藏成成功或等价声明。
3. **控制面边界**：Web 通过 Launcher 控制面转发受支持请求；控制面保留 runtime 的接受、拒绝和错误语义，而不重写结算或授权规则。
4. **反馈层**：动作表面将 executable、blocked、accepted/rejected/failed 与下一步呈现给用户，并在 in-flight 期间防止误导性的重复提交。
5. **LLM 设置适配层**：native 通过 launcher 本地配置的 `[llm]` 字段保存并读回 `api_key`、`base_url` 和 `model`；Web 通过浏览器本地存储保存并读回。两个表面都将读取、解析或写入失败作为可恢复的可见结果，不把草稿、旧值或缺省值当成已保存状态。

## 边界

- native-only 字段不进入 Web 表单或 Web required 校验；Web 专有前置仍由 Web authority 判断。
- 浏览器本地存储仅是 Web 设置适配。存储不可用必须反馈失败，不能推断安全凭据持久化。
- native 配置保存与 Web 本地读回都只是 Launcher 表面结果；它们不证明 provider 鉴权、runtime 凭据注入或跨 profile/重启持久性。secret 不进入状态快照、GUI-agent、可见错误、截图或任务证据。
- action acceptance 是提交阶段结果；最终状态继续由 runtime 及其查询/验证 authority 决定。
- 最近历史可为有界 process-static 视图，设计不得把它显示为完整或重启后仍存在的记录。

## 演进约束

新增跨表面动作必须先明确共享语义、平台差异、控制面边界、失败恢复和相应专业验证；不得以“parity”标题替代这些证据。

<a id="action-result-continuity"></a>
## 读回与转账结果连续性

本节细化[专业合同](game-client-launcher-cross-surface-action-parity.prd.md)中的失败可见和[当前 action](game-client-launcher-cross-surface-action-parity.prd.md#transfer-current-action)，用于 [#4406 C3 与转账窗口子包](https://github.com/eng-cc/oasis7/issues/4406) 的实现与独立验收。它不增加 runtime 动作、经济规则或跨会话持久化。

### LLM Reload 的结果归属

native 与 Web 的 loader 应把本次读回的成功/失败明确返回给调用者，由按钮 handler 在同一分支决定草稿和状态：成功才同时替换 draft、发布 Success；失败保留原 draft、发布 Failed，并保留修复后再试的入口。默认回退是 loader 原有合法结果，不把 NotFound、空值或缺 key 统一升级为错误。Reload 不写回配置、不触发 provider 请求，错误文本继续遵守 secret 保护。

在基准 `5e930face77dd284bbba6186fd997e7e63fa9164`，`crates/oasis7_client_launcher/src/llm_settings.rs` 与 `llm_settings_web.rs` 的 Reload handler 均在调用 loader 后无条件赋值 Success，而 loader 失败分支先赋值 Failed；只测 loader 返回错误无法证明最终界面正确。目标验证在两个表面分别执行按钮路径，覆盖格式错误、存储 I/O 错误、原草稿保留、修复后成功，以及既有默认回退。文档不声明这些回归已运行。

### 转账追踪的原子切换

受理 B 时一起更新 `tracked_action_id`、清除 `tracked_action_status`、重置本 action 的 poll 时间/完成状态，并安排 B 的状态查询。所有 status 请求在发起时保存所属 action；响应只在该 action 仍为当前追踪对象时写入状态。若同一窗口还有本地请求代次，可用来排除更早请求；这只是客户端排序，不新增 backend action ID 或协议。

状态查询返回值不能自行替换当前 action。旧 A 的成功、失败、终态和网络错误不能改变 B 的追踪或调度；B 查询失败保留 B 身份并允许按既有有界策略继续查询。终态判断同时核对身份和状态，不只看上一次残留的 confirmed。提交拒绝继续作为本次提交结果，不伪造 B 的 action ID。

基准的 `crates/oasis7_client_launcher/src/transfer_window.rs::apply_web_transfer_submit_result` 切换 ID 时未清旧 status，`apply_web_transfer_query_result` 又直接用响应改写追踪 ID；自动 poll 按残留 status 判断终态。这三处需共同满足合同。目标验证经过提交受理、响应消费、清除 in-flight 和下一次实际查询调度，覆盖旧 A 迟到、B 首查失败后恢复、B 自身 pending/confirmed/failed 与正常拒绝；使用受控 transport 和非空 accounts/history，核对实际请求的 action ID，不执行真实转账。

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 验证方法、test/manual source 与环境 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [LLM 读回结果](game-client-launcher-cross-surface-action-parity.prd.md#llm-settings-reload-result) | [结果连续性](#action-result-continuity) | C3 两表面失败保草稿、修复成功与合法默认回退 | [测试手册](../../../testing-manual.md) launcher/S6；在 `llm_settings.rs`、`llm_settings_web.rs` 按上述错误矩阵执行真实按钮 handler 并检查最终状态；planned | 实现 PR 的当前候选和各表面结果 | 本次未运行，loader 单测不证明 handler，读回不证明 provider |
| [当前转账 action](game-client-launcher-cross-surface-action-parity.prd.md#transfer-current-action) | [结果连续性](#action-result-continuity) | A confirmed→B accepted→B 首查失败恢复；A 在途→B 受理→A 迟到分别取证 | [测试手册](../../../testing-manual.md) launcher/Transfer；`transfer_window.rs` 受控 transport 贯通结果消费和下一轮实际请求，保留 B 终态/拒绝正例，非空 accounts/history；planned | 实现 PR 的状态及实际请求 action ID | 本次未运行，不执行真实转账，不证明 runtime 结算 |
| Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1；[REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | [已有设计](#action-result-continuity) | Reload 成功才替换草稿/发布成功；失败保留草稿并反馈错误，合法默认回退保持原义 | [测试手册](../../../testing-manual.md)；原 C3 native/Web 按钮路径的格式/I/O 错误、草稿保留、修复成功及合法默认回退，同候选分别验证，适用可见变化另验 S6 | 实现 PR 的真实 handler/状态/存储结果；planned，本轮未运行 | 配置读回或画像成功不能代签全部设置应用、跨 profile、世界恢复或发行 |
| Trace refs: O7-REQ-1d9a996e-dd9d-4d59-9b3b-b79191f17676；[REQ-ENTRY-LAUNCH-002](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-002) | [已有设计](#action-result-continuity) | Reload 成功才替换草稿/发布成功；失败保留草稿并反馈错误，合法默认回退保持原义 | [测试手册](../../../testing-manual.md)；原 C3 native/Web 按钮路径的格式/I/O 错误、草稿保留、修复成功及合法默认回退，同候选分别验证，适用可见变化另验 S6 | 实现 PR 的真实 handler/状态/存储结果；planned，本轮未运行 | 配置读回或画像成功不能代签全部设置应用、跨 profile、世界恢复或发行 |

## 决策、风险与未决

沿用共享语义和平台适配分离的决定；主要风险是 loader 失败被按钮成功覆盖、旧 action 迟到响应污染当前追踪，以及本地保存被误解为 provider 或世界成功。解除条件为下列 C3 与转账追踪目标矩阵的真实结果消费验证；完整跨平台可用性和世界结算仍须独立证据。

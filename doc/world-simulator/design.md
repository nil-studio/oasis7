# world-simulator 模块设计总览

审计轮次: 6

- 对应需求文档: `doc/world-simulator/prd.md`
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history
- 对应文件级索引: `doc/world-simulator/prd.index.md`

## 实现 crate 与宿主接线

`oasis7_simulator` 拥有确定性 WorldKernel/WorldModel、动作与原生事件、scenario、query、快照与回放。它只依赖已有 API/ABI 合同和序列化/hash 库，独立 native/WASM 使用同一套内核实现；AgentBehavior、Runner、Provider/model 执行、源码编译 IO 与旧归档组合仍由 `oasis7` 宿主适配。纯内核的默认 compiler 不接触宿主环境，native 宿主明确注入原 compiler。内核规则与恢复测试随实现迁入新 crate；完整旧归档及宿主 compiler 集成测试留宿主。

宿主只通过限定的 effect intent/receipt 记录入口提交原 trace 值，event identity/time 仍由内核分配。只读 schedule query 明确区分 SimulatorStock 与 HostLedgers；后者只省略模拟库存检查，宿主仍负责自己的账本可负担性与实际执行时重检，query 结果不是准入凭据。Viewer 通过公开只读查询消费纯内核，不跨 crate 借用私有 recorder 或内部校验方法。

## 内核持久化与历史组合归档

原生 `KernelSnapshot` / `KernelJournal` / `KernelEvent` 仅表达内核状态、动作队列与事件，不拥有 Runtime snapshot/event、player gameplay 或 chain manifest/delta。旧 `WorldSnapshot` / `WorldJournal` / `WorldEvent` 是宿主的历史组合归档合同，保留原字段顺序、默认值、版本和 JSON/CBOR 编码。

宿主通过显式拆分、重组方法保存内核值与归档扩展；拆出原生内核状态不是删除扩展的授权。完整归档的读取、编辑和回写必须同时保留扩展，不能把原生快照或裁剪的读取视图当作完整归档。内核导出/恢复只消费自身字段，旧归档适配承担扩展所有权，持久化兼容回归覆盖非空 Runtime、player、chain 字段及关联事件。

内核 journal、step 和 replay 使用原生事件。宿主 `WorldKernel` 封装持有完整历史事件并同步所有可变入口，不提供绕过同步的可变内核引用；旧规则回调仍读取完整宿主日志。直接 `WorldKernel` serde 保留旧字段布局与缺省值，进程内 hook/compiler 不进入编码。

加载后普通保存保留宿主提供的 Runtime/player 扩展及原 committed-cut chain 证据，不按缺省链身份重建或伪造新提交；显式 `snapshot_with_chain_resource_context` 才负责按调用者提供的上下文重新派生 chain 字段。原本完全由纯内核缺省上下文生成的归档继续随内核状态更新派生字段。

## Source compiler 宿主能力

源码编译通过 `oasis7_agent_api::module_source::ModuleSourceCompiler` 接收模块标识与既有源码包，返回 WASM bytes 或保持原分类的错误。文件、进程、环境与超时处理归宿主；内核只在原动作执行阶段调用能力，然后沿既有部署规则记录产物 bytes/hash。进程内 compiler hook 不进入快照；journal replay 应只消费已记录的产物，不再次编译。现有 native 宿主安装开发 compiler，WASM 缺省保持不可用错误；外部能力不是玩家准入或部署权限。Runtime 的 production release 禁编译策略继续在调用前生效。

## 共享类型与资源派生边界

Simulator 原生 Action、Agent、WorldModel 与 Runtime 模型各自保持现有语义。共享库存/元素预算、机器人规格/视觉实体、chunk 几何和 player-safe intent summary 直接由 `oasis7_agent_api` 拥有，Simulator 旧路径保留兼容导出。几何派生仅接收所需空间尺寸，不依赖完整 WorldConfig。

`simulator::chain_resource` 从内核模型、配置和 journal 构建 API 资源 manifest/delta；Runtime 派生位于其自己的模块。共享 DTO 的字段、默认值、canonical 排序与 hash 算法保持不变；旧组合 snapshot 的 Runtime、player、chain 字段仍须完整保存，不能用裁剪后的读取值覆盖完整归档。

## 1. 设计定位
`world-simulator` 模块的 `design.md` 负责描述世界模拟、Viewer、Launcher、LLM 与场景系统的总体设计入口。

## 2. 阅读顺序
1. `doc/world-simulator/prd.md`
2. `doc/world-simulator/design.md`
3. `doc/world-simulator/prd.index.md`
4. 对应 GitHub task issue（需要当前任务状态、阻断或验收过程时）
5. 下钻 `viewer/`、`launcher/`、`llm/`、`kernel/`、`scenario/`、`m4/` 等专题目录

### 2.1 需求承接与分配表

本表承接本总览拥有的 kernel adapter 兼容与跨 Runner 动作指标边界；其余主题仍由下钻专题承担。

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [professional_acceptance: runner-metrics-action-window-contract（RunnerMetrics 动作与窗口）](prd.md#runner-metrics-action-window-contract) | 动作去重、终态与同范围分母，缺测/截断可识别 | [指标设计](#runner-metrics-action-window-design) | Simulator Runner/runtime-live/Metrics 消费者 | 不改变动作执行或业务结算，不把目标表达称为现行 wire |
| [professional_acceptance: kernel-rule-cost-contract（Kernel 规则成本兼容边界）](prd.md#kernel-rule-cost-contract) | 保留历史 adapter 的决策聚合行为；不得由 `cost` 字段推导自动结算或持久成本记录。 | [cost 边界](design.md#kernel-rule-cost-boundary) | `runtime_engineer`；simulator hook 与正式 runtime 规则执行链。 | 不启用新的扣账规则，不扩展模块生命周期、WASM ABI 或持久化能力。 |

## 3. 设计结构
- 交互层：Viewer / Launcher / Web Console / UI 流程。
- 模拟层：kernel、scenario、资源与世界状态演化。
- 智能层：LLM、agent 行为、间接控制与多场景评估。

## 4. 集成点
- `doc/world-runtime/prd.md`
- `doc/game/prd.md`
- `doc/site/prd.md`
- `doc/testing/prd.md`

## 5. 专题导航
- 交互体验进入 `viewer/`、`launcher/`
- 核心模拟进入 `kernel/`、`scenario/`
- LLM 与 agent 行为进入 `llm/`
- 发行阶段能力进入 `m4/`

## 6. Simulator kernel rule / Wasm adapter boundary

- `WorldKernel` 的 completed rule-hook foundation 保持默认 no-op 不改变动作行为；pre-action 决策按固定注册顺序合并，`deny` 优先于 `modify/allow`，缺失或冲突的 override 显式拒绝。pre-action hook 只能读取 `&WorldKernel`，post-action hook 只观察已产生的 event。

<a id="kernel-rule-cost-boundary"></a>

### 规则成本字段

产品 N/A；reason=本条维护历史 simulator adapter 的决策数据兼容，聚合字段不产生新的余额检查、扣账或独立持久成本记录；scope=仅 KernelRuleDecision.cost 的合并与不自动结算边界。该适用性不覆盖动作自身资源变化、正式 runtime 的规则成本结算或其产品结果，亦不免除下列非零 cost、溢出与持久化证据的未证明边界。

`KernelRuleDecision.cost` 是历史 adapter 保留的决策数据。`crates/oasis7_simulator/src/kernel/types.rs::merge_kernel_rule_decisions` 聚合 cost；`kernel/step.rs::execute_action_envelope_at_time` 随后只按 verdict、override 和拒绝 notes 决定动作执行，不使用 cost 进行余额检查或资源扣账，也不把聚合 cost 作为独立事件写入 journal。动作自己的资源变化由其实现负责，不能据此推断所有动作都无成本。

正式 runtime 的 `RuleDecision` 与该 adapter 分属不同执行入口：`crates/oasis7_runtime/src/world/step.rs` 检查资源缺口并应用 `decision.cost`，成本应用失败形成结构化拒绝。保留 simulator 的兼容行为不改变正式 runtime 的结算合同。若以后让 simulator cost 生效，应明确付款主体、数值及原子性、事件/replay、旧决策与旧消费者兼容，并以针对该行为的回归验证替代对字段存在的推断。

### Wasm adapter 与其余模拟边界

- simulator 的可选 Wasm evaluator 使用 `KernelRuleModuleInput/Output` 作为 adapter 输入输出；缺失 `rule.decision` 明确表示 allow，而 evaluator、sandbox 调用、重复/不匹配/非法的 `rule.decision` payload 或 action id 校验失败都必须转为可解释的 structured deny，不能 silent fallback。`ModuleSandbox` bridge 只接单个 pre-action 模块，不定义多模块编排、发布或权限治理。
- 旧的 simulator 内存 artifact registry 是已完成接线阶段的局部实现细节；hook、evaluator 与 registry 都是 process-local，snapshot restore 时重置，不是当前模块生命周期或持久化 authority。replay 只消费已记录的 journal event，不重新求值 hook；`ActionRejected` 在 replay 中不改变 world state。当前 ABI / `rule.decision` wire contract、executor limits、artifact registry、release governance 和 Docker-canonical build 分别由 `doc/world-runtime/wasm/wasm-interface.md`、`wasm-executor.prd.md`、`doc/world-runtime/prd.md` 与 `wasm-deterministic-build-pipeline.prd.md` 拥有。
- `tools/wasm_build_suite` 仍是活跃实现入口，但其发布级构建语义只服从 Docker-canonical build authority；不得由历史 host-side build-suite 文档推导发布或跨宿主确定性结论。
- simulator `PowerStorage`、`StorePower`、`DrawPower` 与 Location electricity pool 已下线；该移除边界由 `m4/industrial-resource-flow-contract.prd.md` 维护。它不删除名称相近但独立的 runtime builtin `m1_power_storage`，也不取消对 legacy `require_power_storages` 输入的明确拒绝。
- simulator 内建 `ResourceKind` 只保留 `Electricity` 与 `Data`。`Compound` / `Hardware` 不接受旧别名或自动迁移，也不得作为内建 kind 重新进入 kernel、parser、Viewer 统计或 LLM prompt；工业材料语义由 M4/domain contract 表达，模块资产与 ABI/执行权限由 `doc/world-runtime/wasm/` authority 拥有。该边界不等于已经提供通用 WASM 资产标准。

<a id="runner-metrics-action-window-design"></a>
## 7. 跨 Runner 的动作指标设计

产品 N/A；reason=动作去重、终态关联和同范围聚合是跨 Runner 的工程统计机制，不单独新增玩家任务完成率、等待时延或结算承诺；scope=仅本节已有统计机制的专业归属，不覆盖实际 Metrics 消费者结果。消费者与验收摘要区分未测、partial、有效测量及统计范围的原义务仍适用；producer、兼容 wire 与各实际消费者的承接和验证仍是未闭合范围，不能以此 N/A、已有数值字段或文档检查代签。

此处补齐 #4478 的统计合同；基准 `5e930face77dd284bbba6186fd997e7e63fa9164` 的 `runtime_live/support.rs::runtime_metrics` 仍取 `journal.len()`，普通 `simulator/runner.rs::metrics` 的 `success_rate` 仍是占位值。这些事实不能作为正确成功率的实现证据。

1. 聚合输入来自受支持的权威动作结果，不把每条 journal event 都视为动作。按世界与既有 action 身份（含适用的 ID era）关联 admission、结果和原因；一次唯一裁决只计一次，重复 receipt/replay 不新增计数。尚无终态的 admission 留作待决，不能猜成成功；未知/冲突结果显式标为不可完整统计。
2. 同一范围内，以已裁决的唯一动作数为 `total_actions`，成功终态数除以已裁决数得到 `success_rate`；失败只贡献一个拒绝。一次成功启动异步任务是一次动作结果，任务后来完成/派生事件不新增动作，也不改变这一次动作的语义。`actions_per_tick` 使用该范围实际覆盖的 tick 长度，不能把 retained 事件窗口计数除以世界累计 time。
3. 全历史计数需要可恢复的累计基线和其后连续动作结果；仅有 retained journal 时，只能报告可证明的窗口和完整性。恢复使用同范围基线，窗口边缘缺 admission/终态时显示 partial；不靠清空或重扫残缺事件伪造累计。`total_decisions` 与动作计数继续分开，模型决策、Wait、系统事件不自动互相折算。
4. 数值 wire 的既有形状和 0 默认值按兼容处理。实施须让 Metrics 消费者和验收摘要能够识别窗口、分母与未测/partial；若现有传输无法表达，就同步采用明确兼容扩展，而非修改字段含义却保持“已测”展示。无动作或 0 tick 不产生 NaN/Infinity，兼容 0 不作为通过证据。

待执行验证由同一候选的普通 simulator 与 runtime-live 分别完成：单成功动作含多事件计 1；admission 后拒绝为 0 成功/1 拒绝；系统事件不改变动作率；空样本不误报通过；截断窗口与累计分开；snapshot/replay 不重复累计；实际 Metrics wire 往返保留可用性与范围。优先扩展 [Runner](../../crates/oasis7/src/simulator/runner.rs) 和 [runtime-live 聚合](../../crates/oasis7/src/viewer/runtime_live/support.rs) 的现有测试入口；本次未运行行为测试，不宣称某个 UI 已展示这些字段。
## 设计目标

- 提供 `world-simulator` 模块的总体设计入口。

## 设计范围
- 覆盖模块级结构、主链路、分层与专题导航。
- 不替代专题 `*.design.md` 的细化设计。

## 关键接口 / 入口
- 需求入口：`doc/world-simulator/prd.md`
- 当前任务入口：对应 GitHub task issue / Project
- 索引入口：`doc/world-simulator/prd.index.md`

## 设计演进计划
- 历史建档（2026-03-09，ROUND-006）：模块级 `design.md` 入口来源；原 Task 未唯一定位，记录保留，不作为当前排期。
- 持续结构要求：高复杂度专题在对应 `*.design.md` 维护细化合同；不恢复旧 M2 任务。

## 设计风险
- 若专题级设计未及时补齐，模块级 `design.md` 可能承载过多导航职责。
- 若 legacy redirect 未明确标注为兼容跳转，读者可能误判历史入口为当前执行入口。

## 11. 验证设计与可追溯性

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [professional_acceptance: runner-metrics-action-window-contract（RunnerMetrics 动作与窗口）](prd.md#runner-metrics-action-window-contract) | [指标设计](#runner-metrics-action-window-design) | 唯一动作、最终结果、同窗口分母与未测语义 | [runtime-live 基线](../../crates/oasis7/src/viewer/runtime_live/tests/runtime_live_core.rs)及本页多事件/拒绝/系统事件/空样本/截断/恢复/wire 七类待执行回归；普通 Runner 与 runtime-live 分别观察，不以一方代签另一方 | 实施 PR 的候选、输入 action/event、计数分母、序列化结果及实际命令 | 文档不修复当前代码；不证明玩家等待时延或 job 最终完成率 |
| [professional_acceptance: kernel-rule-cost-contract（Kernel 规则成本兼容边界）](prd.md#kernel-rule-cost-contract) | [cost 边界](design.md#kernel-rule-cost-boundary) | 当前 cost 聚合不自动结算；已有 deny/modify 与事件后 hook 行为保持原义。 | 固定候选，回读 `kernel/types.rs::merge_kernel_rule_decisions` 与 `kernel/step.rs::execute_action_envelope_at_time`；[kernel_rule_decisions.rs](../../crates/oasis7_simulator/src/tests/kernel_rule_decisions.rs) 的 `kernel_pre_action_rule_deny_rejects_action`、`kernel_pre_action_rule_modify_overrides_action`、`post_action_hook_receives_event_after_modify_decision` 是相关 L1 hook 基线。文档修补核对源码和既有断言，行为改动时运行适用回归。 | PR 的候选、源码回读及实际运行结果，明确区分未运行的测试计划。 | 现有用例没有非零 cost 不扣账的专门断言；不能据此声称该行为已有自动回归、聚合溢出已验证或成本已持久记录。 |

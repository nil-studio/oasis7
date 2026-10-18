# oasis7 Runtime：WASM 可观测性与耗时指标设计

- 专业 owner：`wasm_platform_engineer`；状态：active professional authority（目标合同不等于已实现）。
- 内容审读基线：`eng-cc/oasis7`，源提交 `9c41d57b4436f71f0cf9b481043d882cfdc551ed`；内容整理日期：2026-09-26。
- 独立审读与实际实现/测试候选证据由该 task evidence 记录；此处不预报审读通过、runtime capability、发布或 full proof。下方既有条款仍有效；仅显式日期/历史基线条款按其证据范围解释。

## 1. 问题、目标与非目标

本次把既有专业义务承接为可执行的 target contract 与验证设计。成功结果是精确身份、边界、失败和恢复算法能被消费者读取；不改变 ABI/code/经济价格，不宣称目标已落地。下方原有细节、limits、公式、ID 与历史 evidence 原序保留并继续约束其适用范围。

## 2. 上游约束与相关角色

本页的本地计时、缓存热点和窗口统计不独立新增玩家SLA或计费要求。确定性隔离必须继续满足 [interface observation边界](wasm-interface.md#sr2-types)，不能把观测成功当世界执行成功。产品端到端 authority 为 [deterministic world execution](../../product/world-infrastructure/deterministic-world-execution.prd.md) 与 [distributed state availability](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md)；专业细化由 paired PRD、runtime/module 与 P2P 合同拥有。WASM owns codec/artifact/metering，runtime owns staged publication/replay，QA owns same-candidate acceptance，consumer owners own real projection。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [OBS-01](wasm-observability-timing-metrics.prd.md#sr2-domain-case-1) | 输入 restart窗口/reset、counter回退、zero traffic；断言 checked delta缩窗、reset=true、无fabricated p95 | [5.7 External Summary Script](#obs-summary) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: reset缩窗和bucket percentile是operator诊断算法；scope: 仅本地summary的窗口delta、桶界、latest module/build解释，不决定费用或玩家完成时间 |
| [OBS-02](wasm-observability-timing-metrics.prd.md#sr2-domain-case-2) | 输入 poisoned metrics lock、top11、large/default payload；断言 degraded而主调用继续；top10+truncated；64/128KiB budgets | [5.4 Executor Instrumentation Points](#obs-executor) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: cache路径及compile/call分段计时用于本地归因；scope: 仅计数、失败观测和锁降级记录，不以metrics证明执行结果正确或恢复成功 |
| [OBS-02](wasm-observability-timing-metrics.prd.md#sr2-domain-case-2) | 输入 poisoned metrics lock、top11、large/default payload；断言 degraded而主调用继续；top10+truncated；64/128KiB budgets | [5.8 Cardinality 与 Payload Guardrails](#obs-capacity) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: 本地status数据裁剪是观测容量与隐私约束；scope: 仅topN/ID/payload边界，保留本页单列确定性隔离和原NFR预算，不新增玩家指标 |
| [OBS-02](wasm-observability-timing-metrics.prd.md#sr2-domain-case-2) | 输入 poisoned metrics lock、top11、large/default payload；断言 degraded而主调用继续；top10+truncated；64/128KiB budgets | [5.6 Status Payload Exposure](#obs-status) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: status汇聚诊断快照是只读观测接口；scope: 仅availability、字段暴露与降级信息，不代表finality、可服务状态或world SLA |
| [OBS-03](wasm-observability-timing-metrics.prd.md#sr2-domain-case-3) | 输入 dry_run无timing、repeat0、private payload、timing变化；断言 显式missing/error；无secret；execution ID/charge不变 | [5.2 Build Timing Contract](#obs-build) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: 构建阶段wall-clock记录是诊断成本；scope: 仅total/cargo/canonicalize/hash/写入timing及dry_run缺值，不授予工件发布或世界结果权威 |
| [OBS-03](wasm-observability-timing-metrics.prd.md#sr2-domain-case-3) | 输入 dry_run无timing、repeat0、private payload、timing变化；断言 显式missing/error；无secret；execution ID/charge不变 | [9. Module-local observe 层](#obs-module) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: spec-driven observe runner是模块开发验证工具；scope: 仅spec加载、repeat、case/probe与summary产物，不以局部fixture代签模块业务或世界效果 |
| [OBS-03](wasm-observability-timing-metrics.prd.md#sr2-domain-case-3) | 输入 dry_run无timing、repeat0、private payload、timing变化；断言 显式missing/error；无secret；execution ID/charge不变 | [3. 设计原则](#obs-isolation) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品只承接本行确定性隔离或历史兼容子范围；interface/runtime负责完整执行、激活、历史replay与finality；本页不代签产品AC |
| [SC-1](wasm-observability-timing-metrics.prd.md#sr2-obligation-sc-1) | SC-1: /v1/chain/status.wasm 必须稳定暴露 build/executor/router 三组 machine-readable 指标，并与节点生命周期累计同步。 | [5.3 Runtime Snapshot Contract](#obs-snapshot) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: build最近快照和executor/router进程累计是本地诊断数据结构；scope: 仅snapshot字段/availability组合，不声明节点可服务或世界SLA |
| [SC-1](wasm-observability-timing-metrics.prd.md#sr2-obligation-sc-1) | SC-1: /v1/chain/status.wasm 必须稳定暴露 build/executor/router 三组 machine-readable 指标，并与节点生命周期累计同步。 | [5.6 Status Payload Exposure](#obs-status) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: status汇聚诊断快照是只读观测接口；scope: 仅availability、字段暴露与降级信息，不代表finality、可服务状态或world SLA |
| [SC-2](wasm-observability-timing-metrics.prd.md#sr2-obligation-sc-2) | SC-2: 执行器必须区分 memory cache hit、disk cache hit、compile miss 三条路径，并提供对应的 wall-clock timing 聚合。 | [5.4 Executor Instrumentation Points](#obs-executor) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: cache路径及compile/call分段计时用于本地归因；scope: 仅计数、失败观测和锁降级记录，不以metrics证明执行结果正确或恢复成功 |
| [SC-3](wasm-observability-timing-metrics.prd.md#sr2-obligation-sc-3) | SC-3: build suite 必须输出 total_build_wall_ms 与至少 3 个阶段耗时字段，确保 canonical build 不再只有 hash/size 而无 cost 证据。 | [5.2 Build Timing Contract](#obs-build) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: 构建阶段wall-clock记录是诊断成本；scope: 仅total/cargo/canonicalize/hash/写入timing及dry_run缺值，不授予工件发布或世界结果权威 |
| [SC-4](wasm-observability-timing-metrics.prd.md#sr2-obligation-sc-4) | SC-4: router 必须提供 prepared subscription 与 parse-each-time 的 timing 对比入口，保证 TASK-WORLD_RUNTIME-060 的优化收益能持续复核。 | [5.5 Router Instrumentation Points](#obs-router) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: prepared/fallback/regex计数用于路由性能归因；scope: 仅router本地timing，匹配业务语义与确定性接收仍独立验收 |
| [SC-5](wasm-observability-timing-metrics.prd.md#sr2-obligation-sc-5) | SC-5: 指标设计不得把 trace_id、原始 payload 或无界 module_id 明细直接写入默认 status payload，默认口径需满足 bounded cardinality 与 deterministic isolation。 | [5.8 Cardinality 与 Payload Guardrails](#obs-capacity) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: 本地status数据裁剪是观测容量与隐私约束；scope: 仅topN/ID/payload边界，保留本页单列确定性隔离和原NFR预算，不新增玩家指标 |
| [SC-5](wasm-observability-timing-metrics.prd.md#sr2-obligation-sc-5) | SC-5: 指标设计不得把 trace_id、原始 payload 或无界 module_id 明细直接写入默认 status payload，默认口径需满足 bounded cardinality 与 deterministic isolation。 | [3. 设计原则](#obs-isolation) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品只承接本行确定性隔离或历史兼容子范围；interface/runtime负责完整执行、激活、历史replay与finality；本页不代签产品AC |
| [PRD-WORLD_RUNTIME-036](wasm-observability-timing-metrics.prd.md#sr2-obligation-prd-world_runtime-036) | PRD-WORLD_RUNTIME-036: As a wasm_platform_engineer / runtime_engineer / qa_engineer, I want the WASM build, executor, and router paths to emit bounded cumulative timing metrics and status snapshots, so that hotspot attribution no longer depends on ad hoc logs or ignored local perf probes. | [5.2 Build Timing Contract](#obs-build) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: 构建阶段wall-clock记录是诊断成本；scope: 仅total/cargo/canonicalize/hash/写入timing及dry_run缺值，不授予工件发布或世界结果权威 |
| [PRD-WORLD_RUNTIME-036](wasm-observability-timing-metrics.prd.md#sr2-obligation-prd-world_runtime-036) | PRD-WORLD_RUNTIME-036: As a wasm_platform_engineer / runtime_engineer / qa_engineer, I want the WASM build, executor, and router paths to emit bounded cumulative timing metrics and status snapshots, so that hotspot attribution no longer depends on ad hoc logs or ignored local perf probes. | [5.3 Runtime Snapshot Contract](#obs-snapshot) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: build最近快照和executor/router进程累计是本地诊断数据结构；scope: 仅snapshot字段/availability组合，不声明节点可服务或世界SLA |
| [PRD-WORLD_RUNTIME-036](wasm-observability-timing-metrics.prd.md#sr2-obligation-prd-world_runtime-036) | PRD-WORLD_RUNTIME-036: As a wasm_platform_engineer / runtime_engineer / qa_engineer, I want the WASM build, executor, and router paths to emit bounded cumulative timing metrics and status snapshots, so that hotspot attribution no longer depends on ad hoc logs or ignored local perf probes. | [5.4 Executor Instrumentation Points](#obs-executor) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: cache路径及compile/call分段计时用于本地归因；scope: 仅计数、失败观测和锁降级记录，不以metrics证明执行结果正确或恢复成功 |
| [PRD-WORLD_RUNTIME-036](wasm-observability-timing-metrics.prd.md#sr2-obligation-prd-world_runtime-036) | PRD-WORLD_RUNTIME-036: As a wasm_platform_engineer / runtime_engineer / qa_engineer, I want the WASM build, executor, and router paths to emit bounded cumulative timing metrics and status snapshots, so that hotspot attribution no longer depends on ad hoc logs or ignored local perf probes. | [5.5 Router Instrumentation Points](#obs-router) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: prepared/fallback/regex计数用于路由性能归因；scope: 仅router本地timing，匹配业务语义与确定性接收仍独立验收 |
| [PRD-WORLD_RUNTIME-036](wasm-observability-timing-metrics.prd.md#sr2-obligation-prd-world_runtime-036) | PRD-WORLD_RUNTIME-036: As a wasm_platform_engineer / runtime_engineer / qa_engineer, I want the WASM build, executor, and router paths to emit bounded cumulative timing metrics and status snapshots, so that hotspot attribution no longer depends on ad hoc logs or ignored local perf probes. | [5.6 Status Payload Exposure](#obs-status) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: status汇聚诊断快照是只读观测接口；scope: 仅availability、字段暴露与降级信息，不代表finality、可服务状态或world SLA |
| [PRD-WORLD_RUNTIME-036](wasm-observability-timing-metrics.prd.md#sr2-obligation-prd-world_runtime-036) | PRD-WORLD_RUNTIME-036: As a wasm_platform_engineer / runtime_engineer / qa_engineer, I want the WASM build, executor, and router paths to emit bounded cumulative timing metrics and status snapshots, so that hotspot attribution no longer depends on ad hoc logs or ignored local perf probes. | [5.7 External Summary Script](#obs-summary) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: reset缩窗和bucket percentile是operator诊断算法；scope: 仅本地summary的窗口delta、桶界、latest module/build解释，不决定费用或玩家完成时间 |
| [AC-1](wasm-observability-timing-metrics.prd.md#sr2-obligation-ac-1) | AC-1: tools/wasm_build_suite 必须为 canonical build 输出 total_build_wall_ms 与阶段耗时字段，且 dry-run 与真实构建在 schema 上可区分。 | [5.2 Build Timing Contract](#obs-build) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: 构建阶段wall-clock记录是诊断成本；scope: 仅total/cargo/canonicalize/hash/写入timing及dry_run缺值，不授予工件发布或世界结果权威 |
| [AC-2](wasm-observability-timing-metrics.prd.md#sr2-obligation-ac-2) | AC-2: oasis7_wasm_executor 必须区分 memory-cache hit、disk-cache hit、compile miss 三类路径，并为 compile/deserialize/instantiate/entrypoint/decode 提供累计耗时或固定 bucket。 | [5.4 Executor Instrumentation Points](#obs-executor) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: cache路径及compile/call分段计时用于本地归因；scope: 仅计数、失败观测和锁降级记录，不以metrics证明执行结果正确或恢复成功 |
| [AC-3](wasm-observability-timing-metrics.prd.md#sr2-obligation-ac-3) | AC-3: oasis7_wasm_router 必须输出 subscription prepare 与 filter match 的 timing/counter 指标，至少覆盖 prepared-hit、parse-fallback 与 regex-compile 三类信号。 | [5.5 Router Instrumentation Points](#obs-router) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: prepared/fallback/regex计数用于路由性能归因；scope: 仅router本地timing，匹配业务语义与确定性接收仍独立验收 |
| [AC-4](wasm-observability-timing-metrics.prd.md#sr2-obligation-ac-4) | AC-4: /v1/chain/status 必须新增 wasm section，并沿用 storage/traffic 的共享 snapshot 语义，不得把 timing 指标直接写入世界状态或共识数据。 | [5.6 Status Payload Exposure](#obs-status) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: status汇聚诊断快照是只读观测接口；scope: 仅availability、字段暴露与降级信息，不代表finality、可服务状态或world SLA |
| [AC-4](wasm-observability-timing-metrics.prd.md#sr2-obligation-ac-4) | AC-4: /v1/chain/status 必须新增 wasm section，并沿用 storage/traffic 的共享 snapshot 语义，不得把 timing 指标直接写入世界状态或共识数据。 | [3. 设计原则](#obs-isolation) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品只承接本行确定性隔离或历史兼容子范围；interface/runtime负责完整执行、激活、历史replay与finality；本页不代签产品AC |
| [AC-5](wasm-observability-timing-metrics.prd.md#sr2-obligation-ac-5) | AC-5: 默认 status payload 不得包含 trace_id、原始 input/output bytes、无界 module_id -> metrics map；若需要模块级明细，必须限制为 top-N 或显式 allowlist。 | [5.8 Cardinality 与 Payload Guardrails](#obs-capacity) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: 本地status数据裁剪是观测容量与隐私约束；scope: 仅topN/ID/payload边界，保留本页单列确定性隔离和原NFR预算，不新增玩家指标 |
| [AC-6](wasm-observability-timing-metrics.prd.md#sr2-obligation-ac-6) | AC-6: 节点或 status lock 降级时，degraded_reason 必须显式说明 WASM metrics 不可用，但模块执行本身不得因此失败。 | [5.4 Executor Instrumentation Points](#obs-executor) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: cache路径及compile/call分段计时用于本地归因；scope: 仅计数、失败观测和锁降级记录，不以metrics证明执行结果正确或恢复成功 |
| [AC-6](wasm-observability-timing-metrics.prd.md#sr2-obligation-ac-6) | AC-6: 节点或 status lock 降级时，degraded_reason 必须显式说明 WASM metrics 不可用，但模块执行本身不得因此失败。 | [5.6 Status Payload Exposure](#obs-status) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: status汇聚诊断快照是只读观测接口；scope: 仅availability、字段暴露与降级信息，不代表finality、可服务状态或world SLA |
| [AC-7](wasm-observability-timing-metrics.prd.md#sr2-obligation-ac-7) | AC-7: repo 内必须提供可复用的 summary 入口，把 cumulative snapshot 转成窗口 delta、bucket-derived p50/p95 与热点摘要。 | [5.7 External Summary Script](#obs-summary) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: reset缩窗和bucket percentile是operator诊断算法；scope: 仅本地summary的窗口delta、桶界、latest module/build解释，不决定费用或玩家完成时间 |
| [AC-8](wasm-observability-timing-metrics.prd.md#sr2-obligation-ac-8) | AC-8: 观测设计必须与现有 TASK-WORLD_RUNTIME-060 perf probe、chain-status-traffic-metrics status snapshot 模式、runtime-release-gate-metrics-template 兼容，不得再引入一套平行口径。 | [6. 与现有体系的关系](#obs-relations) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: 复用perf probe和release模板避免平行计时口径；scope: 仅单位、采样和额外开销专业预算，不改QA tier或新增产品SLA |
| [NFR-1](wasm-observability-timing-metrics.prd.md#sr2-obligation-nfr-1) | NFR-1: 默认 /v1/chain/status.wasm payload 必须保持 bounded cardinality，未启用模块级 top-N 时不得随模块总数线性增长。 | [5.8 Cardinality 与 Payload Guardrails](#obs-capacity) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: 本地status数据裁剪是观测容量与隐私约束；scope: 仅topN/ID/payload边界，保留本页单列确定性隔离和原NFR预算，不新增玩家指标 |
| [NFR-2](wasm-observability-timing-metrics.prd.md#sr2-obligation-nfr-2) | NFR-2: 观测写入不得改变 deterministic execution 输出；所有 timing/metrics 仅限本地观测层，不得进入共识数据、world state 或 replay contract。 | [3. 设计原则](#obs-isolation) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品只承接本行确定性隔离或历史兼容子范围；interface/runtime负责完整执行、激活、历史replay与finality；本页不代签产品AC |
| [NFR-3](wasm-observability-timing-metrics.prd.md#sr2-obligation-nfr-3) | NFR-3: 默认状态下，WASM metrics instrumentation 对现有 release perf probe 的额外 wall-clock 开销目标不高于 10%。 | [6. 与现有体系的关系](#obs-relations) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: 复用perf probe和release模板避免平行计时口径；scope: 仅单位、采样和额外开销专业预算，不改QA tier或新增产品SLA |
| [NFR-4](wasm-observability-timing-metrics.prd.md#sr2-obligation-nfr-4) | NFR-4: status payload 在默认配置下必须可在单次请求中稳定序列化，建议预算 <=64 KiB；开启 bounded top-N 明细后建议预算 <=128 KiB。 | [5.8 Cardinality 与 Payload Guardrails](#obs-capacity) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: 本地status数据裁剪是观测容量与隐私约束；scope: 仅topN/ID/payload边界，保留本页单列确定性隔离和原NFR预算，不新增玩家指标 |
| [NFR-5](wasm-observability-timing-metrics.prd.md#sr2-obligation-nfr-5) | NFR-5: 计时字段必须统一使用毫秒或微秒语义并显式标注单位，禁止不同子系统混用无单位整数。 | [5.2 Build Timing Contract](#obs-build) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: 构建阶段wall-clock记录是诊断成本；scope: 仅total/cargo/canonicalize/hash/写入timing及dry_run缺值，不授予工件发布或世界结果权威 |
| [NFR-5](wasm-observability-timing-metrics.prd.md#sr2-obligation-nfr-5) | NFR-5: 计时字段必须统一使用毫秒或微秒语义并显式标注单位，禁止不同子系统混用无单位整数。 | [5.3 Runtime Snapshot Contract](#obs-snapshot) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: build最近快照和executor/router进程累计是本地诊断数据结构；scope: 仅snapshot字段/availability组合，不声明节点可服务或世界SLA |
| [NFR-6](wasm-observability-timing-metrics.prd.md#sr2-obligation-nfr-6) | NFR-6: 模块级明细若存在，默认上限不得超过 top 10，且不得包含原始 payload 内容。 | [5.8 Cardinality 与 Payload Guardrails](#obs-capacity) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: 本地status数据裁剪是观测容量与隐私约束；scope: 仅topN/ID/payload边界，保留本页单列确定性隔离和原NFR预算，不新增玩家指标 |
| [PRD-WORLD_RUNTIME-037](wasm-observability-timing-metrics.prd.md#sr2-obligation-prd-world_runtime-037) | &#124; PRD-WORLD_RUNTIME-037 &#124; wasm-module-observability-standardization &#124; test_tier_required &#124; observe runner tests、代表模块 spec、wrapper shell check、JSON/Markdown summary &#124; 模块级 contract/perf 证据与新模块接入 &#124; | [9. Module-local observe 层](#obs-module) | wasm_platform_engineer负责所列专业条款；runtime接收status/隔离，QA保存同candidate证据 | 产品 N/A；reason: spec-driven observe runner是模块开发验证工具；scope: 仅spec加载、repeat、case/probe与summary产物，不以局部fixture代签模块业务或世界效果 |

## 3. 当前状态、目标状态与差距

| 对象 | 当前状态 / 基线 | 目标 | 差距与证据边界 |
| --- | --- | --- | --- |
| wasm-1 foundation | 原有代码/契约保留，源commit见身份块 | 兼容consumer及精确target contracts | 原有tests为located，不是本次run |
| receipt / migration / gate | interface仍声明target或partial | `sr2-metrics-contract` 与runtime authority相连 | 代码、full finality/provider/headed证明不在本任务 |
| 文档与schema | 本文目标合同 | 语义/结构/anchors检查 | 文档通过不意味着runtime pass或SC9/DC5闭合 |

## 4. 边界与结构

输入声明和provider response为advisory，host/runtime负责authority/admission；sandbox仅确定性计算。build证明字节来源、SDK证明codec、metrics证明local诊断，各自不能签署world finality。ContentRef必须fetch/hash/length验证，再由相应authority验trust；current caches与local signature不能越过生产许可。

## 5. 关键运行流程

先绑定candidate/world branch/window与exact manifest/artifact/schema；strict decode和limits preflight后验证authority。成功只进入有权的下一阶段，世界效果须canonical commit receipt。缺字段、版本、proof、兼容或head continuity即停止，不隐式fallback或重试另一身份。重试同request hash幂等，changed hash拒绝；恢复重读当前canonical边界，历史receipt按记录版本重放。

## 6. 接口与数据合同

<a id="sr2-metrics-contract"></a>
### 6.1 本地 observation window 合同

WasmObservationWindowV1 是 target diagnostic projection：`{schema_version:"WasmObservationWindowV1",node_id:Id,process_window_id:Id,observed_since_unix_ms:U64,sample_start_unix_ms:U64,sample_end_unix_ms:U64,metrics_available:bool,degraded_reason?:Id,window_reset_detected:bool,truncated:bool,delta_calls:U64,delta_compile_ms:U64,delta_timeout_count:U64,executor_p50_call_ms?:U64,executor_p95_call_ms?:U64,router_p50_match_ms?:U64,router_p95_match_ms?:U64,top_hotspots:[{module_id:Id,match_ms:U64}]}`。新 projection 的 U64 JSON 为 decimal strings，现行 metrics endpoint number format 不在本文偷偷改变。所有值是毫秒/计数的本地诊断，不进入 execution/receipt/manifest consensus core 或计费。

同 node/process 窗口以 cumulative snapshot 求 checked delta；observed_since 改变或 counter 回退即缩窗且标 reset，不跨 restart 求负 delta。start<=end；no traffic 样本不给 percentile；bucket-derived percentile 只能从同窗口固定 buckets 计算且注明 bucket 精度，不能伪造精确 latency。metrics lock poisoned/unavailable 返回 bounded degraded reason，执行主路径继续；top_hotspots<=10，裁剪显式 truncated，禁止 raw payload/trace/keys/auth proof。默认 status<=64KiB、top-N<=128KiB、instrumentation overhead目标<=10% 沿用 PRD NFR，不用本次未运行宣称达标。

当前 executor `call_wall_ms_buckets` 与 router `prepare_ms_buckets` / `match_ms_buckets` 按首个满足 `elapsed_ms <= threshold` 的桶递增，因此每个 label 表示互斥频数；有限上界依次为 1、5、10、25、50、100、250、500、1000ms，首桶包含 `[0,1]`，后续有限桶依次为 `(previous_threshold, upper_threshold]`，`gt_1000_ms` 为 `>1000`。不能按 cumulative upper-bound counts 解释。仓库本地 monitor 对同窗各桶取非负 delta，将有限桶与 overflow delta 求和为样本数 `N`，p50/p95 使用 nearest-rank `r=ceil(N*p/100)=(N*p+99)//100`，按阈值顺序累加频数至首个达到 `r` 的桶；输出桶边界和全窗 `samples=N`，不插值。`N=0` 时 percentile 为空；rank 落入 overflow 时上界为空、下界为 1000ms。`observed_since_unix_ms` 改变或已有被监测 aggregate counter 回退时，reset 样本成为新窗口基线，不跨 reset 计数；monitor 不另行推断 histogram-map 回退。

以上 p50/p95 是本地、未版本化 monitor summary 的派生值，沿用 `window.executor.p50_call_ms` / `p95_call_ms`、`window.router.p50_match_ms` / `p95_match_ms` 路径及 `{bucket_label,upper_bound_ms,lower_bound_ms,samples}` 对象形状。status snapshot 的 producer 字段及 JSON 数字格式保持现状；本地 monitor 修复不会激活 `WasmObservationWindowV1`、加入 status schema/version 字段或改变 endpoint 表示。`WasmObservationWindowV1` 仍是后续 consumer 显式协商的 target diagnostic projection。

当前源码复核于 `f97f2253cdcb163bf984e3c90d59d792fe1c9b19`（2026-10-15），仅静态核读。当前已有 code locations 为 build suite timing、executor/router shared snapshots、status_payload.wasm 和 monitor/module_observe。它们仅 located implementation anchors：`tools/wasm_build_suite/src/lib.rs`、`crates/oasis7_wasm_executor/src/lib.rs`、`crates/oasis7_wasm_router/src/lib.rs`、`crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/status_payload.rs`、`scripts/oasis7-node-wasm-metrics-monitor.test.sh`、`tools/wasm_module_observe`；本次未执行。module observe contract/perf 不等于 functional/release/DC5 proof。观测 restart/reset、zero traffic、degraded lock、top11 clipping、dry_run missing timing、repeat0 rejection、changed locator equal bytes 必须各有 fixture。新字段 consumer 显式版本协商；rollback 原 reader 不能静默误解新版本，也不得影响执行状态。


## 7. 状态、事务与持久化

本域记录不能把 accepted/built/decoded/observed 当 applied。SDK/build/metrics 的本地输出不拥有世界事务；interface定义receipt core，runtime staged commit拥有state/effects/charges/registry/journal/winner同一发布点。失败全体不发布，crash在prepublish与postpublish由durable receipt/winner/index恢复，禁止重跑已commit WASM。实际prototype atomic seam仅bounded prepared proposal，不能推导完整事务。snapshot保留原manifest/artifact/schedule/lineage与历史bytes。

## 8. 部署、安全与运行约束

生产binary-only及external finality仍由release/runtime policy；无Docker/wasm32/provider环境不能伪报通过。modules无clock/random/I/O，host注入caller/time/target；namespace/capability/AUTHZ约束全部仍有效。计数、字节和费用checked bounds-before-allocation/mutation，limit来源为现行ModuleLimits与各paired PRD；diagnostics不得泄露raw payload/keys/proof。

## 9. 质量与容量

| 环境 / 输入 | 预期响应 | 阈值来源 | 验证入口 / 当前范围 |
| --- | --- | --- | --- |
| required canonical fixture / malformed bytes、integer overflow、wrongscope | structured rejection且不发布state/effect/charge | interface类型、现行limits及paired PRD | §11.1各case；target设计未运行 |
| full实际canonical环境 / restart、activation、proof撤销 | durable replay/identity与no-new-effect gate一致 | runtime/GWSC各精确receiver | samecandidate artifacts mandatory，未证明 |
| 现行资源/perf输入 | 不扩大既有budget，不用localtiming计费 | 下方原有limits、NFR与perf方案 | 既有tests/scripts仅located partial |

## 10. 兼容、迁移与回滚

保留legacy serde defaults、wire exports、工件不可覆盖与历史bytes。新target先固定合同再逐consumer启用，unknown version拒绝；当前ABI不变。回滚到身份块源码/旧consumer，仅影响新请求；已canonical commit的事件、charges、receipt不可撤销或改价。迁移须explicit from/to descriptor及governed artifact；缺proof停在旧版，不能以代码合入或健康恢复启用。

## 11. 验证设计与可追溯性

本次文档变更只交付文档合同采纳与local checks；实际实现、runtime required/full、provider、headful、production proof分别在真实实现task evidence记录candidate/source/integration/tested tree与环境，缺失保持未证明，不能用FutureIssue占位代替本页receiver。全DC5验证由 [GWSC receiver](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-dc5-attachment) 与其 [schema](../../testing/schemas/world-execution-state-sync-attachment.schema.json) 拥有。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [OBS-01](wasm-observability-timing-metrics.prd.md#sr2-domain-case-1) | [5.7 External Summary Script](#obs-summary) | checked delta缩窗、reset=true、无fabricated p95 | [实施验证手册](../../../testing-manual.md)；`scripts/oasis7-node-wasm-metrics-monitor.test.sh` (monitor tests)：check_reset_window/check_exclusive_bucket_percentiles/check_single_sample_compat/check_unavailable_metrics_disable_window；核reset后delta、nearest-rank桶界、无流量null、单样本禁delta及不可用说明；check_module_hotspot_window只断言latest cumulative topN；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [OBS-02](wasm-observability-timing-metrics.prd.md#sr2-domain-case-2) | [5.4 Executor Instrumentation Points](#obs-executor) | degraded而主调用继续；top10+truncated；64/128KiB budgets | [实施验证手册](../../../testing-manual.md)；`crates/oasis7_wasm_executor/src/metrics.rs` (executor metrics) observe_wasm_executor_compile/call_result 静态核对；实施时分别触发 memory/disk/miss、失败与poisoned lock，对照各stage delta及主调用结果，未取得本轮运行证据；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [OBS-02](wasm-observability-timing-metrics.prd.md#sr2-domain-case-2) | [5.8 Cardinality 与 Payload Guardrails](#obs-capacity) | degraded而主调用继续；top10+truncated；64/128KiB budgets | [实施验证手册](../../../testing-manual.md)；`crates/oasis7_wasm_executor/src/metrics.rs` (executor metrics) top10/ID边界静态核对；实施时默认及top11/超长ID/大payload序列化实际测64/128KiB、truncated与隐私字段，并以同candidate release perf probe复核<=10%；monitor fixture不证明这些门槛；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [OBS-02](wasm-observability-timing-metrics.prd.md#sr2-domain-case-2) | [5.6 Status Payload Exposure](#obs-status) | degraded而主调用继续；top10+truncated；64/128KiB budgets | [实施验证手册](../../../testing-manual.md)；`crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/status_payload.rs` (status payload) 与 `crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/wasm_status.rs` (wasm status tests)；真实HTTP序列化、poisoned lock后module继续执行及与storage/traffic一致性须实施验证，局部compose不代签；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [OBS-03](wasm-observability-timing-metrics.prd.md#sr2-domain-case-3) | [5.2 Build Timing Contract](#obs-build) | 显式missing/error；无secret；execution ID/charge不变 | [实施验证手册](../../../testing-manual.md)；`tools/wasm_build_suite/src/lib.rs` (build suite) 的 run_build/dry_run 分支与 BuildTimingSnapshot；后续同 candidate 真实构建核对 metadata/receipt 与返回值，dry_run 必须无真实 timing；现有参数/路径/canonicalize 单测不证明 timing 全链或跨宿主；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [OBS-03](wasm-observability-timing-metrics.prd.md#sr2-domain-case-3) | [9. Module-local observe 层](#obs-module) | 显式missing/error；无secret；execution ID/charge不变 | [实施验证手册](../../../testing-manual.md)；`tools/wasm_module_observe/src/spec.rs` (spec tests) load_spec_resolves_manifest_relative_to_spec/load_spec_rejects_zero_repeat/load_spec_rejects_unknown_schema_version；`tools/wasm_module_observe/tests/m1_rule_move.rs` (m1_rule_move tests) 三case/两probe、case/router/prepare失败artifact；缺wasm32显式skip，不作pass；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [OBS-03](wasm-observability-timing-metrics.prd.md#sr2-domain-case-3) | [3. 设计原则](#obs-isolation) | 显式missing/error；无secret；execution ID/charge不变 | [实施验证手册](../../../testing-manual.md)；对照 `wasm-interface.md#sr2-types` (interface types) 的execution/receipt边界，实施同candidate同输入仅改变timing开关，比较world/replay/执行ID/charge不变，并检查默认status无secret；源码字段及本地summary测试不能代签确定性隔离全链；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [SC-1](wasm-observability-timing-metrics.prd.md#sr2-obligation-sc-1) | [5.3 Runtime Snapshot Contract](#obs-snapshot) | SC-1: /v1/chain/status.wasm 必须稳定暴露 build/executor/router 三组 machine-readable 指标，并与节点生命周期累计同步。 | [实施验证手册](../../../testing-manual.md)；`crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/wasm_status.rs` (status owner) 的 compose_chain_wasm_status_surfaces_partial_build_degradation、load_build_metrics_payload_reads_build_timing_snapshot、load_build_metrics_payload_reports_parse_failure；验证 partial availability、6项ms字段和显式解析失败，不证明真实节点HTTP全链；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [SC-1](wasm-observability-timing-metrics.prd.md#sr2-obligation-sc-1) | [5.6 Status Payload Exposure](#obs-status) | SC-1: /v1/chain/status.wasm 必须稳定暴露 build/executor/router 三组 machine-readable 指标，并与节点生命周期累计同步。 | [实施验证手册](../../../testing-manual.md)；`crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/status_payload.rs` (status payload) 与 `crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/wasm_status.rs` (wasm status tests)；真实HTTP序列化、poisoned lock后module继续执行及与storage/traffic一致性须实施验证，局部compose不代签；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [SC-2](wasm-observability-timing-metrics.prd.md#sr2-obligation-sc-2) | [5.4 Executor Instrumentation Points](#obs-executor) | SC-2: 执行器必须区分 memory cache hit、disk cache hit、compile miss 三条路径，并提供对应的 wall-clock timing 聚合。 | [实施验证手册](../../../testing-manual.md)；`crates/oasis7_wasm_executor/src/metrics.rs` (executor metrics) observe_wasm_executor_compile/call_result 静态核对；实施时分别触发 memory/disk/miss、失败与poisoned lock，对照各stage delta及主调用结果，未取得本轮运行证据；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [SC-3](wasm-observability-timing-metrics.prd.md#sr2-obligation-sc-3) | [5.2 Build Timing Contract](#obs-build) | SC-3: build suite 必须输出 total_build_wall_ms 与至少 3 个阶段耗时字段，确保 canonical build 不再只有 hash/size 而无 cost 证据。 | [实施验证手册](../../../testing-manual.md)；`tools/wasm_build_suite/src/lib.rs` (build suite) 的 run_build/dry_run 分支与 BuildTimingSnapshot；后续同 candidate 真实构建核对 metadata/receipt 与返回值，dry_run 必须无真实 timing；现有参数/路径/canonicalize 单测不证明 timing 全链或跨宿主；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [SC-4](wasm-observability-timing-metrics.prd.md#sr2-obligation-sc-4) | [5.5 Router Instrumentation Points](#obs-router) | SC-4: router 必须提供 prepared subscription 与 parse-each-time 的 timing 对比入口，保证 TASK-WORLD_RUNTIME-060 的优化收益能持续复核。 | [实施验证手册](../../../testing-manual.md)；`crates/oasis7_wasm_router/src/metrics.rs` (router metrics) prepare/match/regex计数；`tools/wasm_module_observe/tests/m1_rule_move.rs` (代表 observe tests) 读取prepared/fallback与失败artifact断言，具体regex分支和性能差值仍需同candidate验证；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [SC-5](wasm-observability-timing-metrics.prd.md#sr2-obligation-sc-5) | [5.8 Cardinality 与 Payload Guardrails](#obs-capacity) | SC-5: 指标设计不得把 trace_id、原始 payload 或无界 module_id 明细直接写入默认 status payload，默认口径需满足 bounded cardinality 与 deterministic isolation。 | [实施验证手册](../../../testing-manual.md)；`crates/oasis7_wasm_executor/src/metrics.rs` (executor metrics) top10/ID边界静态核对；实施时默认及top11/超长ID/大payload序列化实际测64/128KiB、truncated与隐私字段，并以同candidate release perf probe复核<=10%；monitor fixture不证明这些门槛；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [SC-5](wasm-observability-timing-metrics.prd.md#sr2-obligation-sc-5) | [3. 设计原则](#obs-isolation) | SC-5: 指标设计不得把 trace_id、原始 payload 或无界 module_id 明细直接写入默认 status payload，默认口径需满足 bounded cardinality 与 deterministic isolation。 | [实施验证手册](../../../testing-manual.md)；对照 `wasm-interface.md#sr2-types` (interface types) 的execution/receipt边界，实施同candidate同输入仅改变timing开关，比较world/replay/执行ID/charge不变，并检查默认status无secret；源码字段及本地summary测试不能代签确定性隔离全链；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [PRD-WORLD_RUNTIME-036](wasm-observability-timing-metrics.prd.md#sr2-obligation-prd-world_runtime-036) | [5.2 Build Timing Contract](#obs-build) | PRD-WORLD_RUNTIME-036: As a wasm_platform_engineer / runtime_engineer / qa_engineer, I want the WASM build, executor, and router paths to emit bounded cumulative timing metrics and status snapshots, so that hotspot attribution no longer depends on ad hoc logs or ignored local perf probes. | [实施验证手册](../../../testing-manual.md)；`tools/wasm_build_suite/src/lib.rs` (build suite) 的 run_build/dry_run 分支与 BuildTimingSnapshot；后续同 candidate 真实构建核对 metadata/receipt 与返回值，dry_run 必须无真实 timing；现有参数/路径/canonicalize 单测不证明 timing 全链或跨宿主；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [PRD-WORLD_RUNTIME-036](wasm-observability-timing-metrics.prd.md#sr2-obligation-prd-world_runtime-036) | [5.3 Runtime Snapshot Contract](#obs-snapshot) | PRD-WORLD_RUNTIME-036: As a wasm_platform_engineer / runtime_engineer / qa_engineer, I want the WASM build, executor, and router paths to emit bounded cumulative timing metrics and status snapshots, so that hotspot attribution no longer depends on ad hoc logs or ignored local perf probes. | [实施验证手册](../../../testing-manual.md)；`crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/wasm_status.rs` (status owner) 的 compose_chain_wasm_status_surfaces_partial_build_degradation、load_build_metrics_payload_reads_build_timing_snapshot、load_build_metrics_payload_reports_parse_failure；验证 partial availability、6项ms字段和显式解析失败，不证明真实节点HTTP全链；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [PRD-WORLD_RUNTIME-036](wasm-observability-timing-metrics.prd.md#sr2-obligation-prd-world_runtime-036) | [5.4 Executor Instrumentation Points](#obs-executor) | PRD-WORLD_RUNTIME-036: As a wasm_platform_engineer / runtime_engineer / qa_engineer, I want the WASM build, executor, and router paths to emit bounded cumulative timing metrics and status snapshots, so that hotspot attribution no longer depends on ad hoc logs or ignored local perf probes. | [实施验证手册](../../../testing-manual.md)；`crates/oasis7_wasm_executor/src/metrics.rs` (executor metrics) observe_wasm_executor_compile/call_result 静态核对；实施时分别触发 memory/disk/miss、失败与poisoned lock，对照各stage delta及主调用结果，未取得本轮运行证据；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [PRD-WORLD_RUNTIME-036](wasm-observability-timing-metrics.prd.md#sr2-obligation-prd-world_runtime-036) | [5.5 Router Instrumentation Points](#obs-router) | PRD-WORLD_RUNTIME-036: As a wasm_platform_engineer / runtime_engineer / qa_engineer, I want the WASM build, executor, and router paths to emit bounded cumulative timing metrics and status snapshots, so that hotspot attribution no longer depends on ad hoc logs or ignored local perf probes. | [实施验证手册](../../../testing-manual.md)；`crates/oasis7_wasm_router/src/metrics.rs` (router metrics) prepare/match/regex计数；`tools/wasm_module_observe/tests/m1_rule_move.rs` (代表 observe tests) 读取prepared/fallback与失败artifact断言，具体regex分支和性能差值仍需同candidate验证；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [PRD-WORLD_RUNTIME-036](wasm-observability-timing-metrics.prd.md#sr2-obligation-prd-world_runtime-036) | [5.6 Status Payload Exposure](#obs-status) | PRD-WORLD_RUNTIME-036: As a wasm_platform_engineer / runtime_engineer / qa_engineer, I want the WASM build, executor, and router paths to emit bounded cumulative timing metrics and status snapshots, so that hotspot attribution no longer depends on ad hoc logs or ignored local perf probes. | [实施验证手册](../../../testing-manual.md)；`crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/status_payload.rs` (status payload) 与 `crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/wasm_status.rs` (wasm status tests)；真实HTTP序列化、poisoned lock后module继续执行及与storage/traffic一致性须实施验证，局部compose不代签；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [PRD-WORLD_RUNTIME-036](wasm-observability-timing-metrics.prd.md#sr2-obligation-prd-world_runtime-036) | [5.7 External Summary Script](#obs-summary) | PRD-WORLD_RUNTIME-036: As a wasm_platform_engineer / runtime_engineer / qa_engineer, I want the WASM build, executor, and router paths to emit bounded cumulative timing metrics and status snapshots, so that hotspot attribution no longer depends on ad hoc logs or ignored local perf probes. | [实施验证手册](../../../testing-manual.md)；`scripts/oasis7-node-wasm-metrics-monitor.test.sh` (monitor tests)：check_reset_window/check_exclusive_bucket_percentiles/check_single_sample_compat/check_unavailable_metrics_disable_window；核reset后delta、nearest-rank桶界、无流量null、单样本禁delta及不可用说明；check_module_hotspot_window只断言latest cumulative topN；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [AC-1](wasm-observability-timing-metrics.prd.md#sr2-obligation-ac-1) | [5.2 Build Timing Contract](#obs-build) | AC-1: tools/wasm_build_suite 必须为 canonical build 输出 total_build_wall_ms 与阶段耗时字段，且 dry-run 与真实构建在 schema 上可区分。 | [实施验证手册](../../../testing-manual.md)；`tools/wasm_build_suite/src/lib.rs` (build suite) 的 run_build/dry_run 分支与 BuildTimingSnapshot；后续同 candidate 真实构建核对 metadata/receipt 与返回值，dry_run 必须无真实 timing；现有参数/路径/canonicalize 单测不证明 timing 全链或跨宿主；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [AC-2](wasm-observability-timing-metrics.prd.md#sr2-obligation-ac-2) | [5.4 Executor Instrumentation Points](#obs-executor) | AC-2: oasis7_wasm_executor 必须区分 memory-cache hit、disk-cache hit、compile miss 三类路径，并为 compile/deserialize/instantiate/entrypoint/decode 提供累计耗时或固定 bucket。 | [实施验证手册](../../../testing-manual.md)；`crates/oasis7_wasm_executor/src/metrics.rs` (executor metrics) observe_wasm_executor_compile/call_result 静态核对；实施时分别触发 memory/disk/miss、失败与poisoned lock，对照各stage delta及主调用结果，未取得本轮运行证据；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [AC-3](wasm-observability-timing-metrics.prd.md#sr2-obligation-ac-3) | [5.5 Router Instrumentation Points](#obs-router) | AC-3: oasis7_wasm_router 必须输出 subscription prepare 与 filter match 的 timing/counter 指标，至少覆盖 prepared-hit、parse-fallback 与 regex-compile 三类信号。 | [实施验证手册](../../../testing-manual.md)；`crates/oasis7_wasm_router/src/metrics.rs` (router metrics) prepare/match/regex计数；`tools/wasm_module_observe/tests/m1_rule_move.rs` (代表 observe tests) 读取prepared/fallback与失败artifact断言，具体regex分支和性能差值仍需同candidate验证；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [AC-4](wasm-observability-timing-metrics.prd.md#sr2-obligation-ac-4) | [5.6 Status Payload Exposure](#obs-status) | AC-4: /v1/chain/status 必须新增 wasm section，并沿用 storage/traffic 的共享 snapshot 语义，不得把 timing 指标直接写入世界状态或共识数据。 | [实施验证手册](../../../testing-manual.md)；`crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/status_payload.rs` (status payload) 与 `crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/wasm_status.rs` (wasm status tests)；真实HTTP序列化、poisoned lock后module继续执行及与storage/traffic一致性须实施验证，局部compose不代签；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [AC-4](wasm-observability-timing-metrics.prd.md#sr2-obligation-ac-4) | [3. 设计原则](#obs-isolation) | AC-4: /v1/chain/status 必须新增 wasm section，并沿用 storage/traffic 的共享 snapshot 语义，不得把 timing 指标直接写入世界状态或共识数据。 | [实施验证手册](../../../testing-manual.md)；对照 `wasm-interface.md#sr2-types` (interface types) 的execution/receipt边界，实施同candidate同输入仅改变timing开关，比较world/replay/执行ID/charge不变，并检查默认status无secret；源码字段及本地summary测试不能代签确定性隔离全链；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [AC-5](wasm-observability-timing-metrics.prd.md#sr2-obligation-ac-5) | [5.8 Cardinality 与 Payload Guardrails](#obs-capacity) | AC-5: 默认 status payload 不得包含 trace_id、原始 input/output bytes、无界 module_id -> metrics map；若需要模块级明细，必须限制为 top-N 或显式 allowlist。 | [实施验证手册](../../../testing-manual.md)；`crates/oasis7_wasm_executor/src/metrics.rs` (executor metrics) top10/ID边界静态核对；实施时默认及top11/超长ID/大payload序列化实际测64/128KiB、truncated与隐私字段，并以同candidate release perf probe复核<=10%；monitor fixture不证明这些门槛；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [AC-6](wasm-observability-timing-metrics.prd.md#sr2-obligation-ac-6) | [5.4 Executor Instrumentation Points](#obs-executor) | AC-6: 节点或 status lock 降级时，degraded_reason 必须显式说明 WASM metrics 不可用，但模块执行本身不得因此失败。 | [实施验证手册](../../../testing-manual.md)；`crates/oasis7_wasm_executor/src/metrics.rs` (executor metrics) observe_wasm_executor_compile/call_result 静态核对；实施时分别触发 memory/disk/miss、失败与poisoned lock，对照各stage delta及主调用结果，未取得本轮运行证据；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [AC-6](wasm-observability-timing-metrics.prd.md#sr2-obligation-ac-6) | [5.6 Status Payload Exposure](#obs-status) | AC-6: 节点或 status lock 降级时，degraded_reason 必须显式说明 WASM metrics 不可用，但模块执行本身不得因此失败。 | [实施验证手册](../../../testing-manual.md)；`crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/status_payload.rs` (status payload) 与 `crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/wasm_status.rs` (wasm status tests)；真实HTTP序列化、poisoned lock后module继续执行及与storage/traffic一致性须实施验证，局部compose不代签；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [AC-7](wasm-observability-timing-metrics.prd.md#sr2-obligation-ac-7) | [5.7 External Summary Script](#obs-summary) | AC-7: repo 内必须提供可复用的 summary 入口，把 cumulative snapshot 转成窗口 delta、bucket-derived p50/p95 与热点摘要。 | [实施验证手册](../../../testing-manual.md)；`scripts/oasis7-node-wasm-metrics-monitor.test.sh` (monitor tests)：check_reset_window/check_exclusive_bucket_percentiles/check_single_sample_compat/check_unavailable_metrics_disable_window；核reset后delta、nearest-rank桶界、无流量null、单样本禁delta及不可用说明；check_module_hotspot_window只断言latest cumulative topN；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [AC-8](wasm-observability-timing-metrics.prd.md#sr2-obligation-ac-8) | [6. 与现有体系的关系](#obs-relations) | AC-8: 观测设计必须与现有 TASK-WORLD_RUNTIME-060 perf probe、chain-status-traffic-metrics status snapshot 模式、runtime-release-gate-metrics-template 兼容，不得再引入一套平行口径。 | [实施验证手册](../../../testing-manual.md)；对照原TASK-WORLD_RUNTIME-060 perf probe、chain-status-traffic-metrics及runtime-release-gate模板字段与单位；`scripts/oasis7-node-wasm-metrics-monitor.test.sh` (monitor tests)只验本地summary，实施同candidate测额外开销与输出隔离，不能用文本一致性证明性能；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [NFR-1](wasm-observability-timing-metrics.prd.md#sr2-obligation-nfr-1) | [5.8 Cardinality 与 Payload Guardrails](#obs-capacity) | NFR-1: 默认 /v1/chain/status.wasm payload 必须保持 bounded cardinality，未启用模块级 top-N 时不得随模块总数线性增长。 | [实施验证手册](../../../testing-manual.md)；`crates/oasis7_wasm_executor/src/metrics.rs` (executor metrics) top10/ID边界静态核对；实施时默认及top11/超长ID/大payload序列化实际测64/128KiB、truncated与隐私字段，并以同candidate release perf probe复核<=10%；monitor fixture不证明这些门槛；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [NFR-2](wasm-observability-timing-metrics.prd.md#sr2-obligation-nfr-2) | [3. 设计原则](#obs-isolation) | NFR-2: 观测写入不得改变 deterministic execution 输出；所有 timing/metrics 仅限本地观测层，不得进入共识数据、world state 或 replay contract。 | [实施验证手册](../../../testing-manual.md)；对照 `wasm-interface.md#sr2-types` (interface types) 的execution/receipt边界，实施同candidate同输入仅改变timing开关，比较world/replay/执行ID/charge不变，并检查默认status无secret；源码字段及本地summary测试不能代签确定性隔离全链；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [NFR-3](wasm-observability-timing-metrics.prd.md#sr2-obligation-nfr-3) | [6. 与现有体系的关系](#obs-relations) | NFR-3: 默认状态下，WASM metrics instrumentation 对现有 release perf probe 的额外 wall-clock 开销目标不高于 10%。 | [实施验证手册](../../../testing-manual.md)；对照原TASK-WORLD_RUNTIME-060 perf probe、chain-status-traffic-metrics及runtime-release-gate模板字段与单位；`scripts/oasis7-node-wasm-metrics-monitor.test.sh` (monitor tests)只验本地summary，实施同candidate测额外开销与输出隔离，不能用文本一致性证明性能；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [NFR-4](wasm-observability-timing-metrics.prd.md#sr2-obligation-nfr-4) | [5.8 Cardinality 与 Payload Guardrails](#obs-capacity) | NFR-4: status payload 在默认配置下必须可在单次请求中稳定序列化，建议预算 <=64 KiB；开启 bounded top-N 明细后建议预算 <=128 KiB。 | [实施验证手册](../../../testing-manual.md)；`crates/oasis7_wasm_executor/src/metrics.rs` (executor metrics) top10/ID边界静态核对；实施时默认及top11/超长ID/大payload序列化实际测64/128KiB、truncated与隐私字段，并以同candidate release perf probe复核<=10%；monitor fixture不证明这些门槛；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [NFR-5](wasm-observability-timing-metrics.prd.md#sr2-obligation-nfr-5) | [5.2 Build Timing Contract](#obs-build) | NFR-5: 计时字段必须统一使用毫秒或微秒语义并显式标注单位，禁止不同子系统混用无单位整数。 | [实施验证手册](../../../testing-manual.md)；`tools/wasm_build_suite/src/lib.rs` (build suite) 的 run_build/dry_run 分支与 BuildTimingSnapshot；后续同 candidate 真实构建核对 metadata/receipt 与返回值，dry_run 必须无真实 timing；现有参数/路径/canonicalize 单测不证明 timing 全链或跨宿主；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [NFR-5](wasm-observability-timing-metrics.prd.md#sr2-obligation-nfr-5) | [5.3 Runtime Snapshot Contract](#obs-snapshot) | NFR-5: 计时字段必须统一使用毫秒或微秒语义并显式标注单位，禁止不同子系统混用无单位整数。 | [实施验证手册](../../../testing-manual.md)；`crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/wasm_status.rs` (status owner) 的 compose_chain_wasm_status_surfaces_partial_build_degradation、load_build_metrics_payload_reads_build_timing_snapshot、load_build_metrics_payload_reports_parse_failure；验证 partial availability、6项ms字段和显式解析失败，不证明真实节点HTTP全链；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [NFR-6](wasm-observability-timing-metrics.prd.md#sr2-obligation-nfr-6) | [5.8 Cardinality 与 Payload Guardrails](#obs-capacity) | NFR-6: 模块级明细若存在，默认上限不得超过 top 10，且不得包含原始 payload 内容。 | [实施验证手册](../../../testing-manual.md)；`crates/oasis7_wasm_executor/src/metrics.rs` (executor metrics) top10/ID边界静态核对；实施时默认及top11/超长ID/大payload序列化实际测64/128KiB、truncated与隐私字段，并以同candidate release perf probe复核<=10%；monitor fixture不证明这些门槛；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |
| [PRD-WORLD_RUNTIME-037](wasm-observability-timing-metrics.prd.md#sr2-obligation-prd-world_runtime-037) | [9. Module-local observe 层](#obs-module) | &#124; PRD-WORLD_RUNTIME-037 &#124; wasm-module-observability-standardization &#124; test_tier_required &#124; observe runner tests、代表模块 spec、wrapper shell check、JSON/Markdown summary &#124; 模块级 contract/perf 证据与新模块接入 &#124; | [实施验证手册](../../../testing-manual.md)；`tools/wasm_module_observe/src/spec.rs` (spec tests) load_spec_resolves_manifest_relative_to_spec/load_spec_rejects_zero_repeat/load_spec_rejects_unknown_schema_version；`tools/wasm_module_observe/tests/m1_rule_move.rs` (m1_rule_move tests) 三case/两probe、case/router/prepare失败artifact；缺wasm32显式skip，不作pass；同candidate按本行专业范围选required/full，实际命令/结果由实施证据记录，本轮未运行 | 实施证据记录具体case、candidate/source、features/target、输入、命令、exit及产物；本次仅文档 | 未证明payload/perf、真实节点隔离、full finality或release |

现有partial/unrun来源：`crates/oasis7_wasm_executor/src/tests.rs`（output/fuel/memory/cache），`crates/oasis7_wasm_abi/tests/open_module_commands.rs`（legacy/envelope），`crates/oasis7_runtime/src/world/module_runtime_tests.rs`（metrics/due-preflight），`scripts/oasis7-node-wasm-metrics-monitor.test.sh`（window summary），`scripts/ci-verify-m1-wasm-summaries.py`（build对账）。这些不是新target receipt/migration/DC5已覆盖证据。GWSC [negative matrix](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-qa-negative-matrix) 接收全部17QA cases，[runtime scenarios](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-runtime-scenarios) 接收10组runtime；本页domain rows为具体输入/assertion分配。

## 12. 决策、长期风险与未决问题

选择单authority typedtarget而非重复wire，是为避免scope/hash/版本漂移。已知风险是implementation缺失、proof不可读、U64/UTF8投影误读、递归hash与历史误作新候选证据；WASM/runtime/QA分别按本页case和runtimereceiver解除。触发器为新ABI/schedule/schema、trust epoch、activation或consumer变更；需重新冻结合同与专业联审。SC9八fullcells及activeLLM/provider API parity、真实desktop+narrow/headed证据保持产品/QAauthority，不因本页文档完成消除。


- 对应需求文档: `doc/world-runtime/wasm/wasm-observability-timing-metrics.prd.md`
- 稳定证据入口: `doc/world-runtime/wasm/evidence.md`

审计轮次: 1

## 1. 设计定位
本设计把 WASM 体系里“只能靠 ignored perf probe 和临时日志回答”的性能问题，收敛为正式 runtime 观测面。目标不是替代本地 benchmark，而是提供一条长期可复用的 `build -> executor -> router -> status -> summary` 证据链，让 release candidate、CI incident 与节点侧热点都用同一套字段归因。

实时样本、release candidate 结论和任务推进状态不属于本设计；它们由 GitHub
task issue / Project 维护。稳定 schema、边界和验证入口由本文、PRD 与
[`evidence.md`](evidence.md)共同承接。

## 2. 现状盘点

**历史基线限定：** 下表是原观测专题启动时的缺口盘点，原文按历史事实保留；不是2026-09-26当前absence断言。当前已存在的shared snapshot/status/monitor/module-observe代码locators及本次未运行边界见 [当前内容基线](#sr2-metrics-contract)。

| 层级 | 当前实现事实 | 观测缺口 |
| --- | --- | --- |
| build suite | `tools/wasm_build_suite` 已输出 `source_hash/build_manifest_hash/wasm_hash/wasm_size_bytes` | 没有 `build_wall_ms/cargo_build_ms/canonicalize_ms`，无法判断 canonical build 成本 |
| executor | `compile_module_cached()` 已天然区分 memory hit、disk hit、compile miss | 这些路径没有累计 timing/counter snapshot，只有零散 `Instant` 与失败码 |
| router | 本地 perf probe 已能测 `parse_each_time -> prepared_once` | 结果只停留在 ignored test `eprintln!`，没有持续观测 surface |
| status | `/v1/chain/status` 已有 `storage` 与 `traffic` 共享 snapshot 模式 | 还没有 `wasm` section，无法在节点侧看到 executor/router/build 热点 |
| summary | 现有 triad traffic monitor 已能把 cumulative counter 转窗口 delta | wasm 还没有对应 summary 入口，无法持续输出 p50/p95/top hotspot |

<a id="obs-isolation"></a>
## 3. 设计原则

Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607；既有产品要求：[REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001)。仅承接本节已限定的局部义务，不替代该要求的完整验收。
- 原则-1：指标必须复用既有 `shared snapshot -> status payload -> external summary` 模式，不再平行创造另一套日志协议。
- 原则-2：wall-clock timing 只属于本地观测层，绝不能进入 deterministic world state、event log 或共识数据。
- 原则-3：默认 surface 只暴露 bounded counters、sums、buckets；模块级明细必须是 top-N 或 allowlist，而非全量 map。
- 原则-4：先解决“归因可回答”，再追求极致精细度；首期优先覆盖 build、compile/cache、executor call、router match 四类热点。

## 4. 目标态架构

```text
tools/wasm_build_suite
  -> build timing metadata / receipt
           |
           v
oasis7_wasm_executor -----+
  -> cache/compile/call   |
           |              |
           v              |
oasis7_wasm_router        |
  -> prepare/match timing |
           |              |
           +------> shared wasm metrics snapshot
                              |
                              v
                 /v1/chain/status.wasm
                              |
                              v
             repo-owned window summary script
                              |
                              v
                 summary.md / summary.json
```

关键变化：
- build receipt 从“只有 hash/size”升级为“hash/size + timing”。
- executor/router 从“一次性 perf probe”升级为“进程级累计 snapshot”。
- `/v1/chain/status` 从“storage/traffic only”升级为“storage/traffic/wasm 三面并列”。

## 5. 详细设计

### 5.1 数据面分层
- build 层：
  - 写入 `metadata.json` / `build-receipt.json`
  - 适合记录单次构建耗时
- runtime 层：
  - 维护 `SharedWasmMetricsSnapshot`
  - 适合记录节点生命周期内累计 counters/sums/buckets
- summary 层：
  - 读取 status samples
  - 适合计算窗口 delta、bucket 派生 p50/p95、top hotspot

这三层明确分工，避免“单次构建耗时”和“进程累计执行耗时”混在一个 schema 里。

<a id="obs-build"></a>
### 5.2 Build Timing Contract
`tools/wasm_build_suite` 新增 timing 字段：
- `total_build_wall_ms`
- `cargo_build_ms`
- `canonicalize_ms`
- `hash_ms`
- `receipt_write_ms`
- 可选：`metadata_write_ms`

约束：
- 字段必须进入 metadata 与 receipt 的 machine-readable schema。
- `dry_run=true` 时允许部分字段为空，但必须显式区分，不得伪造 `0ms success`。
- timing 单位统一使用毫秒。

<a id="obs-snapshot"></a>
### 5.3 Runtime Snapshot Contract
新增 `WasmMetricsSnapshot`，建议与 `StorageMetricsSnapshot` 风格对齐：
- `observed_since_unix_ms`
- `metrics_available`
- `degraded_reason`
- `build`: 最近一次构建或导入的 bounded timing 摘要
- `executor`: 累计 counters/sums/buckets
- `router`: 累计 counters/sums/buckets

其中 `executor` 至少包括：
- `calls_total`
- `memory_cache_hits`
- `disk_cache_hits`
- `compile_misses`
- `failure_by_code`
- `compile_ms_total`
- `deserialize_ms_total`
- `instantiate_ms_total`
- `entrypoint_call_ms_total`
- `decode_ms_total`
- `call_wall_ms_buckets`

其中 `router` 至少包括：
- `prepare_ms_total`
- `match_ms_total`
- `regex_compile_ms_total`
- `prepared_hits`
- `parse_fallbacks`
- `prepare_ms_buckets`
- `match_ms_buckets`

上述为 `WasmRouterMetricsSnapshot` 当前字段，单位仍为毫秒；旧文中的 `prepare_subscriptions_ms_total`、`match_filters_ms_total`、`match_wall_ms_buckets` 分别是当前 `prepare_ms_total`、`match_ms_total`、`match_ms_buckets` 的历史说明名称，不是并存 JSON 字段或已支持别名。未来 `WasmObservationWindowV1` target 仍按本文目标/现状边界处理；不因此修改 producer、endpoint、互斥 bucket、nearest-rank 或 reset/no-traffic 规则。

<a id="obs-executor"></a>
### 5.4 Executor Instrumentation Points
`oasis7_wasm_executor` 的关键埋点：
- `compile_module_cached()`
  - memory cache lookup
  - disk cache load
  - deserialize compiled artifact
  - compile miss
  - serialize/store compiled artifact
- `call()`
  - instantiate
  - input write
  - entrypoint call
  - output read
  - output decode
  - total wall clock

设计约束：
- timing 记录在本地变量结束后统一更新 snapshot，减少锁持有时间。
- 失败路径也要记 timing 与 failure code，避免只统计成功样本。
- snapshot 更新必须使用短临界区；锁失败时仅打 `degraded_reason`，不影响主执行流程。

<a id="obs-router"></a>
### 5.5 Router Instrumentation Points
`oasis7_wasm_router` 的关键埋点：
- `prepare_subscriptions()`
- `prepare_subscription_filters()`
- regex compile cache miss
- `prepared_module_subscribes_to_event/action()`

设计重点：
- router 默认不输出全量 `module_id -> timing` 明细。
- 若后续需要模块级热点，建议先提供 bounded `top_modules_by_match_ms`，并限制为 `top 10`。

<a id="obs-status"></a>
### 5.6 Status Payload Exposure
`oasis7_chain_runtime` 新增：
- `status_payload.rs` 中的 `wasm: WasmMetricsSnapshot`
- 构建/刷新 `snapshot_wasm_metrics(...)`

语义与现有 storage/traffic 保持一致：
- 状态接口只读
- 返回 cumulative snapshot
- 外部脚本自行计算窗口 delta

不做的事：
- 不在 status 接口直接输出 p95
- 不在 status 接口直接输出全量原始时序
- 不在 status 接口暴露未裁剪的模块级 map

<a id="obs-summary"></a>
### 5.7 External Summary Script

当前 monitor 的三种时间口径必须分开：reset-aware runtime窗口至少需要两份可用executor/router样本，从最后reset样本缩窗；stage热点与percentile来自该窗口delta。module hotspot取latest cumulative bounded top-N，不对两个topN集合做差；build timing也是latest snapshot，build timestamp churn不重置runtime窗口。对应测试分别为check_module_hotspot_window、check_build_timestamp_churn_keeps_runtime_window与check_single_sample_compat。
新增 repo-owned summary 入口，职责是：
- 读取多次 `/v1/chain/status` 采样
- 识别 `observed_since_unix_ms` reset
- 计算 delta/s
- 从固定 buckets 派生 p50/p95
- 输出 `summary.md/json`

推荐最少输出：
- `delta_calls`
- `delta_compile_ms`
- `delta_timeout_count`
- `executor_p50/p95_call_ms`
- `router_p50/p95_match_ms`
- `top_hotspots`
- `degraded_reason`

<a id="obs-capacity"></a>
### 5.8 Cardinality 与 Payload Guardrails

当前executor最多跟踪256个独立module键，超额聚合到额外overflow键；输出按wall_ms_total、calls_total、module_id稳定排序后取top10，ID按UTF-8边界限制120 bytes。此结构上界不等于完整status满足64/128KiB，不证明truncated目标已实现。poisoned snapshot返回不可用原因，update锁失败直接返回；尚需真实主调用测试证明执行不中断。
默认 guardrails：
- 禁止 `trace_id`
- 禁止原始 input/output bytes
- 禁止全量 `module_id -> metrics`
- 允许：
  - 全局累计 counters
  - 固定 buckets
  - bounded top-N

若后续需要更细维度：
- 必须显式 env-gated
- 必须有 payload budget
- 必须写明裁剪策略

<a id="obs-relations"></a>
## 6. 与现有体系的关系
- 与 `TASK-WORLD_RUNTIME-060` 的关系：
  - 不是替代 perf probe
  - 而是把其结论沉淀为长期可复核字段
- 与 `chain-status-traffic-metrics` 的关系：
  - 直接复用 shared snapshot + status payload 设计模式
- 与 `runtime-release-gate-metrics-template` 的关系：
  - 后续可把 `wasm.build/executor/router` 作为 runtime 候选指标的一部分

## 7. 回退与降级策略
- 若 metrics 初始化失败：
  - `metrics_available=false`
  - 写 `degraded_reason`
  - 继续允许 module 执行
- 若 status payload 裁剪命中：
  - 显式输出 `truncated=true` 或等价说明
- 若 summary 脚本遇到 reset：
  - 自动缩窗
  - 在 summary 中标记 `window_reset_detected=true`

## 8. 实施顺序建议
1. build suite timing schema
2. executor snapshot
3. status payload `wasm` section
4. summary 脚本
5. router snapshot 与 bounded top-N

原因：
- build/executor 是最高价值且埋点最明确的两段。
- status payload 需要先有统一 schema。
- router 明细容易受 cardinality 影响，放在后半段更稳。

<a id="obs-module"></a>
## 9. Module-local observe 层

```text
module_observe.json
  -> generic wasm_module_observe runner
  -> wasm_build_suite
  -> executor contract cases + metrics delta
  -> prepared/fallback router probes + metrics delta
  -> summary.json + summary.md
```

- spec 路径固定在模块 `observability/` 下，manifest 相对 spec 解析。
- cases 统一表达 request、expect、repeat 与 failure assertions；router probes 统一表达 prepared/fallback 和 match expectation。
- runner 只处理 generic JSON -> Canonical CBOR 与共享 ABI 输出，不知道模块私有类型。
- `repeat` 至少为 1；assertion、path 或 decode 失败返回结构化错误并停止对应运行。
- summary 同时保留 build timing、case wall-clock、executor/cache delta 与 router delta，但默认不包含 raw payload。
- 模板与 `m1_rule_move` 样例提供接入真值；扩展更多模块只新增 spec/fixture，不修改 runner 分支。

## 10. 权威边界

- 全局 status/window 适合节点与候选热点归因；module-local observe 适合单模块 contract/perf 验证，两者互补。
- instrumentation 降级不得阻断模块执行，也不得代签模块功能、产品闭环或 release readiness。
- 所有 timing 保持本地观测属性和 bounded cardinality。

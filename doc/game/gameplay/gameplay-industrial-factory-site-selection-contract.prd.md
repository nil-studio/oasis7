# 工厂选址与物流拓扑玩家合同

- 主题范围：工厂建设前的站点候选、输入/输出拓扑取舍、失败恢复与下一工业动力；本文件是 gameplay 详细 authority，不新增独立玩法系统。
- 上层产品映射：承接 `doc/product/world-rules-core-gameplay/prd.md` §2「工厂选址与网络拓扑决策」；该产品章节拥有站点候选与产品承诺，本文件拥有玩家节奏、取舍、收益、失败和 progression 表达。
- 产品语义入口：[`工厂选址与网络拓扑决策`](../../product/world-rules-core-gameplay/prd.md#工厂选址与网络拓扑决策（补齐-buildfactory-的地点选择产品缺口）)；本合同保留站点比较、提交与恢复权威。
- M4 authority：`doc/world-simulator/m4/industrial-resource-flow-contract.prd.md` 拥有 factory/material ledger、path/edge、capacity、loss、power、buffer 与 terminal 的领域事实；本文件不复制或改写这些事实。
- gameplay 路由：由 `doc/game/gameplay/gameplay-top-level-design.prd.md` §2.5 的 `BuildFactory` 规则进入；产品、M4、world-runtime 的当前状态仍以各自 authority 与 fresh evidence 为准。

## 1. 决策目标与边界

建厂不只是“地点可建/不可建”的付款检查。玩家在确认 `BuildFactory` 前，必须能理解所选站点如何改变原料输入、产物输出、能源、容量、终端机会和未来扩张；站点选择必须成为“选择 → 建设 receipt → 下一工业目标”的可追踪小循环。站点候选是当前工业目标与同一 authority snapshot 派生的只读产品结果，不是新的世界实体。

若 recipe 尚未固定，recipe-specific path、capacity、loss、buffer/terminal fit 与 production power 只能作为 advisory estimate；它们不得伪装成普遍建厂前置。recipe 固定后，只有 profile 明示为 construction prerequisite 的 topology 才能阻断提交。任何 site/path authority 缺失均显示 `site_unknown` 或 `unknown/degraded`，不得按最近地点、零损耗、无限容量或 Agent 推荐补齐。

## 2. 玩家候选与动作

在同一目标、同一 snapshot、同一 factory kind 下，玩家至少可以比较当前专业 profile 真实支持的：

| 玩家动作 | 即时收益与下一动力 | 代价、风险与不可假设 |
| --- | --- | --- |
| `build_at_site` | 选择一条可证明的输入/输出拓扑，获得可追溯工厂能力入口，并打开配方候选/首个工业目标下一步 | 消费建设投入并承担该站点的运输、容量、能源、终端与扩张机会成本；不保证未来路线或市场收益 |
| `prepare_inputs_or_route` | 先补齐输入、容量或合法路径，降低建成后首条产线立即阻塞的风险 | 延迟建厂并继续占用/损失材料、电力、路径或 buffer 机会；不创建工厂或隐式预留 |
| `restore_power_first` | 先恢复可行动能源，保住建设后能执行的下一步 | 承担补电的资源/时间成本；`electricity_after` 不等于长期 runway 或维护安全 |
| `use_another_legal_site` | 用站点比较换取更近原料、更近终端、更好容量或更高扩张弹性 | 迁移/移动机会成本只在尚未建厂时可比较；已接受工作不得因此自动迁移 |
| `defer` | 保留当前资源，等待更好的 authority、容量、路线或目标证据 | 延迟能力与交付机会；不能无限等待，必须有 `next_recheck` 或转入 `no_safe_site_fallback` |

每个候选至少展示：站点/地点/chunk 与 owner/作用域前置；输入来源到工厂的有效 path/edge；产物到目的账本或 terminal 的有效 path/edge；capacity、预计 loss、power、输入/输出 buffer、terminal fit；建设成本、仍占用价值、首个工业目标关联、未来扩张/换线机会成本、primary blocker、`next_action`、`next_recheck` 与 recommendation reason。`build_at_site` 只有在 profile 与所需事实均可证明时可选择；其他候选只能显示为解释性 blocker 或 advisory，不得静默降级为可建。

## 3. 选择到结算的闭环

预览只读，不创建工厂、扣建设资源、锁 site/edge/capacity/buffer、改变队列顺位或承诺未来电力。玩家确认一个候选后，提交必须绑定所选 site 与当时有效的 site-level context、candidate/config/world revision，并以 fresh state 重新校验 owner、location/chunk、factory kind、construction prerequisites 与 profile 明示的 topology prerequisites。

成功收益是一次 `FactoryBuilt` 或等价的权威建设 receipt，以及明确的下一步：进入 recipe candidate discovery、准备输入、恢复 power 或打开当前目标的首个可执行工业选择。建设 receipt 不等于物流已连通、配方已启用、生产已开始、稳定窗口已推进或 terminal 已可交付。

报价后的 owner/site/chunk/path/capacity/power/terminal prerequisite 漂移只能产生 fresh requote 或无设施/资源/义务副作用的 atomic reject。recipe 未固定时 advisory topology 漂移不阻断建厂，但必须在 recipe discovery/排程重新读取；recipe 固定且 profile 明示为建厂前置的 topology 漂移必须阻断或重报。系统不得静默切换站点、自动改路线、降级损耗、免费补容量或把推荐当成已建设。

## 4. 建设后的失败恢复与禁止迁移

已接受但尚未开工的 intent、WIP、in-transit、buffer-held 与 terminal-pending 工作保留其原 site、factory、path、ledger、batch 与 root identity，不因新站点候选自动迁移；terminal-pending 同时保留 recipient、destination、admission、capacity obligation 与 provenance，直到 matching settlement 或一次受支持处置。若未来专业 profile 支持 relocation，必须按既有 cutover/处置合同逐项产生 parent-linked 结果；本合同不把 `use_another_legal_site` 扩展为既有工作迁移。

站点被阻塞时，玩家必须看见最早 root blocker、已消费/仍占用价值、受影响的输入/输出边、稳定窗口影响与下一复查点，并只能执行当前 profile 支持的补料/补电、等待容量、改路、换合法站点、降载、延期或放弃。没有合法站点或安全恢复路径时返回 `no_safe_site_fallback`，结束当前建厂意图并要求 `reprioritize_goal`、`return_to_goal_selection` 或等待新的 authority；不得用后台重试、自动迁厂或“已接受”伪造建设成功。

## 5. Current/target evidence cutline

当前 bounded runtime evidence 仅证明 `BuildFactory` 携带 owner、`location_id`、factory id/kind，并校验 location 存在、owner 共址/chunk 与通用建设资源；它不证明玩家可比较的 site topology、输入/输出路径、capacity/loss、terminal fit 或扩张价值。当前 `BuildFactory` 结果不得被表述为完整站点决策闭环。

目标 evidence 必须以同一 fresh site/goal snapshot 与 runtime + Viewer/QA 组合证据证明：至少两个合法站点的相反取舍可读，提交会绑定所选站点与当前 topology context，owner/site/chunk 或 profile 明示的 construction/topology prerequisite 漂移会重报/原子拒绝，而 recipe 未固定时的 advisory topology 漂移不会阻断建厂并会在 recipe discovery/排程重新读取，建设后下一工业动作与 site choice 有可追溯因果。取得该证据前，`site_unknown`/`unknown/degraded` 与 `no_safe_site_fallback` 优先于“推荐建厂”或当前 readiness claim；本合同不把目标规则当成 current implementation。

## 6. Exactly-once、replay 与跨 surface parity

- 同一 site candidate、goal、factory kind、authority snapshot 与 accepted intent 只能产生一次建设效果；重复确认、重连、Agent retry、snapshot restore、乱序事件与 replay 不得复制 `FactoryBuilt`、资源 sink、path reservation、PowerPlant 输出、稳定进度或奖励。
- 站点候选、site/path context、primary blocker、已消费/仍占用价值、`next_action`、`next_recheck` 与 `progression_effect` 必须在 Viewer、pure API、Agent 读面保持同义；Agent 推荐不能替玩家确认换站、治理、长期路径或 relocation 承诺。
- 已接受/WIP/in-transit/buffer/terminal-pending 的 site/path identity 不得由 replay、刷新、换站或 relocation candidate 重写；terminal-pending 同时保留 recipient、destination、admission、capacity obligation 与 provenance，直到 matching settlement 或一次受支持处置。失败只保留原状态、产生一次 receipt，或进入声明的 pending/hold/return/reroute/handoff/reject disposition。

## 7. Required acceptance

`test_tier_required` 至少覆盖：

- 同一工业目标下两个合法站点（原料近但输出拥挤；终端近但输入损耗较高）的候选比较；
- site/path/capacity/power/buffer/terminal authority 缺失时的 `site_unknown`/`unknown/degraded`；
- 报价后 owner、地点、chunk 或 profile 明示的 construction/topology prerequisite 漂移会 fresh requote/atomic reject；
- recipe 未固定时 advisory path/capacity/power/buffer/terminal 漂移不阻断建厂，并在 recipe discovery/排程重新读取；
- 建厂成功只产生一次 receipt 并打开真实下一步；
- 无合法站点时 `no_safe_site_fallback`；
- 已接受/WIP/in-transit/buffer/terminal-pending 工作不静默迁移，terminal-pending 保留 recipient/destination/admission/capacity obligation/provenance 并只接受一次合法处置；
- 重复提交、重连、Agent retry、snapshot restore 与 replay 不复制效果；
- Viewer/pure API/Agent 对候选、取舍、blocker、旧工作 identity/处置、下一步与复查点保持一致。

`test_tier_full` 另覆盖三站点以上的共享边/终端容量争用、recipe 固定前后的 advisory/prerequisite 分界、拓扑变化期间的 WIP/in-transit/buffer/terminal-pending 处置、支持的 relocation/cutover profile 与 parent linkage；本合同要求结果可解释，但不把这些目标测试视为当前实现通过。

## 8. Non-goals 与 residual risk

本合同不新增 runtime/API/schema 字段，不规定 path 搜索、容量、损耗、价格、税费、能源或队列算法，不实现 relocation，不改变 recipe lifecycle、source allocation、batch、demand、byproduct 或 starter completion profile，不设计 UI 布局，也不宣称当前 runtime、Viewer、Agent 或 QA 已完成。

主要残余风险是当前 BuildFactory action 的 site 语义仍窄于产品拓扑承诺；在 topology context、site choice receipt 与跨 surface parity 未获得 fresh evidence 前，玩家可能仍只能看到“可建/不可建”。后续实现必须先补 authority 与验证，再把 `build_at_site` 作为当前可执行推荐。

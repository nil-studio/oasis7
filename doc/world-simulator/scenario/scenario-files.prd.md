# oasis7 Simulator：场景文件化（设计文档）

- 对应设计文档: `doc/world-simulator/scenario/scenario-files.design.md`
- 当前任务状态与历史变更：GitHub PR、实际验证记录与 Git history；Issue 按需。

审计轮次: 5


本分册描述将 `WorldScenario` 的内置定义迁移为**场景文件**，用于提升可维护性与可扩展性。

## 1. Executive Summary

### 目标与范围
- 将现有所有 `WorldScenario` 定义迁移为**场景文件**（JSON），作为单一来源。
- 保持现有 API（`WorldInitConfig::from_scenario` / `WorldScenario::parse` 等）可用。
- 通过 `seed + location_generator` 生成地点，保证同场景配置可复现。

## 2. User Experience & Functionality

### In Scope
- `crates/oasis7_simulator/scenarios/*.json` 作为默认场景文件集合。
- `WorldScenario` 从场景文件加载配置（include_str 嵌入编译产物）。
- 地点生成表达 `location_generator`：由 `seed` 决定地点数量与命名。
- 场景文件覆盖所有现有场景：minimal/two_bases/.../asteroid_fragment_detail_bootstrap/asteroid_fragment_triad_region_bootstrap。
- 单元测试验证场景文件可加载与稳定性。
- `oasis7_init_demo` 支持 `--scenario-file` 从 JSON 文件加载场景。

### Out of Scope
- 通用生产 runtime 的任意路径场景热加载或热更新；已支持的 `oasis7_init_demo --scenario-file` 初始化时读取显式 JSON 文件不在此排除项内。
- 复杂场景 DSL（条件生成、随机分布等）。
- 场景文件的版本迁移工具。

## 3. AI System Requirements (If Applicable)
- N/A: 本专题不新增 AI 专属要求。

## 4. Technical Specifications

### 场景文件结构（JSON）
```json
{
  "id": "two_bases",
  "name": "Two Bases",
  "seed": 2,
  "asteroid_fragment": { "enabled": false, "min_fragment_spacing_cm": 50000 },
  "agents": { "count": 2 },
  "location_generator": { "count": 2, "id_prefix": "base-", "name_prefix": "Base" }
}
```

`asteroid_fragment.min_fragment_spacing_cm` 为可选字段，用于覆盖小行星碎片最小间距（cm）；未设置则沿用 `WorldConfig.asteroid_fragment` 的 `50_000` cm 默认值，有效值 `<= 0` 表示关闭额外 spacing 约束。

初始化时先复制 `WorldConfig.asteroid_fragment`，再应用场景 JSON 中显式出现的字段，形成本次 `WorldInitConfig` 的 effective asteroid-fragment config。覆盖只在初始化阶段生效，不修改全局默认值，也不是运行期 hot configuration。相同场景 seed、effective config 与 chunk trigger order 必须产生相同布局；replay 使用已提交的生成/补种 delta，不以当前默认值重算历史结果。

### 地点生成表达
- `seed`：场景随机根种子。
- `location_generator.count`：生成地点数量。
- `location_generator.id_prefix`：地点 ID 前缀（最终形如 `prefix + index`）。
- `location_generator.name_prefix`：地点名称前缀（最终形如 `prefix + 空格 + index`）。

> `origin.pos` 仍支持 `center` / `center_offset` / `absolute`，仅用于 origin 的显式定位。

<a id="scenario-loading-contract"></a>
### 加载策略

Trace refs: O7-REQ-3cf9f857-8083-4527-9648-95e9425a8448；本条承接[Agent 根 SC-1](../../product/agents-world-simulation/prd.md#sc-agent-001)中的场景初始化或来源重建子范围，不代签后续全部权威事件与正式交互结果。
- 内置场景通过 `include_str!` 嵌入编译产物，`WorldInitConfig::from_scenario` 使用该嵌入配置，避免运行时场景文件 I/O；修改内置 JSON 后须重新编译对应程序。
- `oasis7_init_demo --scenario-file <path>` 在本次初始化时通过 `WorldScenarioSpec::load_from_path` 读取显式 JSON；修改该外部文件后重新运行 demo 即可，无需仅为场景数据重编译。
- 两条加载路径使用相同场景解析、effective config 和初始化校验；demo 例外不构成生产 runtime 热加载承诺。

## 5. Risks & Roadmap
- **F1**：输出场景文件设计文档。
- **F2**：完成场景文件迁移与加载逻辑，更新测试与文档。

### Technical Risks

#### 当前风险与未决
- 场景文件与代码结构漂移导致解析失败。
- 相对位置表达不当引入边界越界风险。
- 修改嵌入的内置 JSON 需重编译；demo 自定义 JSON 的风险是给定文件不可读或解析/初始化失败，不能静默改用内置场景。

## 场景测试覆盖矩阵（2026-02-06）

> 目标：将“场景是否有意义”转化为可验证口径。每个场景至少对应一个**稳定断言**，避免保留“只存在但无测试价值”的场景。

| 场景 ID | 主要测试目标 | 关键覆盖测试 |
| --- | --- | --- |
| `minimal` | 最小初始化基线（origin + 默认 agent） | `scenario_specs_match_ids`、`scenario_templates_build_models`、`scenarios_are_stable`、`oasis7_init_demo_runs_summary_only`、`oasis7_init_demo_runs_from_scenario_file` |
| `two_bases` | 双基地拓扑与双 agent 基础分布 | `scenario_specs_match_ids`、`scenario_templates_build_models`、`scenarios_are_stable`、`scenario_aliases_parse(two-bases)` |
| `llm_bootstrap` | LLM 驱动预置基线（双站点 + 辐射 profile + data/electricity 资源） | `scenario_specs_match_ids`、`scenario_templates_build_models`、`scenarios_are_stable`、`scenario_aliases_parse(llm)`、`oasis7_init_demo_runs_llm_bootstrap_summary` |
| `power_bootstrap` | 显式 `power_plants` 场景夹具与 owner 约束（不代表 storage/runtime ABI） | `scenario_specs_match_ids`、`scenario_templates_build_models`、`scenarios_are_stable`、`scenario_aliases_parse(bootstrap)` |
| `resource_bootstrap` | 资源初值注入（origin/agent） | `resource_bootstrap_seeds_stock`、`scenario_specs_match_ids`、`scenarios_are_stable`、`scenario_aliases_parse(resources)` |
| `twin_region_bootstrap` | 双区域结构（location/agents） | `twin_region_bootstrap_seeds_regions`、`scenarios_are_stable`、`scenario_aliases_parse(twin-regions)`、`plan_demo_actions_includes_move_for_multi_location_scenario` |
| `triad_region_bootstrap` | 三区域结构（location/agents/resource） | `triad_region_bootstrap_seeds_regions`、`scenarios_are_stable`、`scenario_aliases_parse(triad-regions)`、`oasis7_init_demo_runs_triad_summary` |
| `triad_p2p_bootstrap` | P2P 节点化分布（seed 生成节点 + 随机出生） | `triad_p2p_bootstrap_seeds_nodes_and_agents`、`scenarios_are_stable`、`scenario_aliases_parse(p2p-triad)` |
| `asteroid_fragment_bootstrap` | 碎片分块生成 + bootstrap chunk + 预算账本（无默认设施） | `asteroid_fragment_bootstrap_seeds_fragments_and_resources`、`scenarios_are_stable`、`oasis7_init_demo_runs_asteroid_fragment_summary` |
| `asteroid_fragment_detail_bootstrap` | 细粒度渲染调试场景（高密度碎片，无 origin/agent） | `asteroid_fragment_detail_bootstrap_seeds_dense_fragments_for_viewer`、`scenarios_are_stable`、`oasis7_init_demo_runs_asteroid_fragment_detail_summary` |
| `asteroid_fragment_twin_region_bootstrap` | 碎片分块 + 双区域结构联动（无默认设施） | `asteroid_fragment_twin_region_bootstrap_seeds_fragments_and_regions`、`scenarios_are_stable`、`oasis7_init_demo_runs_asteroid_fragment_twin_summary` |
| `asteroid_fragment_triad_region_bootstrap` | 碎片分块 + 三区域结构联动（无默认设施） | `asteroid_fragment_triad_region_bootstrap_seeds_fragments_and_regions`、`scenarios_are_stable`、`oasis7_init_demo_runs_asteroid_fragment_triad_summary` |

### 场景初始化与消费入口矩阵

Trace refs: O7-REQ-3cf9f857-8083-4527-9648-95e9425a8448；本条承接[Agent 根 SC-1](../../product/agents-world-simulation/prd.md#sc-agent-001)中的场景初始化或来源重建子范围，不代签后续全部权威事件与正式交互结果。

此表区分初始 Simulator 模型与 Runtime 对外快照，不能从场景 JSON 解析成功推断各入口保留同一模型。

| 入口 | 初始模型与默认实体语义 | 可证明范围 |
| --- | --- | --- |
| Simulator `build_world_model` / `initialize_kernel`；`oasis7_init_demo` | 按场景的 origin、agents、bootstrap chunks 和显式设施装配 | 场景初始状态；`asteroid_fragment_detail_bootstrap` 保留四个 bootstrap chunks、关闭 origin、Agent 数为 0 |
| 普通 named runtime-live `--scenario`（含 Web 回归脚本转发） | 把 Simulator 配置投影为 Runtime；不保留完整 Simulator seed model，空 Agent 集会补 Runtime 默认 Agent | Runtime 调试启动/投影；不能用作纯碎片、零 Agent 的初始模型保留证据 |
| runtime-live `--generated-world-dir` | 当前只检查 provenance 路径是文件，再加载 generated-world sidecar；保留 seed model 参与快照映射，仍使用 Runtime Agent 投影 | 仅证明 sidecar 可加载和该消费者的模型映射；不证明 provenance 内容与 sidecar 对应或生成可复现，也不能等同于 named `--scenario` 或自动保证零 Agent |

`asteroid_fragment_detail_bootstrap` 的纯碎片回归目前推荐 Simulator 离线初始化入口；普通 Web 主入口或 `software_safe` 脚本的 `--scenario` 不满足该条件。未来若提供 Web 专项入口，须显式定义 debug-only 模型保留和默认 Agent 处理，不能无条件改变正式 Runtime。

推荐纯碎片入口的验收应读取初始模型/快照，逐项核对 JSON 指定的四个 bootstrap chunk 的碎片均保留、无补入的默认 Agent、origin 未注入；只校验 JSON 可解析、模型非空或网页已加载均不足。此要求不宣称已有 Web 入口通过，也不覆盖 live refill 的 enabled/available 边界。

<a id="generated-world-provenance-contract"></a>
### generated-world 来源与可复现声明

Trace refs: O7-REQ-3cf9f857-8083-4527-9648-95e9425a8448；本条承接[Agent 根 SC-1](../../product/agents-world-simulation/prd.md#sc-agent-001)中的场景初始化或来源重建子范围，不代签后续全部权威事件与正式交互结果。

当前 [`bootstrap_generated_sidecar_runtime_world_with_chunk_runtime`](../../../crates/oasis7/src/viewer/runtime_live/support.rs) 对 `world-generation-provenance.json` 仅调用 `is_file()`，不解析 JSON，也不比较 seed、场景配置、manifest 或 sidecar 内容。文件存在只满足当前调试加载的前置条件，不能把加载成功写成“来源已验证”或“同输入可复现”。

以下是允许作出上述声明的目标验收条件，尚未由该加载入口实现：

- provenance 必须可解析，`artifact_kind` 与生成工件类型相符；`scenario_id`、场景 `config` 和 `seed` 必须与实际采用的场景及有效初始化输入一致。需核对载入快照的 seed、有效配置和初始化模型，不能只检查两个标签相同。
- 必须核验实际消费的 public manifest 字节摘要及条目数，并将 provenance 与同次生成的 sidecar `snapshot.json` / `journal.json` 绑定到可重算的内容摘要；加载时重算并比较这组绑定。相同目录、文件名或 seed 不构成内容绑定，`public_manifest_sha256` 也不是 sidecar 的摘要。当前 provenance 的六个字段没有提供 sidecar 内容绑定，这仍是生成与消费实现的缺口。
- 可复现声明还须固定生成程序版本、场景输入、effective `WorldConfig`/fragment 配置及 chunk trigger order，并以独立重建的初始模型、预算和 journal 比较支持结论。当前 provenance 的 `config` 保存场景 spec，不能单独代表它未显式覆盖的全部有效默认值；加载现成快照也不能替代重建比较。

验收至少覆盖一致产物、非法或缺失 provenance、seed 不匹配、同 seed 的不同配置/sidecar，以及 manifest 不匹配。无法建立绑定时，结果只能是来源未验证；明确不匹配则报告对应原因，不得填为验证通过。当前存在检查与未来内容验证应分别记录，补齐文档不代表加载器已拒绝所有不匹配样本，也不引入新的生产热加载入口。

## 物流瓶颈验证口径（2026-02-14）

- 多账本/物流约束的回归基线以多站点场景为前提，建议优先使用 `triad_region_bootstrap` 或 `asteroid_fragment_triad_region_bootstrap` 做端到端验证。
- 关键验证口径：
  - 模块求值请求可见分账本库存：`build_factory_with_module_request_exposes_available_inputs_by_ledger`、`schedule_recipe_with_module_request_exposes_available_inputs_by_ledger`。
  - 跨站物流存在延迟与损耗：`transfer_material_cross_site_creates_transit_and_applies_loss`。
  - 物流吞吐瓶颈可触发拒绝：`transfer_material_rejects_when_inflight_capacity_exceeded`。
- 通过以上口径，确保“owner/location 多账本 + 物流约束”在场景层具备可验证闭环，而非仅停留在数据结构层。

说明：
- 只有显式 `power_bootstrap` 场景 JSON 注入 `power_plants`；其他内置场景（包括全部 `asteroid_fragment*` 场景）均无隐式电力设施。如需设施，必须在场景数据中显式声明。
- `WorldInitConfig` / `build_world_model` 的设施注入只是场景数据初始化行为，不新增 runtime 电力规则、WASM/动作 ABI、owner 电力余额、链上经济或治理权威，也不得与 `m1_power_storage` 语义混同。
- `scenario_specs_match_ids` 定位于 `crates/oasis7_simulator/src/scenario.rs`，用于约束“枚举 ID 与 JSON ID 一致”。
- 其余命名测试主要位于 `crates/oasis7_simulator/src/tests/init.rs` 与 `crates/oasis7_ops_tools/tests/oasis7_init_demo.rs`。
- 场景矩阵应随测试变更同步更新，避免“文档保留但测试漂移”。

## 6. Validation & Decision Record
- 追溯: GitHub PR、实际验证记录与 Git history；Issue 按需 保留原实施过程；本文与 design 保持现行约束语义。

## 6. 上游需求承接与产品边界

[Agent 根 SC-1](../../product/agents-world-simulation/prd.md#sc-agent-001)是完整场景重现的产品组合要求；本专题的加载策略、消费入口矩阵及 generated-world 来源合同只承接场景有效输入与来源重建子范围。后续事件执行、provider 选择、正式表面交互和同候选端到端结果仍由各专业 authority 与 QA 承接，不因本专题加载成功而完成。

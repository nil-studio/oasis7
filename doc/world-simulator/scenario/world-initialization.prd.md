# oasis7 Simulator：世界初始化（设计分册）

- 对应设计文档: `doc/world-simulator/scenario/world-initialization.design.md`
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history。

审计轮次: 5

本分册描述世界初始化（World Initialization）的最小实现，用于在不依赖外部输入的情况下生成可运行的初始世界状态。

## 1. Executive Summary
- 提供**确定性**的世界初始化流程：同一配置与种子产生一致的世界模型。
- 支持最小可运行世界：原点地点 + 初始 Agent + 可选小行星带碎片。
- 初始化产物可直接用于 `WorldKernel`，并保持时间与事件日志语义清晰。

## 2. User Experience & Functionality

### In Scope（V1）
- `WorldInitConfig`：初始化参数（seed、原点地点、自定义地点、小行星带碎片生成、初始 Agent、初始电力设施）。
- 原点地点创建：默认位于空间中心，可配置位置与画像与资源存量。
- 自定义地点创建：显式指定地点列表，可配置位置/画像/初始资源。
- 小行星带碎片生成：复用 `generate_fragments`，并支持单独 seed 偏移。
- 初始 Agent 生成：按数量与前缀批量创建，按 seed 从可用地点中确定性随机出生，可配置初始资源。
- 初始电力设施：支持发电设施的预置与校验。
- 初始化输出：生成 `WorldModel`，并提供 `WorldKernel` 便捷构造。
- 基础校验：越界位置、ID 冲突、出生地点不存在、负资源、设施参数非法等。

### Out of Scope（V1 不做）
- 初始设施/资产/合约/交易等复杂社会系统初始化。
- 多阵营/多区域的高级场景模板。
- 大规模地图分片与并行生成。

## 3. AI System Requirements (If Applicable)
- N/A: 本专题不新增 AI 专属要求。

## 4. Technical Specifications

<a id="init-explicit-facilities"></a>
### 关键结构
- `WorldInitConfig`
  - `seed: u64`
  - `origin: OriginLocationConfig`
  - `locations: Vec<LocationSeedConfig>`
  - `asteroid_fragment: AsteroidFragmentInitConfig`（小行星带碎片生成配置）
  - `agents: AgentSpawnConfig`
  - `power_plants: Vec<PowerPlantSeedConfig>`
- `WorldScenario` 的场景能力与覆盖测试以[场景文件主矩阵](scenario-files.prd.md#场景测试覆盖矩阵2026-02-06)为唯一完整清单；本页只解释初始化用途，不再维护第二份能力表。
  - 场景定义文件：`crates/oasis7_simulator/scenarios/*.json`（编译期嵌入）。
  - 只有 `power_bootstrap` 显式声明默认发电设施；`twin_region_bootstrap`、`asteroid_fragment_bootstrap` 的默认设施数均为 0。初始电力资源库存不等于发电设施。

### 场景使用建议
- `minimal`：用于测试核心流程或最小单 Agent 回归。
- `two_bases`：用于验证多地点移动与基础互动。
- `power_bootstrap`：用于验证发电设施路径。
- `resource_bootstrap`：用于验证资源库存与交易/消耗逻辑。
- `twin_region_bootstrap` / `triad_region_bootstrap`：用于验证多区域结构、资源分布与运输成本；设施差异必须另行提供显式 `power_plants` 的自定义 JSON/初始化配置。
- `triad_p2p_bootstrap`：用于验证 P2P 多节点拓扑与节点内 Agent 初始分布。
- `asteroid_fragment_bootstrap`：用于验证小行星带碎片/辐射采集与热管理路径。
- `asteroid_fragment_twin_region_bootstrap`：用于验证小行星带碎片下的多区域结构与运输成本，不默认注入设施。
- `asteroid_fragment_triad_region_bootstrap`：用于验证小行星带碎片下的三方结构与协作/竞争；需要设施的用例须显式声明 `power_plants`。

### 场景别名
- `two_bases`：`two-bases`
- `power_bootstrap`：`bootstrap`
- `resource_bootstrap`：`resources`
- `twin_region_bootstrap`：`twin-regions`
- `triad_region_bootstrap`：`triad-regions`
- `triad_p2p_bootstrap`：`triad-p2p` / `p2p-triad`
- `asteroid_fragment_bootstrap`：`asteroid_fragment`
- `asteroid_fragment_twin_region_bootstrap`：`asteroid-fragment-regions`
- `asteroid_fragment_triad_region_bootstrap`：`asteroid-fragment-triad`

### 小行星带碎片种子策略
- 初始化使用 `seed + asteroid_fragment.seed_offset` 生成小行星带碎片，确保与其他随机源解耦。
- 各场景为小行星带碎片设置不同的 `seed_offset`，避免同一 seed 下的碎片分布完全一致。
- 新增场景时应选择未使用过的 `seed_offset`，保持分布差异可追踪。
- `WorldInitReport.asteroid_fragment_seed` 会记录实际使用的小行星带碎片种子，便于回放与比对。

### 场景 ID 稳定性
- 现有场景 ID 视为稳定接口，不应随意改名或删除。
- 新增场景应保持命名风格一致，并补充别名与稳定性测试。
 - 场景文件名需与场景 ID 一致（`{id}.json`）。
- `OriginLocationConfig`
  - `enabled: bool`
  - `location_id/name/profile`
  - `pos: Option<GeoPos>`（空则取空间中心；`GeoPos` 直接使用整数厘米）
  - `resources: ResourceStock`
- `LocationSeedConfig`
  - `location_id/name/profile`
  - `pos: Option<GeoPos>`（空则取空间中心；`GeoPos` 直接使用整数厘米）
  - `resources: ResourceStock`
- `AsteroidFragmentInitConfig`
  - `enabled: bool`
  - `seed_offset: u64`
  - `min_fragment_spacing_cm: Option<i64>`（可选，覆盖小行星碎片最小间距）
- `AgentSpawnConfig`
  - `count: usize`
  - `id_prefix: String`
  - `start_index: u32`
  - `resources: ResourceStock`
- `PowerPlantSeedConfig`
  - `facility_id/location_id/owner`
  - `capacity_per_tick/fuel_cost_per_pu/maintenance_cost`
  - `efficiency/degradation`
- `WorldInitReport`
  - 统计创建数量（locations/agents）与使用的 seed
- `WorldInitError`
  - 越界位置、ID 冲突、出生地点缺失、负资源等错误

<a id="init-time-events"></a>
### 初始化语义
- 初始化在**时间 0** 完成：
  - `WorldKernel.time = 0`
  - 时间 0 不等于 `journal` 必为空。启用碎片且已生成 chunk 时，`initialize_kernel` 会记录 `ChunkGenerated`、`cause=Init`；关闭碎片或没有生成 chunk 的路径仍可为空。
  - snapshot 承载完整初始状态；chunk 初始化事件仅承载专业合同声明的生成事实，不能据此声称 origin、自定义地点、Agent、设施等全部对象均可只靠事件重建。事件与回放范围见[分块生成合同](chunked-fragment-generation.prd.md)。
- 生成过程使用 `WorldConfig` 的 `space/asteroid_fragment/power` 配置。
- 任意初始化入口写入 `WorldModel` 时，位置坐标都必须已经满足 `SPACE_UNIT_CM=1` 的整数厘米合同；不接受也不持久化 sub-cm 坐标。
- 生成顺序：origin → custom locations → asteroid fragments → agents → facilities（确保依赖对象已存在）。
- `LocationSeedConfig` / `PowerPlantSeedConfig` / 后续 `Factory` 站点绑定继续使用 `location_id` 作为离散原生分辨率；其物理真值由对应 `Location.pos` 与 `Location.profile.radius_cm` 提供，不支持初始化阶段写入 sub-location offset。
- 场景模板通过 `WorldInitConfig::from_scenario(scenario, config)` 生成初始化配置。
- 场景文件由 `WorldScenario` 加载并转换为 `WorldInitConfig`。
- 本节约束 Simulator 初始化；named runtime-live 与 generated-world 的保留/投影差异见[消费入口矩阵](scenario-files.prd.md#场景初始化与消费入口矩阵)，不以 Runtime 默认实体补入行为改写场景模型合同。

### 示例工具
- `oasis7_init_demo`：命令行示例，按场景生成世界并输出统计摘要。

### 使用示例
- 运行示例工具：`env -u RUSTC_WRAPPER cargo run -p oasis7_ops_tools --bin oasis7_init_demo -- minimal`
- 查看场景列表：`env -u RUSTC_WRAPPER cargo run -p oasis7_ops_tools --bin oasis7_init_demo -- --help`
- 运行自定义场景文件：`env -u RUSTC_WRAPPER cargo run -p oasis7_ops_tools --bin oasis7_init_demo -- --scenario-file path/to/scenario.json`
- 示例输出包含每个地点的资源摘要（electricity/hardware/data）。
- 示例输出包含每个 Agent 的资源摘要（electricity/hardware/data）。
- 示例输出包含估算的小行星带碎片数量（asteroid_fragment_fragments）。
- 示例输出包含每个地点的设施统计（power_plants）。
- `--summary-only` 可仅输出概要统计（隐藏详细列表）。

## 5. Risks & Roadmap
- **I1**：定义初始化配置与报表结构，输出 `WorldModel`。
- **I2**：接入 `WorldKernel` 便捷构造与基础校验；补充单元测试。
- **I3**：扩展初始化模板（资源/多据点）与更多可配置项。
- **I4**：扩展初始化模板（发电设施）与更多校验/测试。

### Technical Risks
- 默认小行星带碎片生成可能导致初始化耗时波动（需可配置关闭）。
- 依赖浮点随机数的确定性：需确保同平台可复现。
- 初始状态不全部由 journal 描述；消费方须按 snapshot 与已声明事件的覆盖范围恢复，不因时间 0 存在合法 Init 事件而清空日志。

## 6. Validation & Decision Record
- 文档/离线回归分别覆盖：两个默认区域/碎片场景设施数 0、`power_bootstrap` 显式设施正例、自定义设施声明；时间 0 下 enabled chunk 的 Init 事件与 fragment-disabled 路径。复用 `simulator/tests/init.rs`、`simulator/tests/persist.rs` 的对应测试，文档核对不代签运行结果。
- 追溯: GitHub task issue evidence 与 Git history 保留原实施过程；本文与 design 保持现行约束语义。

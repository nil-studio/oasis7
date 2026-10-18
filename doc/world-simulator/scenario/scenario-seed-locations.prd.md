# oasis7 Simulator：场景种子化地点生成（设计文档）

- 对应设计文档: `doc/world-simulator/scenario/scenario-seed-locations.design.md`
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history。

审计轮次: 5

本分册定义“场景不再显式声明 `locations`，而是由 `seed` 确定性生成地点”的新策略，并将 Agent 初始出生点改为“从可用地点中随机（但可复现）选择”。

## 1. Executive Summary
- 以场景 `seed` 作为地点生成的唯一随机源，保证同输入可复现。
- 场景文件移除 `locations` 显式列表，改为声明地点生成参数。
- Agent 初始化时不再依赖固定 `location_id`，改为随机地点出生。
- 普通 `location_generator` 按 seed 与地点序号派生结果，不依赖外部 chunk 触发顺序；此承诺不扩展为 chunk/fragment 布局的无条件顺序无关。

## 2. User Experience & Functionality

### In Scope
- 扩展 `WorldScenarioSpec`：新增地点生成配置，移除 `locations`。
- 基于 `seed` + 空间尺寸，确定性生成 `LocationSeedConfig` 列表。
- `build_world_model` 中 Agent 出生逻辑改为“从现有地点随机挑选”。
- 调整内置 `crates/oasis7_simulator/scenarios/*.json` 到新格式（不做兼容）。
- 更新相关单元测试与文档。

### Out of Scope
- 对旧场景 JSON 的兼容迁移工具。
- 复杂地点分布 DSL（噪声层、聚类约束、多模板混合）。
- 运行时热更新地点生成参数。

## 3. AI System Requirements (If Applicable)
- N/A: 本专题不新增 AI 专属要求。

## 4. Technical Specifications

### 场景文件结构（新）
```json
{
  "id": "twin_region_bootstrap",
  "name": "Twin Region Bootstrap",
  "seed": 42,
  "location_generator": {
    "count": 2,
    "id_prefix": "region-",
    "name_prefix": "Region"
  },
  "agents": { "count": 2 }
}
```

### 新增数据结构
- `ScenarioLocationGeneratorConfig`
  - `count: usize`：要生成的地点数量。
  - `id_prefix: String`：地点 ID 前缀，按序号生成（如 `region-0`）。
  - `name_prefix: String`：地点名称前缀（如 `Region 0`）。

### 生成规则
- 地点数量由 `location_generator.count` 决定。
- 每个地点的位置由 `seed` 与地点序号派生：
  - `x/y/z` 分别映射到 `space.width/depth/height` 范围内。
  - 保证位置在世界边界内。
- 地点 profile/resources 采用默认值（后续迭代再扩展参数化）。

### Agent 出生规则
- 当 `agents.count > 0`：
  - 从 `model.locations` 当前可用地点集合中，按 `seed` 派生随机序列选取出生地点。
  - 每个 Agent 选择一次；允许多个 Agent 出生在同一地点。
- 若无可用地点则报错 `SpawnLocationMissing`。

<a id="location-determinism"></a>
### 确定性约束
- 普通地点在同 `seed` + 同 `WorldConfig.space` + 同 `location_generator` 下得到相同 ID 与位置；Agent 出生还要求相同可用地点集合与确定性枚举顺序。
- 随机逻辑只依赖显式 seed，不依赖运行时时钟或哈希随机状态。
- chunk/fragment 按[专业生成合同](chunked-fragment-generation.prd.md)约束：同 world seed、effective config 和 chunk trigger order 下可复现，边界冲突先生成者优先；重排触发顺序不能自动视为等价。历史回放应用 committed delta，不用当前默认配置重新生成。

<a id="location-effective-seed"></a>
### 场景级有效 seed 与显式坐标
- 内置场景接受 seed override 时，先在 `WorldScenarioSpec` 上确定有效 seed，再生成普通地点并转换 `WorldInitConfig`；Agent 出生与 fragment 的派生随机源使用同一有效根 seed（fragment 保留自身 offset）。不得先按模板生成地点后只改 `init.seed`。
- 显式 `WorldInitConfig.locations` 表示调用者指定坐标；覆盖 seed 不重新生成这些地点。无 override 时保留模板 seed，现有配置 sanitization 仍生效。
- 验收覆盖不同 seed 的生成差异、无覆盖与模板等价、显式坐标不变、Agent/fragment 使用有效 seed；`scenario_test_runner` 的接线修复需独立实现/回归，本条不表示该实现已经完成。

## 5. Risks & Roadmap
- **SL1**：完成设计/项目文档与场景 schema 改造。
- **SL2**：完成地点确定性生成与 Agent 随机出生实现。
- **SL3**：补充测试（确定性、无地点错误、场景加载）并更新文档/日志。

### Technical Risks
- 旧场景文件不兼容会导致历史命令失败，需要同步更新示例与说明。
- 场景失去显式坐标后，可解释性下降（需依赖 seed 回放）。
- 若后续需要精细布局，可能需要再引入“受限随机 + 局部手工锚点”混合方案。

## 6. Validation & Decision Record
- 追溯: GitHub task issue evidence 与 Git history 保留原实施过程；本文与 design 保持现行约束语义。

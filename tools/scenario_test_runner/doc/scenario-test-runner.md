# oasis7: 场景文件驱动测试框架（设计文档）

## 目标
- 提供一套**场景文件驱动**的测试框架，便于按需添加场景并快速验证核心世界初始化与关键断言。
- 框架**独立于核心工程与 CI**：不进入 workspace，不进入 CI 测试清单，按需手动运行。
- 场景文件尽量简洁：支持内置 `WorldScenario` 或显式 `WorldInitConfig`，并提供可读的断言配置。

## 范围

### In Scope
- 场景工具：`tools/scenario_test_runner`（根 Cargo workspace 成员，使用根锁文件，直接依赖 `oasis7_simulator`）。
- 场景文件格式（YAML/JSON）：可选 `scenario` 或 `init`，可选 `WorldConfig`，含基础断言。
- CLI：按单文件或目录批量执行，输出可读的通过/失败摘要。
- 内置断言：agents/locations 数量、必含 IDs、小行星碎片带是否启用、agent->location 位置匹配。

### Out of Scope
- 接入 CI 或 pre-commit。
- 复杂仿真回放与长期性能测试。
- 自动生成场景文件或 UI 管理面板。

## 接口 / 数据

### 场景文件结构（YAML/JSON）
```yaml
version: 1
name: "triad-p2p"
scenario: "triad_p2p_bootstrap"
seed: 7
config: {} # 可选，等同 WorldConfig（不写则默认）
expect:
  agents: 3
  locations: 3
  require_locations: ["node-a", "node-b", "node-c"]
  require_agents: ["agent-0", "agent-1", "agent-2"]
  expect_asteroid_fragment: false
  agent_locations:
    agent-0: "node-a"
    agent-1: "node-b"
    agent-2: "node-c"
```

- `scenario` 与 `init` 二选一：
  - `scenario`：内置 `WorldScenario`（字符串）。
  - `init`：显式 `WorldInitConfig`（完全自定义）。
- `seed`：若提供，覆盖初始化的 `seed`。
- `config`：可选 `WorldConfig`，缺省则 `WorldConfig::default()`。
- 顶层字段与 `expect` 字段均采用严格解析；未知字段会直接解析失败，避免断言拼写错误被静默忽略。

### CLI 入口
- 单文件：`env -u RUSTC_WRAPPER cargo run -- --scenario path/to/file.yaml`
- 目录批量：`env -u RUSTC_WRAPPER cargo run -- --dir path/to/scenarios`
- 在 `tools/scenario_test_runner` 目录内执行；目录扫描默认非递归，支持 `.yaml/.yml/.json`。
- 非 0 退出码表示存在失败或解析错误。

## 里程碑
- **S1**：设计文档与项目管理文档。
- **S2**：独立 runner 实现 + 示例场景文件 + 基础断言测试。

## 风险
- 场景文件与内核能力漂移，导致断言过时或误报。
- 非 CI 路径易被忽视，需要明确手动运行指引。
- 自定义 `WorldInitConfig` 写法较冗长，需要示例与模板约束。

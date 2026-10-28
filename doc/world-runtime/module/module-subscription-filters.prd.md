# oasis7 Runtime：模块订阅过滤器（设计分册）

- 对应设计文档: `doc/world-runtime/module/module-subscription-filters.design.md`
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history。

审计轮次: 4


本分册定义模块订阅的过滤规则，用于在路由阶段按事件/动作内容进行细粒度筛选。

## 1. Executive Summary
- 支持在 `ModuleSubscription.filters` 中声明筛选条件。
- 路由阶段在调用模块前执行过滤，避免无关调用。
- 过滤规则保持简单、确定性、可回放。

## 2. User Experience & Functionality
### In Scope（V2）
- 基于 JSON Pointer 的等值/非等值匹配（`path` + `eq`/`ne`）。
- 数值比较（`gt`/`gte`/`lt`/`lte`）。
- 正则匹配（`re`）。
- 事件/动作分别支持独立规则集（`all`/`any`）。
- 过滤失败时不调用模块（不产生额外事件）。

### Out of Scope（V2 不做）
- 复杂逻辑组合（嵌套 OR/NOT/括号）。
- 自定义脚本或 host 过滤逻辑。


## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用（文档迁移任务）。
- Evaluation Strategy: 通过文档治理校验、引用扫描与任务日志检查验证迁移质量。

## 4. Technical Specifications
### Subscription 过滤结构
`ModuleSubscription.filters` 为可选 JSON 对象，结构如下：

```
{
  "event": [
    { "path": "/body/payload/data/agent_id", "eq": "agent-1" }
  ],
  "action": [
    { "path": "/action/data/agent_id", "eq": "agent-1" }
  ]
}
```

- `event`：用于事件路由。
- `action`：用于动作路由。
- `path`：JSON Pointer（RFC 6901）。
- `eq`：与指向值做 JSON 等值比较。

### 扩展匹配规则（V2）
支持 OR 逻辑、正则与数值比较，结构示例：

```
{
  "event": {
    "all": [
      { "path": "/body/payload/data/pos/x_cm", "gte": 0 },
      { "path": "/body/payload/data/pos/x_cm", "lt": 10 }
    ],
    "any": [
      { "path": "/body/payload/data/agent_id", "re": "^agent-" }
    ]
  },
  "action": [
    { "path": "/action/data/agent_id", "ne": "agent-legacy" }
  ]
}
```

规则字段：
- `eq` / `ne`：JSON 等值 / 非等值比较，支持显式 `null`；字段缺失与目标值为 `null` 不等价。
- `gt` / `gte` / `lt` / `lte`：数值比较（仅在目标为数字时生效）。
- `re`：正则匹配（仅在目标为字符串时生效）。

范围匹配通过两条规则组合实现（例如 `gte` + `lt`）。

### 匹配对象
- 事件路由：使用 `WorldEvent` 的 JSON 表示。
- 动作路由：使用 `ActionEnvelope` 的 JSON 表示。

### 规则语义
- 若 `filters` 为空或缺失：视为通过。
- 若对应规则集为空：视为通过。
- 若规则集为数组：按 AND 语义（全部匹配才通过）。
- 若规则集为对象：`all` 必须全部匹配，`any` 若非空必须至少一条匹配。
- 规则中 `path` 未命中：视为不通过。
- `filters` 解析失败：Shadow/Apply 阶段拒绝该模块变更。
- `path` 必须为空或以 `/` 开头；`~` 只能组成 `~0`（波浪号）或 `~1`（斜杠）转义，非法或截断转义必须拒绝。
- 数值及正则操作符不接受 `null`，不能通过显式 `null` 绕过类型或操作符数量校验。
- 每条规则必须且只能包含一个操作符字段（`eq`/`ne`/`gt`/`gte`/`lt`/`lte`/`re`）。

## 5. Risks & Roadmap
- **F1**：实现过滤解析与匹配（事件/动作）。
- **F2**：补充过滤相关测试与文档示例。
- **F3**：支持 OR/数值比较/正则匹配，并完善校验与测试。

### Technical Risks
- 过滤依赖 JSON 结构稳定性，事件/动作结构变化可能导致过滤失效。
- 过滤配置错误会导致模块不被调用，需要良好配置校验与测试覆盖。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-ENGINEERING-006 | 文档内既有任务条目 | `test_tier_required` | `./scripts/doc-governance-check.sh` + 引用可达性扫描 | 迁移文档命名一致性与可追溯性 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-DOC-MIG-20260303 | 逐篇阅读后人工重写为 `.prd` 命名 | 仅批量重命名 | 保证语义保真与审计可追溯。 |

## 原文约束点映射（内容保真）
- 原“目标” -> 第 1 章 Executive Summary。
- 原“范围” -> 第 2 章 User Experience & Functionality。
- 原“接口 / 数据” -> 第 4 章 Technical Specifications。
- 原“里程碑/风险” -> 第 5 章 Risks & Roadmap。

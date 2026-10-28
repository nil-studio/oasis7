# oasis7 Runtime：模块订阅过滤器（设计分册）设计

- 产品约束来源：[确定性世界执行 REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001)；本专题承接模块过滤的确定性与拒绝边界，不代表整体共识/回放验收完成。
- 对应需求文档: `doc/world-runtime/module/module-subscription-filters.prd.md`
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history。

## 1. 设计定位

### 问题与目标

统一模块订阅在事件/动作路由前的过滤表达、匹配语义与拒收边界。

## 2. 设计结构
- 过滤表达层：用 JSON Pointer、等值/非等值、数值比较与正则规则描述订阅条件。
- 路由执行层：在模块调用前执行事件/动作过滤，避免无关模块被触发。
- 校验拒收层：对非法 filters 在 schema/shadow/apply 阶段直接拒绝。
- 回归验证层：覆盖事件过滤、动作过滤、OR 逻辑与数值/正则规则。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001) | 对相同事件/动作和过滤配置得到相同匹配结果；非法过滤规则在调用模块前拒绝。 | [过滤匹配与拒收边界](#des-filter-001) | `wasm_platform_engineer` / `runtime_engineer`；依赖模块 manifest 校验和版本化输入结构。 | 本专题只承接过滤边界，不覆盖完整共识、finality 与多节点回放验收。 |

## 3. 关键接口 / 入口
- `ModuleSubscription.filters`
- 事件/动作路由过滤入口
- filters schema 校验
- 过滤器回归用例：`crates/oasis7_wasm_router/tests/filter_contract.rs`，覆盖 null/缺失区分、操作符拒收、RFC 6901 转义及普通/预编译事件和动作路由一致性。

<a id="des-filter-001"></a>
## 4. 约束与边界
- 过滤规则必须保持确定性、可回放。
- 过滤失败不得产生额外副作用；直接匹配入口也拒绝任一事件/动作规则集中的非法规则。
- `eq`/`ne` 的显式 `null` 必须保留为操作数；JSON Pointer 转义必须完整校验。
- 不在本专题支持复杂脚本型过滤逻辑。

## 5. 设计演进计划
- 先固化 filters 结构。
- 再补路由执行与非法配置拒收。
- 最后扩展 OR/数值/正则并固化测试。

## 6. 风险与未证明范围

- 过滤依赖事件/动作的 JSON 结构稳定性；结构变化或配置错误可能导致模块不被调用。配置校验与回归覆盖的风险依据见[对应 PRD 的 Technical Risks](module-subscription-filters.prd.md#technical-risks)。
- [过滤匹配与拒收边界](#des-filter-001)要求非法规则直接拒绝；拒收收紧后仍须重新验证既有模块配置，不能由新增拒收规则推断历史配置兼容。
- [验证映射](#111-验证映射表)只承接过滤合同，不证明完整 sandbox 副作用隔离、共识或跨版本回放。上述未证明范围保留为其他适用设计与验收的边界，不在本专题宣告完成。

## 11. 验证设计与可追溯性

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-DWE-001](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001) | [过滤匹配与拒收边界](#des-filter-001) | 校验、预编译和直接匹配一致拒绝非法配置；合法 null、缺失路径、转义路径在事件/动作路由中行为一致。 | [filter_contract.rs](../../../crates/oasis7_wasm_router/tests/filter_contract.rs) 的四组回归；对候选版本运行 `env -u RUSTC_WRAPPER cargo test -p oasis7_wasm_router`，覆盖 library 与 integration 层。 | 候选版本的 Cargo 测试结果。 | 不证明完整 sandbox 副作用隔离、共识或跨版本回放；拒收收紧后应重新验证既有模块配置。 |

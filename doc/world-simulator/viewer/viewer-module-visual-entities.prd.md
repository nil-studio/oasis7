# oasis7 Simulator：WASM 模块驱动新事物的通用可视实体机制

- 对应设计文档: `doc/world-simulator/viewer/viewer-module-visual-entities.design.md`
- 历史模块渲染实施与验证记录：GitHub task issue evidence。

## 文档身份与审读范围

- 专业专题：World Simulator / Viewer 模块可视实体；需求语义仍由配对 PRD 的原条款拥有，专业入口为 [World Simulator](../README.md)。
- 协作角色：`runtime_engineer` 核对数据、初始化与事件边界；`viewer_engineer` 核对渲染与交互消费者；`qa_engineer` 分层记录验证。
- 原文未显式声明生命周期；本次不新增 active/retired 或交付状态，也不以历史实施入口、原 M1–M5 计划推定当前完成。
- 审读基线：2026-10-15，`cde57786e711b03a9c9fa2a5174aba357e8350d5`。本次只补既有合同身份、精确接收与验证定位；源码断言仅静态阅读，未执行游戏、模型、Rust 或 UI。

审计轮次: 5

## 1. Executive Summary

- 为“模块驱动的新生事物”提供统一承载，避免为每种新模块能力新增一套专用 simulator 实体与 viewer 渲染分支。
- 将可视化语义下沉为 **runtime/simulator 可发布的数据实体**：模块只需上报“我想在世界中可视化什么”，viewer 统一渲染。
- 与“默认电力设施语义下沉为场景配置”保持一致：基础供能由 runtime 预置模块承担，模拟器只维护通用世界状态与可视化载体。

## 2. User Experience & Functionality

### In Scope
- 在 simulator 增加通用 `ModuleVisualEntity` 数据结构与初始化/场景配置接入。
- 在 action/event 层提供模块可调用的通用更新接口：`UpsertModuleVisualEntity` / `RemoveModuleVisualEntity`。
- 在回放链路保证该类事件可重放，保持快照与事件一致。
- viewer 侧新增统一渲染路径：从 `snapshot.model.module_visual_entities` 与增量事件中构建/更新 marker。
- viewer 详情面板支持展示模块可视实体的基础信息（module/kind/anchor/label）。
- 事件列表对象联动支持模块可视实体（点击事件可定位对应 marker）。
- 右侧信息区支持滚动浏览，保证长详情/长事件列表可访问。

### Out of Scope
- 不定义具体 WASM ABI 的细节字段（本期仅定义 simulator/viewer 内部数据形态）。
- 不做模块可视实体的复杂物理交互（碰撞、力学、资源账本影响）。
- 不引入独立的“模块可视实体治理权限系统”。

## 3. AI System Requirements (If Applicable)
- N/A: 本专题不新增 AI 专属要求。

## 4. Technical Specifications

<a id="module-visual-data-contract"></a>
### `ModuleVisualEntity`

Trace ID: O7-REQ-c95f5eba-5944-4e5c-aef4-0862c9f07120

本身份仅标识本节原字段、[三类锚点](#module-visual-anchor-contract)与[初始化/场景校验](#module-visual-init-contract)的既有内部数据合同。产品适用性：N/A；reason=本条只约束内部模型形态与输入有效性，不独立表达玩家结果；scope=ModuleVisualEntity schema、三类 anchor 和初始化 ID/边界校验。玩家读取与定位义务仍分别见下方 V1/V1.1 的真实产品接收，不用此处 N/A 代替它们。
- `entity_id: String`：实体唯一 ID（世界内唯一）。
- `module_id: String`：来源模块 ID（例如 `m1.power.radiation_harvest`）。
- `kind: String`：实体类别（如 `radiation_harvester`、`relay`、`beacon`）。
- `label: Option<String>`：可选显示名。
- `anchor: ModuleVisualAnchor`：锚点。

<a id="module-visual-anchor-contract"></a>
### `ModuleVisualAnchor`
- `Agent { agent_id }`：挂载在 Agent。
- `Location { location_id }`：挂载在 Location。
- `Absolute { pos }`：世界绝对坐标。

<a id="module-visual-event-contract"></a>
### 世界模型与事件

Trace ID: O7-REQ-c69ce228-a086-4fd7-9745-c5a41edaa9c8

本身份仅标识本节原 BTreeMap/serde default、Action/Event 与 §2 既有快照和增量事件可回放一致性合同。产品适用性：N/A；reason=本条是状态序列化与事件应用机制，不单独证明玩家看到或理解结果；scope=内部 module_visual_entities 状态、upsert/remove 及 snapshot replay。Viewer 消费者及产品范围另由 V1/V1.1 接收；不把 marker 或 UI selection 提升为世界 authority。
- `WorldModel.module_visual_entities: BTreeMap<String, ModuleVisualEntity>`（`serde default`）。
- `Action` 增加：
  - `UpsertModuleVisualEntity { entity }`
  - `RemoveModuleVisualEntity { entity_id }`
- `WorldEventKind` 增加：
  - `ModuleVisualEntityUpserted { entity }`
  - `ModuleVisualEntityRemoved { entity_id }`

<a id="module-visual-init-contract"></a>
### 初始化/场景
- `WorldInitConfig.module_visual_entities` 与 `WorldScenarioSpec.module_visual_entities`。
- 初始化时校验：
  - `entity_id` 非空且不冲突；
  - 锚点存在（Agent/Location）或坐标在空间边界内（Absolute）。

<a id="module-visual-render-contract"></a>
### Viewer 渲染约定（V1）

Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；局部消费 [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001) / [AC-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-001) 中“相关对象可读”的子义务：已由权威数据发布且适用于该 surface 的模块实体具有可读 marker/label。以下技术条款不承接整个 primary read、玩家因果、密度变化、下一步或恢复组合，也不新增所有正式入口的支持声明。
- 模块可视实体采用统一 marker 渲染路径（单一 mesh/material 族）。
- 渲染位置由 `anchor` 解析；`label` 为空时回退为 `kind:entity_id`。
- 通过增量事件做 upsert/remove，避免全量重建。

<a id="module-visual-interaction-contract"></a>
### Viewer 交互约定（V1.1）

Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；局部消费 [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003) / [AC-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-003) 中“选择对象/事件并查看”的子义务：原 §2 的基础详情、事件定位与长内容滚动可达。下列兼容查找及滚轮路由仍为原专业合同，不据此承接完整多尺度探索、返回当前目标、观察授权或跨入口验收，也不把可见/选中解释为控制权。
- `ModuleVisualEntityUpserted/Removed` 事件可映射为对象联动目标（`SelectionKind::Asset` 复用路径）。
- 对象实体查找顺序：先真实资产 `asset_entities`，再模块可视实体 `module_visual_entities`。
- 右侧内容区域启用滚动条，滚轮仅在右侧面板命中时生效，避免干扰 3D 视图缩放。

## 5. Risks & Roadmap

- **M1**：设计文档与GitHub Issue/Project task truth。
- **M2**：simulator 数据结构、action/event、初始化与回放链路打通。
- **M3**：viewer 通用渲染 + 基础详情展示。
- **M4**：测试回归、文档状态回写、任务日志与提交。
- **M5**：补齐事件联动与右侧滚动交互。

### Technical Risks

- **ID 冲突**：模块侧若未约束命名空间，可能与其他实体产生重名冲突。
- **锚点失效**：锚点对象被删除或场景不含对应对象时，需要明确拒绝策略。
- **信息噪声**：模块实体过多会挤占视野，后续可能需要分层过滤。
- **语义漂移**：若模块把业务状态直接塞入标签文本，可能导致可视化不可检索；后续可考虑结构化扩展字段。

## 6. Validation & Decision Record
- 追溯: 历史模块渲染实施与验证记录由 GitHub task issue evidence 维护。

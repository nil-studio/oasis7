# Viewer 模块可视实体通用机制设计

- 对应需求文档: `doc/world-simulator/viewer/viewer-module-visual-entities.prd.md`
- 历史模块渲染实施与验证记录：GitHub task issue evidence。

## 文档身份与审读范围

- 专业专题：World Simulator / Viewer 模块可视实体；需求语义仍由配对 PRD 的原条款拥有，专业入口为 [World Simulator](../README.md)。
- 协作角色：`runtime_engineer` 核对数据、初始化与事件边界；`viewer_engineer` 核对渲染与交互消费者；`qa_engineer` 分层记录验证。
- 原文未显式声明生命周期；本次不新增 active/retired 或交付状态，也不以历史实施入口、原 M1–M5 计划推定当前完成。
- 审读基线：2026-10-15，`cde57786e711b03a9c9fa2a5174aba357e8350d5`。本次只补既有合同身份、精确接收与验证定位；源码断言仅静态阅读，未执行游戏、模型、Rust 或 UI。

## 1. 设计定位
定义“模块驱动的新事物”统一可视实体机制：让 runtime/simulator 只发布通用 `ModuleVisualEntity` 数据，viewer 负责统一渲染与详情呈现，避免每种模块能力都扩一套专用实体分支。

## 2. 设计结构
- 数据模型层：在 simulator 中引入通用 `ModuleVisualEntity` 与初始化/场景接入。
- 事件更新层：通过 `UpsertModuleVisualEntity` / `RemoveModuleVisualEntity` 维持快照与增量一致。
- Viewer 渲染层：从 `snapshot.model.module_visual_entities` 和事件流构建 marker。
- 详情联动层：右侧详情、事件对象联动和滚动浏览都支持模块可视实体。

### 2.1 需求承接与分配表

每行保持配对 PRD 原语义。前两行仅内部工程机制没有独立产品结果，后两行通过原专业条款精确引用现有产品 ID；不为工程机制虚构产品承诺，也不把真实用户结果统一写成 N/A。

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-c95f5eba-5944-4e5c-aef4-0862c9f07120；[内部数据与初始化合同](viewer-module-visual-entities.prd.md#module-visual-data-contract) | 原字段、三类 anchor、初始化/场景输入及 ID 非空/冲突、锚点存在或坐标边界校验。产品适用性：N/A；reason=内部数据输入合同无独立玩家结果；scope=schema、anchor 与初始化校验。 | [数据与初始化接收](#module-visual-data-receiver) | `runtime_engineer`；runtime/simulator 生产者与原场景输入 | 不改 schema/权限，不新增旧 snapshot 迁移，不裁定后续锚点对象删除策略。 |
| Trace refs: O7-REQ-c69ce228-a086-4fd7-9745-c5a41edaa9c8；[状态与事件合同](viewer-module-visual-entities.prd.md#module-visual-event-contract) | BTreeMap/serde default、upsert/remove Action/Event、快照与增量回放一致。产品适用性：N/A；reason=内部状态应用机制不能单独表达玩家结果；scope=状态序列化、事件应用与回放。 | [事件与快照接收](#module-visual-event-receiver) | `runtime_engineer`；原 kernel/model/journal authority，Viewer 只消费 | 不把本地 marker 更新当成 runtime 接受、世界因果或跨入口完成。 |
| Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；[V1 渲染合同](viewer-module-visual-entities.prd.md#module-visual-render-contract) | 适用 surface 的已发布模块实体按原统一 marker 族、anchor 解析、空 label→kind:entity_id 与增量更新保持对象可读；仅 STAGE-001 的相关对象读取子范围。 | [渲染与 fallback 接收](#module-visual-render-receiver) | `viewer_engineer`；runtime/simulator snapshot/event 输入；`qa_engineer` 分层证据 | 不覆盖完整目的/因果/密度/下一步组合；不升级产品状态，不扩大 Web/原生支持。 |
| Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；[V1.1 交互合同](viewer-module-visual-entities.prd.md#module-visual-interaction-contract) | 原基础详情、事件定位、SelectionKind::Asset 兼容、资产优先查找与右侧命中滚动；仅 STAGE-003 的对象/事件选择检查可达子范围。 | [详情与事件交互接收](#module-visual-interaction-receiver) | `viewer_engineer`；各实际 selection/event consumer；`qa_engineer` | 不覆盖完整缩放探索/回到目标；不改查找顺序、滚轮路由或控制权限；不把不同 surface 的实现差异视为合同已变。 |

## 3. 关键接口 / 入口
- `ModuleVisualEntity`
- `UpsertModuleVisualEntity` / `RemoveModuleVisualEntity`
- `snapshot.model.module_visual_entities`
- viewer 详情与事件联动入口

### 3.1 既有合同的精确接收

<a id="module-visual-data-receiver"></a>
#### 数据与初始化接收

Trace refs: O7-REQ-c95f5eba-5944-4e5c-aef4-0862c9f07120；对应 [专业数据合同](viewer-module-visual-entities.prd.md#module-visual-data-contract)。`entity_id`、`module_id`、`kind`、可选 `label` 与 `anchor` 沿原类型；Agent/Location/Absolute 沿原锚点与初始化存在/空间边界校验，ID 非空及不冲突义务保持。初始化/场景仍是原输入路径，不添加字段、schema revision 或权限语义。产品适用性：N/A；reason=这里只接收内部数据与初始化有效性；scope=原模型/anchor/输入校验，用户读取另由渲染与交互行接收。

<a id="module-visual-event-receiver"></a>
#### 事件与快照接收

Trace refs: O7-REQ-c69ce228-a086-4fd7-9745-c5a41edaa9c8；对应 [专业状态与事件合同](viewer-module-visual-entities.prd.md#module-visual-event-contract)。保留 `WorldModel.module_visual_entities: BTreeMap<String, ModuleVisualEntity>` 的 `serde default`，以及 `UpsertModuleVisualEntity` / `RemoveModuleVisualEntity` 和 `ModuleVisualEntityUpserted` / `ModuleVisualEntityRemoved`。快照与增量事件一致、可回放是原义务；消费者只能呈现权威状态，不形成第二份世界真值。产品适用性：N/A；reason=序列化及事件应用属于内部机制；scope=原状态/事件/replay，不代签界面或玩家因果。

<a id="module-visual-render-receiver"></a>
#### 渲染与 fallback 接收

Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；对应 [V1 原条款](viewer-module-visual-entities.prd.md#module-visual-render-contract)。从原 snapshot/event 输入构建、增量 upsert/remove 统一 marker，按原 anchor 解析位置，空 `label` 保留 `kind:entity_id` fallback。此处只接收相关对象可读的局部贡献；源码中具体表面使用的 sprite、identity chip、本地化或标签抑制只能作为该表面现状证据，不能据此改写原单一 mesh/material 族合同、fallback 或新增图层过滤义务。

<a id="module-visual-interaction-receiver"></a>
#### 详情与事件交互接收

Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；对应 [V1.1 原条款](viewer-module-visual-entities.prd.md#module-visual-interaction-contract)。详情继续展示 module/kind/anchor/label；事件对象联动复用 `SelectionKind::Asset`，先查 `asset_entities`，再查 `module_visual_entities`。右侧内容有滚动条且滚轮仅命中该面板时生效，保持原避免干扰 3D 缩放的边界，不借此次身份化推进新 3D。对使用 `selectedKind=module_visual` 的 software_safe fixture，须另核真实 surface 的接线与兼容关系；手动赋值不是事件映射、拾取或资产优先查找的证明。可见、选中、label 不表示所有权、Agent 控制或操作授权。

## 4. 约束与边界
- 本期只定义 simulator/viewer 内部数据形态，不展开 WASM ABI 细节。
- 不做复杂物理交互或独立治理权限系统。
- 新增模块能力优先发布通用实体，而不是继续堆专用渲染分支。
- 长详情和长事件列表必须可滚动访问，不能因实体变多而失去可读性。

## 5. 设计演进计划
- 先落通用实体模型和事件接口。
- 再接 viewer 渲染与详情联动。
- 最后通过回放一致性和 UI 测试收口模块可视机制。

## 6. 目标范围、残余风险与未决边界

目标仍是原通用实体机制与可读详情，§1–§5 原设计与配对 PRD 全部原字段/条款保持。专业 domain 入口、runtime-only/Web-first 等上位约束没有因本次文档接收改变；原历史 M1–M5 与设计演进顺序不表示当前实现/验收完成。

原 Technical Risks 中“锚点对象后续删除或场景缺对象时的拒绝策略”仍待细化，不借已有初始化拒绝断言替它作决定；初始化已确定的存在/边界校验并非未决。分层过滤、结构化扩展字段仍是未来可能性，不纳入此次交付合同。不增加 WASM ABI、碰撞/力学/资源账本影响或独立治理权限系统。

本次是小型兼容文档编辑，只补原义接收和验证定位，不改变状态、序列化、接口、authority 或运行实现；未补写无关部署/迁移/运维段落的残余风险，是这些既有专题义务仍须由原专业 authority 在实际变更时验证，不能以本次文档通过代签。

### 11.1 验证映射表

下列 source 的断言已在固定源码基线 `cde57786e711b03a9c9fa2a5174aba357e8350d5` 阅读，均未执行；`planned` 是尚待实施/执行的场景，不是同名测试已存在。未来验证须记录自己的候选 SHA、surface、fixture/输入、环境及实际输出，不把本次文档检查称为业务测试。纯工程行的产品 N/A 不豁免本表技术验证。表中 Web 计划统一使用 [Web UI 闭环手册](../../testing/manual/web-ui-agent-browser-closure-manual.manual.md) 的适用入口，只作为未来方法，不证明本轮执行。

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-c95f5eba-5944-4e5c-aef4-0862c9f07120；[内部数据与初始化合同](viewer-module-visual-entities.prd.md#module-visual-data-contract) | [数据与初始化接收](#module-visual-data-receiver) | 原模型、三类 anchor、初始化有效性 | `planned`；`test_tier_required`；[Simulator 测试源](../../../crates/oasis7_simulator/src/tests/module_visual.rs) 已读 `init_accepts_module_visual_entities_with_all_anchor_types`、`init_rejects_module_visual_when_agent_anchor_missing`、`init_rejects_module_visual_when_absolute_anchor_out_of_bounds` 的断言。未来同一候选在 Simulator 初始化层逐项运行；空/冲突 ID、missing Location、场景输入等剩余分支须先补核或建立准确 fixture。 | 候选 SHA、固定输入、每一初始化正/负例和实际输出；schema 兼容另行记录 | 未运行；这些断言不证明所有 ID/anchor 冲突、所有场景输入或后续锚点删除策略。 |
| Trace refs: O7-REQ-c69ce228-a086-4fd7-9745-c5a41edaa9c8；[状态与事件合同](viewer-module-visual-entities.prd.md#module-visual-event-contract) | [事件与快照接收](#module-visual-event-receiver) | upsert/remove Action/Event、快照+增量一致与回放 | `planned`；`test_tier_required`；[Simulator 测试源](../../../crates/oasis7_simulator/src/tests/module_visual.rs) 已读 `kernel_supports_module_visual_entity_upsert_and_remove`、`kernel_rejects_module_visual_upsert_when_anchor_missing`、`replay_from_snapshot_applies_module_visual_upsert_event`。未来在同候选 kernel/replay 层运行；remove replay、serde default 兼容和完整状态对账仍须单独补核/fixture。 | 候选 SHA、snapshot/journal 输入、事件类型、model 输出与 replay 对账 | 未运行；现有 replay 只断言 upsert 后 key 存在，不证明 remove 回放、全部字段等价或跨入口玩家体验。 |
| Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；[V1 渲染合同](viewer-module-visual-entities.prd.md#module-visual-render-contract) | [渲染与 fallback 接收](#module-visual-render-receiver) | 适用 surface 的 marker/anchor/label 与增量对象可读 | `planned`；`test_tier_required`；[Bevy render 测试源](../../../crates/pixel_world_bridge/src/render_module_visual_entities_tests.rs) 已读 `module_visual_entities_render_interactive_markers_and_reconcile_stale_markers` 与 `module_visual_labels_are_zoom_gated_stably_suppressed_and_reconciled`，包含 marker/hit-region、删除清理与无 label fallback 断言。未来同候选分别核 producer 的三类 anchor 解析、实际 Viewer 消费及下述 Web 手册的适用入口；正常、增改删、缺 label 分层留证。 | render-layer 输入/断言，实际 surface 的对象读取与删除结果；各层分开记录候选/环境 | 未运行；CPU test app/raster 不证明 GPU、真实输入事件、统一 mesh/material 跨 surface 实现或完整 STAGE-001。 |
| Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；[V1.1 交互合同](viewer-module-visual-entities.prd.md#module-visual-interaction-contract) | [详情与事件交互接收](#module-visual-interaction-receiver) | 基础详情、事件目标、资产优先与面板内长内容可达 | `planned`；`test_tier_full`；[详情测试源](../../../crates/oasis7_viewer/software_safe_src/module_visual_details.test.jsx) 已读可读详情/无 Agent 控制/本地化 fallback；它 mock PixelWorldHost 并手动设 selectedKind。另 `crates/pixel_world_bridge/src/render_module_visual_entities_tests.rs` 的 `co_anchored_module_hit_stack_keeps_agent_and_location_centers_selectable` 只核本地 hit_test。未来按下述 Web 手册在固定真实候选/surface 核事件定位、Asset 兼容映射、同 ID 资产优先、删除对象、长详情/长事件列表及面板内外滚轮；每个场景均单独留证。 | 真实输入→对象/详情的结果、同 ID 查找结果、面板内滚动与面板外视图行为；candidate/surface/environment | 未运行；mock selectedKind、Bevy hit_test 不证明真实拾取/事件接线/滚轮路由；不代签完整 STAGE-003、多尺度或返回目标。 |

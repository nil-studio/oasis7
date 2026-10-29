# Viewer Pixel World Bridge Rendering 设计

- 对应需求文档: `doc/world-simulator/viewer/viewer-pixel-world-bridge-rendering.prd.md`
- 已完成的实施追溯见 GitHub task issue evidence comments 与 `.pm/github-project-sync/task-archive.jsonl`。
- 专业 owner：`viewer_engineer`；视觉判定由 `game_visual_interaction_designer` 与 `qa_engineer` 协作。
- 历史审读基线：`eng-cc/oasis7@fd5fa7c8ff1851a32cb3e71d488783ca59e05a5e`（2026-10-12）；以下规定局部复用与视觉断言的实现合同。各候选的 Vitest、Rust native、wasm 构建和真实浏览器结果分别记录于 PR；源码及合成输入回归不能代替真实视觉验收。性能收益须单独测量。

## 1. 设计定位
维护 `pixel_world_bridge` 的增量更新结构，保持当前 Web 宿主合同与 wasm-only 出货边界。grid/location/agent 已采用按类型 reconcile；以下承接剩余的局部重复工作与视觉回归判定缺口，不把早期整场景重建问题重新列为当前事实。

## 2. 设计结构
- 共享态同步层：沿用 `render_version + input_events` 的宿主同步模式。
- runtime cache 层：沿用 grid key、location entity map、agent entity map。
- scene update 层：按类型增量更新、按需回收；`static_reconcile` 与仅动画更新分开。
- hit-test cache 层：保留现有矩形命中模型，由 dirty 状态和缓存 key 决定重建。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef；[增量更新与视觉回归要求](viewer-pixel-world-bridge-rendering.prd.md#render-reconcile-and-visual-regression-requirements) | 构建世界尺度 surface 时只枚举一次，selected、fallback 和 nearest 的既有语义相同 | [锚点枚举](viewer-pixel-world-bridge-rendering.design.md#anchor-enumeration-target) | `viewer_engineer`；snapshot locations 原始顺序 | 不修改无效坐标策略，不新增跨 snapshot 缓存 |
| Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef；[增量更新与视觉回归要求](viewer-pixel-world-bridge-rendering.prd.md#render-reconcile-and-visual-regression-requirements) | 静态 reconcile 复用有效 depot 的 8 段实体，并清理失效项；纯动画路径保持隔离 | [范围圈复用](viewer-pixel-world-bridge-rendering.design.md#depot-outline-reuse-target) | `viewer_engineer`；Runtime 设施身份/位置/半径 | 不新增设施交互、camera 语义或帧率承诺 |
| Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef；[增量更新与视觉回归要求](viewer-pixel-world-bridge-rendering.prd.md#render-reconcile-and-visual-regression-requirements) | selected raster 能检出缺臂、位置或遮挡退化 | [selected raster](viewer-pixel-world-bridge-rendering.design.md#selected-raster-regression-target) | `qa_engineer`、`game_visual_interaction_designer`；固定 fixture 与动画时刻 | 不另建 CI 框架，不代替真实 WebGL 证据 |
| Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef；[增量更新与视觉回归要求](viewer-pixel-world-bridge-rendering.prd.md#render-reconcile-and-visual-regression-requirements) | 真实 browser crop 对缺失或错位的普通 location 角框能够失败 | [browser crop](viewer-pixel-world-bridge-rendering.design.md#browser-crop-regression-target) | `qa_engineer`、`game_visual_interaction_designer`；同候选 bundle/browser | 不代替 #3017 的独立视觉验收 |

## 3. 关键接口 / 入口
- `BevyRuntimeState`
- `render_scene`
- `reconcile_grid`
- location / agent sprite 更新路径

## 4. 约束与边界

以下继续固定现有宿主与世界真值边界；本次只规范文档关系。
- 不改 `pixel_world_runtime_module_wasm.js` 的宿主消费方式。
- 不新增第二套 `pixel_world_bevy_bridge.js` JS 世界渲染路径。
- 不引入新的 render DTO 字段。
- 不在本轮切换到 camera transform / shader grid / picking plugin。

<a id="render-reconcile-and-visual-regression-target"></a>
## 5. 局部复用与视觉回归实现

直接上游为[增量更新与视觉回归专业要求](viewer-pixel-world-bridge-rendering.prd.md#render-reconcile-and-visual-regression-requirements)。本节承接 #2116 汇总中仍有效的低优先级工程项，区分源码实现、自动测试和真实视觉验收；旧汇总关闭不代替其中任何验证。Runtime 继续拥有世界位置、设施半径及库存真值，Viewer 仅计算表现与局部缓存，不新增 DTO、宿主事件、持久化、权限或部署接口。已有英文 anchor 保持稳定，名称中的 `target` 不作为实现状态。

### 5.1 当前状态与差距

| 对象 | 审读基线中的差距 | 历史候选实现（非本次新增） | 未证明范围 | 实现/测试来源 |
| --- | --- | --- | --- | --- |
| 世界尺度锚点 | 有效 selected 分支只枚举 locations 一次；未选中或无效 selected 分支会在锚点与 nearest 构建中分别枚举 | 每次 surface 构建共用一次原始枚举，结果及 fallback 顺序不变；既有测试定义断言每次构建仅枚举一次 | 未测量用户可感知延迟收益 | [`viewer_world_scale_module.js`](../../../crates/oasis7_viewer/software_safe_src/viewer_world_scale_module.js) |
| depot 服务范围圈 | 每个有效正半径设施在每次静态 reconcile 删除并重建 8 段，且逐设施扫描全部旧范围圈；纯动画 reconcile 已不走此分支 | 用 `(depot_id, segment)` 复用实体，一次建立索引，更新和清理按键处理；新增身份、几何及清理回归 | 同候选 Rust 结果记录于 PR；真实场景 CPU、帧时间或容量未测量时，不能称为已确认瓶颈 | [`render_micro_depot_facilities.rs`](../../../crates/pixel_world_bridge/src/render_micro_depot_facilities.rs)、[`render.rs`](../../../crates/pixel_world_bridge/src/render.rs) |
| raster / browser 判定 | 两个 selected raster 入口以图层/颜色存在为主；location-frame browser 全图与 crop 主要检查非黑比例和亮度 | 既有 selected 入口增加固定 ROI 像素掩码；既有 browser 入口读取 crop 的四条臂及另两角留白，新增像素解析和负例测试 | Rust raster 和真实 WebGL 执行仍须区分；合成图通过不证明真实 GPU 颜色、文本遮挡或可读性通过 | [`render_selected_raster_tests.rs`](../../../crates/pixel_world_bridge/src/render_selected_raster_tests.rs)、[`location-frame smoke`](../../../crates/oasis7_viewer/scripts/pixel-world-location-frame-visual-smoke.mjs) |

<a id="anchor-enumeration-target"></a>
### 5.2 锚点枚举一次，保留 fallback 语义

Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef；局部承接[REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001)：该条仅保持现有选中对象与空间显示语义；工程算法不构成产品性能或全矩阵验收承诺。

`buildWorldScaleSurface` 先获取一次原始 `Object.values(snapshot.model.locations || {})`，将同一数组传入 `selectedWorldAnchor` 并供 nearest 候选过滤使用，数组只在本次构建内存活。有效 selected 仍优先，保留其 `kind/id/pos/radiusCm/locationId` 映射。

fallback 继续按原始枚举顺序取第一个 `location?.pos` 为真值的记录，保留 `id || name || "location"`、`locationId: id || null` 等映射；不得先将数组按 `id` 或有限坐标过滤后再选锚点。nearest 候选仍独立要求 `id && pos`，距离计算继续排除不能解析的坐标。无 id、非法但 truthy 的 pos、空集合及等距离项的旧结果必须保持；若要改变这些语义，应单独修改其专业要求。

<a id="depot-outline-reuse-target"></a>
### 5.3 depot 范围圈按段复用

Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef；局部承接[REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001)：该条仅保持现有选中对象与空间显示语义；工程算法不构成产品性能或全矩阵验收承诺。

内部 `PixelWorldMicroDepotServiceRadiusVisual` 新增 `segment: u8`，取值 `0..8`。每次静态 reconcile 从现有 query 建一次 `(id, segment) -> Entity` 索引与 active-key 集；沿用现有资格判断、8 点几何、颜色、厚度和层级，已有键只更新 `Sprite/Transform`，缺失键才 spawn，未活跃键才 despawn。半径归零、设施消失、既有资格条件不再满足、world bounds 缺失、无 render state 或卸载时，不得残留范围圈，也不得增加 hit region。

保留 [`render_scene`](../../../crates/pixel_world_bridge/src/render.rs) 的调度边界：内容/选择、输入或画布尺寸变化可触发静态 reconcile；`animation_dirty` 单独触发的更新不得进入 depot 重建分支。静态再次处理同一有效设施集合或仅其他对象变化时，8 段实体 ID 保持不变；camera/radius 改变只更新其几何。目标是一次索引和按键处理、消除重复创建销毁；实际耗时收益仍未知，宣称性能改善前须记录硬件、设施规模、更新刺激、样本窗口及帧时间分位数。

<a id="visual-regression-target"></a>
### 5.4 增强已有视觉判定

<a id="selected-raster-regression-target"></a>

Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef；局部承接[REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001)：该局部视觉判定保护既有提示与决策锚点可辨；只验证本条层级，不代签完整跨 surface AC。

selected location 与 agent 的现有两项 `bevy_pixel_regression_exports_selected_*` 测试使用 960×540 画布、中心 `(480,270)` 的固定目标和 reduced motion。location 的独立整数掩码要求 160 个琥珀色像素组成完整 2px 空心圈；agent 要求 92 个像素组成四个 L 角，保留角间空白，并排除同为琥珀色的 Agent 本体区域。期望不从 cue 的实际 Transform/Sprite 反算，因此缺臂、错位、错误填满或可见遮挡会失败。Agent 测试还删除一条真实 ECS 角臂作为负控：原始「仍有琥珀色」条件成立，而新掩码必须拒绝。

存在琥珀色、文件存在或 hash 不等于全背景均不能单独判为正确。继续使用 [`viewer-pixel-world-bevy-pixel-regression.sh`](../../../scripts/viewer-pixel-world-bevy-pixel-regression.sh)，导出正常 fixture 的 PNG 与摘要；不另建全图 golden 平台或 CI 框架。这些是源码中的断言合同，实际 Rust 执行结果须在同候选 PR 中记录。

<a id="browser-crop-regression-target"></a>

Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef；局部承接[REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001)：该局部视觉判定保护既有提示与决策锚点可辨；只验证本条层级，不代签完整跨 surface AC。

普通未选中 location 角框已有几何、Entity 复用、命中区域、排除/清理测试，以及 `location_corner_frame_pixels == 23` 和基色 `[82, 170, 138, 255]` 的[软件 raster 断言](../../../crates/pixel_world_bridge/src/render_location_corner_frame_raster_tests.rs)，这些覆盖继续保留。浏览器增量补真实 WebGL 输出的判定入口：现有 location-frame smoke 设置并回读 reduced motion，以 `recent_event_glyphs` 的 `loc-1` 固定相位和 DTO 尺寸预期，在 desktop/narrow 中读取真实 viewport 截图。

[`pixel-world-location-frame-pixels.mjs`](../../../crates/oasis7_viewer/scripts/pixel-world-location-frame-pixels.mjs) 按实际截图尺寸与 CSS viewport 比例裁取 64 CSS 像素区域，不把页面 scroll offset 混入 viewport 坐标；BMP 检查支持 24/32 位 RGB、上下行方向及标准 RGB mask，解析或越界错误明确失败。两处 L 角的四条臂分别在交点之外采样，位置容差为 1 CSS 像素，每臂覆盖至少 60%；另外两角覆盖不得超过 20%，防止整片亮色或四角框误过。

仅靠邻域重复采样会让几颗不相连的亮点误过，因此每角还须有靠近交点的连通 mint 分量，横纵跨度至少为 `max(3, armLength-1)` CSS 像素，面积至少为名义两臂并集的 55%，内侧保持留白。mint 色阈值为 `G >= 60`、`G-R >= 20`、`G-B >= 8`；容差、坐标、DPR、逐臂覆盖、连通面积/跨度与实际 crop 一起记录。

现有 Vitest 入口自动发现的 `pixel_world_location_frame_pixels.test.js` 共 23 项，覆盖 RGB/行方向、格式拒绝、1/1.5/2 倍截图比例、次像素偏移，以及本体仍可见但无角框、缺单角/单臂、明显错位、错色、全白/全 mint、多余角框、不相连短条、孤立交点和实心角块的负例。这些通过只证明解析与判定算法能区分受控输入；真实 Bevy 合成、字体遮挡及具体候选的颜色容差仍待 S6 验证，不能用 DOM/DTO 状态或合成图冒充真实浏览器结果。

[testing-manual S6](../../../testing-manual.md#s6web-ui-闭环-smoke-套件l4a) 继续拥有真实浏览器与模型视觉评审的适用规则。[#3017](https://github.com/eng-cc/oasis7/issues/3017) 承载的独立视觉义务仍须以其受测候选、场景和真实证据验收，不能由本设计承接、Rust 通过、crop 自动检查或 #2116 关闭代签。

<a id="render-reconcile-validation"></a>
## 11. 验证设计与交付边界

回归覆盖包括锚点枚举与输出、实体身份与几何更新、真实 ECS 栅格的精确掩码，以及 crop 判定的合成正负例。Rust native、wasm 构建和真实浏览器 S6 分别验证；合成像素通过不证明真实 Bevy 颜色、字体遮挡或浏览器容差已验收。缺少 `agent-browser`、`sips` 或可运行候选时应记录 blocked，不能标记视觉完成。实际执行按受影响范围复用现有检查，版本、环境、结果和产物记录在对应 PR；不新增通用完成证明或 CI 必跑项。

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef；[增量更新与视觉回归要求](viewer-pixel-world-bridge-rendering.prd.md#render-reconcile-and-visual-regression-requirements) | [锚点枚举](viewer-pixel-world-bridge-rendering.design.md#anchor-enumeration-target) | 修改世界尺度构建时，单次枚举且输出兼容 | `npm --prefix crates/oasis7_viewer run test:ui -- viewer_world_scale_module`；现有 [`viewer_world_scale_module.test.js`](../../../crates/oasis7_viewer/software_safe_src/viewer_world_scale_module.test.js) 覆盖有效/无效/缺失 selected、无 id fallback、非法 pos、空集合、等距离顺序，并观察每次构建 locations 枚举次数为 1 | 同候选 Vitest 10 项通过输出 | 不证明帧率或真实浏览器可读性 |
| Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef；[增量更新与视觉回归要求](viewer-pixel-world-bridge-rendering.prd.md#render-reconcile-and-visual-regression-requirements) | [范围圈复用](viewer-pixel-world-bridge-rendering.design.md#depot-outline-reuse-target) | 修改范围圈 reconcile 时，复用/几何/清理及调度隔离 | 按[测试手册 S5](../../../testing-manual.md)执行 `env -u RUSTC_WRAPPER cargo test -p pixel_world_bridge --lib micro_depot_service_radius`；`render_micro_depot_facilities_tests.rs` 覆盖重复静态更新、DTO 重排/无关对象变化、camera/radius 变化、仅动画、归零/缺 bounds/删除/卸载；Rust/wasm 改动执行 `env -u RUSTC_WRAPPER cargo check -p pixel_world_bridge --target wasm32-unknown-unknown` | 同候选实体 ID、几何和计数断言；分别记录 native/wasm 结果 | 未测量真实 wasm 性能；native 测试不证明 WebGL 输出 |
| Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef；[增量更新与视觉回归要求](viewer-pixel-world-bridge-rendering.prd.md#render-reconcile-and-visual-regression-requirements) | [selected raster](viewer-pixel-world-bridge-rendering.design.md#selected-raster-regression-target) | selected raster 变化时，正确 ROI 通过，缺臂/错位失败 | 复用[测试手册](../../../testing-manual.md)的 `./scripts/viewer-pixel-world-bevy-pixel-regression.sh`；`render_selected_raster_tests.rs` 现有两项 selected 测试校验固定 mask 及缺臂负控；保留 `render_selected_agent_cue_tests.rs` 与 `render_location_corner_frame*_tests.rs` 的几何、复用、命中/清理覆盖 | 同候选 Rust 断言输出、PNG/crop 与像素摘要 | 软件 raster 不代替真实 GPU/browser 证据 |
| Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef；[增量更新与视觉回归要求](viewer-pixel-world-bridge-rendering.prd.md#render-reconcile-and-visual-regression-requirements) | [browser crop](viewer-pixel-world-bridge-rendering.design.md#browser-crop-regression-target) | 真实角框输出/采证脚本变化时，desktop+narrow crop 可识别正确两角及缺角失败 | `npm --prefix crates/oasis7_viewer run test:ui -- pixel_world_location_frame_pixels` 验证受控输入并纳入现有 JS gate；按[S6 浏览器手册](../../testing/manual/web-ui-agent-browser-closure-manual.manual.md)，先 `npm --prefix crates/oasis7_viewer run build:viewer:visual-test`，再 `npm --prefix crates/oasis7_viewer run test:pixel-world:location-frame:visual`；使用同候选 bundle，记录 WebGL renderer、DPR、viewport、console、目标坐标和容差，执行适用视觉评审 | 同候选 Vitest 23 项结果；两种 viewport 的真实截图/crop、自动判定及独立视觉评审 | 合成图不证明 GPU、其他视觉表面、世界效果或 #3017 完成 |

本节为低风险兼容补充：不迁移世界数据、持久化或 ABI；必要时回退对应源码改动即可重建本地实体缓存。回退算法不撤销缺失/错位必须可检出的验收要求。无需新增长期渲染服务、发布协议或审批流；Rust native、wasm 与真实视觉验收分别记录 pass、failed 或 blocked/unverified，不能用文档检查或合成图通过替代。

## 决策与风险

保留按类型 reconcile 与静态/动画分离，避免复用时遗留无效实体；旧 fallback 次序、视觉掩码及 crop 失败条件保持原合同。性能收益仍需实测，软件 raster/合成像素不替代真实 WebGL 或 #3017。2026-10-15 文档核对 main `b57e30eaaa0f3eb48c273eb62363d2ac2ada8981` 只确认源码与断言定义；未执行本文件所列行为测试。

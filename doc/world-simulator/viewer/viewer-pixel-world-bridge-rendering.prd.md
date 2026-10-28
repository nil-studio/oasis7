# Viewer Pixel World Bridge Rendering

- 已完成的实施追溯见 GitHub task issue evidence comments 与 `.pm/github-project-sync/task-archive.jsonl`。
- 关联主专题:
  - `doc/world-simulator/viewer/viewer-page-module-design-2026-06-18.design.md`
  - `doc/world-simulator/viewer/viewer-web-entry-compatibility.prd.md`

审计轮次: 1

## 1. Executive Summary
- 保持当前 `pixel_world_bridge` 的 wasm-only 宿主合同不变，优先收口 Bevy 内核的首轮性能债。
- 最初的优化对象是“每帧全量 despawn/spawn 全部 grid/location/agent”；当前已采用增量 reconcile 与响应式调度，剩余局部目标见[增量与回归要求](#render-reconcile-and-visual-regression-requirements)。
- 第一轮只做低风险 P0 优化：常驻网格、实体池、按需重建 hit region，不同时改宿主 API、world DTO 或页面交互语义。

## 目标
- 在不改变 `createPixelWorldBridge()` / `mount|update|unmount` / `camera_state_changed` / `hover_entity` / `select_entity` 契约的前提下，显著减少 `pixel_world_bridge` 每帧重建成本。
- 为后续 CPU culling、spatial hit test、真正相机下沉和 shader grid 提供更稳定的代码结构。

## 2. User Experience & Functionality
- 范围内：
  - `crates/pixel_world_bridge/src/lib.rs`
  - 与 `pixel_world_bridge` 直接相关的 repo-owned build / runtime regression
  - 对应专题文档、模块主项目追踪与 PR、实际 CI 与评审记录（Issue 按需）
- 范围外：
  - `pixel_world_host.jsx` 宿主布局或 unavailable 文案改版
  - `pixel_world_runtime_module_wasm.js` 事件协议调整
  - 新增 world DTO 字段、3D 渲染、shader/material 重写

## 范围
- 范围内：
  - `pixel_world_bridge` 的 Bevy wasm 内核结构优化。
  - 与该优化直接相关的 repo-owned wasm/unit/UI regression。
  - 对应专题文档、模块项目页 trace 与 GitHub task issue evidence 回写。
- 范围外：
  - 宿主页面结构或文案改版。
  - 运行时协议、world DTO 字段或 JS 事件契约调整。
  - 3D 渲染恢复、shader/material 重写或更激进的 camera 语义改造。

## 3. User Stories
- As a `viewer_engineer`, I want the embedded pixel-world wasm runtime to stop rebuilding the whole Bevy scene every frame, so that the main Web world stage can scale to denser snapshots without wasting CPU on ECS churn.
- As a `qa_engineer`, I want the optimization to preserve current runtime-visible contracts and explicit unavailable behavior, so that existing wasm/runtime regressions stay valid while performance improves.

## 4. Technical Specifications

### 4.1 Historical Bottlenecks
- 以下记录最初优化前的基线，不代表当前 main。历史 `render_scene` 每帧都会：
  - 遍历并 `despawn` 所有 `PixelWorldVisual`
  - 重建 grid 线条 sprite
  - 重建全部 location / agent sprite
  - 重新分配 `hit_regions`
- 当时 `render_state` 只在 `render_version` 变化时替换，但平移/缩放动画仍会触发整场景重建。

### 4.2 P0 Optimization Slice
- 场景实体分层：
  - `PixelWorldGridVisual`: 常驻 grid 实体
  - `PixelWorldLocationVisual`: location 实体池
  - `PixelWorldAgentVisual`: agent 实体池
- 引入 runtime cache：
  - 记录上次 grid 布局 key（窗口宽高、zoom、pan）
  - 维护 `location_id -> Entity` 与 `agent_id -> Entity` 映射
  - 记录当前活跃实体集合，用于增量回收多余实体
- 更新策略：
  - grid 仅在窗口尺寸或 camera 参数变化时更新
  - location / agent 仅更新已有实体的 `Sprite` / `Transform`
  - 数据集缩小时只回收多余实体，不再先清空全部

### 4.3 Behavioral Constraints
- 以下外部行为必须保持不变：
  - `mount` / `update` / `tick` / `unmount` 返回状态
  - `camera_state_changed`
  - `hover_entity`
  - `select_entity`
  - wasm runtime 仍为唯一渲染内核，不新增第二套 JS 世界渲染
- `hit_regions` 允许继续每帧重建，但必须从“场景重建副产物”切换为“基于已更新实体的轻量缓存”。

### 4.4 Out-of-Scope Follow-ups
- 不在本轮处理：
  - spatial hash / quadtree hit test
  - 将 pan/zoom 下沉到真实 `Camera2d` transform
  - procedural/shader grid
  - 渲染对象 viewport culling
- 若本轮后仍有性能压力，再拆下一 task。

## 接口 / 数据
- 保持不变的宿主接口：
  - `createPixelWorldBridge()`
  - `mount` / `update` / `tick` / `unmount`
  - `camera_state_changed`
  - `hover_entity`
  - `select_entity`
- 本轮涉及的数据与内部缓存：
  - `render_state` / `render_version`
  - grid layout key（窗口宽高、zoom、pan）
  - `location_id -> Entity` / `agent_id -> Entity`
  - `hit_regions`
- 本轮不新增任何 world DTO、宿主事件字段或 JS 世界渲染协议。

<a id="render-reconcile-and-visual-regression-requirements"></a>
## 增量更新与视觉回归的剩余目标

Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef；局部承接[REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001)：依产品 §5.3 的专业分配，以下兼容输出、范围圈及 selected/browser 视觉判定保持既有选中对象、空间含义与决策锚点可辨。枚举次数、实体复用和像素容差是已有工程细化，不新增产品性能承诺；不代签完整 viewport、语言、连接恢复组合 AC。

以下要求来源于 [#2116](https://github.com/eng-cc/oasis7/issues/2116) 的局部效率与验证条目，审读基线为 `fd5fa7c8ff1851a32cb3e71d488783ca59e05a5e`（2026-10-12）。它们是局部优化与回归测试的验收要求；实现、测试通过及真实性能证据分别核实，不承诺已测量的帧率收益，不改宿主 API、世界真值或玩家命中语义。对应[局部设计与验证](viewer-pixel-world-bridge-rendering.design.md#render-reconcile-and-visual-regression-target)。

| 来源 | 保留的专业要求 | 当前边界与完成条件 |
| --- | --- | --- |
| #2116 | 同一次 world-scale surface 构建复用 locations 的一次枚举；保持锚点 fallback、nearest 顺序、距离及显示输出。 | 审读基线仅回退锚点路径重复枚举。必须保留无 id 地点等旧输入语义；等价输出与枚举次数回归通过，并按 canonical Viewer build 更新产物。 |
| #3006 | depot 的服务范围圈按 depot/segment 复用实体，静态 reconcile 更新几何并回收失效段。 | 已有响应式调度，纯动画不触发该重建。相同输入、仅其他对象变化和 camera/viewport/location/radius 变化分别验证实体身份、几何与清理；不把旧重复工作直接判作当前性能瓶颈。 |
| #2442、#3035 | 加强现有 selected-location/agent raster 与真实浏览器 location-frame crop 的判定，能检出目标提示缺失或偏移。 | 保留已有 Rust 几何、角框像素和 hit-region 断言。采用确定性局部基线或形状/颜色/位置比较，浏览器允许受控容差；非黑或亮度检查不能单独代替目标可见性。复用现有测试入口，不新增通用 CI gate。 |

本组只承接这些明确的局部要求，不复活已交付任务的全部历史 AC。#3017 对真实连线世界的 desktop/narrow S6 义务仍由[独立入口](https://github.com/eng-cc/oasis7/issues/3017)承接，fixture 或软件 raster 不能代替它。

<a id="claim-evidence-classification"></a>
### 已覆盖的 evidence fixture 归属

#2701 的 claim evidence 页面归属已由[Viewer 前端结构标准](viewer-frontend-structure-standard.prd.md#接口--数据)和[构建真值](viewer-web-build-truth.prd.md#42-generated-artifact-single-source-of-truth)承接：`viewer_first_agent_claim_evidence.html` 是 canonical claim fixture，`dist/software_safe_first_agent_claim_evidence.html` 是生成的兼容测试页。它们不是新增玩家产品页。保留该导航即可，不再把旧文件名的负向检索或整批迁移核证作为当前未完成项；此分类不证明真实浏览器或产品验收完成。

## 5. Risks & Roadmap
- M1：完成专题 PRD / Project 建模，冻结 P0 优化边界。
- M2：完成 `pixel_world_bridge` 的实体池与常驻 grid 改造。
- M3：补齐定向验证、PR、实际 CI 与评审记录（Issue 按需） 与模块主项目追踪。

## 里程碑
- M1：专题 PRD / Project 建模完成。
- M2：`pixel_world_bridge` 的 grid/entity cache 与增量 reconcile 完成。
- M3：host tests、wasm unit tests、wasm check、viewer UI regression 与文档回写完成。

### Technical Risks
- 增量更新若回收逻辑不严，容易留下 stale entity 或错位 hit region。
  - 对策：把 grid/location/agent cache 分开，并补最小单测覆盖布局 key 与实体复用语义。
- 如果把优化 scope 扩到相机语义或 shader，会显著增加回归面。
  - 对策：本轮强制限制为 P0 结构优化，不改宿主合同。

## 风险
- 增量 reconcile 若漏掉 stale entity 回收，可能出现残留 sprite 或错误命中区域。
- 测试如果只停留在 host target，会再次掩盖 wasm runner / schema 漂移问题。
- 若后续把 scope 扩大到 camera/shader 层，本专题的低风险收口边界会被破坏。

## 6. Acceptance Criteria
- AC-1: `pixel_world_bridge` 不再在每一帧先 `despawn` 再 `spawn` 全部 grid/location/agent 实体。
- AC-2: grid 实体在相机与窗口参数不变时保持常驻；location / agent 使用实体池或等价增量更新结构复用已有 entity。
- AC-3: 现有 wasm-only 宿主合同、explicit unavailable contract 与 repo-owned runtime loader/host 测试语义保持不变。
- AC-4: 本轮完成 `cargo check -p pixel_world_bridge --target wasm32-unknown-unknown` 与相关前端 repo-owned 测试回归。

## 7. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-WORLD_SIMULATOR-046 | `task_40310c312e9f4681805b5b74b30cac9a` | `test_tier_required` | `env -u RUSTC_WRAPPER cargo check -p pixel_world_bridge --target wasm32-unknown-unknown` + `npm --prefix crates/oasis7_viewer run test:ui -- pixel_world` + `git diff --check` | `pixel_world_bridge` Bevy 内核增量更新、wasm runtime 宿主合同、repo-owned runtime/host 回归 |

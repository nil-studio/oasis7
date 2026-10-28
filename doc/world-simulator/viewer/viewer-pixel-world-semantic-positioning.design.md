# Viewer Pixel World Semantic Positioning 详细设计

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；[专业条款](viewer-pixel-world-semantic-positioning.prd.md#semantic-positioning-acceptance) | AC-1/5：snapshot/selected alternate 优先；location_derived 或 missing；不从位置推断关系 | [设计条款](#position-and-relation-projection) | Rust host projection / Viewer | 位置存在不证明关系或世界真值；未声明全部矩阵已跑 |
| Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；[专业条款](viewer-pixel-world-semantic-positioning.prd.md#semantic-positioning-acceptance) | AC-1/5：稳定 hash/offset、既有 radius fallback 与 clamp，属于表现坐标 | [设计条款](#derivation-algorithm) | Rust host projection / QA | 显示半径不证明分配范围；不声称既有单文件已覆盖所有断言 |
| Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3；[专业条款](viewer-pixel-world-semantic-positioning.prd.md#semantic-positioning-acceptance) | AC-2：supported kind、active、runtime_projection/current、双端位置；失效清线不撤销合法位置 | [设计条款](#relation-authority-boundary) | Runtime explicit relation / Rust allowlist / Viewer | 不证明旧响应竞态、提交前确认或高后果阻断全链 |
| Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；[专业条款](viewer-pixel-world-semantic-positioning.prd.md#semantic-positioning-acceptance) | AC-3/4/5：provenance badge、world-coordinate DOM；renderer 失败诚实 unavailable | [设计条款](#ui-surface) | Viewer / Rust bridge / QA | mock/软件 raster 不代签 GPU 或完整 P1-B；本次未执行 |

## Current State
- `pixel_world_host.jsx` passes bounded snapshot inputs to the Rust host projection and consumes `world_bounds`, `locations`, `agents`, `links`, and `visual_hotspots`.
- `pixel_world_bridge` consumes those DTO fields and renders Bevy sprites, links, hotspots, selection, pan and zoom.
- Sparse snapshots may have `location_id` without `pos`. Deterministic presentation placement addresses visibility; relationship truth remains a separate input.

## Design Decision
- Keep the semantic repair in Viewer host DTO construction.
- Do not add runtime fields or change wasm bridge API.
- Treat derived coordinates as presentation data with explicit provenance.
- Consume the existing [P1-B relation boundary](viewer-gameplay-release-experience-overhaul.design.md#p1-b-world-semantic-presentation当前切片源码与-headed-验收已完成): never derive assignment, ownership, route or trust from `location_id` or geometry alone.

<a id="position-and-relation-projection"></a>
## Data Flow

Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；局部承接[REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)：精确/派生/missing 的来源诚实与位置优先级；完整多尺度与探索仍在专业上游范围。

1. Build locations from snapshot positions.
2. Build `locationById`.
3. Resolve each agent position:
   - `snapshot` if `agent.pos` exists.
   - `snapshot` if selected-object alternate position exists.
   - `location_derived` if `location_id` resolves to a positioned location.
   - `missing` otherwise.
4. Independently validate the existing explicit `relation` / `assignment` envelope. Resolve a supported kind, valid status, `source_class=runtime_projection`, `freshness=current` and both positioned endpoints before building a link. Missing or invalid authority yields no link even when both positions exist.
5. Render via wasm/Rust bridge; report unavailable if the bridge cannot derive render state.

## Derivation Algorithm

Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；局部承接[REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)：以下确定性偏移只用于抽象/派生显示位置，不声明精确世界坐标或分配权。

- Input: `agent.id`, `agent.location_id`, `location.pos`, `location.radius_cm`, `world_bounds`.
- Hash key: `${agent.id}:${agent.location_id}`.
- Angle: `hash % 360`.
- Radius: `max(10_000cm, min(1.5% of the larger world dimension, radius_hint))`, where `radius_hint` is a positive location radius or the existing `35_000cm` fallback. This is display spacing, not a claim that the point lies within a runtime assignment radius.
- Clamp final point inside `world_bounds`.

## UI Surface

Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；局部承接[REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)：来源 badge 与 world-coordinate DOM 的局部呈现；unavailable 保持独立，不能以 mock 冒充 GPU 通过。

- Toolbar badge: `derived_positions=<count>`.
- Agent DTO: `position_source`.
- Agent status badges: `position=location_derived` only for derived positions.
- Rendered DOM: positions entities using world-coordinate percentages when possible.
- Relationship labels and lines retain their own source/status/freshness; a derived-position badge is never relationship evidence. Reconcile removes an invalidated old relation independently of the Agent's still-valid display point.

## Verification
- Position tests prove a sparse snapshot produces:
  - `agent.position_source === "location_derived"`
  - stable repeated derived coordinates
  - no relationship link from `location_id` alone
- Relation tests add a supported explicit current envelope and assert a stable link, then remove or invalidate kind/status/source/freshness and assert no link. Explicit position priority, missing location, radius fallback/clamp and world-coordinate DOM placement remain separate assertions.
- Existing unavailable-state test continues proving wasm runtime failure produces an explicit diagnostic state.

## Implementation and validation boundary

The existing Rust host projection is owned by `crates/pixel_world_bridge/src/host_state.rs` and `host_relation_projection.rs`; the JS host passes its bounded inputs and consumes the DTO. At source baseline `5e930face77dd284bbba6186fd997e7e63fa9164`, `host_state_tests.rs` already asserts derived position with zero links and the explicit-current relation positive/negative cases. This document correction retains those semantics; it does not add an alternate JS relation renderer or claim a new browser run.

For an implementation change, run the affected `pixel_world_bridge` host-state tests and `pixel_world_host` UI tests, then the canonical Viewer build and applicable S6 scene. Record the actual candidate and results separately; unavailable-path or position tests do not prove the complete P1-B visual matrix.

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；[专业条款](viewer-pixel-world-semantic-positioning.prd.md#semantic-positioning-acceptance) | [设计条款](#position-and-relation-projection) | AC-1/5：snapshot/selected alternate 优先；location_derived 或 missing；不从位置推断关系 | [测试手册](../../../testing-manual.md) Viewer host/bridge 分层入口； `../../../crates/pixel_world_bridge/src/host_state_tests.rs` 的 sparse-position/zero-link 断言；同候选分别验证 explicit 优先、missing；保留独立位置与关系输入 | Rust host-state 与 UI 分层结果，本次仅源码定义核对 | 位置存在不证明关系或世界真值；未声明全部矩阵已跑 |
| Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；[专业条款](viewer-pixel-world-semantic-positioning.prd.md#semantic-positioning-acceptance) | [设计条款](#derivation-algorithm) | AC-1/5：稳定 hash/offset、既有 radius fallback 与 clamp，属于表现坐标 | [测试手册](../../../testing-manual.md) Viewer bridge 场景；同候选核对 `host_state.rs` derive_agent_position，分别断言重复稳定、半径 fallback/边界 clamp | 定向 Rust case/input/output；本次未执行 | 显示半径不证明分配范围；不声称既有单文件已覆盖所有断言 |
| Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3；[专业条款](viewer-pixel-world-semantic-positioning.prd.md#semantic-positioning-acceptance) | [设计条款](#relation-authority-boundary) | AC-2：supported kind、active、runtime_projection/current、双端位置；失效清线不撤销合法位置 | `../../../crates/pixel_world_bridge/src/host_state_tests.rs` 的 assignment 与 generic logistics 正例及 unknown/stale/local_pending/queued 负例；同候选再核撤销关系/保留位置，按 [S6 手册](../../testing/manual/web-ui-agent-browser-closure-manual.manual.md) 检查真实关系呈现 | 每一 authority 字段和撤销场景的输入、输出与真实视觉证据 | 不证明旧响应竞态、提交前确认或高后果阻断全链 |
| Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；[专业条款](viewer-pixel-world-semantic-positioning.prd.md#semantic-positioning-acceptance) | [设计条款](#ui-surface) | AC-3/4/5：provenance badge、world-coordinate DOM；renderer 失败诚实 unavailable | `../../../crates/oasis7_viewer/software_safe_src/pixel_world_host.test.jsx` 的位置/不可用断言和 `../../../crates/pixel_world_bridge/src/host_state_tests.rs` 分层验证；[S6 手册](../../testing/manual/web-ui-agent-browser-closure-manual.manual.md) 同候选核 badge、DOM 与 GPU 可用/不可用分支 | mock host、Rust、真实 headed 结果各自记录 | mock/软件 raster 不代签 GPU 或完整 P1-B；本次未执行 |

<a id="relation-authority-boundary"></a>

## 关系权威边界

Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3；局部承接[REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)：仅当既有受支持 kind、active、runtime_projection、current 与双端位置均有效才绘线；位置、location_id 或空间邻近不补关系。

沿用 `host_relation_projection.rs` 的既有 allowlist，包含 agent_assignment 以及 route/logistics/resource 等受支持 kind，不收窄成只有 assignment。关系被撤销或字段失效时清除旧线，不需撤销仍合法的显示位置。此条只是来源/时效诚实子义务，不代表刷新竞态、提交前再确认或权限全链已完成。

## 目标、范围与风险

稀疏快照保持可解释且确定的表现位置；world truth 与关系 authority 均由原 owner 持有。不增加 runtime 字段、wasm API、关系规则或第二 JS renderer。派生位置可能被误读为真坐标，故保留来源 badge；同地点重叠仍是既有表现风险，不在本次新增避碰算法。

2026-10-15 文档核对 main `b57e30eaaa0f3eb48c273eb62363d2ac2ada8981` 的 Rust/source 与测试定义；旧 2026-10-12 基线、历史 P1-B headed 结果及本次静态核对各自独立，没有新行为或浏览器通过声明。

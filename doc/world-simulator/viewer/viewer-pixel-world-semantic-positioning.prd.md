# Viewer Pixel World Semantic Positioning

- 对应设计文档: `doc/world-simulator/viewer/viewer-pixel-world-semantic-positioning.design.md`
- 历史任务追溯: `task_4ade083740bc4d9f9f9bb742a7ce153f` / GitHub issue #1011；执行证据见 GitHub task issue evidence comments 与 [固定版本归档](https://github.com/nil-studio/oasis7/blob/4d8c742a9f5eca75c02a4156678d71ccc7e87d53/.pm/github-project-sync/task-archive.jsonl)。
- 关联主专题:
  - `doc/world-simulator/viewer/viewer-visual-design-spec-2026-06-05.design.md`
  - `doc/world-simulator/viewer/viewer-pixel-world-bridge-rendering.prd.md`
- 产品层长期承诺：[`doc/product/agents-world-simulation/player-readable-world-stage.prd.md`](../../product/agents-world-simulation/player-readable-world-stage.prd.md)；本文继续拥有位置来源、派生规则、DTO 与验证合同。

审计轮次: 1

## 1. Executive Summary
- 当前 pixel-world 已具备 wasm-only renderer、关系线、热点层和增量渲染，但实际 runtime 快照经常只提供 `agent.location_id`，不提供 agent 精确坐标。
- 当 agent 缺少坐标时，Viewer 可用 `location_id` 派生一个可解释的显示位置；位置可用不等于关系已获权威确认。
- 本轮不改 runtime 协议，通过 Viewer host DTO 增加确定性语义定位：有精确坐标时使用 snapshot truth；缺坐标但有 `location_id` 时派生到地点附近，并标记 `position_source=location_derived`。
- 关系线另行消费完整、当前、显式发布的关系 envelope；`location_id`、派生坐标和空间邻近不能独立生成关系。这一边界与[发行体验 P1-B](viewer-gameplay-release-experience-overhaul.prd.md#p1-b-world-semantic-presentation当前切片已实现)一致。

## 2. User Experience & Functionality

### 2.1 目标
- 让稀疏快照下的 pixel-world 稳定显示 agent 与地点；关系线只在其独立权威条件满足时显示。
- 把派生位置作为 Viewer 表达层语义，不伪装成 runtime 真坐标。
- 保持 `pixel_world_bridge` wasm-only 宿主合同、event contract、build/finalize 产物边界不变。

### 2.2 范围
- 范围内：
  - `crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx`
  - `crates/oasis7_viewer/software_safe_src/pixel_world_host.test.jsx`
  - pixel-world semantic positioning 的 PRD / design / project / index writeback
- 范围外：
  - 新增 runtime snapshot 字段
  - 修改 `pixel_world_bridge` wasm bindgen API
  - 新增第二套 JS 世界渲染或改 3D Viewer

### 2.3 接口 / 数据
- Viewer DTO:
  - `agents[].pos`
  - `agents[].position_source`
  - `agents[].status_badges`
  - `links[]`
- Runtime snapshot inputs:
  - `agent.pos`
  - `agent.location_id`
  - `location.pos`
  - `location.profile.radius_cm`
  - `snapshot.config.space`
- Relation projection inputs：既有 agent `relation` / `assignment` 的 kind、status、source_class、freshness；它们不是从 `location_id` 生成的字段。
- Renderer inputs保持不变:
  - `world_bounds`
  - `locations`
  - `agents`
  - `links`
  - `visual_hotspots`
  - `selection`

## 3. User Stories
- As a player, I want agents without exact coordinates to appear near their reported location with explicit position provenance, while unconfirmed relationships remain absent.
- As a `viewer_engineer`, I want derived positions to be deterministic and labelled, so that tests can lock the rendering DTO without claiming runtime has exact agent coordinates.

## 4. Technical Specifications

### 4.1 Position Source Priority
- `snapshot`: use `agent.pos` or selected-object position when present.
- `location_derived`: if `agent.location_id` resolves to a location with position, derive a deterministic offset near that location.
- `missing`: keep `pos=null`; renderer may still use an internal non-authoritative placement point.

### 4.2 Deterministic Derivation
- The derivation key is `agent.id:agent.location_id`.
- The offset angle comes from a stable string hash.
- The offset radius is bounded by world size and location radius, then clamped into `world_bounds`.
- The DTO exposes `position_source` and adds `position=location_derived` to `status_badges`.

### 4.3 Rendering Contract
- `links` 在位置解析后、独立关系校验通过时构建。只有现有支持的 relation kind、有效 status、`source_class=runtime_projection`、`freshness=current` 和可解析的两端位置同时满足，才生成对应线；当前允许集以既有 P1-B/bridge 合同为准。
- 仅有 `location_id` 的稀疏快照仍可得到 `location_derived` 位置，但 `links=[]`。缺失、陈旧、未知或 `local_pending` 关系不得以默认 assignment、距离或 label 补齐；后续快照撤销关系时清除旧线而保留仍合法的显示位置。
- Rendered DOM positions use world coordinates when available, instead of index-only placement.
- The wasm renderer continues consuming the existing `agents[].pos`, `links[]`, `visual_hotspots[]`, and `selection` DTO fields.

## 5. Risks & Roadmap
- Risk: derived positions may be mistaken for authoritative runtime coordinates.
  - Mitigation: expose `position_source` and badge text, and keep runtime protocol unchanged.
- Risk: multiple agents at one location can overlap.
  - Mitigation: deterministic hash distributes nearby agents around the location; future slices can add collision relaxation if needed.

## 里程碑
- M1: PRD / design / project 建模完成，冻结 sparse snapshot semantic positioning 合同。
- M2: Rust bridge render-state 完成 `location_derived` 派生坐标、source badge 与 rendered DOM world-coordinate placement。
- M3: Targeted UI tests、software-safe build、doc governance 与 diff hygiene 通过。

## 风险
- 派生坐标被误读为 runtime 权威位置。
- 同一地点多 agent 仍可能在视觉上靠得过近。
- 若后续上游补精确坐标，需要保持 `snapshot` source 优先级，避免派生逻辑覆盖真坐标。

<a id="semantic-positioning-acceptance"></a>
## 6. Acceptance Criteria

Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；局部承接[REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003)：AC-1/3/5 的位置来源、精确优先、derived/missing 与 DOM 表达是抽象/派生空间表达的有限子义务，不代表多尺度探索已实现。

Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3；局部承接[REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002)：AC-2 的显式 current relation authority 与撤销清线仅承接来源/时效诚实、失效不造关系；不代签旧响应竞态、提交前权威再确认或高后果阻断全链。

AC-4 保持独立 unavailable 验证要求；以下 AC 原文与历史证据不变。
- AC-1: An agent with `location_id` but no `pos` gets a deterministic `location_derived` position in the Viewer DTO.
- AC-2: A sparse snapshot with only `location_id` produces no relationship link. Adding a complete current authoritative relation produces a stable link; removing or invalidating any required authority field removes it without changing the deterministic position rule.
- AC-3: Rendered DOM uses world-coordinate placement when DTO positions are available.
- AC-4: Tests cover sparse snapshot semantic positioning and explicit renderer unavailable behavior.
- AC-5: 精确 snapshot 位置优先于派生位置；无可解析地点时为 missing。派生坐标、来源 badge、DOM 位置与 relationship authority 分别断言，位置回归不得通过恢复自动推断关系来修复。

2026-10-12 基准 `5e930face77dd284bbba6186fd997e7e63fa9164` 的 `host_state_tests.rs` 已分别包含“稀疏位置且零关系线”和显式 current relation 正负例。本次同步旧专题合同，不新增 runtime/DTO 行为，不声明这些测试在本次已执行或完整视觉已验收。

## 7. Validation & Decision Record
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-WORLD_SIMULATOR-046 | `task_4ade083740bc4d9f9f9bb742a7ce153f` | `test_tier_required` | `npm --prefix crates/oasis7_viewer run test:ui -- pixel_world_host` + `npm --prefix crates/oasis7_viewer run build:software-safe` + `./scripts/doc-governance-check.sh` + `git diff --check` | Pixel-world Rust DTO, sparse snapshot placement, rendered DOM positioning, generated viewer bundle |

# oasis7: Minimal System Run (Viewer Demo)

- 对应设计文档: `doc/world-simulator/viewer/viewer-minimal-system.design.md`
- 历史 demo 实施与验证记录：GitHub task issue evidence。

审计轮次: 5

## 1. Executive Summary
- Provide a deterministic, low-friction way to generate `snapshot.json` + `journal.json` for the viewer.
- Enable a minimal end-to-end run: generate demo data -> start viewer server -> open UI client.
- Keep demo data small, with at least a few events for the event list.

## 2. User Experience & Functionality

### In Scope
- A new CLI binary `oasis7_viewer_demo` in `crates/oasis7`.
- Deterministic demo script that produces at least one event.
- Output directory containing `snapshot.json` and `journal.json` for `oasis7_viewer_server`.
- Simple CLI flags: scenario selection and output directory.

### Out of Scope
- Live simulation server or streaming kernel loop.
- Complex scripting language for demo actions.
- UI/Bevy feature expansion.

## 3. AI System Requirements (If Applicable)
- N/A: 本专题不新增 AI 专属要求。

## 4. Technical Specifications

### CLI (draft)
- `oasis7_viewer_demo [scenario] [--out <dir>]`
- Defaults:
  - `scenario = twin_region_bootstrap`
  - `out = .data/world_viewer_data`

### Output
- `snapshot.json`: `WorldSnapshot` containing the kernel’s current logical time and model after demo actions have been processed; it is not a time-0 reset.
- `journal.json`: `WorldJournal` (events from demo actions).

### Demo Script Strategy
- Select a deterministic agent (lowest id) and its current location.
- If a second location exists:
  - Transfer enough electricity from the current location to cover move cost.
  - Move the agent to the other location.
- If only one location exists:
  - Transfer a small amount of electricity from location to agent (may be rejected if empty).
- The goal is to guarantee at least one event in the journal.

## 5. Risks & Roadmap
- **M1**: Implement demo data generator + CLI (`oasis7_viewer_demo`).
- **M2**: Add tests for demo action planning and persistence.
- **M3**: Document the end-to-end run steps.

### Technical Risks
- Demo action may be rejected if resources are insufficient (still produces an event).
- UI build may fail in offline environments due to Bevy dependencies.
- Users may expect live data; this is offline replay only.

## 6. Validation & Decision Record
- 追溯: 历史 demo 实施与验证记录由 GitHub task issue evidence 维护。

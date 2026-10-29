# oasis7: 测试覆盖与 CI 扩展

- 对应设计文档: [CI 分级执行共享设计](ci-tiered-execution.design.md#ci-coverage-design)；原同名 design 仅保留兼容入口。
- 可变任务状态与历史: PR、实际 CI 与评审记录（Issue 按需）

审计轮次: 4


## ROUND-002 口径归属（2026-03-05）
- `testing-manual.md` 是当前 `required` / `full` 命令和测试层级矩阵的执行入口；本 PRD 记录覆盖目标与设计决策，不替代手册的权威命令。
- 其它文档（含本 PRD 与 `pre-commit.prd.md`）引用 [`testing-manual.md`](../../../testing-manual.md)，不维护第二份当前命令清单。

## 1. Executive Summary
- Problem Statement: 关键链路（viewer 离线回放、wasmtime、在线联测）若未在 CI 中显式覆盖，容易在特性分支阶段漏检回归。
- Proposed Solution: 由 [`testing-manual.md`](../../../testing-manual.md) 维护当前 required/full 执行矩阵，覆盖 viewer 离线回放与 wasmtime 路径。`egui_kittest` 无 `wgpu` snapshot 降级保留为历史/目标要求；在可运行实现和对应 gate 证据链接前，不计入当前自动化覆盖。
- Success Criteria:
  - SC-1: CI 明确执行 viewer 离线回放联测与 wasmtime 特性测试。
  - SC-2: required/full 清单在 `scripts/ci-tests.sh` 保持统一管理。
  - SC-3: `egui_kittest` 无 `wgpu` snapshot 降级目前是历史/目标口径；只有找到可运行的 snapshot 实现并链接对应 CI/gate 证据后，才可把它列为当前覆盖。
  - SC-4: 文档、任务、验证结果可追溯到同一专题闭环。

<a id="coverage-requirements"></a>
## 2. User Experience & Functionality
- User Personas:
  - CI 维护者：需要可控且可复现的覆盖清单。
  - 开发者：需要稳定的门禁反馈，不被环境抖动误伤。
  - 发布负责人：需要确认发布前已充分验证关键链路。
- User Scenarios & Frequency:
  - PR 门禁：默认执行 required，验证核心路径。
  - 每日全量：执行 full，覆盖重型特性与联测。
  - 历史/目标场景：若未来将 `egui_kittest` snapshot 接入无 `wgpu` runner，降级路径应输出可诊断 skip 原因；当前不作为已接线 CI 的事实描述。
- User Stories:
  - PRD-TESTING-CI-COVER-001: As a CI 维护者, I want key integration paths explicitly covered, so that regressions are visible earlier.
  - PRD-TESTING-CI-COVER-002: As a 开发者, I want viewer offline integration to run headless and stable, so that tests do not depend on external UI resources.
  - PRD-TESTING-CI-COVER-003: As a 发布负责人, I want full-tier feature tests retained, so that release risk is bounded.
- Critical User Flows:
  1. Flow-COVER-001: `生成 snapshot/journal -> 运行 viewer_offline_integration -> 校验协议消息链路`
  2. Flow-COVER-002: `执行 full -> 运行 wasmtime/libp2p/viewer 联测 -> 汇总回归结果`
  3. Flow-COVER-003（历史/目标，未计入当前 CI 覆盖）: `snapshot 测试初始化 wgpu 失败 -> 输出 skip 原因 -> 继续执行其他用例`
- Functional Specification Matrix:
| 功能点 | 字段定义 | 按钮/动作行为 | 状态转换 | 排序/计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 离线回放联测 | snapshot/journal 输入、协议输出断言 | 运行 `viewer_offline_integration` | `prepared -> replaying -> passed/failed` | 优先验证 `HelloAck/Snapshot/Event` | CI 自动执行 |
| full 扩展覆盖 | wasmtime/libp2p/viewer_live 特性组合 | 运行当前 full tier；命令见 [`testing-manual.md`](../../../testing-manual.md) | `queued -> running -> passed/failed` | full 在 required 之上追加 | 发布前必须通过 |
| snapshot 降级保护（历史/目标） | 未来 runnable `egui_kittest` 实现的 wgpu 初始化结果、skip 原因 | 仅在实现与 gate 已链接后，初始化失败时记录并跳过快照测试 | `init -> skip/report` 或 `init -> run` | 当前不构成 required/full 的已验证覆盖 | 待对应测试/gate 接线并留存证据后确认 |
- Acceptance Criteria:
  - AC-1: required/full 覆盖清单在文档和脚本一致。
  - AC-2: 离线回放联测可稳定完成并验证关键消息。
  - AC-3: `wasmtime` 路径纳入 CI 覆盖。
  - AC-4（目标；未满足前不得宣称当前已覆盖）: 可运行的 `egui_kittest` snapshot 在无 `wgpu` runner 下可降级并留痕，且文档关联对应实现与 gate 证据。
- Non-Goals:
  - 不做 CI 缓存优化与并行矩阵调优。
  - 不引入 UI 真实渲染端到端依赖。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用（本任务为测试覆盖治理）。
- Evaluation Strategy: 不适用。

## 4. Technical Specifications
- Architecture Overview: 通过统一测试脚本接入 required/full 覆盖项，把联测输入输出和环境降级策略集中到同一门禁链路。
- Integration Points:
  - `scripts/ci-tests.sh`
  - `.github/workflows/rust.yml`
  - `oasis7_viewer_demo`（`snapshot.json` / `journal.json`）
  - `viewer_offline_integration`
  - `oasis7_viewer` `egui_kittest` snapshot（历史/目标；在关联可运行实现和 gate 证据前不视为当前 CI 覆盖）
- Edge Cases & Error Handling:
  - wasmtime 依赖体积增长：执行耗时上升，需要保留分级策略平衡成本。
  - socket 端口竞争：联测必须使用随机端口与超时保护。
  - 无 `wgpu` 环境：作为目标，未来的可运行 snapshot 测试应输出 skip 原因；当前未证明该路径已接入 gate。
  - skip 覆盖盲区：在有图形能力环境保留一次完整视觉快照校验，须在对应实现与 gate 接线后安排。
- Non-Functional Requirements:
  - NFR-COVER-1: required 门禁持续稳定并保持较低执行开销。
  - NFR-COVER-2: full 门禁覆盖 wasmtime/libp2p/viewer 核心路径。
  - NFR-COVER-3: 降级行为可审计（有明确日志）。
- Security & Privacy: 测试数据来自本地回放产物，不涉及新增敏感信息采集。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP (T1): 新增离线回放联测并保证稳定退出。
  - v1.1 (T2): CI 增加 wasmtime 特性测试步骤。
  - v2.0 (T3/T4): 文档回写、任务日志收口、snapshot 无 `wgpu` 降级。
- Technical Risks:
  - 风险-1: wasmtime 路径增加执行时长与依赖负担。
  - 风险-2: 联测端口冲突影响稳定性。
  - 风险-3: 若未来接入 snapshot 且无 `wgpu` 环境触发 skip，该环境的视觉覆盖会下降；这是目标接线后的残余风险，不是当前 CI 行为结论。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-TESTING-CI-COVER-001 | T1/T2/T5/T6 | 按 [`testing-manual.md`](../../../testing-manual.md) 当前矩阵 | 对照 required/full 命令与 `scripts/ci-tests.sh` 实际接线 | CI 基础覆盖能力 |
| PRD-TESTING-CI-COVER-002 | T1/T3 | `test_tier_required` | `viewer_offline_integration` 中 required-feature 用例及 required-tier 结果 | viewer 协议链路稳定性 |
| PRD-TESTING-CI-COVER-003 | T2/T4/T8/T9 | `test_tier_full` + `wasmtime` | full-tier wasmtime 用例；`egui_kittest` snapshot 降级仅在 runnable implementation 与 gate 证据链接后验证 | 发布前深度回归保障 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-COVER-001 | 统一脚本维护 required/full 覆盖 | 分散到多脚本维护 | 集中维护可减少漂移。 |
| DEC-COVER-002 | 离线回放使用 headless 协议联测 | 依赖真实 UI 渲染联测 | headless 更稳定且更适合 CI。 |
| DEC-COVER-003 | runnable snapshot 接线后的目标是无 `wgpu` 环境可诊断 skip；当前标注为历史/目标 | 未经实现和 gate 证据便声称已自动 skip | 在可复现实现与当前 CI 证据出现前，保留覆盖边界。 |

## 原文约束点映射（内容保真）
- 原“目标（覆盖补齐 + CI 显式执行 + snapshot 降级）” -> 第 1 章与第 2 章 AC。
- 原“In/Out of Scope” -> 第 2 章 AC 与 Non-Goals。
- 原“接口/数据（输入/输出/CI 清单）” -> 第 4 章 Integration Points 与功能矩阵。
- 原“里程碑 T1~T4” -> 第 5 章 Phased Rollout。
- 原“风险（耗时、端口竞争、skip 覆盖）” -> 第 4 章 Edge Cases + 第 5 章 Risks。

# Viewer Frontend Structure Standard

- 对应设计文档: `doc/world-simulator/viewer/viewer-frontend-structure-standard.design.md`
- 结构债务 task 与验证记录：GitHub task issue evidence。
- 关联主专题:
  - `doc/world-simulator/viewer/viewer-web-build-truth.prd.md`
  - `doc/world-simulator/viewer/viewer-page-module-design-2026-06-18.design.md`

审计轮次: 1

## 1. 摘要
- Viewer Web 已有局部拆分真值：`viewer-web-build-truth` 要求 `legacy_core.js` 退化为 facade，并把主要实现下沉到 `software_safe_src/` 子模块。
- 当前缺口是没有一份面向 Viewer 前端 `js/html/jsx` 的通用结构标准。结果是 `legacy_core.js`、`main.jsx`、`pixel_world_host.jsx` 仍可能按专题继续膨胀，评审时缺少统一的拆分触发条件和目标边界。
- 本标准把外部方法收敛为 oasis7 可执行规则：Feature-Sliced-inspired 分层、Solid 组件边界、Google HTML/CSS 基础卫生、ESLint/Prettier gate，以及 Viewer generated artifact 单一真值。

## 目标
- 定义 Viewer Web `js/html/jsx` 文件的结构边界、拆分触发条件和完成态。
- 让 `software_safe_src/` 的模块拆分可评审、可测试、可继续迭代，而不是一次性重排。
- 固定 generated artifact、canonical source、compat alias 的区别。
- 给 PR review、role review 和后续 task 创建提供一致的判断依据。

## 范围
- 范围内：
  - `crates/oasis7_viewer/viewer.html`
  - `crates/oasis7_viewer/dist/software_safe.html`（generated compat output）
  - `crates/oasis7_viewer/software_safe_src/**`
  - `crates/oasis7_viewer/viewer.js`
  - `crates/oasis7_viewer/dist/software_safe.js`（generated compat output）
  - `crates/oasis7_viewer/viewer_first_agent_claim_evidence.html` / `dist/software_safe_first_agent_claim_evidence.html`
  - `crates/oasis7_viewer/dist/pixel-world-bridge/**`
  - Viewer Web tests, build/finalize scripts, and browser/manual evidence that consume those files
- 范围外：
  - 不把整个仓库强行迁移到某个外部目录框架。
  - 不在本标准内重写 UI 视觉方向、信息架构或玩家交互规格；这些仍由 `viewer-visual-design-spec`、`viewer-page-module-design` 和具体 PRD 承担。
  - 不要求一次性拆完所有现有大文件；本标准定义触发条件、目标边界和验证要求。
  - 不改变 Viewer runtime protocol、WASM ABI、server line protocol、hosted auth semantics 或 public URL taxonomy。

## 问题陈述
- Viewer 前端既包含手写源码，也包含 checked-in generated artifacts 和 compat aliases；若边界不清，维护者容易在 `viewer.js` / `software_safe.js` / `viewer.html` / `software_safe.html` 之间建立第二套真值。HTML compat alias 只在 dist 中生成，不作为 checkout source。
- `main.jsx` 与 `pixel_world_host.jsx` 承担 UI composition、state projection、copy、diagnostics、event handlers 与 fixture/test hooks，缺少“何时抽组件、何时抽 service/state module”的标准。
- Rust 侧已有体量与真实职责拆分治理；Viewer JS/HTML/JSX 侧需要较轻但明确的对应规则。

<a name="接口-数据"></a>

## 接口 / 数据

| 类型 | 示例 | 规则 |
| --- | --- | --- |
| Source HTML | `crates/oasis7_viewer/viewer.html` | 手写 canonical HTML shell；只承载 document shell、stable anchors、bundle reference 和 minimal static metadata。 |
| Compat HTML | `crates/oasis7_viewer/dist/software_safe.html` | 由 canonical `viewer.html` 生成的发布兼容副本；不得成为独立设计或 bundle 真值。 |
| Source JS/JSX | `crates/oasis7_viewer/software_safe_src/**` | 手写 Viewer Web source truth；组件、state projection、services、fixtures 和 tests 必须在这里形成清晰边界。 |
| Generated bundle | `crates/oasis7_viewer/viewer.js` | canonical generated bundle；由 build/finalize flow 写入，不手改。 |
| Compat bundle | `crates/oasis7_viewer/dist/software_safe.js` | generated compat alias；必须显式指向 canonical bundle，不能承载第二份 bundle logic。 |
| Compat evidence page | `crates/oasis7_viewer/dist/software_safe_first_agent_claim_evidence.html` | 由 canonical claim fixture 生成的 served compatibility route；scenario data stays in the canonical fixture. |
| Generated runtime | `crates/oasis7_viewer/dist/pixel-world-bridge/**` | finalize flow 管理的 runtime artifact；不是手写 source module。 |

## 结构分层

Viewer Web source uses a lightweight Feature-Sliced-inspired model. The names below are governance categories, not mandatory folder names until a task needs a new directory.

| 分层 | 拥有职责 | 不应拥有 |
| --- | --- | --- |
| `app` | bootstrapping, mount, top-level route/query wiring, global providers, test API exposure | large domain transforms, detailed component bodies, transport protocol implementation |
| `pages` | page-level composition for the canonical Viewer entry | reusable widgets, state mutation services, generated artifact handling |
| `widgets` | self-contained screen regions such as World, Targets, Command, Diagnostics, Agent Chat | global state ownership, WebSocket send/reconnect loops |
| `features` | user-visible actions such as command submit, hosted login, prompt override, locale selection | unrelated rendering, bundle finalization, world derivation shared by multiple features |
| `entities` | domain display models for agents, locations, resources, events, player session, runtime health | UI layout policy and command side effects |
| `shared` | constants, pure helpers, formatting, storage adapters, test fixtures, browser capability helpers | product-specific flows or cross-layer imports back into widgets/pages |

Import direction should move from higher composition to lower reusable layers. Lower layers must not import from higher layers. A module may expose a small public API file when direct file imports start leaking internal layout.

## 拆分触发条件

A touched Viewer frontend file should be split or receive an explicit exemption when any trigger applies:

- Production source file grows above 1200 lines after the change.
- Test source file grows above 1600 lines after the change.
- A JS/JSX file owns more than one layer from `结构分层` in a way that makes review or tests cross unrelated domains.
- A component body mixes view markup, transport side effects, domain derivation, and local storage/persistence in one place.
- New code expands `legacy_core.js` beyond facade/export assembly unless the task is explicitly retiring legacy internals.
- New generated artifact logic appears outside the existing finalize/build freshness chain.
- A PR touches both canonical and compat artifacts but cannot explain canonical -> compat direction.

Existing files above the soft threshold are not automatically blockers for unrelated changes. If touched for behavior work, the PR must either shrink the file, extract a coherent boundary, or record a debt exemption in the task evidence with owner, reason, and next trigger.

### 当前已登记的结构债务

- `legacy_core.js` 仍包含 control、semantic-command、DOM-rendering 与 bootstrap assembly；后续切片应优先抽出一个 coherent boundary，保留 facade、记录前后行数并运行 scoped UI/build/feedback checks。
- 现有阈值豁免的实时文件清单与 cap 以 `crates/oasis7_viewer/scripts/frontend-structure-audit.mjs` 的 `knownLineDebt` registry 为准，由 `test:frontend-structure` 守护；不再以历史 `main.jsx` 等文件枚举代替当前 owning module。生产 1200 / 测试 1600 行软阈值及上述拆分触发条件不变；新增体量或失效豁免必须在 PR 与实际验证记录说明 owner、reason 和 nextTrigger，registry 引用不表示债务清零。
- `dist/software_safe.js` 仍是 `viewer.js` 的生成 compat alias；dist 中的 `software_safe.html` 必须保持 `viewer.html` 的 byte-for-byte generated copy；compat evidence page 从 canonical fixture 生成。它们不能承接修复或债务绕行。

## 可接受拆分模式

Preferred patterns:
- Component extraction: move a repeated or large visual region into a named Solid component with props limited to display inputs and callbacks.
- State/service extraction: move browser storage, hosted auth, command transport, runtime loading, or metrics into `*_module.js` or a named service module.
- Display-model extraction: move raw snapshot/event interpretation into pure helpers that can be unit tested without DOM.
- Fixture/test helper extraction: move large fixtures and query helpers out of broad UI tests when they obscure the behavior under test.
- Facade preservation: keep a stable import path only when it assembles exports and delegates to real modules.

Rejected patterns:
- Mechanical `part1` / `part2` / `misc` files as a final state.
- Copying state shapes or fixture payloads into parallel modules to avoid imports.
- Creating a second source of truth for canonical `viewer.js`, `viewer.html`, the claim fixture, generated compatibility aliases, or runtime files.
- Moving JSX into string templates when Solid components would keep structure and test selectors clearer.

<a name="html-jsx-js-职责"></a>

## HTML / JSX / JS 职责

- HTML files define document shell, root mount points, preload/meta policy, and canonical bundle references.
- JSX files define Solid components, screen composition, UI state projection, and event callback wiring.
- JS modules define pure helpers, constants, state/service modules, transport/runtime loaders, storage adapters, and generated-runtime integration.
- CSS may remain in the canonical HTML shell only while the task is not introducing a reusable style system. Large repeated visual rules should move only under a documented Viewer style/token task.
- Stable test selectors and DOM anchors are part of the public test surface; removing or renaming them requires test and manual update evidence.

## 文件命名

- Component files use domain nouns and may end in `.jsx`: `pixel_world_host.jsx`, `agent_chat_panel.jsx`.
- Service/state modules use precise `*_module.js`, `*_state.js`, `*_loader.js`, or `*_crypto.js` names.
- Tests sit next to the source when practical: `foo.js` with `foo.test.js`, `foo.jsx` with `foo.test.jsx`.
- Generated files keep existing canonical names and should include generated/compat comments when the file format allows it.

## 评审清单

Every Viewer frontend PR should answer:
- Which layer from `结构分层` owns the change?
- Does any touched source file cross a split trigger?
- If a large file remains large, is the exemption task-scoped and owner-tagged?
- Are generated artifacts updated only through the canonical build/finalize flow?
- Are compat files still aliases/copies rather than behavior owners?
- Did tests cover the extracted boundary rather than only the old broad entry?
- If visible UI changed, is S6/browser/screenshot evidence present or explicitly exempted?

## 验证矩阵

| 变更类型 | 必需本地验证 |
| --- | --- |
| Pure docs standard update | `./scripts/doc-governance-check.sh` and `git diff --check` |
| JS/JSX source structure only | `npm --prefix crates/oasis7_viewer run test:ui`, targeted tests for extracted modules, `git diff --check` |
| Bundle/finalize/generated artifact flow | `npm --prefix crates/oasis7_viewer run build:software-safe`, `npm --prefix crates/oasis7_viewer run test:feedback-contract`, freshness helper if touched |
| Visible Viewer UI behavior | Targeted `test:ui`, build when bundle output changes, S6/agent-browser or documented visual-evidence exemption |
| Runtime/WASM bridge integration | Relevant `pixel_world_runtime_*` tests plus build/finalize evidence |

## 里程碑

- M1: Treat this standard as the review baseline for new Viewer frontend changes.
- M2: Do not mass-rewrite existing files solely to satisfy the threshold.
- M3: When touching `legacy_core.js`, `main.jsx`, or `pixel_world_host.jsx`, prefer one coherent extraction per task and record before/after line counts in task evidence.
- M4: Promote repeated exemptions into a GitHub-backed debt task.
- M5: If a future repo-wide frontend standard is created, this Viewer standard becomes the Viewer-specific profile for that broader rule.

## 风险
- Risk-1: If thresholds are treated as a mass rewrite mandate, the standard can create broad review churn without reducing real coupling.
- Risk-2: If generated bundles are reviewed like source files, reviewers may miss the true handwritten boundary in `software_safe_src/**`.
- Risk-3: If exemptions are not owner-tagged, large files can keep growing under the appearance of governance.
- Risk-4: If future tooling gates are added without a migration plan, CI could fail on historical debt instead of current-task regressions.

## 验收标准
- AC-1: Viewer frontend source taxonomy distinguishes hand-written source, generated artifacts, and compat aliases.
- AC-2: The standard defines layer ownership, import direction, split triggers, accepted split patterns, and rejected pseudo-splits.
- AC-3: The standard preserves the existing `viewer-web-single-source-build-truth` canonical/compat contract.
- AC-4: The standard maps change types to local verification commands.
- AC-5: Viewer README, world-simulator PRD index, and related role cards expose this standard as the first-read entry for JS/HTML/JSX structure questions.

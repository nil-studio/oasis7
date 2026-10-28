# Viewer Brand System Specification (2026-06-05)

- Professional owner: `game_visual_interaction_designer`
- Integration owner: `tpm`
- Source task uid: `task_a25bf76359be45719edfcda1759626d1` / GitHub issue #1385; execution evidence is in GitHub task issue evidence comments and `.pm/github-project-sync/task-archive.jsonl`.
- Parent visual spec: `viewer-visual-design-spec-2026-06-05.design.md`

## 1. Purpose
This document is the executable brand-system companion for Viewer visual work.
It completes the brand book, token taxonomy, icon/status vocabulary, asset
language, and screenshot matrix that the parent visual spec requires.

The system is scoped to Viewer and player-facing world-simulator surfaces. It
does not define public marketing identity, launcher branding, runtime protocol,
or a new art-production pipeline.

## 2. Brand Book
### 2.1 Brand Position
Viewer is an industrial world command table. It should feel precise, playable,
and causally honest.

The brand is:
- world-first;
- command-oriented;
- technically credible;
- calm under failure;
- explicit about player agency.

The brand is not:
- a generic observability dashboard;
- a marketing landing page;
- a sci-fi decoration layer;
- a raw DTO inspector as the first experience;
- a single-color dark admin console.

### 2.2 First Five Seconds
Every Viewer first screen must answer:
1. Which world/stage is this?
2. Which objective, blocker, route, or actor matters?
3. What is the next player move?
4. Is there a player-facing action receipt?
5. Where are diagnostics if needed?

If a screenshot cannot answer these, the brand system has failed even when the
colors are correct.

### 2.3 Terminal shell brand summary
The paired [`viewer-gameplay-release-experience-overhaul.prd.md`](viewer-gameplay-release-experience-overhaul.prd.md)
is the single terminal shell authority. The brand contribution is visual: keep
the world board first, use a quiet dock and on-demand console, keep command and
Action Receipt legible, and demote diagnostics. The implemented World Feed v1 must
remain visually distinct from causality and appear as a compact collapsed top-right
edge overlay/ambient chip until explicitly opened. HUD placement follows the
[shell's current implementation boundary](viewer-gameplay-release-experience-overhaul.prd.md#current-implementation-boundary):
Next Move is the primary region with the shell-defined inline blocker;
Objective / Player Leverage are supporting context. Narrow screens put that primary region first, not below the context.
This brand spec does not retain a parallel three-peer-card layout. Current
focus/right-panel hooks and `#entity-search` are implementation references, not brand modes.

### 2.4 Voice and Labels
Player-facing copy uses action verbs and concrete state. Diagnostics may use
technical labels, but they must be visually demoted and grouped under explicit
diagnostic surfaces.

Preferred label forms:
- `Objective`, not `Goal DTO`.
- `Next Move`, not `Pending Handler`.
- `Action Receipt`, not `Latest Event`.
- `Renderer Unavailable`, not silent degradation.
- `position=location_derived`, not hidden inferred coordinate.

### 2.5 Bilingual terminal copy matrix (target)
The matrix fixes short, action-oriented labels for both locales. It is a visual
copy contract, not proof that the labels are currently shipped.

| Surface/state | English | 简体中文 | Usage rule |
| --- | --- | --- | --- |
| primary rail | `World` / `Targets` / `Command` | `世界` / `目标` / `指令` | keep as three primary routes |
| secondary rail | `More` / `Diagnostics` | `更多` / `诊断` | Diagnostics stays secondary |
| target filter | `Search targets` | `搜索目标` | filters current Targets only; never executes |
| objective | `Objective` | `当前目标` | one concise goal line |
| next action | `Next Move` | `下一步` | route to Command; do not promise execution |
| leverage | `Player Leverage` | `玩家杠杆` | only authoritative projection |
| selection | `Selected` / `Current Target` | `已选中` / `当前目标` | selection recenters/highlights only |
| command surface | `Command` / `Context` | `指令` / `上下文` | command/chat before details |
| quote | `Quote` | `报价` | contextual Command route, not a rail item |
| receipt | `Action Receipt` | `行动回执` | sole player-causality surface |
| no receipt | `No action receipt yet` | `尚无行动回执` | explicit, muted, never omitted |
| pending | `Waiting for world confirmation` | `等待世界确认` | accepted/queued is not completion |
| completed | `Completed` | `已完成` | only with authoritative receipt |
| no progress | `Completed with no progress` | `已完成但没有推进` | keep blocker/recovery visible |
| blocked | `Blocked` / `Reason` / `Next` | `受阻` / `原因` / `下一步` | reason and recovery together |
| world feed | `World Feed` | `世界动态` | ambient context; never replaces receipt |
| feed loading/empty | `Loading world feed` / `No world events yet` | `正在加载世界动态` / `暂无世界事件` | distinct loading and empty states; `world_feed/v1` only |
| feed recovery | `Replay required` / `World feed unavailable` | `需要重放` / `世界动态不可用` | gap/reorg/unavailable copy is explicit |
| director entry | `Open operator diagnostics` | `打开操作员诊断` | server-validated capability-gated, ephemeral secondary action; unavailable is fail-closed |
| director recovery | `Return to Player` / `Diagnostics access unavailable` | `返回玩家模式` / `诊断权限不可用` | fail closed and explain recovery |
| renderer | `Renderer Unavailable` | `渲染器不可用` | visible near affected board |

Copy must wrap before shrinking the board, preserve visible text equivalents for
icons, and avoid raw enum/state identifiers in Player-facing surfaces.

## 3. Token Taxonomy
Tokens are semantic roles. Implementation variables may alias existing CSS
values, but new work must refer to these roles in docs, review notes, and
acceptance criteria.

### 3.1 Color Tokens
| Token | Role | Current Viewer alias |
| --- | --- | --- |
| `--color-world-background` | page/stage base | `--bg` |
| `--color-world-depth` | deepest stage background | `--bg-deep` |
| `--color-panel-surface` | command/context panels | `--panel` |
| `--color-panel-strong` | panel headers and raised tools | `--panel-strong` |
| `--color-stage-surface` | primary world stage | `--panel-stage` |
| `--color-border-subtle` | default low-emphasis border | `--border` |
| `--color-border-stage` | world/stage emphasis | `--border-strong` |
| `--color-text-primary` | primary readable text | `--text` |
| `--color-text-muted` | diagnostic/context detail | `--muted` |
| `--color-agent-primary` | active player-relevant actor | `--accent` |
| `--color-objective-accent` | objective and next move | `--accent-strong` |
| `--color-route-active` | active relationship/path | `--accent` |
| `--color-receipt-success` | completed/applied result | `--good` |
| `--color-blocker-danger` | blocked/error/missing requirement | `--bad` |
| `--color-warning` | recoverable warning/blocker | `--warn` |
| `--color-diagnostic-muted` | QA/runtime metadata | `--muted` |
| `--color-focus-ring` | keyboard/selection focus | `--accent-strong` |

### 3.2 Typography Tokens
| Token | Role | Current implementation |
| --- | --- | --- |
| `--font-viewer-body` | all normal UI text | `--font-body` |
| `--font-viewer-display` | stage/hero identity | `--font-display` |
| `--font-viewer-mono` | JSON and technical values | `--font-mono` |
| `--type-world-title` | true stage title | `.stage-hero__title` |
| `--type-objective-title` | objective/selected subject | `.pixel-world-command-cell__value` |
| `--type-action-label` | next executable move | `.pixel-world-command-cell__value` |
| `--type-receipt-body` | result/cause/effect | `.pixel-world-action-receipt__summary` |
| `--type-diagnostic-caption` | runtime/provider metadata | `.diagnostic-surface__meta` |

### 3.3 Space, Radius, and Motion Tokens
| Token | Role | Current value |
| --- | --- | --- |
| `--space-panel-gap` | desktop shell/panel gap | `18px` |
| `--space-stack-gap` | internal vertical rhythm | `16px` |
| `--space-control-gap` | badge/toolbar gap | `8px` |
| `--radius-tool` | buttons, inputs, compact tools | `12px` |
| `--radius-stage` | primary stage/pixel-world frame | `18px` |
| `--radius-panel` | section panels | `20px` |
| `--motion-feedback-fast` | hover/focus/status emphasis | `140ms` |

### 3.4 Governance Rules
- Do not add new hard-coded colors without mapping them to this table.
- If a task needs a new semantic state, add the token and status vocabulary
  first, then use it in CSS/JS/tests.
- Token aliases may evolve, but token meanings must not drift without updating
  this document and the parent visual spec.
- Any CSS token change requires at least one desktop screenshot and mobile
  screenshot when responsive or first-screen surfaces are affected.

## 4. Icon and Status Vocabulary
Viewer currently uses text/badge status marks rather than a shipped icon pack.
That remains valid: stable text, shape, placement, and data attributes are the
canonical icon vocabulary until an icon library is adopted.

### 4.1 Required Status Marks
| Status | Visual mark | Text equivalent | Data/test hook |
| --- | --- | --- | --- |
| selected | accent border or selected callout | `Selected` / `Current Selection` | `data-selected`, selection callout |
| target | accent badge and target panel | `Current Target` | `data-select-kind` |
| blocker | warn/danger badge or receipt state | `Blocker`, `World Constraint` | `data-receipt-state=blocked` |
| active route | route line between agent/location | route kind label | `data-route-kind` |
| accepted intent | command-stage card | `Accepted Intent` | stable text |
| executing | progress/action copy | `Executing` or explicit stage | future `data-action-state` |
| completed | success badge/receipt | `completed` / receipt summary | `data-receipt-state=completed` |
| no receipt | muted receipt state | `No action receipt yet` | `data-receipt-present=false` |
| derived position | badge/source label | `position=location_derived` | `data-position-source` |
| renderer unavailable | visible diagnostic | `Renderer Unavailable` | `data-renderer-state=unavailable` |
| diagnostics | muted collapsed surface | `Runtime Diagnostics` | `#viewer-diagnostics-panel` |

### 4.2 Icon Pack Rule
If a future task introduces an icon library, it must map icons to the table
above and preserve text equivalents. Icons may reinforce state, but they must
not be the only state signal.

## 5. Asset Language
### 5.1 Current Asset Scope
The current Viewer brand system relies on CSS, DOM shapes, text badges, canvas
diagnostic surfaces, and generated pixel-world screenshots. No new bitmap art pack
is required for this task.

### 5.2 Allowed Asset Types
- screenshots and visual smoke artifacts under ignored `output/playwright/`;
- favicon and existing static Viewer assets;
- future small symbol/icon assets only when they map to the status vocabulary;
- generated bitmap references only when a task needs marketing or illustrative
  material outside the software-safe product surface.

### 5.3 Asset Acceptance
An asset is acceptable only if it:
- makes agent, route, objective, blocker, receipt, or unavailable state easier to
  read;
- does not compete with the world stage;
- has a text or semantic equivalent;
- works in desktop and mobile screenshots;
- has an owner and evidence path in the task log.

Assets that are purely atmospheric, ornamental, blurred, or unrelated to the
actual world state are not valid for Viewer product surfaces.

<a id="visual-evidence-matrix-design"></a>

## 6. Screenshot and Visual Evidence Matrix
Any visible change must choose the smallest row set that covers its risk. A
large visual-system change must run all required rows. Every triggered review retains target screenshots, including mobile when required by the parent spec; DOM/test/smoke output supplements those images. One screenshot may cover several selected rows without separate images for each row, provided each risk is visible.

| Row | Viewport/state | Required for | Evidence |
| --- | --- | --- | --- |
| desktop player first screen | 1280x720 or wider | all stage/layout/token changes | screenshot, supplemented by browser DOM/visual smoke |
| mobile player first screen | 390x844 or equivalent | navigation, first screen, text density | screenshot, supplemented by browser DOM/visual smoke |
| diagnostics demotion | Runtime Diagnostics collapsed | diagnostics, routing, QA surfaces | DOM/test assertion plus screenshot when visual |
| renderer unavailable | unavailable diagnostic visible | renderer/pixel-world work | targeted UI/browser evidence |
| action receipt | present receipt | command/receipt/action work | `action-receipt-visual.png` or equivalent |
| no receipt | no player-caused receipt | causality/data honesty work | UI test or targeted screenshot |
| dense pixel-world | agent/location/route/terrain together | layer priority changes | pixel-world visual smoke |
| CJK/long text | `locale=zh` and long labels where touched | typography/density/localization changes | screenshot plus targeted DOM overflow probe |
| focus/keyboard | focusable controls/canvas where touched | accessibility changes | CSS/test/browser evidence |

Blocking findings:
- horizontal overflow on supported mobile width;
- clipped primary command or unreadable receipt/blocker;
- diagnostics louder than command/world surfaces;
- unavailable renderer without explicit unavailable label;
- derived position without source label;
- color-only critical state;
- focusable product control with no visible focus style.

### 6.1 Terminal acceptance rows
| Row | Target state | Desktop/mobile and input | Acceptance evidence |
| --- | --- | --- | --- |
| terminal shell | Player first load | desktop + 390x844 | stage/HUD/board/receipt hierarchy screenshot |
| Director boundary | allowed and denied/stale/revoked | fresh tab/reload | sanitized fail-closed state; no persistence |
| focus contract | console, drawer, Focus presentation | keyboard + IME | Escape priority and invoker focus return |
| feed contract | loading, empty, replay, gap, unavailable, reorg | desktop/mobile | distinct copy, cursor recovery, no receipt inference |
| bilingual density | matrix labels + long text | `locale=en|zh` | no clipping/overflow and readable primary actions |

## 7. Current Implementation Hooks
Current implementation anchors for this brand system:
- `#viewer-stage-panel`: world/stage surface.
- `#viewer-targets-panel`: target selection surface.
- `#viewer-details-panel`: command/context surface.
- `#viewer-diagnostics-panel`: demoted diagnostics surface.
- `.mobile-rail`: mobile route.
- `.pixel-world-command-strip`: objective/next move/player leverage.
- `.pixel-world-action-receipt`: player-facing receipt state.
- `.pixel-world-canvas`: world surface and rendered DOM.
- `.pixel-world-render-diagnostics`: renderer diagnostics.

These hooks are part of the visual-system contract. Rename or remove them only
with test and documentation updates.

## 8. Done Criteria for Visual-System Changes
A visual-system task is done only when:
- this document and the parent visual spec stay in sync;
- CSS/DOM hooks use semantic tokens or documented aliases;
- status marks include text equivalents and stable hooks;
- required screenshots and supporting automated evidence are recorded; screenshot-free DOM or smoke output cannot substitute for the visual review input;
- `npm --prefix crates/oasis7_viewer run test:ui` passes;
- `npm --prefix crates/oasis7_viewer run build:software-safe` passes;
- `git diff --check` and `./scripts/doc-governance-check.sh` pass;
- task execution log records role verdicts and evidence paths; verdict/confidence follows the [canonical decision table](../../testing/manual/model-visual-review-sop-2026-05-29.manual.md#visual-review-decision-contract), including escalation on the first `watch` and on `medium/low`.


## 目标、范围与设计边界

本轮只整理既有阅读顺序与视觉证据合同，不新增布局、交互、状态或产品承诺。
产品阅读义务来自 [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001)；
具体 shell 布局继续由 [当前实现边界](viewer-gameplay-release-experience-overhaul.prd.md#current-implementation-boundary)
拥有，终态/后续目标不得替代当前实现。品牌 token、固定 CSS 数值与完整状态字典仍是专业设计选择，
不从该产品 REQ 推导；下面的产品映射只适用于明确列出的局部条款。

## 风险与未覆盖范围

本次仅建立文档与静态源码的局部关系，不能证明产品 AC 已完成。相关 Agent/地点/路线的完整
呈现、所有正式 surface、密度变化后的可读性、完整继续/纠正/中断/恢复路径及真实玩家理解
仍需对应专业实现与验收证据。静态 JSX/CSS 不证明浏览器视觉、输入、a11y、renderer、
生产 Director 成功路径或发布通过；历史截图和 visual cards 不升级为本候选证据。

## Visual review traceability

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-AGENT-STAGE-001：目的、blocker 与下一步的阅读顺序](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001) | Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；仅承接当前目的、blocker 与下一步的首读层级：单一 Next Move 主区、Objective / Player Leverage 从属；具体布局沿专业 shell authority，窄屏主区优先 | [本设计布局条款](#23-terminal-shell-brand-summary) | viewer_engineer；game_visual_interaction_designer | 仅阅读顺序局部承接；不证明相关对象/路线全覆盖、密度变化可读性、完整恢复路径或产品 AC 完成；不扩展 target 合同 |
| [QA visual review contract](../../testing/prd.md#visual-review-product-contract) | 截图为必要输入；DOM/smoke 为补充；按风险最小行集且允许一图覆盖多行 | [本设计条款](#visual-evidence-matrix-design) | QA 与视觉 owner；产品 N/A：理由=本行只规定专业截图取证与评审处置机制，不新增玩家舞台义务；范围=本行 visual-review 机制，不能排除上述真实产品阅读关系 | 不替代 required review、发布放行、方向决策或 L5；不重写历史卡 |

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-AGENT-STAGE-001：目的、blocker 与下一步的阅读顺序](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001) | [本设计布局条款](#23-terminal-shell-brand-summary) | 逐句核对产品目的/blocker/下一步子义务与专业主合同；不恢复三张平级卡片或 Next Move 第二行 | 对同候选主合同与 `crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx`、`crates/oasis7_viewer/viewer_terminal_shell.css` 做静态对照；未来可见改动另按 [Viewer S6 手册](../../testing/manual/web-ui-agent-browser-closure-manual.manual.md) 的视觉矩阵取得真实桌面/窄屏及输入证据 | 本 PR 的 source/diff 与限定静态核对；没有新增浏览器证据 | 源码接线不证明新 S6、真实玩家体验、完整 renderer/a11y 或发布通过 |
| [QA visual review contract](../../testing/prd.md#visual-review-product-contract) | [本设计条款](#visual-evidence-matrix-design) | 同修订文档对相同证据与 verdict/confidence 给出同一处置 | 按 [SOP 判定表](../../testing/manual/model-visual-review-sop-2026-05-29.manual.md#visual-review-decision-contract)离线审读无截图有 DOM、缺视口、首轮/连续 watch、high/medium/low、block、公开/内部与方向争议 fixture；实际视觉改动另采本候选截图 | 文档 fixture 的条件、预期处置与交叉引用；执行后另留真实截图/card | 本次规则审读不生成浏览器运行、视觉通过或发布证据 |

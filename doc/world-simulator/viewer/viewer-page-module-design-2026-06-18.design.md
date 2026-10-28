# Viewer Page and Module Design (2026-06-18)

- Professional owner: `game_visual_interaction_designer`
- Integration owner: `tpm`
- Source task uid: `task_e7760ad76a0046dfa5a17d0a5a89e59c` / GitHub issue #1496; execution evidence is in GitHub task issue evidence comments and `.pm/github-project-sync/task-archive.jsonl`.
- Slice type: `page_overall_and_module_design`
- Companion specs:
  - `doc/world-simulator/viewer/viewer-visual-design-spec-2026-06-05.design.md`
  - `doc/world-simulator/viewer/viewer-brand-system-2026-06-05.design.md`
- Image2 visual target:
  - `output/visual-targets/viewer-visual-polish/image2-viewer-first-screen-target.png`

<a id="1-purpose"></a>

## 1. Purpose（目标）
This document converts the current complex Viewer page into an executable
page-level and module-level visual interaction design. It does not replace the
June 5 visual and brand-system specifications. It narrows them to the current
`software_safe` Viewer page so implementation can proceed in safe slices.

The page is a player-facing command table. Its job is to let a player read the
world, understand the active objective, find the next command route, see the
latest action receipt, and reach details or diagnostics without the first screen
becoming a diagnostics dashboard.

The paired [`viewer-gameplay-release-experience-overhaul.prd.md`](viewer-gameplay-release-experience-overhaul.prd.md)
owns the terminal shell contract. This module design translates that authority
into visual zones and interaction details; focus/right-panel hooks are current
implementation baseline/debt, not a second product mode.

This design owns visual hierarchy, interaction readability, module rhythm,
state expression, and screenshot-review expectations. It does not own runtime
correctness, engineering feasibility, QA release judgment, product priority, or
new gameplay semantics.

<a id="2-overall-page-design"></a>

## 2. Overall Page Design（页面整体结构）
### 2.1 Screen Purpose
The Viewer first screen must answer, in order:

1. What world or stage am I looking at?
2. Which objective, actor, blocker, or route matters now?
3. What can I do next?
4. What happened because of the latest action or intent?
5. Where can I inspect targets, command details, gameplay state, or diagnostics?

The page should feel like an industrial world command table, neither a landing page
nor an internal observability console.

### 2.2 Information Architecture
Use four priority levels:

| Level | Viewer surface | Purpose | First-screen rule |
| --- | --- | --- | --- |
| Stage | `#viewer-stage-panel`, `.pixel-world-canvas` | world comprehension | largest visual subject |
| Command | `.pixel-world-command-strip`, `.pixel-world-action-receipt`, `#viewer-details-panel` | decision and feedback | visible within one step from stage |
| Context | `#viewer-targets-panel`, `#viewer-gameplay-details`, selected target details | selection, explanation, state machine | useful, not louder than stage |
| Diagnostics | `#viewer-diagnostics-panel`, renderer details, raw JSON | debug and QA visibility | collapsed or visually muted by default |

Player layout uses one world-first stage with a quiet edge dock:

- stage: full-stage world board and compact HUD own the first visual read;
- edge dock: `World / Targets / Command` are primary; Search is the Targets
  `#entity-search` compatibility filter and Diagnostics is secondary;
- contextual console: opens on demand in the order primary command/chat, selected
  context, gameplay details, diagnostics/raw state;
- Director-only layout: a dense three-column tool arrangement may be exposed when
  explicitly selected through the server-validated capability boundary, without
  changing any product or gameplay semantics. The local endpoint fails closed when
  no trusted issuer can bind the grant to the live session epoch.

Targets, details, and diagnostics remain reachable routes, not equal-weight
columns competing with the Player stage.

### 2.3 Reading Order
Desktop first viewport:

1. Compact stage identity and current objective.
2. Command strip with a primary Next Move region and supporting Objective / Player Leverage context.
3. Pixel World Board/Canvas.
4. Action Receipt.
5. Edge dock routes for Targets and Command; Search stays inside Targets and
   Diagnostics remains secondary.
6. Gameplay Details and raw diagnostics inside the contextual console as disclosure
   surfaces.

Mobile reading order:

1. Edge/mobile rail: `World / Targets / Command`, with Search inside Targets and
   Diagnostics secondary.
2. Compact stage identity.
3. Command strip with Next Move on the first row and supporting context on the second row.
4. World Board/Canvas.
5. Action Receipt.
6. Targets, Command, Gameplay Details, Diagnostics through anchors or tabs.

### 2.4 Layout Zones
The page should behave as a command-table shell:

- Header identity: compact, task-aware, no large marketing hero.
- Command band: objective, next move, player leverage.
- World board: the main visual canvas and route/selection read.
- Feedback band: action receipt, blocker, no-receipt, or unavailable state.
- Edge dock: target navigation and route entry, with Search filtering the current
  authoritative Targets list.
- Contextual console: command/chat first, then selected context, gameplay details,
  auth/economy, and diagnostics/raw state.

The command band and receipt may bracket the board, but they must not compress
the board into a secondary widget.

The [shell's current implementation boundary](viewer-gameplay-release-experience-overhaul.prd.md#current-implementation-boundary)
owns HUD placement: one primary Next Move region and subordinate Objective /
Player Leverage context. Narrow screens place Next Move first and supporting
context second. The previous three-peer-card layout and Next Move second row
are superseded, not an alternative current default. `More / Diagnostics` remains
a reachable secondary route rather than a competing primary column.

### 2.5 Responsive Route
Player desktop, tablet, and mobile preserve one conceptual route: stage first,
edge dock second, contextual console on demand. Director may use three columns
only after explicit opt-in. Tablet stacks dock routes without demoting the stage;
mobile uses the rail and does not rely on a long undifferentiated page stack.
Keep `World / Targets / Command` primary, Search inside Targets, Diagnostics secondary, and
preserve keyboard focus, close/`Escape`, CJK labels, no-overflow, and honest
empty/unavailable contracts at every width.

### 2.6 Target wireframes (ASCII; implementation handoff)
These wireframes illustrate the world-first hierarchy under the shell contract
above, not pixel placement, a screenshot, or new shipped-layout evidence. HUD
regions follow the canonical primary/context order; they are not three peer
cards. Director may expose a denser tool arrangement only after the explicit,
capability-gated action described by the paired PRD.

Desktop (world-first, 1280px+):

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ world / stage identity       Next Move [primary] · supporting context        │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌───────────────────────────────────────────────┐  ┌──────────────────────┐  │
│  │                                               │  │ World  Targets       │  │
│  │              PIXEL WORLD BOARD                │  │ Command  More        │  │
│  │   agent · route · selected target · blocker   │  │ (quiet edge dock)    │  │
│  │                                               │  └──────────────────────┘  │
│  └───────────────────────────────────────────────┘                           │
│  Action Receipt: result / reason / next       [open contextual console]       │
└──────────────────────────────────────────────────────────────────────────────┘
```

Mobile (390x844; sticky rail, one route at a time):

```text
┌──────────────────────────────┐
│ world / objective             │
├──────────────────────────────┤
│ World  Targets  Command  More │  ← sticky rail; Search is inside Targets
├──────────────────────────────┤
│ Next Move [primary]          │
│ Objective / Player Leverage  │
├──────────────────────────────┤
│                              │
│       PIXEL WORLD BOARD      │
│                              │
├──────────────────────────────┤
│ Action Receipt / blocker      │
└──────────────────────────────┘
```

`More` contains secondary Diagnostics. Quote is reachable from the contextual
Command route, never as a fifth rail item. Target selection recenters/highlights
and opens context; it never executes an action.

Targets and contextual Command are one-at-a-time overlays, not permanent columns:

```text
┌──────── Targets ────────┐      ┌──────── Contextual Command ─────────┐
│ Search targets           │      │ Command / Chat                      │
│ A-017  Mining  [selected]│      │ Selected context                    │
│ B-04   Moving            │      │ Gameplay Details ▸                  │
│ C-12   Thinking          │      │ Diagnostics / raw state ▸           │
│                [Close]   │      │ Quote appears here when relevant    │
└──────────────────────────┘      └─────────────────────────────────────┘
```

Director is a capability-gated dense tool view entered only through the secondary
operator action; it is never the fresh-load layout:

```text
┌──────────────────────────── Director ──────────────────────────────────┐
│ World/selection │ stage + authoritative state │ diagnostics/tools      │
│                 │ semantics unchanged          │ [Exit Director]        │
└────────────────────────────────────────────────────────────────────────┘
```

Target source anchors are `#viewer-director-entry` and `#viewer-director-exit`.
They are implemented source anchors; the flow remains fail-closed until the server
returns a valid short-lived grant, and the current issuer gap keeps production entry
`capability_blocked`.

### 2.7 Interaction state machine and focus contract
The implementation must preserve these transitions; labels may be localized but
the transitions and causality boundaries are stable:

| From | Trigger | To | Required feedback |
| --- | --- | --- | --- |
| `player_ready` | `World` | `stage_visible` | board remains first focus; no new action |
| `player_ready` | `Targets` | `targets_open` | focus moves to heading/close, Search remains inside Targets |
| `targets_open` | target select | `target_selected` | row, board callout, and context mirror selection; no execution |
| `target_selected` | `Command`/Next Move | `console_open` | command/chat first, then selected context/details/diagnostics |
| `console_open` | submit command | `receipt_pending` | pending copy; never show completion from ambient feed |
| `receipt_pending` | runtime receipt | `receipt_completed`/`receipt_blocked`/`receipt_rejected` | Action Receipt shows result, reason, and next |
| any local drawer | `Escape` (no IME composition) | prior surface | close only the innermost drawer and restore its invoker |
| focus presentation | `Escape` (no local drawer) | prior Player route | exit Focus/presentation and return focus to Focus invoker |
| Player | explicit Diagnostics + capability | `director_open` | ephemeral dense tools; no semantic or progress change |
| Player/Director entry | invalid/stale/revoked/unauthorized | `player_ready` | sanitize Director surfaces, preserve world/selection, explain recovery |

Escape is ordered local surface/drawer first, then Focus. While an IME composition
is active, the browser/IME consumes Escape and the Viewer must not close the surface.
Every opened surface records its invoker and returns focus to it on close; if the
invoker disappeared, return to the owning rail item or stage heading.

## 3. Module Design
### 3.1 Stage Hero
Current anchors: `WorldStageHero`, `.stage-hero`, `.hero-focus-grid`.

Role:
- establishes screen identity and current situation;
- exposes connection warnings only when they require attention;
- provides quick routes to targets and command on narrow screens.

Design contract:
- keep it compact when `PixelWorldHost` has a commercial surface;
- show one stage identity and one current goal line;
- avoid repeating the full Objective, Next Move, and receipt already handled by
  Pixel World Board modules;
- connection warning may be visually elevated only when disconnected,
  connecting, closed, or otherwise abnormal;
- selected-target badges may appear, but they remain secondary to the board.

State handling:
- loading: reserve height and show a short "syncing world" style line;
- empty/no snapshot: explain that the world is waiting for canonical state;
- blocked/connection issue: elevate the warning and recovery route;
- long text or zh locale: title wraps to two lines before pushing the board
  below the first viewport.

### 3.2 Pixel World Board / Canvas
Current anchors: `PixelWorldHost`, `.pixel-world-canvas`,
`.pixel-world-canvas--rendered`, `.pixel-world-canvas__surface`.

Role:
- primary visual subject;
- shows world, active agent, selected target, route, objective callout, blocker,
  and unavailable-state honesty;
- gives the player spatial confidence before command or diagnostics detail.

Design contract:
- occupies the largest center-zone area on desktop;
- preserves a stable aspect ratio and minimum height across loading, unavailable,
  rendered, and selected states;
- active agent, selected target, objective callout, and route must read above
  fragments, terrain, and ambient labels;
- renderer unavailable is valid only as an explicit diagnostic state;
- canvas controls or renderer metadata must never look like the primary action.

State handling:
- loading: skeleton or reserved board frame with stage label;
- unavailable: clear renderer unavailable state plus honest reason;
- blocked: blocker callout near the route or target, paired with receipt;
- selected: selected entity gets a visible focus callout and details route;
- long labels/zh: prefer label truncation with details route over shrinking
  the board text below readability.

### 3.3 Command Strip
Current anchors: `.pixel-world-command-strip`, `.pixel-world-command-cell--next`,
`[data-shell-region="next-move-primary"]`, and
`[data-shell-region="supporting-context"]`.

Role:
- one primary action region with subordinate context over the world-first board;
- Next Move first and supporting context second on narrow screens;
- tells the player current objective, next move, and why player action matters.

Design contract:
- Next Move owns the primary action affordance; Objective and Player Leverage
  explain it as supporting context, not equally weighted peer cards;
- `Go to Command` may be used as a conservative route when direct execution has
  not been verified by `viewer_engineer`;
- do not imply a direct runtime action unless an existing published action is
  safely bound;
- blocked or unavailable next move must show the reason in the same cell.

State handling:
- empty/no commercial surface: strip should not render or should collapse into
  a clear waiting state;
- blocked: Next Move cell changes from action route to blocker/recovery route;
- completed: Next Move may point to next objective or review receipt;
- long text/zh: cells wrap within fixed tracks; action control remains visible.

### 3.4 Action Receipt
Current anchor: `.pixel-world-action-receipt`.

Role:
- confirms player-caused or accepted-intent feedback;
- distinguishes no receipt, accepted intent, world delta, blocked, rejected, and
  completed states;
- prevents ambient simulation motion from being misread as player progress.

Design contract:
- visible near the board and command strip, before diagnostics;
- title, summary, detail, and confidence meta are all visually distinct;
- no-receipt is a real state, not an absence of UI;
- blocked/rejected states use warning or danger language plus recovery route;
- completed states can be positive but must not imply unsupported causality.
- World Feed v1 is implemented: Viewer owner `viewer_engineer`, source the ordered
  persisted runtime `WorldEvent` journal projection, stable anchor `#viewer-world-feed`,
  and acceptance that it remains ambient, never replaces a missing receipt, and
  exposes honest empty/loading/unavailable states. In fullscreen Player it stays a
  compact collapsed top-right edge overlay/ambient chip and opens on demand; it must
  not become a persistent first-screen column.
  Current `state.recentEvents` is only a non-contract ambient preview;
  `player_gameplay.recent_feedback` remains exclusive to Action Receipt. Current
  Recent Events/Feedback are not silently renamed.

State handling:
- no receipt: muted but explicit;
- blocked/rejected: visually higher than diagnostic metadata;
- completed: success state with concise summary;
- long detail: clamp or wrap with a details affordance, not layout overflow.

### 3.5 Targets Panel
Current anchor: `#viewer-targets-panel`.

Role:
- selection and orientation surface;
- lets the player choose the agent, location, or target that the stage and
  command panel will follow.
- Search filters the current authoritative visible-target list through
  `#entity-search`; it is not an independent snapshot query. Selection never
  executes an action. Quote remains a contextual Command/Console route, not
  a peer dock item.

Design contract:
- target rows should read as selectable controls, not diagnostics;
- current selection must be mirrored in the stage or board;
- pending/syncing states reserve list space;
- raw counts are secondary to selectable names and state badges.

State handling:
- loading: pending row skeleton;
- empty: explain whether there is no snapshot, no targets, or a filter result;
- selected: visible selected state and anchor to stage/command;
- zh/long names: preserve row controls and avoid horizontal overflow.

### 3.6 Details / Command Panel
Current anchor: `#viewer-details-panel`.

Role:
- command execution route and selected-target details;
- contextual-console command area on every Player viewport;
- target of `Go to Command` on mobile and conservative Next Move routing.

Design contract:
- primary command or chat route appears before raw prompt and inspection tools;
- disabled command states must show reason and recovery;
- selected-target context appears before raw JSON;
- if Next Move routes here, the destination section should visibly match the
  action label enough that the player knows they arrived correctly.
- primary command/chat appears before gameplay details and diagnostics/raw state;
- action result is represented by the Action Receipt, never by ambient World Feed
  activity or a selection event.

Handoff:
- direct Next Move execution feasibility belongs to `viewer_engineer`;
- gameplay meaning of new action states belongs to `gameplay_designer` and
  `runtime_engineer`.

### 3.7 Gameplay Details
Current anchor: `#viewer-gameplay-details`.

Role:
- expanded state-machine, economy, auth, and formal gameplay details;
- context layer, not first-screen command layer.

Design contract:
- default collapsed after this polish pass unless the page lacks summary data;
- summary line explains why it exists: full state machine and economy detail;
- never duplicates the command strip at equal visual weight;
- contains formal status, accepted intent, execution state, economy, auth, and
  hosted surfaces without re-promoting diagnostics.

State handling:
- empty/no canonical gameplay snapshot: concise waiting explanation;
- blocked: formal blocker detail lives here, but primary blocker remains near
  command/receipt;
- completed: can show history and state machine, not replace receipt.

### 3.8 Diagnostics
Current anchors: `#viewer-diagnostics-panel`, `.diagnostic-surface`,
`.pixel-world-render-diagnostics`, `.pixel-world-render-unavailable`.

Role:
- engineering and QA visibility;
- renderer, provider, auth, raw DTO, and bridge details;
- reachable, scriptable, and honest, but visually quiet.

Design contract:
- collapsed by default in Player mode;
- renderer unavailable is visible when it affects the world board, but diagnostic
  detail stays in a secondary disclosure;
- raw JSON never appears before command, board, receipt, or target selection;
- badges use diagnostic-muted weight unless warning or failure affects player
  recovery.

State handling:
- unavailable: explicit label, reason, and route to details;
- fatal renderer: warning badge plus diagnostic expansion;
- no runtime/provider: distinguish player-fixable from system issue;
- long diagnostic text: scroll or collapse inside diagnostic surface.

### 3.9 Player Shell Immersive Presentation
Current anchors: `.pixel-world-host--focus`, `.pixel-world-focus-hud`,
`.pixel-world-focus-cinematic`, `.pixel-world-focus-drawer`.

Role:
- implementation presentation for the default Player shell: immersive world-first
  reading of objective, route, blocker, receipt, and command without the Director
  tool density. These hooks are an implementation substrate, not a separate
  product mode or alternative authority.

Design contract:
- Player HUD uses stable command vocabulary: World, Current Objective,
  Mission Progress, World Tick, Blocker, Receipt;
- board remains the main subject;
- command drawer is a clear route, not a hidden debug panel;
- diagnostics drawer remains separate and lower priority;
- exit/maximize controls are visible and keyboard focusable.

State handling:
- renderer unavailable: minimap is absent and the diagnostic is honest and compact;
- no selected agent: command surface explains selection requirement;
- blocked: blocker cell rises above receipt detail;
- mobile: immersive HUD stacks without overlapping the board or controls; close/
  `Escape` returns to the same Player route.

### 3.10 Mobile Route
Current anchor: `.mobile-rail`.

Role:
- preserves the same IA without forcing the player to scroll blindly.

Design contract:
- primary route is `World / Targets / Command`;
- Search stays inside Targets; Diagnostics is a quieter secondary link;
- first mobile viewport should include stage identity plus either command strip
  or board start;
- avoid horizontal overflow from badges, long labels, or command controls;
- buttons and links need stable tap targets.

State handling:
- long zh labels: wrap or shorten nav labels, do not overflow;
- blocked/offline: show warning near world, not only inside diagnostics;
- immersive Player presentation: keep close/`Escape` and command controls reachable.

## 4. State Model
All modules should map to these player-facing states:

| State | Visual requirement | Notes |
| --- | --- | --- |
| empty | explicit empty reason and next observable signal | distinguish empty world, no target, no receipt |
| loading | reserved layout, `aria-busy` or equivalent where DOM supports it | no stage jump |
| unavailable | clear renderer unavailable label near affected board | raw details stay secondary |
| blocked | blocker reason near next move and receipt | recovery route visible |
| no receipt | muted receipt surface remains present | prevents false progress |
| completed | positive receipt state plus next route | do not imply unsupported runtime causality |
| selected | visible focus state in list, board, and details | selection must not hide objective |
| long text | wrap or clamp within stable modules | no horizontal overflow |
| zh locale | CJK labels fit primary controls and panels | screenshot required when touched |

### 4.1 Stable anchor and compatibility migration
Existing hooks remain compatibility inputs. New anchors are added only in source
JSX, with one unique ID each; generated bundles are never hand-edited.

| Existing hook | Terminal role | Migration action | Compatibility/verification |
| --- | --- | --- | --- |
| `#viewer-stage-panel` | Player stage | retain | one ID; stage-first route smoke |
| `#viewer-targets-panel` | Targets dock/route | retain; nest `#entity-search` | Search filters visible authoritative targets only |
| `#viewer-details-panel` | contextual Command route | retain as section; optionally add `#viewer-console` in source JSX | no duplicate IDs; command/chat first |
| `#viewer-diagnostics-panel` | secondary More/Diagnostics | retain; optional `#viewer-diagnostics-drawer` | hidden by default in Player; explicit unavailable state |
| `.mobile-rail` | sticky World/Targets/Command rail | retain class; add source button labels | 390x844 keyboard/tap route |
| `.pixel-world-action-receipt` | causal receipt | retain class; add `#viewer-action-receipt` only in source JSX if needed | no World Feed substitution |
| `.pixel-world-host--focus` / focus HUD/drawer hooks | Player immersive presentation | classify as implementation substrate, not separate mode | Focus close/Escape/focus-return tests |
| `#viewer-director-entry` / `#viewer-director-exit` | capability-gated Director transition | retain source anchors; server-validated short-lived grant and focus-return handling | fresh-load Player, denied/stale/revoked/unavailable, reload and exit tests |
| `#viewer-world-feed` | World Feed v1 ambient projection | retain implemented panel; runtime journal source, cursor/reorg recovery, explicit/null-safe receipt refs | schema/order/dedup/gap/reload tests; no Action Receipt substitution |

These anchors are now implemented in source and generated output. They remain
compatibility surfaces: Director grant state is ephemeral, and World Feed links are
rendered only for explicit runtime `receipt_ref` values (current runtime uses `null`).

## 5. Image2 Target Usage
The Image2 target is binding as a direction for:

- world board as the dominant first-screen subject;
- compact Objective / Next Move / Player Leverage command band;
- visible but concise Action Receipt;
- diagnostics and auxiliary panels demoted to side or disclosure areas;
- industrial command-table tone and clear spatial route reading.

The Image2 target is aspirational, not binding, for:

- exact map art, terrain density, icons, and pixel illustration style;
- exact browser chrome, brand mark, or historical right/left panel labels;
- exact colors beyond the existing Viewer token taxonomy;
- exact layout measurements.

The Image2 target cannot infer:

- runtime DTO availability, action execution feasibility, or renderer status;
- DOM hooks, tests, focus behavior, keyboard accessibility, or screen-reader
  labels;
- mobile stacking behavior;
- zh locale length and CJK typography;
- empty, loading, no-receipt, unavailable, blocked, completed, and long-text
  states.

Use the target by comparing real desktop and mobile Viewer screenshots against
the binding direction above. Record gap notes before claiming the visual pass
has converged.

## 6. Implementation Slicing Recommendation
### Slice 1: Page IA and duplication control
Goal: make the first screen read as one command table.

Recommended work:
- keep `WorldStageHero` compact;
- keep `WorldSummaryPanel` as collapsed `Gameplay Details`;
- preserve stable anchors:
  - `#viewer-stage-panel`
  - `#viewer-targets-panel`
  - `#viewer-details-panel`
  - `#viewer-diagnostics-panel`
  - `#viewer-gameplay-details`

Verification gate:
- UI tests for stable anchors and copy changes;
- desktop screenshot confirms no three equal objective summaries.

### Slice 2: Command strip and receipt hierarchy
Goal: make Objective, Next Move, Player Leverage, and Action Receipt the
command band around the board.

Recommended work:
- strengthen Next Move as route to command, not direct action unless verified;
- ensure blocked/no-receipt/completed variants have clear visual states;
- quiet readout badges.

Verification gate:
- `test:ui`;
- receipt/no-receipt assertions;
- desktop and mobile screenshot comparison with Image2 target direction.

### Slice 3: Board prominence and unavailable honesty
Goal: make the canvas board the main subject across renderer states.

Recommended work:
- enforce stable board dimensions;
- keep renderer unavailable explicit;
- ensure selected/route/objective/blocker layers read above terrain.

Verification gate:
- `build:software-safe`;
- `test:pixel-world:visual`;
- external browser runtime check if renderer/WebGPU behavior conflicts with
  in-app browser evidence.

### Slice 4: Player shell presentation and mobile route
Goal: keep the Player shell presentation and mobile route coherent after
hierarchy changes.

Recommended work:
- verify Player HUD labels, contextual command/diagnostics drawers, and close/
  `Escape` controls;
- verify mobile `World / Targets / Command` route, Targets-contained Search and secondary Diagnostics,
  long labels, and no overlap.

Verification gate:
- Player shell visual screenshot, desktop and mobile;
- mobile screenshot at 390x844 or equivalent;
- zh locale screenshot or overflow probe.

### Slice 5: Terminal handoff order (superseding implementation order)
The six-slice order below is the integration contract for the terminal shell. It
keeps feed/runtime work separate from the initial IA migration.

1. **Anchors/tests** — retain canonical anchors, add only source-JSX IDs, and add
   duplicate-ID/compatibility assertions.
2. **Shell layout** — implement the world-first stage, compact HUD, quiet dock and
   mobile sticky rail without changing data semantics.
3. **Console/receipt** — open contextual Command on demand, place Quote inside it,
   and render explicit no-receipt/pending/blocked/completed Action Receipt states.
4. **Focus/a11y** — implement local-drawer-first Escape, IME protection, focus return,
   keyboard names, CJK/long-text wrapping and no-overflow checks.
5. **Separate World Feed** — consume only the implemented additive `world_feed/v1`
   projection; preserve Recent Events/Feedback names and explicit/null-safe
   `receipt_ref` semantics.
6. **Headed QA** — run desktop/mobile/keyboard/locale/state evidence and record
   residual risk; no screenshot-only preview substitutes for interaction smoke.

## 7. Verification Gates
Recommended commands when the local Node environment is available:

- `npm --prefix crates/oasis7_viewer run test:ui`
- `npm --prefix crates/oasis7_viewer run build:software-safe`
- `npm --prefix crates/oasis7_viewer run test:pixel-world:visual`

Recommended screenshot scenarios:

- desktop first screen, `locale=en`;
- desktop first screen, `locale=zh`;
- mobile first screen, `locale=en`;
- mobile first screen, `locale=zh`;
- renderer unavailable;
- action receipt present;
- no receipt;
- blocked next move;
- completed receipt;
- Player shell presentation, desktop and mobile.

### 7.1 Terminal QA evidence matrix
| ID | Scenario | Required evidence | Blocking observation |
| --- | --- | --- | --- |
| QA-IA-D | Player desktop first screen | headed screenshot + DOM landmark read | stage is not dominant or dock becomes a competing column |
| QA-IA-M | Player mobile 390x844 | headed screenshot + rail interaction | rail missing/scrolls away or horizontal overflow appears |
| QA-DIR | Director allowed/denied/stale/revoked | tab reload/new-tab smoke + sanitized screenshot | Director persists, changes semantics, or fails to return to Player |
| QA-FOCUS | console/drawer/focus presentation | keyboard sequence with focus target log | Escape closes wrong layer, loses focus, or interrupts IME |
| QA-ANCHOR | source anchors and compatibility | DOM ID uniqueness + route assertions | duplicate ID, generated artifact edit, or missing invoker route |
| QA-REC | no receipt/queued/advanced/no-progress/blocked | state fixtures + screenshot/DOM copy | ambient event or queued state reads as success |
| QA-FEED | loading/empty/replay/gap/unavailable/reorg | feed fixture + cursor/reload evidence | feed guesses receipt linkage or merges recovery states |
| QA-COPY | English/Chinese/long labels | `locale=en|zh` screenshots or overflow probe | clipped primary action, raw enum, or unreadable CJK |
| QA-RENDER | renderer unavailable | explicit unavailable state screenshot | blank board or diagnostics-only explanation |

These remain acceptance inputs for `qa_engineer`. Task #3248 epoch 2 now has
source/package tests plus external-Chrome desktop/mobile Player-shell evidence;
World Feed schema/transport/state and Director fail-closed boundary are implemented,
while issuer-backed Director allowed entry and full headed renderer/Focus release
rows remain outstanding.

<a id="8-residual-risks-and-role-handoffs"></a>

## 8. Residual Risks and Role Handoffs（残余风险与角色交接）
- Task #3248 epoch 2 removes Quote from the peer rail, defaults Gameplay Details
  closed, ships stage-first Targets/Command drawers, opens/focuses secondary
  Diagnostics, and guarantees route/Focus drawer Escape, IME suppression, and
  invoker focus return in source and packaged Web tests. Quote remains contextual
  Command content only when its real surface exists.
- World Feed v1 DTO, cursor/reorg recovery, and `#viewer-world-feed` anchor are shipped
  in runtime/viewer code; current projections keep `receipt_ref=null` unless runtime
  supplies explicit causal identity. Cross-surface headed QA remains required.
- Director verifier/state machine and fail-closed endpoint are shipped, but the trusted
  operator issuer is not wired; successful production entry remains `capability_blocked`.
- WebGL2 has GPU-enabled `ready` evidence and GPU-disabled explicit `Renderer Unavailable`
  fallback evidence; this does not replace full renderer/release judgment.
- `viewer_engineer`: decide whether Next Move can directly execute a published
  action or should remain an anchor to `#viewer-details-panel`; validate DOM,
  renderer, responsive, and implementation feasibility.
- `qa_engineer`: own pass/fail release judgment after tests, screenshots, and
  browser smoke evidence.
- `runtime_engineer`: required if UI changes need new receipt, blocker, world
  delta, or renderer status semantics.
- `gameplay_designer`: required if Next Move, Player Leverage, or objective copy
  changes gameplay meaning, player verbs, or progression expectations.
- `producer_system_designer`: required if this changes product priority,
  Player/Director defaults, or scope boundaries.
- `liveops_community`: not required for this design unless later work drafts
  public messaging, release notes, player promises, or community explanations.

## 9. Professional Slice Verdict
Proceed with the visual polish only as a page-architecture and module-contract
pass, then implement in small slices. Existing implementation hooks can support
the compact HUD, collapsed gameplay details, command proximity, and diagnostics
demotion goals, but real screenshot gap notes against the Image2 target and
module-state verification are still required before any completion claim.

## Current shell layout traceability

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 未覆盖范围 |
| --- | --- | --- | --- | --- |
| [Player shell 当前布局](viewer-gameplay-release-experience-overhaul.prd.md#current-implementation-boundary) | 单一 Next Move 主区、Objective / Player Leverage 从属上下文；窄屏主区第一行 | [本设计布局条款](#33-command-strip) | viewer_engineer；game_visual_interaction_designer | 不新增布局决策、DTO、玩法或发布承诺；未扩展 target 合同 |

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [Player shell 当前布局](viewer-gameplay-release-experience-overhaul.prd.md#current-implementation-boundary) | [本设计布局条款](#33-command-strip) | 当前操作条款与主合同一致，不能要求恢复三张平级卡片或 Next Move 第二行 | 对同候选主合同与 `crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx`、`crates/oasis7_viewer/viewer_terminal_shell.css` 做静态对照；未来可见改动另按 [Viewer S6 手册](../../testing/manual/web-ui-agent-browser-closure-manual.manual.md) 的视觉矩阵取得真实桌面/窄屏及输入证据 | 本 PR 的 source/diff 与限定静态核对；没有新增浏览器证据 | 源码接线不证明新 S6、真实玩家体验、完整 renderer/a11y 或发布通过 |

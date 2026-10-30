# Viewer Page and Module Design (2026-06-18)

- Professional owner: `game_visual_interaction_designer`
- Integration owner: `tpm`
- Source task uid: `task_e7760ad76a0046dfa5a17d0a5a89e59c` / GitHub issue #1496; execution evidence is in GitHub task issue evidence comments and `.pm/github-project-sync/task-archive.jsonl`.
- Slice type: `page_overall_and_module_design`
- Companion specs:
  - `doc/world-simulator/viewer/viewer-visual-design-spec-2026-06-05.design.md`
  - `doc/world-simulator/viewer/viewer-brand-system-2026-06-05.design.md`
- Historical Image2 target (unversioned reference, not tracked in the current repository):
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
The lists below describe attention priority, not top-to-bottom placement. Next Move
and Action Receipt remain in the lower HUD shown in §2.6 and §10.4.

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

The command band and independent receipt stay in the lower HUD, with Next Move
above the receipt. They must not compress the world into a secondary widget.

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

Desktop (world-first, 1280px+; Next Move stays in the lower HUD above the independent receipt):

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ world / stage identity                                  World Feed [closed] │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌───────────────────────────────────────────────┐  ┌──────────────────────┐  │
│  │                                               │  │ World  Targets       │  │
│  │              PIXEL WORLD BOARD                │  │ Command  More        │  │
│  │   agent · route · selected target · blocker   │  │ (quiet edge dock)    │  │
│  │                                               │  └──────────────────────┘  │
│  └───────────────────────────────────────────────┘                           │
│  Next Move [primary] · supporting Objective / Player Leverage                │
│  Action Receipt: original subject / result / reason / next                    │
└──────────────────────────────────────────────────────────────────────────────┘
```

Mobile (390x844; sticky rail, one route at a time; primary first means the first HUD row, not the top of the viewport):

```text
┌──────────────────────────────┐
│ world / objective             │
├──────────────────────────────┤
│ World  Targets  Command  More │  ← sticky rail; Search is inside Targets
├──────────────────────────────┤
│                              │
│       PIXEL WORLD BOARD      │
│                              │
├──────────────────────────────┤
│ Next Move [primary]          │
│ Objective / Player Leverage  │
│ Action Receipt / blocker    │
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

<a id="detailed-uiux-handoff"></a>
## 10. Viewer 与 Bevy 像素世界的详细 UI/UX 交接

本节细化既有 Player shell，不重新定义玩法、权限、世界真值或 renderer。设计稿为 **设计目标 / DESIGN TARGET**：示例身份、文案、数值和世界图形只用于说明布局与状态，不是运行截图或当前能力证明。

本节由 `game_visual_interaction_designer` 维护，`viewer_engineer` 承接实现，`qa_engineer` 按末尾映射验证。设计目标不把已记录的平台/真实交互缺口改判为已实现。本轮只交付文档与静态稿，不变更 JS、Rust、CSS、协议、默认行为或游戏规则。

<a id="uiux-screen-regions"></a>
### 10.1 屏幕分区与首读

| 区域 | 内容及优先级 | 展开/收起与拥挤处理 |
| --- | --- | --- |
| 世界身份 | 世界名、当前范围；只有需要行动的连接异常才提升为警告 | 常规元数据不占第二条主横幅；长名最多两行，完整名在可聚焦详情中 |
| Next Move | 一个主动作入口；同一区域说明 blocker、不可用原因或继续路径 | 默认进入 Command，不把示意按钮画成已授权直接执行；窄屏第一行 |
| 从属上下文 | Objective 与 Player Leverage；后者仅消费权威投影 | 窄屏第二行；不复制三张等权卡，不补造资源/收益 |
| 世界舞台 | 地形背景、合法位置、对象/关系、选择与目标 | 最大连续视觉区域；先移除装饰与次级标签，不先缩小关键文字 |
| 原行动反馈 | Action Receipt、原主体、结果/原因/下一步 | 默认紧邻舞台；保留短摘要及展开入口，不与 Feed 混同 |
| 导航 | World / Targets / Command；More 中的 Diagnostics | 导航始终可达；打开抽屉不生成新的任务或操作 |
| 世界动态 | 右上边缘折叠 World Feed，按需展开 | 不形成常驻第四栏；数字/动画不暗示玩家收益 |
| 只读工具 | 总览、返回目标、定位、退出跟随与合法尺度浏览 | 次于 Next Move；命名明确“查看/定位”，不使用“执行路线” |

默认状态不同时展开 Targets 和 Command。切换路线保留已授权选择、当前观察范围及未提交草稿；不将选择偷偷改写为 Command 的提交主体。打开非模态辅助内容不锁死世界；真正模态确认必须明确命名、管理焦点且不允许点击穿透。任何新弹层行为是后续实现目标，不能用本稿声称已有模态实现。

<a id="uiux-primary-journeys"></a>
### 10.2 主要旅程及状态转移

| 旅程 | 操作前提与入口 | 屏幕变化、焦点和反馈 | 失败/返回 |
| --- | --- | --- | --- |
| J1 首次读世界 | Player 打开；世界/会话由既有入口确定 | 世界身份与保留尺寸的舞台先出现；等待快照时显示“正在同步世界”；快照完成后显露单一 Next Move 和真实上下文 | 无快照、无权对象、空世界分别说明；连接异常不显示零库存或“世界不存在” |
| J2 找到对象 | Targets；搜索只过滤当前有权列表 | 打开抽屉，将焦点移至标题/首个合理控件；搜索结果给出范围与数量，选择行同步选中标记及只读检查 | 无匹配显示“当前筛选没有结果”及清除筛选；部分加载提示“仅显示已加载对象”，不声称全世界无对象 |
| J3 探索至返回 | 世界总览→区域/Fragment→对象；定位/跟随有可信位置 | 相机改变不提交动作；显示当前范围与可返回入口；手动浏览退出跟随，状态文案即时反映 | 目标失去位置时保持获准文本、停止不可信跟随；返回目标不暗换对象，无当前目标时解释原因 |
| J4 重叠/密集选择 | 同一可见位置存在多个合法候选 | 提供带名称、类型与必要区分信息的候选列表；键盘/触控可逐项选，不要求像素级点中 | 不用随机散点代替可选性；撤权候选立即移除；无候选时保留明确空态 |
| J5 从观察进入指导 | 自有且获准 Agent，Command 或 Next Move | Command/Chat 优先；显示提交主体与相关上下文；草稿、预览和生效分开；提交前最后一次明确主体 | 无权对象仍可按授权观察；不可提交时给原因和合法返回，不借用第一个 Agent；服务器重验失败保留可安全保留的草稿 |
| J6 等待并读原结果 | 原操作已经提交，有原操作身份 | 输入本地状态与世界确认分开；回执保持原主体；切到 B 时 A 的 pending 不改名；新建议不覆盖原操作结果 | 断连/超时显示“结果待确认”，入口为查询原结果；禁止以新身份自动再提交；已完成但无进展保留原因及恢复 |
| J7 查看世界动态 | 展开 World Feed | 明确“世界动态”，保留事件来源/时间与支持的关联；有合法事件锚点才允许地图定位 | gap/replay/reorg/unavailable 各自解释；没有 receipt_ref 不补造行动关联；无锚点仍可读事件 |
| J8 图形失败恢复 | 启动不可用或 ready 后失效 | 标明渲染器不可用及原因；停止旧图命中/动画；保留获准 Targets、原回执、文字与合法安全控制 | 显式重试；恢复后重验当前授权视图，不恢复旧私有画面，不重放世界动作；不绘制 JS 替代世界 |
| J9 诊断与返回 | More→Diagnostics；Director 另需服务器能力 | 默认诊断折叠；普通查看不授予 Director；成功/拒绝均保留明确返回 Player | stale/revoked/denied 立即清理受限信息；不能凭本地开关持久保留权限 |
| J10 退出、刷新与再次进入 | 退出先清理旧会话私有内容；再入沿 hosted_public_join 恢复当前身份及读取资格 | 按当前授权重新读取世界身份、当前目标、主要 blocker、最近可信原结果和下一决策；焦点回到可用的 Player 入口，不恢复旧私有画面 | 读取受阻则等待、重新认证或安全返回；原操作无法核对时仍显示结果未知，不从空回执推定未执行，不自动重发；不承诺跨会话草稿持久化 |

<a id="uiux-context-identity"></a>
### 10.3 选择、草稿与原操作的三种身份

界面必须能同时表达：**正在看 B**、**正在编辑发给 C 的草稿**、**正在等待 A 的原操作结果**。三者没有必要相同，不因导航或渲染排序自动合并。

- Targets 和舞台强调“已选中 B”；Command 标题明确“发送给 C”；Action Receipt 标明“A · 原操作”。只在确有身份时显示名称，没有 Agent 的玩家级结果保持玩家级，不填最近选中对象。
- 本人的个人行动反馈与其他玩家/匿名请求隔离；身份切换先清理旧身份私有回执与草稿，再读取当前身份可见结果。公共 Feed 不用作本人反馈来源。
- 更换草稿主体是显式动作；在有内容时保留原草稿或由用户选择丢弃/另存，不把同一内容静默应用到新主体。此设计不创建新的持久化承诺。
- 预览展示将受影响对象、现有合同提供的资源/权限条件及不确定性。预览不等于获准；服务器拒绝时说明条件已变化，而非显示成功后再撤回。
- 新世界快照可更新当前观察；不可更改原操作身份。授权收窄应移除私有名称/详情，同时按当前权限保留允许披露的原结果说明。
- Chat 请求与执行世界 action 分别按其现有授权和可用性判断；没有可执行的世界动作不自动禁用仍获准的 Chat。显式发送只报告聊天请求结果，不展示为世界执行成功。
- pending 状态选择 B 不自动开回 A、不抢焦点；回执更新采用克制的状态提示。只有用户点击“查看原对象”才导航，且再次确认当前可见性。
- “撤销编辑”只撤本地未提交变更；“停止/撤销已提交操作”只有既有服务器合同支持时才出现。不得承诺通用撤销、退款或世界回滚。

<a id="uiux-responsive"></a>
### 10.4 响应式和可用空间

本表是交接设计与验证样本，不重写产品支持声明或现有 CSS 断点。

| 样本 | 布局策略 | 不可牺牲项 |
| --- | --- | --- |
| 1440×900 / 1280×720 桌面 | 全幅舞台、沿现行 shell 底部位置的紧凑 Next Move、安静边缘导航；按需侧向抽屉 | 主体/原回执/关键恢复可见；侧抽屉不得成为常驻三栏 |
| 1024×768 平板 | 单舞台；抽屉覆盖而非将世界挤成窄列；控件和文本可换行 | 不把 tablet 当桌面缩略图；触控无需 hover |
| 390×844 窄屏 | World/Targets/Command 可达；Next Move 第一行、上下文第二行；一次展示一个主要路线 | 回执短摘要和恢复入口不藏在长页面末端；水平无溢出 |
| 844×390 低高度/横屏 | 压缩身份与次要上下文，不压缩主控件；展开内容独立滚动，保留关闭/返回 | 键盘弹出后输入、发送、错误与关闭可达；不将舞台最小高硬塞出屏幕 |
| 文本放大/长 CJK | 按内容换行，允许面板纵向滚动；尽量不截断核心状态 | 不缩小文本来适配；完整名称/原因有键盘可达路线 |
| 移动软键盘 | 使用可见视口安排 Command 输入和当前编辑行；滚动目标为输入上下文 | 不让粘性 rail 遮住输入或发送；键盘关闭恢复可预测位置 |

滚动只作用于可理解的区域，避免三个嵌套滚动容器；面板上下边界可识别。安全区、浏览器缩放和软键盘出现时重新核验可用高度。切屏或旋转保留合法选择/草稿，不把 resize 当用户提交。

<a id="uiux-input-access"></a>
### 10.5 输入、焦点与可访问性

- 每个图标控件有可见或可访问名称；有状态者暴露当前选中/展开/禁用含义。不能只用颜色或 tooltip 区分 selected、pending、blocked。
- 键盘顺序按当前活动路线：标题/关闭→主要内容→动作→辅助说明。关闭恢复 invoker；invoker 消失则返回对应导航或舞台标题。焦点环不能被容器裁切或低对比边框吞掉。
- Escape 先关闭最内层 surface，再退出 Focus；compositionstart 至 compositionend 期间及 keyCode 229 的兼容输入不触发发送或关闭。Enter / Shift+Enter 按多行编辑处理，均不发送；只有显式 Send Chat 提交。组合输入与连续键入不得重复插入或提交。
- 舞台快捷键只在舞台适用焦点内响应；文本输入、contenteditable、浏览器快捷键和 IME 不被截获。控件 disabled 的原因紧邻显示，不能要求点击 disabled 才知道原因。
- 指针事件归属沿 [Chat 输入验收](viewer-manual.manual.md#当前-agent-chat-与高级-prompt-设置)：输入区、Targets/Command 抽屉及控件内的点击、拖拽、滚轮不能穿透到世界相机或拾取，正文仍可选择复制。舞台平移拖动与点击选择必须区分，拖动结束不再追加一次选择；指针取消或捕获丢失后结束本次手势，不沿用旧拖动状态。后续验证覆盖鼠标、触控、抽屉滚动、拖出边界与取消后再次点击；本稿不设新的手势像素阈值，也不声明当前实现已通过。
- 拖动/双指缩放不是唯一操作：提供等价的缩放、总览、定位、退出跟随和候选选择。触控目标优先 44×44 CSS px 的可用点击区，相邻控件有间隔；小符号可扩大命中但不伪装物理面积。
- 普通文字按 4.5:1、大字按 3:1、重要图形边界/焦点按 3:1 检查设计配色；这些是验收目标，不是本稿的认证。颜色状态同时带短文本、图形或位置。
- 世界 canvas 的可访问替代为同一授权模型的 Targets/只读检查与原回执，不再画独立语义世界。动态加载只通报相关状态变化，不逐帧播报所有 Agent。
- 减少动态效果时保留状态、方向与结果意义；取消装饰性脉冲/镜头飞行，不取消必要反馈。关键提示不依赖短时闪烁。
- 关键回执、blocker、权限错误不自动消失；短暂 toast 只能重复已有可恢复信息，不能成为唯一证据。
- 双语文案和不受控名称按纯文本处理；不让世界名称、Agent 名或日志成为 HTML。超长字符串必须断行并保留完整读取路径。

<a id="uiux-bevy-layer-sheet"></a>
### 10.6 Bevy 像素世界的视觉与交互层

详细符号、尺寸、图层、LOD、材质与密集场景交接见[玩家可读渲染设计 §13](viewer-pixel-world-player-readable-rendering.design.md#bevy-detailed-visual-handoff)；本节只维护与页面阅读顺序的接口。

| 层 | 视觉表达 | 显示条件/语义限制 |
| --- | --- | --- |
| Fragment 地形 | 低对比、有层次的背景块；远景减少细节 | LOD 隐藏不代表地形删除；非可采集/建造编辑格 |
| Location | 稳定逻辑锚点、必要名称与选中边框 | 地点坐标与 Agent 精确位置不互相替代；角框/halo 仅为可读性 |
| 设施/产业聚合 | 以形状、身份和支持的数量表达 | 只汇总获准数据；聚合不是全世界总数；可下钻不等于可操作 |
| Agent | 比背景更高对比的符号和必要名字 | 无位置保留列表/文本，不造坐标；派生位置同时说明来源 |
| 路线/关系 | 低噪声方向/端点表达，选中关系加强 | 仅独立当前权威完整时显示；没有确认关系允许无线，邻近不等于分配 |
| 当前选择 | 单一可识别边框/指示，不改变世界结果 | 与推荐目标、原回执对象使用不同文字/符号；选择不授予权限 |
| blocker/原结果 | 优先级高但范围克制，关联真实对象时才定位 | 无授权锚点用文字；不把 ambient 动画当成功反馈 |

缩小时优先退背景细节、次级名称及非相关关系，保留当前任务、选择、blocker与原结果入口。保留关键对象不意味着显示已撤权数据。密集场景应有候选列表、计数范围和当前过滤提示；列表排序变化不能偷偷替换当前选择。

相机跟随、手动浏览、返回目标是独立可见状态；持续数据刷新不能不断拉回用户视点。缺当前位置时“定位”解释为何不可用，其他获准只读检查继续工作。当前/最近已知/未知/冲突、精确/派生、完整/部分覆盖是不同维度，不压成一个“正常/异常”颜色。

<a id="uiux-state-feedback"></a>
### 10.7 状态、反馈时序与恢复

| 状态 | 主文案示例 | 允许动作 / 禁止误读 |
| --- | --- | --- |
| 等待快照 | 正在同步世界 / Syncing world | 保留空间、显示连接恢复；不是空世界 |
| 无权对象 | 当前账号没有可查看对象 / No objects available to this account | 返回合法入口；不泄漏对象数量或名称 |
| 筛选为空 | 当前筛选没有结果 / No matches in this filter | 清除筛选；不是全世界无对象 |
| 部分覆盖 | 仅显示已加载范围 / Showing loaded scope only | 查询/继续加载；不能当完整删除清单 |
| 最近已知 | 最近已知，等待确认 / Last known; awaiting confirmation | 来源和适用范围可读；不是 current |
| 冲突 | 信息冲突，需要重新确认 / Conflicting information; recheck needed | 刷新/核查；不靠最后到达选择真值 |
| 草稿/预览 | 尚未提交 / Not submitted | 编辑、返回；不显示 accepted |
| 已受理 | 等待世界确认 / Waiting for world confirmation | 查看原操作；不是完成 |
| 无回执 | 尚无行动回执 / No action receipt yet | 保留明确空态；Feed 不补位 |
| 结果不确定 | 结果待确认 / Outcome not yet confirmed | 查询原结果；不是失败重做许可 |
| 已完成无进展 | 已完成但没有推进 / Completed with no progress | 解释原因和下一步；不用成功庆祝动画 |
| 被拒绝/受阻 | 未执行：条件已变化 / Not applied: conditions changed | 重新读取/预览合法方案；不自动批准新方案 |
| 已确认结果 | 已完成：{权威结果摘要} / Completed: {confirmed result} | 原主体和影响保持可核对；不外推未来收益 |
| 渲染不可用 | 渲染器不可用 / Renderer Unavailable | 合法文本、原结果和显式重试；不是世界不可用 |
| 权限撤销 | 访问权限已变化 / Access has changed | 清理受限内容并提供返回；不将私有快照留作 last-known |

点击、按键和选择应在本地下一次可用绘制给出可感知反馈；它只证明输入处理，不代表服务器接受。网络/世界等待采用明确 pending，而非虚构百分比。沿用 `--motion-feedback-fast` 的 140ms 轻量交互过渡；状态更新本身不等待动画完成。长等待提供仍在等待的原因、原操作查询及已有合同允许的控制，不用任意客户端超时判世界失败。

刷新合并次级视觉更新，避免列表跳动、焦点丢失和重复播报；关键权限/结果变化不可为动画节流而延迟。性能预算、容量阈值及平台承诺仍由相关质量权威定义，本稿不新增 FPS/延迟 SLA。视觉检验应同时观察输入响应、关键文字稳定性和 dense 场景，不以平均 FPS 代替交互可用性。

<a id="uiux-design-sheets"></a>
### 10.8 设计稿索引与对比方案

全部文件位于 `assets/ui-ux/`，页眉标明 DESIGN TARGET / 设计目标。完整画面稿标注视口；组件、状态与图层解剖稿不代表完整视口。表中区分画面与说明性标注，文档条款提供对应语义。图内世界与数据仅为设计示例，不用于 renderer 真实性验证。

| 文件 | 设计范围 | 文档条款 |
| --- | --- | --- |
| [01-desktop-world-first.svg](assets/ui-ux/01-desktop-world-first.svg) | 1440×900 默认 Player；主动作/原回执/折叠 Feed | [分区](#uiux-screen-regions) |
| [02-narrow-world-targets.svg](assets/ui-ux/02-narrow-world-targets.svg) | 390×844 世界和 Targets 路线 | [响应式](#uiux-responsive) |
| [03-panels-ime-focus.svg](assets/ui-ux/03-panels-ime-focus.svg) | 独立面板状态、Chat/IME/焦点解剖，不是同时展开布局 | [输入](#uiux-input-access) |
| [04-receipts-selection-identity.svg](assets/ui-ux/04-receipts-selection-identity.svg) | 原主体/观察/草稿分离及回执状态 | [身份](#uiux-context-identity) |
| [05-bevy-layers-lod-provenance.svg](assets/ui-ux/05-bevy-layers-lod-provenance.svg) | 图层、LOD 与来源的示意及标注；未绘制完整密集/未知状态变体 | [Bevy 层](#uiux-bevy-layer-sheet) |
| [06-renderer-recovery-privacy.svg](assets/ui-ux/06-renderer-recovery-privacy.svg) | 启动/运行中失败、授权清理、合法文本恢复 | [恢复](#uiux-state-feedback) |
| [07-low-height-layout.svg](assets/ui-ux/07-low-height-layout.svg) | 1024×600 单一低高度画面构图；不含抽屉展开或方案对比 | [响应式](#uiux-responsive) |
| [08-tokens-components-focus.svg](assets/ui-ux/08-tokens-components-focus.svg) | token/组件状态、文本与焦点 | [输入](#uiux-input-access) |
| [09-account-passkey-return-recovery.svg](assets/ui-ux/09-account-passkey-return-recovery.svg) | Passkey/账号/准入/控制分层及恢复；状态画板 | [账号与世界](#uiux-entry-session-world) |
| [10-target-search-binding-readonly.svg](assets/ui-ux/10-target-search-binding-readonly.svg) | 原绑定、Targets查找/只读、首读与重入；互斥场景 | [首次与重入](#uiux-first-return-targets) |
| [11-byok-consent-budget-stop-forget.svg](assets/ui-ux/11-byok-consent-budget-stop-forget.svg) | 任务/执行身份/模型/开始分开；费用/存储/停止忘记 | [模型连接](#uiux-byok-control-flow) |
| [12-command-quote-original-receipt.svg](assets/ui-ux/12-command-quote-original-receipt.svg) | 只读报价、条件性确认说明与原结果未知；Chat沿原03/04，不新增提交能力 | [Command](#uiux-command-form-result) |
| [13-live-playback-feed-return.svg](assets/ui-ux/13-live-playback-feed-return.svg) | Live、离线Playback、Feed追平；非新模式选择器 | [模式语义](#uiux-live-playback-context) |
| [14-more-help-text-director-gate.svg](assets/ui-ux/14-more-help-text-director-gate.svg) | 语言/帮助/Prompt/Director次级披露及安全返回；不新增设置中心 | [次级工具](#uiux-secondary-help-privacy) |

09–14 扩展既有 Viewer 外围旅程；多帧/多栏均为互斥状态与说明，不是运行时常驻布局。01–08 继续提供世界舞台、输入、图层和组件母版。设计补充不代签能力开关、真实设备/供应商、权限竞态、浏览器可访问性或玩家理解；覆盖与真实证据分工见[整体状态矩阵](#uiux-whole-viewer-state-matrix)。

后续若调整按需上下文占位，应比较侧向与下沿抽屉，保持 Player/Director、操作权限和信息真值不变。本轮未交付该对比图。桌面侧抽屉有利于保留纵向舞台与回执阅读；下沿抽屉适合横向长回执但低高度占用较大；窄屏保留单路线覆盖。取舍和最终尺寸须由真实候选验证，不将此分析写成已完成比较或第二默认。密集候选及未知位置的完整画面变体仍须后续按图层条款与专项手册验证。


<a id="uiux-module-coverage"></a>
### 10.9 模块覆盖与不变边界

未画出的外围模块不代表删除或新增能力；下列入口保留现有 authority。

| 模块/状态 | 本次呈现与交互约定 | 权威与未覆盖范围 |
| --- | --- | --- |
| Web 入口/兼容 alias | viewer 为正式入口；兼容入口不形成第二品牌/世界；当前世界与会话身份清楚 | [Web entry](viewer-web-entry-compatibility.prd.md)；不新增世界选择器或 URL/权限合同 |
| 登录/退出与账号安全 | hosted_public_join 依当前 capability 呈现推荐 Passkey、可选邮箱与受限导入；账号/准入/控制分开，退出和撤权清理旧私有内容 | [账号与世界](#uiux-entry-session-world)、[多凭证 authority](../../p2p/blockchain/hosted-public-join-key-based-onboarding.design.md#viewer)；不新增账号协议、准入或资产权限 |
| 本地/正式世界 | 本地 standalone/provider test、observer/debug和正式共享世界不混淆；本地成果不宣传为正式成果 | [本地测试](viewer-manual.manual.md#本地真实-llm-游戏测试)、[证据边界](viewer-manual.manual.md#证据边界)；不恢复 retired trusted_local_only 用户流程 |
| QA 专用登录 | 不出现在普通 Player 稿；现有 QA 入口保持限制 | [页面能力](viewer-manual.manual.md#页面能力)；不绕过 loopback、显式启用或生产账号验证 |
| 认领/绑定/只读权限 | 当前无控制权时区分待登记、原绑定待确认和只读；已有绑定复用，未知/owner mismatch不替换Agent | [首次与重入](#uiux-first-return-targets)、[Chat/Prompt](viewer-manual.manual.md#当前-agent-chat-与高级-prompt-设置)；不改变身份/认领/任务authority |
| Agent Context/Details | Command/chat 优先，只读上下文按需展开；观察、推荐、原结果分开 | [模块设计](#3-module-design)；不新增资源、生产事实或完整产业面板 |
| Chat/provider | 多行草稿与显式 Send Chat；provider 不可达、未配置、权限与原结果未知分开 | [Command](#uiux-command-form-result)；Chat结果不证明世界执行，不借模型Key授予游戏权限 |
| 浏览器模型连接 | 核对原绑定/任务→授权执行身份→保存Key→明确付费检查→确认开始；停止/忘记/只读原件恢复分开 | [模型连接](#uiux-byok-control-flow)、[BYOK authority](../llm/self-supplied-model-connection.design.md#des-byok-001)；不宣称OpenRouter、后台续跑、供应商账单封顶或迁移全量完成 |
| Advanced Prompt Settings | operator-level 默认收起；override、preview、apply、rollback与反馈分开 | [Chat/Prompt](viewer-manual.manual.md#当前-agent-chat-与高级-prompt-设置)；rollback仅为既有Prompt能力，不是世界回滚；strong_auth由后端决定 |
| 报价/玩法表单 | 真实存在的情境表单留在Command；字段错误就地说明，保留安全输入与主体；Quote不是主导航 | [控制面](viewer-control-plane-split-live-playback.prd.md)；不新增资产转账或未发布玩法 |
| 连接/重放 | 连接恢复不等于世界结果；Feed gap/replay/reorg分开；恢复查询原操作 | [状态恢复](#uiux-state-feedback)、[连续性](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；不建立新幂等协议 |
| 语言/复制/文本缩放 | en/zh切换、正文选择复制、浏览器放大不丢主动作 | [页面能力](viewer-manual.manual.md#页面能力)、[输入](#uiux-input-access)；未确认应用独立字号设置，不画新设置页 |
| 音量/音效/通用偏好 | 不新增控件或声音成功信号；减少动态效果不依赖声音替代 | [视觉规范](viewer-visual-design-spec-2026-06-05.design.md#12-accessibility-and-robustness)；已审读authority未确认音量/音效设置，属于未覆盖，不宣称不存在 |
| Director/Diagnostics | 默认Player，诊断次级，Director能力受限；拒绝/失效安全返回 | [shell](viewer-gameplay-release-experience-overhaul.prd.md#current-implementation-boundary)；不增加issuer或生产成功路径 |
| 原生/发布 | 概念路径可复用，实际逐平台验证 | [原生权威](external-agent-world-view.design.md#view-native)；本次不提供原生实现、发行或支持认证 |


<a id="uiux-entry-session-world"></a>
#### 10.9.1 账号入口 世界确认与安全恢复

玩家首先确认“正在使用哪个入口、当前账号是否登录、是否能进入这个世界”，而不是一次读完认证和底层协议。入口延续 [canonical Viewer](viewer-web-entry-compatibility.prd.md#41-entry-contract)，`software_safe` 仅是兼容 alias，不作为第二个世界或版本选项。世界由当前入口和权威配置确定；画面显示已确认的世界名称、必要环境标签及当前状态，不新增世界目录、服务器选择器、新建世界或跨世界迁移。离线示例、本地观察与正式共享世界不得混用品牌承诺。

账号表面承接[多凭证账号 PRD](../../p2p/blockchain/hosted-public-join-key-based-onboarding.prd.md#prd-keyauth-001)和[Viewer 身份状态](../../p2p/blockchain/hosted-public-join-key-based-onboarding.design.md#viewer)。部署启用 identity v2 时，支持环境推荐通行密钥，邮箱为可选通道；新注册是否开放、其他凭证是否可用都由当前 capability 决定。注册关闭不关闭已有账号登录；邮箱不可用不伪装为所有登录不可用。legacy 邮箱路径只在专业合同允许的兼容部署存在，不能作为当前唯一产品默认。

主入口按当前任务突出“使用通行密钥登录”或已开放的“免邮箱创建账号”，不自动唤起系统认证器。用户主动选择后，原按钮附近说明当前阶段；账号创建、账号 session、世界准入、runtime 控制绑定分别显示，不共用一个绿色“完成”。账号已创建而世界受阻时保留账号，下一步沿既有入场流程，不要求重新注册。`registration_pending` 不等于 `bound`；连接成功或看到世界也不等于获得控制权。当前 surface 仅在 capabilities 返回 HTTP 404 时显示 legacy 表单；其他错误保持能力检查失败，不能静默降级。

| 场景 | 玩家需要看见 | 合法恢复与禁止误读 |
| --- | --- | --- |
| 浏览器/认证器不支持 | 说明当前设备限制，不猜测具体系统原因 | 仅提供确实可用的其他登录方式/受支持浏览器指引；不降级保存明文长期私钥 |
| 系统确认取消或无法完成 | 回到原表面，焦点返回发起按钮；没有完成的阶段仍未完成 | 能否新建尝试按原仪式结果决定；超时或取消响应未知先核对原操作，不自动新建账号 |
| 已登录，世界未绑定 | “账号已登录，等待入场”；世界状态与账号状态分开 | 可继续既有入场；不宣称聊天/控制已开放 |
| 其他页面/设备已控制 | 指明已有控制会话，但不猜硬件身份 | 保留原控制或显式确认接管；第二设备仅登录不自动抢占 |
| 凭证/会话列表读取失败 | 列表局部失败，账号 session 单独核对 | 管理列表失败不把已登录误写为登出；session 明确失效时清理旧私有内容 |
| 原身份缺失或不匹配 | 已停止；需要恢复原玩家身份 | 不重建玩家、替换 Agent、迁移资产 signer 或凭本地名称合并账号 |

“账号与安全”按登录方式、已登录会话、恢复方式分组。凭证可能同步到多设备，不能把撤销凭证称作“退出这台设备”；退出单个 session 也不等于删除凭证。撤销影响、最后有效凭证保护和新鲜重新认证应在动作前可读。恢复仍沿[固定候选、等待和原子替换](../../p2p/blockchain/hosted-public-join-key-based-onboarding.design.md#recovery)的既有 authority；倒计时结束不画成自动恢复，邮箱恢复不授予资产密钥重置权限，不承诺客服无条件找回。

[已绑定密钥的网页导入](../../p2p/blockchain/hosted-public-join-key-based-onboarding.prd.md#imported-key-login)保持默认收起，展开前说明仅本页内存使用、不是 Passkey 导入、不要输入钱包或节点私钥。公私钥输入提交后清空；截图仅显示空字段或掩码，不能展示真实材料。此入口不延伸为本地密钥生成、加密备份或账号自动创建。认证反馈只使用安全阶段和诊断编号，不把原始异常、Cookie、OTP 或完整响应写进 UI。

<a id="uiux-first-return-targets"></a>
#### 10.9.2 从首次观察到再次进入

首次世界画面继续按 §10.1 的五个问题组织，不新增强制教学向导：世界/阶段、当前目标或 blocker、唯一 Next Move、原行动回执、可达的辅助路线。世界尚未同步时占位保留尺寸，显示“正在同步世界”；这不是空世界、零资源或无 Agent 的证明。已登录但尚无可控制 Agent 时，只读观察按授权保留；沿当前入口说明是待登记、绑定待确认或无控制权，不在 UI 任意借用第一个 Agent。

**首次入口准备与控制分开。** 浏览器 Game Client 的既有表面先只读核对账号原绑定；仅在明确确认及当前 authority 允许时准备缺失的玩家、Agent、Task 或目标，已有绑定和已有任务目标保持不变。世界确认了实际 Agent 但 Viewer 投影尚未同步时，分别显示“实际绑定已确认”和“画面仍待同步”；不得为消除等待创建第二个 Agent。原件 pending、history unavailable 或 owner mismatch 时只提供受支持的核对/停止/恢复说明，不继续创建。

**Targets 查找不改变世界。** Search 仅过滤当前可见的权威列表，显示搜索范围及真实匹配数量；它不裁剪舞台实体、不执行动作，也不声称全世界检索。以下空态分别表达：未取得快照、授权列表为空、当前筛选无匹配、仅有部分覆盖。筛选无匹配提供“清除筛选”；不显示隐藏对象数量。列表重排不偷偷改选；选中行、舞台标记和只读详情同步显示“已选中”，能否指导另由权限解释。

详情先回答“它是什么、当前状态、与当前目标有什么已知关系、我有什么获准下一步”，再按需展开来源/细节。现有 Agent Context 是 Agent 范围；其他实体仅展示已有获准字段，不借全局稿宣告通用 Inspector 或全产业面板已完成。无可靠位置仍可查看获准文本，但定位明确不可用；不以虚构坐标填空。地图点击只选择/查看，不出现移动、采集、建造命令光标。

**再次进入先恢复阅读链，不恢复旧权限。** 沿 §10.2 J10，重新核对当前账号、世界、目标、主要 blocker、最近可信原结果与下一步；本地旧画面或最近选中不能替代权威读取。当前账号切换先清理旧账号私有字段和可安全清理的草稿；不能把 P1 的 pending 贴到 P2 身上。原游戏请求按[原请求回读](viewer-control-plane-split-live-playback.design.md#gameplay-original-result-lookup)恢复，查询错误不是原操作 rejected；无相应协议能力或权限时保持未知。可靠记录证明 `not_sent` 的原件才可按既有合同明确继续同一原件，可能已发送的原件不能换 nonce/主体/目标重提。

首产物、后继候选和可执行下一动作依[当前方案交互](viewer-gameplay-release-experience-overhaul.design.md#starter-plan-interaction)分别表达。已确认成果引用唯一 Action Receipt；不强制领奖、不重新庆祝旧回执、不生成离线收益。用户可以自然离场；若当前页面承载模型执行，离场影响按下一节说明，世界已受理作业的继续不被写成页面在后台代跑。

<a id="uiux-byok-control-flow"></a>
#### 10.9.3 模型连接 执行授权与停止忘记

模型连接属于 Command 的上下文能力，不是第四项 Dock 或独立管理后台。本节只承接[网页自带模型产品要求](../../product/agents-world-simulation/self-supplied-model-connection.prd.md)及[客户端侧系统设计](../llm/self-supplied-model-connection.design.md#des-byok-001)。`BrowserByokSurface` 和 `BrowserModelConnectionPanel` 是当前源码接缝；它们的存在不证明部署已开启、实际供应商可用或整条游戏链已验收。

沿当前表面使用以下步骤，始终把模型状态、执行方权限和世界结果分开：

1. **核对入口原绑定。** 先确认账号和本机原玩家身份、实际 Agent/Task/目标；可明确停止入口准备。停止后重开只读核对，不自动补发可能已发送的原件。
2. **核对并登记执行身份。** 先只读查看实际任务，再由玩家明确授权当前浏览器；已有其他执行方时需明确切换。登录成功、选中 Agent 或填入模型 Key 都不替代 executor 授权。
3. **保存模型 Key。** 执行前置缺失时不收 Key。保存前在主阅读层说明供应商接收方、当前浏览器执行、本站 localStorage 保存和同源脚本可读风险。按游戏账号命名空间使用，不宣称这是加密隔离；Key 不上传 Oasis7 后端，刷新/关页后仍保留，保存不自动调用模型。
4. **单独确认有限付费检查。** 采用当前预设与限制，费用由供应商账号承担。当前源码的浏览器组合为 DeepSeek 官方 `deepseek-flash`，OpenRouter 明确尚未支持；产品目标中的其他供应商/协议不得画成可用下拉选项。检查成功只显示“工具连接检查通过，尚未开始 Agent”，不填入世界成功回执。
5. **单独确认开始。** 玩家复核实际 Agent、Task、目标、执行方与当前运行限制；检查和运行预算如实来自当前配置。没有模型检查、有效任务或授权就解释 blocker，不保留看似可点的“开始”。不得把设计示例限额写成永久产品 SLA 或供应商账单硬封顶。
6. **停止、忘记与核对原件。** Stop/Forget 在 pending/运行态保持可达；停止新决策保留 Key，断开并忘记停止后删除当前账号本站 Key。两者都不撤销供应商处 Key、不保证已发调用退款、不取消已受理世界作业。删除失败明确“已停止，但本机 Key 尚未删除”，提供既有清站点数据指引，不伪报彻底清除；未决操作的非敏感恢复索引不因此丢弃。

模型 Key、页内导入的登录私钥、短期世界控制恢复材料与 executor identity 使用不同存储合同，UI 不能笼统写“所有密钥只在内存”或“所有密钥都保存本地”。必要隐私说明放在对应输入前，不能藏在 Diagnostics。Key 字段默认密码样式，提交后清空输入；安全说明不靠 hover。

隐藏、断网、休眠、刷新或关闭后不承诺后台持续推理；返回先核对原件并另行明确开始。旧模型输出、旧页面和旧账号回调不能取得新提交资格。预算不足、网络/CORS、供应商认证、存储/锁、任务 fence 与旧托管迁移阻断分别说明，不能一概要求“重新连接”。未知检查用量不能显示为零，也不能用换 Key/重开页重置预算；未知世界原件不以新模型或重新开始来替代。

<a id="uiux-command-form-result"></a>
#### 10.9.4 Command 表单与原结果

Command 仍以获准 Chat/高层指导为主，所选上下文、Gameplay Details、Advanced Prompt Settings 和诊断渐进披露；Quote 只属于此情境，不增加导航。§10.3 的观察主体、草稿主体、原操作主体三种身份贯穿所有表单：正在看 B 不将 A 的草稿和原请求改为 B；改变草稿收件人必须显式。多行 Chat 使用明确的 Send Chat，Enter/Shift+Enter 均不提交；IME 合成不触发发送、快捷键或关闭。

情境表单沿[当前方案交互](viewer-gameplay-release-experience-overhaul.design.md#starter-plan-interaction)显示对象、实际方案、主要成本/阻塞、下一动作和可展开依据。世界修改字段只有在既有真实提交能力下才开放；独立只读预估查询参数仍可编辑，但必须明确不会修改 Next Move 或提交世界动作。报价和 action 各有等待状态；quote 错误不清 action pending，普通 snapshot 不无条件清已收到报价。相关条件变化后旧报价可说明缘由但失去执行资格，不能由旧 `allowed` 恢复按钮。未知成本显示“未确认”而非 0。

当前不同 Quote 的可执行性不能合并：`schedule_recipe` / `transfer_material` 报价是只读估算，不预留资源、不构成 accepted 或 Action Receipt；当前宣战提交禁用、governance vote 的 `submission_allowed=false` 必须保持。不得为了完成统一表单旅程，把这些报价画成已开放的提交。

确认不是新的通用确认 API。若既有动作本来要求确认，确认内容直接复述本次真实对象、方案、主要代价和权限影响；不增加同一动作的第二份提交，不通过局部确认绕过服务端重验。预览/报价读取、提交受理、runtime 应用和领域结果在文案及样式上分开。确认后变化了条件就解释并重读合法方案，不能悄悄批准替代方案。

| 表单或结果状态 | UI 主任务 | 原结果与返回边界 |
| --- | --- | --- |
| 草稿/预览 | 可编辑安全字段，显示“尚未提交” | 返回只退出编辑；没有世界回执就保持空态 |
| 注册/签名中 | 解释尚未发送阶段，防重复触发 | 不被旧 ACK/error 终结，也不能称为世界受理 |
| 已发出/等待结果 | 保留原操作和允许的核对入口 | 用户可看其他目标；关闭不取消已提交动作 |
| 明确受阻/拒绝/失效 | 显示权威原因、适用下一步 | 字段错误就地提示；权限变化按当前资格脱敏 |
| 结果未知 | 核对同一原件，说明范围 | 超时不是失败；查询失败不是原动作被拒绝；无安全查询时保持未知 |
| 已确认/部分成果/无进展 | 展示原主体、真实影响及尚待决范围 | 引用唯一 Action Receipt；Feed、模型解释、动画和选中变化不能补因果 |

Advanced Prompt Settings 属于 operator-level 次级能力；Preview、Apply、Prompt rollback 分开，rollback 不能命名为世界倒带。增强 Prompt 的 `applied` 只消费[权威应用 scope](viewer-control-plane-split-live-playback.prd.md#prompt-control-application-scope)，不能推导持久保存、跨会话同步或完整世界目标完成。stale、control_lost 与 hidden result 按[结果可见性](viewer-control-plane-split-live-playback.prd.md#prompt-control-control-loss-precedence)展示；控制丢失时旧草稿只能保持未提交，不得提交、重放或转移到其他主体，错误区和诊断不补露已隐藏 profile/digest。退出 Prompt 或 Director 保留安全返回路径，不复活旧 grant，不自动应用草稿。

<a id="uiux-live-playback-context"></a>
#### 10.9.5 Live 离线回放与动态追平

三个容易混淆的“回放”语境必须分开；它们不是新增模式选择器或三项主导航。

| 当前语境 | 来源与可见表达 | 控制与误读边界 |
| --- | --- | --- |
| Live 世界 | profile 以 `HelloAck.control_profile` 为准，观察时间/事件序列只由权威消息更新 | 世界单调向前；不提供 Seek/seek_event。unsupported action 解释为模式不支持，不称作断线或建议重连重试 |
| 离线 Playback | 明确“离线回放示例”，来源是既有完整 snapshot/journal 数据对 | 底层 playback 的 play/pause/step/seek 按专业合同成立；当前 Web 不因此新增 Seek 控件。离线 demo 不证明正式 live 可玩 |
| World Feed 补齐历史 | Live 中的 ambient feed 可处于 replay/gap/reorg/unavailable | 只在补齐世界动态，不改变世界控制 profile；空增量不抹除已有事件，不伪报世界回退或我的行动完成 |

具体动作及兼容行为沿[控制面当前合同](viewer-control-plane-split-live-playback.prd.md#当前合同)；离线输入沿[最小 Demo](viewer-minimal-system.design.md#demo-flow)。普通 Player 不为展示底层协议而增加暂停世界、跳时或回滚按钮；已有控件仍受当前表面能力和权限约束。profile 尚未确认时显示“控制模式待确认”，不靠 URL、本地标签或视觉样式赋予能力。

回放/seek 后的 Step 只按实际输出事件及原请求 completion 解释进展，允许逻辑时间差为零；UI 不通过动画或自增 tick 制造推进。相关实现与验收继续由控制面维护，本节只把语义分界落实到标签、提示和可发现性。

<a id="uiux-secondary-help-privacy"></a>
#### 10.9.6 次级设置 帮助 隐私与可访问性

完整 Viewer 体验不等于增加一个设置中心。语言切换、账号与安全、上下文帮助、模型说明、Advanced Prompt Settings、Diagnostics 和受限 Director 沿各自既有表面组织；主 Dock 仍只有 World / Targets / Command。独立字号、音量、主题、账号删除、通用数据导出或全局隐私模式未由本轮核实为既有能力，不画成可用功能，也不以未画出断言仓库不存在。

帮助内容就地解决当下疑问：点选是查看；Command 是高层指导；为什么不可提交；保存/检查/开始有什么不同；怎样核对原结果；停止与忘记分别做什么。帮助不把 raw JSON 搬到首屏，不用空泛“出错了”代替阶段和恢复。现有入口未承载完整帮助面板时，只交付说明文案和布局目标，不声明新的 Help 页面已实现。

语言切换、窄屏重排、浏览器缩放和减少动态效果遵循 §10.4–10.5；保留合法选择、原操作身份和安全草稿。关键状态使用文字+符号，等待与未知不使用完成勾。文字放大时允许信息重排和纵向滚动，输入、错误、主行动、关闭以及恢复入口不被 Dock/软键盘遮住；不能靠缩小字号或只剩 icon 解决。系统认证弹窗返回焦点应回到原入口或当前阶段控件；非模态状态、回执更新不抢焦点。

隐私清理以当前身份/可见范围为准，而不以“用户看过”作为保留理由。注销、换号、撤权、Director 失效与目标失去访问资格时，清理旧私有详情、受限 Prompt 值和无权诊断；可披露的未知结果按原身份保留安全说明。Screenshot/复制/诊断不得包含 Key、OTP、Cookie、批准码、原始标准认证响应或受限 trace。复制功能的设计只针对已获准的正文与脱敏诊断，不新增自动上传支持工单或外部发送。

Director 仍是次级入口、显式选择、短时服务端能力；没有可信 issuer 时展示 `capability_blocked` 的真实含义。退出或能力失效返回 Player、恢复合理焦点并清理受限内容，不把 Director 成功进入画成默认现状，也不把它与账号登录、Agent 所有权或模型 Key 混为一项权限。

<a id="uiux-whole-viewer-state-matrix"></a>
#### 10.9.7 屏幕状态覆盖与验证分工

以下是补充设计覆盖，不是已运行的测试结果。表中图号与文字条款共同承接状态要求，不表示每个状态都有独立完整画板；09–14 均为 1600×1200 说明页，实际画面及旁注范围见[设计稿索引](#uiux-design-sheets)。Chat/IME 仍由原03/04和本文承接，12不新增独立Chat画板；200%文字、软键盘与真实焦点仍是待验证状态。状态按身份/权限、连接、资料新鲜度、renderer、原操作结果正交描述；例如世界连接可用、renderer 不可用、获准文本可读、原结果未知可以同时成立，不能压成一个“离线”。当多个 blocker 共存，主提示解释当前任务最先需要解决的条件，其他独立状态仍可查；不提供通用“一键修复”。

| 屏幕或旅程 | 必须覆盖的状态 | 图示 / 文字承接 | 后续真实证据 |
| --- | --- | --- | --- |
| 入口/认证 | 注册关闭但可登录、认证器不支持、取消或未知、账号已建但未入场 | 09；§10.9.1 | hosted v2 同候选真实设备、键盘/读屏与原尝试恢复；不以虚拟认证器代签 |
| 世界控制/绑定 | 未绑定、registration pending、bound、其他设备控制、owner mismatch、投影待同步 | 09/10；§10.9.1–2 | 当前账号/世界的权威绑定与可见状态对账；旧 release 不影响新控制 |
| 世界首屏 | 等待快照、空世界/授权为空、默认首读、renderer unavailable | 原01/02/06；10 | 同候选 GPU/浏览器与授权数据；原稿不替代实际截图 |
| Targets/详情 | 未加载、无匹配、部分范围、已选中、只读、无位置、撤权 | 10；原02/05 | 搜索不裁舞台；选择不提交；密集与缺位置场景，键盘/触控回焦点 |
| 模型/执行准备 | 未启用不收Key、任务未就绪、授权待确认、检查未过、准备完成未开始 | 11；§10.9.3 | 真实 capability、当前任务/owner/executor 与 UI 对账；模型检查不代签游戏 |
| 模型运行/离开 | running、预算不足、hidden/断网停止、另一页占用、forget失败、未知原件 | 11；§10.9.3 | 跨页/账号、存储失败、旧输出隔离、供应商费用/预算及真实原件零重投 |
| Chat/Quote/Prompt | 草稿、IME、预览、签名中、报价stale、原提交pending、hidden/control_lost | 12；原03/04 | 原请求 ACK/error 竞态、提交参数/授权、中文/焦点、无双提交 |
| 成果/重入 | confirmed、no progress、partial、unknown、查询不可用、换号旧反馈 | 12；原04/06 | 同原 signer/nonce/payload/target、当前读取 fence 与领域结果；无盲重试 |
| 模式与 Feed | live、未知profile、offline playback、feed replay/gap/reorg/empty increment | 13；§10.9.5 | no-live-seek、真实控制反馈与 Feed cursor；非live证据不代签live |
| 次级工具 | en/zh、长CJK、200%文字、低高度、Director拒绝/撤权、安全退出 | 14；原07/08 | 同候选真实浏览器/辅助技术、焦点/几何、权限清理；静态稿不认证a11y |

跨稿复用同一词义：`账号已登录` 不等于 `已进入世界`；`已选中` 不等于 `可指导`；`检查通过` 不等于 `已开始`；`已受理` 不等于 `已完成`；`原结果未知` 不等于 `未执行`；`已停止新决策` 不等于 `世界作业已取消`。所有能力和恢复入口仍受对应专业 authority 限制，不通过设计稿扩大 current support。

<a id="uiux-handoff-trace"></a>
### 10.10 承接、验证与交付边界

下列行为是后续实现验收合同；本次文档与静态稿只核对内容一致、链接、可读性和覆盖。真实 Web/native、GPU、键盘/IME、权限竞态及玩家五秒理解仍分别取得实际证据。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001) | Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；目的/下一步/原结果首读，原主体与选择分离 | [屏幕分区](#uiux-screen-regions) | visual / Viewer；Runtime 原结果 | 不创造因果或宣称真实理解通过 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001) | Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；目的/下一步/原结果首读，原主体与选择分离 | [首次与重入](#uiux-first-return-targets) | visual / Viewer；Runtime 原结果 | 不创造因果或宣称真实理解通过 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001) | Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；目的/下一步/原结果首读，原主体与选择分离 | [身份](#uiux-context-identity) | visual / Viewer；Runtime 原结果 | 不创造因果或宣称真实理解通过 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002) | Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3；未知、来源、时效、冲突与撤权可辨 | [状态反馈](#uiux-state-feedback) | Runtime 授权投影；Viewer | 不自定 TTL、权限或重新提交语义 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003) | Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；合法探索、重叠候选、返回目标与密度可读 | [旅程](#uiux-primary-journeys) | Viewer / Rust bridge；空间投影 | 不声称全世界覆盖或新增关系权威 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003) | Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；合法探索、重叠候选、返回目标与密度可读 | [Targets与重入](#uiux-first-return-targets) | Viewer / Rust bridge；空间投影 | 不声称全世界覆盖或新增关系权威 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003) | Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；合法探索、重叠候选、返回目标与密度可读 | [Bevy 层](#uiux-bevy-layer-sheet) | Viewer / Rust bridge；空间投影 | 不声称全世界覆盖或新增关系权威 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004) | Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a；受影响 Web 视口、输入和图形恢复呈现 | [响应式](#uiux-responsive) | Viewer / QA；现有平台合同 | 仅细化 Web 设计；原生容器及平台支持不由本稿确立 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004) | Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a；受影响 Web 视口、输入和图形恢复呈现 | [输入](#uiux-input-access) | Viewer / QA；现有平台合同 | 仅细化 Web 设计；原生容器及平台支持不由本稿确立 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004) | Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a；受影响 Web 视口、输入和图形恢复呈现 | [次级工具](#uiux-secondary-help-privacy) | Viewer / QA；现有平台合同 | 仅细化 Web 设计；原生容器及平台支持不由本稿确立 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004) | Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a；受影响 Web 视口、输入和图形恢复呈现 | [恢复](#uiux-state-feedback) | Viewer / QA；现有平台合同 | 仅细化 Web 设计；原生容器及平台支持不由本稿确立 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005) | Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096；只读观察与合法间接指导、草稿/预览/生效区分 | [旅程](#uiux-primary-journeys) | Agent / Runtime 权限与幂等；Viewer | 不新增直接世界动作、通用撤销或回滚 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005) | Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096；只读观察与合法间接指导、草稿/预览/生效区分 | [首次与重入](#uiux-first-return-targets) | Agent / Runtime 权限与幂等；Viewer | 不新增直接世界动作、通用撤销或回滚 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005) | Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096；只读观察与合法间接指导、草稿/预览/生效区分 | [身份](#uiux-context-identity) | Agent / Runtime 权限与幂等；Viewer | 不新增直接世界动作、通用撤销或回滚 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005) | Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096；只读观察与合法间接指导、草稿/预览/生效区分 | [Command原结果](#uiux-command-form-result) | Agent / Runtime 权限与幂等；Viewer | 不新增直接世界动作、通用撤销或回滚 |
| [REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001) | Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef；退出再入/刷新后重找目标、blocker、可信原结果和下一决策，失败时有安全入口 | [旅程 J10](#uiux-primary-journeys) | Viewer / Runtime 当前身份与读取资格 | 不新增会话、草稿持久化或原结果查询协议 |
| [REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001) | Trace refs: O7-REQ-20b49f66-b663-41ed-9b52-253ce76ed1ef；退出再入/刷新后重找目标、blocker、可信原结果和下一决策，失败时有安全入口 | [首次与重入](#uiux-first-return-targets) | Viewer / Runtime 当前身份与读取资格 | 不新增会话、草稿持久化或原结果查询协议 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003) | Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6；个人反馈只属于当前玩家；匿名/其他玩家不污染本人结果 | [身份](#uiux-context-identity) | Runtime / Viewer 反馈隔离 | 不通过当前选择、Feed 或客户端缓存推导归属 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003) | Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6；个人反馈只属于当前玩家；匿名/其他玩家不污染本人结果 | [Command原结果](#uiux-command-form-result) | Runtime / Viewer 反馈隔离 | 不通过当前选择、Feed 或客户端缓存推导归属 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003) | Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6；个人反馈只属于当前玩家；匿名/其他玩家不污染本人结果 | [状态](#uiux-state-feedback) | Runtime / Viewer 反馈隔离 | 不通过当前选择、Feed 或客户端缓存推导归属 |
| [Player shell 当前布局](viewer-gameplay-release-experience-overhaul.prd.md#current-implementation-boundary) | 单一 Next Move 主区、Objective / Player Leverage 从属上下文；窄屏主区第一行 | [本设计布局条款](#33-command-strip) | viewer_engineer；game_visual_interaction_designer | 不新增布局决策、DTO、玩法或发布承诺；未扩展 target 合同 |
| [REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001) | Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264；入口模式、账号/准入/控制及demo/live分开 | [账号](#uiux-entry-session-world) | identity / runtime authority；Viewer呈现 | 不新增世界选择器、准入、生产能力或发行结论 |
| [REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001) | Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264；入口模式、账号/准入/控制及demo/live分开 | [模式](#uiux-live-playback-context) | identity / runtime authority；Viewer呈现 | 不新增世界选择器、准入、生产能力或发行结论 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) | Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；认证/绑定/列表失败和原尝试未知有阶段与安全恢复 | [账号](#uiux-entry-session-world) | identity及原仪式查询；Viewer焦点与文案 | 不以fallback或重新认证复活旧authority |
| [Flow-KEYAUTH-008](../../p2p/blockchain/hosted-public-join-key-based-onboarding.prd.md#imported-key-login) | Trace refs: O7-REQ-45c9a6e2-d624-4a57-bced-b96b6aa78f69；受限网页导入默认收起、安全告知、输入清空 | [账号](#uiux-entry-session-world) | identity native-key合同 | 不新增密钥生成/备份、Passkey导入或资产signer权限；独立 product 层条款 N/A；reason=受限导入由专业账号 PRD 的 Flow-KEYAUTH-008 唯一声明，尚无单列 doc/product 导入条款；scope=仅网页入口折叠、安全告知和输入清空，仍须完整验证该专业合同及上位接入要求 |
| [REQ-BYOK-001](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-001) | Trace refs: O7-REQ-a95a56b1-22b6-4955-b152-0c672310ddb8；部署前置不足不收Key，连接/检查/开始分开 | [模型](#uiux-byok-control-flow) | agent Game Client /部署capability | 不宣称尚未支持供应商可用，不替代真实部署 |
| [REQ-BYOK-002](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-002) | Trace refs: O7-REQ-26cefe7a-deb5-4b21-845a-a2d4556f7dca；本机保存/接收方/费用知情、Stop/Forget与删除失败可读 | [模型](#uiux-byok-control-flow) | 本地存储/多页管理和供应商费用authority | 不承诺加密隔离、跨设备同步、账户账单硬封顶或退款 |
| [REQ-BYOK-003](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-003) | Trace refs: O7-REQ-84294c93-3275-4d19-8faf-389950b87464；实际任务、executor授权、模型就绪、运行和世界结果分开 | [模型](#uiux-byok-control-flow) | owner/executor/task/fence与浏览器许可 | 不以Key授予权限、不承诺后台或关页续跑 |
| [REQ-BYOK-004](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-004) | Trace refs: O7-REQ-d3d63078-ef30-45fb-8ae6-2f6db1f46c34；停止后旧输出无新权限，未知原件先查不盲重投 | [模型](#uiux-byok-control-flow) | 原签名/持久原件与Stop generation | 不取消已受理世界作业，不以新运行替代原结果 |
| [REQ-BYOK-005](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-005) | Trace refs: O7-REQ-e0ae0d80-31ca-488b-8ba2-a728bfb9ce8d；按供应商/模型组合表达支持和失败 | [模型](#uiux-byok-control-flow) | preset/协议适配与实际供应商证据 | 当前DeepSeek源码不证明OpenRouter、全部模型或真实可玩 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002) | Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64；模式不支持、连接和原件未知分开，Live不Seek | [模式](#uiux-live-playback-context) | 控制profile/Runtime反馈与Viewer | 不增加Web Seek或自动重试能力 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002) | Trace refs: O7-REQ-d7081137-94a9-4524-b08a-fc3fbfcb9f64；模式不支持、连接和原件未知分开，Live不Seek | [状态矩阵](#uiux-whole-viewer-state-matrix) | 控制profile/Runtime反馈与Viewer | 不增加Web Seek或自动重试能力 |
| [REQ-PROMPT-004](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-004) | Trace refs: O7-REQ-59fca4ef-cd56-44ee-b982-b5eb9e15a111；Preview/accepted/applied分开 | [Command](#uiux-command-form-result) | PromptControl原结果authority | 不以UI受理或模型解释确认世界效果 |
| [REQ-PROMPT-007](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-007) | Trace refs: O7-REQ-458146d9-d3c2-4e14-935d-29a91594e8dd；控制丢失优先，hidden值不重现，旧草稿不迁移/提交 | [Command](#uiux-command-form-result) | PromptControl权限与visibility | 不增加恢复授权或借缓存披露最新值 |
| [REQ-PROMPT-011](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-011) | Trace refs: O7-REQ-36cc9e9d-eaa7-436f-b16b-6f6af089b04c；applied只陈述authority声明的应用范围 | [Command](#uiux-command-form-result) | Runtime instance与scope tuple | 不推导持久化、跨会话/入口同步或世界目标完成 |
| [REQ-FIRST-INDUSTRIAL-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-002) | Trace refs: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95；对象/查询参数/实际方案/成本与提交范围不混同 | [Command](#uiux-command-form-result) | 发行设计§8与M4同方案报价 | 不把只读Quote或禁用的宣战/治理提交画成可执行 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005) | Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48；stale报价与迟到结果保留原请求归属 | [Command](#uiux-command-form-result) | M4/Runtime identity及原结果查询 | 不换nonce重提，不用退出UI取消已提交动作 |
| [REQ-FIRST-INDUSTRIAL-008](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-008) | Trace refs: O7-REQ-757636b1-c9e9-4e7a-97d1-460de105daf3；成果、后继候选与下一动作分开，离场和重入保留真实义务 | [首次与重入](#uiux-first-return-targets) | 发行设计§8与Runtime原成果 | 不把候选当建成，不生成离线收益或复活旧授权 |

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001) | [屏幕分区](#uiux-screen-regions) | [AC-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-001)；单一 Next Move；A 在途切 B，原结果仍属 A；无回执不补造 | 本稿检查桌面/窄屏静态稿；实现候选另执行 [PW-READABILITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-readability) 及该手册 PW-IDENTITY 场景 | 当前 PR 的稿件核对；后续同候选浏览器/结果记录 | 静态稿不证明真实因果、输入或五秒理解 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001) | [身份](#uiux-context-identity) | [AC-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-001)；单一 Next Move；A 在途切 B，原结果仍属 A；无回执不补造 | 本稿检查桌面/窄屏静态稿；实现候选另执行 [PW-READABILITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-readability) 及该手册 PW-IDENTITY 场景 | 当前 PR 的稿件核对；后续同候选浏览器/结果记录 | 静态稿不证明真实因果、输入或五秒理解 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002) | [状态反馈](#uiux-state-feedback) | [AC-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-002)；未知/冲突/撤权与最近已知不混同，未知结果不盲重做 | [PW-FRESHNESS](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-freshness)；静态逐状态审读；后续专项手册 PW-FRESHNESS，注入旧响应/撤权/断连并对账原操作 | PR 状态稿；后续权限与原结果证据 | 不证明真实 Runtime 重验、TTL 或恢复 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003) | [旅程](#uiux-primary-journeys) | [AC-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-003)；稀疏/密集、缺位置/关系、重叠候选与返回目标 | [PW-EXPLORATION](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-exploration)；稿件核对层次及无虚构关系；后续专项手册 PW-EXPLORATION/PERFORMANCE，真实同候选高密度与输入 | PR 图层稿；后续浏览器/Rust 及容量结果 | 不证明 GPU 命中、容量或跨端能力 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003) | [Bevy 层](#uiux-bevy-layer-sheet) | [AC-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-003)；稀疏/密集、缺位置/关系、重叠候选与返回目标 | [PW-EXPLORATION](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-exploration)；稿件核对层次及无虚构关系；后续专项手册 PW-EXPLORATION/PERFORMANCE，真实同候选高密度与输入 | PR 图层稿；后续浏览器/Rust 及容量结果 | 不证明 GPU 命中、容量或跨端能力 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004) | [响应式](#uiux-responsive) | [AC-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-004)；Web 桌面/平板/窄屏/低高度、CJK/键盘/IME、启动和 ready 后失败 | 静态视口与文案检查；后续 [PW-ACCESSIBILITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-accessibility)、PW-RECOVERY/PLATFORM，真实受支持组合逐项验证 | PR 静态稿；后续截图、焦点/IME操作及恢复记录 | 静态设计不证明浏览器 a11y、真实 GPU 或 native |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004) | [输入](#uiux-input-access) | [AC-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-004)；Web 桌面/平板/窄屏/低高度、CJK/键盘/IME、启动和 ready 后失败 | 静态视口与文案检查；后续 [PW-ACCESSIBILITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-accessibility)、PW-RECOVERY/PLATFORM，真实受支持组合逐项验证 | PR 静态稿；后续截图、焦点/IME操作及恢复记录 | 静态设计不证明浏览器 a11y、真实 GPU 或 native |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004) | [恢复](#uiux-state-feedback) | [AC-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-004)；Web 桌面/平板/窄屏/低高度、CJK/键盘/IME、启动和 ready 后失败 | 静态视口与文案检查；后续 [PW-ACCESSIBILITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-accessibility)、PW-RECOVERY/PLATFORM，真实受支持组合逐项验证 | PR 静态稿；后续截图、焦点/IME操作及恢复记录 | 静态设计不证明浏览器 a11y、真实 GPU 或 native |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005) | [旅程](#uiux-primary-journeys) | [AC-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-005)；观察不执行；明确主体；拒绝/重验不自动批准新方案 | [PW-IDENTITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-identity)；静态标注检查；后续专项手册 PW-IDENTITY/FRESHNESS，以自己的/无权 Agent、转让/撤权和延迟原结果验证 | PR 流程稿；后续权限负例与世界结果对账 | 不证明服务器授权、幂等或实际世界效果 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005) | [身份](#uiux-context-identity) | [AC-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-005)；观察不执行；明确主体；拒绝/重验不自动批准新方案 | [PW-IDENTITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-identity)；静态标注检查；后续专项手册 PW-IDENTITY/FRESHNESS，以自己的/无权 Agent、转让/撤权和延迟原结果验证 | PR 流程稿；后续权限负例与世界结果对账 | 不证明服务器授权、幂等或实际世界效果 |
| [REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001) | [旅程 J10](#uiux-primary-journeys) | [AC-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#ac-agent-surface-001)；退出/刷新/再入后四个决策锚点可读，查询受阻不猜测原结果 | 本稿逐步审读 J10；后续 [PW-FRESHNESS](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-freshness) 及上游恢复场景，覆盖重新认证成功/失败、权限收窄与原结果未知 | 当前 PR 静态旅程核对；后续同候选会话/授权/原结果记录 | 不证明跨会话恢复实现、实际读取资格或幂等 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003) | [身份](#uiux-context-identity) | [AC-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#ac-agent-surface-003)；两名玩家和匿名失败相互隔离；切换身份清理旧反馈 | [PW-IDENTITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-identity)；静态状态与标注审读；后续按上游 AC 的两玩家/匿名、刷新/重连/身份切换组合，以真实权限和反馈读取核对 | PR 设计稿；后续同候选反馈隔离测试及真实结果 | 不证明多人 Runtime、匿名失败路由或授权恢复 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003) | [状态](#uiux-state-feedback) | [AC-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#ac-agent-surface-003)；两名玩家和匿名失败相互隔离；切换身份清理旧反馈 | [PW-IDENTITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-identity)；静态状态与标注审读；后续按上游 AC 的两玩家/匿名、刷新/重连/身份切换组合，以真实权限和反馈读取核对 | PR 设计稿；后续同候选反馈隔离测试及真实结果 | 不证明多人 Runtime、匿名失败路由或授权恢复 |
| [Player shell 当前布局](viewer-gameplay-release-experience-overhaul.prd.md#current-implementation-boundary) | [本设计布局条款](#33-command-strip) | 当前操作条款与主合同一致，不能要求恢复三张平级卡片或 Next Move 第二行 | 对同候选主合同与 `crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx`、`crates/oasis7_viewer/viewer_terminal_shell.css` 做静态对照；未来可见改动另按 [Viewer S6 手册](../../testing/manual/web-ui-agent-browser-closure-manual.manual.md) 的视觉矩阵取得真实桌面/窄屏及输入证据 | 本 PR 的 source/diff 与限定静态核对；没有新增浏览器证据 | 源码接线不证明新 S6、真实玩家体验、完整 renderer/a11y 或发布通过 |
| [REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001) | [账号](#uiux-entry-session-world) | 注册/账号/准入/控制分别确认；取消/未知/404与其他失败不混同 | 后续按[多凭证手册](../../testing/manual/hosted-passkey-account.manual.md#real-device)检查真实设备、返回焦点、账号列表局部失败、原仪式查询及多设备接管 | 同候选阶段/原attempt/当前session与UI记录 | 静态稿、虚拟认证器不代签真实设备/云后端/准入 |
| [REQ-ENTRY-MODE-002](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-002) | [账号](#uiux-entry-session-world) | 注册/账号/准入/控制分别确认；取消/未知/404与其他失败不混同 | 后续按[多凭证手册](../../testing/manual/hosted-passkey-account.manual.md#real-device)检查真实设备、返回焦点、账号列表局部失败、原仪式查询及多设备接管 | 同候选阶段/原attempt/当前session与UI记录 | 静态稿、虚拟认证器不代签真实设备/云后端/准入 |
| [Flow-KEYAUTH-008](../../p2p/blockchain/hosted-public-join-key-based-onboarding.prd.md#imported-key-login) | [账号](#uiux-entry-session-world) | 导入次级披露、输入清空、scope安全且不生成备份 | 后续按[网页导入验收](../../testing/manual/hosted-passkey-account.manual.md#网页导入密钥登录)走实际页面，检查网络/storage/日志不含私钥和错误账号拒绝 | 同候选脱敏网络/存储/页面证据 | 图稿不证明WebCrypto、安全存储或真实代理路由 |
| [REQ-BYOK-001](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-001) | [模型](#uiux-byok-control-flow) | 无前置不收Key，当前支持组合真实，检查不自动启动 | 后续按[模型验收手册](../../testing/manual/self-supplied-model-connection.manual.md#byok-01)，按 BYOK-01 核对浏览器部署 capability、任务前置、凭证接收方与显式检查/开始；当前未支持组合保持禁用；该手册旧托管步骤须沿其浏览器适用范围修订，不用旧通过记录代签 | 同候选供应商/模型/协议/费用授权与界面状态 | 一个组合成功不代签OpenRouter或完整游戏 |
| [REQ-BYOK-005](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-005) | [模型](#uiux-byok-control-flow) | 无前置不收Key，当前支持组合真实，检查不自动启动 | 后续按[模型验收手册](../../testing/manual/self-supplied-model-connection.manual.md#byok-07)，按 BYOK-07/08 分 provider/model/protocol 检查实际 CSP/CORS、工具往返和获授权有限付费调用；一个组合不代签另一个；该手册旧托管步骤须沿其浏览器适用范围修订，不用旧通过记录代签 | 同候选供应商/模型/协议/费用授权与界面状态 | 一个组合成功不代签OpenRouter或完整游戏 |
| [REQ-BYOK-002](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-002) | [模型](#uiux-byok-control-flow) | 保存/检查/授权/开始分开，Stop/Forget删除失败可读，旧输出不重放 | 后续按[模型验收手册](../../testing/manual/self-supplied-model-connection.manual.md#byok-02)，按 BYOK-02/03 核对账号隔离、保存/删除失败、双页配置与 Forget 冲突、存储拒绝及未知计费；检查 localStorage、脱敏网络与预算连续性；该手册旧托管步骤须沿其浏览器适用范围修订，不用旧通过记录代签 | 同候选实际点击、原签名/lookup零submit、存储及预算记录 | 静态源码/模拟响应不证明供应商费用或真实世界效果 |
| [REQ-BYOK-003](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-003) | [模型](#uiux-byok-control-flow) | 保存/检查/授权/开始分开，Stop/Forget删除失败可读，旧输出不重放 | 后续按[模型验收手册](../../testing/manual/self-supplied-model-connection.manual.md#byok-04)，按 BYOK-04/05 核对实际任务、owner/executor fence、明确授权与开始；覆盖隐藏/断网/重开、另一页运行和世界结果分离；该手册旧托管步骤须沿其浏览器适用范围修订，不用旧通过记录代签 | 同候选实际点击、原签名/lookup零submit、存储及预算记录 | 静态源码/模拟响应不证明供应商费用或真实世界效果 |
| [REQ-BYOK-004](../../product/agents-world-simulation/self-supplied-model-connection.prd.md#req-byok-004) | [模型](#uiux-byok-control-flow) | 保存/检查/授权/开始分开，Stop/Forget删除失败可读，旧输出不重放 | 后续按[模型验收手册](../../testing/manual/self-supplied-model-connection.manual.md#byok-06)，按 BYOK-06 核对 Stop/Forget、双页/换号、迟到输出与只读原件查询；对账原 signer/fence，无重复 submit、不取消已受理世界作业；该手册旧托管步骤须沿其浏览器适用范围修订，不用旧通过记录代签 | 同候选实际点击、原签名/lookup零submit、存储及预算记录 | 静态源码/模拟响应不证明供应商费用或真实世界效果 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002) | [模式](#uiux-live-playback-context) | Live不Seek、offline不代签Live、Feed replay不改profile/不补回执 | 后续按[Viewer 验收手册](viewer-manual.manual.md#target-qa-evidence-checklist)的同候选桌面/窄屏与 Feed 状态场景，核对 unknown/live/playback、unsupported-before-send、empty increment 及 profile/连接/原件结果各自表达；无实际入口保留未验证 | profile/消息/请求结果与对应UI证据 | 文案不证明Runtime路由或完整timeline |
| [REQ-PROMPT-004](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-004) | [Command](#uiux-command-form-result) | 原结果/visibility/scope不扩大，控制丢失安全退出 | 后续按[Web UI 闭环手册](../../testing/manual/web-ui-agent-browser-closure-manual.manual.md#qa-infrastructure-receiver-evidence-boundary)，固定同候选授权/原请求与 before→pending→fresh recovery→final 证据；验证 A 迟到/切 B/正确 ACK、撤权隐藏、应用 scope、控制丢失安全退出和真实焦点；API oracle 与可见输入分别核对 | 原request、授权结果与真实界面/输入记录 | 图稿不证明authority幂等、持久化或原世界效果 |
| [REQ-PROMPT-007](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-007) | [Command](#uiux-command-form-result) | 原结果/visibility/scope不扩大，控制丢失安全退出 | 后续按[Web UI 闭环手册](../../testing/manual/web-ui-agent-browser-closure-manual.manual.md#qa-infrastructure-receiver-evidence-boundary)，固定同候选授权/原请求与 before→pending→fresh recovery→final 证据；验证 A 迟到/切 B/正确 ACK、撤权隐藏、应用 scope、控制丢失安全退出和真实焦点；API oracle 与可见输入分别核对 | 原request、授权结果与真实界面/输入记录 | 图稿不证明authority幂等、持久化或原世界效果 |
| [REQ-PROMPT-011](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md#req-prompt-011) | [Command](#uiux-command-form-result) | 原结果/visibility/scope不扩大，控制丢失安全退出 | 后续按[Web UI 闭环手册](../../testing/manual/web-ui-agent-browser-closure-manual.manual.md#qa-infrastructure-receiver-evidence-boundary)，固定同候选授权/原请求与 before→pending→fresh recovery→final 证据；验证 A 迟到/切 B/正确 ACK、撤权隐藏、应用 scope、控制丢失安全退出和真实焦点；API oracle 与可见输入分别核对 | 原request、授权结果与真实界面/输入记录 | 图稿不证明authority幂等、持久化或原世界效果 |
| [REQ-FIRST-INDUSTRIAL-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-002) | [Command](#uiux-command-form-result) | 只读估算/合法提交不混同；参数变更、stale、原结果未知可恢复 | 后续按[测试手册](../../../testing-manual.md)选择同候选适用 S6 场景，沿真实表单/quote/action 消费者对账对象、参数与原 receipt；覆盖只读估算、当前禁用提交负例、参数变化、stale、原结果未知、IME/缩放，缺真实消费者保留未验证 | 同候选对象/参数/quote/提交与原receipt；真实输入截图 | 不代签未开放玩法或真人理解 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005) | [Command](#uiux-command-form-result) | 只读估算/合法提交不混同；参数变更、stale、原结果未知可恢复 | 后续按[测试手册](../../../testing-manual.md)选择同候选适用 S6 场景，沿真实表单/quote/action 消费者对账对象、参数与原 receipt；覆盖只读估算、当前禁用提交负例、参数变化、stale、原结果未知、IME/缩放，缺真实消费者保留未验证 | 同候选对象/参数/quote/提交与原receipt；真实输入截图 | 不代签未开放玩法或真人理解 |
| [REQ-FIRST-INDUSTRIAL-008](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-008) | [首次与重入](#uiux-first-return-targets) | 实际成果/后继候选/可执行动作分开；自然离场不改原结果 | 后续按[测试手册](../../../testing-manual.md)的同候选适用 S6 场景核对成果回看、后继受阻、自然离场与重入、旧成功不重复播报和授权不复活；无提示续玩观察单独记录 | 原成果、当前条件与同候选真实页面证据 | 静态稿不证明留存、主体感或离线收益 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001) | [首次与重入](#uiux-first-return-targets) | [AC-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-001)；单一 Next Move；A 在途切 B，原结果仍属 A；无回执不补造 | 本稿检查桌面/窄屏静态稿；实现候选另执行 [PW-READABILITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-readability) 及该手册 PW-IDENTITY 场景 | 当前 PR 的稿件核对；后续同候选浏览器/结果记录 | 静态稿不证明真实因果、输入或五秒理解 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003) | [Targets与重入](#uiux-first-return-targets) | [AC-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-003)；稀疏/密集、缺位置/关系、重叠候选与返回目标 | [PW-EXPLORATION](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-exploration)；稿件核对层次及无虚构关系；后续专项手册 PW-EXPLORATION/PERFORMANCE，真实同候选高密度与输入 | PR 图层稿；后续浏览器/Rust 及容量结果 | 不证明 GPU 命中、容量或跨端能力 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004) | [次级工具](#uiux-secondary-help-privacy) | [AC-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-004)；Web 桌面/平板/窄屏/低高度、CJK/键盘/IME、启动和 ready 后失败 | 静态视口与文案检查；后续 [PW-ACCESSIBILITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-accessibility)、PW-RECOVERY/PLATFORM，真实受支持组合逐项验证 | PR 静态稿；后续截图、焦点/IME操作及恢复记录 | 静态设计不证明浏览器 a11y、真实 GPU 或 native |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005) | [首次与重入](#uiux-first-return-targets) | [AC-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-005)；观察不执行；明确主体；拒绝/重验不自动批准新方案 | [PW-IDENTITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-identity)；静态标注检查；后续专项手册 PW-IDENTITY/FRESHNESS，以自己的/无权 Agent、转让/撤权和延迟原结果验证 | PR 流程稿；后续权限负例与世界结果对账 | 不证明服务器授权、幂等或实际世界效果 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005) | [Command原结果](#uiux-command-form-result) | [AC-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#ac-agent-stage-005)；观察不执行；明确主体；拒绝/重验不自动批准新方案 | [PW-IDENTITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-identity)；静态标注检查；后续专项手册 PW-IDENTITY/FRESHNESS，以自己的/无权 Agent、转让/撤权和延迟原结果验证 | PR 流程稿；后续权限负例与世界结果对账 | 不证明服务器授权、幂等或实际世界效果 |
| [REQ-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-001) | [首次与重入](#uiux-first-return-targets) | [AC-AGENT-SURFACE-001](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#ac-agent-surface-001)；退出/刷新/再入后四个决策锚点可读，查询受阻不猜测原结果 | 本稿逐步审读 J10；后续 [PW-FRESHNESS](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-freshness) 及上游恢复场景，覆盖重新认证成功/失败、权限收窄与原结果未知 | 当前 PR 静态旅程核对；后续同候选会话/授权/原结果记录 | 不证明跨会话恢复实现、实际读取资格或幂等 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003) | [Command原结果](#uiux-command-form-result) | [AC-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#ac-agent-surface-003)；两名玩家和匿名失败相互隔离；切换身份清理旧反馈 | [PW-IDENTITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-identity)；静态状态与标注审读；后续按上游 AC 的两玩家/匿名、刷新/重连/身份切换组合，以真实权限和反馈读取核对 | PR 设计稿；后续同候选反馈隔离测试及真实结果 | 不证明多人 Runtime、匿名失败路由或授权恢复 |
| [REQ-ENTRY-MODE-001](../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-mode-001) | [模式](#uiux-live-playback-context) | 入口来源、正式 Live 与离线示例分开；账号/准入/控制不由示例画面推导，profile 未知不赋予能力 | 后续按[Viewer 验收手册](viewer-manual.manual.md#target-qa-evidence-checklist)的同候选页面状态采样，并按该手册证据边界核对正式入口、离线示例与控制 profile；来源未确认时不能标正式 Live，静态/QA 入口不能代签生产准入 | 同候选入口/部署来源、profile 消息与页面标签；正式与离线证据分列 | 静态稿、虚拟认证器不代签真实设备/云后端/准入 |
| [REQ-AGENT-SURFACE-002](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002) | [状态矩阵](#uiux-whole-viewer-state-matrix) | Live不Seek、offline不代签Live、Feed replay不改profile/不补回执；按状态矩阵分别核对连接、renderer、新鲜度、权限和原结果，不能合并成单一离线 | 后续按[Viewer 验收手册](viewer-manual.manual.md#target-qa-evidence-checklist)的同候选桌面/窄屏与 Feed 状态场景，核对 unknown/live/playback、unsupported-before-send、empty increment 及 profile/连接/原件结果各自表达；无实际入口保留未验证 | profile/消息/请求结果与对应UI证据 | 文案不证明Runtime路由或完整timeline |

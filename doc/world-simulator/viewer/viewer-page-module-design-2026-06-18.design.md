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

后续若调整按需上下文占位，应比较侧向与下沿抽屉，保持 Player/Director、操作权限和信息真值不变。本轮未交付该对比图。桌面侧抽屉有利于保留纵向舞台与回执阅读；下沿抽屉适合横向长回执但低高度占用较大；窄屏保留单路线覆盖。取舍和最终尺寸须由真实候选验证，不将此分析写成已完成比较或第二默认。密集候选及未知位置的完整画面变体仍须后续按图层条款与专项手册验证。


<a id="uiux-module-coverage"></a>
### 10.9 模块覆盖与不变边界

未画出的外围模块不代表删除或新增能力；下列入口保留现有 authority。

| 模块/状态 | 本次呈现与交互约定 | 权威与未覆盖范围 |
| --- | --- | --- |
| Web 入口/兼容 alias | viewer 为正式入口；兼容入口不形成第二品牌/世界；当前世界与会话身份清楚 | [Web entry](viewer-web-entry-compatibility.prd.md)；不新增世界选择器或 URL/权限合同 |
| 登录/退出 | hosted_public_join；登录等待/失败/过期与世界加载分开；不展示凭据，释放会话清理私有内容 | [页面能力](viewer-manual.manual.md#页面能力)；不重写邮箱认证、账号映射或 session 协议 |
| 本地/正式世界 | 本地 standalone/provider test、observer/debug和正式共享世界不混淆；本地成果不宣传为正式成果 | [本地测试](viewer-manual.manual.md#本地真实-llm-游戏测试)、[证据边界](viewer-manual.manual.md#证据边界)；不恢复 retired trusted_local_only 用户流程 |
| QA 专用登录 | 不出现在普通 Player 稿；现有 QA 入口保持限制 | [页面能力](viewer-manual.manual.md#页面能力)；不绕过 loopback、显式启用或生产邮箱链路 |
| 认领/绑定/只读权限 | 无可控制 Agent 时引导认领或等待绑定；可见他人不等于可操作；认领 pending 与 committed 分开 | [Chat/Prompt](viewer-manual.manual.md#当前-agent-chat-与高级-prompt-设置)；不改变身份/认领权威 |
| Agent Context/Details | Command/chat 优先，只读上下文按需展开；观察、推荐、原结果分开 | [模块设计](#3-module-design)；不新增资源、生产事实或完整产业面板 |
| Chat/provider | 多行草稿、显式 Send Chat；provider 未配置/不可达与权限拒绝分开；失败保留安全文本 | [Chat/Prompt](viewer-manual.manual.md#当前-agent-chat-与高级-prompt-设置)；不重写凭据存储或承诺连接/自动重试 |
| Advanced Prompt Settings | operator-level 默认收起；override、preview、apply、rollback与反馈分开 | [Chat/Prompt](viewer-manual.manual.md#当前-agent-chat-与高级-prompt-设置)；rollback仅为既有Prompt能力，不是世界回滚；strong_auth由后端决定 |
| 报价/玩法表单 | 真实存在的情境表单留在Command；字段错误就地说明，保留安全输入与主体；Quote不是主导航 | [控制面](viewer-control-plane-split-live-playback.prd.md)；不新增资产转账或未发布玩法 |
| 连接/重放 | 连接恢复不等于世界结果；Feed gap/replay/reorg分开；恢复查询原操作 | [状态恢复](#uiux-state-feedback)、[连续性](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-002)；不建立新幂等协议 |
| 语言/复制/文本缩放 | en/zh切换、正文选择复制、浏览器放大不丢主动作 | [页面能力](viewer-manual.manual.md#页面能力)、[输入](#uiux-input-access)；未确认应用独立字号设置，不画新设置页 |
| 音量/音效/通用偏好 | 不新增控件或声音成功信号；减少动态效果不依赖声音替代 | [视觉规范](viewer-visual-design-spec-2026-06-05.design.md#12-accessibility-and-robustness)；已审读authority未确认音量/音效设置，属于未覆盖，不宣称不存在 |
| Director/Diagnostics | 默认Player，诊断次级，Director能力受限；拒绝/失效安全返回 | [shell](viewer-gameplay-release-experience-overhaul.prd.md#current-implementation-boundary)；不增加issuer或生产成功路径 |
| 原生/发布 | 概念路径可复用，实际逐平台验证 | [原生权威](external-agent-world-view.design.md#view-native)；本次不提供原生实现、发行或支持认证 |


<a id="uiux-handoff-trace"></a>
### 10.10 承接、验证与交付边界

下列行为是后续实现验收合同；本次文档与静态稿只核对内容一致、链接、可读性和覆盖。真实 Web/native、GPU、键盘/IME、权限竞态及玩家五秒理解仍分别取得实际证据。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001) | Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；目的/下一步/原结果首读，原主体与选择分离 | [屏幕分区](#uiux-screen-regions) | visual / Viewer；Runtime 原结果 | 不创造因果或宣称真实理解通过 |
| [REQ-AGENT-STAGE-001](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001) | Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；目的/下一步/原结果首读，原主体与选择分离 | [身份](#uiux-context-identity) | visual / Viewer；Runtime 原结果 | 不创造因果或宣称真实理解通过 |
| [REQ-AGENT-STAGE-002](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-002) | Trace refs: O7-REQ-8b88187a-985b-4cf0-a9d4-b6137882ffc3；未知、来源、时效、冲突与撤权可辨 | [状态反馈](#uiux-state-feedback) | Runtime 授权投影；Viewer | 不自定 TTL、权限或重新提交语义 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003) | Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；合法探索、重叠候选、返回目标与密度可读 | [旅程](#uiux-primary-journeys) | Viewer / Rust bridge；空间投影 | 不声称全世界覆盖或新增关系权威 |
| [REQ-AGENT-STAGE-003](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-003) | Trace refs: O7-REQ-2ede7851-f1c0-4851-8d6b-5da4afcdefcf；合法探索、重叠候选、返回目标与密度可读 | [Bevy 层](#uiux-bevy-layer-sheet) | Viewer / Rust bridge；空间投影 | 不声称全世界覆盖或新增关系权威 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004) | Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a；受影响 Web 视口、输入和图形恢复呈现 | [响应式](#uiux-responsive) | Viewer / QA；现有平台合同 | 仅细化 Web 设计；原生容器及平台支持不由本稿确立 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004) | Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a；受影响 Web 视口、输入和图形恢复呈现 | [输入](#uiux-input-access) | Viewer / QA；现有平台合同 | 仅细化 Web 设计；原生容器及平台支持不由本稿确立 |
| [REQ-AGENT-STAGE-004](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-004) | Trace refs: O7-REQ-ecbc5765-ec25-4bfe-9aaa-4bbdd0a3064a；受影响 Web 视口、输入和图形恢复呈现 | [恢复](#uiux-state-feedback) | Viewer / QA；现有平台合同 | 仅细化 Web 设计；原生容器及平台支持不由本稿确立 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005) | Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096；只读观察与合法间接指导、草稿/预览/生效区分 | [旅程](#uiux-primary-journeys) | Agent / Runtime 权限与幂等；Viewer | 不新增直接世界动作、通用撤销或回滚 |
| [REQ-AGENT-STAGE-005](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-005) | Trace refs: O7-REQ-9cb09cbd-3c98-4742-a290-ea98a64ab096；只读观察与合法间接指导、草稿/预览/生效区分 | [身份](#uiux-context-identity) | Agent / Runtime 权限与幂等；Viewer | 不新增直接世界动作、通用撤销或回滚 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003) | Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6；个人反馈只属于当前玩家；匿名/其他玩家不污染本人结果 | [身份](#uiux-context-identity) | Runtime / Viewer 反馈隔离 | 不通过当前选择、Feed 或客户端缓存推导归属 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003) | Trace refs: O7-REQ-41b03765-cccf-4a1f-814c-7b498dc975f6；个人反馈只属于当前玩家；匿名/其他玩家不污染本人结果 | [状态](#uiux-state-feedback) | Runtime / Viewer 反馈隔离 | 不通过当前选择、Feed 或客户端缓存推导归属 |
| [Player shell 当前布局](viewer-gameplay-release-experience-overhaul.prd.md#current-implementation-boundary) | 单一 Next Move 主区、Objective / Player Leverage 从属上下文；窄屏主区第一行 | [本设计布局条款](#33-command-strip) | viewer_engineer；game_visual_interaction_designer | 不新增布局决策、DTO、玩法或发布承诺；未扩展 target 合同 |

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
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003) | [身份](#uiux-context-identity) | [AC-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#ac-agent-surface-003)；两名玩家和匿名失败相互隔离；切换身份清理旧反馈 | [PW-IDENTITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-identity)；静态状态与标注审读；后续按上游 AC 的两玩家/匿名、刷新/重连/身份切换组合，以真实权限和反馈读取核对 | PR 设计稿；后续同候选反馈隔离测试及真实结果 | 不证明多人 Runtime、匿名失败路由或授权恢复 |
| [REQ-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003) | [状态](#uiux-state-feedback) | [AC-AGENT-SURFACE-003](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#ac-agent-surface-003)；两名玩家和匿名失败相互隔离；切换身份清理旧反馈 | [PW-IDENTITY](../../testing/manual/pixel-world-visualization-validation.manual.md#pw-identity)；静态状态与标注审读；后续按上游 AC 的两玩家/匿名、刷新/重连/身份切换组合，以真实权限和反馈读取核对 | PR 设计稿；后续同候选反馈隔离测试及真实结果 | 不证明多人 Runtime、匿名失败路由或授权恢复 |
| [Player shell 当前布局](viewer-gameplay-release-experience-overhaul.prd.md#current-implementation-boundary) | [本设计布局条款](#33-command-strip) | 当前操作条款与主合同一致，不能要求恢复三张平级卡片或 Next Move 第二行 | 对同候选主合同与 `crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx`、`crates/oasis7_viewer/viewer_terminal_shell.css` 做静态对照；未来可见改动另按 [Viewer S6 手册](../../testing/manual/web-ui-agent-browser-closure-manual.manual.md) 的视觉矩阵取得真实桌面/窄屏及输入证据 | 本 PR 的 source/diff 与限定静态核对；没有新增浏览器证据 | 源码接线不证明新 S6、真实玩家体验、完整 renderer/a11y 或发布通过 |

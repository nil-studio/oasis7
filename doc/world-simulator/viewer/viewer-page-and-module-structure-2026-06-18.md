# Viewer 页面整体结构与模块设计（分册）

- 主设计文档：[`viewer-page-module-design-2026-06-18.design.md`](viewer-page-module-design-2026-06-18.design.md)
- 本分册按 #6507 原样承接原第 2–3 节（Overall Page Design 与 Module Design），不改变既有视觉/交互契约或验证边界。

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

1. Compact world identity and current viewing scope (a place name alone is not world identity).
2. Command strip with Next Move on the first row and supporting context on the second row.
3. World Board/Canvas.
4. Action Receipt.
5. Reachable `World / Targets / Command` rail, with Search inside Targets and Diagnostics secondary.
6. Targets, Command, Gameplay Details, Diagnostics through anchors or tabs.

The rail remains reachable regardless of placement; reachability does not make navigation
more visually prominent than the world identity or the current decision.

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
- lets the player choose the agent, location, or target shown by the stage and
  selected-target context. Selection does not retarget a Command draft or an
  original receipt; their subjects follow the [identity contract](viewer-page-module-design-2026-06-18.design.md#uiux-context-identity).
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

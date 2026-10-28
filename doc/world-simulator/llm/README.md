# World-simulator LLM and provider authority

This directory contains durable requirements and contracts for in-world Agent
decision providers. It is not a task board: mutable delivery facts belong to
Git, PRs and actual verification records; Issues and Projects are used as needed
for tracking, as defined by the development workflow.

## Product outcome

The [external Agent Runtime gameplay PRD](../../product/agents-world-simulation/external-agent-runtime-play.prd.md)
owns the user-facing goal, first-release Runtime scope, complete first-session
journey and support claims. Users bring an existing Agent Runtime, bind it to a
lawfully acquired or authorized in-world Agent, and let it repeatedly observe,
act and learn from authoritative outcomes. OpenClaw and Codex are separate
first-release acceptance targets; naming either is not evidence of support.

The contracts below supply the provider, transport, observation, cognition and
evaluation requirements for that outcome. Their original low-frequency NPC P0
smoke and technical milestones remain scoped evidence. They do not replace the
product's representative production, recovery and return-session acceptance.

## External-runtime product direction

The product-owned target is **an official Skill plus an independently callable,
authorized Game API**: an OpenClaw/Codex client actively reads observations,
discovers permitted capabilities, submits intents and retrieves authoritative
receipts/events. See [external Runtime product scope](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#external-runtime-canonical-interface)
and its paired design. This is a *target*, not evidence of a shipped endpoint
or published OpenClaw/Codex integration.

The provider contracts listed below describe existing/specialized callback
mechanics where the **game calls a provider for a candidate decision**. They
may be reused or retired without requiring legacy wire compatibility for the
new, externally initiated Game API. Do not conflate the launcher/operator
control API, the `oasis7_agent_api` shared DTO crate, NewAPI model-quota
routing, or the local HTTP provider callback with an authenticated world
play API. Removal requires checking actual independent consumers and preserving
world execution authority, durable results and active player permissions.

## Agent-initiated gameplay system designs

The following target contracts implement the product scope merged in #4493.
They are technical design inputs, not shipped endpoints or runtime proof.

| Technical responsibility | Canonical design |
| --- | --- |
| Versioned Game API, identity, capabilities, admission and recoverable operations | [Game API](external-agent-game-api.design.md) |
| Runtime-owned task/goal/approval state, executor fences and durable message recovery | [Player/runtime collaboration](../../world-runtime/runtime/player-runtime-collaboration.design.md) |
| Official Skill, OpenClaw/Codex drivers, local worker lifecycle and budget boundaries | [Runtime adapters](external-agent-runtime-adapter.design.md) |
| Shared Web/native world projections, spatial navigation and authorized guidance | [World view](../viewer/external-agent-world-view.design.md) |
| Deterministic contract, fault-injection and separately authorized real-runtime/UI scenarios | [Validation manual](../../testing/manual/external-agent-runtime-contract-validation.manual.md) |

All four designs consume the existing product requirements and preserve Runtime
world authority. The new Game API is not the old Provider callback under an alias;
its rollout may retire unnecessary Bridge code after checking real consumers.

## Session-scoped BYOK consumption

[Deployment and validation runbook](byok-model-consumption.runbook.md) documents
the experimental model session layer, default-off authenticated receiver and
remaining native executor integration. Fixture checks are separate from real
provider and gameplay acceptance.

## First-party Harness as a game client

The [product boundary](../../product/agents-world-simulation/provider-agent-experience-continuity.prd.md#first-party-agent-client)
defines the builtin Harness as a first-party client, not part of world authority.
The [client-boundary system design](agent-game-client-boundary.design.md#des-agc-001)
reuses the extracted Harness and ordinary signed client: shared SDK reliability,
one game contract and one Runtime admission/effect path. External Runtimes keep
their own planning; neither the official Harness nor SDK is a protocol prerequisite.

This is a migration target, not a completed cutover. Existing native task slices
and read-only Game HTTP bootstrap retain their actual scope. Internal calls may
avoid network serialization only if they preserve the same identity, capability,
fence and receipt checks. World-owned tasks, leases, deterministic jobs, durable
results and recovery stay in Runtime. See [migration and verification](agent-game-client-boundary.design.md#des-agc-006)
and [implementation tracking](https://github.com/eng-cc/oasis7/issues/4726).
The external Skill/HTTP product goal and independent Runtime acceptance remain
unchanged; SDK extraction does not turn the product into a CLI-only interface.

## Web self-supplied model connections

The [product PRD](../../product/agents-world-simulation/self-supplied-model-connection.prd.md)
and [paired interaction design](../../product/agents-world-simulation/self-supplied-model-connection.design.md)
define browser-direct use of a player's DeepSeek/OpenRouter inference Key.
The [model-connection system design](self-supplied-model-connection.design.md#des-byok-001)
places the Key in site localStorage and protocol adapters and bounded Agent
execution in the visible browser page. The Key goes only to the selected model
provider, never to the Oasis7 backend, World/Runtime or Token Bridge.
This is a target with separate provider, deployment and gameplay verification;
it does not claim the browser migration is implemented or enable unattended hosting.

## Authority map

- `decision-provider-contract.prd.md` and `.design.md` define the provider-
  agnostic decision boundary: providers propose structured decisions, while the
  runtime validates and executes actions, owns world facts, and returns ordered
  feedback.
- Their [builtin input contract](decision-provider-contract.prd.md#builtin-input-integrity)
  and [request design](decision-provider-contract.design.md#builtin-request-contract)
  own required current observations, goals, query-tool replies, final-request
  capacity checks and configuration-insufficient failure. The separate
  [parameter contract](decision-provider-contract.prd.md#builtin-parameter-consistency)
  keeps builtin text, tool schema, parser and Runtime constraints consistent.
  These are requirements; implementation and verification remain PR evidence.
- `continuous-agent-harness.prd.md` and `.design.md` define the Agent-owned
  cognition lifecycle above that provider boundary: session/turn/request
  identity, provider-neutral context and feedback isolation, memory-write
  policy, bounded continuation, and one Harness shared by Builtin and
  ProviderBacked implementations. Runtime scheduling, action MVCC, durable
  cognition journal, recovery, and receipts remain owned by the paired
  `doc/world-runtime/runtime/agent-cognition-lifecycle.*` contract.
- The Continuous Harness target uses a non-recursive V1 `request_digest`
  (canonical outer context without its output digest or `transport_attempt`) and a global per-agent
  active-session/turn mutex; session IDs partition state but do not permit
  concurrent turns for one Agent. `committed/rejected/failed/pending` are the
  only feedback statuses; cancellation and scheduler backpressure map through
  explicit terminal/pending reasons and late-response cleanup.
- Loopback target decision traffic uses
  `POST /v1/world-simulator/decision-context` with the
  `ContinuousAgentRequestContextV1` / `ContinuousAgentResponseContextV1` outer wrappers;
  target feedback uses `POST /v1/world-simulator/feedback-context` with `FeedbackEnvelopeV1`.
  Bare `/v1/world-simulator/decision` and `/v1/world-simulator/feedback` are
  legacy compatibility-only routes for the old DTO bodies. The current HTTP body does not carry
  a `compatibility_lane` field; route/adapter selection is the explicit lane boundary.
- The target response wrapper preserves tagged `wait/wait_ticks/act/query/module_command/
  module_command_response` variants; Query is read-only, and typed module responses require host
  validation before Runtime handling.
- Target memory writes use `MemoryWriteIntentV1`; omitted summary is explicit `present=false`
  (present summaries are non-empty after normalization); tags may be an empty list, but empty tag
  elements are invalid. The legacy inner DTO is compatibility-lane only.
- P0 memory write scope is limited to `turn_private` and `session_private`; the reserved
  `agent_private_long_term` scope is disabled by default and deferred to P2 authority.
- Continuation proposals remain Harness policy inputs; Runtime projections carry the shared
  `FinalityBindingV1`/world bindings, and proposal/context digests use the paired `H_v1` registry.
- `provider-loopback-http-contract.prd.md` and `.design.md` define the local
  HTTP adapter and its explicit failure/fallback contract.
- `provider-agent-experience-parity.prd.md` and `.design.md` define parity
  evaluation and the conditions for remaining experimental or becoming a
  default experience.
- `provider-agent-dual-mode.prd.md` and
  `provider-agent-dual-mode-contract.md` define the `player_parity` and
  `headless_agent` execution/observation lanes. They share the same action schema
  and runtime validation; only their observation exposure differs. Formal
  player access modes remain `viewer` and `pure_api`, as defined by the
  [player-entry authority](../../product/player-entry-distribution/prd.md).
- `llm-factory-strategy-optimization.*` and `llm-lmso29-stability.*` retain
  builtin strategy, typed-decision, recovery and bounded stability-tuning
  requirements. Prompt-budget tuning consumes the
  [builtin input contract](decision-provider-contract.prd.md#builtin-input-integrity)
  and its paired request design; historical soft/peak-cap samples do not prove
  that current required inputs reach the next model request.

## Decision and evidence boundaries

- A provider is a candidate-decision source, never the authority for action
  semantics, resource effects, replay, memory truth, or player promises.
  Unknown, malformed, timed-out, or disallowed provider output must have a
  structured trace and follow the documented safe failure path.
- Provider-neutral contracts preserve each external Runtime's own planning,
  private memory and tool orchestration within the applicable authorization
  and budget. They standardize world-facing requests and outcomes, not the
  Runtime's internal implementation. Runtime-private beliefs cannot become
  authoritative world facts without the world's validated outcome.
- Prompt/module history, memory digests, raw provider output, token counts,
  latency, and retries are diagnostic or evaluation inputs. They do not by
  themselves prove parity, cost, replay closure, release readiness, or default
  enablement.
- Legacy DTOs and heuristic fallback are compatibility-only lanes: they require
  an explicitly selected legacy route or fixture/profile, and cannot satisfy production async,
  target parity, or automatic memory/continuation proof. The current HTTP wire does not define a
  `compatibility_lane` marker; absence of the target outer context remains non-proof. Target traces and feedback use fixed byte/count
  bounds, deterministic redaction, and negative overflow fixtures.
- Provider `info.capabilities` and `supported_action_sets` are provider metadata only. They are not
  a complete Runtime subject-bound capability discovery result; target requests must carry the
  Runtime-generated capability catalog and invocation context, and no standalone provider
  endpoint grants or discovers world authority.
- Evaluation evidence must retain a fixed scenario/fixture/profile/provider/
  adapter/protocol/timeout epoch, per-scenario artifacts, and repeated samples
  for nondeterministic providers. Aggregate counts or historical samples do
  not replace the current PRD-specific gates.
- `player_parity` evidence must not include headless-only information; a
  headless success does not establish player-experience parity. Dual-mode
  contract completion likewise does not grant default enablement; that remains
  subject to the parity PRD's behavior and latency gates.
- Running without a GUI assumes the external Runtime is still online and its
  budget and delegation remain valid. Closing a Viewer does not create hosted
  execution or continued computation after that Runtime stops. Each claimed
  Runtime/version/configuration and primary access mode needs its own evidence.

Use the linked PRD/design/contract for durable technical or product claims.
Use PRs and actual verification records for mutable work history.

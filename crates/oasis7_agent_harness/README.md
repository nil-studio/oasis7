# Agent harness

The existing portable actor, TurnEngine, continuation and provider primitives stay
available to their consumers. Native builds additionally expose
`NativeTaskRunner` and `SdkNativeTaskHost` for the explicit
native industry library profile. They consume the same signed HTTP SDK
as the ordinary CLI; neither the Harness nor SDK links the World or Viewer.

The current delivery prioritizes external Agent clients, including Codex. The
built-in industry profile remains explicitly opt-in. Per the operator decision
on 2026-10-15, real-model play and further built-in entrypoint refinement are
deferred TODOs; no paid inference was started. This does not establish complete
built-in capability migration or real-Runtime acceptance.

The native runner reads the full authorized task goal, constraints, observation
and capability catalog. It accepts strict observation, quote, move, transfer,
built-in build/schedule, bounded world-progress wait and abort tools. Unknown tools and authority arguments
fail explicitly. Module execution, legacy chat, private memory and continuation
are still served by their existing consumers and are not advertised by this
profile. It is not full built-in capability parity.

Each run has an immutable ID/configuration/task-fence binding and explicit model
call limit. Encrypted, locked budget reservations commit before each physical
model call and survive process restart. A failed, unknown or interrupted call
still consumes its reservation. Increasing the same run's limit or changing its
configuration fails closed; intentionally starting another run needs a new ID
and a new operator-authorized budget. Local storage is not an anti-rollback
hardware boundary.

A bounded worker runs model inference while the runner independently polls
authenticated task control. Stop, changed goal/epoch, expiry and timeout discard
late model output. This does not claim upstream cancellation or refunded usage.
Original operation IDs, signatures and cached results are recovered before new
contexts; unknown originals stop progress instead of acquiring replacement IDs.
World progress and authoritative admission remain with the Runtime.

The built-in model consumer uses the trusted launcher BYOK session boundary,
not a node-process environment-key worker. The launcher authenticates the owner,
binds an authorized GameClient executor, and supplies a `SessionModelProvider`
from [`oasis7_model_provider`](../oasis7_model_provider/README.md). A model
connection alone does not grant task authority or establish that a worker started.
Missing trusted executor/session bindings fail closed. Legacy simulator consumers
retain their existing behavior independently of this native library.

Real-model play through the trusted launcher BYOK entry requires explicit user
authorization and is not exercised by ordinary CI. Scripted HTTP tests do not
establish UI startup, paid-provider acceptance, full legacy capability parity,
or complete goal accomplishment.

The deterministic fake-host/provider tests and real HTTP conformance fixture
exercise protocol behavior. Initial shared-client harness routing landed under
[#4726](https://github.com/nil-studio/oasis7/issues/4726); scripted tests still
do not replace authorized real-model play, full legacy capability migration,
performance comparison, or deployment evidence.

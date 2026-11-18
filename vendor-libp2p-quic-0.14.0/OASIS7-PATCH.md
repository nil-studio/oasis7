# QUIC inbound admission patch

Source: libp2p/rust-libp2p revision `2d8497b2615086bd88018ef3a1fb32bc613a07ca`,
`transports/quic`, package 0.14.0. MIT license retained in LICENSE and source headers.
Cargo workspace dependencies are expanded to the same pinned git revision or
upstream registry versions; no dependency version upgrade is intended. The
workspace vendors the production library. The upstream unit and integration
suites remain in the pinned upstream tree. Oasis7 exercises the patched crate
through the root-lock admission regressions below.

Refs #5506. Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3.

The upstream transport calls Quinn Incoming::accept before returning an Incoming
transport event. Quinn then drives TLS independently, before Swarm has evaluated
its pending-inbound policy. Retain Incoming in the existing Connecting future
until its first poll, after that policy has accepted the upgrade. Dropping a denied
upgrade refuses the unaccepted Quinn Incoming. Preserve the deadline from receipt,
including time spent waiting for admission; an expired unpolled upgrade refuses
without starting TLS. Outbound and hole-punch future types remain unchanged.

This is a transport ordering prerequisite only. It does not implement #5506's
connection pools, byte quotas, development configuration, or resource status, and
does not claim complete resource-budget or formal network qualification. Initial
packet parsing, OS buffers and Quinn endpoint bookkeeping remain outside this
handshake ordering guarantee.

Regression entry: `cargo test -p oasis7_net --lib quic_inbound_admission`.

Compatibility detail: an Incoming::accept error is now returned by the specific
listener upgrade as ConnectionError, rather than emitted as ListenerError. This
attributes rejection to its connection attempt instead of the listening socket.
The four admission timing cases exercise real QUIC transports; they are not full
resource-budget or production qualification tests.
The additional Swarm regression configures zero pending inbound slots and checks
that the real pending-inbound policy produces IncomingConnectionError::Denied
and the remote dial fails. Upgrade errors use the existing pool task mapping to
PendingInboundConnectionError::Transport and Swarm ListenFailure /
IncomingConnectionError; they do not terminate the listener.

Regressions live in oasis7_net's library test target so existing net CI executes
them. The wasm-builder manifest-only context also carries this root path patch;
its metadata stub is not transport source qualification.

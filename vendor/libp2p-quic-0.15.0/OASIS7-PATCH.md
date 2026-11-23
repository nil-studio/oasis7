# QUIC inbound admission patch

Source: `libp2p/rust-libp2p` revision `2cb3c04346fdcb0ecf6717728664602378edac13`,
`transports/quic`, package `0.15.0`. MIT license retained in `LICENSE` and source headers.
Cargo workspace dependencies are expanded to the same pinned git revision or
upstream registry versions; no dependency version upgrade is intended. All
unchanged upstream `.rs` files are verified against `upstream-source-sha256.json`,
and the two patched `.rs` files (`src/connection/connecting.rs`, `src/transport.rs`)
are verified against `local-patch-sha256.json`.

Refs #5506, #6605, #6670. Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3.

## Behavior & Upstream Contribution Summary

Upstream `libp2p-quic` calls `quinn::Incoming::accept()` inside `GenTransport::poll`
before emitting `TransportEvent::Incoming`. Quinn immediately spawns and drives
the server TLS handshake before `Swarm` has evaluated `NetworkBehaviour::handle_pending_inbound_connection`.

This minimal two-file patch:
1. Retains `quinn::Incoming` inside `Connecting` (`incoming: Option<quinn::Incoming>`) until
   `Connecting` is polled for the first time (after `Swarm` pending-inbound admission
   has accepted the upgrade);
2. Drops unpolled denied upgrades cleanly, causing Quinn to refuse the unaccepted
   `Incoming` (`CONNECTION_REFUSED`) before starting TLS state allocation;
3. Captures the handshake timeout deadline at packet receipt (`Connecting::incoming`)
   rather than first poll so admission delay cannot extend the total handshake budget;
4. Maps `Incoming::accept()` failure to `Error::Connection(ConnectionError)` on that
   specific listener upgrade future instead of emitting a listener-wide `ListenerError`.

When upstream `libp2p/rust-libp2p` merges an equivalent deferred-`Incoming::accept()`
change in `transports/quic`, delete `vendor/libp2p-quic-*` and its `[patch]` entry in
root `Cargo.toml`, then verify `cargo test --locked -p oasis7_net --features libp2p --lib quic_inbound_admission`.

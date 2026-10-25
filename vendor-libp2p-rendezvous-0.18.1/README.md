# libp2p Rendezvous 0.18.1 compatibility snapshot

Source: `rust-libp2p` revision
`2d8497b2615086bd88018ef3a1fb32bc613a07ca`, `protocols/rendezvous`.
The upstream MIT license and source copyright notices are preserved.

This local snapshot lets the Oasis native rendezvous client schedule
registration and discovery through its shared work budget. The standalone Cargo
manifest expands the upstream workspace dependencies and pins libp2p git
dependencies to the same revision. The upstream `swarm-test` integration
dependency is omitted because its workspace path is not present in this
standalone package. Copied upstream source includes generated protobuf source
(`src/generated/rpc.proto`).

Local changes are confined to `src/client.rs` and its unit tests:

- Automatic registration refresh on external-address changes remains enabled
  by default. `Behaviour::with_automatic_registration_refresh(false)` disables
  only that automatic loop; address tracking and forwarding to the inner
  request-response behaviour continue unchanged. The setting is fixed when the
  behaviour is built; live policy migration is not provided.
- `register_with_request_id` and `discover_with_request_id` expose the real
  libp2p request ID. Existing `register` and `discover` signatures remain
  source-compatible and delegate to the new methods.
- `Registered`, `RegisterFailed`, `Discovered`, and `DiscoverFailed` expose
  the originating `OutboundRequestId`; `Expired` remains unchanged.
- A response with the wrong message kind, including a request message received
  as a response, ends the matching outstanding operation exactly once with
  `ErrorCode::InternalError` instead of leaving it pending or panicking.

No wire message, codec, generated protobuf, or rendezvous server behavior is
changed. The public client event API is extended with request IDs, so consumers
that exhaustively construct or destructure those event variants may need to
update.

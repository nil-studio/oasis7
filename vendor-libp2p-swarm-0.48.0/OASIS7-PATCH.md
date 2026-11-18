# Transport attempt admission

The fixed libp2p-swarm 0.48.0 snapshot exposes an optional outbound attempt
admission callback on Swarm config. The callback receives the Swarm connection
ID, expected peer and individual address before transport dial construction.
One Swarm connection can generate multiple address attempts. A denied address
never reaches the transport and does not enter the remote failure list.

An accepted attempt owns the caller's permit. On cancellation the transport
future is dropped first. A winning attempt transfers the permit into its muxer,
so requesting close does not release the permit before the underlying muxer
terminates. Constructors with no callback preserve the default path.

The optional inbound callback receives the connection ID and both transport
addresses after behaviour admission and before the listener upgrade is
polled. A local refusal drops the upgrade. A permitted upgrade transfers its
permit to the connected muxer on success. The handshake-start boundary
requires a transport that defers handshake start until upgrade polling.

Refs #5506. Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3.
The development resource profile still needs pending and established accounting,
authenticated peer transfer, time limits, and shared I/O admission before
activation.

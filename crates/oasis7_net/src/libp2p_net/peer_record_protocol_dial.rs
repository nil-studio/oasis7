//! One-shot authority from validated protocol operations, never peer-wide address trust.
use libp2p::swarm::{ConnectionId, NetworkBehaviour};
use libp2p::{Multiaddr, PeerId};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub(super) struct ProtocolDialOrigins(Arc<Mutex<HashSet<ConnectionId>>>);

impl ProtocolDialOrigins {
    pub(super) fn take(&self, id: ConnectionId) -> bool {
        self.0.lock().is_ok_and(|mut origins| origins.remove(&id))
    }
    pub(super) fn cleanup(&self, id: ConnectionId) {
        let _ = self.take(id);
    }
}

pub(super) struct ProtocolDialBehaviour<B> {
    inner: B,
    origins: ProtocolDialOrigins,
}

impl<B> ProtocolDialBehaviour<B> {
    pub(super) fn new(inner: B, origins: ProtocolDialOrigins) -> Self {
        Self { inner, origins }
    }
}
impl<B> std::ops::Deref for ProtocolDialBehaviour<B> {
    type Target = B;
    fn deref(&self) -> &B {
        &self.inner
    }
}
impl<B> std::ops::DerefMut for ProtocolDialBehaviour<B> {
    fn deref_mut(&mut self) -> &mut B {
        &mut self.inner
    }
}

impl<B: NetworkBehaviour> NetworkBehaviour for ProtocolDialBehaviour<B> {
    type ConnectionHandler = B::ConnectionHandler;
    type ToSwarm = B::ToSwarm;

    fn handle_pending_inbound_connection(
        &mut self,
        id: libp2p::swarm::ConnectionId,
        local: &Multiaddr,
        remote: &Multiaddr,
    ) -> Result<(), libp2p::swarm::ConnectionDenied> {
        self.inner
            .handle_pending_inbound_connection(id, local, remote)
    }
    fn handle_established_inbound_connection(
        &mut self,
        id: libp2p::swarm::ConnectionId,
        peer: PeerId,
        local: &Multiaddr,
        remote: &Multiaddr,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        self.inner
            .handle_established_inbound_connection(id, peer, local, remote)
    }
    fn handle_pending_outbound_connection(
        &mut self,
        id: libp2p::swarm::ConnectionId,
        peer: Option<PeerId>,
        addresses: &[Multiaddr],
        role: libp2p::core::Endpoint,
    ) -> Result<Vec<Multiaddr>, libp2p::swarm::ConnectionDenied> {
        self.inner
            .handle_pending_outbound_connection(id, peer, addresses, role)
    }

    fn handle_established_outbound_connection(
        &mut self,
        id: libp2p::swarm::ConnectionId,
        peer: PeerId,
        address: &Multiaddr,
        role: libp2p::core::Endpoint,
        port: libp2p::core::transport::PortUse,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        self.inner
            .handle_established_outbound_connection(id, peer, address, role, port)
    }
    fn on_swarm_event(&mut self, event: libp2p::swarm::FromSwarm<'_>) {
        self.inner.on_swarm_event(event);
    }
    fn on_connection_handler_event(
        &mut self,
        peer: PeerId,
        id: libp2p::swarm::ConnectionId,
        event: libp2p::swarm::THandlerOutEvent<Self>,
    ) {
        self.inner.on_connection_handler_event(peer, id, event);
    }
    fn poll(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<libp2p::swarm::ToSwarm<Self::ToSwarm, libp2p::swarm::THandlerInEvent<Self>>>
    {
        let action = self.inner.poll(cx);
        if let std::task::Poll::Ready(libp2p::swarm::ToSwarm::Dial { opts }) = &action
            && let Ok(mut origins) = self.origins.0.lock()
        {
            origins.insert(opts.connection_id());
        }
        action
    }
}

#[cfg(test)]
pub(super) fn emit_test_dial(
    origins: ProtocolDialOrigins,
    opts: libp2p::swarm::dial_opts::DialOpts,
) {
    tests::emit_origin(origins, opts);
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::swarm::{ToSwarm, dial_opts::DialOpts};
    use std::task::{Context, Poll};

    struct Fixture {
        inner: libp2p::swarm::dummy::Behaviour,
        opts: Option<DialOpts>,
    }
    impl NetworkBehaviour for Fixture {
        type ConnectionHandler =
            <libp2p::swarm::dummy::Behaviour as NetworkBehaviour>::ConnectionHandler;
        type ToSwarm = std::convert::Infallible;

        fn handle_pending_inbound_connection(
            &mut self,
            id: libp2p::swarm::ConnectionId,
            local: &Multiaddr,
            remote: &Multiaddr,
        ) -> Result<(), libp2p::swarm::ConnectionDenied> {
            self.inner
                .handle_pending_inbound_connection(id, local, remote)
        }
        fn handle_established_inbound_connection(
            &mut self,
            id: libp2p::swarm::ConnectionId,
            peer: PeerId,
            local: &Multiaddr,
            remote: &Multiaddr,
        ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
            self.inner
                .handle_established_inbound_connection(id, peer, local, remote)
        }
        fn handle_pending_outbound_connection(
            &mut self,
            id: libp2p::swarm::ConnectionId,
            peer: Option<PeerId>,
            addresses: &[Multiaddr],
            role: libp2p::core::Endpoint,
        ) -> Result<Vec<Multiaddr>, libp2p::swarm::ConnectionDenied> {
            self.inner
                .handle_pending_outbound_connection(id, peer, addresses, role)
        }

        fn handle_established_outbound_connection(
            &mut self,
            id: libp2p::swarm::ConnectionId,
            peer: PeerId,
            address: &Multiaddr,
            role: libp2p::core::Endpoint,
            port: libp2p::core::transport::PortUse,
        ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
            self.inner
                .handle_established_outbound_connection(id, peer, address, role, port)
        }
        fn on_swarm_event(&mut self, event: libp2p::swarm::FromSwarm<'_>) {
            self.inner.on_swarm_event(event);
        }
        fn on_connection_handler_event(
            &mut self,
            peer: PeerId,
            id: libp2p::swarm::ConnectionId,
            event: libp2p::swarm::THandlerOutEvent<Self>,
        ) {
            self.inner.on_connection_handler_event(peer, id, event);
        }
        fn poll(
            &mut self,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<
            libp2p::swarm::ToSwarm<Self::ToSwarm, libp2p::swarm::THandlerInEvent<Self>>,
        > {
            let _ = cx;
            self.opts.take().map_or(std::task::Poll::Pending, |opts| {
                std::task::Poll::Ready(libp2p::swarm::ToSwarm::Dial { opts })
            })
        }
    }

    pub(super) fn emit_origin(origins: ProtocolDialOrigins, opts: DialOpts) {
        let mut child = ProtocolDialBehaviour::new(
            Fixture {
                inner: libp2p::swarm::dummy::Behaviour,
                opts: Some(opts),
            },
            origins,
        );
        let mut cx = Context::from_waker(futures::task::noop_waker_ref());
        assert!(matches!(
            child.poll(&mut cx),
            Poll::Ready(ToSwarm::Dial { .. })
        ));
    }

    #[test]
    fn child_emitted_dials_receive_consumable_origin_in_both_roles() {
        for listener in [false, true] {
            let peer = PeerId::random();
            let builder =
                DialOpts::peer_id(peer).addresses(vec!["/ip4/127.0.0.1/tcp/5000".parse().unwrap()]);
            let opts = if listener {
                builder.override_role().build()
            } else {
                builder.build()
            };
            let id = opts.connection_id();
            let origins = ProtocolDialOrigins::default();
            assert!(!origins.take(id));
            let mut child = ProtocolDialBehaviour::new(
                Fixture {
                    inner: libp2p::swarm::dummy::Behaviour,
                    opts: Some(opts),
                },
                origins.clone(),
            );
            let mut cx = Context::from_waker(futures::task::noop_waker_ref());
            assert!(matches!(
                child.poll(&mut cx),
                Poll::Ready(ToSwarm::Dial { .. })
            ));
            assert!(origins.take(id));
            assert!(!origins.take(id));
            assert!(child.poll(&mut cx).is_pending());
        }
    }

    #[test]
    fn failed_or_condition_rejected_dial_cleans_its_one_shot_origin() {
        let opts = DialOpts::peer_id(PeerId::random()).build();
        let id = opts.connection_id();
        let origins = ProtocolDialOrigins::default();
        let mut child = ProtocolDialBehaviour::new(
            Fixture {
                inner: libp2p::swarm::dummy::Behaviour,
                opts: Some(opts),
            },
            origins.clone(),
        );
        let mut cx = Context::from_waker(futures::task::noop_waker_ref());
        assert!(child.poll(&mut cx).is_ready());
        origins.cleanup(id);
        assert!(!origins.take(id));
    }
}

use super::*;
use crate::NodeError;

#[test]
fn owned_gossip_socket_survives_handoff_and_stop_releases_it_for_a_fresh_restart() {
    let root = PrivateRoot::new();
    let identities = identities();
    let sockets: Vec<_> = (0..identities.len())
        .map(|_| UdpSocket::bind("127.0.0.1:0").unwrap())
        .collect();
    let mut addresses: Vec<_> = sockets
        .iter()
        .map(|socket| socket.local_addr().unwrap())
        .collect();
    let first_address = addresses[0];
    let mut sockets = sockets.into_iter();
    let socket = sockets.next().unwrap();
    let _offline = sockets.collect::<Vec<_>>();
    let mut node = make_node(&identities[0], 0, &addresses, root.path());
    node.gossip_socket_for_test = Some(socket);
    assert_eq!(
        UdpSocket::bind(first_address).unwrap_err().kind(),
        std::io::ErrorKind::AddrInUse
    );
    node.start()
        .expect("start must consume the owned socket without a second bind");
    assert_eq!(
        node.gossip_endpoint
            .as_ref()
            .unwrap()
            .local_addr_for_test()
            .unwrap(),
        first_address
    );
    let original_endpoint = Arc::downgrade(node.gossip_endpoint.as_ref().unwrap());
    node.stop().expect("join worker and release its socket");
    assert!(original_endpoint.upgrade().is_none());
    // Occupy the original port deterministically. A real restart must acquire
    // a fresh socket; no retained clone may mask the stop/restart lifecycle.
    let competitor = UdpSocket::bind(first_address).expect("stopped Node closed its socket");
    let successor_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    addresses[0] = successor_socket.local_addr().unwrap();
    assert_ne!(addresses[0], first_address);
    let mut successor = make_node(&identities[0], 0, &addresses, root.path());
    successor.gossip_socket_for_test = Some(successor_socket);
    successor
        .start()
        .expect("restart with a genuinely fresh bound socket");
    assert_eq!(
        successor
            .gossip_endpoint
            .as_ref()
            .unwrap()
            .local_addr_for_test()
            .unwrap(),
        addresses[0]
    );
    successor.stop().expect("stop successor");
    drop(competitor);
}

/// Each incarnation owns its actual kernel binding. Address-only probes are
/// never used for this mesh, and no clone keeps a stopped Node's socket alive.
pub(super) struct ReservedMesh {
    addresses: Vec<SocketAddr>,
    sockets: Vec<Option<UdpSocket>>,
}
impl ReservedMesh {
    pub(super) fn new(count: usize) -> Self {
        let sockets: Vec<_> = (0..count)
            .map(|_| Some(UdpSocket::bind("127.0.0.1:0").expect("reserve actual Node socket")))
            .collect();
        let addresses = sockets
            .iter()
            .map(|socket| socket.as_ref().unwrap().local_addr().unwrap())
            .collect();
        Self { addresses, sockets }
    }
    pub(super) fn address(&self, index: usize) -> SocketAddr {
        self.addresses[index]
    }
    pub(super) fn fresh(&mut self, index: usize) {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("fresh Node incarnation socket");
        self.addresses[index] = socket.local_addr().unwrap();
        // Discard any offline reservation's queued traffic, never carrying it
        // into a late or restarted worker.
        self.sockets[index] = Some(socket);
    }
    pub(super) fn node(
        &mut self,
        identity: &NodeIdentity,
        index: usize,
        root: &Path,
    ) -> NodeRuntime {
        let mut node = make_node(identity, index, &self.addresses, root);
        node.gossip_socket_for_test = Some(
            self.sockets[index]
                .take()
                .expect("one socket handoff per incarnation"),
        );
        node
    }
    pub(super) fn refresh_live_peers(&self, nodes: &mut [NodeRuntime]) {
        for node in nodes {
            let config = node.config.gossip.as_mut().expect("gossip configured");
            let endpoint = node.gossip_endpoint.as_ref().expect("live Node endpoint");
            assert_eq!(endpoint.local_addr_for_test().unwrap(), config.bind_addr);
            assert!(self.addresses.contains(&config.bind_addr));
            let mut peers = self
                .addresses
                .iter()
                .copied()
                .filter(|peer| *peer != config.bind_addr)
                .collect::<Vec<_>>();
            peers.sort_unstable();
            config.peers = peers.clone();
            endpoint
                .replace_static_peers_for_test(&peers)
                .expect("refresh actual static mesh addresses");
        }
    }
}

#[test]
fn reserved_socket_rejects_a_different_configured_address() {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let address = socket.local_addr().unwrap();
    let other = UdpSocket::bind("127.0.0.1:0").unwrap();
    let config = crate::NodeGossipConfig {
        bind_addr: other.local_addr().unwrap(),
        peers: vec![],
        max_dynamic_peers: 1,
        dynamic_peer_ttl_ms: 1,
    };
    assert!(
        matches!(crate::GossipEndpoint::from_reserved_for_test(&config, socket), Err(NodeError::Gossip { reason }) if reason.contains("differs from configured"))
    );
    assert!(
        UdpSocket::bind(address).is_ok(),
        "rejected handoff releases its socket"
    );
}

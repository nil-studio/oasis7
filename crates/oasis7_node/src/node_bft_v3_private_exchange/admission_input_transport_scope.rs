//! The same bounded transport scope serves candidate and admission protocols.
//! An independent provider can own it without inventing a Node execution hook.
use super::*;
use std::collections::BTreeSet;
#[derive(Clone)]
pub struct NodeBftPrivateTransportScopeV1 {
    pub(super) cache: Arc<Mutex<super::server::Cache>>,
    buffers: Arc<crate::NodeBftPrivateTransportBuffersV1>,
    peers: Arc<BTreeSet<libp2p::PeerId>>,
    timeout: Duration,
    alive: Arc<AtomicBool>,
}
impl NodeBftPrivateTransportScopeV1 {
    pub fn new(peers: Vec<String>, timeout: Duration) -> Result<Self, String> {
        if peers.len() > 256 || timeout.is_zero() || timeout > Duration::from_secs(600) {
            return Err("private shared transport scope capacity".into());
        }
        let identities = peers
            .iter()
            .map(|peer| {
                peer.parse::<libp2p::PeerId>()
                    .map_err(|_| "private shared Noise route identity".to_string())
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        if identities.len() != peers.len() {
            return Err("private shared route duplicate".into());
        }
        let canonical = identities.iter().map(ToString::to_string);
        Ok(Self {
            cache: Arc::new(Mutex::new(super::server::Cache::for_peers(canonical))),
            buffers: Arc::new(crate::NodeBftPrivateTransportBuffersV1::default()),
            peers: Arc::new(identities),
            timeout,
            alive: Arc::new(AtomicBool::new(true)),
        })
    }
    pub(super) fn close(&self) {
        self.alive.store(false, Ordering::Release);
    }
    pub(super) fn release_guard(&self) -> Arc<AtomicBool> {
        self.alive.clone()
    }
    pub fn buffers(&self) -> Arc<crate::NodeBftPrivateTransportBuffersV1> {
        self.buffers.clone()
    }
    pub(super) fn contains(&self, peer: &libp2p::PeerId) -> bool {
        self.peers.contains(peer)
    }
    pub(super) fn original_deadline(
        &self,
        peer: String,
        nonce: String,
        base: String,
        identity: String,
        manifest: bool,
    ) -> Result<Instant, String> {
        if !self.alive.load(Ordering::Acquire)
            || !self.peers.iter().any(|known| known.to_string() == peer)
        {
            return Err("private shared original route unavailable".into());
        }
        self.cache
            .lock()
            .map_err(|_| "private cache poisoned")?
            .admission_deadline(peer, nonce, base, identity, manifest, self.timeout, None)
    }
    pub(super) fn input_deadline(
        &self,
        peer: String,
        nonce: String,
        base: String,
        identity: String,
        manifest: bool,
    ) -> Result<Instant, String> {
        if !self.alive.load(Ordering::Acquire)
            || !self.peers.iter().any(|known| known.to_string() == peer)
        {
            return Err("private shared original route unavailable".into());
        }
        let hint = super::wire::digest(
            b"oasis7.bft.admission-input.hint-nonce.v1",
            nonce.as_bytes(),
        );
        self.cache
            .lock()
            .map_err(|_| "private cache poisoned")?
            .admission_deadline(
                peer,
                nonce,
                base,
                identity,
                manifest,
                self.timeout,
                Some(&hint),
            )
    }
}
/// This adapter transfers only the already bounded scope; it grants neither
/// private reads nor validator/provider authority.
pub trait NodeBftAdmissionInputTransportV1: Send + Sync {
    fn admission_transport(&self) -> NodeBftPrivateTransportScopeV1;
}
impl NodeBftAdmissionInputTransportV1 for NodeBftPrivateTransportScopeV1 {
    fn admission_transport(&self) -> NodeBftPrivateTransportScopeV1 {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn candidate_and_input_scope_clones_keep_one_nonce_budget_and_deadline() {
        let peer = libp2p::PeerId::random().to_string();
        let scope = NodeBftPrivateTransportScopeV1::new(vec![peer.clone()], Duration::from_secs(1))
            .unwrap();
        let input = scope.clone();
        assert!(Arc::ptr_eq(&scope.cache, &input.cache));
        assert!(Arc::ptr_eq(&scope.buffers(), &input.buffers()));
        let nonce = format!("{:064x}", 0);
        let base = "11".repeat(32);
        let identity = "22".repeat(32);
        let original = scope
            .original_deadline(
                peer.clone(),
                nonce.clone(),
                base.clone(),
                identity.clone(),
                true,
            )
            .unwrap();
        assert_eq!(
            input
                .original_deadline(
                    peer.clone(),
                    nonce.clone(),
                    base.clone(),
                    identity.clone(),
                    false
                )
                .unwrap(),
            original
        );
        assert!(
            input
                .original_deadline(
                    peer.clone(),
                    nonce,
                    "33".repeat(32),
                    identity.clone(),
                    false
                )
                .is_err()
        );
        for n in 1..1024 {
            let consumer = if n % 2 == 0 { &scope } else { &input };
            consumer
                .original_deadline(
                    peer.clone(),
                    format!("{n:064x}"),
                    base.clone(),
                    identity.clone(),
                    true,
                )
                .unwrap();
        }
        assert!(
            input
                .original_deadline(peer, format!("{:064x}", 1024), base, identity, true)
                .is_err(),
            "cloning the scope must not grant a second original-nonce allowance"
        );
    }
    #[test]
    fn original_hint_and_manifest_share_one_attempt_deadline() {
        let peer = libp2p::PeerId::random().to_string();
        let scope = NodeBftPrivateTransportScopeV1::new(vec![peer.clone()], Duration::from_secs(1))
            .unwrap();
        let original = "44".repeat(32);
        let hint_nonce = super::super::wire::digest(
            b"oasis7.bft.admission-input.hint-nonce.v1",
            original.as_bytes(),
        );
        let hinted = scope
            .original_deadline(
                peer.clone(),
                hint_nonce,
                "11".repeat(32),
                "22".repeat(32),
                true,
            )
            .unwrap();
        let manifested = scope
            .input_deadline(
                peer.clone(),
                original.clone(),
                "33".repeat(32),
                "55".repeat(32),
                true,
            )
            .unwrap();
        assert_eq!(
            manifested, hinted,
            "issuance work must not restart the server operation budget"
        );
        assert_eq!(
            scope
                .input_deadline(peer, original, "33".repeat(32), "55".repeat(32), false)
                .unwrap(),
            hinted
        );
    }
    #[test]
    fn shared_transport_owner_close_revokes_other_protocol_release_guard() {
        let scope = NodeBftPrivateTransportScopeV1::new(
            vec![libp2p::PeerId::random().to_string()],
            Duration::from_secs(1),
        )
        .unwrap();
        let input = scope.clone();
        let allowed = input.release_guard();
        assert!(allowed.load(Ordering::Acquire));
        scope.close();
        assert!(!allowed.load(Ordering::Acquire));
    }
}

//! Final address authority for autonomous protocol dials, including cached Kademlia routes.
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use libp2p::multiaddr::Protocol;
use libp2p::{Multiaddr, PeerId};

use super::peer_record_time::RecordLease;

#[derive(Debug)]
struct RecordPaths {
    lease: RecordLease,
    addresses: HashSet<Multiaddr>,
}

#[derive(Debug, Default)]
struct DialPolicyState {
    configured: HashSet<(PeerId, Multiaddr)>,
    records: HashMap<PeerId, RecordPaths>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct DiscoveryDialPolicy(Arc<Mutex<DialPolicyState>>);

fn normalized(peer: PeerId, addr: &Multiaddr) -> Option<Multiaddr> {
    let mut addr = addr.clone();
    match addr.pop() {
        Some(Protocol::P2p(suffix)) if suffix != peer => return None,
        Some(Protocol::P2p(_)) => {}
        Some(protocol) => addr.push(protocol),
        None => return None,
    }
    Some(addr)
}

impl DiscoveryDialPolicy {
    /// Configuration authorizes exactly the endpoint bound to its configured peer.
    pub(super) fn set_configured(&self, addresses: &[Multiaddr]) {
        let Ok(mut state) = self.0.lock() else {
            return;
        };
        state.configured = addresses
            .iter()
            .filter_map(|addr| {
                let Some(Protocol::P2p(peer)) = addr.iter().last() else {
                    return None;
                };
                normalized(peer, addr).map(|addr| (peer, addr))
            })
            .collect();
    }

    pub(super) fn install(
        &self,
        peer: PeerId,
        addresses: impl IntoIterator<Item = Multiaddr>,
        lease: RecordLease,
    ) -> bool {
        let Ok(mut state) = self.0.lock() else {
            return false;
        };
        state.records.retain(|_, paths| paths.lease.is_valid());
        if !lease.is_valid() || (!state.records.contains_key(&peer) && state.records.len() >= 1024)
        {
            return false;
        }
        let mut normalized_addresses = HashSet::new();
        let mut bytes = 0usize;
        for addr in addresses {
            let Some(addr) = normalized(peer, &addr) else {
                return false;
            };
            bytes = bytes.saturating_add(addr.len());
            if bytes > 16 * 1024 {
                return false;
            }
            normalized_addresses.insert(addr);
        }
        state.records.insert(
            peer,
            RecordPaths {
                lease,
                addresses: normalized_addresses,
            },
        );
        true
    }

    pub(super) fn permits(&self, peer: Option<PeerId>, address: &Multiaddr) -> bool {
        let Some(peer) = peer else {
            return false;
        };
        let Some(address) = normalized(peer, address) else {
            return false;
        };
        let Ok(state) = self.0.lock() else {
            return false;
        };
        state.configured.contains(&(peer, address.clone()))
            || state
                .records
                .get(&peer)
                .is_some_and(|paths| paths.lease.is_valid() && paths.addresses.contains(&address))
    }

    pub(super) fn filter(&self, peer: Option<PeerId>, addresses: Vec<Multiaddr>) -> Vec<Multiaddr> {
        addresses
            .into_iter()
            .filter(|addr| self.permits(peer, addr))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::peer_record_time::DiscoveryClock;
    use super::*;
    use libp2p::identity::Keypair;

    fn address(port: u16) -> Multiaddr {
        format!("/ip4/127.0.0.1/tcp/{port}").parse().unwrap()
    }

    #[test]
    fn automatic_unknown_or_cached_unsigned_routes_are_deferred() {
        let policy = DiscoveryDialPolicy::default();
        let peer = PeerId::random();
        assert!(policy.filter(Some(peer), vec![address(1)]).is_empty());
        assert!(!policy.permits(None, &address(1)));
    }

    #[test]
    fn configuration_preserves_only_exact_peer_and_endpoint_and_revokes() {
        let policy = DiscoveryDialPolicy::default();
        let peer = PeerId::random();
        let mut configured = address(1);
        configured.push(Protocol::P2p(peer));
        policy.set_configured(&[configured.clone()]);
        assert!(policy.permits(Some(peer), &address(1)));
        assert!(policy.permits(Some(peer), &configured));
        assert!(!policy.permits(Some(PeerId::random()), &address(1)));
        assert!(!policy.permits(Some(peer), &address(2)));
        policy.set_configured(&[]);
        assert!(!policy.permits(Some(peer), &configured));
    }

    #[test]
    fn only_current_signed_record_paths_contribute_and_lease_expiry_revokes_cached_routes() {
        let policy = DiscoveryDialPolicy::default();
        let signed = super::super::tests::signed_discovery_peer_record(
            &Keypair::generate_ed25519(),
            vec![],
            super::super::utils::now_ms(),
        );
        let peer: PeerId = signed.record.peer_id.parse().unwrap();
        let clock = DiscoveryClock::default();
        let lease = clock.check(&signed.record).unwrap();
        assert!(policy.install(peer, vec![address(1)], lease.clone()));
        assert!(policy.permits(Some(peer), &address(1)));
        assert!(policy.install(peer, vec![address(2)], lease));
        assert!(!policy.permits(Some(peer), &address(1)));
        assert!(policy.permits(Some(peer), &address(2)));
        // Move known wall time beyond signed expiry. A cached protocol route
        // remains in its own store, but can no longer contribute an address.
        clock
            .observe(
                signed.record.expires_at_ms().unwrap(),
                std::time::Instant::now(),
            )
            .unwrap();
        assert!(!policy.permits(Some(peer), &address(2)));
    }
}

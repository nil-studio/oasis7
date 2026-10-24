use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use libp2p::identity::Keypair;
use libp2p::kad;
use libp2p::swarm::Swarm;
use libp2p::{Multiaddr, PeerId};

use crate::error::WorldError;
use crate::util::to_canonical_cbor;
use oasis7_proto::distributed::DistributedErrorCode;
use oasis7_proto::distributed_dht::PeerRecord;
use oasis7_proto::distributed_net::NetworkRequest;

use super::discovery::publish_discovery_provider;
use super::durable_peer_record_publisher::LocalPeerRecordPublisher;
use super::kad_queries::PendingDhtQuery;
use super::peer_record::{
    build_current_peer_record_for_remote_request, publish_configured_peer_record,
};
use super::peer_record_cache::PeerRecordCache;
use super::push_bounded_clone;
use super::reachability::Libp2pReachabilitySnapshot;
use super::swarm_behaviour::Behaviour;
use super::swarm_reachability_events::{
    handle_external_addr_confirmed, handle_external_addr_expired,
};
use super::utils::now_ms;

pub(super) const RR_GET_LOCAL_PEER_RECORD: &str = "/aw/rr/1.0.0/get_local_peer_record";
pub(super) const RR_GET_CACHED_PEER_RECORD: &str = "/aw/rr/1.0.0/get_cached_peer_record";
pub(super) const RR_GET_CACHED_DISCOVERY_PEERS: &str = "/aw/rr/1.0.0/get_cached_discovery_peers";

#[expect(
    clippy::too_many_arguments,
    reason = "Peer-record exchange uses the live publisher, cache, and reachability context"
)]
pub(super) fn handle_peer_record_exchange_request(
    request: &NetworkRequest,
    peer_record_template: Option<&PeerRecord>,
    keypair: &Keypair,
    listening_addrs: &Arc<Mutex<Vec<Multiaddr>>>,
    reachability: &Arc<Mutex<Libp2pReachabilitySnapshot>>,
    allow_loopback_external_addrs_for_testing: bool,
    discovered_peer_records: &PeerRecordCache,
    local_publisher: &Arc<Mutex<LocalPeerRecordPublisher>>,
) -> Result<Option<Vec<u8>>, WorldError> {
    let response = match request.protocol.as_str() {
        RR_GET_LOCAL_PEER_RECORD => {
            let Some(template) = peer_record_template else {
                return Err(WorldError::NetworkProtocolUnavailable {
                    protocol: RR_GET_LOCAL_PEER_RECORD.to_string(),
                });
            };
            let record = build_current_peer_record_for_remote_request(
                keypair,
                template,
                listening_addrs,
                reachability,
                allow_loopback_external_addrs_for_testing,
                local_publisher,
            )?;
            discovered_peer_records.clock().check(&record.record)?;
            Some(to_canonical_cbor(&record)?)
        }
        RR_GET_CACHED_PEER_RECORD => {
            let peer_id = String::from_utf8(request.payload.clone())
                .map_err(|_| WorldError::NetworkProtocolUnavailable {
                    protocol: "cached peer record payload must be utf-8".to_string(),
                })?
                .parse::<PeerId>()
                .map_err(|_| WorldError::NetworkProtocolUnavailable {
                    protocol: "cached peer record peer_id must be valid".to_string(),
                })?;
            let record = discovered_peer_records
                .get_for_forwarding(&peer_id)
                .ok_or_else(|| WorldError::NetworkRequestFailed {
                    code: DistributedErrorCode::ErrNotFound,
                    message: format!("cached peer record not found for peer={peer_id}"),
                    retryable: true,
                })?;
            Some(to_canonical_cbor(&record)?)
        }
        RR_GET_CACHED_DISCOVERY_PEERS => {
            let mut peers: Vec<String> = discovered_peer_records
                .values()
                .filter(|record| {
                    peer_record_template
                        .map(|template| {
                            record.record.world_id == template.world_id
                                && record.record.network_id == template.network_id
                        })
                        .unwrap_or(true)
                })
                .map(|record| record.record.peer_id.clone())
                .collect();
            peers.sort();
            peers.dedup();
            peers.truncate(super::discovery_budget::MAX_DISCOVERY_IDS);
            Some(to_canonical_cbor(&peers)?)
        }
        _ => None,
    };
    Ok(response)
}

pub(super) struct LocalPeerRecordRepublisher<'a> {
    pub keypair: &'a Keypair,
    pub peer_record_template: Option<&'a PeerRecord>,
    pub listening_addrs: &'a Arc<Mutex<Vec<Multiaddr>>>,
    pub reachability: &'a Arc<Mutex<Libp2pReachabilitySnapshot>>,
    pub event_error_sink: (&'a Arc<Mutex<Vec<String>>>, usize),
    pub allow_loopback_external_addrs_for_testing: bool,
    pub local_peer_record_publisher: &'a Arc<Mutex<LocalPeerRecordPublisher>>,
}

impl LocalPeerRecordRepublisher<'_> {
    pub(super) fn from_context<'a>(
        ctx: &super::runtime_loop::CommandContext<'a>,
    ) -> LocalPeerRecordRepublisher<'a> {
        LocalPeerRecordRepublisher {
            keypair: ctx.keypair,
            peer_record_template: ctx.peer_record_template,
            listening_addrs: ctx.event_listening_addrs,
            reachability: ctx.event_reachability,
            event_error_sink: (ctx.event_errors, ctx.max_error_messages),
            allow_loopback_external_addrs_for_testing: ctx
                .allow_loopback_external_addrs_for_testing,
            local_peer_record_publisher: ctx.local_peer_record_publisher,
        }
    }

    pub(super) fn republish(
        &self,
        swarm: &mut Swarm<Behaviour>,
        pending_dht: &mut HashMap<kad::QueryId, PendingDhtQuery>,
        provider_keys: &mut HashMap<String, i64>,
        peer_record_last_published_at_ms: &mut Option<i64>,
    ) {
        if let Some(template) = self.peer_record_template {
            match publish_configured_peer_record(
                swarm,
                pending_dht,
                self.keypair,
                template,
                self.listening_addrs,
                self.reachability,
                self.allow_loopback_external_addrs_for_testing,
                self.local_peer_record_publisher,
                None,
            ) {
                Ok(_) => {
                    *peer_record_last_published_at_ms = Some(now_ms());
                    publish_discovery_provider(swarm, provider_keys, template.world_id.as_str());
                }
                Err(err) => {
                    let (event_errors, max_error_messages) = self.event_error_sink;
                    push_bounded_clone(
                        event_errors,
                        format!("republish local peer record failed: {err:?}"),
                        max_error_messages,
                        "lock errors",
                    );
                }
            }
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Peer-record republish state remains explicit to preserve address and publication ordering"
)]
pub(super) fn log_external_addr_confirmed_and_republish(
    event_errors: &Arc<Mutex<Vec<String>>>,
    max_error_messages: usize,
    reachability: &Arc<Mutex<Libp2pReachabilitySnapshot>>,
    address: &Multiaddr,
    republisher: &LocalPeerRecordRepublisher<'_>,
    swarm: &mut Swarm<Behaviour>,
    pending_dht: &mut HashMap<kad::QueryId, PendingDhtQuery>,
    provider_keys: &mut HashMap<String, i64>,
    peer_record_last_published_at_ms: &mut Option<i64>,
) {
    push_bounded_clone(
        event_errors,
        handle_external_addr_confirmed(reachability, address),
        max_error_messages,
        "lock errors",
    );
    republisher.republish(
        swarm,
        pending_dht,
        provider_keys,
        peer_record_last_published_at_ms,
    );
}

#[expect(
    clippy::too_many_arguments,
    reason = "Peer-record republish state remains explicit to preserve address and publication ordering"
)]
pub(super) fn log_external_addr_expired_and_republish(
    event_errors: &Arc<Mutex<Vec<String>>>,
    max_error_messages: usize,
    reachability: &Arc<Mutex<Libp2pReachabilitySnapshot>>,
    address: &Multiaddr,
    republisher: &LocalPeerRecordRepublisher<'_>,
    swarm: &mut Swarm<Behaviour>,
    pending_dht: &mut HashMap<kad::QueryId, PendingDhtQuery>,
    provider_keys: &mut HashMap<String, i64>,
    peer_record_last_published_at_ms: &mut Option<i64>,
) {
    push_bounded_clone(
        event_errors,
        handle_external_addr_expired(reachability, address),
        max_error_messages,
        "lock errors",
    );
    republisher.republish(
        swarm,
        pending_dht,
        provider_keys,
        peer_record_last_published_at_ms,
    );
}

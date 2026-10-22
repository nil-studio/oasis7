use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use libp2p::Multiaddr;
use libp2p::identity::Keypair;
use libp2p::kad;
use libp2p::swarm::Swarm;

use oasis7_proto::distributed_dht::PeerRecord;

use super::discovery::publish_discovery_provider;
use super::kad_queries::PendingDhtQuery;
use super::peer_record::publish_configured_peer_record;
use super::push_bounded_clone;
use super::reachability::Libp2pReachabilitySnapshot;
use super::swarm_behaviour::Behaviour;
use super::swarm_reachability_events::{
    handle_external_addr_confirmed, handle_external_addr_expired,
};
use super::utils::now_ms;

pub(super) struct LocalPeerRecordRepublisher<'a> {
    pub keypair: &'a Keypair,
    pub peer_record_template: Option<&'a PeerRecord>,
    pub listening_addrs: &'a Arc<Mutex<Vec<Multiaddr>>>,
    pub reachability: &'a Arc<Mutex<Libp2pReachabilitySnapshot>>,
    pub event_error_sink: (&'a Arc<Mutex<Vec<String>>>, usize),
    pub allow_loopback_external_addrs_for_testing: bool,
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

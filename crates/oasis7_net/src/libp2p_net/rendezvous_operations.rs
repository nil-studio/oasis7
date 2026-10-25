use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::error::WorldError;
use libp2p::PeerId;
use libp2p::kad;
use libp2p::rendezvous;
use libp2p::swarm::Swarm;
use oasis7_proto::distributed::rendezvous_namespace as distributed_rendezvous_namespace;
use oasis7_proto::distributed_dht::{PeerDiscoverySource, PeerRecord};

use super::discovery::PendingPeerRecordRequest;
use super::discovery_budget::{RendezvousOperationKind, rendezvous_external_addresses_digest};
use super::kad_queries::PendingDhtQuery;
use super::peer_record_cache::PeerRecordCache;
use super::swarm_behaviour::Behaviour;
use super::traffic_metrics::SharedLibp2pTrafficMetrics;
use super::utils::push_bounded_clone;

pub(super) struct ClientEventContext<'a> {
    pub swarm: &'a mut Swarm<Behaviour>,
    pub pending_registers: &'a mut HashSet<PeerId>,
    pub registered_nodes: &'a mut HashSet<PeerId>,
    pub pending_discovers: &'a mut HashSet<PeerId>,
    pub cookies: &'a mut HashMap<PeerId, rendezvous::Cookie>,
    pub pending_dht: &'a mut HashMap<kad::QueryId, PendingDhtQuery>,
    pub pending_peer_record_requests:
        &'a mut HashMap<libp2p::request_response::OutboundRequestId, PendingPeerRecordRequest>,
    pub pending_discovery_peer_records: &'a mut HashSet<PeerId>,
    pub pending_cached_peer_records: &'a mut HashSet<PeerId>,
    pub cached_peer_record_cooldowns: &'a mut HashMap<PeerId, i64>,
    pub traffic_metrics: &'a SharedLibp2pTrafficMetrics,
    pub peers: &'a [PeerId],
    pub discovered_peer_records: &'a PeerRecordCache,
    pub local_peer_id: PeerId,
    pub peer_record_template: Option<&'a PeerRecord>,
    pub max_error_messages: usize,
    pub event_errors: &'a Arc<Mutex<Vec<String>>>,
}

pub(super) fn handle_client_event(event: rendezvous::client::Event, ctx: ClientEventContext<'_>) {
    match event {
        rendezvous::client::Event::Registered {
            request_id,
            rendezvous_node,
            ..
        } => {
            finish_register_event(
                ctx.swarm,
                request_id,
                rendezvous_node,
                true,
                ctx.pending_registers,
                ctx.registered_nodes,
            );
        }
        rendezvous::client::Event::RegisterFailed {
            request_id,
            rendezvous_node,
            namespace,
            error,
        } => {
            if finish_register_event(
                ctx.swarm,
                request_id,
                rendezvous_node,
                false,
                ctx.pending_registers,
                ctx.registered_nodes,
            ) {
                push_bounded_clone(
                    ctx.event_errors,
                    format!(
                        "libp2p rendezvous register rejected peer={rendezvous_node} namespace={namespace}: {error:?}"
                    ),
                    ctx.max_error_messages,
                    "lock errors",
                );
            }
        }
        rendezvous::client::Event::Discovered {
            request_id,
            rendezvous_node,
            registrations,
            cookie,
        } => {
            if finish_discover_event(
                ctx.swarm,
                request_id,
                rendezvous_node,
                true,
                ctx.pending_discovers,
            ) {
                ctx.cookies.insert(rendezvous_node, cookie);
                super::rendezvous_trigger::handle_rendezvous_discovered(
                    ctx.swarm,
                    rendezvous_node,
                    registrations,
                    ctx.pending_dht,
                    ctx.pending_peer_record_requests,
                    ctx.pending_discovery_peer_records,
                    ctx.pending_cached_peer_records,
                    ctx.cached_peer_record_cooldowns,
                    ctx.traffic_metrics,
                    ctx.peers,
                    ctx.discovered_peer_records,
                    ctx.local_peer_id,
                    ctx.peer_record_template,
                    ctx.max_error_messages,
                    ctx.event_errors,
                );
            }
        }
        rendezvous::client::Event::DiscoverFailed {
            request_id,
            rendezvous_node,
            namespace,
            error,
        } => {
            if finish_discover_event(
                ctx.swarm,
                request_id,
                rendezvous_node,
                false,
                ctx.pending_discovers,
            ) {
                push_bounded_clone(
                    ctx.event_errors,
                    format!(
                        "libp2p rendezvous discover rejected peer={rendezvous_node} namespace={namespace:?}: {error:?}"
                    ),
                    ctx.max_error_messages,
                    "lock errors",
                );
            }
        }
        rendezvous::client::Event::Expired { peer } => {
            ctx.cookies.remove(&peer);
        }
    }
}

pub(super) fn maybe_register_rendezvous_namespace(
    swarm: &mut Swarm<Behaviour>,
    pending_rendezvous_registers: &mut HashSet<PeerId>,
    registered_rendezvous_nodes: &HashSet<PeerId>,
    peer_id: PeerId,
    local_peer_id: PeerId,
    template: &PeerRecord,
) -> Result<bool, WorldError> {
    if !peer_record_enables_rendezvous(template) {
        return Ok(false);
    }
    if peer_id == local_peer_id || pending_rendezvous_registers.contains(&peer_id) {
        return Ok(false);
    }
    let namespace = rendezvous_namespace(template)?;
    let address_digest = current_external_addresses_digest(swarm)?;
    if registered_rendezvous_nodes.contains(&peer_id)
        && swarm
            .behaviour()
            .discovery_budget
            .rendezvous_registration_matches(peer_id, Some(address_digest))
    {
        return Ok(false);
    }
    if !swarm
        .behaviour_mut()
        .discovery_budget
        .begin_rendezvous(RendezvousOperationKind::Register, peer_id)
    {
        return Ok(false);
    }
    let result = swarm
        .behaviour_mut()
        .rendezvous_client
        .as_mut()
        .expect("rendezvous client enabled when peer record opts in")
        .register_with_request_id(namespace, peer_id, None);
    match result {
        Ok(request_id) => {
            swarm.behaviour_mut().discovery_budget.track_rendezvous(
                request_id,
                RendezvousOperationKind::Register,
                peer_id,
                Some(address_digest),
            );
            pending_rendezvous_registers.insert(peer_id);
            Ok(true)
        }
        Err(err) => {
            swarm
                .behaviour_mut()
                .discovery_budget
                .finish_rendezvous_schedule_failure(RendezvousOperationKind::Register, peer_id);
            Err(WorldError::NetworkProtocolUnavailable {
                protocol: format!("rendezvous register failed: {err}"),
            })
        }
    }
}

pub(super) fn maybe_discover_rendezvous_namespace(
    swarm: &mut Swarm<Behaviour>,
    pending_rendezvous_discovers: &mut HashSet<PeerId>,
    rendezvous_cookies: &HashMap<PeerId, rendezvous::Cookie>,
    peer_id: PeerId,
    local_peer_id: PeerId,
    template: &PeerRecord,
) -> Result<bool, WorldError> {
    if !peer_record_enables_rendezvous(template) {
        return Ok(false);
    }
    if peer_id == local_peer_id || pending_rendezvous_discovers.contains(&peer_id) {
        return Ok(false);
    }
    let namespace = rendezvous_namespace(template)?;
    if !swarm
        .behaviour_mut()
        .discovery_budget
        .begin_rendezvous(RendezvousOperationKind::Discover, peer_id)
    {
        return Ok(false);
    }
    let cookie = rendezvous_cookies.get(&peer_id).cloned();
    let request_id = swarm
        .behaviour_mut()
        .rendezvous_client
        .as_mut()
        .expect("rendezvous client enabled when peer record opts in")
        .discover_with_request_id(Some(namespace), cookie, None, peer_id);
    swarm.behaviour_mut().discovery_budget.track_rendezvous(
        request_id,
        RendezvousOperationKind::Discover,
        peer_id,
        None,
    );
    pending_rendezvous_discovers.insert(peer_id);
    Ok(true)
}

pub(super) fn finish_rendezvous_terminal(
    swarm: &mut Swarm<Behaviour>,
    request_id: libp2p::request_response::OutboundRequestId,
    kind: RendezvousOperationKind,
    peer_id: PeerId,
    succeeded: bool,
) -> bool {
    let current_addresses = if kind == RendezvousOperationKind::Register {
        current_external_addresses_digest(swarm).ok()
    } else {
        None
    };
    swarm.behaviour_mut().discovery_budget.finish_rendezvous(
        request_id,
        kind,
        peer_id,
        current_addresses,
        succeeded,
    )
}

pub(super) fn finish_register_event(
    swarm: &mut Swarm<Behaviour>,
    request_id: libp2p::request_response::OutboundRequestId,
    peer_id: PeerId,
    succeeded: bool,
    pending: &mut HashSet<PeerId>,
    registered: &mut HashSet<PeerId>,
) -> bool {
    if !finish_rendezvous_terminal(
        swarm,
        request_id,
        RendezvousOperationKind::Register,
        peer_id,
        succeeded,
    ) {
        return false;
    }
    pending.remove(&peer_id);
    if succeeded && rendezvous_registration_is_current(swarm, peer_id) {
        registered.insert(peer_id);
    } else {
        registered.remove(&peer_id);
    }
    true
}

pub(super) fn finish_discover_event(
    swarm: &mut Swarm<Behaviour>,
    request_id: libp2p::request_response::OutboundRequestId,
    peer_id: PeerId,
    succeeded: bool,
    pending: &mut HashSet<PeerId>,
) -> bool {
    if finish_rendezvous_terminal(
        swarm,
        request_id,
        RendezvousOperationKind::Discover,
        peer_id,
        succeeded,
    ) {
        pending.remove(&peer_id);
        true
    } else {
        false
    }
}

pub(super) fn rendezvous_registration_is_current(
    swarm: &Swarm<Behaviour>,
    peer_id: PeerId,
) -> bool {
    let Ok(address_digest) = current_external_addresses_digest(swarm) else {
        return false;
    };
    swarm
        .behaviour()
        .discovery_budget
        .rendezvous_registration_matches(peer_id, Some(address_digest))
}

pub(super) fn forget_rendezvous_registration(swarm: &mut Swarm<Behaviour>, peer_id: PeerId) {
    swarm
        .behaviour_mut()
        .discovery_budget
        .forget_rendezvous_registration(peer_id);
}

fn current_external_addresses_digest(swarm: &Swarm<Behaviour>) -> Result<[u8; 32], WorldError> {
    let client = swarm
        .behaviour()
        .rendezvous_client
        .as_ref()
        .ok_or_else(|| WorldError::NetworkProtocolUnavailable {
            protocol: "rendezvous client is not enabled".to_string(),
        })?;
    rendezvous_external_addresses_digest(client.external_addresses().cloned()).ok_or_else(|| {
        WorldError::NetworkProtocolUnavailable {
            protocol: "rendezvous external address set exceeds its tracking bound".to_string(),
        }
    })
}

pub(super) fn peer_record_enables_rendezvous(template: &PeerRecord) -> bool {
    template
        .discovery_sources
        .iter()
        .any(|source| matches!(source, PeerDiscoverySource::Rendezvous))
}

pub(super) fn rendezvous_namespace(
    template: &PeerRecord,
) -> Result<rendezvous::Namespace, WorldError> {
    rendezvous::Namespace::new(distributed_rendezvous_namespace(
        template.world_id.as_str(),
        template.network_id.as_str(),
    ))
    .map_err(|_| WorldError::NetworkProtocolUnavailable {
        protocol: "rendezvous namespace must be <= 255 bytes".to_string(),
    })
}

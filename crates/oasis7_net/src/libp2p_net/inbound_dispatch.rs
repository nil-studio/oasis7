use super::peer_record_cache::PeerRecordCache;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use libp2p::identity::Keypair;
use libp2p::request_response;
use libp2p::swarm::Swarm;
use libp2p::{Multiaddr, PeerId};
use oasis7_proto::distributed_dht::PeerRecord;
use oasis7_proto::distributed_net::{NetworkRequest, NetworkResponse};

use crate::error::WorldError;

use super::discovery::handle_request_response_request;
use super::response_budget::FetchBlobResponseBudget;
use super::response_workers::ResponseWorkers;
use super::runtime_loop::{Handler, HandlerRegistration};
use super::{
    Behaviour, Libp2pReachabilitySnapshot, SharedLibp2pTrafficMetrics, now_ms, push_bounded_clone,
    record_request_inbound,
};

const PRE_CRYPTO_WINDOW_MS: i64 = 1_000;
const PRE_CRYPTO_PER_PEER_REQUESTS: u16 = 16;
const PRE_CRYPTO_GLOBAL_REQUESTS: u16 = 128;
const PRE_CRYPTO_MAX_PEERS: usize = 256;
const EXPENSIVE_FETCH_BLOB_PROTOCOL: &str = "/aw/node/replication/fetch-blob/1.0.0";
const EXPENSIVE_FETCH_COMMIT_PROTOCOL: &str = "/aw/node/replication/fetch-commit/1.0.0";

#[derive(Default)]
pub(super) struct PreCryptoAdmissionBudget {
    window_started_at_ms: i64,
    global: u16,
    peers: HashMap<PeerId, u16>,
}

impl PreCryptoAdmissionBudget {
    fn admit(&mut self, peer: PeerId, protocol: &str, now_ms: i64) -> Result<(), WorldError> {
        if protocol != EXPENSIVE_FETCH_BLOB_PROTOCOL && protocol != EXPENSIVE_FETCH_COMMIT_PROTOCOL
        {
            return Ok(());
        }
        if now_ms.saturating_sub(self.window_started_at_ms) >= PRE_CRYPTO_WINDOW_MS {
            self.window_started_at_ms = now_ms;
            self.global = 0;
            self.peers.clear();
        }
        let peer_count = self.peers.get(&peer).copied().unwrap_or(0);
        if self.global >= PRE_CRYPTO_GLOBAL_REQUESTS || peer_count >= PRE_CRYPTO_PER_PEER_REQUESTS {
            return Err(WorldError::NetworkRequestFailed {
                code: oasis7_proto::distributed::DistributedErrorCode::ErrRateLimited,
                message: "pre-crypto request admission exhausted; retry after short window".into(),
                retryable: true,
            });
        }
        if !self.peers.contains_key(&peer) && self.peers.len() >= PRE_CRYPTO_MAX_PEERS {
            return Err(WorldError::NetworkRequestFailed {
                code: oasis7_proto::distributed::DistributedErrorCode::ErrRateLimited,
                message: "pre-crypto peer admission capacity exhausted".into(),
                retryable: true,
            });
        }
        self.global += 1;
        self.peers.insert(peer, peer_count + 1);
        Ok(())
    }
}

/// Run cheap request admission on the swarm loop before a scarce response-worker reservation.
pub(super) fn admit_inbound_handler(
    registration: &HandlerRegistration,
    payload: &[u8],
) -> Result<Handler, WorldError> {
    if let Some(admission) = registration.admission.as_ref() {
        admission(payload)?;
    }
    Ok(std::sync::Arc::clone(&registration.handler))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch_inbound_request(
    request: NetworkRequest,
    channel: request_response::ResponseChannel<NetworkResponse>,
    peer: PeerId,
    handlers: &HashMap<String, HandlerRegistration>,
    response_workers: &ResponseWorkers,
    response_budget: &mut FetchBlobResponseBudget,
    admission_budget: &mut PreCryptoAdmissionBudget,
    traffic_metrics: &SharedLibp2pTrafficMetrics,
    swarm: &mut Swarm<Behaviour>,
    event_errors: &Arc<Mutex<Vec<String>>>,
    max_error_messages: usize,
    peer_record_template: Option<&PeerRecord>,
    keypair: &Keypair,
    listening_addrs: &Arc<Mutex<Vec<Multiaddr>>>,
    reachability: &Arc<Mutex<Libp2pReachabilitySnapshot>>,
    allow_loopback_external_addrs_for_testing: bool,
    discovered_peer_records: &PeerRecordCache,
) {
    let dispatch_now_ms = now_ms();
    record_request_inbound(
        traffic_metrics,
        request.protocol.as_str(),
        request.payload.len(),
    );
    if let Some(registration) = handlers.get(&request.protocol) {
        if let Err(reply) = admission_budget.admit(peer, request.protocol.as_str(), dispatch_now_ms)
        {
            response_workers.send_immediate(
                channel,
                request.protocol.as_str(),
                peer,
                Err(reply),
                response_budget,
                dispatch_now_ms,
                traffic_metrics,
                swarm,
            );
            return;
        }
        let handler = match admit_inbound_handler(registration, request.payload.as_slice()) {
            Ok(handler) => handler,
            Err(reply) => {
                response_workers.send_immediate(
                    channel,
                    request.protocol.as_str(),
                    peer,
                    Err(reply),
                    response_budget,
                    dispatch_now_ms,
                    traffic_metrics,
                    swarm,
                );
                return;
            }
        };
        if let Err(rejected) = response_workers.schedule(
            channel,
            request.protocol,
            peer,
            request.payload,
            dispatch_now_ms,
            handler,
        ) {
            let protocol = rejected.protocol;
            let peer = rejected.peer;
            response_workers.send_immediate(
                rejected.channel,
                protocol.as_str(),
                peer,
                Err(rejected.reply),
                response_budget,
                dispatch_now_ms,
                traffic_metrics,
                swarm,
            );
            push_bounded_clone(
                event_errors,
                format!("libp2p response worker queue full protocol={protocol} peer={peer}"),
                max_error_messages,
                "lock errors",
            );
        }
        return;
    }
    let reply = handle_request_response_request(
        &request,
        handlers,
        peer_record_template,
        keypair,
        listening_addrs,
        reachability,
        allow_loopback_external_addrs_for_testing,
        discovered_peer_records,
    );
    response_workers.send_immediate(
        channel,
        request.protocol.as_str(),
        peer,
        reply,
        response_budget,
        dispatch_now_ms,
        traffic_metrics,
        swarm,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_blob_flood_is_bounded_without_blocking_head_lane() {
        let mut budget = PreCryptoAdmissionBudget::default();
        let attacker = PeerId::random();
        for _ in 0..PRE_CRYPTO_PER_PEER_REQUESTS {
            budget
                .admit(attacker, "/aw/node/replication/fetch-blob/1.0.0", 1_000)
                .unwrap();
        }
        assert!(
            budget
                .admit(attacker, "/aw/node/replication/fetch-blob/1.0.0", 1_000)
                .is_err()
        );
        assert!(
            budget
                .admit(
                    attacker,
                    "/aw/node/replication/fetch-commit/head/1.0.0",
                    1_000
                )
                .is_ok()
        );
        assert!(budget.peers.len() <= PRE_CRYPTO_MAX_PEERS);
    }

    #[test]
    fn rotating_identity_flood_is_globally_bounded_and_recovers() {
        let mut budget = PreCryptoAdmissionBudget::default();
        for _ in 0..PRE_CRYPTO_GLOBAL_REQUESTS {
            budget
                .admit(
                    PeerId::random(),
                    "/aw/node/replication/fetch-blob/1.0.0",
                    1_000,
                )
                .unwrap();
        }
        assert!(
            budget
                .admit(
                    PeerId::random(),
                    "/aw/node/replication/fetch-blob/1.0.0",
                    1_000
                )
                .is_err()
        );
        budget
            .admit(
                PeerId::random(),
                "/aw/node/replication/fetch-blob/1.0.0",
                1_000 + PRE_CRYPTO_WINDOW_MS,
            )
            .unwrap();
    }

    #[test]
    fn signed_fetch_commit_flood_is_bounded_while_head_is_always_admitted() {
        let mut budget = PreCryptoAdmissionBudget::default();
        let attacker = PeerId::random();
        for _ in 0..PRE_CRYPTO_PER_PEER_REQUESTS {
            budget
                .admit(attacker, EXPENSIVE_FETCH_COMMIT_PROTOCOL, 5_000)
                .unwrap();
        }
        assert!(
            budget
                .admit(attacker, EXPENSIVE_FETCH_COMMIT_PROTOCOL, 5_000)
                .is_err()
        );
        for _ in 0..PRE_CRYPTO_GLOBAL_REQUESTS {
            budget
                .admit(
                    PeerId::random(),
                    "/aw/node/replication/fetch-commit/head/1.0.0",
                    5_000,
                )
                .expect("head protocol is exempt from expensive admission budget");
        }
        assert!(budget.peers.len() <= PRE_CRYPTO_MAX_PEERS);
    }
}

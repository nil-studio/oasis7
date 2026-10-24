use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use libp2p::identity::Keypair;
use libp2p::kad::{self, Quorum, RecordKey};
use libp2p::multiaddr::Protocol;
use libp2p::swarm::Swarm;
use libp2p::{Multiaddr, PeerId};

use crate::error::WorldError;
use crate::util::to_canonical_cbor;
use oasis7_proto::distributed::dht_peer_record_key;
use oasis7_proto::distributed_dht::{PeerRecord, PeerRecordPublicationV2, SignedPeerRecord};

use super::durable_peer_record_publisher::LocalPeerRecordPublisher;
use super::kad_queries::PendingDhtQuery;
use super::reachability::{
    Libp2pReachabilitySnapshot, is_loopback_direct_addr, is_public_direct_addr, snapshot_clone,
};
use super::{Behaviour, CommandResponseSender};

pub(super) const MAX_SIGNED_PEER_RECORD_BYTES: usize = 16 * 1024;

#[expect(
    clippy::too_many_arguments,
    reason = "Peer-record publication keeps signing and response state explicit to preserve wire ordering"
)]
pub(super) fn publish_configured_peer_record(
    swarm: &mut Swarm<Behaviour>,
    pending_dht: &mut HashMap<kad::QueryId, PendingDhtQuery>,
    keypair: &Keypair,
    template: &PeerRecord,
    listening_addrs: &Arc<Mutex<Vec<Multiaddr>>>,
    reachability: &Arc<Mutex<Libp2pReachabilitySnapshot>>,
    allow_loopback_external_addrs_for_testing: bool,
    local_publisher: &Arc<Mutex<LocalPeerRecordPublisher>>,
    response: Option<CommandResponseSender<()>>,
) -> Result<SignedPeerRecord, WorldError> {
    let signed = build_configured_peer_record_with_publisher(
        keypair,
        template,
        listening_addrs,
        reachability,
        allow_loopback_external_addrs_for_testing,
        local_publisher,
    )?;
    let key = dht_peer_record_key(
        signed.record.world_id.as_str(),
        signed.record.peer_id.as_str(),
    );
    let payload = to_canonical_cbor(&signed)?;
    let query_id = put_record_query(swarm, key, payload)?;
    pending_dht.insert(query_id, PendingDhtQuery::PutPeerRecord { response });
    Ok(signed)
}

#[cfg(test)]
pub(super) fn build_configured_peer_record(
    keypair: &Keypair,
    template: &PeerRecord,
    listening_addrs: &Arc<Mutex<Vec<Multiaddr>>>,
    reachability: &Arc<Mutex<Libp2pReachabilitySnapshot>>,
    allow_loopback_external_addrs_for_testing: bool,
) -> Result<SignedPeerRecord, WorldError> {
    let materialized = materialize_peer_record(
        template,
        listening_addrs,
        reachability,
        allow_loopback_external_addrs_for_testing,
    );
    sign_peer_record(&materialized, keypair)
}

pub(super) fn build_configured_peer_record_with_publisher(
    keypair: &Keypair,
    template: &PeerRecord,
    listening_addrs: &Arc<Mutex<Vec<Multiaddr>>>,
    reachability: &Arc<Mutex<Libp2pReachabilitySnapshot>>,
    allow_loopback_external_addrs_for_testing: bool,
    local_publisher: &Arc<Mutex<LocalPeerRecordPublisher>>,
) -> Result<SignedPeerRecord, WorldError> {
    let materialized = materialize_peer_record(
        template,
        listening_addrs,
        reachability,
        allow_loopback_external_addrs_for_testing,
    );
    let mut local_publisher =
        local_publisher
            .lock()
            .map_err(|_| WorldError::NetworkProtocolUnavailable {
                protocol: "local peer-record publisher lock is poisoned".into(),
            })?;
    local_publisher.sign(&materialized, keypair)
}

pub(super) fn build_current_peer_record_for_remote_request(
    keypair: &Keypair,
    template: &PeerRecord,
    listening_addrs: &Arc<Mutex<Vec<Multiaddr>>>,
    reachability: &Arc<Mutex<Libp2pReachabilitySnapshot>>,
    allow_loopback_external_addrs_for_testing: bool,
    local_publisher: &Arc<Mutex<LocalPeerRecordPublisher>>,
) -> Result<SignedPeerRecord, WorldError> {
    let current = peer_record_with_current_addresses(
        template,
        listening_addrs,
        reachability,
        allow_loopback_external_addrs_for_testing,
    );
    let local_publisher =
        local_publisher
            .lock()
            .map_err(|_| WorldError::NetworkProtocolUnavailable {
                protocol: "local peer-record publisher lock is poisoned".into(),
            })?;
    local_publisher.signed_record_for_remote_request(&current, keypair)
}

pub(super) fn validate_discovered_peer_record(
    discovered: &SignedPeerRecord,
    local_template: Option<&PeerRecord>,
) -> Result<(), WorldError> {
    verify_signed_peer_record(discovered)?;
    if let Some(local_template) = local_template {
        if discovered.record.world_id != local_template.world_id {
            return Err(WorldError::NetworkProtocolUnavailable {
                protocol: format!(
                    "peer record world mismatch expected={} actual={}",
                    local_template.world_id, discovered.record.world_id
                ),
            });
        }
        if discovered.record.network_id != local_template.network_id {
            return Err(WorldError::NetworkProtocolUnavailable {
                protocol: format!(
                    "peer record network mismatch expected={} actual={}",
                    local_template.network_id, discovered.record.network_id
                ),
            });
        }
    }
    Ok(())
}

pub(super) fn decode_peer_record(bytes: &[u8]) -> Result<SignedPeerRecord, WorldError> {
    if bytes.len() > MAX_SIGNED_PEER_RECORD_BYTES {
        return Err(WorldError::NetworkProtocolUnavailable {
            protocol: "peer record payload capacity exceeded".into(),
        });
    }
    let record: SignedPeerRecord = serde_cbor::from_slice(bytes)?;
    verify_signed_peer_record(&record)?;
    Ok(record)
}

pub(super) fn sign_peer_record(
    record: &PeerRecord,
    keypair: &Keypair,
) -> Result<SignedPeerRecord, WorldError> {
    let record = normalized_record_for_signing(record, keypair)?;
    let payload = encode_peer_record_signing_payload(&record)?;
    let signature =
        keypair
            .sign(payload.as_slice())
            .map_err(|err| WorldError::NetworkProtocolUnavailable {
                protocol: format!("peer record sign failed: {err}"),
            })?;
    Ok(SignedPeerRecord {
        record,
        identity_public_key_protobuf_hex: hex::encode(keypair.public().encode_protobuf()),
        signature_hex: hex::encode(signature),
        publication: None,
    })
}

pub(super) fn sign_peer_record_v2(
    record: &PeerRecord,
    publication: PeerRecordPublicationV2,
    keypair: &Keypair,
) -> Result<SignedPeerRecord, WorldError> {
    publication.validate_for_record(&record).map_err(|reason| {
        WorldError::NetworkProtocolUnavailable {
            protocol: reason.to_string(),
        }
    })?;
    let record = normalized_record_for_signing(record, keypair)?;
    let payload = encode_peer_record_signing_payload_v2(&publication, &record)?;
    let signature =
        keypair
            .sign(payload.as_slice())
            .map_err(|err| WorldError::NetworkProtocolUnavailable {
                protocol: format!("peer record sign failed: {err}"),
            })?;
    Ok(SignedPeerRecord {
        record,
        identity_public_key_protobuf_hex: hex::encode(keypair.public().encode_protobuf()),
        signature_hex: hex::encode(signature),
        publication: Some(publication),
    })
}

fn normalized_record_for_signing(
    record: &PeerRecord,
    keypair: &Keypair,
) -> Result<PeerRecord, WorldError> {
    let mut record = record.clone();
    record
        .validate_policy()
        .map_err(|reason| WorldError::NetworkProtocolUnavailable { protocol: reason })?;
    if record.peer_id.trim().is_empty() {
        record.peer_id = PeerId::from(keypair.public()).to_string();
    }
    Ok(record)
}

pub(super) fn verify_signed_peer_record(record: &SignedPeerRecord) -> Result<(), WorldError> {
    record
        .record
        .validate_policy()
        .map_err(|reason| WorldError::NetworkProtocolUnavailable { protocol: reason })?;
    let public_key_bytes =
        hex::decode(record.identity_public_key_protobuf_hex.as_str()).map_err(|_| {
            WorldError::NetworkProtocolUnavailable {
                protocol: "peer record public key must be valid hex".to_string(),
            }
        })?;
    let public_key = libp2p::identity::PublicKey::try_decode_protobuf(public_key_bytes.as_slice())
        .map_err(|err| WorldError::NetworkProtocolUnavailable {
            protocol: format!("peer record public key decode failed: {err}"),
        })?;
    if public_key.to_peer_id().to_string() != record.record.peer_id {
        return Err(WorldError::NetworkProtocolUnavailable {
            protocol: "peer record peer_id does not match identity public key".to_string(),
        });
    }
    let signature = hex::decode(record.signature_hex.as_str()).map_err(|_| {
        WorldError::NetworkProtocolUnavailable {
            protocol: "peer record signature must be valid hex".to_string(),
        }
    })?;
    let payload = match &record.publication {
        None => encode_peer_record_signing_payload(&record.record)?,
        Some(publication) => {
            publication
                .validate_for_record(&record.record)
                .map_err(|reason| WorldError::NetworkProtocolUnavailable {
                    protocol: reason.to_string(),
                })?;
            encode_peer_record_signing_payload_v2(publication, &record.record)?
        }
    };
    if !public_key.verify(payload.as_slice(), signature.as_slice()) {
        return Err(WorldError::NetworkProtocolUnavailable {
            protocol: "peer record signature verification failed".to_string(),
        });
    }
    Ok(())
}

fn materialize_peer_record(
    template: &PeerRecord,
    listening_addrs: &Arc<Mutex<Vec<Multiaddr>>>,
    reachability: &Arc<Mutex<Libp2pReachabilitySnapshot>>,
    allow_loopback_external_addrs_for_testing: bool,
) -> PeerRecord {
    let mut record = peer_record_with_current_addresses(
        template,
        listening_addrs,
        reachability,
        allow_loopback_external_addrs_for_testing,
    );
    record.published_at_ms = super::now_ms();
    record
}

fn peer_record_with_current_addresses(
    template: &PeerRecord,
    listening_addrs: &Arc<Mutex<Vec<Multiaddr>>>,
    reachability: &Arc<Mutex<Libp2pReachabilitySnapshot>>,
    allow_loopback_external_addrs_for_testing: bool,
) -> PeerRecord {
    let mut record = template.clone();
    let listening_addrs = listening_addrs.lock().expect("lock listening addrs");
    if record.direct_addrs.is_empty() && peer_record_allows_direct_addrs(&record) {
        let reachability = snapshot_clone(reachability);
        record.direct_addrs = materialize_direct_addrs(
            listening_addrs.as_slice(),
            &reachability,
            allow_loopback_external_addrs_for_testing,
        );
    }
    if record.relay_addrs.is_empty() {
        record.relay_addrs = listening_addrs
            .iter()
            .filter(|addr| is_relayed_addr(addr))
            .map(ToString::to_string)
            .collect();
    }
    record.published_at_ms = 0;
    record
}

fn materialize_direct_addrs(
    listening_addrs: &[Multiaddr],
    reachability: &Libp2pReachabilitySnapshot,
    allow_loopback_external_addrs_for_testing: bool,
) -> Vec<String> {
    let mut direct_addrs = if !reachability.confirmed_external_direct_addrs.is_empty() {
        reachability.confirmed_external_direct_addrs.clone()
    } else {
        listening_addrs
            .iter()
            .filter(|addr| {
                is_public_direct_addr(addr)
                    || (allow_loopback_external_addrs_for_testing && is_loopback_direct_addr(addr))
            })
            .map(ToString::to_string)
            .collect()
    };
    direct_addrs.sort();
    direct_addrs.dedup();
    direct_addrs
}

fn peer_record_allows_direct_addrs(record: &PeerRecord) -> bool {
    let Ok(node_role) = record.parsed_node_role() else {
        return false;
    };
    !matches!(
        record.deployment_mode,
        oasis7_proto::distributed_dht::PeerDeploymentMode::Private
            | oasis7_proto::distributed_dht::PeerDeploymentMode::RelayOnly
            | oasis7_proto::distributed_dht::PeerDeploymentMode::ValidatorHidden
    ) && !matches!(
        node_role,
        oasis7_proto::distributed_dht::PeerNodeRole::ValidatorCore
    )
}

fn is_relayed_addr(addr: &Multiaddr) -> bool {
    addr.iter()
        .any(|protocol| matches!(protocol, Protocol::P2pCircuit))
}

pub(super) fn put_record_query(
    swarm: &mut Swarm<Behaviour>,
    key: String,
    payload: Vec<u8>,
) -> Result<kad::QueryId, WorldError> {
    let dht_key = RecordKey::new(&key);
    let mut record = kad::Record {
        key: dht_key,
        value: payload,
        publisher: None,
        expires: None,
    };
    if super::peer_record_store::PeerRecordStore::peer_record_key(&record.key) {
        let signed = decode_peer_record(&record.value)?;
        let expected_key = dht_peer_record_key(&signed.record.world_id, &signed.record.peer_id);
        if record.key != RecordKey::new(&expected_key) {
            return Err(WorldError::NetworkProtocolUnavailable {
                protocol: "peer record DHT key mismatch".into(),
            });
        }
        let lease = swarm
            .behaviour_mut()
            .kademlia
            .store_mut()
            .clock()
            .check(&signed.record)?;
        record.expires = Some(lease.deadline());
    }
    swarm
        .behaviour_mut()
        .kademlia
        .put_record(record, Quorum::One)
        .map_err(|err| WorldError::NetworkProtocolUnavailable {
            protocol: format!("kad put_record failed: {err}"),
        })
}

fn encode_peer_record_signing_payload(record: &PeerRecord) -> Result<Vec<u8>, WorldError> {
    let mut payload = b"oasis7-peer-record-v1|".to_vec();
    payload.extend_from_slice(&to_canonical_cbor(record)?);
    Ok(payload)
}

pub(super) fn encode_peer_record_signing_payload_v2(
    publication: &PeerRecordPublicationV2,
    record: &PeerRecord,
) -> Result<Vec<u8>, WorldError> {
    publication.validate_for_record(record).map_err(|reason| {
        WorldError::NetworkProtocolUnavailable {
            protocol: reason.to_string(),
        }
    })?;
    let mut payload = b"oasis7-peer-record-v2|".to_vec();
    payload.extend_from_slice(&to_canonical_cbor(&(
        publication.schema_version,
        publication.sequence,
        publication.state,
        record,
    ))?);
    Ok(payload)
}

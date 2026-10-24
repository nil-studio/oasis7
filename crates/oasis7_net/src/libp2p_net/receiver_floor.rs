use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use libp2p::{Multiaddr, multiaddr::Protocol};
use libp2p::{PeerId, identity::Keypair};
use oasis7_proto::distributed_dht::{PeerRecord, PeerRecordPublicationV2, SignedPeerRecord};
use serde::{Deserialize, Serialize};

use crate::error::WorldError;

use super::config::{PeerRecordReceiverConfig, PeerRecordReceiverMode};
use super::peer_record::{encode_peer_record_signing_payload_v2, verify_signed_peer_record};

mod persistence;

pub(super) const MAX_RECEIVER_FLOORS: usize = 1024;
pub(super) const MAX_RECEIVER_RECORD_BYTES: usize =
    super::peer_record::MAX_SIGNED_PEER_RECORD_BYTES;
const RECEIVER_SCHEMA_VERSION: u16 = 2;
const RECEIVER_STATE_MAX_BYTES: usize = 64 * 1024 * 1024;

pub(super) type SharedPeerRecordReceiver = Arc<PeerRecordReceiver>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiverBindingV2 {
    schema_version: u16,
    local_peer_id: String,
    world_id: String,
    network_id: String,
    identity_public_key_protobuf_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiverLedgerV2 {
    schema_version: u16,
    binding: ReceiverBindingV2,
    mode: PeerRecordReceiverMode,
    floors: BTreeMap<String, PeerFloorV2>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PeerFloorV2 {
    sequence: u64,
    signing_payload_digest_hex: String,
    primary_signed_record_cbor: Vec<u8>,
    conflict_signed_record_cbor: Option<Vec<u8>>,
    isolated: bool,
}

#[derive(Debug)]
struct DurableReceiver {
    directory: std::path::PathBuf,
    _lock: File,
    ledger: ReceiverLedgerV2,
    poisoned: bool,
}

#[derive(Debug)]
struct ReceiverState {
    mode: PeerRecordReceiverMode,
    durable: Option<Mutex<DurableReceiver>>,
}

/// Shared by DHT storage, public results, and peer-exchange candidates.
/// `ingress_serial` spans the durable floor update and in-memory publication.
#[derive(Debug)]
pub(super) struct PeerRecordReceiver {
    ingress_serial: Mutex<()>,
    state: ReceiverState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReceiverAdmission {
    LegacyV1,
    Accepted,
    Duplicate,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RecordIdentity {
    V1,
    V2 {
        sequence: u64,
        signing_payload_digest: [u8; 32],
    },
}

#[derive(Clone)]
pub(super) struct ReceiverLeaseGuard {
    receiver: SharedPeerRecordReceiver,
    peer_id: String,
    identity: RecordIdentity,
}

impl std::fmt::Debug for ReceiverLeaseGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReceiverLeaseGuard")
            .field("peer_id", &self.peer_id)
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

impl PartialEq for ReceiverLeaseGuard {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.receiver, &other.receiver)
            && self.peer_id == other.peer_id
            && self.identity == other.identity
    }
}

impl Eq for ReceiverLeaseGuard {}

impl PeerRecordReceiver {
    pub(super) fn legacy() -> SharedPeerRecordReceiver {
        Arc::new(Self {
            ingress_serial: Mutex::new(()),
            state: ReceiverState {
                mode: PeerRecordReceiverMode::Legacy,
                durable: None,
            },
        })
    }

    pub(super) fn initialize(
        config: &PeerRecordReceiverConfig,
        keypair: &Keypair,
        template: &PeerRecord,
    ) -> Result<(), WorldError> {
        let mode = require_durable_mode(config.mode)?;
        let directory = receiver_directory(config)?;
        let binding = receiver_binding(template, keypair)?;
        persistence::initialize(directory, binding, mode)
    }

    pub(super) fn open(
        config: &PeerRecordReceiverConfig,
        keypair: &Keypair,
        template: Option<&PeerRecord>,
    ) -> Result<SharedPeerRecordReceiver, WorldError> {
        match config.mode {
            PeerRecordReceiverMode::Legacy => {
                if let Some(directory) = config.directory.as_deref()
                    && persistence::receiver_path_exists(directory)?
                {
                    return Err(protocol_error(
                        "receiver state remains on disk; refusing legacy fallback".into(),
                    ));
                }
                Ok(Self::legacy())
            }
            mode => {
                let template = template.ok_or_else(|| {
                    protocol_error(
                        "durable peer-record receiver requires a configured template".into(),
                    )
                })?;
                let binding = receiver_binding(template, keypair)?;
                let directory = receiver_directory(config)?;
                let durable = persistence::open(directory, binding, mode)?;
                Ok(Arc::new(Self {
                    ingress_serial: Mutex::new(()),
                    state: ReceiverState {
                        mode,
                        durable: Some(Mutex::new(durable)),
                    },
                }))
            }
        }
    }

    pub(super) fn begin_ingress(&self) -> Result<MutexGuard<'_, ()>, WorldError> {
        self.ingress_serial
            .lock()
            .map_err(|_| protocol_error("peer-record receiver ingress lock is poisoned".into()))
    }

    pub(super) fn admit_serialized(
        &self,
        record: &SignedPeerRecord,
    ) -> Result<ReceiverAdmission, WorldError> {
        verify_signed_peer_record(record)?;
        self.preflight_record(record)?;
        let Some(publication) = record.publication.as_ref() else {
            return self.admit_v1(record);
        };
        publication
            .validate()
            .map_err(|reason| protocol_error(reason.into()))?;
        if self.state.mode == PeerRecordReceiverMode::Legacy {
            return Ok(ReceiverAdmission::Accepted);
        }
        self.admit_v2(record, publication)
    }

    pub(super) fn preflight_record(&self, record: &SignedPeerRecord) -> Result<(), WorldError> {
        verify_signed_peer_record(record)?;
        if self.state.mode == PeerRecordReceiverMode::Legacy {
            return Ok(());
        }
        let durable = self.durable_lock()?;
        let durable = durable
            .as_ref()
            .ok_or_else(|| protocol_error("durable receiver state is unavailable".into()))?;
        ensure_durable_usable(durable)?;
        ensure_record_matches_binding(record, &durable.ledger.binding)?;
        match (&self.state.mode, &record.publication) {
            (PeerRecordReceiverMode::V2Only, None) => Err(protocol_error(
                "V2-only receiver rejects legacy peer records".into(),
            )),
            (PeerRecordReceiverMode::DurableCompatible, None)
                if durable.ledger.floors.contains_key(&record.record.peer_id) =>
            {
                Err(protocol_error(
                    "legacy peer record rejected after V2 floor migration".into(),
                ))
            }
            (_, Some(publication)) => {
                validate_record_paths(record)?;
                publication
                    .validate()
                    .map_err(|reason| protocol_error(reason.into()))?;
                if let Some(current) = durable.ledger.floors.get(&record.record.peer_id) {
                    if current.isolated {
                        return Err(protocol_error(
                            "peer is isolated after a V2 sequence conflict".into(),
                        ));
                    }
                    if publication.sequence < current.sequence {
                        return Err(protocol_error("peer-record V2 sequence regressed".into()));
                    }
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub(super) fn would_conflict(&self, record: &SignedPeerRecord) -> Result<bool, WorldError> {
        let Some(publication) = record.publication.as_ref() else {
            return Ok(false);
        };
        if self.state.mode == PeerRecordReceiverMode::Legacy {
            return Ok(false);
        }
        let durable = self.durable_lock()?;
        let durable = durable
            .as_ref()
            .ok_or_else(|| protocol_error("durable receiver state is unavailable".into()))?;
        ensure_durable_usable(durable)?;
        let Some(current) = durable.ledger.floors.get(&record.record.peer_id) else {
            return Ok(false);
        };
        if current.isolated || publication.sequence != current.sequence {
            return Ok(false);
        }
        let identity = record_identity(record, publication)?;
        Ok(current.signing_payload_digest_hex != encode_digest(&identity))
    }

    fn admit_v1(&self, record: &SignedPeerRecord) -> Result<ReceiverAdmission, WorldError> {
        match self.state.mode {
            PeerRecordReceiverMode::Legacy => Ok(ReceiverAdmission::LegacyV1),
            PeerRecordReceiverMode::V2Only => Err(protocol_error(
                "V2-only receiver rejects legacy peer records".into(),
            )),
            PeerRecordReceiverMode::DurableCompatible => {
                let durable = self.durable_lock_required()?;
                ensure_durable_usable(&durable)?;
                if durable.ledger.floors.contains_key(&record.record.peer_id) {
                    return Err(protocol_error(
                        "legacy peer record rejected after V2 floor migration".into(),
                    ));
                }
                Ok(ReceiverAdmission::LegacyV1)
            }
        }
    }

    fn admit_v2(
        &self,
        record: &SignedPeerRecord,
        publication: &PeerRecordPublicationV2,
    ) -> Result<ReceiverAdmission, WorldError> {
        let mut durable = self.durable_lock_required()?;
        ensure_durable_usable(&durable)?;
        ensure_record_matches_binding(record, &durable.ledger.binding)?;
        let identity = record_identity(record, publication)?;
        let signing_payload_digest_hex = encode_digest(&identity);
        let signed_bytes = canonical_record_bytes(record)?;
        if signed_bytes.len() > MAX_RECEIVER_RECORD_BYTES {
            return Err(protocol_error(
                "peer record payload capacity exceeded".into(),
            ));
        }
        let peer_id = record.record.peer_id.clone();
        let current = durable.ledger.floors.get(&peer_id);
        let outcome = match current {
            None => {
                if durable.ledger.floors.len() >= MAX_RECEIVER_FLOORS {
                    return Err(protocol_error(
                        "peer-record receiver floor capacity exhausted".into(),
                    ));
                }
                ReceiverAdmission::Accepted
            }
            Some(current) if current.isolated => {
                if current.sequence == publication.sequence
                    && current.signing_payload_digest_hex == signing_payload_digest_hex
                {
                    return Ok(ReceiverAdmission::Conflict);
                }
                return Err(protocol_error(
                    "peer is isolated after a V2 sequence conflict".into(),
                ));
            }
            Some(current) if publication.sequence < current.sequence => {
                return Err(protocol_error("peer-record V2 sequence regressed".into()));
            }
            Some(current) if publication.sequence == current.sequence => {
                if current.signing_payload_digest_hex == signing_payload_digest_hex {
                    return Ok(ReceiverAdmission::Duplicate);
                }
                ReceiverAdmission::Conflict
            }
            Some(_) => ReceiverAdmission::Accepted,
        };

        let mut next = durable.ledger.clone();
        match outcome {
            ReceiverAdmission::Conflict => {
                let current = next
                    .floors
                    .get_mut(&peer_id)
                    .expect("same-sequence conflict requires an existing floor");
                current.conflict_signed_record_cbor = Some(signed_bytes);
                current.isolated = true;
            }
            ReceiverAdmission::Accepted => {
                next.floors.insert(
                    peer_id,
                    PeerFloorV2 {
                        sequence: publication.sequence,
                        signing_payload_digest_hex,
                        primary_signed_record_cbor: signed_bytes,
                        conflict_signed_record_cbor: None,
                        isolated: false,
                    },
                );
            }
            ReceiverAdmission::Duplicate | ReceiverAdmission::LegacyV1 => {
                unreachable!("duplicate V2 records returned before persistence")
            }
        }
        persist_next(&mut durable, next)?;
        Ok(outcome)
    }

    pub(super) fn lease_guard(
        self: &Arc<Self>,
        record: &SignedPeerRecord,
    ) -> Result<ReceiverLeaseGuard, WorldError> {
        let peer_id = record.record.peer_id.clone();
        match &record.publication {
            None => {
                if self.state.mode == PeerRecordReceiverMode::V2Only {
                    return Err(protocol_error(
                        "V2-only receiver rejects legacy peer records".into(),
                    ));
                }
                let durable = self.durable_lock()?;
                if let Some(durable) = durable.as_ref() {
                    ensure_durable_usable(durable)?;
                    if durable.ledger.floors.contains_key(&peer_id) {
                        return Err(protocol_error(
                            "legacy peer record rejected after V2 floor migration".into(),
                        ));
                    }
                }
                Ok(ReceiverLeaseGuard {
                    receiver: Arc::clone(self),
                    peer_id,
                    identity: RecordIdentity::V1,
                })
            }
            Some(publication) => {
                let identity = record_identity(record, publication)?;
                let durable = self.durable_lock()?;
                if let Some(durable) = durable.as_ref() {
                    ensure_durable_usable(durable)?;
                    ensure_record_matches_binding(record, &durable.ledger.binding)?;
                    let floor = durable.ledger.floors.get(&peer_id).ok_or_else(|| {
                        protocol_error("V2 record has no committed receiver floor".into())
                    })?;
                    if floor.isolated
                        || floor.sequence != publication.sequence
                        || floor.signing_payload_digest_hex != encode_digest(&identity)
                    {
                        return Err(protocol_error(
                            "peer record does not match the current receiver floor".into(),
                        ));
                    }
                }
                Ok(ReceiverLeaseGuard {
                    receiver: Arc::clone(self),
                    peer_id,
                    identity,
                })
            }
        }
    }

    fn durable_lock(&self) -> Result<Option<MutexGuard<'_, DurableReceiver>>, WorldError> {
        let Some(durable) = self.state.durable.as_ref() else {
            return Ok(None);
        };
        durable
            .lock()
            .map(Some)
            .map_err(|_| protocol_error("peer-record receiver state lock is poisoned".into()))
    }

    fn durable_lock_required(&self) -> Result<MutexGuard<'_, DurableReceiver>, WorldError> {
        self.durable_lock()?
            .ok_or_else(|| protocol_error("durable receiver state is unavailable".into()))
    }
}

/// Reject malformed or identity-mismatched paths before a durable floor update.
/// The legacy contract continues to retain its existing path filtering behavior.
fn validate_record_paths(record: &SignedPeerRecord) -> Result<(), WorldError> {
    let peer = record.record.peer_id.parse::<PeerId>().map_err(|_| {
        protocol_error("peer record peer_id is invalid for receiver admission".into())
    })?;
    for raw in &record.record.direct_addrs {
        validate_record_path(raw, peer, false)?;
    }
    for raw in &record.record.hole_punch_addrs {
        validate_record_path(raw, peer, false)?;
    }
    for raw in &record.record.relay_addrs {
        validate_record_path(raw, peer, true)?;
    }
    Ok(())
}

fn validate_record_path(raw: &str, peer: PeerId, allow_relay_hops: bool) -> Result<(), WorldError> {
    let address = raw.parse::<Multiaddr>().map_err(|error| {
        protocol_error(format!(
            "peer record contains an invalid multiaddr: {error}"
        ))
    })?;
    if address.is_empty() {
        return Err(protocol_error(
            "peer record contains an empty multiaddr".into(),
        ));
    }
    let protocols: Vec<_> = address.iter().collect();
    for (index, protocol) in protocols.iter().enumerate() {
        if matches!(protocol, Protocol::P2pCircuit) {
            let follows_relay_peer =
                index > 0 && matches!(protocols.get(index - 1), Some(Protocol::P2p(_)));
            if !allow_relay_hops || !follows_relay_peer {
                return Err(protocol_error(
                    "peer record relay circuit is not bound to a relay hop".into(),
                ));
            }
        }
        if let Protocol::P2p(address_peer) = protocol {
            let terminal = index + 1 == protocols.len();
            if terminal {
                if *address_peer != peer {
                    return Err(protocol_error(
                        "peer record multiaddr has a mismatched or nonterminal peer id".into(),
                    ));
                }
            } else if !(allow_relay_hops
                && matches!(protocols.get(index + 1), Some(Protocol::P2pCircuit)))
            {
                return Err(protocol_error(
                    "peer record multiaddr has a mismatched or nonterminal peer id".into(),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod path_validation_tests {
    use super::*;

    #[test]
    fn only_relay_addrs_allow_immediate_relay_hops_and_owner_destination() {
        let owner = PeerId::random();
        let relay = PeerId::random();
        let wrong_owner = PeerId::random();
        let relay_prefix = format!("/dns4/relay.example/tcp/443/p2p/{relay}");

        assert!(
            validate_record_path(
                &format!("{relay_prefix}/p2p-circuit/p2p/{owner}"),
                owner,
                true,
            )
            .is_ok()
        );
        assert!(validate_record_path(&format!("{relay_prefix}/p2p-circuit"), owner, true).is_ok());
        assert!(
            validate_record_path(
                &format!("{relay_prefix}/p2p-circuit/p2p/{wrong_owner}"),
                owner,
                true,
            )
            .is_err()
        );
        assert!(validate_record_path(&format!("{relay_prefix}/udp/443"), owner, true,).is_err());
        assert!(
            validate_record_path(
                &format!("/ip4/192.0.2.1/tcp/443/p2p/{wrong_owner}"),
                owner,
                false,
            )
            .is_err()
        );
        assert!(
            validate_record_path(
                &format!("{relay_prefix}/p2p-circuit/p2p/{owner}"),
                owner,
                false,
            )
            .is_err()
        );
    }
}

impl ReceiverLeaseGuard {
    pub(super) fn validate(&self) -> Result<(), WorldError> {
        let receiver = &self.receiver;
        match &self.identity {
            RecordIdentity::V1 => {
                if receiver.state.mode == PeerRecordReceiverMode::V2Only {
                    return Err(protocol_error(
                        "V2-only receiver rejects legacy lease".into(),
                    ));
                }
                let durable = receiver.durable_lock()?;
                if let Some(durable) = durable.as_ref() {
                    ensure_durable_usable(durable)?;
                    if durable.ledger.floors.contains_key(&self.peer_id) {
                        return Err(protocol_error("legacy lease revoked by V2 floor".into()));
                    }
                }
            }
            RecordIdentity::V2 {
                sequence,
                signing_payload_digest,
            } => {
                let durable = receiver.durable_lock()?;
                if let Some(durable) = durable.as_ref() {
                    ensure_durable_usable(durable)?;
                    let floor = durable
                        .ledger
                        .floors
                        .get(&self.peer_id)
                        .ok_or_else(|| protocol_error("receiver floor is missing".into()))?;
                    if floor.isolated
                        || floor.sequence != *sequence
                        || floor.signing_payload_digest_hex
                            != encode_digest(&RecordIdentity::V2 {
                                sequence: *sequence,
                                signing_payload_digest: *signing_payload_digest,
                            })
                    {
                        return Err(protocol_error("peer lease is stale or isolated".into()));
                    }
                }
            }
        }
        Ok(())
    }
}

fn record_identity(
    record: &SignedPeerRecord,
    publication: &PeerRecordPublicationV2,
) -> Result<RecordIdentity, WorldError> {
    let payload = encode_peer_record_signing_payload_v2(publication, &record.record)?;
    Ok(RecordIdentity::V2 {
        sequence: publication.sequence,
        signing_payload_digest: *blake3::hash(&payload).as_bytes(),
    })
}

pub(super) fn same_publication_identity(left: &SignedPeerRecord, right: &SignedPeerRecord) -> bool {
    if left.record.peer_id != right.record.peer_id {
        return false;
    }
    match (&left.publication, &right.publication) {
        (Some(left_publication), Some(right_publication)) => {
            matches!(
                (
                    record_identity(left, left_publication),
                    record_identity(right, right_publication),
                ),
                (Ok(left_identity), Ok(right_identity)) if left_identity == right_identity
            )
        }
        (None, None) => left == right,
        _ => false,
    }
}

fn encode_digest(identity: &RecordIdentity) -> String {
    match identity {
        RecordIdentity::V1 => String::new(),
        RecordIdentity::V2 {
            signing_payload_digest,
            ..
        } => hex::encode(signing_payload_digest),
    }
}

fn canonical_record_bytes(record: &SignedPeerRecord) -> Result<Vec<u8>, WorldError> {
    super::durable_peer_record_publisher::canonical_cbor(record)
}

fn ensure_record_matches_binding(
    record: &SignedPeerRecord,
    binding: &ReceiverBindingV2,
) -> Result<(), WorldError> {
    if record.record.world_id != binding.world_id || record.record.network_id != binding.network_id
    {
        return Err(protocol_error(
            "peer record world or network does not match receiver binding".into(),
        ));
    }
    Ok(())
}

fn validate_loaded_ledger(ledger: &ReceiverLedgerV2) -> Result<(), WorldError> {
    if ledger.floors.len() > MAX_RECEIVER_FLOORS {
        return Err(protocol_error("receiver floor capacity exceeded".into()));
    }
    for (peer_id, floor) in &ledger.floors {
        let primary = decode_stored_record(&floor.primary_signed_record_cbor)?;
        validate_record_paths(&primary)?;
        if primary.record.peer_id != *peer_id {
            return Err(protocol_error("receiver primary peer key mismatch".into()));
        }
        ensure_record_matches_binding(&primary, &ledger.binding)?;
        let publication = primary.publication.as_ref().ok_or_else(|| {
            protocol_error("receiver floor primary is not a V2 signed record".into())
        })?;
        let primary_identity = record_identity(&primary, publication)?;
        if publication.sequence != floor.sequence
            || encode_digest(&primary_identity) != floor.signing_payload_digest_hex
        {
            return Err(protocol_error(
                "receiver floor primary sequence or digest mismatch".into(),
            ));
        }
        match (&floor.conflict_signed_record_cbor, floor.isolated) {
            (None, false) => {}
            (Some(bytes), true) => {
                let conflict = decode_stored_record(bytes)?;
                validate_record_paths(&conflict)?;
                if conflict.record.peer_id != *peer_id {
                    return Err(protocol_error("receiver conflict peer key mismatch".into()));
                }
                ensure_record_matches_binding(&conflict, &ledger.binding)?;
                let publication = conflict.publication.as_ref().ok_or_else(|| {
                    protocol_error("receiver conflict is not a V2 signed record".into())
                })?;
                let conflict_identity = record_identity(&conflict, publication)?;
                if publication.sequence != floor.sequence
                    || encode_digest(&conflict_identity) == floor.signing_payload_digest_hex
                {
                    return Err(protocol_error(
                        "receiver conflict does not prove a same-sequence fork".into(),
                    ));
                }
            }
            _ => {
                return Err(protocol_error(
                    "receiver isolation marker and conflict evidence disagree".into(),
                ));
            }
        }
    }
    Ok(())
}

fn decode_stored_record(bytes: &[u8]) -> Result<SignedPeerRecord, WorldError> {
    if bytes.len() > MAX_RECEIVER_RECORD_BYTES {
        return Err(protocol_error("stored peer record exceeds capacity".into()));
    }
    let record: SignedPeerRecord = serde_cbor::from_slice(bytes)
        .map_err(|error| protocol_error(format!("stored peer record decode failed: {error}")))?;
    if canonical_record_bytes(&record)? != bytes {
        return Err(protocol_error(
            "stored peer record is not canonical CBOR".into(),
        ));
    }
    verify_signed_peer_record(&record)?;
    Ok(record)
}

fn receiver_binding(
    template: &PeerRecord,
    keypair: &Keypair,
) -> Result<ReceiverBindingV2, WorldError> {
    template
        .validate_policy()
        .map_err(|reason| protocol_error(format!("receiver template is invalid: {reason}")))?;
    let local_peer_id = PeerId::from(keypair.public());
    if !template.peer_id.trim().is_empty()
        && template.peer_id.parse::<PeerId>().ok() != Some(local_peer_id)
    {
        return Err(protocol_error(
            "receiver template peer identity does not match configured key".into(),
        ));
    }
    Ok(ReceiverBindingV2 {
        schema_version: RECEIVER_SCHEMA_VERSION,
        local_peer_id: local_peer_id.to_string(),
        world_id: template.world_id.clone(),
        network_id: template.network_id.clone(),
        identity_public_key_protobuf_hex: hex::encode(keypair.public().encode_protobuf()),
    })
}

fn require_durable_mode(
    mode: PeerRecordReceiverMode,
) -> Result<PeerRecordReceiverMode, WorldError> {
    if mode == PeerRecordReceiverMode::Legacy {
        return Err(protocol_error(
            "legacy receiver mode has no durable state to initialize".into(),
        ));
    }
    Ok(mode)
}

fn receiver_directory(config: &PeerRecordReceiverConfig) -> Result<&Path, WorldError> {
    config
        .directory
        .as_deref()
        .ok_or_else(|| protocol_error("durable receiver requires a configured directory".into()))
}

fn ensure_durable_usable(durable: &DurableReceiver) -> Result<(), WorldError> {
    if durable.poisoned {
        return Err(protocol_error(
            "peer-record receiver is fail-closed after a persistence error".into(),
        ));
    }
    Ok(())
}

fn persist_next(durable: &mut DurableReceiver, next: ReceiverLedgerV2) -> Result<(), WorldError> {
    let bytes = super::durable_peer_record_publisher::canonical_cbor(&next)?;
    if bytes.len() > RECEIVER_STATE_MAX_BYTES {
        return Err(protocol_error(
            "peer-record receiver ledger is oversized".into(),
        ));
    }
    if let Err(error) = persistence::persist(&durable.directory, &bytes) {
        durable.poisoned = true;
        return Err(error);
    }
    durable.ledger = next;
    Ok(())
}

fn protocol_error(message: String) -> WorldError {
    WorldError::NetworkProtocolUnavailable { protocol: message }
}

#[cfg(all(test, unix))]
mod tests;

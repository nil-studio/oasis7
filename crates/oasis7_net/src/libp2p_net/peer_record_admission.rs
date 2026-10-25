use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use libp2p::PeerId;
use oasis7_proto::distributed::{DistributedErrorCode, ErrorResponse};
use oasis7_proto::distributed_dht::SignedPeerRecord;

use crate::error::WorldError;

const WINDOW: Duration = Duration::from_secs(60);
const GLOBAL_LIMIT: usize = 512;
const SOURCE_LIMIT: u16 = 256;
const TRACKED_SOURCE_LIMIT: usize = 512;
pub(super) const MAX_ADMISSION_PAYLOAD_BYTES: usize = 16 * 1024;

pub(super) type SharedPeerRecordAdmissionBudget = Arc<Mutex<PeerRecordAdmissionBudget>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PeerRecordAdmissionWorkUnit {
    StorePut,
    KadGetPeerRecordResult,
    KadDiscoveryPeerRecordResult,
    SpecialRequestResponseRequest,
    PeerRecordResponse,
    DiscoveryListResponse,
}

#[derive(Default)]
pub(super) struct PeerRecordAdmissionBudget {
    admitted: VecDeque<AdmissionEvent>,
    by_source: HashMap<PeerId, u16>,
}

#[derive(Clone, Copy)]
struct AdmissionEvent {
    admitted_at: Instant,
    source: Option<PeerId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AdmissionDenial {
    Global,
    Source,
    TrackingCapacity,
}

#[derive(Debug)]
pub(super) struct PeerRecordAdmissionPermit {
    source: Option<PeerId>,
    payload_digest: [u8; 32],
    work_unit: PeerRecordAdmissionWorkUnit,
    admitted_at: Instant,
    max_payload_bytes: usize,
}

/// A signed record decoded under a one-use permit. The original bytes remain
/// attached until candidate installation consumes the permit.
#[derive(Debug)]
pub(super) struct AdmittedPeerRecord {
    record: SignedPeerRecord,
    raw_payload: Vec<u8>,
    permit: PeerRecordAdmissionPermit,
}

impl PeerRecordAdmissionBudget {
    pub(super) fn shared() -> SharedPeerRecordAdmissionBudget {
        Arc::new(Mutex::new(Self::default()))
    }

    fn admit_at(
        &mut self,
        source: Option<PeerId>,
        admitted_at: Instant,
    ) -> Result<(), AdmissionDenial> {
        self.evict_expired(admitted_at);

        if source.is_some_and(|peer| {
            self.by_source.get(&peer).copied().unwrap_or_default() >= SOURCE_LIMIT
        }) {
            return Err(AdmissionDenial::Source);
        }
        if self.admitted.len() >= GLOBAL_LIMIT {
            return Err(AdmissionDenial::Global);
        }
        if source.is_some_and(|peer| {
            !self.by_source.contains_key(&peer) && self.by_source.len() >= TRACKED_SOURCE_LIMIT
        }) {
            return Err(AdmissionDenial::TrackingCapacity);
        }

        self.admitted.push_back(AdmissionEvent {
            admitted_at,
            source,
        });
        if let Some(peer) = source {
            let count = self.by_source.entry(peer).or_default();
            *count = count.saturating_add(1);
        }
        Ok(())
    }

    fn evict_expired(&mut self, now: Instant) {
        while self
            .admitted
            .front()
            .is_some_and(|event| now.saturating_duration_since(event.admitted_at) >= WINDOW)
        {
            let expired = self.admitted.pop_front().expect("front was present");
            let Some(peer) = expired.source else {
                continue;
            };
            let Some(count) = self.by_source.get_mut(&peer) else {
                continue;
            };
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.by_source.remove(&peer);
            }
        }
    }
}

impl PeerRecordAdmissionPermit {
    pub(super) fn admit_payload(
        shared: &SharedPeerRecordAdmissionBudget,
        source: Option<PeerId>,
        work_unit: PeerRecordAdmissionWorkUnit,
        payload: &[u8],
        max_payload_bytes: usize,
    ) -> Result<Self, WorldError> {
        let admitted_at = {
            let mut budget = shared.lock().map_err(|_| admission_state_unavailable())?;
            // Timestamp while holding the lock so FIFO eviction remains
            // chronological when concurrent callers race to enter the budget.
            let admitted_at = Instant::now();
            budget
                .admit_at(source, admitted_at)
                .map_err(admission_denied)?;
            admitted_at
        };

        // The charge is retained even for an oversized or malformed payload.
        // Check length before hashing, and never hold the shared budget lock
        // while hashing or decoding attacker-controlled bytes.
        if payload.len() > max_payload_bytes {
            return Err(WorldError::NetworkProtocolUnavailable {
                protocol: "peer-record admission payload exceeds bounded input size".into(),
            });
        }
        Ok(Self {
            source,
            payload_digest: *blake3::hash(payload).as_bytes(),
            work_unit,
            admitted_at,
            max_payload_bytes,
        })
    }

    pub(super) fn consume(
        self,
        expected_source: Option<PeerId>,
        expected_work_unit: PeerRecordAdmissionWorkUnit,
        payload: &[u8],
    ) -> Result<(), WorldError> {
        if self.source != expected_source
            || self.work_unit != expected_work_unit
            || Instant::now().saturating_duration_since(self.admitted_at) >= WINDOW
            || payload.len() > self.max_payload_bytes
            || self.payload_digest != *blake3::hash(payload).as_bytes()
        {
            return Err(WorldError::NetworkRequestFailed {
                code: DistributedErrorCode::ErrBusy,
                message: "peer-record admission permit is mismatched or expired".into(),
                retryable: true,
            });
        }
        Ok(())
    }
}

pub(super) fn admit_special_request(
    shared: &SharedPeerRecordAdmissionBudget,
    source: PeerId,
    protocol: &str,
    payload: &[u8],
) -> Result<Option<PeerRecordAdmissionPermit>, WorldError> {
    let is_special = matches!(
        protocol,
        super::peer_record_republish::RR_GET_LOCAL_PEER_RECORD
            | super::peer_record_republish::RR_GET_CACHED_PEER_RECORD
            | super::peer_record_republish::RR_GET_CACHED_DISCOVERY_PEERS
    );
    if !is_special {
        return Ok(None);
    }
    PeerRecordAdmissionPermit::admit_payload(
        shared,
        Some(source),
        PeerRecordAdmissionWorkUnit::SpecialRequestResponseRequest,
        payload,
        MAX_ADMISSION_PAYLOAD_BYTES,
    )
    .map(Some)
}

pub(super) fn admit_special_response(
    shared: &SharedPeerRecordAdmissionBudget,
    source: PeerId,
    protocol: &str,
    payload: &[u8],
) -> Result<(), WorldError> {
    let (unit, max_payload_bytes) = match protocol {
        super::peer_record_republish::RR_GET_CACHED_DISCOVERY_PEERS => (
            PeerRecordAdmissionWorkUnit::DiscoveryListResponse,
            super::discovery_budget::MAX_DISCOVERY_LIST_BYTES,
        ),
        super::peer_record_republish::RR_GET_LOCAL_PEER_RECORD
        | super::peer_record_republish::RR_GET_CACHED_PEER_RECORD => (
            PeerRecordAdmissionWorkUnit::PeerRecordResponse,
            crate::libp2p_net::peer_record::MAX_SIGNED_PEER_RECORD_BYTES,
        ),
        _ => return Ok(()),
    };
    PeerRecordAdmissionPermit::admit_payload(
        shared,
        Some(source),
        unit,
        payload,
        max_payload_bytes,
    )?
    .consume(Some(source), unit, payload)
}

pub(super) fn admit_discovery_list_response(
    shared: &SharedPeerRecordAdmissionBudget,
    source: PeerId,
    expected_source: PeerId,
    payload: &[u8],
) -> Result<Vec<PeerId>, WorldError> {
    let work_unit = PeerRecordAdmissionWorkUnit::DiscoveryListResponse;
    let permit = PeerRecordAdmissionPermit::admit_payload(
        shared,
        Some(source),
        work_unit,
        payload,
        super::discovery_budget::MAX_DISCOVERY_LIST_BYTES,
    )?;
    if source != expected_source {
        return Err(WorldError::NetworkProtocolUnavailable {
            protocol: "cached discovery response came from an unexpected peer".into(),
        });
    }
    let peer_ids = super::discovery::decode_cached_discovery_peers_response(payload)?;
    permit.consume(Some(source), work_unit, payload)?;
    Ok(peer_ids)
}

pub(super) fn decode_optional_peer_record_response(
    payload: &[u8],
) -> Result<Option<SignedPeerRecord>, WorldError> {
    if payload.len() > crate::libp2p_net::peer_record::MAX_SIGNED_PEER_RECORD_BYTES {
        return Err(WorldError::NetworkProtocolUnavailable {
            protocol: "peer record payload capacity exceeded".into(),
        });
    }
    if let Ok(error) = serde_cbor::from_slice::<ErrorResponse>(payload) {
        if error.code == DistributedErrorCode::ErrNotFound {
            return Ok(None);
        }
        return Err(WorldError::NetworkRequestFailed {
            code: error.code,
            message: error.message,
            retryable: error.retryable,
        });
    }
    Ok(Some(serde_cbor::from_slice(payload)?))
}

pub(super) fn admit_optional_peer_record_response(
    shared: &SharedPeerRecordAdmissionBudget,
    source: PeerId,
    payload: &[u8],
) -> Result<Option<AdmittedPeerRecord>, WorldError> {
    AdmittedPeerRecord::admit_and_decode_optional_with(
        shared,
        Some(source),
        PeerRecordAdmissionWorkUnit::PeerRecordResponse,
        payload,
        crate::libp2p_net::peer_record::MAX_SIGNED_PEER_RECORD_BYTES,
        decode_optional_peer_record_response,
    )
}

impl AdmittedPeerRecord {
    pub(super) fn admit_and_decode_with(
        shared: &SharedPeerRecordAdmissionBudget,
        source: Option<PeerId>,
        work_unit: PeerRecordAdmissionWorkUnit,
        raw_payload: &[u8],
        max_payload_bytes: usize,
        decode: impl FnOnce(&[u8]) -> Result<SignedPeerRecord, WorldError>,
    ) -> Result<Self, WorldError> {
        let permit = PeerRecordAdmissionPermit::admit_payload(
            shared,
            source,
            work_unit,
            raw_payload,
            max_payload_bytes,
        )?;
        let record = decode(raw_payload)?;
        Ok(Self {
            record,
            raw_payload: raw_payload.to_vec(),
            permit,
        })
    }

    pub(super) fn admit_and_decode_optional_with(
        shared: &SharedPeerRecordAdmissionBudget,
        source: Option<PeerId>,
        work_unit: PeerRecordAdmissionWorkUnit,
        raw_payload: &[u8],
        max_payload_bytes: usize,
        decode: impl FnOnce(&[u8]) -> Result<Option<SignedPeerRecord>, WorldError>,
    ) -> Result<Option<Self>, WorldError> {
        let permit = PeerRecordAdmissionPermit::admit_payload(
            shared,
            source,
            work_unit,
            raw_payload,
            max_payload_bytes,
        )?;
        let Some(record) = decode(raw_payload)? else {
            permit.consume(source, work_unit, raw_payload)?;
            return Ok(None);
        };
        Ok(Some(Self {
            record,
            raw_payload: raw_payload.to_vec(),
            permit,
        }))
    }

    pub(super) fn record(&self) -> &SignedPeerRecord {
        &self.record
    }

    pub(super) fn source(&self) -> Option<PeerId> {
        self.permit.source
    }

    pub(super) fn consume(
        self,
        expected_source: Option<PeerId>,
        expected_work_unit: PeerRecordAdmissionWorkUnit,
    ) -> Result<SignedPeerRecord, WorldError> {
        self.permit.consume(
            expected_source,
            expected_work_unit,
            self.raw_payload.as_slice(),
        )?;
        Ok(self.record)
    }
}

fn admission_denied(reason: AdmissionDenial) -> WorldError {
    let message = match reason {
        AdmissionDenial::Global => "peer-record global admission budget is busy",
        AdmissionDenial::Source => "peer-record source admission budget is busy",
        AdmissionDenial::TrackingCapacity => "peer-record source tracking capacity is busy",
    };
    WorldError::NetworkRequestFailed {
        code: DistributedErrorCode::ErrBusy,
        message: message.into(),
        retryable: true,
    }
}

fn admission_state_unavailable() -> WorldError {
    WorldError::NetworkRequestFailed {
        code: DistributedErrorCode::ErrBusy,
        message: "peer-record admission state is unavailable".into(),
        retryable: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_denials_do_not_spend_global_budget_and_expire_at_sixty_seconds() {
        let source = PeerId::random();
        let other = PeerId::random();
        let start = Instant::now();
        let mut budget = PeerRecordAdmissionBudget::default();
        for _ in 0..usize::from(SOURCE_LIMIT) {
            assert!(budget.admit_at(Some(source), start).is_ok());
        }
        assert_eq!(
            budget.admit_at(Some(source), start + WINDOW - Duration::from_nanos(1)),
            Err(AdmissionDenial::Source)
        );
        assert!(
            budget
                .admit_at(Some(other), start + WINDOW - Duration::from_nanos(1))
                .is_ok()
        );
        assert!(budget.admit_at(Some(source), start + WINDOW).is_ok());
        assert_eq!(budget.by_source.get(&source), Some(&1));
    }

    #[test]
    fn a_permit_binds_source_work_unit_and_exact_wire_payload() {
        let shared = PeerRecordAdmissionBudget::shared();
        let source = PeerId::random();
        let payload = b"signed-wire-payload";
        let permit = PeerRecordAdmissionPermit::admit_payload(
            &shared,
            Some(source),
            PeerRecordAdmissionWorkUnit::PeerRecordResponse,
            payload,
            64,
        )
        .expect("admit bounded payload");
        assert!(
            permit
                .consume(
                    Some(PeerId::random()),
                    PeerRecordAdmissionWorkUnit::PeerRecordResponse,
                    payload,
                )
                .is_err()
        );

        let permit = PeerRecordAdmissionPermit::admit_payload(
            &shared,
            Some(source),
            PeerRecordAdmissionWorkUnit::PeerRecordResponse,
            payload,
            64,
        )
        .expect("admit second bounded payload");
        assert!(
            permit
                .consume(
                    Some(source),
                    PeerRecordAdmissionWorkUnit::KadGetPeerRecordResult,
                    payload,
                )
                .is_err()
        );

        let permit = PeerRecordAdmissionPermit::admit_payload(
            &shared,
            Some(source),
            PeerRecordAdmissionWorkUnit::PeerRecordResponse,
            payload,
            64,
        )
        .expect("admit third bounded payload");
        assert!(
            permit
                .consume(
                    Some(source),
                    PeerRecordAdmissionWorkUnit::PeerRecordResponse,
                    b"different-wire-payload",
                )
                .is_err()
        );
    }

    #[test]
    fn oversized_payload_is_charged_but_never_hashed_or_decoded() {
        let shared = PeerRecordAdmissionBudget::shared();
        let source = PeerId::random();
        let payload = [0_u8; 65];
        let result = PeerRecordAdmissionPermit::admit_payload(
            &shared,
            Some(source),
            PeerRecordAdmissionWorkUnit::PeerRecordResponse,
            &payload,
            64,
        );
        assert!(result.is_err());
        assert_eq!(shared.lock().unwrap().admitted.len(), 1);
    }
}

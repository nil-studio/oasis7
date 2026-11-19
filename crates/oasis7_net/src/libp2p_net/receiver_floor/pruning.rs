use std::collections::BTreeSet;

use libp2p::identity::Keypair;
use oasis7_proto::distributed_dht::PeerRecord;

use crate::error::WorldError;

use super::super::config::{PeerRecordReceiverConfig, PeerRecordReceiverMode};
use super::{
    MAX_FUTURE_SKEW_MS, MAX_RECEIVER_FLOORS, MAX_RECORD_TTL_MS, PeerFloorV2, PeerRecordReceiver,
    ReceiverLedgerV2, decode_stored_record, ensure_durable_usable, persist_next, protocol_error,
};

impl PeerRecordReceiver {
    pub(in crate::libp2p_net) fn prune_floors(
        &self,
        trusted_now_ms: i64,
        retained_peers: &BTreeSet<String>,
    ) -> Result<usize, WorldError> {
        if self.state.mode == PeerRecordReceiverMode::Legacy {
            return Ok(0);
        }
        let _ingress = self.begin_ingress()?;
        let mut durable = self.durable_lock_required()?;
        ensure_durable_usable(&durable)?;
        if trusted_now_ms <= 0 {
            return Err(protocol_error("trusted time must be positive".into()));
        }
        if let Some(last) = durable.last_trusted_now_ms
            && trusted_now_ms < last
        {
            return Err(protocol_error("trusted time moved backward".into()));
        }
        durable.last_trusted_now_ms = Some(trusted_now_ms);

        let mut prunable_peers = Vec::new();
        for (peer_id, floor) in &durable.ledger.floors {
            if is_floor_prunable(peer_id, floor, trusted_now_ms, retained_peers)? {
                prunable_peers.push(peer_id.clone());
            }
        }
        let expired_v1_count = durable
            .ledger
            .pre_migration_v1_expiries
            .values()
            .filter(|expiry| **expiry <= trusted_now_ms)
            .count();
        if prunable_peers.is_empty() && expired_v1_count == 0 {
            return Ok(0);
        }
        let mut next = durable.ledger.clone();
        for peer_id in &prunable_peers {
            next.floors.remove(peer_id);
        }
        next.pre_migration_v1_expiries
            .retain(|_peer_id, expiry| *expiry > trusted_now_ms);
        next.pruned_trusted_time_watermark_ms = Some(
            next.pruned_trusted_time_watermark_ms
                .map_or(trusted_now_ms, |prev| prev.max(trusted_now_ms)),
        );
        persist_next(&mut durable, next)?;
        Ok(prunable_peers.len())
    }

    pub(in crate::libp2p_net) fn observe_trusted_time(
        &self,
        trusted_now_ms: i64,
    ) -> Result<(), WorldError> {
        if self.state.mode == PeerRecordReceiverMode::Legacy {
            return Ok(());
        }
        let mut durable = self.durable_lock_required()?;
        ensure_durable_usable(&durable)?;
        if trusted_now_ms <= 0 {
            return Err(protocol_error("trusted time must be positive".into()));
        }
        if let Some(last) = durable.last_trusted_now_ms
            && trusted_now_ms < last
        {
            return Err(protocol_error("trusted time moved backward".into()));
        }
        durable.last_trusted_now_ms = Some(trusted_now_ms);
        Ok(())
    }

    pub(in crate::libp2p_net) fn prune_offline(
        config: &PeerRecordReceiverConfig,
        keypair: &Keypair,
        template: &PeerRecord,
        trusted_now_ms: i64,
        retained_peers: &BTreeSet<String>,
    ) -> Result<usize, WorldError> {
        let receiver = Self::open(config, keypair, Some(template))?;
        receiver.prune_floors(trusted_now_ms, retained_peers)
    }

    pub(in crate::libp2p_net) fn floor_count(&self) -> Result<usize, WorldError> {
        let durable = self.durable_lock()?;
        if let Some(durable) = durable.as_ref() {
            ensure_durable_usable(durable)?;
            return Ok(durable.ledger.floors.len());
        }
        Ok(0)
    }

    #[cfg(all(test, unix))]
    pub(in crate::libp2p_net) fn contains_floor(&self, peer_id: &str) -> Result<bool, WorldError> {
        let durable = self.durable_lock()?;
        if let Some(durable) = durable.as_ref() {
            ensure_durable_usable(durable)?;
            return Ok(durable.ledger.floors.contains_key(peer_id));
        }
        Ok(false)
    }
}

pub(super) fn is_floor_prunable(
    peer_id: &str,
    floor: &PeerFloorV2,
    trusted_now_ms: i64,
    retained_peers: &BTreeSet<String>,
) -> Result<bool, WorldError> {
    if floor.isolated || floor.conflict_signed_record_cbor.is_some() {
        return Ok(false);
    }
    if retained_peers.contains(peer_id) {
        return Ok(false);
    }
    let primary = decode_stored_record(&floor.primary_signed_record_cbor)?;
    let primary_expiry = primary
        .record
        .expires_at_ms()
        .map_err(|reason| protocol_error(format!("invalid stored record expiry: {reason}")))?;
    let effective_max_expiry = floor
        .max_expiry_ms
        .unwrap_or(primary_expiry)
        .max(primary_expiry);
    if trusted_now_ms < effective_max_expiry {
        return Ok(false);
    }
    let admission_time = match floor.admitted_at_ms {
        Some(t) => t.max(primary.record.published_at_ms),
        None => {
            if floor.sequence > 1 {
                // Pre-upgrade or unproven floor with sequence > 1 cannot prove
                // that unobserved lower-sequence records have expired.
                return Ok(false);
            }
            primary.record.published_at_ms
        }
    };
    let max_possible_expiry = admission_time
        .saturating_add(MAX_RECORD_TTL_MS)
        .saturating_add(MAX_FUTURE_SKEW_MS);
    if trusted_now_ms < max_possible_expiry {
        return Ok(false);
    }
    Ok(true)
}

pub(super) fn validate_ledger_structure(ledger: &ReceiverLedgerV2) -> Result<(), WorldError> {
    if ledger.floors.len() + ledger.pre_migration_v1_expiries.len() > MAX_RECEIVER_FLOORS {
        return Err(protocol_error("receiver floor capacity exceeded".into()));
    }
    if let Some(watermark) = ledger.pruned_trusted_time_watermark_ms
        && watermark <= 0
    {
        return Err(protocol_error(
            "receiver pruning watermark must be positive".into(),
        ));
    }
    for (peer_id, expiry) in &ledger.pre_migration_v1_expiries {
        if *expiry <= 0 {
            return Err(protocol_error(
                "receiver pre-migration V1 expiry must be positive".into(),
            ));
        }
        if ledger.floors.contains_key(peer_id) {
            return Err(protocol_error(
                "receiver pre-migration V1 record retained after floor migration".into(),
            ));
        }
    }
    Ok(())
}

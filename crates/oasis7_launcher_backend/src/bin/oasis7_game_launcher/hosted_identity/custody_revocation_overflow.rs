//! Bounded permanent fences; full journal history is retained separately.
use super::*;

pub(crate) const KIND: &str = "custody_source_overflow";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OverflowRecord {
    pub trigger: SourceOutbox,
    pub checkpoint: SignedSourceRevocationCheckpointV1,
}
pub(super) fn record_key(manager: &str) -> String {
    format!("custody_source_overflow:{manager}")
}
impl RevocationSigner {
    pub(super) fn record_overflow(
        &self,
        tx: &mut IdentityTransaction<'_>,
        original: SignedSourceRevocationEventV1,
    ) -> Result<(), StoreError> {
        let manager = &original.event.manager_account_id;
        let key = record_key(manager);
        let stored = tx.get::<OverflowRecord>(&key)?;
        if let Some(old) = &stored {
            verify_checkpoint(&old.value, &self.metadata)?;
        }
        let (first, previous_checkpoint_digest, watermark, previous_chain) = match &stored {
            Some(old) => (
                old.value.trigger.original.clone(),
                old.value
                    .checkpoint
                    .digest()
                    .map_err(|_| StoreError::Corrupt)?,
                old.value
                    .checkpoint
                    .checkpoint
                    .watermark
                    .0
                    .saturating_add(1),
                old.value.checkpoint.checkpoint.chain_digest.clone(),
            ),
            None => (original.clone(), checkpoint_seed(), 1, checkpoint_seed()),
        };
        let latest_event_digest = original.digest().map_err(|_| StoreError::Corrupt)?;
        let checkpoint = SourceRevocationCheckpointV1 {
            version: 1,
            key_id: self.key_id.clone(),
            identity_namespace: self.namespace.clone(),
            environment: self.environment.clone(),
            identity_instance_epoch: DecimalU64(self.epoch),
            manager_account_id: manager.clone(),
            owner_audience: self.audience.clone(),
            participant_registry_digest: participant_registry_digest(
                &self.namespace,
                DecimalU64(self.epoch),
                &self.metadata.participants,
            )
            .map_err(|_| StoreError::Corrupt)?,
            first_event_id: first.event.event_id.clone(),
            first_event_digest: first.digest().map_err(|_| StoreError::Corrupt)?,
            previous_checkpoint_digest,
            latest_event_digest: latest_event_digest.clone(),
            watermark: DecimalU64(watermark),
            chain_digest: checkpoint_chain(
                &previous_chain,
                DecimalU64(watermark),
                &latest_event_digest,
            )
            .map_err(|_| StoreError::Corrupt)?,
            observed_at: original.event.observed_at,
            state: SourceCheckpointStateV1::PermanentFence,
        };
        let signed = SignedSourceRevocationCheckpointV1 {
            signature: URL_SAFE_NO_PAD.encode(
                self.key
                    .sign(
                        &checkpoint
                            .signing_bytes()
                            .map_err(|_| StoreError::Corrupt)?,
                    )
                    .to_bytes(),
            ),
            checkpoint,
        };
        let trigger = stored
            .as_ref()
            .map(|old| old.value.trigger.clone())
            .unwrap_or(SourceOutbox {
                original: first,
                acknowledgement: None,
                last_delivery: DecimalU64(0),
            });
        let record = OverflowRecord {
            trigger,
            checkpoint: signed,
        };
        if let Some(old) = stored {
            tx.replace(&key, old.row_version, KIND, Some(manager), None, &record)
        } else {
            if tx.count_kind(KIND)? >= self.metadata.participants.len() {
                return Err(StoreError::Corrupt);
            }
            tx.create(&key, KIND, Some(manager), None, &record)
        }
    }
}
pub(crate) fn verify_checkpoint(
    record: &OverflowRecord,
    metadata: &BridgeMetadata,
) -> Result<(), StoreError> {
    let c = &record.checkpoint.checkpoint;
    let event = &record.trigger.original;
    if c.key_id != metadata.native_key_id
        || c.identity_namespace != metadata.namespace
        || c.identity_instance_epoch.0 > metadata.epoch
        || c.identity_instance_epoch.0 < event.event.identity_instance_epoch.0
        || event.event.key_id != c.key_id
        || event.event.identity_namespace != c.identity_namespace
        || event.event.owner_audience != c.owner_audience
        || c.environment != event.event.environment
        || c.owner_audience != metadata.owner_audience
        || c.manager_account_id != event.event.manager_account_id
        || !metadata.participants.contains(&c.manager_account_id)
        || c.first_event_id != event.event.event_id
        || c.first_event_digest != event.digest().map_err(|_| StoreError::Corrupt)?
        || c.observed_at.0 < event.event.observed_at.0
        || c.participant_registry_digest
            != participant_registry_digest(
                &metadata.namespace,
                c.identity_instance_epoch,
                &metadata.participants,
            )
            .map_err(|_| StoreError::Corrupt)?
    {
        return Err(StoreError::Corrupt);
    }
    let key_bytes: [u8; 32] = hex::decode(&metadata.native_public_key)
        .map_err(|_| StoreError::Corrupt)?
        .try_into()
        .map_err(|_| StoreError::Corrupt)?;
    let key = VerifyingKey::from_bytes(&key_bytes).map_err(|_| StoreError::Corrupt)?;
    for (bytes, signature) in [
        (
            c.signing_bytes().map_err(|_| StoreError::Corrupt)?,
            &record.checkpoint.signature,
        ),
        (
            event
                .event
                .signing_bytes()
                .map_err(|_| StoreError::Corrupt)?,
            &event.signature,
        ),
    ] {
        let raw = URL_SAFE_NO_PAD
            .decode(signature)
            .map_err(|_| StoreError::Corrupt)?;
        if URL_SAFE_NO_PAD.encode(&raw) != *signature || key.is_weak() {
            return Err(StoreError::Corrupt);
        }
        key.verify_strict(
            &bytes,
            &Signature::from_slice(&raw).map_err(|_| StoreError::Corrupt)?,
        )
        .map_err(|_| StoreError::Corrupt)?;
    }
    if c.watermark.0 == 1
        && (c.previous_checkpoint_digest != checkpoint_seed()
            || c.latest_event_digest != c.first_event_digest
            || c.chain_digest
                != checkpoint_chain(&checkpoint_seed(), c.watermark, &c.first_event_digest)
                    .map_err(|_| StoreError::Corrupt)?)
    {
        return Err(StoreError::Corrupt);
    }
    Ok(())
}
/// Only custody APIs are gated. Ordinary authentication and invalidation stay available.
pub(crate) fn require_participant(
    tx: &IdentityTransaction<'_>,
    manager: &str,
) -> Result<(), StoreError> {
    let meta = tx
        .get::<BridgeMetadata>("custody_revocation_bridge_meta")?
        .ok_or_else(|| reject("custody_participant_required"))?;
    if !meta.value.participants.iter().any(|id| id == manager) {
        return Err(reject("custody_participant_required"));
    }
    if tx.get::<OverflowRecord>(&record_key(manager))?.is_some() {
        return Err(reject("custody_source_permanently_fenced"));
    }
    Ok(())
}

pub(super) fn poll_records(
    tx: &mut IdentityTransaction<'_>,
    limit: usize,
) -> Result<Vec<SignedSourceRevocationEventV1>, StoreError> {
    let metadata = tx
        .get::<BridgeMetadata>("custody_revocation_bridge_meta")?
        .ok_or(StoreError::Corrupt)?
        .value;
    let mut entries = Vec::new();
    for (key, row) in tx.all::<SourceOutbox>("custody_source_revocation", None)? {
        entries.push((key, row.row_version, row.value, None));
    }
    for (key, row) in tx.all::<OverflowRecord>(KIND, None)? {
        verify_checkpoint(&row.value, &metadata)?;
        entries.push((
            key,
            row.row_version,
            row.value.trigger,
            Some(row.value.checkpoint),
        ));
    }
    let mut sequence = entries
        .iter()
        .map(|(_, _, record, _)| record.last_delivery.0)
        .max()
        .unwrap_or(0);
    entries.sort_by(|a, b| {
        a.2.last_delivery
            .0
            .cmp(&b.2.last_delivery.0)
            .then(a.0.cmp(&b.0))
    });
    let mut result = Vec::new();
    for (key, version, mut record, checkpoint) in entries
        .into_iter()
        .filter(|e| e.2.acknowledgement.is_none())
        .take(limit)
    {
        // Delivery counters may saturate, but never block a security mutation.
        sequence = sequence.saturating_add(1);
        record.last_delivery = DecimalU64(sequence);
        let manager = record.original.event.manager_account_id.clone();
        if let Some(checkpoint) = checkpoint {
            tx.replace(
                &key,
                version,
                KIND,
                Some(&manager),
                None,
                &OverflowRecord {
                    trigger: record.clone(),
                    checkpoint,
                },
            )?;
        } else {
            tx.replace(
                &key,
                version,
                "custody_source_revocation",
                Some(&manager),
                None,
                &record,
            )?;
        }
        result.push(record.original);
    }
    Ok(result)
}
pub(super) fn acknowledge(
    tx: &mut IdentityTransaction<'_>,
    signed: &SignedSourceClosureAckV1,
) -> Result<(), StoreError> {
    let ack = &signed.ack;
    let normal_key = format!("custody_source_revocation:{}", ack.event_id);
    let normal = tx.get::<SourceOutbox>(&normal_key)?;
    let overflow_key = record_key(&ack.manager_account_id);
    let overflow = if normal.is_none() {
        tx.get::<OverflowRecord>(&overflow_key)?
    } else {
        None
    };
    let metadata = tx
        .get::<BridgeMetadata>("custody_revocation_bridge_meta")?
        .ok_or(StoreError::Corrupt)?
        .value;
    if let Some(row) = &overflow {
        verify_checkpoint(&row.value, &metadata)?;
    }
    let mut record = normal
        .as_ref()
        .map(|r| r.value.clone())
        .or_else(|| overflow.as_ref().map(|r| r.value.trigger.clone()))
        .ok_or_else(|| reject("revocation_event_missing"))?;
    if record.original.event.event_id != ack.event_id {
        return Err(reject("revocation_ack_binding_mismatch"));
    }
    let mut candidate = record.clone();
    candidate.acknowledgement = Some(signed.clone());
    verify_barrier_ack(&candidate, &metadata)?;
    if let Some(original) = &record.acknowledgement {
        return if original == signed {
            Ok(())
        } else {
            Err(reject("idempotency_conflict"))
        };
    }
    record.acknowledgement = Some(signed.clone());
    if let Some(row) = normal {
        tx.replace(
            &normal_key,
            row.row_version,
            "custody_source_revocation",
            Some(&ack.manager_account_id),
            None,
            &record,
        )
    } else {
        let row = overflow.ok_or(StoreError::Corrupt)?;
        tx.replace(
            &overflow_key,
            row.row_version,
            KIND,
            Some(&ack.manager_account_id),
            None,
            &OverflowRecord {
                trigger: record,
                checkpoint: row.value.checkpoint,
            },
        )
    }
}

//! Charged cold reconstruction of original rollback records from typed child roots.
use crate::{
    incremental_field_member::MemberPayloadCodecV1, incremental_rollback_genesis::RollbackWorkV1,
    incremental_rollback_read::read_rollback_cell_v1, incremental_rollback_records::RollbackPathV1,
    incremental_semantic_key::SemanticKeyBudget, snapshot::*,
};
use RollbackPathV1::{Column as C, Ordinal as O};
use oasis7_formal_protocol::{
    incremental_state::StateRootV1,
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
pub struct RollbackDecodeLimitsV1 {
    pub max_output_bytes: usize,
    pub max_text_bytes: u64,
    pub max_record_bytes: u64,
}
pub struct RollbackDecodeContextV1<'a, 'v, R, C> {
    pub values: &'a mut ValueReadContext<'v, R>,
    pub keys: &'a mut SemanticKeyBudget,
    pub work: &'a mut RollbackWorkV1,
    pub limits: RollbackDecodeLimitsV1,
    pub cancelled: &'a mut C,
}
struct Decoder<'a, 'b, 'v, R, C> {
    ctx: &'a mut RollbackDecodeContextV1<'b, 'v, R, C>,
    root: &'a StateRootV1,
    schema: [u8; 32],
    remaining: usize,
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("rollback decoded profile")
}
fn path(prefix: &[RollbackPathV1], last: &'static str) -> ([RollbackPathV1; 4], usize) {
    let mut path = [C(""); 4];
    path[..prefix.len()].copy_from_slice(prefix);
    path[prefix.len()] = C(last);
    (path, prefix.len() + 1)
}
impl<R: ValueNodeReader, Cb: FnMut() -> bool> Decoder<'_, '_, '_, R, Cb> {
    fn admit(&mut self, n: usize) -> Result<(), ValueTreeError> {
        self.remaining = self
            .remaining
            .checked_sub(n)
            .ok_or(ValueTreeError::ResourceLimited("rollback decoded output"))?;
        Ok(())
    }
    fn read(
        &mut self,
        path: &[RollbackPathV1],
        codec: MemberPayloadCodecV1,
        max: u64,
        output: &mut impl std::io::Write,
    ) -> Result<u64, ValueTreeError> {
        let info = read_rollback_cell_v1(
            self.root,
            self.schema,
            path,
            codec,
            self.ctx.values,
            self.ctx.keys,
            self.ctx.work,
            max,
            self.ctx.limits.max_record_bytes,
            output,
            self.ctx.cancelled,
        )?;
        Ok(info.payload_bytes)
    }
    fn text(
        &mut self,
        prefix: &[RollbackPathV1],
        name: &'static str,
    ) -> Result<String, ValueTreeError> {
        let (path, n) = path(prefix, name);
        self.text_path(&path[..n])
    }
    fn text_path(&mut self, path: &[RollbackPathV1]) -> Result<String, ValueTreeError> {
        let len = self.read(
            path,
            MemberPayloadCodecV1::Text,
            self.ctx.limits.max_text_bytes,
            &mut std::io::sink(),
        )?;
        let len = usize::try_from(len).map_err(|_| invalid())?;
        self.admit(len)?;
        let mut bytes = vec![0u8; len];
        self.admit(bytes.capacity().checked_sub(len).ok_or_else(invalid)?)?;
        self.read(
            path,
            MemberPayloadCodecV1::Text,
            len as u64,
            &mut std::io::Cursor::new(bytes.as_mut_slice()),
        )?;
        String::from_utf8(bytes).map_err(|_| invalid())
    }
    fn u64(
        &mut self,
        prefix: &[RollbackPathV1],
        name: &'static str,
    ) -> Result<u64, ValueTreeError> {
        let (path, n) = path(prefix, name);
        let mut bytes = [0; 8];
        if self.read(
            &path[..n],
            MemberPayloadCodecV1::U64,
            8,
            &mut std::io::Cursor::new(bytes.as_mut_slice()),
        )? != 8
        {
            return Err(invalid());
        }
        Ok(u64::from_be_bytes(bytes))
    }
    fn usize(
        &mut self,
        prefix: &[RollbackPathV1],
        name: &'static str,
    ) -> Result<usize, ValueTreeError> {
        usize::try_from(self.u64(prefix, name)?).map_err(|_| invalid())
    }
    fn boolean(
        &mut self,
        prefix: &[RollbackPathV1],
        name: &'static str,
    ) -> Result<bool, ValueTreeError> {
        let (path, n) = path(prefix, name);
        let mut bytes = [0];
        if self.read(
            &path[..n],
            MemberPayloadCodecV1::Bool,
            1,
            &mut std::io::Cursor::new(bytes.as_mut_slice()),
        )? != 1
        {
            return Err(invalid());
        }
        match bytes[0] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(invalid()),
        }
    }
    fn strings(
        &mut self,
        prefix: &[RollbackPathV1],
        name: &'static str,
    ) -> Result<Vec<String>, ValueTreeError> {
        let (path, n) = path(prefix, name);
        let prefix = &path[..n];
        let count = self.usize(prefix, "count")?;
        self.admit(
            count
                .checked_mul(std::mem::size_of::<String>())
                .ok_or_else(invalid)?,
        )?;
        let mut result = Vec::with_capacity(count);
        self.admit(
            result
                .capacity()
                .checked_sub(count)
                .and_then(|n| n.checked_mul(std::mem::size_of::<String>()))
                .ok_or_else(invalid)?,
        )?;
        for i in 0..count {
            let mut key = [C(""); 4];
            key[..n].copy_from_slice(prefix);
            key[n] = O(u64::try_from(i).map_err(|_| invalid())?);
            result.push(self.text_path(&key[..n + 1])?);
        }
        Ok(result)
    }
    fn optional_text(
        &mut self,
        prefix: &[RollbackPathV1],
        name: &'static str,
    ) -> Result<Option<String>, ValueTreeError> {
        let (path, n) = path(prefix, name);
        if self.boolean(&path[..n], "present")? {
            Ok(Some(self.text(&path[..n], "value")?))
        } else {
            Ok(None)
        }
    }
    fn disposition(&mut self, index: usize) -> Result<RollbackEventDisposition, ValueTreeError> {
        let prefix = &[
            C("dispositions"),
            O(u64::try_from(index).map_err(|_| invalid())?),
        ];
        let source_batch_id = self.text(prefix, "source_batch_id")?;
        let source_event_id = self.u64(prefix, "source_event_id")?;
        let status = match self.u64(prefix, "status")? {
            0 => RollbackDispositionStatus::PreservedAtTarget,
            1 => RollbackDispositionStatus::Replayed,
            2 => RollbackDispositionStatus::RejectedFork,
            3 => RollbackDispositionStatus::CompensationRequired,
            _ => return Err(invalid()),
        };
        let player_id = self.optional_text(prefix, "player_id")?;
        let action_id = self.optional_text(prefix, "action_id")?;
        let system_authored = self.boolean(prefix, "system_authored")?;
        let compensation_prefix = &[prefix[0], prefix[1], C("compensation")];
        let compensation = if self.boolean(compensation_prefix, "present")? {
            Some(RollbackCompensationCaseRef {
                owner_id: self.text(compensation_prefix, "owner_id")?,
                ticket_id: self.text(compensation_prefix, "ticket_id")?,
                state: match self.u64(compensation_prefix, "state")? {
                    0 => RollbackCompensationState::PendingAuthorization,
                    1 => RollbackCompensationState::Authorized,
                    2 => RollbackCompensationState::InProgress,
                    3 => RollbackCompensationState::Completed,
                    4 => RollbackCompensationState::Rejected,
                    _ => return Err(invalid()),
                },
            })
        } else {
            None
        };
        Ok(RollbackEventDisposition {
            source_batch_id,
            source_event_id,
            status,
            compensation,
            player_id,
            action_id,
            system_authored,
        })
    }
    fn receipt(&mut self) -> Result<RollbackReceiptProjection, ValueTreeError> {
        let prefix = &[C("receipt")];
        Ok(RollbackReceiptProjection {
            receipt_id: self.text(prefix, "receipt_id")?,
            canonical_intent_digest: self.text(prefix, "canonical_intent_digest")?,
            rollback_checkpoint_batch_id: self.text(prefix, "rollback_checkpoint_batch_id")?,
            rollback_checkpoint_snapshot_hash: self
                .text(prefix, "rollback_checkpoint_snapshot_hash")?,
            replay_target_batch_id: self.text(prefix, "replay_target_batch_id")?,
            replay_target_state_root: self.text(prefix, "replay_target_state_root")?,
            replay_target_journal_commitment: self
                .text(prefix, "replay_target_journal_commitment")?,
            affected_census_digest: self.text(prefix, "affected_census_digest")?,
            snapshot_hash: self.text(prefix, "snapshot_hash")?,
            rollback_checkpoint_journal_len: self
                .usize(prefix, "rollback_checkpoint_journal_len")?,
            replay_target_journal_len: self.usize(prefix, "replay_target_journal_len")?,
            affected_census_count: self.usize(prefix, "affected_census_count")?,
            snapshot_height: self.u64(prefix, "snapshot_height")?,
            log_cursor: self.u64(prefix, "log_cursor")?,
            acknowledged_at_tick: self.u64(prefix, "acknowledged_at_tick")?,
            readiness_blockers: self.strings(prefix, "readiness_blockers")?,
        })
    }
}
pub fn recover_rollback_outcome_v1<R: ValueNodeReader, Cb: FnMut() -> bool>(
    root: &StateRootV1,
    ctx: &mut RollbackDecodeContextV1<'_, '_, R, Cb>,
) -> Result<ReservedValue<RollbackNonceOutcome>, ValueTreeError> {
    let charge = ctx
        .values
        .memory
        .reserve(ctx.limits.max_output_bytes, ctx.values.class)?;
    let remaining = ctx
        .limits
        .max_output_bytes
        .checked_sub(std::mem::size_of::<RollbackNonceOutcome>())
        .ok_or(ValueTreeError::ResourceLimited("rollback output header"))?;
    let mut decoder = Decoder {
        root,
        schema: *blake3::hash(b"runtime.rollback.outcome.v1").as_bytes(),
        ctx,
        remaining,
    };
    let prefix = &[];
    let canonical_intent_hash = decoder.text(prefix, "canonical_intent_hash")?;
    let rollback_checkpoint_batch_id = decoder.text(prefix, "rollback_checkpoint_batch_id")?;
    let rollback_checkpoint_snapshot_hash =
        decoder.text(prefix, "rollback_checkpoint_snapshot_hash")?;
    let rollback_ticket = decoder.text(prefix, "rollback_ticket")?;
    let target_journal_commitment = decoder.text(prefix, "target_journal_commitment")?;
    let target_state_root = decoder.text(prefix, "target_state_root")?;
    let affected_census_digest = decoder.text(prefix, "affected_census_digest")?;
    let target_batch_id = decoder.text(prefix, "target_batch_id")?;
    let rollback_checkpoint_journal_len =
        decoder.usize(prefix, "rollback_checkpoint_journal_len")?;
    let target_journal_len = decoder.usize(prefix, "target_journal_len")?;
    let affected_census_count = decoder.usize(prefix, "affected_census_count")?;
    let rollback_event_id = decoder.u64(prefix, "rollback_event_id")?;
    let prior_reorg_epoch = decoder.u64(prefix, "prior_reorg_epoch")?;
    let committed_reorg_epoch = decoder.u64(prefix, "committed_reorg_epoch")?;
    let readiness_blockers = decoder.strings(prefix, "readiness_blockers")?;
    let invalidated_batch_ids = decoder.strings(prefix, "invalidated_batch_ids")?;
    let count = decoder.usize(&[C("dispositions")], "count")?;
    decoder.admit(
        count
            .checked_mul(std::mem::size_of::<RollbackEventDisposition>())
            .ok_or_else(invalid)?,
    )?;
    let mut dispositions = Vec::with_capacity(count);
    decoder.admit(
        dispositions
            .capacity()
            .checked_sub(count)
            .and_then(|n| n.checked_mul(std::mem::size_of::<RollbackEventDisposition>()))
            .ok_or_else(invalid)?,
    )?;
    for i in 0..count {
        dispositions.push(decoder.disposition(i)?);
    }
    let receipt = if decoder.boolean(&[C("receipt")], "present")? {
        Some(decoder.receipt()?)
    } else {
        None
    };
    if (decoder.ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    let outcome = RollbackNonceOutcome {
        canonical_intent_hash,
        rollback_checkpoint_batch_id,
        rollback_checkpoint_snapshot_hash,
        rollback_ticket,
        target_journal_commitment,
        target_state_root,
        affected_census_digest,
        target_batch_id,
        rollback_checkpoint_journal_len,
        target_journal_len,
        affected_census_count,
        rollback_event_id,
        prior_reorg_epoch,
        committed_reorg_epoch,
        readiness_blockers,
        invalidated_batch_ids,
        dispositions,
        receipt,
    };
    crate::incremental_rollback_qualification::qualify_rollback_record_v1(
        root,
        crate::incremental_rollback_genesis::RollbackRecordRefV1::Outcome(&outcome),
        decoder.ctx,
    )?;
    Ok(charge.retain(outcome))
}

/// Reconstruct one original authority record; registry-wide role/key semantics
/// remain checked by RollbackAuthorityRegistry::new on assembled members.
pub fn recover_rollback_authority_v1<R: ValueNodeReader, Cb: FnMut() -> bool>(
    root: &StateRootV1,
    ctx: &mut RollbackDecodeContextV1<'_, '_, R, Cb>,
) -> Result<ReservedValue<RollbackAuthorityRecord>, ValueTreeError> {
    let charge = ctx
        .values
        .memory
        .reserve(ctx.limits.max_output_bytes, ctx.values.class)?;
    let remaining = ctx
        .limits
        .max_output_bytes
        .checked_sub(std::mem::size_of::<RollbackAuthorityRecord>())
        .ok_or(ValueTreeError::ResourceLimited("rollback output header"))?;
    let mut decoder = Decoder {
        root,
        schema: *blake3::hash(b"runtime.rollback.authority.v1").as_bytes(),
        ctx,
        remaining,
    };
    let prefix = &[];
    let authority_id = decoder.text(prefix, "authority_id")?;
    let role = match decoder.u64(prefix, "role")? {
        0 => RollbackAuthorityRole::OnCall,
        1 => RollbackAuthorityRole::Governance,
        _ => return Err(invalid()),
    };
    let public_key_hex = decoder.text(prefix, "public_key_hex")?;
    let active = decoder.boolean(prefix, "active")?;
    let authority = RollbackAuthorityRecord {
        authority_id,
        role,
        public_key_hex,
        active,
    };
    crate::incremental_rollback_qualification::qualify_rollback_record_v1(
        root,
        crate::incremental_rollback_genesis::RollbackRecordRefV1::Authority(&authority),
        decoder.ctx,
    )?;
    Ok(charge.retain(authority))
}

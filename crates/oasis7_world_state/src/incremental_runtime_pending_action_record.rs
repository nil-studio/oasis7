//! Exact typed projection of one immutable queued original input.
//! Data recovery alone grants no read authorization, original or World proof.
pub use crate::incremental_typed_record_codec::{
    TypedRecordReadWorkV1 as PendingActionReadWorkV1,
    TypedRecordWriteWorkV1 as PendingActionWriteWorkV1,
};
use crate::{
    ActionEnvelope, incremental_pending_action_codec as action,
    incremental_runtime_pending_actions::PENDING_ACTIONS_FIELD_V1,
    incremental_typed_record_codec::*,
};
use oasis7_formal_protocol::{
    incremental_artifacts::OriginalArtifactRefV1, incremental_record_root::RECORD_ROOT_BYTES,
};
use oasis7_proto::memory_budget::MemoryClass;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PendingActionPositionV1 {
    pub era: u64,
    pub ordinal: u64,
}
impl PendingActionPositionV1 {
    pub(crate) fn key(self) -> (u64, u64) {
        (self.era, self.ordinal)
    }
}
#[derive(Debug, PartialEq)]
pub struct PendingActionStoredRecordV1 {
    original_id: String,
    original_bytes: u64,
    envelope: ActionEnvelope,
}
dto!(PendingActionStoredRecordV1,{original_id,original_bytes,envelope});
impl PendingActionStoredRecordV1 {
    pub fn envelope(&self) -> &ActionEnvelope {
        &self.envelope
    }
    pub fn original(&self) -> Result<OriginalArtifactRefV1, ValueTreeError> {
        if self.original_id.len() != 64
            || self.original_bytes == 0
            || !self
                .original_id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid());
        }
        let id = *blake3::Hash::from_hex(&self.original_id)
            .map_err(|_| invalid())?
            .as_bytes();
        Ok(OriginalArtifactRefV1 {
            id,
            byte_len: self.original_bytes,
        })
    }
}
pub struct RecoveredPendingActionRecordV1 {
    record: RecoveredTypedRecordV1<PendingActionStoredRecordV1>,
    class: MemoryClass,
    admission: oasis7_proto::memory_budget::ReservedValue<()>,
}
impl RecoveredPendingActionRecordV1 {
    pub fn value(&self) -> &PendingActionStoredRecordV1 {
        self.record.value()
    }
    pub(crate) fn class(&self) -> MemoryClass {
        self.class
    }
    pub(crate) fn uses_budget(&self, memory: &oasis7_proto::memory_budget::MemoryBudget) -> bool {
        self.admission.uses_budget(memory)
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("pending action stored projection")
}

/// Borrow the source. The original callback must already have finished its
/// physical stream; independent family validation still binds actual originals.
pub(crate) fn prepare_pending_action_record_v1(
    position: PendingActionPositionV1,
    source: &ActionEnvelope,
    original: OriginalArtifactRefV1,
    ctx: &mut MemberRecordWriteContext<
        '_,
        impl StatePageWriter + ValueNodeReader,
        impl FnMut() -> bool,
    >,
    work: &mut PendingActionWriteWorkV1,
) -> Result<MemberRecordV1, MemberRecordError> {
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if original.byte_len == 0 {
        return Err(invalid().into());
    }
    let _phase = ctx
        .memory
        .reserve(128 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let profile = action::profile();
    let mut body = new_tree(&profile, ctx, work)?;
    let text = blake3::Hash::from_bytes(original.id).to_hex();
    put(
        &profile,
        &mut body,
        "original_id",
        Encoded::Scalar(MemberPayloadRefV1::Text(text.as_str())),
        ctx,
        work,
    )?;
    put(
        &profile,
        &mut body,
        "original_bytes",
        Encoded::Scalar(MemberPayloadRefV1::U64(original.byte_len)),
        ctx,
        work,
    )?;
    let envelope = source.encode(&profile, ctx, work)?;
    put(&profile, &mut body, "envelope", envelope, ctx, work)?;
    let body = body.into_reserved();
    let key = position.key();
    let owner = hash_semantic_key_v1(
        &key,
        ctx.memory,
        MemoryClass::Ordinary,
        ctx.key_work,
        &mut ctx.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    let _reference = ctx
        .memory
        .reserve(512, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let reference = RecordRootRefV1::new(action::schema(), owner.digest, (*body).clone())
        .map_err(|_| invalid())?;
    work.spend(&mut ctx.cancelled)?;
    write_member_record_v1(
        PENDING_ACTIONS_FIELD_V1,
        WorldFieldRecordKindV1::Entry,
        &key,
        MemberPayloadRefV1::RecordRoot(&reference),
        ctx,
    )
}
fn check<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    ctx.check()?;
    if let Some(error) = (ctx.take_state_error)() {
        return Err(error);
    }
    Ok(())
}
/// Read one typed record under the caller's authenticated Scheduler snapshot.
/// No CBOR DTO or complete queue is deserialized or collected here.
pub fn recover_pending_action_record_v1(
    position: PendingActionPositionV1,
    ctx: &mut WorldFieldReadContextV1<
        '_,
        impl MerkleNodeReader,
        impl ValueNodeReader,
        impl FnMut() -> Option<ValueTreeError>,
        impl FnMut() -> bool,
    >,
    scan: &mut StateScanBudget,
    work: &mut PendingActionReadWorkV1,
) -> Result<Option<RecoveredPendingActionRecordV1>, ValueTreeError> {
    let result = (|| {
        check(ctx)?;
        if ctx.root.namespace != StateNamespaceV1::Scheduler {
            return Err(invalid());
        }
        let _phase = ctx.values.memory.reserve(8 << 10, ctx.values.class)?;
        let meta = WorldFieldKeyV1::new(
            PENDING_ACTIONS_FIELD_V1,
            WorldFieldRecordKindV1::Metadata,
            None,
        )
        .map_err(ValueTreeError::Invalid)?;
        let id = ctx.lookup(&meta)?.ok_or_else(invalid)?;
        ctx.records().presence(&id, &meta, 0)?;
        let (key, _) = ctx.key(
            PENDING_ACTIONS_FIELD_V1,
            WorldFieldRecordKindV1::Entry,
            &position.key(),
        )?;
        let Some(id) = ctx.lookup(&key)? else {
            return Ok(None);
        };
        recover_selected_value(position, id, &key, ctx, scan, work).map(Some)
    })();
    check(ctx)?;
    result
}

#[cfg(test)]
#[path = "incremental_runtime_pending_action_record_tests.rs"]
pub(crate) mod tests;

// Shared closed decoder. The caller must obtain the reference from a genuine
// authenticated lookup; this helper is deliberately private.
fn recover_selected_value(
    _position: PendingActionPositionV1,
    id: [u8; 32],
    key: &WorldFieldKeyV1,
    ctx: &mut WorldFieldReadContextV1<
        '_,
        impl MerkleNodeReader,
        impl ValueNodeReader,
        impl FnMut() -> Option<ValueTreeError>,
        impl FnMut() -> bool,
    >,
    scan: &mut StateScanBudget,
    work: &mut PendingActionReadWorkV1,
) -> Result<RecoveredPendingActionRecordV1, ValueTreeError> {
    ctx.values
        .verify_closure(&id, ctx.record_bytes, &mut ctx.cancelled)?;
    let mut bytes = [0; RECORD_ROOT_BYTES];
    let info = read_member_record_v1(
        &id,
        key,
        &mut ctx.values,
        MemberRecordLimitsV1 {
            semantic_bytes: ctx.semantic_bytes,
            payload_bytes: RECORD_ROOT_BYTES as u64,
            record_bytes: ctx.record_bytes,
        },
        &mut std::io::sink(),
        &mut &mut bytes[..],
        &mut ctx.cancelled,
    )?;
    if info.codec != MemberPayloadCodecV1::RecordRoot
        || info.payload_bytes != RECORD_ROOT_BYTES as u64
    {
        return Err(invalid());
    }
    let _reference = ctx.values.memory.reserve(512, ctx.values.class)?;
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
    let owner = WorldFieldKeyV1::from_bytes(key.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or_else(invalid)?;
    let body = reference
        .validate_binding(&action::schema(), &owner, StateNamespaceV1::Scheduler)
        .map_err(|_| invalid())?;
    let admission = ctx.values.memory.reserve(256, ctx.values.class)?;
    let value = recover_typed_record_body_v1::<PendingActionStoredRecordV1, _, _, _, _>(
        body,
        &action::profile(),
        ctx,
        scan,
        work,
    )?;
    value.value().original()?;
    check(ctx)?;
    Ok(RecoveredPendingActionRecordV1 {
        record: value,
        class: ctx.values.class,
        admission: admission.retain(()),
    })
}

#[path = "incremental_runtime_pending_action_selected.rs"]
mod selected;
pub use selected::recover_current_pending_action_record_v1;

//! Same-transaction exact-member CAS after the real Core result. The old public
//! strict Current guard remains unchanged; no stale whole-body authorization.
use super::*;
use crate::incremental_field_member::{MemberPayloadRefV1, MemberRecordWriteContext};
use crate::incremental_record_columns_read::ColumnCellV1;
use crate::incremental_semantic_key::{SemanticKeyV1, hash_semantic_key_v1, write_semantic_key_v1};
use crate::incremental_typed_record_diff::{ChangeDecoder, point_typed_column_v1};
use oasis7_formal_protocol::incremental_record_root::RecordRootRefV1;
use oasis7_formal_protocol::incremental_state::StatePageWriter;
use oasis7_formal_protocol::incremental_value::ValueNodeWriter;
use std::io::{self, Write};
struct KeyCounter {
    bytes: usize,
}
impl Write for KeyCounter {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(b.len())
            .ok_or_else(|| io::Error::other("SafeAlternative key length"))?;
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn key_bytes<K: SemanticKeyV1 + ?Sized>(
    key: &K,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
    let mut count = KeyCounter { bytes: 0 };
    write_semantic_key_v1(key, &mut count).map_err(|_| invalid())?;
    let fee = memory.reserve(
        count
            .bytes
            .checked_add(std::mem::size_of::<Vec<u8>>())
            .ok_or_else(invalid)?,
        MemoryClass::Ordinary,
    )?;
    let mut out = Vec::with_capacity(count.bytes);
    if out.capacity() > count.bytes {
        return Err(invalid());
    }
    write_semantic_key_v1(key, &mut out).map_err(|_| invalid())?;
    if out.len() != count.bytes || out.capacity() > count.bytes {
        return Err(invalid());
    }
    Ok(fee.retain(out))
}
/// Data receipt for the exact `safe_alternatives[lineage]` write performed
/// by this module. It is not an execution or durable-acceptance permit.
pub struct SafeAlternativeRecordWriteReceiptV1 {
    before: oasis7_formal_protocol::incremental_state::StateRootV1,
    after: oasis7_formal_protocol::incremental_state::StateRootV1,
    lineage: String,
    guard: PreparedGuard,
}
impl SafeAlternativeRecordWriteReceiptV1 {
    pub fn before_body(&self) -> &oasis7_formal_protocol::incremental_state::StateRootV1 {
        &self.before
    }
    pub fn after_body(&self) -> &oasis7_formal_protocol::incremental_state::StateRootV1 {
        &self.after
    }
    pub fn lineage(&self) -> &str {
        &self.lineage
    }
    pub fn validate_transaction<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_transaction_identity(&self.guard)?;
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub fn replace_current_safe_alternative_record_v1<M, R, E, C, P, V, ER, CR, W, D>(
    expected: &ReservedValue<CurrentSafeAlternativeRecordV1>,
    after: &serde_json::Value,
    qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
    current: &mut RuntimeRevocationCurrentV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
    write: &mut MemberRecordWriteContext<'_, W, D>,
    write_work: &mut RevocationWriteWorkV1,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    D: FnMut() -> bool,
{
    replace_current_safe_alternative_impl(
        expected,
        after,
        qualification,
        current,
        overlay,
        read,
        scan,
        work,
        write,
        write_work,
        live,
        false,
    )
    .map(|(changed, _)| changed)
}
#[allow(clippy::too_many_arguments)]
pub fn replace_current_safe_alternative_record_with_receipt_v1<M, R, E, C, P, V, ER, CR, W, D>(
    expected: &ReservedValue<CurrentSafeAlternativeRecordV1>,
    after: &serde_json::Value,
    qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
    current: &mut RuntimeRevocationCurrentV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
    write: &mut MemberRecordWriteContext<'_, W, D>,
    write_work: &mut RevocationWriteWorkV1,
    live: &ExecutionFence,
) -> Result<(bool, ReservedValue<SafeAlternativeRecordWriteReceiptV1>), WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    D: FnMut() -> bool,
{
    let (changed, receipt) = replace_current_safe_alternative_impl(
        expected,
        after,
        qualification,
        current,
        overlay,
        read,
        scan,
        work,
        write,
        write_work,
        live,
        true,
    )?;
    Ok((changed, receipt.ok_or_else(invalid)?))
}
#[allow(clippy::too_many_arguments)]
fn replace_current_safe_alternative_impl<M, R, E, C, P, V, ER, CR, W, D>(
    expected: &ReservedValue<CurrentSafeAlternativeRecordV1>,
    after: &serde_json::Value,
    qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
    current: &mut RuntimeRevocationCurrentV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
    write: &mut MemberRecordWriteContext<'_, W, D>,
    write_work: &mut RevocationWriteWorkV1,
    live: &ExecutionFence,
    retain_receipt: bool,
) -> Result<
    (
        bool,
        Option<ReservedValue<SafeAlternativeRecordWriteReceiptV1>>,
    ),
    WorldFieldOverlayErrorV1,
>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    D: FnMut() -> bool,
{
    let result = (|| {
        if !expected.uses_budget(write.memory) || !overlay.values.memory.same_pool(write.memory) {
            return Err(invalid().into());
        }
        overlay
            .transaction
            .validate_transaction_identity(&expected.guard)?;
        let action = expected.record()["authorization"]["action_id"]
            .as_u64()
            .ok_or_else(invalid)?;
        if after["authorization"]["action_id"].as_u64() != Some(action) {
            return Err(invalid().into());
        }
        let guard = overlay.transaction.guard();
        let actual = read_current_safe_alternative_for_action_v1(
            action,
            qualification,
            current,
            overlay,
            read,
            scan,
            work,
            &guard,
            live,
        )?
        .ok_or_else(invalid)?;
        if actual.lineage != expected.lineage || actual.member_root != expected.member_root {
            return Err(invalid().into());
        }
        let profile = super::super::profile();
        let map = {
            let mut d = ChangeDecoder {
                trees_remaining: work.max_retained_trees,
                tree_depth: 0,
                profile,
                ctx: read,
                scan,
                work,
            };
            point_typed_column_v1(&profile, current.body(), "safe_alternatives", &mut d, 8192)?
        };
        let ColumnCellV1::Root(map) = &map.cell else {
            return Err(invalid().into());
        };
        let old = {
            let mut records = read.records();
            let (key, _) = records.key(
                profile.field,
                WorldFieldRecordKindV1::Entry,
                &("entry", expected.lineage.as_str()),
            )?;
            crate::incremental_record_tree::lookup(map, &key, &mut records, profile.labels)?
                .ok_or_else(invalid)?
        };
        let child = crate::incremental_typed_record_codec::prepare_typed_record_body_v1(
            after, &profile, write, write_work,
        )?;
        let owner = hash_semantic_key_v1(
            &("entry", expected.lineage.as_str()),
            write.memory,
            MemoryClass::Ordinary,
            write.key_work,
            &mut write.cancelled,
        )
        .map_err(MemberRecordError::Key)?;
        let reference = RecordRootRefV1::new(profile.schema, owner.digest, (*child).clone())
            .map_err(|_| invalid())?;
        let first = key_bytes("safe_alternatives", write.memory)?;
        let second = key_bytes(&("entry", expected.lineage.as_str()), write.memory)?;
        let path = [first.as_slice(), second.as_slice()];
        let edit = RuntimeRevocationPathEditV1 {
            path: &path,
            expected: Some(old),
            after: Some(MemberPayloadRefV1::RecordRoot(&reference)),
        };
        let guard = overlay.transaction.guard();
        // Only this fixed-path producer can mint the receipt. Legacy callers
        // retain their existing allocation and admission behavior.
        let receipt_before = if retain_receipt {
            let bytes = std::mem::size_of::<SafeAlternativeRecordWriteReceiptV1>()
                .checked_add(expected.lineage.len())
                .and_then(|n| n.checked_add(current.body().node_root.len()))
                .and_then(|n| n.checked_add(64))
                .ok_or_else(invalid)?;
            let fee = write
                .memory
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let lineage = expected.lineage.clone();
            let before = current.body().clone();
            if lineage.capacity() > expected.lineage.len()
                || before.node_root.capacity() > current.body().node_root.len()
            {
                return Err(invalid().into());
            }
            Some((fee, lineage, before))
        } else {
            None
        };
        let changed = patch_runtime_revocation_overlay_v1(
            qualification,
            current,
            &[edit],
            overlay,
            read,
            scan,
            work,
            write,
            write_work,
            &guard,
            live,
        )?;
        let receipt = if let Some((fee, lineage, before)) = receipt_before {
            if current.body().node_root.len() > 64 {
                return Err(invalid().into());
            }
            let after = current.body().clone();
            if after.node_root.capacity() > 64 {
                return Err(invalid().into());
            }
            Some(fee.retain(SafeAlternativeRecordWriteReceiptV1 {
                before,
                after,
                lineage,
                guard: overlay.transaction.guard(),
            }))
        } else {
            None
        };
        Ok((changed, receipt))
    })();
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            overlay.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}

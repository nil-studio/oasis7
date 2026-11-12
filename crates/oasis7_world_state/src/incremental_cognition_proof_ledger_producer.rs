//! Fixed side producers. GEN borrows original fields; DELTA edits only actual paths.
use super::*;
use crate::incremental_cognition_fields::CognitionEnvelopeRefV1;
use crate::incremental_json_records::{
    JsonPathSegmentV1, JsonPointWriteV1, JsonSubtreeEditV1, JsonWriteWorkV1,
    prepare_json_subtree_change_v1,
};
/// True GEN projection of the five independent original side owners.
pub fn prepare_cognition_proof_ledger_genesis_v1<W, C>(
    envelope: CognitionEnvelopeRefV1<'_>,
    roots: [u8; 32],
    write: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut ProofLedgerWriteWorkV1,
) -> Result<ReservedValue<CognitionProofLedgerManifestV1>, MemberRecordError>
where
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
{
    let fee = write
        .memory
        .reserve(
            std::mem::size_of::<CognitionProofLedgerManifestV1>() + 5 * 512,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut fields = std::array::from_fn(|_| None);
    for field in CognitionProofLedgerFieldV1::ALL {
        match envelope.field(field.name()) {
            Some(value) => {
                let root = prepare_cognition_proof_ledger_field_v1(field, value, write, work)?;
                fields[field.index()] = Some((*root).clone());
            }
            None if field == CognitionProofLedgerFieldV1::RuntimeBinding => {}
            None => return Err(invalid().into()),
        }
    }
    if (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(fee.retain(CognitionProofLedgerManifestV1 {
        roots,
        previous_record: None,
        fields,
    }))
}
/// Fixed-field bounded path edits consume the accepted parent's real JSON Q.
/// Results remain provisional: a genuine successor Result must bind and accept
/// the resulting independent manifest before any source can be consumed.
pub fn prepare_cognition_proof_ledger_edit_v1<M, R, E, C, W, D>(
    parent: &ReservedValue<AcceptedCognitionProofLedgerV1>,
    field: CognitionProofLedgerFieldV1,
    path: &[JsonPathSegmentV1<'_>],
    edit: JsonSubtreeEditV1<'_>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, D>,
    work: &mut JsonWriteWorkV1,
) -> Result<ReservedValue<JsonPointWriteV1>, MemberRecordError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    D: FnMut() -> bool,
{
    if !parent.uses_budget(write.memory) || !read.values.memory.same_pool(write.memory) {
        return Err(invalid().into());
    }
    parent.check_pool(write.memory, MemoryClass::Ordinary)?;
    let q = parent.fields[field.index()].as_ref().ok_or_else(invalid)?;
    let p = profile(field);
    let mut field_read = WorldFieldReadContextV1 {
        root: q.root(),
        state: read.state,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: read.values.source,
            memory: read.values.memory,
            class: read.values.class,
            io: &mut *read.values.io,
        },
        key_work: &mut *read.key_work,
        semantic_bytes: read.semantic_bytes,
        record_bytes: read.record_bytes,
        take_state_error: &mut read.take_state_error,
        cancelled: &mut read.cancelled,
    };
    prepare_json_subtree_change_v1(
        JsonRecordProfileV1 {
            field: p.field,
            schema: p.schema,
        },
        q,
        path,
        edit,
        &mut field_read,
        write,
        work,
    )
}

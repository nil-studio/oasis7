//! Candidate changed closure and prepaid refinement under genuine acceptance.
use super::*;
use crate::incremental_json_records::{
    JsonChangeWorkV1, advance_json_subtree_v1, copy_json_subtree_metadata_ordinary_v1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::VerifiedFormalCommitV1,
    incremental_execution::ExecutionFactKindV1,
    incremental_material::{
        CandidateFactRequestV1, CheckedCandidateFactOccurrenceV1, CoreError,
        PreparedFactPromotionV1, check_candidate_fact_v1, finish_prepared_fact_promotion_v1,
        prepare_candidate_fact_promotion_v1,
    },
};
#[derive(Debug)]
pub enum ProofLedgerCandidateErrorV1 {
    Value(ValueTreeError),
    Formal(CoreError<std::convert::Infallible>),
}
impl From<ValueTreeError> for ProofLedgerCandidateErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
pub struct CheckedCognitionProofLedgerV1 {
    manifest: ReservedValue<CognitionProofLedgerManifestV1>,
    occurrence: ReservedValue<CheckedCandidateFactOccurrenceV1>,
    fields: [Option<ReservedValue<JsonSubtreeQualificationV1>>; 5],
}
pub struct PreparedCognitionProofLedgerPromotionV1 {
    manifest: ReservedValue<CognitionProofLedgerManifestV1>,
    occurrence: PreparedFactPromotionV1,
    fields: [Option<ReservedValue<JsonSubtreeQualificationV1>>; 5],
    charge: oasis7_proto::memory_budget::MemoryReservation,
}
pub fn check_candidate_cognition_proof_ledger_v1<M, R, E, C>(
    request: &CandidateFactRequestV1<'_>,
    routing: &ReservedValue<crate::incremental_world_result_router::CheckedWorldResultRoutingV1>,
    raw: &[u8],
    parent: Option<&ReservedValue<AcceptedCognitionProofLedgerV1>>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    cold: &mut JsonReadWorkV1,
    changed: &mut JsonChangeWorkV1,
) -> Result<ReservedValue<CheckedCognitionProofLedgerV1>, ProofLedgerCandidateErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    read.check()?;
    let memory = read.values.memory;
    if read.values.class != MemoryClass::Ordinary || request.kind != ExecutionFactKindV1::Result {
        return Err(invalid().into());
    }
    let routed = routing.proof_ledger().ok_or_else(invalid)?;
    let selected = routed.occurrence();
    if !routing.uses_budget(memory)
        || !selected.uses_budget(memory)
        || selected.kind() != ExecutionFactKindV1::Result
        || selected.candidate_record_id() != request.core.candidate_record_id()
        || selected.body_id() != request.originals.body_id()
        || selected.roots_id() != request.core.body().roots
        || selected.location().ordinal != request.ordinal
    {
        return Err(invalid().into());
    }
    let charge = memory
        .reserve(
            std::mem::size_of::<CheckedCognitionProofLedgerV1>() + 4096,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let manifest_charge = memory
        .reserve(
            std::mem::size_of::<CognitionProofLedgerManifestV1>() + 5 * 512,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let occurrence = check_candidate_fact_v1(request, memory, &mut read.cancelled)
        .map_err(ProofLedgerCandidateErrorV1::Formal)?;
    if occurrence.location() != selected.location()
        || occurrence.body_id() != selected.body_id()
        || occurrence.candidate_record_id() != selected.candidate_record_id()
        || raw.len() != COGNITION_PROOF_LEDGER_BYTES_V1
        || occurrence.original_bytes() != raw.len() as u64
        || blake3::hash(raw).as_bytes() != &occurrence.location().original
    {
        return Err(invalid().into());
    }
    let manifest = CognitionProofLedgerManifestV1::from_canonical_bytes(raw)?;
    if manifest.roots != request.core.body().roots
        || manifest.previous_record != request.core.parent_record_id()
    {
        return Err(invalid().into());
    }
    let mut fields = std::array::from_fn(|_| None);
    match parent {
        None => {
            if request.core.parent_record_id().is_some() {
                return Err(invalid().into());
            }
            fields = qualify_fields(&manifest, read, cold)?;
        }
        Some(parent) => {
            if !parent.uses_budget(memory) {
                return Err(invalid().into());
            }
            parent.check_pool(memory, MemoryClass::Ordinary)?;
            if request.core.parent_record_id() != Some(parent.occurrence.head_record_id())
                || request.core.body().parent_commit != Some(parent.occurrence.head_body_id())
                || request
                    .core
                    .parent_body()
                    .is_none_or(|b| b.roots != parent.manifest.roots)
            {
                return Err(invalid().into());
            }
            for field in CognitionProofLedgerFieldV1::ALL {
                let i = field.index();
                let p = profile(field);
                let jp = JsonRecordProfileV1 {
                    field: p.field,
                    schema: p.schema,
                };
                let Some(root) = manifest.fields[i].as_ref() else {
                    fields[i] = None;
                    continue;
                };
                let mut field_read = WorldFieldReadContextV1 {
                    root,
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
                fields[i] = match (&parent.fields[i], &manifest.fields[i]) {
                    (Some(old), Some(after)) if old.root() == after => {
                        Some(copy_json_subtree_metadata_ordinary_v1(
                            old,
                            jp,
                            memory,
                            &mut field_read.cancelled,
                        )?)
                    }
                    (Some(old), Some(after)) => Some(advance_json_subtree_v1(
                        jp,
                        old,
                        after,
                        &mut field_read,
                        changed,
                    )?),
                    (None, Some(after)) => {
                        Some(qualify_json_subtree_v1(jp, after, &mut field_read, cold)?)
                    }
                    (_, None) => None,
                };
                if i < 3 {
                    if let Some(q) = &fields[i] {
                        crate::incremental_json_records::require_json_container_records_v1(
                            jp,
                            q,
                            true,
                            &mut field_read.records(),
                            cold,
                        )?;
                    }
                }
            }
        }
    }
    read.check()?;
    Ok(charge.retain(CheckedCognitionProofLedgerV1 {
        manifest: manifest_charge.retain(manifest),
        occurrence,
        fields,
    }))
}
pub fn prepare_cognition_proof_ledger_promotion_v1(
    checked: &ReservedValue<CheckedCognitionProofLedgerV1>,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<PreparedCognitionProofLedgerPromotionV1, ProofLedgerCandidateErrorV1> {
    if !checked.uses_budget(memory)
        || !checked.manifest.uses_budget(memory)
        || checked
            .fields
            .iter()
            .flatten()
            .any(|q| !q.uses_budget(memory))
    {
        return Err(invalid().into());
    }
    let charge = memory
        .reserve(
            std::mem::size_of::<AcceptedCognitionProofLedgerV1>() + 4096,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let occurrence =
        prepare_candidate_fact_promotion_v1(&checked.occurrence, memory, &mut cancelled)
            .map_err(ProofLedgerCandidateErrorV1::Formal)?;
    let fee = memory
        .reserve(
            std::mem::size_of::<CognitionProofLedgerManifestV1>() + 5 * 512,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut fields = std::array::from_fn(|_| None);
    for field in CognitionProofLedgerFieldV1::ALL {
        if let Some(q) = &checked.fields[field.index()] {
            let p = profile(field);
            fields[field.index()] = Some(copy_json_subtree_metadata_ordinary_v1(
                q,
                JsonRecordProfileV1 {
                    field: p.field,
                    schema: p.schema,
                },
                memory,
                &mut cancelled,
            )?);
        }
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(PreparedCognitionProofLedgerPromotionV1 {
        manifest: fee.retain((*checked.manifest).clone()),
        fields,
        occurrence,
        charge,
    })
}
pub fn finish_prepared_cognition_proof_ledger_promotion_v1(
    prepared: PreparedCognitionProofLedgerPromotionV1,
    accepted: &VerifiedFormalCommitV1,
) -> Result<ReservedValue<AcceptedCognitionProofLedgerV1>, ProofLedgerCandidateErrorV1> {
    let occurrence = finish_prepared_fact_promotion_v1(prepared.occurrence, accepted)
        .map_err(ProofLedgerCandidateErrorV1::Formal)?;
    Ok(prepared.charge.retain(AcceptedCognitionProofLedgerV1 {
        manifest: prepared.manifest,
        occurrence,
        fields: prepared.fields,
    }))
}

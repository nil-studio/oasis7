//! Changed ledger qualifications inherit the actual preceding suffix owners.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::VerifiedCandidateContextV1,
    incremental_execution::ExecutionFactKindV1,
    incremental_material::{
        CandidateFactRequestV1, CheckedCommitCore, CheckedFactOriginals, OriginalObjectStreamReader,
    },
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_cognition_proof_ledger as proof,
    incremental_cognition_retention_ledger as retention,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_result_router::CheckedWorldResultRoutingV1,
};

pub(super) struct LedgerRefs<'a> {
    pub(super) candidate: &'a VerifiedCandidateContextV1,
    pub(super) head: &'a VerifiedFormalCommitV1,
    pub(super) core: &'a ReservedValue<CheckedCommitCore>,
    pub(super) originals: &'a ReservedValue<CheckedFactOriginals>,
    pub(super) routed: &'a ReservedValue<CheckedWorldResultRoutingV1>,
    pub(super) parent: &'a super::cold_readmission_paid::OrdinaryColdSourcesV1,
}
pub(super) struct Ledgers {
    pub(super) proof: Option<ReservedValue<proof::AcceptedCognitionProofLedgerV1>>,
    pub(super) retention: Option<ReservedValue<retention::AcceptedCognitionRetentionLedgerV1>>,
}

fn metadata(
    work: &mut super::work::IwsCandidateSourceWorkV1,
    amount: u64,
) -> Result<(), FormalError> {
    work.proof_ledger_metadata_steps = work
        .proof_ledger_metadata_steps
        .checked_sub(amount)
        .ok_or_else(|| invalid("suffix ledger metadata steps"))?;
    work.proof_ledger_metadata_bytes = work
        .proof_ledger_metadata_bytes
        .checked_sub(amount)
        .ok_or_else(|| invalid("suffix ledger metadata bytes"))?;
    Ok(())
}

pub(super) fn read_ledgers<O, M, R, E, C>(
    refs: LedgerRefs<'_>,
    objects: &mut O,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut super::work::IwsCandidateSourceWorkV1,
) -> Result<Ledgers, FormalError>
where
    O: OriginalObjectStreamReader,
    O::Error: std::fmt::Debug,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let memory = read.values.memory;
    let proof = if let Some(row) = refs.routed.proof_ledger() {
        let occurrence = row.occurrence();
        let expected = usize::try_from(occurrence.original_bytes())
            .map_err(|_| invalid("suffix ProofLedger original size"))?;
        // The original candidate consumer's fixed capsule/clone bound is
        // retained; changed subtrees have their own continuing scalar lanes.
        let bound = expected.checked_mul(16)
            .and_then(|n| n.checked_add(8 * (
                std::mem::size_of::<oasis7_world_state::incremental_json_records::JsonSubtreeQualificationV1>()
                + 4 * oasis7_formal_protocol::incremental_state::STATE_ROOT_BYTES + 1024)))
            .and_then(|n| n.checked_add(8 * (
                std::mem::size_of::<oasis7_formal_protocol::incremental_material::CheckedCandidateFactOccurrenceV1>()
                + std::mem::size_of::<proof::CognitionProofLedgerManifestV1>()) + 4096))
            .and_then(|n| u64::try_from(n).ok())
            .ok_or_else(|| invalid("suffix ProofLedger metadata layout"))?;
        metadata(work, bound)?;
        read.check()
            .map_err(|e| invalid(format!("suffix ledger context: {e}")))?;
        work.router.facts.original_bytes_remaining = work
            .router
            .facts
            .original_bytes_remaining
            .checked_sub(occurrence.original_bytes())
            .ok_or_else(|| invalid("suffix ProofLedger original allowance"))?;
        let raw = objects
            .read_original(&occurrence.location().original, expected, memory)
            .map_err(|e| invalid(format!("suffix ProofLedger original: {e:?}")))?;
        if !raw.uses_budget(memory) || raw.len() != expected {
            return Err(invalid("suffix ProofLedger original binding"));
        }
        let checked = proof::check_candidate_cognition_proof_ledger_v1(
            &CandidateFactRequestV1 {
                kind: ExecutionFactKindV1::Result,
                candidate: refs.candidate,
                core: refs.core,
                originals: refs.originals,
                ordinal: occurrence.location().ordinal,
            },
            refs.routed,
            &raw,
            refs.parent.ledger.as_ref(),
            read,
            &mut work.proof_ledger_cold,
            &mut work.proof_ledger_changed,
        )
        .map_err(|e| invalid(format!("suffix ProofLedger transition: {e:?}")))?;
        let prepared = proof::prepare_cognition_proof_ledger_promotion_v1(
            &checked,
            memory,
            &mut read.cancelled,
        )
        .map_err(|e| invalid(format!("suffix ProofLedger admission: {e:?}")))?;
        Some(
            proof::finish_prepared_cognition_proof_ledger_promotion_v1(prepared, refs.head)
                .map_err(|e| invalid(format!("suffix ProofLedger accepted binding: {e:?}")))?,
        )
    } else {
        None
    };
    let retention = if let Some(id) = refs.core.retention_ledger_artifact_id() {
        metadata(work, 128 * 1024)?;
        read.check()
            .map_err(|e| invalid(format!("suffix retention context: {e}")))?;
        let bound =
            oasis7_formal_protocol::incremental_retention::RETENTION_LEDGER_MANIFEST_MAX_BYTES_V1;
        work.router.facts.original_bytes_remaining = work
            .router
            .facts
            .original_bytes_remaining
            .checked_sub(bound as u64)
            .ok_or_else(|| invalid("suffix retention original allowance"))?;
        let raw = objects
            .read_original(&id, bound, memory)
            .map_err(|e| invalid(format!("suffix retention original: {e:?}")))?;
        if !raw.uses_budget(memory) || raw.len() > bound {
            return Err(invalid("suffix retention original binding"));
        }
        let checked = retention::check_candidate_cognition_retention_ledger_v1(
            refs.core,
            &raw,
            refs.parent.retention.as_ref(),
            read,
            &mut work.proof_ledger_cold,
            &mut work.proof_ledger_changed,
        )
        .map_err(|e| invalid(format!("suffix retention transition: {e:?}")))?;
        let prepared = retention::prepare_cognition_retention_ledger_promotion_v1(
            &checked,
            memory,
            &mut read.cancelled,
        )
        .map_err(|e| invalid(format!("suffix retention admission: {e:?}")))?;
        Some(
            retention::finish_prepared_cognition_retention_ledger_promotion_v1(prepared, refs.head)
                .map_err(|e| invalid(format!("suffix retention accepted binding: {e:?}")))?,
        )
    } else {
        None
    };
    if read.values.class != MemoryClass::Ordinary {
        return Err(invalid("suffix ledger Ordinary context"));
    }
    Ok(Ledgers { proof, retention })
}

//! Resource-only Recovery to Ordinary copy of an already authenticated source.
//! No caller roots, reader reports or restored flags mint a ProofLedger.
use super::*;
use crate::incremental_json_records::copy_json_subtree_metadata_ordinary_v1;
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        FormalError, VerifiedFormalCommitV1, bounded_context::FormalBoundedContextV1,
    },
    incremental_execution::RootSetV1,
    incremental_material::{CoreError, readmit_fact_occurrence_ordinary_v1},
};
#[derive(Debug)]
pub enum ProofLedgerReadmissionErrorV1 {
    Value(ValueTreeError),
    Common(FormalError),
    Formal(CoreError<std::convert::Infallible>),
}
impl From<ValueTreeError> for ProofLedgerReadmissionErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
pub fn readmit_cognition_proof_ledger_ordinary_v1(
    source: &ReservedValue<AcceptedCognitionProofLedgerV1>,
    head: &VerifiedFormalCommitV1,
    roots: &RootSetV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<AcceptedCognitionProofLedgerV1>, ProofLedgerReadmissionErrorV1> {
    let measured = oasis7_formal_protocol::controlled_authority::formal::context_wire::encode_paid(
        head.record(),
        64 << 20,
        common,
        "ProofLedger head bound",
        "ProofLedger readmission head",
    )
    .map_err(ProofLedgerReadmissionErrorV1::Common)?;
    // The fixed Formal readmission performs two record validations plus one
    // canonical encoding/hash. RootSet canonical framing is bounded to 1024;
    // its two validations/digests and fixed metadata copies are prepaid too.
    let upper = measured
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(4 * 1024 + COGNITION_PROOF_LEDGER_BYTES_V1 + 5 * 512 + 4096))
        .ok_or_else(invalid)?;
    common
        .charge(upper as u64, upper as u64)
        .map_err(ProofLedgerReadmissionErrorV1::Common)?;
    let (memory, meter, stop) = common.parts();
    source.check_pool(memory, MemoryClass::Recovery)?;
    if !source.uses_budget(memory)
        || source.occurrence.head_record_id() != head.verified_record_id()
        || source.occurrence.fact_body_id() != source.occurrence.head_body_id()
    {
        return Err(invalid().into());
    }
    let _phase = memory
        .reserve(4096, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    roots.validate().map_err(|_| invalid())?;
    if roots.digest().map_err(|_| invalid())? != source.manifest.roots {
        return Err(invalid().into());
    }
    let fee = memory
        .reserve(
            std::mem::size_of::<AcceptedCognitionProofLedgerV1>() + 4096,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mf = memory
        .reserve(
            std::mem::size_of::<CognitionProofLedgerManifestV1>() + 5 * 512,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let child = meter
        .begin_child()
        .map_err(ProofLedgerReadmissionErrorV1::Common)?;
    let result = (|| {
        let occurrence = readmit_fact_occurrence_ordinary_v1(
            &source.occurrence,
            head,
            roots,
            memory,
            &mut *stop,
        )
        .map_err(ProofLedgerReadmissionErrorV1::Formal)?;
        let mut fields = std::array::from_fn(|_| None);
        for field in CognitionProofLedgerFieldV1::ALL {
            if let Some(q) = &source.fields[field.index()] {
                let p = profile(field);
                fields[field.index()] = Some(copy_json_subtree_metadata_ordinary_v1(
                    q,
                    JsonRecordProfileV1 {
                        field: p.field,
                        schema: p.schema,
                    },
                    memory,
                    &mut *stop,
                )?);
            }
        }
        if stop() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(fee.retain(AcceptedCognitionProofLedgerV1 {
            manifest: mf.retain((*source.manifest).clone()),
            occurrence,
            fields,
        }))
    })();
    child.finish();
    result
}

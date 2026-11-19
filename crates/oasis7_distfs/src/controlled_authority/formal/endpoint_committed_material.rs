//! Historical material promotion requires a current-canonical-root delegated
//! cut token. A provider floor or raw archived query cannot grant old anchor trust.
use super::*;
use crate::controlled_authority::replicated_protocol::ClosedRecord;
pub struct VerifiedCommittedHistoricalMaterialV1 {
    commit: VerifiedFormalCommitV1,
    authority: VerifiedAuthorityV1,
    proof: FormalQualifiedCommitProofV1,
    candidate: FormalCandidateMaterialV1,
    delegating_head: FormalHeadRefV1,
}
impl VerifiedCommittedHistoricalMaterialV1 {
    pub fn commit(&self) -> &VerifiedFormalCommitV1 {
        &self.commit
    }
    pub fn authority(&self) -> &VerifiedAuthorityV1 {
        &self.authority
    }
    pub fn proof(&self) -> &FormalQualifiedCommitProofV1 {
        &self.proof
    }
    pub fn candidate(&self) -> &FormalCandidateMaterialV1 {
        &self.candidate
    }
    pub fn material(&self) -> &ClosedRecord {
        &self.candidate.material
    }
    pub fn delegating_head(&self) -> &FormalHeadRefV1 {
        &self.delegating_head
    }
}
pub(crate) fn verify_committed_historical_material(
    cut: &VerifiedCommittedHistoricalCutV1,
    state: FormalRequestStateV1,
    reader: &impl super::super::business_index::BusinessIndexNodeReader,
) -> Result<VerifiedCommittedHistoricalMaterialV1, FormalError> {
    let FormalRequestStateV1::Qualified {
        candidate, proof, ..
    } = state
    else {
        return Err(invalid(
            "HistoryUnavailable: delegated old entry must be Qualified",
        ));
    };
    if proof != *cut.proof()
        || candidate.candidate.record != *cut.commit().record()
        || candidate.branch != proof.decision.branch
        || candidate.candidate.writer_signature_hex != proof.decision.writer_signature_hex
        || candidate.frozen != proof.decision.frozen
        || cut.authority().definition().profile_version != 2
    {
        return Err(invalid(
            "historical material exact delegated canonical/proof/context",
        ));
    }
    verify_record_material(cut.commit().record(), &candidate.material)?;
    match (&candidate.frozen, &candidate.frozen_material) {
        (Some(cert), Some(material)) => verify_frozen_material(&cert.reservation.body, material)?,
        (None, None) if cut.commit().record().kind == FormalRecordKindV1::InitialActivation => {}
        _ => return Err(invalid("historical material exact original Frozen closure")),
    }
    let result = &candidate.material.roots
        [&crate::controlled_authority::replicated_protocol::ArtifactRole::Result];
    let bytes = &candidate
        .material
        .objects
        .iter()
        .find(|object| &object.content_hash == result)
        .ok_or_else(|| invalid("historical Result envelope object"))?
        .bytes;
    let envelope = FormalBusinessResultEnvelopeV1::decode(bytes)?;
    if envelope.delta.new_root != cut.commit().record().result_root
        || envelope
            .delta
            .new
            .canonical_result_root()
            .map_err(invalid)?
            != cut.commit().record().result_root
    {
        return Err(invalid(
            "historical material business root exact canonical binding",
        ));
    }
    super::super::business_material::verify_committed_business_material(
        cut.commit().record(),
        &candidate.material,
        candidate.frozen_material.as_ref(),
        reader,
    )?;
    Ok(VerifiedCommittedHistoricalMaterialV1 {
        commit: cut.commit().clone(),
        authority: cut.authority().clone(),
        proof,
        candidate,
        delegating_head: cut.delegating_head().clone(),
    })
}

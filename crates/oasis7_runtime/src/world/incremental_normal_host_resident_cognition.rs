//! Warm Cognition owners come from this tick's captured deltas and the actual
//! checked candidate. No accepted-history scan or empty evidence constructor
//! is part of ordinary resident promotion.
use super::*;
use crate::world::incremental_normal_cognition_service_evidence::{
    AcceptedCognitionWakeEvidenceV2, FrozenCognitionWakeEvidenceV2,
    PreparedCognitionWakeEvidenceV2, finish_prepared_cognition_wake_evidence_v2,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::VerifiedFormalCommitV1, incremental_execution::RootSetV1,
};
use oasis7_world_state::{
    incremental_cognition_history::CheckedCognitionHistorySourceV3,
    incremental_cognition_proof_ledger::{
        AcceptedCognitionProofLedgerV1, CheckedCognitionProofLedgerV1,
        PreparedCognitionProofLedgerPromotionV1,
        finish_prepared_cognition_proof_ledger_promotion_v1,
        prepare_cognition_proof_ledger_promotion_v1,
    },
};

pub(super) struct PreparedNormalHostCognitionV1 {
    ledger: PreparedCognitionProofLedgerPromotionV1,
    evidence: PreparedCognitionWakeEvidenceV2,
}

pub(super) struct AcceptedNormalHostCognitionV1 {
    ledger: ReservedValue<AcceptedCognitionProofLedgerV1>,
    evidence: ReservedValue<AcceptedCognitionWakeEvidenceV2>,
}

fn prepare_ledger(
    checked: &ReservedValue<CheckedCognitionProofLedgerV1>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PreparedCognitionProofLedgerPromotionV1, BodyMutationError> {
    // The existing promotion copies only eight qualified metadata fields and
    // one occurrence. Prepay those fixed copies plus their actual root text;
    // it neither reads nor materializes any ledger row.
    let bytes = checked
        .manifest()
        .fields
        .iter()
        .flatten()
        .try_fold(8192u64, |bytes, root| {
            let copied = u64::try_from(root.node_root.len()).map_err(|_| bad())?;
            bytes
                .checked_add(copied.checked_mul(2).ok_or_else(bad)?)
                .ok_or_else(bad)
        })?;
    common.charge(32, bytes).map_err(formal)?;
    let (memory, _, cancelled) = common.parts();
    prepare_cognition_proof_ledger_promotion_v1(checked, memory, cancelled).map_err(original)
}

pub(super) fn prepare_primary(
    frozen: Option<&FrozenCognitionWakeEvidenceV2>,
    terminal: &crate::world::incremental_normal_host_finish::SealedNormalResidentSourceV1,
    roots: &RootSetV1,
    source: &ReservedValue<CheckedCognitionHistorySourceV3>,
    ledger: Option<&ReservedValue<CheckedCognitionProofLedgerV1>>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Option<PreparedNormalHostCognitionV1>, BodyMutationError> {
    common.charge(1, 128).map_err(formal)?;
    match (frozen, ledger) {
        (Some(frozen), Some(ledger)) => {
            let evidence = frozen.prepare_candidate(terminal, roots, source, ledger, common)?;
            let ledger = prepare_ledger(ledger, common)?;
            Ok(Some(PreparedNormalHostCognitionV1 { ledger, evidence }))
        }
        (None, None) if source.disabled().is_some() && source.indexes()[0].is_none() => Ok(None),
        _ => Err(bad()),
    }
}

pub(super) fn prepare_secondary(
    primary: Option<&PreparedNormalHostCognitionV1>,
    source: &ReservedValue<CheckedCognitionHistorySourceV3>,
    ledger: Option<&ReservedValue<CheckedCognitionProofLedgerV1>>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Option<PreparedNormalHostCognitionV1>, BodyMutationError> {
    common.charge(1, 128).map_err(formal)?;
    match (primary, ledger) {
        (Some(primary), Some(ledger)) => {
            let evidence = primary
                .evidence
                .prepare_secondary_candidate_v2(source, ledger, common)?;
            let ledger = prepare_ledger(ledger, common)?;
            Ok(Some(PreparedNormalHostCognitionV1 { ledger, evidence }))
        }
        (None, None) if source.disabled().is_some() && source.indexes()[0].is_none() => Ok(None),
        _ => Err(bad()),
    }
}

impl PreparedNormalHostCognitionV1 {
    pub(super) fn finish(
        self,
        accepted: &VerifiedFormalCommitV1,
    ) -> Result<AcceptedNormalHostCognitionV1, ExecutionError> {
        // Both existing finish functions only compare the actual record and
        // move prepaid owners; no IO, allocation or hashing follows acceptance.
        let evidence = finish_prepared_cognition_wake_evidence_v2(self.evidence, accepted)
            .map_err(|_| ExecutionError::Invalid("normal Cognition evidence accepted record"))?;
        let ledger = finish_prepared_cognition_proof_ledger_promotion_v1(self.ledger, accepted)
            .map_err(|_| ExecutionError::Invalid("normal Cognition ledger accepted record"))?;
        Ok(AcceptedNormalHostCognitionV1 { ledger, evidence })
    }
}

impl NormalHostResidentSourcesV1 {
    pub(in crate::world) fn warm_cognition_sources_v1<'a>(
        &'a self,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<
        crate::world::incremental_normal_host_driver::NormalHostCognitionParentSourcesV1<'a>,
        BodyMutationError,
    > {
        use crate::world::incremental_normal_host_driver::NormalHostCognitionParentSourcesV1;
        parent.validate_context(common).map_err(formal)?;
        self.validate_live_binding_v1(
            parent.refs().head.verified_record_id(),
            &parent.fence(),
            common,
        )?;
        let ResidentStorage::Warm { cognition, .. } = &self.storage else {
            return Err(BodyMutationError::Invalid(
                "Cognition resident requires accepted ordinary promotion",
            ));
        };
        match cognition {
            Some(value) => {
                cognition_bootstrap::validate_ledger(parent, &value.ledger, common)?;
                value
                    .evidence
                    .validate_parent(parent, &value.ledger, common)?;
                Ok(NormalHostCognitionParentSourcesV1::Present {
                    accepted: &value.ledger,
                    evidence: &value.evidence,
                })
            }
            None if parent.refs().storage.results().proof_ledger().is_none()
                && parent.refs().cognition.disabled().is_some()
                && parent.refs().cognition.indexes()[0].is_none() =>
            {
                Ok(NormalHostCognitionParentSourcesV1::Absent)
            }
            None => Err(bad()),
        }
    }
}

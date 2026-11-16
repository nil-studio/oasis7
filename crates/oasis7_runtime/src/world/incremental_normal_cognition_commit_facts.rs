//! Real first-request marker and original stored-action fallback. This probe
//! is DATA only; the response ANY join and original receipt lineage must still
//! be read and verified before any retry can return a committed disposition.
use super::*;
use crate::world::{
    incremental_module_invocation::*,
    incremental_normal_cognition_expiry::{
        CurrentCognitionExpiryWorkV1, with_current_cognition_io_v1,
    },
    incremental_normal_cognition_service_evidence::CurrentCognitionWakeEvidenceV2,
    incremental_normal_sources::BoundNormalTickParentV1,
};
use oasis7_formal_protocol::{
    incremental_material::OriginalObjectStreamReader,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter},
};
use oasis7_world_state::{
    incremental_cognition_json_read::CognitionJsonEntryQualificationV1,
    incremental_cognition_proof_ledger::CognitionProofLedgerFieldV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_json_records::JsonScalarPathReadWorkV1,
};
pub(in crate::world) struct CurrentCognitionRetryProbeV1<'a, 'e, 'p, 'i> {
    input: &'a CurrentCognitionCommitInputV1<'i>,
    evidence: &'a CurrentCognitionWakeEvidenceV2<'e, 'p>,
    marker: Option<ReservedValue<crate::cognition_recovery::WorldCommitRecordV1>>,
    stored: Option<Stored>,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    _fee: MemoryReservation,
}
enum Stored {
    Direct(ReservedValue<serde_json::Value>),
    Staged {
        value: ReservedValue<serde_json::Value>,
        digest: oasis7_agent_api::world_cognition::PaidCognitionDigestDataV1<MemoryReservation>,
    },
}
impl Stored {
    fn digest(&self) -> Option<&str> {
        match self {
            Self::Direct(v) => v.as_str(),
            Self::Staged { digest, .. } => Some(digest.as_str()),
        }
    }
}
impl CurrentCognitionRetryProbeV1<'_, '_, '_, '_> {
    pub(in crate::world) fn marker(
        &self,
    ) -> Option<&crate::cognition_recovery::WorldCommitRecordV1> {
        self.marker.as_deref()
    }
    pub(in crate::world) fn input(&self) -> &CurrentCognitionCommitInputV1<'_> {
        self.input
    }
    pub(super) fn ledger_source_data_v1(
        &self,
    ) -> &oasis7_world_state::incremental_cognition_proof_ledger::CurrentCognitionProofLedgerV1
    {
        self.evidence.ledger_for_retry_data_v1()
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        parent: &BoundNormalTickParentV1<'_>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.input.validate(parent, tx, common, live)?;
        self.evidence.validate(tx, common, live)?;
        tx.validate_exact_transaction_identity(&self.guard)?;
        tx.validate_read_source_guard(&self.guard, live)?;
        common.charge(3, 64).map_err(original)?;
        if self.core != common.work().identity()
            || !self._fee.uses_budget(common.memory())
            || self
                .marker
                .as_ref()
                .is_some_and(|v| !v.uses_budget(common.memory()))
        {
            return Err(bad("Current Cognition retry probe context"));
        }
        if let Some(stored) = &self.stored {
            let value = match stored {
                Stored::Direct(v) | Stored::Staged { value: v, .. } => v,
            };
            if !value.uses_budget(common.memory()) {
                return Err(bad("Current Cognition retry stored action pool"));
            }
        }
        Ok(())
    }
}
fn overlay(
    error: oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1,
) -> BodyMutationError {
    match error {
        oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Execution(e) => {
            e.into()
        }
        oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(e) => {
            e.into()
        }
        e => original(e),
    }
}
impl<M, R, E, C, W, F, O> RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    pub(in crate::world) fn read_current_cognition_retry_probe_v1<'a, 'e, 'p, 'i, P, V, PE, PC>(
        &mut self,
        input: &'a CurrentCognitionCommitInputV1<'i>,
        parent: &BoundNormalTickParentV1<'_>,
        evidence: &'a CurrentCognitionWakeEvidenceV2<'e, 'p>,
        staged_actions: Option<&ReservedValue<CognitionJsonEntryQualificationV1>>,
        read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
        work: &mut CurrentCognitionExpiryWorkV1,
        path: &mut JsonScalarPathReadWorkV1,
        canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<CurrentCognitionRetryProbeV1<'a, 'e, 'p, 'i>, CurrentCognitionCommitErrorV1>
    where
        P: MerkleNodeReader,
        V: ValueNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            input.validate(parent, self.ctx.transaction, common, &live())?;
            let marker = evidence.read_first_request_marker_v1(
                input.request(),
                self.ctx.transaction,
                read,
                work,
                path,
                common,
                &live(),
            )?;
            let mut stored = None;
            if let Some(marker) = &marker {
                let direct = evidence
                    .ledger_for_retry_data_v1()
                    .read_service_object_member_v3(
                        CognitionProofLedgerFieldV1::ActionDigests,
                        &marker.commit_id,
                        self.ctx.transaction,
                        read,
                        &mut work.ledger.json,
                        path,
                        work.max_selected_heap,
                        common,
                        &live(),
                    )?;
                if let Some(direct) = direct.filter(|value| value.is_string()) {
                    stored = Some(Stored::Direct(direct));
                } else {
                    let selected = with_current_cognition_io_v1(
                        self.ctx,
                        self.members,
                        work,
                        common,
                        |ctx, _, work| {
                            let guard = ctx.transaction.guard();
                            oasis7_world_state::incremental_cognition_json_overlay::read_current_staged_cognition_action_v1(staged_actions,&marker.commit_id,ctx,&mut work.read,work.max_selected_heap,&guard,&live()).map_err(overlay)
                        },
                    )?;
                    if let Some(value) = selected {
                        let common_cell = RefCell::new(&mut *common);
                        let source = work::Source::new(&common_cell, canonical);
                        let digest = source.result(
                            oasis7_agent_api::world_cognition::cognition_digest_with_work_v1(
                                "oasis7.cognition.action.v1",
                                &*value,
                                &source,
                            ),
                        )?;
                        stored = Some(Stored::Staged { value, digest });
                    }
                }
                let common_cell = RefCell::new(&mut *common);
                let source = work::Source::new(&common_cell, canonical);
                if let Err(code)=source.result(crate::world::cognition_persistence::validate_existing_retry_action_with_work_v1(marker,stored.as_ref().and_then(Stored::digest),input.action_digest(),&source))?{return Err(source.refusal(code)?)}
            }
            let n = size_of::<CurrentCognitionRetryProbeV1<'_, '_, '_, '_>>();
            common.charge(1, n as u64).map_err(original)?;
            let fee = common
                .memory()
                .reserve(n, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let output = CurrentCognitionRetryProbeV1 {
                input,
                evidence,
                marker,
                stored,
                guard: self.ctx.transaction.guard(),
                core: common.work().identity(),
                _fee: fee,
            };
            output.validate(parent, self.ctx.transaction, common, &live())?;
            Ok(output)
        }));
        match attempt {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                Err(error)
            }
            Err(panic) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                std::panic::resume_unwind(panic)
            }
        }
    }
}

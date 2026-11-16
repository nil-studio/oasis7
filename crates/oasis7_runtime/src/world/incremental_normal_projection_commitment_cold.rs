//! Opaque complete qualified cold sources. Only this module's fixed actual
//! capture can mint the seed; consuming it retains every source graph/fee.
use super::cold_sources::QualifiedNormalStateSourcesV1;
use super::*;
use crate::world::{
    incremental_normal_projection_runtime::ActualCurrentNormalProjectionAuxV1,
    incremental_normal_sources::BoundNormalTickParentV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction},
    incremental_state::MerkleNodeReader,
};
use oasis7_world_state::incremental_world_rule_projection_delta::QualifiedWorldRuleProjectionColumnsV1;
pub(in crate::world) struct QualifiedColdProjectionSourceFeesV1 {
    sources: QualifiedNormalStateSourcesV1,
    aux: ActualCurrentNormalProjectionAuxV1,
    record: [u8; 32],
    body: [u8; 32],
    roots: [u8; 32],
    memory: MemoryBudget,
    core: WorkIdentityV1,
    _fee: MemoryReservation,
}
impl QualifiedColdProjectionSourceFeesV1 {
    pub(in crate::world) fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common.consume_steps(1).map_err(paid)?;
        if !self._fee.uses_budget(common.memory())
            || !self.memory.same_pool(common.memory())
            || self.core != common.work().identity()
        {
            return Err(invalid("qualified cold retained source context"));
        }
        Ok(())
    }
    fn validate_v1<M: MerkleNodeReader>(
        &self,
        parent: &BoundNormalTickParentV1<'_>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.validate_context(common)?;
        parent.validate_context(common).map_err(paid)?;
        tx.validate_bootstrap_read_only_v1(live)?;
        if tx.parent_fence() != parent.fence() || !tx.uses_budget(common.memory()) {
            return Err(invalid("qualified cold actual source TX"));
        }
        let refs = parent.refs();
        common.charge(3, 96).map_err(paid)?;
        if self.record != refs.head.verified_record_id()
            || self.body != refs.runtime.occurrence().head_body_id()
            || self.roots != refs.core.body().roots
        {
            return Err(invalid("qualified cold actual accepted binding"));
        }
        self.sources.validate_v1(refs.roots, tx, common, live)?;
        self.aux.validate(parent, tx, common, live)
    }
}
pub(in crate::world) struct QualifiedColdProjectionSeedV1 {
    columns: NormalProjectionColumnSeedDataV1,
    sources: QualifiedColdProjectionSourceFeesV1,
}
impl QualifiedColdProjectionSeedV1 {
    pub(in crate::world) fn validate_v1<M: MerkleNodeReader>(
        &self,
        parent: &BoundNormalTickParentV1<'_>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.columns.validate_context(common)?;
        self.sources.validate_v1(parent, tx, common, live)
    }
    pub(in crate::world) fn into_parts_v1(
        self,
    ) -> (
        NormalProjectionColumnSeedDataV1,
        QualifiedColdProjectionSourceFeesV1,
    ) {
        (self.columns, self.sources)
    }
}
use crate::world::{
    incremental_authorization_root_current::FrozenNormalAuthorizationProjectionV1,
    incremental_module_capability_admission_rules::PaidCurrentBootstrapPolicyV1,
    incremental_module_invocation::*,
    incremental_normal_business_commitment::NormalTerminalReadMetadataV1,
};
use oasis7_formal_protocol::{
    incremental_material::OriginalObjectStreamReader,
    incremental_projection_sidecar::PaidVerifiedProjectionSidecarV1,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter},
};
use oasis7_world_state::{
    incremental_cognition_proof_ledger::AcceptedCognitionProofLedgerV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_revocation::RuntimeRevocationCurrentV1,
};
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
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn capture_qualified_cold_projection_seed_v1<
        P: MerkleNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    >(
        &mut self,
        parent: &BoundNormalTickParentV1<'_>,
        side: &PaidVerifiedProjectionSidecarV1,
        revocation: &RuntimeRevocationCurrentV1,
        accepted_ledger: Option<&ReservedValue<AcceptedCognitionProofLedgerV1>>,
        accepted_retention:&ReservedValue<oasis7_world_state::incremental_cognition_retention_ledger::AcceptedCognitionRetentionLedgerV1>,
        policy: &PaidCurrentBootstrapPolicyV1,
        authorization: &FrozenNormalAuthorizationProjectionV1,
        metadata: &NormalTerminalReadMetadataV1,
        read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        work: &mut NormalProjectionCurrentWorkV1<'_>,
        journal_work:&mut crate::world::incremental_normal_industry_due::delivery::ProductValidationDeliveryWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<QualifiedColdProjectionSeedV1, RootModuleExecutionErrorV1<O::Error>> {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parent.validate_context(common).map_err(paid)?;
            let refs = parent.refs();
            side.validate_context(refs.head, refs.core, common)
                .map_err(paid)?;
            self.ctx
                .transaction
                .validate_bootstrap_read_only_v1(&live())?;
            metadata.validate_provenance(self.ctx.transaction, common, &live())?;
            let sources = self
                .capture_qualified_projection_state_sources_v1(parent, read, work, common, live)?;
            let aux = self.prepare_qualified_normal_projection_aux_v1(
                parent,
                revocation,
                metadata,
                accepted_ledger,
                accepted_retention,
                policy,
                authorization,
                read,
                work,
                journal_work,
                common,
                live,
            )?;
            let columns = super::cold_assemble::assemble_qualified_projection_seed_v1(
                parent,
                &sources,
                &aux,
                side.retained_history_root_json(),
                self.ctx.transaction,
                work.max_json_output_bytes,
                common,
                &live(),
            )?;
            sources.validate_v1(refs.roots, self.ctx.transaction, common, &live())?;
            aux.validate(parent, self.ctx.transaction, common, &live())?;
            let n = std::mem::size_of::<QualifiedColdProjectionSeedV1>()
                .checked_add(std::mem::size_of::<QualifiedColdProjectionSourceFeesV1>())
                .ok_or_else(|| invalid("qualified complete cold source layout"))?;
            common.charge(1, n as u64).map_err(paid)?;
            let fee = common
                .memory()
                .reserve(n, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let keeper = QualifiedColdProjectionSourceFeesV1 {
                sources,
                aux,
                record: refs.head.verified_record_id(),
                body: refs.runtime.occurrence().head_body_id(),
                roots: refs.core.body().roots,
                memory: common.memory().clone(),
                core: common.work().identity(),
                _fee: fee,
            };
            let source = QualifiedColdProjectionSeedV1 {
                columns,
                sources: keeper,
            };
            source.validate_v1(parent, self.ctx.transaction, common, &live())?;
            Ok(source)
        }));
        match outcome {
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

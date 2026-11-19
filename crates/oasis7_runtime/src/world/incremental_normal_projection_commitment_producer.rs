//! Fixed normal projection capture. The cache is an actual selected parent;
//! every changed value comes from a closed typed reader before either hash.
use super::*;
use crate::world::PaidProjectionRetainedOriginalReaderV1;
use crate::world::{
    incremental_module_capability_admission_rules::PaidCurrentBootstrapPolicyV1,
    incremental_module_invocation::*,
    incremental_normal_business_commitment::NormalTerminalReadMetadataV1,
    incremental_normal_event_commitment::NormalEventCommitmentV1,
    incremental_normal_final_sources::NormalCognitionProjectionInputsV1,
    incremental_normal_sources::BoundNormalTickParentV1,
};
use oasis7_formal_protocol::{
    incremental_material::OriginalObjectStreamReader,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter},
};
use oasis7_world_state::incremental_field_read::WorldFieldReadContextV1;
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
    O: OriginalObjectStreamReader
        + PaidProjectionRetainedOriginalReaderV1<RetainedError = BodyMutationError>,
{
    pub(in crate::world) fn prepare_current_normal_projection_commitment_v1<
        P: MerkleNodeReader,
        V: ValueNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    >(
        &mut self,
        parent: &BoundNormalTickParentV1<'_>,
        cache: &ReservedValue<AcceptedNormalProjectionCacheV1>,
        revocation: &oasis7_world_state::incremental_runtime_revocation::RuntimeRevocationCurrentV1,
        metadata: &NormalTerminalReadMetadataV1,
        events: &NormalEventCommitmentV1,
        cognition: &NormalCognitionProjectionInputsV1<'_>,
        authorization:&crate::world::incremental_authorization_root_current::FrozenNormalAuthorizationProjectionV1,
        policy: &PaidCurrentBootstrapPolicyV1,
        process_source: &crate::world::incremental_normal_process_source::NormalProcessSourceV1,
        read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
        work: &mut NormalProjectionCurrentWorkV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<CurrentNormalProjectionCommitmentV1, RootModuleExecutionErrorV1<O::Error>> {
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parent.validate_context(common).map_err(paid)?;
            cache.validate_parent_v1(parent, common)?;
            metadata.validate_provenance(self.ctx.transaction, common, &live())?;
            if !cache.uses_budget(common.memory()) {
                return Err(BodyMutationError::Invalid("projection accepted outer pool").into());
            }
            let sources =
                self.capture_normal_projection_state_sources_v1(parent, read, work, common, live)?;
            // WorldRules owns its own real IO child and must not nest in source IO.
            let rules = self.capture_current_projection_world_rules_delta_v1(
                parent,
                read,
                work.world_rules,
                common,
                live,
            )?;
            let mut state = super::super::fold::fold_actual_normal_state_sources_v1(
                &cache.columns.state,
                &sources,
                self.ctx.transaction,
                common,
                &live(),
            )?;
            state.apply_world_rule_delta_data_v1(&rules, self.ctx.transaction, common, &live())?;
            let electricity = super::super::electricity::apply_delta_v1(
                &cache.columns.agent_electricity,
                &sources.agents,
                common,
            )?;
            let agent_intents = super::super::intents::apply_delta_v1(
                &cache.columns.agent_intents,
                &sources.agents,
                common,
            )?;
            let observation = super::super::observation::fold_observation_sources_v1(
                &cache.columns.observation_state,
                &sources,
                &state,
                common,
            )?;
            let observation = super::super::diplomacy_observation::fold_diplomacy_observation_v1(
                &observation,
                &state,
                &sources.diplomacy,
                self.ctx.transaction,
                work.max_json_output_bytes,
                common,
                &live(),
            )?;
            let observation = super::super::governance_observation::fold_governance_observation_v1(
                &observation,
                &state,
                &sources.governance,
                self.ctx.transaction,
                work.max_json_output_bytes,
                common,
                &live(),
            )?;
            let observation = super::super::market_observation::fold_market_observation_v1(
                &observation,
                &state,
                &sources.marketplace,
                self.ctx.transaction,
                work.max_json_output_bytes,
                common,
                &live(),
            )?;
            let observation = super::super::observation::fold_world_rules_observation_v1(
                &observation,
                &state,
                &rules,
                self.ctx.transaction,
                work.max_json_output_bytes,
                common,
                &live(),
            )?;
            let aux = self.prepare_current_normal_projection_aux_v1(
                parent,
                cache,
                revocation,
                metadata,
                events,
                cognition,
                authorization,
                policy,
                read,
                work,
                common,
                live,
            )?;
            aux.validate(parent, self.ctx.transaction, common, &live())?;
            let raw_state = super::super::raw_hash::hash_current_state_columns_data_v1(
                &state,
                metadata,
                policy,
                self.ctx.transaction,
                common,
                &live(),
            )?;
            let metadata_source=crate::world::incremental_normal_business_commitment::OriginalProjectionReadMetadataV1::Current(metadata);
            let binding = metadata_source.runtime_binding_for_state_hash_bounded_v1(
                raw_state.as_str(),
                common,
                work.canonical,
            )?;
            let finality =
                self.prepare_current_bootstrap_finality_v1(parent, work.finality, common, live)?;
            let process = process_source.capture_projection_facts_v1(parent, common)?;
            let projection_root = super::super::leaf::compute_current_projection_root_v1(
                &state,
                &electricity,
                &agent_intents,
                &observation,
                aux.columns_v1(),
                &cache.columns.template,
                &metadata_source,
                &process,
                authorization,
                &finality,
                &raw_state,
                binding.as_ref(),
                self.ctx.transaction,
                &mut *self.originals,
                common,
                work.canonical,
                live,
            )?;
            aux.validate(parent, self.ctx.transaction, common, &live())?;
            metadata.validate_provenance(self.ctx.transaction, common, &live())?;
            let template = cache.columns.template.share_data_v1(common)?;
            let memory = common.memory().clone();
            let auxiliary = aux
                .columns_v1()
                .share_data(&memory, &mut |s, b| common.charge(s, b))
                .map_err(projection_data_error)?;
            let n = size_of::<ProjectionReadonlyColumnsV1>()
                .checked_add(2 * size_of::<usize>())
                .ok_or_else(|| invalid("projection terminal columns layout"))?;
            common.charge(1, n as u64).map_err(paid)?;
            let columns_fee = common
                .memory()
                .reserve(n, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let columns = Arc::new(columns_fee.retain(ProjectionReadonlyColumnsV1 {
                state,
                agent_electricity: electricity,
                agent_intents,
                observation_state: observation,
                auxiliary,
                template,
            }));
            let n = size_of::<CurrentNormalProjectionCommitmentV1>();
            common.charge(1, n as u64).map_err(paid)?;
            let fee = common
                .memory()
                .reserve(n, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let refs = parent.refs();
            let guard = self.ctx.transaction.guard();
            self.ctx.transaction.validate_guard(&guard, &live())?;
            let output = CurrentNormalProjectionCommitmentV1 {
                columns,
                raw_state,
                projection_root,
                parent_record: refs.head.verified_record_id(),
                parent_body: refs.runtime.occurrence().head_body_id(),
                parent_roots: refs.core.body().roots,
                guard,
                core: common.work().identity(),
                memory,
                _fee: fee,
            };
            output.validate(
                self.ctx.transaction,
                common.memory(),
                common.work().identity(),
                &live(),
            )?;
            Ok(output)
        }));
        match attempt {
            Ok(Ok(output)) => Ok(output),
            Ok(Err(error)) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                Err(error)
            }
            Err(payload) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                std::panic::resume_unwind(payload)
            }
        }
    }
}

/// A paid exact Current semantic hash cut. It is neither a completed step nor
/// a Cognition terminal marker, and its State columns cannot be detached.
pub(in crate::world) struct ActualCurrentStateHashSnapshotV1 {
    state: super::super::state_columns::OriginalStateColumnsDataV1,
    manifest_hash: String,
    raw_state: super::super::raw_hash::PaidOriginalStateHashDataV1,
    guard: oasis7_formal_protocol::incremental_execution::PreparedGuard,
    parent_record: [u8; 32],
    parent_body: [u8; 32],
    parent_roots: [u8; 32],
    core: WorkIdentityV1,
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
impl ActualCurrentStateHashSnapshotV1 {
    pub(in crate::world) fn hash(&self) -> &str {
        self.raw_state.as_str()
    }
    pub(in crate::world) fn manifest_hash_data_v1(&self) -> &str {
        &self.manifest_hash
    }
    pub(in crate::world) fn validate_current_context<M: MerkleNodeReader>(
        &self,
        parent: &BoundNormalTickParentV1<'_>,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        parent.validate_context(common).map_err(paid)?;
        common.charge(8, 96).map_err(paid)?;
        tx.validate_exact_transaction_identity(&self.guard)?;
        tx.validate_read_source_guard(&self.guard, live)?;
        self.state.validate_context(common)?;
        self.raw_state.validate_context(common)?;
        let refs = parent.refs();
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self._fee.uses_budget(common.memory())
            || !tx.uses_budget(common.memory())
            || self.parent_record != refs.head.verified_record_id()
            || self.parent_body != refs.runtime.occurrence().head_body_id()
            || self.parent_roots != refs.core.body().roots
            || tx.parent_fence().parent_commit != self.parent_body
            || tx.parent_fence().parent_roots != self.parent_roots
        {
            return Err(invalid("actual Current State hash snapshot provenance"));
        }
        Ok(())
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
    /// Capture the actual dirty typed columns and fixed WorldRules branches,
    /// then invoke the original WorldState/Manifest/Policy SHA stream. No
    /// terminal metadata, accepted hash, RootSet digest or caller hash enters.
    pub(in crate::world) fn capture_actual_current_state_hash_v1<P, V, PE, PC>(
        &mut self,
        parent: &BoundNormalTickParentV1<'_>,
        cache: &ReservedValue<AcceptedNormalProjectionCacheV1>,
        manifest: &ReservedValue<
            crate::world::incremental_module_execution_manifest_hash::CurrentWorldManifestHashV1,
        >,
        policy: &PaidCurrentBootstrapPolicyV1,
        read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
        work: &mut NormalProjectionCurrentWorkV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<ActualCurrentStateHashSnapshotV1, RootModuleExecutionErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        V: ValueNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parent.validate_context(common).map_err(paid)?;
            cache.validate_parent_v1(parent, common)?;
            manifest.validate_current_read_source_v1(
                parent,
                self.ctx.transaction,
                common,
                &live(),
            )?;
            policy.validate_current(self.ctx.transaction, common, &live())?;
            if !cache.uses_budget(common.memory()) {
                return Err(invalid("actual Current hash accepted cache outer pool").into());
            }
            let before = self.ctx.transaction.guard();
            let sources =
                self.capture_normal_projection_state_sources_v1(parent, read, work, common, live)?;
            let rules = self.capture_current_projection_world_rules_delta_v1(
                parent,
                read,
                work.world_rules,
                common,
                live,
            )?;
            let mut state = super::super::fold::fold_actual_normal_state_sources_v1(
                &cache.columns.state,
                &sources,
                self.ctx.transaction,
                common,
                &live(),
            )?;
            state.apply_world_rule_delta_data_v1(&rules, self.ctx.transaction, common, &live())?;
            let raw_state =
                super::super::raw_hash::hash_current_state_columns_with_manifest_data_v1(
                    &state,
                    manifest,
                    policy,
                    parent,
                    self.ctx.transaction,
                    common,
                    &live(),
                )?;
            // Only new reads may have appeared throughout capture/fold/hash.
            self.ctx
                .transaction
                .validate_read_source_guard(&before, &live())?;
            manifest.validate_current_read_source_v1(
                parent,
                self.ctx.transaction,
                common,
                &live(),
            )?;
            policy.validate_current(self.ctx.transaction, common, &live())?;
            cache.validate_parent_v1(parent, common)?;
            let manifest_len = manifest.hash().len();
            let n = size_of::<ActualCurrentStateHashSnapshotV1>()
                .checked_add(manifest_len)
                .ok_or_else(|| invalid("Current State snapshot manifest layout"))?;
            common.charge(1, n as u64).map_err(paid)?;
            let fee = common
                .memory()
                .reserve(n, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let mut manifest_bytes = Vec::with_capacity(manifest_len);
            for chunk in manifest.hash().as_bytes().chunks(8192) {
                common.charge(1, chunk.len() as u64).map_err(paid)?;
                manifest_bytes.extend_from_slice(chunk);
            }
            let manifest_hash = String::from_utf8(manifest_bytes).map_err(paid)?;
            manifest.validate_current_read_source_v1(
                parent,
                self.ctx.transaction,
                common,
                &live(),
            )?;
            let refs = parent.refs();
            let value = ActualCurrentStateHashSnapshotV1 {
                state,
                manifest_hash,
                raw_state,
                guard: self.ctx.transaction.guard(),
                parent_record: refs.head.verified_record_id(),
                parent_body: refs.runtime.occurrence().head_body_id(),
                parent_roots: refs.core.body().roots,
                core: common.work().identity(),
                memory: common.memory().clone(),
                _fee: fee,
            };
            value.validate_current_context(parent, self.ctx.transaction, common, &live())?;
            Ok(value)
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

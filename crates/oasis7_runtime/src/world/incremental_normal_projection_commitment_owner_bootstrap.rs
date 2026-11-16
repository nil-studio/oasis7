//! The one fixed accepted-cache mint: consumes an actual bound bootstrap
//! source, rebuilds the original public/private projection and compares its
//! complete root with the independently verified accepted head.
use super::*;
use crate::world::{
    PaidProjectionRetainedOriginalReaderV1,
    incremental_module_capability_admission_rules::PaidCurrentBootstrapPolicyV1,
    incremental_normal_business_commitment::OriginalProjectionReadMetadataV1,
    incremental_normal_projection_bootstrap::BoundNormalProjectionBootstrapSourceV1,
    incremental_normal_sources::BoundNormalTickParentV1,
};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_world_state::observation_projection::current_commitment::ProjectionDataPathRefV1 as Path;

impl BoundNormalProjectionBootstrapSourceV1 {
    /// No graph, root, completion flag or bare identity is accepted. Failed
    /// authentication consumes the source and aborts its bootstrap transaction.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn into_accepted_projection_cache_v1<
        M: MerkleNodeReader,
        R: PaidProjectionRetainedOriginalReaderV1<RetainedError = BodyMutationError>,
    >(
        self,
        parent: &BoundNormalTickParentV1<'_>,
        metadata: &OriginalProjectionReadMetadataV1<'_>,
        policy: &PaidCurrentBootstrapPolicyV1,
        process_source: &crate::world::incremental_normal_process_source::NormalProcessSourceV1,
        authorization: &crate::world::incremental_authorization_root_current::FrozenNormalAuthorizationProjectionV1,
        finality: &crate::world::incremental_normal_bootstrap_finality::CurrentBootstrapLiveFinalityV1,
        tx: &mut ExecutionTransaction<'_, M>,
        retained: &mut R,
        common: &mut FormalBoundedContextV1<'_>,
        canonical: &mut CanonicalCborWorkV1,
        live: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<ReservedValue<AcceptedNormalProjectionCacheV1>, BodyMutationError> {
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parent.validate_context(common).map_err(paid)?;
            self.validate_context(common)?;
            if tx.parent_fence() != parent.fence() || !tx.uses_budget(common.memory()) {
                return Err(invalid("projection bootstrap actual selected transaction"));
            }
            tx.validate_bootstrap_read_only_v1(&live())?;
            metadata.validate_provenance(tx, common, &live())?;
            policy.validate_current(tx, common, &live())?;
            let (seed, record, body, roots, fees) = self.into_parts_v1();
            let refs = parent.refs();
            common.charge(3, 96).map_err(paid)?;
            if record != refs.head.verified_record_id()
                || body != refs.runtime.occurrence().head_body_id()
                || roots != refs.core.body().roots
            {
                return Err(invalid("projection bootstrap actual accepted parent"));
            }
            let state=super::super::state_columns::OriginalStateColumnsDataV1::from_original_seed_data_v1(&seed,common)?;
            let electricity = super::super::electricity::from_seed_v1(&seed, common)?;
            let agent_intents=super::super::intents::from_seed_v1(&seed,common)?;
            let observation = seed.observation_columns_data_v1(common)?;
            let mut auxiliary = seed.auxiliary_columns_data_v1(common)?;
            let memory = common.memory().clone();
            // These two control values come from the actual accepted History
            // manifest, never from caller-provided flags or guessed GEN values.
            let manifest = refs.cognition.selected().manifest();
            for (name, value) in [
                (
                    "enabled",
                    super::super::capture_projection_json_data_v1(&manifest.enabled, common)?,
                ),
                (
                    "revision",
                    super::super::capture_projection_json_data_v1(&manifest.revision, common)?,
                ),
            ] {
                let decoded=crate::world::incremental_normal_projection_runtime::bootstrap_projection_value_v1(&value,common)?;
                auxiliary = auxiliary
                    .with_path_data_v1(
                        &[Path::Field("cognition"), Path::Field(name)],
                        Some(&decoded),
                        &memory,
                        &mut |s, b| common.charge(s, b),
                    )
                    .map_err(projection_data_error)?;
            }
            let template = super::super::template::build_static_projection_schema_data_v1(common)?;
            // Both variants retain an actual typed readonly governance source.
            // GEN additionally checks every metadata field against its immutable
            // original producer before that source can be used here.
            let terminal = match metadata {
                OriginalProjectionReadMetadataV1::Current(source) => *source,
                OriginalProjectionReadMetadataV1::Genesis { governance, .. } => *governance,
            };
            let raw_state = super::super::raw_hash::hash_current_state_columns_data_v1(
                &state,
                terminal,
                policy,
                tx,
                common,
                &live(),
            )?;
            if let Some(expected) = fees.expected_genesis_raw_state_hash() {
                common
                    .charge(1, (expected.len() + raw_state.as_str().len()) as u64)
                    .map_err(paid)?;
                if expected != raw_state.as_str() {
                    return Err(invalid("projection GEN original State hash mismatch"));
                }
            }
            let binding = metadata.runtime_binding_for_state_hash_bounded_v1(
                raw_state.as_str(),
                common,
                canonical,
            )?;
            let process = process_source.capture_projection_facts_v1(parent, common)?;
            let actual = super::super::leaf::compute_current_projection_root_v1(
                &state,
                &electricity,
                &agent_intents,
                &observation,
                &auxiliary,
                &template,
                metadata,
                &process,
                authorization,
                finality,
                &raw_state,
                binding.as_ref(),
                tx,
                retained,
                common,
                canonical,
                live,
            )?;
            common
                .charge(
                    1,
                    (actual.len() + fees.expected_projection_root().len()) as u64,
                )
                .map_err(paid)?;
            if actual.as_str() != fees.expected_projection_root() {
                return Err(invalid(
                    "projection bootstrap signed original projection root mismatch",
                ));
            }
            // Keep physical/side/read owners alive through the complete root
            // comparison and final read-only guard validation.
            fees.validate_context(common)?;
            metadata.validate_provenance(tx, common, &live())?;
            tx.validate_bootstrap_read_only_v1(&live())?;
            let n = size_of::<ProjectionReadonlyColumnsV1>()
                .checked_add(2 * size_of::<usize>())
                .ok_or_else(|| invalid("projection bootstrap resident layout"))?;
            common.charge(1, n as u64).map_err(paid)?;
            let fee = common
                .memory()
                .reserve(n, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let columns = Arc::new(fee.retain(ProjectionReadonlyColumnsV1 {
                state,
                agent_electricity: electricity,
                agent_intents,
                observation_state: observation,
                auxiliary,
                template,
            }));
            let n = size_of::<AcceptedNormalProjectionCacheV1>();
            common.charge(1, n as u64).map_err(paid)?;
            let fee = common
                .memory()
                .reserve(n, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let cache = fee.retain(AcceptedNormalProjectionCacheV1 {
                columns,
                record,
                body,
                roots,
                memory,
            });
            cache.validate_parent_v1(parent, common)?;
            Ok(cache)
        }));
        match attempt {
            Ok(Ok(cache)) => Ok(cache),
            Ok(Err(error)) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    tx.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                Err(error)
            }
            Err(panic) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    tx.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                std::panic::resume_unwind(panic)
            }
        }
    }
}

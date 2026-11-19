//! Fixed GEN source -> original signed projection check -> resident install.
//! The base has already been built from the same authenticated accepted parent;
//! provisional physical GEN data never grants a normal execution capability.
use super::*;
use crate::world::{
    PaidProjectionRetainedOriginalReaderV1,
    incremental_genesis_projection::RuntimeProjectedGenesisV1,
    incremental_module_capability_admission_rules::PaidCurrentBootstrapPolicyV1,
    incremental_normal_business_commitment::{
        NormalTerminalReadMetadataV1, OriginalProjectionReadMetadataV1,
        PreparedNormalBusinessMetadataGenesisV1,
    },
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionTransaction,
    incremental_projection_sidecar::PaidVerifiedProjectionSidecarV1,
};
use oasis7_wasm_abi::CanonicalCborWorkV1;
impl PendingNormalHostResidentSourcesV1 {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn finish_accepted_genesis_projection_v1<
        M: MerkleNodeReader,
        R: PaidProjectionRetainedOriginalReaderV1<RetainedError = BodyMutationError>,
    >(
        self,
        projected: RuntimeProjectedGenesisV1,
        side: PaidVerifiedProjectionSidecarV1,
        parent: &BoundNormalTickParentV1<'_>,
        governance: &NormalTerminalReadMetadataV1,
        policy: &PaidCurrentBootstrapPolicyV1,
        authorization: &crate::world::incremental_authorization_root_current::FrozenNormalAuthorizationProjectionV1,
        finality: &crate::world::incremental_normal_bootstrap_finality::CurrentBootstrapLiveFinalityV1,
        tx: &mut ExecutionTransaction<'_, M>,
        retained: &mut R,
        common: &mut FormalBoundedContextV1<'_>,
        canonical: &mut CanonicalCborWorkV1,
        live: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<
        (
            ReservedValue<NormalHostResidentSourcesV1>,
            super::super::incremental_genesis::RuntimeGenesisSummaryV1,
            PreparedNormalBusinessMetadataGenesisV1,
        ),
        BodyMutationError,
    > {
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parent.validate_context(common).map_err(formal)?;
            tx.validate_bootstrap_read_only_v1(&live())?;
            let (source, summary, metadata) = projected
                .bind_accepted_genesis_projection_source_v1(parent, tx, side, common, &live())?;
            metadata.check_actual_parent(self.business_v1(), common)?;
            let read = OriginalProjectionReadMetadataV1::Genesis {
                source: &metadata,
                governance,
            };
            let cache = source.into_accepted_projection_cache_v1(
                parent,
                &read,
                policy,
                &self.process,
                authorization,
                finality,
                tx,
                retained,
                common,
                canonical,
                live,
            )?;
            // This is the only conversion to a full normal resident. Its exact
            // parent/cache/Core checks run after the signed-root comparison.
            let resident = self.finish_projection_cache_v1(parent, cache, common)?;
            tx.validate_bootstrap_read_only_v1(&live())?;
            Ok((resident, summary, metadata))
        }));
        match attempt {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(error)) => {
                let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
                Err(error)
            }
            Err(panic) => {
                let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
                std::panic::resume_unwind(panic)
            }
        }
    }
}

//! Closed provenance adapters for the shared observation DATA kernel.
//! A GEN source has no transaction. Current still uses its original guard,
//! metadata, authorization, process and accepted-history proofs.
use super::*;
use crate::world::incremental_normal_bootstrap_finality::CurrentBootstrapFinalityErrorV1;
use crate::world::incremental_normal_business_commitment::OriginalProjectionReadMetadataV1;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::MerkleNodeReader,
};

pub(in crate::world) mod sealed {
    pub trait Source {}
}

#[derive(Clone, Copy)]
pub(in crate::world) struct ProjectionProcessPolicyDataV1 {
    pub(in crate::world) hardened: bool,
    pub(in crate::world) local_finality: bool,
}
impl ProjectionProcessPolicyDataV1 {
    pub(in crate::world) fn production_hardened(&self) -> bool {
        self.hardened
    }
    pub(in crate::world) fn allow_local_finality_signing(&self) -> bool {
        self.local_finality
    }
}

pub(in crate::world) trait ProjectionReadSourceV1: sealed::Source {
    fn validate_metadata(
        &self,
        metadata: &OriginalProjectionReadMetadataV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError>;
    fn begin_facts(&self, common: &mut FormalBoundedContextV1<'_>)
    -> Result<(), BodyMutationError>;
    fn check(&self, common: &mut FormalBoundedContextV1<'_>) -> Result<(), BodyMutationError>;
    fn process_policy(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ProjectionProcessPolicyDataV1, BodyMutationError>;
    fn authorization(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError>;
    fn finality_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError>;
    fn finality(
        &self,
        epoch: u64,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), CurrentBootstrapFinalityErrorV1>;
}

pub(in crate::world) struct CurrentProjectionReadSourceV1<'a, 's, M> {
    tx: &'a ExecutionTransaction<'s, M>,
    guard: PreparedGuard,
    observed: std::cell::Cell<ExecutionFence>,
    live: RefCell<&'a mut dyn FnMut() -> ExecutionFence>,
    process: Option<&'a crate::world::incremental_normal_process_source::PaidProjectionProcessFactsV1>,
    authorization: Option<&'a crate::world::incremental_authorization_root_current::FrozenNormalAuthorizationProjectionV1>,
    finality: Option<&'a crate::world::incremental_normal_bootstrap_finality::CurrentBootstrapLiveFinalityV1>,
}
impl<'a, 's, M: MerkleNodeReader> CurrentProjectionReadSourceV1<'a, 's, M> {
    pub(in crate::world) fn new(
        tx: &'a ExecutionTransaction<'s, M>,
        live: &'a mut dyn FnMut() -> ExecutionFence,
    ) -> Self {
        Self {
            tx,
            guard: tx.guard(),
            observed: std::cell::Cell::new(tx.parent_fence()),
            live: RefCell::new(live),
            process: None,
            authorization: None,
            finality: None,
        }
    }
    pub(in crate::world) fn with_security(
        mut self,
        process: &'a crate::world::incremental_normal_process_source::PaidProjectionProcessFactsV1,
        authorization: &'a crate::world::incremental_authorization_root_current::FrozenNormalAuthorizationProjectionV1,
        finality: &'a crate::world::incremental_normal_bootstrap_finality::CurrentBootstrapLiveFinalityV1,
    ) -> Self {
        self.process = Some(process);
        self.authorization = Some(authorization);
        self.finality = Some(finality);
        self
    }
}
impl<M> sealed::Source for CurrentProjectionReadSourceV1<'_, '_, M> {}
impl<M: MerkleNodeReader> ProjectionReadSourceV1 for CurrentProjectionReadSourceV1<'_, '_, M> {
    fn validate_metadata(
        &self,
        metadata: &OriginalProjectionReadMetadataV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        let fence = (self.live.borrow_mut())();
        metadata.validate_provenance(self.tx, common, &fence)?;
        self.observed.set(fence);
        Ok(())
    }
    fn begin_facts(&self, _: &mut FormalBoundedContextV1<'_>) -> Result<(), BodyMutationError> {
        self.tx
            .validate_guard(&self.guard, &self.observed.get())
            .map_err(Into::into)
    }
    fn check(&self, _: &mut FormalBoundedContextV1<'_>) -> Result<(), BodyMutationError> {
        self.tx
            .validate_read_source_guard(&self.guard, &(self.live.borrow_mut())())
            .map_err(Into::into)
    }
    fn process_policy(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ProjectionProcessPolicyDataV1, BodyMutationError> {
        let source = self
            .process
            .ok_or_else(|| invalid("actual projection Host process facts missing"))?;
        source.validate_for_tx_v1(self.tx, common)?;
        Ok(ProjectionProcessPolicyDataV1 {
            hardened: source.production_hardened(),
            local_finality: source.allow_local_finality_signing(),
        })
    }
    fn authorization(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        let source = self
            .authorization
            .ok_or_else(|| invalid("actual projection verified authorization source missing"))?;
        source
            .verify_projection_root_data_v1(self.tx, common, &(self.live.borrow_mut())())
            .map_err(|error| paid(ProjectionAuthorizationCause(error)))
    }
    fn finality_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        let source = self
            .finality
            .ok_or_else(|| invalid("actual accepted projection finality source missing"))?;
        source.validate_projection_context_v1(self.tx, common, &(self.live.borrow_mut())())
    }
    fn finality(
        &self,
        epoch: u64,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), CurrentBootstrapFinalityErrorV1> {
        let source = self
            .finality
            .ok_or_else(|| invalid("actual accepted projection finality source missing"))?;
        source.validate_projection_audience_v1(self.tx, epoch, common, &(self.live.borrow_mut())())
    }
}

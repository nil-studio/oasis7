//! Provisional GEN business source from one actual immutable World wrapper.
//! Final roots are index-finish DATA, never accepted state or a read capability.
use super::*;
use crate::world::incremental_genesis_projection::RuntimeProjectedGenesisV1;
use oasis7_formal_protocol::incremental_execution::RootSetV1;
pub struct PaidProjectedGenesisBusinessSourceV1<'a> {
    projected: &'a RuntimeProjectedGenesisV1,
    roots: &'a RootSetV1,
    after: String,
    core: WorkIdentityV1,
    fee: MemoryReservation,
}
impl RuntimeProjectedGenesisV1 {
    pub fn business_source_bounded_v1<'a>(
        &'a self,
        actual_final_roots: &'a RootSetV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<PaidProjectedGenesisBusinessSourceV1<'a>, BodyMutationError> {
        self.validate_context_bounded_v1(common)?;
        let n = std::mem::size_of::<PaidProjectedGenesisBusinessSourceV1<'_>>() + 64;
        common.charge(32, n as u64 + 1024).map_err(original)?;
        let fee = common
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let after = hex::encode(actual_final_roots.digest().map_err(original)?);
        Ok(PaidProjectedGenesisBusinessSourceV1 {
            projected: self,
            roots: actual_final_roots,
            after,
            core: common.work().identity(),
            fee,
        })
    }
}
impl PaidProjectedGenesisBusinessSourceV1<'_> {
    pub fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common.consume_steps(1).map_err(original)?;
        if self.core != common.work().identity() || !self.fee.uses_budget(common.memory()) {
            return Err(bad());
        }
        self.projected.validate_context_bounded_v1(common)
    }
    pub fn identity(&self) -> (&str, &str, Option<&str>) {
        self.projected.business_metadata().identity()
    }
    pub fn world_result_count(&self) -> usize {
        self.projected.business_metadata().result_count()
    }
    pub fn after_state_root(&self) -> &str {
        &self.after
    }
    pub fn final_roots(&self) -> &RootSetV1 {
        self.roots
    }
    pub fn authority_generation(&self) -> u64 {
        self.projected.business_metadata().authority()
    }
    pub fn permission_generation(&self) -> u64 {
        self.projected.business_metadata().permission()
    }
    pub fn journal_bounds(&self) -> (u64, u64, u64) {
        self.projected.business_metadata().journal_bounds()
    }
    pub fn runtime_binding_bounded_v1(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
        canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
    ) -> Result<ReservedValue<oasis7_agent_api::RuntimeBindingV1>, BodyMutationError> {
        self.validate_context(common)?;
        self.projected
            .business_metadata()
            .runtime_binding_bounded(common, canonical)
    }
}

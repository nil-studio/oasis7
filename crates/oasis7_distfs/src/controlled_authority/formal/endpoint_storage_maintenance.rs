//! Maintenance never grants serving authority or changes accepted world state.
use super::*;
impl FileFormalEndpoint {
    pub fn collect_physical_orphans(
        &mut self,
    ) -> Result<super::super::physical::FormalPhysicalReclaimReport, FormalError> {
        let _authority_guard = self.guard()?;
        super::super::physical::roots::verify(&self.store.path.join("formal-objects-v1"))?;
        let roots = self.store.verify_storage_roots_durable(
            self.authority.definition().profile_version,
            &self.public_key,
        )?;
        let checkpoint = self.store.checkpoint_bytes()?;
        super::super::physical::collect_verified(
            &self.store.path.join("formal-objects-v1"),
            &roots,
            &checkpoint,
        )
    }
    /// Advance one bounded background batch; repeat until building=false.
    /// Calls can be interleaved with ordinary endpoint writes. Final selection
    /// checks the exact overlay under the stable reader/writer fence.
    pub fn advance_physical_generation(
        &mut self,
    ) -> Result<super::super::physical::FormalGenerationProgress, FormalError> {
        let _authority_guard = self.guard()?;
        let root = self.store.path.join("formal-objects-v1");
        // guard() fixes and verifies the actual protected checkpoint. The
        // generation operation streams the complete retained logical namespace
        // and verifies each original object, including every old phase, rather
        // than repeatedly materializing an ever-growing typed mark set here.
        // It never reclaims logical objects or external dependencies.
        super::super::physical::advance_generation(&root)
    }
}

use super::World;
use crate::DomainEvent;
use crate::domains::module_lifecycle::ModuleArtifactRetirementDelta;
use crate::world::transition::{RootInstallContext, SubsystemDelta};

pub(super) struct PreparedModuleArtifactRetirement(pub(super) ModuleArtifactRetirementDelta);

impl PreparedModuleArtifactRetirement {
    pub(super) fn matches_event(&self, event: &DomainEvent) -> bool {
        self.0.matches_event(event)
    }

    pub(super) fn install(self, world: &mut World) {
        let mut ctx = RootInstallContext::new(world);
        self.0.apply_infallible(&mut ctx);
    }
}

impl World {
    pub(super) fn prepare_module_artifact_retirement(
        &self,
        wasm_hash: String,
    ) -> PreparedModuleArtifactRetirement {
        PreparedModuleArtifactRetirement(ModuleArtifactRetirementDelta::prepare(self, wasm_hash))
    }
}

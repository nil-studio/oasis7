use super::World;
use crate::DomainEvent;

pub(super) struct PreparedModuleArtifactRetirement {
    wasm_hash: String,
    max_cached_modules: usize,
}

impl PreparedModuleArtifactRetirement {
    pub(super) fn matches_event(&self, event: &DomainEvent) -> bool {
        matches!(event, DomainEvent::ModuleArtifactDestroyed { wasm_hash, .. } if wasm_hash == &self.wasm_hash)
    }

    pub(super) fn install(self, world: &mut World) {
        world.module_artifacts.remove(&self.wasm_hash);
        world.module_artifact_bytes.remove(&self.wasm_hash);
        world.module_cache = oasis7_wasm_abi::ModuleCache::new(self.max_cached_modules);
    }
}

impl World {
    pub(super) fn prepare_module_artifact_retirement(
        &self,
        wasm_hash: String,
    ) -> PreparedModuleArtifactRetirement {
        PreparedModuleArtifactRetirement {
            wasm_hash,
            max_cached_modules: self.module_cache.max_cached_modules(),
        }
    }
}

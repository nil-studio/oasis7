use super::*;

impl World {
    /// Install a process-local development compiler. Admission policy still gates invocation.
    /// The capability is not serialized into snapshots or replay journals.
    pub fn set_module_source_compiler(
        &mut self,
        compiler: Arc<dyn oasis7_agent_api::module_source::ModuleSourceCompiler>,
    ) {
        self.source_compiler =
            super::super::module_source_compiler::RuntimeSourceCompiler::new(compiler);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_agent_api::world_action::ModuleSourcePackage;

    fn assert_unavailable(world: &World) {
        let error = world
            .source_compiler
            .compile(
                "",
                &ModuleSourcePackage {
                    manifest_path: String::new(),
                    files: Default::default(),
                },
            )
            .unwrap_err();
        assert!(matches!(error, WorldError::ModuleChangeInvalid { reason }
            if reason == "source compilation requires a host compiler capability"));
    }
    #[test]
    fn raw_compiler_capability_is_unavailable_after_snapshot_and_serde() {
        let raw = World::new();
        assert_unavailable(&raw);
        let restored = World::from_snapshot(raw.snapshot(), raw.journal().clone()).unwrap();
        assert_unavailable(&restored);
        let encoded = serde_json::to_vec(&raw).unwrap();
        let decoded: World = serde_json::from_slice(&encoded).unwrap();
        assert_unavailable(&decoded);
        assert_eq!(encoded, serde_json::to_vec(&decoded).unwrap());
    }
}

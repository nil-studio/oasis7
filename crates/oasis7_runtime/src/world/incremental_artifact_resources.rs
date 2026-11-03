//! Runtime-owned borrowed adapter for the three Artifacts state records.
use super::{BuiltinReleaseManifestEntry, BuiltinReleaseManifestState, World};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{
    incremental_field_member::MemberRecordError,
    incremental_runtime_artifact_resources::{
        BuiltinReleaseEntryRefV1, BuiltinReleaseManifestSourceV1, BuiltinReleaseModuleSetSourceV1,
        RuntimeArtifactResourcesRefV1, RuntimeArtifactResourcesWorkV1,
        prepare_runtime_artifact_resources_genesis_v1,
    },
};
use std::collections::BTreeMap;

struct BuiltinReleaseView<'a>(&'a BuiltinReleaseManifestState);
struct BuiltinModuleSetView<'a>(&'a BTreeMap<String, BuiltinReleaseManifestEntry>);

impl<'source> BuiltinReleaseModuleSetSourceV1 for BuiltinModuleSetView<'source> {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn for_each_module<'a, E, F>(&'a self, mut visit: F) -> Result<(), E>
    where
        F: FnMut(&'a str, BuiltinReleaseEntryRefV1<'a>) -> Result<(), E>,
    {
        for (module_id, entry) in self.0 {
            visit(
                module_id,
                BuiltinReleaseEntryRefV1 {
                    hash_tokens: &entry.hash_tokens,
                    artifact_identities: &entry.artifact_identities,
                },
            )?;
        }
        Ok(())
    }
}

impl<'source> BuiltinReleaseManifestSourceV1 for BuiltinReleaseView<'source> {
    type ModuleSet<'a>
        = BuiltinModuleSetView<'a>
    where
        Self: 'a;

    fn len(&self) -> usize {
        self.0.module_sets.len()
    }

    fn for_each_module_set<'a, E, F>(&'a self, mut visit: F) -> Result<(), E>
    where
        F: FnMut(&'a str, Self::ModuleSet<'a>) -> Result<(), E>,
    {
        for (set_id, modules) in &self.0.module_sets {
            visit(set_id, BuiltinModuleSetView(modules))?;
        }
        Ok(())
    }
}

impl World {
    /// Borrow the three original Runtime Artifacts fields into their typed IWS
    /// provisional records. This does not commit or qualify a complete World.
    pub fn prepare_incremental_artifact_resources_genesis_v1<W, C, S>(
        &self,
        context: &mut oasis7_world_state::incremental_field_member::MemberRecordWriteContext<
            '_,
            W,
            C,
        >,
        work: &mut RuntimeArtifactResourcesWorkV1,
        sink: &mut S,
    ) -> Result<u64, MemberRecordError>
    where
        W: StatePageWriter + ValueNodeReader,
        C: FnMut() -> bool,
        S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
    {
        prepare_runtime_artifact_resources_genesis_v1(
            &RuntimeArtifactResourcesRefV1 {
                chain_resource_manifest: &self.chain_resource_manifest,
                latest_chain_resource_delta: self.latest_chain_resource_delta.as_ref(),
                builtin_release_manifest: &BuiltinReleaseView(&self.builtin_release_manifest),
            },
            context,
            work,
            sink,
        )
    }
}

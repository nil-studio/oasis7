//! Typed Runtime Artifacts records. These codecs preserve original state fields;
//! they are not business authority or a World activation proof.
use oasis7_agent_api::world_resource::{ChainResourceDelta, ChainResourceManifest};
use oasis7_wasm_abi::ModuleArtifactIdentity;
use std::collections::BTreeMap;

#[path = "incremental_runtime_artifact_resources_schema.rs"]
mod schema;
pub use schema::{RuntimeArtifactFieldV1, RuntimeArtifactRecordV1};

#[path = "incremental_runtime_artifact_resources_read.rs"]
mod read;
/// Decoded value returned by the typed Runtime Artifacts point readers.
pub use crate::incremental_record_value_read::RecordValueV1 as RuntimeArtifactValueV1;
pub use read::{
    RuntimeArtifactChildRootV1, RuntimeArtifactMemberProfileV1, RuntimeArtifactRootBindingV1,
    inspect_runtime_artifact_record_root_v1, read_runtime_artifact_member_v1,
    read_runtime_artifact_record_reference_v1,
};
#[path = "incremental_runtime_artifact_resources_family.rs"]
mod family;
#[path = "incremental_runtime_artifact_resources_key.rs"]
mod key;
pub use family::{
    RuntimeArtifactResourcesQualificationV1, RuntimeArtifactResourcesReadContextV1,
    qualify_runtime_artifact_resources_v1,
};

#[path = "incremental_runtime_artifact_resources_genesis.rs"]
mod genesis;
pub use genesis::{RuntimeArtifactResourcesWorkV1, prepare_runtime_artifact_resources_genesis_v1};

/// A borrowed view of one builtin manifest entry. Strings and maps remain
/// owned by Runtime and are traversed without cloning.
pub struct BuiltinReleaseEntryRefV1<'a> {
    pub hash_tokens: &'a [String],
    pub artifact_identities: &'a BTreeMap<String, ModuleArtifactIdentity>,
}

pub trait BuiltinReleaseModuleSetSourceV1 {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn for_each_module<'a, E, F>(&'a self, visit: F) -> Result<(), E>
    where
        F: FnMut(&'a str, BuiltinReleaseEntryRefV1<'a>) -> Result<(), E>;
}

/// Borrow-only adapter contract for Runtime's local builtin release types.
/// Implementations expose the original outer set key, inner module key and
/// full entry without flattening or allocating a shadow map.
pub trait BuiltinReleaseManifestSourceV1 {
    type ModuleSet<'a>: BuiltinReleaseModuleSetSourceV1
    where
        Self: 'a;

    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn for_each_module_set<'a, E, F>(&'a self, visit: F) -> Result<(), E>
    where
        F: FnMut(&'a str, Self::ModuleSet<'a>) -> Result<(), E>;
}

/// Complete borrowed input for the three original Runtime Artifacts fields.
pub struct RuntimeArtifactResourcesRefV1<'a, B> {
    pub chain_resource_manifest: &'a ChainResourceManifest,
    pub latest_chain_resource_delta: Option<&'a ChainResourceDelta>,
    pub builtin_release_manifest: &'a B,
}

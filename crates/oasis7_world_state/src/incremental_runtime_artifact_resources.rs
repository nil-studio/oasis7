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
    RUNTIME_ARTIFACT_RESOURCES_SCAN_DEPTH_V1, RuntimeArtifactResourcesPreparedTransitionV1,
    RuntimeArtifactResourcesQualificationV1, RuntimeArtifactResourcesReadContextV1,
    accept_runtime_artifact_resources_transition_v1, qualify_runtime_artifact_resources_v1,
    readmit_runtime_artifact_resources_ordinary_v1,
    verify_runtime_artifact_resources_change_transition_v1,
    verify_runtime_artifact_resources_transition_v1,
};
#[path = "incremental_runtime_artifact_resources_overlay.rs"]
mod overlay;
pub use overlay::{
    RuntimeArtifactDeltaTickChangeV1, change_runtime_artifact_delta_tick_overlay_v1,
};

#[path = "incremental_runtime_artifact_resources_genesis.rs"]
mod genesis;
pub use genesis::{RuntimeArtifactResourcesWorkV1, prepare_runtime_artifact_resources_genesis_v1};

#[path = "incremental_runtime_artifact_resources_pages.rs"]
mod pages;
pub use pages::{
    RuntimeArtifactResourcesPagesV1, RuntimeArtifactResourcesPagesWorkV1,
    mark_runtime_artifact_resources_pages_v1, transfer_runtime_artifact_resources_pages_v1,
};

/// A borrowed view of one builtin manifest entry. Strings and maps remain
/// owned by Runtime and are traversed without cloning.
#[derive(Clone, Copy)]
pub struct BuiltinReleaseEntryRefV1<'a> {
    pub hash_tokens: &'a [String],
    pub artifact_identities: &'a BTreeMap<String, ModuleArtifactIdentity>,
}

/// A complete replacement input for a snapshot-derived Runtime artifact.
/// `expected_root` is the exact previously authenticated child StateRoot
/// digest; `None` is only valid for the optional latest delta when it was
/// absent. The manifest is always required by the family schema.
#[derive(Clone, Copy)]
pub enum RuntimeArtifactBodyChangeV1<'a> {
    ReplaceManifest {
        expected_root: [u8; 32],
        after: &'a ChainResourceManifest,
    },
    ReplaceLatestDelta {
        expected_root: Option<[u8; 32]>,
        after: Option<&'a ChainResourceDelta>,
    },
}

/// A local operation matching Runtime's real two-key builtin manifest API.
/// `expected_entry_root` binds the whole old entry without materializing its
/// hash-token vector or identity map. `after` is the caller's complete
/// upsert value; `None` removes the module and prunes an empty module set.
#[derive(Clone, Copy)]
pub struct RuntimeBuiltinEntryChangeV1<'a> {
    pub module_set: &'a str,
    pub module_id: &'a str,
    pub expected_entry_root: Option<[u8; 32]>,
    pub after: Option<BuiltinReleaseEntryRefV1<'a>>,
}

/// One Runtime Artifact operation replayed by the changed-only receiver.
#[derive(Clone, Copy)]
pub enum RuntimeArtifactResourcesChangeV1<'a> {
    DeltaTick(RuntimeArtifactDeltaTickChangeV1),
    Body(RuntimeArtifactBodyChangeV1<'a>),
    BuiltinEntry(RuntimeBuiltinEntryChangeV1<'a>),
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

#[path = "incremental_runtime_artifact_resources_change.rs"]
mod change;
pub use change::{
    change_runtime_artifact_body_overlay_v1, change_runtime_builtin_entry_overlay_v1,
};

#[cfg(test)]
#[path = "incremental_runtime_artifact_resources_receiver_cold_tests.rs"]
mod receiver_cold_tests;

pub(crate) use family::accept_runtime_artifact_resources_verified_v1;

use crate::incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape};
use oasis7_formal_protocol::incremental_state::StateNamespaceV1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeArtifactFieldV1 {
    ChainResourceManifest,
    LatestChainResourceDelta,
    BuiltinReleaseManifest,
}
impl RuntimeArtifactFieldV1 {
    pub const ALL: [Self; 3] = [
        Self::ChainResourceManifest,
        Self::LatestChainResourceDelta,
        Self::BuiltinReleaseManifest,
    ];
    pub const fn name(self) -> &'static str {
        match self {
            Self::ChainResourceManifest => "chain_resource_manifest",
            Self::LatestChainResourceDelta => "latest_chain_resource_delta",
            Self::BuiltinReleaseManifest => "builtin_release_manifest",
        }
    }
    pub fn schema(self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"oasis7.iws.runtime.artifact.field.v1\0");
        h.update(self.name().as_bytes());
        *h.finalize().as_bytes()
    }
    pub const fn descriptor(self) -> FieldDescriptor {
        FieldDescriptor {
            owner: FieldOwner::Runtime,
            name: self.name(),
            shape: FieldShape::Value,
            destination: FieldDestination::State(StateNamespaceV1::Artifacts),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeArtifactRecordV1 {
    ChainResourceManifest,
    MaterialLedger,
    ChunkManifest,
    FragmentRef,
    CommitRef,
    ChainResourceDelta,
    OrderingKey,
    DeltaRuntimeResource,
    DeltaMaterialLedger,
    DeltaChunkResource,
    BuiltinReleaseManifest,
    BuiltinModuleSet,
    BuiltinReleaseEntry,
    ModuleArtifactIdentity,
}
impl RuntimeArtifactRecordV1 {
    pub const fn name(self) -> &'static str {
        match self {
            Self::ChainResourceManifest => "runtime.artifact.chain_resource_manifest.v1",
            Self::MaterialLedger => "runtime.artifact.material_ledger.v1",
            Self::ChunkManifest => "runtime.artifact.chunk_manifest.v1",
            Self::FragmentRef => "runtime.artifact.fragment_ref.v1",
            Self::CommitRef => "runtime.artifact.commit_ref.v1",
            Self::ChainResourceDelta => "runtime.artifact.chain_resource_delta.v1",
            Self::OrderingKey => "runtime.artifact.ordering_key.v1",
            Self::DeltaRuntimeResource => "runtime.artifact.delta.runtime_resource.v1",
            Self::DeltaMaterialLedger => "runtime.artifact.delta.material_ledger.v1",
            Self::DeltaChunkResource => "runtime.artifact.delta.chunk_resource.v1",
            Self::BuiltinReleaseManifest => "runtime.artifact.builtin_manifest.v1",
            Self::BuiltinModuleSet => "runtime.artifact.builtin_module_set.v1",
            Self::BuiltinReleaseEntry => "runtime.artifact.builtin_release_entry.v1",
            Self::ModuleArtifactIdentity => "runtime.artifact.module_identity.v1",
        }
    }
    pub fn schema(self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"oasis7.iws.runtime.artifact.record.v1\0");
        h.update(self.name().as_bytes());
        *h.finalize().as_bytes()
    }
    pub const fn descriptor(self) -> FieldDescriptor {
        FieldDescriptor {
            owner: FieldOwner::Runtime,
            name: self.name(),
            shape: FieldShape::NestedMap,
            destination: FieldDestination::State(StateNamespaceV1::Artifacts),
        }
    }
}

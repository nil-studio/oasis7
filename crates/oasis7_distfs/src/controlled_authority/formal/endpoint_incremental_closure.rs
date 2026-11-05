//! Endpoint-private read/retention evidence, never an execution permission.
//! Receipt publication still requires business execution and every source gate.
use super::*;
use oasis7_formal_protocol::{
    incremental_material::{
        CheckedCommitCore, CheckedExecutionArtifacts, CheckedNativeEnvironment, CoreBudget,
        ExecutionArtifactBudget, ExecutionArtifactReadContext, OriginalObjectStreamReader,
        verify_execution_artifacts, verify_native_environment,
    },
    incremental_state::MerkleNodeReader,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

/// Actual descriptor/chunks and artifact inventories, bound to one true Core.
/// These opaque content checks explicitly do not attest interpreter/ABI semantic
/// loading, current executable equivalence, or a business execution result.
pub(super) struct DeclaredExecutionMaterial {
    pub(super) native: ReservedValue<CheckedNativeEnvironment>,
    pub(super) artifacts: ReservedValue<CheckedExecutionArtifacts>,
    // Keep the existing profile-two immutable object namespace pinned while
    // the endpoint retains the actual native original-byte verification proof.
    _native_pin: std::fs::File,
}
#[path = "endpoint_incremental_closure_pages.rs"]
mod pages;
pub(super) use pages::{StoreReadWitness, StoreSessions, restore_store_read_witness_v1};

#[path = "endpoint_incremental_closure_material_v1.rs"]
mod material_v1;
pub(super) use material_v1::{
    CandidateExecutionMaterialV1, NativeChunkReadWorkV1, verify_candidate_execution_material_v1,
};

#[path = "endpoint_incremental_closure_catalog.rs"]
mod catalog;
#[path = "endpoint_incremental_closure_evidence.rs"]
mod evidence;
#[path = "endpoint_incremental_closure_originals.rs"]
mod originals;
pub(super) use evidence::{CompleteDataClosureEvidenceV1, consume_candidate_data_closure_v1};

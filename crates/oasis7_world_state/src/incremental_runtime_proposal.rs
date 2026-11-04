//! Original Runtime manifest proposals, distinct from WorldState proposals.
use crate::{
    incremental_field_member::*,
    incremental_json_records::{JsonRecordProfileV1, JsonWriteWorkV1},
    incremental_semantic_key::SemanticKeyV1,
};
use oasis7_formal_protocol::{
    incremental_state::{StateNamespaceV1, StateRootV1},
    incremental_value::ValueTreeError,
};
pub fn runtime_proposal_descriptor_v1() -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: "proposals",
        shape: FieldShape::Map,
        destination: FieldDestination::State(StateNamespaceV1::Authorization),
    }
}
pub(crate) fn columns() -> FieldDescriptor {
    FieldDescriptor {
        name: "proposals.columns",
        ..runtime_proposal_descriptor_v1()
    }
}
pub(crate) fn schema() -> [u8; 32] {
    *blake3::hash(b"oasis7.iws.runtime-proposal.columns.v1\0").as_bytes()
}
pub(crate) fn json_profile() -> JsonRecordProfileV1 {
    JsonRecordProfileV1 {
        field: FieldDescriptor {
            name: "proposals.json",
            ..columns()
        },
        schema: *blake3::hash(b"oasis7.iws.runtime-proposal.json.v1\0").as_bytes(),
    }
}
pub struct RuntimeProposalWriteWorkV1 {
    pub cells_remaining: u64,
    pub max_pending_node_bytes: usize,
    pub json: JsonWriteWorkV1,
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Runtime proposal record binding")
}
#[path = "incremental_runtime_proposal_genesis.rs"]
mod genesis;
pub use genesis::*;

#[path = "incremental_runtime_proposal_recovery.rs"]
mod recovery;
pub use recovery::{RecoveredRuntimeProposalV1, recover_runtime_proposal_body_v1};

#[path = "incremental_runtime_proposal_family.rs"]
mod family;
pub use family::*;

#[cfg(test)]
#[path = "incremental_runtime_proposal_tests.rs"]
mod tests;

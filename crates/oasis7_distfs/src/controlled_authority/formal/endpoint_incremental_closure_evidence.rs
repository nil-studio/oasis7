//! Local complete byte-closure evidence. Only the private actual consumer child
//! may construct it after every logical pass and durability barrier succeeds.
//! It is neither a Formal acceptance token nor business execution permission.
use super::*;
use oasis7_formal_protocol::incremental_material::FactLocationV1;

pub(crate) struct CompleteDataClosureEvidenceV1 {
    material: ReservedValue<CandidateExecutionMaterialV1>,
    pages: ReservedValue<StoreReadWitness>,
    cognition: FactLocationV1,
    runtime: FactLocationV1,
}
impl CompleteDataClosureEvidenceV1 {
    pub(crate) fn candidate_record_id(&self) -> [u8; 32] {
        self.material.candidate_record_id()
    }
    pub(crate) fn body_id(&self) -> [u8; 32] {
        self.material.body_id()
    }
    pub(crate) fn roots_id(&self) -> [u8; 32] {
        self.material.roots_id()
    }
    pub(crate) fn manifest_id(&self) -> [u8; 32] {
        self.material.manifest_id()
    }
    pub(crate) fn page_namespace_identity(&self) -> (u64, u64) {
        self.pages.page_namespace_identity()
    }
    pub(crate) fn native_namespace_identity(&self) -> (u64, u64) {
        self.material.native_namespace_identity()
    }
    pub(crate) fn cognition_location(&self) -> FactLocationV1 {
        self.cognition
    }
    pub(crate) fn runtime_location(&self) -> FactLocationV1 {
        self.runtime
    }
    pub(crate) fn material(&self) -> &ReservedValue<CandidateExecutionMaterialV1> {
        &self.material
    }
    pub(crate) fn pages(&self) -> &ReservedValue<StoreReadWitness> {
        &self.pages
    }
}

// The endpoint owner supplies this child. No constructor taking a tuple of
// public logical qualifications is exposed from this module.
#[path = "endpoint_incremental_closure_pipeline.rs"]
mod pipeline;
pub(crate) use pipeline::consume_candidate_data_closure_v1;

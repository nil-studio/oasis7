//! Shared exact-root instance qualification; no invocation authority.
pub(super) use oasis7_world_state::incremental_module_instance_family::validate_module_instance_transaction_v1 as binding;
pub use oasis7_world_state::incremental_module_instance_family::{
    ModuleInstanceFamilyQualificationV1, readmit_module_instance_family_ordinary_v1,
};

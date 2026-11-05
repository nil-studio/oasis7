//! Reuse the storage-facing structural qualification without minting authority.
pub(super) use oasis7_world_state::incremental_module_state_family::validate_module_state_transaction_v1 as check;
pub use oasis7_world_state::incremental_module_state_family::{
    ModuleStateFamilyQualificationV1, accept_module_state_transition_v1,
    readmit_module_state_family_ordinary_v1,
};

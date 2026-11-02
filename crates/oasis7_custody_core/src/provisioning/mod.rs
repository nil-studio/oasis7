//! Recoverable provisioning records. No native/world side effects or implicit grants.
//! Authentication, fresh proof verification and atomic persistence remain adapter duties.
mod closure;
mod directory;
mod state;
mod types;
pub use closure::*;
pub use directory::*;
pub use state::*;
pub use types::*;

mod http;
pub use http::*;

mod approval;
pub use approval::*;

pub use crate::context_body::*;

mod resource_v2_contract;
pub use resource_v2_contract::*;

#[path = "owner_setup_resource_world_body_v2_shared.rs"]
mod resource_world_body_v2;
pub use resource_world_body_v2::{OwnerSetupResourceWorldBodyV2, RESOURCE_READ_PROOF_HEADER};

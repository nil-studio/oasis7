//! Capability authorization bounded context: live grant projection and
//! authority/parent/audience validation.
//!
//! This initial slice remains a child of World to read its private canonical
//! authority state without widening that state to public mutable access.
//! Mutating grant, command and delegation paths are still owned by World until
//! their root-staged transaction migration is completed.

use super::{World, capability_authorization};

mod catalog;
pub(super) mod command_projection;
pub(super) mod effect_receipt_projection;
mod validation;
mod transaction;

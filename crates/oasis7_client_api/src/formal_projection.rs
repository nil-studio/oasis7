//! Visibility-aware commitments re-exported from `oasis7_domain_types` plus terminal v2 payload.
pub use oasis7_domain_types::formal_projection::*;
use serde::{Deserialize, Serialize};

#[path = "formal_projection_terminal_v2.rs"]
mod terminal_v2;
pub use terminal_v2::ClosedTaskFormalProjectionV2;

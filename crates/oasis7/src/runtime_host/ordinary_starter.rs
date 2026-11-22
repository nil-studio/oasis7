//! Compatibility surface for the native WorldService-owned finite ordinary starter.
pub use oasis7_world_service::ordinary_starter::*;

// Preserve the original signed-claim and publication integration tests here:
// these intentionally require the full legacy application host assembly.
#[cfg(test)]
mod legacy_claim_tests;

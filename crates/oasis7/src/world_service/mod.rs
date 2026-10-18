//! Compatibility facade for the independently owned service implementation.
pub use oasis7_world_service::*;
// Node frame encoding and browser request action assembly are application adapters.
pub mod correlation;
#[cfg(test)]
pub(crate) mod formal_read_fixtures;
pub mod gameplay;

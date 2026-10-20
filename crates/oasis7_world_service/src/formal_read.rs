//! Compatibility exports for the concrete projection proof consumer.
pub use oasis7_world_projection::formal_read::*;
#[cfg(test)]
#[path = "formal_read_adapter_tests.rs"]
mod tests;

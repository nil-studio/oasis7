//! Native host owns the original composite WorldEvent and archive extension.
pub use oasis7_runtime_host::simulator_archive::archive::*;
#[cfg(test)]
use super::persist::WorldSnapshot;
#[cfg(test)]
use oasis7_agent_api::world_resource::ChainResourceDelta;

#[cfg(test)]
#[path = "archive/retention_tests.rs"]
mod retention_tests;
#[cfg(test)]
#[path = "archive/tests.rs"]
mod tests;

use super::*;
pub use oasis7_world_state::world_rules::industrial_work_admission::*;
#[cfg(test)]
pub(crate) mod tests;

use super::industrial_work::industrial_ref;
use oasis7_agent_api::world_action::*;
use std::collections::BTreeMap;

use super::*;
pub use oasis7_world_state::world_rules::recipe_substitution::*;
#[cfg(test)]
pub(crate) mod tests;

use super::industrial_work as work;
use oasis7_agent_api::world_action::*;
use std::collections::BTreeMap;

//! Bounded business outcomes, distinct from cognition submission receipts.
use crate::{CapabilityAuthorizationEvent, CausedBy, DomainEvent, World, WorldEventBody};
use serde_json::{Value, json};

const JOURNAL_WINDOW: usize = 2048;
const RESULT_LIMIT: usize = 16;

impl World {
    pub fn recent_world_rule_action_results(&self, reader: Option<&str>) -> Value {
        match readonly::recent_from_sources_v1(&super::status_readonly::Legacy(self),reader){Ok(value)=>value,Err(never)=>match never{}}
    }
}

#[path = "action_results_readonly.rs"]
pub(crate) mod readonly;

//! Canonical owner resolution for primary and claim-backed industrial subjects.

use crate::WorldState;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAgentOwnerV1 {
    pub primary_agent_id: String,
    pub player_id: String,
    pub public_key: Option<String>,
    pub source_id: String,
    pub generation: u64,
}
impl WorldState {
    pub fn is_manufactured_agent(&self, agent: &str) -> bool {
        match crate::world_rules::is_manufactured_agent_readonly_v1(self,agent) {Ok(value)=>value,Err(never)=>match never{}}
    }
    pub fn resolved_agent_owner(&self, agent: &str) -> Option<ResolvedAgentOwnerV1> {
        match crate::world_rules::resolve_industrial_owner_readonly_v1(self,agent) {Ok(value)=>value,Err(never)=>match never{}}
    }
}

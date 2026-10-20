use super::*;
use crate::GovernanceEvent;

#[derive(Debug)]
pub struct PreparedGovernanceRegistryEvent {
    pub event: GovernanceEvent,
    pub controller_registry: Option<GovernanceMainTokenControllerRegistry>,
    pub admissions: BTreeMap<String, GovernanceValidatorAdmissionRecord>,
    pub identity_bindings: BTreeMap<String, String>,
    pub account_bindings: BTreeMap<String, String>,
}

impl PreparedGovernanceRegistryEvent {
    pub fn matches_event(&self, event: &GovernanceEvent) -> bool {
        &self.event == event
    }

    pub fn install(self, world: &mut WorldState) {
        world.governance_main_token_controller_registry = self.controller_registry;
        world.governance_validator_admissions = self.admissions;
        world.node_identity_bindings = self.identity_bindings;
        world.node_main_token_account_bindings = self.account_bindings;
    }
}

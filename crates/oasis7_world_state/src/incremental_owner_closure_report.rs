use oasis7_proto::memory_budget::ReservedValue;

/// Bounded diagnostics for a state whose generic tree/value closure passed but
/// whose typed owner inventory is not yet complete. Keeping the fields charged
/// prevents the diagnostic from escaping the endpoint's shared pool.
#[derive(Debug)]
pub struct UnsupportedWorldStateOwnersV1 {
    pub(super) root_set_digest: [u8; 32],
    pub(super) unsupported_fields: ReservedValue<Vec<&'static str>>,
    pub(super) runtime_registry_missing: bool,
    pub(super) cognition_registry_missing: bool,
    pub(super) checked_u64_fields: u8,
    pub(super) checked_id_set_fields: u8,
    pub(super) checked_number_map_fields: u8,
    pub(super) checked_text_byte_fields: u8,
    pub(super) checked_installed_target_fields: u8,
    pub(super) checked_world_balance_fields: u8,
    pub(super) checked_module_instance_fields: u8,
    pub(super) checked_module_marketplace_fields: u8,
    pub(super) checked_governance_fields: u8,
    pub(super) checked_factory_authority_fields: u8,
    pub(super) checked_main_token_policy_fields: u8,
    pub(super) checked_material_fields: u8,
    pub(super) checked_logistics_fields: u8,
    pub(super) checked_fixed_economy_fields: u8,
    pub(super) checked_agent_authority_fields: u8,
    pub(super) checked_reward_fields: u8,
    pub(super) checked_organization_fields: u8,
    pub(super) checked_supply_fields: u8,
    pub(super) checked_diplomacy_fields: u8,
}

impl UnsupportedWorldStateOwnersV1 {
    /// Digest of the exact canonical RootSet scanned by this diagnostic.
    /// This does not qualify the roots or their authority.
    pub fn root_set_digest(&self) -> [u8; 32] {
        self.root_set_digest
    }

    pub fn unsupported_fields(&self) -> &[&'static str] {
        self.unsupported_fields.as_slice()
    }

    pub fn runtime_registry_missing(&self) -> bool {
        self.runtime_registry_missing
    }

    pub fn cognition_registry_missing(&self) -> bool {
        self.cognition_registry_missing
    }

    pub fn checked_u64_fields(&self) -> u8 {
        self.checked_u64_fields
    }

    pub fn checked_id_set_fields(&self) -> u8 {
        self.checked_id_set_fields
    }

    pub fn checked_number_map_fields(&self) -> u8 {
        self.checked_number_map_fields
    }

    pub fn checked_text_byte_fields(&self) -> u8 {
        self.checked_text_byte_fields
    }

    pub fn checked_installed_target_fields(&self) -> u8 {
        self.checked_installed_target_fields
    }

    pub fn checked_world_balance_fields(&self) -> u8 {
        self.checked_world_balance_fields
    }

    pub fn checked_module_instance_fields(&self) -> u8 {
        self.checked_module_instance_fields
    }

    pub fn checked_module_marketplace_fields(&self) -> u8 {
        self.checked_module_marketplace_fields
    }

    pub fn checked_governance_fields(&self) -> u8 {
        self.checked_governance_fields
    }

    pub fn checked_factory_authority_fields(&self) -> u8 {
        self.checked_factory_authority_fields
    }

    pub fn checked_main_token_policy_fields(&self) -> u8 {
        self.checked_main_token_policy_fields
    }

    pub fn checked_material_fields(&self) -> u8 {
        self.checked_material_fields
    }

    pub fn checked_logistics_fields(&self) -> u8 {
        self.checked_logistics_fields
    }

    pub fn checked_fixed_economy_fields(&self) -> u8 {
        self.checked_fixed_economy_fields
    }

    pub fn checked_agent_authority_fields(&self) -> u8 {
        self.checked_agent_authority_fields
    }

    pub fn checked_reward_fields(&self) -> u8 {
        self.checked_reward_fields
    }

    pub fn checked_organization_fields(&self) -> u8 {
        self.checked_organization_fields
    }

    pub fn checked_supply_fields(&self) -> u8 {
        self.checked_supply_fields
    }

    pub fn checked_diplomacy_fields(&self) -> u8 {
        self.checked_diplomacy_fields
    }
}

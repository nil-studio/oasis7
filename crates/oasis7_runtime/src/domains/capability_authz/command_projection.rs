//! Owned authorization projection for a raw command-commit journal event.

use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

use crate::{
    CapabilityAuthorizationAuditReceipt, CapabilityAuthorizationEvent,
    CapabilityAuthorizationNonceRecord, CapabilityBudgetAccount, CapabilityEffectReceiptLink,
};
use crate::world::World;

pub(in crate::world) struct PreparedCapabilityCommandCommit {
    pub(in crate::world) event: CapabilityAuthorizationEvent,
    pub(in crate::world) capability_grants_v2: BTreeMap<String, JsonValue>,
    pub(in crate::world) capability_nonce_records: BTreeMap<String, CapabilityAuthorizationNonceRecord>,
    pub(in crate::world) capability_authorization_receipts:
        BTreeMap<String, CapabilityAuthorizationAuditReceipt>,
    pub(in crate::world) capability_budget_accounts: BTreeMap<String, CapabilityBudgetAccount>,
    pub(in crate::world) capability_effect_receipt_links: BTreeMap<String, CapabilityEffectReceiptLink>,
    pub(in crate::world) capability_authorization_root: String,
}

impl PreparedCapabilityCommandCommit {
    pub(in crate::world) fn matches_event(&self, event: &CapabilityAuthorizationEvent) -> bool {
        &self.event == event
    }

    pub(in crate::world) fn install(self, world: &mut World) {
        world.capability_grants_v2 = self.capability_grants_v2;
        world.capability_nonce_records = self.capability_nonce_records;
        world.capability_authorization_receipts = self.capability_authorization_receipts;
        world.capability_budget_accounts = self.capability_budget_accounts;
        world.capability_effect_receipt_links = self.capability_effect_receipt_links;
        world.capability_authorization_root = self.capability_authorization_root;
    }
}

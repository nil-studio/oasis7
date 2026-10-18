//! Owned authorization projection for a raw command-commit journal event.

use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

use super::super::{
    CapabilityAuthorizationAuditReceipt, CapabilityAuthorizationEvent,
    CapabilityAuthorizationNonceRecord, CapabilityBudgetAccount, CapabilityEffectReceiptLink,
};
use super::World;

pub(super) struct PreparedCapabilityCommandCommit {
    pub(super) event: CapabilityAuthorizationEvent,
    pub(super) capability_grants_v2: BTreeMap<String, JsonValue>,
    pub(super) capability_nonce_records: BTreeMap<String, CapabilityAuthorizationNonceRecord>,
    pub(super) capability_authorization_receipts:
        BTreeMap<String, CapabilityAuthorizationAuditReceipt>,
    pub(super) capability_budget_accounts: BTreeMap<String, CapabilityBudgetAccount>,
    pub(super) capability_effect_receipt_links: BTreeMap<String, CapabilityEffectReceiptLink>,
    pub(super) capability_authorization_root: String,
}

impl PreparedCapabilityCommandCommit {
    pub(super) fn matches_event(&self, event: &CapabilityAuthorizationEvent) -> bool {
        &self.event == event
    }

    pub(super) fn install(self, world: &mut World) {
        world.capability_grants_v2 = self.capability_grants_v2;
        world.capability_nonce_records = self.capability_nonce_records;
        world.capability_authorization_receipts = self.capability_authorization_receipts;
        world.capability_budget_accounts = self.capability_budget_accounts;
        world.capability_effect_receipt_links = self.capability_effect_receipt_links;
        world.capability_authorization_root = self.capability_authorization_root;
    }
}

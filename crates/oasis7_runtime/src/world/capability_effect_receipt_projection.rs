//! Owned authorization projection for a raw effect-receipt closure event.

use std::collections::BTreeMap;

use super::super::{
    CapabilityAuthorizationAuditReceipt, CapabilityAuthorizationEvent, CapabilityEffectReceiptLink,
};
use super::World;

pub(super) struct PreparedCapabilityEffectReceipt {
    pub(super) event: CapabilityAuthorizationEvent,
    pub(super) capability_authorization_receipts:
        BTreeMap<String, CapabilityAuthorizationAuditReceipt>,
    pub(super) capability_effect_receipt_links: BTreeMap<String, CapabilityEffectReceiptLink>,
    pub(super) capability_authorization_root: String,
}

impl PreparedCapabilityEffectReceipt {
    pub(super) fn matches_event(&self, event: &CapabilityAuthorizationEvent) -> bool {
        &self.event == event
    }

    pub(super) fn install(self, world: &mut World) {
        world.capability_authorization_receipts = self.capability_authorization_receipts;
        world.capability_effect_receipt_links = self.capability_effect_receipt_links;
        world.capability_authorization_root = self.capability_authorization_root;
    }
}

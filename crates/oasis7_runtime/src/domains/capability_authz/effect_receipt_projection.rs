//! Owned authorization projection for a raw effect-receipt closure event.

use std::collections::BTreeMap;

use crate::{
    CapabilityAuthorizationAuditReceipt, CapabilityAuthorizationEvent, CapabilityEffectReceiptLink,
};
use crate::world::World;

pub(in crate::world) struct PreparedCapabilityEffectReceipt {
    pub(in crate::world) event: CapabilityAuthorizationEvent,
    pub(in crate::world) capability_authorization_receipts:
        BTreeMap<String, CapabilityAuthorizationAuditReceipt>,
    pub(in crate::world) capability_effect_receipt_links: BTreeMap<String, CapabilityEffectReceiptLink>,
    pub(in crate::world) capability_authorization_root: String,
}

impl PreparedCapabilityEffectReceipt {
    pub(in crate::world) fn matches_event(&self, event: &CapabilityAuthorizationEvent) -> bool {
        &self.event == event
    }

    pub(in crate::world) fn install(self, world: &mut World) {
        world.capability_authorization_receipts = self.capability_authorization_receipts;
        world.capability_effect_receipt_links = self.capability_effect_receipt_links;
        world.capability_authorization_root = self.capability_authorization_root;
    }
}

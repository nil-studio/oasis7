use super::super::{CapabilityGrant, PolicySet, ReceiptSigner};
use super::World;

impl World {
    // ---------------------------------------------------------------------
    // Policy and capability management
    // ---------------------------------------------------------------------

    pub fn set_policy(&mut self, policy: PolicySet) {
        self.policies = policy;
    }

    pub fn add_capability(&mut self, grant: CapabilityGrant) {
        self.capabilities.insert(grant.name.clone(), grant);
    }

    pub fn set_receipt_signer(&mut self, signer: ReceiptSigner) {
        self.receipt_signer = Some(signer);
    }
}

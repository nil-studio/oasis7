//! Shared identities and escrow facts for industrial work and substitution.
use super::MaterialLedgerId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct IndustrialRefV1 {
    pub world_binding: String,
    pub domain: String,
    pub schema: String,
    pub id: String,
    pub revision: u64,
    pub digest: String,
}
/// A child and a revision use the same fully qualified identity envelope;
/// their role is fixed by the containing canonical record.
pub type ChildRefV1 = IndustrialRefV1;
pub type RevisionV1 = IndustrialRefV1;

impl IndustrialRefV1 {
    pub fn is_qualified(&self) -> bool {
        !self.world_binding.is_empty()
            && !self.domain.is_empty()
            && !self.schema.is_empty()
            && !self.id.is_empty()
            && self.revision > 0
            && self.digest.len() == 64
            && self.digest.bytes().all(|b| b.is_ascii_hexdigit())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndustrialLotV1 {
    pub lot_ref: IndustrialRefV1,
    pub owner_id: String,
    pub source_ledger: MaterialLedgerId,
    pub material: String,
    pub unit: String,
    pub spec: String,
    pub quality: String,
    pub custody: String,
    pub provenance_ref: IndustrialRefV1,
    pub quantity: u64,
    pub remaining: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndustrialParentV1 {
    pub lot_ref: IndustrialRefV1,
    pub settlement_ref: IndustrialRefV1,
    pub quantity: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndustrialInputSlotV1 {
    pub material_profile: oasis7_wasm_abi::SubstitutionProfileRefV1,
    pub material: String,
    pub unit: String,
    pub spec: String,
    pub quality: String,
    pub custody: String,
    pub quantity: u64,
    pub binding_revision: u64,
    pub parents: Vec<IndustrialParentV1>,
    pub consumed: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootOperationV1 {
    pub owner_binding_digest: String,
    pub allowed_dispositions: Vec<IndustrialDispositionV1>,
    pub output_ledger: MaterialLedgerId,
    /// Capacity in each exact material's native integer units; never cross-summed.
    pub terminal_capacities: BTreeMap<String, u64>,
    pub started_job_id: Option<u64>,
    pub root_ref: IndustrialRefV1,
    pub owner_id: String,
    pub factory_id: String,
    pub pinned_recipe_id: String,
    pub accepted_batches: u32,
    pub pinned_profile_digest: String,
    pub causal_digest: String,
    pub work: u64,
    pub first_sink: bool,
    pub slots: BTreeMap<String, IndustrialInputSlotV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParentBindingRevisionV1 {
    pub root_ref: IndustrialRefV1,
    pub slot_id: String,
    pub expected_revision: u64,
    pub decision_ref: IndustrialRefV1,
    pub parents: Vec<IndustrialParentV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndustrialReceiptV1 {
    pub receipt_ref: IndustrialRefV1,
    pub root_ref: IndustrialRefV1,
    pub child_ref: IndustrialRefV1,
    pub effect_role: String,
    pub payload_digest: String,
    pub input: Vec<IndustrialParentV1>,
    pub output: Vec<IndustrialRefV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispositionPlanV1 {
    pub root_ref: IndustrialRefV1,
    pub plan_ref: IndustrialRefV1,
    pub expected_causal_digest: String,
    pub disposition: IndustrialDispositionV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndustrialDispositionV1 {
    Hold,
    Terminate,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndustrialLotAuthorityV1 {
    pub owner_binding_digest: String,
    pub expires_at: u64,
    pub escrow_capacity: u64,
    pub material_profile: oasis7_wasm_abi::SubstitutionProfileRefV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct IndustrialWorkStateV1 {
    pub roots: BTreeMap<String, RootOperationV1>,
    pub lots: BTreeMap<String, IndustrialLotV1>,
    pub imported_lot_authorities: BTreeMap<String, IndustrialLotAuthorityV1>,
    pub receipts: BTreeMap<String, IndustrialReceiptV1>,
    pub sink_keys: BTreeSet<String>,
    pub bindings: Vec<ParentBindingRevisionV1>,
    pub dispositions: BTreeMap<String, DispositionPlanV1>,
}

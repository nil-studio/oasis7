//! Finite public starter offers and durable, owner-bound claim receipts.
use super::MaterialLedgerId;
use oasis7_wasm_abi::{FactoryProfileV1, MaterialStack};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const ORDINARY_STARTER_PROFILE_ID: &str = "ordinary-industrial-starter-v1";
pub const ORDINARY_STARTER_PROFILE_REVISION: u64 = 1;
pub const ORDINARY_STARTER_FACTORY_KIND: &str = "factory.smelter.mk1";
pub const ORDINARY_STARTER_RECIPE_ID: &str = "recipe.smelter.iron_ingot";

/// Initialization selects existing authorities; a player cannot create a slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrdinaryStarterSlotInputV1 {
    pub slot_id: String,
    pub site_id: String,
    pub factory_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrdinaryStarterSlotV1 {
    pub slot_id: String,
    pub site_id: String,
    pub factory_id: String,
    pub location_id: String,
    pub location_anchor_revision: u64,
    pub site_authority_revision: u64,
    pub factory_profile: FactoryProfileV1,
    pub construction_power_revision: u64,
    pub construction_electricity: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrdinaryStarterOfferV1 {
    pub offer_id: String,
    pub revision: u64,
    pub world_binding: String,
    pub supplier_agent_id: String,
    pub profile_id: String,
    pub profile_revision: u64,
    pub factory_kind: String,
    pub recipe_id: String,
    pub input_batches: u32,
    /// One canonical build plus two complete recipe inputs, including maintenance.
    pub materials: BTreeMap<String, i64>,
    pub recipe_electricity: i64,
    /// Pins the actual canonical recipe and maintenance profiles used for the package.
    pub dependency_digest: String,
    pub slots: BTreeMap<String, OrdinaryStarterSlotV1>,
    pub published_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", deny_unknown_fields)]
pub enum OrdinaryStarterCommandV1 {
    /// Only a trusted initialization event with action_id zero may publish.
    Publish { offer: OrdinaryStarterOfferV1 },
    Claim {
        offer_id: String,
        offer_revision: u64,
        slot_id: String,
        offer_digest: String,
        owner_binding: String,
        owner_generation: u64,
    },
}

impl OrdinaryStarterCommandV1 {
    pub fn authority_scope(&self) -> String {
        match self {
            Self::Publish { offer } => offer.offer_id.clone(),
            Self::Claim { offer_id, .. } => offer_id.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrdinaryStarterClaimReceiptV1 {
    pub receipt_key: String,
    pub request_id: String,
    pub actor_id: String,
    pub command: OrdinaryStarterCommandV1,
    pub committed_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrdinaryStarterFirstProductionV1 {
    pub owner_binding: String,
    pub agent_id: String,
    pub site_id: String,
    pub factory_id: String,
    pub profile_id: String,
    pub profile_revision: u64,
    pub recipe_id: String,
    pub job_id: u64,
    pub settled_at: u64,
    pub output_ledger: MaterialLedgerId,
    pub accepted_batches: u32,
    pub consume: Vec<MaterialStack>,
    pub power_required: i64,
    pub produce: Vec<MaterialStack>,
    pub byproducts: Vec<MaterialStack>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrdinaryStarterClaimV1 {
    pub world_binding: String,
    pub owner_binding: String,
    pub owner_generation: u64,
    pub agent_id: String,
    pub offer_id: String,
    pub offer_revision: u64,
    pub offer_digest: String,
    pub slot_id: String,
    pub site_id: String,
    pub factory_id: String,
    pub location_id: String,
    pub profile_id: String,
    pub profile_revision: u64,
    pub recipe_id: String,
    pub materials: BTreeMap<String, i64>,
    pub electricity: i64,
    pub receipt: OrdinaryStarterClaimReceiptV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_production: Option<OrdinaryStarterFirstProductionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OrdinaryStarterStateV1 {
    pub offers: BTreeMap<String, OrdinaryStarterOfferV1>,
    /// Stable world/genesis + owner binding, independent of credential generation.
    pub claims: BTreeMap<String, OrdinaryStarterClaimV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrdinaryStarterOfferStatusV1 {
    pub offer: OrdinaryStarterOfferV1,
    pub offer_digest: String,
    pub available_slots: Vec<String>,
    pub supplier_materials: BTreeMap<String, i64>,
    pub supplier_electricity: i64,
    pub blocker: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OrdinaryStarterStatusV1 {
    pub world_binding: Option<String>,
    pub eligible: bool,
    pub blocker: Option<String>,
    pub owner_binding: Option<String>,
    pub owner_generation: Option<u64>,
    pub offers: BTreeMap<String, OrdinaryStarterOfferStatusV1>,
    pub claim: Option<OrdinaryStarterClaimV1>,
}

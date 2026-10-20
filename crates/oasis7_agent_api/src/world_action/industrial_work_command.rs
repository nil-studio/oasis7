//! Certified native admission to the shared industrial escrow/work authority.
use super::{GovernanceFinalityCertificate, IndustrialRefV1, MaterialLedgerId};
use oasis7_wasm_abi::SubstitutionProfileRefV1;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualifiedIndustrialLotFactsV1 {
    pub world_binding: String,
    pub owner_id: String,
    pub owner_binding_digest: String,
    pub source_ledger: MaterialLedgerId,
    pub material: String,
    pub material_profile: SubstitutionProfileRefV1,
    pub unit: String,
    pub spec: String,
    pub quality: String,
    pub custody: String,
    pub provenance_ref: IndustrialRefV1,
    pub quantity: u64,
    pub escrow_capacity: u64,
    pub expires_at: u64,
    pub consensus_height: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndustrialInputBaselineV1 {
    pub material_profile: SubstitutionProfileRefV1,
    pub unit: String,
    pub spec: String,
    pub quality: String,
    pub custody: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndustrialBaselineAuthorityV1 {
    pub world_binding: String,
    pub owner_binding_digest: String,
    pub state_digest: String,
    pub profile_digest: String,
    pub plan_digest: String,
    pub baseline_impacts: oasis7_wasm_abi::SubstitutionImpactV1,
    pub allowed_dispositions: Vec<super::IndustrialDispositionV1>,
    pub terminal_capacities: BTreeMap<String, u64>,
    pub quoted_at: u64,
    pub expires_at: u64,
    pub consensus_height: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum IndustrialWorkCommandV1 {
    ImportQualifiedLot {
        facts: QualifiedIndustrialLotFactsV1,
        certificate: GovernanceFinalityCertificate,
    },
    AcceptRoot {
        factory_id: String,
        recipe_id: String,
        batches: u32,
        input_baselines: BTreeMap<String, IndustrialInputBaselineV1>,
        authority: IndustrialBaselineAuthorityV1,
        certificate: GovernanceFinalityCertificate,
    },
    StartRoot {
        root_ref: IndustrialRefV1,
        expected_profile_digest: String,
        logistics_route_ids: Vec<String>,
        logistics_path_ids: Vec<String>,
    },
    TerminateRoot {
        root_ref: IndustrialRefV1,
    },
    ReleaseLot {
        lot_ref: IndustrialRefV1,
    },
}
impl IndustrialWorkCommandV1 {
    pub fn authority_scope(&self) -> String {
        match self {
            Self::ImportQualifiedLot { facts, .. } => facts.source_ledger.to_string(),
            Self::AcceptRoot { factory_id, .. } => factory_id.clone(),
            Self::TerminateRoot { root_ref } => root_ref.id.clone(),
            Self::StartRoot { root_ref, .. } => root_ref.id.clone(),
            Self::ReleaseLot { lot_ref } => lot_ref.id.clone(),
        }
    }
}

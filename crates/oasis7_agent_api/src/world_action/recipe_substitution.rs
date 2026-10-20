//! Explicit, certified recipe substitution; a decision carries no economic effect.
use super::*;
use oasis7_wasm_abi::{SubstitutionImpactV1, SubstitutionProfileRefV1, SubstitutionQuantityPlanV1};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SubstitutionSelectionV1 {
    SameMaterialSource {
        source_lot: IndustrialRefV1,
        source_material: SubstitutionProfileRefV1,
    },
    ProfileRule {
        rule: SubstitutionProfileRefV1,
        source_lot: IndustrialRefV1,
        source_material: SubstitutionProfileRefV1,
    },
    NewRecipe {
        candidate_root: IndustrialRefV1,
    },
}
impl SubstitutionSelectionV1 {
    pub fn source(&self) -> Option<(&IndustrialRefV1, &SubstitutionProfileRefV1)> {
        match self {
            Self::SameMaterialSource {
                source_lot,
                source_material,
            }
            | Self::ProfileRule {
                source_lot,
                source_material,
                ..
            } => Some((source_lot, source_material)),
            Self::NewRecipe { .. } => None,
        }
    }
}
/// Certified opportunity-cost disclosure; never a currency balance or credit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubstitutionValuationV1 {
    pub unit: String,
    pub held: u64,
    pub consumed: u64,
    pub expected_output: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubstitutionExecutionAuthorityV1 {
    pub authority_ref: IndustrialRefV1,
    pub owner_id: String,
    pub owner_binding_digest: String,
    pub factory_digest: String,
    pub root_ref: IndustrialRefV1,
    pub slot_id: String,
    pub expected_binding_revision: u64,
    pub profile_digest: String,
    pub selection: SubstitutionSelectionV1,
    pub target_recipe: String,
    pub baseline_impacts: SubstitutionImpactV1,
    pub candidate_impacts: SubstitutionImpactV1,
    pub output_material: String,
    pub output_unit: String,
    pub output_spec: String,
    pub output_quality: String,
    pub output_custody: String,
    pub owner_ledger_path_policy: SubstitutionProfileRefV1,
    pub input_state: String,
    pub costs: BTreeMap<String, u64>,
    pub power_cost: u64,
    pub duration_ticks: u64,
    pub terminal_capacity: u64,
    pub valid_until: u64,
    pub valuation: SubstitutionValuationV1,
    pub risks: Vec<String>,
    pub next_recheck: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubstitutionPreviewRequestV1 {
    pub root_ref: IndustrialRefV1,
    pub slot_id: String,
    pub authority_refs: Vec<IndustrialRefV1>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SubstitutionCandidateKindV1 {
    SameMaterial,
    ProfileSubstitute,
    NewRecipe,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubstitutionCandidateV1 {
    pub candidate_digest: String,
    pub authority: SubstitutionExecutionAuthorityV1,
    pub rule: Option<oasis7_wasm_abi::SubstitutionRuleV1>,
    pub kind: SubstitutionCandidateKindV1,
    pub plan: Option<SubstitutionQuantityPlanV1>,
    pub native_source_quantity: Option<u64>,
    pub source_material_kind: Option<String>,
    pub candidate_root: Option<RootOperationV1>,
    pub cutover_plan: Option<DispositionPlanV1>,
    pub cutover_plan_digest: Option<String>,
    pub recipe_plan: Option<oasis7_wasm_abi::RecipeExecutionPlan>,
    pub causal_equivalent: bool,
    pub conversion_required: bool,
    pub source_remaining: u64,
    pub binding_revision: u64,
    pub resource_balances: BTreeMap<String, i64>,
}
impl SubstitutionCandidateV1 {
    pub fn source_required(&self) -> u64 {
        self.plan
            .as_ref()
            .map(|p| p.source_required)
            .or(self.native_source_quantity)
            .unwrap_or(0)
    }
    pub fn target_executable(&self) -> u64 {
        self.plan
            .as_ref()
            .map(|p| p.target_executable)
            .or(self.native_source_quantity)
            .unwrap_or(0)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SubstitutionPreviewStatusV1 {
    Available,
    NoLegalSubstitute,
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubstitutionPreviewV1 {
    pub preview_digest: String,
    pub status: SubstitutionPreviewStatusV1,
    pub blocker: Option<String>,
    pub next_action: String,
    pub next_recheck: u64,
    pub request: SubstitutionPreviewRequestV1,
    pub candidates: Vec<SubstitutionCandidateV1>,
    pub evaluated_at: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SubstitutionScopeV1 {
    DecisionOnly,
    DecisionAndExactConversion,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmSubstitutionV1 {
    pub preview: SubstitutionPreviewV1,
    pub candidate_digest: String,
    pub scope: SubstitutionScopeV1,
    pub maximum_source: u64,
    pub maximum_costs: BTreeMap<String, u64>,
    pub maximum_power: u64,
    pub cutover_plan_digest: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubstitutionDecisionV1 {
    pub decision_ref: IndustrialRefV1,
    pub owner_id: String,
    pub confirmation_key: String,
    pub intent: ConfirmSubstitutionV1,
    pub candidate: SubstitutionCandidateV1,
    pub confirmed_at: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversionSettlementV1 {
    pub decision_ref: IndustrialRefV1,
    pub conversion_ref: IndustrialRefV1,
    pub receipt_ref: IndustrialRefV1,
    pub output_lot: IndustrialRefV1,
    pub source_consumed: u64,
    pub source_loss: u64,
    pub residual_numerator: u64,
    pub residual_denominator: u64,
    pub committed_at: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubstitutionResidualBalanceV1 {
    pub owner_id: String,
    pub domain_key: String,
    pub source_lot: IndustrialRefV1,
    pub settlement_ref: IndustrialRefV1,
    pub unit: String,
    pub numerator: u64,
    pub denominator: u64,
    pub next_action: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SubstitutionStateV1 {
    pub authorities: BTreeMap<String, SubstitutionExecutionAuthorityV1>,
    pub decisions: BTreeMap<String, SubstitutionDecisionV1>,
    pub settlements: BTreeMap<String, ConversionSettlementV1>,
    pub conversion_domains: BTreeSet<String>,
    pub terminal_used: BTreeMap<String, u64>,
    pub retained_residuals: BTreeMap<String, SubstitutionResidualBalanceV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum RecipeSubstitutionCommandV1 {
    PublishAuthority {
        authority: Box<SubstitutionExecutionAuthorityV1>,
        consensus_height: u64,
        finality_certificate: GovernanceFinalityCertificate,
    },
    Confirm {
        confirmation: ConfirmSubstitutionV1,
    },
    Convert {
        decision_ref: IndustrialRefV1,
    },
    ApplyBinding {
        decision_ref: IndustrialRefV1,
    },
    Cutover {
        decision_ref: IndustrialRefV1,
        disposition: DispositionPlanV1,
        candidate_root: Box<RootOperationV1>,
    },
}

impl RecipeSubstitutionCommandV1 {
    pub fn authority_scope(&self) -> String {
        match self {
            Self::PublishAuthority { authority, .. } => authority.root_ref.id.clone(),
            Self::Confirm { confirmation } => confirmation.preview.request.root_ref.id.clone(),
            Self::Convert { decision_ref }
            | Self::ApplyBinding { decision_ref }
            | Self::Cutover { decision_ref, .. } => decision_ref.id.clone(),
        }
    }
}

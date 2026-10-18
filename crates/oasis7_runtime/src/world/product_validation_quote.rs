use oasis7_wasm_abi::MaterialStack;

use super::super::IndustryStage;
use super::World;
use super::bootstrap_economy::m4_default_product_profiles;

const PRODUCT_VALIDATION_QUOTE_MISSING: &str = "product_validation_quote_missing";

/// Deterministic player-facing preflight for a product validation action.
///
/// This quote derives only from the governed product profile and the current
/// industry stage. It deliberately does not execute the product module or
/// append any event, so it is safe to request repeatedly before submission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductValidationQuote {
    pub product_id: String,
    pub product_role: String,
    pub tradable: bool,
    pub stage_before: String,
    pub stage_after: String,
    pub unlock_or_value_class: String,
    pub recommended_action: String,
    pub submission_allowed: bool,
    pub missing_prerequisite: String,
    pub reachable_advance_or_recovery: String,
}

impl World {
    pub fn product_validation_quote(
        &self,
        _requester_agent_id: &str,
        module_id: &str,
        stack: &MaterialStack,
        _deterministic_seed: u64,
    ) -> Result<ProductValidationQuote, &'static str> {
        if module_id.trim().is_empty() || stack.amount <= 0 {
            return Err(PRODUCT_VALIDATION_QUOTE_MISSING);
        }
        let profile = self
            .product_profile(stack.kind.as_str())
            .cloned()
            .or_else(|| {
                m4_default_product_profiles()
                    .into_iter()
                    .find(|profile| profile.product_id == stack.kind)
            })
            .ok_or(PRODUCT_VALIDATION_QUOTE_MISSING)?;

        let stage_before = industry_stage_label(self.state.industry_progress.stage);
        let required_stage = parse_industry_stage(profile.unlock_stage.as_str());
        let stage_is_met = required_stage
            .is_none_or(|required_stage| self.state.industry_progress.stage >= required_stage);
        let (recommended_action, missing_prerequisite, reachable_advance_or_recovery) =
            if stage_is_met {
                (
                    "validate_product_with_module".to_string(),
                    String::new(),
                    String::new(),
                )
            } else {
                (
                    "advance_industry_stage".to_string(),
                    format!("industry_stage={}", profile.unlock_stage),
                    "complete_reachable_industry_progress".to_string(),
                )
            };

        Ok(ProductValidationQuote {
            product_id: profile.product_id,
            product_role: profile.role_tag,
            tradable: profile.tradable,
            stage_before: stage_before.to_string(),
            // Product validation itself records validation state but does not
            // advance industry progression, matching the submit-time event flow.
            stage_after: stage_before.to_string(),
            unlock_or_value_class: profile.unlock_stage.clone(),
            recommended_action,
            // Keep this aligned with the submit-time syntactic checks. Stage
            // guidance is informational until product validation enforces an
            // unlock-stage rejection through the module action contract.
            submission_allowed: true,
            missing_prerequisite,
            reachable_advance_or_recovery,
        })
    }
}

fn industry_stage_label(stage: IndustryStage) -> &'static str {
    match stage {
        IndustryStage::Bootstrap => "bootstrap",
        IndustryStage::ScaleOut => "scale_out",
        IndustryStage::Governance => "governance",
    }
}

fn parse_industry_stage(raw: &str) -> Option<IndustryStage> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "bootstrap" => Some(IndustryStage::Bootstrap),
        "scale_out" | "scaleout" | "scale-out" => Some(IndustryStage::ScaleOut),
        "governance" => Some(IndustryStage::Governance),
        _ => None,
    }
}

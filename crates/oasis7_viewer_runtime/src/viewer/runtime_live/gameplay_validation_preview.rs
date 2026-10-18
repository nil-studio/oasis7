use crate::runtime::{IndustryStage, WorldState};
use crate::runtime_host::archive::ProductValidationUnlockPreview;

fn stage_label(stage: IndustryStage) -> &'static str {
    match stage {
        IndustryStage::Bootstrap => "bootstrap",
        IndustryStage::ScaleOut => "scale_out",
        IndustryStage::Governance => "governance",
    }
}

fn parse_required_stage(raw: &str) -> Option<IndustryStage> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "bootstrap" => Some(IndustryStage::Bootstrap),
        "scale_out" | "scaleout" | "scale-out" => Some(IndustryStage::ScaleOut),
        "governance" => Some(IndustryStage::Governance),
        _ => None,
    }
}

pub(super) fn product_validation_unlock_preview(
    state: &WorldState,
) -> Option<ProductValidationUnlockPreview> {
    let validation = state.latest_product_validation.as_ref()?;
    let current_stage = stage_label(state.industry_progress.stage).to_string();
    let Some(profile) = state.product_profiles.get(validation.product_id.as_str()) else {
        return Some(ProductValidationUnlockPreview {
            product_id: validation.product_id.clone(),
            role_tag: "unknown".to_string(),
            tradable: validation.tradable,
            required_stage: "unknown".to_string(),
            current_stage,
            stage_status: "unknown".to_string(),
            value_summary: "Validated product has no governed role profile.".to_string(),
            next_step_hint: "Inspect product use before relying on this validation.".to_string(),
        });
    };
    let required_stage = parse_required_stage(profile.unlock_stage.as_str());
    let required_stage_label = required_stage
        .map(stage_label)
        .unwrap_or("none")
        .to_string();
    let stage_status = match required_stage {
        Some(required) if state.industry_progress.stage < required => "denied",
        Some(_) | None => "available",
    };
    let role_tag = if profile.role_tag.trim().is_empty() {
        "unknown".to_string()
    } else {
        profile.role_tag.clone()
    };
    let (value_summary, next_step_hint) = match stage_status {
        "available" => (
            format!(
                "Validated {role_tag} product; {}.",
                if validation.tradable {
                    "trading enabled"
                } else {
                    "trading disabled"
                }
            ),
            format!(
                "Use this product in its {role_tag} role; validation unlocks no new capability."
            ),
        ),
        "denied" => (
            format!("Validated {role_tag} product remains gated by stage {required_stage_label}."),
            format!(
                "Advance industry from {current_stage} to {required_stage_label}; validation unlocks no new capability."
            ),
        ),
        _ => (
            format!("Validated {role_tag} product has an unknown stage requirement."),
            "Inspect the governed product profile before relying on this validation.".to_string(),
        ),
    };
    Some(ProductValidationUnlockPreview {
        product_id: validation.product_id.clone(),
        role_tag,
        tradable: validation.tradable,
        required_stage: required_stage_label,
        current_stage,
        stage_status: stage_status.to_string(),
        value_summary,
        next_step_hint,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{DomainEvent, MaterialStack, ProductProfileV1};

    fn preview_for_gate(unlock_stage: &str) -> ProductValidationUnlockPreview {
        let mut state = WorldState::default();
        state
            .apply_domain_event(
                &DomainEvent::ProductValidated {
                    requester_agent_id: "agent-1".to_string(),
                    module_id: "module-1".to_string(),
                    stack: MaterialStack::new("validated_product", 1),
                    stack_limit: 1,
                    tradable: true,
                    quality_levels: Vec::new(),
                    notes: Vec::new(),
                },
                0,
            )
            .expect("accepted validation");
        state.product_profiles.insert(
            "validated_product".to_string(),
            ProductProfileV1 {
                product_id: "validated_product".to_string(),
                role_tag: "scale".to_string(),
                maintenance_sink: Vec::new(),
                tradable: true,
                unlock_stage: unlock_stage.to_string(),
            },
        );

        product_validation_unlock_preview(&state).expect("preview for validated product")
    }

    #[test]
    fn canonicalizes_runtime_accepted_scale_out_aliases() {
        for gate in [" Scale-Out ", "scaleout", "SCALE_OUT"] {
            let preview = preview_for_gate(gate);
            assert_eq!(preview.required_stage, "scale_out", "gate={gate}");
            assert_eq!(preview.stage_status, "denied", "gate={gate}");
        }
    }

    #[test]
    fn treats_blank_runtime_gate_as_available_without_required_stage() {
        let preview = preview_for_gate("  ");

        assert_eq!(preview.required_stage, "none");
        assert_eq!(preview.stage_status, "available");
    }
}

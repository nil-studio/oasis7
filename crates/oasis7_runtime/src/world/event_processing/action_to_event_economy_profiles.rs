use super::*;

impl World {
    pub(super) fn profile_governance_event(
        &self,
        action_id: ActionId,
        action: &Action,
    ) -> Option<DomainEvent> {
        match action {
            Action::GovernMaterialProfile {
                operator_agent_id,
                proposal_id,
                profile,
            } => Some(self.evaluate_govern_material_profile_action(
                action_id,
                operator_agent_id.as_str(),
                *proposal_id,
                profile,
            )),
            Action::GovernProductProfile {
                operator_agent_id,
                proposal_id,
                profile,
            } => Some(self.evaluate_govern_product_profile_action(
                action_id,
                operator_agent_id.as_str(),
                *proposal_id,
                profile,
            )),
            Action::GovernRecipeProfile {
                operator_agent_id,
                proposal_id,
                profile,
            } => Some(self.evaluate_govern_recipe_profile_action(
                action_id,
                operator_agent_id.as_str(),
                *proposal_id,
                profile,
            )),
            Action::GovernFactoryProfile {
                operator_agent_id,
                proposal_id,
                profile,
            } => Some(self.evaluate_govern_factory_profile_action(
                action_id,
                operator_agent_id.as_str(),
                *proposal_id,
                profile,
            )),
            _ => None,
        }
    }

    pub(super) fn evaluate_govern_material_profile_action(
        &self,
        action_id: ActionId,
        operator_agent_id: &str,
        proposal_id: ProposalId,
        profile: &crate::MaterialProfileV1,
    ) -> DomainEvent {
        if let Some(rejected) = self.evaluate_profile_governance_gate(
            action_id,
            operator_agent_id,
            proposal_id,
            "govern material profile",
        ) {
            return rejected;
        }
        let allowed_fields = [
            "kind",
            "tier",
            "category",
            "stack_limit",
            "transport_loss_class",
            "decay_bps_per_tick",
            "default_priority",
        ];
        if let Err(reason) =
            ensure_profile_field_whitelist(profile, allowed_fields.as_slice(), "material profile")
        {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![reason],
                },
            };
        }
        let event = DomainEvent::MaterialProfileGoverned {
            operator_agent_id: operator_agent_id.to_string(),
            proposal_id,
            profile: profile.clone(),
        };
        let mut preview_state = self.state.clone();
        if let Err(err) = preview_state.apply_domain_event(&event, self.state.time) {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!("govern material profile rejected: {err:?}")],
                },
            };
        }
        event
    }

    pub(super) fn evaluate_govern_product_profile_action(
        &self,
        action_id: ActionId,
        operator_agent_id: &str,
        proposal_id: ProposalId,
        profile: &crate::ProductProfileV1,
    ) -> DomainEvent {
        if let Some(rejected) = self.evaluate_profile_governance_gate(
            action_id,
            operator_agent_id,
            proposal_id,
            "govern product profile",
        ) {
            return rejected;
        }
        let allowed_fields = [
            "product_id",
            "role_tag",
            "maintenance_sink",
            "tradable",
            "unlock_stage",
        ];
        if let Err(reason) =
            ensure_profile_field_whitelist(profile, allowed_fields.as_slice(), "product profile")
        {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![reason],
                },
            };
        }
        let event = DomainEvent::ProductProfileGoverned {
            operator_agent_id: operator_agent_id.to_string(),
            proposal_id,
            profile: profile.clone(),
        };
        let mut preview_state = self.state.clone();
        if let Err(err) = preview_state.apply_domain_event(&event, self.state.time) {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!("govern product profile rejected: {err:?}")],
                },
            };
        }
        event
    }

    pub(super) fn evaluate_govern_recipe_profile_action(
        &self,
        action_id: ActionId,
        operator_agent_id: &str,
        proposal_id: ProposalId,
        profile: &crate::RecipeProfileV1,
    ) -> DomainEvent {
        if let Some(rejected) = self.evaluate_profile_governance_gate(
            action_id,
            operator_agent_id,
            proposal_id,
            "govern recipe profile",
        ) {
            return rejected;
        }
        let allowed_fields = [
            "recipe_id",
            "bottleneck_tags",
            "stage_gate",
            "preferred_factory_tags",
        ];
        if let Err(reason) =
            ensure_profile_field_whitelist(profile, allowed_fields.as_slice(), "recipe profile")
        {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![reason],
                },
            };
        }
        let event = DomainEvent::RecipeProfileGoverned {
            operator_agent_id: operator_agent_id.to_string(),
            proposal_id,
            profile: profile.clone(),
        };
        let mut preview_state = self.state.clone();
        if let Err(err) = preview_state.apply_domain_event(&event, self.state.time) {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!("govern recipe profile rejected: {err:?}")],
                },
            };
        }
        event
    }

    pub(super) fn evaluate_govern_factory_profile_action(
        &self,
        action_id: ActionId,
        operator_agent_id: &str,
        proposal_id: ProposalId,
        profile: &crate::FactoryProfileV1,
    ) -> DomainEvent {
        if let Some(rejected) = self.evaluate_profile_governance_gate(
            action_id,
            operator_agent_id,
            proposal_id,
            "govern factory profile",
        ) {
            return rejected;
        }
        let allowed_fields = ["factory_id", "tier", "recipe_slots", "tags"];
        if let Err(reason) =
            ensure_profile_field_whitelist(profile, allowed_fields.as_slice(), "factory profile")
        {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![reason],
                },
            };
        }
        let event = DomainEvent::FactoryProfileGoverned {
            operator_agent_id: operator_agent_id.to_string(),
            proposal_id,
            profile: profile.clone(),
        };
        let mut preview_state = self.state.clone();
        if let Err(err) = preview_state.apply_domain_event(&event, self.state.time) {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!("govern factory profile rejected: {err:?}")],
                },
            };
        }
        event
    }

    pub(super) fn evaluate_profile_governance_gate(
        &self,
        action_id: ActionId,
        operator_agent_id: &str,
        proposal_id: ProposalId,
        action_label: &str,
    ) -> Option<DomainEvent> {
        if !self.state.agents.contains_key(operator_agent_id) {
            return Some(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::AgentNotFound {
                    agent_id: operator_agent_id.to_string(),
                },
            });
        }
        if proposal_id == 0 {
            return Some(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!("{action_label} rejected: proposal_id must be > 0")],
                },
            });
        }
        let Some(proposal) = self.proposals.get(&proposal_id) else {
            return Some(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "{action_label} rejected: governance proposal not found ({proposal_id})"
                    )],
                },
            });
        };
        match proposal.status {
            ProposalStatus::Approved { .. } | ProposalStatus::Applied { .. } => None,
            _ => Some(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "{action_label} rejected: governance proposal must be approved or applied ({proposal_id})"
                    )],
                },
            }),
        }
    }
}

use super::*;
pub(super) fn action(action: &crate::Action) -> Option<(&str, u64, &'static str)> {
    Some(match action {
        crate::Action::GovernMaterialProfile {
            operator_agent_id,
            proposal_id,
            ..
        } => (operator_agent_id, *proposal_id, "govern material profile"),
        crate::Action::GovernProductProfile {
            operator_agent_id,
            proposal_id,
            ..
        } => (operator_agent_id, *proposal_id, "govern product profile"),
        crate::Action::GovernRecipeProfile {
            operator_agent_id,
            proposal_id,
            ..
        } => (operator_agent_id, *proposal_id, "govern recipe profile"),
        crate::Action::GovernFactoryProfile {
            operator_agent_id,
            proposal_id,
            ..
        } => (operator_agent_id, *proposal_id, "govern factory profile"),
        _ => return None,
    })
}
pub(super) fn operator(event: &crate::WorldEvent) -> Option<&str> {
    match &event.body {
        crate::WorldEventBody::Domain(body) => domain_operator(body),
        _ => None,
    }
}
pub(super) fn domain_operator(body: &crate::DomainEvent) -> Option<&str> {
    match body {
        crate::DomainEvent::MaterialProfileGoverned {
            operator_agent_id, ..
        }
        | crate::DomainEvent::ProductProfileGoverned {
            operator_agent_id, ..
        }
        | crate::DomainEvent::RecipeProfileGoverned {
            operator_agent_id, ..
        }
        | crate::DomainEvent::FactoryProfileGoverned {
            operator_agent_id, ..
        } => Some(operator_agent_id),
        _ => None,
    }
}
pub(super) fn event_matches(event: &crate::WorldEvent) -> bool {
    operator(event).is_some()
}
pub(super) fn reserve_phase(
    action: &crate::Action,
    work: &mut ProfileGovernanceWorkV1,
    memory: &MemoryBudget,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<MemoryReservation, BodyMutationError> {
    struct Count<'a, C> {
        work: &'a mut ProfileGovernanceWorkV1,
        bytes: usize,
        error: Option<BodyMutationError>,
        cancelled: &'a mut C,
    }
    impl<C: FnMut() -> bool> std::io::Write for Count<'_, C> {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            if (self.cancelled)() {
                self.error = Some(ValueTreeError::Cancelled.into());
                return Err(std::io::ErrorKind::Other.into());
            }
            if let Err(e) = self.work.spend(b.len()) {
                self.error = Some(e);
                return Err(std::io::ErrorKind::Other.into());
            }
            self.bytes = self
                .bytes
                .checked_add(b.len())
                .ok_or(std::io::ErrorKind::Other)?;
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let _measure = memory
        .reserve(4096, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut count = Count {
        work,
        bytes: 0,
        error: None,
        cancelled,
    };
    let encoded = serde_json::to_writer(&mut count, action);
    if let Some(e) = count.error {
        return Err(e);
    }
    encoded.map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let bytes = count
        .bytes
        .checked_mul(128)
        .and_then(|n| n.checked_add(32 << 10))
        .ok_or(ValueTreeError::ResourceLimited("profile phase peak"))?;
    work.spend(bytes)?;
    memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(|e| ValueTreeError::Memory(e).into())
}
pub(super) fn profile(
    action_id: u64,
    action: &crate::Action,
    existing_factory: Option<&oasis7_wasm_abi::FactoryProfileV1>,
) -> Result<crate::DomainEvent, BodyMutationError> {
    use crate::world::event_processing::action_to_event_economy::ensure_profile_field_whitelist as whitelist;
    let (operator, proposal, label) =
        self::action(action).ok_or(BodyMutationError::Invalid("profile effective variant"))?;
    let rejected = |reason: String| crate::DomainEvent::ActionRejected {
        action_id,
        reason: crate::RejectReason::RuleDenied {
            notes: vec![reason],
        },
    };
    let result = match action {
        crate::Action::GovernMaterialProfile { profile, .. } => {
            if let Err(e) = whitelist(
                profile,
                &[
                    "kind",
                    "tier",
                    "category",
                    "stack_limit",
                    "transport_loss_class",
                    "decay_bps_per_tick",
                    "default_priority",
                ],
                "material profile",
            ) {
                return Ok(rejected(e));
            }
            oasis7_world_state::state::core_policy_transition::validate_material_profile(
                proposal, profile,
            )
            .map(|_| crate::DomainEvent::MaterialProfileGoverned {
                operator_agent_id: operator.to_owned(),
                proposal_id: proposal,
                profile: profile.clone(),
            })
        }
        crate::Action::GovernProductProfile { profile, .. } => {
            if let Err(e) = whitelist(
                profile,
                &[
                    "product_id",
                    "role_tag",
                    "maintenance_sink",
                    "tradable",
                    "unlock_stage",
                ],
                "product profile",
            ) {
                return Ok(rejected(e));
            }
            oasis7_world_state::state::module_release_transition::validate_governed_product_profile_v1(profile).map(|_|crate::DomainEvent::ProductProfileGoverned{operator_agent_id:operator.to_owned(),proposal_id:proposal,profile:profile.clone()})
        }
        crate::Action::GovernRecipeProfile { profile, .. } => {
            if let Err(e) = whitelist(
                profile,
                &[
                    "recipe_id",
                    "bottleneck_tags",
                    "stage_gate",
                    "preferred_factory_tags",
                    "substitution_rules",
                ],
                "recipe profile",
            ) {
                return Ok(rejected(e));
            }
            oasis7_world_state::state::module_release_transition::validate_governed_recipe_profile_v1(profile).map(|_|crate::DomainEvent::RecipeProfileGoverned{operator_agent_id:operator.to_owned(),proposal_id:proposal,profile:profile.clone()})
        }
        crate::Action::GovernFactoryProfile { profile, .. } => {
            if let Err(e) = whitelist(
                profile,
                &["factory_id", "tier", "recipe_slots", "tags"],
                "factory profile",
            ) {
                return Ok(rejected(e));
            }
            oasis7_world_state::state::module_release_transition::validate_governed_factory_profile_v1(profile,existing_factory).map(|_|crate::DomainEvent::FactoryProfileGoverned{operator_agent_id:operator.to_owned(),proposal_id:proposal,profile:profile.clone()})
        }
        _ => return Err(BodyMutationError::Invalid("profile effective variant")),
    };
    Ok(result.unwrap_or_else(|e| rejected(format!("{label} rejected: {e:?}"))))
}

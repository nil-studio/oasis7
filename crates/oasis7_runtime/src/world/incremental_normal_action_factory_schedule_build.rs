//! Sole original BuildFactory admission kernel, fed by sealed read sources.
use super::factory_schedule_sources::FactoryBuildFactsV1;
use super::*;
use crate::{FACTORY_BUILD_STARTED_MODERN_VERSION, FactoryBuildPowerObligationV1};
use oasis7_wasm_abi::FactoryModuleSpec;
#[derive(Clone, Copy)]
pub(in crate::world) enum BuildMaterialCheckV1 {
    Required,
    LegacyModulePreview,
}
pub(in crate::world) fn evaluate_build_factory_v1<'source, S: FactoryBuildFactsV1<'source>>(
    source: &mut S,
    action_id: ActionId,
    builder_agent_id: &String,
    site_id: &String,
    spec: &FactoryModuleSpec,
    materials: BuildMaterialCheckV1,
) -> Result<WorldEventBody, S::Error> {
    let validate_materials = matches!(materials, BuildMaterialCheckV1::Required);
    source.reserve_action_output(builder_agent_id, site_id, spec)?;
    if !source.agent_exists(builder_agent_id)? {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::AgentNotFound {
                agent_id: builder_agent_id.clone(),
            },
        }));
    }
    if spec.factory_id.trim().is_empty() {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::RuleDenied {
                notes: vec!["factory_id cannot be empty".to_string()],
            },
        }));
    }
    if source.retired(&spec.factory_id)? {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::RuleDenied {
                notes: vec![format!("factory identity is retired: {}", spec.factory_id)],
            },
        }));
    }
    if source.factory_exists(&spec.factory_id)? || source.any_pending_factory(&spec.factory_id)? {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::RuleDenied {
                notes: vec![format!("factory already exists: {}", spec.factory_id)],
            },
        }));
    }
    if let Some(reason) = source.starter_assembler_blocker(spec.factory_id.as_str())? {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::RuleDenied {
                notes: vec![reason],
            },
        }));
    }
    if spec.recipe_slots == 0 {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::RuleDenied {
                notes: vec!["recipe_slots must be > 0".to_string()],
            },
        }));
    }
    let Some(factory_profile) = source.factory_profile(&spec.factory_id)? else {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::RuleDenied {
                notes: vec![format!(
                    "factory profile unknown: factory_id={}",
                    spec.factory_id
                )],
            },
        }));
    };
    if let Err(reason) =
        super::action_to_event_economy_support::factory_profile_matches_spec(&factory_profile, spec)
    {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::RuleDenied {
                notes: vec![reason],
            },
        }));
    }
    let Some(site_authority) = source.site(site_id)? else {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::RuleDenied {
                notes: vec![format!("site_unknown: site is not registered: {site_id}")],
            },
        }));
    };
    let Some(location_authority) = source.location(builder_agent_id)? else {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::RuleDenied {
                notes: vec![format!(
                    "builder location authority is unknown: builder_agent_id={builder_agent_id}"
                )],
            },
        }));
    };
    let location_anchor_revision =
        match source.anchor_revision(site_authority.location_id.as_str())? {
            Ok(revision) => revision,
            Err(WorldError::ResourceBalanceInvalid { reason }) => {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![reason],
                    },
                }));
            }
            Err(error) => return Err(error.into()),
        };
    let authorized = site_authority.owner_agent_id == *builder_agent_id
        || site_authority
            .authorized_agent_ids
            .iter()
            .any(|agent_id| agent_id == builder_agent_id);
    if site_authority.site_id != *site_id
        || location_authority.agent_id != *builder_agent_id
        || !site_authority.active
        || !site_authority.chunk_ready
        || site_authority.location_id.trim().is_empty()
        || site_authority.authority_revision == 0
        || !location_authority.active
        || location_authority.authority_revision == 0
        || location_authority.location_id != site_authority.location_id
        || location_authority.effective_at > source.now()
        || !authorized
    {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::RuleDenied {
                notes: vec![format!(
                    "site_access_or_location_blocked: site={} builder={} site_active={} chunk_ready={} site_location={} builder_location={} builder_location_active={} authorized={}",
                    site_id,
                    builder_agent_id,
                    site_authority.active,
                    site_authority.chunk_ready,
                    site_authority.location_id,
                    location_authority.location_id,
                    location_authority.active,
                    authorized
                )],
            },
        }));
    }
    let Some(power_profile) = source.power_profile(spec.factory_id.as_str())? else {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::RuleDenied {
                notes: vec![format!(
                    "construction power profile unknown: factory_id={}",
                    spec.factory_id
                )],
            },
        }));
    };
    if power_profile.factory_id != spec.factory_id
        || !power_profile.active
        || power_profile.authority_revision == 0
        || power_profile.electricity_amount < 0
    {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::RuleDenied {
                notes: vec![format!(
                    "construction power profile inactive_or_stale: factory_id={} revision={} active={}",
                    spec.factory_id, power_profile.authority_revision, power_profile.active
                )],
            },
        }));
    }
    let available_power = source.electricity(builder_agent_id)?;
    if available_power < power_profile.electricity_amount {
        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id,
            reason: RejectReason::InsufficientResource {
                agent_id: builder_agent_id.clone(),
                kind: ResourceKind::Electricity,
                requested: power_profile.electricity_amount,
                available: available_power,
            },
        }));
    }
    // Direct builds use the builder ledger; module fallback is resolved upstream.
    let consume_ledger = MaterialLedgerId::agent(builder_agent_id.clone());
    let (material_balances_before, material_balances_after) = if validate_materials {
        source.reserve_material_balances(&spec.build_cost)?;
        let required_materials =
            match super::action_to_event_economy_support::aggregate_material_stacks_for_admission(
                "factory build_cost",
                &spec.build_cost,
            ) {
                Ok(required_materials) => required_materials,
                Err(reason) => {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![reason],
                        },
                    }));
                }
            };
        let mut material_balances_before = BTreeMap::new();
        let mut material_balances_after = BTreeMap::new();
        for (material_kind, requested) in &required_materials {
            source.charge(material_kind.len())?;
            let available = source.material(&consume_ledger, material_kind.as_str())?;
            if available < *requested {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::InsufficientMaterial {
                        material_kind: material_kind.clone(),
                        requested: *requested,
                        available,
                    },
                }));
            }
            material_balances_before.insert(material_kind.clone(), available);
            material_balances_after
                .insert(material_kind.clone(), available.saturating_sub(*requested));
        }
        (material_balances_before, material_balances_after)
    } else {
        (BTreeMap::new(), BTreeMap::new())
    };

    let construction_power_obligation = FactoryBuildPowerObligationV1 {
        payer_agent_id: builder_agent_id.clone(),
        profile_key: spec.factory_id.clone(),
        profile_revision: power_profile.authority_revision,
        electricity_amount: power_profile.electricity_amount,
        mode: power_profile.mode,
        material_ledger: Some(consume_ledger.clone()),
        electricity_before: Some(available_power),
        electricity_after: Some(available_power.saturating_sub(power_profile.electricity_amount)),
        material_balances_before: Some(material_balances_before),
        material_balances_after: Some(material_balances_after),
    };

    let build_ticks = spec.build_time_ticks.max(1);
    let ready_at = source.now().saturating_add(build_ticks as u64);
    Ok(WorldEventBody::Domain(DomainEvent::FactoryBuildStarted {
        job_id: action_id,
        builder_agent_id: builder_agent_id.clone(),
        site_id: site_id.clone(),
        spec: spec.clone(),
        consume_ledger,
        ready_at,
        contract_version: Some(FACTORY_BUILD_STARTED_MODERN_VERSION),
        site_authority_revision: Some(site_authority.authority_revision),
        site_location_id: Some(site_authority.location_id.clone()),
        location_anchor_revision: Some(location_anchor_revision),
        construction_power_obligation: Some(construction_power_obligation),
    }))
}

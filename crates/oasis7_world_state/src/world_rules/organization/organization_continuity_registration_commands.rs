use super::*;

#[allow(clippy::too_many_arguments)]

pub(super) fn apply_continuity_registration_command(
    state: &WorldState,
    rules: &mut WorldRulesStateV1,
    continuity: &mut OrganizationContinuityStateV1,
    actor: &str,
    key: &str,
    command: &Command,
    now: u64,
    world_binding: &str,
) -> Result<String, WorldError> {
    match command {
        Command::Enable {
            organization_id,
            expected_charter_hash,
            governance_proposal_key,
            designated_successors,
            neutral_maintainers,
            appointment_authorities,
            remedy_adjudicators,
            residual_share_bps,
        } => {
            let alliance = state
                .alliances
                .get(organization_id)
                .ok_or_else(|| invalid("organization_missing"))?;
            if actor != alliance.formed_by_agent_id {
                return Err(invalid("organization_founder_confirmation_required"));
            }
            if continuity.registrations.contains_key(organization_id)
                || rules.organization_cases.values().any(|case| {
                    case.organization_id == *organization_id
                        && case.phase
                            != super::super::super::OrganizationProtectionPhaseV1::Resolved
                })
            {
                return Err(invalid(
                    "organization_continuity_already_enabled_or_legacy_case_open",
                ));
            }
            let parsed = parse_charter(&alliance.charter)?;
            let charter_hash = crate::util::sha256_hex(alliance.charter.as_bytes());
            if &charter_hash != expected_charter_hash
                || parsed.designated_successors != *designated_successors
                || parsed.neutral_maintainers != *neutral_maintainers
                || parsed.appointment_authorities != *appointment_authorities
                || parsed.remedy_adjudicators != *remedy_adjudicators
                || parsed.residual_share_bps != *residual_share_bps
            {
                return Err(invalid("organization_charter_binding_mismatch"));
            }
            validate_charter_authorities(state, alliance, &parsed)?;
            let controller_bindings = charter_controller_bindings(state, alliance, &parsed)?;
            require_continuity_governance(state, actor, governance_proposal_key, command)?;
            let assets = manifest_for_organization(state, rules, organization_id)?;
            if assets.len() > MAX_ASSETS_PER_REGISTRATION {
                return Err(invalid("organization_manifest_over_limit"));
            }
            let manifest_root = manifest_commitment_root(&assets)?;
            for asset in &assets {
                continuity
                    .assets
                    .insert(asset.asset_ref.clone(), asset.clone());
                if let Some(effect) = &asset.effect_receipt {
                    continuity
                        .consumed_effects
                        .insert(effect.clone(), asset.asset_ref.clone());
                }
            }
            let registration_receipt = key.to_owned();
            let members = alliance.members.iter().cloned().collect::<BTreeSet<_>>();
            let history = continuity
                .membership_history
                .entry(organization_id.clone())
                .or_default();
            for member in members {
                history
                    .entry(member)
                    .or_default()
                    .push((now, None, registration_receipt.clone()));
            }
            let registration = OrganizationRegistrationV1 {
                organization_id: organization_id.clone(),
                founder_id: alliance.formed_by_agent_id.clone(),
                policy_version: ORGANIZATION_CONTINUITY_POLICY_VERSION_V1,
                enabled_at_world_tick: now,
                last_accounted_world_tick: now,
                activity_evidence_complete_through_world_tick: now,
                activity_evidence_paused_reason: Some("activity_evidence_unavailable".into()),
                availability_paused_reason: Some("availability_unknown".into()),
                world_binding: world_binding.to_owned(),
                charter_hash,
                registration_receipt,
                manifest_root,
                manifest_asset_refs: assets.iter().map(|asset| asset.asset_ref.clone()).collect(),
                designated_successors: parsed.designated_successors,
                neutral_maintainers: parsed.neutral_maintainers,
                appointment_authorities: parsed.appointment_authorities,
                remedy_adjudicators: parsed.remedy_adjudicators,
                residual_share_bps: parsed.residual_share_bps,
                membership_controller_bindings: alliance
                    .members
                    .iter()
                    .filter_map(|member| {
                        controller_bindings
                            .get(member)
                            .cloned()
                            .map(|binding| (member.clone(), binding))
                    })
                    .collect(),
                controller_bindings,
                activity_sources: BTreeMap::new(),
                activity_observation_started_at_world_tick: None,
                activity_close_cursor_world_tick: now,
                activity_close_hash: None,
                activity_last_closed_event_id: None,
                activity_last_closed_event_hash: None,
            };
            continuity
                .registrations
                .insert(organization_id.clone(), registration);
            Ok(organization_id.clone())
        }
        Command::RegisterActivityContract {
            organization_id,
            contract_id,
            expected_terms_hash,
            governance_proposal_key,
        } => {
            let registration = continuity
                .registrations
                .get(organization_id)
                .ok_or_else(|| invalid("organization_continuity_not_enabled"))?
                .clone();
            if actor != registration.founder_id
                || !controller_binding_matches(state, &registration, actor)
                || !registration.activity_sources.is_empty()
                    && registration.activity_sources.contains_key(contract_id)
                || continuity.cases.values().any(|case| {
                    case.organization_id == *organization_id
                        && !matches!(
                            case.phase,
                            OrganizationContinuityPhaseV1::Resolved
                                | OrganizationContinuityPhaseV1::Cancelled
                        )
                })
            {
                return Err(invalid("organization_activity_source_registration_denied"));
            }
            let contract = state
                .economic_contracts
                .get(contract_id)
                .ok_or_else(|| invalid("organization_activity_contract_missing"))?;
            if contract.contract_id != *contract_id
                || contract.creator_agent_id != actor
                || contract.counterparty_agent_id == actor
                || contract.fulfillment_kind
                    != crate::EconomicContractFulfillmentKind::AtomicExchange
                || contract.status != crate::EconomicContractStatus::Accepted
                || contract.expires_at < now
                || contract.settlement_amount <= 0
                || contract.reputation_stake < 0
                || economic_contract_terms_hash(contract)? != *expected_terms_hash
                || registration.activity_sources.contains_key(contract_id)
                || continuity.registrations.values().any(|other| {
                    other.organization_id != *organization_id
                        && other.activity_sources.contains_key(contract_id)
                })
            {
                return Err(invalid("organization_activity_contract_not_admissible"));
            }
            let founder_binding = registration
                .controller_bindings
                .get(actor)
                .cloned()
                .ok_or_else(|| invalid("organization_founder_controller_binding_missing"))?;
            let counterparty_binding =
                controller_binding_for(state, &contract.counterparty_agent_id)?;
            if !counterparty_is_external_to_current_members(
                state,
                organization_id,
                &contract.counterparty_agent_id,
            ) || counterparty_binding.player_id == founder_binding.player_id
                || counterparty_binding.root_agent_id == founder_binding.root_agent_id
            {
                return Err(invalid("organization_activity_counterparty_not_external"));
            }
            require_continuity_governance(state, actor, governance_proposal_key, command)?;
            let source = OrganizationActivityContractSourceV1 {
                contract_id: contract_id.clone(),
                terms_hash: expected_terms_hash.clone(),
                creator_binding: founder_binding,
                counterparty_binding,
                registered_at_world_tick: now,
                registration_receipt: key.to_owned(),
                world_binding: registration.world_binding.clone(),
            };
            let registration = continuity
                .registrations
                .get_mut(organization_id)
                .expect("registration validated");
            if registration
                .activity_observation_started_at_world_tick
                .is_none()
            {
                registration.activity_observation_started_at_world_tick = Some(now);
                registration.activity_close_cursor_world_tick = now.saturating_sub(1);
                registration.activity_evidence_complete_through_world_tick = now.saturating_sub(1);
                registration.activity_close_hash = None;
                registration.activity_last_closed_event_id = None;
                registration.activity_last_closed_event_hash = None;
            }
            registration
                .activity_sources
                .insert(contract_id.clone(), source);
            if !matches!(
                registration.activity_evidence_paused_reason.as_deref(),
                Some("activity_evidence_gap" | "activity_source_integrity_failed")
            ) {
                registration.activity_evidence_paused_reason =
                    Some("activity_close_pending".into());
            }
            Ok(contract_id.clone())
        }
        Command::CloseActivityTick {
            organization_id,
            through_world_tick,
            previous_close_hash,
            source_catalog_root,
            through_event_id,
            through_event_hash,
            close_hash,
        } => {
            if *through_world_tick != now {
                return Err(invalid("organization_activity_close_not_current_tick"));
            }
            continuity.close_activity_tick(
                state,
                organization_id,
                now,
                previous_close_hash.as_deref(),
                source_catalog_root,
                *through_event_id,
                through_event_hash,
                close_hash,
            )
        }
        _ => unreachable!("unsupported command reached the specialized dispatcher"),
    }
}

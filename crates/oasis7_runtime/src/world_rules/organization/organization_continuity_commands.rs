use super::*;

pub(super) fn apply_continuity_command(
    state: &WorldState,
    rules: &mut WorldRulesStateV1,
    continuity: &mut OrganizationContinuityStateV1,
    actor: &str,
    key: &str,
    command: &Command,
    now: u64,
    world_binding: &str,
    is_system_event: bool,
    ledgers: &mut BTreeMap<MaterialLedgerId, BTreeMap<String, i64>>,
) -> Result<String, WorldError> {
    if world_binding.trim().is_empty() {
        return Err(invalid("world_binding_required"));
    }
    let organization_id = match command {
        Command::Enable {
            organization_id, ..
        }
        | Command::RecordAvailability {
            organization_id, ..
        }
        | Command::FinalizeAvailabilityUnknown {
            organization_id, ..
        }
        | Command::CloseActivityTick {
            organization_id, ..
        }
        | Command::RegisterActivityContract {
            organization_id, ..
        }
        | Command::OpenDormancyCase {
            organization_id, ..
        }
        | Command::OpenVoluntaryCase {
            organization_id, ..
        } => Some(organization_id.as_str()),
        Command::AdvancePhase { case_id, .. }
        | Command::PublishNotice { case_id, .. }
        | Command::SubmitSuccessor { case_id, .. }
        | Command::CommitHandoff { case_id, .. }
        | Command::SubmitRemedy { case_id, .. }
        | Command::ResolveRemedy { case_id, .. }
        | Command::AdvanceDisposition { case_id, .. }
        | Command::SettleEstateAsset { case_id, .. } => continuity
            .cases
            .get(case_id)
            .map(|case| case.organization_id.as_str()),
    };
    if let Some(registration) =
        organization_id.and_then(|organization_id| continuity.registrations.get(organization_id))
        && (registration.policy_version != ORGANIZATION_CONTINUITY_POLICY_VERSION_V1
            || registration.world_binding != world_binding)
    {
        return Err(invalid("organization_world_binding_or_policy_changed"));
    }
    if !is_system_event
        && let Some(registration) = organization_id
            .and_then(|organization_id| continuity.registrations.get(organization_id))
        && registration.controller_bindings.contains_key(actor)
        && !controller_binding_matches(state, registration, actor)
        && !member_controller_binding_matches(state, continuity, registration, actor)
    {
        return Err(invalid("organization_actor_controller_binding_changed"));
    }
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
                        && case.phase != super::super::OrganizationProtectionPhaseV1::Resolved
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
        Command::RecordAvailability {
            organization_id,
            from_world_tick,
            through_world_tick,
            world_writes_available,
            notice_available,
            remedy_available,
            certificate,
        } => {
            if world_binding.is_empty() {
                return Err(invalid("world_binding_required"));
            }
            let registration = continuity
                .registrations
                .get(organization_id)
                .ok_or_else(|| invalid("organization_continuity_not_enabled"))?
                .clone();
            let accounted_through = accounted_through(continuity, &registration);
            let intervals = continuity
                .availability
                .entry(organization_id.clone())
                .or_default();
            let expected_from = accounted_through.saturating_add(1);
            if *from_world_tick != expected_from
                || through_world_tick < from_world_tick
                || *through_world_tick > now
                || through_world_tick
                    .saturating_sub(*from_world_tick)
                    .saturating_add(1)
                    > MAX_INTERVAL_TICKS
            {
                return Err(invalid("availability_interval_not_contiguous_or_bounded"));
            }
            if manifest_root_for_organization(state, &registration)? != registration.manifest_root {
                return Err(invalid("organization_manifest_drift"));
            }
            validate_availability_certificate(
                state,
                &registration,
                world_binding,
                *from_world_tick,
                *through_world_tick,
                *world_writes_available,
                *notice_available,
                *remedy_available,
                certificate,
            )?;
            let interval = OrganizationAvailabilityIntervalV1 {
                from_world_tick: *from_world_tick,
                through_world_tick: *through_world_tick,
                world_writes_available: *world_writes_available,
                notice_available: *notice_available,
                remedy_available: *remedy_available,
                evidence_status: OrganizationAvailabilityEvidenceStatusV1::Certified,
                world_binding: world_binding.to_owned(),
                registration_receipt: registration.registration_receipt,
                certificate_hash: crate::util::hash_json(certificate)?,
            };
            intervals.push(interval);
            let registration = continuity
                .registrations
                .get_mut(organization_id)
                .expect("registration validated above");
            registration.last_accounted_world_tick = *through_world_tick;
            registration.availability_paused_reason = if *through_world_tick < now {
                Some("availability_unknown".into())
            } else if !(*world_writes_available && *notice_available && *remedy_available) {
                Some("service_unavailable".into())
            } else {
                None
            };
            Ok(organization_id.clone())
        }
        Command::FinalizeAvailabilityUnknown {
            organization_id,
            through_world_tick,
        } => {
            let registration = continuity
                .registrations
                .get(organization_id)
                .ok_or_else(|| invalid("organization_continuity_not_enabled"))?
                .clone();
            let expected_from = accounted_through(continuity, &registration).saturating_add(1);
            if *through_world_tick != now || *through_world_tick < expected_from {
                return Err(invalid(
                    "availability_unknown_finalization_not_current_or_contiguous",
                ));
            }
            let intervals = continuity
                .availability
                .entry(organization_id.clone())
                .or_default();
            if let Some(previous) = intervals.last_mut().filter(|previous| {
                previous.evidence_status == OrganizationAvailabilityEvidenceStatusV1::Unknown
                    && previous.through_world_tick.saturating_add(1) == expected_from
                    && previous.world_binding == registration.world_binding
                    && previous.registration_receipt == registration.registration_receipt
            }) {
                previous.through_world_tick = *through_world_tick;
            } else {
                intervals.push(OrganizationAvailabilityIntervalV1 {
                    from_world_tick: expected_from,
                    through_world_tick: *through_world_tick,
                    world_writes_available: false,
                    notice_available: false,
                    remedy_available: false,
                    evidence_status: OrganizationAvailabilityEvidenceStatusV1::Unknown,
                    world_binding: registration.world_binding.clone(),
                    registration_receipt: registration.registration_receipt.clone(),
                    certificate_hash: String::new(),
                });
            }
            let registration = continuity
                .registrations
                .get_mut(organization_id)
                .expect("registration validated above");
            registration.last_accounted_world_tick = *through_world_tick;
            registration.availability_paused_reason = Some("availability_unknown".into());
            Ok(organization_id.clone())
        }
        Command::OpenDormancyCase {
            organization_id,
            expected_manifest_root,
            trigger_activity_receipt,
        } => {
            let registration = continuity
                .registrations
                .get(organization_id)
                .ok_or_else(|| invalid("organization_continuity_not_enabled"))?
                .clone();
            if !activity_evidence_complete(&registration, now) {
                return Err(invalid("organization_activity_evidence_unavailable"));
            }
            require_current_manifest(state, &registration)?;
            ensure_no_open_v1_case(continuity, organization_id)?;
            let root = manifest_root_for_organization(state, &registration)?;
            if &root != expected_manifest_root || root != registration.manifest_root {
                return Err(invalid("organization_manifest_root_stale"));
            }
            let latest = latest_activity(continuity, organization_id);
            if latest.as_ref().map(|activity| &activity.source_ref)
                != trigger_activity_receipt.as_ref()
            {
                return Err(invalid("organization_activity_trigger_stale"));
            }
            let clock = continuity.continuity_clock_at(organization_id, now);
            let Some(observation_start_clock) = registration
                .activity_observation_started_at_world_tick
                .map(|tick| continuity.continuity_clock_at(organization_id, tick))
            else {
                return Err(invalid("organization_activity_baseline_unavailable"));
            };
            let last_clock = latest
                .as_ref()
                .and_then(|activity| {
                    activity_clock(continuity, organization_id, activity.world_tick)
                })
                .map_or(observation_start_clock, |activity_clock| {
                    activity_clock.max(observation_start_clock)
                });
            if clock.saturating_sub(last_clock) < ORGANIZATION_INACTIVITY_TICKS_V1 {
                return Err(invalid("organization_inactivity_window_pending"));
            }
            if !actor_is_organization_holder(state, rules, organization_id, actor) {
                return Err(invalid("organization_case_open_authority_required"));
            }
            let case = new_case(
                key,
                organization_id,
                &registration,
                OrganizationCaseOriginV1::Dormancy,
                latest.map(|activity| activity.source_ref),
                now,
                clock,
            )?;
            continuity.cases.insert(case.case_id.clone(), case);
            Ok(key.to_owned())
        }
        Command::OpenVoluntaryCase {
            organization_id,
            expected_charter_hash,
            governance_proposal_key,
            kind,
        } => {
            let registration = continuity
                .registrations
                .get(organization_id)
                .ok_or_else(|| invalid("organization_continuity_not_enabled"))?
                .clone();
            require_current_manifest(state, &registration)?;
            if registration.charter_hash != *expected_charter_hash
                || !controller_binding_matches(state, &registration, actor)
                || state
                    .alliances
                    .get(organization_id)
                    .is_none_or(|alliance| alliance.formed_by_agent_id != actor)
            {
                return Err(invalid(
                    "organization_founder_or_charter_confirmation_required",
                ));
            }
            ensure_no_open_v1_case(continuity, organization_id)?;
            require_continuity_governance(state, actor, governance_proposal_key, command)?;
            let clock = continuity.continuity_clock_at(organization_id, now);
            let origin = match kind {
                VoluntaryKind::Dissolution => OrganizationCaseOriginV1::VoluntaryDissolution,
                VoluntaryKind::Reorganization => OrganizationCaseOriginV1::VoluntaryReorganization,
            };
            let case = new_case(
                key,
                organization_id,
                &registration,
                origin,
                None,
                now,
                clock,
            )?;
            continuity.cases.insert(case.case_id.clone(), case);
            Ok(key.to_owned())
        }
        Command::AdvancePhase {
            case_id,
            expected_revision,
        } => {
            let mut case = checked_v1_case(continuity, case_id, *expected_revision)?;
            ensure_case_manifest_fresh(state, continuity, &case)?;
            let registration = continuity.registrations[&case.organization_id].clone();
            if case.origin == OrganizationCaseOriginV1::Dormancy
                && !activity_evidence_complete(&registration, now)
            {
                return Err(invalid("organization_activity_evidence_unavailable"));
            }
            let founder_authorized = actor == registration.founder_id
                && controller_binding_matches(state, &registration, actor);
            let appointment_authorized = registration.appointment_authorities.contains(actor)
                && controller_binding_matches(state, &registration, actor);
            let member_authorized =
                actor_is_organization_holder(state, rules, &case.organization_id, actor);
            if !founder_authorized && !appointment_authorized && !member_authorized {
                return Err(invalid("organization_phase_advance_authority_required"));
            }
            let clock = continuity.continuity_clock_at(&case.organization_id, now);
            match case.phase {
                OrganizationContinuityPhaseV1::Notice => {
                    let resumed = latest_activity_after_notice(
                        continuity,
                        &case.organization_id,
                        case.notified_at_clock.unwrap_or(case.opened_at_clock),
                    );
                    if let Some(activity) = resumed {
                        case.phase = OrganizationContinuityPhaseV1::Cancelled;
                        case.stage_receipts.push(activity.source_ref);
                    } else if clock
                        >= case
                            .notified_at_clock
                            .unwrap_or(clock)
                            .saturating_add(ORGANIZATION_NOTICE_TICKS_V1)
                    {
                        case.phase = OrganizationContinuityPhaseV1::Protection;
                        case.protection_started_at_clock = Some(clock);
                        case.candidate_window_starts_at_clock = Some(
                            clock.saturating_add(
                                ORGANIZATION_PROTECTION_TICKS_V1
                                    .saturating_sub(ORGANIZATION_CANDIDATE_TICKS_V1),
                            ),
                        );
                        case.protection_ends_at_clock =
                            Some(clock.saturating_add(ORGANIZATION_PROTECTION_TICKS_V1));
                    } else {
                        return Err(invalid("organization_notice_window_pending"));
                    }
                }
                OrganizationContinuityPhaseV1::Protection => {
                    if case
                        .candidate_window_starts_at_clock
                        .is_some_and(|start| clock >= start)
                    {
                        case.phase = OrganizationContinuityPhaseV1::CandidateCollection;
                    } else {
                        return Err(invalid("organization_protection_window_pending"));
                    }
                }
                OrganizationContinuityPhaseV1::CandidateCollection => {
                    if case
                        .protection_ends_at_clock
                        .is_some_and(|end| clock >= end)
                    {
                        case.phase = OrganizationContinuityPhaseV1::Protected;
                    } else {
                        return Err(invalid("organization_candidate_window_pending"));
                    }
                }
                _ => return Err(invalid("organization_phase_not_advanceable")),
            }
            case.revision = case.revision.saturating_add(1);
            case.stage_receipts.push(key.to_owned());
            continuity.cases.insert(case_id.clone(), case);
            Ok(case_id.clone())
        }
        Command::PublishNotice {
            case_id,
            expected_revision,
            recipient_ids,
            scope_refs,
            reason_code,
            trigger_fact_ref,
            evidence_refs,
            allowed_actions,
            restricted_actions,
        } => {
            let mut case = checked_v1_case(continuity, case_id, *expected_revision)?;
            if case.phase != OrganizationContinuityPhaseV1::AwaitingNotice {
                return Err(invalid("organization_notice_already_published"));
            }
            let expected_recipients =
                expected_recipients(state, continuity, &case.organization_id)?;
            let expected_reason = match case.origin {
                OrganizationCaseOriginV1::Dormancy => "organization_dormancy_continuity",
                OrganizationCaseOriginV1::VoluntaryDissolution => {
                    "organization_voluntary_dissolution"
                }
                OrganizationCaseOriginV1::VoluntaryReorganization => {
                    "organization_voluntary_reorganization"
                }
            };
            if recipient_ids != &expected_recipients
                || scope_refs != &case.manifest_asset_refs
                || reason_code != expected_reason
                || trigger_fact_ref
                    != case
                        .trigger_activity_receipt
                        .as_deref()
                        .unwrap_or(&case.registration_receipt)
                || evidence_refs.len() > 32
                || !evidence_refs
                    .iter()
                    .any(|reference| reference == &case.manifest_root)
                || !evidence_refs
                    .iter()
                    .any(|reference| reference == &case.registration_receipt)
                || allowed_actions != &default_notice_allowed_actions()
                || restricted_actions != &default_notice_restricted_actions()
            {
                return Err(invalid(
                    "organization_notice_content_incomplete_or_untrusted",
                ));
            }
            let registration = &continuity.registrations[&case.organization_id];
            let appointment_authorized = registration.appointment_authorities.contains(actor)
                && controller_binding_matches(state, registration, actor);
            if !actor_is_organization_holder(state, rules, &case.organization_id, actor)
                && !appointment_authorized
            {
                return Err(invalid("organization_notice_authority_required"));
            }
            let clock = continuity.continuity_clock_at(&case.organization_id, now);
            case.phase = OrganizationContinuityPhaseV1::Notice;
            case.recipients = recipient_ids.clone();
            case.notified_at_clock = Some(clock);
            case.notified_at_world_tick = Some(now);
            case.notice_receipt = Some(key.to_owned());
            case.revision = case.revision.saturating_add(1);
            case.stage_receipts.push(key.to_owned());
            continuity.cases.insert(case_id.clone(), case);
            Ok(key.to_owned())
        }
        Command::SubmitSuccessor {
            case_id,
            expected_revision,
            candidate_id,
            scope_refs,
            budget_ref,
            qualification_receipts,
            conflict_disclosures,
            acceptance_receipt,
        } => {
            let mut case = checked_v1_case(continuity, case_id, *expected_revision)?;
            ensure_case_manifest_fresh(state, continuity, &case)?;
            let registration = continuity.registrations[&case.organization_id].clone();
            let clock = continuity.continuity_clock_at(&case.organization_id, now);
            let is_charter_candidate = registration.designated_successors.contains(candidate_id)
                || registration.neutral_maintainers.contains(candidate_id);
            let is_charter_candidate = is_charter_candidate
                && controller_binding_matches(state, &registration, candidate_id);
            let is_member_candidate =
                state
                    .alliances
                    .get(&case.organization_id)
                    .is_some_and(|alliance| {
                        alliance.members.iter().any(|member| member == candidate_id)
                    });
            if case.phase != OrganizationContinuityPhaseV1::CandidateCollection
                || actor != candidate_id
                || !state.agents.contains_key(candidate_id)
                || state.gameplay_policy.blocked_agents.contains(candidate_id)
                || !controller_binding_matches(state, &registration, candidate_id)
                    && !member_controller_binding_matches(
                        state,
                        continuity,
                        &registration,
                        candidate_id,
                    )
                || !is_charter_candidate && !is_member_candidate
                || scope_refs.is_empty()
                || !scope_refs.is_subset(&case.manifest_asset_refs)
                || budget_ref != &MaterialLedgerId::agent(candidate_id).to_string()
                || qualification_receipts.len() < 2
                || conflict_disclosures.is_empty()
                || acceptance_receipt.trim().is_empty()
                || acceptance_receipt.len() > 256
            {
                return Err(invalid(
                    "organization_successor_not_eligible_or_scope_invalid",
                ));
            }
            if is_member_candidate
                && !is_charter_candidate
                && !member_qualified_at_notice(state, continuity, &case, candidate_id)
            {
                return Err(invalid("organization_member_tenure_unverified"));
            }
            let actual_qualifications = eligible_delivery_receipts(
                state,
                continuity,
                &case.organization_id,
                candidate_id,
                case.notified_at_clock.unwrap_or(clock),
            );
            let requested = qualification_receipts
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>();
            if requested.len() != qualification_receipts.len()
                || requested.len() < 2
                || !requested.is_subset(&actual_qualifications)
            {
                return Err(invalid("organization_successor_qualification_unverified"));
            }
            let ledger_id = MaterialLedgerId::agent(candidate_id);
            let budget_snapshot = state
                .material_ledgers
                .get(&ledger_id)
                .ok_or_else(|| invalid("organization_successor_budget_missing"))?
                .iter()
                .filter(|(_, amount)| **amount > 0)
                .map(|(material, amount)| (material.clone(), *amount))
                .collect::<BTreeMap<_, _>>();
            if budget_snapshot.is_empty() {
                return Err(invalid("organization_successor_budget_empty"));
            }
            if case.candidates.contains_key(candidate_id) {
                return Err(invalid("organization_successor_already_submitted"));
            }
            let controller = state
                .resolved_agent_owner(candidate_id)
                .ok_or_else(|| invalid("organization_successor_controller_unavailable"))?;
            case.candidates.insert(
                candidate_id.clone(),
                OrganizationSuccessorCandidateV1 {
                    candidate_id: candidate_id.clone(),
                    controller_root_id: controller.primary_agent_id,
                    controller_player_id: controller.player_id,
                    controller_source_id: controller.source_id,
                    controller_generation: controller.generation,
                    scope_refs: scope_refs.clone(),
                    budget_ref: budget_ref.clone(),
                    budget_snapshot,
                    qualification_receipts: requested,
                    conflict_disclosures: conflict_disclosures.clone(),
                    acceptance_receipt: acceptance_receipt.clone(),
                    submitted_at_clock: clock,
                    submission_order: case.candidates.len().saturating_add(1) as u32,
                    appointed: false,
                },
            );
            case.revision = case.revision.saturating_add(1);
            case.stage_receipts.push(key.to_owned());
            continuity.cases.insert(case_id.clone(), case);
            Ok(candidate_id.clone())
        }
        Command::CommitHandoff {
            case_id,
            expected_revision,
            candidate_id,
            scope_refs,
            governance_proposal_key,
        } => {
            let mut case = checked_v1_case(continuity, case_id, *expected_revision)?;
            ensure_case_manifest_fresh(state, continuity, &case)?;
            let registration = continuity.registrations[&case.organization_id].clone();
            let clock = continuity.continuity_clock_at(&case.organization_id, now);
            if !case.handoff_scope_refs.is_empty()
                && case
                    .handoff_expires_at_clock
                    .is_none_or(|expires| clock < expires)
            {
                return Err(invalid("organization_handoff_already_active"));
            }
            let candidate = case
                .candidates
                .get(candidate_id)
                .ok_or_else(|| invalid("organization_successor_candidate_missing"))?;
            if !matches!(
                case.phase,
                OrganizationContinuityPhaseV1::CandidateCollection
                    | OrganizationContinuityPhaseV1::Protected
            ) || !candidate.scope_refs.is_superset(scope_refs)
                || scope_refs.is_empty()
                || scope_quarantined(&case, scope_refs)
                || has_pending_remedy(&case, scope_refs)
                || scope_quarantined_in_organization(continuity, &case.organization_id, scope_refs)
                || has_pending_remedy_in_organization(continuity, &case.organization_id, scope_refs)
                || conflicted_with_case(&case, candidate_id, scope_refs)
                || candidate.budget_ref != MaterialLedgerId::agent(candidate_id).to_string()
                || current_positive_materials(state, candidate_id)? != candidate.budget_snapshot
                || !candidate_still_qualified(
                    state,
                    continuity,
                    &registration,
                    &case,
                    candidate,
                    case.notified_at_clock.unwrap_or(clock),
                )
                || next_eligible_candidate(&registration, &case, state, continuity, clock)
                    .as_deref()
                    != Some(candidate_id.as_str())
                || !registration.appointment_authorities.contains(actor)
                || !controller_binding_matches(state, &registration, actor)
                || actor == candidate_id
            {
                return Err(invalid(
                    "organization_handoff_authority_or_qualification_invalid",
                ));
            }
            require_continuity_governance(state, actor, governance_proposal_key, command)?;
            if let Some(previous_candidate_id) = case.handoff_candidate_id.take()
                && let Some(previous_candidate) = case.candidates.get_mut(&previous_candidate_id)
            {
                previous_candidate.appointed = false;
            }
            case.handoff_candidate_id = Some(candidate_id.clone());
            case.handoff_scope_refs = scope_refs.clone();
            case.handoff_expires_at_clock = Some(clock.saturating_add(ORGANIZATION_GRANT_TICKS_V1));
            case.candidates
                .get_mut(candidate_id)
                .expect("candidate validated")
                .appointed = true;
            case.phase = OrganizationContinuityPhaseV1::Protected;
            case.revision = case.revision.saturating_add(1);
            case.stage_receipts.push(key.to_owned());
            continuity.cases.insert(case_id.clone(), case);
            Ok(candidate_id.clone())
        }
        Command::SubmitRemedy {
            case_id,
            expected_revision,
            request_id,
            kind,
            scope_refs,
            fact_ref,
            evidence_refs,
        } => {
            let mut case = checked_v1_case(continuity, case_id, *expected_revision)?;
            ensure_case_manifest_fresh(state, continuity, &case)?;
            if request_id.is_empty()
                || request_id.len() > 128
                || scope_refs.is_empty()
                || !scope_refs.iter().all(|scope| {
                    scope == &format!("case:{case_id}") || case.manifest_asset_refs.contains(scope)
                })
                || fact_ref.is_empty()
                || evidence_refs.is_empty()
                || evidence_refs.len() > 32
                || !evidence_refs
                    .iter()
                    .all(|reference| known_evidence_reference(state, continuity, &case, reference))
                || !known_evidence_reference(state, continuity, &case, fact_ref)
            {
                return Err(invalid("organization_remedy_evidence_or_scope_invalid"));
            }
            if !remedy_requester_has_interest(state, continuity, &case, actor, *kind) {
                return Err(invalid("organization_remedy_requester_not_affected"));
            }
            if case.remedies.contains_key(request_id) {
                return Err(invalid("organization_remedy_request_duplicate"));
            }
            let case_pending = case
                .remedies
                .values()
                .filter(|remedy| remedy.outcome.is_none())
                .count();
            let actor_pending = case
                .remedies
                .values()
                .filter(|remedy| remedy.outcome.is_none() && remedy.requester_id == actor)
                .count();
            if case_pending >= MAX_ACTIVE_REMEDIES_PER_CASE
                || actor_pending >= MAX_ACTIVE_REMEDIES_PER_ACTOR
            {
                return Err(invalid("organization_remedy_capacity_exhausted"));
            }
            let clock = continuity.continuity_clock_at(&case.organization_id, now);
            case.remedies.insert(
                request_id.clone(),
                OrganizationRemedyRequestV1 {
                    request_id: request_id.clone(),
                    requester_id: actor.to_owned(),
                    kind: *kind,
                    scope_refs: scope_refs.clone(),
                    fact_ref: fact_ref.clone(),
                    evidence_refs: evidence_refs.clone(),
                    admitted_at_clock: clock,
                    deadline_clock: clock.saturating_add(ORGANIZATION_APPEAL_TICKS_V1),
                    outcome: None,
                    supported_scope_refs: BTreeSet::new(),
                    adjudication_ref: None,
                    resolved_by: None,
                },
            );
            case.revision = case.revision.saturating_add(1);
            case.stage_receipts.push(key.to_owned());
            continuity.cases.insert(case_id.clone(), case);
            Ok(request_id.clone())
        }
        Command::ResolveRemedy {
            case_id,
            expected_revision,
            request_id,
            outcome,
            supported_scope_refs,
            adjudication_ref,
            governance_proposal_key,
        } => {
            let mut case =
                checked_v1_case_for_remedy_resolution(continuity, case_id, *expected_revision)?;
            let registration = continuity.registrations[&case.organization_id].clone();
            let remedy = case
                .remedies
                .get(request_id)
                .cloned()
                .ok_or_else(|| invalid("organization_remedy_missing"))?;
            if remedy.outcome.is_some()
                || !registration.remedy_adjudicators.contains(actor)
                || !controller_binding_matches(state, &registration, actor)
                || actor == remedy.requester_id
                || case.handoff_candidate_id.as_deref() == Some(actor)
                || adjudication_ref.is_empty()
                || !known_evidence_reference(state, continuity, &case, adjudication_ref)
                || !supported_scope_refs.is_subset(&remedy.scope_refs)
                || matches!(
                    outcome,
                    RemedyOutcome::Supported | RemedyOutcome::PartiallySupported
                ) && supported_scope_refs.is_empty()
            {
                return Err(invalid("organization_remedy_adjudication_invalid"));
            }
            ensure_remedy_adjudicator_independent(state, rules, continuity, &case, &remedy, actor)?;
            require_continuity_governance(state, actor, governance_proposal_key, command)?;
            let target = case.remedies.get_mut(request_id).expect("validated remedy");
            target.outcome = Some(*outcome);
            target.supported_scope_refs = supported_scope_refs.clone();
            target.adjudication_ref = Some(adjudication_ref.clone());
            target.resolved_by = Some(key.to_owned());
            if matches!(
                outcome,
                RemedyOutcome::Supported | RemedyOutcome::PartiallySupported
            ) {
                case.quarantined_scope_refs
                    .extend(supported_scope_refs.iter().cloned());
                if target.kind == RemedyKind::Reclaim
                    && case.phase != OrganizationContinuityPhaseV1::Cancelled
                {
                    if remedy.requester_id != registration.founder_id {
                        return Err(invalid("organization_reclaim_requires_current_founder"));
                    }
                    case.phase = match case.phase {
                        OrganizationContinuityPhaseV1::Notice => {
                            OrganizationContinuityPhaseV1::Cancelled
                        }
                        _ => OrganizationContinuityPhaseV1::Protected,
                    };
                    if supported_scope_refs.contains(&format!("case:{}", case.case_id)) {
                        case.handoff_scope_refs.clear();
                    } else {
                        case.handoff_scope_refs
                            .retain(|scope| !supported_scope_refs.contains(scope));
                    }
                    if case.handoff_scope_refs.is_empty() {
                        if let Some(candidate_id) = case.handoff_candidate_id.take()
                            && let Some(candidate) = case.candidates.get_mut(&candidate_id)
                        {
                            candidate.appointed = false;
                        }
                        case.handoff_expires_at_clock = None;
                    }
                }
            }
            case.revision = case.revision.saturating_add(1);
            case.stage_receipts.push(key.to_owned());
            continuity.cases.insert(case_id.clone(), case);
            Ok(request_id.clone())
        }
        Command::AdvanceDisposition {
            case_id,
            expected_revision,
            governance_proposal_key,
        } => {
            let mut case = checked_v1_case(continuity, case_id, *expected_revision)?;
            let registration = continuity.registrations[&case.organization_id].clone();
            let clock = continuity.continuity_clock_at(&case.organization_id, now);
            if !matches!(
                case.phase,
                OrganizationContinuityPhaseV1::Protected
                    | OrganizationContinuityPhaseV1::Disposition
            ) || !registration.appointment_authorities.contains(actor)
                || !controller_binding_matches(state, &registration, actor)
                || has_pending_remedy(&case, &case.manifest_asset_refs)
                || clock < case.protection_ends_at_clock.unwrap_or(u64::MAX)
            {
                return Err(invalid("organization_disposition_gate_closed"));
            }
            require_continuity_governance(state, actor, governance_proposal_key, command)?;
            case.disposition_stage = match case.disposition_stage {
                OrganizationDispositionStageV1::None => {
                    case.phase = OrganizationContinuityPhaseV1::Disposition;
                    case.disposition_started_at_clock = Some(clock);
                    OrganizationDispositionStageV1::Frozen
                }
                OrganizationDispositionStageV1::Frozen => {
                    OrganizationDispositionStageV1::ExistingContracts
                }
                OrganizationDispositionStageV1::ExistingContracts => {
                    return Err(invalid(
                        "organization_contract_inventory_adapter_unavailable",
                    ));
                }
                OrganizationDispositionStageV1::CustodyReturn => {
                    if case.manifest_asset_refs.iter().any(|asset_ref| {
                        continuity
                            .assets
                            .get(asset_ref)
                            .is_some_and(|asset| asset.held_in_trust && !asset.disposed)
                    }) {
                        return Err(invalid("organization_custody_returns_pending"));
                    }
                    OrganizationDispositionStageV1::ClaimsAndCosts
                }
                OrganizationDispositionStageV1::ClaimsAndCosts => {
                    return Err(invalid("organization_liability_adapter_unavailable"));
                }
                OrganizationDispositionStageV1::BusinessTransfer => {
                    return Err(invalid(
                        "organization_business_transfer_adapter_unavailable",
                    ));
                }
                OrganizationDispositionStageV1::Residual => {
                    return Err(invalid("organization_liability_clearance_unavailable"));
                }
                OrganizationDispositionStageV1::Complete => {
                    return Err(invalid("organization_disposition_already_complete"));
                }
            };
            case.revision = case.revision.saturating_add(1);
            case.stage_receipts.push(key.to_owned());
            continuity.cases.insert(case_id.clone(), case);
            Ok(case_id.clone())
        }
        Command::SettleEstateAsset {
            case_id,
            expected_revision,
            asset_ref,
            expected_asset_version,
            operation,
            amount,
            allocations,
            effect_ref,
            governance_proposal_key,
        } => {
            let mut case = checked_v1_case(continuity, case_id, *expected_revision)?;
            let registration = continuity.registrations[&case.organization_id].clone();
            let clock = continuity.continuity_clock_at(&case.organization_id, now);
            let mut asset = continuity
                .assets
                .get(asset_ref)
                .cloned()
                .ok_or_else(|| invalid("organization_asset_missing"))?;
            if !case.manifest_asset_refs.contains(asset_ref)
                || asset.organization_id != case.organization_id
                || asset.disposed
                || asset.version != *expected_asset_version
                || *amount <= 0
                || *amount != asset.amount
                || effect_ref != key
                || continuity.consumed_effects.contains_key(effect_ref)
                || scope_quarantined(&case, &BTreeSet::from([asset_ref.clone()]))
                || has_pending_remedy(&case, &BTreeSet::from([asset_ref.clone()]))
                || scope_quarantined_in_organization(
                    continuity,
                    &case.organization_id,
                    &BTreeSet::from([asset_ref.clone()]),
                )
                || has_pending_remedy_in_organization(
                    continuity,
                    &case.organization_id,
                    &BTreeSet::from([asset_ref.clone()]),
                )
                || clock < case.protection_ends_at_clock.unwrap_or(u64::MAX)
            {
                return Err(invalid("organization_asset_effect_gate_closed"));
            }
            if *operation == EstateOperation::ReturnCustody {
                let appointment_authorized = registration.appointment_authorities.contains(actor)
                    && controller_binding_matches(state, &registration, actor)
                    && case.phase == OrganizationContinuityPhaseV1::Disposition
                    && case.disposition_stage == OrganizationDispositionStageV1::CustodyReturn;
                let scope_includes_asset = case.handoff_scope_refs.contains(asset_ref);
                let active_scope_handoff = scope_includes_asset
                    && case
                        .handoff_expires_at_clock
                        .is_some_and(|expires| clock < expires);
                // v1's only delegated material effect is returning an
                // explicitly scoped held-in-trust asset to its recorded
                // contributor. This does not grant a general disposition
                // right, and the action is re-authorized against the live
                // handoff and continuity gates on every request.
                let delegated_authorized = case.handoff_candidate_id.as_deref() == Some(actor)
                    && active_scope_handoff
                    && case.candidates.get(actor).is_some_and(|candidate| {
                        candidate.appointed
                            && candidate_still_qualified(
                                state,
                                continuity,
                                &registration,
                                &case,
                                candidate,
                                case.notified_at_clock.unwrap_or(clock),
                            )
                    })
                    && (case.phase == OrganizationContinuityPhaseV1::Protected
                        || (case.phase == OrganizationContinuityPhaseV1::Disposition
                            && case.disposition_stage
                                == OrganizationDispositionStageV1::CustodyReturn));
                if !asset.held_in_trust
                    || scope_includes_asset && !delegated_authorized
                    || !(appointment_authorized || delegated_authorized)
                {
                    return Err(invalid(
                        "organization_custody_return_authority_or_stage_invalid",
                    ));
                }
            }
            require_continuity_governance(state, actor, governance_proposal_key, command)?;
            match operation {
                EstateOperation::ReturnCustody if asset.held_in_trust => {
                    if allocations != &BTreeMap::from([(asset.contributor_id.clone(), *amount)]) {
                        return Err(invalid("organization_custody_return_allocation_invalid"));
                    }
                    credit_material(
                        state,
                        ledgers,
                        &MaterialLedgerId::agent(&asset.contributor_id),
                        &asset.material,
                        *amount,
                    )?;
                }
                EstateOperation::PaySecuredClaim
                | EstateOperation::PayApprovedPreservationCost
                | EstateOperation::PayOrdinaryClaim => {
                    return Err(invalid("organization_liability_source_adapter_unavailable"));
                }
                EstateOperation::TransferBusiness => {
                    return Err(invalid(
                        "organization_business_transfer_adapter_unavailable",
                    ));
                }
                EstateOperation::DistributeResidual => {
                    return Err(invalid("organization_liability_clearance_unavailable"));
                }
                EstateOperation::ReturnCustody => {
                    return Err(invalid("organization_asset_not_held_in_custody"));
                }
            }
            asset.disposed = true;
            asset.version = asset.version.saturating_add(1);
            asset.effect_receipt = Some(key.to_owned());
            continuity.assets.insert(asset_ref.clone(), asset);
            continuity
                .consumed_effects
                .insert(effect_ref.clone(), asset_ref.clone());
            if let Some(deposit) = rules.organization_estate.get_mut(asset_ref) {
                deposit.disposed = true;
                deposit.effect_receipt = Some(key.to_owned());
            }
            case.revision = case.revision.saturating_add(1);
            case.stage_receipts.push(key.to_owned());
            continuity.cases.insert(case_id.clone(), case);
            let _ = registration;
            Ok(asset_ref.clone())
        }
    }
}

use super::*;

#[allow(clippy::too_many_arguments)]

pub(super) fn apply_continuity_case_command(
    state: &WorldState,
    rules: &mut WorldRulesStateV1,
    continuity: &mut OrganizationContinuityStateV1,
    actor: &str,
    key: &str,
    command: &Command,
    now: u64,
) -> Result<String, WorldError> {
    match command {
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
        _ => unreachable!("unsupported command reached the specialized dispatcher"),
    }
}

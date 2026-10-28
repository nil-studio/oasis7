use super::*;

#[allow(clippy::too_many_arguments)]

pub(super) fn apply_continuity_handoff_command(
    state: &WorldState,
    continuity: &mut OrganizationContinuityStateV1,
    actor: &str,
    key: &str,
    command: &Command,
    now: u64,
) -> Result<String, WorldError> {
    match command {
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
        _ => unreachable!("unsupported command reached the specialized dispatcher"),
    }
}

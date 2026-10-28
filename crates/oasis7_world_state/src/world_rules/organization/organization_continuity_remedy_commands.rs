use super::*;

#[allow(clippy::too_many_arguments)]

pub(super) fn apply_continuity_remedy_command(
    state: &WorldState,
    rules: &mut WorldRulesStateV1,
    continuity: &mut OrganizationContinuityStateV1,
    actor: &str,
    key: &str,
    command: &Command,
    now: u64,
    ledgers: &mut BTreeMap<MaterialLedgerId, BTreeMap<String, i64>>,
) -> Result<String, WorldError> {
    match command {
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
        _ => unreachable!("unsupported command reached the specialized dispatcher"),
    }
}

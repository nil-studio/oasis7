use super::*;

pub fn parse_charter(raw: &str) -> Result<OrganizationContinuityCharterV1, WorldError> {
    let parsed: OrganizationContinuityCharterV1 = serde_json::from_str(raw)
        .map_err(|_| invalid("organization_charter_adapter_unavailable"))?;
    if parsed.schema != "organization-continuity-v1"
        || parsed.activity_source_policy != ORGANIZATION_ACTIVITY_SOURCE_POLICY_V1
        || parsed.designated_successors.len() > 64
        || parsed.neutral_maintainers.len() > 64
        || parsed.appointment_authorities.len() > 128
        || parsed.remedy_adjudicators.len() > 128
        || parsed.residual_share_bps.len() > 128
    {
        return Err(invalid("organization_charter_invalid"));
    }
    ensure_unique_ids(parsed.designated_successors.iter().map(String::as_str))?;
    ensure_unique_ids(parsed.neutral_maintainers.iter().map(String::as_str))?;
    Ok(parsed)
}

pub fn validate_charter_authorities(
    state: &WorldState,
    alliance: &crate::AllianceState,
    charter: &OrganizationContinuityCharterV1,
) -> Result<(), WorldError> {
    let members = alliance.members.iter().cloned().collect::<BTreeSet<_>>();
    if alliance.members.is_empty()
        || members.len() != alliance.members.len()
        || charter.appointment_authorities.is_empty()
        || charter.remedy_adjudicators.is_empty()
        || charter.activity_source_policy != ORGANIZATION_ACTIVITY_SOURCE_POLICY_V1
        || charter
            .appointment_authorities
            .iter()
            .any(|id| !members.contains(id))
        || charter
            .remedy_adjudicators
            .iter()
            .any(|id| !state.agents.contains_key(id))
        || charter
            .designated_successors
            .iter()
            .any(|id| !state.agents.contains_key(id))
        || charter
            .neutral_maintainers
            .iter()
            .any(|id| !state.agents.contains_key(id))
        || charter
            .residual_share_bps
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>()
            != members
        || charter
            .residual_share_bps
            .values()
            .copied()
            .map(u32::from)
            .sum::<u32>()
            != 10_000
    {
        return Err(invalid(
            "organization_charter_authority_or_residual_rule_invalid",
        ));
    }
    let _ = charter_controller_bindings(state, alliance, charter)?;
    Ok(())
}

pub fn controller_binding_for(
    state: &WorldState,
    agent_id: &str,
) -> Result<OrganizationControllerBindingV1, WorldError> {
    let owner = state
        .resolved_agent_owner(agent_id)
        .ok_or_else(|| invalid("organization_controller_binding_unavailable"))?;
    if owner.primary_agent_id.is_empty()
        || owner.player_id.is_empty()
        || owner.source_id.is_empty()
        || owner.generation == 0
    {
        return Err(invalid("organization_controller_binding_incomplete"));
    }
    Ok(OrganizationControllerBindingV1 {
        root_agent_id: owner.primary_agent_id,
        player_id: owner.player_id,
        source_id: owner.source_id,
        generation: owner.generation,
    })
}

/// Returns false when any current member cannot be resolved or shares the
/// counterparty's player or root controller. Membership is read from the live
/// alliance, so joining after source registration invalidates externality.
pub fn counterparty_is_external_to_current_members(
    state: &WorldState,
    organization_id: &str,
    counterparty_agent_id: &str,
) -> bool {
    let Some(alliance) = state.alliances.get(organization_id) else {
        return false;
    };
    let Ok(counterparty) = controller_binding_for(state, counterparty_agent_id) else {
        return false;
    };
    alliance.members.iter().all(|member| {
        controller_binding_for(state, member).is_ok_and(|binding| {
            binding.player_id != counterparty.player_id
                && binding.root_agent_id != counterparty.root_agent_id
        })
    })
}

pub fn controller_binding_matches(
    state: &WorldState,
    registration: &OrganizationRegistrationV1,
    agent_id: &str,
) -> bool {
    registration
        .controller_bindings
        .get(agent_id)
        .zip(state.resolved_agent_owner(agent_id))
        .is_some_and(|(binding, owner)| {
            !binding.root_agent_id.is_empty()
                && !binding.player_id.is_empty()
                && !binding.source_id.is_empty()
                && binding.generation > 0
                && owner.primary_agent_id == binding.root_agent_id
                && owner.player_id == binding.player_id
                && owner.source_id == binding.source_id
                && owner.generation == binding.generation
        })
}

pub fn member_controller_binding_matches(
    state: &WorldState,
    continuity: &OrganizationContinuityStateV1,
    registration: &OrganizationRegistrationV1,
    agent_id: &str,
) -> bool {
    if agent_id == registration.founder_id
        || !state
            .alliances
            .get(&registration.organization_id)
            .is_some_and(|alliance| alliance.members.iter().any(|member| member == agent_id))
        || !continuity
            .membership_history
            .get(&registration.organization_id)
            .and_then(|members| members.get(agent_id))
            .and_then(|intervals| intervals.last())
            .is_some_and(|(_, left_at, _)| left_at.is_none())
    {
        return false;
    }
    registration
        .membership_controller_bindings
        .get(agent_id)
        .zip(state.resolved_agent_owner(agent_id))
        .is_some_and(|(binding, owner)| {
            !binding.root_agent_id.is_empty()
                && !binding.player_id.is_empty()
                && !binding.source_id.is_empty()
                && binding.generation > 0
                && owner.primary_agent_id == binding.root_agent_id
                && owner.player_id == binding.player_id
                && owner.source_id == binding.source_id
                && owner.generation == binding.generation
        })
}

pub fn charter_controller_bindings(
    state: &WorldState,
    alliance: &crate::AllianceState,
    charter: &OrganizationContinuityCharterV1,
) -> Result<BTreeMap<String, OrganizationControllerBindingV1>, WorldError> {
    let mut ids = BTreeSet::from([alliance.formed_by_agent_id.clone()]);
    ids.extend(alliance.members.iter().cloned());
    ids.extend(charter.designated_successors.iter().cloned());
    ids.extend(charter.neutral_maintainers.iter().cloned());
    ids.extend(charter.appointment_authorities.iter().cloned());
    ids.extend(charter.remedy_adjudicators.iter().cloned());
    ids.extend(charter.residual_share_bps.keys().cloned());
    ids.into_iter()
        .map(|id| Ok((id.clone(), controller_binding_for(state, &id)?)))
        .collect()
}

pub fn economic_contract_terms_hash(
    contract: &crate::EconomicContractState,
) -> Result<String, WorldError> {
    crate::util::hash_json(&(
        "oasis7.organization-continuity.atomic-exchange-terms.v1",
        &contract.contract_id,
        &contract.creator_agent_id,
        &contract.counterparty_agent_id,
        contract.fulfillment_kind,
        contract.settlement_kind,
        contract.settlement_amount,
        contract.reputation_stake,
        contract.expires_at,
        &contract.description,
    ))
}

pub fn activity_catalog_root(
    registration: &OrganizationRegistrationV1,
) -> Result<String, WorldError> {
    crate::util::hash_json(&(
        "oasis7.organization-continuity.activity-catalog.v1",
        &registration.organization_id,
        &registration.world_binding,
        &registration.registration_receipt,
        &registration.activity_sources,
    ))
}

fn ensure_unique_ids<'a>(ids: impl Iterator<Item = &'a str>) -> Result<(), WorldError> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if id.is_empty() || id.trim() != id || id.len() > 128 || !seen.insert(id) {
            return Err(invalid("organization_charter_ids_invalid_or_duplicate"));
        }
    }
    Ok(())
}

pub fn require_continuity_governance(
    state: &WorldState,
    actor: &str,
    proposal_key: &str,
    command: &Command,
) -> Result<(), WorldError> {
    let outer = WorldRuleCommandV1::OrganizationContinuity {
        command: command.clone(),
    };
    require_governance(state, actor, proposal_key, &outer)
}

// Every availability channel and interval is independently bound by the signed preimage.
#[allow(clippy::too_many_arguments)]
pub fn validate_availability_certificate(
    state: &WorldState,
    registration: &OrganizationRegistrationV1,
    world_binding: &str,
    from: u64,
    through: u64,
    world_writes: bool,
    notice: bool,
    remedy: bool,
    certificate: &GovernanceFinalityCertificate,
) -> Result<(), WorldError> {
    let payload_binding = (
        "oasis7.organization-continuity.availability.v1",
        world_binding,
        &registration.registration_receipt,
        &registration.organization_id,
        &registration.charter_hash,
        &registration.world_binding,
        from,
        through,
        world_writes,
        notice,
        remedy,
    );
    let manifest_hash = crate::util::hash_json(&payload_binding)?;
    let proposal_id = u64::from_str_radix(&manifest_hash[..16], 16)
        .map_err(|_| invalid("organization_availability_proposal_id_invalid"))?;
    if certificate.proposal_id != proposal_id {
        return Err(invalid("organization_availability_proposal_id_mismatch"));
    }
    verify_world_rule_authority(state, &manifest_hash, through, certificate)
}

pub fn manifest_for_organization(
    state: &WorldState,
    rules: &WorldRulesStateV1,
    organization_id: &str,
) -> Result<Vec<OrganizationManifestAssetV1>, WorldError> {
    let mut assets = Vec::new();
    for deposit in rules
        .organization_estate
        .values()
        .filter(|deposit| deposit.organization_id == organization_id)
    {
        if !rules.requests.values().any(|receipt| {
            receipt.result_id == deposit.id
                && matches!(
                    &receipt.command,
                    WorldRuleCommandV1::DepositOrganizationEstate {
                        organization_id: id,
                        material,
                        amount,
                        held_in_trust,
                    } if id == organization_id
                        && material == &deposit.material
                        && *amount == deposit.amount
                        && *held_in_trust == deposit.held_in_trust
                )
        }) {
            return Err(invalid("organization_manifest_source_receipt_missing"));
        }
        if deposit.id.is_empty()
            || deposit.material.is_empty()
            || deposit.amount <= 0
            || !state.agents.contains_key(&deposit.contributor_id)
        {
            return Err(invalid("organization_manifest_asset_source_invalid"));
        }
        assets.push(OrganizationManifestAssetV1 {
            asset_ref: deposit.id.clone(),
            source_receipt: deposit.id.clone(),
            organization_id: organization_id.to_owned(),
            contributor_id: deposit.contributor_id.clone(),
            material: deposit.material.clone(),
            amount: deposit.amount,
            held_in_trust: deposit.held_in_trust,
            version: 1,
            disposed: deposit.disposed,
            effect_receipt: deposit.effect_receipt.clone(),
        });
    }
    assets.sort_by(|left, right| left.asset_ref.cmp(&right.asset_ref));
    Ok(assets)
}

pub fn manifest_root_for_organization(
    state: &WorldState,
    registration: &OrganizationRegistrationV1,
) -> Result<String, WorldError> {
    let rules = state.world_rules.as_ref().cloned().unwrap_or_default();
    let assets = manifest_for_organization(state, &rules, &registration.organization_id)?;
    manifest_commitment_root(&assets)
}

pub fn manifest_commitment_root(
    assets: &[OrganizationManifestAssetV1],
) -> Result<String, WorldError> {
    let immutable = assets
        .iter()
        .map(|asset| {
            (
                asset.asset_ref.as_str(),
                asset.source_receipt.as_str(),
                asset.organization_id.as_str(),
                asset.contributor_id.as_str(),
                asset.material.as_str(),
                asset.amount,
                asset.held_in_trust,
            )
        })
        .collect::<Vec<_>>();
    crate::util::hash_json(&immutable)
}

pub fn require_current_manifest(
    state: &WorldState,
    registration: &OrganizationRegistrationV1,
) -> Result<(), WorldError> {
    let root = manifest_root_for_organization(state, registration)?;
    if root != registration.manifest_root {
        return Err(invalid("organization_manifest_drift"));
    }
    Ok(())
}

pub fn ensure_case_manifest_fresh(
    state: &WorldState,
    continuity: &OrganizationContinuityStateV1,
    case: &OrganizationContinuityCaseV1,
) -> Result<(), WorldError> {
    let registration = continuity
        .registrations
        .get(&case.organization_id)
        .ok_or_else(|| invalid("organization_registration_missing"))?;
    require_current_manifest(state, registration)?;
    if registration.manifest_root != case.manifest_root
        || registration.manifest_asset_refs != case.manifest_asset_refs
    {
        return Err(invalid("organization_case_manifest_drift"));
    }
    Ok(())
}

pub fn new_case(
    case_id: &str,
    organization_id: &str,
    registration: &OrganizationRegistrationV1,
    origin: OrganizationCaseOriginV1,
    trigger_activity_receipt: Option<String>,
    now: u64,
    clock: u64,
) -> Result<OrganizationContinuityCaseV1, WorldError> {
    Ok(OrganizationContinuityCaseV1 {
        case_id: case_id.to_owned(),
        organization_id: organization_id.to_owned(),
        registration_receipt: registration.registration_receipt.clone(),
        policy_version: ORGANIZATION_CONTINUITY_POLICY_VERSION_V1,
        origin,
        revision: 1,
        phase: OrganizationContinuityPhaseV1::AwaitingNotice,
        disposition_stage: OrganizationDispositionStageV1::None,
        manifest_root: registration.manifest_root.clone(),
        manifest_asset_refs: registration.manifest_asset_refs.clone(),
        recipients: BTreeSet::new(),
        trigger_activity_receipt,
        opened_at_world_tick: now,
        opened_at_clock: clock,
        notice_receipt: None,
        notified_at_clock: None,
        notified_at_world_tick: None,
        protection_started_at_clock: None,
        protection_ends_at_clock: None,
        candidate_window_starts_at_clock: None,
        disposition_started_at_clock: None,
        candidates: BTreeMap::new(),
        remedies: BTreeMap::new(),
        quarantined_scope_refs: BTreeSet::new(),
        handoff_candidate_id: None,
        handoff_scope_refs: BTreeSet::new(),
        handoff_expires_at_clock: None,
        stage_receipts: vec![case_id.to_owned()],
    })
}

pub fn checked_v1_case(
    state: &OrganizationContinuityStateV1,
    case_id: &str,
    expected_revision: u64,
) -> Result<OrganizationContinuityCaseV1, WorldError> {
    let case = state
        .cases
        .get(case_id)
        .ok_or_else(|| invalid("organization_continuity_case_missing"))?;
    if case.revision != expected_revision
        || matches!(
            case.phase,
            OrganizationContinuityPhaseV1::Resolved | OrganizationContinuityPhaseV1::Cancelled
        )
    {
        return Err(invalid("organization_continuity_case_stale_or_terminal"));
    }
    Ok(case.clone())
}

pub fn checked_v1_case_for_remedy_resolution(
    state: &OrganizationContinuityStateV1,
    case_id: &str,
    expected_revision: u64,
) -> Result<OrganizationContinuityCaseV1, WorldError> {
    let case = state
        .cases
        .get(case_id)
        .ok_or_else(|| invalid("organization_continuity_case_missing"))?;
    if case.revision != expected_revision || case.phase == OrganizationContinuityPhaseV1::Resolved {
        return Err(invalid("organization_continuity_case_stale_or_terminal"));
    }
    Ok(case.clone())
}

pub fn ensure_no_open_v1_case(
    continuity: &OrganizationContinuityStateV1,
    organization_id: &str,
) -> Result<(), WorldError> {
    if continuity.cases.values().any(|case| {
        case.organization_id == organization_id
            && !matches!(
                case.phase,
                OrganizationContinuityPhaseV1::Resolved | OrganizationContinuityPhaseV1::Cancelled
            )
    }) {
        return Err(invalid("organization_continuity_case_already_open"));
    }
    Ok(())
}

pub fn latest_activity(
    continuity: &OrganizationContinuityStateV1,
    organization_id: &str,
) -> Option<OrganizationActivityReceiptV1> {
    let complete_through = continuity
        .registrations
        .get(organization_id)?
        .activity_evidence_complete_through_world_tick;
    continuity
        .activity_receipts
        .get(organization_id)?
        .values()
        .filter(|activity| activity.world_tick <= complete_through)
        .max_by_key(|activity| (activity.world_tick, activity.source_ref.clone()))
        .cloned()
}

pub fn activity_clock(
    continuity: &OrganizationContinuityStateV1,
    organization_id: &str,
    world_tick: u64,
) -> Option<u64> {
    let registration = continuity.registrations.get(organization_id)?;
    if world_tick > registration.activity_evidence_complete_through_world_tick {
        return None;
    }
    continuity
        .availability
        .get(organization_id)?
        .iter()
        .any(|interval| {
            interval.evidence_status == OrganizationAvailabilityEvidenceStatusV1::Certified
                && interval.world_binding == registration.world_binding
                && interval.registration_receipt == registration.registration_receipt
                && interval.world_writes_available
                && interval.notice_available
                && interval.remedy_available
                && interval.from_world_tick <= world_tick
                && interval.through_world_tick >= world_tick
        })
        .then(|| continuity.continuity_clock_at(organization_id, world_tick))
}

pub fn latest_activity_after_notice(
    continuity: &OrganizationContinuityStateV1,
    organization_id: &str,
    notice_clock: u64,
) -> Option<OrganizationActivityReceiptV1> {
    continuity
        .activity_receipts
        .get(organization_id)?
        .values()
        .filter(|activity| {
            activity_clock(continuity, organization_id, activity.world_tick)
                .is_some_and(|clock| clock > notice_clock)
        })
        .max_by_key(|activity| (activity.world_tick, activity.source_ref.clone()))
        .cloned()
}

pub fn expected_recipients(
    state: &WorldState,
    continuity: &OrganizationContinuityStateV1,
    organization_id: &str,
) -> Result<BTreeSet<String>, WorldError> {
    let alliance = state
        .alliances
        .get(organization_id)
        .ok_or_else(|| invalid("organization_missing"))?;
    let mut recipients = alliance.members.iter().cloned().collect::<BTreeSet<_>>();
    for asset in continuity.assets.values().filter(|asset| {
        asset.organization_id == organization_id && asset.held_in_trust && !asset.disposed
    }) {
        recipients.insert(asset.contributor_id.clone());
    }
    Ok(recipients)
}

pub fn default_notice_allowed_actions() -> BTreeSet<String> {
    BTreeSet::from([
        "submit_successor".into(),
        "submit_reclaim".into(),
        "submit_appeal".into(),
    ])
}

pub fn default_notice_restricted_actions() -> BTreeSet<String> {
    BTreeSet::from([
        "new_long_term_obligation".into(),
        "asset_sale_or_encumbrance".into(),
        "residual_distribution".into(),
        "scope_expansion".into(),
    ])
}

pub fn command_kind(command: &Command) -> &'static str {
    match command {
        Command::PublishNotice { .. } => "notice",
        Command::AdvancePhase { .. } => "phase",
        Command::OpenDormancyCase { .. } => "open",
        _ => "other",
    }
}

pub fn short_id(value: &str) -> String {
    crate::util::sha256_hex(value.as_bytes())[..16].to_owned()
}

pub fn actor_is_organization_holder(
    state: &WorldState,
    rules: &WorldRulesStateV1,
    organization_id: &str,
    actor: &str,
) -> bool {
    let Some(registration) = rules
        .organization_continuity
        .registrations
        .get(organization_id)
    else {
        return false;
    };
    if actor == registration.founder_id {
        return controller_binding_matches(state, registration, actor)
            && state
                .alliances
                .get(organization_id)
                .is_some_and(|alliance| alliance.members.iter().any(|member| member == actor));
    }
    member_controller_binding_matches(state, &rules.organization_continuity, registration, actor)
}

pub fn known_evidence_reference(
    state: &WorldState,
    continuity: &OrganizationContinuityStateV1,
    case: &OrganizationContinuityCaseV1,
    reference: &str,
) -> bool {
    reference == case.case_id
        || reference == case.registration_receipt
        || reference == case.manifest_root
        || case.stage_receipts.iter().any(|item| item == reference)
        || continuity
            .assets
            .get(reference)
            .is_some_and(|asset| case.manifest_asset_refs.contains(&asset.asset_ref))
        || continuity
            .activity_receipts
            .get(&case.organization_id)
            .is_some_and(|items| items.contains_key(reference))
        || state.world_rules.as_ref().is_some_and(|rules| {
            rules
                .requests
                .values()
                .any(|receipt| receipt.request_id == reference || receipt.result_id == reference)
        })
}

/// An adjudicator must be independent from the durable effect under review.
/// Evidence labels alone do not prove who executed or benefited from an
/// effect, so only recognized committed world-rule receipts can be adjudicated.
pub fn ensure_remedy_adjudicator_independent(
    state: &WorldState,
    rules: &WorldRulesStateV1,
    continuity: &OrganizationContinuityStateV1,
    case: &OrganizationContinuityCaseV1,
    remedy: &OrganizationRemedyRequestV1,
    adjudicator_id: &str,
) -> Result<(), WorldError> {
    let mut matching_receipts = rules.requests.iter().filter(|(key, receipt)| {
        key.as_str() == remedy.fact_ref
            || receipt.request_id == remedy.fact_ref
            || receipt.result_id == remedy.fact_ref
    });
    let Some((_, receipt)) = matching_receipts.next() else {
        return Err(invalid("organization_remedy_review_blocked"));
    };
    if matching_receipts.next().is_some() {
        return Err(invalid("organization_remedy_review_blocked"));
    }

    let mut disputed_parties =
        BTreeSet::from([receipt.actor_id.clone(), remedy.requester_id.clone()]);
    if let Some(candidate_id) = &case.handoff_candidate_id {
        disputed_parties.insert(candidate_id.clone());
    }

    match &receipt.command {
        WorldRuleCommandV1::OrganizationContinuity {
            command:
                Command::CommitHandoff {
                    case_id,
                    candidate_id,
                    ..
                },
        } if case_id == &case.case_id => {
            disputed_parties.insert(candidate_id.clone());
        }
        WorldRuleCommandV1::OrganizationContinuity {
            command:
                Command::SettleEstateAsset {
                    case_id,
                    asset_ref,
                    allocations,
                    ..
                },
        } if case_id == &case.case_id => {
            if !case.manifest_asset_refs.contains(asset_ref) {
                return Err(invalid("organization_remedy_review_blocked"));
            }
            disputed_parties.extend(allocations.keys().cloned());
            if let Some(asset) = continuity.assets.get(asset_ref) {
                disputed_parties.insert(asset.contributor_id.clone());
            }
        }
        WorldRuleCommandV1::OrganizationContinuity {
            command: Command::AdvanceDisposition { case_id, .. },
        } if case_id == &case.case_id => {
            disputed_parties.extend(case.recipients.iter().cloned());
            disputed_parties.extend(case.candidates.keys().cloned());
            disputed_parties.extend(
                continuity
                    .assets
                    .values()
                    .filter(|asset| case.manifest_asset_refs.contains(&asset.asset_ref))
                    .map(|asset| asset.contributor_id.clone()),
            );
        }
        WorldRuleCommandV1::OrganizationContinuity {
            command:
                Command::PublishNotice {
                    case_id,
                    recipient_ids,
                    ..
                },
        } if case_id == &case.case_id => disputed_parties.extend(recipient_ids.iter().cloned()),
        WorldRuleCommandV1::OrganizationContinuity {
            command:
                Command::SubmitSuccessor {
                    case_id,
                    candidate_id,
                    ..
                },
        } if case_id == &case.case_id => {
            disputed_parties.insert(candidate_id.clone());
        }
        WorldRuleCommandV1::OrganizationContinuity {
            command:
                Command::ResolveRemedy {
                    case_id,
                    request_id,
                    ..
                },
        } if case_id == &case.case_id => {
            let prior = case
                .remedies
                .get(request_id)
                .ok_or_else(|| invalid("organization_remedy_review_blocked"))?;
            disputed_parties.insert(prior.requester_id.clone());
        }
        _ => return Err(invalid("organization_remedy_review_blocked")),
    }

    if disputed_parties.is_empty() || disputed_parties.contains(adjudicator_id) {
        return Err(invalid("organization_remedy_review_blocked"));
    }
    let adjudicator_owner = state
        .resolved_agent_owner(adjudicator_id)
        .ok_or_else(|| invalid("organization_remedy_review_blocked"))?;
    for party in disputed_parties {
        let party_owner = state
            .resolved_agent_owner(&party)
            .ok_or_else(|| invalid("organization_remedy_review_blocked"))?;
        if party_owner.primary_agent_id == adjudicator_owner.primary_agent_id
            || party_owner.player_id == adjudicator_owner.player_id
        {
            return Err(invalid("organization_remedy_review_blocked"));
        }
    }
    Ok(())
}

pub fn remedy_requester_has_interest(
    state: &WorldState,
    continuity: &OrganizationContinuityStateV1,
    case: &OrganizationContinuityCaseV1,
    actor: &str,
    kind: RemedyKind,
) -> bool {
    if !state.agents.contains_key(actor) {
        return false;
    }
    match kind {
        RemedyKind::Reclaim => state
            .alliances
            .get(&case.organization_id)
            .is_some_and(|alliance| alliance.formed_by_agent_id == actor),
        RemedyKind::Appeal => {
            state
                .alliances
                .get(&case.organization_id)
                .is_some_and(|alliance| alliance.members.iter().any(|member| member == actor))
                || continuity.assets.values().any(|asset| {
                    asset.organization_id == case.organization_id && asset.contributor_id == actor
                })
        }
    }
}

pub fn has_pending_remedy(
    case: &OrganizationContinuityCaseV1,
    scope_refs: &BTreeSet<String>,
) -> bool {
    case.remedies.values().any(|remedy| {
        remedy.outcome.is_none()
            && (remedy
                .scope_refs
                .contains(&format!("case:{}", case.case_id))
                || !remedy.scope_refs.is_disjoint(scope_refs))
    })
}

pub fn scope_quarantined(
    case: &OrganizationContinuityCaseV1,
    scope_refs: &BTreeSet<String>,
) -> bool {
    case.quarantined_scope_refs
        .contains(&format!("case:{}", case.case_id))
        || !case.quarantined_scope_refs.is_disjoint(scope_refs)
}

pub fn has_pending_remedy_in_organization(
    continuity: &OrganizationContinuityStateV1,
    organization_id: &str,
    scope_refs: &BTreeSet<String>,
) -> bool {
    continuity.cases.values().any(|case| {
        case.organization_id == organization_id
            && case.remedies.values().any(|remedy| {
                if remedy.outcome.is_some() {
                    return false;
                }
                if !remedy.scope_refs.is_disjoint(scope_refs) {
                    return true;
                }
                remedy
                    .scope_refs
                    .contains(&format!("case:{}", case.case_id))
                    && !case.manifest_asset_refs.is_disjoint(scope_refs)
            })
    })
}

pub fn scope_quarantined_in_organization(
    continuity: &OrganizationContinuityStateV1,
    organization_id: &str,
    scope_refs: &BTreeSet<String>,
) -> bool {
    continuity.cases.values().any(|case| {
        case.organization_id == organization_id
            && (case
                .quarantined_scope_refs
                .iter()
                .any(|scope| scope_refs.contains(scope))
                || case
                    .quarantined_scope_refs
                    .contains(&format!("case:{}", case.case_id))
                    && !case.manifest_asset_refs.is_disjoint(scope_refs))
    })
}

pub fn conflicted_with_case(
    case: &OrganizationContinuityCaseV1,
    candidate_id: &str,
    scope_refs: &BTreeSet<String>,
) -> bool {
    case.remedies.values().any(|remedy| {
        remedy.outcome.is_none()
            && remedy.requester_id == candidate_id
            && !remedy.scope_refs.is_disjoint(scope_refs)
    })
}

pub fn eligible_delivery_receipts(
    state: &WorldState,
    continuity: &OrganizationContinuityStateV1,
    organization_id: &str,
    candidate_id: &str,
    clock: u64,
) -> BTreeSet<String> {
    let Some(rules) = &state.world_rules else {
        return BTreeSet::new();
    };
    rules
        .agent_orders
        .values()
        .filter_map(|order| {
            let delivered_world_tick = order.delivered_at?;
            let delivered_clock =
                activity_clock(continuity, organization_id, delivered_world_tick)?;
            (order.owner_id == candidate_id
                && order.phase == crate::world_rules::AgentIndustrialPhaseV1::Delivered
                && continuity
                    .registrations
                    .get(organization_id)
                    .is_some_and(|registration| {
                        order.quote.world_binding.as_deref()
                            == Some(registration.world_binding.as_str())
                    })
                && delivered_clock <= clock
                && clock.saturating_sub(delivered_clock)
                    <= 120 * ORGANIZATION_CONTINUITY_PERIOD_TICKS_V1)
                .then(|| order.order_id.clone())
        })
        .collect()
}

pub fn candidate_still_qualified(
    state: &WorldState,
    continuity: &OrganizationContinuityStateV1,
    registration: &OrganizationRegistrationV1,
    case: &OrganizationContinuityCaseV1,
    candidate: &OrganizationSuccessorCandidateV1,
    clock: u64,
) -> bool {
    let organization_id = &case.organization_id;
    let actual = eligible_delivery_receipts(
        state,
        continuity,
        organization_id,
        &candidate.candidate_id,
        clock,
    );
    let control_unchanged = state
        .resolved_agent_owner(&candidate.candidate_id)
        .is_some_and(|owner| {
            !candidate.controller_root_id.is_empty()
                && !candidate.controller_player_id.is_empty()
                && !candidate.controller_source_id.is_empty()
                && owner.primary_agent_id == candidate.controller_root_id
                && owner.player_id == candidate.controller_player_id
                && owner.source_id == candidate.controller_source_id
                && owner.generation == candidate.controller_generation
        });
    candidate.qualification_receipts.len() >= 2
        && control_unchanged
        && candidate.qualification_receipts.is_subset(&actual)
        && state.agents.contains_key(&candidate.candidate_id)
        && !state
            .gameplay_policy
            .blocked_agents
            .contains(&candidate.candidate_id)
        && (((registration
            .designated_successors
            .contains(&candidate.candidate_id)
            || registration
                .neutral_maintainers
                .contains(&candidate.candidate_id))
            && controller_binding_matches(state, registration, &candidate.candidate_id))
            || member_qualified_at_notice(state, continuity, case, &candidate.candidate_id))
}

pub fn next_eligible_candidate(
    registration: &OrganizationRegistrationV1,
    case: &OrganizationContinuityCaseV1,
    state: &WorldState,
    continuity: &OrganizationContinuityStateV1,
    clock: u64,
) -> Option<String> {
    let eligible = |id: &str| {
        case.candidates.get(id).is_some_and(|candidate| {
            candidate_still_qualified(state, continuity, registration, case, candidate, clock)
                && candidate.budget_ref == MaterialLedgerId::agent(id).to_string()
                && current_positive_materials(state, id)
                    .is_ok_and(|materials| materials == candidate.budget_snapshot)
                && !has_pending_remedy(case, &candidate.scope_refs)
                && !has_pending_remedy_in_organization(
                    continuity,
                    &case.organization_id,
                    &candidate.scope_refs,
                )
                && !scope_quarantined_in_organization(
                    continuity,
                    &case.organization_id,
                    &candidate.scope_refs,
                )
                && !conflicted_with_case(case, id, &candidate.scope_refs)
        })
    };
    registration
        .designated_successors
        .iter()
        .find(|id| eligible(id))
        .cloned()
        .or_else(|| {
            let notice_tick = case.notified_at_world_tick?;
            case.candidates
                .values()
                .filter(|candidate| {
                    !registration
                        .designated_successors
                        .contains(&candidate.candidate_id)
                        && !registration
                            .neutral_maintainers
                            .contains(&candidate.candidate_id)
                        && eligible(&candidate.candidate_id)
                })
                .filter_map(|candidate| {
                    membership_start_at_notice(
                        continuity,
                        &case.organization_id,
                        &candidate.candidate_id,
                        notice_tick,
                    )
                    .map(|joined_at| {
                        (
                            joined_at,
                            candidate.submission_order,
                            candidate.candidate_id.clone(),
                        )
                    })
                })
                .min()
                .map(|(_, _, candidate_id)| candidate_id)
        })
        .or_else(|| {
            registration
                .neutral_maintainers
                .iter()
                .find(|id| eligible(id))
                .cloned()
        })
}

pub fn member_qualified_at_notice(
    state: &WorldState,
    continuity: &OrganizationContinuityStateV1,
    case: &OrganizationContinuityCaseV1,
    candidate_id: &str,
) -> bool {
    let Some(alliance) = state.alliances.get(&case.organization_id) else {
        return false;
    };
    let Some(registration) = continuity.registrations.get(&case.organization_id) else {
        return false;
    };
    member_controller_binding_matches(state, continuity, registration, candidate_id)
        && alliance.members.iter().any(|id| id == candidate_id)
        && case
            .notified_at_world_tick
            .and_then(|notice_tick| {
                membership_start_at_notice(
                    continuity,
                    &case.organization_id,
                    candidate_id,
                    notice_tick,
                )
                .map(|joined_at| (notice_tick, joined_at))
            })
            .is_some_and(|(notice_tick, joined_at)| {
                let notice_clock = case.notified_at_clock.unwrap_or(0);
                let joined_clock = continuity.continuity_clock_at(&case.organization_id, joined_at);
                notice_tick >= joined_at
                    && joined_clock <= notice_clock
                    && notice_clock.saturating_sub(joined_clock) >= ORGANIZATION_CANDIDATE_TICKS_V1
            })
}

fn membership_start_at_notice(
    continuity: &OrganizationContinuityStateV1,
    organization_id: &str,
    candidate_id: &str,
    notice_tick: u64,
) -> Option<u64> {
    continuity
        .membership_history
        .get(organization_id)?
        .get(candidate_id)?
        .iter()
        .filter(|(joined_at, left_at, _)| {
            *joined_at <= notice_tick && left_at.is_none_or(|left| notice_tick < left)
        })
        .map(|(joined_at, _, _)| *joined_at)
        .max()
}

pub fn current_positive_materials(
    state: &WorldState,
    candidate_id: &str,
) -> Result<BTreeMap<String, i64>, WorldError> {
    let ledger_id = MaterialLedgerId::agent(candidate_id);
    Ok(state
        .material_ledgers
        .get(&ledger_id)
        .ok_or_else(|| invalid("organization_successor_budget_missing"))?
        .iter()
        .filter(|(_, amount)| **amount > 0)
        .map(|(material, amount)| (material.clone(), *amount))
        .collect())
}

pub fn credit_material(
    state: &WorldState,
    ledgers: &mut BTreeMap<MaterialLedgerId, BTreeMap<String, i64>>,
    ledger_id: &MaterialLedgerId,
    material: &str,
    amount: i64,
) -> Result<(), WorldError> {
    if amount < 0 {
        return Err(invalid("organization_negative_credit"));
    }
    let values = ledgers.entry(ledger_id.clone()).or_insert_with(|| {
        state
            .material_ledgers
            .get(ledger_id)
            .cloned()
            .unwrap_or_default()
    });
    let balance = values.entry(material.to_owned()).or_default();
    *balance = balance
        .checked_add(amount)
        .ok_or_else(|| invalid("organization_material_overflow"))?;
    Ok(())
}

/// Resolve the organization addressed by an organization-level or case command.
pub fn command_organization_id<'a>(
    continuity: &'a OrganizationContinuityStateV1,
    command: &'a Command,
) -> Option<&'a str> {
    match command {
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
    }
}

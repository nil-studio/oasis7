//! Fixed normal-tick organization lifecycle facts and reducer kernel.
//!
//! The value types in this file are data only. Current callers must retain and
//! validate the Current source owners from which they derived these rows; a
//! `OrganizationCurrentLifecycleFactsV1` is not a qualification or write
//! capability.
use super::*;
use crate::{
    EconomicContractState, EconomicContractStatus, OrganizationEstateDepositV1, WorldRuleReceiptV1,
};

/// A controller identity read from the real Current agent owner resolver.
/// This is data used by the fixed organization predicate, not a capability.
pub type OrganizationCurrentOwnerV1 = OrganizationControllerBindingV1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrganizationCurrentAgentFactV1 {
    pub agent_id: String,
    /// Presence mirrors the old `WorldState::agents.contains_key` predicate.
    pub present: bool,
    /// Present only after the exact Current claim/starter/acquisition path has
    /// resolved the current controller binding.
    pub owner: Option<OrganizationControllerBindingV1>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrganizationCurrentAllianceFactV1 {
    pub organization_id: String,
    pub formed_by_agent_id: String,
    pub members: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrganizationCurrentContractFactV1 {
    pub contract: EconomicContractState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrganizationCurrentEstateFactV1 {
    pub deposit: OrganizationEstateDepositV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrganizationCurrentRequestFactV1 {
    pub receipt: WorldRuleReceiptV1,
}

/// Per-organization facts selected from the actual Current roots. Callers
/// should only include the rows needed for the selected registration. These
/// facts preserve the old outer and embedded IDs separately.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrganizationCurrentLifecycleFactsV1 {
    pub organization_id: String,
    pub alliance: Option<OrganizationCurrentAllianceFactV1>,
    pub agents: BTreeMap<String, OrganizationCurrentAgentFactV1>,
    pub contracts: BTreeMap<String, OrganizationCurrentContractFactV1>,
    pub estate: BTreeMap<String, OrganizationCurrentEstateFactV1>,
    pub requests: BTreeMap<String, OrganizationCurrentRequestFactV1>,
}

impl OrganizationCurrentLifecycleFactsV1 {
    /// Compatibility adapter for the original in-memory reducer. Production
    /// incremental callers construct the same data from fixed Current readers.
    fn from_world(
        state: &WorldState,
        continuity: &OrganizationContinuityStateV1,
        organization_id: &str,
    ) -> Self {
        let registration = continuity.registrations.get(organization_id);
        let alliance =
            state
                .alliances
                .get(organization_id)
                .map(|value| OrganizationCurrentAllianceFactV1 {
                    organization_id: organization_id.to_owned(),
                    formed_by_agent_id: value.formed_by_agent_id.clone(),
                    members: value.members.clone(),
                });
        let mut agent_ids = BTreeSet::new();
        if let Some(registration) = registration {
            agent_ids.insert(registration.founder_id.clone());
            agent_ids.extend(registration.controller_bindings.keys().cloned());
            agent_ids.extend(registration.membership_controller_bindings.keys().cloned());
            for source in registration.activity_sources.values() {
                if let Some(contract) = state.economic_contracts.get(&source.contract_id) {
                    agent_ids.insert(contract.creator_agent_id.clone());
                    agent_ids.insert(contract.counterparty_agent_id.clone());
                }
            }
        }
        if let Some(alliance) = &alliance {
            agent_ids.insert(alliance.formed_by_agent_id.clone());
            agent_ids.extend(alliance.members.iter().cloned());
        }
        let mut agents = BTreeMap::new();
        for agent_id in agent_ids {
            let present = state.agents.contains_key(&agent_id);
            let owner = present
                .then(|| controller_binding_for(state, &agent_id).ok())
                .flatten();
            agents.insert(
                agent_id.clone(),
                OrganizationCurrentAgentFactV1 {
                    agent_id,
                    present,
                    owner,
                },
            );
        }
        let mut contracts = BTreeMap::new();
        if let Some(registration) = registration {
            for source in registration.activity_sources.values() {
                if let Some(contract) = state.economic_contracts.get(&source.contract_id) {
                    contracts.insert(
                        source.contract_id.clone(),
                        OrganizationCurrentContractFactV1 {
                            contract: contract.clone(),
                        },
                    );
                }
            }
        }
        let mut estate = BTreeMap::new();
        let mut deposit_ids = BTreeSet::new();
        if let Some(rules) = &state.world_rules {
            for deposit in rules
                .organization_estate
                .values()
                .filter(|deposit| deposit.organization_id == organization_id)
            {
                deposit_ids.insert(deposit.id.clone());
                estate.insert(
                    deposit.id.clone(),
                    OrganizationCurrentEstateFactV1 {
                        deposit: deposit.clone(),
                    },
                );
            }
        }
        let mut requests = BTreeMap::new();
        if let Some(rules) = &state.world_rules {
            for receipt in rules
                .requests
                .values()
                .filter(|receipt| deposit_ids.contains(&receipt.result_id))
            {
                requests.insert(
                    receipt.request_id.clone(),
                    OrganizationCurrentRequestFactV1 {
                        receipt: receipt.clone(),
                    },
                );
            }
        }
        Self {
            organization_id: organization_id.to_owned(),
            alliance,
            agents,
            contracts,
            estate,
            requests,
        }
    }

    fn agent_present(&self, id: &str) -> bool {
        self.agents.get(id).is_some_and(|agent| agent.present)
    }

    fn owner(&self, id: &str) -> Option<&OrganizationControllerBindingV1> {
        self.agents.get(id).and_then(|agent| agent.owner.as_ref())
    }
}

fn current_controller_binding_matches(
    facts: &OrganizationCurrentLifecycleFactsV1,
    registration: &OrganizationRegistrationV1,
    agent_id: &str,
) -> bool {
    registration
        .controller_bindings
        .get(agent_id)
        .zip(facts.owner(agent_id))
        .is_some_and(|(binding, owner)| valid_binding(binding) && binding == owner)
}

fn current_member_controller_binding_matches(
    facts: &OrganizationCurrentLifecycleFactsV1,
    continuity: &OrganizationContinuityStateV1,
    registration: &OrganizationRegistrationV1,
    agent_id: &str,
) -> bool {
    if agent_id == registration.founder_id
        || !facts
            .alliance
            .as_ref()
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
        .zip(facts.owner(agent_id))
        .is_some_and(|(binding, owner)| valid_binding(binding) && binding == owner)
}

fn current_actor_is_organization_holder(
    facts: &OrganizationCurrentLifecycleFactsV1,
    continuity: &OrganizationContinuityStateV1,
    registration: &OrganizationRegistrationV1,
    actor: &str,
) -> bool {
    if actor == registration.founder_id {
        current_controller_binding_matches(facts, registration, actor)
            && facts
                .alliance
                .as_ref()
                .is_some_and(|alliance| alliance.members.iter().any(|member| member == actor))
    } else {
        current_member_controller_binding_matches(facts, continuity, registration, actor)
    }
}

fn valid_binding(binding: &OrganizationControllerBindingV1) -> bool {
    !binding.root_agent_id.is_empty()
        && !binding.player_id.is_empty()
        && !binding.source_id.is_empty()
        && binding.generation > 0
}

fn current_manifest_for_organization(
    facts: &OrganizationCurrentLifecycleFactsV1,
    organization_id: &str,
) -> Result<Vec<OrganizationManifestAssetV1>, WorldError> {
    let mut assets = Vec::new();
    for fact in facts
        .estate
        .values()
        .filter(|fact| fact.deposit.organization_id == organization_id)
    {
        let deposit = &fact.deposit;
        let receipt_found = facts.requests.values().any(|fact| {
            let receipt = &fact.receipt;
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
        });
        if !receipt_found {
            return Err(invalid("organization_manifest_source_receipt_missing"));
        }
        if deposit.id.is_empty()
            || deposit.material.is_empty()
            || deposit.amount <= 0
            || !facts.agent_present(&deposit.contributor_id)
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

fn current_manifest_root_for_organization(
    facts: &OrganizationCurrentLifecycleFactsV1,
    registration: &OrganizationRegistrationV1,
) -> Result<String, WorldError> {
    manifest_commitment_root(&current_manifest_for_organization(
        facts,
        &registration.organization_id,
    )?)
}

fn current_expected_recipients(
    facts: &OrganizationCurrentLifecycleFactsV1,
    continuity: &OrganizationContinuityStateV1,
    organization_id: &str,
) -> Result<BTreeSet<String>, WorldError> {
    let alliance = facts
        .alliance
        .as_ref()
        .filter(|alliance| alliance.organization_id == organization_id)
        .ok_or_else(|| invalid("organization_missing"))?;
    let mut recipients = alliance.members.iter().cloned().collect::<BTreeSet<_>>();
    for asset in continuity.assets.values().filter(|asset| {
        asset.organization_id == organization_id && asset.held_in_trust && !asset.disposed
    }) {
        recipients.insert(asset.contributor_id.clone());
    }
    Ok(recipients)
}

fn current_source_catalog_integrity(
    facts: &OrganizationCurrentLifecycleFactsV1,
    continuity: &OrganizationContinuityStateV1,
    registration: &OrganizationRegistrationV1,
) -> bool {
    registration.activity_sources.values().all(|source| {
        let Some(contract) = facts
            .contracts
            .get(&source.contract_id)
            .map(|fact| &fact.contract)
        else {
            return false;
        };
        if !economic_contract_terms_hash(contract).is_ok_and(|hash| hash == source.terms_hash)
            || source.world_binding != registration.world_binding
        {
            return false;
        }
        if contract.status == EconomicContractStatus::Accepted {
            let Some(counterparty) = facts.owner(&contract.counterparty_agent_id) else {
                return false;
            };
            return current_controller_binding_matches(
                facts,
                registration,
                &contract.creator_agent_id,
            ) && current_controller_binding_matches(
                facts,
                registration,
                &contract.counterparty_agent_id,
            ) && facts.alliance.as_ref().is_some_and(|alliance| {
                alliance.members.iter().all(|member| {
                    facts.owner(member).is_some_and(|owner| {
                        owner.player_id != counterparty.player_id
                            && owner.root_agent_id != counterparty.root_agent_id
                    })
                })
            });
        }
        if contract.status == EconomicContractStatus::Settled {
            let expected_receipt = format!("economic-contract-settlement:{}", source.contract_id);
            return contract.settlement_success == Some(true)
                && contract.transfer_amount == contract.settlement_amount
                && contract.settled_at.is_some_and(|settled_at| {
                    continuity
                        .activity_receipts
                        .get(&registration.organization_id)
                        .and_then(|receipts| receipts.get(&expected_receipt))
                        .is_some_and(|receipt| {
                            receipt.world_tick == settled_at
                                && receipt.kind
                                    == OrganizationActivityKindV1::ExternalAtomicExchangeSettlement
                        })
                });
        }
        true
    })
}

/// Build current close commands in the original registration-map order. The
/// caller provides the actual final journal event identity and hash; missing
/// selected facts fail closed by omitting that registration.
pub fn prepare_activity_close_commands_from_current_v1(
    continuity: &OrganizationContinuityStateV1,
    facts: &BTreeMap<String, OrganizationCurrentLifecycleFactsV1>,
    now: u64,
    current_world_binding: Option<&str>,
    through_event_id: u64,
    through_event_hash: &str,
) -> Result<Vec<(String, String, Command)>, WorldError> {
    let mut work = Vec::new();
    for registration in continuity.registrations.values() {
        let Some(facts) = facts.get(&registration.organization_id) else {
            continue;
        };
        if let Some(command) = prepare_activity_close_command_for_registration_v1(
            registration,
            facts,
            now,
            current_world_binding,
            through_event_id,
            through_event_hash,
        )? {
            work.push(command);
        }
    }
    Ok(work)
}

/// Prepare exactly one Current registration. The caller must reopen Current
/// and the journal tail between publications, preserving sequential closes.
pub fn prepare_activity_close_command_for_registration_v1(
    registration: &super::OrganizationRegistrationV1,
    facts: &OrganizationCurrentLifecycleFactsV1,
    now: u64,
    current_world_binding: Option<&str>,
    through_event_id: u64,
    through_event_hash: &str,
) -> Result<Option<(String, String, Command)>, WorldError> {
    if registration.activity_sources.is_empty()
        || registration
            .activity_observation_started_at_world_tick
            .is_none()
        || registration.activity_close_cursor_world_tick >= now
        || current_world_binding != Some(registration.world_binding.as_str())
    {
        return Ok(None);
    }
    let actor_id = if facts.agent_present(&registration.founder_id) {
        registration.founder_id.clone()
    } else {
        let Some(id) = registration
            .controller_bindings
            .keys()
            .find(|id| facts.agent_present(id))
        else {
            return Ok(None);
        };
        id.clone()
    };
    let through_event_hash_valid = through_event_hash.len() == 64
        && through_event_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit());
    if !through_event_hash_valid || through_event_id == 0 {
        return Ok(None);
    }
    let source_catalog_root = activity_catalog_root(registration)?;
    let previous_close_hash = registration.activity_close_hash.clone();
    let close_hash = activity_close_commitment(
        registration,
        now,
        previous_close_hash.as_deref(),
        &source_catalog_root,
        through_event_id,
        through_event_hash,
    )?;
    let organization_digest = crate::util::sha256_hex(registration.organization_id.as_bytes());
    Ok(Some((
        actor_id,
        format!(
            "organization-activity-close:{}:{now}",
            &organization_digest[..16]
        ),
        Command::CloseActivityTick {
            organization_id: registration.organization_id.clone(),
            through_world_tick: now,
            previous_close_hash,
            source_catalog_root,
            through_event_id,
            through_event_hash: through_event_hash.to_owned(),
            close_hash,
        },
    )))
}

/// Produce the same fixed lifecycle command list as the original WorldState
/// method, using only per-organization Current facts supplied by its owner.
pub fn prepare_lifecycle_commands_from_current_v1(
    continuity: &OrganizationContinuityStateV1,
    facts: &BTreeMap<String, OrganizationCurrentLifecycleFactsV1>,
    now: u64,
) -> Result<Vec<(String, String, Command)>, WorldError> {
    prepare_lifecycle_commands_selected_from_current_v1(continuity, facts, now, None)
}
/// Same lifecycle kernel restricted to one registration at the initial batch cut.
pub fn prepare_lifecycle_commands_selected_from_current_v1(
    continuity: &OrganizationContinuityStateV1,
    facts: &BTreeMap<String, OrganizationCurrentLifecycleFactsV1>,
    now: u64,
    organization: Option<&str>,
) -> Result<Vec<(String, String, Command)>, WorldError> {
    let mut work = Vec::new();
    for registration in continuity
        .registrations
        .values()
        .filter(|registration| organization.is_none_or(|id| id == registration.organization_id))
    {
        let accounted = accounted_through(continuity, registration);
        if accounted < now {
            work.push((
                registration.founder_id.clone(),
                format!(
                    "organization-continuity:availability-unknown:{}:{}",
                    short_id(&registration.organization_id),
                    now
                ),
                Command::FinalizeAvailabilityUnknown {
                    organization_id: registration.organization_id.clone(),
                    through_world_tick: now,
                },
            ));
            continue;
        }
        let currently_available = continuity
            .availability
            .get(&registration.organization_id)
            .and_then(|intervals| intervals.last())
            .is_some_and(|interval| {
                interval.through_world_tick == now
                    && interval.evidence_status
                        == OrganizationAvailabilityEvidenceStatusV1::Certified
                    && interval.world_binding == registration.world_binding
                    && interval.registration_receipt == registration.registration_receipt
                    && interval.world_writes_available
                    && interval.notice_available
                    && interval.remedy_available
            });
        if !currently_available {
            continue;
        }
        let clock = continuity.continuity_clock_at(&registration.organization_id, now);
        let active_case = continuity.cases.values().find(|case| {
            case.organization_id == registration.organization_id
                && !matches!(
                    case.phase,
                    OrganizationContinuityPhaseV1::Resolved
                        | OrganizationContinuityPhaseV1::Cancelled
                )
        });
        let Some(facts) = facts.get(&registration.organization_id) else {
            continue;
        };
        if let Some(case) = active_case {
            if case.origin == OrganizationCaseOriginV1::Dormancy
                && !activity_evidence_complete(registration, now)
            {
                continue;
            }
            let command = match case.phase {
                OrganizationContinuityPhaseV1::AwaitingNotice => {
                    let recipients =
                        current_expected_recipients(facts, continuity, &case.organization_id)?;
                    Some(Command::PublishNotice {
                        case_id: case.case_id.clone(),
                        expected_revision: case.revision,
                        recipient_ids: recipients,
                        scope_refs: case.manifest_asset_refs.clone(),
                        reason_code: match case.origin {
                            OrganizationCaseOriginV1::Dormancy => {
                                "organization_dormancy_continuity".into()
                            }
                            OrganizationCaseOriginV1::VoluntaryDissolution => {
                                "organization_voluntary_dissolution".into()
                            }
                            OrganizationCaseOriginV1::VoluntaryReorganization => {
                                "organization_voluntary_reorganization".into()
                            }
                        },
                        trigger_fact_ref: case
                            .trigger_activity_receipt
                            .clone()
                            .unwrap_or_else(|| case.registration_receipt.clone()),
                        evidence_refs: vec![
                            case.manifest_root.clone(),
                            case.registration_receipt.clone(),
                        ],
                        allowed_actions: default_notice_allowed_actions(),
                        restricted_actions: default_notice_restricted_actions(),
                    })
                }
                OrganizationContinuityPhaseV1::Notice
                | OrganizationContinuityPhaseV1::Protection
                | OrganizationContinuityPhaseV1::CandidateCollection => {
                    let current_activity = latest_activity_after_notice(
                        continuity,
                        &case.organization_id,
                        case.notified_at_clock.unwrap_or(case.opened_at_clock),
                    );
                    let phase_due = match case.phase {
                        OrganizationContinuityPhaseV1::Notice => {
                            current_activity.is_some()
                                || clock
                                    >= case
                                        .notified_at_clock
                                        .unwrap_or(clock)
                                        .saturating_add(ORGANIZATION_NOTICE_TICKS_V1)
                        }
                        OrganizationContinuityPhaseV1::Protection => case
                            .candidate_window_starts_at_clock
                            .is_some_and(|start| clock >= start),
                        OrganizationContinuityPhaseV1::CandidateCollection => case
                            .protection_ends_at_clock
                            .is_some_and(|end| clock >= end),
                        _ => false,
                    };
                    phase_due.then(|| Command::AdvancePhase {
                        case_id: case.case_id.clone(),
                        expected_revision: case.revision,
                    })
                }
                _ => None,
            };
            if let Some(command) = command {
                work.push((
                    registration.founder_id.clone(),
                    format!(
                        "organization-continuity:lifecycle:{}:{}:{}",
                        short_id(&case.case_id),
                        case.revision,
                        command_kind(&command)
                    ),
                    command,
                ));
            }
            continue;
        }
        if !activity_evidence_complete(registration, now) {
            continue;
        }
        let Ok(current_root) = current_manifest_root_for_organization(facts, registration) else {
            continue;
        };
        if current_root != registration.manifest_root {
            continue;
        }
        let last_activity_clock = continuity
            .activity_receipts
            .get(&registration.organization_id)
            .and_then(|items| {
                items
                    .values()
                    .filter_map(|item| {
                        activity_clock(continuity, &registration.organization_id, item.world_tick)
                            .map(|at| (at, item.source_ref.as_str()))
                    })
                    .max_by_key(|(at, _)| *at)
                    .map(|(at, ref_)| (at, ref_.to_owned()))
            });
        let Some(observation_start_clock) = registration
            .activity_observation_started_at_world_tick
            .map(|tick| continuity.continuity_clock_at(&registration.organization_id, tick))
        else {
            continue;
        };
        let idle_from = last_activity_clock
            .as_ref()
            .map_or(observation_start_clock, |(clock, _)| {
                (*clock).max(observation_start_clock)
            });
        if clock.saturating_sub(idle_from) >= ORGANIZATION_INACTIVITY_TICKS_V1 {
            let trigger_activity_receipt = last_activity_clock.map(|(_, id)| id);
            work.push((
                registration.founder_id.clone(),
                format!(
                    "organization-continuity:dormancy:{}:{}",
                    short_id(&registration.organization_id),
                    clock
                ),
                Command::OpenDormancyCase {
                    organization_id: registration.organization_id.clone(),
                    expected_manifest_root: current_root,
                    trigger_activity_receipt,
                },
            ));
        }
    }
    Ok(work)
}

/// Fixed system command reducer for Current organization lifecycle sources.
/// It updates only the supplied continuity branch value; the Current owner is
/// responsible for the typed sparse write and the transaction abort on error.
#[allow(clippy::too_many_arguments)]
pub fn apply_system_lifecycle_command_from_current_v1(
    continuity: &mut OrganizationContinuityStateV1,
    facts: &OrganizationCurrentLifecycleFactsV1,
    actor: &str,
    key: &str,
    command: &Command,
    now: u64,
    world_binding: &str,
) -> Result<String, WorldError> {
    if !is_system_lifecycle_command_v1(command) {
        return Err(invalid("organization_system_lifecycle_command_required"));
    }
    if world_binding.trim().is_empty() {
        return Err(invalid("world_binding_required"));
    }
    let organization_id = command_organization_id(continuity, command);
    if organization_id.is_some_and(|id| id != facts.organization_id) {
        return Err(invalid("organization_current_lifecycle_fact_identity"));
    }
    if let Some(registration) = organization_id.and_then(|id| continuity.registrations.get(id))
        && (registration.policy_version != ORGANIZATION_CONTINUITY_POLICY_VERSION_V1
            || registration.world_binding != world_binding)
    {
        return Err(invalid("organization_world_binding_or_policy_changed"));
    }
    match command {
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
            let registration = continuity
                .registrations
                .get(organization_id)
                .ok_or_else(|| invalid("organization_continuity_not_enabled"))?
                .clone();
            apply_close_activity_tick_from_current_v1(
                continuity,
                facts,
                &registration,
                now,
                previous_close_hash.as_deref(),
                source_catalog_root,
                *through_event_id,
                through_event_hash,
                close_hash,
            )
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
                .expect("registration checked above");
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
            let current_root = current_manifest_root_for_organization(facts, &registration)?;
            if current_root != registration.manifest_root {
                return Err(invalid("organization_manifest_drift"));
            }
            ensure_no_open_v1_case(continuity, organization_id)?;
            if expected_manifest_root != &current_root || current_root != registration.manifest_root
            {
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
                .map_or(observation_start_clock, |at| {
                    at.max(observation_start_clock)
                });
            if clock.saturating_sub(last_clock) < ORGANIZATION_INACTIVITY_TICKS_V1 {
                return Err(invalid("organization_inactivity_window_pending"));
            }
            if !current_actor_is_organization_holder(facts, continuity, &registration, actor) {
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
        Command::AdvancePhase {
            case_id,
            expected_revision,
        } => apply_advance_phase_from_current_v1(
            continuity,
            facts,
            case_id,
            *expected_revision,
            actor,
            key,
            now,
        ),
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
        } => apply_publish_notice_from_current_v1(
            continuity,
            facts,
            case_id,
            *expected_revision,
            recipient_ids,
            scope_refs,
            reason_code,
            trigger_fact_ref,
            evidence_refs,
            allowed_actions,
            restricted_actions,
            actor,
            key,
            now,
        ),
        _ => Err(invalid("organization_system_lifecycle_command_required")),
    }
}

pub(super) fn apply_close_activity_tick_from_current_v1(
    continuity: &mut OrganizationContinuityStateV1,
    facts: &OrganizationCurrentLifecycleFactsV1,
    prior: &OrganizationRegistrationV1,
    now: u64,
    previous_close_hash: Option<&str>,
    source_catalog_root: &str,
    through_event_id: u64,
    through_event_hash: &str,
    close_hash: &str,
) -> Result<String, WorldError> {
    let mut registration = prior.clone();
    if registration.activity_sources.is_empty()
        || registration
            .activity_observation_started_at_world_tick
            .is_none()
    {
        return Err(invalid("organization_activity_source_catalog_empty"));
    }
    if now == 0
        || registration.world_binding.is_empty()
        || activity_catalog_root(&registration)? != source_catalog_root
        || previous_close_hash != registration.activity_close_hash.as_deref()
        || through_event_id == 0
        || through_event_hash.len() != 64
        || !through_event_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(invalid("organization_activity_close_binding_invalid"));
    }
    let expected_hash = activity_close_commitment(
        &registration,
        now,
        previous_close_hash,
        source_catalog_root,
        through_event_id,
        through_event_hash,
    )?;
    if expected_hash != close_hash {
        return Err(invalid("organization_activity_close_hash_mismatch"));
    }
    if now <= registration.activity_close_cursor_world_tick {
        if now == registration.activity_close_cursor_world_tick
            && registration.activity_close_hash.as_deref() == Some(close_hash)
        {
            return Ok(close_hash.to_owned());
        }
        return Err(invalid("organization_activity_close_tick_stale"));
    }
    if through_event_id <= registration.activity_last_closed_event_id.unwrap_or(0) {
        return Err(invalid("organization_activity_event_cursor_stale"));
    }
    let catalog_integrity = current_source_catalog_integrity(facts, continuity, &registration);
    let continuous = now
        == registration
            .activity_close_cursor_world_tick
            .saturating_add(1);
    let observation_started = registration
        .activity_observation_started_at_world_tick
        .is_some_and(|started| now >= started);
    registration.activity_close_cursor_world_tick = now;
    registration.activity_close_hash = Some(close_hash.to_owned());
    registration.activity_last_closed_event_id = Some(through_event_id);
    registration.activity_last_closed_event_hash = Some(through_event_hash.to_owned());
    let prior_pause = registration.activity_evidence_paused_reason.as_deref();
    let evidence_restarted = catalog_integrity
        && continuous
        && matches!(
            prior_pause,
            Some("activity_evidence_gap" | "activity_source_integrity_failed")
        );
    if !catalog_integrity {
        registration.activity_evidence_paused_reason =
            Some("activity_source_integrity_failed".into());
    } else if !continuous || !observation_started {
        registration.activity_evidence_paused_reason = Some("activity_evidence_gap".into());
    } else if evidence_restarted {
        registration.activity_observation_started_at_world_tick = Some(now);
        registration.activity_evidence_complete_through_world_tick = now;
        registration.activity_evidence_paused_reason = None;
    } else if !matches!(
        prior_pause,
        Some("activity_evidence_gap" | "activity_source_integrity_failed")
    ) {
        registration.activity_evidence_complete_through_world_tick = now;
        registration.activity_evidence_paused_reason = None;
    }
    if !catalog_integrity || !continuous || !observation_started {
        let reason = if catalog_integrity {
            "activity-evidence-gap"
        } else {
            "activity-source-integrity-failed"
        };
        let cancellation_ref = format!("{reason}:{close_hash}");
        for case in continuity.cases.values_mut().filter(|case| {
            case.organization_id == registration.organization_id
                && case.origin == OrganizationCaseOriginV1::Dormancy
                && !matches!(
                    case.phase,
                    OrganizationContinuityPhaseV1::Resolved
                        | OrganizationContinuityPhaseV1::Cancelled
                )
        }) {
            case.phase = OrganizationContinuityPhaseV1::Cancelled;
            case.revision = case.revision.saturating_add(1);
            if !case.stage_receipts.contains(&cancellation_ref) {
                case.stage_receipts.push(cancellation_ref.clone());
            }
        }
    }
    continuity
        .registrations
        .insert(registration.organization_id.clone(), registration);
    Ok(close_hash.to_owned())
}

fn apply_advance_phase_from_current_v1(
    continuity: &mut OrganizationContinuityStateV1,
    facts: &OrganizationCurrentLifecycleFactsV1,
    case_id: &str,
    expected_revision: u64,
    actor: &str,
    key: &str,
    now: u64,
) -> Result<String, WorldError> {
    let mut case = checked_v1_case(continuity, case_id, expected_revision)?;
    let registration = continuity
        .registrations
        .get(&case.organization_id)
        .ok_or_else(|| invalid("organization_registration_missing"))?
        .clone();
    let root = current_manifest_root_for_organization(facts, &registration)?;
    if root != registration.manifest_root
        || registration.manifest_root != case.manifest_root
        || registration.manifest_asset_refs != case.manifest_asset_refs
    {
        return Err(invalid("organization_case_manifest_drift"));
    }
    if case.origin == OrganizationCaseOriginV1::Dormancy
        && !activity_evidence_complete(&registration, now)
    {
        return Err(invalid("organization_activity_evidence_unavailable"));
    }
    let founder_authorized = actor == registration.founder_id
        && current_controller_binding_matches(facts, &registration, actor);
    let appointment_authorized = registration.appointment_authorities.contains(actor)
        && current_controller_binding_matches(facts, &registration, actor);
    let member_authorized =
        current_actor_is_organization_holder(facts, continuity, &registration, actor);
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
    continuity.cases.insert(case_id.to_owned(), case);
    Ok(case_id.to_owned())
}

#[allow(clippy::too_many_arguments)]
fn apply_publish_notice_from_current_v1(
    continuity: &mut OrganizationContinuityStateV1,
    facts: &OrganizationCurrentLifecycleFactsV1,
    case_id: &str,
    expected_revision: u64,
    recipient_ids: &BTreeSet<String>,
    scope_refs: &BTreeSet<String>,
    reason_code: &str,
    trigger_fact_ref: &str,
    evidence_refs: &[String],
    allowed_actions: &BTreeSet<String>,
    restricted_actions: &BTreeSet<String>,
    actor: &str,
    key: &str,
    now: u64,
) -> Result<String, WorldError> {
    let mut case = checked_v1_case(continuity, case_id, expected_revision)?;
    if case.phase != OrganizationContinuityPhaseV1::AwaitingNotice {
        return Err(invalid("organization_notice_already_published"));
    }
    let expected_recipients =
        current_expected_recipients(facts, continuity, &case.organization_id)?;
    let expected_reason = match case.origin {
        OrganizationCaseOriginV1::Dormancy => "organization_dormancy_continuity",
        OrganizationCaseOriginV1::VoluntaryDissolution => "organization_voluntary_dissolution",
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
            .any(|value| value == &case.manifest_root)
        || !evidence_refs
            .iter()
            .any(|value| value == &case.registration_receipt)
        || allowed_actions != &default_notice_allowed_actions()
        || restricted_actions != &default_notice_restricted_actions()
    {
        return Err(invalid(
            "organization_notice_content_incomplete_or_untrusted",
        ));
    }
    let registration = continuity
        .registrations
        .get(&case.organization_id)
        .ok_or_else(|| invalid("organization_registration_missing"))?;
    let appointment_authorized = registration.appointment_authorities.contains(actor)
        && current_controller_binding_matches(facts, registration, actor);
    if !current_actor_is_organization_holder(facts, continuity, registration, actor)
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
    continuity.cases.insert(case_id.to_owned(), case);
    Ok(key.to_owned())
}

/// Old World compatibility adapter. This keeps both legacy stages on the
/// exact same fixed Current facts kernel used by the incremental producer.
pub(super) fn old_world_facts(
    state: &WorldState,
    continuity: &OrganizationContinuityStateV1,
    organization_id: &str,
) -> OrganizationCurrentLifecycleFactsV1 {
    OrganizationCurrentLifecycleFactsV1::from_world(state, continuity, organization_id)
}

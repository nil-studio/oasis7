//! Persisted, fail-closed organization continuity v1 state machine.
//!
//! Only explicitly enabled organizations participate. Legacy alliance text,
//! Agent activity timestamps, and self-reported external facts are never used
//! as evidence for continuity, qualification, or estate ownership.

use super::super::{
    PreparedWorldRule, WorldRulesStateV1, invalid, require_governance, verify_world_rule_authority,
};
use crate::{MaterialLedgerId, WorldError, WorldState};
use oasis7_agent_api::world_action::{
    GovernanceFinalityCertificate, OrganizationContinuityCommandV1 as Command,
    OrganizationEstateOperationV1 as EstateOperation, OrganizationRemedyKindV1 as RemedyKind,
    OrganizationRemedyOutcomeV1 as RemedyOutcome, OrganizationVoluntaryCaseKindV1 as VoluntaryKind,
    WorldRuleCommandV1,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[path = "organization_continuity_commands.rs"]
mod commands;

#[path = "organization_continuity_types.rs"]
mod types;
pub use types::*;
#[path = "organization_continuity_helpers.rs"]
mod helpers;
use helpers::*;
pub use helpers::{activity_catalog_root, economic_contract_terms_hash};
pub use helpers::{
    controller_binding_from_owner_v1, controller_binding_matches_owner_v1,
    economic_contract_terms_hash_with_work_v1,
};

#[path = "organization_continuity_system.rs"]
mod system;
pub use system::{
    OrganizationCurrentAgentFactV1, OrganizationCurrentAllianceFactV1,
    OrganizationCurrentContractFactV1, OrganizationCurrentEstateFactV1,
    OrganizationCurrentLifecycleFactsV1, OrganizationCurrentOwnerV1,
    OrganizationCurrentRequestFactV1, apply_system_lifecycle_command_from_current_v1,
    prepare_activity_close_command_for_registration_v1,
    prepare_activity_close_commands_from_current_v1, prepare_lifecycle_commands_from_current_v1,
    prepare_lifecycle_commands_selected_from_current_v1,
};

/// A plain fixed semantic plan, never a source or execution capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MembershipEventPlanV1 {
    Noop,
    EnsureHistory,
    Join,
    Close,
}
/// Shared with the typed Current owner. The callback only charges work; every
/// predicate and the original already-open rejection remain inside this kernel.
#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_membership_event_with_work_v1<E: From<WorldError>>(
    registered: bool,
    history: Option<&[(u64, Option<u64>, String)]>,
    member_id: &str,
    world_tick: u64,
    event_ref: &str,
    joined: bool,
    work: &mut impl FnMut(usize) -> Result<(), E>,
) -> Result<MembershipEventPlanV1, E> {
    work(member_id.len())?;
    work(event_ref.len())?;
    if !registered || member_id.is_empty() || event_ref.is_empty() || world_tick == 0 {
        return Ok(MembershipEventPlanV1::Noop);
    }
    for (_, _, reference) in history.unwrap_or(&[]) {
        work(reference.len())?;
        work(event_ref.len())?;
        if reference == event_ref {
            return Ok(MembershipEventPlanV1::Noop);
        }
    }
    let open = history
        .and_then(|events| events.last())
        .is_some_and(|(_, left, _)| left.is_none());
    work(std::mem::size_of::<(u64, Option<u64>)>())?;
    if joined {
        if open {
            return Err(invalid("organization_membership_already_open").into());
        }
        Ok(MembershipEventPlanV1::Join)
    } else if open {
        Ok(MembershipEventPlanV1::Close)
    } else {
        Ok(MembershipEventPlanV1::EnsureHistory)
    }
}

fn accounted_through(
    continuity: &OrganizationContinuityStateV1,
    registration: &OrganizationRegistrationV1,
) -> u64 {
    registration
        .last_accounted_world_tick
        .max(registration.enabled_at_world_tick)
        .max(
            continuity
                .availability
                .get(&registration.organization_id)
                .and_then(|intervals| intervals.last())
                .map_or(0, |interval| interval.through_world_tick),
        )
}

fn activity_evidence_complete(registration: &OrganizationRegistrationV1, through: u64) -> bool {
    !registration.activity_sources.is_empty()
        && registration
            .activity_observation_started_at_world_tick
            .is_some()
        && registration.activity_evidence_paused_reason.is_none()
        && registration.activity_evidence_complete_through_world_tick >= through
}

pub fn activity_close_commitment(
    registration: &OrganizationRegistrationV1,
    through_world_tick: u64,
    previous_close_hash: Option<&str>,
    source_catalog_root: &str,
    through_event_id: u64,
    through_event_hash: &str,
) -> Result<String, WorldError> {
    crate::util::hash_json(&(
        "oasis7.organization-continuity.activity-close.v1",
        &registration.world_binding,
        &registration.registration_receipt,
        &registration.organization_id,
        through_world_tick,
        previous_close_hash,
        source_catalog_root,
        through_event_id,
        through_event_hash,
    ))
}

impl OrganizationContinuityStateV1 {
    pub fn activity_contract_candidates(
        &self,
        state: &WorldState,
        reader: Option<&str>,
        world_binding: Option<&str>,
    ) -> Vec<OrganizationActivityContractCandidateV1> {
        crate::world_rules::readonly_organization::legacy_candidates(
            state,
            self,
            reader,
            world_binding,
        )
    }

    /// Observe only a previously registered AtomicExchange. The economic
    /// reducer calls this while preparing the same settlement delta, so the
    /// activity receipt is committed atomically with the transfer.
    pub fn record_atomic_exchange_settlement(
        &mut self,
        state: &WorldState,
        contract: &crate::EconomicContractState,
        now: u64,
        world_binding: Option<&str>,
    ) -> Result<bool, WorldError> {
        if contract.fulfillment_kind != crate::EconomicContractFulfillmentKind::AtomicExchange
            || contract.status != crate::EconomicContractStatus::Settled
            || contract.settlement_success != Some(true)
            || contract.transfer_amount <= 0
        {
            return Ok(false);
        }
        let organizations = self
            .registrations
            .values()
            .filter(|registration| {
                world_binding == Some(registration.world_binding.as_str())
                    && registration
                        .activity_sources
                        .contains_key(&contract.contract_id)
            })
            .map(|registration| registration.organization_id.clone())
            .collect::<Vec<_>>();
        let mut changed = false;
        for organization_id in organizations {
            let valid = self
                .registrations
                .get(&organization_id)
                .is_some_and(|registration| {
                    registration
                        .activity_sources
                        .get(&contract.contract_id)
                        .is_some_and(|source| {
                            let terms_match = economic_contract_terms_hash(contract)
                                .is_ok_and(|hash| hash == source.terms_hash);
                            let creator_matches =
                                controller_binding_for(state, &contract.creator_agent_id)
                                    .is_ok_and(|binding| binding == source.creator_binding);
                            let counterparty_matches =
                                controller_binding_for(state, &contract.counterparty_agent_id)
                                    .is_ok_and(|binding| binding == source.counterparty_binding);
                            let counterparty_is_external =
                                counterparty_is_external_to_current_members(
                                    state,
                                    &registration.organization_id,
                                    &contract.counterparty_agent_id,
                                );
                            source.world_binding == registration.world_binding
                                && source.registered_at_world_tick <= now
                                && contract.creator_agent_id != contract.counterparty_agent_id
                                && contract.expires_at >= now
                                && contract.transfer_amount == contract.settlement_amount
                                && terms_match
                                && creator_matches
                                && counterparty_matches
                                && counterparty_is_external
                        })
                });
            if !valid {
                if let Some(registration) = self.registrations.get_mut(&organization_id) {
                    registration.activity_evidence_paused_reason =
                        Some("activity_source_integrity_failed".into());
                    changed = true;
                }
                continue;
            }
            let source_ref = format!("economic-contract-settlement:{}", contract.contract_id);
            changed |= self.record_activity_from_event(
                &organization_id,
                &source_ref,
                now,
                OrganizationActivityKindV1::ExternalAtomicExchangeSettlement,
            )?;
        }
        Ok(changed)
    }

    // Preserve the full event-cursor and catalog commitment at this replay boundary.
    #[allow(clippy::too_many_arguments)]
    pub fn close_activity_tick(
        &mut self,
        state: &WorldState,
        organization_id: &str,
        now: u64,
        previous_close_hash: Option<&str>,
        source_catalog_root: &str,
        through_event_id: u64,
        through_event_hash: &str,
        close_hash: &str,
    ) -> Result<String, WorldError> {
        let registration = self
            .registrations
            .get(organization_id)
            .cloned()
            .ok_or_else(|| invalid("organization_continuity_not_enabled"))?;
        let facts = system::old_world_facts(state, self, organization_id);
        system::apply_close_activity_tick_from_current_v1(
            self,
            &facts,
            &registration,
            now,
            previous_close_hash,
            source_catalog_root,
            through_event_id,
            through_event_hash,
            close_hash,
        )
    }

    // The authoritative ledger adapter supplies one exact deposit fact tuple.
    #[allow(clippy::too_many_arguments)]
    pub fn accept_material_deposit(
        &mut self,
        organization_id: &str,
        asset_ref: &str,
        source_receipt: &str,
        contributor_id: &str,
        material: &str,
        amount: i64,
        held_in_trust: bool,
    ) -> Result<(), WorldError> {
        let registration = self
            .registrations
            .get(organization_id)
            .ok_or_else(|| invalid("organization_continuity_enable_required"))?;
        if registration.policy_version != ORGANIZATION_CONTINUITY_POLICY_VERSION_V1
            || registration.world_binding.is_empty()
            || asset_ref.is_empty()
            || source_receipt.is_empty()
            || contributor_id.is_empty()
            || material.is_empty()
            || amount <= 0
            || amount > 1_000_000_000
            || self.assets.contains_key(asset_ref)
            || self.cases.values().any(|case| {
                case.organization_id == organization_id
                    && !matches!(
                        case.phase,
                        OrganizationContinuityPhaseV1::Resolved
                            | OrganizationContinuityPhaseV1::Cancelled
                    )
            })
        {
            return Err(invalid("organization_material_deposit_not_admissible"));
        }
        let current_assets = self
            .assets
            .values()
            .filter(|asset| asset.organization_id == organization_id)
            .cloned()
            .collect::<Vec<_>>();
        if manifest_commitment_root(&current_assets)? != registration.manifest_root {
            return Err(invalid("organization_manifest_drift"));
        }
        let mut assets = current_assets;
        let new_asset = OrganizationManifestAssetV1 {
            asset_ref: asset_ref.to_owned(),
            source_receipt: source_receipt.to_owned(),
            organization_id: organization_id.to_owned(),
            contributor_id: contributor_id.to_owned(),
            material: material.to_owned(),
            amount,
            held_in_trust,
            version: 1,
            disposed: false,
            effect_receipt: None,
        };
        assets.push(new_asset.clone());
        if assets.len() > MAX_ASSETS_PER_REGISTRATION {
            return Err(invalid("organization_manifest_over_limit"));
        }
        assets.sort_by(|left, right| left.asset_ref.cmp(&right.asset_ref));
        let root = manifest_commitment_root(&assets)?;
        let asset_refs = assets.iter().map(|asset| asset.asset_ref.clone()).collect();
        self.assets.insert(asset_ref.to_owned(), new_asset);
        let registration = self
            .registrations
            .get_mut(organization_id)
            .expect("registration validated above");
        registration.manifest_root = root;
        registration.manifest_asset_refs = asset_refs;
        Ok(())
    }

    /// Called only from a validated production/settlement receipt adapter.
    /// Public commands cannot manufacture activity receipts.
    pub fn record_activity_from_event(
        &mut self,
        organization_id: &str,
        source_ref: &str,
        world_tick: u64,
        kind: OrganizationActivityKindV1,
    ) -> Result<bool, WorldError> {
        if !self.registrations.contains_key(organization_id)
            || source_ref.is_empty()
            || source_ref.len() > 256
            || world_tick == 0
        {
            return Ok(false);
        }
        let records = self
            .activity_receipts
            .entry(organization_id.to_owned())
            .or_default();
        if records.contains_key(source_ref) {
            return Ok(false);
        }
        records.insert(
            source_ref.to_owned(),
            OrganizationActivityReceiptV1 {
                source_ref: source_ref.to_owned(),
                world_tick,
                kind,
            },
        );
        Ok(true)
    }

    /// Called by the authoritative alliance event reducer; initial legacy
    /// membership starts at explicit v1 enablement, never at an inferred tick.
    pub fn record_membership_from_event(
        &mut self,
        state: &WorldState,
        organization_id: &str,
        member_id: &str,
        world_tick: u64,
        event_ref: &str,
        joined: bool,
    ) -> Result<(), WorldError> {
        if !self.registrations.contains_key(organization_id)
            || member_id.is_empty()
            || event_ref.is_empty()
            || world_tick == 0
        {
            return Ok(());
        }
        // Preserve the legacy observable entry creation point, including a
        // leave with no open history and errors after this point.
        let events = self
            .membership_history
            .entry(organization_id.to_owned())
            .or_default()
            .entry(member_id.to_owned())
            .or_default();
        let plan = plan_membership_event_with_work_v1(
            true,
            Some(events),
            member_id,
            world_tick,
            event_ref,
            joined,
            &mut |_| Ok::<(), WorldError>(()),
        )?;
        match plan {
            MembershipEventPlanV1::Noop | MembershipEventPlanV1::EnsureHistory => {}
            MembershipEventPlanV1::Join => {
                let binding = controller_binding_for(state, member_id)?;
                events.push((world_tick, None, event_ref.to_owned()));
                self.registrations
                    .get_mut(organization_id)
                    .expect("registration checked above")
                    .membership_controller_bindings
                    .insert(member_id.to_owned(), binding);
            }
            MembershipEventPlanV1::Close => {
                // The fixed plan observed this exact borrowed last row.
                if let Some((_, left, _)) = events.last_mut() {
                    *left = Some(world_tick);
                }
                if let Some(registration) = self.registrations.get_mut(organization_id) {
                    registration
                        .membership_controller_bindings
                        .remove(member_id);
                }
            }
        }
        Ok(())
    }

    pub fn lifecycle_commands(
        &self,
        state: &WorldState,
        now: u64,
    ) -> Result<Vec<(String, String, Command)>, WorldError> {
        let mut facts = BTreeMap::new();
        for organization_id in self.registrations.keys() {
            facts.insert(
                organization_id.clone(),
                system::old_world_facts(state, self, organization_id),
            );
        }
        system::prepare_lifecycle_commands_from_current_v1(self, &facts, now)
    }

    pub fn continuity_clock_at(&self, organization_id: &str, world_tick: u64) -> u64 {
        let Some(registration) = self.registrations.get(organization_id) else {
            return 0;
        };
        self.availability
            .get(organization_id)
            .into_iter()
            .flatten()
            .filter(|interval| {
                interval.evidence_status == OrganizationAvailabilityEvidenceStatusV1::Certified
                    && interval.world_binding == registration.world_binding
                    && interval.registration_receipt == registration.registration_receipt
                    && interval.world_writes_available
                    && interval.notice_available
                    && interval.remedy_available
                    && interval.from_world_tick <= world_tick
                    && interval.through_world_tick >= registration.enabled_at_world_tick
            })
            .map(|interval| {
                let start = interval
                    .from_world_tick
                    .max(registration.enabled_at_world_tick.saturating_add(1));
                let end = interval.through_world_tick.min(world_tick);
                end.saturating_sub(start).saturating_add(1)
            })
            .sum()
    }
}

impl PreparedWorldRule {
    // Forward the authenticated event context without merging independent authority fields.
    #[allow(clippy::too_many_arguments)]
    pub fn organization_continuity(
        &mut self,
        state: &WorldState,
        actor: &str,
        key: &str,
        command: &Command,
        now: u64,
        world_binding: &str,
        is_system_event: bool,
    ) -> Result<String, WorldError> {
        let mut continuity = self.rules.organization_continuity.clone();
        let result = if is_system_event && is_system_lifecycle_command_v1(command) {
            apply_system_lifecycle_command_v1(
                state,
                &mut self.rules,
                &mut continuity,
                actor,
                key,
                command,
                now,
                world_binding,
                &mut self.ledgers,
            )?
        } else {
            commands::apply_continuity_command(
                state,
                &mut self.rules,
                &mut continuity,
                actor,
                key,
                command,
                now,
                world_binding,
                is_system_event,
                &mut self.ledgers,
            )?
        };
        self.rules.organization_continuity = continuity;
        Ok(result)
    }
}

/// The fixed subset emitted by the normal tail. This dispatch is deliberately
/// separate from public command admission so the tail cannot accidentally
/// route another continuity command through its system-event path.
pub fn is_system_lifecycle_command_v1(command: &Command) -> bool {
    matches!(
        command,
        Command::CloseActivityTick { .. }
            | Command::FinalizeAvailabilityUnknown { .. }
            | Command::OpenDormancyCase { .. }
            | Command::AdvancePhase { .. }
            | Command::PublishNotice { .. }
    )
}

/// Apply one of the five fixed tail commands through the original continuity
/// reducer. Callers still have to provide the selected Current facts used by
/// the reducer and validate the Current source after the sparse write; this is
/// a reducer entry point, not a qualification or publication capability.
#[allow(clippy::too_many_arguments)]
pub fn apply_system_lifecycle_command_v1(
    state: &WorldState,
    rules: &mut WorldRulesStateV1,
    continuity: &mut OrganizationContinuityStateV1,
    actor: &str,
    key: &str,
    command: &Command,
    now: u64,
    world_binding: &str,
    ledgers: &mut BTreeMap<MaterialLedgerId, BTreeMap<String, i64>>,
) -> Result<String, WorldError> {
    if !is_system_lifecycle_command_v1(command) {
        return Err(invalid("organization_system_lifecycle_command_required"));
    }
    commands::apply_continuity_command(
        state,
        rules,
        continuity,
        actor,
        key,
        command,
        now,
        world_binding,
        true,
        ledgers,
    )
}

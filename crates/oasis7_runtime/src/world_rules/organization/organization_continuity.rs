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
pub(crate) use helpers::activity_catalog_root;
#[cfg(test)]
pub(crate) use helpers::economic_contract_terms_hash;
use helpers::*;

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

pub(crate) fn activity_close_commitment(
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
    pub(crate) fn activity_contract_candidates(
        &self,
        state: &WorldState,
        reader: Option<&str>,
        world_binding: Option<&str>,
    ) -> Vec<OrganizationActivityContractCandidateV1> {
        let Some(reader) = reader else {
            return Vec::new();
        };
        let Some(world_binding) = world_binding else {
            return Vec::new();
        };
        let mut candidates = Vec::new();
        for registration in self.registrations.values() {
            if registration.founder_id != reader
                || registration.policy_version != ORGANIZATION_CONTINUITY_POLICY_VERSION_V1
                || registration.world_binding != world_binding
                || !controller_binding_matches(state, registration, reader)
                || self.cases.values().any(|case| {
                    case.organization_id == registration.organization_id
                        && !matches!(
                            case.phase,
                            OrganizationContinuityPhaseV1::Resolved
                                | OrganizationContinuityPhaseV1::Cancelled
                        )
                })
            {
                continue;
            }
            let Some(alliance) = state.alliances.get(&registration.organization_id) else {
                continue;
            };
            let Some(founder_binding) = registration.controller_bindings.get(reader) else {
                continue;
            };
            let Ok(member_bindings) = alliance
                .members
                .iter()
                .map(|member| controller_binding_for(state, member))
                .collect::<Result<Vec<_>, _>>()
            else {
                continue;
            };
            for contract in state.economic_contracts.values() {
                if contract.creator_agent_id != reader
                    || contract.counterparty_agent_id == reader
                    || contract.fulfillment_kind
                        != crate::EconomicContractFulfillmentKind::AtomicExchange
                    || contract.status != crate::EconomicContractStatus::Accepted
                    || contract.expires_at < state.time
                    || contract.settlement_amount <= 0
                    || contract.reputation_stake < 0
                    || registration
                        .activity_sources
                        .contains_key(&contract.contract_id)
                    || self.registrations.values().any(|other| {
                        other.organization_id != registration.organization_id
                            && other.activity_sources.contains_key(&contract.contract_id)
                    })
                {
                    continue;
                }
                let Ok(counterparty_binding) =
                    controller_binding_for(state, &contract.counterparty_agent_id)
                else {
                    continue;
                };
                let overlaps_member = member_bindings.iter().any(|member| {
                    member.player_id == counterparty_binding.player_id
                        || member.root_agent_id == counterparty_binding.root_agent_id
                }) || founder_binding.player_id
                    == counterparty_binding.player_id
                    || founder_binding.root_agent_id == counterparty_binding.root_agent_id;
                if overlaps_member {
                    continue;
                }
                let Ok(expected_terms_hash) = economic_contract_terms_hash(contract) else {
                    continue;
                };
                candidates.push(OrganizationActivityContractCandidateV1 {
                    organization_id: registration.organization_id.clone(),
                    contract_id: contract.contract_id.clone(),
                    expected_terms_hash,
                    expires_at: contract.expires_at,
                });
            }
        }
        candidates.sort_by(|left, right| {
            (&left.organization_id, &left.contract_id)
                .cmp(&(&right.organization_id, &right.contract_id))
        });
        candidates
    }

    /// Observe only a previously registered AtomicExchange. The economic
    /// reducer calls this while preparing the same settlement delta, so the
    /// activity receipt is committed atomically with the transfer.
    pub(crate) fn record_atomic_exchange_settlement(
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

    pub(crate) fn close_activity_tick(
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
        let mut registration = self
            .registrations
            .get(organization_id)
            .cloned()
            .ok_or_else(|| invalid("organization_continuity_not_enabled"))?;
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

        let catalog_integrity = registration.activity_sources.values().all(|source| {
            let Some(contract) = state.economic_contracts.get(&source.contract_id) else {
                return false;
            };
            if !economic_contract_terms_hash(contract)
                .is_ok_and(|hash| hash == source.terms_hash)
                || source.world_binding != registration.world_binding
            {
                return false;
            }
            if contract.status == crate::EconomicContractStatus::Accepted {
                return controller_binding_for(state, &contract.creator_agent_id)
                    .is_ok_and(|binding| binding == source.creator_binding)
                    && controller_binding_for(state, &contract.counterparty_agent_id)
                        .is_ok_and(|binding| binding == source.counterparty_binding)
                    && counterparty_is_external_to_current_members(
                        state,
                        organization_id,
                        &contract.counterparty_agent_id,
                    );
            }
            if contract.status == crate::EconomicContractStatus::Settled {
                let expected_receipt = format!("economic-contract-settlement:{}", source.contract_id);
                return contract.settlement_success == Some(true)
                    && contract.transfer_amount == contract.settlement_amount
                    && contract.settled_at.is_some_and(|settled_at| {
                        self.activity_receipts
                            .get(organization_id)
                            .and_then(|receipts| receipts.get(&expected_receipt))
                            .is_some_and(|receipt| {
                                receipt.world_tick == settled_at
                                    && receipt.kind
                                        == OrganizationActivityKindV1::ExternalAtomicExchangeSettlement
                            })
                    });
            }
            true
        });
        let expected_tick = registration
            .activity_close_cursor_world_tick
            .saturating_add(1);
        let continuous = now == expected_tick;
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
            // Do not let a later close retroactively certify the unknown
            // interval. Start a new complete observation window here.
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
            let cancellation_ref = format!("{reason}:{}", close_hash);
            for case in self.cases.values_mut().filter(|case| {
                case.organization_id == organization_id
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
        self.registrations
            .insert(organization_id.to_owned(), registration);
        Ok(close_hash.to_owned())
    }

    pub(crate) fn accept_material_deposit(
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
    pub(crate) fn record_activity_from_event(
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
    pub(crate) fn record_membership_from_event(
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
        let events = self
            .membership_history
            .entry(organization_id.to_owned())
            .or_default()
            .entry(member_id.to_owned())
            .or_default();
        if events
            .iter()
            .any(|(_, _, reference)| reference == event_ref)
        {
            return Ok(());
        }
        if joined {
            if events.last().is_some_and(|(_, left, _)| left.is_none()) {
                return Err(invalid("organization_membership_already_open"));
            }
            let binding = controller_binding_for(state, member_id)?;
            events.push((world_tick, None, event_ref.to_owned()));
            self.registrations
                .get_mut(organization_id)
                .expect("registration checked above")
                .membership_controller_bindings
                .insert(member_id.to_owned(), binding);
        } else if let Some((_, left, _)) = events.last_mut() {
            if left.is_none() {
                *left = Some(world_tick);
                if let Some(registration) = self.registrations.get_mut(organization_id) {
                    registration
                        .membership_controller_bindings
                        .remove(member_id);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn lifecycle_commands(
        &self,
        state: &WorldState,
        now: u64,
    ) -> Result<Vec<(String, String, Command)>, WorldError> {
        let mut work = Vec::new();
        for registration in self.registrations.values() {
            let accounted_through = accounted_through(self, registration);
            if accounted_through < now {
                let request_id = format!(
                    "organization-continuity:availability-unknown:{}:{}",
                    short_id(&registration.organization_id),
                    now
                );
                work.push((
                    registration.founder_id.clone(),
                    request_id,
                    Command::FinalizeAvailabilityUnknown {
                        organization_id: registration.organization_id.clone(),
                        through_world_tick: now,
                    },
                ));
                continue;
            }
            let currently_available = self
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
            let clock = self.continuity_clock_at(&registration.organization_id, now);
            let active_case = self.cases.values().find(|case| {
                case.organization_id == registration.organization_id
                    && !matches!(
                        case.phase,
                        OrganizationContinuityPhaseV1::Resolved
                            | OrganizationContinuityPhaseV1::Cancelled
                    )
            });
            if let Some(case) = active_case {
                if case.origin == OrganizationCaseOriginV1::Dormancy
                    && !activity_evidence_complete(registration, now)
                {
                    continue;
                }
                let command = match case.phase {
                    OrganizationContinuityPhaseV1::AwaitingNotice => {
                        let recipients = expected_recipients(state, self, &case.organization_id)?;
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
                            self,
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
                    let request_id = format!(
                        "organization-continuity:lifecycle:{}:{}:{}",
                        short_id(&case.case_id),
                        case.revision,
                        command_kind(&command)
                    );
                    work.push((registration.founder_id.clone(), request_id, command));
                }
                continue;
            }
            if !activity_evidence_complete(registration, now) {
                continue;
            }
            let Ok(current_root) = manifest_root_for_organization(state, registration) else {
                continue;
            };
            if current_root != registration.manifest_root {
                continue;
            }
            let last_activity_clock = self
                .activity_receipts
                .get(&registration.organization_id)
                .and_then(|items| {
                    items
                        .values()
                        .filter_map(|item| {
                            activity_clock(self, &registration.organization_id, item.world_tick)
                                .map(|at| (at, item.source_ref.as_str()))
                        })
                        .max_by_key(|(at, _)| *at)
                        .map(|(at, ref_)| (at, ref_.to_owned()))
                });
            let Some(observation_start_clock) = registration
                .activity_observation_started_at_world_tick
                .map(|tick| self.continuity_clock_at(&registration.organization_id, tick))
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
                let request_id = format!(
                    "organization-continuity:dormancy:{}:{}",
                    short_id(&registration.organization_id),
                    clock
                );
                work.push((
                    registration.founder_id.clone(),
                    request_id,
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
    pub(crate) fn organization_continuity(
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
        let result = commands::apply_continuity_command(
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
        )?;
        self.rules.organization_continuity = continuity;
        Ok(result)
    }
}

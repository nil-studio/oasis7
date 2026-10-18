//! Typed, borrowed-base staging for trusted capability commands.
//!
//! This stage owns only the projections a command can touch.  The canonical
//! [`World`] remains read-only until [`PreparedTrustedCommand::install`], whose
//! assignments are deliberately infallible after preparation has completed.

use oasis7_wasm_abi::{
    ModuleArtifact, ModuleCallErrorCode, ModuleCallFailure, ModuleCallRequest, ModuleOutput,
    ModuleSandbox, canonical_hash,
};
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use super::super::state::{CommandStateOverlay, WorldStateProjection};
use super::super::{
    CapabilityAuthorizationEvent, EffectIntent, EffectOrigin, ModuleManifest,
    ModuleRuntimeChargeEvent, PolicyDecisionRecord, WorldError, WorldEvent, WorldEventBody,
    WorldTime,
};
use super::World;
use super::prepared_base_head::WorldPreparedBaseHead;
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_agent_api::world_entity::ModuleVisualEntity;

const MODULE_RUNTIME_FEE_BYTES_PER_UNIT: u64 = 1_024;

pub(super) trait ModuleCallTarget {
    fn active_manifest_for_call(&self, module_id: &str) -> Result<ModuleManifest, WorldError>;
    fn current_manifest_hash_for_call(&self) -> Result<String, WorldError>;
    fn state_time_for_call(&self) -> WorldTime;
    fn journal_height_for_call(&self) -> u64;
    fn call_module_raw_for_call(
        &mut self,
        module_id: &str,
        trace_id: &str,
        input: Vec<u8>,
        manifest: &ModuleManifest,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<ModuleOutput, ModuleCallFailure>;
    fn module_call_failed_for_call(&mut self, failure: ModuleCallFailure)
    -> Result<(), WorldError>;
    fn append_module_event_for_call(&mut self, body: WorldEventBody) -> Result<u64, WorldError>;
    fn try_charge_module_runtime_for_call(
        &mut self,
        module_id: &str,
        trace_id: &str,
        manifest: &ModuleManifest,
        input_bytes: u64,
        output: &ModuleOutput,
    ) -> Result<(), ModuleCallFailure>;
    fn build_effect_intent_for_call(
        &mut self,
        kind: String,
        params: serde_json::Value,
        cap_ref: String,
        origin: EffectOrigin,
    ) -> Result<EffectIntent, WorldError>;
}

pub(super) fn execute_module_call_for_target<T: ModuleCallTarget + ?Sized>(
    target: &mut T,
    module_id: &str,
    state_key: &str,
    manifest: &ModuleManifest,
    trace_id: String,
    input: Vec<u8>,
    sandbox: &mut dyn ModuleSandbox,
) -> Result<ModuleOutput, WorldError> {
    let input_bytes = input.len() as u64;
    let output =
        match target.call_module_raw_for_call(module_id, &trace_id, input, manifest, sandbox) {
            Ok(output) => output,
            Err(failure) => {
                target.module_call_failed_for_call(failure)?;
                unreachable!("module_call_failed_for_call always returns Err")
            }
        };
    super::module_runtime::process_module_output_for_target(
        target,
        module_id,
        state_key,
        &trace_id,
        manifest,
        input_bytes,
        &output,
        sandbox,
    )?;
    Ok(output)
}

fn metering_units(bytes: u64) -> i64 {
    if bytes == 0 {
        return 0;
    }
    let units = bytes.saturating_add(MODULE_RUNTIME_FEE_BYTES_PER_UNIT - 1)
        / MODULE_RUNTIME_FEE_BYTES_PER_UNIT;
    i64::try_from(units).unwrap_or(i64::MAX)
}

pub(super) struct PreparedCapabilityAuthorizationProjection {
    pub(super) capability_grants_v2: BTreeMap<String, JsonValue>,
    pub(super) capability_nonce_records:
        BTreeMap<String, super::super::CapabilityAuthorizationNonceRecord>,
    pub(super) capability_authorization_receipts:
        BTreeMap<String, super::super::CapabilityAuthorizationAuditReceipt>,
    pub(super) capability_budget_accounts: BTreeMap<String, super::super::CapabilityBudgetAccount>,
    pub(super) capability_effect_receipt_links:
        BTreeMap<String, super::super::CapabilityEffectReceiptLink>,
    pub(super) capability_authorization_root: String,
}

pub(super) struct PreparedModuleTickRoutingSample {
    pub(super) schedule_len: usize,
    pub(super) due_count: usize,
    pub(super) invoked_count: usize,
    pub(super) missing_invocation_count: usize,
    pub(super) oldest_overdue_ticks: Option<u64>,
    pub(super) duration: Duration,
}

/// All state that a trusted command can publish after validation.
pub(super) struct PreparedTrustedCommand {
    pub(super) base_head: WorldPreparedBaseHead,
    pub(super) module_cache_additions: Vec<ModuleArtifact>,
    pub(super) module_states: BTreeMap<String, Vec<u8>>,
    pub(super) module_visual_entities: BTreeMap<String, ModuleVisualEntity>,
    pub(super) agent_updates: BTreeMap<String, super::super::agent_cell::AgentCell>,
    pub(super) resource_updates: BTreeMap<ResourceKind, i64>,
    pub(super) pending_effects: VecDeque<EffectIntent>,
    pub(super) pending_effects_evicted: u64,
    pub(super) next_event_id: u64,
    pub(super) next_event_id_era: u64,
    pub(super) next_intent_id: u64,
    pub(super) next_intent_id_era: u64,
    pub(super) journal_events: Vec<WorldEvent>,
    pub(super) journal_events_evicted: u64,
    pub(super) module_tick_schedule: BTreeMap<String, WorldTime>,
    pub(super) module_tick_routing_sample: Option<PreparedModuleTickRoutingSample>,
    pub(super) consensus_record: Option<super::super::TickConsensusRecord>,
    pub(super) capability_authorization: Option<PreparedCapabilityAuthorizationProjection>,
}

/// A command-local overlay over immutable canonical state.
pub(super) struct TrustedCommandStage<'a> {
    base: &'a World,
    base_head: WorldPreparedBaseHead,
    module_cache: oasis7_wasm_abi::ModuleCache,
    module_cache_additions: Vec<ModuleArtifact>,
    module_states: BTreeMap<String, Vec<u8>>,
    module_visual_entities: BTreeMap<String, ModuleVisualEntity>,
    agent_updates: BTreeMap<String, super::super::agent_cell::AgentCell>,
    resource_updates: BTreeMap<ResourceKind, i64>,
    pending_effects: VecDeque<EffectIntent>,
    pending_effects_evicted: u64,
    next_event_id: u64,
    next_event_id_era: u64,
    next_intent_id: u64,
    next_intent_id_era: u64,
    events: Vec<WorldEvent>,
    module_tick_schedule: BTreeMap<String, WorldTime>,
    module_tick_routing_sample: Option<PreparedModuleTickRoutingSample>,
}

impl<'a> TrustedCommandStage<'a> {
    pub(super) fn new(base: &'a World) -> Result<Self, WorldError> {
        Ok(Self {
            base,
            base_head: WorldPreparedBaseHead::capture(base)?,
            module_cache: base.module_cache.clone(),
            module_cache_additions: Vec::new(),
            module_states: BTreeMap::new(),
            module_visual_entities: base.state.module_visual_entities.clone(),
            agent_updates: BTreeMap::new(),
            resource_updates: BTreeMap::new(),
            pending_effects: base.pending_effects.clone(),
            pending_effects_evicted: 0,
            next_event_id: base.next_event_id,
            next_event_id_era: base.next_event_id_era,
            next_intent_id: base.next_intent_id,
            next_intent_id_era: base.next_intent_id_era,
            events: Vec::new(),
            module_tick_schedule: base.module_tick_schedule.clone(),
            module_tick_routing_sample: None,
        })
    }

    pub(super) fn next_event_id(&self) -> u64 {
        self.next_event_id.max(1)
    }

    /// Finish a staged route without borrowing the canonical world.  The
    /// caller may then apply the common failpoint/audit publication policy.
    pub(super) fn prepare_route(
        self,
        routed: Result<usize, WorldError>,
    ) -> Result<Option<(usize, PreparedTrustedCommand)>, WorldError> {
        match routed {
            Ok(0) => Ok(None),
            Ok(invoked) => {
                let state_root = self.consensus_state_root_hash()?;
                Ok(Some((invoked, self.prepare(state_root)?)))
            }
            Err(error) => Err(error),
        }
    }

    pub(super) fn current_head(&self) -> u64 {
        self.events
            .last()
            .map(|event| event.id)
            .or_else(|| self.base.journal.events.last().map(|event| event.id))
            .unwrap_or(0)
    }

    /// Read the birth era before allocation can roll the stage into a new era.
    pub(super) fn next_effect_birth_era(&self) -> Option<u64> {
        self.base.effect_birth_era_for(self.next_intent_id_era)
    }

    pub(super) fn allocate_next_intent_seq(&mut self) -> u64 {
        let (allocated, next_id, next_era) =
            World::preview_next_intent_seq(self.next_intent_id, self.next_intent_id_era);
        self.next_intent_id = next_id;
        self.next_intent_id_era = next_era;
        allocated
    }

    pub(super) fn effect_intent_ids(&self, world_head_before: u64) -> Vec<String> {
        self.events
            .iter()
            .filter(|event| event.id > world_head_before)
            .filter_map(|event| match &event.body {
                WorldEventBody::EffectQueued(intent) => Some(intent.identity_key()),
                _ => None,
            })
            .collect()
    }

    pub(super) fn pending_effects(&self) -> &VecDeque<EffectIntent> {
        &self.pending_effects
    }

    pub(super) fn reserve_capability_budget(
        &self,
        account: &mut super::super::CapabilityBudgetAccount,
        reservation_units: i64,
    ) -> Result<(), WorldError> {
        if reservation_units < 0 || account.remaining_units < reservation_units {
            return Err(super::capability_authorization::deny(
                "capability budget is insufficient before sandbox",
            ));
        }
        account.remaining_units -= reservation_units;
        account.reserved_units = account
            .reserved_units
            .checked_add(reservation_units)
            .ok_or_else(|| {
                super::capability_authorization::deny("capability budget reservation overflow")
            })?;
        Ok(())
    }

    pub(super) fn settle_capability_budget(
        &self,
        account: &mut super::super::CapabilityBudgetAccount,
        reservation_units: i64,
        actual_units: i64,
    ) -> Result<i64, WorldError> {
        if actual_units < 0 || actual_units > reservation_units {
            return Err(super::capability_authorization::deny(
                "capability budget actual cost exceeds reservation",
            ));
        }
        account.reserved_units = account
            .reserved_units
            .checked_sub(reservation_units)
            .ok_or_else(|| {
                super::capability_authorization::deny("capability budget reservation underflow")
            })?;
        account.remaining_units = account
            .remaining_units
            .checked_add(reservation_units - actual_units)
            .ok_or_else(|| {
                super::capability_authorization::deny("capability budget release overflow")
            })?;
        account.spent_units = account
            .spent_units
            .checked_add(actual_units)
            .ok_or_else(|| {
                super::capability_authorization::deny("capability budget spend overflow")
            })?;
        Ok(account.remaining_units)
    }

    pub(super) fn module_state(&self, module_id: &str) -> Vec<u8> {
        self.module_states
            .get(module_id)
            .cloned()
            .or_else(|| self.base.state.module_states.get(module_id).cloned())
            .unwrap_or_default()
    }

    pub(super) fn state_time_for_route(&self) -> WorldTime {
        self.base.state.time
    }

    pub(super) fn journal_height_for_route(&self) -> u64 {
        self.journal_height_for_call()
    }

    pub(super) fn remove_tick_schedule(&mut self, instance_id: &str) {
        self.module_tick_schedule.remove(instance_id);
    }

    pub(super) fn schedule_tick(&mut self, instance_id: String, wake_at: WorldTime) {
        self.module_tick_schedule.insert(instance_id, wake_at);
    }

    pub(super) fn record_tick_routing_metrics(
        &mut self,
        schedule_len: usize,
        due_count: usize,
        invoked_count: usize,
        missing_invocation_count: usize,
        oldest_overdue_ticks: Option<u64>,
        duration: std::time::Duration,
    ) {
        self.module_tick_routing_sample = Some(PreparedModuleTickRoutingSample {
            schedule_len,
            due_count,
            invoked_count,
            missing_invocation_count,
            oldest_overdue_ticks,
            duration,
        });
    }

    pub(super) fn execute_module_call_with_manifest_and_state_key(
        &mut self,
        module_id: &str,
        state_key: &str,
        manifest: &ModuleManifest,
        trace_id: String,
        input: Vec<u8>,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<ModuleOutput, WorldError> {
        execute_module_call_for_target(
            self, module_id, state_key, manifest, trace_id, input, sandbox,
        )
    }

    fn build_effect_intent(
        &mut self,
        kind: String,
        params: serde_json::Value,
        cap_ref: String,
        origin: EffectOrigin,
    ) -> Result<EffectIntent, WorldError> {
        let birth_era = self.base.effect_birth_era_for(self.next_intent_id_era);
        let intent_id = format!("intent-{}", self.allocate_next_intent_seq());
        let intent = EffectIntent {
            birth_era,
            intent_id: intent_id.clone(),
            kind: kind.clone(),
            params,
            cap_ref: cap_ref.clone(),
            origin,
        };
        let grant =
            self.base
                .capabilities
                .get(&cap_ref)
                .ok_or_else(|| WorldError::CapabilityMissing {
                    cap_ref: cap_ref.clone(),
                })?;
        if grant.is_expired(self.base.state.time) {
            return Err(WorldError::CapabilityExpired { cap_ref });
        }
        if !grant.allows(&kind) {
            return Err(WorldError::CapabilityNotAllowed { cap_ref, kind });
        }

        let decision = self.base.policies.decide(&intent);
        self.append_event(WorldEventBody::PolicyDecisionRecorded(
            PolicyDecisionRecord::from_intent(&intent, decision.clone()),
        ))?;
        if !decision.is_allowed() {
            return Err(WorldError::PolicyDenied {
                intent_id,
                reason: decision
                    .reason()
                    .unwrap_or_else(|| "policy_deny".to_string()),
            });
        }
        Ok(intent)
    }

    pub(super) fn state_hash(&self) -> Result<String, WorldError> {
        let overlay = CommandStateOverlay {
            module_states: &self.module_states,
            resources: &self.resource_updates,
            agents: &self.agent_updates,
        };
        canonical_hash(
            &WorldStateProjection::borrowed(&self.base.state)
                .with_command_overlay(overlay)
                .with_module_visual_entities_overlay(&self.module_visual_entities),
        )
        .map_err(|error| super::capability_authorization::deny(format!("state hash: {error}")))
    }

    pub(super) fn consensus_state_root_hash(&self) -> Result<String, WorldError> {
        self.base
            .state_root_hash_with_command_and_module_visual_overlay(
                CommandStateOverlay {
                    module_states: &self.module_states,
                    resources: &self.resource_updates,
                    agents: &self.agent_updates,
                },
                &self.module_visual_entities,
            )
    }

    pub(super) fn call_module_raw(
        &mut self,
        module_id: &str,
        trace_id: &str,
        input: Vec<u8>,
        manifest: &ModuleManifest,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<ModuleOutput, ModuleCallFailure> {
        let wasm_hash = manifest.wasm_hash.clone();
        // Keep the process-local cache in the stage as well.  A failed command
        // discards its cache insert, while a successful command installs the
        // same cache projection the legacy clone path would have published.
        let bytes: Arc<[u8]> = if let Some(artifact) = self.module_cache.get(&wasm_hash) {
            artifact.bytes
        } else {
            let bytes = self
                .base
                .module_artifact_bytes
                .get(&wasm_hash)
                .cloned()
                .ok_or_else(|| ModuleCallFailure {
                    module_id: module_id.to_string(),
                    trace_id: trace_id.to_string(),
                    code: ModuleCallErrorCode::SandboxUnavailable,
                    detail: format!(
                        "load module failed: {:?}",
                        WorldError::ModuleChangeInvalid {
                            reason: format!("module artifact bytes missing {wasm_hash}"),
                        }
                    ),
                })?;
            self.module_cache.insert(oasis7_wasm_abi::ModuleArtifact {
                wasm_hash: wasm_hash.clone(),
                bytes: bytes.clone(),
            });
            self.module_cache_additions.push(ModuleArtifact {
                wasm_hash: wasm_hash.clone(),
                bytes: bytes.clone(),
            });
            bytes
        };
        let request = ModuleCallRequest {
            module_id: module_id.to_string(),
            wasm_hash,
            trace_id: trace_id.to_string(),
            entrypoint: manifest.kind.entrypoint().to_string(),
            input,
            limits: manifest.limits.clone(),
            wasm_bytes: bytes,
        };
        sandbox.call(&request)
    }

    pub(super) fn append_event(&mut self, body: WorldEventBody) -> Result<u64, WorldError> {
        match &body {
            WorldEventBody::CapabilityAuthorization(
                CapabilityAuthorizationEvent::CommandCommitted { .. },
            ) => {}
            WorldEventBody::PolicyDecisionRecorded(_) => {}
            WorldEventBody::ModuleEmitted(event) => {
                if let Some(next) = self
                    .base
                    .state
                    .prepare_module_visual_event_at_with_entities(
                        event,
                        self.base.state.time,
                        &self.module_visual_entities,
                    )?
                {
                    self.module_visual_entities = next;
                }
            }
            WorldEventBody::ModuleStateUpdated(update) => {
                self.module_states
                    .insert(update.module_id.clone(), update.state.clone());
            }
            WorldEventBody::ModuleRuntimeCharged(charge) => {
                self.apply_module_runtime_charge(charge)?;
            }
            WorldEventBody::EffectQueued(intent) => {
                self.push_pending_effect(intent.clone())?;
            }
            _ => {
                return Err(super::capability_authorization::deny(
                    "trusted command stage received unsupported event body",
                ));
            }
        }
        let (event_id, next_event_id, next_event_id_era) =
            World::preview_next_event_id(self.next_event_id, self.next_event_id_era);
        self.events.push(WorldEvent {
            id: event_id,
            time: self.base.state.time,
            caused_by: None,
            body,
        });
        self.next_event_id = next_event_id;
        self.next_event_id_era = next_event_id_era;
        Ok(event_id)
    }

    pub(super) fn try_charge_module_runtime(
        &mut self,
        module_id: &str,
        trace_id: &str,
        manifest: &ModuleManifest,
        input_bytes: u64,
        output: &ModuleOutput,
    ) -> Result<(), ModuleCallFailure> {
        let Some(payer_agent_id) = self
            .base
            .state
            .module_artifact_owners
            .get(manifest.wasm_hash.as_str())
            .filter(|owner| self.base.state.agents.contains_key(owner.as_str()))
            .cloned()
        else {
            return Ok(());
        };

        let effect_count = i64::try_from(output.effects.len()).unwrap_or(i64::MAX);
        let emit_count = i64::try_from(output.emits.len()).unwrap_or(i64::MAX);
        let has_new_state = i64::from(output.new_state.is_some());
        let compute_fee_amount = metering_units(input_bytes)
            .saturating_add(metering_units(output.output_bytes))
            .saturating_add(effect_count.saturating_mul(2))
            .saturating_add(emit_count);
        let electricity_fee_amount = 1_i64
            .saturating_add(effect_count)
            .saturating_add(emit_count)
            .saturating_add(has_new_state);
        let (available_compute, available_electricity) = self
            .agent_updates
            .get(&payer_agent_id)
            .or_else(|| self.base.state.agents.get(&payer_agent_id))
            .map(|cell| {
                (
                    cell.state.resources.get(ResourceKind::Data),
                    cell.state.resources.get(ResourceKind::Electricity),
                )
            })
            .unwrap_or((0, 0));
        if available_compute < compute_fee_amount || available_electricity < electricity_fee_amount
        {
            return Err(ModuleCallFailure {
                module_id: module_id.to_string(),
                trace_id: trace_id.to_string(),
                code: ModuleCallErrorCode::PolicyDenied,
                detail: format!(
                    "module runtime fee insufficient payer={} data {}/{} electricity {}/{}",
                    payer_agent_id,
                    available_compute,
                    compute_fee_amount,
                    available_electricity,
                    electricity_fee_amount
                ),
            });
        }

        self.append_event(WorldEventBody::ModuleRuntimeCharged(
            ModuleRuntimeChargeEvent {
                module_id: module_id.to_string(),
                trace_id: trace_id.to_string(),
                payer_agent_id,
                compute_fee_kind: ResourceKind::Data,
                compute_fee_amount,
                electricity_fee_kind: ResourceKind::Electricity,
                electricity_fee_amount,
                input_bytes,
                output_bytes: output.output_bytes,
                effect_count: u32::try_from(output.effects.len()).unwrap_or(u32::MAX),
                emit_count: u32::try_from(output.emits.len()).unwrap_or(u32::MAX),
            },
        ))
        .map_err(|err| ModuleCallFailure {
            module_id: module_id.to_string(),
            trace_id: trace_id.to_string(),
            code: ModuleCallErrorCode::PolicyDenied,
            detail: format!("module runtime fee apply failed: {err:?}"),
        })?;
        Ok(())
    }

    fn push_pending_effect(&mut self, intent: EffectIntent) -> Result<(), WorldError> {
        let max_len = self.base.runtime_memory_limits.max_pending_effects.max(1);
        if self.pending_effects.len() >= max_len
            && !self.pending_effects.iter().any(|pending| {
                !self
                    .base
                    .capability_effect_receipt_links
                    .contains_key(&pending.identity_key())
            })
        {
            return Err(WorldError::CapabilityAuthorizationDenied {
                reason: "effect queue is full of authorization-linked intents".to_string(),
            });
        }
        self.pending_effects.push_back(intent);
        while self.pending_effects.len() > max_len {
            let Some(eviction_index) = self.pending_effects.iter().position(|pending| {
                !self
                    .base
                    .capability_effect_receipt_links
                    .contains_key(&pending.identity_key())
            }) else {
                break;
            };
            let _ = self.pending_effects.remove(eviction_index);
            self.pending_effects_evicted = self.pending_effects_evicted.saturating_add(1);
        }
        Ok(())
    }

    fn apply_module_runtime_charge(
        &mut self,
        charge: &ModuleRuntimeChargeEvent,
    ) -> Result<(), WorldError> {
        if charge.compute_fee_amount < 0 || charge.electricity_fee_amount < 0 {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!(
                    "module runtime fee must be >= 0: compute={} electricity={}",
                    charge.compute_fee_amount, charge.electricity_fee_amount
                ),
            });
        }
        let mut cell = self
            .agent_updates
            .get(&charge.payer_agent_id)
            .cloned()
            .or_else(|| self.base.state.agents.get(&charge.payer_agent_id).cloned())
            .ok_or_else(|| WorldError::AgentNotFound {
                agent_id: charge.payer_agent_id.clone(),
            })?;
        let available_compute = cell.state.resources.get(charge.compute_fee_kind);
        let available_electricity = cell.state.resources.get(charge.electricity_fee_kind);
        if available_compute < charge.compute_fee_amount {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!(
                    "module runtime compute fee debit failed: payer={} kind={:?} amount={} err=Insufficient",
                    charge.payer_agent_id, charge.compute_fee_kind, charge.compute_fee_amount
                ),
            });
        }
        if available_electricity < charge.electricity_fee_amount {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!(
                    "module runtime electricity fee debit failed: payer={} kind={:?} amount={} err=Insufficient",
                    charge.payer_agent_id,
                    charge.electricity_fee_kind,
                    charge.electricity_fee_amount
                ),
            });
        }
        if charge.compute_fee_amount > 0 {
            cell.state
                .resources
                .remove(charge.compute_fee_kind, charge.compute_fee_amount)
                .map_err(|err| WorldError::ResourceBalanceInvalid {
                    reason: format!(
                        "module runtime compute fee debit failed: payer={} kind={:?} amount={} err={err:?}",
                        charge.payer_agent_id,
                        charge.compute_fee_kind,
                        charge.compute_fee_amount
                    ),
                })?;
        }
        if charge.electricity_fee_amount > 0 {
            cell.state
                .resources
                .remove(charge.electricity_fee_kind, charge.electricity_fee_amount)
                .map_err(|err| WorldError::ResourceBalanceInvalid {
                    reason: format!(
                        "module runtime electricity fee debit failed: payer={} kind={:?} amount={} err={err:?}",
                        charge.payer_agent_id,
                        charge.electricity_fee_kind,
                        charge.electricity_fee_amount
                    ),
                })?;
        }
        cell.last_active = self.base.state.time;
        self.agent_updates
            .insert(charge.payer_agent_id.clone(), cell);
        self.add_resource_update(charge.compute_fee_kind, charge.compute_fee_amount);
        self.add_resource_update(charge.electricity_fee_kind, charge.electricity_fee_amount);
        Ok(())
    }

    fn add_resource_update(&mut self, kind: ResourceKind, amount: i64) {
        if amount <= 0 {
            return;
        }
        let current = self
            .resource_updates
            .get(&kind)
            .copied()
            .unwrap_or_else(|| self.base.state.resources.get(&kind).copied().unwrap_or(0));
        self.resource_updates
            .insert(kind, current.saturating_add(amount));
    }

    pub(super) fn prepare(self, state_root: String) -> Result<PreparedTrustedCommand, WorldError> {
        let (journal_events, overflow) = self.journal_projection();
        let tick_events: Vec<WorldEvent> = journal_events
            .iter()
            .filter(|event| event.time == self.base.state.time)
            .cloned()
            .collect();
        let consensus_record = self.base.build_tick_consensus_record_for_prepared_events(
            self.base.state.time,
            tick_events.as_slice(),
            state_root.as_str().to_string(),
        )?;
        self.base
            .validate_tick_consensus_candidate_for_prepared_publication(
                &consensus_record,
                tick_events.as_slice(),
                state_root.as_str(),
            )?;
        Ok(PreparedTrustedCommand {
            base_head: self.base_head,
            module_cache_additions: self.module_cache_additions,
            module_states: self.module_states,
            module_visual_entities: self.module_visual_entities,
            agent_updates: self.agent_updates,
            resource_updates: self.resource_updates,
            pending_effects: self.pending_effects,
            pending_effects_evicted: self.pending_effects_evicted,
            next_event_id: self.next_event_id,
            next_event_id_era: self.next_event_id_era,
            next_intent_id: self.next_intent_id,
            next_intent_id_era: self.next_intent_id_era,
            journal_events,
            journal_events_evicted: overflow as u64,
            module_tick_schedule: self.module_tick_schedule,
            module_tick_routing_sample: self.module_tick_routing_sample,
            consensus_record: Some(consensus_record),
            capability_authorization: None,
        })
    }

    pub(super) fn prepare_tick_route(
        self,
        routed: Result<usize, WorldError>,
    ) -> Result<Option<(usize, PreparedTrustedCommand)>, WorldError> {
        match routed {
            Ok(invoked) => {
                let (journal_events, overflow) = self.journal_projection();
                Ok(Some((
                    invoked,
                    PreparedTrustedCommand {
                        base_head: self.base_head,
                        module_cache_additions: self.module_cache_additions,
                        module_states: self.module_states,
                        module_visual_entities: self.module_visual_entities,
                        agent_updates: self.agent_updates,
                        resource_updates: self.resource_updates,
                        pending_effects: self.pending_effects,
                        pending_effects_evicted: self.pending_effects_evicted,
                        next_event_id: self.next_event_id,
                        next_event_id_era: self.next_event_id_era,
                        next_intent_id: self.next_intent_id,
                        next_intent_id_era: self.next_intent_id_era,
                        journal_events,
                        journal_events_evicted: overflow as u64,
                        module_tick_schedule: self.module_tick_schedule,
                        module_tick_routing_sample: self.module_tick_routing_sample,
                        consensus_record: None,
                        capability_authorization: None,
                    },
                )))
            }
            Err(error) => Err(error),
        }
    }

    fn journal_projection(&self) -> (Vec<WorldEvent>, usize) {
        let mut journal_events = self.base.journal.events.clone();
        journal_events.extend(self.events.iter().cloned());
        let max_len = self.base.runtime_memory_limits.max_journal_events.max(1);
        let overflow = journal_events.len().saturating_sub(max_len);
        if overflow > 0 {
            journal_events.drain(0..overflow);
        }
        (journal_events, overflow)
    }
}

impl ModuleCallTarget for TrustedCommandStage<'_> {
    fn active_manifest_for_call(&self, module_id: &str) -> Result<ModuleManifest, WorldError> {
        self.base.active_module_manifest(module_id).cloned()
    }

    fn current_manifest_hash_for_call(&self) -> Result<String, WorldError> {
        self.base.current_manifest_hash()
    }

    fn state_time_for_call(&self) -> WorldTime {
        self.base.state.time
    }

    fn journal_height_for_call(&self) -> u64 {
        self.base.journal.events.len() as u64 + self.events.len() as u64
    }

    fn call_module_raw_for_call(
        &mut self,
        module_id: &str,
        trace_id: &str,
        input: Vec<u8>,
        manifest: &ModuleManifest,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<ModuleOutput, ModuleCallFailure> {
        self.call_module_raw(module_id, trace_id, input, manifest, sandbox)
    }

    fn module_call_failed_for_call(
        &mut self,
        failure: ModuleCallFailure,
    ) -> Result<(), WorldError> {
        Err(WorldError::ModuleCallFailed {
            module_id: failure.module_id,
            trace_id: failure.trace_id,
            code: failure.code,
            detail: failure.detail,
        })
    }

    fn append_module_event_for_call(&mut self, body: WorldEventBody) -> Result<u64, WorldError> {
        self.append_event(body)
    }

    fn try_charge_module_runtime_for_call(
        &mut self,
        module_id: &str,
        trace_id: &str,
        manifest: &ModuleManifest,
        input_bytes: u64,
        output: &ModuleOutput,
    ) -> Result<(), ModuleCallFailure> {
        self.try_charge_module_runtime(module_id, trace_id, manifest, input_bytes, output)
    }

    fn build_effect_intent_for_call(
        &mut self,
        kind: String,
        params: serde_json::Value,
        cap_ref: String,
        origin: EffectOrigin,
    ) -> Result<EffectIntent, WorldError> {
        self.build_effect_intent(kind, params, cap_ref, origin)
    }
}

impl PreparedTrustedCommand {
    pub(super) fn with_capability_authorization_projection(
        mut self,
        projection: PreparedCapabilityAuthorizationProjection,
    ) -> Self {
        self.capability_authorization = Some(projection);
        self
    }

    pub(super) fn install(self, world: &mut World) -> Result<(), WorldError> {
        if WorldPreparedBaseHead::capture(world)? != self.base_head {
            return Err(WorldPreparedBaseHead::stale_error());
        }
        if let Some(consensus_record) = self.consensus_record {
            world.install_prepared_tick_consensus_record(consensus_record)?;
        }
        for artifact in self.module_cache_additions {
            world.module_cache.insert(artifact);
        }
        for (module_id, state) in self.module_states {
            world.state.module_states.insert(module_id, state);
        }
        world.state.module_visual_entities = self.module_visual_entities;
        for (agent_id, cell) in self.agent_updates {
            world.state.agents.insert(agent_id, cell);
        }
        for (kind, amount) in self.resource_updates {
            world.state.resources.insert(kind, amount);
        }
        world.pending_effects = self.pending_effects;
        world.next_event_id = self.next_event_id;
        world.next_event_id_era = self.next_event_id_era;
        world.next_intent_id = self.next_intent_id;
        world.next_intent_id_era = self.next_intent_id_era;
        world.journal.events = self.journal_events;
        world.module_tick_schedule = self.module_tick_schedule;
        if let Some(sample) = self.module_tick_routing_sample {
            world.record_module_tick_routing_metrics(
                sample.schedule_len,
                sample.due_count,
                sample.invoked_count,
                sample.missing_invocation_count,
                sample.oldest_overdue_ticks,
                sample.duration,
            );
        }
        world.runtime_backpressure_stats.pending_effects_evicted = world
            .runtime_backpressure_stats
            .pending_effects_evicted
            .saturating_add(self.pending_effects_evicted);
        world.runtime_backpressure_stats.journal_events_evicted = world
            .runtime_backpressure_stats
            .journal_events_evicted
            .saturating_add(self.journal_events_evicted);
        if let Some(projection) = self.capability_authorization {
            world.capability_grants_v2 = projection.capability_grants_v2;
            world.capability_nonce_records = projection.capability_nonce_records;
            world.capability_authorization_receipts = projection.capability_authorization_receipts;
            world.capability_budget_accounts = projection.capability_budget_accounts;
            world.capability_effect_receipt_links = projection.capability_effect_receipt_links;
            world.capability_authorization_root = projection.capability_authorization_root;
        }
        Ok(())
    }
}

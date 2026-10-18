//! WorldKernel: time, events, actions, and observation.

mod actions;
mod effect_recording;
mod fragment_refill_quote;
mod fragment_replenish;
mod micro_depot;
mod micro_depot_commissioning;
mod micro_depot_install_quote;
mod micro_depot_quote;
mod micro_depot_snapshot;
mod micro_depot_v1;
mod micro_depot_validation;
mod module_lifecycle;
mod module_market_lifecycle;
mod observation;
mod persistence;
mod power;
mod replay;
mod replay_module_lifecycle;
mod replay_power_market;
mod social;
mod state;
mod step;
pub use state::KernelState;
mod types;

use oasis7_wasm_abi::{
    ModuleCallCaller, ModuleCallInput, ModuleCallOrigin, ModuleCallRequest, ModuleContext,
    ModuleLimits, ModuleOutput, ModuleSandbox,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use super::memory_entry::LongTermMemoryEntry;
use super::types::{
    Action, ActionEnvelope, ActionId, AgentId, FragmentElementKind, WorldEventId, WorldTime,
};
use super::world_model::{AgentPromptProfile, FragmentResourceError, WorldConfig, WorldModel};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChunkRuntimeConfig {
    pub world_seed: u64,
    pub asteroid_fragment_enabled: bool,
    pub asteroid_fragment_seed_offset: u64,
    pub min_fragment_spacing_cm: Option<i64>,
}

impl Default for ChunkRuntimeConfig {
    fn default() -> Self {
        Self {
            world_seed: 0,
            asteroid_fragment_enabled: false,
            asteroid_fragment_seed_offset: 1,
            min_fragment_spacing_cm: None,
        }
    }
}

impl ChunkRuntimeConfig {
    pub fn asteroid_fragment_seed(&self) -> u64 {
        self.world_seed
            .wrapping_add(self.asteroid_fragment_seed_offset)
    }
}

pub use actions::{RecipeResourceCost, ScheduleResourceScope};
pub use micro_depot::{
    MICRO_DEPOT_DEBIT_AMOUNT_CRITICAL, MICRO_DEPOT_DEBIT_AMOUNT_HIGH, MICRO_DEPOT_DEBIT_AMOUNT_LOW,
    MICRO_DEPOT_DEBIT_AMOUNT_MEDIUM, MICRO_DEPOT_DEBIT_AMOUNT_NONE, MICRO_DEPOT_INSTALL_DATA_COST,
    MICRO_DEPOT_PROPOSAL_EMIT_KIND, MICRO_DEPOT_UPKEEP_DATA_COST, MicroDepotActionContext,
    MicroDepotActionKind, MicroDepotConsumedResourceClass, MicroDepotDecision,
    MicroDepotDeltaClass, MicroDepotEffectPreview, MicroDepotEvalInput, MicroDepotFacilityContext,
    MicroDepotPlayerContext, MicroDepotPlayerFacilitySnapshot, MicroDepotPressureClass,
    MicroDepotProposal, MicroDepotQuotePreview, MicroDepotRegionContext, MicroDepotStatus,
    compute_micro_depot_proposal_hash, evaluate_micro_depot_quote_with_module,
};
pub use micro_depot_install_quote::MicroDepotInstallQuote;
#[allow(unused_imports)]
pub use step::{IntentBatchReport, IntentConflictResolution};
#[allow(unused_imports)]
pub use types::{
    ChunkGenerationCause, FragmentReplenishedEntry, KernelRuleCost, KernelRuleDecision,
    KernelRuleDecisionMergeError, KernelRuleModuleContext, KernelRuleModuleInput,
    KernelRuleModuleOutput, KernelRuleVerdict, MicroDepotProposalResourceDebit,
    MicroDepotResourceDebit, Observation, ObservedAgent, ObservedLocation,
    ObservedModuleArtifactRecord, ObservedModuleLifecycleState, ObservedModuleMarketState,
    ObservedPowerMarketState, ObservedSocialState, PowerOrderFill, PromptUpdateOperation,
    RejectReason, WorldEvent, WorldEventKind, merge_kernel_rule_decisions,
};

type PreActionRuleHook =
    Arc<dyn Fn(ActionId, &Action, &WorldKernel) -> KernelRuleDecision + Send + Sync>;
type PostActionRuleHook = Arc<dyn Fn(ActionId, &Action, &WorldEvent) + Send + Sync>;
type PreActionWasmRuleEvaluator =
    Arc<dyn Fn(&KernelRuleModuleInput) -> Result<KernelRuleModuleOutput, String> + Send + Sync>;
type MicroDepotWasmQuoteEvaluator =
    Arc<dyn Fn(&MicroDepotEvalInput) -> Result<MicroDepotQuotePreview, String> + Send + Sync>;
const RULE_DECISION_EMIT_KIND: &str = "rule.decision";

#[derive(Default, Clone)]
struct RuleHookRegistry {
    pre_action: Vec<PreActionRuleHook>,
    post_action: Vec<PostActionRuleHook>,
    pre_action_wasm: Option<PreActionWasmRuleEvaluator>,
    pre_action_wasm_artifacts: BTreeMap<String, Vec<u8>>,
    micro_depot_wasm: Option<MicroDepotWasmQuoteEvaluator>,
    micro_depot_wasm_artifacts: BTreeMap<String, Vec<u8>>,
}

impl std::fmt::Debug for RuleHookRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuleHookRegistry")
            .field("pre_action_len", &self.pre_action.len())
            .field("post_action_len", &self.post_action.len())
            .field("pre_action_wasm_enabled", &self.pre_action_wasm.is_some())
            .field("micro_depot_wasm_enabled", &self.micro_depot_wasm.is_some())
            .field(
                "pre_action_wasm_artifact_count",
                &self.pre_action_wasm_artifacts.len(),
            )
            .field(
                "micro_depot_wasm_artifact_count",
                &self.micro_depot_wasm_artifacts.len(),
            )
            .finish()
    }
}

impl PartialEq for RuleHookRegistry {
    fn eq(&self, _other: &Self) -> bool {
        // Runtime hooks are process-local closures and intentionally excluded from state equality.
        true
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WorldKernel {
    time: WorldTime,
    config: WorldConfig,
    next_event_id: WorldEventId,
    next_action_id: ActionId,
    pending_actions: VecDeque<ActionEnvelope>,
    journal: Vec<WorldEvent>,
    model: WorldModel,
    #[serde(default)]
    chunk_runtime: ChunkRuntimeConfig,
    #[serde(default)]
    intel_ttl_ticks: WorldTime,
    #[serde(skip, default)]
    intel_cache: BTreeMap<AgentId, IntelCacheEntry>,
    #[serde(skip, default)]
    last_intent_batch_report: Option<IntentBatchReport>,
    #[serde(skip, default)]
    rule_hooks: RuleHookRegistry,
    #[serde(skip, default)]
    source_compiler: super::source_compiler::KernelSourceCompiler,
}

#[derive(Debug, Clone, PartialEq)]
struct IntelCacheEntry {
    observation: Observation,
    expires_at_tick: WorldTime,
}

impl WorldKernel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config(config: WorldConfig) -> Self {
        Self {
            config: config.sanitized(),
            ..Self::default()
        }
    }

    pub fn with_model(config: WorldConfig, model: WorldModel) -> Self {
        Self {
            time: 0,
            config: config.sanitized(),
            next_event_id: 0,
            next_action_id: 0,
            pending_actions: VecDeque::new(),
            journal: Vec::new(),
            model,
            chunk_runtime: ChunkRuntimeConfig::default(),
            intel_ttl_ticks: 0,
            intel_cache: BTreeMap::new(),
            last_intent_batch_report: None,
            rule_hooks: RuleHookRegistry::default(),
            source_compiler: Default::default(),
        }
    }

    pub fn with_model_and_chunk_runtime(
        config: WorldConfig,
        model: WorldModel,
        chunk_runtime: ChunkRuntimeConfig,
    ) -> Self {
        Self {
            time: 0,
            config: config.sanitized(),
            next_event_id: 0,
            next_action_id: 0,
            pending_actions: VecDeque::new(),
            journal: Vec::new(),
            model,
            chunk_runtime,
            intel_ttl_ticks: 0,
            intel_cache: BTreeMap::new(),
            last_intent_batch_report: None,
            rule_hooks: RuleHookRegistry::default(),
            source_compiler: Default::default(),
        }
    }

    /// Install a host capability. Source compilation still occurs at action execution,
    /// and the resulting deployed bytes are journaled for compiler-free replay.
    pub fn set_module_source_compiler(
        &mut self,
        compiler: Arc<dyn oasis7_agent_api::module_source::ModuleSourceCompiler>,
    ) {
        self.source_compiler = super::source_compiler::KernelSourceCompiler::new(compiler);
    }

    pub fn set_intel_ttl_ticks(&mut self, ttl_ticks: WorldTime) {
        self.intel_ttl_ticks = ttl_ticks;
        if ttl_ticks == 0 {
            self.intel_cache.clear();
        }
    }

    pub fn intel_ttl_ticks(&self) -> WorldTime {
        self.intel_ttl_ticks
    }

    pub fn add_pre_action_rule_hook<F>(&mut self, hook: F)
    where
        F: Fn(ActionId, &Action, &WorldKernel) -> KernelRuleDecision + Send + Sync + 'static,
    {
        self.rule_hooks.pre_action.push(Arc::new(hook));
    }

    pub fn add_post_action_rule_hook<F>(&mut self, hook: F)
    where
        F: Fn(ActionId, &Action, &WorldEvent) + Send + Sync + 'static,
    {
        self.rule_hooks.post_action.push(Arc::new(hook));
    }

    pub fn set_pre_action_wasm_rule_evaluator<F>(&mut self, evaluator: F)
    where
        F: Fn(&KernelRuleModuleInput) -> Result<KernelRuleModuleOutput, String>
            + Send
            + Sync
            + 'static,
    {
        self.rule_hooks.pre_action_wasm = Some(Arc::new(evaluator));
    }

    pub fn clear_pre_action_wasm_rule_evaluator(&mut self) {
        self.rule_hooks.pre_action_wasm = None;
    }

    pub fn set_pre_action_wasm_rule_module_evaluator<S>(
        &mut self,
        module_id: impl Into<String>,
        wasm_hash: impl Into<String>,
        entrypoint: impl Into<String>,
        wasm_bytes: Vec<u8>,
        limits: ModuleLimits,
        sandbox: Arc<Mutex<S>>,
    ) where
        S: ModuleSandbox + Send + 'static,
    {
        let module_id = module_id.into();
        let wasm_hash = wasm_hash.into();
        let entrypoint = entrypoint.into();
        self.set_pre_action_wasm_rule_evaluator(move |input| {
            let request = build_pre_action_wasm_call_request(
                input,
                &module_id,
                &wasm_hash,
                &entrypoint,
                &wasm_bytes,
                &limits,
            )?;
            let output = {
                let mut locked = sandbox
                    .lock()
                    .map_err(|_| "wasm sandbox mutex poisoned".to_string())?;
                locked.call(&request).map_err(|failure| {
                    format!("module call failed {:?}: {}", failure.code, failure.detail)
                })?
            };
            let decision = parse_pre_action_wasm_rule_decision(input.action_id, &output)?;
            Ok(KernelRuleModuleOutput::from_decision(decision))
        });
    }

    pub fn register_pre_action_wasm_rule_artifact(
        &mut self,
        wasm_hash: impl Into<String>,
        wasm_bytes: Vec<u8>,
    ) -> Result<(), String> {
        let wasm_hash = wasm_hash.into();
        if wasm_hash.trim().is_empty() {
            return Err("wasm hash is empty".to_string());
        }
        if wasm_bytes.is_empty() {
            return Err(format!("wasm bytes are empty for hash {wasm_hash}"));
        }
        if let Some(existing) = self.rule_hooks.pre_action_wasm_artifacts.get(&wasm_hash) {
            if existing != &wasm_bytes {
                return Err(format!(
                    "artifact hash {wasm_hash} already registered with different bytes"
                ));
            }
            return Ok(());
        }

        self.rule_hooks
            .pre_action_wasm_artifacts
            .insert(wasm_hash, wasm_bytes);
        Ok(())
    }

    pub fn remove_pre_action_wasm_rule_artifact(&mut self, wasm_hash: &str) -> bool {
        self.rule_hooks
            .pre_action_wasm_artifacts
            .remove(wasm_hash)
            .is_some()
    }

    pub fn set_pre_action_wasm_rule_module_from_registry<S>(
        &mut self,
        module_id: impl Into<String>,
        wasm_hash: impl Into<String>,
        entrypoint: impl Into<String>,
        limits: ModuleLimits,
        sandbox: Arc<Mutex<S>>,
    ) -> Result<(), String>
    where
        S: ModuleSandbox + Send + 'static,
    {
        let wasm_hash = wasm_hash.into();
        let wasm_bytes = self
            .rule_hooks
            .pre_action_wasm_artifacts
            .get(&wasm_hash)
            .cloned()
            .ok_or_else(|| format!("pre-action wasm artifact missing for hash {wasm_hash}"))?;

        self.set_pre_action_wasm_rule_module_evaluator(
            module_id, wasm_hash, entrypoint, wasm_bytes, limits, sandbox,
        );
        Ok(())
    }

    pub fn time(&self) -> WorldTime {
        self.time
    }

    pub fn config(&self) -> &WorldConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: WorldConfig) {
        self.config = config.sanitized();
    }

    pub fn model(&self) -> &WorldModel {
        &self.model
    }

    pub fn consume_fragment_resource(
        &mut self,
        location_id: &str,
        kind: FragmentElementKind,
        amount_g: i64,
    ) -> Result<i64, FragmentResourceError> {
        self.model
            .consume_fragment_resource(location_id, &self.config.space, kind, amount_g)
    }

    pub fn journal(&self) -> &[WorldEvent] {
        &self.journal
    }

    pub fn apply_agent_prompt_profile_update(
        &mut self,
        profile: AgentPromptProfile,
        operation: PromptUpdateOperation,
        applied_fields: Vec<String>,
        digest: String,
        rolled_back_to_version: Option<u64>,
    ) -> WorldEvent {
        self.model
            .agent_prompt_profiles
            .insert(profile.agent_id.clone(), profile.clone());
        self.record_event(WorldEventKind::AgentPromptUpdated {
            profile,
            operation,
            applied_fields,
            digest,
            rolled_back_to_version,
        })
    }

    pub fn player_binding_for_agent(&self, agent_id: &str) -> Option<&str> {
        self.model
            .agent_player_bindings
            .get(agent_id)
            .map(String::as_str)
    }

    pub fn public_key_binding_for_agent(&self, agent_id: &str) -> Option<&str> {
        self.model
            .agent_player_public_key_bindings
            .get(agent_id)
            .map(String::as_str)
    }

    pub fn player_auth_last_nonce(&self, player_id: &str) -> Option<u64> {
        let player_id = player_id.trim();
        if player_id.is_empty() {
            return None;
        }
        self.model.player_auth_last_nonce.get(player_id).copied()
    }

    pub fn consume_player_auth_nonce(&mut self, player_id: &str, nonce: u64) -> Result<(), String> {
        let player_id = player_id.trim();
        if player_id.is_empty() {
            return Err("player_id cannot be empty".to_string());
        }
        if nonce == 0 {
            return Err("auth nonce must be greater than zero".to_string());
        }

        if let Some(last_nonce) = self.model.player_auth_last_nonce.get(player_id)
            && nonce <= *last_nonce
        {
            return Err(format!(
                "auth nonce replay for {}: expected nonce > {}, received {}",
                player_id, last_nonce, nonce
            ));
        }

        self.model
            .player_auth_last_nonce
            .insert(player_id.to_string(), nonce);
        Ok(())
    }

    pub fn bind_agent_player(
        &mut self,
        agent_id: &str,
        player_id: &str,
        public_key: Option<&str>,
    ) -> Result<Option<WorldEvent>, String> {
        if !self.model.agents.contains_key(agent_id) {
            return Err(format!("agent not found: {agent_id}"));
        }
        let player_id = player_id.trim();
        if player_id.is_empty() {
            return Err("player_id cannot be empty".to_string());
        }
        let requested_public_key = public_key
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);

        let current_player = self
            .player_binding_for_agent(agent_id)
            .map(ToOwned::to_owned);
        let current_public_key = self
            .public_key_binding_for_agent(agent_id)
            .map(ToOwned::to_owned);

        // Keep existing key binding on no-key requests when player stays unchanged,
        // so legacy clients do not downgrade key-bound agents.
        let target_public_key = if current_player.as_deref() == Some(player_id) {
            requested_public_key
                .clone()
                .or_else(|| current_public_key.clone())
        } else {
            requested_public_key.clone()
        };

        if current_player.as_deref() == Some(player_id) && current_public_key == target_public_key {
            return Ok(None);
        }

        self.model
            .agent_player_bindings
            .insert(agent_id.to_string(), player_id.to_string());
        match target_public_key.clone() {
            Some(value) => {
                self.model
                    .agent_player_public_key_bindings
                    .insert(agent_id.to_string(), value);
            }
            None => {
                self.model.agent_player_public_key_bindings.remove(agent_id);
            }
        }
        Ok(Some(self.record_event(WorldEventKind::AgentPlayerBound {
            agent_id: agent_id.to_string(),
            player_id: player_id.to_string(),
            public_key: target_public_key,
        })))
    }

    pub fn long_term_memory_for_agent(&self, agent_id: &str) -> Option<&[LongTermMemoryEntry]> {
        self.model
            .agent_long_term_memories
            .get(agent_id)
            .map(Vec::as_slice)
    }

    pub fn set_agent_long_term_memory(
        &mut self,
        agent_id: &str,
        entries: Vec<LongTermMemoryEntry>,
    ) -> Result<(), String> {
        if !self.model.agents.contains_key(agent_id) {
            return Err(format!("agent not found: {agent_id}"));
        }
        if entries.is_empty() {
            self.model.agent_long_term_memories.remove(agent_id);
        } else {
            self.model
                .agent_long_term_memories
                .insert(agent_id.to_string(), entries);
        }
        Ok(())
    }

    pub(super) fn record_event(&mut self, kind: WorldEventKind) -> WorldEvent {
        let event = WorldEvent {
            id: self.next_event_id,
            time: self.time,
            kind,
        };
        self.next_event_id = self.next_event_id.saturating_add(1);
        self.journal.push(event.clone());
        event
    }
}

fn build_pre_action_wasm_call_request(
    input: &KernelRuleModuleInput,
    module_id: &str,
    wasm_hash: &str,
    entrypoint: &str,
    wasm_bytes: &[u8],
    limits: &ModuleLimits,
) -> Result<ModuleCallRequest, String> {
    let action_bytes = to_canonical_cbor(input)?;
    let trace_id = format!(
        "sim-kernel-action-{}-t{}",
        input.action_id, input.context.time
    );
    let call_input = ModuleCallInput {
        ctx: ModuleContext {
            v: "wasm-1".to_string(),
            module_id: module_id.to_string(),
            trace_id: trace_id.clone(),
            time: input.context.time,
            origin: ModuleCallOrigin {
                kind: "simulator_action".to_string(),
                id: input.action_id.to_string(),
            },
            caller: ModuleCallCaller::System {
                system_id: "simulator_action".to_string(),
            },
            limits: limits.clone(),
            stage: Some("simulator_pre_action".to_string()),
            world_config_hash: None,
            manifest_hash: None,
            journal_height: None,
            module_version: None,
            module_kind: None,
            module_role: None,
        },
        event: None,
        action: Some(action_bytes),
        state: None,
    };
    let input_bytes = to_canonical_cbor(&call_input)?;

    Ok(ModuleCallRequest {
        module_id: module_id.to_string(),
        wasm_hash: wasm_hash.to_string(),
        trace_id,
        entrypoint: entrypoint.to_string(),
        input: input_bytes,
        limits: limits.clone(),
        wasm_bytes: wasm_bytes.into(),
    })
}

fn parse_pre_action_wasm_rule_decision(
    action_id: ActionId,
    output: &ModuleOutput,
) -> Result<KernelRuleDecision, String> {
    let mut decision = None;
    for emit in &output.emits {
        if emit.kind != RULE_DECISION_EMIT_KIND {
            continue;
        }
        if decision.is_some() {
            return Err("multiple rule.decision emits in wasm module output".to_string());
        }
        let parsed: KernelRuleDecision = serde_json::from_value(emit.payload.clone())
            .map_err(|err| format!("failed to decode rule.decision payload: {err}"))?;
        if parsed.action_id != action_id {
            return Err(format!(
                "rule.decision action_id mismatch expected {action_id} got {}",
                parsed.action_id
            ));
        }
        decision = Some(parsed);
    }

    Ok(decision.unwrap_or_else(|| KernelRuleDecision::allow(action_id)))
}

fn to_canonical_cbor<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let mut buf = Vec::with_capacity(256);
    let canonical_value = serde_cbor::value::to_value(value)
        .map_err(|err| format!("failed to convert value to canonical cbor: {err}"))?;
    let mut serializer = serde_cbor::ser::Serializer::new(&mut buf);
    serializer
        .self_describe()
        .map_err(|err| format!("failed to write cbor self describe tag: {err}"))?;
    canonical_value
        .serialize(&mut serializer)
        .map_err(|err| format!("failed to serialize canonical cbor value: {err}"))?;
    Ok(buf)
}

use super::*;
use oasis7_wasm_abi::{ModuleLimits, ModuleSandbox};
use std::sync::Mutex;

impl WorldKernel {
    pub fn set_module_source_compiler(
        &mut self,
        compiler: Arc<dyn oasis7_agent_api::module_source::ModuleSourceCompiler>,
    ) {
        self.kernel.set_module_source_compiler(compiler);
        self.assert_journal_sidecar();
    }
    pub fn set_intel_ttl_ticks(&mut self, ttl_ticks: WorldTime) {
        self.kernel.set_intel_ttl_ticks(ttl_ticks);
        self.assert_journal_sidecar();
    }
    pub fn add_pre_action_rule_hook<F>(&mut self, hook: F)
    where
        F: Fn(ActionId, &Action, &WorldKernel) -> KernelRuleDecision + Send + Sync + 'static,
    {
        let runtime_events = self.runtime_events.clone();
        let archive_origin = self.archive_origin.clone();
        self.kernel
            .add_pre_action_rule_hook(move |id, action, kernel| {
                let view = WorldKernel {
                    kernel: kernel.clone(),
                    runtime_events: runtime_events.clone(),
                    archive_origin: archive_origin.clone(),
                };
                hook(id, action, &view)
            });
    }

    pub fn set_pre_action_wasm_rule_evaluator<F>(&mut self, evaluator: F)
    where
        F: Fn(&KernelRuleModuleInput) -> Result<KernelRuleModuleOutput, String>
            + Send
            + Sync
            + 'static,
    {
        self.kernel.set_pre_action_wasm_rule_evaluator(evaluator);
        self.assert_journal_sidecar();
    }
    pub fn clear_pre_action_wasm_rule_evaluator(&mut self) {
        self.kernel.clear_pre_action_wasm_rule_evaluator();
        self.assert_journal_sidecar();
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
        self.kernel.set_pre_action_wasm_rule_module_evaluator(
            module_id, wasm_hash, entrypoint, wasm_bytes, limits, sandbox,
        );
        self.assert_journal_sidecar();
    }
    pub fn register_pre_action_wasm_rule_artifact(
        &mut self,
        wasm_hash: impl Into<String>,
        wasm_bytes: Vec<u8>,
    ) -> Result<(), String> {
        let result = self
            .kernel
            .register_pre_action_wasm_rule_artifact(wasm_hash, wasm_bytes);
        self.assert_journal_sidecar();
        result
    }
    pub fn remove_pre_action_wasm_rule_artifact(&mut self, wasm_hash: &str) -> bool {
        let result = self.kernel.remove_pre_action_wasm_rule_artifact(wasm_hash);
        self.assert_journal_sidecar();
        result
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
        let result = self.kernel.set_pre_action_wasm_rule_module_from_registry(
            module_id, wasm_hash, entrypoint, limits, sandbox,
        );
        self.assert_journal_sidecar();
        result
    }
    pub fn set_config(&mut self, config: WorldConfig) {
        self.kernel.set_config(config);
        self.assert_journal_sidecar();
    }
    pub fn consume_fragment_resource(
        &mut self,
        location_id: &str,
        kind: FragmentElementKind,
        amount_g: i64,
    ) -> Result<i64, FragmentResourceError> {
        let result = self
            .kernel
            .consume_fragment_resource(location_id, kind, amount_g);
        self.assert_journal_sidecar();
        result
    }
    pub fn apply_agent_prompt_profile_update(
        &mut self,
        profile: AgentPromptProfile,
        operation: PromptUpdateOperation,
        applied_fields: Vec<String>,
        digest: String,
        rolled_back_to_version: Option<u64>,
    ) -> WorldEvent {
        let result = self.kernel.apply_agent_prompt_profile_update(
            profile,
            operation,
            applied_fields,
            digest,
            rolled_back_to_version,
        );
        self.assert_journal_sidecar();
        WorldEvent::from_kernel_parts(result, None)
    }
    pub fn consume_player_auth_nonce(&mut self, player_id: &str, nonce: u64) -> Result<(), String> {
        let result = self.kernel.consume_player_auth_nonce(player_id, nonce);
        self.assert_journal_sidecar();
        result
    }
    pub fn bind_agent_player(
        &mut self,
        agent_id: &str,
        player_id: &str,
        public_key: Option<&str>,
    ) -> Result<Option<WorldEvent>, String> {
        let result = self
            .kernel
            .bind_agent_player(agent_id, player_id, public_key);
        self.assert_journal_sidecar();
        result.map(|event| event.map(|event| WorldEvent::from_kernel_parts(event, None)))
    }
    pub fn set_agent_long_term_memory(
        &mut self,
        agent_id: &str,
        entries: Vec<LongTermMemoryEntry>,
    ) -> Result<(), String> {
        let result = self.kernel.set_agent_long_term_memory(agent_id, entries);
        self.assert_journal_sidecar();
        result
    }
    pub fn submit_action(&mut self, action: Action) -> ActionId {
        let result = self.kernel.submit_action(action);
        self.assert_journal_sidecar();
        result
    }
    pub fn submit_action_from_system(&mut self, action: Action) -> ActionId {
        let result = self.kernel.submit_action_from_system(action);
        self.assert_journal_sidecar();
        result
    }
    pub fn submit_action_from_agent(
        &mut self,
        agent_id: impl Into<String>,
        action: Action,
    ) -> ActionId {
        let result = self.kernel.submit_action_from_agent(agent_id, action);
        self.assert_journal_sidecar();
        result
    }
    pub fn submit_action_from_player(
        &mut self,
        player_id: impl Into<String>,
        action: Action,
    ) -> ActionId {
        let result = self.kernel.submit_action_from_player(player_id, action);
        self.assert_journal_sidecar();
        result
    }
    pub fn step(&mut self) -> Option<WorldEvent> {
        let result = self.kernel.step();
        self.assert_journal_sidecar();
        result.map(|event| WorldEvent::from_kernel_parts(event, None))
    }
    pub fn step_intents_batch(&mut self) -> Vec<WorldEvent> {
        let result = self.kernel.step_intents_batch();
        self.assert_journal_sidecar();
        result
            .into_iter()
            .map(|event| WorldEvent::from_kernel_parts(event, None))
            .collect()
    }
    pub fn step_intents_batch_from_envelopes(
        &mut self,
        intents: Vec<ActionEnvelope>,
    ) -> Vec<WorldEvent> {
        let result = self.kernel.step_intents_batch_from_envelopes(intents);
        self.assert_journal_sidecar();
        result
            .into_iter()
            .map(|event| WorldEvent::from_kernel_parts(event, None))
            .collect()
    }
    pub fn step_until_empty(&mut self) -> Vec<WorldEvent> {
        let result = self.kernel.step_until_empty();
        self.assert_journal_sidecar();
        result
            .into_iter()
            .map(|event| WorldEvent::from_kernel_parts(event, None))
            .collect()
    }
    pub fn process_power_tick(&mut self) -> Vec<WorldEvent> {
        let result = self.kernel.process_power_tick();
        self.assert_journal_sidecar();
        result
            .into_iter()
            .map(|event| WorldEvent::from_kernel_parts(event, None))
            .collect()
    }
    pub fn process_power_generation_tick(&mut self) -> Vec<WorldEvent> {
        let result = self.kernel.process_power_generation_tick();
        self.assert_journal_sidecar();
        result
            .into_iter()
            .map(|event| WorldEvent::from_kernel_parts(event, None))
            .collect()
    }
    pub fn consume_agent_power(
        &mut self,
        agent_id: &AgentId,
        amount: i64,
        reason: ConsumeReason,
    ) -> Option<WorldEvent> {
        let result = self.kernel.consume_agent_power(agent_id, amount, reason);
        self.assert_journal_sidecar();
        result.map(|event| WorldEvent::from_kernel_parts(event, None))
    }
    pub fn charge_agent_power(&mut self, agent_id: &AgentId, amount: i64) -> Option<WorldEvent> {
        let result = self.kernel.charge_agent_power(agent_id, amount);
        self.assert_journal_sidecar();
        result.map(|event| WorldEvent::from_kernel_parts(event, None))
    }
    pub fn observe(&mut self, agent_id: &str) -> Result<Observation, RejectReason> {
        let result = self.kernel.observe(agent_id);
        self.assert_journal_sidecar();
        result
    }
    pub fn set_micro_depot_wasm_module_evaluator<S>(
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
        self.kernel.set_micro_depot_wasm_module_evaluator(
            module_id, wasm_hash, entrypoint, wasm_bytes, limits, sandbox,
        );
        self.assert_journal_sidecar();
    }
    pub fn register_micro_depot_wasm_artifact(
        &mut self,
        wasm_hash: impl Into<String>,
        wasm_bytes: Vec<u8>,
    ) -> Result<(), String> {
        let result = self
            .kernel
            .register_micro_depot_wasm_artifact(wasm_hash, wasm_bytes);
        self.assert_journal_sidecar();
        result
    }
    pub fn set_micro_depot_wasm_module_from_registry<S>(
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
        let result = self.kernel.set_micro_depot_wasm_module_from_registry(
            module_id, wasm_hash, entrypoint, limits, sandbox,
        );
        self.assert_journal_sidecar();
        result
    }

    pub fn add_post_action_rule_hook<F>(&mut self, hook: F)
    where
        F: Fn(ActionId, &Action, &WorldEvent) + Send + Sync + 'static,
    {
        self.kernel
            .add_post_action_rule_hook(move |id, action, event| {
                hook(
                    id,
                    action,
                    &WorldEvent::from_kernel_parts(event.clone(), None),
                );
            });
    }
}

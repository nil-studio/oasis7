use std::collections::BTreeSet;
use std::sync::Arc;

use oasis7_wasm_abi::{
    ModuleCallCaller, ModuleCallErrorCode, ModuleCallFailure, ModuleCallInput, ModuleCallOrigin,
    ModuleCallRequest, ModuleCommandEnvelope, ModuleContext, ModuleEmitEvent,
    ModuleInvocationProvenance, ModuleOutput, ModuleSandbox, ModuleStateUpdate,
    validate_module_command_declarations, validate_module_command_envelope,
};
#[cfg(test)]
use oasis7_wasm_router::{PreparedSubscription, prepare_subscriptions};

use super::super::util::{hash_json, to_canonical_cbor};
use super::super::{
    EffectOrigin, ModuleArtifact, ModuleEvent, ModuleEventKind, ModuleKind, ModuleLimits,
    ModuleManifest, ModuleRegistry, WorldError, WorldEventBody, WorldTime,
};
#[cfg(test)]
use super::PreparedSubscriptionCacheEntry;
use super::World;
use super::capability_authorization_command_stage::{
    ModuleCallTarget, TrustedCommandStage, execute_module_call_for_target,
};
use super::module_runtime_labels::{module_kind_label, module_role_label};
use oasis7_agent_api::world_action::ModuleInstallTarget;

fn count_exceeds_limit(count: usize, limit: u32) -> bool {
    match u32::try_from(count) {
        Ok(value) => value > limit,
        Err(_) => true,
    }
}

/// The small operation surface shared by the canonical world and a borrowed
#[expect(
    clippy::too_many_arguments,
    reason = "Module execution context is passed explicitly to keep capability and sandbox boundaries visible."
)]
pub(super) fn process_module_output_for_target<T: ModuleCallTarget + ?Sized>(
    target: &mut T,
    module_id: &str,
    state_key: &str,
    trace_id: &str,
    manifest: &ModuleManifest,
    input_bytes: u64,
    output: &ModuleOutput,
    sandbox: &mut dyn ModuleSandbox,
) -> Result<(), WorldError> {
    if manifest.kind == ModuleKind::Pure && output.new_state.is_some() {
        return target.module_call_failed_for_call(ModuleCallFailure {
            module_id: module_id.to_string(),
            trace_id: trace_id.to_string(),
            code: ModuleCallErrorCode::InvalidOutput,
            detail: "pure module returned new_state".to_string(),
        });
    }
    if count_exceeds_limit(output.effects.len(), manifest.limits.max_effects) {
        return target.module_call_failed_for_call(ModuleCallFailure {
            module_id: module_id.to_string(),
            trace_id: trace_id.to_string(),
            code: ModuleCallErrorCode::EffectLimitExceeded,
            detail: "effects exceeded".to_string(),
        });
    }
    if count_exceeds_limit(output.emits.len(), manifest.limits.max_emits) {
        return target.module_call_failed_for_call(ModuleCallFailure {
            module_id: module_id.to_string(),
            trace_id: trace_id.to_string(),
            code: ModuleCallErrorCode::EmitLimitExceeded,
            detail: "emits exceeded".to_string(),
        });
    }
    if output.output_bytes > manifest.limits.max_output_bytes {
        return target.module_call_failed_for_call(ModuleCallFailure {
            module_id: module_id.to_string(),
            trace_id: trace_id.to_string(),
            code: ModuleCallErrorCode::OutputTooLarge,
            detail: "output bytes exceeded".to_string(),
        });
    }

    let mut resolved_caps = Vec::with_capacity(output.effects.len());
    for effect in &output.effects {
        let resolved_cap_ref = if let Some(slot) = effect.cap_slot.as_deref() {
            let Some(bound_cap_ref) = manifest.abi_contract.cap_slots.get(slot) else {
                return target.module_call_failed_for_call(ModuleCallFailure {
                    module_id: module_id.to_string(),
                    trace_id: trace_id.to_string(),
                    code: ModuleCallErrorCode::CapsDenied,
                    detail: format!("cap_slot not bound {}", slot),
                });
            };
            if !effect.cap_ref.trim().is_empty() && effect.cap_ref != *bound_cap_ref {
                return target.module_call_failed_for_call(ModuleCallFailure {
                    module_id: module_id.to_string(),
                    trace_id: trace_id.to_string(),
                    code: ModuleCallErrorCode::CapsDenied,
                    detail: format!(
                        "cap_slot {} conflicts with cap_ref {}",
                        slot, effect.cap_ref
                    ),
                });
            }
            bound_cap_ref.clone()
        } else {
            if effect.cap_ref.trim().is_empty() {
                return target.module_call_failed_for_call(ModuleCallFailure {
                    module_id: module_id.to_string(),
                    trace_id: trace_id.to_string(),
                    code: ModuleCallErrorCode::CapsDenied,
                    detail: "cap_ref is empty".to_string(),
                });
            }
            effect.cap_ref.clone()
        };

        if !manifest
            .required_caps
            .iter()
            .any(|cap| cap == &resolved_cap_ref)
        {
            return target.module_call_failed_for_call(ModuleCallFailure {
                module_id: module_id.to_string(),
                trace_id: trace_id.to_string(),
                code: ModuleCallErrorCode::CapsDenied,
                detail: format!("cap_ref not allowed {}", resolved_cap_ref),
            });
        }

        if let Err(failure) = enforce_pure_policy_hooks_for_target(
            target,
            module_id,
            trace_id,
            manifest,
            effect,
            &resolved_cap_ref,
            sandbox,
        ) {
            return target.module_call_failed_for_call(failure);
        }
        resolved_caps.push(resolved_cap_ref);
    }

    if let Err(failure) = target.try_charge_module_runtime_for_call(
        module_id,
        trace_id,
        manifest,
        input_bytes,
        output,
    ) {
        return target.module_call_failed_for_call(failure);
    }

    let mut intents = Vec::new();
    for (effect, resolved_cap_ref) in output.effects.iter().zip(resolved_caps) {
        let intent = match target.build_effect_intent_for_call(
            effect.kind.clone(),
            effect.params.clone(),
            resolved_cap_ref,
            EffectOrigin::Module {
                module_id: module_id.to_string(),
            },
        ) {
            Ok(intent) => intent,
            Err(err) => {
                let (code, detail) = match err {
                    WorldError::CapabilityMissing { cap_ref } => (
                        ModuleCallErrorCode::CapsDenied,
                        format!("cap missing {cap_ref}"),
                    ),
                    WorldError::CapabilityExpired { cap_ref } => (
                        ModuleCallErrorCode::CapsDenied,
                        format!("cap expired {cap_ref}"),
                    ),
                    WorldError::CapabilityNotAllowed { cap_ref, kind } => (
                        ModuleCallErrorCode::CapsDenied,
                        format!("cap not allowed {cap_ref} {kind}"),
                    ),
                    WorldError::PolicyDenied { reason, .. } => {
                        (ModuleCallErrorCode::PolicyDenied, reason)
                    }
                    other => (ModuleCallErrorCode::InvalidOutput, format!("{other:?}")),
                };
                return target.module_call_failed_for_call(ModuleCallFailure {
                    module_id: module_id.to_string(),
                    trace_id: trace_id.to_string(),
                    code,
                    detail,
                });
            }
        };
        intents.push(intent);
    }

    if let Some(state) = &output.new_state {
        target.append_module_event_for_call(WorldEventBody::ModuleStateUpdated(
            ModuleStateUpdate {
                module_id: state_key.to_string(),
                trace_id: trace_id.to_string(),
                state: state.clone(),
            },
        ))?;
    }

    for intent in intents {
        target.append_module_event_for_call(WorldEventBody::EffectQueued(intent))?;
    }

    for emit in &output.emits {
        target.append_module_event_for_call(WorldEventBody::ModuleEmitted(ModuleEmitEvent {
            module_id: module_id.to_string(),
            trace_id: trace_id.to_string(),
            kind: emit.kind.clone(),
            payload: emit.payload.clone(),
        }))?;
    }

    Ok(())
}

fn enforce_pure_policy_hooks_for_target<T: ModuleCallTarget + ?Sized>(
    target: &mut T,
    module_id: &str,
    trace_id: &str,
    manifest: &ModuleManifest,
    effect: &oasis7_wasm_abi::ModuleEffectIntent,
    resolved_cap_ref: &str,
    sandbox: &mut dyn ModuleSandbox,
) -> Result<(), ModuleCallFailure> {
    for policy_module_id in &manifest.abi_contract.policy_hooks {
        let policy_manifest = target
            .active_manifest_for_call(policy_module_id)
            .map_err(|err| ModuleCallFailure {
                module_id: module_id.to_string(),
                trace_id: trace_id.to_string(),
                code: ModuleCallErrorCode::PolicyDenied,
                detail: format!(
                    "pure policy hook {} not available: {err:?}",
                    policy_module_id
                ),
            })?;
        if policy_manifest.kind != ModuleKind::Pure {
            return Err(ModuleCallFailure {
                module_id: module_id.to_string(),
                trace_id: trace_id.to_string(),
                code: ModuleCallErrorCode::PolicyDenied,
                detail: format!("pure policy hook {} is not pure", policy_module_id),
            });
        }
        let hook_trace_id = format!("policy-{trace_id}-{policy_module_id}");
        let world_config_hash =
            target
                .current_manifest_hash_for_call()
                .map_err(|err| ModuleCallFailure {
                    module_id: module_id.to_string(),
                    trace_id: trace_id.to_string(),
                    code: ModuleCallErrorCode::PolicyDenied,
                    detail: format!(
                        "pure policy hook {} cannot read world config hash: {err:?}",
                        policy_module_id
                    ),
                })?;
        let policy_manifest_hash =
            hash_json(&policy_manifest).map_err(|err| ModuleCallFailure {
                module_id: module_id.to_string(),
                trace_id: trace_id.to_string(),
                code: ModuleCallErrorCode::PolicyDenied,
                detail: format!(
                    "pure policy hook {} cannot hash module manifest: {err:?}",
                    policy_module_id
                ),
            })?;
        let ctx = ModuleContext {
            v: "wasm-1".to_string(),
            module_id: policy_module_id.clone(),
            trace_id: hook_trace_id.clone(),
            time: target.state_time_for_call(),
            origin: ModuleCallOrigin {
                kind: "module_policy".to_string(),
                id: trace_id.to_string(),
            },
            caller: ModuleCallCaller::Module {
                module_id: module_id.to_string(),
            },
            limits: policy_manifest.limits.clone(),
            stage: Some("module_policy".to_string()),
            world_config_hash: Some(world_config_hash),
            manifest_hash: Some(policy_manifest_hash),
            journal_height: Some(target.journal_height_for_call()),
            module_version: Some(policy_manifest.version.clone()),
            module_kind: Some(module_kind_label(&policy_manifest.kind).to_string()),
            module_role: Some(module_role_label(&policy_manifest.role).to_string()),
        };
        let policy_payload = serde_json::json!({
            "source_module_id": module_id,
            "trace_id": trace_id,
            "effect_kind": effect.kind,
            "effect_params": effect.params,
            "cap_ref": resolved_cap_ref,
        });
        let input = ModuleCallInput {
            ctx,
            event: None,
            action: Some(
                to_canonical_cbor(&policy_payload).map_err(|err| ModuleCallFailure {
                    module_id: module_id.to_string(),
                    trace_id: trace_id.to_string(),
                    code: ModuleCallErrorCode::PolicyDenied,
                    detail: format!(
                        "pure policy hook {} input encode failed: {err:?}",
                        policy_module_id
                    ),
                })?,
            ),
            state: None,
        };
        let input_bytes = to_canonical_cbor(&input).map_err(|err| ModuleCallFailure {
            module_id: module_id.to_string(),
            trace_id: trace_id.to_string(),
            code: ModuleCallErrorCode::PolicyDenied,
            detail: format!(
                "pure policy hook {} input envelope encode failed: {err:?}",
                policy_module_id
            ),
        })?;
        let hook_output = target
            .call_module_raw_for_call(
                policy_module_id,
                &hook_trace_id,
                input_bytes,
                &policy_manifest,
                sandbox,
            )
            .map_err(|failure| ModuleCallFailure {
                module_id: module_id.to_string(),
                trace_id: trace_id.to_string(),
                code: ModuleCallErrorCode::PolicyDenied,
                detail: format!(
                    "pure policy hook {} call failed: {}",
                    policy_module_id, failure.detail
                ),
            })?;
        if hook_output.new_state.is_some() || !hook_output.effects.is_empty() {
            return Err(ModuleCallFailure {
                module_id: module_id.to_string(),
                trace_id: trace_id.to_string(),
                code: ModuleCallErrorCode::PolicyDenied,
                detail: format!(
                    "pure policy hook {} returned state/effects",
                    policy_module_id
                ),
            });
        }
        if hook_output.emits.len() > 1 {
            return Err(ModuleCallFailure {
                module_id: module_id.to_string(),
                trace_id: trace_id.to_string(),
                code: ModuleCallErrorCode::PolicyDenied,
                detail: format!(
                    "pure policy hook {} returned multiple emits",
                    policy_module_id
                ),
            });
        }
        if let Some(emit) = hook_output.emits.first() {
            match emit.kind.as_str() {
                "policy.allow" => {}
                "policy.deny" => {
                    let reason = emit
                        .payload
                        .get("reason")
                        .and_then(|value| value.as_str())
                        .unwrap_or("pure_policy_denied");
                    return Err(ModuleCallFailure {
                        module_id: module_id.to_string(),
                        trace_id: trace_id.to_string(),
                        code: ModuleCallErrorCode::PolicyDenied,
                        detail: format!(
                            "pure policy hook {} denied effect: {}",
                            policy_module_id, reason
                        ),
                    });
                }
                other => {
                    return Err(ModuleCallFailure {
                        module_id: module_id.to_string(),
                        trace_id: trace_id.to_string(),
                        code: ModuleCallErrorCode::PolicyDenied,
                        detail: format!(
                            "pure policy hook {} returned unknown emit {}",
                            policy_module_id, other
                        ),
                    });
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "module_runtime_tests.rs"]
mod tests;

#[cfg(test)]
fn prepared_subscription_cache_key(manifest: &ModuleManifest) -> Result<String, WorldError> {
    let record_key = ModuleRegistry::record_key(&manifest.module_id, &manifest.version);
    let subscription_hash = hash_json(&manifest.subscriptions)?;
    Ok(format!(
        "{record_key}|wasm={}|subs={subscription_hash}",
        manifest.wasm_hash
    ))
}

#[cfg(test)]
fn prepared_subscription_lookup_key(manifest: &ModuleManifest) -> String {
    format!(
        "{}|wasm={}",
        ModuleRegistry::record_key(&manifest.module_id, &manifest.version),
        manifest.wasm_hash
    )
}

#[derive(Debug, Clone)]
pub(super) struct ActiveModuleInvocation {
    pub(super) instance_id: String,
    pub(super) module_id: String,
    pub(super) install_target: ModuleInstallTarget,
    pub(super) manifest: ModuleManifest,
}

impl World {
    // ---------------------------------------------------------------------
    // Module artifact and limits
    // ---------------------------------------------------------------------

    fn remove_prepared_subscription_cache_entries(&mut self, module_id: &str, version: &str) {
        let prefix = format!("{}|", ModuleRegistry::record_key(module_id, version));
        self.prepared_subscription_cache
            .retain(|key, _| !key.starts_with(&prefix));
    }

    pub(super) fn prepare_module_artifact_registration(
        &self,
        wasm_hash: impl Into<String>,
        bytes: &[u8],
    ) -> Result<PreparedModuleArtifactRegistration, WorldError> {
        let wasm_hash = wasm_hash.into();
        let computed = super::super::util::sha256_hex(bytes);
        if computed != wasm_hash {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!("artifact hash mismatch expected {wasm_hash} found {computed}"),
            });
        }
        Ok(PreparedModuleArtifactRegistration {
            wasm_hash,
            bytes: Arc::<[u8]>::from(bytes),
        })
    }

    pub fn register_module_artifact(
        &mut self,
        wasm_hash: impl Into<String>,
        bytes: &[u8],
    ) -> Result<(), WorldError> {
        self.prepare_module_artifact_registration(wasm_hash, bytes)?
            .install(self);
        Ok(())
    }

    pub fn set_module_limits_max(&mut self, limits: ModuleLimits) {
        self.module_limits_max = limits;
    }

    pub fn set_module_cache_max(&mut self, max_cached_modules: usize) {
        self.module_cache.set_max_cached_modules(max_cached_modules);
    }

    pub fn load_module(&mut self, wasm_hash: &str) -> Result<ModuleArtifact, WorldError> {
        if let Some(artifact) = self.module_cache.get(wasm_hash) {
            return Ok(artifact);
        }
        let bytes = self
            .module_artifact_bytes
            .get(wasm_hash)
            .ok_or_else(|| WorldError::ModuleChangeInvalid {
                reason: format!("module artifact bytes missing {wasm_hash}"),
            })?
            .clone();
        let artifact = ModuleArtifact {
            wasm_hash: wasm_hash.to_string(),
            bytes,
        };
        self.module_cache.insert(artifact.clone());
        Ok(artifact)
    }

    pub fn validate_module_output_limits(
        &self,
        module_id: &str,
        limits: &ModuleLimits,
        effect_count: usize,
        emit_count: usize,
        output_bytes: u64,
    ) -> Result<(), WorldError> {
        if count_exceeds_limit(effect_count, limits.max_effects) {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!("module output effects exceeded {module_id}"),
            });
        }
        if count_exceeds_limit(emit_count, limits.max_emits) {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!("module output emits exceeded {module_id}"),
            });
        }
        if output_bytes > limits.max_output_bytes {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!("module output bytes exceeded {module_id}"),
            });
        }
        Ok(())
    }

    #[cfg(test)]
    fn prepared_subscriptions_for_manifest(
        &mut self,
        manifest: &ModuleManifest,
    ) -> Result<Arc<[PreparedSubscription]>, WorldError> {
        let key = prepared_subscription_lookup_key(manifest);
        if let Some(entry) = self.prepared_subscription_cache.get(&key)
            && entry.subscriptions == manifest.subscriptions
        {
            return Ok(entry.prepared.clone());
        }
        let subscription_fingerprint = prepared_subscription_cache_key(manifest)?;
        let prepared = prepare_subscriptions(&manifest.subscriptions, &manifest.module_id)
            .map_err(|reason| WorldError::ModuleChangeInvalid { reason })?;
        self.prepared_subscription_cache.insert(
            key,
            PreparedSubscriptionCacheEntry {
                subscriptions: manifest.subscriptions.clone(),
                _subscription_fingerprint: subscription_fingerprint,
                prepared: prepared.clone(),
            },
        );
        Ok(prepared)
    }

    pub(super) fn execute_module_command_with_provenance_inner(
        &self,
        staged: &mut TrustedCommandStage<'_>,
        module_id: &str,
        trace_id: impl Into<String>,
        envelope: ModuleCommandEnvelope,
        provenance: ModuleInvocationProvenance,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<ModuleOutput, WorldError> {
        let manifest = self.active_module_manifest(module_id)?.clone();
        self.validate_required_module_capabilities(&manifest)?;
        validate_module_command_declarations(&manifest.abi_contract.declarations).map_err(
            |error| WorldError::ModuleChangeInvalid {
                reason: format!(
                    "module command declarations invalid for {}: {error}",
                    manifest.module_id
                ),
            },
        )?;
        validate_module_command_envelope(&envelope, &manifest.abi_contract.declarations).map_err(
            |error| WorldError::ModuleChangeInvalid {
                reason: format!("module command envelope invalid for {module_id}: {error}"),
            },
        )?;
        let canonical_input =
            envelope
                .encode_canonical()
                .map_err(|error| WorldError::ModuleChangeInvalid {
                    reason: format!("module command envelope canonical encoding failed: {error}"),
                })?;

        let trace_id = trace_id.into();
        let world_config_hash = self.current_manifest_hash()?;
        let module_manifest_hash = hash_json(&manifest)?;
        let state = match manifest.kind {
            ModuleKind::Reducer => Some(staged.module_state(module_id)),
            ModuleKind::Pure => None,
        };
        let input = ModuleCallInput {
            ctx: ModuleContext {
                v: "wasm-1".to_string(),
                module_id: module_id.to_string(),
                trace_id: trace_id.clone(),
                time: self.state.time,
                origin: provenance.origin,
                caller: provenance.caller,
                limits: manifest.limits.clone(),
                stage: Some("module_command".to_string()),
                world_config_hash: Some(world_config_hash),
                manifest_hash: Some(module_manifest_hash),
                journal_height: Some(self.journal.events.len() as u64),
                module_version: Some(manifest.version.clone()),
                module_kind: Some(module_kind_label(&manifest.kind).to_string()),
                module_role: Some(module_role_label(&manifest.role).to_string()),
            },
            event: None,
            action: Some(canonical_input),
            state,
        };
        let input_bytes = to_canonical_cbor(&input)?;

        execute_module_call_for_target(
            staged,
            module_id,
            module_id,
            &manifest,
            trace_id,
            input_bytes,
            sandbox,
        )
    }

    fn validate_required_module_capabilities(
        &self,
        manifest: &ModuleManifest,
    ) -> Result<(), WorldError> {
        for cap in &manifest.required_caps {
            self.validate_module_required_capability(cap)?;
        }
        Ok(())
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

    pub(super) fn collect_active_module_invocations(
        &self,
    ) -> Result<Vec<ActiveModuleInvocation>, WorldError> {
        let mut invocations = Vec::new();
        let mut module_ids_with_instances = BTreeSet::new();

        for instance in self.state.module_instances.values() {
            module_ids_with_instances.insert(instance.module_id.clone());
        }

        let mut instance_ids: Vec<String> = self.state.module_instances.keys().cloned().collect();
        instance_ids.sort();
        for instance_id in instance_ids {
            let Some(instance) = self.state.module_instances.get(&instance_id) else {
                continue;
            };
            if !instance.active {
                continue;
            }
            let key = ModuleRegistry::record_key(&instance.module_id, &instance.module_version);
            let record = self.module_registry.records.get(&key).ok_or_else(|| {
                WorldError::ModuleChangeInvalid {
                    reason: format!("module record missing {key}"),
                }
            })?;
            invocations.push(ActiveModuleInvocation {
                instance_id: instance.instance_id.clone(),
                module_id: instance.module_id.clone(),
                install_target: instance.install_target.clone(),
                manifest: record.manifest.clone(),
            });
        }

        let mut module_ids_without_instances: Vec<String> =
            self.module_registry.active.keys().cloned().collect();
        module_ids_without_instances.sort();
        for module_id in module_ids_without_instances {
            if module_ids_with_instances.contains(&module_id) {
                continue;
            }
            let version = self.module_registry.active.get(&module_id).ok_or_else(|| {
                WorldError::ModuleChangeInvalid {
                    reason: format!("module not active {module_id}"),
                }
            })?;
            let key = ModuleRegistry::record_key(&module_id, version);
            let record = self.module_registry.records.get(&key).ok_or_else(|| {
                WorldError::ModuleChangeInvalid {
                    reason: format!("module record missing {key}"),
                }
            })?;
            invocations.push(ActiveModuleInvocation {
                instance_id: module_id.clone(),
                module_id: module_id.clone(),
                install_target: self
                    .state
                    .installed_module_targets
                    .get(&module_id)
                    .cloned()
                    .unwrap_or(ModuleInstallTarget::SelfAgent),
                manifest: record.manifest.clone(),
            });
        }

        invocations.sort_by(|left, right| left.instance_id.cmp(&right.instance_id));
        Ok(invocations)
    }

    pub(super) fn active_module_invocation_for_id(
        &self,
        invocation_id: &str,
    ) -> Result<Option<ActiveModuleInvocation>, WorldError> {
        if let Some(instance) = self.state.module_instances.get(invocation_id) {
            if !instance.active {
                return Ok(None);
            }
            let key = ModuleRegistry::record_key(&instance.module_id, &instance.module_version);
            let record = self.module_registry.records.get(&key).ok_or_else(|| {
                WorldError::ModuleChangeInvalid {
                    reason: format!("module record missing {key}"),
                }
            })?;
            return Ok(Some(ActiveModuleInvocation {
                instance_id: instance.instance_id.clone(),
                module_id: instance.module_id.clone(),
                install_target: instance.install_target.clone(),
                manifest: record.manifest.clone(),
            }));
        }

        if self
            .state
            .module_instances
            .values()
            .any(|instance| instance.module_id == invocation_id)
        {
            return Ok(None);
        }

        let Some(version) = self.module_registry.active.get(invocation_id) else {
            return Ok(None);
        };
        let key = ModuleRegistry::record_key(invocation_id, version);
        let record = self.module_registry.records.get(&key).ok_or_else(|| {
            WorldError::ModuleChangeInvalid {
                reason: format!("module record missing {key}"),
            }
        })?;
        Ok(Some(ActiveModuleInvocation {
            instance_id: invocation_id.to_string(),
            module_id: invocation_id.to_string(),
            install_target: self
                .state
                .installed_module_targets
                .get(invocation_id)
                .cloned()
                .unwrap_or(ModuleInstallTarget::SelfAgent),
            manifest: record.manifest.clone(),
        }))
    }

    fn module_call_failed(&mut self, failure: ModuleCallFailure) -> Result<(), WorldError> {
        self.append_event(WorldEventBody::ModuleCallFailed(failure.clone()), None)?;
        Err(WorldError::ModuleCallFailed {
            module_id: failure.module_id,
            trace_id: failure.trace_id,
            code: failure.code,
            detail: failure.detail,
        })
    }

    #[cfg(any(test, feature = "test_tier_required", feature = "test-support"))]
    pub(super) fn apply_module_changes(
        &mut self,
        proposal_id: super::super::ProposalId,
        changes: &super::super::ModuleChangeSet,
        actor: &str,
    ) -> Result<(), WorldError> {
        self.apply_prepared_module_change_batch(proposal_id, changes, actor)
    }

    #[cfg(any(test, feature = "test_tier_required", feature = "test-support"))]
    pub(crate) fn apply_module_changes_for_test(
        &mut self,
        proposal_id: super::super::ProposalId,
        changes: &super::super::ModuleChangeSet,
        actor: &str,
    ) -> Result<(), WorldError> {
        self.apply_module_changes(proposal_id, changes, actor)
    }

    pub(super) fn apply_module_event(
        &mut self,
        event: &ModuleEvent,
        time: super::super::WorldTime,
    ) -> Result<(), WorldError> {
        match &event.kind {
            ModuleEventKind::RegisterModule {
                module,
                registered_by,
            } => {
                let key = ModuleRegistry::record_key(&module.module_id, &module.version);
                self.remove_prepared_subscription_cache_entries(&module.module_id, &module.version);
                self.module_registry.records.insert(
                    key,
                    super::super::ModuleRecord {
                        manifest: module.clone(),
                        registered_at: time,
                        registered_by: registered_by.clone(),
                        audit_event_id: None,
                    },
                );
                self.module_artifacts.insert(module.wasm_hash.clone());
            }
            ModuleEventKind::UpgradeModule {
                module_id,
                to_version,
                manifest,
                upgraded_by,
                ..
            } => {
                let key = ModuleRegistry::record_key(module_id, to_version);
                self.remove_prepared_subscription_cache_entries(module_id, to_version);
                self.module_registry.records.insert(
                    key,
                    super::super::ModuleRecord {
                        manifest: manifest.clone(),
                        registered_at: time,
                        registered_by: upgraded_by.clone(),
                        audit_event_id: None,
                    },
                );
                self.module_artifacts.insert(manifest.wasm_hash.clone());
            }
            ModuleEventKind::ActivateModule {
                module_id, version, ..
            } => {
                self.module_registry
                    .active
                    .insert(module_id.clone(), version.clone());
                self.sync_tick_schedule_for_activation(module_id, version, time)?;
            }
            ModuleEventKind::DeactivateModule { module_id, .. } => {
                self.module_registry.active.remove(module_id);
                self.remove_tick_schedule(module_id);
            }
        }
        Ok(())
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
        let artifact = self
            .load_module(&wasm_hash)
            .map_err(|err| ModuleCallFailure {
                module_id: module_id.to_string(),
                trace_id: trace_id.to_string(),
                code: ModuleCallErrorCode::SandboxUnavailable,
                detail: format!("load module failed: {err:?}"),
            })?;

        let request = ModuleCallRequest {
            module_id: module_id.to_string(),
            wasm_hash,
            trace_id: trace_id.to_string(),
            entrypoint: manifest.kind.entrypoint().to_string(),
            input,
            limits: manifest.limits.clone(),
            wasm_bytes: artifact.bytes.clone(),
        };
        sandbox.call(&request)
    }
}

pub(super) struct PreparedModuleArtifactRegistration {
    wasm_hash: String,
    bytes: Arc<[u8]>,
}

impl PreparedModuleArtifactRegistration {
    pub(super) fn wasm_hash(&self) -> &str {
        &self.wasm_hash
    }

    pub(super) fn install(self, world: &mut World) {
        world.module_artifacts.insert(self.wasm_hash.clone());
        world
            .module_artifact_bytes
            .insert(self.wasm_hash, self.bytes);
    }
}

impl ModuleCallTarget for World {
    fn active_manifest_for_call(&self, module_id: &str) -> Result<ModuleManifest, WorldError> {
        self.active_module_manifest(module_id).cloned()
    }

    fn current_manifest_hash_for_call(&self) -> Result<String, WorldError> {
        self.current_manifest_hash()
    }

    fn state_time_for_call(&self) -> WorldTime {
        self.state.time
    }

    fn journal_height_for_call(&self) -> u64 {
        self.journal.events.len() as u64
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
        self.module_call_failed(failure)
    }

    fn append_module_event_for_call(&mut self, body: WorldEventBody) -> Result<u64, WorldError> {
        self.append_event(body, None)
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
    ) -> Result<super::super::EffectIntent, WorldError> {
        self.build_effect_intent(kind, params, cap_ref, origin)
    }
}

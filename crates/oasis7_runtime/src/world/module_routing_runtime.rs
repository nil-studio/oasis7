use oasis7_wasm_abi::{
    ModuleCallCaller, ModuleCallFailure, ModuleCallInput, ModuleCallOrigin, ModuleContext,
    ModuleSandbox,
};
use oasis7_wasm_router::{
    prepare_subscriptions, prepared_module_subscribes_to_action,
    prepared_module_subscribes_to_event,
};

use super::super::util::{hash_json, to_canonical_cbor};
use super::super::{
    ActionEnvelope, ModuleKind, ModuleSubscriptionStage, WorldError, WorldEvent, WorldEventBody,
};
use super::World;
use super::capability_authorization_command_stage::{
    PreparedTrustedCommand, TrustedCommandStage, execute_module_call_for_target,
};
use super::module_runtime_labels::{
    action_kind_label, event_kind_label, module_kind_label, module_role_label,
    subscription_stage_label,
};

impl World {
    pub fn route_event_to_modules(
        &mut self,
        event: &WorldEvent,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<usize, WorldError> {
        self.route_event_to_modules_with_event_era(event, None, sandbox)
    }

    pub(super) fn route_event_to_modules_with_event_era(
        &mut self,
        event: &WorldEvent,
        product_validation_event_id_era: Option<u64>,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<usize, WorldError> {
        let mut staged = TrustedCommandStage::new(self)?;
        let routed = self.route_event_to_staged(
            &mut staged,
            event,
            product_validation_event_id_era,
            sandbox,
        );
        self.finalize_prepared_module_route(staged.prepare_route(routed))
    }

    fn route_event_to_staged(
        &self,
        staged: &mut TrustedCommandStage<'_>,
        event: &WorldEvent,
        product_validation_event_id_era: Option<u64>,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<usize, WorldError> {
        let event_kind = event_kind_label(&event.body);
        let event_value = serde_json::to_value(event)?;
        let invocations = self.collect_active_module_invocations()?;
        let mut event_bytes = None;
        let mut world_config_hash = None;
        let mut invoked = 0;

        for invocation in invocations {
            let manifest = invocation.manifest;
            let module_id = invocation.module_id;
            let instance_id = invocation.instance_id;
            let prepared = prepare_subscriptions(&manifest.subscriptions, &manifest.module_id)
                .map_err(|reason| WorldError::ModuleChangeInvalid { reason })?;
            if !prepared_module_subscribes_to_event(prepared.as_ref(), event_kind, &event_value) {
                continue;
            }

            if event_bytes.is_none() {
                event_bytes = Some(to_canonical_cbor(event)?);
            }
            if world_config_hash.is_none() {
                world_config_hash = Some(self.current_manifest_hash()?);
            }

            let trace_id = product_validation_event_id_era.map_or_else(
                || format!("event-{}-{}", event.id, instance_id),
                |event_id_era| {
                    super::economy_product_validation::product_validation_event_trace_id(
                        event_id_era,
                        event.id,
                        instance_id.as_str(),
                    )
                },
            );
            let module_manifest_hash = hash_json(&manifest)?;
            let input = ModuleCallInput {
                ctx: ModuleContext {
                    v: "wasm-1".to_string(),
                    module_id: module_id.clone(),
                    trace_id: trace_id.clone(),
                    time: event.time,
                    origin: ModuleCallOrigin {
                        kind: "event".to_string(),
                        id: event.id.to_string(),
                    },
                    caller: ModuleCallCaller::LegacyUnspecified,
                    limits: manifest.limits.clone(),
                    stage: Some("post_event".to_string()),
                    world_config_hash: world_config_hash.clone(),
                    manifest_hash: Some(module_manifest_hash),
                    journal_height: Some(event.id),
                    module_version: Some(manifest.version.clone()),
                    module_kind: Some(module_kind_label(&manifest.kind).to_string()),
                    module_role: Some(module_role_label(&manifest.role).to_string()),
                },
                event: event_bytes.clone(),
                action: None,
                state: match manifest.kind {
                    ModuleKind::Reducer => Some(staged.module_state(&instance_id)),
                    ModuleKind::Pure => None,
                },
            };
            let input_bytes = to_canonical_cbor(&input)?;
            execute_module_call_for_target(
                staged,
                module_id.as_str(),
                instance_id.as_str(),
                &manifest,
                trace_id,
                input_bytes,
                sandbox,
            )?;
            invoked += 1;
        }

        Ok(invoked)
    }

    pub fn route_action_to_modules(
        &mut self,
        envelope: &ActionEnvelope,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<usize, WorldError> {
        self.route_action_to_modules_with_stage(
            envelope,
            ModuleSubscriptionStage::PreAction,
            sandbox,
        )
    }

    pub fn route_action_to_modules_with_stage(
        &mut self,
        envelope: &ActionEnvelope,
        stage: ModuleSubscriptionStage,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<usize, WorldError> {
        self.route_action_to_modules_with_stage_and_event(envelope, stage, None, sandbox)
    }

    pub(super) fn route_action_to_modules_with_stage_and_event(
        &mut self,
        envelope: &ActionEnvelope,
        stage: ModuleSubscriptionStage,
        result_event: Option<&WorldEvent>,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<usize, WorldError> {
        let mut staged = TrustedCommandStage::new(self)?;
        let routed =
            self.route_action_to_staged(&mut staged, envelope, stage, result_event, sandbox);
        self.finalize_prepared_module_route(staged.prepare_route(routed))
    }

    fn route_action_to_staged(
        &self,
        staged: &mut TrustedCommandStage<'_>,
        envelope: &ActionEnvelope,
        stage: ModuleSubscriptionStage,
        result_event: Option<&WorldEvent>,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<usize, WorldError> {
        let action_kind = action_kind_label(&envelope.action);
        let action_value = serde_json::to_value(envelope)?;
        let invocations = self.collect_active_module_invocations()?;
        let mut action_bytes = None;
        let mut event_bytes = None;
        let mut world_config_hash = None;
        let mut invoked = 0;

        for invocation in invocations {
            let manifest = invocation.manifest;
            let module_id = invocation.module_id;
            let instance_id = invocation.instance_id;
            let prepared = prepare_subscriptions(&manifest.subscriptions, &manifest.module_id)
                .map_err(|reason| WorldError::ModuleChangeInvalid { reason })?;
            let subscribed = prepared_module_subscribes_to_action(
                prepared.as_ref(),
                stage,
                action_kind,
                &action_value,
            );
            if !subscribed {
                continue;
            }

            if action_bytes.is_none() {
                action_bytes = Some(to_canonical_cbor(envelope)?);
            }
            if stage == ModuleSubscriptionStage::PostAction
                && event_bytes.is_none()
                && let Some(event) = result_event
            {
                event_bytes = Some(to_canonical_cbor(event)?);
            }
            if world_config_hash.is_none() {
                world_config_hash = Some(self.current_manifest_hash()?);
            }

            let trace_id = format!("action-{}-{}", envelope.id, instance_id);
            let module_manifest_hash = hash_json(&manifest)?;
            let input = ModuleCallInput {
                ctx: ModuleContext {
                    v: "wasm-1".to_string(),
                    module_id: module_id.clone(),
                    trace_id: trace_id.clone(),
                    time: staged.state_time_for_route(),
                    origin: ModuleCallOrigin {
                        kind: "action".to_string(),
                        id: envelope.id.to_string(),
                    },
                    caller: ModuleCallCaller::LegacyUnspecified,
                    limits: manifest.limits.clone(),
                    stage: Some(subscription_stage_label(stage).to_string()),
                    world_config_hash: world_config_hash.clone(),
                    manifest_hash: Some(module_manifest_hash),
                    journal_height: Some(staged.journal_height_for_route()),
                    module_version: Some(manifest.version.clone()),
                    module_kind: Some(module_kind_label(&manifest.kind).to_string()),
                    module_role: Some(module_role_label(&manifest.role).to_string()),
                },
                event: event_bytes.clone(),
                action: action_bytes.clone(),
                state: match manifest.kind {
                    ModuleKind::Reducer => Some(staged.module_state(&instance_id)),
                    ModuleKind::Pure => None,
                },
            };
            let input_bytes = to_canonical_cbor(&input)?;
            execute_module_call_for_target(
                staged,
                module_id.as_str(),
                instance_id.as_str(),
                &manifest,
                trace_id,
                input_bytes,
                sandbox,
            )?;
            invoked += 1;
        }

        Ok(invoked)
    }

    pub(super) fn finalize_prepared_module_route(
        &mut self,
        prepared: Result<Option<(usize, PreparedTrustedCommand)>, WorldError>,
    ) -> Result<usize, WorldError> {
        match prepared {
            Ok(None) => Ok(0),
            Ok(Some((invoked, prepared))) => {
                if self.take_fail_next_append_after_publication_prepare_for_test() {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: "injected append_event failure after publication preparation"
                            .to_string(),
                    });
                }
                prepared.install(self)?;
                Ok(invoked)
            }
            Err(error @ WorldError::ModuleCallFailed { .. }) => {
                self.publish_route_failure_audit(&error)?;
                Err(error)
            }
            Err(error) => Err(error),
        }
    }

    pub(super) fn publish_route_failure_audit(
        &mut self,
        error: &WorldError,
    ) -> Result<(), WorldError> {
        let WorldError::ModuleCallFailed {
            module_id,
            trace_id,
            code,
            detail,
        } = error
        else {
            unreachable!("route failure pattern checked above")
        };
        self.append_event(
            WorldEventBody::ModuleCallFailed(ModuleCallFailure {
                module_id: module_id.clone(),
                trace_id: trace_id.clone(),
                code: code.clone(),
                detail: detail.clone(),
            }),
            None,
        )?;
        Ok(())
    }
}

use oasis7_wasm_abi::{
    ModuleCallErrorCode, ModuleCallFailure, ModuleCallRequest, ModuleOutput, ModuleSandbox,
};

use super::super::{
    ActionEnvelope, ActionOverrideRecord, CausedBy, ModuleSubscriptionStage, ResourceDelta,
    RuleDecision, RuleDecisionMergeError, RuleDecisionRecord, RuleVerdict, WorldError,
    WorldEventBody, WorldEventId, merge_rule_decisions,
};
use super::World;

const RULE_DECISION_EMIT_KIND: &str = "rule.decision";

struct CapturedOutput {
    module_id: String,
    trace_id: String,
    output: ModuleOutput,
}

struct CapturingSandbox<'a> {
    inner: &'a mut dyn ModuleSandbox,
    outputs: Vec<CapturedOutput>,
}

impl<'a> CapturingSandbox<'a> {
    fn new(inner: &'a mut dyn ModuleSandbox) -> Self {
        Self {
            inner,
            outputs: Vec::new(),
        }
    }
}

impl<'a> ModuleSandbox for CapturingSandbox<'a> {
    fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        let result = self.inner.call(request);
        if let Ok(output) = &result {
            self.outputs.push(CapturedOutput {
                module_id: request.module_id.clone(),
                trace_id: request.trace_id.clone(),
                output: output.clone(),
            });
        }
        result
    }
}

impl World {
    // ---------------------------------------------------------------------
    // Rule decision audit helpers
    // ---------------------------------------------------------------------

    pub fn record_rule_decision(
        &mut self,
        record: RuleDecisionRecord,
        caused_by: Option<CausedBy>,
    ) -> Result<WorldEventId, WorldError> {
        self.append_event(WorldEventBody::RuleDecisionRecorded(record), caused_by)
    }

    pub fn record_action_override(
        &mut self,
        record: ActionOverrideRecord,
        caused_by: Option<CausedBy>,
    ) -> Result<WorldEventId, WorldError> {
        self.append_event(WorldEventBody::ActionOverridden(record), caused_by)
    }

    pub(super) fn evaluate_rule_decisions(
        &mut self,
        envelope: &ActionEnvelope,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<RuleDecision, WorldError> {
        let mut capture = CapturingSandbox::new(sandbox);
        self.route_action_to_modules_with_stage(
            envelope,
            ModuleSubscriptionStage::PreAction,
            &mut capture,
        )?;

        let mut decisions = Vec::new();
        for output in capture.outputs {
            if let Some(decision) = self.extract_rule_decision(
                &output,
                envelope.id,
                ModuleSubscriptionStage::PreAction,
            )? {
                let record = RuleDecisionRecord {
                    action_id: envelope.id,
                    module_id: output.module_id.clone(),
                    stage: ModuleSubscriptionStage::PreAction,
                    verdict: decision.verdict,
                    override_action: decision.override_action.clone(),
                    cost: decision.cost.clone(),
                    notes: decision.notes.clone(),
                };
                self.record_rule_decision(record, Some(CausedBy::Action(envelope.id)))?;
                decisions.push(decision);
            }
        }

        match merge_rule_decisions(envelope.id, decisions) {
            Ok(merged) => Ok(merged),
            Err(err) => Ok(RuleDecision {
                action_id: envelope.id,
                verdict: RuleVerdict::Deny,
                override_action: None,
                cost: ResourceDelta::default(),
                notes: vec![format_rule_merge_error(&err)],
            }),
        }
    }

    fn extract_rule_decision(
        &mut self,
        output: &CapturedOutput,
        action_id: u64,
        stage: ModuleSubscriptionStage,
    ) -> Result<Option<RuleDecision>, WorldError> {
        let mut decision: Option<RuleDecision> = None;
        for emit in &output.output.emits {
            if emit.kind != RULE_DECISION_EMIT_KIND {
                continue;
            }
            if decision.is_some() {
                return self.module_output_invalid(
                    output,
                    format!("multiple rule decisions emitted for stage {stage:?}"),
                );
            }
            let parsed: RuleDecision = match serde_json::from_value(emit.payload.clone()) {
                Ok(parsed) => parsed,
                Err(err) => {
                    return self.module_output_invalid(
                        output,
                        format!("rule decision decode failed: {err}"),
                    );
                }
            };
            if parsed.action_id != action_id {
                return self.module_output_invalid(
                    output,
                    format!(
                        "rule decision action_id mismatch expected {action_id} got {}",
                        parsed.action_id
                    ),
                );
            }
            decision = Some(parsed);
        }
        Ok(decision)
    }

    fn module_output_invalid(
        &mut self,
        output: &CapturedOutput,
        detail: String,
    ) -> Result<Option<RuleDecision>, WorldError> {
        let failure = ModuleCallFailure {
            module_id: output.module_id.clone(),
            trace_id: output.trace_id.clone(),
            code: ModuleCallErrorCode::InvalidOutput,
            detail,
        };
        self.append_event(WorldEventBody::ModuleCallFailed(failure.clone()), None)?;
        Err(WorldError::ModuleCallFailed {
            module_id: failure.module_id,
            trace_id: failure.trace_id,
            code: failure.code,
            detail: failure.detail,
        })
    }
}

fn format_rule_merge_error(err: &RuleDecisionMergeError) -> String {
    match err {
        RuleDecisionMergeError::ActionIdMismatch { expected, got } => {
            format!("rule decision action_id mismatch expected {expected} got {got}")
        }
        RuleDecisionMergeError::MissingOverride { action_id } => {
            format!("rule decision missing override for action {action_id}")
        }
        RuleDecisionMergeError::ConflictingOverride { action_id } => {
            format!("rule decision conflicting override for action {action_id}")
        }
        RuleDecisionMergeError::CostOverflow {
            action_id,
            kind,
            current,
            delta,
        } => format!(
            "rule decision cost overflow for action {action_id}: kind={kind:?} current={current} delta={delta}"
        ),
    }
}

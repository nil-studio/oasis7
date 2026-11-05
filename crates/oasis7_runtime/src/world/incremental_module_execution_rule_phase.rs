//! Rule phases execute the actual ordered invocation directory and sealed VM.
use super::*;
use crate::world::incremental_module_invocation::{
    AcceptedInvocationDirectoryV1, CurrentInvocationEnumerationWorkV1,
    collect_current_module_invocation_ids_v1,
};
use oasis7_world_state::incremental_rule_decision_decode::RuleDecisionPayloadDecodeWorkV1;
use oasis7_world_state::incremental_rule_decision_json_decode::{
    RuleDecisionJsonDecodeErrorV1, decode_rule_decision_json_payload_owned_v1,
};
pub struct RootRulePhaseWorkV1 {
    pub enumeration: CurrentInvocationEnumerationWorkV1,
    pub payload: RuleDecisionPayloadDecodeWorkV1,
    pub canonical: oasis7_wasm_abi::CanonicalCborWorkV1,
    pub max_captured_outputs: usize,
    pub max_payload_bytes: usize,
    pub merge_units_remaining: u64,
}
pub struct RootRulePhaseOutcomeV1 {
    pub decision: crate::RuleDecision,
    pub movement_rejection_cost: Option<crate::ResourceDelta>,
    _fees: Vec<MemoryReservation>,
    identity: oasis7_formal_protocol::incremental_execution::PreparedGuard,
    action_id: u64,
}
impl RootRulePhaseOutcomeV1 {
    pub(in crate::world) fn override_action_for_selected_v1<M: MerkleNodeReader>(
        &self,
        selected: &crate::world::incremental_action_execution::SelectedPendingActionV1,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
    ) -> Result<Option<&crate::Action>, BodyMutationError> {
        selected.validate(tx)?;
        tx.validate_transaction_identity(&self.identity)?;
        if self.action_id != selected.envelope().id {
            return Err(BodyMutationError::Invalid("rule outcome selected action"));
        }
        Ok((self.decision.verdict == crate::RuleVerdict::Modify)
            .then_some(self.decision.override_action.as_ref())
            .flatten())
    }

    fn post_action_envelope<'a, M: MerkleNodeReader>(
        &'a self,
        selected: &'a crate::world::incremental_action_execution::SelectedPendingActionV1,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
    ) -> Result<PreActionEnvelopeV1<'a>, BodyMutationError> {
        let override_action = self.override_action_for_selected_v1(selected, tx)?;
        Ok(PreActionEnvelopeV1 {
            original: selected.envelope(),
            action: override_action.unwrap_or(&selected.envelope().action),
            // Final overrides clear these fields in World::step. The Pricing
            // view used during evaluation deliberately retains them instead.
            retain_metadata: override_action.is_none(),
        })
    }
}
fn admit_merge_work(
    rule: &mut RootRulePhaseWorkV1,
    fees: &[MemoryReservation],
    count: usize,
) -> Result<(), ValueTreeError> {
    let units = fees
        .iter()
        .try_fold(0u64, |n, fee| {
            n.checked_add(fee.bytes() as u64)
                .ok_or(ValueTreeError::ResourceLimited("rule merge work"))
        })?
        .checked_mul(
            (count as u64)
                .checked_add(1)
                .ok_or(ValueTreeError::ResourceLimited("rule merge work"))?,
        )
        .ok_or(ValueTreeError::ResourceLimited("rule merge work"))?;
    rule.merge_units_remaining = rule
        .merge_units_remaining
        .checked_sub(units)
        .ok_or(ValueTreeError::ResourceLimited("rule merge work"))?;
    Ok(())
}
fn merge_fee(decisions: &[crate::RuleDecision]) -> Result<usize, ValueTreeError> {
    let notes = decisions.iter().try_fold(0usize, |n, d| {
        n.checked_add(d.notes.len())
            .ok_or(ValueTreeError::ResourceLimited("rule merge notes"))
    })?;
    // Owned Strings move unchanged. Vec::extend geometric capacity <= 2*sum
    // (at least four slots for the first allocation). ResourceKind has two keys;
    // 4096 covers the two BTree nodes and bounded original merge-error text.
    notes
        .max(4)
        .checked_mul(2 * std::mem::size_of::<String>())
        .and_then(|n| n.checked_add(4096))
        .ok_or(ValueTreeError::ResourceLimited("rule merge workspace"))
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader
        + ValueNodeWriter
        + oasis7_formal_protocol::incremental_state::StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    /// Route the immutable decision produced by this transaction's real Rule
    /// phase. A final Modify uses its derived action and clears envelope-only
    /// authorization metadata exactly as the original step does.
    pub(in crate::world) fn route_post_action_after_rules_v1(
        &mut self,
        selected: &crate::world::incremental_action_execution::SelectedPendingActionV1,
        outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
        result_event: Option<
            &crate::world::incremental_module_execution_event::PersistedWorldEventV1<'_>,
        >,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<usize, RootModuleExecutionErrorV1<O::Error>> {
        let result = (|| {
            if !outcome.uses_budget(self.ctx.values.memory) {
                return Err(ValueTreeError::Invalid("rule outcome pool").into());
            }
            let envelope = outcome
                .post_action_envelope(selected, self.ctx.transaction)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            if let Some(event) = result_event {
                event.validate(self.ctx.transaction)?;
                if event.event().caused_by != Some(crate::CausedBy::Action(selected.envelope().id))
                {
                    return Err(ValueTreeError::Invalid("post-action result action binding").into());
                }
            }
            self.route_completed_invocations(
                Some(envelope),
                result_event,
                None,
                directory,
                enumeration,
                live_owner,
            )
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }

    pub(in crate::world) fn execute_pre_action_rule_phases_v1(
        &mut self,
        selected: &crate::world::incremental_action_execution::SelectedPendingActionV1,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        rule: &mut RootRulePhaseWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<ReservedValue<RootRulePhaseOutcomeV1>, RootModuleExecutionErrorV1<O::Error>> {
        let result = (|| {
            selected.validate(self.ctx.transaction)?;
            let capacity = rule
                .max_captured_outputs
                .checked_mul(2)
                .and_then(|n| n.checked_add(16))
                .ok_or(ValueTreeError::ResourceLimited("rule fee owners"))?;
            let bytes = capacity
                .checked_mul(std::mem::size_of::<MemoryReservation>())
                .and_then(|n| n.checked_add(std::mem::size_of::<RootRulePhaseOutcomeV1>()))
                .ok_or(ValueTreeError::ResourceLimited("rule fee owners"))?;
            let fee = self
                .ctx
                .values
                .memory
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let mut fees = Vec::new();
            fees.try_reserve_exact(capacity)
                .map_err(|_| ValueTreeError::ResourceLimited("rule fee allocation"))?;
            if fees.capacity() > capacity {
                return Err(ValueTreeError::ResourceLimited("rule fee capacity").into());
            }
            let envelope = selected.envelope();
            let (decision, rejection) =
                if matches!(envelope.action, crate::Action::MoveAgent { .. }) {
                    let general = self.rule_phase(
                        PreActionEnvelopeV1 {
                            original: envelope,
                            action: &envelope.action,
                            retain_metadata: true,
                        },
                        crate::world::module_routing_runtime::MovementRulePhase::General,
                        directory,
                        rule,
                        live_owner,
                        &mut fees,
                    )?;
                    let priced = PreActionEnvelopeV1 {
                        original: envelope,
                        retain_metadata: true,
                        action: if general.verdict == crate::RuleVerdict::Modify {
                            general.override_action.as_ref().unwrap_or(&envelope.action)
                        } else {
                            &envelope.action
                        },
                    };
                    fees.push(
                        self.ctx
                            .values
                            .memory
                            .reserve(4096, MemoryClass::Ordinary)
                            .map_err(ValueTreeError::Memory)?,
                    );
                    let rejection = general.cost.clone();
                    let pricing = self.rule_phase(
                        priced,
                        crate::world::module_routing_runtime::MovementRulePhase::Pricing,
                        directory,
                        rule,
                        live_owner,
                        &mut fees,
                    )?;
                    fees.push(
                        self.ctx
                            .values
                            .memory
                            .reserve(
                                2 * std::mem::size_of::<crate::RuleDecision>(),
                                MemoryClass::Ordinary,
                            )
                            .map_err(ValueTreeError::Memory)?,
                    );
                    let both = vec![general, pricing];
                    admit_merge_work(rule, &fees, both.len())?;
                    fees.push(
                        self.ctx
                            .values
                            .memory
                            .reserve(
                                merge_fee(&both)? + 2 * std::mem::size_of::<crate::RuleDecision>(),
                                MemoryClass::Ordinary,
                            )
                            .map_err(ValueTreeError::Memory)?,
                    );
                    (
                        crate::world::rules::merge_action_rule_decisions_v1(envelope.id, both),
                        Some(rejection),
                    )
                } else {
                    (
                        self.rule_phase(
                            PreActionEnvelopeV1 {
                                original: envelope,
                                action: &envelope.action,
                                retain_metadata: true,
                            },
                            crate::world::module_routing_runtime::MovementRulePhase::All,
                            directory,
                            rule,
                            live_owner,
                            &mut fees,
                        )?,
                        None,
                    )
                };
            if (self.ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let live = live_owner();
            self.ctx
                .transaction
                .validate_guard(&self.ctx.transaction.guard(), &live)?;
            Ok(fee.retain(RootRulePhaseOutcomeV1 {
                decision,
                movement_rejection_cost: rejection,
                _fees: fees,
                identity: self.ctx.transaction.guard(),
                action_id: envelope.id,
            }))
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }
    fn rule_phase(
        &mut self,
        envelope: PreActionEnvelopeV1<'_>,
        phase: crate::world::module_routing_runtime::MovementRulePhase,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        rule: &mut RootRulePhaseWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
        fees: &mut Vec<MemoryReservation>,
    ) -> Result<crate::RuleDecision, RootModuleExecutionErrorV1<O::Error>> {
        let _diagnostic_phase = self
            .ctx
            .values
            .memory
            .reserve(8192, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let live = live_owner();
        let guard = self.ctx.transaction.guard();
        let ids = collect_current_module_invocation_ids_v1(
            directory,
            self.families.instances,
            self.families.config,
            self.state,
            self.ctx,
            &mut rule.enumeration,
            &guard,
            &live,
        )
        .map_err(|_| ValueTreeError::Invalid("rule invocation directory"))?;
        let capture = RefCell::new(rule_capture::RuleOutputCaptureV1::new(
            self.ctx.values.memory,
            rule.max_captured_outputs,
        )?);
        for id in ids.iter() {
            let live = live_owner();
            let Some(selected) = self.select(id, &live)? else {
                continue;
            };
            let pricing = matches!(
                selected.module_id(),
                "m1.rule.move" | "m1.mobility.basic" | "m1.power.storage"
            );
            let applicable = match phase {
                crate::world::module_routing_runtime::MovementRulePhase::All => true,
                crate::world::module_routing_runtime::MovementRulePhase::General => !pricing,
                crate::world::module_routing_runtime::MovementRulePhase::Pricing => pricing,
            };
            drop(selected);
            if applicable {
                self.execute_invocation_inner(id, Some(envelope), Some(&capture), live_owner)?;
            }
        }
        let memory = self.ctx.values.memory;
        let metadata = rule
            .max_captured_outputs
            .checked_mul(std::mem::size_of::<crate::RuleDecision>())
            .ok_or(ValueTreeError::ResourceLimited("rule decisions"))?;
        fees.push(
            memory
                .reserve(metadata, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?,
        );
        let mut decisions = Vec::new();
        decisions
            .try_reserve_exact(rule.max_captured_outputs)
            .map_err(|_| ValueTreeError::ResourceLimited("rule decision allocation"))?;
        if decisions.capacity() > rule.max_captured_outputs {
            return Err(ValueTreeError::ResourceLimited("rule decision capacity").into());
        }
        let captured = capture.borrow();
        for output in captured.outputs()? {
            let mut found = None;
            for emit in &output.output.emits {
                if (self.ctx.cancelled)() {
                    return Err(ValueTreeError::Cancelled.into());
                }
                if emit.kind != "rule.decision" {
                    continue;
                }
                if found.is_some() {
                    return self.rule_invalid_output(
                        &output.module_id,
                        &output.trace_id,
                        "multiple rule decisions emitted for stage PreAction".into(),
                    );
                }
                let payload = &emit.payload;
                let decoded = match decode_rule_decision_json_payload_owned_v1(
                    payload,
                    memory,
                    MemoryClass::Ordinary,
                    &mut rule.payload,
                    &mut rule.canonical,
                    &mut self.ctx.cancelled,
                ) {
                    Ok(value) => value,
                    Err(RuleDecisionJsonDecodeErrorV1::Host(error)) => return Err(error.into()),
                    Err(RuleDecisionJsonDecodeErrorV1::Payload(error)) => {
                        return self.rule_payload_invalid_output(
                            &output.module_id,
                            &output.trace_id,
                            &error,
                            rule,
                        );
                    }
                };
                let (decision, fee) = decoded.into_parts();
                if decision.action_id != envelope.original.id {
                    return self.rule_invalid_output(
                        &output.module_id,
                        &output.trace_id,
                        format!(
                            "rule decision action_id mismatch expected {} got {}",
                            envelope.original.id, decision.action_id
                        ),
                    );
                }
                found = Some((payload, decision, fee));
            }
            let Some((payload, decision, fee)) = found else {
                continue;
            };
            // Decode independently for the audit-owned DTO: no uncharged deep clone.
            let audit = match decode_rule_decision_json_payload_owned_v1(
                payload,
                memory,
                MemoryClass::Ordinary,
                &mut rule.payload,
                &mut rule.canonical,
                &mut self.ctx.cancelled,
            ) {
                Ok(value) => value,
                Err(RuleDecisionJsonDecodeErrorV1::Host(error)) => return Err(error.into()),
                Err(RuleDecisionJsonDecodeErrorV1::Payload(error)) => {
                    return self.rule_payload_invalid_output(
                        &output.module_id,
                        &output.trace_id,
                        &error,
                        rule,
                    );
                }
            };
            let (audit, audit_fee) = audit.into_parts();
            let identity_fee = memory
                .reserve(output.module_id.len(), MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let record = crate::RuleDecisionRecord {
                action_id: audit.action_id,
                module_id: output.module_id.clone(),
                stage: crate::ModuleSubscriptionStage::PreAction,
                verdict: audit.verdict,
                override_action: audit.override_action,
                cost: audit.cost,
                notes: audit.notes,
            };
            crate::world::incremental_module_execution_event::append_rule_decision_audit_v1(
                self.now,
                record,
                self.ctx.transaction,
                self.events,
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            drop(identity_fee);
            drop(audit_fee);
            fees.push(fee);
            decisions.push(decision);
        }
        fees.push(
            memory
                .reserve(merge_fee(&decisions)?, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?,
        );
        admit_merge_work(rule, fees, decisions.len())?;
        if (self.ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(crate::world::rules::merge_action_rule_decisions_v1(
            envelope.original.id,
            decisions,
        ))
    }
    fn rule_payload_invalid_output<T>(
        &mut self,
        module: &str,
        trace: &str,
        error: &impl std::fmt::Display,
        rule: &mut RootRulePhaseWorkV1,
    ) -> Result<T, RootModuleExecutionErrorV1<O::Error>> {
        struct Count<'a, C> {
            n: usize,
            stopped: bool,
            work: &'a mut u64,
            cancel: &'a mut C,
        }
        impl<C: FnMut() -> bool> std::fmt::Write for Count<'_, C> {
            fn write_str(&mut self, text: &str) -> std::fmt::Result {
                if (self.cancel)() {
                    self.stopped = true;
                    return Err(std::fmt::Error);
                }
                *self.work = self
                    .work
                    .checked_sub(text.len() as u64)
                    .ok_or(std::fmt::Error)?;
                self.n = self.n.checked_add(text.len()).ok_or(std::fmt::Error)?;
                Ok(())
            }
        }
        let mut count = Count {
            n: 0,
            stopped: false,
            work: &mut rule.canonical.bytes_remaining,
            cancel: &mut self.ctx.cancelled,
        };
        std::fmt::write(
            &mut count,
            format_args!("rule decision decode failed: {error}"),
        )
        .map_err(|_| {
            if count.stopped {
                ValueTreeError::Cancelled
            } else {
                ValueTreeError::ResourceLimited("rule error formatting work")
            }
        })?;
        *count.work =
            count
                .work
                .checked_sub(count.n as u64)
                .ok_or(ValueTreeError::ResourceLimited(
                    "rule error formatting work",
                ))?;
        let size = count
            .n
            .checked_mul(2)
            .and_then(|n| n.checked_add(8192))
            .ok_or(ValueTreeError::ResourceLimited("rule error formatting"))?;
        let _format = self
            .ctx
            .values
            .memory
            .reserve(size, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        self.rule_invalid_output(
            module,
            trace,
            format!("rule decision decode failed: {error}"),
        )
    }
    fn rule_invalid_output<T>(
        &mut self,
        module: &str,
        trace: &str,
        detail: String,
    ) -> Result<T, RootModuleExecutionErrorV1<O::Error>> {
        let fee = self
            .ctx
            .values
            .memory
            .reserve(
                module.len() + trace.len() + detail.len() + 8192,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let failure = ModuleCallFailure {
            module_id: module.into(),
            trace_id: trace.into(),
            code: ModuleCallErrorCode::InvalidOutput,
            detail,
        };
        crate::world::incremental_module_execution_event::append_module_output_event_v1(
            self.now,
            crate::WorldEventBody::ModuleCallFailed(failure.clone()),
            self.ctx.transaction,
            self.events,
        )
        .map_err(RootModuleExecutionErrorV1::Body)?;
        Err(RootModuleExecutionErrorV1::Business(fee.retain(
            crate::WorldError::ModuleCallFailed {
                module_id: failure.module_id,
                trace_id: failure.trace_id,
                code: failure.code,
                detail: failure.detail,
            },
        )))
    }
}

use super::super::{
    ActionId, DomainEvent, MaterialStack, ProductValidationAttemptV1, ProductValidationDecision,
    ProductValidationDeliveryCursor, ProductValidationReceiptV1, WorldError, WorldEvent,
    WorldEventBody, WorldEventId,
};
use super::World;
use oasis7_wasm_abi::ModuleSandbox;

impl World {
    pub(super) fn cached_product_validation_decision(
        &self,
        job_id: ActionId,
        validation_index: Option<u32>,
        requester_agent_id: &str,
        module_id: &str,
        stack: &MaterialStack,
    ) -> Result<Option<ProductValidationDecision>, WorldError> {
        let Some(receipts) = self.state.product_validation_receipts.get(&job_id) else {
            return Ok(None);
        };
        let Some(receipt) = receipts
            .iter()
            .find(|receipt| receipt.validation_index == validation_index)
        else {
            return Ok(None);
        };
        if receipt.requester_agent_id != requester_agent_id
            || receipt.module_id != module_id
            || receipt.stack != *stack
        {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!(
                    "product validation retry conflicts with persisted receipt: job_id={job_id} index={validation_index:?}"
                ),
            });
        }
        Ok(Some(receipt.decision.clone()))
    }

    pub(super) fn product_validation_receipt_for_output(
        &self,
        job_id: ActionId,
        validation_index: Option<u32>,
        requester_agent_id: &str,
        stack: &MaterialStack,
    ) -> Result<Option<ProductValidationReceiptV1>, WorldError> {
        let Some(receipt) = self
            .state
            .product_validation_receipts
            .get(&job_id)
            .and_then(|receipts| {
                receipts
                    .iter()
                    .find(|receipt| receipt.validation_index == validation_index)
            })
            .cloned()
        else {
            return Ok(None);
        };
        if receipt.requester_agent_id != requester_agent_id || receipt.stack != *stack {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!(
                    "product validation receipt conflicts with job: job_id={job_id} index={validation_index:?}"
                ),
            });
        }
        Ok(Some(receipt))
    }

    pub(super) fn append_missing_product_validation_blocker(
        &mut self,
        job_id: ActionId,
        requester_agent_id: &str,
        factory_id: &str,
        recipe_id: &str,
        stack: &MaterialStack,
        emitted: &mut Vec<WorldEvent>,
    ) -> Result<(), WorldError> {
        if self
            .state
            .factory_production_failure_dispositions
            .contains_key(&job_id)
        {
            return Ok(());
        }
        self.append_event(
            WorldEventBody::Domain(DomainEvent::FactoryProductionBlocked {
                action_id: job_id,
                requester_agent_id: requester_agent_id.to_string(),
                factory_id: factory_id.to_string(),
                recipe_id: recipe_id.to_string(),
                blocker_kind: "product_validation".to_string(),
                blocker_detail: format!(
                    "product validation rejected for {} before production settlement",
                    stack.kind
                ),
            }),
            None,
        )?;
        if let Some(event) = self.journal.events.last() {
            emitted.push(event.clone());
        }
        Ok(())
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Validation resume inputs mirror the persisted product validation transaction identity."
    )]
    pub(super) fn resume_product_validation_receipt(
        &mut self,
        job_id: ActionId,
        validation_index: Option<u32>,
        requester_agent_id: &str,
        factory_id: &str,
        recipe_id: &str,
        stack: &MaterialStack,
        sandbox: &mut dyn ModuleSandbox,
        emitted: &mut Vec<WorldEvent>,
    ) -> Result<Option<bool>, WorldError> {
        let Some(receipt) = self.product_validation_receipt_for_output(
            job_id,
            validation_index,
            requester_agent_id,
            stack,
        )?
        else {
            return Ok(None);
        };
        self.product_validation_attempt_for_output(
            job_id,
            validation_index,
            requester_agent_id,
            stack,
            Some(receipt.module_id.as_str()),
        )?;
        if Self::product_validation_decision_matches_stack(&receipt.decision, stack) {
            self.ensure_product_validation_delivery(&receipt, sandbox)?;
            return Ok(Some(false));
        }
        self.append_missing_product_validation_blocker(
            job_id,
            requester_agent_id,
            factory_id,
            recipe_id,
            stack,
            emitted,
        )?;
        Ok(Some(true))
    }

    fn ensure_product_validation_delivery(
        &mut self,
        receipt: &ProductValidationReceiptV1,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<(), WorldError> {
        let (existing_events, has_delivery) = self.product_validation_recovery_events(receipt);
        if has_delivery || !existing_events.is_empty() {
            for (event, event_id_era) in &existing_events {
                // A journaled event may have been persisted by the next
                // output's pre-call checkpoint before the event loop routed it
                // to subscribers. Route that exact event without appending
                // another domain event or invoking the validator again.
                self.route_product_validation_event_at(event, *event_id_era, sandbox)?;
            }
            if has_delivery {
                return Ok(());
            }
        }
        let event_id = self.append_event(
            WorldEventBody::Domain(DomainEvent::ProductValidated {
                requester_agent_id: receipt.requester_agent_id.clone(),
                module_id: receipt.module_id.clone(),
                stack: receipt.stack.clone(),
                stack_limit: receipt.decision.stack_limit,
                tradable: receipt.decision.tradable,
                quality_levels: receipt.decision.quality_levels.clone(),
                notes: receipt.decision.notes.clone(),
            }),
            None,
        )?;
        let event_id_era = self.product_validation_event_id_era_after_append(event_id);
        if let Some(event) = self.journal.events.last() {
            let event = event.clone();
            self.route_product_validation_event_at(&event, event_id_era, sandbox)?;
        }
        Ok(())
    }

    fn product_validation_recovery_events(
        &self,
        receipt: &ProductValidationReceiptV1,
    ) -> (Vec<(WorldEvent, u64)>, bool) {
        let Some(receipt_position) = self.journal.events.iter().rposition(|event| {
            matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::ProductValidationRecorded {
                    receipt: recorded,
                }) if recorded == receipt
            )
        }) else {
            return (Vec::new(), false);
        };
        let recorded_event = &self.journal.events[receipt_position];
        let mut events = Vec::new();
        let recorded_event_id_era = self.product_validation_event_id_era_at(receipt_position);
        if !self.product_validation_event_was_routed(recorded_event, recorded_event_id_era) {
            events.push((recorded_event.clone(), recorded_event_id_era));
        }
        let delivery_event = self
            .journal
            .events
            .iter()
            .enumerate()
            .skip(receipt_position.saturating_add(1))
            .take_while(|(_, event)| {
                !matches!(
                    &event.body,
                    WorldEventBody::Domain(DomainEvent::ProductValidationRecorded {
                        receipt: next,
                    }) if next.job_id == receipt.job_id
                        && next.validation_index == receipt.validation_index
                )
            })
            .find(|(_, event)| Self::product_validation_delivery_matches(event, receipt));
        let has_delivery = delivery_event.is_some();
        if let Some((event_index, event)) = delivery_event
            && !self.product_validation_event_was_routed(
                event,
                self.product_validation_event_id_era_at(event_index),
            )
        {
            events.push((
                event.clone(),
                self.product_validation_event_id_era_at(event_index),
            ));
        }
        (events, has_delivery)
    }

    fn product_validation_event_was_routed(&self, event: &WorldEvent, event_id_era: u64) -> bool {
        if self
            .state
            .product_validation_delivery_cursor
            .has_routed(event_id_era, event.id)
        {
            return true;
        }
        let trace_prefix = product_validation_event_trace_prefix(event_id_era, event.id);
        self.journal.events.iter().any(|journal_event| {
            matches!(
                &journal_event.body,
                WorldEventBody::ModuleRuntimeCharged(charge)
                    if charge.trace_id.starts_with(trace_prefix.as_str())
            )
        })
    }

    pub(super) fn route_product_validation_event(
        &mut self,
        event: &WorldEvent,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<(), WorldError> {
        let event_id_era = self.product_validation_event_id_era_without_position(event.id);
        self.route_product_validation_event_at(event, event_id_era, sandbox)
    }

    pub(super) fn route_product_validation_event_at(
        &mut self,
        event: &WorldEvent,
        event_id_era: u64,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<(), WorldError> {
        if Self::is_product_validation_event(event)
            && self.product_validation_event_was_routed(event, event_id_era)
        {
            return Ok(());
        }
        if !Self::is_product_validation_event(event) {
            self.route_event_to_modules(event, sandbox)?;
            return Ok(());
        }
        // The cursor advances after the routing attempt completes. A zero-
        // subscriber result is also terminal for this committed event: a
        // later module activation must not replay historical events.
        self.route_event_to_modules_with_event_era(event, Some(event_id_era), sandbox)?;
        self.append_event(
            WorldEventBody::ProductValidationDeliveryCursorUpdated(
                ProductValidationDeliveryCursor {
                    routed_through_event_id: event.id,
                    event_id_era,
                },
            ),
            None,
        )?;
        Ok(())
    }

    fn product_validation_event_id_era_without_position(&self, event_id: WorldEventId) -> u64 {
        if self.next_event_id == 1 && event_id == u64::MAX {
            self.next_event_id_era.saturating_sub(1)
        } else {
            self.next_event_id_era
        }
    }

    pub(super) fn product_validation_event_id_era_after_append(
        &self,
        event_id: WorldEventId,
    ) -> u64 {
        self.product_validation_event_id_era_without_position(event_id)
    }

    fn product_validation_event_id_era_at(&self, event_index: usize) -> u64 {
        let Some(last_event) = self.journal.events.last() else {
            return self.next_event_id_era;
        };
        let mut era = if self.next_event_id == 1 && last_event.id == u64::MAX {
            self.next_event_id_era.saturating_sub(1)
        } else {
            self.next_event_id_era
        };
        for pair in self.journal.events[event_index..].windows(2).rev() {
            if pair[0].id == u64::MAX && pair[1].id == 1 {
                era = era.saturating_sub(1);
            }
        }
        era
    }

    fn is_product_validation_event(event: &WorldEvent) -> bool {
        matches!(
            event.body,
            WorldEventBody::Domain(
                DomainEvent::ProductValidationAttemptStarted { .. }
                    | DomainEvent::ProductValidationRecorded { .. }
                    | DomainEvent::ProductValidated { .. }
            )
        )
    }

    fn product_validation_delivery_matches(
        event: &WorldEvent,
        receipt: &ProductValidationReceiptV1,
    ) -> bool {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ProductValidated {
                requester_agent_id,
                module_id,
                stack,
                stack_limit,
                tradable,
                quality_levels,
                notes,
            }) if requester_agent_id == &receipt.requester_agent_id
                && module_id == &receipt.module_id
                && stack == &receipt.stack
                && *stack_limit == receipt.decision.stack_limit
                && *tradable == receipt.decision.tradable
                && quality_levels == &receipt.decision.quality_levels
                && notes == &receipt.decision.notes
        )
    }

    pub(super) fn product_validation_decision_matches_stack(
        decision: &ProductValidationDecision,
        stack: &MaterialStack,
    ) -> bool {
        decision.accepted
            && decision.product_id == stack.kind
            && stack.amount > 0
            && stack.amount <= decision.stack_limit as i64
    }

    pub(super) fn product_validation_attempt_for_output(
        &self,
        job_id: ActionId,
        validation_index: Option<u32>,
        requester_agent_id: &str,
        stack: &MaterialStack,
        receipt_module_id: Option<&str>,
    ) -> Result<Option<ProductValidationAttemptV1>, WorldError> {
        let attempt = self
            .state
            .product_validation_attempts
            .get(&job_id)
            .and_then(|attempts| {
                attempts
                    .iter()
                    .find(|attempt| attempt.validation_index == validation_index)
            })
            .cloned();
        if let Some(attempt) = &attempt
            && (attempt.requester_agent_id != requester_agent_id
                || receipt_module_id.is_some_and(|module_id| attempt.module_id != module_id)
                || attempt.stack != *stack)
        {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!(
                    "product validation attempt conflicts with job or receipt: job_id={job_id} index={validation_index:?}"
                ),
            });
        }
        Ok(attempt)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Validation attempt inputs mirror the persisted product validation event schema."
    )]
    pub(super) fn record_product_validation_attempt(
        &mut self,
        job_id: ActionId,
        validation_index: Option<u32>,
        requester_agent_id: &str,
        module_id: &str,
        stack: &MaterialStack,
        sandbox: &mut dyn ModuleSandbox,
        emitted: &mut Vec<WorldEvent>,
    ) -> Result<(), WorldError> {
        let attempt_event_id = self.append_event(
            WorldEventBody::Domain(DomainEvent::ProductValidationAttemptStarted {
                attempt: ProductValidationAttemptV1 {
                    job_id,
                    validation_index,
                    requester_agent_id: requester_agent_id.to_string(),
                    module_id: module_id.to_string(),
                    stack: stack.clone(),
                },
            }),
            None,
        )?;
        let attempt_event_id_era =
            self.product_validation_event_id_era_after_append(attempt_event_id);
        let pending = std::mem::take(emitted);
        for event in pending {
            self.route_product_validation_event(&event, sandbox)?;
        }
        if let Some(event) = self.journal.events.last().cloned() {
            self.route_product_validation_event_at(&event, attempt_event_id_era, sandbox)?;
        }
        Ok(())
    }

    pub(super) fn fail_closed_product_validation(
        module_id: &str,
        stack: &MaterialStack,
        reason: impl Into<String>,
    ) -> (ProductValidationDecision, String) {
        let detail = format!(
            "product validation module failed and output was rejected: module_id={module_id} reason={}",
            reason.into()
        );
        (
            ProductValidationDecision::rejected(
                stack.kind.clone(),
                0,
                false,
                Vec::new(),
                vec![detail.clone()],
            ),
            detail,
        )
    }
}

pub(super) fn product_validation_event_trace_id(
    event_id_era: u64,
    event_id: WorldEventId,
    instance_id: &str,
) -> String {
    if event_id_era == 0 {
        format!("event-{event_id}-{instance_id}")
    } else {
        format!("event-era-{event_id_era}-{event_id}-{instance_id}")
    }
}

fn product_validation_event_trace_prefix(event_id_era: u64, event_id: WorldEventId) -> String {
    if event_id_era == 0 {
        format!("event-{event_id}-")
    } else {
        format!("event-era-{event_id_era}-{event_id}-")
    }
}

#[cfg(test)]
mod tests {
    use super::{World, product_validation_event_trace_id};
    use crate::{
        DomainEvent, ModuleRuntimeChargeEvent, ProductValidationAttemptV1, WorldEvent,
        WorldEventBody,
    };
    use oasis7_agent_api::world_action::ResourceKind;

    #[test]
    fn old_era_charge_with_same_event_id_does_not_suppress_new_era_validation() {
        let mut world = World::new();
        world.journal.events.push(WorldEvent {
            id: 7,
            time: 0,
            caused_by: None,
            body: WorldEventBody::ModuleRuntimeCharged(ModuleRuntimeChargeEvent {
                module_id: "observer".to_string(),
                trace_id: product_validation_event_trace_id(0, 7, "observer"),
                payer_agent_id: "unused".to_string(),
                compute_fee_kind: ResourceKind::Data,
                compute_fee_amount: 0,
                electricity_fee_kind: ResourceKind::Electricity,
                electricity_fee_amount: 0,
                input_bytes: 0,
                output_bytes: 0,
                effect_count: 0,
                emit_count: 0,
            }),
        });
        let validation = WorldEvent {
            id: 7,
            time: 0,
            caused_by: None,
            body: WorldEventBody::Domain(DomainEvent::ProductValidationAttemptStarted {
                attempt: ProductValidationAttemptV1 {
                    job_id: 1,
                    validation_index: Some(0),
                    requester_agent_id: "builder".to_string(),
                    module_id: "validator".to_string(),
                    stack: super::super::super::MaterialStack {
                        kind: "steel_plate".to_string(),
                        amount: 1,
                    },
                },
            }),
        };

        assert_eq!(
            product_validation_event_trace_id(1, validation.id, "observer"),
            "event-era-1-7-observer"
        );
        assert!(!world.product_validation_event_was_routed(&validation, 1));
    }
}

//! Trusted product-result validation shared by external actions and due jobs.
use crate::world::World;
use crate::{ActionId, DomainEvent, RejectReason, WorldEventBody};
use oasis7_wasm_abi::{MaterialStack, ProductValidationDecision};

impl World {
    // This is a result converter, not an action admission entrypoint. External
    // actions reach it only after normal authorization; due jobs have already
    // bound the real module result/receipt to their committed job and output.
    pub(in crate::world) fn product_validation_result_to_event(
        &self,
        action_id: ActionId,
        requester_agent_id: &str,
        module_id: &str,
        stack: &MaterialStack,
        decision: &ProductValidationDecision,
    ) -> WorldEventBody {
        if !self.state.agents.contains_key(requester_agent_id) {
            return WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::AgentNotFound {
                    agent_id: requester_agent_id.to_string(),
                },
            });
        }
        if module_id.trim().is_empty() {
            return WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["product module_id cannot be empty".to_string()],
                },
            });
        }
        if !decision.accepted {
            return WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: if decision.notes.is_empty() {
                        vec!["product validation rejected".to_string()]
                    } else {
                        decision.notes.clone()
                    },
                },
            });
        }
        if decision.product_id != stack.kind {
            return WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "validated product mismatch expected={} got={}",
                        stack.kind, decision.product_id
                    )],
                },
            });
        }
        if stack.amount <= 0 {
            return WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["product stack amount must be > 0".to_string()],
                },
            });
        }
        if stack.amount > decision.stack_limit as i64 {
            return WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "product stack exceeds limit amount={} limit={}",
                        stack.amount, decision.stack_limit
                    )],
                },
            });
        }
        WorldEventBody::Domain(DomainEvent::ProductValidated {
            requester_agent_id: requester_agent_id.to_string(),
            module_id: module_id.to_string(),
            stack: stack.clone(),
            stack_limit: decision.stack_limit,
            tradable: decision.tradable,
            quality_levels: decision.quality_levels.clone(),
            notes: decision.notes.clone(),
        })
    }
}

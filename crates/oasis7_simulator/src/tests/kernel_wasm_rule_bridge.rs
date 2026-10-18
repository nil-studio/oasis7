use super::*;
use std::sync::{Arc, Mutex};

fn register_location_action(location_id: &str) -> Action {
    Action::RegisterLocation {
        location_id: location_id.to_string(),
        name: format!("name-{location_id}"),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    }
}

#[test]
fn kernel_wasm_pre_action_evaluator_can_allow_and_read_context() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(register_location_action("loc-ready"));
    kernel.step().expect("register seed location");

    let captured = Arc::new(Mutex::new(None::<KernelRuleModuleInput>));
    let captured_clone = Arc::clone(&captured);
    kernel.set_pre_action_wasm_rule_evaluator(move |input| {
        *captured_clone.lock().expect("lock captured") = Some(input.clone());
        Ok(KernelRuleModuleOutput::from_decision(
            KernelRuleDecision::allow(input.action_id),
        ))
    });

    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-wasm".to_string(),
        location_id: "loc-ready".to_string(),
    });
    let event = kernel.step().expect("agent registration should pass");

    match event.kind {
        WorldEventKind::AgentRegistered {
            agent_id,
            location_id,
            ..
        } => {
            assert_eq!(agent_id, "agent-wasm");
            assert_eq!(location_id, "loc-ready");
        }
        other => panic!("unexpected event kind: {other:?}"),
    }

    let input = captured
        .lock()
        .expect("lock captured")
        .clone()
        .expect("captured wasm input");
    assert_eq!(input.context.time, 1);
    assert_eq!(input.context.location_ids, vec!["loc-ready".to_string()]);
    assert!(input.context.agent_ids.is_empty());
}

#[test]
fn kernel_wasm_pre_action_evaluator_modify_overrides_action() {
    let mut kernel = WorldKernel::new();
    kernel.set_pre_action_wasm_rule_evaluator(|input| {
        Ok(KernelRuleModuleOutput::from_decision(
            KernelRuleDecision::modify(
                input.action_id,
                register_location_action("loc-wasm-override"),
            ),
        ))
    });

    kernel.submit_action(register_location_action("loc-original"));
    let event = kernel.step().expect("modified action emits event");

    match event.kind {
        WorldEventKind::LocationRegistered { location_id, .. } => {
            assert_eq!(location_id, "loc-wasm-override");
        }
        other => panic!("unexpected event kind: {other:?}"),
    }
    assert!(!kernel.model().locations.contains_key("loc-original"));
    assert!(kernel.model().locations.contains_key("loc-wasm-override"));
}

#[test]
fn kernel_wasm_pre_action_evaluator_deny_rejects_action() {
    let mut kernel = WorldKernel::new();
    kernel.set_pre_action_wasm_rule_evaluator(|input| {
        Ok(KernelRuleModuleOutput::from_decision(
            KernelRuleDecision::deny(
                input.action_id,
                vec!["blocked by wasm pre-action rule".to_string()],
            ),
        ))
    });

    kernel.submit_action(register_location_action("loc-denied"));
    let event = kernel.step().expect("deny emits reject event");

    match event.kind {
        WorldEventKind::ActionRejected { reason } => match reason {
            RejectReason::RuleDenied { notes } => {
                assert!(
                    notes
                        .iter()
                        .any(|note| note.contains("blocked by wasm pre-action rule"))
                );
            }
            other => panic!("unexpected reject reason: {other:?}"),
        },
        other => panic!("unexpected event kind: {other:?}"),
    }
    assert!(!kernel.model().locations.contains_key("loc-denied"));
}

#[test]
fn kernel_wasm_pre_action_evaluator_error_is_translated_to_rule_denied() {
    let mut kernel = WorldKernel::new();
    kernel.set_pre_action_wasm_rule_evaluator(|_| Err("sandbox timeout".to_string()));

    kernel.submit_action(register_location_action("loc-error"));
    let event = kernel.step().expect("error emits reject event");

    match event.kind {
        WorldEventKind::ActionRejected { reason } => match reason {
            RejectReason::RuleDenied { notes } => {
                assert!(
                    notes
                        .iter()
                        .any(|note| note.contains("wasm pre-action evaluator failed"))
                );
                assert!(notes.iter().any(|note| note.contains("sandbox timeout")));
            }
            other => panic!("unexpected reject reason: {other:?}"),
        },
        other => panic!("unexpected event kind: {other:?}"),
    }
}

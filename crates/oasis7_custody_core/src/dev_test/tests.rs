use super::*;
use serde_json::json;

fn scope() -> Scope {
    Scope {
        account_ref: "A".into(),
        environment: "test".into(),
        world: "world".into(),
        genesis_digest: "genesis".into(),
        agent: "agent-A".into(),
        resource: "A-write".into(),
        executor: "executor".into(),
        fence_digest: "fence".into(),
    }
}
fn ledger() -> Ledger {
    Ledger {
        version: 1,
        ..Ledger::default()
    }
}
fn context() -> Call {
    Call {
        request_id: "context-A".into(),
        arguments: Arguments::Context,
    }
}
fn grant() -> Grant {
    Grant {
        grant_ref: "A-write".into(),
        grant_id: "grant".into(),
        caller_id: "caller".into(),
        binding: Binding {
            account_ref: "A".into(),
            environment: "test".into(),
            world: "world".into(),
            genesis_digest: "genesis".into(),
            agent_id: "agent-A".into(),
            resource: "A-write".into(),
            executor_config_digest: "digest".into(),
            authorization_version: 1,
        },
        expires_at_unix: 100,
        enabled: true,
        methods: vec!["lookup".into()],
        move_targets: vec![],
    }
}
#[test]
fn fixed_native_operation_json_is_preserved_and_closed() {
    let wire = r#"{"request_id":"move-A","arguments":{"kind":"move","context_request_id":"context-A","to":[1,0,-1]}}"#;
    let call: Call = serde_json::from_str(wire).unwrap();
    assert_eq!(serde_json::to_string(&call).unwrap(), wire);
    assert_eq!(call.recovery_request_id(), "move-A");
    for bad in [
        r#"{"request_id":"R","arguments":{"kind":"move","context_request_id":"C","to":[1,0]}}"#,
        r#"{"request_id":"R","arguments":{"kind":"context","credential":"injected"}}"#,
        r#"{"request_id":"R","arguments":{"kind":"raw_sign"}}"#,
        r#"{"request_id":"R","arguments":{"kind":"context"},"url":"https://attacker.invalid"}"#,
    ] {
        assert!(serde_json::from_str::<Call>(bad).is_err());
    }
}
#[test]
fn exact_replay_spends_new_budget_but_conflict_never_mutates_original() {
    let mut state = ledger();
    let call = context();
    let (first, fresh) = state
        .admit("grant", "caller", &scope(), &call, 2, |_| Ok(None))
        .unwrap();
    assert!(fresh);
    assert_eq!(state.used, 1);
    let (same, fresh) = state
        .admit("grant", "caller", &scope(), &call, 2, |_| {
            panic!("no history")
        })
        .unwrap();
    assert!(!fresh);
    assert_eq!(first.native_context, same.native_context);
    assert_eq!(state.used, 2);
    let before = serde_json::to_value(&state).unwrap();
    let changed = Call {
        arguments: Arguments::Lookup {
            original_request_id: "missing".into(),
        },
        ..call.clone()
    };
    assert_eq!(
        state
            .admit("grant", "caller", &scope(), &changed, 2, |_| panic!(
                "no history"
            ))
            .unwrap_err(),
        "request_body_conflict"
    );
    assert_eq!(
        state
            .admit("grant", "caller", &scope(), &call, 2, |_| panic!(
                "no history"
            ))
            .unwrap_err(),
        "call_limit_exhausted"
    );
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
}
#[test]
fn denied_scope_or_unverified_context_cannot_reserve_a_move() {
    let mut state = ledger();
    state
        .admit("grant", "caller", &scope(), &context(), 9, |_| Ok(None))
        .unwrap();
    let call = Call {
        request_id: "move-A".into(),
        arguments: Arguments::Move {
            context_request_id: "context-A".into(),
            to: [1, 0, 0],
        },
    };
    let mut wrong = scope();
    wrong.agent = "agent-B".into();
    let before = serde_json::to_value(&state).unwrap();
    assert_eq!(
        state
            .admit("grant", "caller", &wrong, &call, 9, |_| Ok(None))
            .unwrap_err(),
        "original_scope_denied"
    );
    assert_eq!(
        state
            .admit("grant", "caller", &scope(), &call, 9, |_| Ok(None))
            .unwrap_err(),
        "original_context_unverified"
    );
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
}
#[test]
fn recovery_imports_only_lookup_association_and_keeps_new_budget_separate() {
    let mut old = ledger();
    let (original, _) = old
        .admit("old", "caller", &scope(), &context(), 1, |_| Ok(None))
        .unwrap();
    let old_bytes = serde_json::to_vec(&old).unwrap();
    let mut new = ledger();
    let lookup = Call {
        request_id: "query".into(),
        arguments: Arguments::Lookup {
            original_request_id: "context-A".into(),
        },
    };
    let (recovered, _) = new
        .admit("new", "caller", &scope(), &lookup, 1, |_| {
            Ok(Some(original.clone()))
        })
        .unwrap();
    assert!(recovered.historical_lookup_only);
    assert_eq!(recovered.native_context, original.native_context);
    assert_eq!(new.used, 1);
    assert_eq!(serde_json::to_vec(&old).unwrap(), old_bytes);
    assert!(!new.operations.contains_key("context-A"));
    let action = Call {
        request_id: "new-move".into(),
        arguments: Arguments::Move {
            context_request_id: "context-A".into(),
            to: [1, 0, 0],
        },
    };
    assert_eq!(
        new.admit("new", "caller", &scope(), &action, 9, |_| panic!(
            "Move cannot import history"
        ))
        .unwrap_err(),
        "original_context_unknown"
    );
}
#[test]
fn missing_history_is_unknown_and_does_not_charge_or_create_originals() {
    let mut state = ledger();
    let before = serde_json::to_value(&state).unwrap();
    let lookup = Call {
        request_id: "query".into(),
        arguments: Arguments::Lookup {
            original_request_id: "missing".into(),
        },
    };
    assert_eq!(
        state
            .admit("new", "caller", &scope(), &lookup, 9, |_| Ok(None))
            .unwrap_err(),
        "original_operation_unknown"
    );
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
}
#[test]
fn observation_retains_canonical_result_without_private_goal_text() {
    let mut state = ledger();
    let call = context();
    state
        .admit("grant", "caller", &scope(), &call, 9, |_| Ok(None))
        .unwrap();
    state
        .observe(
            &call,
            json!({"status":"committed","active_goal":{"content":"PRIVATE"}}),
        )
        .unwrap();
    assert_eq!(
        state.operations["context-A"].last_observation,
        Some(json!({"status":"committed"}))
    );
    assert!(!serde_json::to_string(&state).unwrap().contains("PRIVATE"));
}
#[test]
fn sessions_require_current_caller_exact_policy_and_strict_expiry() {
    let mut sessions = Sessions::with_epoch("ab".repeat(32)).unwrap();
    let mut current = grant();
    let reference = sessions.open("caller", &current, 50, 1).unwrap();
    assert!(
        sessions
            .authorize(&reference, "caller", &current, "lookup", 49)
            .is_ok()
    );
    assert!(
        sessions
            .authorize(&reference, "caller", &current, "lookup", 50)
            .is_err()
    );
    assert!(
        sessions
            .authorize(&reference, "other", &current, "lookup", 49)
            .is_err()
    );
    assert!(
        sessions
            .authorize(&reference, "caller", &current, "move", 49)
            .is_err()
    );
    current.binding.authorization_version += 1;
    assert!(
        sessions
            .authorize(&reference, "caller", &current, "lookup", 49)
            .is_err()
    );
    current.binding.authorization_version -= 1;
    sessions.close(&reference, "caller").unwrap();
    assert!(
        sessions
            .authorize(&reference, "caller", &current, "lookup", 49)
            .is_err()
    );
}
#[test]
fn fresh_run_epoch_invalidates_old_session_without_resetting_ledger() {
    let mut sessions = Sessions::with_epoch("ab".repeat(32)).unwrap();
    let reference = sessions.open("caller", &grant(), 50, 1).unwrap();
    let fresh = Sessions::with_epoch("cd".repeat(32)).unwrap();
    assert!(
        fresh
            .authorize(&reference, "caller", &grant(), "lookup", 2)
            .is_err()
    );
    assert!(Sessions::with_epoch("predictable".into()).is_err());
}

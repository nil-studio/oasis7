//! Actual activated/claimed V2, real RFC Action PoP and Native Source/LF.
use super::*;
#[path = "test_session_v2_execution_prepare_current_ports_tests.rs"]
mod current_ports_tests;
#[path = "test_session_v2_execution_prepare_lf_restore_tests.rs"]
mod lf_restore_tests;
#[path = "test_session_v2_execution_prepare_signer_tests.rs"]
mod signer_tests;
fn allocation() -> ExecutionPrepareAllocationV2 {
    ExecutionPrepareAllocationV2 {
        operation_id: "actual-book-prepare-op".into(),
        factory_ingress_id: "actual-prepare-factory-ingress".into(),
        client_nonce: "a1".repeat(32),
        observation_nonce: "b2".repeat(32),
        gateway_nonce: "c3".repeat(32),
    }
}
struct ReadyPrepare {
    f: SessionFixture,
    body: Vec<u8>,
    pop: oasis7_custody_core::http_pop::HttpPossessionReceipt,
    fact: SignedManagedNativeSessionLifetimeV1,
}
fn ready() -> ReadyPrepare {
    ready_with_status_window(None)
}
#[derive(Clone, Copy)]
enum ManagerWindow {
    Session,
    Idle,
}
fn ready_with_status_window(status_window_ms: Option<u64>) -> ReadyPrepare {
    ready_with_windows(status_window_ms, None)
}
fn ready_with_windows(
    status_window_ms: Option<u64>,
    manager_window: Option<ManagerWindow>,
) -> ReadyPrepare {
    ready_with_current_ticket(status_window_ms, manager_window, true)
}
fn ready_with_current_ticket(
    status_window_ms: Option<u64>,
    manager_window: Option<ManagerWindow>,
    reserve_current: bool,
) -> ReadyPrepare {
    ready_with_current_ticket_and_hook(status_window_ms, manager_window, reserve_current, |_| {})
}
fn ready_with_current_ticket_and_hook(
    status_window_ms: Option<u64>,
    manager_window: Option<ManagerWindow>,
    reserve_current: bool,
    before_source: impl FnOnce(&mut SessionFixture),
) -> ReadyPrepare {
    let mut f = claimed_execution();
    // Existing consumed record is left intact. A new real Native Source below
    // is strongly signed with these tighter current Manager natural versions.
    match manager_window {
        Some(ManagerWindow::Session) => f.context.source.session_expires_at = DecimalU64(14_500),
        Some(ManagerWindow::Idle) => f.context.source.session_idle_expires_at = DecimalU64(14_500),
        None => {}
    }
    let (body, pop) = action_pop(&mut f, "actual-prepare-factory-action", 13_000, false);
    let ingress = "actual-prepare-parent-ingress";
    charge(
        &mut f,
        &body,
        &pop,
        ingress,
        SessionPurposeV1::Execution,
        13_002,
    )
    .unwrap();
    before_source(&mut f);
    let source = "actual-prepare-factory-source";
    execution_source(&mut f, source, (&body, &pop, ingress), 14_000);
    let policy = Policy::new(f.state.draft());
    let c = policy.current(&f.context);
    let nonce = f
        .state
        .claim_test_session_v2_execution_lifetime(
            &f.session_id,
            source,
            SessionProofRequest {
                ingress_attempt_id: ingress,
                original_bytes: &body,
                possession: &pop,
            },
            &c,
            UnixMillis::new(14_001),
            &mut StrictCrypto,
        )
        .unwrap();
    let mut fact = lifetime(&f, nonce, 14_002);
    if let Some(window) = status_window_ms {
        fact.lifetime.status.expires_at = DecimalU64(14_002u64.checked_add(window).unwrap());
        fact.signature = URL_SAFE_NO_PAD.encode(
            key()
                .sign(&fact.lifetime.signing_bytes().unwrap())
                .to_bytes(),
        );
    }
    let ns = scope(&f.pin);
    f.state
        .archive_test_session_v2_execution_lifetime(
            &f.session_id,
            &fact,
            &ns,
            UnixMillis::new(14_003),
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(
        f.state
            .reserve_test_session_v2_source_read(&f.session_id, source, UnixMillis::new(14_003))
            .unwrap(),
        4
    );
    let current = TestSessionLiveCurrent {
        provision: &c,
        native_scope: &ns,
        lifetime: &fact,
        private_sid_generation: DecimalU64(1),
        source_ticket: 4,
        source_operation_id: source,
        observed_at: UnixMillis::new(14_000),
    };
    f.state
        .admit_test_session_v2_execution_lifetime(
            &f.session_id,
            &current,
            UnixMillis::new(14_004),
            &mut StrictCrypto,
        )
        .unwrap();
    if reserve_current {
        // This is only the shared eight-use once reservation, not a transport fee.
        // The actual driver must separately prepay every observation HTTP/RPC edge.
        assert_eq!(
            f.state
                .reserve_test_session_v2_source_read(&f.session_id, source, UnixMillis::new(14_005))
                .unwrap(),
            5
        );
    }
    ReadyPrepare { f, body, pop, fact }
}
fn prepay(
    f: &mut SessionFixture,
    body: &[u8],
    pop: &oasis7_custody_core::http_pop::HttpPossessionReceipt,
) -> String {
    let a = allocation();
    f.state
        .charge_test_session_v2_execution_prepare_attempt(
            &f.session_id,
            &a,
            SessionProofRequest {
                ingress_attempt_id: &a.factory_ingress_id,
                original_bytes: body,
                possession: pop,
            },
            UnixMillis::new(14_006),
        )
        .unwrap()
}
fn admit(
    f: &mut SessionFixture,
    fact: &SignedManagedNativeSessionLifetimeV1,
    paid: &str,
    ticket: u8,
    at: u64,
) -> core::result::Result<ExecutionPrepareSigningAuthorityV2, ProvisionApprovalError> {
    let policy = Policy::new(f.state.draft());
    let c = policy.current(&f.context);
    let ns = scope(&f.pin);
    let current = TestSessionLiveCurrent {
        provision: &c,
        native_scope: &ns,
        lifetime: fact,
        private_sid_generation: DecimalU64(1),
        source_ticket: ticket,
        source_operation_id: "actual-prepare-factory-source",
        observed_at: UnixMillis::new(14_000),
    };
    f.state.allocate_test_session_v2_execution_prepare(
        &f.session_id,
        paid,
        &current,
        UnixMillis::new(at),
        &mut StrictCrypto,
    )
}
#[test]
fn session_v2_prepare_factory_durable_fee_then_exact_sdk_allocation() {
    let ReadyPrepare {
        mut f,
        body,
        pop,
        fact,
    } = ready();
    let before = f.state.clone();
    let mut collision = allocation();
    collision.client_nonce = fact.lifetime.status.nonce.clone();
    let unchanged = f.state.clone();
    assert!(
        f.state
            .charge_test_session_v2_execution_prepare_attempt(
                &f.session_id,
                &collision,
                SessionProofRequest {
                    ingress_attempt_id: &collision.factory_ingress_id,
                    original_bytes: &body,
                    possession: &pop
                },
                UnixMillis::new(14_005)
            )
            .is_err()
    );
    assert_eq!(unchanged, f.state);
    let paid = prepay(&mut f, &body, &pop);
    assert!(paid.len() > 256);
    let pending = f.state.clone();
    let restored: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&pending).unwrap()).unwrap();
    assert_eq!(restored, pending);
    assert_eq!(prepay(&mut f, &body, &pop), paid);
    assert_eq!(f.state, pending);
    let old = before.test_session_v2_snapshot(&f.session_id).unwrap();
    let new = f.state.test_session_v2_snapshot(&f.session_id).unwrap();
    assert_eq!(
        old.roots()[&SessionPurposeV1::Execution]
            .budgets
            .transport
            .remaining()
            .unwrap()
            - 1,
        new.roots()[&SessionPurposeV1::Execution]
            .budgets
            .transport
            .remaining()
            .unwrap()
    );
    let old_json = serde_json::to_value(&before).unwrap();
    let pending_json = serde_json::to_value(&pending).unwrap();
    assert_eq!(
        old_json["test_sessions_v2"][&f.session_id]["identity_transport"],
        pending_json["test_sessions_v2"][&f.session_id]["identity_transport"]
    );
    assert!(admit(&mut f, &fact, &paid, 4, 14_006).is_err());
    assert_eq!(f.state, pending);
    let authority = admit(&mut f, &fact, &paid, 5, 14_007).unwrap();
    let bundle = authority.bundle();
    assert_eq!(authority.paid_key(), paid);
    assert_eq!(authority.admitted_at().get(), 14_007);
    assert_eq!(authority.source_observed_at().get(), 14_000);
    let ExecutionSigningActionV2::Prepare {
        original,
        observation_nonce,
    } = bundle.action.as_ref()
    else {
        panic!("fixed Prepare stage required")
    };
    assert_eq!(original.client_operation_id, allocation().operation_id);
    assert_eq!(original.client_nonce, allocation().client_nonce);
    assert_eq!(observation_nonce, &allocation().observation_nonce);
    assert_eq!(
        bundle.observation.observation_nonce.as_deref(),
        Some(allocation().gateway_nonce.as_str())
    );
    assert_eq!(
        original.fence,
        f.state
            .test_session_v2_header(&f.session_id)
            .unwrap()
            .1
            .task_fence
    );
    let committed = f.state.clone();
    assert!(admit(&mut f, &fact, &paid, 5, 14_008).is_err());
    assert_eq!(committed, f.state);
    let restored: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&committed).unwrap()).unwrap();
    assert_eq!(restored, committed);
}
#[test]
fn session_v2_prepare_factory_failed_current_preserves_paid_original_and_rejects_downgrade() {
    let ReadyPrepare {
        mut f,
        body,
        pop,
        fact,
    } = ready();
    let paid = prepay(&mut f, &body, &pop);
    let pending = f.state.clone();
    assert!(admit(&mut f, &fact, &paid, 5, 20_001).is_err());
    assert_eq!(pending, f.state);
    let mut raw = serde_json::to_value(&pending).unwrap();
    raw["test_sessions_v2"][&f.session_id]["purpose_transports"][&paid] =
        serde_json::json!("execution");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(raw).is_err());
    let mut changed = allocation();
    changed.client_nonce = "d4".repeat(32);
    let before = f.state.clone();
    assert!(
        f.state
            .charge_test_session_v2_execution_prepare_attempt(
                &f.session_id,
                &changed,
                SessionProofRequest {
                    ingress_attempt_id: &changed.factory_ingress_id,
                    original_bytes: &body,
                    possession: &pop
                },
                UnixMillis::new(14_006)
            )
            .is_err()
    );
    assert_eq!(before, f.state);
}

#[test]
fn session_v2_prepare_factory_opaque_never_outlives_actual_signed_lf_status_expiry() {
    let ReadyPrepare {
        mut f,
        body,
        pop,
        fact,
    } = ready_with_status_window(Some(1000));
    let ns = scope(&f.pin);
    fact.verify_root(ns.native_root_key_id, ns.native_root_public_key)
        .unwrap();
    assert_eq!(fact.lifetime.status.expires_at.0, 15_002);
    assert!(14_007 < fact.lifetime.status.expires_at.0);
    let deadline = f
        .state
        .test_session_v2_snapshot(&f.session_id)
        .unwrap()
        .roots()[&SessionPurposeV1::Execution]
        .deadline;
    // Natural managed-session expiry still covers the original execution root.
    // Only the strong status observation validity is narrower in this case.
    fact.lifetime.covers(deadline).unwrap();
    let paid = prepay(&mut f, &body, &pop);
    let authority = admit(&mut f, &fact, &paid, 5, 14_007).unwrap();
    assert!(
        authority.expires_at().get() <= fact.lifetime.status.expires_at.0,
        "opaque signing authority exceeds the actual signed LF status expiry"
    );
}

#[test]
fn session_v2_prepare_factory_opaque_obeys_tightened_current_handoff() {
    let ReadyPrepare {
        mut f,
        body,
        pop,
        fact,
    } = ready();
    let paid = prepay(&mut f, &body, &pop);
    let original_until = f.context.handoff_until;
    assert!(original_until.0 > 14_500);
    // check_recorded_context explicitly permits a narrower Current handoff.
    // All signed Source versions and LF/natural/root deadlines remain original.
    f.context.handoff_until = DecimalU64(14_500);
    assert!(14_007 < f.context.handoff_until.0);
    let authority = admit(&mut f, &fact, &paid, 5, 14_007).unwrap();
    assert!(
        authority.expires_at().get() <= f.context.handoff_until.0,
        "opaque signing authority exceeds tightened Current handoff"
    );
}

fn check_manager_window(window: ManagerWindow) {
    let ReadyPrepare {
        mut f,
        body,
        pop,
        fact,
    } = ready_with_windows(None, Some(window));
    assert!(f.context.handoff_until.0 > 14_500);
    let source = f
        .state
        .test_session_v2_source(&f.session_id, "actual-prepare-factory-source")
        .unwrap();
    let strong = &source.source.verification.as_ref().unwrap().verification;
    assert_eq!(
        strong.source_session_expires_at,
        f.context.source.session_expires_at
    );
    assert_eq!(
        strong.source_session_idle_expires_at,
        f.context.source.session_idle_expires_at
    );
    let until = match window {
        ManagerWindow::Session => strong.source_session_expires_at.0,
        ManagerWindow::Idle => strong.source_session_idle_expires_at.0,
    };
    assert_eq!(until, 14_500);
    let deadline = f
        .state
        .test_session_v2_snapshot(&f.session_id)
        .unwrap()
        .roots()[&SessionPurposeV1::Execution]
        .deadline;
    fact.lifetime.covers(deadline).unwrap();
    let paid = prepay(&mut f, &body, &pop);
    // All actual Source/LF/Current strong jobs and the complete factory must
    // succeed first; changing an unsigned field alone cannot reach this target.
    let authority = admit(&mut f, &fact, &paid, 5, 14_007).unwrap();
    assert!(
        authority.expires_at().get() <= until,
        "opaque authority exceeds actual signed Manager source natural expiry"
    );
}
#[test]
fn session_v2_prepare_factory_opaque_obeys_manager_session_expiry() {
    check_manager_window(ManagerWindow::Session);
}
#[test]
fn session_v2_prepare_factory_opaque_obeys_manager_idle_expiry() {
    check_manager_window(ManagerWindow::Idle);
}

use super::*;
use crate::preset::{Protocol, ProviderId, RequestLimits};
use std::sync::{
    Barrier,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
struct FakeClock(AtomicU64);
impl Clock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}
struct Fixture {
    manager: ConnectionManager,
    principal: Principal,
    clock: Arc<FakeClock>,
    live: Arc<AtomicBool>,
    binding: Arc<AtomicBool>,
}
fn preset() -> ProviderPreset {
    ProviderPreset {
        id: "deepseek-fixture".into(),
        provider: ProviderId::DeepSeek,
        protocol: Protocol::ChatCompletions,
        model: "fixture-model".into(),
        routes: vec![],
        revision: 1,
        limits: RequestLimits::default(),
    }
}
fn target() -> BindingTarget {
    BindingTarget {
        full_fence_digest: "fixture-full-fence".into(),
        world_id: "world".into(),
        agent_id: "agent".into(),
        task_id: "task".into(),
        goal_revision: 1,
        executor_id: "executor".into(),
        executor_epoch: 1,
    }
}
impl Fixture {
    fn new(limits: Limits) -> Self {
        let principal = Principal {
            account_id: "account".into(),
            session_id: "session".into(),
            auth_ref: "opaque-server-lookup".into(),
        };
        let expected = principal.clone();
        let live = Arc::new(AtomicBool::new(true));
        let active = live.clone();
        let binding = Arc::new(AtomicBool::new(true));
        let authorized = binding.clone();
        let clock = Arc::new(FakeClock(AtomicU64::new(1000)));
        let manager = ConnectionManager::new(
            Arc::new(move |p: &Principal| p == &expected && active.load(Ordering::SeqCst)),
            Arc::new(move |_: &Principal, _: &BindingTarget| authorized.load(Ordering::SeqCst)),
            clock.clone(),
            limits,
        )
        .unwrap();
        Self {
            manager,
            principal,
            clock,
            live,
            binding,
        }
    }
    fn create(&self) -> ConnectionSnapshot {
        self.manager
            .create(
                &self.principal,
                preset(),
                Zeroizing::new(b"canary-secret".to_vec()),
                true,
            )
            .unwrap()
    }
    fn begin(&self, c: &ConnectionSnapshot, op: &str) -> ProbeOperationPermit {
        match self
            .manager
            .begin_probe(&self.principal, &c.id, c.version, op)
            .unwrap()
        {
            ProbeStart::Operation(op) => op,
            _ => panic!("expected new probe"),
        }
    }
    fn ready(&self) -> ConnectionSnapshot {
        let c = self.create();
        let op = self.begin(&c, "ready");
        for phase in 0..2 {
            let permit = op.reserve_phase(phase, 10).unwrap();
            permit.before_send().unwrap();
            permit.finish(Some(10), true).unwrap();
        }
        self.manager.snapshot(&self.principal, &c.id).unwrap()
    }
    fn active(&self) -> ConnectionSnapshot {
        let c = self.ready();
        self.manager
            .activate(&self.principal, &c.id, c.version, target())
            .unwrap()
    }
}
fn limits() -> Limits {
    Limits {
        max_calls: 20,
        max_tokens: 2000,
        max_input_tokens: 100,
        max_output_tokens: 100,
        max_concurrent: 2,
        lease_ms: 100,
        absolute_ms: 300,
        renew_interval_ms: 20,
    }
}
#[test]
fn tenant_session_and_auth_reference_are_required_for_every_object_access() {
    let f = Fixture::new(limits());
    let c = f.ready();
    for field in 0..3 {
        let mut other = f.principal.clone();
        match field {
            0 => other.account_id.push('x'),
            1 => other.session_id.push('x'),
            _ => other.auth_ref.push('x'),
        }
        assert!(matches!(
            f.manager.snapshot(&other, &c.id),
            Err(ConnectionError::Unavailable)
        ));
        assert!(matches!(
            f.manager.begin_probe(&other, &c.id, c.version, "guess"),
            Err(ConnectionError::Unavailable)
        ));
        assert!(matches!(
            f.manager.stop(&other, &c.id, c.version),
            Err(ConnectionError::Unavailable)
        ));
        assert!(matches!(
            f.manager.forget(&other, &c.id, c.version),
            Err(ConnectionError::Unavailable)
        ));
        assert!(matches!(
            f.manager.renew(&other, &c.id, c.version),
            Err(ConnectionError::Unavailable)
        ));
        assert!(matches!(
            f.manager.activate(&other, &c.id, c.version, target()),
            Err(ConnectionError::Unavailable)
        ));
    }
    assert!(matches!(
        f.manager.snapshot(&f.principal, "missing"),
        Err(ConnectionError::Unavailable)
    ));
    let serialized = serde_json::to_string(&f.manager.list(&f.principal).unwrap()).unwrap();
    assert!(!serialized.contains("canary-secret"));
    assert!(!serialized.contains("opaque-server-lookup"));
    assert!(!format!("{:?}", f.principal).contains("opaque-server-lookup"));
    assert_eq!(
        f.manager.lock().unwrap().connections[&c.id]
            .key
            .as_ref()
            .unwrap()
            .as_slice(),
        b"canary-secret"
    );
}
#[test]
fn two_phase_probe_is_idempotent_and_cannot_skip_repeat_or_finish_early() {
    let f = Fixture::new(limits());
    let c = f.create();
    let op = f.begin(&c, "operation");
    assert!(matches!(
        f.manager
            .begin_probe(&f.principal, &c.id, 1, "operation")
            .unwrap(),
        ProbeStart::Pending
    ));
    assert!(op.reserve_phase(1, 100).is_err());
    let one = op.reserve_phase(0, 100).unwrap();
    assert!(op.reserve_phase(0, 100).is_err());
    assert_eq!(one.finish(Some(1), true), Err(ConnectionError::Invalid));
    one.before_send().unwrap();
    assert_eq!(one.before_send(), Err(ConnectionError::AlreadySent));
    assert_eq!(one.with_key(|key| key == b"canary-secret"), Ok(true));
    one.finish(Some(30), true).unwrap();
    assert_eq!(
        one.finish(Some(0), true),
        Err(ConnectionError::AlreadyFinished)
    );
    assert!(one.with_key(|_| ()).is_err());
    assert_eq!(
        f.manager.snapshot(&f.principal, &c.id).unwrap().state,
        ConnectionState::Checking
    );
    assert!(op.reserve_phase(0, 100).is_err());
    let two = op.reserve_phase(1, 100).unwrap();
    two.before_send().unwrap();
    two.finish(None, true).unwrap();
    assert!(matches!(
        f.manager
            .begin_probe(&f.principal, &c.id, 1, "operation")
            .unwrap(),
        ProbeStart::Ready
    ));
    assert!(op.reserve_phase(1, 100).is_err());
    let snapshot = f.manager.snapshot(&f.principal, &c.id).unwrap();
    assert_eq!(snapshot.state, ConnectionState::Ready);
    assert_eq!(snapshot.budget.calls, 2);
    assert_eq!(snapshot.budget.tokens, 130);
    assert_eq!(snapshot.budget.in_flight, 0);
}
#[test]
fn failure_and_unsent_local_failure_do_not_become_ready_or_refund_budget() {
    let f = Fixture::new(limits());
    let c = f.create();
    let op = f.begin(&c, "failed");
    let permit = op.reserve_phase(0, 100).unwrap();
    permit.finish(None, false).unwrap();
    assert!(permit.before_send().is_err());
    assert!(op.reserve_phase(1, 100).is_err());
    assert!(matches!(
        f.manager
            .begin_probe(&f.principal, &c.id, 1, "failed")
            .unwrap(),
        ProbeStart::Failed
    ));
    let state = f.manager.snapshot(&f.principal, &c.id).unwrap();
    assert_eq!(state.state, ConnectionState::Blocked);
    assert_eq!(state.budget.tokens, 100);
    assert_eq!(state.budget.in_flight, 0);
    let retry = f.begin(&c, "explicit-new-operation");
    let p = retry.reserve_phase(0, 100).unwrap();
    p.before_send().unwrap();
    p.finish(None, false).unwrap();
    assert!(matches!(
        f.manager
            .begin_probe(&f.principal, &c.id, 1, "explicit-new-operation")
            .unwrap(),
        ProbeStart::Failed
    ));
}
#[test]
fn atomic_concurrency_budget_is_shared_across_connections_and_unknown_drops() {
    let mut l = limits();
    l.max_concurrent = 1;
    let f = Fixture::new(l);
    let first = f.create();
    let second = f.create();
    let a = f.begin(&first, "a");
    let b = f.begin(&second, "b");
    let barrier = Arc::new(Barrier::new(3));
    let handles = [a, b]
        .into_iter()
        .map(|op| {
            let gate = barrier.clone();
            std::thread::spawn(move || {
                gate.wait();
                op.reserve_phase(0, 100)
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    let results = handles
        .into_iter()
        .map(|t| t.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|v| v.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|v| matches!(v, Err(ConnectionError::BudgetExceeded)))
            .count(),
        1
    );
    drop(results);
    let state = f.manager.snapshot(&f.principal, &first.id).unwrap();
    assert_eq!(state.budget.calls, 1);
    assert_eq!(state.budget.tokens, 100);
    assert_eq!(state.budget.in_flight, 1);
    let third = f.create();
    let retry = f.begin(&third, "c");
    assert!(matches!(
        retry.reserve_phase(0, 100),
        Err(ConnectionError::BudgetExceeded)
    ));
}
#[test]
fn known_usage_refunds_only_its_ticket_and_overrun_blocks_further_runs() {
    let f = Fixture::new(limits());
    let c = f.active();
    let first = f
        .manager
        .reserve_run(&f.principal, &c.id, c.version, c.generation, 100)
        .unwrap();
    let second = f
        .manager
        .reserve_run(&f.principal, &c.id, c.version, c.generation, 100)
        .unwrap();
    first.before_send().unwrap();
    second.before_send().unwrap();
    first.finish(Some(20), true).unwrap();
    assert_eq!(
        first.finish(Some(0), true),
        Err(ConnectionError::AlreadyFinished)
    );
    let snapshot = f.manager.snapshot(&f.principal, &c.id).unwrap();
    assert_eq!(snapshot.budget.in_flight, 1);
    assert_eq!(snapshot.budget.tokens, 140);
    second.finish(None, true).unwrap();
    assert_eq!(
        f.manager
            .snapshot(&f.principal, &c.id)
            .unwrap()
            .budget
            .tokens,
        140
    );
    let third = f
        .manager
        .reserve_run(&f.principal, &c.id, c.version, c.generation, 100)
        .unwrap();
    third.before_send().unwrap();
    assert_eq!(
        third.finish(Some(101), true),
        Err(ConnectionError::BudgetExceeded)
    );
    let blocked = f.manager.snapshot(&f.principal, &c.id).unwrap();
    assert_eq!(blocked.state, ConnectionState::Blocked);
    assert_eq!(blocked.run_state, RunState::Stopped);
    assert!(
        f.manager
            .reserve_run(&f.principal, &c.id, blocked.version, blocked.generation, 10)
            .is_err()
    );
}
#[test]
fn stop_cas_receipt_and_new_activation_permanently_reject_old_generation() {
    let f = Fixture::new(limits());
    let c = f.active();
    let old = f
        .manager
        .reserve_run(&f.principal, &c.id, c.version, c.generation, 100)
        .unwrap();
    old.before_send().unwrap();
    let stopped = f
        .manager
        .stop_operation(&f.principal, &c.id, c.version, "stop-once")
        .unwrap();
    assert_eq!(stopped.run_state, RunState::Stopped);
    assert!(
        f.manager
            .activate(&f.principal, &c.id, c.version, target())
            .is_err()
    );
    let active = f
        .manager
        .activate(&f.principal, &c.id, stopped.version, target())
        .unwrap();
    let duplicate = f
        .manager
        .stop_operation(&f.principal, &c.id, c.version, "stop-once")
        .unwrap();
    assert_eq!(duplicate.generation, active.generation);
    assert_eq!(duplicate.run_state, RunState::Active);
    assert!(
        f.manager
            .stop_operation(&f.principal, &c.id, active.version, "stop-once")
            .is_err()
    );
    assert!(old.with_key(|_| ()).is_err());
    assert!(old.finish(Some(1), true).is_err());
    let state = f.manager.snapshot(&f.principal, &c.id).unwrap();
    assert_eq!(state.budget.in_flight, 0);
    assert_eq!(state.budget.tokens, 120);
    assert!(
        f.manager
            .reserve_run(&f.principal, &c.id, active.version, c.generation, 10)
            .is_err()
    );
    let current = f
        .manager
        .reserve_run(&f.principal, &c.id, active.version, active.generation, 10)
        .unwrap();
    current.before_send().unwrap();
    current.finish(Some(10), true).unwrap();
    f.manager
        .forget(&f.principal, &c.id, active.version)
        .unwrap();
    f.manager
        .forget(&f.principal, &c.id, active.version)
        .unwrap();
    assert!(f.manager.lock().unwrap().connections[&c.id].key.is_none());
}
#[test]
fn trusted_binding_is_checked_at_activation_reservation_send_and_result() {
    let f = Fixture::new(limits());
    let c = f.ready();
    f.binding.store(false, Ordering::SeqCst);
    assert_eq!(
        f.manager
            .activate(&f.principal, &c.id, c.version, target())
            .unwrap_err(),
        ConnectionError::BindingDenied
    );
    f.binding.store(true, Ordering::SeqCst);
    let active = f
        .manager
        .activate(&f.principal, &c.id, c.version, target())
        .unwrap();
    let permit = f
        .manager
        .reserve_run(&f.principal, &c.id, active.version, active.generation, 100)
        .unwrap();
    f.binding.store(false, Ordering::SeqCst);
    assert_eq!(permit.before_send(), Err(ConnectionError::BindingDenied));
    f.binding.store(true, Ordering::SeqCst);
    permit.before_send().unwrap();
    f.binding.store(false, Ordering::SeqCst);
    assert_eq!(
        permit.finish(Some(0), true),
        Err(ConnectionError::BindingDenied)
    );
    let state = f.manager.snapshot(&f.principal, &c.id).unwrap();
    assert_eq!(state.budget.tokens, 120);
    assert_eq!(state.budget.in_flight, 0);
}
#[test]
fn sliding_absolute_expiry_clock_rollback_and_session_revocation_destroy_keys() {
    let f = Fixture::new(limits());
    let c = f.create();
    f.clock.0.store(1080, Ordering::SeqCst);
    let renewed = f.manager.renew(&f.principal, &c.id, 1).unwrap();
    assert_eq!(renewed.expires_at_ms, 1180);
    f.clock.0.store(1160, Ordering::SeqCst);
    let renewed = f
        .manager
        .renew(&f.principal, &c.id, renewed.version)
        .unwrap();
    f.clock.0.store(1240, Ordering::SeqCst);
    let renewed = f
        .manager
        .renew(&f.principal, &c.id, renewed.version)
        .unwrap();
    assert_eq!(renewed.expires_at_ms, 1300);
    f.clock.0.store(1300, Ordering::SeqCst);
    f.manager.sweep().unwrap();
    f.clock.0.store(1001, Ordering::SeqCst);
    assert!(
        f.manager
            .renew(&f.principal, &c.id, renewed.version)
            .is_err()
    );
    assert!(f.manager.lock().unwrap().connections[&c.id].key.is_none());
    let next = f.create();
    f.live.store(false, Ordering::SeqCst);
    f.manager.revoke_invalid_sessions().unwrap();
    assert!(
        f.manager.lock().unwrap().connections[&next.id]
            .key
            .is_none()
    );
    f.live.store(true, Ordering::SeqCst);
    assert!(f.manager.renew(&f.principal, &next.id, 1).is_err());
}
#[test]
fn logout_revokes_unsent_and_sent_permits_without_resurrecting_keys() {
    let f = Fixture::new(limits());
    let c = f.active();
    let p = f
        .manager
        .reserve_run(&f.principal, &c.id, c.version, c.generation, 100)
        .unwrap();
    p.before_send().unwrap();
    f.manager.logout(&f.principal).unwrap();
    assert!(p.with_key(|_| ()).is_err());
    assert!(p.finish(Some(0), true).is_err());
    assert!(f.manager.lock().unwrap().connections[&c.id].key.is_none());
    let next = f.create();
    assert_eq!(
        next.budget.tokens, 120,
        "new key never resets the session budget"
    );
}
#[test]
fn credentials_require_consent_and_limits_are_finite() {
    let f = Fixture::new(limits());
    assert!(
        f.manager
            .create(
                &f.principal,
                preset(),
                Zeroizing::new(b"canary".to_vec()),
                false
            )
            .is_err()
    );
    assert!(
        f.manager
            .create(
                &f.principal,
                preset(),
                Zeroizing::new(b"bad\nkey".to_vec()),
                true
            )
            .is_err()
    );
    let mut l = limits();
    l.max_calls = u64::MAX;
    assert!(!l.valid());
    l = limits();
    l.max_concurrent = 9;
    assert!(!l.valid());
    assert!(
        serde_json::from_value::<Limits>(
            serde_json::json!({"max_calls":1,"arbitrary_budget":true})
        )
        .is_err()
    );
}

#[test]
fn renewing_or_stopping_between_probe_phases_terminalizes_old_operation() {
    for renew in [true, false] {
        let f = Fixture::new(limits());
        let c = f.create();
        let op = f.begin(&c, "retired");
        let first = op.reserve_phase(0, 10).unwrap();
        first.before_send().unwrap();
        first.finish(Some(10), true).unwrap();
        f.clock.0.store(1030, Ordering::SeqCst);
        let current = if renew {
            f.manager.renew(&f.principal, &c.id, 1).unwrap()
        } else {
            f.manager.stop(&f.principal, &c.id, 1).unwrap()
        };
        assert!(op.reserve_phase(1, 10).is_err());
        assert!(matches!(
            f.manager
                .begin_probe(&f.principal, &c.id, current.version, "retired")
                .unwrap(),
            ProbeStart::Failed
        ));
        assert!(matches!(
            f.manager
                .begin_probe(&f.principal, &c.id, current.version, "explicit-new-check")
                .unwrap(),
            ProbeStart::Operation(_)
        ));
    }
}
#[test]
fn concurrent_activate_and_stop_have_a_single_cas_winner() {
    let f = Fixture::new(limits());
    let c = f.active();
    let barrier = Arc::new(Barrier::new(3));
    let handles = [true, false]
        .into_iter()
        .map(|activate| {
            let manager = f.manager.clone();
            let p = f.principal.clone();
            let c = c.clone();
            let gate = barrier.clone();
            std::thread::spawn(move || {
                gate.wait();
                if activate {
                    manager.activate(&p, &c.id, c.version, target())
                } else {
                    manager.stop_operation(&p, &c.id, c.version, "concurrent-stop")
                }
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    let results = handles
        .into_iter()
        .map(|t| t.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Err(ConnectionError::Conflict)))
            .count(),
        1
    );
    assert_eq!(
        f.manager.snapshot(&f.principal, &c.id).unwrap().generation,
        c.generation + 1
    );
}
#[test]
fn session_authority_and_sweeper_callbacks_never_hold_core_mutex() {
    let slot = Arc::new(Mutex::new(None::<ConnectionManager>));
    let callback = slot.clone();
    let manager = ConnectionManager::new(
        Arc::new(move |_: &Principal| {
            if let Some(manager) = callback.lock().unwrap().as_ref() {
                assert!(manager.shared.state.try_lock().is_ok())
            }
            true
        }),
        Arc::new(|_: &Principal, _: &BindingTarget| false),
        Arc::new(FakeClock(AtomicU64::new(1000))),
        limits(),
    )
    .unwrap();
    *slot.lock().unwrap() = Some(manager.clone());
    let p = Principal {
        account_id: "a".into(),
        session_id: "s".into(),
        auth_ref: "ref".into(),
    };
    let c = manager
        .create(&p, preset(), Zeroizing::new(b"canary".to_vec()), true)
        .unwrap();
    manager.snapshot(&p, &c.id).unwrap();
    manager.sweep().unwrap();
    manager.logout(&p).unwrap();
    slot.lock().unwrap().take();
}

#[test]
fn lease_renewal_preserves_active_epoch_but_stop_never_revives_it() {
    let f = Fixture::new(limits());
    let active = f.active();
    let permit = f
        .manager
        .reserve_run(
            &f.principal,
            &active.id,
            active.version,
            active.generation,
            20,
        )
        .unwrap();
    permit.before_send().unwrap();
    f.clock.0.store(1020, Ordering::SeqCst);
    let renewed = f
        .manager
        .renew(&f.principal, &active.id, active.version)
        .unwrap();
    assert_ne!(renewed.version, active.version);
    assert_eq!(renewed.active_version, Some(active.version));
    permit.finish(Some(10), true).unwrap();
    f.manager
        .check_execution(&f.principal, &active.id, active.version, active.generation)
        .unwrap();
    let next = f
        .manager
        .reserve_run(
            &f.principal,
            &active.id,
            active.version,
            active.generation,
            20,
        )
        .unwrap();
    next.before_send().unwrap();
    let stopped = f
        .manager
        .stop(&f.principal, &active.id, renewed.version)
        .unwrap();
    assert_eq!(stopped.active_version, None);
    assert!(next.finish(Some(1), true).is_err());
    assert!(
        f.manager
            .check_execution(&f.principal, &active.id, active.version, active.generation)
            .is_err()
    );
    let restarted = f
        .manager
        .activate(&f.principal, &active.id, stopped.version, target())
        .unwrap();
    assert_ne!(restarted.generation, active.generation);
    assert!(
        f.manager
            .reserve_run(
                &f.principal,
                &active.id,
                active.version,
                active.generation,
                20
            )
            .is_err()
    );
    f.manager
        .check_execution(
            &f.principal,
            &active.id,
            restarted.version,
            restarted.generation,
        )
        .unwrap();
}
#[test]
fn tool_execution_rechecks_authority_and_lease_without_reserving_a_call() {
    let f = Fixture::new(limits());
    let active = f.active();
    f.binding.store(false, Ordering::SeqCst);
    assert_eq!(
        f.manager
            .check_execution(&f.principal, &active.id, active.version, active.generation),
        Err(ConnectionError::BindingDenied)
    );
    f.binding.store(true, Ordering::SeqCst);
    f.clock.0.store(1100, Ordering::SeqCst);
    assert!(
        f.manager
            .check_execution(&f.principal, &active.id, active.version, active.generation)
            .is_err()
    );
    let snapshot = f.manager.snapshot(&f.principal, &active.id).unwrap();
    assert_eq!(snapshot.active_version, None);
    assert_eq!(snapshot.budget.calls, active.budget.calls);
}

#[path = "connection_custody_metadata_tests.rs"]
mod custody_metadata;

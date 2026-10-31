use super::*;
fn emit(scope: &NodeBftRouteTraceScopeV3, route: usize, digest: Option<&str>) {
    event_message(
        scope,
        route,
        "vote",
        "send",
        "ack",
        Duration::ZERO,
        Duration::ZERO,
        Duration::ZERO,
        1,
        digest,
        Some(Position {
            height: 6,
            round: 0,
            phase: "prevote",
        }),
    );
}
fn window(scope: &NodeBftRouteTraceScopeV3, route: usize) -> Arc<Window> {
    trace(scope, route).lock().unwrap().window.clone().unwrap()
}
#[test]
fn sealed_window_keeps_prelude_and_first_wait_after_later_noise_and_reports_unknown() {
    let scope = NodeBftRouteTraceScopeV3::new();
    for _ in 0..150 {
        emit(&scope, 0, Some("same"));
    }
    {
        let _guard = scope.phase_for_test("first_wait_h6");
        emit(&scope, 0, Some("h6"));
        emit(&scope, 0, None);
    }
    for _ in 0..5000 {
        emit(&scope, 0, Some("later-h8"));
    }
    let saved = window(&scope, 0);
    assert_eq!(
        (saved.prelude, saved.count, saved.unknown, saved.overflow),
        (128, 2, 1, 0)
    );
    assert_eq!(saved.records.iter().flatten().count(), 130);
    let end = scope.inner.window_end.load(Ordering::Acquire);
    {
        let _again = scope.phase_for_test("first_wait_h6");
    }
    assert_eq!(scope.inner.window_end.load(Ordering::Acquire), end);
    assert_eq!(window(&scope, 0).count, 2);
}
#[test]
fn window_panic_seals_and_separate_scope_can_reuse_route_ids() {
    let first = NodeBftRouteTraceScopeV3::new();
    let second = NodeBftRouteTraceScopeV3::new();
    let failed = std::panic::catch_unwind(|| {
        let _guard = first.phase_for_test("first_wait_h6");
        emit(&first, 0, Some("before-panic"));
        panic!("controlled sealed-window unwind");
    });
    assert!(failed.is_err());
    assert_ne!(first.inner.window_end.load(Ordering::Acquire), 0);
    {
        let _guard = second.phase_for_test("first_wait_h6");
        emit(&second, 0, Some("other"));
    }
    emit(&first, 0, Some("after-panic"));
    assert_eq!(window(&first, 0).count, 1);
    assert_eq!(window(&second, 0).count, 1);
    assert_ne!(first.inner.ordinal, second.inner.ordinal);
}
#[test]
fn window_capacity_and_loss_are_explicit_and_storage_has_a_hard_bound() {
    let scope = NodeBftRouteTraceScopeV3::new();
    let _guard = scope.phase_for_test("first_wait_h6");
    for _ in 0..EVENTS + 5 {
        emit(&scope, 0, Some("same"));
    }
    {
        let _held = trace(&scope, 0).lock().unwrap();
        emit(&scope, 0, Some("lost"));
    }
    let saved = window(&scope, 0);
    assert_eq!((saved.count, saved.overflow), (EVENTS, 5));
    assert_eq!(scope.inner.window_lost[0].load(Ordering::Relaxed), 1);
    assert_eq!(saved.records.len(), CAPACITY);
    const {
        assert!(MAX_WINDOW_BYTES < 192 * 1024 * 1024);
    }
    assert_eq!(
        MAX_RECORD_BYTES,
        ROUTES * CAPACITY * std::mem::size_of::<Option<Record>>()
    );
    eprintln!(
        "bounded window record_size={} route_bytes={} max_total_bytes={MAX_WINDOW_BYTES}",
        std::mem::size_of::<Option<Record>>(),
        CAPACITY * std::mem::size_of::<Option<Record>>()
    );
    // Snapshot holds an Arc, never duplicates the large backing allocation.
    let copied = snapshots(&scope, &[0]);
    assert!(Arc::ptr_eq(&saved, copied[0].1.as_ref().unwrap()));
}
#[test]
fn worker_cadence_totals_do_not_exhaust_message_window_and_default_scope_is_off() {
    let scope = NodeBftRouteTraceScopeV3::new();
    let _guard = scope.phase_for_test("first_wait_h6");
    for _ in 0..5000 {
        event_message(
            &scope,
            0,
            "worker",
            "worker.loop",
            "end",
            Duration::ZERO,
            Duration::ZERO,
            Duration::ZERO,
            1,
            None,
            None,
        );
    }
    drop(worker_span(&scope, 0, 1, "commit.hook"));
    assert_eq!(trace(&scope, 0).lock().unwrap().recent.len(), 2);
    let saved = window(&scope, 0);
    assert_eq!(saved.worker_totals[0].count, 5000);
    assert_eq!(saved.count, 2);
    assert_eq!(saved.unknown, 0);
}

#[test]
fn a_poll_spanning_the_wait_boundary_remains_visible_after_sealing() {
    let scope = NodeBftRouteTraceScopeV3::new();
    let outstanding = worker_span(&scope, 0, 1, "worker.poll");
    {
        let _guard = scope.phase_for_test("first_wait_h6");
    }
    drop(outstanding);
    let saved = window(&scope, 0);
    assert_eq!(saved.active[1].as_ref().unwrap().stage, "worker.poll");
    assert_eq!(saved.worker_totals[1].count, 0);
}

#[test]
fn message_stages_keep_transport_ordinals_and_active_lifetime_at_the_deadline() {
    let scope = NodeBftRouteTraceScopeV3::new();
    emit(&scope, 0, Some("opaque-content"));
    let position = Some(Position {
        height: 6,
        round: 2,
        phase: "proposal",
    });
    let lifetime = message_span(
        &scope,
        0,
        7,
        "worker.fetch_lifetime",
        "opaque-content".into(),
        position,
    );
    {
        let _guard = scope.phase_for_test("first_wait_h6");
        for stage in &WORKER_STAGES[17..] {
            if *stage != "worker.fetch_lifetime" {
                drop(message_span(
                    &scope,
                    0,
                    7,
                    stage,
                    "opaque-content".into(),
                    position,
                ));
            }
        }
    }
    drop(lifetime);
    let saved = window(&scope, 0);
    let active = saved.active[WORKER_STAGES
        .iter()
        .position(|s| *s == "worker.fetch_lifetime")
        .unwrap()]
    .as_ref()
    .unwrap();
    assert_eq!(active.message, 1);
    assert_eq!(active.position.unwrap().height, 6);
    assert_eq!(active.position.unwrap().round, 2);
    let mut rendered = Vec::new();
    render(&scope, 7, snapshots(&scope, &[0]), &mut rendered);
    let rendered = String::from_utf8(rendered).unwrap();
    let active_line = rendered
        .lines()
        .find(|line| line.starts_with("BFT_WINDOW_ACTIVE "))
        .unwrap();
    assert!(active_line.contains("stage=worker.fetch_lifetime"));
    assert!(active_line.contains("message=1 position_known=1 height=6 round=2 phase=proposal"));
    assert!(!rendered.contains("opaque-content"));
    for stage in &WORKER_STAGES[17..] {
        let index = WORKER_STAGES.iter().position(|s| s == stage).unwrap();
        if *stage != "worker.fetch_lifetime" {
            assert_eq!(saved.worker_totals[index].count, 1, "{stage}");
        }
    }
    assert!(
        saved
            .records
            .iter()
            .flatten()
            .filter(|r| r.message != 0)
            .all(|r| r.message == 1)
    );
    // A different fixture with the same content has its own ordinal table.
    let other = NodeBftRouteTraceScopeV3::new();
    emit(&other, 0, Some("other-first"));
    emit(&other, 0, Some("opaque-content"));
    assert_eq!(
        trace(&other, 0)
            .lock()
            .unwrap()
            .recent
            .back()
            .unwrap()
            .message,
        2
    );
}

#[test]
fn correlated_worker_message_overflow_is_explicit_even_when_position_is_unknown() {
    let scope = NodeBftRouteTraceScopeV3::new();
    for n in 0..MESSAGES {
        emit(&scope, 0, Some(&format!("opaque-{n}")));
    }
    let _phase = scope.phase_for_test("first_wait_h6");
    drop(message_span(
        &scope,
        0,
        1,
        "worker.selected",
        "beyond-bound".into(),
        None,
    ));
    let saved = window(&scope, 0);
    assert_eq!(saved.unknown, 2);
    assert_eq!(saved.count, 2);
    assert!(
        saved
            .records
            .iter()
            .skip(PRELUDE)
            .flatten()
            .all(|r| r.message == 0)
    );
}

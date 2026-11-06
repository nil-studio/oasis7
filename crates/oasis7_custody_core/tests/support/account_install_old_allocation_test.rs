// Append within test_session_termination_tests.rs (existing real world/source fixtures).
#[test]
fn real_legacy_allocation_wins_over_supplemental_prepare() {
    let (mut f, _, _) = claimed();
    f.state
        .expire_test_sessions(UnixMillis::new(50_000))
        .unwrap();
    let input =
        super::super::super::account_install_actual_state_tests::supplemental_input(&f, 52_000);
    let life = funded_source(
        &mut f,
        SessionSourceFundingV1::TerminationPlan,
        SessionPurposeV1::TerminationPlan,
        "legacy-wins-source",
        &"dc".repeat(32),
        52_000,
    );
    let policy = Policy::new(f.state.draft());
    let p = policy.current(&f.context);
    let ns = scope(&f.pin);
    let live = TestSessionLiveCurrent {
        provision: &p,
        native_scope: &ns,
        lifetime: &life,
        private_sid_generation: DecimalU64(1),
        source_operation_id: "legacy-wins-source",
        source_ticket: 2,
        observed_at: UnixMillis::new(52_002),
    };
    let evidence = world(
        &f.state,
        TerminationOperationV1::Stop,
        "legacy-wins-source",
        None,
    );
    f.state
        .allocate_test_session_termination(
            ID,
            TerminationOperationV1::Stop,
            &TestSessionTerminationCurrent {
                live: &live,
                world: &evidence,
            },
            UnixMillis::new(52_003),
            &mut StrictCrypto,
        )
        .unwrap();
    let before = f.state.clone();
    assert!(
        f.state
            .begin_termination_v3_install_fixture(
                input,
                "db".repeat(32),
                &p,
                UnixMillis::new(52_004),
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(f.state, before);
}

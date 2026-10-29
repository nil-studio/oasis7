use super::*;
#[test]
fn tool_nonce_maintenance_reconciles_historical_purpose_transport_exhaustion() {
    for purpose in [SessionPurposeV1::Execution, SessionPurposeV1::ReconcileRead] {
        let (mut f, _, _) = claimed();
        let now = UnixMillis::new(12_000);
        let before = remaining(&f, purpose);
        for n in 0..before {
            // Real existing internal ledger API, not a forged balance or closing flag.
            f.state
                .reserve_test_session_purpose_transport(
                    ID,
                    purpose,
                    &format!("historic-exhaustion-{n}"),
                    now,
                )
                .unwrap();
        }
        assert_eq!(remaining(&f, purpose), 0);
        assert!(f.state.test_sessions()[ID].closing.is_none());
        assert_eq!(f.state.next_test_session_expiry_at(now).unwrap(), Some(now));
        assert_eq!(
            f.state.expire_test_sessions(now).unwrap(),
            vec![ID.to_owned()]
        );
        let progress = f.state.test_session_progress(ID, now).unwrap();
        assert_eq!(progress.stage, TestSessionPhaseV1::Closing);
        assert_eq!(
            progress.closing_reason,
            Some(TestSessionClosingReasonV1::BudgetExhausted)
        );
        assert!(progress.write_closed);
        assert_eq!(f.state.next_test_session_expiry_at(now).unwrap(), None);
        let closed = f.state.clone();
        assert!(
            f.state
                .expire_test_sessions(UnixMillis::new(13_000))
                .unwrap()
                .is_empty()
        );
        assert_eq!(f.state, closed);
        restore(&f.state);
    }
}

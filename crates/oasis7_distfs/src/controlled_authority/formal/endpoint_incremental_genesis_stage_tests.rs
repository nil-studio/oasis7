use super::super::super::super::tests::with_signed_request_states_fixture;
use super::*;
#[test]
fn explicit_stage_cancellation_does_not_record_or_supply_source_work() {
    with_signed_request_states_fixture(|stage, endpoint, _, original| {
        if stage != "Prepared" {
            return;
        }
        let FormalRequestStateV1::Prepared { candidate, .. } = original else {
            unreachable!()
        };
        let before = endpoint.store.checkpoint_bytes().unwrap();
        let error = endpoint
            .stage_genesis_source(candidate, || true)
            .unwrap_err();
        assert!(error.to_string().contains("cancelled before verification"));
        assert_eq!(endpoint.store.checkpoint_bytes().unwrap(), before);
    });
}
#[test]
fn legacy_profile_cannot_enter_explicit_gen_stage_or_paid_qualified_read() {
    with_signed_request_states_fixture(|stage, endpoint, _, original| {
        if stage != "Prepared" {
            return;
        }
        let FormalRequestStateV1::Prepared { candidate, .. } = original else {
            unreachable!()
        };
        let before = endpoint.store.checkpoint_bytes().unwrap();
        assert!(endpoint.stage_genesis_source(candidate, || false).is_err());
        let context = verify_formal_candidate(
            &candidate.candidate,
            candidate.frozen.as_ref(),
            &endpoint.authority,
            endpoint.parent.as_ref(),
            None,
            candidate.branch,
        )
        .unwrap();
        let memory = MemoryBudget::new(128 << 20, 0).unwrap();
        let work = FormalArchiveReadWorkV1::new(1_000_000, 64 << 20).unwrap();
        assert!(
            endpoint
                .read_genesis_qualified_bounded(candidate, &context, &memory, &work, &mut || false)
                .is_err()
        );
        assert_eq!(memory.used(), 0);
        assert_eq!(endpoint.store.checkpoint_bytes().unwrap(), before);
    });
}

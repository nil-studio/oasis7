use super::super::super::tests::with_unaccepted_host_endpoint_fixture;
use super::*;
#[test]
fn actual_unaccepted_endpoint_cannot_mint_host_source() {
    with_unaccepted_host_endpoint_fixture(|endpoint, provider, policy| {
        let memory = MemoryBudget::new(16 << 20, 0).unwrap();
        let work = FormalArchiveReadWorkV1::new(1000, 1 << 20).unwrap();
        let error = endpoint
            .into_accepted_genesis_host_source_bounded(
                provider.as_ref(),
                policy,
                &memory,
                &work,
                &mut || false,
            )
            .err()
            .unwrap();
        assert!(
            matches!(error,BoundedAcquireErrorV1::Work(FormalError::Invalid(reason)) if reason=="Host GEN accepted parent missing")
        );
        assert_eq!(memory.used(), 0);
    });
}
#[test]
fn actual_source_cancellation_and_zero_memory_release_admission() {
    for cancel in [true, false] {
        with_unaccepted_host_endpoint_fixture(|endpoint, provider, policy| {
            let memory = MemoryBudget::new(0, 0).unwrap();
            let work = FormalArchiveReadWorkV1::new(1000, 1 << 20).unwrap();
            let error = endpoint
                .into_accepted_genesis_host_source_bounded(
                    provider.as_ref(),
                    policy,
                    &memory,
                    &work,
                    &mut || cancel,
                )
                .err()
                .unwrap();
            match error {
                BoundedAcquireErrorV1::Work(FormalError::Invalid(reason)) => assert_eq!(
                    reason,
                    if cancel {
                        "bounded archive read cancelled"
                    } else {
                        "Host source owner admission"
                    }
                ),
                other => panic!("unexpected admission error {other:?}"),
            }
            assert_eq!(memory.used(), 0);
        });
    }
}

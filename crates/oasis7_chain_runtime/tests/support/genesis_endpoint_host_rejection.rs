//! Preserve the original GEN source/Host rejection checks on the shared real GEN.
use super::*;
use oasis7_chain_runtime::iws_host_runtime::{IwsHostRuntimeErrorV1, IwsHostRuntimeV1};
use oasis7_distfs::controlled_authority::formal::endpoint::FormalArchiveReadWorkV1;
use oasis7_wasm_executor::bounded_worker::WorkerLimitsV1;

pub(super) fn check(fixture: AcceptedGenesisFixture<'_>) {
    let AcceptedGenesisFixture {
        primary,
        replica,
        primary_provider,
        replica_provider,
        policy,
        memory,
        work,
        expected,
        ..
    } = fixture;
    // The true accepted sources are consumed into the actual Host API.
    // This fixture's native descriptor/executable is deliberately not a
    // runnable platform worker; it must never become a current Host.
    let source = primary
        .into_accepted_genesis_host_source_bounded(
            primary_provider.as_ref(),
            policy,
            memory,
            work,
            &mut || false,
        )
        .unwrap();
    assert_eq!(source.record_id(), expected.verified_record_id());
    assert!(source.uses_budget(memory));
    assert!(source.reserved_bytes() > 0);
    let foreign = MemoryBudget::new(1 << 20, 0).unwrap();
    {
        let mut stop = || false;
        let mut ctx = work.bounded_context(&foreign, &mut stop);
        assert!(source.validate_context(&mut ctx).is_err());
    }
    assert_eq!(foreign.used(), 0);
    let other_work = FormalArchiveReadWorkV1::new(1000, 1 << 20).unwrap();
    {
        let mut stop = || false;
        let mut ctx = other_work.bounded_context(memory, &mut stop);
        assert!(source.validate_context(&mut ctx).is_err());
    }
    let worker_limits = || WorkerLimitsV1 {
        address_space_bytes: 64 << 20,
        max_request_bytes: 1 << 20,
        max_output_bytes: 4096,
        wall_time_ms: 5000,
    };
    let mut host = IwsHostRuntimeV1::default();
    assert!(
        host.install_accepted_genesis_source_bounded(
            source,
            worker_limits(),
            memory,
            work,
            &AtomicBool::new(true)
        )
        .is_err()
    );
    assert!(!host.is_installed());
    assert_eq!(host.current_fence(), None);
    assert_eq!(host.generation(), 0);
    let error = host
        .install_genesis_bounded(
            replica,
            replica_provider.as_ref(),
            policy,
            worker_limits(),
            memory,
            work,
            &AtomicBool::new(false),
        )
        .unwrap_err();
    assert!(
        matches!(
            error,
            IwsHostRuntimeErrorV1::Formal(_)
                | IwsHostRuntimeErrorV1::Protocol(_)
                | IwsHostRuntimeErrorV1::Environment(_)
        ),
        "{error:?}"
    );
    assert!(!host.is_installed());
    assert_eq!(host.current_fence(), None);
    assert_eq!(host.generation(), 0);
}

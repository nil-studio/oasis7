//! Atomicity and rollback checks use explicit in-memory doubles, not hardware custody.
use super::*;
use crate::protected_bft_v3::archive_transfer::*;
fn snapshot() -> (BftSignerBindingV3, BftSignerBackendSnapshotV3) {
    let binding = binding();
    let snapshot = BftSignerBackendSnapshotV3 {
        state: BftSignerStateV3::from_handoff(&binding).unwrap(),
        successor_archive_transfer: Some(BftSuccessorArchiveTransferLedgerV1::provisioned()),
        lease_generation: 7,
        lease_token_digest: hash(81),
        protected_time_floor_ms: 100,
    };
    (binding, snapshot)
}
fn reserve(
    s: &BftSignerBackendSnapshotV3,
    amount: u64,
) -> Result<BftSuccessorArchiveTransferLedgerV1, BftSignerErrorV3> {
    reserve_successor_archive_transfer_v1(
        s,
        s.lease_generation,
        &s.lease_token_digest,
        s.state.revision,
        &s.state.state_digest,
        s.successor_archive_transfer
            .map_or(0, |l| l.reserved_response_bytes),
        amount,
    )
}
#[test]
fn transfer_reserve_bounds_and_combined_capacity_are_fail_closed() {
    let (_, mut s) = snapshot();
    assert!(reserve(&s, 0).is_err());
    assert!(
        reserve(
            &s,
            BFT_SUCCESSOR_ARCHIVE_TRANSFER_MAX_RESERVATION_BYTES_V1 + 1
        )
        .is_err()
    );
    s.successor_archive_transfer
        .as_mut()
        .unwrap()
        .reserved_response_bytes = u64::MAX;
    assert!(reserve(&s, 1).is_err());
    s.successor_archive_transfer
        .as_mut()
        .unwrap()
        .reserved_response_bytes =
        crate::protected_bft_v3::successor_window::BFT_SUCCESSOR_WINDOW_MAX_ARCHIVE_BYTES_V1;
    assert!(reserve(&s, 1).is_err());
    let mut window = crate::protected_bft_v3::successor_state::BftSuccessorWindowStateV1::empty();
    window.usage.total_bytes =
        crate::protected_bft_v3::successor_window::BFT_SUCCESSOR_WINDOW_MAX_ARCHIVE_BYTES_V1 - 100;
    s.state.successor_window = Some(window);
    s.successor_archive_transfer
        .as_mut()
        .unwrap()
        .reserved_response_bytes = 90;
    assert_eq!(reserve(&s, 10).unwrap().reserved_response_bytes, 100);
    assert!(reserve(&s, 11).is_err());
    assert!(
        reserve_successor_archive_transfer_v1(
            &s,
            6,
            &s.lease_token_digest,
            s.state.revision,
            &s.state.state_digest,
            90,
            1
        )
        .is_err()
    );
    assert!(
        reserve_successor_archive_transfer_v1(
            &s,
            7,
            &s.lease_token_digest,
            s.state.revision,
            &s.state.state_digest,
            89,
            1
        )
        .is_err()
    );
}
#[test]
fn transfer_receipt_must_preserve_exact_state_lease_floor_and_counter() {
    let (_, s) = snapshot();
    let mut after = s.clone();
    after.successor_archive_transfer = Some(reserve(&s, 100).unwrap());
    validate_successor_archive_transfer_reservation_v1(&s, &after, 100).unwrap();
    let good = after.clone();
    after.state.revision += 1;
    assert!(validate_successor_archive_transfer_reservation_v1(&s, &after, 100).is_err());
    after = good.clone();
    after.protected_time_floor_ms += 1;
    assert!(validate_successor_archive_transfer_reservation_v1(&s, &after, 100).is_err());
    after = good.clone();
    after.lease_generation += 1;
    assert!(validate_successor_archive_transfer_reservation_v1(&s, &after, 100).is_err());
    after = good.clone();
    after.lease_token_digest = hash(82);
    assert!(validate_successor_archive_transfer_reservation_v1(&s, &after, 100).is_err());
    after = good;
    after
        .successor_archive_transfer
        .as_mut()
        .unwrap()
        .reserved_response_bytes += 1;
    assert!(validate_successor_archive_transfer_reservation_v1(&s, &after, 100).is_err());
}
#[test]
fn transfer_budget_survives_state_cas_and_new_lease() {
    let (binding, s) = snapshot();
    let shared = Arc::new(Mutex::new(s.clone()));
    let mut backend = Backend {
        binding: binding.digest().unwrap(),
        snapshot: shared.clone(),
    };
    let reserved = backend
        .reserve_successor_archive_transfer(
            7,
            &s.lease_token_digest,
            s.state.revision,
            &s.state.state_digest,
            0,
            17,
        )
        .unwrap();
    let mut next = reserved.state.clone();
    next.revision += 1;
    let mut window = crate::protected_bft_v3::successor_state::BftSuccessorWindowStateV1::empty();
    window.usage.total_bytes =
        crate::protected_bft_v3::successor_window::BFT_SUCCESSOR_WINDOW_MAX_ARCHIVE_BYTES_V1;
    next.successor_window = Some(window);
    assert!(
        backend
            .compare_and_swap(
                7,
                &s.lease_token_digest,
                s.state.revision,
                &s.state.state_digest,
                Some(500),
                next
            )
            .is_err()
    );
    assert_eq!(*shared.lock().unwrap(), reserved);
    let released = backend.release_lease(7, &s.lease_token_digest).unwrap();
    assert_eq!(
        released.successor_archive_transfer,
        reserved.successor_archive_transfer
    );
    let acquired = backend
        .acquire_lease(
            &binding.digest().unwrap(),
            released.lease_generation,
            &hash(83),
        )
        .unwrap();
    let BftSignerAcquireOutcomeV3::Acquired(acquired) = acquired else {
        panic!("new lease refused")
    };
    assert_eq!(
        acquired.successor_archive_transfer,
        reserved.successor_archive_transfer
    );
    let mut next = acquired.state.clone();
    next.revision += 1;
    let installed = backend
        .compare_and_swap(
            acquired.lease_generation,
            &acquired.lease_token_digest,
            acquired.state.revision,
            &acquired.state.state_digest,
            None,
            next,
        )
        .unwrap();
    assert_eq!(
        installed.successor_archive_transfer,
        reserved.successor_archive_transfer
    );
}
#[test]
fn runtime_rejects_cloned_old_counter_and_live_capability_changes() {
    let (binding, s) = snapshot();
    let service = BftSignerServiceV3::open(
        binding.clone(),
        Box::new(Handle {
            key: key(33),
            disabled: None,
        }),
        vec![
            Box::new(Handle {
                key: key(31),
                disabled: None,
            }),
            Box::new(Handle {
                key: key(32),
                disabled: None,
            }),
        ],
        Box::new(Backend {
            binding: binding.digest().unwrap(),
            snapshot: Arc::new(Mutex::new(s.clone())),
        }),
    )
    .unwrap();
    let mut runtime = service.runtime.lock().unwrap();
    let after = service
        .reserve_archive_transfer(&mut runtime, &s, 27)
        .unwrap();
    assert_eq!(after.state, s.state);
    assert!(runtime.observe_archive_transfer(&s).is_err());
    assert!(runtime.poisoned);
    assert!(
        validate_successor_archive_transfer_observation_v1(
            after.successor_archive_transfer,
            &BftSignerBackendSnapshotV3 {
                successor_archive_transfer: None,
                ..after.clone()
            }
        )
        .is_err()
    );
    assert!(validate_successor_archive_transfer_observation_v1(None, &after).is_err());
}
#[test]
fn unsupported_ledger_never_initializes_on_reserve_or_new_owner() {
    let (binding, mut s) = snapshot();
    s.successor_archive_transfer = None;
    let shared = Arc::new(Mutex::new(s.clone()));
    let mut backend = Backend {
        binding: binding.digest().unwrap(),
        snapshot: shared.clone(),
    };
    assert!(
        backend
            .reserve_successor_archive_transfer(
                7,
                &s.lease_token_digest,
                s.state.revision,
                &s.state.state_digest,
                0,
                10
            )
            .is_err()
    );
    assert_eq!(*shared.lock().unwrap(), s);
    let released = backend.release_lease(7, &s.lease_token_digest).unwrap();
    assert!(released.successor_archive_transfer.is_none());
    let BftSignerAcquireOutcomeV3::Acquired(acquired) = backend
        .acquire_lease(
            &binding.digest().unwrap(),
            released.lease_generation,
            &hash(83),
        )
        .unwrap()
    else {
        panic!("new owner refused")
    };
    assert!(acquired.successor_archive_transfer.is_none());
}

#[test]
fn legacy_backend_default_is_unsupported_and_does_not_touch_provisioned_data() {
    struct Legacy(Backend);
    impl BftSignerBackendV3 for Legacy {
        fn load_provisioned(
            &mut self,
            b: &str,
        ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
            self.0.load_provisioned(b)
        }
        fn acquire_lease(
            &mut self,
            b: &str,
            g: u64,
            t: &str,
        ) -> Result<BftSignerAcquireOutcomeV3, BftSignerErrorV3> {
            self.0.acquire_lease(b, g, t)
        }
        fn release_lease(
            &mut self,
            g: u64,
            t: &str,
        ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
            self.0.release_lease(g, t)
        }
        fn read_for_lease(
            &mut self,
            g: u64,
            t: &str,
        ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
            self.0.read_for_lease(g, t)
        }
        fn protected_time_ms(&mut self, g: u64, t: &str) -> Result<i64, BftSignerErrorV3> {
            self.0.protected_time_ms(g, t)
        }
        fn compare_and_swap(
            &mut self,
            g: u64,
            t: &str,
            r: u64,
            d: &str,
            e: Option<i64>,
            n: BftSignerStateV3,
        ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
            self.0.compare_and_swap(g, t, r, d, e, n)
        }
    }
    let (binding, mut s) = snapshot();
    s.successor_archive_transfer = None;
    let shared = Arc::new(Mutex::new(s.clone()));
    let mut legacy = Legacy(Backend {
        binding: binding.digest().unwrap(),
        snapshot: shared.clone(),
    });
    assert!(
        legacy
            .reserve_successor_archive_transfer(
                7,
                &s.lease_token_digest,
                s.state.revision,
                &s.state.state_digest,
                0,
                10
            )
            .unwrap_err()
            .to_string()
            .contains("unsupported")
    );
    assert_eq!(*shared.lock().unwrap(), s);
}

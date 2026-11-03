//! Load-only fault clones of the actual Source AppliedCut namespace.
//! No synthesized valid history and no new lease/provider authority.
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionSealedCurrentV1, BftSignerAcquireOutcomeV3, BftSignerBackendSnapshotV3,
    BftSignerBackendV3, BftSignerErrorV3,
};

#[derive(Clone)]
struct Image {
    binding: String,
    signer: BftSignerBackendSnapshotV3,
    current: BftPrivateRetentionCurrentV1,
    sealed: BftPrivateRetentionSealedCurrentV1,
}
struct LoadOnly(Image);
struct WithoutSealed(Image);
impl LoadOnly {
    fn image(&self) -> &Image {
        &self.0
    }
}
impl WithoutSealed {
    fn image(&self) -> &Image {
        &self.0
    }
}
macro_rules! load_only_backend {
    ($ty:ty $(, $sealed:item)?) => {
        impl BftSignerBackendV3 for $ty {
            fn load_provisioned(&mut self, binding: &str)
                -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
                assert_eq!(binding, self.image().binding);
                Ok(self.image().signer.clone())
            }
            fn load_private_retention_current(&mut self, binding: &str)
                -> Result<BftPrivateRetentionCurrentV1, BftSignerErrorV3> {
                assert_eq!(binding, self.image().binding);
                Ok(self.image().current.clone())
            }
            $($sealed)?
            fn acquire_lease(&mut self, _: &str, _: u64, _: &str)
                -> Result<BftSignerAcquireOutcomeV3, BftSignerErrorV3> {
                panic!("load-only reopen must not Acquire")
            }
            fn release_lease(&mut self, _: u64, _: &str)
                -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
                panic!("load-only reopen must not release")
            }
            fn read_for_lease(&mut self, _: u64, _: &str)
                -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
                panic!("load-only reopen must not borrow stored lease credentials")
            }
            fn protected_time_ms(&mut self, _: u64, _: &str)
                -> Result<i64, BftSignerErrorV3> {
                panic!("load-only reopen must not advance protected time")
            }
            fn compare_and_swap(&mut self, _: u64, _: &str, _: u64, _: &str,
                _: Option<i64>, _: BftSignerStateV3)
                -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
                panic!("load-only reopen must not CAS")
            }
        }
    };
}
load_only_backend!(
    LoadOnly,
    fn load_private_retention_sealed_current(
        &mut self,
        binding: &str,
    ) -> Result<BftPrivateRetentionSealedCurrentV1, BftSignerErrorV3> {
        assert_eq!(binding, self.0.binding);
        Ok(self.0.sealed.clone())
    }
);
// Tests the actual trait default, rather than copying its error into a double.
load_only_backend!(WithoutSealed);

pub(super) fn check(
    binding: &BftSignerBindingV3,
    request: &FreshBftPrivateSourceInputsV1,
    role: &BftPrivateRetentionAttestorV1,
    backend: &mut BftPrivateRetentionMemoryBackendV1,
) {
    let digest = binding.digest().unwrap();
    let before = Image {
        binding: digest.clone(),
        signer: backend.load_provisioned(&digest).unwrap(),
        current: backend.load_private_retention_current(&digest).unwrap(),
        sealed: backend
            .load_private_retention_sealed_current(&digest)
            .unwrap(),
    };
    assert!(before.sealed.current == before.current);
    assert!(before.current.signer == before.signer);
    assert!(!before.sealed.seals.is_empty());
    assert!(!before.sealed.history.is_empty());
    assert!(!before.sealed.sealed_inputs.is_empty());
    before.sealed.validate_inventory(binding, role).unwrap();
    let original_ledger = before.current.retention.as_ref().unwrap();
    let seal_id = original_ledger
        .map
        .previous_seal_receipt_digest
        .clone()
        .unwrap();
    let receipt_id = oasis7_cas::to_canonical_cbor(&original_ledger.receipt).unwrap();
    // Locate the actual current original by exact receipt, not an invented hash.
    let current_history_key = before
        .sealed
        .history
        .iter()
        .find(|(_, original)| {
            oasis7_cas::to_canonical_cbor(&original.retention.receipt).unwrap() == receipt_id
        })
        .map(|(id, _)| id.clone())
        .unwrap();
    for case in 0..7 {
        let mut image = before.clone();
        let mut supplied_role = role.clone();
        let expected = match case {
            0 => {
                assert!(image.sealed.seals.remove(&seal_id).is_some());
                "original sealed chain gap"
            }
            1 => {
                assert!(image.sealed.history.remove(&current_history_key).is_some());
                "original retention receipt history gap"
            }
            2 => {
                let id = image.sealed.sealed_inputs.keys().next().unwrap().clone();
                image.sealed.sealed_inputs.remove(&id).unwrap();
                "sealed original input missing"
            }
            3 => {
                image.sealed.current.signer.protected_time_floor_ms += 1;
                "private retention sealed open snapshot mismatch"
            }
            4 => {
                let id = image.sealed.current.inputs.keys().next().unwrap().clone();
                image.sealed.current.inputs.remove(&id).unwrap();
                "private retention sealed open snapshot mismatch"
            }
            5 => {
                supplied_role.source.binding_digest = "fe".repeat(32);
                "private retention external capability differs from original source"
            }
            _ => "complete original retention load unsupported",
        };
        let wrapped: Box<dyn BftSignerBackendV3> = if case == 6 {
            Box::new(WithoutSealed(image))
        } else {
            Box::new(LoadOnly(image))
        };
        let result = BftSignerServiceV3::open_with_private_retention(
            binding.clone(),
            Box::new(MemoryTestSignerKey(fixtures::key(116))),
            vec![Box::new(ReadbackOnlyKey(Arc::new(
                std::sync::atomic::AtomicUsize::new(0),
            )))],
            wrapped,
            // Private validation precedes successor support validation. Exact
            // error assertions prevent a later missing-support error passing.
            None,
            BftPrivateRetentionSupportV1::new(
                supplied_role,
                Arc::new(
                    ActualBftPrivateSourceCreationVerifierV1::for_original_source(request.clone())
                        .unwrap(),
                ),
                Duration::from_secs(120),
            )
            .unwrap(),
        );
        let error = match result {
            Ok(_) => panic!("sealed reopen fault {case} was accepted"),
            Err(error) => error,
        };
        assert_eq!(error.0, expected, "sealed reopen case {case}");
        assert!(backend.load_provisioned(&digest).unwrap() == before.signer);
        assert!(backend.load_private_retention_current(&digest).unwrap() == before.current);
        let after = backend
            .load_private_retention_sealed_current(&digest)
            .unwrap();
        assert!(
            after == before.sealed,
            "fault clones must not mutate original backend"
        );
        for (id, original) in &before.sealed.sealed_inputs {
            assert!(Arc::ptr_eq(original, after.sealed_inputs.get(id).unwrap()));
        }
        for (id, original) in &before.sealed.history {
            assert!(Arc::ptr_eq(original, after.history.get(id).unwrap()));
        }
        for (id, original) in &before.sealed.seals {
            assert!(Arc::ptr_eq(original, after.seals.get(id).unwrap()));
        }
    }
}

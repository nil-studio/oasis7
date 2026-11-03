//! Coherent historical readback faults; current lease and current State stay intact.
use super::*;

pub(super) struct ReadbackFault {
    pub inner: BftPrivateRetentionMemoryBackendV1,
    pub mode: Arc<AtomicUsize>,
    pub input_hold: Arc<AtomicUsize>,
    pub role: BftPrivateRetentionAttestorV1,
}

impl ReadbackFault {
    fn forge(&self, mut original: BftPrivateRetentionSnapshotV1) -> BftPrivateRetentionSnapshotV1 {
        match self.mode.load(Ordering::SeqCst) {
            1 => original.signer.state.state_digest = hash(97),
            2 => {
                original.signer.state.binding_digest = hash(98);
                original.signer.state.state_digest =
                    crate::protected_bft_v3::state::test_state_digest(&original.signer.state);
                original.signer.state.validate_digest().unwrap();
                assert_ne!(
                    original.signer.state.binding_digest,
                    self.role.source.binding_digest
                );
            }
            _ => return original,
        }
        // Re-sign a coherent receipt so rejection is not an invalid signature,
        // changed map, wrong original revision or mismatched post-digest fixture.
        original.retention.receipt.body.after_state_digest =
            original.signer.state.state_digest.clone();
        let bytes = crate::protected_bft_v3::crypto::framed_cbor(
            RECEIPT_DOMAIN,
            &original.retention.receipt.body,
            crate::protected_bft_v3::crypto::BFT_SIGNER_MAX_SIGNING_BYTES_V3,
        )
        .unwrap();
        original.retention.receipt.signature_hex = hex::encode(key(33).sign(&bytes).to_bytes());
        original
            .retention
            .receipt
            .verify_map(&self.role, &original.retention.map)
            .unwrap();
        original
    }
}

macro_rules! forward {
    ($name:ident($($argument:ident: $ty:ty),*) -> $result:ty) => {
        fn $name(&mut self, $($argument: $ty),*) -> $result {
            self.inner.$name($($argument),*)
        }
    };
}

impl BftSignerBackendV3 for ReadbackFault {
    forward!(read_private_retention_creation(generation: u64, token: &str) -> Result<ValidatedBftPrivateRetentionCreationV1, BftSignerErrorV3>);
    forward!(load_provisioned(binding: &str) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3>);
    forward!(acquire_lease(binding: &str, expected: u64, token: &str) -> Result<BftSignerAcquireOutcomeV3, BftSignerErrorV3>);
    forward!(release_lease(generation: u64, token: &str) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3>);
    forward!(read_for_lease(generation: u64, token: &str) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3>);
    forward!(protected_time_ms(generation: u64, token: &str) -> Result<i64, BftSignerErrorV3>);
    forward!(reserve_successor_archive_transfer(generation: u64, token: &str, revision: u64, digest: &str, expected: u64, amount: u64) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3>);
    forward!(compare_and_swap(generation: u64, token: &str, revision: u64, digest: &str, minimum: Option<i64>, next: BftSignerStateV3) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3>);
    fn compare_and_swap_private_retention(
        &mut self,
        request: &BftPrivateRetentionCasV1,
    ) -> Result<BftPrivateRetentionSnapshotV1, BftSignerErrorV3> {
        let old_count = request
            .expected_retention
            .as_ref()
            .map_or(0, |ledger| ledger.map.entries.len());
        if request.after.inputs.len() > old_count {
            assert_eq!(
                self.input_hold.load(Ordering::SeqCst),
                1,
                "original input permit must survive verifier return through actual CAS/readback"
            );
        }
        self.inner.compare_and_swap_private_retention(request)
    }
    forward!(load_private_retention_current(binding: &str) -> Result<BftPrivateRetentionCurrentV1, BftSignerErrorV3>);

    fn read_private_retention_sealed_current(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<BftPrivateRetentionSealedCurrentV1, BftSignerErrorV3> {
        if self.mode.load(Ordering::SeqCst) != 0 {
            // Preserve the existing malformed-original oracle below: its fault
            // is deliberately injected at the original current readback seam.
            return Err(BftSignerErrorV3(
                "complete original retention history unsupported".into(),
            ));
        }
        self.inner
            .read_private_retention_sealed_current(generation, token)
    }

    fn read_private_retention_current(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<BftPrivateRetentionCurrentV1, BftSignerErrorV3> {
        let mut current = self
            .inner
            .read_private_retention_current(generation, token)?;
        if self.mode.load(Ordering::SeqCst) != 0 {
            let ledger = current.retention.as_ref().unwrap();
            assert!(
                current.signer.state.revision > ledger.receipt.body.after_revision,
                "fault must retain an actual later current State"
            );
            let original = self.inner.read_private_retention_operation(
                generation,
                token,
                &ledger.receipt.body.operation_digest,
            )?;
            current.retention = Some(self.forge(original).retention);
        }
        Ok(current)
    }

    fn read_private_retention_operation(
        &mut self,
        generation: u64,
        token: &str,
        operation: &str,
    ) -> Result<BftPrivateRetentionSnapshotV1, BftSignerErrorV3> {
        let original = self
            .inner
            .read_private_retention_operation(generation, token, operation)?;
        Ok(self.forge(original))
    }
}

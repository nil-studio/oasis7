//! One actual backend owner shared by signer service and read-only encrypted
//! retained-input egress. No namespace is reopened and no State is copied here.
use oasis7_distfs::protected_bft_v3::*;
use std::sync::{Arc, Mutex, MutexGuard};
#[derive(Clone)]
pub(crate) struct SharedBftSignerBackendOwnerV3(Arc<Mutex<Box<dyn BftSignerBackendV3>>>);
impl SharedBftSignerBackendOwnerV3 {
    pub(crate) fn new(backend: Box<dyn BftSignerBackendV3>) -> Self {
        Self(Arc::new(Mutex::new(backend)))
    }
    pub(crate) fn service_backend(&self) -> Box<dyn BftSignerBackendV3> {
        Box::new(ServiceBackend(self.clone()))
    }
    pub(crate) fn input_backend(&self) -> Box<dyn BftSignerBackendV3> {
        Box::new(InputBackend(self.clone()))
    }
    fn lock(&self) -> Result<MutexGuard<'_, Box<dyn BftSignerBackendV3>>, BftSignerErrorV3> {
        self.0
            .lock()
            .map_err(|_| BftSignerErrorV3("original backend owner poisoned".into()))
    }
}
struct ServiceBackend(SharedBftSignerBackendOwnerV3);
struct InputBackend(SharedBftSignerBackendOwnerV3);
fn readonly<T>() -> Result<T, BftSignerErrorV3> {
    Err(BftSignerErrorV3(
        "retained input handle cannot mutate or read signer authority".into(),
    ))
}

impl BftSignerBackendV3 for ServiceBackend {
    fn read_private_retention_sealed_current(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<BftPrivateRetentionSealedCurrentV1, BftSignerErrorV3> {
        self.0
            .lock()?
            .read_private_retention_sealed_current(generation, token)
    }
    fn compare_and_swap_private_retention_seal(
        &mut self,
        request: &BftPrivateRetentionSealCasV1,
    ) -> Result<BftPrivateRetentionSnapshotV1, BftSignerErrorV3> {
        self.0
            .lock()?
            .compare_and_swap_private_retention_seal(request)
    }

    fn read_private_retention_creation(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
    ) -> Result<ValidatedBftPrivateRetentionCreationV1, BftSignerErrorV3> {
        self.0
            .lock()?
            .read_private_retention_creation(lease_generation, lease_token_digest)
    }
    fn load_private_retention_current(
        &mut self,
        binding_digest: &str,
    ) -> Result<BftPrivateRetentionCurrentV1, BftSignerErrorV3> {
        self.0
            .lock()?
            .load_private_retention_current(binding_digest)
    }
    fn read_private_retention_current(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
    ) -> Result<BftPrivateRetentionCurrentV1, BftSignerErrorV3> {
        self.0
            .lock()?
            .read_private_retention_current(lease_generation, lease_token_digest)
    }
    fn load_private_retention_input(
        &mut self,
        binding_digest: &str,
        reference: &BftPrivateRetentionAdmissionInputRefV1,
    ) -> Result<Arc<BftPrivateRetentionAdmissionInputV1>, BftSignerErrorV3> {
        self.0
            .lock()?
            .load_private_retention_input(binding_digest, reference)
    }
    fn compare_and_swap_private_retention(
        &mut self,
        request: &BftPrivateRetentionCasV1,
    ) -> Result<BftPrivateRetentionSnapshotV1, BftSignerErrorV3> {
        self.0.lock()?.compare_and_swap_private_retention(request)
    }
    fn read_private_retention_operation(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
        operation_digest: &str,
    ) -> Result<BftPrivateRetentionSnapshotV1, BftSignerErrorV3> {
        self.0.lock()?.read_private_retention_operation(
            lease_generation,
            lease_token_digest,
            operation_digest,
        )
    }
    fn load_provisioned(
        &mut self,
        binding_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.0.lock()?.load_provisioned(binding_digest)
    }
    fn acquire_lease(
        &mut self,
        binding_digest: &str,
        expected_generation: u64,
        new_lease_token_digest: &str,
    ) -> Result<BftSignerAcquireOutcomeV3, BftSignerErrorV3> {
        self.0
            .lock()?
            .acquire_lease(binding_digest, expected_generation, new_lease_token_digest)
    }
    fn release_lease(
        &mut self,
        expected_generation: u64,
        expected_lease_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.0
            .lock()?
            .release_lease(expected_generation, expected_lease_token_digest)
    }
    fn read_for_lease(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.0
            .lock()?
            .read_for_lease(lease_generation, lease_token_digest)
    }
    fn protected_time_ms(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
    ) -> Result<i64, BftSignerErrorV3> {
        self.0
            .lock()?
            .protected_time_ms(lease_generation, lease_token_digest)
    }
    fn reserve_successor_archive_transfer(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
        expected_revision: u64,
        expected_state_digest: &str,
        expected_reserved_response_bytes: u64,
        reserved_response_bytes: u64,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.0.lock()?.reserve_successor_archive_transfer(
            lease_generation,
            lease_token_digest,
            expected_revision,
            expected_state_digest,
            expected_reserved_response_bytes,
            reserved_response_bytes,
        )
    }
    fn compare_and_swap(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
        expected_revision: u64,
        expected_state_digest: &str,
        minimum_event_time_ms: Option<i64>,
        next_state: BftSignerStateV3,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.0.lock()?.compare_and_swap(
            lease_generation,
            lease_token_digest,
            expected_revision,
            expected_state_digest,
            minimum_event_time_ms,
            next_state,
        )
    }
}

impl BftSignerBackendV3 for InputBackend {
    fn read_private_retention_sealed_current(
        &mut self,
        _generation: u64,
        _token: &str,
    ) -> Result<BftPrivateRetentionSealedCurrentV1, BftSignerErrorV3> {
        readonly()
    }
    fn compare_and_swap_private_retention_seal(
        &mut self,
        _request: &BftPrivateRetentionSealCasV1,
    ) -> Result<BftPrivateRetentionSnapshotV1, BftSignerErrorV3> {
        readonly()
    }

    fn read_private_retention_creation(
        &mut self,
        _lease_generation: u64,
        _lease_token_digest: &str,
    ) -> Result<ValidatedBftPrivateRetentionCreationV1, BftSignerErrorV3> {
        readonly()
    }
    fn load_private_retention_current(
        &mut self,
        _binding_digest: &str,
    ) -> Result<BftPrivateRetentionCurrentV1, BftSignerErrorV3> {
        readonly()
    }
    fn read_private_retention_current(
        &mut self,
        _lease_generation: u64,
        _lease_token_digest: &str,
    ) -> Result<BftPrivateRetentionCurrentV1, BftSignerErrorV3> {
        readonly()
    }
    fn load_private_retention_input(
        &mut self,
        binding_digest: &str,
        reference: &BftPrivateRetentionAdmissionInputRefV1,
    ) -> Result<Arc<BftPrivateRetentionAdmissionInputV1>, BftSignerErrorV3> {
        // Drop this short backend-only guard before encoding or network egress.
        self.0
            .0
            .try_lock()
            .map_err(|_| BftSignerErrorV3("original backend input busy or poisoned".into()))?
            .load_private_retention_input(binding_digest, reference)
    }
    fn compare_and_swap_private_retention(
        &mut self,
        _request: &BftPrivateRetentionCasV1,
    ) -> Result<BftPrivateRetentionSnapshotV1, BftSignerErrorV3> {
        readonly()
    }
    fn read_private_retention_operation(
        &mut self,
        _lease_generation: u64,
        _lease_token_digest: &str,
        _operation_digest: &str,
    ) -> Result<BftPrivateRetentionSnapshotV1, BftSignerErrorV3> {
        readonly()
    }
    fn load_provisioned(
        &mut self,
        _binding_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        readonly()
    }
    fn acquire_lease(
        &mut self,
        _binding_digest: &str,
        _expected_generation: u64,
        _new_lease_token_digest: &str,
    ) -> Result<BftSignerAcquireOutcomeV3, BftSignerErrorV3> {
        readonly()
    }
    fn release_lease(
        &mut self,
        _expected_generation: u64,
        _expected_lease_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        readonly()
    }
    fn read_for_lease(
        &mut self,
        _lease_generation: u64,
        _lease_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        readonly()
    }
    fn protected_time_ms(
        &mut self,
        _lease_generation: u64,
        _lease_token_digest: &str,
    ) -> Result<i64, BftSignerErrorV3> {
        readonly()
    }
    fn reserve_successor_archive_transfer(
        &mut self,
        _lease_generation: u64,
        _lease_token_digest: &str,
        _expected_revision: u64,
        _expected_state_digest: &str,
        _expected_reserved_response_bytes: u64,
        _reserved_response_bytes: u64,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        readonly()
    }
    fn compare_and_swap(
        &mut self,
        _lease_generation: u64,
        _lease_token_digest: &str,
        _expected_revision: u64,
        _expected_state_digest: &str,
        _minimum_event_time_ms: Option<i64>,
        _next_state: BftSignerStateV3,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        readonly()
    }
}

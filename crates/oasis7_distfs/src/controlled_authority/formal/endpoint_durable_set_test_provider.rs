//! Local native-install protocol fixture. Keys and monotonic state belong only
//! to this test provider; this is NOT evidence of a physical HSM/fault domain.
use super::*;
use std::sync::Mutex;
#[derive(Clone)]
struct Installed {
    instance: EndpointInstanceBindingV1,
    state: DurableSetProviderStateV1,
    native: Vec<u8>,
    checkpoint: EndpointCheckpointBindingV1,
    pending: Option<CheckpointIntentV1>,
}
pub(super) struct InstallProvider {
    directory: PathBuf,
    key_number: u8,
    resource: String,
    old: Option<Arc<dyn MonotonicEndpointProvider>>,
    state: Arc<Mutex<Option<Installed>>>,
    lease: Arc<AtomicBool>,
    activation_fault: std::sync::atomic::AtomicUsize,
    query_fault: AtomicBool,
    pub(super) activation_calls: std::sync::atomic::AtomicUsize,
    pub(super) query_calls: std::sync::atomic::AtomicUsize,
}
impl InstallProvider {
    pub(super) fn new(
        directory: PathBuf,
        key_number: u8,
        resource: String,
        old: Option<Arc<dyn MonotonicEndpointProvider>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            directory,
            key_number,
            resource,
            old,
            state: Arc::new(Mutex::new(None)),
            lease: Arc::new(AtomicBool::new(false)),
            activation_fault: std::sync::atomic::AtomicUsize::new(0),
            query_fault: AtomicBool::new(false),
            activation_calls: std::sync::atomic::AtomicUsize::new(0),
            query_calls: std::sync::atomic::AtomicUsize::new(0),
        })
    }
    pub(super) fn fail_activation_once(&self, after_commit: bool) {
        self.activation_fault
            .store(if after_commit { 2 } else { 1 }, Ordering::SeqCst);
    }
    pub(super) fn fail_query_once(&self) {
        self.query_fault.store(true, Ordering::SeqCst);
    }
    fn guard(
        &self,
        instance: &EndpointInstanceBindingV1,
    ) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
        let snapshot = self
            .state
            .lock()
            .unwrap()
            .clone()
            .ok_or(FencingError::FencingUnproven)?;
        if &snapshot.instance != instance {
            return Err(FencingError::EpochSealed);
        }
        self.lease
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| FencingError::Locked)?;
        Ok(Box::new(InstallGuard {
            shared: self.state.clone(),
            lease: self.lease.clone(),
            snapshot,
            resource: self.resource.clone(),
            key_number: self.key_number,
        }))
    }
}
fn evidence(
    instance: &EndpointInstanceBindingV1,
    checkpoint: &EndpointCheckpointBindingV1,
    resource: &str,
) -> MonotonicProviderEvidenceV1 {
    let statement = MonotonicProviderStatementV1 {
        schema_version: 1,
        capability: ProviderCapabilityV1::MonotonicExclusiveSignerStorageV1,
        resource_binding: resource.into(),
        instance_digest: instance.digest().unwrap(),
        checkpoint: checkpoint.clone(),
        epoch_floor: instance.authority_epoch,
        seal_digest: None,
    };
    MonotonicProviderEvidenceV1 {
        signature: signed(
            crate::controlled_authority::formal::crypto::bytes(
                "oasis7.endpoint.monotonic-provider.v1",
                &statement,
            )
            .unwrap(),
            3,
        ),
        statement,
    }
}
impl MonotonicEndpointProvider for InstallProvider {
    fn initialize_protected(
        &self,
        instance: &EndpointInstanceBindingV1,
        registration: ProtectedEndpointRegistrationV1,
    ) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
        if self.state.lock().unwrap().is_some() {
            return Err(FencingError::FencingUnproven);
        }
        self.old
            .as_ref()
            .ok_or(FencingError::FencingUnproven)?
            .initialize_protected(instance, registration)
    }
    fn acquire(
        &self,
        instance: &EndpointInstanceBindingV1,
        initial: &EndpointCheckpointBindingV1,
    ) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
        if self.state.lock().unwrap().is_some() {
            let g = self.guard(instance)?;
            if initial.revision > g.checkpoint().revision
                || (initial.revision == g.checkpoint().revision && initial != g.checkpoint())
            {
                return Err(FencingError::Rollback);
            }
            return Ok(g);
        }
        self.old
            .as_ref()
            .ok_or(FencingError::FencingUnproven)?
            .acquire(instance, initial)
    }
    fn install_durable_set(
        &self,
        instance: &EndpointInstanceBindingV1,
        sealed: &VerifiedSealedDurableSetTransitionV1,
        request: DurableSetNativeInstallRequestV1,
    ) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
        let mut durable = self.state.lock().unwrap();
        if let Some(current) = durable.as_ref() {
            if current.state.transition_digest != sealed.transition().digest()
                || &current.instance != instance
            {
                return Err(FencingError::FencingUnproven);
            }
        } else {
            // P consumes the SAME provider's old sealed floor. The old instance
            // is permanently rejected after the CAS below; new R has no old key.
            if instance.endpoint_role == EndpointRole::Primary {
                let old = self.old.as_ref().ok_or(FencingError::FencingUnproven)?;
                let old_instance = &sealed.evidence().primary_seal.body.instance;
                let old_cp = request
                    .expected_old_checkpoint
                    .as_ref()
                    .ok_or(FencingError::FencingUnproven)?;
                let old_guard = old.acquire(old_instance, old_cp)?;
                if old_guard.checkpoint() != old_cp
                    || old_guard.seal() != Some(&sealed.evidence().primary_seal.body)
                {
                    return Err(FencingError::FencingUnproven);
                }
            }
            let plan = verify_durable_set_native_install_plan(
                &self.directory,
                instance,
                sealed,
                &request,
                64 * 1024 * 1024,
            )
            .map_err(|e| FencingError::Invalid(e.to_string()))?;
            let native = plan
                .signed_checkpoint(
                    &signed(plan.checkpoint_signing_preimage().to_vec(), self.key_number),
                    instance,
                )
                .map_err(|e| FencingError::Invalid(e.to_string()))?;
            let checkpoint = Store::binding(plan.body(), &native);
            let body = DurableSetInstallBodyV1 {
                schema_version: 1,
                transition_digest: sealed.transition().digest().into(),
                successor_authority_id: sealed
                    .transition()
                    .body()
                    .successor
                    .authority_id()
                    .unwrap(),
                instance: instance.clone(),
                boundary: sealed.transition().boundary().head(),
                independent_closure_root: sealed
                    .transition()
                    .body()
                    .independent_closure_root
                    .clone(),
                independent_inventory_digest: plan.inventory_digest().into(),
                checkpoint: checkpoint.clone(),
            };
            let receipt = DurableSetInstallReceiptV1 {
                endpoint_signature_hex: signed(
                    durable_set_install_endpoint_signing_bytes(&body).unwrap(),
                    self.key_number,
                ),
                provider_signature_hex: signed(
                    durable_set_install_provider_signing_bytes(&body).unwrap(),
                    3,
                ),
                provider_evidence: evidence(instance, &checkpoint, &self.resource),
                body,
            };
            *durable = Some(Installed {
                instance: instance.clone(),
                state: DurableSetProviderStateV1 {
                    transition_digest: sealed.transition().digest().into(),
                    successor_authority_id: sealed
                        .transition()
                        .body()
                        .successor
                        .authority_id()
                        .unwrap(),
                    install_receipt: receipt,
                    activation_digest: None,
                },
                native,
                checkpoint,
                pending: None,
            });
        }
        drop(durable);
        self.guard(instance)
    }
    fn query_durable_set_install(
        &self,
        instance: &EndpointInstanceBindingV1,
        sealed: &VerifiedSealedDurableSetTransitionV1,
    ) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
        self.query_calls.fetch_add(1, Ordering::SeqCst);
        if self.query_fault.swap(false, Ordering::SeqCst) {
            return Err(FencingError::Rollback);
        }
        let guard = self.guard(instance)?;
        if guard.durable_set_state().unwrap().transition_digest != sealed.transition().digest() {
            return Err(FencingError::FencingUnproven);
        }
        Ok(guard)
    }
    fn activate_durable_set(
        &self,
        instance: &EndpointInstanceBindingV1,
        sealed: &VerifiedSealedDurableSetTransitionV1,
        activation: &VerifiedDurableSetActivationV1,
        request: DurableSetNativeActivationRequestV1,
    ) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
        self.activation_calls.fetch_add(1, Ordering::SeqCst);
        let fault = self.activation_fault.swap(0, Ordering::SeqCst);
        if fault == 1 {
            return Err(FencingError::Io(
                "injected before provider activation".into(),
            ));
        }
        let checked = verify_durable_set_activation(
            sealed.transition(),
            [
                &sealed.evidence().primary_seal,
                &sealed.evidence().replica_seal,
            ],
            [&request.primary_install, &request.replica_install],
        )
        .map_err(|e| FencingError::Invalid(e.to_string()))?;
        if checked.activation_digest() != activation.activation_digest() {
            return Err(FencingError::FencingUnproven);
        }
        let plan = verify_durable_set_native_install_plan(
            &self.directory,
            instance,
            sealed,
            &request.install,
            64 * 1024 * 1024,
        )
        .map_err(|e| FencingError::Invalid(e.to_string()))?;
        let mut state = self.state.lock().unwrap();
        let current = state.as_mut().ok_or(FencingError::FencingUnproven)?;
        if &current.instance != instance
            || current.state.transition_digest != sealed.transition().digest()
        {
            return Err(FencingError::FencingUnproven);
        }
        if let Some(digest) = &current.state.activation_digest {
            if digest != activation.activation_digest() {
                return Err(FencingError::FencingUnproven);
            }
        } else {
            let active = plan
                .activation_plan(activation)
                .map_err(|e| FencingError::Invalid(e.to_string()))?;
            current.native = active
                .signed_checkpoint(
                    &signed(
                        active.checkpoint_signing_preimage().to_vec(),
                        self.key_number,
                    ),
                    instance,
                )
                .map_err(|e| FencingError::Invalid(e.to_string()))?;
            current.checkpoint = Store::binding(active.body(), &current.native);
            current.state.activation_digest = Some(activation.activation_digest().into());
        }
        drop(state);
        let guard = self.guard(instance)?;
        if fault == 2 {
            // The provider committed, but the owner never received its lease or
            // active checkpoint. Recovery must independently query the provider.
            drop(guard);
            return Err(FencingError::Io(
                "injected lost active provider reply".into(),
            ));
        }
        Ok(guard)
    }
}
struct InstallGuard {
    shared: Arc<Mutex<Option<Installed>>>,
    lease: Arc<AtomicBool>,
    snapshot: Installed,
    resource: String,
    key_number: u8,
}
impl Drop for InstallGuard {
    fn drop(&mut self) {
        self.lease.store(false, Ordering::SeqCst);
    }
}
impl MonotonicEndpointGuard for InstallGuard {
    fn durable_set_state(&self) -> Option<&DurableSetProviderStateV1> {
        Some(&self.snapshot.state)
    }
    fn protected_native_checkpoint(&self) -> Option<&[u8]> {
        Some(&self.snapshot.native)
    }
    fn checkpoint(&self) -> &EndpointCheckpointBindingV1 {
        &self.snapshot.checkpoint
    }
    fn pending(&self) -> Option<&CheckpointIntentV1> {
        self.snapshot.pending.as_ref()
    }
    fn seal(&self) -> Option<&EpochSealBodyV1> {
        None
    }
    fn evidence(&self) -> Result<MonotonicProviderEvidenceV1, FencingError> {
        if self.snapshot.pending.is_some() {
            return Err(FencingError::FencingUnproven);
        }
        Ok(evidence(
            &self.snapshot.instance,
            &self.snapshot.checkpoint,
            &self.resource,
        ))
    }
    fn sign_endpoint(
        &mut self,
        request: ProtectedEndpointSigningRequestV1,
    ) -> Result<ProtectedEndpointSignatureV1, FencingError> {
        request.validate()?;
        if self.snapshot.state.activation_digest.is_none()
            || self.snapshot.pending.is_some()
            || request.instance_digest != self.snapshot.instance.digest()?
            || request.checkpoint != self.snapshot.checkpoint
        {
            return Err(FencingError::FencingUnproven);
        }
        let body = ProtectedEndpointSignatureBodyV1 {
            schema_version: 1,
            custody: ProtectedSignerCustodyV1::NonexportableLeaseBoundV1,
            resource_binding: self.resource.clone(),
            instance_digest: request.instance_digest.clone(),
            checkpoint: request.checkpoint.clone(),
            request_digest: request.digest()?,
            challenge: request.challenge.clone(),
            endpoint_signature: signed(request.preimage.clone(), self.key_number),
        };
        Ok(ProtectedEndpointSignatureV1 {
            provider_signature: signed(protected_endpoint_signature_signing_bytes(&body)?, 3),
            body,
        })
    }
    fn begin(&mut self, intent: CheckpointIntentV1) -> Result<(), FencingError> {
        if self.snapshot.state.activation_digest.is_none()
            || self.snapshot.pending.is_some()
            || intent.previous != self.snapshot.checkpoint
            || intent.next.revision
                != intent
                    .previous
                    .revision
                    .checked_add(1)
                    .ok_or(FencingError::Rollback)?
        {
            return Err(FencingError::FencingUnproven);
        }
        let body = Store::verify_public(
            &intent.checkpoint_bytes,
            &self.snapshot.instance.endpoint_public_key,
        )
        .map_err(|e| FencingError::Invalid(e.to_string()))?;
        let before = Store::verify_public(
            &self.snapshot.native,
            &self.snapshot.instance.endpoint_public_key,
        )
        .unwrap();
        if Store::binding(&body, &intent.checkpoint_bytes) != intent.next
            || body.durable_set != before.durable_set
            || body.instance_digest != before.instance_digest
            || body.authority_id != before.authority_id
        {
            return Err(FencingError::FencingUnproven);
        }
        self.snapshot.pending = Some(intent);
        *self.shared.lock().unwrap() = Some(self.snapshot.clone());
        Ok(())
    }
    fn finish(&mut self) -> Result<(), FencingError> {
        let intent = self
            .snapshot
            .pending
            .take()
            .ok_or(FencingError::FencingUnproven)?;
        self.snapshot.checkpoint = intent.next;
        self.snapshot.native = intent.checkpoint_bytes;
        *self.shared.lock().unwrap() = Some(self.snapshot.clone());
        Ok(())
    }
}

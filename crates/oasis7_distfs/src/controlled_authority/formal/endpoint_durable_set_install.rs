//! Candidate owners cannot issue formal durability receipts. Their only path to
//! a writable endpoint consumes a fully verified dual-install activation.
use super::*;
use crate::controlled_authority::fencing::EndpointInstanceBindingDirectoryExt;
use crate::controlled_authority::formal::fencing::{
    DurableSetNativeActivationRequestV1, DurableSetNativeInstallRequestV1,
};
use std::fs::File;
use std::path::PathBuf;
use store::durable_set::copy_authenticated_closure;

struct CandidateTiming {
    phase: &'static str,
    phase_started: Option<std::time::Instant>,
    started: Option<std::time::Instant>,
}
impl CandidateTiming {
    fn new() -> Self {
        let started = std::env::var_os("OASIS7_HEALTHY_NATIVE_TRACE")
            .is_some_and(|value| value == "1")
            .then(std::time::Instant::now);
        Self {
            phase: "verify_transfer",
            phase_started: started,
            started,
        }
    }
    fn report(&self) {
        if let Some(started) = self.phase_started {
            eprintln!(
                "healthy-native-staging phase={} elapsed_us={}",
                self.phase,
                started.elapsed().as_micros(),
            );
        }
    }
    fn next(&mut self, phase: &'static str) {
        self.report();
        self.phase = phase;
        self.phase_started = self.started.map(|_| std::time::Instant::now());
    }
}
impl Drop for CandidateTiming {
    fn drop(&mut self) {
        self.report();
        if let Some(started) = self.started {
            eprintln!(
                "healthy-native-staging total_us={}",
                started.elapsed().as_micros(),
            );
        }
    }
}

enum Backend {
    Primary(Box<FileFormalEndpoint>),
    Replica(File),
}
pub struct DurableSetCandidateV1 {
    path: PathBuf,
    sealed: Arc<VerifiedSealedDurableSetTransitionV1>,
    provider: Arc<dyn fencing::MonotonicEndpointProvider>,
    backend: Backend,
    role: EndpointRole,
    initial: HotBody,
    request: DurableSetNativeInstallRequestV1,
    installed: Option<DurableSetInstallReceiptV1>,
}
#[expect(
    clippy::too_many_arguments,
    reason = "role, custody, source, closure budget and owner are independently bound"
)]
fn make_candidate(
    path: &Path,
    sealed: Arc<VerifiedSealedDurableSetTransitionV1>,
    manifest: SignedFormalArchiveTransferV1,
    source: Arc<dyn FormalArchiveSourceV1>,
    provider: Arc<dyn fencing::MonotonicEndpointProvider>,
    backend: Backend,
    role: EndpointRole,
    budget: u64,
) -> Result<DurableSetCandidateV1, FormalError> {
    let mut timing = CandidateTiming::new();
    let transition = sealed.transition();
    let old = transition.predecessor_authority();
    let transfer = verify_formal_archive_transfer(&manifest, old, source.clone())?;
    let source_body = Store::verify_public(
        &manifest.checkpoint_bytes,
        &manifest.seal.body.instance.endpoint_public_key,
    )?;
    if transfer.cut() != &transition.boundary().head()
        || transfer.operation_id != transition.digest()
        || transfer.next_epoch_floor != transition.body().successor.epoch
        || transfer.tail != fencing::SealedTailV1::empty()
        || ![
            &sealed.evidence().primary_seal,
            &sealed.evidence().replica_seal,
        ]
        .iter()
        .any(|seal| {
            seal.body == manifest.seal.body
                && seal.provider.statement == manifest.seal.provider.statement
        })
    {
        return Err(invalid(
            "healthy candidate source exact authorized sealed roots",
        ));
    }
    let instance = match role {
        EndpointRole::Primary => &transition.body().successor.primary.instance,
        EndpointRole::Replica => &transition.body().successor.replica.instance,
    };
    instance.verify_directory(path).map_err(fencing_error)?;
    timing.next("copy_closure");
    let inventory = copy_authenticated_closure(
        path,
        &manifest.checkpoint_bytes,
        &manifest.seal.body.instance.endpoint_public_key,
        source.as_ref(),
        old.definition().profile_version,
        budget,
        true,
    )?;
    timing.next("bind_boundary");
    let node_id = transfer
        .head_node
        .as_ref()
        .ok_or_else(|| invalid("healthy candidate missing H node"))?;
    let node: HistoryNode = serde_cbor::from_slice(
        &source
            .read_object(node_id)?
            .ok_or_else(|| invalid("healthy H history missing"))?,
    )
    .map_err(|e| invalid(e.to_string()))?;
    let read_entry = |id: &str| -> Result<FormalQualifiedCommitProofV1, FormalError> {
        let entry: FormalRequestStateV1 = serde_cbor::from_slice(
            &source
                .read_object(id)?
                .ok_or_else(|| invalid("healthy source qualified entry missing"))?,
        )
        .map_err(|e| invalid(e.to_string()))?;
        let FormalRequestStateV1::Qualified { proof, .. } = entry else {
            return Err(invalid("healthy source entry is not Qualified"));
        };
        Ok(proof)
    };
    let boundary_proof = read_entry(&node.entry)?;
    if boundary_proof.decision.record != *transition.boundary().record() {
        return Err(invalid("healthy source boundary record mismatch"));
    }
    let predecessor_link = node
        .ancestors
        .first()
        .ok_or_else(|| invalid("healthy H predecessor link absent"))?;
    if predecessor_link.head != transition.body().predecessor {
        return Err(invalid("healthy exact original predecessor"));
    }
    let parent_node: HistoryNode = serde_cbor::from_slice(
        &source
            .read_object(&predecessor_link.node)?
            .ok_or_else(|| invalid("healthy original predecessor node missing"))?,
    )
    .map_err(|e| invalid(e.to_string()))?;
    if parent_node.head != predecessor_link.head {
        return Err(invalid("healthy predecessor authenticated node"));
    }
    let predecessor_proof = read_entry(&parent_node.entry)?;
    if predecessor_proof.decision.record.record_id()? != transition.body().predecessor.record_id {
        return Err(invalid("healthy predecessor proof canonical hash"));
    }
    timing.next("bind_install");
    let revision = match role {
        EndpointRole::Primary => sealed
            .evidence()
            .primary_seal
            .provider
            .statement
            .checkpoint
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("healthy P install revision overflow"))?,
        EndpointRole::Replica => 1,
    };
    let initial = HotBody {
        version: 1,
        instance_digest: instance.digest().map_err(fencing_error)?,
        authority_id: transition.body().successor.authority_id()?,
        activation_ref: old.activation_ref().into(),
        epoch: transition.body().successor.epoch,
        revision,
        head: transition.boundary().head(),
        head_node: None,
        imported_head_node: transfer.head_node.clone(),
        import_digest: Some(transition.digest().into()),
        base_parent: Some(CheckpointParent {
            record: transition.boundary().record().clone(),
            finalizing_authority_id: transition.boundary().finalizing_authority_id().into(),
        }),
        index: source_body.index,
        pending: None,
        decision_head: None,
        seal: None,
        durable_set: Some(fencing::DurableSetNativeBindingV1 {
            transition_digest: transition.digest().into(),
            independent_inventory_digest: inventory.inventory_digest.clone(),
            activation_digest: None,
        }),
    };
    timing.next("persist_manifest");
    let manifest_bytes = serde_cbor::to_vec(&manifest).map_err(|e| invalid(e.to_string()))?;
    Store::persist_candidate_manifest(path, transition.digest(), &manifest_bytes)?;
    timing.next("install_request");
    let request = DurableSetNativeInstallRequestV1 {
        schema_version: 1,
        authorization: sealed.evidence().clone(),
        predecessor_proof,
        boundary_proof,
        source_checkpoint_bytes: manifest.checkpoint_bytes,
        local_closure_manifest_digest: inventory.inventory_digest,
        registration: fencing::ProtectedEndpointRegistrationV1 {
            authority_id: initial.authority_id.clone(),
            activation_ref: initial.activation_ref.clone(),
            unsigned_checkpoint_preimage: Store::checkpoint_signing_bytes(&initial)?,
            existing_checkpoint: None,
        },
        expected_old_checkpoint: (role == EndpointRole::Primary).then(|| {
            sealed
                .evidence()
                .primary_seal
                .provider
                .statement
                .checkpoint
                .clone()
        }),
    };
    Ok(DurableSetCandidateV1 {
        path: path.to_path_buf(),
        sealed,
        provider,
        backend,
        role,
        initial,
        request,
        installed: None,
    })
}
impl FileFormalEndpoint {
    pub fn into_durable_set_primary_candidate(
        mut self,
        sealed: Arc<VerifiedSealedDurableSetTransitionV1>,
        operation_byte_budget: u64,
    ) -> Result<DurableSetCandidateV1, FormalError> {
        if self.role != EndpointRole::Primary {
            return Err(invalid("healthy P rebind requires primary owner"));
        }
        self.query_durable_set(sealed.transition())?
            .ok_or_else(|| invalid("healthy P not sealed"))?;
        let manifest = self.archive_manifest()?;
        let policy = self.signing_policy();
        let seal = fencing::verify_epoch_seal(&manifest.seal, &policy).map_err(fencing_error)?;
        let transfer = self.export_sealed_archive(&seal)?;
        let path = self.store.path.clone();
        let provider = self.provider.clone();
        make_candidate(
            &path,
            sealed,
            manifest,
            transfer.source,
            provider,
            Backend::Primary(Box::new(self)),
            EndpointRole::Primary,
            operation_byte_budget,
        )
    }
}
impl DurableSetCandidateV1 {
    pub fn has_staged_manifest(
        path: &Path,
        sealed: &VerifiedSealedDurableSetTransitionV1,
    ) -> Result<bool, FormalError> {
        Store::has_candidate_manifest(path, sealed.transition().digest())
    }
    /// Disk evidence is only a proposal to recover activation. A fresh protected
    /// provider query must still prove Active before any ordinary listener opens.
    pub fn retained_activation_with_receipts(
        path: &Path,
        sealed: &VerifiedSealedDurableSetTransitionV1,
    ) -> Result<
        Option<(
            VerifiedDurableSetActivationV1,
            [DurableSetInstallReceiptV1; 2],
        )>,
        FormalError,
    > {
        Store::retained_activation_receipts(path, sealed.transition().digest())?
            .map(|receipts| {
                let activation = verify_durable_set_activation(
                    sealed.transition(),
                    [
                        &sealed.evidence().primary_seal,
                        &sealed.evidence().replica_seal,
                    ],
                    [&receipts[0], &receipts[1]],
                )?;
                Ok((activation, receipts))
            })
            .transpose()
    }
    pub fn retained_activation(
        path: &Path,
        sealed: &VerifiedSealedDurableSetTransitionV1,
    ) -> Result<Option<VerifiedDurableSetActivationV1>, FormalError> {
        Store::retained_activation_receipts(path, sealed.transition().digest())?
            .map(|receipts| {
                verify_durable_set_activation(
                    sealed.transition(),
                    [
                        &sealed.evidence().primary_seal,
                        &sealed.evidence().replica_seal,
                    ],
                    [&receipts[0], &receipts[1]],
                )
            })
            .transpose()
    }

    /// Recover the SAME staged operation after an uncertain install/activation.
    /// The raw descriptor is only an untrusted locator: dual seals and every
    /// referenced local object are revalidated against the supplied opaque token.
    pub fn reopen(
        path: &Path,
        sealed: Arc<VerifiedSealedDurableSetTransitionV1>,
        role: EndpointRole,
        provider: Arc<dyn fencing::MonotonicEndpointProvider>,
        operation_byte_budget: u64,
    ) -> Result<Self, FormalError> {
        let lock = Store::candidate_lock(path)?;
        let bytes = Store::read_candidate_manifest(path, sealed.transition().digest())?;
        let manifest: SignedFormalArchiveTransferV1 =
            serde_cbor::from_slice(&bytes).map_err(|e| invalid(e.to_string()))?;
        let source = Arc::new(FileFormalArchiveSourceV1::new(path.to_path_buf())?);
        make_candidate(
            path,
            sealed,
            manifest,
            source,
            provider,
            Backend::Replica(lock),
            role,
            operation_byte_budget,
        )
    }

    pub fn stage_replica(
        path: &Path,
        sealed: Arc<VerifiedSealedDurableSetTransitionV1>,
        manifest: SignedFormalArchiveTransferV1,
        source: Arc<dyn FormalArchiveSourceV1>,
        provider: Arc<dyn fencing::MonotonicEndpointProvider>,
        operation_byte_budget: u64,
    ) -> Result<Self, FormalError> {
        let lock = Store::candidate_lock(path)?;
        make_candidate(
            path,
            sealed,
            manifest,
            source,
            provider,
            Backend::Replica(lock),
            EndpointRole::Replica,
            operation_byte_budget,
        )
    }
    fn instance(&self) -> &fencing::EndpointInstanceBindingV1 {
        match self.role {
            EndpointRole::Primary => &self.sealed.transition().body().successor.primary.instance,
            EndpointRole::Replica => &self.sealed.transition().body().successor.replica.instance,
        }
    }
    pub fn install(&mut self) -> Result<DurableSetInstallReceiptV1, FormalError> {
        let guard = self
            .provider
            .install_durable_set(self.instance(), &self.sealed, self.request.clone())
            .map_err(fencing_error)?;
        let state = guard
            .durable_set_state()
            .ok_or(FormalError::FencingUnproven)?;
        verify_one_durable_set_install(&self.sealed, self.role, &state.install_receipt)?;
        if state.install_receipt.body.independent_inventory_digest
            != self.request.local_closure_manifest_digest
        {
            return Err(invalid(
                "healthy installed inventory differs from local readback",
            ));
        }
        if state.install_receipt.body.independent_inventory_digest
            != self.request.local_closure_manifest_digest
        {
            return Err(invalid("healthy queried install inventory mismatch"));
        }
        let receipt = state.install_receipt.clone();
        self.installed = Some(receipt.clone());
        Ok(receipt)
    }
    pub fn query_install(&mut self) -> Result<Option<DurableSetInstallReceiptV1>, FormalError> {
        let guard = self
            .provider
            .query_durable_set_install(self.instance(), &self.sealed)
            .map_err(fencing_error)?;
        let state = guard
            .durable_set_state()
            .ok_or(FormalError::FencingUnproven)?;
        verify_one_durable_set_install(&self.sealed, self.role, &state.install_receipt)?;
        if state.install_receipt.body.independent_inventory_digest
            != self.request.local_closure_manifest_digest
        {
            return Err(invalid("healthy queried install inventory mismatch"));
        }
        let receipt = state.install_receipt.clone();
        self.installed = Some(receipt.clone());
        Ok(Some(receipt))
    }
    pub fn activate(
        self,
        activation: &VerifiedDurableSetActivationV1,
        primary_install: DurableSetInstallReceiptV1,
        replica_install: DurableSetInstallReceiptV1,
    ) -> Result<FileFormalEndpoint, FormalError> {
        let checked = verify_durable_set_activation(
            self.sealed.transition(),
            [
                &self.sealed.evidence().primary_seal,
                &self.sealed.evidence().replica_seal,
            ],
            [&primary_install, &replica_install],
        )?;
        if checked.activation_digest() != activation.activation_digest() {
            return Err(invalid("healthy native activation exact dual evidence"));
        }
        Store::persist_activation_receipts(
            &self.path,
            self.sealed.transition().digest(),
            &[primary_install.clone(), replica_install.clone()],
        )?;
        let request = DurableSetNativeActivationRequestV1 {
            schema_version: 1,
            install: self.request.clone(),
            primary_install,
            replica_install,
        };
        let guard = self
            .provider
            .activate_durable_set(self.instance(), &self.sealed, activation, request)
            .map_err(fencing_error)?;
        let state = guard
            .durable_set_state()
            .ok_or(FormalError::FencingUnproven)?;
        if state.activation_digest.as_deref() != Some(activation.activation_digest()) {
            return Err(FormalError::FencingUnproven);
        }
        let public_key = self.instance().endpoint_public_key.clone();
        let bytes = guard
            .protected_native_checkpoint()
            .ok_or(FormalError::FencingUnproven)?
            .to_vec();
        let body = Store::verify_public(&bytes, &public_key)?;
        let mut expected = self.initial.clone();
        expected.revision = body.revision;
        expected
            .durable_set
            .as_mut()
            .expect("candidate binding")
            .activation_digest = Some(activation.activation_digest().into());
        if body != expected
            || Store::binding(&body, &bytes) != *guard.checkpoint()
            || body.revision <= state.install_receipt.body.checkpoint.revision
        {
            return Err(invalid(
                "healthy active native checkpoint exact installed roots/forward floor",
            ));
        }
        let store = match self.backend {
            Backend::Primary(old) => old.store.adopt_installed(body, bytes, &public_key)?,
            Backend::Replica(lock) => Store::from_candidate_lock(
                &self.path,
                lock,
                body,
                bytes,
                &public_key,
                &self.request.source_checkpoint_bytes,
            )?,
        };
        drop(guard);
        let mut endpoint = FileFormalEndpoint {
            store,
            authority: activation.successor().clone(),
            original: activation.predecessor().clone(),
            recovery: None,
            role: self.role,
            key: None,
            public_key,
            provider: self.provider,
            parent: Some(activation.boundary().clone()),
            poisoned: false,
            native_environment_jobs: std::collections::BTreeMap::new(),
            published_closure_jobs: std::collections::BTreeMap::new(),
            #[cfg(test)]
            controlled: false,
        };
        let _guard = endpoint.guard()?;
        endpoint.reload_parent(&_guard.evidence().map_err(fencing_error)?)?;
        endpoint.verify_business_checkpoint()?;
        Ok(endpoint)
    }
}
impl FileFormalEndpoint {
    /// Reopens an already activated healthy epoch without re-installing H or
    /// rewinding a later head. The provider's current floor and native checkpoint
    /// are revalidated; an unfinished normal checkpoint uses existing CAS intent
    /// reconciliation. The staging descriptor never supplies authority.
    pub fn open_durable_set_active(
        path: &Path,
        sealed: Arc<VerifiedSealedDurableSetTransitionV1>,
        activation: &VerifiedDurableSetActivationV1,
        role: EndpointRole,
        provider: Arc<dyn fencing::MonotonicEndpointProvider>,
        operation_byte_budget: u64,
    ) -> Result<Self, FormalError> {
        let candidate =
            DurableSetCandidateV1::reopen(path, sealed, role, provider, operation_byte_budget)?;
        if candidate.sealed.transition().digest() != activation.transition_digest()
            || candidate.sealed.transition().boundary().head() != activation.boundary().head()
            || candidate
                .sealed
                .transition()
                .predecessor_authority()
                .context_digest()?
                != activation.predecessor().context_digest()?
        {
            return Err(invalid("healthy active open exact opaque activation"));
        }
        let mut guard = candidate
            .provider
            .query_durable_set_install(candidate.instance(), &candidate.sealed)
            .map_err(fencing_error)?;
        let state = guard
            .durable_set_state()
            .ok_or(FormalError::FencingUnproven)?;
        verify_one_durable_set_install(&candidate.sealed, role, &state.install_receipt)?;
        if state.activation_digest.as_deref() != Some(activation.activation_digest())
            || state.install_receipt.body.independent_inventory_digest
                != candidate.request.local_closure_manifest_digest
        {
            return Err(FormalError::FencingUnproven);
        }
        let bytes = guard
            .protected_native_checkpoint()
            .ok_or(FormalError::FencingUnproven)?
            .to_vec();
        let public_key = candidate.instance().endpoint_public_key.clone();
        let body = Store::verify_public(&bytes, &public_key)?;
        let expected_binding = fencing::DurableSetNativeBindingV1 {
            transition_digest: activation.transition_digest().into(),
            independent_inventory_digest: candidate.request.local_closure_manifest_digest.clone(),
            activation_digest: Some(activation.activation_digest().into()),
        };
        if body.durable_set.as_ref() != Some(&expected_binding)
            || body.authority_id != activation.successor().authority_id()?
            || body.instance_digest != candidate.instance().digest().map_err(fencing_error)?
            || body.epoch != activation.successor().definition().epoch
            || body.activation_ref != activation.successor().activation_ref()
            || Store::binding(&body, &bytes) != *guard.checkpoint()
        {
            return Err(invalid("healthy active open protected current checkpoint"));
        }
        let Backend::Replica(lock) = candidate.backend else {
            unreachable!("reopen owns candidate lock")
        };
        let (body, bytes) = if let Some(intent) = guard.pending() {
            match std::fs::symlink_metadata(path.join("formal-checkpoint-v1.cbor")) {
                Ok(_) => {
                    let existing = Store::read_native_checkpoint(path)?;
                    let existing_body = Store::verify_public(&existing, &public_key)?;
                    let binding = Store::binding(&existing_body, &existing);
                    if binding == intent.next {
                        (existing_body, existing)
                    } else if binding == intent.previous {
                        (body, bytes)
                    } else {
                        return Err(invalid(
                            "healthy reopen native file outside protected pending CAS",
                        ));
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (body, bytes),
                Err(error) => return Err(FormalError::Io(error.to_string())),
            }
        } else {
            (body, bytes)
        };
        let mut store = Store::from_candidate_lock(
            path,
            lock,
            body,
            bytes,
            &public_key,
            &candidate.request.source_checkpoint_bytes,
        )?;
        store.reconcile_public(guard.as_mut(), &public_key)?;
        store.verify_minimum(&activation.boundary().head())?;
        drop(guard);
        let mut endpoint = Self {
            store,
            authority: activation.successor().clone(),
            original: activation.successor().clone(),
            recovery: None,
            role,
            key: None,
            public_key,
            provider: candidate.provider,
            parent: Some(activation.boundary().clone()),
            poisoned: false,
            native_environment_jobs: std::collections::BTreeMap::new(),
            published_closure_jobs: std::collections::BTreeMap::new(),
            #[cfg(test)]
            controlled: false,
        };
        let _guard = endpoint.guard()?;
        endpoint.reload_parent(&_guard.evidence().map_err(fencing_error)?)?;
        endpoint.verify_business_checkpoint()?;
        Ok(endpoint)
    }
}

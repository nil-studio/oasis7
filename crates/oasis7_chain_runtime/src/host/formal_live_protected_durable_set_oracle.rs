//! Actual loopback oracle command fixture, explicitly simulated custody/floor.
//! Parent provisioning consumes an independently C0-replayed opaque permission.
//! Incoming requests never register authority, endpoint keys or storage paths.
use super::*;
use formal::{
    DurableSetInstallBodyV1, DurableSetInstallReceiptV1, VerifiedSealedDurableSetTransitionV1,
};
use std::sync::Mutex;

pub(in super::super) struct HealthyProvisionHandle {
    approval: Arc<Mutex<Option<Arc<VerifiedSealedDurableSetTransitionV1>>>>,
    original: VerifiedAuthorityV1,
    role: EndpointRole,
    directory: PathBuf,
    signer_public: String,
}
impl HealthyProvisionHandle {
    pub(in super::super) fn provision(
        &self,
        sealed: Arc<VerifiedSealedDurableSetTransitionV1>,
    ) -> Result<(), String> {
        let old = sealed.transition().predecessor_authority();
        let successor = &sealed.transition().body().successor;
        let new_instance = match self.role {
            EndpointRole::Primary => &successor.primary.instance,
            EndpointRole::Replica => &successor.replica.instance,
        };
        if old.context_digest().map_err(|e| e.to_string())?
            != self.original.context_digest().map_err(|e| e.to_string())?
            || new_instance.endpoint_public_key != self.signer_public
            || old.policy().fencing_provider_public_key != public(3)
        {
            return Err("healthy oracle independent registration context/signer mismatch".into());
        }
        new_instance
            .verify_directory(&self.directory)
            .map_err(|e| e.to_string())?;
        let mut approval = self
            .approval
            .lock()
            .map_err(|_| "healthy provisioning poisoned")?;
        if let Some(previous) = approval.as_ref() {
            if previous.transition().digest() != sealed.transition().digest()
                || previous
                    .transition()
                    .predecessor_authority()
                    .context_digest()
                    .map_err(|e| e.to_string())?
                    != old.context_digest().map_err(|e| e.to_string())?
            {
                return Err("healthy oracle original operation cannot be replaced".into());
            }
        } else {
            *approval = Some(sealed);
        }
        Ok(())
    }
}
struct HealthyHook {
    approval: Arc<Mutex<Option<Arc<VerifiedSealedDurableSetTransitionV1>>>>,
    original: VerifiedAuthorityV1,
    role: EndpointRole,
    directory: PathBuf,
    installed: Option<DurableSetProviderStateV1>,
    install_request_digest: Option<String>,
}
fn digest<T: Serialize>(value: &T) -> Result<String, ()> {
    Ok(blake3::hash(&serde_cbor::to_vec(value).map_err(|_| ())?)
        .to_hex()
        .to_string())
}
fn evidence(
    instance: &EndpointInstanceBindingV1,
    checkpoint: &EndpointCheckpointBindingV1,
    resource: &str,
) -> Result<MonotonicProviderEvidenceV1, ()> {
    let statement = MonotonicProviderStatementV1 {
        schema_version: 1,
        capability: ProviderCapabilityV1::MonotonicExclusiveSignerStorageV1,
        resource_binding: resource.into(),
        instance_digest: instance.digest().map_err(|_| ())?,
        checkpoint: checkpoint.clone(),
        epoch_floor: instance.authority_epoch,
        seal_digest: None,
    };
    let signature = hex::encode(
        key(3)
            .sign(&super::super::crypto::bytes(
                "oasis7.endpoint.monotonic-provider.v1",
                &statement,
            )?)
            .to_bytes(),
    );
    Ok(MonotonicProviderEvidenceV1 {
        statement,
        signature,
    })
}
impl oracle::DurableSetOracleHook for HealthyHook {
    fn accepts_instance(
        &self,
        requested: &EndpointInstanceBindingV1,
        current: &EndpointInstanceBindingV1,
        command: &MonotonicOracleCommandV1,
    ) -> bool {
        // After install even read/sign requests from the retired epoch fail.
        if self.installed.is_some() {
            return requested == current;
        }
        if self.role == EndpointRole::Primary && requested == current {
            return !matches!(
                command,
                MonotonicOracleCommandV1::InstallDurableSet(_)
                    | MonotonicOracleCommandV1::QueryDurableSetInstall(_)
                    | MonotonicOracleCommandV1::ActivateDurableSet(_)
            );
        }
        let Ok(approval) = self.approval.lock() else {
            return false;
        };
        let Some(sealed) = approval.as_ref() else {
            return false;
        };
        let expected = match self.role {
            EndpointRole::Primary => &sealed.transition().body().successor.primary.instance,
            EndpointRole::Replica => &sealed.transition().body().successor.replica.instance,
        };
        requested == expected
            && matches!(
                command,
                MonotonicOracleCommandV1::InstallDurableSet(_)
                    | MonotonicOracleCommandV1::QueryDurableSetInstall(_)
                    | MonotonicOracleCommandV1::ActivateDurableSet(_)
            )
    }
    fn normal_allowed(&self, command: &MonotonicOracleCommandV1) -> bool {
        match self.installed.as_ref() {
            None => self.role == EndpointRole::Primary,
            Some(state) if state.activation_digest.is_none() => {
                matches!(command, MonotonicOracleCommandV1::Acquire(_))
            }
            Some(_) => true,
        }
    }
    fn installed_state(&self) -> Option<DurableSetProviderStateV1> {
        self.installed.clone()
    }
    fn dispatch(
        &mut self,
        command: &MonotonicOracleCommandV1,
        requested: &EndpointInstanceBindingV1,
        current: &mut EndpointInstanceBindingV1,
        authority: &mut VerifiedAuthorityV1,
        ledger: &mut Option<Ledger>,
        role_key: &SigningKey,
        resource: &str,
    ) -> Result<bool, ()> {
        if !matches!(
            command,
            MonotonicOracleCommandV1::InstallDurableSet(_)
                | MonotonicOracleCommandV1::QueryDurableSetInstall(_)
                | MonotonicOracleCommandV1::ActivateDurableSet(_)
        ) {
            return Ok(false);
        }
        let sealed = self.approval.lock().map_err(|_| ())?.clone().ok_or(())?;
        let expected = match self.role {
            EndpointRole::Primary => &sealed.transition().body().successor.primary.instance,
            EndpointRole::Replica => &sealed.transition().body().successor.replica.instance,
        };
        if requested != expected
            || requested.endpoint_public_key != hex::encode(role_key.verifying_key().to_bytes())
            || sealed
                .transition()
                .predecessor_authority()
                .context_digest()
                .map_err(|_| ())?
                != self.original.context_digest().map_err(|_| ())?
            || !self
                .original
                .policy()
                .authorized_fencing_resource_bindings
                .iter()
                .any(|s| s == resource)
        {
            return Err(());
        }
        match command {
            MonotonicOracleCommandV1::InstallDurableSet(request) => {
                let request_digest = digest(request.as_ref())?;
                if let Some(installed) = &self.installed {
                    if installed.transition_digest != sealed.transition().digest()
                        || self.install_request_digest.as_deref() != Some(request_digest.as_str())
                    {
                        return Err(());
                    }
                    return Ok(true);
                }
                if request.authorization != *sealed.evidence() {
                    return Err(());
                }
                if self.role == EndpointRole::Primary {
                    let old = ledger.as_ref().ok_or(())?;
                    let old_seal = &sealed.evidence().primary_seal;
                    if current != &old_seal.body.instance
                        || old.pending.is_some()
                        || old.checkpoint != old_seal.provider.statement.checkpoint
                        || old.seal.as_ref() != Some(&old_seal.body)
                        || old_seal.provider.statement.resource_binding != resource
                        || request.expected_old_checkpoint.as_ref() != Some(&old.checkpoint)
                        || request.source_checkpoint_bytes != old.native
                    {
                        return Err(());
                    }
                } else if ledger.is_some() || request.expected_old_checkpoint.is_some() {
                    return Err(());
                }
                // Registered directory is out-of-band. Every history, index,
                // material and retained/business CAS node is independently read.
                let readback_started = std::time::Instant::now();
                eprintln!("healthy oracle {:?} independent readback begin", self.role);
                let plan = formal::endpoint::verify_durable_set_native_install_plan(
                    &self.directory,
                    requested,
                    &sealed,
                    request,
                    1u64 << 40,
                )
                .map_err(|error| {
                    eprintln!(
                        "healthy oracle {:?} install readback rejected after {}ms: {error}",
                        self.role,
                        readback_started.elapsed().as_millis()
                    );
                })?;
                eprintln!(
                    "healthy oracle {:?} install readback complete {}ms",
                    self.role,
                    readback_started.elapsed().as_millis()
                );
                let signature =
                    hex::encode(role_key.sign(plan.checkpoint_signing_preimage()).to_bytes());
                let native = plan
                    .signed_checkpoint(&signature, requested)
                    .map_err(|_| ())?;
                let body = Store::verify_public(&native, &requested.endpoint_public_key)?;
                let checkpoint = Store::binding(&body, &native);
                let install_body = DurableSetInstallBodyV1 {
                    schema_version: 1,
                    transition_digest: sealed.transition().digest().into(),
                    successor_authority_id: sealed
                        .transition()
                        .body()
                        .successor
                        .authority_id()
                        .map_err(|_| ())?,
                    instance: requested.clone(),
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
                    endpoint_signature_hex: hex::encode(
                        role_key
                            .sign(
                                &formal::durable_set_install_endpoint_signing_bytes(&install_body)
                                    .map_err(|_| ())?,
                            )
                            .to_bytes(),
                    ),
                    provider_signature_hex: hex::encode(
                        key(3)
                            .sign(
                                &formal::durable_set_install_provider_signing_bytes(&install_body)
                                    .map_err(|_| ())?,
                            )
                            .to_bytes(),
                    ),
                    provider_evidence: evidence(requested, &checkpoint, resource)?,
                    body: install_body,
                };
                // Single-threaded oracle lease: floor, native checkpoint and
                // instance scope change together. Retired P scope cannot return.
                *ledger = Some(Ledger {
                    checkpoint,
                    native,
                    pending: None,
                    seal: None,
                });
                *current = requested.clone();
                self.installed = Some(DurableSetProviderStateV1 {
                    transition_digest: sealed.transition().digest().into(),
                    successor_authority_id: sealed
                        .transition()
                        .body()
                        .successor
                        .authority_id()
                        .map_err(|_| ())?,
                    install_receipt: receipt,
                    activation_digest: None,
                });
                self.install_request_digest = Some(request_digest);
            }
            MonotonicOracleCommandV1::QueryDurableSetInstall(transition) => {
                let installed = self.installed.as_ref().ok_or(())?;
                if transition != sealed.transition().digest()
                    || installed.transition_digest != *transition
                    || current != requested
                {
                    return Err(());
                }
            }
            MonotonicOracleCommandV1::ActivateDurableSet(request) => {
                let installed = self.installed.as_mut().ok_or(())?;
                if request.schema_version != 1
                    || self.install_request_digest.as_deref()
                        != Some(digest(&request.install)?.as_str())
                {
                    return Err(());
                }
                let own = match self.role {
                    EndpointRole::Primary => &request.primary_install,
                    EndpointRole::Replica => &request.replica_install,
                };
                if own != &installed.install_receipt {
                    return Err(());
                }
                let activation = formal::verify_durable_set_activation(
                    sealed.transition(),
                    [
                        &sealed.evidence().primary_seal,
                        &sealed.evidence().replica_seal,
                    ],
                    [&request.primary_install, &request.replica_install],
                )
                .map_err(|_| ())?;
                if let Some(previous) = &installed.activation_digest {
                    if previous != activation.activation_digest() {
                        return Err(());
                    }
                    return Ok(true);
                }
                let state = ledger.as_mut().ok_or(())?;
                if state.pending.is_some()
                    || state.seal.is_some()
                    || state.checkpoint != installed.install_receipt.body.checkpoint
                {
                    return Err(());
                }
                let readback_started = std::time::Instant::now();
                eprintln!("healthy oracle {:?} independent readback begin", self.role);
                let plan = formal::endpoint::verify_durable_set_native_install_plan(
                    &self.directory,
                    requested,
                    &sealed,
                    &request.install,
                    1u64 << 40,
                )
                .map_err(|error| {
                    eprintln!(
                        "healthy oracle {:?} activation readback rejected after {}ms: {error}",
                        self.role,
                        readback_started.elapsed().as_millis()
                    );
                })?;
                eprintln!(
                    "healthy oracle {:?} activation readback complete {}ms",
                    self.role,
                    readback_started.elapsed().as_millis()
                );
                let active = plan.activation_plan(&activation).map_err(|_| ())?;
                let native = active
                    .signed_checkpoint(
                        &hex::encode(
                            role_key
                                .sign(active.checkpoint_signing_preimage())
                                .to_bytes(),
                        ),
                        requested,
                    )
                    .map_err(|_| ())?;
                let body = Store::verify_public(&native, &requested.endpoint_public_key)?;
                state.checkpoint = Store::binding(&body, &native);
                state.native = native;
                installed.activation_digest = Some(activation.activation_digest().into());
                *authority = activation.successor().clone();
            }
            _ => return Err(()),
        }
        Ok(true)
    }
}
fn registration(
    original: VerifiedAuthorityV1,
    role: EndpointRole,
    directory: PathBuf,
    signer_public: String,
) -> (
    HealthyProvisionHandle,
    Box<dyn oracle::DurableSetOracleHook>,
) {
    let approval = Arc::new(Mutex::new(None));
    let handle = HealthyProvisionHandle {
        approval: approval.clone(),
        original: original.clone(),
        role,
        directory: directory.clone(),
        signer_public,
    };
    let hook = HealthyHook {
        approval,
        original,
        role,
        directory,
        installed: None,
        install_request_digest: None,
    };
    (handle, Box::new(hook))
}
/// Start BEFORE genesis so P's eventual install uses the SAME original oracle,
/// listener, registered resource and monotonic ledger. Provision only after H.
pub(in super::super) fn start_healthy_primary_oracle(
    fixture: &Fixture,
) -> (OracleHandle, HealthyProvisionHandle) {
    let (handle, hook) = registration(
        fixture.authority.clone(),
        EndpointRole::Primary,
        fixture.pdir.clone(),
        public(5),
    );
    let oracle = oracle::Oracle::start_with_durable_set_hook(
        fixture.authority.clone(),
        EndpointRole::Primary,
        key(5),
        fixture
            .authority
            .policy()
            .authorized_fencing_resource_bindings[0]
            .clone(),
        hook,
    );
    (OracleHandle(oracle), handle)
}
pub(in super::super) fn start_healthy_replica_oracle(
    sealed: Arc<VerifiedSealedDurableSetTransitionV1>,
    directory: PathBuf,
    resource: String,
) -> Result<OracleHandle, String> {
    let old = sealed.transition().predecessor_authority().clone();
    let (handle, hook) = registration(old.clone(), EndpointRole::Replica, directory, public(12));
    handle.provision(sealed)?;
    Ok(OracleHandle(oracle::Oracle::start_with_durable_set_hook(
        old,
        EndpointRole::Replica,
        key(12),
        resource,
        hook,
    )))
}

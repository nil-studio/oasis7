use super::*;
use crate::controlled_authority::fencing::EndpointInstanceBindingDirectoryExt;
use std::fs;
use std::io::Read;

type Sources = Vec<Arc<dyn FormalArchiveSourceV1>>;
pub(super) fn initial_hot(
    authority: &VerifiedAuthorityV1,
    original: &VerifiedAuthorityV1,
    recovery: &Option<VerifiedRecovery>,
    parent: &Option<VerifiedFormalCommitV1>,
    instance: &fencing::EndpointInstanceBindingV1,
    archive: Option<&VerifiedFormalArchiveTransferV1>,
) -> Result<(HotBody, Sources), FormalError> {
    if let Some(recovery) = recovery
        && recovery.original_authority().authority_id()? != original.authority_id()?
    {
        return Err(invalid("formal native original recovery permission"));
    }
    let head = if recovery.is_some() {
        parent
            .as_ref()
            .map(VerifiedFormalCommitV1::head)
            .unwrap_or(authority.definition().world.genesis_head()?)
    } else {
        // A supplied thin read anchor cannot create a history-less endpoint at
        // H>0. Existing storage is restored from the protected native checkpoint.
        authority.definition().world.genesis_head()?
    };
    let mut initial = HotBody {
        version: 1,
        instance_digest: instance.digest().map_err(fencing_error)?,
        authority_id: authority.authority_id()?,
        activation_ref: authority.activation_ref().into(),
        epoch: authority.definition().epoch,
        revision: 0,
        head,
        head_node: None,
        imported_head_node: None,
        import_digest: None,
        base_parent: recovery
            .as_ref()
            .and(parent.as_ref())
            .map(|parent| CheckpointParent {
                record: parent.record().clone(),
                finalizing_authority_id: parent.finalizing_authority_id().into(),
            }),
        index: None,
        pending: None,
        decision_head: None,
        seal: None,
        durable_set: None,
    };
    let mut sources = vec![];
    if let Some(archive) = archive {
        let recovery = recovery
            .as_ref()
            .ok_or_else(|| invalid("formal archive import without verified recovery"))?;
        if archive.predecessor_authority_id != recovery.predecessor().authority_id()?
            || archive.operation_id != recovery.body().operation_id
            || archive.cut != recovery.body().sealed_cut
            || archive.cut != initial.head
            || archive.next_epoch_floor != authority.definition().epoch
        {
            return Err(invalid(
                "formal sealed index transfer recovery/cut/operation binding",
            ));
        }
        let (reservation, record) = match &recovery.body().tail {
            RecoveryTailV1::Empty => (None, None),
            RecoveryTailV1::Frozen { reservation_id, .. } => (Some(reservation_id), None),
            RecoveryTailV1::Prepared {
                reservation_id,
                record_id,
                ..
            }
            | RecoveryTailV1::Decision {
                reservation_id,
                record_id,
                ..
            } => (Some(reservation_id), Some(record_id)),
        };
        if archive
            .tail
            .frozen_input_digest
            .as_ref()
            .is_some_and(|id| Some(id) != reservation)
            || archive
                .tail
                .proposal_digest
                .as_ref()
                .is_some_and(|id| Some(id) != record)
        {
            return Err(invalid(
                "formal sealed index transfer cannot downgrade or replace retained tail",
            ));
        }
        initial.index = archive.index.clone();
        initial.imported_head_node = archive.head_node.clone();
        initial.import_digest = Some(archive.digest()?);
        import_tail::install_imported_tail(&mut initial, archive, recovery)?;
        sources.push(archive.source.clone());
    } else if recovery.is_some() {
        return Err(invalid(
            "formal recovery missing sealed permanent-index transfer",
        ));
    }
    Ok((initial, sources))
}
pub(super) fn endpoint_sign(
    instance: &fencing::EndpointInstanceBindingV1,
    policy: &fencing::TrustedFencingPolicyV1,
    local: Option<&SigningKey>,
    guard: &mut dyn fencing::MonotonicEndpointGuard,
    purpose: fencing::ProtectedEndpointSignaturePurposeV1,
    preimage: Vec<u8>,
) -> Result<String, FormalError> {
    if let Some(key) = local {
        #[cfg(test)]
        {
            return Ok(hex::encode(key.sign(&preimage).to_bytes()));
        }
        #[cfg(not(test))]
        {
            let _ = key;
            return Err(FormalError::FencingUnproven);
        }
    }
    if guard.seal().is_some() && purpose.mutation() {
        return Err(FormalError::Conflict(
            "formal protected signer epoch sealed".into(),
        ));
    }
    let request = fencing::ProtectedEndpointSigningRequestV1 {
        schema_version: 1,
        instance_digest: instance.digest().map_err(fencing_error)?,
        checkpoint: guard.checkpoint().clone(),
        challenge: fencing::fresh_challenge().map_err(fencing_error)?,
        purpose,
        preimage,
    };
    let signed = guard
        .sign_endpoint(request.clone())
        .map_err(fencing_error)?;
    fencing::verify_protected_endpoint_signature(&signed, &request, instance, policy)
        .map_err(fencing_error)
}
impl FileFormalEndpoint {
    pub(super) fn publish_native(
        &mut self,
        next: HotBody,
        guard: &mut dyn fencing::MonotonicEndpointGuard,
    ) -> Result<(), FormalError> {
        let instance = self.instance().clone();
        let policy = self.signing_policy();
        let local = self.key.clone();
        self.store
            .publish_with(next, guard, &self.public_key, |body, gate| {
                endpoint_sign(
                    &instance,
                    &policy,
                    local.as_ref(),
                    gate,
                    fencing::ProtectedEndpointSignaturePurposeV1::NativeCheckpoint,
                    Store::checkpoint_signing_bytes(body)?,
                )
            })
    }
    pub(super) fn signed_seal(
        &mut self,
        body: fencing::EpochSealBodyV1,
        guard: &mut dyn fencing::MonotonicEndpointGuard,
    ) -> Result<fencing::EpochSealReceiptV1, FormalError> {
        if let Some(key) = &self.key {
            #[cfg(test)]
            return fencing::EpochSealReceiptV1::sign(
                body,
                guard.evidence().map_err(fencing_error)?,
                key,
            )
            .map_err(fencing_error);
            #[cfg(not(test))]
            {
                let _ = key;
                return Err(FormalError::FencingUnproven);
            }
        }
        let request = fencing::ProtectedEndpointSigningRequestV1 {
            schema_version: 1,
            instance_digest: self.instance().digest().map_err(fencing_error)?,
            checkpoint: guard.checkpoint().clone(),
            challenge: fencing::fresh_challenge().map_err(fencing_error)?,
            purpose: fencing::ProtectedEndpointSignaturePurposeV1::EpochSeal,
            preimage: fencing::epoch_seal_signing_bytes(&body).map_err(fencing_error)?,
        };
        let signed = guard
            .sign_endpoint(request.clone())
            .map_err(fencing_error)?;
        let endpoint_signature = fencing::verify_protected_endpoint_signature(
            &signed,
            &request,
            self.instance(),
            &self.signing_policy(),
        )
        .map_err(fencing_error)?;
        Ok(fencing::EpochSealReceiptV1 {
            body,
            endpoint_signature,
            provider: guard.evidence().map_err(fencing_error)?,
            protected_signature: Some(signed),
        })
    }
    /// No endpoint private key enters this process. The independently registered
    /// oracle retains signed native metadata and performs every endpoint signature
    /// under the same nonrollback/exclusive lease as publication.
    pub fn open_protected(
        path: &Path,
        authorization: FormalEndpointAuthorizationV1,
        role: EndpointRole,
        provider: Arc<dyn fencing::MonotonicEndpointProvider>,
        archive: Option<&VerifiedFormalArchiveTransferV1>,
    ) -> Result<Self, FormalError> {
        let FormalEndpointAuthorizationV1 {
            authority,
            original,
            recovery,
            parent,
        } = authorization;
        authority.definition().verify_policy(authority.policy())?;
        original.definition().verify_policy(original.policy())?;
        match &recovery {
            None if authority.authority_id()? != original.authority_id()? => {
                return Err(invalid("formal protected open missing recovery"));
            }
            Some(recovery)
                if recovery.successor().authority_id()? != authority.authority_id()?
                    || recovery.original_authority().authority_id()?
                        != original.authority_id()? =>
            {
                return Err(invalid("formal protected recovery context"));
            }
            _ => {}
        }
        let instance = match role {
            EndpointRole::Primary => &authority.definition().primary.instance,
            EndpointRole::Replica => &authority.definition().replica.instance,
        };
        instance.verify_directory(path).map_err(fencing_error)?;
        let (initial, sources) =
            initial_hot(&authority, &original, &recovery, &parent, instance, archive)?;
        let file = path.join("formal-checkpoint-v1.cbor");
        let existing = match fs::symlink_metadata(&file) {
            Ok(metadata) => {
                if !metadata.is_file() || metadata.len() > 64 * 1024 {
                    return Err(invalid("formal protected native checkpoint regular/bound"));
                }
                let mut bytes = Vec::new();
                fs::File::open(&file)
                    .map_err(|e| FormalError::Io(e.to_string()))?
                    .take(64 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| FormalError::Io(e.to_string()))?;
                if bytes.len() > 64 * 1024 {
                    return Err(invalid("formal protected native checkpoint bound"));
                }
                Some(bytes)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(FormalError::Io(error.to_string())),
        };
        let registration = fencing::ProtectedEndpointRegistrationV1 {
            authority_id: authority.authority_id()?,
            activation_ref: authority.activation_ref().into(),
            unsigned_checkpoint_preimage: Store::checkpoint_signing_bytes(&initial)?,
            existing_checkpoint: existing,
        };
        let mut guard = provider
            .initialize_protected(instance, registration)
            .map_err(fencing_error)?;
        let bytes = guard
            .protected_native_checkpoint()
            .ok_or(FormalError::FencingUnproven)?
            .to_vec();
        let body = Store::verify_public(&bytes, &instance.endpoint_public_key)?;
        if Store::binding(&body, &bytes) != *guard.checkpoint() && guard.pending().is_none() {
            return Err(FormalError::FencingUnproven);
        }
        let public_key = instance.endpoint_public_key.clone();
        let mut store = Store::open_signed(path, initial, &public_key, bytes, sources)?;
        store.reconcile_public(guard.as_mut(), &public_key)?;
        fencing::verify_monotonic_checkpoint_evidence(
            &guard.evidence().map_err(fencing_error)?,
            instance,
            guard.checkpoint(),
            &fencing::TrustedFencingPolicyV1 {
                provider_public_key: authority.policy().fencing_provider_public_key.clone(),
                authorized_resource_bindings: authority
                    .policy()
                    .authorized_fencing_resource_bindings
                    .clone(),
            },
        )
        .map_err(fencing_error)?;

        drop(guard);
        let mut endpoint = Self {
            store,
            authority,
            original,
            recovery,
            role,
            key: None,
            public_key,
            provider,
            parent,
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
    pub(super) fn signing_policy(&self) -> fencing::TrustedFencingPolicyV1 {
        fencing::TrustedFencingPolicyV1 {
            provider_public_key: self.authority.policy().fencing_provider_public_key.clone(),
            authorized_resource_bindings: self
                .authority
                .policy()
                .authorized_fencing_resource_bindings
                .clone(),
        }
    }
    pub(super) fn sign_at_checkpoint(
        &mut self,
        guard: &mut dyn fencing::MonotonicEndpointGuard,
        purpose: fencing::ProtectedEndpointSignaturePurposeV1,
        preimage: Vec<u8>,
    ) -> Result<String, FormalError> {
        let result = endpoint_sign(
            self.instance(),
            &self.signing_policy(),
            self.key.as_ref(),
            guard,
            purpose,
            preimage,
        );
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
}

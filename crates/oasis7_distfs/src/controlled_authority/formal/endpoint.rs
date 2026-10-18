//! Independently held endpoint keys, full material durability and canonical heads.
use super::{crypto::*, *};
use crate::controlled_authority::fencing::EndpointInstanceBindingDirectoryExt;
use crate::controlled_authority::replicated_protocol::EndpointRole;
#[cfg(test)]
use ed25519_dalek::Signer;
use ed25519_dalek::SigningKey;
use std::path::Path;
use std::sync::Arc;
#[path = "endpoint_types.rs"]
mod types;
pub use types::*;
#[path = "endpoint_chunk.rs"]
pub(super) mod chunk;
#[path = "endpoint_published_closure.rs"]
mod published_closure;
#[path = "endpoint_storage_maintenance.rs"]
mod storage_maintenance;
#[path = "endpoint_store.rs"]
mod store;
pub use chunk::{FORMAL_ARCHIVE_CHUNK_LIMIT, FORMAL_ARCHIVE_OBJECT_LIMIT, FormalArchiveChunkV1};
#[path = "endpoint_native_restore.rs"]
mod native_restore;
pub use native_restore::VerifiedNativeCheckpointRestoreV1;
pub(crate) use native_restore::restore_native_with_durable_set;
#[path = "endpoint_native_history.rs"]
mod native_history;
pub use native_history::VerifiedNativeHistoricalRestoreV1;
pub(crate) use native_history::restore_native_position;
use store::{CheckpointParent, HistoryNode, HotBody, Store};
#[path = "endpoint_archive_file.rs"]
mod archive_file;
#[path = "endpoint_archive_transfer.rs"]
mod archive_transfer;
pub use archive_file::FileFormalArchiveSourceV1;
#[path = "endpoint_business.rs"]
mod business;
#[path = "endpoint_durable_set_install.rs"]
mod durable_set_install;
#[path = "endpoint_durable_set_seal.rs"]
mod durable_set_seal;
#[path = "endpoint_import_tail.rs"]
mod import_tail;
#[path = "endpoint_fencing.rs"]
mod native_fencing;
pub use durable_set_install::DurableSetCandidateV1;
#[path = "endpoint_durable_set_provider.rs"]
mod durable_set_provider;
pub use durable_set_provider::{
    VerifiedDurableSetNativeInstallPlanV1, verify_durable_set_native_install_plan,
};
#[path = "endpoint_recovery.rs"]
mod native_recovery;
#[path = "endpoint_rpc.rs"]
mod native_rpc;
#[path = "endpoint_protected.rs"]
mod protected_signer;
pub use archive_transfer::{
    FormalArchiveSourceV1, SignedFormalArchiveTransferV1, VerifiedFormalArchiveTransferV1,
    verify_formal_archive_transfer,
};
pub(crate) use native_rpc::verify_formal_rpc_checkpoint;
#[cfg(test)]
#[path = "endpoint_tests.rs"]
mod tests;

// Checked byte provenance lives with the independent proof constructors.
pub(crate) use oasis7_formal_protocol::controlled_authority::formal::proof::NativeCheckpointVerdictV1;

pub(super) fn fencing_error(error: fencing::FencingError) -> FormalError {
    match error {
        fencing::FencingError::FencingUnproven => FormalError::FencingUnproven,
        other => FormalError::Rollback(other.to_string()),
    }
}
#[derive(Debug, Clone)]
pub struct FormalEndpointAuthorizationV1 {
    pub authority: VerifiedAuthorityV1,
    pub original: VerifiedAuthorityV1,
    pub recovery: Option<VerifiedRecovery>,
    pub parent: Option<VerifiedFormalCommitV1>,
}
pub struct FileFormalEndpoint {
    store: Store,
    authority: VerifiedAuthorityV1,
    original: VerifiedAuthorityV1,
    recovery: Option<VerifiedRecovery>,
    role: EndpointRole,
    key: Option<SigningKey>,
    public_key: String,
    provider: Arc<dyn fencing::MonotonicEndpointProvider>,
    parent: Option<VerifiedFormalCommitV1>,
    poisoned: bool,
    native_environment_jobs:
        std::collections::BTreeMap<String, super::native_environment::EndpointVerificationJob>,
    published_closure_jobs:
        std::collections::BTreeMap<String, store::published_closure::ClosureJob>,
    #[cfg(test)]
    controlled: bool,
}
impl FileFormalEndpoint {
    /// Recovery endpoints receive the verified predecessor as `original` and
    /// the verified successor as `authority`; their keys and storage are fresh.
    pub fn open(
        path: &Path,
        authorization: FormalEndpointAuthorizationV1,
        role: EndpointRole,
        key: SigningKey,
        provider: Arc<dyn fencing::MonotonicEndpointProvider>,
    ) -> Result<Self, FormalError> {
        if authorization.recovery.is_some() {
            return Err(invalid(
                "formal recovery open requires sealed permanent-index transfer",
            ));
        }
        Self::open_inner(path, authorization, role, key, provider, false, None)
    }
    pub fn open_with_archive(
        path: &Path,
        authorization: FormalEndpointAuthorizationV1,
        role: EndpointRole,
        key: SigningKey,
        provider: Arc<dyn fencing::MonotonicEndpointProvider>,
        archive: &VerifiedFormalArchiveTransferV1,
    ) -> Result<Self, FormalError> {
        if authorization.recovery.is_none() {
            return Err(invalid("formal archive import requires verified recovery"));
        }
        Self::open_inner(
            path,
            authorization,
            role,
            key,
            provider,
            false,
            Some(archive),
        )
    }
    fn open_inner(
        path: &Path,
        authorization: FormalEndpointAuthorizationV1,
        role: EndpointRole,
        key: SigningKey,
        provider: Arc<dyn fencing::MonotonicEndpointProvider>,
        controlled: bool,
        archive: Option<&VerifiedFormalArchiveTransferV1>,
    ) -> Result<Self, FormalError> {
        let FormalEndpointAuthorizationV1 {
            authority,
            original,
            recovery,
            parent,
        } = authorization;
        // A plaintext local endpoint key cannot establish protected custody.
        // A production path requires a protected signer; this constructor is
        // retained solely for explicit controlled storage fixtures.
        if !controlled {
            return Err(FormalError::FencingUnproven);
        }
        authority.definition().verify_policy(authority.policy())?;
        original.definition().verify_policy(original.policy())?;
        match &recovery {
            None if authority.authority_id()? != original.authority_id()? => {
                return Err(invalid("formal endpoint missing recovery authorization"));
            }
            Some(recovery)
                if recovery.successor().authority_id()? != authority.authority_id()?
                    || recovery.original_authority().authority_id()?
                        != original.authority_id()? =>
            {
                return Err(invalid("formal endpoint recovery authority context"));
            }
            _ => {}
        }
        let instance = match role {
            EndpointRole::Primary => &authority.definition().primary.instance,
            EndpointRole::Replica => &authority.definition().replica.instance,
        };
        if hex::encode(key.verifying_key().to_bytes()) != instance.endpoint_public_key {
            return Err(invalid("formal endpoint independent instance key"));
        }
        // Creation precedes fingerprint checking; no signatures are issued yet.
        let (initial, sources) = protected_signer::initial_hot(
            &authority, &original, &recovery, &parent, instance, archive,
        )?;
        let public_key = instance.endpoint_public_key.clone();
        let store = Store::open(path, initial, &key, sources)?;
        instance.verify_directory(path).map_err(fencing_error)?;
        let mut endpoint = Self {
            store,
            authority,
            original,
            recovery,
            role,
            key: Some(key),
            public_key,
            provider,
            parent,
            poisoned: false,
            native_environment_jobs: std::collections::BTreeMap::new(),
            published_closure_jobs: std::collections::BTreeMap::new(),
            #[cfg(test)]
            controlled,
        };
        #[cfg(not(test))]
        if controlled {
            return Err(FormalError::FencingUnproven);
        }
        let _guard = endpoint.guard()?;
        endpoint.reload_parent(&_guard.evidence().map_err(fencing_error)?)?;
        endpoint.verify_business_checkpoint()?;
        Ok(endpoint)
    }
    pub fn authority(&self) -> &VerifiedAuthorityV1 {
        &self.authority
    }
    pub fn role(&self) -> EndpointRole {
        self.role
    }
    fn instance(&self) -> &fencing::EndpointInstanceBindingV1 {
        match self.role {
            EndpointRole::Primary => &self.authority.definition().primary.instance,
            EndpointRole::Replica => &self.authority.definition().replica.instance,
        }
    }
    fn guard(&mut self) -> Result<Box<dyn fencing::MonotonicEndpointGuard>, FormalError> {
        if self.poisoned {
            return Err(FormalError::Io(
                "formal endpoint outcome unknown; reopen required".into(),
            ));
        }
        let result = (|| {
            self.store.refresh_public(&self.public_key)?;
            let mut guard = self
                .provider
                .acquire(self.instance(), &self.store.checkpoint)
                .map_err(fencing_error)?;
            self.store
                .reconcile_public(guard.as_mut(), &self.public_key)?;
            if let Some(binding) = &self.store.body.durable_set {
                if binding.activation_digest.as_deref()
                    != self.authority.durable_set_activation_digest()
                    || binding.activation_digest.is_none()
                    || guard.durable_set_state().is_none_or(|state| {
                        state.activation_digest != binding.activation_digest
                            || state.transition_digest != binding.transition_digest
                            || state.install_receipt.body.independent_inventory_digest
                                != binding.independent_inventory_digest
                    })
                {
                    return Err(FormalError::FencingUnproven);
                }
            } else if self.authority.durable_set_activation_digest().is_some() {
                return Err(FormalError::FencingUnproven);
            }
            let evidence = guard.evidence().map_err(fencing_error)?;
            #[cfg(test)]
            if self.controlled
                && evidence.statement.capability
                    == fencing::ProviderCapabilityV1::ControlledLocalTest
            {
                return Ok(guard);
            }
            fencing::verify_monotonic_checkpoint_evidence(
                &evidence,
                self.instance(),
                &self.store.checkpoint,
                &fencing::TrustedFencingPolicyV1 {
                    provider_public_key: self
                        .authority
                        .policy()
                        .fencing_provider_public_key
                        .clone(),
                    authorized_resource_bindings: self
                        .authority
                        .policy()
                        .authorized_fencing_resource_bindings
                        .clone(),
                },
            )
            .map_err(fencing_error)?;
            Ok(guard)
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    fn reload_parent(
        &mut self,
        provider: &fencing::MonotonicProviderEvidenceV1,
    ) -> Result<(), FormalError> {
        if let Some(entry) = self.store.head_entry()? {
            let FormalRequestStateV1::Qualified { candidate, proof } = entry else {
                return Err(invalid("formal checkpoint qualified entry"));
            };
            verify_record_material(&proof.decision.record, &candidate.material)?;
            if proof.decision.record != candidate.candidate.record
                || proof.decision.record.record_id()? != self.store.body.head.record_id
            {
                return Err(invalid("formal signed checkpoint head identity"));
            }
            let finalizing_id = proof.decision.primary.body.finalizing_authority_id.clone();
            if finalizing_id != self.authority.authority_id()?
                || proof.decision.replica.body.finalizing_authority_id != finalizing_id
            {
                return Err(invalid("formal checkpoint finalizing authority"));
            }
            let head_node: HistoryNode = self.store.get(
                self.store
                    .body
                    .head_node
                    .as_ref()
                    .expect("loaded head node"),
            )?;
            let parent = if let Some(link) = head_node.ancestors.first() {
                if link.head.position.checked_add(1) != Some(head_node.head.position) {
                    return Err(invalid("formal checkpoint immediate ancestor position"));
                }
                let node: HistoryNode = self.store.get(&link.node)?;
                if node.head != link.head {
                    return Err(invalid("formal checkpoint immediate ancestor binding"));
                }
                let FormalRequestStateV1::Qualified { proof: parent, .. } =
                    self.store.get(&node.entry)?
                else {
                    return Err(invalid("formal checkpoint ancestor qualified entry"));
                };
                if parent.decision.record.record_id()? != link.head.record_id {
                    return Err(invalid("formal checkpoint ancestor canonical identity"));
                }
                Some(CheckpointParent {
                    record: parent.decision.record,
                    finalizing_authority_id: parent.decision.primary.body.finalizing_authority_id,
                })
            } else {
                self.store.body.base_parent.clone()
            };
            let checkpoint_bytes = self.store.checkpoint_bytes()?;
            let instance = match self.role {
                EndpointRole::Primary => &self.authority.definition().primary.instance,
                EndpointRole::Replica => &self.authority.definition().replica.instance,
            };
            let verdict = NativeCheckpointVerdictV1::verify(
                &checkpoint_bytes,
                &self.authority,
                &instance.endpoint_public_key,
                &self.store.checkpoint,
                provider,
                &self.store.body.head,
                &self.store.body.head,
                &self.authority.authority_id()?,
                |id| {
                    self.store
                        .raw_object(id)?
                        .ok_or_else(|| invalid("native checkpoint object missing"))
                },
            )?;
            if verdict.proof() != &proof
                || verdict.verified_parent_record() != parent.as_ref().map(|p| &p.record)
                || verdict.verified_parent_finalizing_authority_id()
                    != parent.as_ref().map(|p| p.finalizing_authority_id.as_str())
            {
                return Err(invalid(
                    "native checked checkpoint context differs from held storage",
                ));
            }
            let original = if verdict.proof().decision.branch == FormalCompletionBranchV1::Normal {
                &self.authority
            } else {
                &self.original
            };
            let recovery = (verdict.proof().decision.branch != FormalCompletionBranchV1::Normal)
                .then_some(self.recovery.as_ref())
                .flatten();
            self.parent = Some(restore_formal_commit_from_checkpoint(
                verdict.proof(),
                original,
                recovery,
                &verdict,
            )?);
        } else if self
            .parent
            .as_ref()
            .map(VerifiedFormalCommitV1::head)
            .unwrap_or(self.authority.definition().world.genesis_head()?)
            != self.store.body.head
        {
            return Err(invalid("formal checkpoint initial verified parent"));
        }
        Ok(())
    }
    fn load(&mut self, request: &str) -> Result<Option<FormalRequestStateV1>, FormalError> {
        let result = (|| {
            self.store
                .lookup(request)?
                .map(|id| {
                    let entry: FormalRequestStateV1 = self.store.get(&id)?;
                    if entry.request_digest() != request {
                        return Err(invalid("formal index request binding"));
                    }
                    Ok(entry)
                })
                .transpose()
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    fn write(
        &mut self,
        entry: FormalRequestStateV1,
        qualified: bool,
        guard: &mut dyn fencing::MonotonicEndpointGuard,
    ) -> Result<(), FormalError> {
        let result = (|| {
            let id = self.store.put(&entry)?;
            let mut next = self.store.body.clone();
            next.index = Some(self.store.insert(
                next.index.as_deref(),
                entry.request_digest(),
                &id,
            )?);
            if qualified {
                let FormalRequestStateV1::Qualified { proof, .. } = &entry else {
                    return Err(invalid("formal qualified state"));
                };
                next.head = FormalHeadRefV1 {
                    position: proof.decision.record.position,
                    record_id: proof.decision.record.record_id()?,
                };
                next.head_node = Some(self.store.append_node(
                    next.head.clone(),
                    entry.request_digest().into(),
                    id,
                )?);
                next.pending = None;
                next.decision_head = None;
            } else {
                next.pending = Some(id);
                if let FormalRequestStateV1::Decided { candidate, .. }
                | FormalRequestStateV1::Ready { candidate, .. } = &entry
                {
                    next.decision_head = Some(FormalHeadRefV1 {
                        position: candidate.candidate.record.position,
                        record_id: candidate.candidate.record.record_id()?,
                    });
                }
            }
            self.publish_native(next, guard)
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    fn fresh_slot(&self, request: &str, position: u64, parent: &str) -> Result<(), FormalError> {
        if self.store.body.seal.is_some() {
            return Err(FormalError::Conflict("formal epoch sealed".into()));
        }
        if Some(position) != self.store.body.head.position.checked_add(1)
            || parent != self.store.body.head.record_id
        {
            return Err(FormalError::Conflict(
                "formal exact canonical parent/next slot".into(),
            ));
        }
        if let Some(id) = &self.store.body.pending {
            let entry: FormalRequestStateV1 = self.store.get(id)?;
            if entry.request_digest() != request {
                return Err(FormalError::Conflict(
                    "formal one durable reservation slot".into(),
                ));
            }
        }
        Ok(())
    }
    pub fn head(&mut self) -> Result<FormalHeadRefV1, FormalError> {
        let _guard = self.guard()?;
        Ok(self.store.body.head.clone())
    }
    pub fn verify_minimum(&mut self, minimum: &FormalHeadRefV1) -> Result<(), FormalError> {
        let _guard = self.guard()?;
        if minimum.position == 0 && minimum != &self.authority.definition().world.genesis_head()? {
            return Err(FormalError::Rollback(
                "formal pinned genesis minimum fork".into(),
            ));
        }
        let result = self.store.verify_minimum(minimum);
        if matches!(&result, Err(FormalError::Invalid(_) | FormalError::Io(_))) {
            self.poisoned = true;
        }
        result
    }
    pub fn query(&mut self, request: &str) -> Result<FormalQueryOutcomeV1, FormalError> {
        let _guard = self.guard()?;
        match self.load(request) {
            Ok(Some(entry)) => Ok(FormalQueryOutcomeV1::Recorded(Box::new(entry))),
            Ok(None) => Ok(FormalQueryOutcomeV1::NotRecorded),
            Err(error) => {
                self.poisoned = true;
                Err(error)
            }
        }
    }
    pub fn freeze(
        &mut self,
        input: SignedFormalFrozenV1,
        material: crate::controlled_authority::replicated_protocol::ClosedRecord,
    ) -> Result<FormalFrozenReceiptV1, FormalError> {
        let mut guard = self.guard()?;
        verify_frozen_material(&input.body, &material)?;
        drop(self.take_native_environment(&material)?);
        let request = input.body.request_digest.clone();
        if let Some(entry) = self.load(&request)? {
            match entry {
                FormalRequestStateV1::Frozen {
                    input: old,
                    material: old_material,
                    receipt: Some(receipt),
                    ..
                } if old == input && old_material == material => return Ok(receipt),
                FormalRequestStateV1::Frozen {
                    input: old,
                    material: old_material,
                    receipt: None,
                    ..
                } if old == input && old_material == material => {}
                other => {
                    if let Some(candidate) = other.candidate()
                        && candidate
                            .frozen
                            .as_ref()
                            .is_some_and(|cert| cert.reservation == input)
                        && candidate.frozen_material.as_ref() == Some(&material)
                    {
                        let certificate = candidate.frozen.as_ref().expect("checked certificate");
                        return Ok(match self.role {
                            EndpointRole::Primary => certificate.primary.clone(),
                            EndpointRole::Replica => certificate.replica.clone(),
                        });
                    }
                    return Err(FormalError::Conflict(
                        "formal request identity already bound".into(),
                    ));
                }
            }
        }
        self.fresh_slot(
            &request,
            input.body.next_position,
            &input.body.parent.record_id,
        )?;
        verify_formal_reservation(
            &input,
            &self.authority,
            self.parent
                .as_ref()
                .ok_or_else(|| invalid("formal freeze requires committed activation"))?,
        )?;
        if self.parent.as_ref().and_then(|parent| parent.record().artifact_roots.get(
            &crate::controlled_authority::replicated_protocol::ArtifactRole::NativeExecutionEnvironment))
            != material.roots.get(&crate::controlled_authority::replicated_protocol::ArtifactRole::NativeExecutionEnvironment) {
            return Err(invalid("native frozen environment differs from committed parent"));
        }
        self.write(
            FormalRequestStateV1::Frozen {
                input: input.clone(),
                material: material.clone(),
                receipt: None,
                certificate: None,
            },
            false,
            guard.as_mut(),
        )?;
        let body = FormalFrozenReceiptBodyV1 {
            authority_id: self.authority.authority_id()?,
            role: self.role,
            instance_digest: self.instance().digest().map_err(fencing_error)?,
            reservation_id: input.body.reservation_id()?,
        };
        let receipt = FormalFrozenReceiptV1 {
            signature_hex: self.sign_at_checkpoint(
                guard.as_mut(),
                fencing::ProtectedEndpointSignaturePurposeV1::FrozenReceipt,
                frozen_receipt_signing_bytes(&body)?,
            )?,
            body,
        };
        self.write(
            FormalRequestStateV1::Frozen {
                input,
                material,
                receipt: Some(receipt.clone()),
                certificate: None,
            },
            false,
            guard.as_mut(),
        )?;
        Ok(receipt)
    }
    pub fn complete_frozen(
        &mut self,
        certificate: FormalFrozenCertificateV1,
    ) -> Result<(), FormalError> {
        let mut guard = self.guard()?;
        certificate.verify(self.authority.definition(), self.authority.activation_ref())?;
        let request = &certificate.reservation.body.request_digest;
        let entry = self
            .load(request)?
            .ok_or_else(|| invalid("formal frozen local reservation missing"))?;
        let FormalRequestStateV1::Frozen {
            input,
            material,
            receipt: Some(receipt),
            certificate: old,
        } = entry
        else {
            if entry
                .candidate()
                .is_some_and(|candidate| candidate.frozen.as_ref() == Some(&certificate))
            {
                return Ok(());
            }
            return Err(invalid("formal frozen completion state"));
        };
        let local = match self.role {
            EndpointRole::Primary => &certificate.primary,
            EndpointRole::Replica => &certificate.replica,
        };
        if input != certificate.reservation
            || &receipt != local
            || old.as_ref().is_some_and(|old| old != &certificate)
        {
            return Err(FormalError::Conflict(
                "formal frozen original certificate".into(),
            ));
        }
        if old.is_some() {
            return Ok(());
        }
        self.write(
            FormalRequestStateV1::Frozen {
                input,
                material,
                receipt: Some(receipt),
                certificate: Some(certificate),
            },
            false,
            guard.as_mut(),
        )
    }
    fn candidate_context<'a>(
        &self,
        candidate: &'a FormalCandidateMaterialV1,
        retain_snapshot_bytes: bool,
    ) -> Result<
        (
            VerifiedCandidateContextV1,
            super::business_material::VerifiedCandidateMaterialClosureV1<'a>,
        ),
        FormalError,
    > {
        // This flag controls allocation lifetime only: both entry points do
        // the same complete material verification. Only Prepare reuses bytes.
        let closure = if retain_snapshot_bytes {
            super::business_material::VerifiedCandidateMaterialClosureV1::verify_retaining_snapshots(candidate)?
        } else {
            super::business_material::VerifiedCandidateMaterialClosureV1::verify(candidate)?
        };
        let recovery = (candidate.branch != FormalCompletionBranchV1::Normal)
            .then_some(self.recovery.as_ref())
            .flatten();
        let original = if candidate.branch == FormalCompletionBranchV1::Normal {
            &self.authority
        } else {
            &self.original
        };
        let context = verify_formal_candidate(
            &candidate.candidate,
            candidate.frozen.as_ref(),
            original,
            self.parent.as_ref(),
            recovery,
            candidate.branch,
        )?;
        Ok((context, closure))
    }
    pub fn prepare(
        &mut self,
        candidate: FormalCandidateMaterialV1,
    ) -> Result<FormalPrepareReceiptV1, FormalError> {
        let mut guard = self.guard()?;
        let request = candidate.candidate.record.request_digest.clone();
        let (context, material_closure) = self.candidate_context(&candidate, true)?;
        drop(self.take_native_environment(&candidate.material)?);
        if context.finalizing_authority().authority_id()? != self.authority.authority_id()? {
            return Err(invalid("formal candidate finalizer endpoint"));
        }
        self.persist_business_candidate(&candidate, &material_closure)?;
        let expected_receipt = context.receipt_body(self.role)?;
        if let Some(entry) = self.load(&request)? {
            match &entry {
                FormalRequestStateV1::Prepared {
                    candidate: old,
                    receipt: Some(receipt),
                } if old == &candidate && receipt.body.final_receipt == expected_receipt => {
                    return Ok(receipt.clone());
                }
                FormalRequestStateV1::Decided {
                    candidate: old,
                    prepare,
                    ..
                } if old == &candidate && prepare.body.final_receipt == expected_receipt => {
                    return Ok(prepare.clone());
                }
                FormalRequestStateV1::Ready {
                    candidate: old,
                    prepare,
                    ..
                } if old == &candidate && prepare.body.final_receipt == expected_receipt => {
                    return Ok(prepare.clone());
                }
                FormalRequestStateV1::Qualified { candidate: old, .. } if old == &candidate => {
                    return Err(FormalError::Conflict(
                        "formal original request already qualified; query result".into(),
                    ));
                }
                FormalRequestStateV1::Frozen {
                    certificate,
                    material,
                    ..
                } if certificate == &candidate.frozen
                    && Some(material) == candidate.frozen_material.as_ref()
                    && certificate.is_some() => {}
                FormalRequestStateV1::Frozen {
                    input, material, ..
                } if candidate.branch != FormalCompletionBranchV1::Normal
                    && candidate
                        .frozen
                        .as_ref()
                        .is_some_and(|cert| &cert.reservation == input)
                    && candidate.frozen_material.as_ref() == Some(material) => {}
                FormalRequestStateV1::Prepared {
                    candidate: old,
                    receipt: None,
                } if old == &candidate => {}
                FormalRequestStateV1::Prepared { candidate: old, .. }
                | FormalRequestStateV1::Decided { candidate: old, .. }
                | FormalRequestStateV1::Ready { candidate: old, .. }
                    if candidate.branch == FormalCompletionBranchV1::CompletePrepared && {
                        let mut retained = old.clone();
                        retained.branch = candidate.branch;
                        retained == candidate
                    } => {}
                _ => {
                    return Err(FormalError::Conflict(
                        "formal retained original candidate".into(),
                    ));
                }
            }
        } else if candidate.branch == FormalCompletionBranchV1::Normal && candidate.frozen.is_some()
        {
            return Err(invalid(
                "formal execution requires locally completed frozen certificate",
            ));
        }
        self.fresh_slot(
            &request,
            candidate.candidate.record.position,
            &candidate.candidate.record.parent_record_id,
        )?;
        self.write(
            FormalRequestStateV1::Prepared {
                candidate: candidate.clone(),
                receipt: None,
            },
            false,
            guard.as_mut(),
        )?;
        let body = FormalPrepareReceiptBodyV1 {
            final_receipt: context.receipt_body(self.role)?,
            candidate_material_digest: formal_candidate_material_digest(&candidate)?,
        };
        let receipt = FormalPrepareReceiptV1 {
            signature_hex: self.sign_at_checkpoint(
                guard.as_mut(),
                fencing::ProtectedEndpointSignaturePurposeV1::PrepareReceipt,
                formal_prepare_receipt_signing_bytes(&body)?,
            )?,
            body,
        };
        self.write(
            FormalRequestStateV1::Prepared {
                candidate,
                receipt: Some(receipt.clone()),
            },
            false,
            guard.as_mut(),
        )?;
        Ok(receipt)
    }
    pub fn decide(
        &mut self,
        request: &str,
        primary: FormalPrepareReceiptV1,
        replica: FormalPrepareReceiptV1,
    ) -> Result<FormalFinalReceiptV1, FormalError> {
        let mut guard = self.guard()?;
        let entry = self
            .load(request)?
            .ok_or_else(|| invalid("formal decision requires retained prepare"))?;
        if let FormalRequestStateV1::Decided { receipt, .. } = &entry {
            return Ok(receipt.clone());
        }
        if let FormalRequestStateV1::Ready { decision, .. } = &entry {
            return Ok(match self.role {
                EndpointRole::Primary => decision.primary.clone(),
                EndpointRole::Replica => decision.replica.clone(),
            });
        }
        if let FormalRequestStateV1::Qualified { proof, .. } = &entry {
            return Ok(match self.role {
                EndpointRole::Primary => proof.decision.primary.clone(),
                EndpointRole::Replica => proof.decision.replica.clone(),
            });
        }
        let FormalRequestStateV1::Prepared {
            candidate,
            receipt: Some(prepare),
        } = entry
        else {
            return Err(invalid("formal decision durable prepare receipt missing"));
        };
        let (context, _material_closure) = self.candidate_context(&candidate, false)?;
        verify_formal_prepare_receipt(&primary, &candidate, &context, EndpointRole::Primary)?;
        verify_formal_prepare_receipt(&replica, &candidate, &context, EndpointRole::Replica)?;
        let local = match self.role {
            EndpointRole::Primary => &primary,
            EndpointRole::Replica => &replica,
        };
        if local != &prepare {
            return Err(invalid("formal decision original local prepare"));
        }
        self.fresh_slot(
            request,
            candidate.candidate.record.position,
            &candidate.candidate.record.parent_record_id,
        )?;
        let body = context.receipt_body(self.role)?;
        let receipt = FormalFinalReceiptV1 {
            signature_hex: self.sign_at_checkpoint(
                guard.as_mut(),
                fencing::ProtectedEndpointSignaturePurposeV1::FinalReceipt,
                formal_final_receipt_signing_bytes(&body)?,
            )?,
            body,
        };
        self.write(
            FormalRequestStateV1::Decided {
                candidate,
                prepare,
                receipt: receipt.clone(),
            },
            false,
            guard.as_mut(),
        )?;
        Ok(receipt)
    }
    pub fn install_proof(
        &mut self,
        decision: FormalDecisionProofV1,
    ) -> Result<FormalProofCompleteReceiptV1, FormalError> {
        let mut guard = self.guard()?;
        let request = &decision.record.request_digest;
        let entry = self
            .load(request)?
            .ok_or_else(|| invalid("formal proof installation local decision missing"))?;
        match &entry {
            FormalRequestStateV1::Ready {
                decision: old,
                complete: Some(receipt),
                ..
            } if old == &decision => return Ok(receipt.clone()),
            FormalRequestStateV1::Qualified { proof, .. } if proof.decision == decision => {
                return Ok(match self.role {
                    EndpointRole::Primary => proof.primary_complete.clone(),
                    EndpointRole::Replica => proof.replica_complete.clone(),
                });
            }
            _ => {}
        }
        let (candidate, prepare, local) = match entry {
            FormalRequestStateV1::Decided {
                candidate,
                prepare,
                receipt,
            } => (candidate, prepare, receipt),
            FormalRequestStateV1::Ready {
                candidate,
                prepare,
                decision: old,
                complete: None,
            } if old == decision => {
                let receipt = match self.role {
                    EndpointRole::Primary => old.primary,
                    EndpointRole::Replica => old.replica,
                };
                (candidate, prepare, receipt)
            }
            _ => {
                return Err(invalid(
                    "formal proof installation requires original durable decision",
                ));
            }
        };
        if decision.record != candidate.candidate.record
            || decision.branch != candidate.branch
            || decision.writer_signature_hex != candidate.candidate.writer_signature_hex
            || decision.frozen != candidate.frozen
            || &local
                != match self.role {
                    EndpointRole::Primary => &decision.primary,
                    EndpointRole::Replica => &decision.replica,
                }
        {
            return Err(invalid(
                "formal proof installation exact retained original decision",
            ));
        }
        let original = if decision.branch == FormalCompletionBranchV1::Normal {
            &self.authority
        } else {
            &self.original
        };
        let recovery = (decision.branch != FormalCompletionBranchV1::Normal)
            .then_some(self.recovery.as_ref())
            .flatten();
        let verified = verify_formal_decision(&decision, original, self.parent.as_ref(), recovery)?;
        // Full proof and material are synced BEFORE the complete receipt is signed.
        self.write(
            FormalRequestStateV1::Ready {
                candidate: candidate.clone(),
                prepare: prepare.clone(),
                decision: decision.clone(),
                complete: None,
            },
            false,
            guard.as_mut(),
        )?;
        let body = verified.complete_receipt_body(self.role)?;
        let complete = FormalProofCompleteReceiptV1 {
            signature_hex: self.sign_at_checkpoint(
                guard.as_mut(),
                fencing::ProtectedEndpointSignaturePurposeV1::ProofComplete,
                formal_proof_complete_signing_bytes(&body)?,
            )?,
            body,
        };
        self.write(
            FormalRequestStateV1::Ready {
                candidate,
                prepare,
                decision,
                complete: Some(complete.clone()),
            },
            false,
            guard.as_mut(),
        )?;
        Ok(complete)
    }
    pub fn publish_qualified(
        &mut self,
        proof: FormalQualifiedCommitProofV1,
    ) -> Result<FormalHeadRefV1, FormalError> {
        let mut guard = self.guard()?;
        let request = &proof.decision.record.request_digest;
        let entry = self
            .load(request)?
            .ok_or_else(|| invalid("formal qualification local complete proof missing"))?;
        if let FormalRequestStateV1::Qualified { proof: old, .. } = &entry {
            if old == &proof {
                return Ok(FormalHeadRefV1 {
                    position: proof.decision.record.position,
                    record_id: proof.decision.record.record_id()?,
                });
            }
            return Err(FormalError::Conflict(
                "formal original qualified proof".into(),
            ));
        }
        let FormalRequestStateV1::Ready {
            candidate,
            decision,
            complete: Some(complete),
            ..
        } = entry
        else {
            return Err(invalid(
                "formal qualification requires durable ProofComplete",
            ));
        };
        if proof.decision != decision
            || &complete
                != match self.role {
                    EndpointRole::Primary => &proof.primary_complete,
                    EndpointRole::Replica => &proof.replica_complete,
                }
        {
            return Err(invalid(
                "formal qualification exact locally installed proof",
            ));
        }
        let original = if proof.decision.branch == FormalCompletionBranchV1::Normal {
            &self.authority
        } else {
            &self.original
        };
        let recovery = (proof.decision.branch != FormalCompletionBranchV1::Normal)
            .then_some(self.recovery.as_ref())
            .flatten();
        let verified = verify_formal_qualified(&proof, original, self.parent.as_ref(), recovery)?;
        self.write(
            FormalRequestStateV1::Qualified { candidate, proof },
            true,
            guard.as_mut(),
        )?;
        self.parent = Some(verified.clone());
        Ok(verified.head())
    }
}

#[path = "endpoint_business_parent.rs"]
mod business_parent;
pub use business_parent::VerifiedFormalBusinessParentV1;

#[path = "endpoint_committed_material.rs"]
mod committed_material;
pub use committed_material::VerifiedCommittedHistoricalMaterialV1;
pub(crate) use committed_material::verify_committed_historical_material;

#[path = "endpoint_native_environment.rs"]
mod native_environment;

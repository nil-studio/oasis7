use super::*;
use crate::controlled_authority::fencing::EndpointInstanceBindingDirectoryExt;
use crate::controlled_authority::fencing::{
    EndpointCheckpointBindingV1, EndpointInstanceBindingV1, EpochSealBodyV1, EpochSealReceiptV1,
    FencingError, MonotonicEndpointGuard, MonotonicEndpointProvider, SealedTailKindV1,
    SealedTailV1,
};
use ed25519_dalek::SigningKey;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use storage::Fault;
#[path = "endpoint_fencing.rs"]
mod fencing_impl;
#[derive(Clone)]
struct EndpointFencing {
    instance: EndpointInstanceBindingV1,
    provider: Arc<dyn MonotonicEndpointProvider>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Preparation {
    pub(super) proposal: SignedProposal,
    pub(super) receipt: Option<DurableReceipt>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Entry {
    pub(super) proposal: SignedProposal,
    pub(super) replica_prepare: DurableReceipt,
    pub(super) primary_receipt: Option<DurableReceipt>,
    pub(super) replica_receipt: Option<DurableReceipt>,
    pub(super) published: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) frozen_certificate: Option<FrozenCertificate>,
}
impl Entry {
    pub(super) fn evidence(&self) -> Option<DurabilityEvidence> {
        Some(DurabilityEvidence {
            proposal: self.proposal.clone(),
            replica_prepare: self.replica_prepare.clone(),
            primary_receipt: self.primary_receipt.clone()?,
            replica_receipt: self.replica_receipt.clone()?,
            frozen_certificate: self.frozen_certificate.clone(),
        })
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct State {
    pub(super) schema_version: u32,
    pub(super) trust: FixedTrust,
    pub(super) role: EndpointRole,
    pub(super) prepared: Vec<Preparation>,
    pub(super) decisions: Vec<Entry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) frozen: Option<FrozenReservation>,
}
/// This endpoint owns immutable archives and its signed hot checkpoint: callers cannot advance epoch
/// or replace identity through an append request. Its lock/CAS covers cooperating
/// processes sharing this actual directory. A service must exclusively expose
/// these methods to remote writers; direct disk access is outside this contract.
pub struct FileEndpoint {
    path: PathBuf,
    _lock: crate::controlled_authority::writer_lock::WriterLock,
    key: SigningKey,
    state: State,
    archive: archive::Store,
    base: HeadAnchor,
    head: HeadAnchor,
    poisoned: bool,
    fencing: Option<EndpointFencing>,
    #[cfg(test)]
    fault: Fault,
    #[cfg(test)]
    fault_after: usize,
    #[cfg(test)]
    fail_read_sync: bool,
}
impl FileEndpoint {
    pub fn open(
        path: impl AsRef<Path>,
        trust: FixedTrust,
        role: EndpointRole,
        key: SigningKey,
        minimum: &HeadAnchor,
    ) -> Result<Self, ProtocolError> {
        Self::open_impl(path, trust, role, key, minimum, None)
    }
    pub fn open_with_fencing(
        path: impl AsRef<Path>,
        trust: FixedTrust,
        role: EndpointRole,
        key: SigningKey,
        minimum: &HeadAnchor,
        instance: EndpointInstanceBindingV1,
        provider: Arc<dyn MonotonicEndpointProvider>,
    ) -> Result<Self, ProtocolError> {
        instance
            .matches_trust(&trust, role)
            .map_err(fencing_error)?;
        Self::open_impl(
            path,
            trust,
            role,
            key,
            minimum,
            Some(EndpointFencing { instance, provider }),
        )
    }
    fn open_impl(
        path: impl AsRef<Path>,
        trust: FixedTrust,
        role: EndpointRole,
        key: SigningKey,
        minimum: &HeadAnchor,
        fencing: Option<EndpointFencing>,
    ) -> Result<Self, ProtocolError> {
        trust.validate()?;
        if hex::encode(key.verifying_key().to_bytes()) != trust.endpoint(role).1 {
            return Err(invalid("endpoint signer mismatch"));
        }
        let path = path.as_ref().to_owned();
        let lock = storage::lock(&path)?;
        let genesis = trust.genesis_anchor()?;
        let open_minimum = if fencing.is_some() { &genesis } else { minimum };
        let mut archive = if archive::Store::exists(&path) {
            archive::Store::open(&path, &trust, role, open_minimum)?
        } else {
            let mut archive = archive::Store::empty(&path, trust.clone(), role)?;
            migrate_legacy(&path, &trust, role, &mut archive)?;
            archive.minimum(minimum)?;
            archive.publish(&key, Fault::None)?;
            archive
        };
        if let Some(binding) = &fencing {
            binding
                .instance
                .verify_directory(&path)
                .map_err(fencing_error)?;
            let digest = binding.instance.digest().map_err(fencing_error)?;
            if archive
                .instance_binding()
                .is_some_and(|stored| stored != digest)
            {
                return Err(invalid("exact instance checkpoint mismatch"));
            }
            let initial = archive::Store::checkpoint_binding_at(&path)?;
            let mut guard = binding
                .provider
                .acquire(&binding.instance, &initial)
                .map_err(fencing_error)?;
            archive::Store::reconcile_guard(&path, &trust, role, minimum, guard.as_mut())?;
            archive = archive::Store::open(&path, &trust, role, minimum)?;
            if archive.seal() != guard.seal() {
                return Err(ProtocolError::Poisoned);
            }
            if archive.instance_binding().is_none() {
                archive.bind_instance(digest);
                archive.publish_guarded(&key, Fault::None, Some(guard.as_mut()))?;
            }
        } else if archive.instance_binding().is_some() {
            return Err(invalid(
                "FENCING_UNPROVEN: native bound endpoint requires provider",
            ));
        }
        let head = archive.head();
        let (state, base) = archive.hot()?;
        let endpoint = Self {
            path,
            _lock: lock,
            key,
            state,
            archive,
            base,
            head,
            poisoned: false,
            fencing,
            #[cfg(test)]
            fault: Fault::None,
            #[cfg(test)]
            fault_after: 1,
            #[cfg(test)]
            fail_read_sync: false,
        };
        Ok(endpoint)
    }
    pub(super) fn verify_rpc_minimum(&mut self, minimum: &HeadAnchor) -> Result<(), ProtocolError> {
        let _gate = self.acquire_fencing_guard()?;
        self.archive.refresh(&self.head)?.minimum(minimum)
    }
    pub(super) fn sign_rpc_response<T: Serialize>(
        &self,
        body: &T,
    ) -> Result<String, ProtocolError> {
        crypto::sign("oasis7.endpoint.rpc.response.v1", body, &self.key)
    }
    pub fn role(&self) -> EndpointRole {
        self.state.role
    }
    pub fn trust(&self) -> &FixedTrust {
        &self.state.trust
    }
    pub fn head(&self) -> HeadAnchor {
        self.head.clone()
    }
    pub(super) fn path(&self) -> &Path {
        &self.path
    }
    /// Re-read, validate and sync under the retained lock. Does not unpoison.
    /// A missing local record never proves global absence or an aborted request.
    pub fn lookup(
        &mut self,
        request: &LocalRequestIdentity,
    ) -> Result<EndpointStatus, ProtocolError> {
        request.validate().map_err(|e| invalid(e.to_string()))?;
        let _gate = self.acquire_fencing_guard()?;
        let refreshed = (|| {
            let archive = self.archive.refresh(&self.head)?;
            let head = archive.head();
            let (next, base) = archive.hot()?;
            #[cfg(test)]
            if self.fail_read_sync {
                return Err(io(std::io::Error::other("injected read sync failure")));
            }
            Ok((next, base, head, archive))
        })();
        let (next, base, head, archive) = match refreshed {
            Ok(value) => value,
            Err(error) => {
                self.poisoned = true;
                return Err(error);
            }
        };
        self.state = next;
        self.base = base;
        self.head = head;
        self.archive = archive;
        if let Some(e) = self.archived(request)? {
            return Ok(if e.published {
                EndpointStatus::Qualified(Box::new(e.evidence().expect("validated")))
            } else {
                EndpointStatus::DecisionUnqualified(Box::new(e.proposal))
            });
        }
        if let Some(e) = self
            .state
            .decisions
            .iter()
            .find(|e| &e.proposal.body.request == request)
        {
            return Ok(if e.published {
                EndpointStatus::Qualified(Box::new(e.evidence().expect("validated")))
            } else {
                EndpointStatus::DecisionUnqualified(Box::new(e.proposal.clone()))
            });
        }
        if let Some(prepared) = self
            .archive
            .prepared(request)
            .inspect_err(|_| self.poisoned = true)?
        {
            return Ok(EndpointStatus::Prepared(Box::new(prepared.proposal)));
        }
        Ok(self
            .state
            .prepared
            .iter()
            .find(|e| &e.proposal.body.request == request)
            .map(|e| EndpointStatus::Prepared(Box::new(e.proposal.clone())))
            .unwrap_or(EndpointStatus::NotRecorded))
    }
    pub fn prepare(&mut self, proposal: &SignedProposal) -> Result<DurableReceipt, ProtocolError> {
        self.ensure_writable()?;
        let mut gate = self.acquire_fencing_guard()?;
        self.check_frozen_candidate(proposal)?;
        let digest = crypto::verify_proposal(proposal, &self.state.trust)?;
        self.check_collision(proposal)?;
        if let Some(e) = self.archived(&proposal.body.request)? {
            if e.proposal != *proposal {
                return Err(invalid("request identity bound to different proposal"));
            }
            if self.role() == EndpointRole::Replica {
                return Ok(e.replica_prepare);
            }
        }
        if let Some(previous) = self
            .archive
            .prepared(&proposal.body.request)
            .inspect_err(|_| self.poisoned = true)?
        {
            if previous.proposal != *proposal {
                return Err(invalid("request identity bound to archived preparation"));
            }
            if let Some(receipt) = previous.receipt {
                return Ok(receipt);
            }
        }
        if let Some(e) = self.state.prepared.iter().find(|p| p.proposal == *proposal)
            && let Some(r) = &e.receipt
        {
            return Ok(r.clone());
        }
        if let Some(e) = self
            .state
            .decisions
            .iter()
            .find(|p| p.proposal == *proposal)
        {
            // A durable decision subsumes this preparation. Replica's original
            // prepare receipt remains stored in the decision transaction.
            if self.role() == EndpointRole::Replica {
                return Ok(e.replica_prepare.clone());
            }
        }
        self.fence(proposal)?;
        let mut next = self.state.clone();
        if !next.prepared.iter().any(|p| p.proposal == *proposal) {
            next.prepared.push(Preparation {
                proposal: proposal.clone(),
                receipt: None,
            });
            self.persist(next, &mut gate)?;
        }
        // Signing occurs only AFTER the full proposal/artifact closure fsync.
        Self::check_gate_write(&gate)?;
        let receipt = crypto::sign_receipt(
            &self.state.trust,
            self.role(),
            ReceiptStage::Prepared,
            digest,
            &self.key,
        )?;
        let mut next = self.state.clone();
        next.prepared
            .iter_mut()
            .find(|p| p.proposal == *proposal)
            .expect("prepared")
            .receipt = Some(receipt.clone());
        self.persist(next, &mut gate)?;
        Ok(receipt)
    }
    pub fn decide_primary(
        &mut self,
        proposal: &SignedProposal,
        replica_prepare: &DurableReceipt,
    ) -> Result<DurableReceipt, ProtocolError> {
        if self.role() != EndpointRole::Primary {
            return Err(invalid("primary method at replica"));
        }
        self.decide(proposal, replica_prepare, None)
    }
    pub fn decide_replica(
        &mut self,
        proposal: &SignedProposal,
        replica_prepare: &DurableReceipt,
        primary_receipt: &DurableReceipt,
    ) -> Result<DurableReceipt, ProtocolError> {
        if self.role() != EndpointRole::Replica {
            return Err(invalid("replica method at primary"));
        }
        self.decide(proposal, replica_prepare, Some(primary_receipt))
    }
    fn decide(
        &mut self,
        proposal: &SignedProposal,
        preparation: &DurableReceipt,
        primary: Option<&DurableReceipt>,
    ) -> Result<DurableReceipt, ProtocolError> {
        self.ensure_writable()?;
        let mut gate = self.acquire_fencing_guard()?;
        self.check_frozen_candidate(proposal)?;
        let digest = crypto::verify_proposal(proposal, &self.state.trust)?;
        crypto::verify_receipt(
            preparation,
            &self.state.trust,
            EndpointRole::Replica,
            ReceiptStage::Prepared,
            &digest,
        )?;
        if let Some(receipt) = primary {
            crypto::verify_receipt(
                receipt,
                &self.state.trust,
                EndpointRole::Primary,
                ReceiptStage::DecisionDurable,
                &digest,
            )?;
        }
        self.check_collision(proposal)?;
        if let Some(e) = self.archived(&proposal.body.request)? {
            if e.proposal != *proposal {
                return Err(invalid("request identity bound to different proposal"));
            }
            let receipt = if self.role() == EndpointRole::Primary {
                e.primary_receipt
            } else {
                e.replica_receipt
            };
            if let Some(receipt) = receipt {
                return Ok(receipt);
            }
        }
        let existing = self
            .state
            .decisions
            .iter()
            .position(|e| e.proposal == *proposal);
        if let Some(i) = existing {
            let local = match self.role() {
                EndpointRole::Primary => &self.state.decisions[i].primary_receipt,
                EndpointRole::Replica => &self.state.decisions[i].replica_receipt,
            };
            if let Some(r) = local {
                return Ok(r.clone());
            }
        } else {
            self.fence(proposal)?;
            if self.state.decisions.last().is_some_and(|e| !e.published) {
                return Err(invalid("finish previous decision before advancing"));
            }
            let mut next = self.state.clone();
            next.prepared
                .retain(|p| p.proposal.body.request != proposal.body.request);
            next.decisions.push(Entry {
                proposal: proposal.clone(),
                replica_prepare: preparation.clone(),
                primary_receipt: primary.cloned(),
                replica_receipt: None,
                published: false,
                frozen_certificate: self
                    .state
                    .frozen
                    .as_ref()
                    .and_then(|f| f.certificate.clone()),
            });
            // The irrevocable CAS updates history/head/request facts in ONE durable frame.
            self.persist(next, &mut gate)?;
        }
        Self::check_gate_write(&gate)?;
        let receipt = crypto::sign_receipt(
            &self.state.trust,
            self.role(),
            ReceiptStage::DecisionDurable,
            digest,
            &self.key,
        )?;
        let mut next = self.state.clone();
        let entry = next
            .decisions
            .iter_mut()
            .find(|e| e.proposal == *proposal)
            .expect("decision");
        match self.state.role {
            EndpointRole::Primary => entry.primary_receipt = Some(receipt.clone()),
            EndpointRole::Replica => entry.replica_receipt = Some(receipt.clone()),
        }
        self.persist(next, &mut gate)?;
        Ok(receipt)
    }
    /// Final public evidence is saved locally only after both durable decision
    /// attestations exist. Coordinator saves it at BOTH endpoints before return.
    pub fn finalize(&mut self, evidence: &DurabilityEvidence) -> Result<(), ProtocolError> {
        self.ensure_writable()?;
        let mut gate = self.acquire_fencing_guard()?;
        if evidence.proposal.body.position < self.head.position {
            let previous = self
                .archive
                .lookup(&evidence.proposal.body.request)?
                .ok_or_else(|| invalid("finalize archive missing"))?;
            return if previous.evidence().as_ref() == Some(evidence) && previous.published {
                Ok(())
            } else {
                Err(invalid("historical evidence disagreement"))
            };
        }
        let body = &evidence.proposal.body;
        if body.position == 0 {
            return Err(invalid("zero evidence position"));
        }
        let parent = HeadAnchor {
            position: body.position - 1,
            decision_hash: body.parent_hash.clone(),
            qualified: false,
        };
        crypto::verify_evidence(evidence, &self.state.trust, &parent)?;
        let mut next = self.state.clone();
        let entry = next
            .decisions
            .iter_mut()
            .find(|e| e.proposal == evidence.proposal)
            .ok_or_else(|| invalid("finalize without local decision"))?;
        if entry.frozen_certificate != evidence.frozen_certificate {
            return Err(invalid("different frozen certificate"));
        }
        if entry.replica_prepare != evidence.replica_prepare {
            return Err(invalid("different preparation"));
        }
        entry.primary_receipt = Some(evidence.primary_receipt.clone());
        entry.replica_receipt = Some(evidence.replica_receipt.clone());
        entry.published = true;
        if next
            .frozen
            .as_ref()
            .is_some_and(|f| f.input.body.request == evidence.proposal.body.request)
        {
            next.frozen = None;
        }
        self.persist(next, &mut gate)
    }
    /// Export locally stored evidence for recovery, not a live activation claim.
    pub fn export_evidence(&self) -> Vec<DurabilityEvidence> {
        self.try_export_evidence().expect("verified archive export")
    }
    /// After explicit reopen, regenerate a missing local receipt from the same
    /// verified/synced decision. This does not advance head or publish evidence.
    /// It lets a surviving replica finish its receipt before restoring a lost
    /// primary, even when the original reply/receipt write was interrupted.
    pub fn repair_local_receipt(
        &mut self,
        request: &LocalRequestIdentity,
    ) -> Result<ReceiptRepairOutcome, ProtocolError> {
        request.validate().map_err(|e| invalid(e.to_string()))?;
        if self.poisoned {
            return Ok(ReceiptRepairOutcome::Unknown {
                request: request.clone(),
            });
        }
        let result = self.repair_receipt_inner(request);
        match result {
            Err(_) if self.poisoned => Ok(ReceiptRepairOutcome::Unknown {
                request: request.clone(),
            }),
            other => other,
        }
    }
    fn repair_receipt_inner(
        &mut self,
        request: &LocalRequestIdentity,
    ) -> Result<ReceiptRepairOutcome, ProtocolError> {
        self.lookup(request)?;
        let Some(entry) = self.archived(request)? else {
            return Ok(ReceiptRepairOutcome::NotLocallyDecided);
        };
        let receipt = match self.role() {
            EndpointRole::Primary => {
                self.decide_primary(&entry.proposal, &entry.replica_prepare)?
            }
            EndpointRole::Replica => self.decide_replica(
                &entry.proposal,
                &entry.replica_prepare,
                entry
                    .primary_receipt
                    .as_ref()
                    .ok_or_else(|| invalid("replica missing primary receipt"))?,
            )?,
        };
        Ok(ReceiptRepairOutcome::Durable(Box::new(receipt)))
    }
    /// Restore a lost endpoint only from a complete, contiguous two-receipt
    /// history and external minimum anchor. No endpoint/key/epoch replacement.
    /// Prepared-only or unqualified history cannot be promoted by this method.
    /// An interrupted restore resumes only an exact qualified archive prefix.
    pub fn restore(
        path: impl AsRef<Path>,
        trust: FixedTrust,
        role: EndpointRole,
        key: SigningKey,
        minimum: &HeadAnchor,
        history: &[DurabilityEvidence],
    ) -> Result<Self, ProtocolError> {
        Self::restore_with_fault(path, trust, role, key, minimum, history, Fault::None)
    }
    pub(super) fn restore_with_fault(
        path: impl AsRef<Path>,
        trust: FixedTrust,
        role: EndpointRole,
        key: SigningKey,
        minimum: &HeadAnchor,
        history: &[DurabilityEvidence],
        fault: Fault,
    ) -> Result<Self, ProtocolError> {
        trust.validate()?;
        if history.is_empty() {
            return Err(invalid("empty recovery history"));
        }
        let mut endpoint = Self::open(path, trust.clone(), role, key, &trust.genesis_anchor()?)?;
        #[cfg(test)]
        endpoint.inject_after(1, fault);
        #[cfg(not(test))]
        let _ = fault;
        if !endpoint.state.prepared.is_empty()
            || endpoint.state.frozen.is_some()
            || endpoint.head.position > history.len() as u64
        {
            return Err(invalid("restore qualified prefix required"));
        }
        let prefix = endpoint.head.position;
        let mut parent = trust.genesis_anchor()?;
        for e in history {
            parent = crypto::verify_evidence(e, &trust, &parent)?;
        }
        // Validate requested minimum against the supplied complete chain before writes.
        if minimum.position > parent.position {
            return Err(invalid("restore minimum missing"));
        }
        if minimum.position == 0 {
            if minimum != &trust.genesis_anchor()? {
                return Err(invalid("false restore genesis"));
            }
        } else {
            let supplied = history
                .get((minimum.position - 1) as usize)
                .ok_or_else(|| invalid("restore minimum"))?;
            if crypto::verify_proposal(&supplied.proposal, &trust)? != minimum.decision_hash {
                return Err(invalid("restore minimum fork"));
            }
        }
        for e in history.iter().take(prefix as usize) {
            let existing = endpoint
                .archive
                .lookup(&e.proposal.body.request)?
                .ok_or_else(|| invalid("restore prefix missing"))?;
            if !existing.published || existing.evidence().as_ref() != Some(e) {
                return Err(invalid("restore prefix disagreement"));
            }
        }
        for e in history.iter().skip(prefix as usize) {
            let mut next = endpoint.state.clone();
            next.decisions.push(Entry {
                proposal: e.proposal.clone(),
                replica_prepare: e.replica_prepare.clone(),
                primary_receipt: Some(e.primary_receipt.clone()),
                replica_receipt: Some(e.replica_receipt.clone()),
                published: true,
                frozen_certificate: e.frozen_certificate.clone(),
            });
            let mut gate = endpoint.acquire_fencing_guard()?;
            endpoint.persist(next, &mut gate)?;
        }
        endpoint.archive.minimum(minimum)?;
        Ok(endpoint)
    }
    fn fence(&self, proposal: &SignedProposal) -> Result<(), ProtocolError> {
        if self.head.position.checked_add(1) != Some(proposal.body.position)
            || self.head.decision_hash != proposal.body.parent_hash
        {
            return Err(invalid("atomic expected epoch/head/sequence fence"));
        }
        Ok(())
    }
    fn check_collision(&self, proposal: &SignedProposal) -> Result<(), ProtocolError> {
        // Reserve a slot when preparing, before either endpoint signs a receipt.
        // A final append CAS alone would permit competing durable candidates.
        if self
            .state
            .prepared
            .iter()
            .any(|p| p.proposal.body.position == proposal.body.position && p.proposal != *proposal)
        {
            return Err(invalid("slot already reserved by another proposal"));
        }
        let previous = self
            .state
            .decisions
            .iter()
            .map(|e| &e.proposal)
            .chain(self.state.prepared.iter().map(|e| &e.proposal))
            .find(|p| p.body.request == proposal.body.request);
        if previous.is_some_and(|p| p != proposal) {
            return Err(invalid(
                "request identity bound to different payload/record/parent",
            ));
        }
        Ok(())
    }
    fn check_frozen_candidate(&self, proposal: &SignedProposal) -> Result<(), ProtocolError> {
        if let Some(frozen) = &self.state.frozen
            && frozen.input.body.next_position == proposal.body.position
        {
            frozen
                .certificate
                .as_ref()
                .ok_or_else(|| invalid("complete frozen certificate before prepare"))?
                .verify_candidate(proposal)?;
        }
        if proposal.body.frozen_input_digest.is_some()
            && proposal.body.position > self.head.position
        {
            self.state
                .frozen
                .as_ref()
                .and_then(|f| f.certificate.as_ref())
                .ok_or_else(|| invalid("frozen candidate without reservation"))?
                .verify_candidate(proposal)?;
        }
        Ok(())
    }
    pub fn freeze_input(
        &mut self,
        input: &SignedFrozenInput,
    ) -> Result<FrozenReceipt, ProtocolError> {
        self.ensure_writable()?;
        let mut gate = self.acquire_fencing_guard()?;
        input.verify(self.trust())?;
        if let Some(current) = &self.state.frozen {
            if current.input != *input {
                return Err(invalid("different frozen input already reserves slot"));
            }
            if let Some(receipt) = &current.local_receipt {
                return Ok(receipt.clone());
            }
        } else {
            if input.body.parent != self.head
                || self.head.position.checked_add(1) != Some(input.body.next_position)
                || !self.state.prepared.is_empty()
            {
                return Err(invalid("frozen epoch/head/slot fence"));
            }
            let previous = self
                .state
                .decisions
                .last()
                .ok_or_else(|| invalid("frozen predecessor absent"))?;
            if !previous.published || previous.proposal.body.record != input.body.predecessor_record
            {
                return Err(invalid("frozen predecessor differs from qualified head"));
            }
            if self.archived(&input.body.request)?.is_some()
                || self.archive.prepared(&input.body.request)?.is_some()
            {
                return Err(invalid("frozen identity already decided"));
            }
            let mut next = self.state.clone();
            next.frozen = Some(FrozenReservation {
                input: input.clone(),
                local_receipt: None,
                certificate: None,
            });
            self.persist(next, &mut gate)?;
        }
        Self::check_gate_write(&gate)?;
        let receipt = FrozenReceipt::sign(input, self.role(), &self.key)?;
        let mut next = self.state.clone();
        next.frozen.as_mut().expect("frozen").local_receipt = Some(receipt.clone());
        self.persist(next, &mut gate)?;
        Ok(receipt)
    }
    pub fn complete_frozen(
        &mut self,
        certificate: &FrozenCertificate,
    ) -> Result<(), ProtocolError> {
        self.ensure_writable()?;
        let mut gate = self.acquire_fencing_guard()?;
        certificate.verify(self.trust())?;
        let mut next = self.state.clone();
        let frozen = next
            .frozen
            .as_mut()
            .ok_or_else(|| invalid("complete without frozen reservation"))?;
        if frozen.input != certificate.input {
            return Err(invalid("frozen certificate input disagreement"));
        }
        frozen.certificate = Some(certificate.clone());
        frozen.validate(self.trust(), self.role())?;
        self.persist(next, &mut gate)
    }
    pub fn query_frozen(
        &mut self,
        request: &LocalRequestIdentity,
    ) -> Result<FrozenStatus, ProtocolError> {
        self.lookup(request)?;
        if let Some(frozen) = &self.state.frozen
            && frozen.input.body.request == *request
        {
            return Ok(FrozenStatus::Reserved(Box::new(frozen.clone())));
        }
        if let Some(entry) = self.archived(request)?
            && let Some(certificate) = entry.frozen_certificate
        {
            let local_receipt = if self.role() == EndpointRole::Primary {
                certificate.primary_receipt.clone()
            } else {
                certificate.replica_receipt.clone()
            };
            return Ok(FrozenStatus::Reserved(Box::new(FrozenReservation {
                input: certificate.input.clone(),
                local_receipt: Some(local_receipt),
                certificate: Some(certificate),
            })));
        }
        Ok(FrozenStatus::NotRecorded)
    }
    fn archived(&mut self, request: &LocalRequestIdentity) -> Result<Option<Entry>, ProtocolError> {
        self.archive
            .lookup(request)
            .inspect_err(|_| self.poisoned = true)
    }
    pub fn try_export_evidence(&self) -> Result<Vec<DurabilityEvidence>, ProtocolError> {
        self.archive.export()
    }
    fn ensure_writable(&self) -> Result<(), ProtocolError> {
        if self.poisoned {
            Err(ProtocolError::Poisoned)
        } else {
            Ok(())
        }
    }
    fn persist(
        &mut self,
        next: State,
        gate: &mut Option<Box<dyn MonotonicEndpointGuard>>,
    ) -> Result<(), ProtocolError> {
        Self::check_gate_write(gate)?;
        validate_tail(
            &next,
            &self.state.trust,
            self.state.role,
            &self.base,
            &self.head,
        )?;
        #[cfg(test)]
        let fault = if self.fault_after <= 1 {
            self.fault
        } else {
            self.fault_after -= 1;
            Fault::None
        };
        #[cfg(not(test))]
        let fault = Fault::None;
        let result = (|| {
            self.archive.stage(&next)?;
            self.archive.publish_guarded(
                &self.key,
                fault,
                gate.as_mut()
                    .map(|g| g.as_mut() as &mut dyn MonotonicEndpointGuard),
            )
        })();
        if let Err(error) = result {
            self.poisoned = true;
            return Err(error);
        }
        let (state, base) = self.archive.hot()?;
        self.state = state;
        self.base = base;
        self.head = self.archive.head();
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn resident_record_bytes(&self) -> usize {
        let frozen_bytes = |input: &SignedFrozenInput| {
            input.body.predecessor_record.byte_len()
                + input.body.continuation_bytes.len()
                + input.body.action_bytes.len()
        };
        let decisions = self
            .state
            .decisions
            .iter()
            .map(|e| {
                e.proposal.body.record.byte_len()
                    + e.frozen_certificate
                        .as_ref()
                        .map(|c| frozen_bytes(&c.input))
                        .unwrap_or(0)
            })
            .sum::<usize>();
        let preparations = self
            .state
            .prepared
            .iter()
            .map(|p| p.proposal.body.record.byte_len())
            .sum::<usize>();
        let reservation = self
            .state
            .frozen
            .as_ref()
            .map(|f| {
                frozen_bytes(&f.input)
                    + f.certificate
                        .as_ref()
                        .map(|c| frozen_bytes(&c.input))
                        .unwrap_or(0)
            })
            .unwrap_or(0);
        decisions + preparations + reservation
    }
    #[cfg(test)]
    pub(super) fn inject_after(&mut self, writes: usize, fault: Fault) {
        self.fault = fault;
        self.fault_after = writes;
    }
    #[cfg(test)]
    pub(super) fn inject_read_sync_failure(&mut self, fail: bool) {
        self.fail_read_sync = fail;
    }
}

fn validate_state(
    state: &State,
    trust: &FixedTrust,
    role: EndpointRole,
    minimum: &HeadAnchor,
) -> Result<HeadAnchor, ProtocolError> {
    validate_tail(state, trust, role, &trust.genesis_anchor()?, minimum)
}
pub(super) fn validate_tail(
    state: &State,
    trust: &FixedTrust,
    role: EndpointRole,
    base: &HeadAnchor,
    minimum: &HeadAnchor,
) -> Result<HeadAnchor, ProtocolError> {
    if state.schema_version != 1 || &state.trust != trust || state.role != role {
        return Err(invalid("endpoint identity/schema"));
    }
    crypto::hash_hex(&minimum.decision_hash)?;
    if minimum.position == 0 && minimum != &trust.genesis_anchor()? {
        return Err(invalid("false genesis minimum"));
    }
    if let Some(frozen) = &state.frozen {
        frozen.validate(trust, role)?;
    }
    let mut requests = std::collections::BTreeSet::new();
    let mut head = base.clone();
    let mut minimum_seen = minimum == base;
    for e in &state.decisions {
        let digest = crypto::verify_proposal(&e.proposal, trust)?;
        match (&e.frozen_certificate, &e.proposal.body.frozen_input_digest) {
            (Some(cert), Some(_)) => cert.verify_candidate(&e.proposal)?,
            (None, None) => {}
            _ => return Err(invalid("decision frozen certificate missing/mismatch")),
        }
        let b = &e.proposal.body;
        if b.position != head.position + 1
            || b.parent_hash != head.decision_hash
            || !requests.insert(b.request.clone())
        {
            return Err(invalid("decision history/request fork"));
        }
        crypto::verify_receipt(
            &e.replica_prepare,
            trust,
            EndpointRole::Replica,
            ReceiptStage::Prepared,
            &digest,
        )?;
        if role == EndpointRole::Replica && e.primary_receipt.is_none() {
            return Err(invalid("replica decision missing primary receipt"));
        }
        for (receipt, receipt_role) in [
            (&e.primary_receipt, EndpointRole::Primary),
            (&e.replica_receipt, EndpointRole::Replica),
        ] {
            if let Some(r) = receipt {
                crypto::verify_receipt(
                    r,
                    trust,
                    receipt_role,
                    ReceiptStage::DecisionDurable,
                    &digest,
                )?;
            }
        }
        if e.published {
            crypto::verify_evidence(
                &e.evidence()
                    .ok_or_else(|| invalid("published without dual receipts"))?,
                trust,
                &head,
            )?;
        }
        head = HeadAnchor {
            position: b.position,
            decision_hash: digest,
            qualified: e.published,
        };
        if head.position == minimum.position {
            if head.decision_hash != minimum.decision_hash || (minimum.qualified && !head.qualified)
            {
                return Err(invalid("minimum decision/evidence anchor mismatch"));
            }
            minimum_seen = true;
        }
    }
    if let Some(frozen) = &state.frozen {
        if frozen.input.body.parent != head {
            let active = state
                .decisions
                .last()
                .ok_or_else(|| invalid("frozen reservation outside head"))?;
            if frozen.input.body.next_position != head.position
                || active.proposal.body.request != frozen.input.body.request
                || active.published
            {
                return Err(invalid("frozen reservation outside pending slot"));
            }
            frozen
                .certificate
                .as_ref()
                .ok_or_else(|| invalid("pending frozen decision missing certificate"))?
                .verify_candidate(&active.proposal)?;
        } else {
            let predecessor = state
                .decisions
                .last()
                .ok_or_else(|| invalid("frozen predecessor missing"))?;
            if !predecessor.published
                || predecessor.proposal.body.record != frozen.input.body.predecessor_record
            {
                return Err(invalid("frozen predecessor record mismatch"));
            }
        }
    }
    let mut prepared_slots = std::collections::BTreeMap::new();
    for p in &state.prepared {
        if let Some(previous) = prepared_slots.insert(p.proposal.body.position, &p.proposal)
            && previous != &p.proposal
        {
            return Err(invalid("competing persisted preparations"));
        }
        let digest = crypto::verify_proposal(&p.proposal, trust)?;
        if p.proposal.body.frozen_input_digest.is_some() {
            state
                .frozen
                .as_ref()
                .and_then(|f| f.certificate.as_ref())
                .ok_or_else(|| invalid("prepared frozen certificate missing"))?
                .verify_candidate(&p.proposal)?;
        }
        if !requests.insert(p.proposal.body.request.clone()) {
            return Err(invalid("duplicate prepared request"));
        }
        if let Some(r) = &p.receipt {
            crypto::verify_receipt(r, trust, role, ReceiptStage::Prepared, &digest)?;
        }
        // Competing preparations may become stale; preserve them as pending,
        // never infer abort or apply them to the decided world.
    }
    if !minimum_seen {
        return Err(invalid("rollback/minimum anchor"));
    }
    Ok(head)
}

fn migrate_legacy(
    path: &Path,
    trust: &FixedTrust,
    role: EndpointRole,
    archive: &mut archive::Store,
) -> Result<(), ProtocolError> {
    if let Some(bytes) = storage::read_legacy(path)? {
        let old: State = serde_json::from_slice(&bytes).map_err(|e| invalid(e.to_string()))?;
        validate_state(&old, trust, role, &trust.genesis_anchor()?)?;
        for entry in old.decisions {
            let (mut next, base) = archive.hot()?;
            next.decisions.push(entry);
            validate_tail(&next, trust, role, &base, &archive.head())?;
            archive.stage(&next)?;
        }
        let (mut next, base) = archive.hot()?;
        next.prepared = old.prepared;
        validate_tail(&next, trust, role, &base, &archive.head())?;
        archive.stage(&next)?;
    }
    journal::visit(path, |identity, prepared, changes| {
        if identity["trust"] != serde_json::to_value(trust).map_err(|e| invalid(e.to_string()))?
            || identity["role"] != serde_json::to_value(role).map_err(|e| invalid(e.to_string()))?
            || identity["schema_version"] != 1
        {
            return Err(invalid("migration identity"));
        }
        let (mut next, base) = archive.hot()?;
        next.prepared =
            serde_json::from_value(prepared.clone()).map_err(|e| invalid(e.to_string()))?;
        for (index, value) in changes {
            let entry: Entry =
                serde_json::from_value(value.clone()).map_err(|e| invalid(e.to_string()))?;
            if entry.proposal.body.position != *index as u64 + 1 {
                return Err(invalid("migration entry index"));
            }
            if entry.proposal.body.position == archive.head().position {
                next.decisions = vec![entry];
            } else if entry.proposal.body.position == archive.head().position + 1 {
                next.decisions.push(entry);
            } else {
                return Err(invalid("migration historical rewrite/gap"));
            }
        }
        validate_tail(&next, trust, role, &base, &archive.head())?;
        archive.stage(&next)
    })
}

pub(super) fn fencing_error(error: FencingError) -> ProtocolError {
    match error {
        FencingError::Rollback => ProtocolError::Poisoned,
        FencingError::Io(s) => io(std::io::Error::other(s)),
        FencingError::Locked => io(std::io::Error::new(
            std::io::ErrorKind::WouldBlock,
            "fencing provider busy",
        )),
        other => invalid(other.to_string()),
    }
}

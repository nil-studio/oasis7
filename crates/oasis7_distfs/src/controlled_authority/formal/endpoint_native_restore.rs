//! Private promotion from authenticated protected RPC/native prefix. A raw
//! Query result or independently signed head/minimum cannot construct this token.
use super::*;
use crate::controlled_authority::formal::transport::VerifiedNativeRpcContextV1;
#[derive(Clone)]
pub struct VerifiedNativeCheckpointRestoreV1 {
    commit: VerifiedFormalCommitV1,
    active_authority: VerifiedAuthorityV1,
    minimum: FormalHeadRefV1,
    candidate: FormalCandidateMaterialV1,
    proof: FormalQualifiedCommitProofV1,
    pending: Option<FormalRequestStateV1>,
    checkpoint: fencing::EndpointCheckpointBindingV1,
    instance_digest: String,
    resource_binding: String,
}
impl VerifiedNativeCheckpointRestoreV1 {
    pub fn active_authority(&self) -> &VerifiedAuthorityV1 {
        &self.active_authority
    }
    pub fn minimum(&self) -> &FormalHeadRefV1 {
        &self.minimum
    }
    /// Advances an already trusted read lineage using the ancestry bridge from
    /// this client's own request. No new external pin is established here.
    pub fn advance_read_anchor(
        &self,
        anchor: &VerifiedFormalReadAnchorV1,
    ) -> Result<VerifiedFormalReadAnchorV1, FormalError> {
        if anchor.head().head() != self.minimum {
            return Err(invalid("native restore read lineage minimum"));
        }
        if anchor.head().head() == self.commit.head() {
            if anchor.head().record() != self.commit.record()
                || anchor.head().finalizing_authority_id() != self.commit.finalizing_authority_id()
            {
                return Err(invalid("native restore exact unchanged read anchor"));
            }
            return Ok(anchor.clone());
        }
        anchor.after_verified_commit(self.active_authority.clone(), self.commit.clone())
    }

    /// Retain a previously verified origin-to-head ancestry bridge after a
    /// fresh restoration of the exact same qualified record. RPC floors are
    /// never changed; this composes opaque evidence for a trusted read lineage.
    /// `prior_endpoint` binds freshness to this endpoint's own previous floor,
    /// because independent endpoint revision counters cannot be compared.
    pub fn with_verified_ancestry_bridge(
        mut self,
        bridge: &Self,
        prior_endpoint: &Self,
    ) -> Result<Self, FormalError> {
        if self.minimum != bridge.commit.head()
            || self.commit.record() != bridge.commit.record()
            || self.commit.head() != bridge.commit.head()
            || self.commit.finalizing_authority_id() != bridge.commit.finalizing_authority_id()
            || self.candidate != bridge.candidate
            || self.proof != bridge.proof
            || self.active_authority.context_digest()?
                != bridge.active_authority.context_digest()?
            || self.instance_digest != prior_endpoint.instance_digest
            || self.resource_binding != prior_endpoint.resource_binding
            || prior_endpoint.minimum != bridge.minimum
            || self.active_authority.context_digest()?
                != prior_endpoint.active_authority.context_digest()?
            || self.checkpoint.revision < prior_endpoint.checkpoint.revision
            || (self.checkpoint.revision == prior_endpoint.checkpoint.revision
                && self.checkpoint != prior_endpoint.checkpoint)
            || !self.checkpoint.head.qualified
            || self.checkpoint.head.position != self.commit.head().position
            || self.checkpoint.head.decision_hash != self.commit.head().record_id
        {
            return Err(invalid("native restore exact opaque ancestry bridge"));
        }
        // Keep the new protected checkpoint, full material and pending slot.
        self.minimum = bridge.minimum.clone();
        Ok(self)
    }

    pub fn commit(&self) -> &VerifiedFormalCommitV1 {
        &self.commit
    }
    pub fn material(&self) -> &crate::controlled_authority::replicated_protocol::ClosedRecord {
        &self.candidate.material
    }
    pub fn candidate(&self) -> &FormalCandidateMaterialV1 {
        &self.candidate
    }
    pub fn proof(&self) -> &FormalQualifiedCommitProofV1 {
        &self.proof
    }
    pub fn pending(&self) -> Option<&FormalRequestStateV1> {
        self.pending.as_ref()
    }
    pub fn checkpoint(&self) -> &fencing::EndpointCheckpointBindingV1 {
        &self.checkpoint
    }
}
fn object<T: serde::de::DeserializeOwned>(
    id: &str,
    load: &mut impl FnMut(&str) -> Result<Vec<u8>, FormalError>,
) -> Result<T, FormalError> {
    hash_hex(id)?;
    let bytes = load(id)?;
    if bytes.len() as u64 > FORMAL_ARCHIVE_LIMIT || blake3::hash(&bytes).to_hex().as_str() != id {
        return Err(invalid("native restore referenced CAS checksum/bound"));
    }
    serde_cbor::from_slice(&bytes).map_err(|e| invalid(e.to_string()))
}
const FORMAL_ARCHIVE_LIMIT: u64 = 64 * 1024 * 1024;
fn indexed(
    body: &HotBody,
    request: &str,
    id: &str,
    load: &mut impl FnMut(&str) -> Result<Vec<u8>, FormalError>,
) -> Result<(), FormalError> {
    if Store::lookup_from_root(body.index.as_deref(), request, load)?.as_deref() != Some(id) {
        return Err(invalid("native restore head/pending request inclusion"));
    }
    Ok(())
}
pub(crate) fn restore_native_with_durable_set(
    context: &VerifiedNativeRpcContextV1,
    original: &VerifiedAuthorityV1,
    recovery: Option<&VerifiedRecovery>,
    imported_cut: Option<&VerifiedFormalCommitV1>,
    durable_set: Option<&VerifiedDurableSetActivationV1>,
    mut load: impl FnMut(&str) -> Result<Vec<u8>, FormalError>,
) -> Result<VerifiedNativeCheckpointRestoreV1, FormalError> {
    let verified_body =
        Store::verify_public(context.checkpoint_bytes(), context.endpoint_public_key())?;
    let body = &verified_body;
    let node_id = body
        .head_node
        .as_ref()
        .or(body.imported_head_node.as_ref())
        .ok_or_else(|| invalid("native restore no qualified head"))?;
    let node: HistoryNode = object(node_id, &mut load)?;
    if node.head != body.head || node.ancestors.len() > 64 {
        return Err(invalid("native restore authenticated head node"));
    }
    let entry: FormalRequestStateV1 = object(&node.entry, &mut load)?;
    let FormalRequestStateV1::Qualified {
        candidate, proof, ..
    } = entry
    else {
        return Err(invalid("native restore qualified entry required"));
    };
    if node.request != candidate.candidate.record.request_digest
        || candidate.candidate.record != proof.decision.record
        || proof.decision.record.record_id() != Ok(body.head.record_id.clone())
        || proof.decision.record.position != body.head.position
        || proof.decision.writer_signature_hex != candidate.candidate.writer_signature_hex
        || proof.decision.frozen != candidate.frozen
        || proof.decision.branch != candidate.branch
    {
        return Err(invalid("native restore canonical proof/candidate/head"));
    }
    indexed(body, &node.request, &node.entry, &mut load)?;
    verify_record_material(&proof.decision.record, &candidate.material)?;
    let mut embedded = super::native_history::EmbeddedBusinessNodes::default();
    if context.authority().definition().profile_version == 2 {
        embedded.include_material(&proof.decision.record, &candidate.material)?;
    }
    match (&candidate.frozen, &candidate.frozen_material) {
        (Some(cert), Some(material)) => verify_frozen_material(&cert.reservation.body, material)?,
        (None, None) if proof.decision.record.kind == FormalRecordKindV1::InitialActivation => {}
        _ => return Err(invalid("native restore full original frozen material")),
    }
    let mut business_parent = None;
    let parent = if let Some(link) = node.ancestors.first() {
        if link.head.position.checked_add(1) != Some(node.head.position) {
            return Err(invalid("native restore immediate parent position"));
        }
        let parent_node: HistoryNode = object(&link.node, &mut load)?;
        if parent_node.head != link.head {
            return Err(invalid("native restore parent node hash/identity"));
        }
        let FormalRequestStateV1::Qualified {
            candidate: parent_candidate,
            proof: parent,
            ..
        } = object(&parent_node.entry, &mut load)?
        else {
            return Err(invalid("native restore immutable qualified parent"));
        };
        if parent.decision.record.position != link.head.position
            || parent.decision.record.record_id() != Ok(link.head.record_id.clone())
            || parent.decision.primary.body.finalizing_authority_id
                != parent.decision.replica.body.finalizing_authority_id
        {
            return Err(invalid("native restore canonical parent context"));
        }
        if context.authority().definition().profile_version == 2 {
            if parent_candidate.candidate.record != parent.decision.record
                || parent_candidate.candidate.writer_signature_hex
                    != parent.decision.writer_signature_hex
                || parent_candidate.branch != parent.decision.branch
                || parent_candidate.frozen != parent.decision.frozen
            {
                return Err(invalid("native restore parent canonical candidate/proof"));
            }
            verify_record_material(&parent.decision.record, &parent_candidate.material)?;
            embedded.include_material(&parent.decision.record, &parent_candidate.material)?;
        }
        business_parent = Some(VerifiedFormalBusinessParentV1::from_authenticated_parent(
            &CheckpointParent {
                record: parent.decision.record.clone(),
                finalizing_authority_id: parent
                    .decision
                    .primary
                    .body
                    .finalizing_authority_id
                    .clone(),
            },
            parent.clone(),
        )?);
        Some(CheckpointParent {
            record: parent.decision.record,
            finalizing_authority_id: parent.decision.primary.body.finalizing_authority_id,
        })
    } else {
        body.base_parent.clone()
    };
    let commit = if body.head_node.is_none()
        && let Some(activation) = durable_set
    {
        let cut = activation.boundary();
        if recovery.is_some()
            || activation.successor().context_digest()? != context.authority().context_digest()?
            || activation.predecessor().definition().world != original.definition().world
            || cut.head() != body.head
            || cut.record() != &proof.decision.record
            || body.import_digest.as_deref() != Some(activation.transition_digest())
            || body.durable_set.as_ref().is_none_or(|binding| {
                binding.transition_digest != activation.transition_digest()
                    || binding.activation_digest.as_deref() != Some(activation.activation_digest())
            })
            || body.base_parent.as_ref().is_none_or(|parent| {
                parent.record != *cut.record()
                    || parent.finalizing_authority_id != cut.finalizing_authority_id()
            })
        {
            return Err(invalid("native restore exact healthy activated boundary"));
        }
        cut.clone()
    } else if body.head_node.is_none() {
        let recovery =
            recovery.ok_or_else(|| invalid("native restore imported cut recovery context"))?;
        let cut =
            imported_cut.ok_or_else(|| invalid("native restore imported cut verified token"))?;
        if recovery.successor().authority_id()? != context.authority().authority_id()?
            || recovery.original_authority().authority_id()? != original.authority_id()?
            || recovery.body().sealed_cut != body.head
            || cut.head() != body.head
            || cut.record() != &proof.decision.record
            || body.import_digest.is_none()
            || body.base_parent.as_ref().is_none_or(|parent| {
                parent.record != *cut.record()
                    || parent.finalizing_authority_id != cut.finalizing_authority_id()
            })
        {
            return Err(invalid("native restore exact verified imported cut"));
        }
        cut.clone()
    } else {
        let verdict = NativeCheckpointVerdictV1::verify(
            context.checkpoint_bytes(),
            context.authority(),
            context.endpoint_public_key(),
            context.checkpoint(),
            context.provider(),
            &body.head,
            context.minimum(),
            &context.authority().authority_id()?,
            &mut load,
        )?;
        if verdict.proof() != &proof
            || verdict.verified_parent_record() != parent.as_ref().map(|p| &p.record)
            || verdict.verified_parent_finalizing_authority_id()
                != parent.as_ref().map(|p| p.finalizing_authority_id.as_str())
        {
            return Err(invalid("native checked checkpoint exact parent/proof"));
        }
        let origin = if proof.decision.branch == FormalCompletionBranchV1::Normal {
            context.authority()
        } else {
            original
        };
        let recovery = (proof.decision.branch != FormalCompletionBranchV1::Normal)
            .then_some(recovery)
            .flatten();
        restore_formal_commit_from_checkpoint(&proof, origin, recovery, &verdict)?
    };
    {
        use super::super::business_index::BusinessIndexNodeReader;
        let reader = NativeBusinessReader {
            load: std::cell::RefCell::new(&mut load),
            embedded: &embedded,
        };
        if let Some(verified) = super::super::business_material::verify_formal_business_material(
            context.authority(),
            &proof.decision.record,
            &candidate.material,
            business_parent.as_ref(),
            candidate.frozen_material.as_ref(),
            &reader,
        )? {
            for write in verified
                .node_writes()
                .iter()
                .chain(verified.retained_node_writes())
            {
                if reader.read_node(&write.digest).map_err(invalid)?.as_deref()
                    != Some(write.bytes.as_slice())
                {
                    return Err(invalid("native profile2 business node missing/corrupt"));
                }
            }
        }
    }
    let pending = body
        .pending
        .as_ref()
        .map(|id| -> Result<FormalRequestStateV1, FormalError> {
            let pending: FormalRequestStateV1 = object(id, &mut load)?;
            indexed(body, pending.request_digest(), id, &mut load)?;
            if pending.position()
                != commit
                    .head()
                    .position
                    .checked_add(1)
                    .ok_or_else(|| invalid("native pending overflow"))?
                || matches!(pending, FormalRequestStateV1::Qualified { .. })
            {
                return Err(invalid("native restore pending unique next slot"));
            }
            Ok(pending)
        })
        .transpose()?;
    Ok(VerifiedNativeCheckpointRestoreV1 {
        commit,
        active_authority: context.authority().clone(),
        minimum: context.minimum().clone(),
        candidate,
        proof,
        pending,
        checkpoint: context.checkpoint().clone(),
        instance_digest: context.instance_digest().to_owned(),
        resource_binding: context.resource_binding().to_owned(),
    })
}

struct NativeBusinessReader<'a, F> {
    load: std::cell::RefCell<&'a mut F>,
    embedded: &'a super::native_history::EmbeddedBusinessNodes,
}
impl<F: FnMut(&str) -> Result<Vec<u8>, FormalError>>
    super::super::business_index::BusinessIndexNodeReader for NativeBusinessReader<'_, F>
{
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        if let Some(bytes) = self.embedded.read_node(digest)? {
            return Ok(Some(bytes));
        }
        (self.load.borrow_mut())(digest)
            .map(Some)
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
#[path = "endpoint_native_bridge_tests.rs"]
mod bridge_tests;
#[cfg(test)]
pub(super) use bridge_tests::assert_bridge_negatives;

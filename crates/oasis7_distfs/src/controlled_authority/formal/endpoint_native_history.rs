//! Bounded historical restoration beneath a fresh protected native checkpoint.
//! A caller's historical reference is a lookup target, never a new trust pin.
use super::*;
use crate::controlled_authority::formal::transport::VerifiedNativeRpcContextV1;
use crate::controlled_authority::replicated_protocol::ClosedRecord;
use std::collections::BTreeMap;

const MAX_EMBEDDED_BUSINESS_NODES: usize = 4096;
const MAX_EMBEDDED_BUSINESS_BYTES: usize = 16 * 1024 * 1024;

pub struct VerifiedNativeHistoricalRestoreV1 {
    commit: VerifiedFormalCommitV1,
    authority: VerifiedAuthorityV1,
    candidate: FormalCandidateMaterialV1,
    proof: FormalQualifiedCommitProofV1,
    current_head: FormalHeadRefV1,
    minimum: FormalHeadRefV1,
    checkpoint: fencing::EndpointCheckpointBindingV1,
    provenance: NativeCheckpointVerdictV1,
}
impl VerifiedNativeHistoricalRestoreV1 {
    /// Private service publication recovery, never a new ordinary-reader pin.
    /// The pure constructor retains exact restored authority/head context even
    /// when this is the same canonical cut as the configured minimum.
    pub fn service_read_anchor(
        &self,
        trusted: &VerifiedFormalReadAnchorV1,
    ) -> Result<VerifiedFormalReadAnchorV1, FormalError> {
        VerifiedFormalReadAnchorV1::from_native_historical_service_restore(
            trusted,
            &self.provenance,
            &self.authority,
            &self.commit,
        )
    }
    pub fn reference(&self) -> FormalHeadRefV1 {
        self.commit.head()
    }
    pub fn commit(&self) -> &VerifiedFormalCommitV1 {
        &self.commit
    }
    pub fn authority(&self) -> &VerifiedAuthorityV1 {
        &self.authority
    }
    pub fn candidate(&self) -> &FormalCandidateMaterialV1 {
        &self.candidate
    }
    pub fn material(&self) -> &ClosedRecord {
        &self.candidate.material
    }
    pub fn proof(&self) -> &FormalQualifiedCommitProofV1 {
        &self.proof
    }
    pub fn current_head(&self) -> &FormalHeadRefV1 {
        &self.current_head
    }
    pub fn minimum(&self) -> &FormalHeadRefV1 {
        &self.minimum
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
    if bytes.len() > 64 * 1024 * 1024 || blake3::hash(&bytes).to_hex().as_str() != id {
        return Err(invalid("historical native referenced CAS checksum/bound"));
    }
    serde_cbor::from_slice(&bytes).map_err(|error| invalid(error.to_string()))
}
fn descend(
    body: &HotBody,
    position: u64,
    record_id: Option<&str>,
    load: &mut impl FnMut(&str) -> Result<Vec<u8>, FormalError>,
) -> Result<HistoryNode, FormalError> {
    if let Some(id) = record_id {
        hash_hex(id)?;
    }
    if position == 0 || position > body.head.position {
        return Err(FormalError::HistoryUnavailable(
            "HistoryUnavailable: historical qualified position".into(),
        ));
    }
    let mut id = body
        .head_node
        .as_ref()
        .or(body.imported_head_node.as_ref())
        .ok_or_else(|| {
            FormalError::HistoryUnavailable("HistoryUnavailable: native history root".into())
        })?
        .clone();
    let mut expected = body.head.clone();
    // Each hop removes its largest available power of two. With 64-bit positions
    // this is bounded independently of total historical record count.
    for _ in 0..=64 {
        let node: HistoryNode = object(&id, load)?;
        if node.head != expected || node.ancestors.len() > 64 {
            return Err(invalid("historical authenticated node/head/skip bound"));
        }
        if node.head.position == position {
            if record_id.is_some_and(|id| node.head.record_id != id) {
                return Err(invalid("historical exact canonical reference/fork"));
            }
            return Ok(node);
        }
        let distance = node
            .head
            .position
            .checked_sub(position)
            .ok_or_else(|| invalid("historical ancestry position"))?;
        let wanted = (63 - distance.leading_zeros()) as usize;
        let level = wanted.min(node.ancestors.len().checked_sub(1).ok_or_else(|| {
            FormalError::HistoryUnavailable(
                "HistoryUnavailable: absent historical skip link".into(),
            )
        })?);
        let link = &node.ancestors[level];
        let step = 1u64
            .checked_shl(level as u32)
            .ok_or_else(|| invalid("historical skip overflow"))?;
        if node.head.position.checked_sub(step) != Some(link.head.position)
            || link.head.position < position
        {
            return Err(invalid("historical authenticated skip distance"));
        }
        hash_hex(&link.node)?;
        hash_hex(&link.head.record_id)?;
        id = link.node.clone();
        expected = link.head.clone();
    }
    Err(invalid("historical skip traversal bound"))
}
#[expect(
    clippy::too_many_arguments,
    reason = "historical restoration binds target, original and active authorities, optional recovery and durable activation, and authenticated loader"
)]
pub(crate) fn restore_native_position(
    context: VerifiedNativeRpcContextV1,
    position: u64,
    record_id: Option<&str>,
    historical_authority: &VerifiedAuthorityV1,
    original: &VerifiedAuthorityV1,
    recovery: Option<&VerifiedRecovery>,
    durable_set: Option<&VerifiedDurableSetActivationV1>,
    mut load: impl FnMut(&str) -> Result<Vec<u8>, FormalError>,
) -> Result<VerifiedNativeHistoricalRestoreV1, FormalError> {
    let body = Store::verify_public(context.checkpoint_bytes(), context.endpoint_public_key())?;
    if historical_authority.policy() != context.authority().policy()
        || historical_authority.activation_ref() != context.authority().activation_ref()
        || historical_authority.definition().world != context.authority().definition().world
        || historical_authority.definition().profile != context.authority().definition().profile
        || historical_authority.definition().profile_version
            != context.authority().definition().profile_version
    {
        return Err(invalid("historical legal authority world/profile context"));
    }
    if let Some(activation) = durable_set
        && (recovery.is_some()
            || activation.successor().context_digest()? != context.authority().context_digest()?
            || historical_authority.context_digest()? != activation.successor().context_digest()?
            || activation.predecessor().definition().world != original.definition().world
            || activation.boundary().head().position.checked_add(1) != Some(position)
            || body.durable_set.as_ref().is_none_or(|binding| {
                binding.transition_digest != activation.transition_digest()
                    || binding.activation_digest.as_deref() != Some(activation.activation_digest())
            }))
    {
        return Err(invalid("historical exact healthy successor activation"));
    }
    let node = descend(&body, position, record_id, &mut load)?;
    let target = &node.head;
    let entry: FormalRequestStateV1 = object(&node.entry, &mut load)?;
    let FormalRequestStateV1::Qualified { candidate, proof } = entry else {
        return Err(FormalError::HistoryUnavailable(
            "HistoryUnavailable: historical entry not Qualified".into(),
        ));
    };
    if candidate.candidate.record != proof.decision.record
        || proof.decision.record.record_id() != Ok(target.record_id.clone())
        || proof.decision.record.position != target.position
        || node.request != proof.decision.record.request_digest
        || candidate.candidate.writer_signature_hex != proof.decision.writer_signature_hex
        || candidate.branch != proof.decision.branch
        || candidate.frozen != proof.decision.frozen
    {
        return Err(invalid("historical exact candidate/proof/canonical head"));
    }
    verify_record_material(&proof.decision.record, &candidate.material)?;
    // Profile 2 result envelopes already carry their immutable node writes.
    // Keep these verified bytes local for the following sparse-index replay;
    // any node outside the two closed records still uses the authenticated
    // transport loader below.
    let mut embedded_business_nodes = EmbeddedBusinessNodes::default();
    if historical_authority.definition().profile_version == 2 {
        embedded_business_nodes.include_material(&proof.decision.record, &candidate.material)?;
    }
    if Store::lookup_from_root(body.index.as_deref(), &node.request, &mut load)?.as_deref()
        != Some(node.entry.as_str())
    {
        return Err(invalid(
            "historical authenticated permanent request inclusion",
        ));
    }
    match (&candidate.frozen, &candidate.frozen_material) {
        (Some(cert), Some(material)) => verify_frozen_material(&cert.reservation.body, material)?,
        (None, None) if proof.decision.record.kind == FormalRecordKindV1::InitialActivation => {}
        _ => return Err(invalid("historical original frozen material closure")),
    }
    let mut business_parent = None;
    let parent = if let Some(link) = node.ancestors.first() {
        if link.head.position.checked_add(1) != Some(target.position) {
            return Err(invalid("historical immediate parent distance"));
        }
        let parent_node: HistoryNode = object(&link.node, &mut load)?;
        if parent_node.head != link.head {
            return Err(invalid("historical parent node identity"));
        }
        let FormalRequestStateV1::Qualified {
            candidate: parent_candidate,
            proof: parent,
        } = object(&parent_node.entry, &mut load)?
        else {
            return Err(invalid("historical parent not Qualified"));
        };
        let profile2 = historical_authority.definition().profile_version == 2;
        if (profile2
            && (parent_candidate.candidate.record != parent.decision.record
                || parent_candidate.candidate.writer_signature_hex
                    != parent.decision.writer_signature_hex
                || parent_candidate.branch != parent.decision.branch
                || parent_candidate.frozen != parent.decision.frozen))
            || parent.decision.record.record_id() != Ok(link.head.record_id.clone())
            || parent.decision.record.position != link.head.position
            || parent.decision.primary.body.finalizing_authority_id
                != parent.decision.replica.body.finalizing_authority_id
        {
            return Err(invalid("historical parent canonical/finalizer context"));
        }
        if historical_authority.definition().profile_version == 2 {
            verify_record_material(&parent.decision.record, &parent_candidate.material)?;
            embedded_business_nodes
                .include_material(&parent.decision.record, &parent_candidate.material)?;
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
    } else if target.position == 1 {
        None
    } else {
        return Err(FormalError::HistoryUnavailable(
            "HistoryUnavailable: historical immediate parent closure".into(),
        ));
    };
    if let Some(activation) = durable_set
        && (proof.decision.branch != FormalCompletionBranchV1::Normal
            || proof.decision.record.kind != FormalRecordKindV1::Execution
            || parent.as_ref().is_none_or(|parent| {
                parent.record != *activation.boundary().record()
                    || parent.finalizing_authority_id
                        != activation.boundary().finalizing_authority_id()
            }))
    {
        return Err(invalid(
            "historical healthy first Normal exact boundary parent",
        ));
    }
    let verdict = NativeCheckpointVerdictV1::verify(
        context.checkpoint_bytes(),
        context.authority(),
        context.endpoint_public_key(),
        context.checkpoint(),
        context.provider(),
        target,
        context.minimum(),
        &historical_authority.authority_id()?,
        &mut load,
    )?;
    if verdict.proof() != &proof
        || verdict.verified_parent_record() != parent.as_ref().map(|p| &p.record)
        || verdict.verified_parent_finalizing_authority_id()
            != parent.as_ref().map(|p| p.finalizing_authority_id.as_str())
    {
        return Err(invalid("historical checked checkpoint exact parent/proof"));
    }
    let origin = if proof.decision.branch == FormalCompletionBranchV1::Normal {
        historical_authority
    } else {
        original
    };
    let recovery = if proof.decision.branch == FormalCompletionBranchV1::Normal {
        None
    } else {
        Some(recovery.ok_or_else(|| {
            FormalError::HistoryUnavailable(
                "HistoryUnavailable: verified historical recovery context".into(),
            )
        })?)
    };
    let commit = restore_formal_commit_from_checkpoint(&proof, origin, recovery, &verdict)?;
    {
        let reader = HistoryBusinessReader {
            load: std::cell::RefCell::new(&mut load),
            embedded: &embedded_business_nodes,
            source_error: std::cell::RefCell::new(None),
        };
        if let Some(verified) = super::super::business_material::verify_formal_business_material(
            historical_authority,
            &proof.decision.record,
            &candidate.material,
            business_parent.as_ref(),
            candidate.frozen_material.as_ref(),
            &reader,
        )
        .map_err(|error| reader.preserve_source(error))?
        {
            use super::super::business_index::BusinessIndexNodeReader;
            for write in verified
                .node_writes()
                .iter()
                .chain(verified.retained_node_writes())
            {
                if reader.read_node_for_read(&write.digest)?.as_deref()
                    != Some(write.bytes.as_slice())
                {
                    return Err(invalid("historical business node missing/corrupt"));
                }
            }
        }
    }
    Ok(VerifiedNativeHistoricalRestoreV1 {
        commit,
        authority: historical_authority.clone(),
        candidate,
        proof,
        current_head: body.head,
        minimum: context.minimum().clone(),
        checkpoint: context.checkpoint().clone(),
        provenance: verdict,
    })
}
/// A bounded byte cache only. `BusinessIndexNodeReader` still validates each
/// fetched digest and canonical node depth before the bytes influence a root.
#[derive(Default)]
pub(super) struct EmbeddedBusinessNodes {
    nodes: BTreeMap<String, Vec<u8>>,
    bytes: usize,
}
impl EmbeddedBusinessNodes {
    pub(super) fn include_material(
        &mut self,
        record: &CanonicalRecordV1,
        material: &ClosedRecord,
    ) -> Result<(), FormalError> {
        let result_root = material
            .roots
            .get(&crate::controlled_authority::replicated_protocol::ArtifactRole::Result)
            .ok_or_else(|| invalid("historical business Result role missing"))?;
        let result = material
            .objects
            .iter()
            .find(|object| &object.content_hash == result_root)
            .ok_or_else(|| invalid("historical business Result bytes missing"))?;
        let envelope =
            super::super::business_material::FormalBusinessResultEnvelopeV1::decode(&result.bytes)?;
        if record.result_root != envelope.delta.new_root {
            return Err(invalid("historical embedded business root mismatch"));
        }
        for write in envelope.delta.node_writes.iter().chain(
            envelope
                .retained_history_delta
                .iter()
                .flat_map(|delta| delta.node_writes.iter()),
        ) {
            self.insert_write(&write.digest, &write.bytes)?;
        }
        Ok(())
    }

    pub(super) fn insert_write(&mut self, digest: &str, bytes: &[u8]) -> Result<(), FormalError> {
        hash_hex(digest)?;
        if blake3::hash(bytes).to_hex().as_str() != digest {
            return Err(invalid("historical embedded business node checksum"));
        }
        if let Some(existing) = self.nodes.get(digest) {
            if existing.as_slice() != bytes {
                return Err(invalid("historical embedded business node collision"));
            }
            return Ok(());
        }
        let Some(next_bytes) = self.bytes.checked_add(bytes.len()) else {
            return Ok(());
        };
        if self.nodes.len() >= MAX_EMBEDDED_BUSINESS_NODES
            || next_bytes > MAX_EMBEDDED_BUSINESS_BYTES
        {
            return Ok(());
        }
        self.nodes.insert(digest.to_owned(), bytes.to_vec());
        self.bytes = next_bytes;
        Ok(())
    }
}
impl super::super::business_index::BusinessIndexNodeReader for EmbeddedBusinessNodes {
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        hash_hex(digest).map_err(|error| error.to_string())?;
        let Some(bytes) = self.nodes.get(digest) else {
            return Ok(None);
        };
        if blake3::hash(bytes).to_hex().as_str() != digest {
            return Err("historical embedded business node checksum".into());
        }
        Ok(Some(bytes.clone()))
    }
}

struct HistoryBusinessReader<'a, F> {
    load: std::cell::RefCell<&'a mut F>,
    embedded: &'a EmbeddedBusinessNodes,
    source_error: std::cell::RefCell<Option<FormalError>>,
}
impl<F> HistoryBusinessReader<'_, F> {
    fn preserve_source(&self, error: FormalError) -> FormalError {
        self.source_error.borrow_mut().take().unwrap_or(error)
    }
}
impl<F: FnMut(&str) -> Result<Vec<u8>, FormalError>>
    super::super::business_index::BusinessIndexNodeReader for HistoryBusinessReader<'_, F>
{
    fn read_node(&self, id: &str) -> Result<Option<Vec<u8>>, String> {
        self.read_node_for_read(id).map_err(|error| {
            let diagnostic = error.to_string();
            self.source_error.borrow_mut().get_or_insert(error);
            diagnostic
        })
    }
    fn read_node_for_read(&self, id: &str) -> Result<Option<Vec<u8>>, FormalError> {
        if let Some(bytes) = self.embedded.read_node(id).map_err(invalid)? {
            return Ok(Some(bytes));
        }
        (self.load.borrow_mut())(id).map(Some)
    }
}

#[cfg(test)]
#[path = "endpoint_native_history_tests.rs"]
mod tests;

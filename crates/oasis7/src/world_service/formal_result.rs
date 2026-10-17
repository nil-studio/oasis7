//! Formal intent outcomes and scoped journal changes.
//!
//! The tree root is intended for `CanonicalRecordV1::result_root`. Result
//! leaves contain the immutable business result and first formal position,
//! never a canonical record ID or CommitRef. A distinct metadata leaf binds
//! the actual World aliases needed to reconstruct a CommitRef.
use super::{
    CanonicalIntentResultV1, WorldServicePayloadV1, agent_authority, authority, correlation,
    formal_read::{FormalReadVerifier, VerifiedFormalProjectionV1},
    projection::WorldServiceProjection,
};
use crate::runtime::World;
use oasis7_client_api::world_service::*;
use oasis7_distfs::controlled_authority::formal::{
    CanonicalRecordV1, VerifiedCommittedHistoricalCutV1, VerifiedFormalReadAnchorV1,
    VerifiedHistoricalCutInclusionV2,
    business_index::{
        BusinessIndexDeltaV1, BusinessIndexInclusionV1, BusinessIndexLeafKindV1,
        BusinessIndexLeafV1, BusinessIndexNodeReader, BusinessIndexNodeV1, BusinessIndexRootV1,
        metadata_key_digest,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_TREE_BYTES: usize = 64 * 1024 * 1024;
const MAX_PAGE_ITEMS: usize = 1024;
const MAX_PAGE_BYTES: usize = 4 * 1024 * 1024;

#[path = "formal_result_history.rs"]
mod historical;
pub use historical::FormalHistoricalBusinessPublicationV1;
#[path = "formal_result_scope.rs"]
mod scope;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum TreeLeafV1 {
    CommitMetadata(FormalBusinessCommitMetadataV1),
    Outcome(FormalBusinessOutcomeLeafV1),
}

/// Durable bounded tree material. The World ledger remains the source of
/// outcome bodies; `extend` checks it against this predecessor before keeping
/// an original formal position across a later record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalBusinessResultTreeV1 {
    schema_version: u32,
    metadata: FormalBusinessCommitMetadataV1,
    index_root: BusinessIndexRootV1,
}

pub struct FormalBusinessResultTreeUpdateV1 {
    pub tree: FormalBusinessResultTreeV1,
    pub delta: BusinessIndexDeltaV1,
}

impl FormalBusinessResultTreeV1 {
    /// Initial metadata has no record-ID dependency and is committed in the
    /// externally approved activation closure before the canonical ID exists.
    pub fn for_initial_world(
        world: &World,
        identity: &oasis7_distfs::controlled_authority::formal::FormalWorldIdentityV1,
        reader: &impl BusinessIndexNodeReader,
    ) -> Result<FormalBusinessResultTreeUpdateV1, String> {
        if !world
            .capability_revocation_state()
            .world_service_results
            .is_empty()
        {
            return Err(
                "formal initial business index requires explicit legacy result migration".into(),
            );
        }
        let root = blake3::hash(&serde_cbor::to_vec(&world.snapshot()).map_err(|e| e.to_string())?)
            .to_hex()
            .to_string();
        let governing = world
            .current_cognition_runtime_binding()
            .map_err(|e| format!("{e:?}"))?
            .runtime_manifest_hash
            .to_string();
        let governing = governing
            .strip_prefix("blake3:")
            .ok_or("formal initial governing digest")?;
        let metadata = metadata_for_boundary(world, identity, 1, &root, governing)?;
        let update = index_leaf(
            metadata_key_digest(),
            BusinessIndexLeafKindV1::CommitMetadata,
            &TreeLeafV1::CommitMetadata(metadata.clone()),
        )?;
        let delta =
            BusinessIndexDeltaV1::build_v2(BusinessIndexRootV1::empty()?, vec![update], reader)?;
        let tree = Self {
            schema_version: FORMAL_BUSINESS_RESULT_SCHEMA_V1,
            metadata,
            index_root: delta.new.clone(),
        };
        tree.validate()?;
        Ok(FormalBusinessResultTreeUpdateV1 { tree, delta })
    }
    /// Advance from the prior sparse root with the exact newly recorded results.
    /// Fresh formal activation rejects a legacy ledger without formal proofs.
    pub fn extend(
        world: &World,
        record: &CanonicalRecordV1,
        previous: Option<&Self>,
        new_results: &[CanonicalIntentResultV1],
        reader: &impl BusinessIndexNodeReader,
    ) -> Result<FormalBusinessResultTreeUpdateV1, String> {
        Self::extend_inner(world, record, previous, new_results, reader, Vec::new())
    }
    /// Profile 2 commits an immutable locator for the already-qualified parent.
    /// Its proof contains hashes of input artifacts, never recursive closures.
    pub fn extend_with_parent_proof(
        world: &World,
        record: &CanonicalRecordV1,
        previous: &Self,
        parent_proof: &oasis7_distfs::controlled_authority::formal::FormalQualifiedCommitProofV1,
        new_results: &[CanonicalIntentResultV1],
        reader: &impl BusinessIndexNodeReader,
    ) -> Result<FormalBusinessResultTreeUpdateV1, String> {
        let parent = &parent_proof.decision.record;
        if parent.world != record.world
            || parent.position.checked_add(1) != Some(record.position)
            || parent.record_id().map_err(|error| error.to_string())? != record.parent_record_id
            || parent.result_root != previous.root()?
        {
            return Err("formal historical cut differs from candidate predecessor".into());
        }
        let cut = oasis7_distfs::controlled_authority::formal::business_history::FormalHistoricalCutV1::from_proof(parent_proof)
            .and_then(|cut| cut.leaf()).map_err(|error| error.to_string())?;
        Self::extend_inner(
            world,
            record,
            Some(previous),
            new_results,
            reader,
            vec![cut],
        )
    }
    fn extend_inner(
        world: &World,
        record: &CanonicalRecordV1,
        previous: Option<&Self>,
        new_results: &[CanonicalIntentResultV1],
        reader: &impl BusinessIndexNodeReader,
        mut updates: Vec<BusinessIndexLeafV1>,
    ) -> Result<FormalBusinessResultTreeUpdateV1, String> {
        let metadata = metadata_for_world(world, record)?;
        if let Some(previous) = previous {
            previous.validate()?;
            if previous.metadata.world_id != metadata.world_id
                || previous.metadata.chain_id != metadata.chain_id
                || previous.metadata.genesis_digest != metadata.genesis_digest
                || previous.metadata.formal_position >= metadata.formal_position
            {
                return Err("formal result tree predecessor identity/position mismatch".into());
            }
        } else if !world
            .capability_revocation_state()
            .world_service_results
            .is_empty()
        {
            return Err(
                "fresh formal activation cannot upgrade unindexed legacy intent results".into(),
            );
        }

        let base = match previous {
            Some(tree) => tree.index_root.clone(),
            None => BusinessIndexRootV1::empty()?,
        };
        if let Some(previous) = previous {
            let old_metadata = base
                .get(&metadata_key_digest(), reader)?
                .ok_or("formal result predecessor metadata leaf missing")?;
            if old_metadata.kind != BusinessIndexLeafKindV1::CommitMetadata
                || decode_index_leaf(&old_metadata)?
                    != TreeLeafV1::CommitMetadata(previous.metadata.clone())
            {
                return Err("formal result predecessor metadata leaf mismatch".into());
            }
        }
        updates.reserve(new_results.len() + 1);
        updates.push(index_leaf(
            metadata_key_digest(),
            BusinessIndexLeafKindV1::CommitMetadata,
            &TreeLeafV1::CommitMetadata(metadata.clone()),
        )?);
        for result in new_results {
            result
                .request
                .validate()
                .map_err(|error| error.to_string())?;
            let key = correlation::key_digest(&result.request.correlation.key)?;
            let ledger_result: CanonicalIntentResultV1 = if let Some(stored_value) = world
                .capability_revocation_state()
                .world_service_results
                .get(&key)
            {
                correlation::validate_result(&key, stored_value)?;
                serde_json::from_value(stored_value.clone()).map_err(|error| error.to_string())?
            } else if world.state().retained_history_root.is_some() {
                world
                    .read_retained_world_service_result(&key)
                    .map_err(|error| error.to_string())?
                    .ok_or("formal result candidate is absent from authenticated World history")?
            } else {
                return Err("formal result candidate is absent from the World ledger".into());
            };
            if &ledger_result != result {
                return Err("formal result candidate differs from the World ledger".into());
            }
            let index_key = result_index_key(&result.request.correlation.key)?;
            let prior = base
                .get(&index_key, reader)?
                .map(|leaf| decode_index_leaf(&leaf))
                .transpose()?;
            let first_position = match prior {
                Some(TreeLeafV1::Outcome(old_leaf)) => {
                    let candidate = outcome_leaf(world, result, old_leaf.first_formal_position)?;
                    if immutable_outcome(&old_leaf)? != immutable_outcome(&candidate)? {
                        return Err("formal result ledger rewrote an immutable outcome".into());
                    }
                    old_leaf.first_formal_position
                }
                Some(TreeLeafV1::CommitMetadata(_)) => {
                    return Err("formal result key collided with metadata key".into());
                }
                None => metadata.formal_position,
            };
            updates.push(index_leaf(
                index_key,
                BusinessIndexLeafKindV1::Outcome,
                &TreeLeafV1::Outcome(outcome_leaf(world, result, first_position)?),
            )?);
        }
        updates.sort_by(|left, right| left.key_digest.cmp(&right.key_digest));
        let delta = BusinessIndexDeltaV1::build_v2(base, updates, reader)?;
        let tree = Self {
            schema_version: FORMAL_BUSINESS_RESULT_SCHEMA_V1,
            metadata,
            index_root: delta.new.clone(),
        };
        tree.validate()?;
        Ok(FormalBusinessResultTreeUpdateV1 { tree, delta })
    }

    pub fn root(&self) -> Result<String, String> {
        self.validate()?;
        self.index_root.canonical_result_root()
    }

    pub fn metadata(&self) -> &FormalBusinessCommitMetadataV1 {
        &self.metadata
    }
    pub fn index_root(&self) -> &BusinessIndexRootV1 {
        &self.index_root
    }
    pub fn historical_cut_inclusion(
        &self,
        world: &oasis7_distfs::controlled_authority::formal::FormalWorldIdentityV1,
        position: u64,
        reader: &impl BusinessIndexNodeReader,
    ) -> Result<BusinessIndexInclusionV1, String> {
        let key = oasis7_distfs::controlled_authority::formal::business_history::formal_historical_cut_key(world, position)
            .map_err(|error| error.to_string())?;
        self.index_root.prove(&key, reader)
    }
    pub fn validate_world_boundary(
        &self,
        world: &World,
        record: &CanonicalRecordV1,
    ) -> Result<(), String> {
        if self.root()? != record.result_root || self.metadata != metadata_for_world(world, record)?
        {
            return Err(
                "formal business descriptor differs from committed World/result root".into(),
            );
        }
        Ok(())
    }

    pub fn encode_cbor(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let bytes = serde_cbor::to_vec(self).map_err(|error| error.to_string())?;
        if bytes.len() > MAX_TREE_BYTES {
            return Err("formal result tree byte bound".into());
        }
        Ok(bytes)
    }

    pub fn decode_cbor(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_TREE_BYTES {
            return Err("formal result tree byte bound".into());
        }
        let tree: Self = serde_cbor::from_slice(bytes).map_err(|error| error.to_string())?;
        tree.validate()?;
        Ok(tree)
    }

    fn validate(&self) -> Result<(), String> {
        if self.schema_version != FORMAL_BUSINESS_RESULT_SCHEMA_V1 {
            return Err("formal result tree version/leaf bound".into());
        }
        self.metadata
            .validate()
            .map_err(|error| error.to_string())?;
        self.index_root
            .validate()
            .map_err(|error| error.to_string())?;
        if self.index_root.leaf_count == 0 {
            return Err("formal result tree leaf bound".into());
        }
        Ok(())
    }

    fn proof(
        &self,
        key: &str,
        reader: &impl BusinessIndexNodeReader,
    ) -> Result<FormalBusinessResultProofV1, String> {
        self.validate()?;
        let outcome_path = self.index_root.prove(key, reader)?;
        if outcome_path.leaf.kind != BusinessIndexLeafKindV1::Outcome {
            return Err("formal business result key has metadata kind".into());
        }
        let outcome = match decode_index_leaf(&outcome_path.leaf)? {
            TreeLeafV1::Outcome(outcome) => outcome,
            TreeLeafV1::CommitMetadata(_) => {
                return Err("formal business result key has metadata kind".into());
            }
        };
        let outcome_inclusion = result_inclusion(outcome_path)?;
        let metadata_path = self.index_root.prove(&metadata_key_digest(), reader)?;
        if metadata_path.leaf.kind != BusinessIndexLeafKindV1::CommitMetadata {
            return Err("formal result metadata key has outcome kind".into());
        }
        let metadata = decode_index_leaf(&metadata_path.leaf)?;
        let commit_metadata = match metadata {
            TreeLeafV1::CommitMetadata(metadata) => metadata,
            TreeLeafV1::Outcome(_) => {
                return Err("formal result metadata key has outcome kind".into());
            }
        };
        if commit_metadata != self.metadata {
            return Err("formal result metadata leaf differs from root descriptor".into());
        }
        let metadata_inclusion = result_inclusion(metadata_path)?;
        let proof = FormalBusinessResultProofV1 {
            schema_version: FORMAL_BUSINESS_RESULT_SCHEMA_V1,
            outcome,
            outcome_inclusion,
            commit_metadata,
            metadata_inclusion,
        };
        proof.validate().map_err(|error| error.to_string())?;
        Ok(proof)
    }

    /// Return `None` only when the qualified sparse root proves the requested
    /// key is absent. Storage, decoding, and leaf-kind failures remain errors.
    fn outcome_for_key_if_present(
        &self,
        key: &str,
        reader: &impl BusinessIndexNodeReader,
    ) -> Result<Option<FormalBusinessOutcomeLeafV1>, String> {
        let Some(leaf) = self.index_root.get(key, reader)? else {
            return Ok(None);
        };
        if leaf.kind != BusinessIndexLeafKindV1::Outcome {
            return Err("formal result key has metadata kind".into());
        }
        match decode_index_leaf(&leaf)? {
            TreeLeafV1::Outcome(outcome) => Ok(Some(outcome)),
            TreeLeafV1::CommitMetadata(_) => Err("formal result key has metadata kind".into()),
        }
    }
}

fn immutable_outcome(leaf: &FormalBusinessOutcomeLeafV1) -> Result<String, String> {
    tree_hash(
        "formal-business-result-immutable-v1",
        &(
            &leaf.correlation,
            &leaf.visibility_scope,
            &leaf.original_request_digest,
            leaf.action_id,
            leaf.committed_height,
            &leaf.outcome,
            &leaf.canonical_result_cbor_hex,
        ),
    )
}

fn verify_historical_outcome_equality(
    current: &FormalBusinessOutcomeLeafV1,
    original: &FormalBusinessOutcomeLeafV1,
) -> Result<(), String> {
    if current != original {
        return Err("historical result differs from current immutable outcome".into());
    }
    Ok(())
}

fn outcome_leaf(
    world: &World,
    result: &CanonicalIntentResultV1,
    first_formal_position: u64,
) -> Result<FormalBusinessOutcomeLeafV1, String> {
    result
        .request
        .validate()
        .map_err(|error| error.to_string())?;
    let original_request_digest =
        tree_hash("formal-business-original-request-v1", &result.request)?;
    let outcome = match &result.rejected {
        Some(reason) => {
            if !result.receipt.is_null() {
                return Err("rejected formal result has a success receipt".into());
            }
            FormalBusinessOutcomeV1::Rejected {
                reason: reason.clone(),
            }
        }
        None => FormalBusinessOutcomeV1::Committed {
            receipt: result.receipt.clone(),
        },
    };
    let visibility_scope = scope::for_request(world, &result.request)?;
    let leaf = FormalBusinessOutcomeLeafV1 {
        schema_version: FORMAL_BUSINESS_RESULT_SCHEMA_V1,
        correlation: result.request.correlation.clone(),
        visibility_scope,
        original_request_digest,
        action_id: result.action_id,
        committed_height: result.committed_height,
        first_formal_position,
        outcome,
        canonical_result_cbor_hex: Some(hex::encode(
            serde_cbor::to_vec(result).map_err(|e| e.to_string())?,
        )),
    };
    leaf.validate().map_err(|error| error.to_string())?;
    Ok(leaf)
}

fn metadata_for_world(
    world: &World,
    record: &CanonicalRecordV1,
) -> Result<FormalBusinessCommitMetadataV1, String> {
    metadata_for_boundary(
        world,
        &record.world,
        record.position,
        &record.after_state_root,
        &record.governing_manifest_digest,
    )
}

fn metadata_for_boundary(
    world: &World,
    identity: &oasis7_distfs::controlled_authority::formal::FormalWorldIdentityV1,
    position: u64,
    after_state_root: &str,
    governing_manifest_digest: &str,
) -> Result<FormalBusinessCommitMetadataV1, String> {
    let snapshot = world.snapshot();
    let snapshot_bytes = serde_cbor::to_vec(&snapshot).map_err(|error| error.to_string())?;
    let snapshot_root = blake3::hash(&snapshot_bytes).to_hex().to_string();
    let manifest = &snapshot.chain_resource_manifest;
    if snapshot_root != after_state_root
        || manifest.world_id != identity.world_id
        || manifest.chain_id != identity.chain_id
        || manifest.genesis_ref.as_deref() != Some(identity.genesis_digest.as_str())
    {
        return Err("formal result World differs from qualified canonical record".into());
    }
    let binding = world
        .current_cognition_runtime_binding()
        .map_err(|error| format!("{error:?}"))?;
    let governing_hash = binding.runtime_manifest_hash.to_string();
    let governing = governing_hash
        .strip_prefix("blake3:")
        .ok_or("formal result governing digest encoding")?;
    if governing != governing_manifest_digest {
        return Err("formal result governing manifest differs from canonical record".into());
    }
    let (header, _) = super::formal_projection_header::committed_header(world)?;
    let metadata = FormalBusinessCommitMetadataV1 {
        schema_version: FORMAL_BUSINESS_RESULT_SCHEMA_V1,
        formal_position: position,
        world_id: identity.world_id.clone(),
        chain_id: identity.chain_id.clone(),
        genesis_digest: identity.genesis_digest.clone(),
        after_state_root: after_state_root.to_owned(),
        provider_world_id: binding.world_id.clone(),
        branch_id: binding.branch_id.clone(),
        finality_epoch: binding.finality_epoch,
        finality_status: binding.finality_status.clone(),
        finality_block_hash: binding
            .finality_block_hash
            .as_ref()
            .map(ToString::to_string)
            .map(|value| value.strip_prefix("blake3:").unwrap_or(&value).to_owned()),
        reorg_generation: binding.reorg_epoch,
        governing_manifest_digest: governing_manifest_digest.to_owned(),
        authority_generation: header.authority_generation,
        permission_generation: header.permission_generation,
        event_id_era: header.event_id_era,
        journal_retention_floor: header.journal_retention_floor,
        journal_high_watermark: header.journal_high_watermark,
    };
    metadata.validate().map_err(|error| error.to_string())?;
    Ok(metadata)
}

fn tree_hash<T: Serialize>(domain: &str, value: &T) -> Result<String, String> {
    let encoded = oasis7_wasm_abi::encode_canonical_cbor(&(
        "oasis7.formal.business-result.v1",
        domain,
        value,
    ))
    .map_err(|error| error.to_string())?;
    Ok(blake3::hash(&encoded).to_hex().to_string())
}

fn index_leaf(
    key_digest: String,
    kind: BusinessIndexLeafKindV1,
    leaf: &TreeLeafV1,
) -> Result<BusinessIndexLeafV1, String> {
    let canonical_value =
        oasis7_wasm_abi::encode_canonical_cbor(leaf).map_err(|error| error.to_string())?;
    Ok(BusinessIndexLeafV1 {
        key_digest,
        kind,
        canonical_value,
    })
}

fn decode_tree_leaf(bytes: &[u8]) -> Result<TreeLeafV1, String> {
    serde_cbor::from_slice(bytes).map_err(|error| error.to_string())
}

fn decode_index_leaf(leaf: &BusinessIndexLeafV1) -> Result<TreeLeafV1, String> {
    match (&leaf.kind, decode_tree_leaf(&leaf.canonical_value)?) {
        (BusinessIndexLeafKindV1::CommitMetadata, leaf @ TreeLeafV1::CommitMetadata(_))
        | (BusinessIndexLeafKindV1::Outcome, leaf @ TreeLeafV1::Outcome(_)) => Ok(leaf),
        _ => Err("business index leaf kind/value mismatch".into()),
    }
}

fn result_inclusion(
    inclusion: BusinessIndexInclusionV1,
) -> Result<FormalBusinessResultInclusionV1, String> {
    Ok(FormalBusinessResultInclusionV1 {
        key_digest: inclusion.leaf.key_digest,
        leaf_count: inclusion.leaf_count,
        siblings: inclusion.siblings,
    })
}

fn verify_inclusion(
    leaf: &TreeLeafV1,
    proof: &FormalBusinessResultInclusionV1,
    expected_key: &str,
    kind: BusinessIndexLeafKindV1,
    root: &str,
) -> Result<(), String> {
    proof.validate().map_err(|error| error.to_string())?;
    if proof.key_digest != expected_key {
        return Err("formal result inclusion key mismatch".into());
    }
    let key_bytes: [u8; 32] = hex::decode(expected_key)
        .map_err(|error| error.to_string())?
        .try_into()
        .map_err(|_| "formal result inclusion key length")?;
    let leaf = index_leaf(expected_key.to_owned(), kind, leaf)?;
    let mut current = BusinessIndexNodeV1::leaf(leaf)?.digest()?;
    for (offset, sibling) in proof.siblings.iter().enumerate() {
        let depth = 255usize.saturating_sub(offset);
        let (left, right) = if key_bytes[depth / 8] & (0x80 >> (depth % 8)) != 0 {
            (sibling.clone(), current)
        } else {
            (current, sibling.clone())
        };
        current = BusinessIndexNodeV1::branch(depth, left, right)?.digest()?;
    }
    let computed = BusinessIndexRootV1 {
        node_root: current,
        leaf_count: proof.leaf_count,
    }
    .canonical_result_root()?;
    if computed != root {
        return Err("formal business result Merkle root mismatch".into());
    }
    Ok(())
}

/// Opaque outcome. Its only constructor verifies a Qualified canonical record,
/// the result tree, exact original request, and audience-bound projection.
#[derive(Debug, Clone)]
pub struct VerifiedBusinessOutcome {
    commit: CommitRef,
    proof: FormalBusinessResultProofV1,
}
impl VerifiedBusinessOutcome {
    pub fn commit(&self) -> &CommitRef {
        &self.commit
    }
    pub fn result(&self) -> &FormalBusinessOutcomeLeafV1 {
        &self.proof.outcome
    }
    /// Construct the legacy committed reply only from the opaque proof result.
    pub fn reply_committed(&self) -> Result<IntentOutcome<Value>, String> {
        let FormalBusinessOutcomeV1::Committed { receipt } = &self.proof.outcome.outcome else {
            return Err("verified formal result is rejected, not committed".into());
        };
        Ok(IntentOutcome::Committed {
            commit: Box::new(self.commit.clone()),
            receipt: receipt.clone(),
        })
    }
    pub fn proof(&self) -> &FormalBusinessResultProofV1 {
        &self.proof
    }
}

#[derive(Debug, Clone)]
#[expect(
    clippy::large_enum_variant,
    reason = "Public typed verification results retain opaque evidence values without changing the client API"
)]
pub enum FormalBusinessPageVerification {
    Progress(VerifiedFormalProjectionV1),
    Complete(VerifiedBusinessOutcome),
}

/// Verifies a bounded page against an already externally anchored formal
/// verifier. The verifier cache advances only after every page field checks.
pub fn verify_formal_business_outcome_page(
    verifier: &mut FormalReadVerifier,
    page: &FormalBusinessOutcomePageV1<WorldServiceProjection>,
    original_request: &SubmitIntentRequest<WorldServicePayloadV1>,
    reader_public_key: &str,
) -> Result<FormalBusinessPageVerification, String> {
    page.validate().map_err(|error| error.to_string())?;
    original_request
        .validate()
        .map_err(|error| error.to_string())?;
    if original_request.correlation != page.requested_correlation {
        return Err("formal result request differs from the caller's original intent".into());
    }
    if verifier.anchor_ref()? != page.from_anchor {
        return Err("formal result page does not continue this verified cache".into());
    }
    let mut next = verifier.clone();
    let verified = next.verify_view(&page.target_view, &page.target_request, reader_public_key)?;
    if let Some(proof) = &page.result {
        if let Some(encoded) = &page.historical_cbor_hex {
            let outcome = historical::verify_historical_result(
                &next,
                &verified,
                page,
                proof,
                encoded,
                original_request,
                reader_public_key,
            )?;
            *verifier = next;
            return Ok(FormalBusinessPageVerification::Complete(outcome));
        }
        let record = verified.commit().record();
        if verified
            .read_anchor()
            .authority()
            .definition()
            .profile_version
            == 2
            || proof.outcome.canonical_result_cbor_hex.is_some()
        {
            let retained = decode_canonical_result_leaf(&proof.outcome)?;
            if &retained.request != original_request {
                return Err("formal result retained original envelope differs from caller".into());
            }
        }
        verify_result_proof(
            proof,
            &page.requested_correlation,
            original_request,
            &page.target_request.scope_id,
            reader_public_key,
            &page.target_view.view,
            record,
            true,
        )?;
        let commit = committed_reference(record, &proof.commit_metadata, &page.target_view.view)?;
        if commit != page.target_view.version.commit {
            return Err("formal business result aliases differ from authenticated target".into());
        }
        let outcome = VerifiedBusinessOutcome {
            commit,
            proof: proof.clone(),
        };
        *verifier = next;
        Ok(FormalBusinessPageVerification::Complete(outcome))
    } else {
        let continuation = page
            .continuation_anchor
            .as_ref()
            .ok_or("formal result continuation missing")?;
        let actual = FormalReadAnchorRefV1 {
            position: verified.commit().record().position,
            record_id: verified.commit().head().record_id,
            active_authority_id: verified
                .read_anchor()
                .authority()
                .authority_id()
                .map_err(|error| error.to_string())?,
        };
        if continuation != &actual {
            return Err("formal result continuation is not the verified page head".into());
        }
        *verifier = next;
        Ok(FormalBusinessPageVerification::Progress(verified))
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Each independent request, reader, scope and canonical publication binding must be checked"
)]
fn verify_result_proof(
    proof: &FormalBusinessResultProofV1,
    expected: &RequestCorrelation,
    original_request: &SubmitIntentRequest<WorldServicePayloadV1>,
    scope_id: &str,
    reader_public_key: &str,
    projection: &WorldServiceProjection,
    record: &CanonicalRecordV1,
    require_outcome_at_record: bool,
) -> Result<(), String> {
    proof.validate().map_err(|error| error.to_string())?;
    let result_position_invalid = if require_outcome_at_record {
        proof.outcome.first_formal_position != record.position
    } else {
        proof.outcome.first_formal_position > record.position
    };
    if &proof.outcome.correlation != expected
        || proof.outcome.original_request_digest
            != tree_hash("formal-business-original-request-v1", original_request)?
        || result_position_invalid
        || proof.commit_metadata.formal_position != record.position
    {
        return Err("formal result is not the original exact request outcome".into());
    }
    validate_result_audience(&proof.outcome, scope_id, reader_public_key)?;
    let metadata_leaf = TreeLeafV1::CommitMetadata(proof.commit_metadata.clone());
    let outcome_leaf = TreeLeafV1::Outcome(proof.outcome.clone());
    let metadata_key = metadata_key_digest();
    let outcome_key = result_index_key(&proof.outcome.correlation.key)?;
    verify_inclusion(
        &metadata_leaf,
        &proof.metadata_inclusion,
        &metadata_key,
        BusinessIndexLeafKindV1::CommitMetadata,
        &record.result_root,
    )?;
    verify_inclusion(
        &outcome_leaf,
        &proof.outcome_inclusion,
        &outcome_key,
        BusinessIndexLeafKindV1::Outcome,
        &record.result_root,
    )?;
    validate_metadata_projection(&proof.commit_metadata, projection, record)?;
    Ok(())
}

fn validate_result_audience(
    outcome: &FormalBusinessOutcomeLeafV1,
    scope_id: &str,
    reader_public_key: &str,
) -> Result<(), String> {
    if scope_id != outcome.visibility_scope {
        return Err("formal business result belongs to another committed visibility scope".into());
    }
    if scope_id == "public" && outcome.correlation.key.verified_subject != reader_public_key {
        return Err("formal business result reader is outside its committed audience".into());
    }
    Ok(())
}

fn validate_metadata_projection(
    metadata: &FormalBusinessCommitMetadataV1,
    projection: &WorldServiceProjection,
    record: &CanonicalRecordV1,
) -> Result<(), String> {
    metadata.validate().map_err(|error| error.to_string())?;
    let runtime = projection
        .runtime_binding
        .as_ref()
        .ok_or("formal result projection has no RuntimeBinding")?;
    let header = projection
        .formal_commit_header
        .as_ref()
        .ok_or("formal result projection has no committed metadata header")?;
    let finality_hash = runtime
        .finality_block_hash
        .as_ref()
        .map(ToString::to_string)
        .map(|value| value.strip_prefix("blake3:").unwrap_or(&value).to_owned());
    if metadata.world_id != record.world.world_id
        || metadata.chain_id != record.world.chain_id
        || metadata.genesis_digest != record.world.genesis_digest
        || metadata.after_state_root != record.after_state_root
        || metadata.governing_manifest_digest != record.governing_manifest_digest
        || metadata.provider_world_id != runtime.world_id
        || metadata.branch_id != runtime.branch_id
        || metadata.finality_epoch != runtime.finality_epoch
        || metadata.finality_status != runtime.finality_status
        || metadata.finality_block_hash != finality_hash
        || metadata.reorg_generation != runtime.reorg_epoch
        || metadata.governing_manifest_digest
            != runtime
                .runtime_manifest_hash
                .to_string()
                .strip_prefix("blake3:")
                .ok_or("formal result runtime governing digest")?
        || metadata.authority_generation != header.authority_generation
        || metadata.permission_generation != header.permission_generation
        || metadata.event_id_era != header.event_id_era
        || metadata.journal_retention_floor != header.journal_retention_floor
        || metadata.journal_high_watermark != header.journal_high_watermark
    {
        return Err("formal result metadata differs from qualified projection aliases".into());
    }
    Ok(())
}

fn metadata_from_projection(
    projection: &WorldServiceProjection,
    record: &CanonicalRecordV1,
) -> Result<FormalBusinessCommitMetadataV1, String> {
    let runtime = projection
        .runtime_binding
        .as_ref()
        .ok_or("formal projection has no RuntimeBinding")?;
    let header = projection
        .formal_commit_header
        .as_ref()
        .ok_or("formal projection has no committed metadata header")?;
    let finality_hash = runtime
        .finality_block_hash
        .as_ref()
        .map(ToString::to_string)
        .map(|value| value.strip_prefix("blake3:").unwrap_or(&value).to_owned());
    let metadata = FormalBusinessCommitMetadataV1 {
        schema_version: FORMAL_BUSINESS_RESULT_SCHEMA_V1,
        formal_position: record.position,
        world_id: record.world.world_id.clone(),
        chain_id: record.world.chain_id.clone(),
        genesis_digest: record.world.genesis_digest.clone(),
        after_state_root: record.after_state_root.clone(),
        provider_world_id: runtime.world_id.clone(),
        branch_id: runtime.branch_id.clone(),
        finality_epoch: runtime.finality_epoch,
        finality_status: runtime.finality_status.clone(),
        finality_block_hash: finality_hash,
        reorg_generation: runtime.reorg_epoch,
        governing_manifest_digest: record.governing_manifest_digest.clone(),
        authority_generation: header.authority_generation,
        permission_generation: header.permission_generation,
        event_id_era: header.event_id_era,
        journal_retention_floor: header.journal_retention_floor,
        journal_high_watermark: header.journal_high_watermark,
    };
    metadata.validate().map_err(|error| error.to_string())?;
    validate_metadata_projection(&metadata, projection, record)?;
    Ok(metadata)
}

fn committed_reference(
    record: &CanonicalRecordV1,
    metadata: &FormalBusinessCommitMetadataV1,
    projection: &WorldServiceProjection,
) -> Result<CommitRef, String> {
    let runtime = projection
        .runtime_binding
        .as_ref()
        .ok_or("formal result projection has no RuntimeBinding")?;
    let finality_ref = authority::request_digest(
        "finality",
        &(
            runtime.finality_epoch,
            &runtime.finality_status,
            &runtime.finality_block_hash,
        ),
    )?;
    let commit = CommitRef {
        world: WorldIdentity {
            world_id: metadata.world_id.clone(),
            genesis_digest: metadata.genesis_digest.clone(),
        },
        position: record.position,
        execution_block_hash: record.record_id().map_err(|error| error.to_string())?,
        state_root_ref: record.after_state_root.clone(),
        binding: ExecutionBinding {
            provider_world_id: metadata.provider_world_id.clone(),
            branch_id: metadata.branch_id.clone(),
            finality_ref,
            reorg_generation: metadata.reorg_generation,
            governing_manifest_ref: format!("blake3:{}", record.governing_manifest_digest),
            authority_generation: metadata.authority_generation,
            permission_generation: metadata.permission_generation,
        },
    };
    commit.validate().map_err(|error| error.to_string())?;
    Ok(commit)
}

/// A publication is constructed only for an externally verified qualified
/// record and its exact restored World snapshot.
pub struct FormalBusinessResultPublication {
    anchor: VerifiedFormalReadAnchorV1,
    world: World,
    tree: FormalBusinessResultTreeV1,
}
impl FormalBusinessResultPublication {
    pub fn new(
        anchor: VerifiedFormalReadAnchorV1,
        world: World,
        tree: FormalBusinessResultTreeV1,
    ) -> Result<Self, String> {
        let record = anchor.head().record();
        let metadata = metadata_for_world(&world, record)?;
        if tree.metadata != metadata || tree.root()? != record.result_root {
            return Err("formal result tree differs from qualified canonical root".into());
        }
        tree.validate()?;
        Ok(Self {
            anchor,
            world,
            tree,
        })
    }
    pub fn anchor(&self) -> &VerifiedFormalReadAnchorV1 {
        &self.anchor
    }
    pub fn tree(&self) -> &FormalBusinessResultTreeV1 {
        &self.tree
    }
    /// Exact result key plus current authenticated Agent scope. This returns
    /// one leaf only; it never exports another request or a full World snapshot.
    pub fn proof_for(
        &self,
        correlation: &RequestCorrelation,
        scope_id: &str,
        reader_public_key: &str,
        index_reader: &impl BusinessIndexNodeReader,
    ) -> Result<FormalBusinessResultProofV1, String> {
        self.proof_for_if_present(correlation, scope_id, reader_public_key, index_reader)?
            .ok_or_else(|| "formal business result unavailable".to_owned())
    }

    /// Exact result lookup with absence separated from proof/storage failures.
    /// `None` means this qualified root has no outcome leaf for the key.
    pub fn proof_for_if_present(
        &self,
        correlation: &RequestCorrelation,
        scope_id: &str,
        reader_public_key: &str,
        index_reader: &impl BusinessIndexNodeReader,
    ) -> Result<Option<FormalBusinessResultProofV1>, String> {
        let key = result_index_key(&correlation.key)?;
        let Some(leaf) = self.tree.outcome_for_key_if_present(&key, index_reader)? else {
            return Ok(None);
        };
        if &leaf.correlation != correlation {
            return Err("request key is bound to another original signed payload".into());
        }
        authorize_world_result_reader(&self.world, &leaf, scope_id, reader_public_key)?;
        self.tree.proof(&key, index_reader).map(Some)
    }

    /// Prove an old result only after this current publication authorizes the
    /// reader and the old cut is cryptographically delegated by this anchor.
    /// The historical publication is checked against that exact cut; its old
    /// World is deliberately not consulted for current reader authorization.
    pub fn proofs_for_committed_history(
        &self,
        original: &Self,
        historical: &VerifiedCommittedHistoricalCutV1,
        correlation: &RequestCorrelation,
        scope_id: &str,
        reader_public_key: &str,
        index_reader: &impl BusinessIndexNodeReader,
    ) -> Result<(FormalBusinessResultProofV1, FormalBusinessResultProofV1), String> {
        let current_proof =
            self.proof_for(correlation, scope_id, reader_public_key, index_reader)?;
        let current_record = self.anchor.head().record();
        let original_record = original.anchor.head().record();
        let historical_delegating_authority = historical
            .delegating_authority()
            .context_digest()
            .map_err(|error| error.to_string())?;
        let current_authority = self
            .anchor
            .authority()
            .context_digest()
            .map_err(|error| error.to_string())?;
        let historical_authority = historical
            .authority()
            .context_digest()
            .map_err(|error| error.to_string())?;
        let original_authority = original
            .anchor
            .authority()
            .context_digest()
            .map_err(|error| error.to_string())?;
        if historical.delegating_head() != &self.anchor.head().head()
            || historical_delegating_authority != current_authority
            || historical.commit().record() != original_record
            || original_record.world != current_record.world
            || original_record.position >= current_record.position
            || original_authority != historical_authority
            || current_proof.outcome.first_formal_position != original_record.position
        {
            return Err("historical result publication differs from its current-root cut".into());
        }

        let key = result_index_key(&correlation.key)?;
        let original_proof = original.tree.proof(&key, index_reader)?;
        verify_historical_outcome_equality(&current_proof.outcome, &original_proof.outcome)?;
        Ok((current_proof, original_proof))
    }
}

fn authorize_world_result_reader(
    world: &World,
    outcome: &FormalBusinessOutcomeLeafV1,
    scope_id: &str,
    reader_public_key: &str,
) -> Result<(), String> {
    if let Some(agent_id) = outcome.visibility_scope.strip_prefix("agent:") {
        if scope_id != outcome.visibility_scope {
            return Err("formal result private Agent scope mismatch".into());
        }
        let owner = agent_authority::owner_public_key(world, agent_id)?;
        let generation = if reader_public_key == owner {
            0
        } else {
            world
                .capability_revocation_state()
                .agent_signer_delegations
                .get(agent_id)
                .filter(|delegation| {
                    delegation.delegate_public_key == reader_public_key && !delegation.revoked
                })
                .map(|delegation| delegation.generation)
                .ok_or("formal result reader lacks current Agent delegation")?
        };
        agent_authority::validate_agent_signer(world, agent_id, reader_public_key, generation)
    } else if scope_id == "public" && reader_public_key == outcome.correlation.key.verified_subject
    {
        Ok(())
    } else {
        Err("formal result reader is outside its authorized audience".into())
    }
}

fn outcome_agent_id(world: &World, payload: &WorldServicePayloadV1) -> Result<String, String> {
    match payload {
        WorldServicePayloadV1::AgentChat(request) => Ok(request.agent_id.clone()),
        WorldServicePayloadV1::Cognition(request) => Ok(request.request.request.agent_id.clone()),
        WorldServicePayloadV1::FeedbackAck(request) => Ok(request.request.agent_id.clone()),
        WorldServicePayloadV1::Delegation(request) => Ok(request.request.agent_id.clone()),
        WorldServicePayloadV1::Scheduler(request) => Ok(request.request.agent_id.clone()),
        WorldServicePayloadV1::GameTaskCreate(request) => Ok(request.request.agent_id.clone()),
        WorldServicePayloadV1::GameGoalPublish(request) => Ok(request.request.agent_id.clone()),
        WorldServicePayloadV1::GameExecutorControl(request) => {
            Ok(request.request.fence.agent_id.clone())
        }
        WorldServicePayloadV1::GameplayJson(bytes) => {
            if let Ok(request) =
                serde_json::from_slice::<crate::viewer::GameplayActionRequest>(bytes)
            {
                if request.target_agent_id.trim().is_empty()
                    || request.target_agent_id != request.target_agent_id.trim()
                {
                    return Err("formal result gameplay target Agent identity is invalid".into());
                }
                return Ok(request.target_agent_id);
            }
            let command: crate::viewer::CollectDataCommand =
                serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
            let crate::viewer::CollectDataCommand::Submit { request } = &command else {
                return Err("formal result cannot publish a non-submit CollectData outcome".into());
            };
            let proof = request
                .auth
                .as_ref()
                .ok_or("formal result CollectData request has no verified owner proof")?;
            let verified = crate::viewer::verify_collect_data_auth_proof(&command, proof)?;
            let mut claims = world.state().starter_oc_claims.values().filter(|claim| {
                claim.player_id == verified.player_id
                    && claim.public_key.as_deref() == Some(verified.public_key.as_str())
            });
            let claim = claims
                .next()
                .ok_or("formal result CollectData owner Agent claim is unavailable")?;
            if claims.next().is_some() {
                return Err("formal result CollectData owner Agent claim is ambiguous".into());
            }
            Ok(claim.agent_id.clone())
        }
    }
}

#[path = "formal_changes.rs"]
mod changes;
pub use changes::*;
pub use oasis7_client_api::world_service::FormalBusinessChangesCursorV1;

#[cfg(test)]
#[path = "formal_result_tests.rs"]
mod tests;

fn result_index_key(key: &RequestKey) -> Result<String, String> {
    correlation::key_digest(key)?
        .strip_prefix("blake3:")
        .map(str::to_owned)
        .ok_or_else(|| "formal result key digest encoding".into())
}

/// Cold replay material is accepted only when all business identity and result
/// fields agree with its authenticated leaf; callers must also prove inclusion.
pub fn decode_canonical_result_leaf(
    leaf: &FormalBusinessOutcomeLeafV1,
) -> Result<CanonicalIntentResultV1, String> {
    leaf.validate().map_err(|e| e.to_string())?;
    let bytes = hex::decode(
        leaf.canonical_result_cbor_hex
            .as_ref()
            .ok_or("HistoryUnavailable: original result bytes were not retained")?,
    )
    .map_err(|_| "formal retained result encoding")?;
    let result: CanonicalIntentResultV1 =
        serde_cbor::from_slice(&bytes).map_err(|e| e.to_string())?;
    let key = correlation::key_digest(&result.request.correlation.key)?;
    correlation::validate_result(
        &key,
        &serde_json::to_value(&result).map_err(|e| e.to_string())?,
    )?;
    let outcome = match &result.rejected {
        Some(reason) if result.receipt.is_null() => FormalBusinessOutcomeV1::Rejected {
            reason: reason.clone(),
        },
        Some(_) => return Err("formal retained rejection has a success receipt".into()),
        None => FormalBusinessOutcomeV1::Committed {
            receipt: result.receipt.clone(),
        },
    };
    if result.request.correlation != leaf.correlation
        || result.action_id != leaf.action_id
        || result.committed_height != leaf.committed_height
        || outcome != leaf.outcome
        || tree_hash("formal-business-original-request-v1", &result.request)?
            != leaf.original_request_digest
    {
        return Err("formal retained result differs from authenticated leaf".into());
    }
    Ok(result)
}

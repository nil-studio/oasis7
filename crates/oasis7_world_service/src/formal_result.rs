//! Formal intent outcomes and scoped journal changes.
//!
//! The tree root is intended for `CanonicalRecordV1::result_root`. Result
//! leaves contain the immutable business result and first formal position,
//! never a canonical record ID or CommitRef. A distinct metadata leaf binds
//! the actual World aliases needed to reconstruct a CommitRef.
use super::{
    CanonicalIntentResultV1, WorldServicePayloadV1, agent_authority, authority, correlation,
    formal_read::FormalReadVerifier, projection::WorldServiceProjection,
};
use oasis7_client_api::world_service::*;
use oasis7_formal_protocol::controlled_authority::formal::{
    CanonicalRecordV1, VerifiedCommittedHistoricalCutV1, VerifiedFormalReadAnchorV1,
    business_index::{
        BusinessIndexDeltaV1, BusinessIndexInclusionV1, BusinessIndexLeafKindV1,
        BusinessIndexLeafV1, BusinessIndexNodeReader, BusinessIndexRootV1, metadata_key_digest,
    },
};
use oasis7_runtime::World;
use serde::{Deserialize, Serialize};

const MAX_TREE_BYTES: usize = 64 * 1024 * 1024;
const MAX_PAGE_ITEMS: usize = 1024;
const MAX_PAGE_BYTES: usize = 4 * 1024 * 1024;

pub type FormalHistoricalBusinessPublicationV1 =
    oasis7_game_client::formal_business::FormalHistoricalBusinessPublicationV1<
        WorldServiceProjection,
    >;
#[path = "formal_result_scope.rs"]
mod scope;

#[path = "formal_result_kernel.rs"]
mod kernel;

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

/// Exact inputs for a BFT successor result-tree update. The parent root and
/// concrete World remain explicit so callers cannot update a tree from only
/// a claimed root tuple.
pub struct FormalBftResultSuccessorInputV1<'a> {
    pub world: &'a World,
    pub identity: &'a oasis7_formal_protocol::controlled_authority::formal::FormalWorldIdentityV1,
    pub height: u64,
    pub parent_result_root: &'a str,
    pub execution_state_root: &'a str,
    pub previous: &'a FormalBusinessResultTreeV1,
    pub new_results: &'a [CanonicalIntentResultV1],
    pub reader: &'a dyn BusinessIndexNodeReader,
}

struct BusinessIndexReaderRef<'a>(&'a dyn BusinessIndexNodeReader);

impl BusinessIndexNodeReader for BusinessIndexReaderRef<'_> {
    fn prefetch_path(&self, root: &BusinessIndexRootV1, key: &str) -> Result<(), String> {
        self.0.prefetch_path(root, key)
    }

    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        self.0.read_node(digest)
    }

    fn read_cached_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        self.0.read_cached_node(digest)
    }

    fn cache_content_node(&self, digest: &str, bytes: &[u8]) -> Result<(), String> {
        self.0.cache_content_node(digest, bytes)
    }
}

impl FormalBusinessResultTreeV1 {
    /// Initial metadata has no record-ID dependency and is committed in the
    /// externally approved activation closure before the canonical ID exists.
    pub fn for_initial_world(
        world: &World,
        identity: &oasis7_formal_protocol::controlled_authority::formal::FormalWorldIdentityV1,
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
        let root = blake3::hash(
            &oasis7_world_state::codec::snapshot_cbor(&world.snapshot())
                .map_err(|e| e.to_string())?,
        )
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
        let metadata = metadata_for_world(world, record)?;
        Self::extend_inner(world, metadata, previous, new_results, reader, Vec::new())
    }
    /// Profile 2 commits an immutable locator for the already-qualified parent.
    /// Its proof contains hashes of input artifacts, never recursive closures.
    pub fn extend_with_parent_proof(
        world: &World,
        record: &CanonicalRecordV1,
        previous: &Self,
        parent_proof: &oasis7_formal_protocol::controlled_authority::formal::FormalQualifiedCommitProofV1,
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
        let cut = oasis7_formal_protocol::controlled_authority::formal::business_history::FormalHistoricalCutV1::from_proof(parent_proof)
            .and_then(|cut| cut.leaf()).map_err(|error| error.to_string())?;
        Self::extend_inner(
            world,
            metadata_for_world(world, record)?,
            Some(previous),
            new_results,
            reader,
            vec![cut],
        )
    }

    /// Advance the existing sparse result tree at a verified BFT successor
    /// boundary. This adapter authenticates the concrete World and exact
    /// predecessor tree without inventing a controlled `CanonicalRecordV1`.
    pub fn extend_bft_successor(
        input: FormalBftResultSuccessorInputV1<'_>,
    ) -> Result<FormalBusinessResultTreeUpdateV1, String> {
        let FormalBftResultSuccessorInputV1 {
            world,
            identity,
            height,
            parent_result_root,
            execution_state_root,
            previous,
            new_results,
            reader,
        } = input;
        previous.validate()?;
        let expected_height = previous
            .metadata
            .formal_position
            .checked_add(1)
            .ok_or("formal BFT result position overflow")?;
        if height != expected_height
            || previous.root()? != parent_result_root
            || previous.metadata.world_id != identity.world_id
            || previous.metadata.chain_id != identity.chain_id
            || previous.metadata.genesis_digest != identity.genesis_digest
        {
            return Err("formal BFT result predecessor differs from exact parent".into());
        }
        let metadata = metadata_for_boundary(
            world,
            identity,
            height,
            execution_state_root,
            &previous.metadata.governing_manifest_digest,
        )?;
        let reader = BusinessIndexReaderRef(reader);
        Self::extend_inner(
            world,
            metadata,
            Some(previous),
            new_results,
            &reader,
            Vec::new(),
        )
    }

    fn extend_inner(
        world: &World,
        metadata: FormalBusinessCommitMetadataV1,
        previous: Option<&Self>,
        new_results: &[CanonicalIntentResultV1],
        reader: &impl BusinessIndexNodeReader,
        mut updates: Vec<BusinessIndexLeafV1>,
    ) -> Result<FormalBusinessResultTreeUpdateV1, String> {
        kernel::extend(
            &mut kernel::LegacyFacts(world),
            metadata,
            previous,
            new_results,
            reader,
            updates,
        )
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
        world: &oasis7_formal_protocol::controlled_authority::formal::FormalWorldIdentityV1,
        position: u64,
        reader: &impl BusinessIndexNodeReader,
    ) -> Result<BusinessIndexInclusionV1, String> {
        let key = oasis7_formal_protocol::controlled_authority::formal::business_history::formal_historical_cut_key(world, position)
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
        Self::decode_cbor_with_root(bytes, |root| root.validate())
    }
    fn decode_cbor_with_root(
        bytes: &[u8],
        validate_root: impl FnOnce(&BusinessIndexRootV1) -> Result<(), String>,
    ) -> Result<Self, String> {
        if bytes.len() > MAX_TREE_BYTES {
            return Err("formal result tree byte bound".into());
        }
        let tree: Self = serde_cbor::from_slice(bytes).map_err(|error| error.to_string())?;
        tree.validate_with_root(validate_root)?;
        Ok(tree)
    }

    fn validate(&self) -> Result<(), String> {
        self.validate_with_root(|root| root.validate())
    }
    fn validate_with_root(
        &self,
        validate_root: impl FnOnce(&BusinessIndexRootV1) -> Result<(), String>,
    ) -> Result<(), String> {
        if self.schema_version != FORMAL_BUSINESS_RESULT_SCHEMA_V1 {
            return Err("formal result tree version/leaf bound".into());
        }
        self.metadata
            .validate()
            .map_err(|error| error.to_string())?;
        validate_root(&self.index_root)?;
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
    immutable_outcome_with(&mut kernel::LegacyCanonical, leaf)
}
fn immutable_outcome_with(
    ops: &mut impl kernel::CanonicalOps,
    leaf: &FormalBusinessOutcomeLeafV1,
) -> Result<String, String> {
    tree_hash_with(
        ops,
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
    outcome_leaf_with_facts(
        &mut kernel::LegacyFacts(world),
        result,
        first_formal_position,
    )
}
fn outcome_leaf_with_facts(
    facts: &mut impl kernel::Facts,
    result: &CanonicalIntentResultV1,
    first_formal_position: u64,
) -> Result<FormalBusinessOutcomeLeafV1, String> {
    result
        .request
        .validate()
        .map_err(|error| error.to_string())?;
    let original_request_digest = tree_hash_with(
        facts,
        "formal-business-original-request-v1",
        &result.request,
    )?;
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
    let visibility_scope = facts.scope(&result.request)?;
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
    identity: &oasis7_formal_protocol::controlled_authority::formal::FormalWorldIdentityV1,
    position: u64,
    after_state_root: &str,
    governing_manifest_digest: &str,
) -> Result<FormalBusinessCommitMetadataV1, String> {
    let snapshot = world.snapshot();
    let snapshot_bytes =
        oasis7_world_state::codec::snapshot_cbor(&snapshot).map_err(|error| error.to_string())?;
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
    tree_hash_with(&mut kernel::LegacyCanonical, domain, value)
}
fn tree_hash_with<T: Serialize>(
    ops: &mut impl kernel::CanonicalOps,
    domain: &str,
    value: &T,
) -> Result<String, String> {
    let encoded = ops.canonical(&("oasis7.formal.business-result.v1", domain, value))?;
    ops.hash_work(encoded.len())?;
    Ok(blake3::hash(&encoded).to_hex().to_string())
}
fn index_leaf(
    key_digest: String,
    kind: BusinessIndexLeafKindV1,
    leaf: &TreeLeafV1,
) -> Result<BusinessIndexLeafV1, String> {
    index_leaf_with(&mut kernel::LegacyCanonical, key_digest, kind, leaf)
}
fn index_leaf_with(
    ops: &mut impl kernel::CanonicalOps,
    key_digest: String,
    kind: BusinessIndexLeafKindV1,
    leaf: &TreeLeafV1,
) -> Result<BusinessIndexLeafV1, String> {
    let canonical_value = ops.canonical(leaf)?;
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

#[cfg(test)]
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

pub use oasis7_game_client::formal_business::VerifiedBusinessOutcome;
pub type FormalBusinessPageVerification =
    oasis7_game_client::formal_business::FormalBusinessPageVerification<
        crate::client::ServiceClientProfile,
    >;
pub fn verify_formal_business_outcome_page(
    verifier: &mut FormalReadVerifier,
    page: &FormalBusinessOutcomePageV1<WorldServiceProjection>,
    original: &SubmitIntentRequest<WorldServicePayloadV1>,
    reader_public_key: &str,
) -> Result<FormalBusinessPageVerification, String> {
    oasis7_game_client::formal_business::verify_formal_business_outcome_page::<
        crate::client::ServiceClientProfile,
    >(verifier, page, original, reader_public_key)
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
        // Alternative receipts contain the complete private original, candidate
        // history and owner proofs. Ordinary delegated Agent reads do not grant
        // access to this owner-control material, nor does replacement ownership.
        if outcome
            .correlation
            .key
            .operation_domain
            .starts_with("safe_alternative.")
            && (reader_public_key != owner
                || reader_public_key != outcome.correlation.key.verified_subject)
        {
            return Err("safe alternative result requires its current original owner".into());
        }
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
    world.formal_business_outcome_agent_id_v1(payload)
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
    oasis7_game_client::formal_business::decode_canonical_result_leaf::<
        crate::client::ServiceClientProfile,
    >(leaf)
}

#[cfg(test)]
use oasis7_formal_protocol::controlled_authority::formal::business_index::BusinessIndexNodeV1;

#[path = "formal_result_paid.rs"]
mod paid;
pub use paid::{
    PaidFormalBusinessDescriptorV1, PaidFormalBusinessErrorV1, PaidFormalBusinessResultUpdateV1,
    build_current_formal_business_result_bounded_v1, decode_formal_business_descriptor_bounded_v1,
};

pub use paid::{
    PaidGenesisFormalBusinessResultUpdateV1, build_genesis_formal_business_result_bounded_v1,
};

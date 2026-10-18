//! Independent result and journal proof consumption. No publication or authoritative World state lives here.
use crate::formal_read::{FormalReadVerifier, VerifiedFormalProjectionV1};
use crate::{
    authority,
    world_service_client::{CanonicalIntentResultV1, ClientWorldProfile, Payload},
};
use oasis7_client_api::world_service::*;
use oasis7_formal_protocol::controlled_authority::formal::{
    CanonicalRecordV1, VerifiedHistoricalCutInclusionV2,
    business_index::{
        BusinessIndexInclusionV1, BusinessIndexLeafKindV1, BusinessIndexLeafV1,
        BusinessIndexNodeV1, BusinessIndexRootV1, metadata_key_digest,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
const MAX_PAGE_BYTES: usize = 4 * 1024 * 1024;
#[path = "formal_business_history.rs"]
mod historical;
pub use historical::FormalHistoricalBusinessPublicationV1;
#[path = "formal_business_changes.rs"]
mod changes;
pub use changes::*;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum TreeLeafV1 {
    CommitMetadata(FormalBusinessCommitMetadataV1),
    Outcome(FormalBusinessOutcomeLeafV1),
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
pub enum FormalBusinessPageVerification<H: ClientWorldProfile> {
    Progress(VerifiedFormalProjectionV1<H::Projection>),
    Complete(VerifiedBusinessOutcome),
}

/// Verifies a bounded page against an already externally anchored formal
/// verifier. The verifier cache advances only after every page field checks.
pub fn verify_formal_business_outcome_page<H: ClientWorldProfile>(
    verifier: &mut FormalReadVerifier<H::Projection>,
    page: &FormalBusinessOutcomePageV1<H::Projection>,
    original_request: &SubmitIntentRequest<Payload<H>>,
    reader_public_key: &str,
) -> Result<FormalBusinessPageVerification<H>, String> {
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
            let outcome = historical::verify_historical_result::<H>(
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
            let retained = decode_canonical_result_leaf::<H>(&proof.outcome)?;
            if &retained.request != original_request {
                return Err("formal result retained original envelope differs from caller".into());
            }
        }
        verify_result_proof::<H>(
            proof,
            &page.requested_correlation,
            original_request,
            &page.target_request.scope_id,
            reader_public_key,
            &page.target_view.view,
            record,
            true,
        )?;
        let commit = committed_reference::<H::Projection>(
            record,
            &proof.commit_metadata,
            &page.target_view.view,
        )?;
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
fn verify_result_proof<H: ClientWorldProfile>(
    proof: &FormalBusinessResultProofV1,
    expected: &RequestCorrelation,
    original_request: &SubmitIntentRequest<Payload<H>>,
    scope_id: &str,
    reader_public_key: &str,
    projection: &H::Projection,
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
    validate_metadata_projection::<H::Projection>(&proof.commit_metadata, projection, record)?;
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

fn validate_metadata_projection<P: crate::formal_read::FormalReadProjection>(
    metadata: &FormalBusinessCommitMetadataV1,
    projection: &P,
    record: &CanonicalRecordV1,
) -> Result<(), String> {
    let projection = projection.read_metadata()?;

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

fn metadata_from_projection<P: crate::formal_read::FormalReadProjection>(
    projection: &P,
    record: &CanonicalRecordV1,
) -> Result<FormalBusinessCommitMetadataV1, String> {
    let original_projection = projection.clone();
    let projection = projection.read_metadata()?;

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
    validate_metadata_projection::<P>(&metadata, &original_projection, record)?;
    Ok(metadata)
}

fn committed_reference<P: crate::formal_read::FormalReadProjection>(
    record: &CanonicalRecordV1,
    metadata: &FormalBusinessCommitMetadataV1,
    projection: &P,
) -> Result<CommitRef, String> {
    let projection = projection.read_metadata()?;

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

fn result_index_key(key: &RequestKey) -> Result<String, String> {
    authority::request_digest("request-key", key)?
        .strip_prefix("blake3:")
        .map(str::to_owned)
        .ok_or_else(|| "formal result key digest encoding".into())
}

/// Cold replay material is accepted only when all business identity and result
/// fields agree with its authenticated leaf; callers must also prove inclusion.
pub fn decode_canonical_result_leaf<H: ClientWorldProfile>(
    leaf: &FormalBusinessOutcomeLeafV1,
) -> Result<CanonicalIntentResultV1<H>, String> {
    leaf.validate().map_err(|e| e.to_string())?;
    let bytes = hex::decode(
        leaf.canonical_result_cbor_hex
            .as_ref()
            .ok_or("HistoryUnavailable: original result bytes were not retained")?,
    )
    .map_err(|_| "formal retained result encoding")?;
    let result: CanonicalIntentResultV1<H> =
        serde_cbor::from_slice(&bytes).map_err(|e| e.to_string())?;
    let key = authority::request_digest("request-key", &result.request.correlation.key)?;
    H::validate_canonical_result(
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

#[cfg(test)]
mod audience_tests {
    use super::*;
    fn h(byte: u8) -> String {
        hex::encode([byte; 32])
    }
    fn outcome() -> FormalBusinessOutcomeLeafV1 {
        FormalBusinessOutcomeLeafV1 {
            schema_version: FORMAL_BUSINESS_RESULT_SCHEMA_V1,
            correlation: RequestCorrelation {
                key: RequestKey {
                    world: WorldIdentity {
                        world_id: "world".into(),
                        genesis_digest: h(1),
                    },
                    verified_subject: h(4),
                    operation_domain: "agent_chat".into(),
                    nonce_scope: "canonical_owner_chat".into(),
                    request_id_or_nonce: "17".into(),
                },
                payload_digest: h(5),
            },
            visibility_scope: "agent:agent-7".into(),
            original_request_digest: h(6),
            action_id: 9,
            committed_height: 12,
            first_formal_position: 13,
            outcome: FormalBusinessOutcomeV1::Committed {
                receipt: serde_json::json!({"opaque_receipt":"private"}),
            },
            canonical_result_cbor_hex: None,
        }
    }

    #[test]
    fn formal_public_result_is_readable_only_by_its_committed_original_sender() {
        let mut leaf = outcome();
        leaf.visibility_scope = "public".into();
        validate_result_audience(&leaf, "public", &leaf.correlation.key.verified_subject).unwrap();
        assert!(validate_result_audience(&leaf, "public", &h(12)).is_err());
    }
    #[test]
    fn private_result_cannot_be_read_through_public_scope() {
        let leaf = outcome();
        assert!(
            validate_result_audience(&leaf, "public", &leaf.correlation.key.verified_subject)
                .is_err()
        );
    }
}

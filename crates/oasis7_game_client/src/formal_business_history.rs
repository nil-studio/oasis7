//! Exact original publication delegated by a separately verified current root.
use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalHistoricalBusinessPublicationV1<P> {
    pub schema_version: u32,
    pub current_index_root: BusinessIndexRootV1,
    pub cut_inclusion: BusinessIndexInclusionV1,
    pub current_result: FormalBusinessResultProofV1,
    pub original_request: ReadWorldViewRequest,
    pub original_view: ReadWorldViewResponse<P>,
}
impl<P: crate::formal_read::FormalReadProjection> FormalHistoricalBusinessPublicationV1<P> {
    pub fn encode_hex(&self) -> Result<String, String> {
        let bytes = serde_cbor::to_vec(self).map_err(|error| error.to_string())?;
        if bytes.len() > MAX_PAGE_BYTES {
            return Err("historical business publication byte bound".into());
        }
        Ok(hex::encode(bytes))
    }
    fn decode_hex(encoded: &str) -> Result<Self, String> {
        if encoded.len() > MAX_PAGE_BYTES * 2 {
            return Err("historical business publication byte bound".into());
        }
        let bytes = hex::decode(encoded).map_err(|error| error.to_string())?;
        serde_cbor::from_slice(&bytes).map_err(|error| error.to_string())
    }
}

pub(super) fn verify_historical_result<H: ClientWorldProfile>(
    verifier: &FormalReadVerifier<H::Projection>,
    current: &VerifiedFormalProjectionV1<H::Projection>,
    page: &FormalBusinessOutcomePageV1<H::Projection>,
    proof: &FormalBusinessResultProofV1,
    encoded: &str,
    original: &SubmitIntentRequest<Payload<H>>,
    reader: &str,
) -> Result<VerifiedBusinessOutcome, String> {
    let publication = FormalHistoricalBusinessPublicationV1::<H::Projection>::decode_hex(encoded)?;
    if publication.schema_version != 2
        || publication.original_request.scope_id != "public"
        || publication.original_request.world != page.target_request.world
        || publication.original_request.fixed_commit.is_some()
        || publication.original_request.min_commit.is_some()
    {
        return Err("historical business publication audience/world binding".into());
    }
    let current_record = current.commit().record();
    if publication.current_result.outcome.first_formal_position
        != proof.outcome.first_formal_position
        || publication.current_result.outcome.first_formal_position > current_record.position
    {
        return Err("current-root historical result position binding".into());
    }
    let current_retained = decode_canonical_result_leaf::<H>(&publication.current_result.outcome)?;
    if &current_retained.request != original {
        return Err("current-root result differs from complete original request".into());
    }
    verify_result_proof::<H>(
        &publication.current_result,
        &page.requested_correlation,
        original,
        &page.target_request.scope_id,
        reader,
        &page.target_view.view,
        current_record,
        false,
    )?;
    if publication.current_result.outcome != proof.outcome {
        return Err("current-root result differs from original immutable outcome".into());
    }
    let historical = VerifiedHistoricalCutInclusionV2::verify(
        current.read_anchor(),
        &publication.current_index_root,
        &publication.cut_inclusion,
        proof.outcome.first_formal_position,
    )
    .map_err(|error| error.to_string())?;
    let verified = verifier.verify_committed_historical_view(
        &historical,
        &publication.original_view,
        &publication.original_request,
        reader,
    )?;
    let retained = decode_canonical_result_leaf::<H>(&proof.outcome)?;
    if &retained.request != original {
        return Err("historical result differs from complete original request".into());
    }
    verify_result_proof::<H>(
        proof,
        &page.requested_correlation,
        original,
        &page.target_request.scope_id,
        reader,
        &publication.original_view.view,
        verified.commit().record(),
        true,
    )?;
    let commit = committed_reference::<H::Projection>(
        verified.commit().record(),
        &proof.commit_metadata,
        &publication.original_view.view,
    )?;
    if commit != publication.original_view.version.commit {
        return Err("historical business aliases differ from original publication".into());
    }
    Ok(VerifiedBusinessOutcome {
        commit,
        proof: proof.clone(),
    })
}

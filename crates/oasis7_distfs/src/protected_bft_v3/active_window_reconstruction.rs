//! Historical immutable install readback, derived from authenticated original S.
//! This is not a current checkpoint, lease, or new installation authority.
use super::archive_query::ValidatedBftSuccessorOriginalStateV1;
use super::binding::{BftSignerBindingV3, BftSignerErrorV3, invalid};
use super::state::{BftSignerStateV3, validate_state_with_active_context};
use super::successor_state::*;
use super::successor_window::*;

#[derive(Debug, Clone)]
pub struct ValidatedBftSuccessorInstalledStateV1 {
    state: BftSignerStateV3,
}
impl ValidatedBftSuccessorInstalledStateV1 {
    pub fn installed_state(&self) -> &BftSignerStateV3 {
        &self.state
    }
}
pub(crate) fn reconstruct(
    original: &ValidatedBftSuccessorOriginalStateV1,
    binding: &BftSignerBindingV3,
    provider: &str,
    record: &BftSuccessorWindowStoredRequestV1,
) -> Result<ValidatedBftSuccessorInstalledStateV1, BftSignerErrorV3> {
    if provider != original.provider_public_key() || binding.digest()? != original.binding_digest()
    {
        return Err(invalid(
            "historical install differs from authenticated original provider/binding provenance",
        ));
    }
    record.validate(binding, provider)?;
    let before = validate_state_with_active_context(
        binding,
        Some(provider),
        &original.state,
        original.active_context.clone(),
    )?;
    let body = &record.request.intent.body;
    if body.expected_revision != original.state.revision
        || body.expected_state_digest != original.state.state_digest
    {
        return Err(invalid(
            "historical install differs from authenticated original State",
        ));
    }
    body.checkpoint.bind_verified_state(
        binding,
        &before,
        original.active_context.as_ref().map(|c| c.checkpoint()),
    )?;
    record
        .original_public_state
        .validate_bytes(&super::crypto::encode_bounded(
            &original.state,
            super::crypto::BFT_SIGNER_MAX_FRAME_BYTES_V3,
        )?)?;
    super::successor_public_archive::validate_actual_public_manifest(
        &record.manifest,
        &original.state,
    )?;
    let mut extension = original
        .state
        .successor_window
        .clone()
        .unwrap_or_else(BftSuccessorWindowStateV1::empty);
    if extension.blocks_writes() {
        return Err(invalid("historical original predecessor is unfinished"));
    }
    validate_manifest_predecessor(
        binding,
        &record.manifest,
        &body.checkpoint,
        extension.requests.last(),
    )?;
    let previous = original
        .state
        .successor_window
        .as_ref()
        .map(BftSuccessorWindowStateV1::prior)
        .transpose()?;
    extension.usage = body.checkpoint.usage;
    extension.active_request_id = Some(body.checkpoint.request_id.clone());
    extension.reservation = Some(BftSuccessorWindowReservationV1 {
        body: body.clone(),
        manifest: record.manifest.clone(),
        original_public_state: record.original_public_state.clone(),
        previous,
    });
    let reserved = original.state.with_successor_window_context(
        binding,
        extension.clone(),
        original.active_context.clone(),
        false,
    )?;
    let reserved = validate_successor_only_transition_with_context(
        binding,
        &before,
        &reserved,
        provider,
        original.active_context.clone(),
    )?;
    let mut prepared_record = record.clone();
    prepared_record.request.status = BftSuccessorWindowStatusV1::Prepared;
    prepared_record.request.receipt = None;
    prepared_record.request.adopted_evidence = None;
    prepared_record.request.superseded_reason = None;
    extension.reservation = None;
    extension.requests.push(prepared_record);
    let prepared = reserved.state.with_successor_window_context(
        binding,
        extension.clone(),
        original.active_context.clone(),
        false,
    )?;
    if prepared.revision != record.prepared_revision {
        return Err(invalid(
            "historical prepared revision differs from exact two-CAS path",
        ));
    }
    let prepared = validate_successor_only_transition_with_context(
        binding,
        &reserved,
        &prepared,
        provider,
        original.active_context.clone(),
    )?;
    let receipt = record
        .request
        .receipt
        .as_ref()
        .ok_or_else(|| invalid("historical installed receipt missing"))?;
    let last = extension
        .requests
        .last_mut()
        .ok_or_else(|| invalid("historical original request missing"))?;
    last.request.status = BftSuccessorWindowStatusV1::InstalledPending;
    last.request.receipt = Some(receipt.clone());
    extension.minimum = Some(body.checkpoint.minimum()?);
    let installed = prepared.state.with_successor_window_context(
        binding,
        extension,
        original.active_context.clone(),
        false,
    )?;
    if installed.revision != receipt.body.committed_revision
        || record
            .request
            .adopted_evidence
            .as_ref()
            .is_some_and(|e| e.readback_state_digest != installed.state_digest)
    {
        return Err(invalid(
            "historical installed revision/readback differs from immutable original evidence",
        ));
    }
    validate_successor_only_transition_with_context(
        binding,
        &prepared,
        &installed,
        provider,
        original.active_context.clone(),
    )?;
    Ok(ValidatedBftSuccessorInstalledStateV1 { state: installed })
}

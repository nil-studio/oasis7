//! The original prepared marker field/identity kernel shared by Legacy and
//! Current. This paid record is DATA, not proof that an action was allocated,
//! submitted, executed, accepted or linked to a durable receipt.
use crate::{
    cognition::AgentDecisionEnvelopeV1,
    cognition_recovery::{RuntimeCognitionResponseArtifactV1, WorldCommitRecordV1},
};
use oasis7_wasm_abi::{CanonicalCborStreamError as Error, CapabilityCanonicalWorkV1};
use serde_json::{Value, json};
pub(in crate::world) struct PaidPreparedCognitionMarkerDataV1<C> {
    record: WorldCommitRecordV1,
    _fee: C,
}
impl<C> PaidPreparedCognitionMarkerDataV1<C> {
    pub(in crate::world) fn record(&self) -> &WorldCommitRecordV1 {
        &self.record
    }
}
impl PaidPreparedCognitionMarkerDataV1<()> {
    pub(in crate::world) fn into_legacy_record(self) -> WorldCommitRecordV1 {
        self.record
    }
}
fn text<A: CapabilityCanonicalWorkV1>(value: &str, source: &A) -> Result<String, Error> {
    let mut out = Vec::with_capacity(value.len());
    for chunk in value.as_bytes().chunks(8192) {
        source.charge_work(1, chunk.len() as u64)?;
        out.extend_from_slice(chunk)
    }
    Ok(String::from_utf8(out).expect("original Cognition UTF8"))
}
fn digest<A: CapabilityCanonicalWorkV1, T: serde::Serialize + ?Sized>(
    domain: &str,
    value: &T,
    source: &A,
) -> Result<String, Error> {
    let value =
        oasis7_agent_api::world_cognition::cognition_digest_with_work_v1(domain, value, source)?;
    text(value.as_str(), source)
}
/// The action number is DATA here. The fixed Current prepared-marker producer
/// must retain its actual AllocatedCurrentCognitionPreparationV1 separately.
pub(in crate::world) fn prepare_cognition_marker_fields_with_work_v1<
    A: CapabilityCanonicalWorkV1,
>(
    envelope: &AgentDecisionEnvelopeV1,
    response_identity: Option<&RuntimeCognitionResponseArtifactV1>,
    response_artifact: Option<&Value>,
    world_root: &str,
    manifest_hash: &str,
    action_number: u64,
    source: &A,
) -> Result<
    Result<PaidPreparedCognitionMarkerDataV1<A::Charge>, (crate::WorldError, A::Charge)>,
    Error,
> {
    let mut layout = 65536usize;
    for value in [
        envelope.envelope_idempotency_key.as_str(),
        envelope.envelope_digest.as_str(),
        envelope.world_id.as_str(),
        envelope.branch_id.as_str(),
        envelope.finality_status.as_str(),
        envelope.agent_id.as_str(),
        envelope.agent_session_id.as_str(),
        envelope.agent_turn_id.as_str(),
        envelope.decision_request_id.as_str(),
        envelope.request_digest.as_str(),
        world_root,
        manifest_hash,
    ]
    .into_iter()
    .map(Some)
    .chain([
        envelope.finality_block_hash.as_ref().map(String::as_str),
        response_identity.map(|v| v.context_discriminator.as_str()),
    ]) {
        source.charge_work(1, 0)?;
        if let Some(value) = value {
            for part in value.as_bytes().chunks(8192) {
                source.charge_work(1, part.len() as u64)?;
            }
            layout = layout
                .checked_add(
                    value
                        .len()
                        .checked_mul(8)
                        .ok_or(Error::ResourceLimited("prepared marker layout"))?,
                )
                .ok_or(Error::ResourceLimited("prepared marker layout"))?;
        }
    }
    source.charge_work(256, layout as u64)?;
    let fee = source.reserve(layout)?;
    let action_id = format!("action:{action_number}");
    let commit_id = digest(
        "oasis7.cognition.commit-id.v1",
        &json!({"envelope_digest":envelope.envelope_digest,"envelope_idempotency_key":envelope.envelope_idempotency_key}),
        source,
    )?;
    let receipt_id = digest(
        "oasis7.cognition.receipt-id.v1",
        &json!({"commit_id":commit_id}),
        source,
    )?;
    // Keep the original initial preimage/processing before the response-bound
    // receipt digest replaces it below.
    let receipt_digest = digest(
        "oasis7.cognition.receipt.v1",
        &json!({"receipt_id":receipt_id,"envelope_digest":envelope.envelope_digest,"action_id":action_id,"world_root":world_root}),
        source,
    )?;
    let staged_event_root = digest(
        "oasis7.cognition.staged-event-root.v1",
        &json!({"world_root":world_root,"envelope_digest":envelope.envelope_digest}),
        source,
    )?;
    let finality = envelope.derive_finality_binding_digest_with_work_v1(source)?;
    let mut marker = WorldCommitRecordV1 {
        schema_version: text("world-commit-record.v1", source)?,
        commit_id,
        envelope_idempotency_key: text(&envelope.envelope_idempotency_key, source)?,
        envelope_digest: text(&envelope.envelope_digest, source)?,
        world_id: text(&envelope.world_id, source)?,
        branch_id: text(&envelope.branch_id, source)?,
        finality_epoch: envelope.finality_epoch,
        finality_block_hash: envelope
            .finality_block_hash
            .as_deref()
            .map(|v| text(v, source))
            .transpose()?,
        finality_status: text(&envelope.finality_status, source)?,
        finality_binding_digest: text(finality.as_str(), source)?,
        runtime_manifest_hash: text(manifest_hash, source)?,
        action_id,
        parent_tick: envelope.base_tick,
        parent_world_hash: text(world_root, source)?,
        staged_event_root,
        staged_state_root: text(world_root, source)?,
        receipt_id,
        receipt_digest,
        reorg_epoch: envelope.reorg_epoch,
        cognition_journal_seq: 0,
        status: text("prepared", source)?,
        agent_id: text(&envelope.agent_id, source)?,
        agent_session_id: text(&envelope.agent_session_id, source)?,
        agent_turn_id: text(&envelope.agent_turn_id, source)?,
        decision_request_id: text(&envelope.decision_request_id, source)?,
        request_digest: text(&envelope.request_digest, source)?,
        feedback_id: digest(
            "oasis7.cognition.feedback-id.v1",
            &envelope.envelope_digest,
            source,
        )?,
        response_context_discriminator: text(
            response_identity.map_or("", |v| v.context_discriminator.as_str()),
            source,
        )?,
        response_context_version: response_identity.map_or(0, |v| v.context_version),
        response_retry_seq: response_identity.map_or(envelope.retry_seq, |v| v.retry_seq),
        transport_attempt: response_identity.map_or(0, |v| v.transport_attempt),
        response_artifact_digest: match response_artifact {
            None => String::new(),
            Some(value) => digest("oasis7.cognition.response.v1", value, source)?,
        },
        abort_reason: None,
    };
    let receipt = crate::world::cognition_persistence::receipt_digest_for_marker_with_work_v1(
        &marker, source,
    )?;
    marker.receipt_digest = text(receipt.as_str(), source)?;
    match crate::world::cognition_persistence::validate_cognition_commit_marker_with_work_v1(
        &marker, source,
    ) {
        Ok(()) => Ok(Ok(PaidPreparedCognitionMarkerDataV1 {
            record: marker,
            _fee: fee,
        })),
        Err(
            crate::world::cognition_persistence::receipt_observation::ReceiptReadErrorV1::Work(
                error,
            ),
        ) => Err(error),
        Err(
            crate::world::cognition_persistence::receipt_observation::ReceiptReadErrorV1::World(
                error,
            ),
        ) => Ok(Err((error, fee))),
    }
}

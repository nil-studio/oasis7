//! A recovery activation changes authority metadata and appends only the exact
//! parent HistoricalCut. All business-state artifacts are retained byte-for-byte.
use super::*;

pub(super) fn verify_control(
    record: &CanonicalRecordV1,
    material: &ClosedRecord,
    parent: &CanonicalRecordV1,
    envelope: &FormalBusinessResultEnvelopeV1,
    reader: &impl BusinessIndexNodeReader,
) -> Result<(), FormalError> {
    let FormalRecordKindV1::RecoveryActivation {
        certificate_digest, ..
    } = &record.kind
    else {
        return Err(invalid("control material record kind"));
    };
    if record.before_state_root != parent.after_state_root
        || record.after_state_root != parent.after_state_root
        || record.parent_record_id != parent.record_id()?
        || record.position
            != parent
                .position
                .checked_add(1)
                .ok_or_else(|| invalid("control next position overflow"))?
        || envelope.retained_history_delta.is_some()
        || envelope.delta.updates.len() != 2
        || envelope.delta.updates.iter().any(|leaf| {
            !matches!(
                leaf.kind,
                business_index::BusinessIndexLeafKindV1::CommitMetadata
                    | business_index::BusinessIndexLeafKindV1::HistoricalCut
            )
        })
    {
        return Err(invalid(
            "recovery control cannot create business results or change retained state",
        ));
    }
    for role in [
        ArtifactRole::Snapshot,
        ArtifactRole::Journal,
        ArtifactRole::NonceIndex,
        ArtifactRole::EffectOutbox,
        ArtifactRole::ExecutionManifest,
        ArtifactRole::Wasm,
        ArtifactRole::Rules,
        ArtifactRole::NativeExecutionEnvironment,
    ] {
        if record.artifact_roots.get(&role) != parent.artifact_roots.get(&role) {
            return Err(invalid(
                "recovery control changed a retained business artifact",
            ));
        }
    }
    let parent_result = parent
        .artifact_roots
        .get(&ArtifactRole::Result)
        .ok_or_else(|| invalid("recovery control original Result root missing"))?;
    let parent_result_object = material
        .objects
        .iter()
        .find(|object| &object.content_hash == parent_result)
        .ok_or_else(|| invalid("recovery control original Result bytes must remain reachable"))?;
    let parent_envelope = FormalBusinessResultEnvelopeV1::decode(&parent_result_object.bytes)?;
    if envelope.execution_result_cbor != parent_envelope.execution_result_cbor {
        return Err(invalid(
            "recovery control changed original execution result bytes",
        ));
    }
    let input = material
        .objects
        .iter()
        .find(|object| object.content_hash == record.payload_digest)
        .ok_or_else(|| invalid("recovery control complete original certificate Input missing"))?;
    let certificate = decode_recovery_activation_input(&input.bytes)?;
    if recovery_certificate_digest(&certificate)? != *certificate_digest
        || certificate.body.sealed_cut.position != parent.position
        || certificate.body.sealed_cut.record_id != parent.record_id()?
        || certificate.body.successor.authority_id()? != record.original_authority_id
        || certificate.body.successor.epoch != record.original_epoch
    {
        return Err(invalid(
            "recovery control exact certificate and original qualified parent",
        ));
    }
    // Equality of the Snapshot artifact root above fixes the exact descriptor
    // bytes. Verify their retained root and referenced prefix without inventing
    // a Frozen request or accepting an omitted prior descriptor.
    retention::verify_retained_transition(false, material, Some(material), None, reader)
}

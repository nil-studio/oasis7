use super::*;
use crate::{
    incremental_field_member::*,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_projection::FieldOwner,
    incremental_record_key_read::{OriginalKeyKind, OriginalSemanticKeyProbe},
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_state::{StateEntryV1, StateNamespaceV1},
    incremental_value::ValueNodeReader,
};
pub(super) fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Runtime artifact content profile")
}
pub(super) fn identify(
    namespace: StateNamespaceV1,
    bytes: &[u8],
) -> Result<Option<(RuntimeArtifactContentFieldV1, WorldFieldRecordKindV1)>, ValueTreeError> {
    if !bytes.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(None);
    }
    let binding = WorldFieldKeyV1::from_bytes(bytes).map_err(ValueTreeError::Invalid)?;
    if binding.owner != FieldOwner::Runtime {
        return Ok(None);
    }
    let Some(field) = RuntimeArtifactContentFieldV1::ALL
        .into_iter()
        .find(|f| f.descriptor().name == binding.name)
    else {
        return Ok(None);
    };
    let expected = WorldFieldKeyV1::new(field.descriptor(), binding.kind, binding.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if namespace != StateNamespaceV1::Artifacts
        || expected.as_bytes() != bytes
        || !(binding.kind == WorldFieldRecordKindV1::Metadata
            || binding.kind
                == if field == RuntimeArtifactContentFieldV1::Original {
                    WorldFieldRecordKindV1::Bytes
                } else {
                    WorldFieldRecordKindV1::Entry
                })
    {
        return Err(invalid());
    }
    Ok(Some((field, binding.kind)))
}
pub(super) fn check_entry<R: ValueNodeReader, C: FnMut() -> bool>(
    field: RuntimeArtifactContentFieldV1,
    kind: WorldFieldRecordKindV1,
    entry: &StateEntryV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<Option<OriginalArtifactRefV1>, ValueTreeError> {
    let key = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let key = WorldFieldKeyV1::new(field.descriptor(), kind, key.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    records
        .values
        .verify_closure(&entry.value, records.record_bytes, &mut *records.cancelled)?;
    if kind == WorldFieldRecordKindV1::Metadata {
        let expected = WorldFieldKeyV1::new(field.descriptor(), kind, None)
            .map_err(ValueTreeError::Invalid)?;
        if key.as_bytes() != expected.as_bytes() {
            return Err(invalid());
        }
        records.presence(&entry.value, &key, 0)?;
        return Ok(None);
    }
    let mut binding = [0; RUNTIME_ARTIFACT_ORIGINAL_BYTES];
    let mut semantic_sink = std::io::sink();
    let mut semantic = OriginalSemanticKeyProbe::new(OriginalKeyKind::Text, &mut semantic_sink);
    let mut payload = std::io::Cursor::new(binding.as_mut_slice());
    let result = read_member_record_v1(
        &entry.value,
        &key,
        records.values,
        MemberRecordLimitsV1 {
            semantic_bytes: records.semantic_bytes,
            payload_bytes: if field == RuntimeArtifactContentFieldV1::Original {
                RUNTIME_ARTIFACT_ORIGINAL_BYTES as u64
            } else {
                0
            },
            record_bytes: records.record_bytes,
        },
        &mut semantic,
        &mut payload,
        &mut *records.cancelled,
    );
    if let Some(error) = semantic.error() {
        return Err(ValueTreeError::Invalid(error));
    }
    let info = result?;
    semantic.finish()?;
    let expected = if field == RuntimeArtifactContentFieldV1::Original {
        MemberPayloadCodecV1::Bytes
    } else {
        MemberPayloadCodecV1::Unit
    };
    if info.codec != expected
        || (field == RuntimeArtifactContentFieldV1::Original
            && info.payload_bytes != RUNTIME_ARTIFACT_ORIGINAL_BYTES as u64)
    {
        return Err(invalid());
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    if field == RuntimeArtifactContentFieldV1::Original {
        runtime_artifact_original_from_v1(&binding).map(Some)
    } else {
        Ok(None)
    }
}

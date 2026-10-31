use super::{OwnerKeyKind, OwnerSemanticKeyProbe, WorldStateOwnerError};
use crate::{
    incremental_field_member::{
        MemberPayloadCodecV1, MemberRecordLimitsV1, WorldFieldKeyV1, read_member_record_v1,
    },
    incremental_field_presence::{PresenceRecordLimitsV1, read_field_presence_v1},
    incremental_ledger_value::Utf8Output,
    incremental_module_instance_fields::ModuleInstanceFieldV1,
    incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape},
    incremental_world_field_key::{WorldFieldKeyRefV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_state::{
        MerkleNodeReader, StateEntryV1, StateNamespaceV1, StateRootV1, StateScanReader,
    },
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use std::io;

#[derive(Debug, Clone, Copy, Default)]
struct Seen {
    metadata: u8,
}

#[derive(Default)]
pub(super) struct Coverage {
    seen: [Seen; 8],
}

pub(super) const FIELD_COUNT: u8 = ModuleInstanceFieldV1::ALL.len() as u8;

pub(super) fn handles_name(name: &str) -> bool {
    ModuleInstanceFieldV1::ALL
        .into_iter()
        .any(|field| field.name() == name)
}

fn descriptor(field: ModuleInstanceFieldV1) -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::WorldState,
        name: field.name(),
        destination: FieldDestination::State(StateNamespaceV1::Business),
        shape: FieldShape::Map,
    }
}

fn field_for_name(name: &str) -> Option<(usize, ModuleInstanceFieldV1)> {
    ModuleInstanceFieldV1::ALL
        .into_iter()
        .enumerate()
        .find(|(_, field)| field.name() == name)
}

fn validate_member<R: ValueNodeReader>(
    field: ModuleInstanceFieldV1,
    key: &WorldFieldKeyV1,
    value_root: &[u8; 32],
    values: &mut ValueReadContext<'_, R>,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), WorldStateOwnerError> {
    use ModuleInstanceFieldV1 as F;

    let mut key_probe = OwnerSemanticKeyProbe::new(OwnerKeyKind::Text, max_value_bytes);
    let mut discarded = io::sink();
    let mut utf8_payload = Utf8Output::new(&mut discarded);
    let mut fixed_payload = [0u8; 8];

    let info = match field {
        F::InstanceId | F::ModuleId | F::Version | F::WasmHash | F::Owner | F::Target => {
            let result = read_member_record_v1(
                value_root,
                key,
                values,
                MemberRecordLimitsV1 {
                    semantic_bytes: max_value_bytes,
                    payload_bytes: max_value_bytes,
                    record_bytes: max_value_bytes,
                },
                &mut key_probe,
                &mut utf8_payload,
                cancelled,
            );
            if key_probe.invalid {
                return Err(WorldStateOwnerError::Invalid(
                    "module instance semantic key profile",
                ));
            }
            if utf8_payload.invalid() {
                return Err(WorldStateOwnerError::Invalid(
                    "module instance text payload UTF-8",
                ));
            }
            result.map_err(WorldStateOwnerError::Value)?
        }
        F::Active => {
            let result = read_member_record_v1(
                value_root,
                key,
                values,
                MemberRecordLimitsV1 {
                    semantic_bytes: max_value_bytes,
                    payload_bytes: max_value_bytes,
                    record_bytes: max_value_bytes,
                },
                &mut key_probe,
                &mut &mut fixed_payload[..],
                cancelled,
            );
            if key_probe.invalid {
                return Err(WorldStateOwnerError::Invalid(
                    "module instance semantic key profile",
                ));
            }
            result.map_err(WorldStateOwnerError::Value)?
        }
        F::InstalledAt => {
            let result = read_member_record_v1(
                value_root,
                key,
                values,
                MemberRecordLimitsV1 {
                    semantic_bytes: max_value_bytes,
                    payload_bytes: max_value_bytes,
                    record_bytes: max_value_bytes,
                },
                &mut key_probe,
                &mut &mut fixed_payload[..],
                cancelled,
            );
            if key_probe.invalid {
                return Err(WorldStateOwnerError::Invalid(
                    "module instance semantic key profile",
                ));
            }
            result.map_err(WorldStateOwnerError::Value)?
        }
    };
    if !key_probe.finish() {
        return Err(WorldStateOwnerError::Invalid(
            "module instance semantic key framing",
        ));
    }
    let codec_matches = match field {
        F::InstanceId | F::ModuleId | F::Version | F::WasmHash | F::Owner => {
            info.codec == MemberPayloadCodecV1::Text && utf8_payload.complete()
        }
        F::Target => {
            (info.codec == MemberPayloadCodecV1::Unit && info.payload_bytes == 0)
                || (info.codec == MemberPayloadCodecV1::Text && utf8_payload.complete())
        }
        F::Active => {
            info.codec == MemberPayloadCodecV1::Bool
                && info.payload_bytes == 1
                && fixed_payload[0] <= 1
        }
        F::InstalledAt => info.codec == MemberPayloadCodecV1::U64 && info.payload_bytes == 8,
    };
    if !codec_matches {
        return Err(WorldStateOwnerError::Invalid(
            "module instance member payload profile",
        ));
    }
    Ok(())
}

/// Validate one module-instance owner leaf and prove all seven related leaves
/// by point lookup through the same guarded outer scan reader. This uses
/// constant memory per row and intentionally charges every lookup's node and
/// byte reads to the shared scan budget.
pub(super) fn visit_entry<M, V, C>(
    coverage: &mut Coverage,
    root: &StateRootV1,
    entry: &StateEntryV1,
    parsed: WorldFieldKeyRefV1<'_>,
    reader: &StateScanReader<'_, M, C>,
    values: &mut ValueReadContext<'_, V>,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<bool, WorldStateOwnerError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    C: FnMut() -> bool,
{
    let Some((index, field)) = field_for_name(parsed.name) else {
        return Ok(false);
    };
    let expected = descriptor(field);
    if parsed.owner != expected.owner
        || parsed.namespace != StateNamespaceV1::Business
        || parsed.shape != expected.shape
        || root.namespace != StateNamespaceV1::Business
        || entry.namespace != StateNamespaceV1::Business
    {
        return Err(WorldStateOwnerError::Invalid(
            "module instance descriptor registration",
        ));
    }
    let key = WorldFieldKeyV1::new(expected, parsed.kind, parsed.semantic_key)
        .map_err(WorldStateOwnerError::Invalid)?;
    if key.as_bytes() != entry.key.as_slice() {
        return Err(WorldStateOwnerError::Invalid(
            "module instance field key mismatch",
        ));
    }
    match parsed.kind {
        WorldFieldRecordKindV1::Metadata => {
            if parsed.semantic_key.is_some() {
                return Err(WorldStateOwnerError::Invalid(
                    "module instance metadata semantic key",
                ));
            }
            coverage.seen[index].metadata = coverage.seen[index].metadata.saturating_add(1);
            read_field_presence_v1(
                &entry.value,
                &key,
                values,
                PresenceRecordLimitsV1 {
                    semantic_bytes: 0,
                    record_bytes: 512,
                },
                &mut io::sink(),
                || cancelled(),
            )
            .map_err(WorldStateOwnerError::Value)?;
        }
        WorldFieldRecordKindV1::Entry => {
            let digest = parsed.semantic_key.ok_or(WorldStateOwnerError::Invalid(
                "module instance member semantic key missing",
            ))?;
            validate_member(
                field,
                &key,
                &entry.value,
                values,
                max_value_bytes,
                cancelled,
            )?;
            for sibling in ModuleInstanceFieldV1::ALL {
                if sibling == field {
                    continue;
                }
                if cancelled() {
                    return Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled));
                }
                let sibling_key = WorldFieldKeyV1::new(
                    descriptor(sibling),
                    WorldFieldRecordKindV1::Entry,
                    Some(digest),
                )
                .map_err(WorldStateOwnerError::Invalid)?;
                let sibling_value = root
                    .get(sibling_key.as_bytes(), reader)
                    .map_err(WorldStateOwnerError::Scan)?
                    .ok_or(WorldStateOwnerError::Invalid(
                        "module instance sibling field missing",
                    ))?;
                validate_member(
                    sibling,
                    &sibling_key,
                    &sibling_value,
                    values,
                    max_value_bytes,
                    cancelled,
                )?;
            }
        }
        _ => {
            return Err(WorldStateOwnerError::Invalid(
                "unexpected module instance record kind",
            ));
        }
    }
    Ok(true)
}

pub(super) fn finish(coverage: &Coverage) -> Result<(), WorldStateOwnerError> {
    if coverage.seen.iter().any(|seen| seen.metadata != 1) {
        return Err(WorldStateOwnerError::Invalid(
            "required module instance metadata missing or duplicate",
        ));
    }
    Ok(())
}

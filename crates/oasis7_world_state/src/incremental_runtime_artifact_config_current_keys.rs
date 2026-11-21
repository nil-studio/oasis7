//! Decode a genuine current RegistryActive member identity with the original
//! semantic-key probe. This is a point consumer, not a registry enumeration.
use super::*;
use crate::{
    incremental_field_member::*,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_key_read::{OriginalKeyKind, OriginalSemanticKeyProbe},
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::io::Write;

/// Strict structural binding for the original current Registry key. A decoded
/// key alone is not a typed member, and this check does not authorize a read.
fn require_registry_entry_key_v1(
    field: ArtifactConfigFieldV1,
    key_bytes: &[u8],
) -> Result<WorldFieldKeyV1, WorldFieldOverlayErrorV1> {
    let key = WorldFieldKeyV1::from_bytes(key_bytes).map_err(ValueTreeError::Invalid)?;
    let expected = WorldFieldKeyV1::new(
        field.descriptor(),
        WorldFieldRecordKindV1::Entry,
        key.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    if expected.as_bytes() != key_bytes {
        return Err(read::invalid().into());
    }
    Ok(expected)
}

fn read_current_registry_id_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: ArtifactConfigFieldV1,
    family: &ReservedValue<ArtifactConfigFamilyQualificationV1>,
    key_bytes: &[u8],
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max_text_bytes: usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<String>, WorldFieldOverlayErrorV1> {
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if ctx.values.class != MemoryClass::Ordinary
            || family.allocation_class() != MemoryClass::Ordinary
            || !family.uses_budget(ctx.values.memory)
            || !tx.uses_budget(ctx.values.memory)
        {
            return Err(read::invalid().into());
        }
        family.validate_binding(&tx.parent_fence().parent_roots)?;
        let fee = ctx
            .values
            .memory
            .reserve(
                max_text_bytes
                    .checked_add(8192)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "registry identity workspace",
                    ))?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let expected = require_registry_entry_key_v1(field, key_bytes)?;
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        let metadata =
            WorldFieldKeyV1::new(field.descriptor(), WorldFieldRecordKindV1::Metadata, None)
                .map_err(ValueTreeError::Invalid)?;
        let metadata_root = lookup(tx, &metadata, &mut records, &mut ctx.take_state_error)?
            .ok_or_else(read::invalid)?;
        records.presence(&metadata_root, &metadata, 0)?;
        let id = lookup(tx, &expected, &mut records, &mut ctx.take_state_error)?
            .ok_or_else(read::invalid)?;
        let _body = family::outer(field, &expected, &id, &mut records)?;
        struct Text {
            bytes: Vec<u8>,
            max: usize,
        }
        impl Write for Text {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if self
                    .bytes
                    .len()
                    .checked_add(bytes.len())
                    .is_none_or(|n| n > self.max)
                {
                    return Err(std::io::Error::other("registry identity bound"));
                }
                self.bytes.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut text = Text {
            bytes: Vec::with_capacity(max_text_bytes),
            max: max_text_bytes,
        };
        let mut probe = OriginalSemanticKeyProbe::new(OriginalKeyKind::Text, &mut text);
        read_member_record_v1(
            &id,
            &expected,
            records.values,
            MemberRecordLimitsV1 {
                semantic_bytes: records.semantic_bytes,
                payload_bytes: 165,
                record_bytes: records.record_bytes,
            },
            &mut probe,
            &mut std::io::sink(),
            &mut *records.cancelled,
        )?;
        probe.finish()?;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(error) = (ctx.take_state_error)() {
            return Err(error.into());
        }
        Ok(fee.retain(String::from_utf8(text.bytes).map_err(|_| read::invalid())?))
    })
}

/// Decode the actual original outer key; this is DATA, not registry admission.
pub fn read_current_registry_active_id_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    family: &ReservedValue<ArtifactConfigFamilyQualificationV1>,
    key_bytes: &[u8],
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max_text_bytes: usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<String>, WorldFieldOverlayErrorV1> {
    read_current_registry_id_v1(
        ArtifactConfigFieldV1::RegistryActive,
        family,
        key_bytes,
        ctx,
        max_text_bytes,
        guard,
        live,
    )
}

/// Decode the actual original outer key; this is DATA, not registry admission.
pub fn read_current_registry_record_id_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    family: &ReservedValue<ArtifactConfigFamilyQualificationV1>,
    key_bytes: &[u8],
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max_text_bytes: usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<String>, WorldFieldOverlayErrorV1> {
    read_current_registry_id_v1(
        ArtifactConfigFieldV1::RegistryManifest,
        family,
        key_bytes,
        ctx,
        max_text_bytes,
        guard,
        live,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_original_registry_entry_keys_can_select_typed_identity() {
        let active = WorldFieldKeyV1::new(
            ArtifactConfigFieldV1::RegistryActive.descriptor(),
            WorldFieldRecordKindV1::Entry,
            Some([7; 32]),
        )
        .unwrap();
        let record = WorldFieldKeyV1::new(
            ArtifactConfigFieldV1::RegistryManifest.descriptor(),
            WorldFieldRecordKindV1::Entry,
            Some([7; 32]),
        )
        .unwrap();
        assert!(require_registry_entry_key_v1(
            ArtifactConfigFieldV1::RegistryActive,
            active.as_bytes()
        ).is_ok());
        assert!(require_registry_entry_key_v1(
            ArtifactConfigFieldV1::RegistryManifest,
            record.as_bytes()
        ).is_ok());
        assert!(require_registry_entry_key_v1(
            ArtifactConfigFieldV1::RegistryManifest,
            active.as_bytes()
        ).is_err());
        assert!(require_registry_entry_key_v1(
            ArtifactConfigFieldV1::RegistryActive,
            record.as_bytes()
        ).is_err());
        let meta = WorldFieldKeyV1::new(
            ArtifactConfigFieldV1::RegistryActive.descriptor(),
            WorldFieldRecordKindV1::Metadata,
            None,
        )
        .unwrap();
        assert!(require_registry_entry_key_v1(
            ArtifactConfigFieldV1::RegistryActive,
            meta.as_bytes()
        ).is_err());
        let mut malformed = active.as_bytes().to_vec();
        malformed.push(0);
        assert!(require_registry_entry_key_v1(
            ArtifactConfigFieldV1::RegistryActive,
            &malformed
        ).is_err());
    }
}

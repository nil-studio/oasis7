//! Typed member readers for the Runtime Artifacts record family.
use super::*;
use crate::{
    incremental_field_member::{MemberPayloadCodecV1, MemberRecordLimitsV1, read_member_record_v1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_value_read::{
        RecordReadLabelsV1, RecordReadProfileV1, RecordValueV1, read_record_value_v1,
    },
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeArtifactMemberProfileV1 {
    Unit,
    Unsigned,
    Signed,
    Boolean,
    Text,
    Ledger,
    OptionalUnsigned,
    OptionalText,
}

impl RuntimeArtifactMemberProfileV1 {
    fn record_profile(self) -> RecordReadProfileV1 {
        let (codec, nullable) = match self {
            Self::Unit => (MemberPayloadCodecV1::Unit, false),
            Self::Unsigned => (MemberPayloadCodecV1::U64, false),
            Self::Signed => (MemberPayloadCodecV1::I64, false),
            Self::Boolean => (MemberPayloadCodecV1::Bool, false),
            Self::Text => (MemberPayloadCodecV1::Text, false),
            Self::Ledger => (MemberPayloadCodecV1::Ledger, false),
            Self::OptionalUnsigned => (MemberPayloadCodecV1::U64, true),
            Self::OptionalText => (MemberPayloadCodecV1::Text, true),
        };
        RecordReadProfileV1 {
            codec,
            nullable,
            integer_max: u64::MAX,
            labels: RecordReadLabelsV1 {
                utf8: "runtime artifact UTF-8",
                ledger_utf8: "runtime artifact ledger UTF-8",
                ledger_frame: "runtime artifact ledger framing",
                boolean: "runtime artifact boolean",
                fixed: "runtime artifact fixed payload",
                profile: "runtime artifact member profile",
                integer: "runtime artifact integer range",
            },
        }
    }
}

fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("runtime artifact record binding")
}

/// Read the outer registered field and return its typed child root. The
/// optional latest-delta field uses Unit for None; the other two fields are
/// required. This authenticates only the outer reference, not the child's
/// complete record closure.
pub fn inspect_runtime_artifact_record_root_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: RuntimeArtifactFieldV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<ReservedValue<RecordRootRefV1>>, ValueTreeError> {
    context.check()?;
    let descriptor = field.descriptor();
    if descriptor.destination
        != crate::incremental_projection::FieldDestination::State(StateNamespaceV1::Artifacts)
    {
        return Err(invalid());
    }
    let metadata = WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let Some(metadata_value) = context.lookup(&metadata)? else {
        return Err(invalid());
    };
    context.records().presence(&metadata_value, &metadata, 0)?;

    let (key, semantic_bytes) = context.key(descriptor, WorldFieldRecordKindV1::Entry, &())?;
    let Some(value_root) = context.lookup(&key)? else {
        return Err(invalid());
    };
    let mut bytes = [0; RECORD_ROOT_BYTES];
    let info = read_member_record_v1(
        &value_root,
        &key,
        &mut context.values,
        MemberRecordLimitsV1 {
            semantic_bytes: context.semantic_bytes,
            payload_bytes: RECORD_ROOT_BYTES as u64,
            record_bytes: context.record_bytes,
        },
        &mut io::sink(),
        &mut &mut bytes[..],
        &mut context.cancelled,
    )?;
    if info.semantic_bytes != semantic_bytes {
        return Err(invalid());
    }
    if info.codec == MemberPayloadCodecV1::Unit {
        if field != RuntimeArtifactFieldV1::LatestChainResourceDelta || info.payload_bytes != 0 {
            return Err(invalid());
        }
        return Ok(None);
    }
    if info.codec != MemberPayloadCodecV1::RecordRoot
        || info.payload_bytes != RECORD_ROOT_BYTES as u64
    {
        return Err(invalid());
    }
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
    let owner = WorldFieldKeyV1::from_bytes(key.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or_else(invalid)?;
    reference
        .validate_binding(&field.schema(), &owner, StateNamespaceV1::Artifacts)
        .map_err(|_| invalid())?;
    let charge = context
        .values
        .memory
        .reserve(std::mem::size_of::<RecordRootRefV1>(), context.values.class)?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(Some(charge.retain(reference)))
}

/// Read one typed cell from a registered Artifact child record. The caller
/// supplies the closed payload profile for the cell. Absence means the member
/// key is absent; this point read does not qualify the containing family.
pub fn read_runtime_artifact_member_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    K: crate::incremental_semantic_key::SemanticKeyV1 + ?Sized,
>(
    record: RuntimeArtifactRecordV1,
    root: &StateRootV1,
    key: &K,
    profile: RuntimeArtifactMemberProfileV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_payload_bytes: u64,
    output: &mut impl io::Write,
) -> Result<Option<RecordValueV1>, ValueTreeError> {
    context.check()?;
    if root.namespace != StateNamespaceV1::Artifacts {
        return Err(invalid());
    }
    let (key, semantic_bytes) =
        context.key(record.descriptor(), WorldFieldRecordKindV1::Entry, key)?;
    let Some(value_root) = context.lookup_raw(root, key.as_bytes())? else {
        return Ok(None);
    };
    let mut values = InventoryValueReader {
        values: &mut context.values,
        key_work: &mut *context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        cancelled: &mut context.cancelled,
    };
    let decoded = read_record_value_v1(
        &value_root,
        &(key, semantic_bytes),
        &mut values,
        max_payload_bytes,
        output,
        profile.record_profile(),
    )?;
    if (values.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(Some(decoded))
}

/// Read a required nested RecordRoot reference from a typed Artifact record.
/// The caller must still qualify the referenced child before relying on it.
pub fn read_runtime_artifact_record_reference_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    K: crate::incremental_semantic_key::SemanticKeyV1 + ?Sized,
>(
    record: RuntimeArtifactRecordV1,
    root: &StateRootV1,
    key: &K,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<ReservedValue<RecordRootRefV1>>, ValueTreeError> {
    context.check()?;
    if root.namespace != StateNamespaceV1::Artifacts {
        return Err(invalid());
    }
    let (key, semantic_bytes) =
        context.key(record.descriptor(), WorldFieldRecordKindV1::Entry, key)?;
    let Some(value_root) = context.lookup_raw(root, key.as_bytes())? else {
        return Ok(None);
    };
    let mut bytes = [0; RECORD_ROOT_BYTES];
    let info = read_member_record_v1(
        &value_root,
        &key,
        &mut context.values,
        MemberRecordLimitsV1 {
            semantic_bytes: context.semantic_bytes,
            payload_bytes: RECORD_ROOT_BYTES as u64,
            record_bytes: context.record_bytes,
        },
        &mut io::sink(),
        &mut &mut bytes[..],
        &mut context.cancelled,
    )?;
    if info.codec != MemberPayloadCodecV1::RecordRoot
        || info.semantic_bytes != semantic_bytes
        || info.payload_bytes != RECORD_ROOT_BYTES as u64
    {
        return Err(invalid());
    }
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
    let owner = WorldFieldKeyV1::from_bytes(key.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or_else(invalid)?;
    reference
        .validate_binding(&record.schema(), &owner, StateNamespaceV1::Artifacts)
        .map_err(|_| invalid())?;
    let charge = context
        .values
        .memory
        .reserve(std::mem::size_of::<RecordRootRefV1>(), context.values.class)?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(Some(charge.retain(reference)))
}

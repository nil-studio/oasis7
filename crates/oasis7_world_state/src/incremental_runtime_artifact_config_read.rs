//! Typed partial inspection. No successful point read mints body/family scope.
use super::*;
use crate::{
    incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_value_read::{
        RecordReadLabelsV1, RecordReadProfileV1, RecordValueV1, read_record_value_v1,
    },
};
use oasis7_formal_protocol::{
    incremental_record_root::RecordRootRefV1,
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::io::{Cursor, Write};

#[derive(Debug, PartialEq, Eq)]
pub enum ArtifactConfigValueV1 {
    Unit,
    Unsigned(u64),
    Boolean(bool),
    Text { utf8_bytes: u64 },
    Label(&'static str),
    JsonRoot(StateRootV1),
}
pub(super) fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("artifact config typed read")
}
const LABELS: RecordReadLabelsV1 = RecordReadLabelsV1 {
    utf8: "artifact config UTF8",
    ledger_utf8: "artifact config unused ledger UTF8",
    ledger_frame: "artifact config unused ledger",
    boolean: "artifact config Bool",
    fixed: "artifact config fixed",
    profile: "artifact config profile",
    integer: "artifact config integer",
};
pub(super) fn key<R: ValueNodeReader, C: FnMut() -> bool>(
    address: ArtifactConfigAddressV1<'_>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    records.key(
        genesis::body_descriptor(),
        WorldFieldRecordKindV1::Entry,
        &address.semantic(),
    )
}
pub(super) fn value<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: ArtifactConfigBodyKindV1,
    address: ArtifactConfigAddressV1<'_>,
    id: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    max_payload_bytes: u64,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    output: &mut impl Write,
) -> Result<ArtifactConfigValueV1, ValueTreeError> {
    let spec = schema::spec(kind, address).map_err(|_| invalid())?;
    if spec.codec == MemberPayloadCodecV1::RecordRoot {
        if max_payload_bytes < 165 {
            return Err(ValueTreeError::ResourceLimited(
                "artifact config root payload",
            ));
        }
        let bytes = records.fixed::<165>(id, &key.0, key.1, MemberPayloadCodecV1::RecordRoot)?;
        let root = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
        let semantic = WorldFieldKeyV1::from_bytes(key.0.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or_else(invalid)?;
        let child = root
            .validate_binding(
                &genesis::json_profile().schema,
                &semantic,
                StateNamespaceV1::Artifacts,
            )
            .map_err(|_| invalid())?;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        return Ok(ArtifactConfigValueV1::JsonRoot(child.clone()));
    }
    let profile = RecordReadProfileV1 {
        codec: spec.codec,
        nullable: spec.nullable,
        integer_max: spec.integer_max,
        labels: LABELS,
    };
    let result = if spec.labels.is_empty() {
        read_record_value_v1(id, key, records, max_payload_bytes, output, profile)?
    } else {
        let mut bytes = [0; 32];
        let result = read_record_value_v1(
            id,
            key,
            records,
            32,
            &mut Cursor::new(bytes.as_mut_slice()),
            profile,
        )?;
        if let RecordValueV1::Text { utf8_bytes } = result {
            let label = spec
                .labels
                .iter()
                .copied()
                .find(|label| label.as_bytes() == &bytes[..utf8_bytes as usize])
                .ok_or_else(invalid)?;
            if (records.cancelled)() {
                return Err(ValueTreeError::Cancelled);
            }
            return Ok(ArtifactConfigValueV1::Label(label));
        }
        result
    };
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    match result {
        RecordValueV1::Unit => Ok(ArtifactConfigValueV1::Unit),
        RecordValueV1::Unsigned(value) => Ok(ArtifactConfigValueV1::Unsigned(value)),
        RecordValueV1::Bool(value) => Ok(ArtifactConfigValueV1::Boolean(value)),
        RecordValueV1::Text { utf8_bytes } => Ok(ArtifactConfigValueV1::Text { utf8_bytes }),
        _ => Err(invalid()),
    }
}

pub(super) fn check_layout<M, R, E, C>(
    kind: ArtifactConfigBodyKindV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    if ctx.root.namespace != StateNamespaceV1::Artifacts {
        return Err(invalid());
    }
    let _scratch = ctx.values.memory.reserve(16 << 10, ctx.values.class)?;
    let meta = WorldFieldKeyV1::new(
        genesis::body_descriptor(),
        WorldFieldRecordKindV1::Metadata,
        None,
    )
    .map_err(ValueTreeError::Invalid)?;
    let id = ctx.lookup(&meta)?.ok_or_else(invalid)?;
    ctx.records().presence(&id, &meta, 0)?;
    let marker = ctx.records().key(
        genesis::body_descriptor(),
        WorldFieldRecordKindV1::Entry,
        &(),
    )?;
    let id = ctx.lookup(&marker.0)?.ok_or_else(invalid)?;
    let mut bytes = [0; 16];
    let observed = read_record_value_v1(
        &id,
        &marker,
        &mut ctx.records(),
        16,
        &mut Cursor::new(bytes.as_mut_slice()),
        RecordReadProfileV1 {
            codec: MemberPayloadCodecV1::Text,
            nullable: false,
            integer_max: u64::MAX,
            labels: LABELS,
        },
    )?;
    let RecordValueV1::Text { utf8_bytes } = observed else {
        return Err(invalid());
    };
    if &bytes[..utf8_bytes as usize] != kind.label().as_bytes() {
        return Err(invalid());
    }
    ctx.check()
}

/// Authenticates metadata, exact body kind and one closed column only. Text
/// output is provisional until success; enum labels are returned without text
/// copying. The caller still needs the outer owner/root binding separately.
pub fn inspect_artifact_config_cell_v1<M, R, E, C>(
    kind: ArtifactConfigBodyKindV1,
    address: ArtifactConfigAddressV1<'_>,
    max_payload_bytes: u64,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    output: &mut impl Write,
) -> Result<ReservedValue<ArtifactConfigValueV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    check_layout(kind, ctx)?;
    // The root-valued result owns a bounded digest String. Keep its admission
    // with the returned carrier rather than releasing it with operation scratch.
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let _scratch = ctx.values.memory.reserve(16 << 10, ctx.values.class)?;
    let key = key(address, &mut ctx.records())?;
    let id = ctx.lookup(&key.0)?.ok_or_else(invalid)?;
    let result = value(
        kind,
        address,
        &id,
        &key,
        max_payload_bytes,
        &mut ctx.records(),
        output,
    )?;
    ctx.check()?;
    Ok(retained.retain(result))
}

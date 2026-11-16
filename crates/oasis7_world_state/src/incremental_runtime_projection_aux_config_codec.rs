//! Narrow original config readers for the fixed projection consumer.
use super::*;
use crate::incremental_world_field_key::WorldFieldKeyV1;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_state::{MerkleNodeReader, StateRootV1},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
pub(crate) fn outer_root_v1<R: ValueNodeReader, C: FnMut() -> bool>(
    field: ArtifactConfigFieldV1,
    key: &WorldFieldKeyV1,
    id: &[u8; 32],
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<ReservedValue<StateRootV1>, ValueTreeError> {
    family::outer(field, key, id, records)
}
pub(crate) fn json_profile_v1() -> crate::incremental_json_records::JsonRecordProfileV1 {
    genesis::json_profile()
}
pub(crate) fn manifest_parts_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    root: &StateRootV1,
    parent: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut ArtifactConfigReadWorkV1,
) -> Result<(u64, ReservedValue<ArtifactConfigValueV1>), ValueTreeError> {
    if root.leaf_count != 4 {
        return Err(read::invalid());
    }
    let mut read = WorldFieldReadContextV1 {
        root,
        state: parent.state,
        values: ValueReadContext {
            source: parent.values.source,
            memory: parent.values.memory,
            class: parent.values.class,
            io: &mut *parent.values.io,
        },
        key_work: &mut *parent.key_work,
        semantic_bytes: parent.semantic_bytes,
        record_bytes: parent.record_bytes,
        take_state_error: &mut parent.take_state_error,
        cancelled: &mut parent.cancelled,
    };
    work.cell()?;
    let version = inspect_artifact_config_cell_v1(
        ArtifactConfigBodyKindV1::Manifest,
        ArtifactConfigAddressV1 {
            section: "manifest",
            ordinal: None,
            inner_ordinal: None,
            member: None,
            column: "version",
        },
        work.max_payload_bytes,
        &mut read,
        &mut std::io::sink(),
    )?;
    let ArtifactConfigValueV1::Unsigned(version) = *version else {
        return Err(read::invalid());
    };
    work.cell()?;
    let content = inspect_artifact_config_cell_v1(
        ArtifactConfigBodyKindV1::Manifest,
        ArtifactConfigAddressV1 {
            section: "manifest",
            ordinal: None,
            inner_ordinal: None,
            member: None,
            column: "content",
        },
        work.max_payload_bytes,
        &mut read,
        &mut std::io::sink(),
    )?;
    if !matches!(&*content, ArtifactConfigValueV1::JsonRoot(_)) {
        return Err(read::invalid());
    }
    Ok((version, content))
}

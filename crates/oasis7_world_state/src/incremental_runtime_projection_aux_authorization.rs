//! Actual grant paths and selected invocation rows, with original schemas.
use super::*;
use crate::{
    incremental_inventory_value_read::InventoryValueReader,
    incremental_runtime_authorization_decode::{
        AuthorizationBodyKindV1, recover_authorization_invocation_body_v1,
    },
    incremental_runtime_authorization_grants::authorization_grant_profile_v1,
};
pub(super) fn capture<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    selected: Selection<'_>,
    index: usize,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut ReadWork<'_>,
    p: &Processing<'_>,
    build: &mut Build,
) -> Result<(), ValueTreeError> {
    let parsed = WorldFieldKeyV1::from_bytes(selected.key()).map_err(ValueTreeError::Invalid)?;
    if selected.namespace()
        != oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization
    {
        return Err(bad());
    }
    if parsed.kind == WorldFieldRecordKindV1::Metadata {
        return Ok(());
    }
    if parsed.kind != WorldFieldRecordKindV1::Entry {
        return Err(bad());
    }
    *work.authorization_cells = work.authorization_cells.checked_sub(1).ok_or_else(limit)?;
    let descriptor = crate::incremental_runtime_registry::RUNTIME_TYPED_GENESIS_FIELDS_V1
        .iter()
        .copied()
        .find(|f| f.name == FIELDS[index])
        .ok_or_else(bad)?;
    let key = WorldFieldKeyV1::new(descriptor, parsed.kind, parsed.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != selected.key() {
        return Err(bad());
    }
    let schema = match index {
        7 => authorization_grant_profile_v1().schema,
        8 => AuthorizationBodyKindV1::Invocation.schema(),
        _ => return Err(bad()),
    };
    let before = selected
        .before()
        .map(|id| member::text_root(&key, &id, &schema, read, p))
        .transpose()?;
    let after = selected
        .after()
        .map(|id| member::text_root(&key, &id, &schema, read, p))
        .transpose()?;
    if let (Some(a), Some(b)) = (&before, &after)
        && a.key.as_str() != b.key.as_str()
    {
        return Err(bad());
    }
    let identity = after.as_ref().or(before.as_ref()).ok_or_else(bad)?;
    let (path, _path_fee) = owned_path(
        &[identity.key.as_str()],
        p,
        &build.memory,
        &mut read.cancelled,
    )?;
    if index == 8 {
        let Some(after) = after else {
            return build.value::<u8, _>(FIELDS[index], &path, None, p, &mut read.cancelled);
        };
        let value = recover_authorization_invocation_body_v1(
            &after.root,
            read,
            work.inner_scan,
            work.selected_heap,
        )?;
        return build.value(
            FIELDS[index],
            &path,
            Some(value.value()),
            p,
            &mut read.cancelled,
        );
    }
    let profile = authorization_grant_profile_v1();
    let mut records = InventoryValueReader {
        values: &mut read.values,
        key_work: &mut *read.key_work,
        semantic_bytes: read.semantic_bytes,
        record_bytes: read.record_bytes,
        cancelled: &mut read.cancelled,
    };
    if !selected.qualified()
        && let Some(after) = &after
    {
        if let Some(before) = &before {
            crate::incremental_json_records::advance_from_qualified_root(
                profile,
                &before.root,
                &after.root,
                &mut records,
                work.json,
            )?;
        } else {
            crate::incremental_json_records::qualify_json_subtree_records_v1(
                profile,
                &after.root,
                &mut records,
                &mut work.json.json,
            )?;
        }
    }
    let mut patches = JsonAuxBuildV1::new(&build.memory)?;
    crate::incremental_json_records::project_json_aux_paths_v1(
        profile,
        before.as_ref().map(|v| &v.root),
        after.as_ref().map(|v| &v.root),
        &mut records,
        &mut work.json.json,
        &mut work.json.paths_remaining,
        p,
        &mut patches,
    )?;
    build.push(
        FIELDS[index],
        &path,
        Payload::Deep(patches),
        p,
        &mut *records.cancelled,
    )
}

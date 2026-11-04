//! Exact-before original JSON grant updates through the shared transaction.
use super::*;
use crate::incremental_runtime_authorization_audit::json_match::exact_authorization_json_match_v1;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_json_records::{JsonWriteWorkV1, materialize_records},
    incremental_runtime_authorization_grants::prepare_authorization_grant_member_v1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::StatePageWriter,
    incremental_value::ValueReadContext,
};
pub struct AuthorizationGrantChangeV1<'a> {
    pub key: &'a str,
    pub expected: Option<&'a serde_json::Value>,
    pub after: Option<&'a serde_json::Value>,
}
pub fn change_authorization_grant_overlay_v1<M, R, E, C, W, CW>(
    parent: &ReservedValue<AuthorizationGrantsQualificationV1>,
    roots: &RootSetV1,
    change: AuthorizationGrantChangeV1<'_>,
    transaction: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    json_read: &mut JsonReadWorkV1,
    max_selected_heap: usize,
    json_write: &mut JsonWriteWorkV1,
) -> Result<bool, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
{
    transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        parent.validate_binding(roots)?;
        if parent.class != oasis7_proto::memory_budget::MemoryClass::Ordinary
            || parent.reserved_bytes() < 512
            || !parent.uses_budget(read.values.memory)
            || read.values.class != parent.class
            || !tx.uses_budget(read.values.memory)
            || !tx.uses_budget(write.memory)
            || tx.parent_fence().parent_roots != roots.digest().map_err(|_| invalid())?
            || !roots.state.iter().any(|root| root == read.root)
        {
            return Err(invalid().into());
        }
        let profile = authorization_grant_profile_v1();
        let cancel = &mut read.cancelled;
        let (key, current) = {
            let mut local = WorldFieldReadContextV1 {
                root: read.root,
                state: read.state,
                values: ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: || cancel() || (write.cancelled)(),
            };
            let metadata =
                WorldFieldKeyV1::new(profile.field, WorldFieldRecordKindV1::Metadata, None)
                    .map_err(ValueTreeError::Invalid)?;
            let (key, current) = {
                let mut records = InventoryValueReader {
                    values: &mut local.values,
                    key_work: &mut *local.key_work,
                    semantic_bytes: local.semantic_bytes,
                    record_bytes: local.record_bytes,
                    cancelled: &mut local.cancelled,
                };
                let meta = lookup(tx, &metadata, &mut records, &mut local.take_state_error)?
                    .ok_or_else(invalid)?;
                records.presence(&meta, &metadata, 0)?;
                let (key, _) =
                    records.key(profile.field, WorldFieldRecordKindV1::Entry, &change.key)?;
                let current = lookup(tx, &key, &mut records, &mut local.take_state_error)?;
                (key, current)
            };
            if let Some(value) = current {
                let mut bytes = [0; RECORD_ROOT_BYTES];
                let info = read_member_record_v1(
                    &value,
                    &key,
                    &mut local.values,
                    MemberRecordLimitsV1 {
                        semantic_bytes: local.semantic_bytes,
                        payload_bytes: RECORD_ROOT_BYTES as u64,
                        record_bytes: local.record_bytes,
                    },
                    &mut std::io::sink(),
                    &mut &mut bytes[..],
                    &mut local.cancelled,
                )?;
                if info.codec != MemberPayloadCodecV1::RecordRoot
                    || info.payload_bytes != RECORD_ROOT_BYTES as u64
                {
                    return Err(invalid().into());
                }
                let owner = WorldFieldKeyV1::from_bytes(key.as_bytes())
                    .map_err(ValueTreeError::Invalid)?
                    .semantic_key
                    .ok_or_else(invalid)?;
                let reference =
                    RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
                let child = reference
                    .validate_binding(&profile.schema, &owner, StateNamespaceV1::Authorization)
                    .map_err(|_| invalid())?;
                let proof = qualify_json_subtree_records_v1(
                    profile,
                    child,
                    &mut local.records(),
                    json_read,
                )?;
                let (value, _charge) = materialize_records(
                    profile,
                    &proof,
                    &mut local.records(),
                    json_read,
                    max_selected_heap,
                )?;
                let Some(expected) = change.expected else {
                    return Err(invalid().into());
                };
                if !exact_authorization_json_match_v1(
                    &value,
                    expected,
                    local.values.memory,
                    local.values.class,
                    json_read,
                    &mut local.cancelled,
                )? {
                    return Err(invalid().into());
                }
            } else if change.expected.is_some() {
                return Err(invalid().into());
            }
            (key, current)
        };
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let unchanged = match (change.expected, change.after) {
            (None, None) => true,
            (Some(a), Some(b)) => exact_authorization_json_match_v1(
                a,
                b,
                read.values.memory,
                read.values.class,
                json_read,
                || cancel() || (write.cancelled)(),
            )?,
            _ => false,
        };
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if unchanged {
            return Ok(false);
        }
        let after = if let Some(value) = change.after {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            let row = prepare_authorization_grant_member_v1(
                change.key,
                value,
                &mut combined,
                json_write,
            )?;
            if row.field_key != key {
                return Err(invalid().into());
            }
            Some(row.value_root)
        } else {
            None
        };
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        // Our exact-before reads legitimately advance the transaction revision.
        // Recheck the live fence against the resulting current guard.
        let ready = tx.guard();
        tx.validate_guard(&ready, live)?;
        // The read above registered the exact current overlay dependency.
        let _ = current;
        tx.write(StateNamespaceV1::Authorization, key.as_bytes(), after)?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}

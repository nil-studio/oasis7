//! Same-transaction exact-before authorization record replacement.
use super::*;
use crate::incremental_runtime_authorization_audit::json_match::exact_authorization_audit_match_v1;
use crate::{
    CapabilityAuthorizationAuditReceipt, CapabilityInvocationContext,
    incremental_field_overlay::{WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_json_records::JsonWriteWorkV1,
    incremental_runtime_authorization_genesis::{
        AuthorizationWriteWorkV1, prepare_authorization_audit_member_v1,
        prepare_authorization_invocation_member_v1,
    },
};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard,
};
pub enum AuthorizationRecordChangeV1<'a> {
    Audit {
        key: &'a str,
        expected: Option<&'a CapabilityAuthorizationAuditReceipt>,
        after: Option<&'a CapabilityAuthorizationAuditReceipt>,
    },
    Invocation {
        key: &'a str,
        expected: Option<&'a CapabilityInvocationContext>,
        after: Option<&'a CapabilityInvocationContext>,
    },
}
#[allow(clippy::too_many_arguments)]
pub fn change_authorization_record_overlay_v1<M, R, E, C, W, CW>(
    parent: &ReservedValue<AuthorizationRecordFamiliesQualificationV1>,
    roots: &RootSetV1,
    change: AuthorizationRecordChangeV1<'_>,
    transaction: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    scan: &mut StateScanBudget,
    json_read: &mut JsonReadWorkV1,
    max_selected_heap: usize,
    work: &mut AuthorizationWriteWorkV1,
    json_write: &mut JsonWriteWorkV1,
) -> Result<bool, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: oasis7_formal_protocol::incremental_state::StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
{
    transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        parent.validate_binding(roots)?;
        if parent.class != MemoryClass::Ordinary
            || parent.reserved_bytes() < 512
            || !parent.uses_budget(read.values.memory)
            || read.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(read.values.memory)
            || !tx.uses_budget(write.memory)
            || tx.parent_fence().parent_roots != roots.digest().map_err(|_| invalid())?
            || !roots.state.iter().any(|root| root == read.root)
        {
            return Err(invalid().into());
        }
        let (index, kind, key) = match &change {
            AuthorizationRecordChangeV1::Audit { key, .. } => {
                (0, AuthorizationBodyKindV1::Audit, *key)
            }
            AuthorizationRecordChangeV1::Invocation { key, .. } => {
                (1, AuthorizationBodyKindV1::Invocation, *key)
            }
        };
        let field = descriptor(index);
        // Combine both cancellation sources throughout reads, comparisons and
        // no-op handling; the producer has its original continuous write budget.
        let cancel = &mut read.cancelled;
        let current = {
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
            let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
                .map_err(ValueTreeError::Invalid)?;
            let (entry, current) = {
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
                let (entry, _) = records.key(field, WorldFieldRecordKindV1::Entry, &key)?;
                let current = lookup(tx, &entry, &mut records, &mut local.take_state_error)?;
                (entry, current)
            };
            if let Some(value) = current {
                let child = child(kind, &entry, &value, &mut local)?;
                match &change {
                    AuthorizationRecordChangeV1::Audit { expected, .. } => {
                        let value = recover_authorization_audit_body_v1(
                            &child,
                            &mut local,
                            scan,
                            json_read,
                            max_selected_heap,
                        )?;
                        let Some(expected) = expected else {
                            return Err(invalid().into());
                        };
                        if !exact_authorization_audit_match_v1(
                            value.value(),
                            expected,
                            local.values.memory,
                            local.values.class,
                            json_read,
                            &mut local.cancelled,
                        )? {
                            return Err(invalid().into());
                        }
                    }
                    AuthorizationRecordChangeV1::Invocation { expected, .. } => {
                        let value = recover_authorization_invocation_body_v1(
                            &child,
                            &mut local,
                            scan,
                            max_selected_heap,
                        )?;
                        if expected.is_none_or(|expected| value.value() != expected) {
                            return Err(invalid().into());
                        }
                    }
                }
            } else if match &change {
                AuthorizationRecordChangeV1::Audit { expected, .. } => expected.is_some(),
                AuthorizationRecordChangeV1::Invocation { expected, .. } => expected.is_some(),
            } {
                return Err(invalid().into());
            }
            (entry, current)
        };
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let unchanged = match &change {
            AuthorizationRecordChangeV1::Audit {
                expected, after, ..
            } => match (expected, after) {
                (None, None) => true,
                (Some(a), Some(b)) => exact_authorization_audit_match_v1(
                    a,
                    b,
                    read.values.memory,
                    read.values.class,
                    json_read,
                    || cancel() || (write.cancelled)(),
                )?,
                _ => false,
            },
            AuthorizationRecordChangeV1::Invocation {
                expected, after, ..
            } => expected == after,
        };
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if unchanged {
            return Ok(false);
        }
        let row = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            match change {
                AuthorizationRecordChangeV1::Audit {
                    after: Some(source),
                    ..
                } => Some(prepare_authorization_audit_member_v1(
                    key,
                    source,
                    &mut combined,
                    work,
                    json_write,
                )?),
                AuthorizationRecordChangeV1::Invocation {
                    after: Some(source),
                    ..
                } => Some(prepare_authorization_invocation_member_v1(
                    key,
                    source,
                    &mut combined,
                    work,
                    json_write,
                )?),
                _ => None,
            }
        };
        let after = match row {
            Some(row) => {
                if row.field_key != current.0 {
                    return Err(invalid().into());
                }
                Some(row.value_root)
            }
            None => None,
        };
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        // Our exact-before reads legitimately advance the transaction revision.
        // Recheck the live fence against the resulting current guard.
        let ready = tx.guard();
        tx.validate_guard(&ready, live)?;
        tx.write(StateNamespaceV1::Authorization, current.0.as_bytes(), after)?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}

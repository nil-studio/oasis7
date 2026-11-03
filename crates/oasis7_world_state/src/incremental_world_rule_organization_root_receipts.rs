//! Qualified Vec successors retain the original ordinal and authenticated length.
use super::super::receipts;
use super::super::{WorldRuleOrganizationReceiptChangeV1, WorldRuleOrganizationReceiptValueV1};
use super::*;
use crate::{
    incremental_expected_bytes::Compare,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{StatePageWriter, StateWriteV1},
};
use oasis7_proto::memory_budget::ReservedValue;
/// The read context names the qualified inner Case root. Expected length is
/// authenticated before receipt Text is decoded or compared.
pub fn read_qualified_organization_receipt_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    case: &str,
    qualification: &ReservedValue<OrganizationRecordQualificationV1>,
    ordinal: u64,
    expected_length: Option<u64>,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_payload: u64,
    output: &mut dyn std::io::Write,
) -> Result<WorldRuleOrganizationReceiptValueV1, ValueTreeError> {
    fields_read::check_request(
        &OrganizationFieldRequestV1 {
            owner: OrganizationRootKeyV1::Case(case),
            record: OrganizationRelativeRecordV1::Case,
            field: OrganizationScalarFieldV1::Id,
            qualification,
        },
        context,
    )?;
    let _scratch = context
        .values
        .memory
        .reserve(16 << 10, context.values.class)
        .map_err(ValueTreeError::Memory)?;
    let mut keys = Vec::with_capacity(4);
    let mut roots = [None; 4];
    for (index, path) in receipts::paths(ordinal).into_iter().enumerate() {
        let key = K::OrganizationCaseInner.fixed_path(path, context)?;
        roots[index] = context.lookup(&key.0)?;
        keys.push(key);
    }
    let keys: receipts::ReceiptKeys = keys
        .try_into()
        .map_err(|_| ValueTreeError::Invalid("organization receipt keys"))?;
    let value = receipts::value(
        true,
        receipts::Query {
            ordinal,
            max_payload,
            expected_len: expected_length,
        },
        (&keys, &roots),
        &mut context.values,
        context.record_bytes,
        output,
        &mut context.cancelled,
    )?;
    context.check()?;
    Ok(value)
}
pub fn change_qualified_organization_receipt_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: WorldRuleOrganizationReceiptChangeV1<'_>,
    qualification: &ReservedValue<OrganizationRecordQualificationV1>,
    max_pending_node_bytes: usize,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<OrganizationRecordQualificationV1>>, WorldFieldOverlayErrorV1> {
    use WorldRuleOrganizationReceiptChangeV1 as C;
    use WorldRuleOrganizationReceiptValueV1 as V;
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !tx.uses_budget(write.memory)
        {
            return Err(ValueTreeError::Invalid("organization receipt pool/class").into());
        }
        let (case, ordinal, expected_len, expected, after, max_payload, new_length) =
            match change {
                C::Replace {
                    case,
                    ordinal,
                    expected_len,
                    expected,
                    after,
                    max_current_payload_bytes,
                } => (
                    case,
                    ordinal,
                    expected_len,
                    expected,
                    Some(after),
                    max_current_payload_bytes,
                    None,
                ),
                C::Append {
                    case,
                    expected_len,
                    after,
                } => {
                    (
                        case,
                        expected_len,
                        expected_len,
                        "",
                        Some(after),
                        0,
                        Some(expected_len.checked_add(1).ok_or(
                            ValueTreeError::ResourceLimited("organization receipt length overflow"),
                        )?),
                    )
                }
                C::Pop {
                    case,
                    expected_len,
                    expected,
                    max_current_payload_bytes,
                } => {
                    let ordinal = expected_len
                        .checked_sub(1)
                        .ok_or(ValueTreeError::Invalid("organization receipt pop empty"))?;
                    (
                        case,
                        ordinal,
                        expected_len,
                        expected,
                        None,
                        max_current_payload_bytes,
                        Some(ordinal),
                    )
                }
            };
        let _scratch = write
            .memory
            .reserve(16 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut compare = Compare::new(expected.as_bytes());
        let (outer, child, keys, roots, current) = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            let owner = OrganizationRootKeyV1::Case(case);
            let (outer, child) = access::current(owner, qualification, tx, &mut records, error)?;
            let mut keys = Vec::with_capacity(4);
            let mut roots = [None; 4];
            for (index, path) in receipts::paths(ordinal).into_iter().enumerate() {
                let key = K::OrganizationCaseInner.overlay_path(path, &mut records)?;
                roots[index] = inner_tree::lookup(child, &key.0, &mut records)?;
                keys.push(key);
            }
            let keys: receipts::ReceiptKeys = keys
                .try_into()
                .map_err(|_| ValueTreeError::Invalid("organization receipt keys"))?;
            let current = receipts::value(
                true,
                receipts::Query {
                    ordinal,
                    max_payload,
                    expected_len: Some(expected_len),
                },
                (&keys, &roots),
                records.values,
                records.record_bytes,
                &mut compare,
                records.cancelled,
            )?;
            (outer, child, keys, roots, current)
        };
        let valid = match (change, current) {
            (C::Append { .. }, V::OutOfBounds { length }) => length == expected_len,
            (C::Replace { .. } | C::Pop { .. }, V::Value { length, .. }) => {
                length == expected_len && compare.is_equal()
            }
            _ => false,
        };
        if !valid {
            return Err(
                ValueTreeError::Invalid("organization receipt expected length/value").into(),
            );
        }
        let unchanged = matches!(change,C::Replace{after,..} if expected==after);
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if unchanged {
            return Ok(None);
        }
        let _operation = write
            .memory
            .reserve(16 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut mutations = Vec::with_capacity(2);
        for (index, path, payload) in [
            (
                3,
                RecordPathV1::Ordinal("stage_receipts", ordinal),
                after.map(MemberPayloadRefV1::Text),
            ),
            (
                2,
                RecordPathV1::Nested("stage_receipts", "length"),
                new_length.map(MemberPayloadRefV1::U64),
            ),
        ] {
            if index == 2 && new_length.is_none() {
                continue;
            }
            if cancel() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let root = if let Some(payload) = payload {
                let mut combined = MemberRecordWriteContext {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    key_work: &mut *write.key_work,
                    limits: write.limits,
                    cancelled: || cancel() || (write.cancelled)(),
                };
                let row = K::OrganizationCaseInner.write_path(path, payload, &mut combined)?;
                if row.field_key != keys[index].0 {
                    return Err(ValueTreeError::Invalid("organization receipt writer key").into());
                }
                Some(row.value_root)
            } else {
                None
            };
            if cancel() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            mutations.push(StateWriteV1 {
                key: keys[index].0.as_bytes().to_vec(),
                before: roots[index],
                after: root,
            });
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        mutations.sort_by(|a, b| a.key.cmp(&b.key));
        let delta = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            inner_tree::build(child, mutations, &mut records, max_pending_node_bytes)?
        };
        let owner = OrganizationRootKeyV1::Case(case);
        let row = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            access::store(owner, &outer, &delta, &mut combined)?
        };
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(
            StateNamespaceV1::Authorization,
            outer.as_bytes(),
            Some(row.value_root),
        )?;
        let digest = WorldFieldKeyV1::from_bytes(outer.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or(ValueTreeError::Invalid("organization receipt owner digest"))?;
        let proof = closure::from_typed(owner, digest, &delta.after, write.memory)?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(proof))
    })
}

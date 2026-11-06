//! Unqualified path edits used inside the owner's guarded ordinary transaction.
//! The owner must accept the complete batch with the typed changed receiver
//! before returning any family qualification or publishing a candidate root.
#[cfg(test)]
#[path = "incremental_typed_record_patch_tests.rs"]
mod tests;
use crate::incremental_semantic_key::CanonicalSemanticKeyRefV1;
use crate::incremental_typed_record_codec::*;
use oasis7_proto::memory_budget::MemoryClass;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("typed record path edit")
}
/// Explicit canonical keys preserve original map IDs and tuple/set identities.
/// `expected` is the actual old member value ID, independent of DTO fields.
#[allow(clippy::too_many_arguments)]
pub(crate) fn patch_typed_path_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    root: &mut TypedTreeBuilderV1,
    path: &[&[u8]],
    expected: Option<[u8; 32]>,
    after: Option<Encoded<'_>>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut TypedRecordWriteWorkV1,
) -> Result<(), MemberRecordError> {
    read.check()?;
    root.check(write.memory)?;
    if read.values.class != MemoryClass::Ordinary
        || !read.values.memory.same_pool(write.memory)
        || path.is_empty()
        || path.len() > 64
    {
        return Err(invalid().into());
    }
    work.spend(&mut write.cancelled)?;
    let _frame = write
        .memory
        .reserve(2048, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let semantic = CanonicalSemanticKeyRefV1::new(path[0]).map_err(ValueTreeError::Invalid)?;
    let (owner, key, current) = {
        let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
            values: &mut read.values,
            key_work: &mut *read.key_work,
            semantic_bytes: read.semantic_bytes,
            record_bytes: read.record_bytes,
            cancelled: &mut read.cancelled,
        };
        let key = records.key(profile.field, WorldFieldRecordKindV1::Entry, &semantic)?;
        let current =
            incremental_record_tree::lookup(root.root(), &key.0, &mut records, profile.labels)?;
        (
            WorldFieldKeyV1::from_bytes(key.0.as_bytes())
                .map_err(ValueTreeError::Invalid)?
                .semantic_key
                .ok_or_else(invalid)?,
            key.0,
            current,
        )
    };
    let after = if path.len() == 1 {
        if current != expected {
            return Err(invalid().into());
        }
        after
    } else {
        let old = current.ok_or_else(invalid)?;
        let entry = oasis7_formal_protocol::incremental_state::StateEntryV1 {
            namespace: profile.namespace()?,
            key: key.as_bytes().to_vec(),
            value: old,
        };
        let row = {
            let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
                values: &mut read.values,
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                cancelled: &mut read.cancelled,
            };
            crate::incremental_record_columns_read::read_column_row_v1(
                ColumnReadProfileV1 {
                    field: profile.field,
                    schema: profile.schema,
                    groups: &[],
                    reference_schema: profile.reference_schema,
                },
                &entry,
                &mut records,
                usize::try_from(read.record_bytes).map_err(|_| invalid())?,
            )?
        };
        let ColumnCellV1::Root(child) = &row.cell else {
            return Err(invalid().into());
        };
        let mut child = TypedTreeBuilderV1::from_root(child, write.memory)?;
        patch_typed_path_v1(
            profile,
            &mut child,
            &path[1..],
            expected,
            after,
            read,
            write,
            work,
        )?;
        if let ColumnCellV1::Root(original) = &row.cell
            && child.root() == original
        {
            return Ok(());
        }
        Some(Encoded::Tree(child))
    };
    let _root_copy = write
        .memory
        .reserve(1536, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let reference;
    let held_tree;
    let value = match after {
        None => {
            held_tree = None;
            None
        }
        Some(Encoded::Scalar(payload)) => {
            held_tree = None;
            read.check()?;
            let mut combined = || (read.cancelled)() || (write.cancelled)();
            let mut ctx = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: &mut combined,
            };
            Some(
                write_member_record_v1(
                    profile.field,
                    WorldFieldRecordKindV1::Entry,
                    &semantic,
                    payload,
                    &mut ctx,
                )?
                .value_root,
            )
        }
        Some(Encoded::Tree(child)) => {
            child.check(write.memory)?;
            let schema = match profile.reference_schema {
                None => profile.schema,
                Some(resolve) => {
                    let key = crate::incremental_record_columns_read::parse_column_key_v1(
                        path[0],
                        write.memory,
                        MemoryClass::Ordinary,
                        usize::try_from(read.record_bytes).map_err(|_| invalid())?,
                    )?;
                    resolve(&key).ok_or_else(invalid)?
                }
            };
            reference =
                RecordRootRefV1::new(schema, owner, child.root().clone()).map_err(|_| invalid())?;
            held_tree = Some(child);
            read.check()?;
            let mut combined = || (read.cancelled)() || (write.cancelled)();
            let mut ctx = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: &mut combined,
            };
            Some(
                write_member_record_v1(
                    profile.field,
                    WorldFieldRecordKindV1::Entry,
                    &semantic,
                    MemberPayloadRefV1::RecordRoot(&reference),
                    &mut ctx,
                )?
                .value_root,
            )
        }
    };
    // The child carrier stays alive until its complete reference is persisted.
    let _held_tree = held_tree;
    crate::incremental_typed_record_codec::edit(
        profile,
        root,
        &key,
        (current, value),
        read,
        write,
        work,
    )?;
    read.check()?;
    Ok(())
}

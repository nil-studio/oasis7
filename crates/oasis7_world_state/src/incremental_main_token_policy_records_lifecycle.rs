//! Creation and deletion bind complete, qualified original owner identities.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("main_token_policy lifecycle binding")
}
pub struct MainTokenPolicyRecordCreateV1<'a> {
    pub key: MainTokenPolicyKeyV1<'a>,
    pub source: MainTokenPolicyRecordRefV1<'a>,
    pub family: &'a ReservedValue<MainTokenPolicyFamilyQualificationV1>,
}
pub fn create_qualified_main_token_policy_record_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
>(
    request: MainTokenPolicyRecordCreateV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut MainTokenPolicyWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<MainTokenPolicyRecordQualificationV1>, WorldFieldOverlayErrorV1> {
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        request
            .family
            .validate_binding(&tx.parent_fence().parent_roots)?;
        if request.key.kind() == MainTokenPolicyKindV1::Config
            || request.source.kind() != request.key.kind()
            || context.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !tx.uses_budget(write.memory)
            || !request.family.uses_budget(write.memory)
        {
            return Err(invalid().into());
        }
        let outer = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            point::metadata(request.key, tx, &mut records, error)?;
            let key = read::outer_key(request.key, &mut records)?;
            if lookup(tx, &key.0, &mut records, error)?.is_some() {
                return Err(invalid().into());
            }
            key.0
        };
        let (child, row) = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            let child = genesis::body(request.key, request.source, &mut combined, work)?;
            work.cell()?;
            let row = genesis::root(request.key, child.clone(), &mut combined)?;
            (child, row)
        };
        if row.field_key != outer {
            return Err(invalid().into());
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(
            request.key.kind().namespace(),
            outer.as_bytes(),
            Some(row.value_root),
        )?;
        let owner = WorldFieldKeyV1::from_bytes(outer.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or_else(invalid)?;
        let proof = closure::mint(
            request.key.kind(),
            owner,
            child,
            write.memory,
            MemoryClass::Ordinary,
        )?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(proof)
    })
}
pub struct MainTokenPolicyRecordDeleteV1<'a> {
    pub key: MainTokenPolicyKeyV1<'a>,
    pub qualification: &'a ReservedValue<MainTokenPolicyRecordQualificationV1>,
}
/// The complete exact qualified child is the expected value. No sibling payload
/// is rescanned merely to delete its authenticated outer reference.
pub fn delete_qualified_main_token_policy_record_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: MainTokenPolicyRecordDeleteV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if request.key.kind() == MainTokenPolicyKindV1::Config {
            return Err(invalid().into());
        }
        let _workspace = context
            .values
            .memory
            .reserve(16 << 10, context.values.class)
            .map_err(ValueTreeError::Memory)?;
        let mut records = InventoryValueReader {
            values: &mut context.values,
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            cancelled: &mut context.cancelled,
        };
        let (outer, _) = point::current(
            request.key,
            request.qualification,
            tx,
            &mut records,
            &mut context.take_state_error,
        )?;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(request.key.kind().namespace(), outer.as_bytes(), None)?;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    })
}

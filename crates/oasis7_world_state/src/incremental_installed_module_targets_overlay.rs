//! Original target contents within one transaction; no installation authority.
use super::*;
use crate::{
    incremental_expected_bytes::Compare,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard,
};
use oasis7_proto::memory_budget::MemoryClass;
fn resolve<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: &str,
    transaction: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
    max_location_bytes: u64,
    output: &mut impl Write,
) -> Result<(WorldFieldKeyV1, Option<InstalledModuleTargetReadV1>), WorldFieldOverlayErrorV1> {
    let field = descriptor()?;
    let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let root = lookup(transaction, &metadata, records, error)?.ok_or(ValueTreeError::Invalid(
        "installed targets missing metadata",
    ))?;
    records.presence(&root, &metadata, 0)?;
    let (key, bytes) = records.key(field, WorldFieldRecordKindV1::Entry, key)?;
    let root = lookup(transaction, &key, records, error)?;
    let target = root
        .map(|root| {
            read_target_value(
                &root,
                &key,
                records.values,
                MemberRecordLimitsV1 {
                    semantic_bytes: records.semantic_bytes,
                    payload_bytes: max_location_bytes,
                    record_bytes: records.record_bytes,
                },
                bytes,
                output,
                records.cancelled,
            )
        })
        .transpose()?;
    Ok((key, target))
}
/// Output is provisional until whole-call success; caller admits retained bytes
/// and the original query. A read error poisons the whole shared transaction.
pub fn read_installed_module_target_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: &str,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max_location_bytes: u64,
    output: &mut impl Write,
) -> Result<Option<InstalledModuleTargetReadV1>, WorldFieldOverlayErrorV1> {
    let mut records = InventoryValueReader {
        values: &mut context.values,
        key_work: context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        cancelled: &mut context.cancelled,
    };
    let error = &mut context.take_state_error;
    context.transaction.contribute(|transaction| {
        if records.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(records.values.memory)
        {
            return Err(ValueTreeError::Invalid("installed targets overlay pool/class").into());
        }
        resolve(
            key,
            transaction,
            &mut records,
            error,
            max_location_bytes,
            output,
        )
        .map(|(_, target)| target)
    })
}
#[derive(Debug, Clone, Copy)]
pub struct InstalledModuleTargetChangeV1<'a> {
    pub key: &'a str,
    pub expected: Option<ModuleInstallTargetRefV1<'a>>,
    pub after: Option<ModuleInstallTargetRefV1<'a>>,
    pub max_current_location_bytes: u64,
}
fn expected_matches(
    expected: Option<ModuleInstallTargetRefV1<'_>>,
    current: Option<InstalledModuleTargetReadV1>,
    bytes: &Compare<'_>,
) -> bool {
    match (expected, current) {
        (None, None)
        | (
            Some(ModuleInstallTargetRefV1::SelfAgent),
            Some(InstalledModuleTargetReadV1::SelfAgent),
        ) => true,
        (
            Some(ModuleInstallTargetRefV1::LocationInfrastructure(_)),
            Some(InstalledModuleTargetReadV1::LocationInfrastructure { .. }),
        ) => bytes.is_equal(),
        _ => false,
    }
}
/// Store only the owner's final target/tombstone. Requests remain borrowed and
/// caller-admitted; this does not validate instance matching or permissions.
pub fn change_installed_module_target_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
>(
    change: InstalledModuleTargetChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let InstalledModuleTargetChangeV1 {
        key,
        expected,
        after,
        max_current_location_bytes,
    } = change;
    let cancelled = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(context.values.memory)
            || !transaction.uses_budget(write.memory)
        {
            return Err(ValueTreeError::Invalid("installed targets overlay pool/class").into());
        }
        let expected_bytes = match expected {
            Some(ModuleInstallTargetRefV1::LocationInfrastructure(s)) => s.as_bytes(),
            _ => &[],
        };
        let mut compare = Compare::new(expected_bytes);
        let (encoded, current) = {
            let mut cancel = || cancelled() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut cancel,
            };
            resolve(
                key,
                transaction,
                &mut records,
                error,
                max_current_location_bytes,
                &mut compare,
            )?
        };
        if !expected_matches(expected, current, &compare) {
            return Err(ValueTreeError::Invalid("installed target expected value").into());
        }
        if expected == after {
            if cancelled() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(false);
        }
        let root = if let Some(after) = after {
            let payload = match after {
                ModuleInstallTargetRefV1::SelfAgent => MemberPayloadRefV1::Unit,
                ModuleInstallTargetRefV1::LocationInfrastructure(s) => MemberPayloadRefV1::Text(s),
            };
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancelled() || (write.cancelled)(),
            };
            let record = write_member_record_v1(
                descriptor()?,
                WorldFieldRecordKindV1::Entry,
                key,
                payload,
                &mut combined,
            )?;
            if record.field_key != encoded {
                return Err(ValueTreeError::Invalid("installed target writer key").into());
            }
            Some(record.value_root)
        } else {
            None
        };
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        transaction.write(StateNamespaceV1::Artifacts, encoded.as_bytes(), root)?;
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}

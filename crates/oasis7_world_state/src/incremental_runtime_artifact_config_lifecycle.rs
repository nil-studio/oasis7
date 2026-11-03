use super::*;
use crate::{
    incremental_field_member::*,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};

pub struct ArtifactConfigRecordChangeV1<'a> {
    pub field: ArtifactConfigFieldV1,
    pub key: ArtifactConfigRecordKeyV1<'a>,
    pub family: &'a ReservedValue<ArtifactConfigFamilyQualificationV1>,
    /// None proves exact absence at all four registry columns (or active).
    pub expected: Option<&'a ReservedValue<ArtifactConfigBodyQualificationV1>>,
    /// Explicit full-body replacement, O(new body/JSON), without cloning it.
    /// None removes a complete Map member; mandatory globals cannot be removed.
    pub after: Option<ArtifactConfigBodyRefV1<'a>>,
}
pub struct ArtifactConfigRecordChangeResultV1 {
    pub changed: bool,
    pub qualification: Option<ReservedValue<ArtifactConfigBodyQualificationV1>>,
}
pub fn change_qualified_artifact_config_record_v1<M, R, E, C, W, CW>(
    change: ArtifactConfigRecordChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut ArtifactConfigWriteWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ArtifactConfigRecordChangeResultV1, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
{
    let cancel = &mut context.cancelled;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if !tx.uses_budget(write.memory)
            || change.field.global() && change.after.is_none()
            || change
                .after
                .as_ref()
                .is_some_and(|source| source.kind() != change.field.body_kind())
        {
            return Err(read::invalid().into());
        }
        let _operation = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let before = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            overlay::resolve_record(
                change.field,
                change.key,
                change.family,
                change.expected,
                tx,
                &mut records,
                &mut context.take_state_error,
            )?
        };
        let after = if let Some(source) = change.after {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            Some(genesis::body(source, &mut combined, work)?)
        } else {
            None
        };
        let unchanged = before.child == after;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if !unchanged {
            for (field, key) in before.outer.into_iter().flatten() {
                let id = if let Some(child) = &after {
                    let mut combined = MemberRecordWriteContext {
                        sink: &mut *write.sink,
                        memory: write.memory,
                        io: &mut *write.io,
                        key_work: &mut *write.key_work,
                        limits: write.limits,
                        cancelled: || cancel() || (write.cancelled)(),
                    };
                    let row = match change.key {
                        ArtifactConfigRecordKeyV1::Global => genesis::root_row(
                            field.descriptor(),
                            &(),
                            field.schema(),
                            child.clone(),
                            &mut combined,
                        )?,
                        ArtifactConfigRecordKeyV1::Member(member) => genesis::root_row(
                            field.descriptor(),
                            member,
                            field.schema(),
                            child.clone(),
                            &mut combined,
                        )?,
                    };
                    if row.field_key != key {
                        return Err(read::invalid().into());
                    }
                    Some(row.value_root)
                } else {
                    None
                };
                tx.write(StateNamespaceV1::Artifacts, key.as_bytes(), id)?;
                if cancel() || (write.cancelled)() {
                    return Err(ValueTreeError::Cancelled.into());
                }
            }
        }
        let qualification = after
            .as_ref()
            .map(|root| {
                closure::mint(
                    change.field.body_kind(),
                    root,
                    write.memory,
                    MemoryClass::Ordinary,
                )
            })
            .transpose()?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(ArtifactConfigRecordChangeResultV1 {
            changed: !unchanged,
            qualification,
        })
    })
}

//! Root-only bootstrap checks the two independent maps and every actual raw
//! original. It proves this logical family, not a selected/durable World head.
use super::read::{check_entry, identify, invalid};
use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_field_key::WorldFieldRecordKindV1,
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{MerkleNodeReader, StateScanBudget, StateScanContext, StateScanError},
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use std::cell::RefCell;
pub struct RuntimeArtifactContentQualificationV1 {
    roots: [u8; 32],
    class: MemoryClass,
}
impl RuntimeArtifactContentQualificationV1 {
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        if roots.digest().map_err(|_| invalid())? != self.roots {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
}
pub fn qualify_runtime_artifact_content_v1<M, R, E, C, O>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    originals: &mut O,
    work: &mut RuntimeArtifactOriginalWorkV1,
) -> Result<
    ReservedValue<RuntimeArtifactContentQualificationV1>,
    RuntimeArtifactOriginalErrorV1<O::Error>,
>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
{
    roots.validate().map_err(|_| invalid())?;
    ctx.check()?;
    if !roots.state.iter().any(|r| r == ctx.root)
        || !originals.allocation_budget().same_pool(ctx.values.memory)
        || originals.allocation_class() != ctx.values.class
    {
        return Err(invalid().into());
    }
    let retained = ctx
        .values
        .memory
        .reserve(512, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    let _scratch = ctx
        .values
        .memory
        .reserve(8192, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    let cancellation = RefCell::new(&mut ctx.cancelled);
    let mut metadata = 0u8;
    for root in &roots.state {
        let mut local = WorldFieldReadContextV1 {
            root,
            state: ctx.state,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: || (cancellation.borrow_mut())(),
        };
        let result = StateScanContext {
            source: ctx.state,
            memory: local.values.memory,
            class: local.values.class,
            io: &mut *scan,
        }
        .visit_entries(
            root,
            |entry| {
                let Some((field, kind)) = identify(entry.namespace, &entry.key)? else {
                    return Ok(());
                };
                if let Some(original) = check_entry(field, kind, entry, &mut local.records())? {
                    verify_runtime_artifact_original_v1(
                        original,
                        originals,
                        local.values.memory,
                        local.values.class,
                        work,
                        &mut local.cancelled,
                    )?;
                }
                if kind == WorldFieldRecordKindV1::Metadata {
                    let bit = match field {
                        RuntimeArtifactContentFieldV1::Registered => 1,
                        RuntimeArtifactContentFieldV1::Original => 2,
                    };
                    if metadata & bit != 0 {
                        return Err(invalid().into());
                    }
                    metadata |= bit;
                }
                Ok::<_, RuntimeArtifactOriginalErrorV1<O::Error>>(())
            },
            || (cancellation.borrow_mut())(),
        );
        let original = (local.take_state_error)();
        if (cancellation.borrow_mut())() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(error) = original {
            return Err(error.into());
        }
        match result {
            Ok(_) => (),
            Err(StateScanError::Visitor(error)) => return Err(error),
            Err(StateScanError::Memory(error)) => return Err(ValueTreeError::Memory(error).into()),
            Err(StateScanError::ResourceLimited) => {
                return Err(ValueTreeError::ResourceLimited("Runtime artifact state scan").into());
            }
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled.into()),
            Err(_) => return Err(invalid().into()),
        }
    }
    if metadata != 3 {
        return Err(invalid().into());
    }
    let digest = roots.digest().map_err(|_| invalid())?;
    if (cancellation.borrow_mut())() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(retained.retain(RuntimeArtifactContentQualificationV1 {
        roots: digest,
        class: ctx.values.class,
    }))
}

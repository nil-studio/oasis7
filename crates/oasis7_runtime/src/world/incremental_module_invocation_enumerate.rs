//! Original invocation ordering from actual current overlay identities.
use crate::world::{
    incremental_body::BodyMutationError, incremental_module_read::ModuleReadContext,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_module_instance_family::ModuleInstanceFamilyQualificationV1,
    incremental_module_instance_fields::{
        ModuleInstanceFieldV1 as Field, ModuleInstanceKeyV1, ModuleInstanceScalarRefV1 as Scalar,
    },
    incremental_runtime_artifact_config::{
        ArtifactConfigFamilyQualificationV1, ArtifactConfigFieldV1,
    },
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
#[path = "incremental_module_invocation_current_keys.rs"]
mod keys;
// Borrow the caller's continuous quotas without replenishing them.
struct CurrentKeyEnumerationBudgetV1<'a> {
    memory: &'a MemoryBudget,
    scan: &'a mut StateScanBudget,
    max_entries: usize,
    max_key_bytes: usize,
    pending_entries: &'a mut usize,
}
#[derive(Debug)]
pub enum CurrentInvocationEnumerationErrorV1 {
    Execution(ExecutionError),
    Value(ValueTreeError),
    Body(BodyMutationError),
    Overlay(WorldFieldOverlayErrorV1),
}
impl From<ExecutionError> for CurrentInvocationEnumerationErrorV1 {
    fn from(e: ExecutionError) -> Self {
        Self::Execution(e)
    }
}
impl From<ValueTreeError> for CurrentInvocationEnumerationErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
impl From<BodyMutationError> for CurrentInvocationEnumerationErrorV1 {
    fn from(e: BodyMutationError) -> Self {
        Self::Body(e)
    }
}
impl From<WorldFieldOverlayErrorV1> for CurrentInvocationEnumerationErrorV1 {
    fn from(e: WorldFieldOverlayErrorV1) -> Self {
        Self::Overlay(e)
    }
}
pub struct CurrentInvocationEnumerationWorkV1 {
    pub instance_scan: StateScanBudget,
    pub registry_scan: StateScanBudget,
    pub pending_entries: usize,
    pub comparison_bytes_remaining: u64,
    pub max_keys: usize,
    pub max_key_bytes: usize,
    pub max_text_bytes: usize,
}
pub struct AcceptedInvocationDirectoryV1 {
    parent_commit: [u8; 32],
    roots: [u8; 32],
    instances: ReservedValue<Vec<Vec<u8>>>,
    registry: ReservedValue<Vec<Vec<u8>>>,
}
#[allow(clippy::too_many_arguments)]
pub fn build_accepted_invocation_directory_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<
        oasis7_world_state::incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1,
    >,
    head: &oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1,
    instances: &ReservedValue<ModuleInstanceFamilyQualificationV1>,
    config: &ReservedValue<ArtifactConfigFamilyQualificationV1>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut CurrentInvocationEnumerationWorkV1,
    live: &ExecutionFence,
) -> Result<ReservedValue<AcceptedInvocationDirectoryV1>, CurrentInvocationEnumerationErrorV1> {
    let result = (|| {
        let _source = crate::world::incremental_world_event_counter::WorldEventCounterV1::validate_parent_source(
            parent,
            head,
            ctx.transaction,
            ctx.values.memory,
        )?;
        if !config.uses_budget(ctx.values.memory)
            || config.allocation_class() != MemoryClass::Ordinary
        {
            return Err(ValueTreeError::Invalid("invocation directory config pool").into());
        }
        config.validate_binding(&ctx.transaction.parent_fence().parent_roots)?;
        let compare = work
            .max_keys
            .checked_mul(work.max_keys)
            .and_then(|n| n.checked_mul(work.max_text_bytes.max(work.max_key_bytes)))
            .and_then(|n| n.checked_mul(8))
            .ok_or(ValueTreeError::ResourceLimited("directory comparisons"))?;
        work.comparison_bytes_remaining = work
            .comparison_bytes_remaining
            .checked_sub(compare as u64)
            .ok_or(ValueTreeError::ResourceLimited("directory comparisons"))?;
        let charge = ctx
            .values
            .memory
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut changed = false;
        for namespace in [
            StateNamespaceV1::Business,
            StateNamespaceV1::Authorization,
            StateNamespaceV1::Effects,
            StateNamespaceV1::Scheduler,
            StateNamespaceV1::Artifacts,
            StateNamespaceV1::Allocator,
        ] {
            ctx.transaction.visit_pending_writes(
                namespace,
                &mut work.pending_entries,
                || {
                    if (ctx.cancelled)() {
                        Err(ExecutionError::Aborted)
                    } else {
                        Ok(())
                    }
                },
                |_, _, _| {
                    changed = true;
                    Ok::<(), ExecutionError>(())
                },
            )?;
        }
        if changed {
            return Err(
                ValueTreeError::Invalid("accepted directory requires pristine parent").into(),
            );
        }
        let instance_keys = keys::current_keys_v1(
            ctx.transaction,
            state,
            None,
            StateNamespaceV1::Business,
            CurrentKeyEnumerationBudgetV1 {
                memory: ctx.values.memory,
                scan: &mut work.instance_scan,
                max_entries: work.max_keys,
                max_key_bytes: work.max_key_bytes,
                pending_entries: &mut work.pending_entries,
            },
            &mut ctx.cancelled,
            ModuleInstanceKeyV1::is_instance_key,
        )?;
        let registry_keys = keys::current_keys_v1(
            ctx.transaction,
            state,
            None,
            StateNamespaceV1::Artifacts,
            CurrentKeyEnumerationBudgetV1 {
                memory: ctx.values.memory,
                scan: &mut work.registry_scan,
                max_entries: work.max_keys,
                max_key_bytes: work.max_key_bytes,
                pending_entries: &mut work.pending_entries,
            },
            &mut ctx.cancelled,
            |key| {
                WorldFieldKeyV1::from_bytes(key).is_ok_and(|binding| {
                    binding.kind == WorldFieldRecordKindV1::Entry
                        && WorldFieldKeyV1::new(
                            ArtifactConfigFieldV1::RegistryActive.descriptor(),
                            WorldFieldRecordKindV1::Entry,
                            binding.semantic_key,
                        )
                        .is_ok_and(|expected| expected.as_bytes() == key)
                })
            },
        )?;
        let fence = ctx.transaction.parent_fence();
        let directory = charge.retain(AcceptedInvocationDirectoryV1 {
            parent_commit: fence.parent_commit,
            roots: fence.parent_roots,
            instances: instance_keys,
            registry: registry_keys,
        });
        let guard = ctx.transaction.guard();
        let _closed = collect_current_module_invocation_ids_v1(
            &directory, instances, config, state, ctx, work, &guard, live,
        )?;
        Ok(directory)
    })();
    if result.is_err() {
        // Readers may already have poisoned the candidate. Retain the first
        // concrete cause while making source/budget failures equally fatal.
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
/// Returned identities are work selection only; each invocation must still pass
/// the actual selected manifest/original/authorization reader before execution.
#[allow(clippy::too_many_arguments)]
pub fn collect_current_module_invocation_ids_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
    instances: &ReservedValue<ModuleInstanceFamilyQualificationV1>,
    config: &ReservedValue<ArtifactConfigFamilyQualificationV1>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut CurrentInvocationEnumerationWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<Vec<String>>, CurrentInvocationEnumerationErrorV1> {
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        let fence = ctx.transaction.parent_fence();
        if !directory.uses_budget(ctx.values.memory)
            || directory.parent_commit != fence.parent_commit
            || directory.roots != fence.parent_roots
        {
            return Err(ValueTreeError::Invalid("invocation directory binding").into());
        }
        if ctx.values.class != MemoryClass::Ordinary
            || !ctx.transaction.uses_budget(ctx.values.memory)
        {
            return Err(ValueTreeError::Invalid("invocation enumeration scope").into());
        }
        let comparisons = work
            .max_keys
            .checked_mul(work.max_keys)
            .and_then(|n| n.checked_mul(work.max_text_bytes.max(work.max_key_bytes)))
            .and_then(|n| n.checked_mul(8))
            .ok_or(ValueTreeError::ResourceLimited(
                "invocation comparison bound",
            ))?;
        work.comparison_bytes_remaining = work
            .comparison_bytes_remaining
            .checked_sub(
                u64::try_from(comparisons)
                    .map_err(|_| ValueTreeError::ResourceLimited("invocation comparison bound"))?,
            )
            .ok_or(ValueTreeError::ResourceLimited(
                "invocation comparison work",
            ))?;
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let bytes = work
            .max_keys
            .checked_mul(
                work.max_text_bytes
                    .checked_add(std::mem::size_of::<String>())
                    .ok_or(ValueTreeError::ResourceLimited("invocation identities"))?,
            )
            .and_then(|n| n.checked_mul(3))
            .ok_or(ValueTreeError::ResourceLimited("invocation identities"))?;
        let charge = ctx
            .values
            .memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let instance_keys = keys::current_keys_v1(
            ctx.transaction,
            state,
            Some(&directory.instances),
            StateNamespaceV1::Business,
            CurrentKeyEnumerationBudgetV1 {
                memory: ctx.values.memory,
                scan: &mut work.instance_scan,
                max_entries: work.max_keys,
                max_key_bytes: work.max_key_bytes,
                pending_entries: &mut work.pending_entries,
            },
            &mut ctx.cancelled,
            ModuleInstanceKeyV1::is_instance_key,
        )?;
        let mut result = Vec::with_capacity(work.max_keys);
        let mut suppressed = Vec::with_capacity(work.max_keys);
        let mut read = ModuleReadContext {
            qualification: instances,
            values: ctx.values.source,
            memory: ctx.values.memory,
            io: &mut *ctx.values.io,
            max_value_bytes: work.max_text_bytes as u64,
            cancelled: &mut ctx.cancelled,
        };
        read.check(ctx.transaction)?;
        let mut previous = None;
        for key in instance_keys.iter() {
            let (id, _) = ModuleInstanceKeyV1::from_bytes(key).map_err(ValueTreeError::Invalid)?;
            if previous == Some(id) {
                continue;
            }
            previous = Some(id);
            let instance =
                read.read_instance(id, ctx.transaction)?
                    .ok_or(ValueTreeError::Invalid(
                        "incomplete current module instance",
                    ))?;
            let Scalar::Text(module) = instance.field(Field::ModuleId) else {
                return Err(ValueTreeError::Invalid("instance module type").into());
            };
            if module.len() > work.max_text_bytes || id.len() > work.max_text_bytes {
                return Err(ValueTreeError::ResourceLimited("invocation identity text").into());
            }
            suppressed.push(module.to_owned());
            if matches!(instance.field(Field::Active), Scalar::Bool(true)) {
                result.push(id.to_owned());
            }
        }
        result.sort_unstable();
        suppressed.sort_unstable();
        suppressed.dedup();
        let registry_keys = keys::current_keys_v1(
            ctx.transaction,
            state,
            Some(&directory.registry),
            StateNamespaceV1::Artifacts,
            CurrentKeyEnumerationBudgetV1 {
                memory: ctx.values.memory,
                scan: &mut work.registry_scan,
                max_entries: work.max_keys,
                max_key_bytes: work.max_key_bytes,
                pending_entries: &mut work.pending_entries,
            },
            &mut ctx.cancelled,
            |key| {
                WorldFieldKeyV1::from_bytes(key).is_ok_and(|binding| {
                    binding.kind == WorldFieldRecordKindV1::Entry
                        && WorldFieldKeyV1::new(
                            ArtifactConfigFieldV1::RegistryActive.descriptor(),
                            WorldFieldRecordKindV1::Entry,
                            binding.semantic_key,
                        )
                        .is_ok_and(|expected| expected.as_bytes() == key)
                })
            },
        )?;
        let mut globals = Vec::with_capacity(work.max_keys);
        for key in registry_keys.iter() {
            let current_guard = ctx.transaction.guard();
            let id=oasis7_world_state::incremental_runtime_artifact_config::read_current_registry_active_id_v1(config,key,ctx,work.max_text_bytes,&current_guard,live)?;
            if suppressed.binary_search(&*id).is_err() {
                globals.push((*id).clone());
            }
        }
        globals.sort_unstable();
        if result
            .len()
            .checked_add(globals.len())
            .is_none_or(|n| n > work.max_keys)
        {
            return Err(ValueTreeError::ResourceLimited("invocation count").into());
        }
        result.extend(globals);
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(error) = (ctx.take_state_error)() {
            return Err(error.into());
        }
        ctx.transaction
            .validate_guard(&ctx.transaction.guard(), live)?;
        Ok(charge.retain(result))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}

#[path = "incremental_module_invocation_directory_freeze.rs"]
mod freeze;
pub(in crate::world) use freeze::{
    FrozenInvocationDirectoryV1, freeze_current_invocation_directory_v1,
};

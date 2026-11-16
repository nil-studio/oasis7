//! Complete Current ModuleRegistry used by the original catalogue commitment.
//! Includes inactive records and every active pointer, including orphan pointers
//! retained by Legacy. No selected-manifest approximation is used for hashing.
use super::*;
use crate::world::incremental_module_invocation::{
    AcceptedInvocationDirectoryV1, current_registry_active_keys_with_work_v1,
    current_registry_record_keys_v1,
};
use oasis7_formal_protocol::incremental_state::StateNamespaceV1;
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::incremental_runtime_artifact_config::*;
use std::{cell::RefCell, collections::BTreeMap};

pub(in crate::world) struct PaidCurrentBootstrapRegistryV1 {
    registry: oasis7_wasm_abi::ModuleRegistry,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    // Authentic pending Artifacts fingerprint permits further reads, but no
    // registry mutation, between this collection and catalogue evaluation.
    cut: [u8; 32],
    fees: Vec<MemoryReservation>,
    _layout: MemoryReservation,
}
fn artifact_cut<M: MerkleNodeReader>(
    tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
    pending: &mut usize,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<[u8; 32], BodyMutationError> {
    let common = RefCell::new(common);
    let mut hash = blake3::Hasher::new();
    hash.update(b"oasis7.bootstrap.current-registry-writes.v1\0");
    tx.visit_pending_writes::<BodyMutationError>(
        StateNamespaceV1::Artifacts,
        pending,
        || {
            common
                .borrow_mut()
                .consume_steps(1)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))
        },
        |key, before, after| {
            let bytes = key.len().checked_add(8 + 2 + 64).ok_or_else(bad)?;
            common
                .borrow_mut()
                .charge(1, bytes as u64)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            hash.update(&(key.len() as u64).to_be_bytes());
            hash.update(key);
            for value in [before, after] {
                hash.update(&[u8::from(value.is_some())]);
                if let Some(value) = value {
                    hash.update(&value);
                }
            }
            Ok(())
        },
    )?;
    common
        .borrow_mut()
        .consume_steps(0)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    Ok(*hash.finalize().as_bytes())
}
impl PaidCurrentBootstrapRegistryV1 {
    pub(in crate::world) fn value(&self) -> &oasis7_wasm_abi::ModuleRegistry {
        &self.registry
    }
    pub(in crate::world) fn validate_current<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        pending: &mut usize,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self._layout.uses_budget(common.memory())
            || self.cut != artifact_cut(tx, pending, common)?
        {
            return Err(bad().into());
        }
        Ok(())
    }
}
fn registry_address(
    section: &'static str,
    column: &'static str,
) -> ArtifactConfigAddressV1<'static> {
    ArtifactConfigAddressV1 {
        section,
        ordinal: None,
        inner_ordinal: None,
        member: None,
        column,
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn read_current_bootstrap_registry_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    sources: &ModuleLifecycleSourcesV1<'_>,
    directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PaidCurrentBootstrapRegistryV1, FactoryScheduleReadErrorV1> {
    let attempted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<_, FactoryScheduleReadErrorV1> {
            ctx.transaction
                .validate_guard(&ctx.transaction.guard(), live)
                .map_err(BodyMutationError::from)?;
            let max = module.max_selected_heap;
            let keys = with_module_lifecycle_read_io_v1(
                sources.families.roots,
                ctx,
                parent,
                factory,
                common,
                module,
                |ctx, parent, factory, module| {
                    let guard = ctx.transaction.guard();
                    current_registry_record_keys_v1(
                        sources.families.config,
                        sources.families.roots,
                        parent.state,
                        ctx,
                        &mut factory.config,
                        &mut factory.config_scan,
                        module.pending_entries,
                        max / std::mem::size_of::<Vec<u8>>(),
                        max,
                        &guard,
                        live,
                    )
                    .map_err(registry_keys_error)
                },
            )?;
            let active = with_module_lifecycle_read_io_v1(
                sources.families.roots,
                ctx,
                parent,
                factory,
                common,
                module,
                |ctx, parent, factory, module| {
                    let guard = ctx.transaction.guard();
                    current_registry_active_keys_with_work_v1(
                        directory,
                        sources.families.roots,
                        parent.state,
                        ctx,
                        &mut factory.config_scan,
                        module.pending_entries,
                        max / std::mem::size_of::<Vec<u8>>(),
                        max,
                        &guard,
                        live,
                    )
                    .map_err(registry_keys_error)
                },
            )?;
            let count = keys.len().checked_add(active.len()).ok_or_else(bad)?;
            let bytes = count
                .checked_mul(std::mem::size_of::<MemoryReservation>())
                .and_then(|n| n.checked_add(std::mem::size_of::<PaidCurrentBootstrapRegistryV1>()))
                .ok_or_else(bad)?;
            common
                .charge(1, bytes as u64)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            let layout = common
                .memory()
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let mut fees = Vec::with_capacity(count);
            if fees.capacity() > count {
                return Err(bad().into());
            }
            let mut records = BTreeMap::new();
            let mut pointers = BTreeMap::new();
            for physical in keys.iter() {
                let (id, row) = with_module_lifecycle_read_io_v1(
                    sources.families.roots,
                    ctx,
                    parent,
                    factory,
                    common,
                    module,
                    |ctx, parent, factory, module| {
                        let guard = ctx.transaction.guard();
                        let id = read_current_registry_record_id_v1(
                            sources.families.config,
                            physical,
                            ctx,
                            max,
                            &guard,
                            live,
                        )
                        .map_err(overlay)?;
                        let body = open_artifact_config_body_overlay_v1(
                            ArtifactConfigSelectedBodyRequestV1 {
                                field: ArtifactConfigFieldV1::RegistryManifest,
                                key: ArtifactConfigRecordKeyV1::Member(&id),
                                family: sources.families.config,
                                parent_roots: sources.families.roots,
                                current: None,
                            },
                            parent.state,
                            ctx,
                            &mut factory.config_scan,
                            &mut factory.config,
                        )
                        .map_err(overlay)?
                        .ok_or_else(bad)?;
                        let mut read = WorldFieldReadContextV1 {
                            root: body.root(),
                            state: parent.state,
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
                            cancelled: &mut ctx.cancelled,
                        };
                        let row = read_selected_module_record_v1(
                            &body,
                            &mut read,
                            &mut factory.config,
                            &mut factory.config_scan,
                            &SelectedModuleManifestLimitsV1 {
                                heap_bytes: module.max_selected_heap,
                                json_heap_bytes: module.max_selected_heap,
                                max_collection_items: module.max_selected_heap as u64,
                            },
                        )?;
                        Ok((id, row))
                    },
                )?;
                let heap = row
                    .reserved_bytes()
                    .checked_add(row.additional_graph_fee_bytes_v1().ok_or_else(bad)?)
                    .and_then(|n| n.checked_add(id.len() + row.registered_by().len() + 4096))
                    .ok_or_else(bad)?;
                // The true selected-output graph remains live until its clone has
                // finished; its retained codec/JSON fees bound the second graph.
                let comparisons = (usize::BITS - records.len().max(1).leading_zeros()) as usize + 1;
                let work = heap
                    .checked_mul(2)
                    .and_then(|n| n.checked_add(id.len().checked_mul(comparisons)?))
                    .ok_or_else(bad)?;
                common
                    .charge(work as u64, work as u64)
                    .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                let fee = common
                    .memory()
                    .reserve(heap, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                let value = oasis7_wasm_abi::ModuleRecord {
                    manifest: row.manifest().clone(),
                    registered_at: row.registered_at(),
                    registered_by: row.registered_by().to_owned(),
                    audit_event_id: row.audit_event_id(),
                };
                if records.insert((*id).clone(), value).is_some() {
                    return Err(bad().into());
                }
                fees.push(fee);
            }
            for physical in active.iter() {
                let (id, version) = with_module_lifecycle_read_io_v1(
                    sources.families.roots,
                    ctx,
                    parent,
                    factory,
                    common,
                    module,
                    |ctx, parent, factory, _| {
                        let guard = ctx.transaction.guard();
                        let id = read_current_registry_active_id_v1(
                            sources.families.config,
                            physical,
                            ctx,
                            max,
                            &guard,
                            live,
                        )
                        .map_err(overlay)?;
                        let body = open_artifact_config_body_overlay_v1(
                            ArtifactConfigSelectedBodyRequestV1 {
                                field: ArtifactConfigFieldV1::RegistryActive,
                                key: ArtifactConfigRecordKeyV1::Member(&id),
                                family: sources.families.config,
                                parent_roots: sources.families.roots,
                                current: None,
                            },
                            parent.state,
                            ctx,
                            &mut factory.config_scan,
                            &mut factory.config,
                        )
                        .map_err(overlay)?
                        .ok_or_else(bad)?;
                        let fee = ctx
                            .values
                            .memory
                            .reserve(
                                max.checked_add(std::mem::size_of::<String>())
                                    .ok_or_else(bad)?,
                                MemoryClass::Ordinary,
                            )
                            .map_err(ValueTreeError::Memory)?;
                        let mut out = Vec::with_capacity(max);
                        if out.capacity() > max {
                            return Err(bad().into());
                        }
                        let guard = ctx.transaction.guard();
                        let value = read_qualified_artifact_config_overlay_v1(
                            ArtifactConfigCellRequestV1 {
                                field: ArtifactConfigFieldV1::RegistryActive,
                                key: ArtifactConfigRecordKeyV1::Member(&id),
                                address: registry_address("active", "value"),
                                family: sources.families.config,
                                qualification: &body,
                                max_payload_bytes: max as u64,
                            },
                            ctx,
                            &guard,
                            live,
                            &mut out,
                        )
                        .map_err(overlay)?;
                        if !matches!(*value,ArtifactConfigValueV1::Text{utf8_bytes}if utf8_bytes==out.len()as u64)
                        {
                            return Err(bad().into());
                        }
                        Ok((id, fee.retain(String::from_utf8(out).map_err(|_| bad())?)))
                    },
                )?;
                let heap = id
                    .len()
                    .checked_add(version.len())
                    .and_then(|n| n.checked_add(4096))
                    .ok_or_else(bad)?;
                let comparisons =
                    (usize::BITS - pointers.len().max(1).leading_zeros()) as usize + 1;
                let work = heap.checked_mul(comparisons).ok_or_else(bad)?;
                common
                    .charge(work as u64, work as u64)
                    .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                let fee = common
                    .memory()
                    .reserve(heap, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                if pointers.insert((*id).clone(), (*version).clone()).is_some() {
                    return Err(bad().into());
                }
                fees.push(fee);
            }
            let cut = artifact_cut(ctx.transaction, module.pending_entries, common)?;
            common
                .consume_steps(0)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            Ok(PaidCurrentBootstrapRegistryV1 {
                registry: oasis7_wasm_abi::ModuleRegistry {
                    records,
                    active: pointers,
                },
                guard: ctx.transaction.guard(),
                core: common.work().identity(),
                memory: common.memory().clone(),
                cut,
                fees,
                _layout: layout,
            })
        },
    ));
    match attempted {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
            Err(error)
        }
        Err(panic) => {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
            std::panic::resume_unwind(panic)
        }
    }
}
fn registry_keys_error(
    error: crate::world::incremental_module_invocation::CurrentInvocationEnumerationErrorV1,
) -> FactoryScheduleReadErrorV1 {
    use crate::world::incremental_module_invocation::CurrentInvocationEnumerationErrorV1 as E;
    FactoryScheduleReadErrorV1::Body(match error {
        E::Execution(e) => e.into(),
        E::Value(e) => e.into(),
        E::Body(e) => e,
        E::Overlay(e) => overlay(e),
    })
}

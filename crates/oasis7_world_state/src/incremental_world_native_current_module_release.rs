//! Current release requests: original unsigned keys, original full typed codec.
//! The accepted directory supplies completeness, never business permission.
use super::*;
use crate::incremental_world_native_gameplay::ModuleReleaseRequestsFieldV1 as F;
use crate::state::ModuleReleaseRequestState;
use oasis7_formal_protocol::incremental_state::StatePageWriter;
use oasis7_proto::memory_budget::MemoryReservation;
use std::collections::BTreeMap;
pub struct CurrentModuleReleaseRequestsV1 {
    rows: family::release_rows::ReleaseRowsV1,
    guard: PreparedGuard,
    memory: MemoryBudget,
    _ordinary: Option<MemoryReservation>,
}
impl CurrentModuleReleaseRequestsV1 {
    pub fn visit<E: From<ValueTreeError>>(
        &self,
        cells: &mut u64,
        c: &mut impl FnMut() -> bool,
        emit: &mut impl FnMut(u64, &ModuleReleaseRequestState, &mut u64) -> Result<(), E>,
    ) -> Result<(), E> {
        self.rows.visit(cells, c, emit)
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory) || !tx.uses_budget(memory) {
            return Err(invalid().into());
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub fn read_current_module_release_request_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: u64,
    parent: &ReservedValue<WorldNativeFieldsQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldNativeSelectedReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<
    ReservedValue<CurrentFactoryNativeRecordV1<ModuleReleaseRequestState>>,
    WorldFieldOverlayErrorV1,
> {
    tx.contribute(|tx| {
        if parent.class != MemoryClass::Ordinary || !parent.uses_budget(read.values.memory) {
            return Err(invalid().into());
        }
        parent.validate_binding(roots)?;
        selected::<F, _, _, _, _, _>(
            SelectedKey::Integer(id),
            &parent.module_release_requests,
            roots,
            tx,
            read,
            scan,
            work,
            guard,
            live,
        )
    })
}
fn step(
    work: &mut WorldNativeSelectedReadWorkV1,
    read: &mut WorldFieldReadContextV1<
        '_,
        impl MerkleNodeReader,
        impl ValueNodeReader,
        impl FnMut() -> Option<ValueTreeError>,
        impl FnMut() -> bool,
    >,
    key: &[u8],
) -> Result<(), ValueTreeError> {
    read::check(read)?;
    work.json.cells_remaining =
        work.json
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "Current release directory cells",
            ))?;
    let n = (key.len() as u64 + 1)
        .checked_mul(2048)
        .ok_or_else(invalid)?;
    read.key_work.bytes_remaining =
        read.key_work
            .bytes_remaining
            .checked_sub(n)
            .ok_or(ValueTreeError::ResourceLimited(
                "Current release directory key work",
            ))?;
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub fn read_current_module_release_requests_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<WorldNativeFieldsQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldNativeSelectedReadWorkV1,
    cells: &mut u64,
    pending: &mut usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentModuleReleaseRequestsV1>, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        parent.validate_binding(roots)?;
        if parent.class != MemoryClass::Ordinary
            || !parent.uses_budget(read.values.memory)
            || !tx.uses_budget(read.values.memory)
            || tx.parent_fence().parent_roots != roots.digest()?
        {
            return Err(invalid().into());
        }
        let ns = F::profile()?.namespace()?;
        if read.root != roots.state_root(ns) {
            return Err(invalid().into());
        }
        let layout = read
            .values
            .memory
            .reserve(
                std::mem::size_of::<CurrentModuleReleaseRequestsV1>() + 4096,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut rows = parent
            .module_release_requests
            .release_directory()
            .ok_or_else(invalid)?
            .clone();
        let mut changes = BTreeMap::new();
        let cancel = std::cell::RefCell::new(&mut read.cancelled);
        for namespace in [
            StateNamespaceV1::Business,
            StateNamespaceV1::Allocator,
            StateNamespaceV1::Scheduler,
            StateNamespaceV1::Authorization,
            StateNamespaceV1::Effects,
            StateNamespaceV1::Artifacts,
        ] {
            tx.visit_pending_writes::<WorldFieldOverlayErrorV1>(
                namespace,
                pending,
                || {
                    if (cancel.borrow_mut())() {
                        Err(ValueTreeError::Cancelled.into())
                    } else {
                        Ok(())
                    }
                },
                |key, before, after| {
                    let Ok(parsed) = WorldFieldKeyV1::from_bytes(key) else {
                        return Ok(());
                    };
                    if parsed.owner != FieldOwner::WorldState || parsed.name != F::NAME {
                        return Ok(());
                    }
                    if namespace != ns
                        || parsed.namespace != ns
                        || parsed.kind != WorldFieldRecordKindV1::Entry
                    {
                        return Err(invalid().into());
                    }
                    *cells = cells.checked_sub(1).ok_or(ValueTreeError::ResourceLimited(
                        "Current release pending cells",
                    ))?;
                    read.key_work.bytes_remaining = read
                        .key_work
                        .bytes_remaining
                        .checked_sub(
                            (key.len() as u64 + 1)
                                .checked_mul(2048)
                                .ok_or_else(invalid)?,
                        )
                        .ok_or(ValueTreeError::ResourceLimited(
                            "Current release pending bytes",
                        ))?;
                    let fee = read
                        .values
                        .memory
                        .reserve(key.len() + 4096, MemoryClass::Ordinary)
                        .map_err(ValueTreeError::Memory)?;
                    if changes.insert(key.to_vec(), (before, after, fee)).is_some() {
                        return Err(invalid().into());
                    }
                    Ok(())
                },
            )?;
        }
        drop(cancel);
        for (key, (before, after, _fee)) in changes {
            if before == after {
                continue;
            }
            step(work, read, &key)?;
            if tx.read(ns, &key)? != after {
                return Err(invalid().into());
            }
            let entry = oasis7_formal_protocol::incremental_state::StateEntryV1 {
                namespace: ns,
                key,
                value: after.or(before).ok_or_else(invalid)?,
            };
            let original = family::key::original::<F>(&entry, read, work.max_tree_heap)?;
            let id = family::release_rows::original(&original)?;
            if rows.find(id, cells, &mut read.cancelled)?.map(|r| r.outer) != before {
                return Err(invalid().into());
            }
            let body = after
                .map(|value| {
                    read::read_body_at_entry_v1::<F, _, _, _, _>(
                        &family::key::canonical::<F>(&entry)?,
                        &value,
                        read,
                    )
                })
                .transpose()?;
            if let Some(body) = &body {
                F::validate_record_binding(WorldNativeKeyRefV1::Unsigned, body, read, scan, work)?;
            }
            rows.replace(id, after, body.as_deref(), read, scan, work, cells)?;
        }
        let coverage = rows.coverage(read.values.memory)?;
        let guard = tx.guard();
        tx.validate_guard(&guard, live)?;
        Ok(layout.retain(CurrentModuleReleaseRequestsV1 {
            rows,
            guard,
            memory: read.values.memory.clone(),
            _ordinary: coverage,
        }))
    })
}
#[allow(clippy::too_many_arguments)]
pub fn write_current_module_release_request_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    expected: &ReservedValue<CurrentFactoryNativeRecordV1<ModuleReleaseRequestState>>,
    id: u64,
    after: &ModuleReleaseRequestState,
    tx: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    records: &mut u64,
    work: &mut crate::incremental_typed_record_codec::TypedRecordWriteWorkV1,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_transaction_identity(&expected.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        read::check(read)?;
        if read.values.class != MemoryClass::Ordinary
            || !expected.uses_budget(read.values.memory)
            || !expected.memory.same_pool(read.values.memory)
            || !tx.uses_budget(read.values.memory)
            || !tx.uses_budget(write.memory)
            || !write.memory.same_pool(read.values.memory)
        {
            return Err(invalid().into());
        }
        let field = F::descriptor()?;
        let ns = F::profile()?.namespace()?;
        let (key, _) = read.key(field, WorldFieldRecordKindV1::Entry, &id)?;
        if key.as_bytes() != expected.key.as_bytes()
            || tx.read(ns, key.as_bytes())? != expected.value_root
        {
            return Err(invalid().into());
        }
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let mut output = None;
        prepare_world_native_record_v1::<F>(&id, after, write, records, work, &mut |row| {
            if output.is_some() {
                return Err(invalid());
            }
            output = Some(row);
            Ok(())
        })?;
        let row = output.ok_or_else(invalid)?;
        if row.namespace != ns
            || row.mutation.key != expected.key.as_bytes()
            || row.mutation.after.is_none()
        {
            return Err(invalid().into());
        }
        read::check(read)?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(ns, key.as_bytes(), row.mutation.after)?;
        read::check(read)?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.validate_guard(&tx.guard(), live)?;
        Ok(())
    })
}

use crate::incremental_world_native_gameplay::ModuleReleaseManifestMappingsFieldV1 as MappingF;
#[allow(clippy::too_many_arguments)]
pub fn read_current_module_release_manifest_mapping_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: u64,
    parent: &ReservedValue<WorldNativeFieldsQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldNativeSelectedReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<
    ReservedValue<CurrentFactoryNativeRecordV1<crate::state::ModuleReleaseManifestMappingState>>,
    WorldFieldOverlayErrorV1,
> {
    tx.contribute(|tx| {
        if parent.class != MemoryClass::Ordinary || !parent.uses_budget(read.values.memory) {
            return Err(invalid().into());
        }
        parent.validate_binding(roots)?;
        selected::<MappingF, _, _, _, _, _>(
            SelectedKey::Integer(id),
            &parent.module_release_manifest_mappings,
            roots,
            tx,
            read,
            scan,
            work,
            guard,
            live,
        )
    })
}
#[allow(clippy::too_many_arguments)]
pub fn write_current_module_release_manifest_mapping_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    expected: &ReservedValue<
        CurrentFactoryNativeRecordV1<crate::state::ModuleReleaseManifestMappingState>,
    >,
    id: u64,
    after: &crate::state::ModuleReleaseManifestMappingState,
    tx: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    records: &mut u64,
    work: &mut crate::incremental_typed_record_codec::TypedRecordWriteWorkV1,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_transaction_identity(&expected.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        read::check(read)?;
        if read.values.class != MemoryClass::Ordinary
            || !expected.uses_budget(read.values.memory)
            || !expected.memory.same_pool(read.values.memory)
            || !tx.uses_budget(read.values.memory)
            || !tx.uses_budget(write.memory)
            || !write.memory.same_pool(read.values.memory)
        {
            return Err(invalid().into());
        }
        let field = MappingF::descriptor()?;
        let ns = MappingF::profile()?.namespace()?;
        let (key, _) = read.key(field, WorldFieldRecordKindV1::Entry, &id)?;
        if key.as_bytes() != expected.key.as_bytes()
            || tx.read(ns, key.as_bytes())? != expected.value_root
        {
            return Err(invalid().into());
        }
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let mut output = None;
        prepare_world_native_record_v1::<MappingF>(&id, after, write, records, work, &mut |row| {
            if output.is_some() {
                return Err(invalid());
            }
            output = Some(row);
            Ok(())
        })?;
        let row = output.ok_or_else(invalid)?;
        if row.namespace != ns
            || row.mutation.key != expected.key.as_bytes()
            || row.mutation.after.is_none()
        {
            return Err(invalid().into());
        }
        read::check(read)?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(ns, key.as_bytes(), row.mutation.after)?;
        read::check(read)?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.validate_guard(&tx.guard(), live)?;
        Ok(())
    })
}

use crate::incremental_world_native_gameplay::ModuleAdmissionFreezesFieldV1 as FreezeF;
#[allow(clippy::too_many_arguments)]
pub fn read_current_module_admission_freeze_entry_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    parent: &ReservedValue<WorldNativeFieldsQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldNativeSelectedReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<
    ReservedValue<CurrentFactoryNativeRecordV1<crate::state::ModuleAdmissionFreeze>>,
    WorldFieldOverlayErrorV1,
> {
    tx.contribute(|tx| {
        if parent.class != MemoryClass::Ordinary || !parent.uses_budget(read.values.memory) {
            return Err(invalid().into());
        }
        parent.validate_binding(roots)?;
        selected::<FreezeF, _, _, _, _, _>(
            SelectedKey::Receipt(id),
            &parent.module_admission_freezes,
            roots,
            tx,
            read,
            scan,
            work,
            guard,
            live,
        )
    })
}
#[allow(clippy::too_many_arguments)]
pub fn insert_current_module_admission_freeze_if_absent_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    expected: &ReservedValue<CurrentFactoryNativeRecordV1<crate::state::ModuleAdmissionFreeze>>,
    id: &str,
    after: &crate::state::ModuleAdmissionFreeze,
    tx: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    records: &mut u64,
    work: &mut crate::incremental_typed_record_codec::TypedRecordWriteWorkV1,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_transaction_identity(&expected.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        read::check(read)?;
        if read.values.class != MemoryClass::Ordinary
            || !expected.uses_budget(read.values.memory)
            || !expected.memory.same_pool(read.values.memory)
            || !tx.uses_budget(read.values.memory)
            || !tx.uses_budget(write.memory)
            || !write.memory.same_pool(read.values.memory)
        {
            return Err(invalid().into());
        }
        let field = FreezeF::descriptor()?;
        let ns = FreezeF::profile()?.namespace()?;
        let (key, _) = read.key(field, WorldFieldRecordKindV1::Entry, id)?;
        if key.as_bytes() != expected.key.as_bytes()
            || tx.read(ns, key.as_bytes())? != expected.value_root
        {
            return Err(invalid().into());
        }
        read::check(read)?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if expected.value_root.is_some() {
            tx.validate_guard(&tx.guard(), live)?;
            return Ok(false);
        }
        let _key_fee = write
            .memory
            .reserve(
                id.len().checked_add(4096).ok_or_else(invalid)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        write.key_work.bytes_remaining = write
            .key_work
            .bytes_remaining
            .checked_sub(id.len() as u64)
            .ok_or(ValueTreeError::ResourceLimited("Current freeze key clone"))?;
        let owned_key = id.to_owned();
        let mut output = None;
        prepare_world_native_record_v1::<FreezeF>(
            &owned_key,
            after,
            write,
            records,
            work,
            &mut |row| {
                if output.is_some() {
                    return Err(invalid());
                }
                output = Some(row);
                Ok(())
            },
        )?;
        let row = output.ok_or_else(invalid)?;
        if row.namespace != ns
            || row.mutation.key != expected.key.as_bytes()
            || row.mutation.after.is_none()
        {
            return Err(invalid().into());
        }
        read::check(read)?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(ns, key.as_bytes(), row.mutation.after)?;
        read::check(read)?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.validate_guard(&tx.guard(), live)?;
        Ok(true)
    })
}

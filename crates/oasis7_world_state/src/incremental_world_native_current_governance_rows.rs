//! Complete current governance DATA from authenticated resident full rows.
//! Only actual pending members are read; unchanged accepted rows remain shared.
use super::*;
use crate::incremental_world_native_governance::*;
use oasis7_proto::memory_budget::MemoryReservation;
use std::collections::BTreeMap;
pub struct CurrentGovernanceRowsV1 {
    admissions: family::governance_rows::GovernanceRowsV1,
    rotations: family::governance_rows::GovernanceRowsV1,
    bootstrap: family::governance_rows::GovernanceRowsV1,
    guard: PreparedGuard,
    memory: MemoryBudget,
    _fees: [MemoryReservation; 3],
    _ordinary: [Option<MemoryReservation>; 3],
}
impl CurrentGovernanceRowsV1 {
    pub fn retained_allocation_bytes_v1(&self) -> Result<usize, ValueTreeError> {
        self.admissions
            .retained_bytes()
            .checked_add(self.rotations.retained_bytes())
            .and_then(|n| n.checked_add(self.bootstrap.retained_bytes()))
            .and_then(|n| n.checked_add(std::mem::size_of::<Self>()))
            .ok_or_else(invalid)
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
    pub fn visit_admissions<E: From<ValueTreeError>>(
        &self,
        cells: &mut u64,
        c: &mut impl FnMut() -> bool,
        emit: &mut impl FnMut(
            &str,
            &crate::governance::GovernanceValidatorAdmissionRecord,
            &mut u64,
        ) -> Result<(), E>,
    ) -> Result<(), E> {
        self.admissions
            .visit(cells, c, &mut |key, row, work| match row {
                family::governance_rows::GovernanceRecordV1::Admission(r) => {
                    emit(key, r.value().value(), work)
                }
                _ => Err(invalid().into()),
            })
    }
    pub fn visit_rotations<E: From<ValueTreeError>>(
        &self,
        cells: &mut u64,
        c: &mut impl FnMut() -> bool,
        emit: &mut impl FnMut(
            &str,
            &crate::governance::FinalityKeyRotationRecordV1,
            &mut u64,
        ) -> Result<(), E>,
    ) -> Result<(), E> {
        self.rotations
            .visit(cells, c, &mut |key, row, work| match row {
                family::governance_rows::GovernanceRecordV1::Rotation(r) => {
                    emit(key, r.value().value(), work)
                }
                _ => Err(invalid().into()),
            })
    }
    pub fn visit_bootstrap_rotations<E: From<ValueTreeError>>(
        &self,
        cells: &mut u64,
        c: &mut impl FnMut() -> bool,
        emit: &mut impl FnMut(
            &str,
            &crate::governance::BootstrapFinalityKeyRotationRecordV1,
            &mut u64,
        ) -> Result<(), E>,
    ) -> Result<(), E> {
        self.bootstrap
            .visit(cells, c, &mut |key, row, work| match row {
                family::governance_rows::GovernanceRecordV1::Bootstrap(r) => {
                    emit(key, r.value().value(), work)
                }
                _ => Err(invalid().into()),
            })
    }
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
                "Current governance directory cells",
            ))?;
    let n = (key.len() as u64 + 1)
        .checked_mul(2048)
        .ok_or_else(invalid)?;
    read.key_work.bytes_remaining =
        read.key_work
            .bytes_remaining
            .checked_sub(n)
            .ok_or(ValueTreeError::ResourceLimited(
                "Current governance directory key work",
            ))?;
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn current<
    F: WorldNativeFieldSpecV1<Key = String>,
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    q: &ReservedValue<family::WorldNativeFieldQualificationV1<F>>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldNativeSelectedReadWorkV1,
    cells: &mut u64,
    pending: &mut usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<
    (
        family::governance_rows::GovernanceRowsV1,
        MemoryReservation,
        Option<MemoryReservation>,
    ),
    WorldFieldOverlayErrorV1,
> {
    tx.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        q.validate_binding(roots)?;
        if q.allocation_class() != MemoryClass::Ordinary
            || !q.uses_budget(read.values.memory)
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
                std::mem::size_of::<CurrentGovernanceRowsV1>() + 4096,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut rows = q.governance_rows().ok_or_else(invalid)?.clone();
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
                        "Current governance pending cells",
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
                            "Current governance pending bytes",
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
            let id = family::governance_rows::original(&original)?;
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
                F::validate_record_binding(WorldNativeKeyRefV1::Text(id), body, read, scan, work)?;
            }
            rows.replace::<F, _, _, _, _>(id, after, body.as_deref(), read, scan, work, cells)?;
        }
        let coverage = rows.coverage(read.values.memory)?;
        tx.validate_guard(&tx.guard(), live)?;
        Ok((rows, layout, coverage))
    })
}

#[allow(clippy::too_many_arguments)]
pub fn read_current_governance_rows_v1<
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
) -> Result<ReservedValue<CurrentGovernanceRowsV1>, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        parent.validate_binding(roots)?;
        let guard = tx.guard();
        let (admissions, a, ao) = current::<GovernanceValidatorAdmissionsFieldV1, _, _, _, _, _>(
            &parent.governance_validator_admissions,
            roots,
            tx,
            read,
            scan,
            work,
            cells,
            pending,
            &guard,
            live,
        )?;
        let guard = tx.guard();
        let (rotations, b, bo) = current::<GovernanceFinalityKeyRotationsFieldV1, _, _, _, _, _>(
            &parent.governance_finality_key_rotations,
            roots,
            tx,
            read,
            scan,
            work,
            cells,
            pending,
            &guard,
            live,
        )?;
        let guard = tx.guard();
        let (bootstrap, c, co) =
            current::<GovernanceBootstrapFinalityKeyRotationsFieldV1, _, _, _, _, _>(
                &parent.governance_bootstrap_finality_key_rotations,
                roots,
                tx,
                read,
                scan,
                work,
                cells,
                pending,
                &guard,
                live,
            )?;
        let guard = tx.guard();
        tx.validate_guard(&guard, live)?;
        let fee = read
            .values
            .memory
            .reserve(
                std::mem::size_of::<CurrentGovernanceRowsV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        Ok(fee.retain(CurrentGovernanceRowsV1 {
            admissions,
            rotations,
            bootstrap,
            guard,
            memory: read.values.memory.clone(),
            _fees: [a, b, c],
            _ordinary: [ao, bo, co],
        }))
    })
}

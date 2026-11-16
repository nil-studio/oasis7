//! Fixed Current finality records from the resident native key directories.
use super::*;
use crate::incremental_world_native_governance::*;
use oasis7_proto::memory_budget::MemoryReservation;
use std::collections::BTreeMap;
pub struct CurrentGovernanceFinalityRecordsV1 {
    pub registry: Option<crate::governance::GovernanceFinalitySignerRegistry>,
    pub admissions: BTreeMap<String, crate::governance::GovernanceValidatorAdmissionRecord>,
    pub rotations: BTreeMap<String, crate::governance::FinalityKeyRotationRecordV1>,
    pub bootstrap_rotations:
        BTreeMap<String, crate::governance::BootstrapFinalityKeyRotationRecordV1>,
    _fees: Vec<MemoryReservation>,
    _slots: MemoryReservation,
}
fn step(
    work: &mut WorldNativeSelectedReadWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    work.json.cells_remaining = work
        .json
        .cells_remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("Current finality map work"))?;
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn map<
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
    pending: &mut usize,
    fees: &mut Vec<MemoryReservation>,
    live: &ExecutionFence,
) -> Result<BTreeMap<String, F::Value>, WorldFieldOverlayErrorV1>
where
    F::Value: Clone,
{
    q.validate_binding(roots)?;
    let directory = q.governance_directory().ok_or_else(invalid)?;
    let field = F::descriptor()?;
    let FieldDestination::State(ns) = field.destination else {
        return Err(invalid().into());
    };
    let mut keys = BTreeMap::new();
    for key in directory.iter() {
        step(work, &mut read.cancelled)?;
        let bytes = (key.len() as u64 + 1)
            .checked_mul(2048)
            .ok_or_else(invalid)?;
        read.key_work.bytes_remaining = read.key_work.bytes_remaining.checked_sub(bytes).ok_or(
            ValueTreeError::ResourceLimited("Current finality directory copy"),
        )?;
        let fee = read
            .values
            .memory
            .reserve(key.len() + 4096, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        keys.insert(key.to_vec(), fee);
    }
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
                work.json.cells_remaining = work.json.cells_remaining.checked_sub(1).ok_or(
                    ValueTreeError::ResourceLimited("Current finality pending keys"),
                )?;
                read.key_work.bytes_remaining = read
                    .key_work
                    .bytes_remaining
                    .checked_sub((key.len() as u64 + 1) * 2048)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "Current finality pending key bytes",
                    ))?;
                if keys.contains_key(key) != before.is_some() {
                    return Err(invalid().into());
                }
                match (before.is_some(), after.is_some()) {
                    (false, true) => {
                        let fee = read
                            .values
                            .memory
                            .reserve(key.len() + 4096, MemoryClass::Ordinary)
                            .map_err(ValueTreeError::Memory)?;
                        keys.insert(key.to_vec(), fee);
                    }
                    (true, false) => {
                        keys.remove(key);
                    }
                    _ => {}
                }
                Ok(())
            },
        )?;
    }
    drop(cancel);
    let mut output = BTreeMap::new();
    for (key, _key_fee) in keys {
        step(work, &mut read.cancelled)?;
        let value = tx.read(ns, &key)?.ok_or_else(invalid)?;
        let entry = oasis7_formal_protocol::incremental_state::StateEntryV1 {
            namespace: ns,
            key,
            value,
        };
        let original = family::key::original::<F>(&entry, read, work.max_tree_heap)?;
        let WorldNativeKeyRefV1::Text(original) = family::key::original_ref::<F>(&original)? else {
            return Err(invalid().into());
        };
        let guard = tx.guard();
        let record =
            selected::<F, _, _, _, _, _>(original, q, roots, tx, read, scan, work, &guard, live)?
                .ok_or_else(invalid)?;
        let bytes = record
            .retained_allocation_bytes_v1(&mut work.json.cells_remaining, &mut read.cancelled)?
            .checked_mul(2)
            .and_then(|n| n.checked_add(original.len() + 4096))
            .ok_or_else(invalid)?;
        read.key_work.bytes_remaining = read
            .key_work
            .bytes_remaining
            .checked_sub(bytes as u64)
            .ok_or(ValueTreeError::ResourceLimited(
                "Current finality record clone",
            ))?;
        let fee = read
            .values
            .memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        output.insert(original.to_owned(), record.value().value().clone());
        fees.push(fee);
    }
    Ok(output)
}
#[allow(clippy::too_many_arguments)]
pub fn recover_current_governance_finality_records_v1<
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
    pending: &mut usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentGovernanceFinalityRecordsV1>, WorldFieldOverlayErrorV1> {
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
        let extra = u64::try_from(*pending)
            .ok()
            .and_then(|n| n.checked_add(4))
            .ok_or_else(invalid)?;
        let count = parent
            .governance_validator_admissions
            .records()
            .checked_add(parent.governance_finality_key_rotations.records())
            .and_then(|n| {
                n.checked_add(parent.governance_bootstrap_finality_key_rotations.records())
            })
            .and_then(|n| n.checked_add(extra))
            .ok_or_else(invalid)?;
        let capacity = usize::try_from(count).map_err(|_| invalid())?;
        let slots = read
            .values
            .memory
            .reserve(
                capacity
                    .checked_mul(std::mem::size_of::<MemoryReservation>())
                    .ok_or_else(invalid)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let fee = read
            .values
            .memory
            .reserve(
                std::mem::size_of::<CurrentGovernanceFinalityRecordsV1>() + 8192,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut fees = Vec::with_capacity(capacity);
        let registry = current_registry(parent, roots, tx, read, scan, work, &mut fees, live)?;
        let admissions = map::<GovernanceValidatorAdmissionsFieldV1, _, _, _, _, _>(
            &parent.governance_validator_admissions,
            roots,
            tx,
            read,
            scan,
            work,
            pending,
            &mut fees,
            live,
        )?;
        let rotations = map::<GovernanceFinalityKeyRotationsFieldV1, _, _, _, _, _>(
            &parent.governance_finality_key_rotations,
            roots,
            tx,
            read,
            scan,
            work,
            pending,
            &mut fees,
            live,
        )?;
        let bootstrap_rotations =
            map::<GovernanceBootstrapFinalityKeyRotationsFieldV1, _, _, _, _, _>(
                &parent.governance_bootstrap_finality_key_rotations,
                roots,
                tx,
                read,
                scan,
                work,
                pending,
                &mut fees,
                live,
            )?;
        if fees.len() > capacity {
            return Err(invalid().into());
        }
        tx.validate_guard(&tx.guard(), live)?;
        Ok(fee.retain(CurrentGovernanceFinalityRecordsV1 {
            registry,
            admissions,
            rotations,
            bootstrap_rotations,
            _fees: fees,
            _slots: slots,
        }))
    })
}

fn current_registry<
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
    fees: &mut Vec<MemoryReservation>,
    live: &ExecutionFence,
) -> Result<Option<crate::governance::GovernanceFinalitySignerRegistry>, WorldFieldOverlayErrorV1> {
    type F = GovernanceFinalitySignerRegistryFieldV1;
    parent
        .governance_finality_signer_registry
        .validate_binding(roots)?;
    let field = F::descriptor()?;
    let profile = F::profile()?;
    let ns = profile.namespace()?;
    let meta = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let value = tx.read(ns, meta.as_bytes())?.ok_or_else(invalid)?;
    read.records().presence(&value, &meta, 0)?;
    let (key, _) = read.key(field, WorldFieldRecordKindV1::Entry, &())?;
    let value = tx.read(ns, key.as_bytes())?.ok_or_else(invalid)?;
    let body = read::read_body_at_entry_v1::<F, _, _, _, _>(&key, &value, read)?;
    let record = recover_typed_record_body_v1::<
        WorldNativeRecordV1<Option<crate::governance::GovernanceFinalitySignerRegistry>>,
        _,
        _,
        _,
        _,
    >(&body, &profile, read, scan, work)?;
    F::validate_record_binding(WorldNativeKeyRefV1::Unit, &body, read, scan, work)?;
    let bytes = record
        .retained_allocation_bytes_v1(&mut work.json.cells_remaining, &mut read.cancelled)?
        .checked_mul(2)
        .and_then(|n| n.checked_add(4096))
        .ok_or_else(invalid)?;
    read.key_work.bytes_remaining = read
        .key_work
        .bytes_remaining
        .checked_sub(bytes as u64)
        .ok_or(ValueTreeError::ResourceLimited(
            "Current finality registry clone",
        ))?;
    let fee = read
        .values
        .memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let value = record.value().value().clone();
    fees.push(fee);
    tx.validate_guard(&tx.guard(), live)?;
    Ok(value)
}

/// Actual selected registry row; reads no admissions or rotation directory.
pub fn current_governance_finality_registry_present_v1<
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
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
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
        let _slots = read
            .values
            .memory
            .reserve(
                std::mem::size_of::<MemoryReservation>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut fees = Vec::with_capacity(1);
        let registry = current_registry(parent, roots, tx, read, scan, work, &mut fees, live)?;
        Ok(registry.is_some())
    })
}

/// The complete original selected registry row, independently of ALL maps.
pub struct CurrentGovernanceRegistryV1 {
    registry: Option<crate::governance::GovernanceFinalitySignerRegistry>,
    _fees: Vec<MemoryReservation>,
    _slots: MemoryReservation,
}
impl CurrentGovernanceRegistryV1 {
    pub fn retained_allocation_bytes_v1(&self) -> Result<usize, ValueTreeError> {
        self._fees
            .iter()
            .try_fold(self._slots.bytes() + std::mem::size_of::<Self>(), |n, f| {
                n.checked_add(f.bytes()).ok_or_else(invalid)
            })
    }
    pub fn registry(&self) -> &Option<crate::governance::GovernanceFinalitySignerRegistry> {
        &self.registry
    }
}
#[allow(clippy::too_many_arguments)]
pub fn read_current_governance_registry_v1<
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
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentGovernanceRegistryV1>, WorldFieldOverlayErrorV1> {
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
        let slots = read
            .values
            .memory
            .reserve(
                std::mem::size_of::<MemoryReservation>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let fee = read
            .values
            .memory
            .reserve(
                std::mem::size_of::<CurrentGovernanceRegistryV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut fees = Vec::with_capacity(1);
        if fees.capacity() > 1 {
            return Err(invalid().into());
        }
        let registry = current_registry(parent, roots, tx, read, scan, work, &mut fees, live)?;
        if fees.len() > 1 {
            return Err(invalid().into());
        }
        tx.validate_guard(&tx.guard(), live)?;
        Ok(fee.retain(CurrentGovernanceRegistryV1 {
            registry,
            _fees: fees,
            _slots: slots,
        }))
    })
}

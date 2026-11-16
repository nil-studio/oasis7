//! Fixed actual Runtime projection sources. Every member comes from its
//! original typed owner; ordinary capture visits net writes, never a registry
//! cold scan. These outputs are DATA, not execution/World qualification.
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_json_records::{
        CognitionAuxJsonPatchV1, CognitionAuxPathStepV1, JsonAuxBuildV1, JsonChangeWorkV1,
    },
    incremental_projection::FieldOwner,
    incremental_runtime_revocation::BusinessDiffProcessingWorkV1,
    incremental_runtime_typed_complete::RuntimeTypedCompleteQualificationV1,
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
    incremental_world_rule_projection_delta::{Processing, ProcessingState, json, spend},
};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
    },
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{cell::RefCell, mem::size_of};
#[path = "incremental_runtime_projection_aux_authorization.rs"]
mod authorization;
#[path = "incremental_runtime_projection_aux_config.rs"]
mod config;
#[path = "incremental_runtime_projection_aux_controls.rs"]
mod controls;
#[path = "incremental_runtime_projection_aux_governance.rs"]
mod governance;
#[path = "incremental_runtime_projection_aux_member.rs"]
pub(crate) mod member;
#[path = "incremental_runtime_projection_aux_proposals.rs"]
mod proposals;
#[path = "incremental_runtime_projection_aux_resources.rs"]
mod resources;
pub(crate) fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("actual Runtime auxiliary projection")
}
pub(crate) fn limit() -> ValueTreeError {
    ValueTreeError::ResourceLimited("Runtime auxiliary projection")
}
const FIELDS: [&str; 15] = [
    "manifest",
    "chain_resource_manifest",
    "module_registry.records.manifest",
    "module_registry.records.registered_at",
    "module_registry.records.registered_by",
    "module_registry.records.audit_event_id",
    "module_registry.active",
    "capability_grants_v2",
    "capability_invocation_contexts",
    "governance_execution_policy",
    "governance_finality_epoch_snapshots",
    "governance_identity_penalties",
    "governance_emergency_brake_until_tick",
    "tick_consensus_authority_source",
    "proposals",
];
/// The view borrows each real caller lane once. Bounds are DATA limits, not
/// permission to invent a replacement family or refill a counter.
pub struct RuntimeProjectionAuxWorkV1<'a>{
 pub pending_entries:&'a mut usize,pub outer_scan:&'a mut StateScanBudget,pub inner_scan:&'a mut StateScanBudget,
 pub config:&'a mut crate::incremental_runtime_artifact_config::ArtifactConfigReadWorkV1,
 pub resource_scans:&'a mut [StateScanBudget;crate::incremental_runtime_artifact_resources::RUNTIME_ARTIFACT_RESOURCES_SCAN_DEPTH_V1],
 pub resource_cells:&'a mut crate::incremental_runtime_artifact_resources::RuntimeArtifactResourcesWorkV1,
 pub governance:&'a mut crate::incremental_runtime_governance::RuntimeGovernanceWorkV1,
 pub json:&'a mut JsonChangeWorkV1,pub authorization_cells:&'a mut u64,pub controls_cells:&'a mut u64,
 pub processing:&'a mut BusinessDiffProcessingWorkV1,pub selected_heap:usize,
}
pub(crate) struct ReadWork<'a>{
 pub pending_entries:&'a mut usize,pub outer_scan:&'a mut StateScanBudget,pub inner_scan:&'a mut StateScanBudget,
 pub config:&'a mut crate::incremental_runtime_artifact_config::ArtifactConfigReadWorkV1,
 pub resource_scans:&'a mut [StateScanBudget;crate::incremental_runtime_artifact_resources::RUNTIME_ARTIFACT_RESOURCES_SCAN_DEPTH_V1],
 pub resource_cells:&'a mut crate::incremental_runtime_artifact_resources::RuntimeArtifactResourcesWorkV1,
 pub governance:&'a mut crate::incremental_runtime_governance::RuntimeGovernanceWorkV1,
 pub json:&'a mut JsonChangeWorkV1,pub authorization_cells:&'a mut u64,pub controls_cells:&'a mut u64,
 pub selected_heap:usize,
}
pub enum RuntimeAuxPatchRefV1<'a> {
    Json(Option<&'a [u8]>),
    Container {
        array: bool,
        length: u64,
        reset: bool,
    },
    Deep(&'a CognitionAuxJsonPatchV1),
}
pub(crate) enum Payload {
    Json(Option<ReservedValue<Vec<u8>>>),
    Container {
        array: bool,
        length: u64,
        reset: bool,
    },
    Deep(JsonAuxBuildV1),
}
struct Row {
    field: &'static str,
    path: Vec<CognitionAuxPathStepV1>,
    payload: Payload,
    _fee: MemoryReservation,
}
pub(crate) struct Build {
    rows: Vec<Row>,
    slots: MemoryReservation,
    pub(crate) memory: MemoryBudget,
}
impl Build {
    fn new(memory: &MemoryBudget) -> Result<Self, ValueTreeError> {
        Ok(Self {
            rows: Vec::new(),
            slots: memory.reserve(0, MemoryClass::Ordinary)?,
            memory: memory.clone(),
        })
    }
    fn path<C: FnMut() -> bool>(
        &self,
        parts: &[CognitionAuxPathStepV1],
        p: &Processing<'_>,
        stop: &mut C,
    ) -> Result<(Vec<CognitionAuxPathStepV1>, MemoryReservation), ValueTreeError> {
        let n = parts.iter().try_fold(size_of::<Row>(), |n, part| {
            n.checked_add(size_of::<CognitionAuxPathStepV1>())
                .and_then(|n| {
                    n.checked_add(match part {
                        CognitionAuxPathStepV1::Name(v) => v.len(),
                        CognitionAuxPathStepV1::Index(_) => 0,
                    })
                })
                .ok_or_else(limit)
        })?;
        spend(p, n, stop)?;
        let fee = self.memory.reserve(n, MemoryClass::Ordinary)?;
        let mut path = Vec::new();
        path.try_reserve_exact(parts.len()).map_err(|_| limit())?;
        if path.capacity() > parts.len() {
            return Err(limit());
        }
        for part in parts {
            path.push(match part {
                CognitionAuxPathStepV1::Name(v) => CognitionAuxPathStepV1::Name(v.clone()),
                CognitionAuxPathStepV1::Index(v) => CognitionAuxPathStepV1::Index(*v),
            })
        }
        Ok((path, fee))
    }
    pub(crate) fn push<C: FnMut() -> bool>(
        &mut self,
        field: &'static str,
        path: &[CognitionAuxPathStepV1],
        payload: Payload,
        p: &Processing<'_>,
        stop: &mut C,
    ) -> Result<(), ValueTreeError> {
        let (path, fee) = self.path(path, p, stop)?;
        if self.rows.len() == self.rows.capacity() {
            let cap = self
                .rows
                .capacity()
                .checked_mul(2)
                .ok_or_else(limit)?
                .max(4);
            let bytes = cap.checked_mul(size_of::<Row>()).ok_or_else(limit)?;
            spend(p, bytes, stop)?;
            let mut fee = self.memory.reserve(bytes, MemoryClass::Ordinary)?;
            self.rows
                .try_reserve_exact(cap - self.rows.len())
                .map_err(|_| limit())?;
            if self.rows.capacity() > cap {
                return Err(limit());
            }
            self.slots
                .merge(&mut fee)
                .map_err(ValueTreeError::Invalid)?
        }
        self.rows.push(Row {
            field,
            path,
            payload,
            _fee: fee,
        });
        Ok(())
    }
    pub(crate) fn value<T: serde::Serialize + ?Sized, C: FnMut() -> bool>(
        &mut self,
        field: &'static str,
        path: &[CognitionAuxPathStepV1],
        value: Option<&T>,
        p: &Processing<'_>,
        stop: &mut C,
    ) -> Result<(), ValueTreeError> {
        let payload = value
            .map(|value| json(&value, &self.memory, p, stop))
            .transpose()?;
        self.push(field, path, Payload::Json(payload), p, stop)
    }
}
pub struct CurrentRuntimeProjectionAuxV1 {
    build: Build,
    guard: PreparedGuard,
    roots: [u8; 32],
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
impl CurrentRuntimeProjectionAuxV1 {
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_read_source_guard(&self.guard, live)?;
        if tx.parent_fence().parent_roots != self.roots
            || !self.memory.same_pool(memory)
            || !self._fee.uses_budget(memory)
            || !self.build.slots.uses_budget(memory)
        {
            return Err(bad().into());
        }
        Ok(())
    }
    pub fn visit_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(
            &'static str,
            &[CognitionAuxPathStepV1],
            RuntimeAuxPatchRefV1<'_>,
        ) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.build.rows {
            match &row.payload {
                Payload::Json(value) => {
                    work(1, 0)?;
                    emit(
                        row.field,
                        &row.path,
                        RuntimeAuxPatchRefV1::Json(value.as_deref().map(Vec::as_slice)),
                    )?
                }
                Payload::Container {
                    array,
                    length,
                    reset,
                } => {
                    work(1, 0)?;
                    emit(
                        row.field,
                        &row.path,
                        RuntimeAuxPatchRefV1::Container {
                            array: *array,
                            length: *length,
                            reset: *reset,
                        },
                    )?
                }
                Payload::Deep(patches) => {
                    for patch in &patches.patches {
                        work(1, 0)?;
                        emit(row.field, &row.path, RuntimeAuxPatchRefV1::Deep(patch))?
                    }
                }
            }
        }
        Ok(())
    }
}
struct Pending {
    namespace: StateNamespaceV1,
    key: Vec<u8>,
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
}
fn field(key: &[u8]) -> Result<Option<usize>, ValueTreeError> {
    if !key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(None);
    }
    let parsed = WorldFieldKeyV1::from_bytes(key).map_err(ValueTreeError::Invalid)?;
    if parsed.owner != FieldOwner::Runtime {
        return Ok(None);
    }
    let Some(index) = FIELDS.iter().position(|name| *name == parsed.name) else {
        return Ok(None);
    };
    let descriptor = crate::incremental_runtime_registry::RUNTIME_TYPED_GENESIS_FIELDS_V1
        .iter()
        .copied()
        .find(|field| field.name == parsed.name)
        .ok_or_else(bad)?;
    let original = WorldFieldKeyV1::new(descriptor, parsed.kind, parsed.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if original.as_bytes() != key {
        return Err(bad());
    }
    Ok(Some(index))
}

#[derive(Clone, Copy)]
pub(crate) enum Selection<'a> {
    Current(&'a Pending),
    Qualified(&'a oasis7_formal_protocol::incremental_state::StateEntryV1),
}
impl Selection<'_> {
    pub(crate) fn key(&self) -> &[u8] {
        match self {
            Self::Current(v) => &v.key,
            Self::Qualified(v) => &v.key,
        }
    }
    pub(crate) fn namespace(&self) -> StateNamespaceV1 {
        match self {
            Self::Current(v) => v.namespace,
            Self::Qualified(v) => v.namespace,
        }
    }
    pub(crate) fn before(&self) -> Option<[u8; 32]> {
        match self {
            Self::Current(v) => v.before,
            Self::Qualified(_) => None,
        }
    }
    pub(crate) fn after(&self) -> Option<[u8; 32]> {
        match self {
            Self::Current(v) => v.after,
            Self::Qualified(v) => Some(v.value),
        }
    }
    pub(crate) fn qualified(&self) -> bool {
        matches!(self, Self::Qualified(_))
    }
}
pub(crate) fn owned_path<C: FnMut() -> bool>(
    names: &[&str],
    p: &Processing<'_>,
    memory: &MemoryBudget,
    stop: &mut C,
) -> Result<(Vec<CognitionAuxPathStepV1>, MemoryReservation), ValueTreeError> {
    let bytes = names
        .iter()
        .try_fold(names.len() * size_of::<CognitionAuxPathStepV1>(), |n, s| {
            n.checked_add(s.len()).ok_or_else(limit)
        })?;
    spend(p, bytes, stop)?;
    let fee = memory.reserve(bytes, MemoryClass::Ordinary)?;
    let mut path = Vec::new();
    path.try_reserve_exact(names.len()).map_err(|_| limit())?;
    if path.capacity() > names.len() {
        return Err(limit());
    }
    for name in names {
        path.push(CognitionAuxPathStepV1::Name((*name).to_owned()))
    }
    Ok((path, fee))
}
impl Build {
    fn contains<C: FnMut() -> bool>(
        &self,
        field: &str,
        names: &[&str],
        p: &Processing<'_>,
        stop: &mut C,
    ) -> Result<bool, ValueTreeError> {
        for row in &self.rows {
            spend(
                p,
                field.len() + row.field.len() + names.iter().map(|s| s.len()).sum::<usize>(),
                stop,
            )?;
            if row.field == field
                && row.path.len() == names.len()
                && row.path.iter().zip(names).all(
                    |(left, right)| matches!(left,CognitionAuxPathStepV1::Name(left)if left==right),
                )
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
struct PaidValues<'a, 'p, R> {
    source: &'a R,
    p: &'a Processing<'p>,
}
impl<R: ValueNodeReader> ValueNodeReader for PaidValues<'_, '_, R> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<std::sync::Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        spend(self.p, 0, &mut || false)?;
        let value = self.source.read_node(id, max, memory, class)?;
        spend(
            self.p,
            value
                .len()
                .checked_mul(16)
                .and_then(|n| n.checked_add(128))
                .ok_or_else(limit)?,
            &mut || false,
        )?;
        Ok(value)
    }
}

fn paid_field(
    key: &[u8],
    p: &Processing<'_>,
    stop: &mut impl FnMut() -> bool,
) -> Result<Option<usize>, ValueTreeError> {
    spend(p, key.len().checked_mul(32).ok_or_else(limit)?, stop)?;
    field(key)
}
#[derive(Clone, Copy)]
enum Scope {
    Current,
    Qualified,
}
struct PendingRows {
    rows: Vec<Pending>,
    slots: MemoryReservation,
    keys: Vec<MemoryReservation>,
}
impl PendingRows {
    fn new(memory: &MemoryBudget) -> Result<Self, ValueTreeError> {
        Ok(Self {
            rows: Vec::new(),
            slots: memory.reserve(0, MemoryClass::Ordinary)?,
            keys: Vec::new(),
        })
    }
    fn push<C: FnMut() -> bool>(
        &mut self,
        namespace: StateNamespaceV1,
        key: &[u8],
        before: Option<[u8; 32]>,
        after: Option<[u8; 32]>,
        memory: &MemoryBudget,
        p: &Processing<'_>,
        stop: &mut C,
    ) -> Result<(), ValueTreeError> {
        if self.rows.len() == self.rows.capacity() {
            let capacity = self
                .rows
                .capacity()
                .checked_mul(2)
                .ok_or_else(limit)?
                .max(4);
            let bytes = capacity
                .checked_mul(size_of::<Pending>() + size_of::<MemoryReservation>())
                .ok_or_else(limit)?;
            spend(p, bytes, stop)?;
            let mut fee = memory.reserve(bytes, MemoryClass::Ordinary)?;
            self.rows
                .try_reserve_exact(capacity - self.rows.len())
                .map_err(|_| limit())?;
            self.keys
                .try_reserve_exact(capacity - self.keys.len())
                .map_err(|_| limit())?;
            if self.rows.capacity() > capacity || self.keys.capacity() > capacity {
                return Err(limit());
            }
            self.slots
                .merge(&mut fee)
                .map_err(ValueTreeError::Invalid)?;
        }
        spend(p, key.len(), stop)?;
        let fee = memory.reserve(key.len(), MemoryClass::Ordinary)?;
        let copy = key.to_vec();
        self.rows.push(Pending {
            namespace,
            key: copy,
            before,
            after,
        });
        self.keys.push(fee);
        Ok(())
    }
}
fn counts(parent: &RuntimeTypedCompleteQualificationV1) -> [u64; 8] {
    let g = parent.governance().record_counts();
    [
        parent.config().registry_records(),
        parent.config().registry_active_count(),
        parent.grants().entries(),
        parent.authorization_records().invocation_contexts(),
        g[0],
        g[1],
        g[2],
        parent.proposals().proposals(),
    ]
}
fn count_index(index: usize) -> Option<usize> {
    match index {
        2 => Some(0),
        6 => Some(1),
        7 => Some(2),
        8 => Some(3),
        9 => Some(4),
        10 => Some(5),
        11 => Some(6),
        14 => Some(7),
        _ => None,
    }
}
fn adjust_count(count: &mut u64, before: bool, after: bool) -> Result<(), ValueTreeError> {
    *count = match (before, after) {
        (false, true) => count.checked_add(1),
        (true, false) => count.checked_sub(1),
        _ => Some(*count),
    }
    .ok_or_else(bad)?;
    Ok(())
}
fn headers<C: FnMut() -> bool>(
    counts: [u64; 8],
    dirty: &[bool; 15],
    all: bool,
    build: &mut Build,
    p: &Processing<'_>,
    stop: &mut C,
) -> Result<(), ValueTreeError> {
    if all || dirty[0] {
        build.push(
            "manifest",
            &[],
            Payload::Container {
                array: false,
                length: 2,
                reset: all,
            },
            p,
            stop,
        )?;
    }
    if all || dirty[2..=6].iter().any(|v| *v) {
        build.push(
            "module_registry",
            &[],
            Payload::Container {
                array: false,
                length: 2,
                reset: all,
            },
            p,
            stop,
        )?;
        for (i, name) in ["records", "active"].into_iter().enumerate() {
            let (path, _fee) = owned_path(&[name], p, &build.memory, stop)?;
            build.push(
                "module_registry",
                &path,
                Payload::Container {
                    array: false,
                    length: counts[i],
                    reset: all,
                },
                p,
                stop,
            )?;
        }
    }
    for (index, count) in [
        (7, counts[2]),
        (8, counts[3]),
        (10, counts[5]),
        (11, counts[6]),
    ] {
        if all || dirty[index] {
            build.push(
                FIELDS[index],
                &[],
                Payload::Container {
                    array: false,
                    length: count,
                    reset: all,
                },
                p,
                stop,
            )?;
        }
    }
    if all || dirty[14] {
        build.push(
            "proposal_statuses",
            &[],
            Payload::Container {
                array: false,
                length: counts[7],
                reset: all,
            },
            p,
            stop,
        )?;
    }
    Ok(())
}
fn dispatch<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    selected: Selection<'_>,
    index: usize,
    parent: &ReservedValue<RuntimeTypedCompleteQualificationV1>,
    roots: &RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, V, E, C>,
    work: &mut ReadWork<'_>,
    p: &Processing<'_>,
    build: &mut Build,
) -> Result<(), WorldFieldOverlayErrorV1> {
    match index {
        0 | 2..=6 => config::capture(selected, index, parent, roots, read, ctx, work, p, build),
        1 => resources::capture(selected, read, work, p, build).map_err(Into::into),
        7 | 8 => authorization::capture(selected, index, read, work, p, build).map_err(Into::into),
        9..=11 => governance::capture(selected, index, read, work, p, build).map_err(Into::into),
        12 | 13 => controls::capture(selected, index, ctx, work, p, build),
        14 => proposals::capture(selected, read, work, p, build).map_err(Into::into),
        _ => Err(bad().into()),
    }
}
fn sort(
    build: &mut Build,
    p: &Processing<'_>,
    stop: &mut impl FnMut() -> bool,
) -> Result<(), ValueTreeError> {
    fn compare(a: &Row, b: &Row) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        let field = a.field.cmp(b.field);
        if field != Ordering::Equal {
            return field;
        }
        for (a, b) in a.path.iter().zip(&b.path) {
            let order = match (a, b) {
                (CognitionAuxPathStepV1::Name(a), CognitionAuxPathStepV1::Name(b)) => a.cmp(b),
                (CognitionAuxPathStepV1::Index(a), CognitionAuxPathStepV1::Index(b)) => a.cmp(b),
                (CognitionAuxPathStepV1::Name(_), _) => Ordering::Less,
                _ => Ordering::Greater,
            };
            if order != Ordering::Equal {
                return order;
            }
        }
        a.path.len().cmp(&b.path.len())
    }
    for i in 1..build.rows.len() {
        let mut at = i;
        while at > 0 {
            let n = build.rows[at]
                .path
                .iter()
                .chain(&build.rows[at - 1].path)
                .try_fold(64usize, |n, v| {
                    n.checked_add(match v {
                        CognitionAuxPathStepV1::Name(v) => v.len(),
                        CognitionAuxPathStepV1::Index(_) => 8,
                    })
                    .ok_or_else(limit)
                })?;
            spend(p, n, stop)?;
            if compare(&build.rows[at - 1], &build.rows[at]) != std::cmp::Ordering::Greater {
                break;
            }
            build.rows.swap(at - 1, at);
            at -= 1;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn capture<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<RuntimeTypedCompleteQualificationV1>,
    roots: &RootSetV1,
    parent_read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, V, E, C>,
    work: &mut RuntimeProjectionAuxWorkV1<'_>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    scope: Scope,
) -> Result<CurrentRuntimeProjectionAuxV1, WorldFieldOverlayErrorV1> {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        capture_inner(parent, roots, parent_read, ctx, work, guard, live, scope)
    }));
    match result {
        Ok(result) => {
            if result.is_err() {
                let _: Result<(), ExecutionError> =
                    ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            }
            result
        }
        Err(panic) => {
            let _: Result<(), ExecutionError> =
                ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            std::panic::resume_unwind(panic)
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn capture_inner<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<RuntimeTypedCompleteQualificationV1>,
    roots: &RootSetV1,
    parent_read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, V, E, C>,
    work: &mut RuntimeProjectionAuxWorkV1<'_>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    scope: Scope,
) -> Result<CurrentRuntimeProjectionAuxV1, WorldFieldOverlayErrorV1> {
    ctx.transaction.validate_read_source_guard(guard, live)?;
    if let Scope::Qualified = scope {
        ctx.transaction.validate_bootstrap_read_only_v1(live)?;
    }
    let memory = ctx.values.memory.clone();
    let RuntimeProjectionAuxWorkV1 {
        pending_entries,
        outer_scan,
        inner_scan,
        config,
        resource_scans,
        resource_cells,
        governance,
        json,
        authorization_cells,
        controls_cells,
        processing,
        selected_heap,
    } = work;
    let p = Processing {
        bank: RefCell::new(&mut **processing),
    };
    let mut work = ReadWork {
        pending_entries: &mut **pending_entries,
        outer_scan: &mut **outer_scan,
        inner_scan: &mut **inner_scan,
        config: &mut **config,
        resource_scans: &mut **resource_scans,
        resource_cells: &mut **resource_cells,
        governance: &mut **governance,
        json: &mut **json,
        authorization_cells: &mut **authorization_cells,
        controls_cells: &mut **controls_cells,
        selected_heap: *selected_heap,
    };
    let root_bytes = roots
        .state
        .iter()
        .try_fold(roots.world_id.len() + 4096, |n, v| {
            n.checked_add(v.node_root.len() + 128).ok_or_else(limit)
        })?;
    spend(
        &p,
        root_bytes.checked_mul(16).ok_or_else(limit)?,
        &mut ctx.cancelled,
    )?;
    parent.validate_binding(roots)?;
    let digest = roots.digest().map_err(|_| bad())?;
    if !parent.uses_budget(&memory)
        || parent.allocation_class() != MemoryClass::Ordinary
        || ctx.values.class != MemoryClass::Ordinary
        || parent_read.values.class != MemoryClass::Ordinary
        || !parent_read.values.memory.same_pool(&memory)
        || !ctx.transaction.uses_budget(&memory)
        || ctx.transaction.parent_fence().parent_roots != digest
    {
        return Err(bad().into());
    }
    let outer_bytes = size_of::<CurrentRuntimeProjectionAuxV1>() + 4096 + 5 * 8192;
    spend(&p, outer_bytes, &mut ctx.cancelled)?;
    let fee = memory
        .reserve(outer_bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut pending = PendingRows::new(&memory)?;
    let mut dirty = [false; 15];
    let mut after_counts = counts(parent);
    if let Scope::Current = scope {
        let stop = RefCell::new(&mut ctx.cancelled);
        for root in &roots.state {
            ctx.transaction.visit_pending_writes(
                root.namespace,
                work.pending_entries,
                || {
                    spend(&p, 0, &mut || (stop.borrow_mut())())
                        .map_err(WorldFieldOverlayErrorV1::from)
                },
                |key, before, after| {
                    spend(&p, key.len(), &mut || (stop.borrow_mut())())?;
                    if before == after {
                        return Ok(());
                    }
                    let Some(index) = paid_field(key, &p, &mut || (stop.borrow_mut())())? else {
                        return Ok(());
                    };
                    let parsed =
                        WorldFieldKeyV1::from_bytes(key).map_err(ValueTreeError::Invalid)?;
                    if parsed.namespace != root.namespace {
                        return Err(bad().into());
                    }
                    dirty[index] = true;
                    if parsed.kind == WorldFieldRecordKindV1::Entry {
                        if let Some(count) = count_index(index) {
                            adjust_count(
                                &mut after_counts[count],
                                before.is_some(),
                                after.is_some(),
                            )?;
                        }
                    }
                    pending.push(root.namespace, key, before, after, &memory, &p, &mut || {
                        (stop.borrow_mut())()
                    })?;
                    Ok(())
                },
            )?;
        }
    }
    let values = PaidValues {
        source: ctx.values.source,
        p: &p,
    };
    let parent_values = PaidValues {
        source: parent_read.values.source,
        p: &p,
    };
    let state = ProcessingState {
        source: parent_read.state,
        processing: &p,
        first: RefCell::new(None),
    };
    let mut read_error = || {
        state
            .first
            .borrow_mut()
            .take()
            .or_else(&mut parent_read.take_state_error)
    };
    let mut read_stop = || (parent_read.cancelled)();
    let mut current_stop = || (ctx.cancelled)();
    let mut current_error = || (ctx.take_state_error)();
    let mut read = WorldFieldReadContextV1 {
        root: parent_read.root,
        state: &state,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: &parent_values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut *parent_read.values.io,
        },
        key_work: &mut *parent_read.key_work,
        semantic_bytes: parent_read.semantic_bytes,
        record_bytes: parent_read.record_bytes,
        take_state_error: &mut read_error as &mut dyn FnMut() -> Option<ValueTreeError>,
        cancelled: &mut read_stop as &mut dyn FnMut() -> bool,
    };
    let mut current = WorldFieldOverlayContextV1 {
        transaction: &mut *ctx.transaction,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut current_error as &mut dyn FnMut() -> Option<ValueTreeError>,
        cancelled: &mut current_stop as &mut dyn FnMut() -> bool,
    };
    let mut build = Build::new(&memory)?;
    if let Scope::Qualified = scope {
        // Only explicit GEN/cold scope may enumerate the authenticated full parent.
        // Keep actual scan rows as StateEntry, never fabricate pending mutations.
        let mut entries = Vec::new();
        let mut slots = memory
            .reserve(0, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut fees = Vec::new();
        let mut observed = [0u64; 8];
        for root in &roots.state {
            let scan_stop = RefCell::new(&mut current.cancelled);
            let result = oasis7_formal_protocol::incremental_state::StateScanContext {
                source: &state,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut *work.outer_scan,
            }
            .visit_entries(
                root,
                |entry| {
                    spend(&p, entry.key.len(), &mut || (scan_stop.borrow_mut())())?;
                    let Some(index) =
                        paid_field(&entry.key, &p, &mut || (scan_stop.borrow_mut())())?
                    else {
                        return Ok(());
                    };
                    let parsed =
                        WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                    if parsed.namespace != entry.namespace || entry.namespace != root.namespace {
                        return Err(bad());
                    }
                    dirty[index] = true;
                    if parsed.kind == WorldFieldRecordKindV1::Entry {
                        if let Some(i) = count_index(index) {
                            observed[i] = observed[i].checked_add(1).ok_or_else(limit)?;
                        }
                    }
                    if entries.len() == entries.capacity() {
                        let cap = entries.capacity().checked_mul(2).ok_or_else(limit)?.max(4);
                        let n = cap
                            .checked_mul(
                                size_of::<oasis7_formal_protocol::incremental_state::StateEntryV1>(
                                ) + size_of::<MemoryReservation>(),
                            )
                            .ok_or_else(limit)?;
                        spend(&p, n, &mut || (scan_stop.borrow_mut())())?;
                        let mut fee = memory.reserve(n, MemoryClass::Ordinary)?;
                        entries
                            .try_reserve_exact(cap - entries.len())
                            .map_err(|_| limit())?;
                        fees.try_reserve_exact(cap - fees.len())
                            .map_err(|_| limit())?;
                        if entries.capacity() > cap || fees.capacity() > cap {
                            return Err(limit());
                        }
                        slots.merge(&mut fee).map_err(ValueTreeError::Invalid)?;
                    }
                    spend(&p, entry.key.len(), &mut || (scan_stop.borrow_mut())())?;
                    let fee = memory.reserve(entry.key.len(), MemoryClass::Ordinary)?;
                    entries.push(entry.clone());
                    fees.push(fee);
                    Ok(())
                },
                || (scan_stop.borrow_mut())(),
            );
            drop(scan_stop);
            if let Some(error) = state.first.borrow_mut().take() {
                return Err(error.into());
            }
            if let Some(error) = (read.take_state_error)() {
                return Err(error.into());
            }
            use oasis7_formal_protocol::incremental_state::StateScanError;
            match result {
                Ok(_) => {}
                Err(StateScanError::Memory(e)) => return Err(ValueTreeError::Memory(e).into()),
                Err(StateScanError::Visitor(e)) => return Err(e.into()),
                Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled.into()),
                Err(StateScanError::ResourceLimited) => return Err(limit().into()),
                _ => return Err(bad().into()),
            }
        }
        if observed != after_counts {
            return Err(bad().into());
        }
        headers(
            after_counts,
            &dirty,
            true,
            &mut build,
            &p,
            &mut current.cancelled,
        )?;
        for entry in &entries {
            let index = paid_field(&entry.key, &p, &mut current.cancelled)?.ok_or_else(bad)?;
            dispatch(
                Selection::Qualified(entry),
                index,
                parent,
                roots,
                &mut read,
                &mut current,
                &mut work,
                &p,
                &mut build,
            )?;
        }
    } else {
        headers(
            after_counts,
            &dirty,
            false,
            &mut build,
            &p,
            &mut current.cancelled,
        )?;
        for entry in &pending.rows {
            let index = paid_field(&entry.key, &p, &mut current.cancelled)?.ok_or_else(bad)?;
            dispatch(
                Selection::Current(entry),
                index,
                parent,
                roots,
                &mut read,
                &mut current,
                &mut work,
                &p,
                &mut build,
            )?;
        }
    }
    sort(&mut build, &p, &mut current.cancelled)?;
    if let Some(error) = (read.take_state_error)() {
        return Err(error.into());
    }
    if let Some(error) = (current.take_state_error)() {
        return Err(error.into());
    }
    current
        .transaction
        .validate_read_source_guard(guard, live)?;
    if let Scope::Qualified = scope {
        current.transaction.validate_bootstrap_read_only_v1(live)?;
    }
    let final_guard = current.transaction.guard();
    Ok(CurrentRuntimeProjectionAuxV1 {
        build,
        guard: final_guard,
        roots: digest,
        memory,
        _fee: fee,
    })
}
#[allow(clippy::too_many_arguments)]
pub fn capture_current_runtime_projection_aux_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<RuntimeTypedCompleteQualificationV1>,
    roots: &RootSetV1,
    parent_read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, V, E, C>,
    work: &mut RuntimeProjectionAuxWorkV1<'_>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<CurrentRuntimeProjectionAuxV1, WorldFieldOverlayErrorV1> {
    capture(
        parent,
        roots,
        parent_read,
        ctx,
        work,
        guard,
        live,
        Scope::Current,
    )
}
#[allow(clippy::too_many_arguments)]
pub fn capture_qualified_runtime_projection_aux_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<RuntimeTypedCompleteQualificationV1>,
    roots: &RootSetV1,
    parent_read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, V, E, C>,
    work: &mut RuntimeProjectionAuxWorkV1<'_>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<CurrentRuntimeProjectionAuxV1, WorldFieldOverlayErrorV1> {
    capture(
        parent,
        roots,
        parent_read,
        ctx,
        work,
        guard,
        live,
        Scope::Qualified,
    )
}

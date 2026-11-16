//! Actual dirty Current WorldRules columns to original typed JSON patches.
//! The six fixed composite schemas retain their sole Codec/changed validators;
//! no branch root is interpreted as generic CBOR or substituted for JSON.
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_runtime_revocation::BusinessDiffProcessingWorkV1,
    incremental_typed_record_codec::*,
    incremental_typed_record_diff::{self as diff, ChangeDecoder, TypedCellRefV1},
    incremental_world_rule_composite::WorldRuleCompositeBranchV1 as Branch,
    incremental_world_rule_composite_family::{
        CompositeBranchCodecV1, WorldRuleCompositeQualificationV1,
    },
    incremental_world_rule_composite_overlay::{
        WorldRuleCompositeOverlayV1, bind_current_world_rule_branch_v1,
    },
    incremental_world_rules_family::WorldRulesQualificationV1,
};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{
    collections::BTreeMap,
    io::{self, Write},
    mem::size_of,
};
#[path = "incremental_world_rule_projection_delta_native.rs"]
mod native;
pub use native::{
    CurrentNativeProjectionDeltaV1, is_native_projection_field_v1,
    read_current_native_projection_delta_v1,
};
#[path = "incremental_world_rule_projection_delta_seed.rs"]
mod seed;
pub use seed::{
    QualifiedWorldRuleLogicalRowsVisitorV1, WorldRuleLogicalColumnV1,
    WorldRuleLogicalContainerKindV1, WorldRuleLogicalRowsVisitorV1, WorldRuleLogicalTopSlotV1,
    visit_original_world_rule_logical_rows_v1,
};
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Current WorldRules typed projection delta")
}
fn limit() -> ValueTreeError {
    ValueTreeError::ResourceLimited("Current WorldRules projection work")
}
pub(crate) struct Processing<'a> {
    pub(crate) bank: std::cell::RefCell<&'a mut BusinessDiffProcessingWorkV1>,
}
impl Processing<'_> {
    fn debit(&self, n: usize) -> Result<(), ValueTreeError> {
        let n = u64::try_from(n).map_err(|_| limit())?;
        let steps = n.checked_add(1).ok_or_else(limit)?;
        let mut p = self
            .bank
            .try_borrow_mut()
            .map_err(|_| ValueTreeError::Invalid("WorldRules processing reentry"))?;
        let old = (p.steps_remaining, p.bytes_remaining);
        p.steps_remaining = p.steps_remaining.saturating_sub(steps);
        p.bytes_remaining = p.bytes_remaining.saturating_sub(n);
        if old.0 < steps || old.1 < n {
            return Err(limit());
        }
        Ok(())
    }
}

pub(crate) fn spend(
    p: &Processing<'_>,
    n: usize,
    stop: &mut impl FnMut() -> bool,
) -> Result<(), ValueTreeError> {
    if stop() {
        return Err(ValueTreeError::Cancelled);
    }
    p.debit(n)
}
pub(crate) struct ProcessingState<'a, 'p, S> {
    pub(crate) source: &'a S,
    pub(crate) processing: &'a Processing<'p>,
    pub(crate) first: std::cell::RefCell<Option<ValueTreeError>>,
}
impl<S: MerkleNodeReader> MerkleNodeReader for ProcessingState<'_, '_, S> {
    type Node = S::Node;
    fn allocation_class(&self) -> Option<MemoryClass> {
        self.source.allocation_class()
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        self.source.allocation_budget()
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        if self.first.borrow().is_some() {
            return Err("Current rule processing already failed".into());
        }
        let result = self.source.read_node(id, max);
        if let Ok(Some(node)) = &result {
            let cost = node
                .as_ref()
                .len()
                .checked_mul(16)
                .and_then(|n| n.checked_add(id.len()))
                .and_then(|n| n.checked_add(256));
            let payment = match cost {
                Some(n) => self.processing.debit(n),
                None => Err(limit()),
            };
            if let Err(error) = payment {
                *self.first.borrow_mut() = Some(error);
                return Err("Current rule page processing failed".into());
            }
        }
        result
    }
}

struct Row {
    path: Vec<String>,
    semantic: ReservedValue<Vec<u8>>,
    json: Option<ReservedValue<Vec<u8>>>,
    _fee: MemoryReservation,
}
pub struct CurrentWorldRuleProjectionDeltaV1 {
    rules_present: bool,
    branch_emission: [Option<bool>; 2],
    rows: Vec<Row>,
    _slots: MemoryReservation,
    _sources: Vec<Source>,
    _source_slots: MemoryReservation,
    guard: PreparedGuard,
    parent: [u8; 32],
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
type Requests = BTreeMap<String, crate::world_rules::WorldRuleReceiptV1>;
type Orders = BTreeMap<String, crate::world_rules::AgentIndustrialOrderV1>;
enum Source {
    Supply(ReservedValue<crate::incremental_world_rule_supply_root::SupplyRecordQualificationV1>),
    Organization(
        ReservedValue<
            crate::incremental_world_rule_organization::root::OrganizationRecordQualificationV1,
        >,
    ),
    Requests(ReservedValue<WorldRuleCompositeOverlayV1<Requests>>),
    Orders(ReservedValue<WorldRuleCompositeOverlayV1<Orders>>),
    Continuity(
        ReservedValue<
            WorldRuleCompositeOverlayV1<crate::world_rules::OrganizationContinuityStateV1>,
        >,
    ),
    Emergency(
        ReservedValue<WorldRuleCompositeOverlayV1<crate::world_rules::EmergencySupplyStateV1>>,
    ),
    Industrial(
        ReservedValue<
            WorldRuleCompositeOverlayV1<oasis7_agent_api::world_action::IndustrialWorkStateV1>,
        >,
    ),
    Substitution(
        ReservedValue<
            WorldRuleCompositeOverlayV1<oasis7_agent_api::world_action::SubstitutionStateV1>,
        >,
    ),
}
impl CurrentWorldRuleProjectionDeltaV1 {
    pub fn rules_present(&self) -> bool {
        self.rules_present
    }
    pub fn branch_emission(&self) -> [Option<bool>; 2] {
        self.branch_emission
    }
    pub fn visit_with_work_v1<E>(
        &self,
        work: &mut dyn FnMut(usize) -> Result<(), E>,
        mut visit: impl FnMut(&[String], &[u8], Option<&[u8]>) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.rows {
            work(0)?;
            visit(
                &row.path,
                &row.semantic,
                row.json.as_deref().map(Vec::as_slice),
            )?;
        }
        Ok(())
    }

    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_read_source_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory)
            || !self._fee.uses_budget(memory)
            || !tx.uses_budget(memory)
            || tx.parent_fence().parent_roots != self.parent
        {
            return Err(bad().into());
        }
        Ok(())
    }
    pub fn visit<E: From<ValueTreeError>>(
        &self,
        processing: &mut BusinessDiffProcessingWorkV1,
        mut stop: impl FnMut() -> bool,
        mut visit: impl FnMut(&[String], &[u8], Option<&[u8]>) -> Result<(), E>,
    ) -> Result<(), E> {
        let processing = Processing {
            bank: std::cell::RefCell::new(processing),
        };
        for row in &self.rows {
            spend(&processing, 0, &mut stop)?;
            visit(
                &row.path,
                &row.semantic,
                row.json.as_deref().map(Vec::as_slice),
            )?;
        }
        Ok(())
    }
}
struct Build {
    emission: [Option<bool>; 2],
    rows: Vec<Row>,
    slots: MemoryReservation,
    memory: MemoryBudget,
    sources: Vec<Source>,
    source_slots: MemoryReservation,
}
impl Build {
    fn push(
        &mut self,
        row: Row,
        p: &Processing<'_>,
        stop: &mut impl FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        if self.rows.len() == self.rows.capacity() {
            let cap = self
                .rows
                .capacity()
                .checked_mul(2)
                .unwrap_or(usize::MAX)
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
                .map_err(ValueTreeError::Invalid)?;
        }
        spend(p, size_of::<Row>(), stop)?;
        self.rows.push(row);
        Ok(())
    }
}
struct Output<'a, 'p, C: FnMut() -> bool> {
    p: &'a Processing<'p>,
    stop: &'a mut C,
    bytes: Option<Vec<u8>>,
    n: usize,
    max: usize,
    first: Option<ValueTreeError>,
}
impl<C: FnMut() -> bool> Write for Output<'_, '_, C> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.first.is_some() {
            return Err(io::ErrorKind::Other.into());
        }
        for chunk in b.chunks(8192) {
            if let Err(e) = spend(self.p, chunk.len(), self.stop) {
                self.first = Some(e);
                return Err(io::ErrorKind::Other.into());
            }
            let Some(n) = self.n.checked_add(chunk.len()).filter(|n| *n <= self.max) else {
                self.first = Some(limit());
                return Err(io::ErrorKind::Other.into());
            };
            if let Some(out) = &mut self.bytes {
                out.extend_from_slice(chunk)
            }
            self.n = n;
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(crate) fn json<T: serde::Serialize, C: FnMut() -> bool>(
    value: &T,
    memory: &MemoryBudget,
    p: &Processing<'_>,
    stop: &mut C,
) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
    let mut count = Output {
        p,
        stop,
        bytes: None,
        n: 0,
        max: usize::MAX,
        first: None,
    };
    let result = serde_json::to_writer(&mut count, value);
    if let Some(e) = count.first.take() {
        return Err(e);
    }
    result.map_err(|_| bad())?;
    let n = count.n;
    drop(count);
    let fee = memory.reserve(
        n.checked_add(size_of::<Vec<u8>>()).ok_or_else(limit)?,
        MemoryClass::Ordinary,
    )?;
    spend(p, n, stop)?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(n).map_err(|_| limit())?;
    if bytes.capacity() > n {
        return Err(limit());
    }
    let mut out = Output {
        p,
        stop,
        bytes: Some(bytes),
        n: 0,
        max: n,
        first: None,
    };
    let result = serde_json::to_writer(&mut out, value);
    if let Some(e) = out.first.take() {
        return Err(e);
    }
    result.map_err(|_| bad())?;
    if out.n != n {
        return Err(bad());
    }
    Ok(fee.retain(out.bytes.take().ok_or_else(bad)?))
}

/// Shared original serde JSON DATA encoder; consumes the caller's continuous
/// processing bank and retains its exact output allocation.
pub(crate) fn encode_original_projection_json_data_v1<T: serde::Serialize>(
    value: &T,
    memory: &MemoryBudget,
    processing: &mut BusinessDiffProcessingWorkV1,
    stop: &mut impl FnMut() -> bool,
) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
    let p = Processing {
        bank: std::cell::RefCell::new(processing),
    };
    json(value, memory, &p, stop)
}
fn path(
    parts: &[&str],
    memory: &MemoryBudget,
    p: &Processing<'_>,
    stop: &mut impl FnMut() -> bool,
) -> Result<(Vec<String>, MemoryReservation), ValueTreeError> {
    let n = parts.iter().try_fold(
        parts
            .len()
            .checked_mul(size_of::<String>())
            .ok_or_else(limit)?,
        |n, s| n.checked_add(s.len()).ok_or_else(limit),
    )?;
    spend(p, n, stop)?;
    let fee = memory.reserve(n, MemoryClass::Ordinary)?;
    let mut out = Vec::new();
    out.try_reserve_exact(parts.len()).map_err(|_| limit())?;
    if out.capacity() > parts.len() {
        return Err(limit());
    }
    for s in parts {
        spend(p, s.len(), stop)?;
        let mut text = String::new();
        text.try_reserve_exact(s.len()).map_err(|_| limit())?;
        if text.capacity() > s.len() {
            return Err(limit());
        }
        text.push_str(s);
        out.push(text);
    }
    Ok((out, fee))
}
fn semantic<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    cell: &TypedCellRefV1<'_>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    p: &Processing<'_>,
) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
    let TypedCellRefV1::Entry(entry) = cell else {
        return Err(bad());
    };
    let parsed = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let expected = WorldFieldKeyV1::new(profile.field, parsed.kind, parsed.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: d.ctx.semantic_bytes,
        payload_bytes: d.ctx.record_bytes,
        record_bytes: d.ctx.record_bytes,
    };
    let header = read_member_record_header_v1(
        &entry.value,
        &expected,
        &mut d.ctx.values,
        limits,
        &mut d.ctx.cancelled,
    )?;
    let n = usize::try_from(header.semantic_bytes).map_err(|_| limit())?;
    spend(p, n, &mut d.ctx.cancelled)?;
    let fee = d.ctx.values.memory.reserve(
        n.checked_add(size_of::<Vec<u8>>()).ok_or_else(limit)?,
        MemoryClass::Ordinary,
    )?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(n).map_err(|_| limit())?;
    if bytes.capacity() > n {
        return Err(limit());
    }
    read_member_record_key_v1(
        &entry.value,
        &expected,
        &mut d.ctx.values,
        limits,
        &mut bytes,
        &mut d.ctx.cancelled,
    )?;
    if bytes.len() != n {
        return Err(bad());
    }
    Ok(fee.retain(bytes))
}
/// Private fixed-owner bridge. Callers pin the actual schema/type; this shared
/// helper supplies the sole typed decode and original typed JSON serializer.
fn read_typed_projection_cell_json_impl<
    T: Codec + serde::Serialize,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    cell: &TypedCellRefV1<'_>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    p: &Processing<'_>,
) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
    let layout = T::owned_decode_layout_v1(&mut || spend(p, 0, &mut d.ctx.cancelled))?;
    if layout.missing.is_some() {
        return Err(bad());
    }
    if let TypedCellRefV1::Body(root) = cell {
        let value =
            recover_typed_record_body_v1::<T, _, _, _, _>(root, profile, d.ctx, d.scan, d.work)?;
        return json(value.value(), d.ctx.values.memory, p, &mut d.ctx.cancelled);
    }
    let raw = diff::row(profile, cell, d)?;
    let extra = match &raw.cell {
        ColumnCellV1::Text(t) => t
            .len()
            .checked_mul(2)
            .and_then(|n| n.checked_add(512))
            .ok_or_else(limit)?,
        _ => 512,
    };
    spend(p, extra, &mut d.ctx.cancelled)?;
    let _copy = d.ctx.values.memory.reserve(extra, MemoryClass::Ordinary)?;
    let value = recover_typed_cell_v1::<T, _, _, _, _>(
        diff::clone_cell(&raw.cell),
        profile,
        d.ctx,
        d.scan,
        d.work,
    )?;
    json(value.value(), d.ctx.values.memory, p, &mut d.ctx.cancelled)
}
/// Fixed WS selectors reuse this bridge with their own original type/schema.
/// The processing bank is borrowed from the enclosing actual Common loan.
pub struct CurrentTypedProjectionJsonReadV1<'a, 'd, 'v, 'cell, M, R, E, C> {
    profile: &'a TypedRecordProfileV1,
    cell: &'a TypedCellRefV1<'cell>,
    decoder: &'a mut ChangeDecoder<'d, 'v, M, R, E, C>,
    processing: &'a mut BusinessDiffProcessingWorkV1,
}
impl<'a, 'd, 'v, 'cell, M, R, E, C>
    CurrentTypedProjectionJsonReadV1<'a, 'd, 'v, 'cell, M, R, E, C>
{
    /// Only fixed in-crate selectors can join the authenticated cell and codec profile.
    pub(crate) fn new(
        profile: &'a TypedRecordProfileV1,
        cell: &'a TypedCellRefV1<'cell>,
        decoder: &'a mut ChangeDecoder<'d, 'v, M, R, E, C>,
        processing: &'a mut BusinessDiffProcessingWorkV1,
    ) -> Self {
        Self {
            profile,
            cell,
            decoder,
            processing,
        }
    }
}
mod typed_projection_json_sealed {
    pub trait Sealed {}
    impl<T: super::Codec + serde::Serialize> Sealed for T {}
}
/// Static DATA decoding through the original closed typed codec.
/// This trait neither constructs a source nor grants Current qualification.
pub trait CurrentTypedProjectionJsonValueV1: typed_projection_json_sealed::Sealed {
    fn read_projection_json_v1<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        source: &mut CurrentTypedProjectionJsonReadV1<'_, '_, '_, '_, M, R, E, C>,
    ) -> Result<ReservedValue<Vec<u8>>, ValueTreeError>;
}
impl<T: Codec + serde::Serialize> CurrentTypedProjectionJsonValueV1 for T {
    fn read_projection_json_v1<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        source: &mut CurrentTypedProjectionJsonReadV1<'_, '_, '_, '_, M, R, E, C>,
    ) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
        read_typed_projection_cell_json_v1::<T, M, R, E, C>(
            source.profile,
            source.cell,
            source.decoder,
            source.processing,
        )
    }
}

pub(crate) fn read_typed_projection_cell_json_v1<
    T: Codec + serde::Serialize,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    cell: &TypedCellRefV1<'_>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    processing: &mut BusinessDiffProcessingWorkV1,
) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
    let p = Processing {
        bank: std::cell::RefCell::new(processing),
    };
    let state = ProcessingState {
        source: d.ctx.state,
        processing: &p,
        first: std::cell::RefCell::new(None),
    };
    let _phase = d.ctx.values.memory.reserve(
        size_of::<ProcessingState<'_, '_, M>>() + size_of::<Processing<'_>>() + 128,
        MemoryClass::Ordinary,
    )?;
    let result = {
        let mut errors = || {
            if let Some(error) = (d.ctx.take_state_error)() {
                Some(error)
            } else {
                state.first.borrow_mut().take()
            }
        };
        let mut read = WorldFieldReadContextV1 {
            root: d.ctx.root,
            state: &state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: d.ctx.values.source,
                memory: d.ctx.values.memory,
                class: d.ctx.values.class,
                io: &mut *d.ctx.values.io,
            },
            key_work: &mut *d.ctx.key_work,
            semantic_bytes: d.ctx.semantic_bytes,
            record_bytes: d.ctx.record_bytes,
            take_state_error: &mut errors,
            cancelled: &mut d.ctx.cancelled,
        };
        let mut decoder = ChangeDecoder {
            ctx: &mut read,
            scan: &mut *d.scan,
            work: &mut *d.work,
            profile: *profile,
            trees_remaining: d.trees_remaining,
            tree_depth: d.tree_depth,
        };
        read_typed_projection_cell_json_impl::<T, _, _, _, _>(profile, cell, &mut decoder, &p)
    };
    let first = state.first.borrow_mut().take();
    match first {
        Some(error) => Err(error),
        None => result,
    }
}
fn leaf<
    T: Codec + serde::Serialize,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parts: &[&str],
    profile: &TypedRecordProfileV1,
    old: Option<&TypedCellRefV1<'_>>,
    new: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    p: &Processing<'_>,
    b: &mut Build,
) -> Result<(), ValueTreeError> {
    if diff::unchanged(old, new) {
        return Ok(());
    }
    T::accept_changed(profile, old, new, d)?;
    let key = semantic(profile, new.or(old).ok_or_else(bad)?, d, p)?;
    let payload = if let Some(new) = new {
        Some(read_typed_projection_cell_json_impl::<T, _, _, _, _>(
            profile, new, d, p,
        )?)
    } else {
        None
    };
    let (path, fee) = path(parts, &b.memory, p, &mut d.ctx.cancelled)?;
    b.push(
        Row {
            path,
            semantic: key,
            json: payload,
            _fee: fee,
        },
        p,
        &mut d.ctx.cancelled,
    )
}
fn branch_semantic<C: FnMut() -> bool>(
    branch: &str,
    memory: &MemoryBudget,
    p: &Processing<'_>,
    stop: &mut C,
) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
    let encode = |mut out: &mut dyn Write| {
        crate::incremental_semantic_key::write_semantic_key_v1(&(branch, ()), &mut out)
            .map_err(|_| bad())
    };
    let mut count = Output {
        p,
        stop,
        bytes: None,
        n: 0,
        max: usize::MAX,
        first: None,
    };
    let result = encode(&mut count);
    if let Some(e) = count.first.take() {
        return Err(e);
    }
    result?;
    let n = count.n;
    drop(count);
    spend(p, n, stop)?;
    let fee = memory.reserve(
        n.checked_add(size_of::<Vec<u8>>()).ok_or_else(limit)?,
        MemoryClass::Ordinary,
    )?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(n).map_err(|_| limit())?;
    if bytes.capacity() > n {
        return Err(limit());
    }
    let mut out = Output {
        p,
        stop,
        bytes: Some(bytes),
        n: 0,
        max: n,
        first: None,
    };
    let result = encode(&mut out);
    if let Some(e) = out.first.take() {
        return Err(e);
    }
    result?;
    if out.n != n {
        return Err(bad());
    }
    Ok(fee.retain(out.bytes.take().ok_or_else(bad)?))
}
fn map<
    V: Codec + serde::Serialize,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parts: &[&str],
    profile: &TypedRecordProfileV1,
    old: Option<&TypedCellRefV1<'_>>,
    new: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    p: &Processing<'_>,
    b: &mut Build,
) -> Result<(), ValueTreeError> {
    if old.is_none() {
        if let Some(new) = new {
            let root = diff::root(profile, new, d)?;
            if root.leaf_count == 2 {
                <BTreeMap<String, V> as Codec>::accept_changed(profile, old, Some(new), d)?;
                let payload =
                    read_typed_projection_cell_json_impl::<BTreeMap<String, V>, _, _, _, _>(
                        profile, new, d, p,
                    )?;
                let key = if matches!(new, TypedCellRefV1::Body(_)) {
                    branch_semantic(
                        parts.get(1).ok_or_else(bad)?,
                        &b.memory,
                        p,
                        &mut d.ctx.cancelled,
                    )?
                } else {
                    semantic(profile, new, d, p)?
                };
                let (path, fee) = path(parts, &b.memory, p, &mut d.ctx.cancelled)?;
                return b.push(
                    Row {
                        path,
                        semantic: key,
                        json: Some(payload),
                        _fee: fee,
                    },
                    p,
                    &mut d.ctx.cancelled,
                );
            }
        }
    }
    diff::accept_container::<BTreeMap<String, V>, _, _, _, _>(
        profile,
        old,
        new,
        d,
        false,
        |key, old, new, d| {
            let ColumnKeyV1::Pair(tag, key) = key else {
                return Err(bad());
            };
            if !matches!(&**tag,ColumnKeyV1::Text(s)if s=="entry") {
                return Err(bad());
            }
            let ColumnKeyV1::Text(key) = &**key else {
                return Err(bad());
            };
            let mut path = [""; 5];
            if parts.len() >= path.len() {
                return Err(bad());
            }
            path[..parts.len()].copy_from_slice(parts);
            path[parts.len()] = key;
            leaf::<V, _, _, _, _>(&path[..parts.len() + 1], profile, old, new, d, p, b)
        },
    )
}
fn map_field<
    Owner,
    V: Codec + serde::Serialize,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    _: fn(&Owner) -> &BTreeMap<String, V>,
    parts: &[&str],
    profile: &TypedRecordProfileV1,
    old: Option<&TypedCellRefV1<'_>>,
    new: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    p: &Processing<'_>,
    b: &mut Build,
) -> Result<(), ValueTreeError> {
    map::<V, _, _, _, _>(parts, profile, old, new, d, p, b)
}
fn whole_field<
    Owner,
    T: Codec + serde::Serialize,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    _: fn(&Owner) -> &T,
    parts: &[&str],
    profile: &TypedRecordProfileV1,
    old: Option<&TypedCellRefV1<'_>>,
    new: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    p: &Processing<'_>,
    b: &mut Build,
) -> Result<(), ValueTreeError> {
    leaf::<T, _, _, _, _>(parts, profile, old, new, d, p, b)
}
trait ProjectBranch: CompositeBranchCodecV1 {
    fn project<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        profile: &TypedRecordProfileV1,
        old: Option<&TypedCellRefV1<'_>>,
        new: Option<&TypedCellRefV1<'_>>,
        d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
        p: &Processing<'_>,
        b: &mut Build,
    ) -> Result<(), ValueTreeError>;
}
macro_rules! direct_map {
    ($ty:ty,$value:ty) => {
        impl ProjectBranch for $ty {
            fn project<
                M: MerkleNodeReader,
                R: ValueNodeReader,
                E: FnMut() -> Option<ValueTreeError>,
                C: FnMut() -> bool,
            >(
                profile: &TypedRecordProfileV1,
                old: Option<&TypedCellRefV1<'_>>,
                new: Option<&TypedCellRefV1<'_>>,
                d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
                p: &Processing<'_>,
                b: &mut Build,
            ) -> Result<(), ValueTreeError> {
                map::<$value, _, _, _, _>(
                    &["world_rules", Self::BRANCH.name()],
                    profile,
                    old,
                    new,
                    d,
                    p,
                    b,
                )
            }
        }
    };
}
direct_map!(Requests, crate::world_rules::WorldRuleReceiptV1);
direct_map!(Orders, crate::world_rules::AgentIndustrialOrderV1);
macro_rules! fields{($ty:ty,{$($field:ident:$mode:ident),+$(,)?})=>{impl ProjectBranch for $ty{
 fn project<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(profile:&TypedRecordProfileV1,old:Option<&TypedCellRefV1<'_>>,new:Option<&TypedCellRefV1<'_>>,d:&mut ChangeDecoder<'_, '_,M,R,E,C>,p:&Processing<'_>,b:&mut Build)->Result<(),ValueTreeError>{
  diff::accept_struct(profile,old,new,d,&[$(stringify!($field)),+],|name,old,new,d|match name{$(stringify!($field)=>$mode::<Self,_,_,_,_,_>(|owner|&owner.$field,&["world_rules",Self::BRANCH.name(),stringify!($field)],profile,old,new,d,p,b),)+_=>Err(bad())})
 }
}};}
fields!(crate::world_rules::OrganizationContinuityStateV1,{registrations:map_field,availability:map_field,activity_receipts:map_field,membership_history:map_field,assets:map_field,cases:map_field,consumed_effects:map_field});
fields!(crate::world_rules::EmergencySupplyStateV1,{world_id:whole_field,minimum_services:map_field,region_lineages:map_field,observations:map_field,observation_interval_coverage:map_field,overflowed_windows:whole_field,services:map_field,evidence:map_field,authorizations:map_field,batches:map_field,needs:map_field,reviews:map_field,restricted_power:map_field});
fields!(oasis7_agent_api::world_action::IndustrialWorkStateV1,{roots:map_field,lots:map_field,imported_lot_authorities:map_field,receipts:map_field,sink_keys:whole_field,bindings:whole_field,dispositions:map_field});
fields!(oasis7_agent_api::world_action::SubstitutionStateV1,{authorities:map_field,decisions:map_field,settlements:map_field,conversion_domains:whole_field,terminal_used:map_field,retained_residuals:map_field});
fn project_branch<
    T: ProjectBranch,
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<T>>,
    roots: &RootSetV1,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
    p: &Processing<'_>,
    b: &mut Build,
    live: &ExecutionFence,
) -> Result<ReservedValue<WorldRuleCompositeOverlayV1<T>>, WorldFieldOverlayErrorV1> {
    let guard = ctx.transaction.guard();
    let observed =
        bind_current_world_rule_branch_v1(parent, roots, state, ctx, scan, work, &guard, live)?;
    let profile = T::BRANCH.profile();
    let mut read = WorldFieldReadContextV1 {
        root: observed.body(),
        state,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
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
    let trees_remaining = work.max_retained_trees;
    let mut decoder = ChangeDecoder {
        ctx: &mut read,
        scan,
        work,
        profile,
        trees_remaining,
        tree_depth: 0,
    };
    if matches!(T::BRANCH, Branch::IndustrialWork | Branch::Substitution) {
        let mut columns = read_columns_v1(
            ColumnReadProfileV1 {
                field: profile.field,
                schema: profile.schema,
                groups: &[],
                reference_schema: profile.reference_schema,
            },
            observed.body(),
            decoder.ctx,
            decoder.scan,
            decoder.work.max_tree_heap,
        )?;
        let mut empty = true;
        for value in columns.cells_mut().values() {
            spend(p, 0, &mut decoder.ctx.cancelled)?;
            let ColumnCellV1::Root(root) = value else {
                return Err(bad().into());
            };
            if root.leaf_count != 2 {
                empty = false;
            }
        }
        b.emission[if T::BRANCH == Branch::IndustrialWork {
            0
        } else {
            1
        }] = Some(!empty);
    }
    T::project(
        &profile,
        parent.body().map(TypedCellRefV1::Body).as_ref(),
        Some(&TypedCellRefV1::Body(observed.body())),
        &mut decoder,
        p,
        b,
    )?;
    Ok(observed)
}
struct Pending {
    key: Vec<u8>,
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
}
fn relevant(raw: &[u8]) -> Result<bool, ValueTreeError> {
    if !raw.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(false);
    }
    let key = WorldFieldKeyV1::from_bytes(raw).map_err(ValueTreeError::Invalid)?;
    if key.name != "world_rules" || key.owner != FieldOwner::WorldState {
        return Ok(false);
    }
    let fixed = WorldFieldKeyV1::new(
        crate::incremental_world_rule_parent::descriptor()?,
        key.kind,
        key.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    if fixed.as_bytes() != raw || key.namespace != StateNamespaceV1::Authorization {
        return Err(bad());
    }
    Ok(true)
}
#[path = "incremental_world_rule_projection_delta_reader.rs"]
mod projection_reader;
pub use projection_reader::read_current_world_rule_projection_delta_v1;

#[path = "incremental_world_rule_projection_delta_flat.rs"]
mod flat;

pub use native::{QualifiedNativeProjectionColumnsV1, read_qualified_native_projection_columns_v1};

#[path = "incremental_world_rule_projection_delta_cold.rs"]
mod cold;
pub use cold::{
    QualifiedWorldRuleProjectionColumnsV1, read_qualified_world_rule_projection_columns_v1,
};

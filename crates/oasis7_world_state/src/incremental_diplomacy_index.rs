//! Private-construction relation data derived from qualified typed diplomacy.
//! Neither an index nor an observation authorizes an alliance action.
use super::*;
use oasis7_formal_protocol::incremental_state::StateEntryV1;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryReservation};
use std::sync::Arc;

#[path = "incremental_diplomacy_index_current.rs"]
mod current;
pub use current::{
    DiplomacyRelationObservationV1, observe_current_active_wars_v1,
    observe_current_alliance_memberships_v1, open_current_diplomacy_relation_record_v1,
};

pub struct DiplomacyIndexWorkV1 {
    pub cells_remaining: u64,
    pub comparison_bytes_remaining: u64,
    pub max_rows: usize,
    pub max_members: usize,
    pub max_text_bytes: usize,
    pub max_pending_rows: usize,
    pub pending_entries_remaining: usize,
}
impl DiplomacyIndexWorkV1 {
    pub(in crate::incremental_diplomacy) fn from_cells(
        cells: u64,
        max: u64,
    ) -> Result<Self, ValueTreeError> {
        let count = usize::try_from(cells).map_err(|_| closure::invalid())?;
        Ok(Self {
            cells_remaining: cells,
            comparison_bytes_remaining: cells.checked_mul(max).ok_or_else(closure::invalid)?,
            max_rows: count,
            max_members: count,
            max_text_bytes: usize::try_from(max).map_err(|_| closure::invalid())?,
            max_pending_rows: count,
            pending_entries_remaining: count,
        })
    }
    fn step(&mut self) -> Result<(), ValueTreeError> {
        self.cells_remaining = self
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("diplomacy index cells"))?;
        Ok(())
    }
    fn compare(&mut self, a: &str, b: &str) -> Result<std::cmp::Ordering, ValueTreeError> {
        self.step()?;
        let n = a.len().checked_add(b.len()).ok_or_else(closure::invalid)?;
        self.comparison_bytes_remaining = self
            .comparison_bytes_remaining
            .checked_sub(u64::try_from(n).map_err(|_| closure::invalid())?)
            .ok_or(ValueTreeError::ResourceLimited(
                "diplomacy index comparison",
            ))?;
        Ok(a.cmp(b))
    }
}
type Text = Arc<ReservedValue<String>>;
pub(super) type HeldRow = Arc<ReservedValue<Row>>;
#[derive(Debug)]
pub(super) struct Row {
    field: DiplomacyFieldV1,
    key: WorldFieldKeyV1,
    member: [u8; 32],
    outer: Text,
    child: StateRootV1,
    members: Vec<Text>,
    active: bool,
    aggressor: Option<Text>,
    defender: Option<Text>,
}
#[derive(Debug)]
struct Relation {
    target: Text,
    row: HeldRow,
    ordinal: Option<u64>,
}
#[derive(Debug)]
pub(in crate::incremental_diplomacy) struct DiplomacyRelationsIndexV1 {
    rows: Vec<HeldRow>,
    members: Vec<Relation>,
    wars: Vec<Relation>,
    // The fee covers these vector carriers. Each retained row/text additionally
    // covers three growing-vector slots, including a reallocation overlap.
    _fee: MemoryReservation,
    _vectors: Option<MemoryReservation>,
    max_text: usize,
    max_members: usize,
}
fn add(a: usize, b: usize) -> Result<usize, ValueTreeError> {
    a.checked_add(b).ok_or_else(closure::invalid)
}
fn mul(a: usize, b: usize) -> Result<usize, ValueTreeError> {
    a.checked_mul(b).ok_or_else(closure::invalid)
}
fn text_fee(
    n: usize,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<MemoryReservation, ValueTreeError> {
    let slots = add(
        mul(3, std::mem::size_of::<Text>())?,
        mul(3, std::mem::size_of::<Relation>())?,
    )?;
    let carrier = add(
        std::mem::size_of::<ReservedValue<String>>(),
        4 * std::mem::size_of::<usize>(),
    )?;
    Ok(memory.reserve(add(add(carrier, slots)?, n)?, class)?)
}
fn hold_text(
    s: &str,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut DiplomacyIndexWorkV1,
) -> Result<Text, ValueTreeError> {
    work.step()?;
    if s.len() > work.max_text_bytes {
        return Err(ValueTreeError::ResourceLimited("diplomacy index text"));
    }
    let fee = text_fee(s.len(), memory, class)?;
    Ok(Arc::new(fee.retain(s.to_owned())))
}
fn sort<T>(
    items: &mut [T],
    work: &mut DiplomacyIndexWorkV1,
    mut cancelled: impl FnMut() -> bool,
    mut cmp: impl FnMut(&T, &T, &mut DiplomacyIndexWorkV1) -> Result<std::cmp::Ordering, ValueTreeError>,
) -> Result<(), ValueTreeError> {
    // In-place heapsort: no uncharged stable-sort scratch, and every comparison
    // is fallible before inspecting variable-length text.
    fn down<T>(
        a: &mut [T],
        mut pos: usize,
        end: usize,
        w: &mut DiplomacyIndexWorkV1,
        c: &mut impl FnMut() -> bool,
        cmp: &mut impl FnMut(
            &T,
            &T,
            &mut DiplomacyIndexWorkV1,
        ) -> Result<std::cmp::Ordering, ValueTreeError>,
    ) -> Result<(), ValueTreeError> {
        loop {
            if c() {
                return Err(ValueTreeError::Cancelled);
            }
            w.step()?;
            let Some(left) = pos.checked_mul(2).and_then(|n| n.checked_add(1)) else {
                return Err(closure::invalid());
            };
            if left >= end {
                return Ok(());
            }
            let mut child = left;
            if left + 1 < end && cmp(&a[left], &a[left + 1], w)?.is_lt() {
                child += 1;
            }
            if !cmp(&a[pos], &a[child], w)?.is_lt() {
                return Ok(());
            }
            a.swap(pos, child);
            pos = child;
        }
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let len = items.len();
    for i in (0..len / 2).rev() {
        down(items, i, len, work, &mut cancelled, &mut cmp)?;
    }
    for end in (1..items.len()).rev() {
        items.swap(0, end);
        down(items, 0, end, work, &mut cancelled, &mut cmp)?;
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}
impl DiplomacyRelationsIndexV1 {
    pub(super) fn record_count(&self) -> usize {
        self.rows.len()
    }
    pub(super) fn empty(memory: &MemoryBudget, class: MemoryClass) -> Result<Self, ValueTreeError> {
        Ok(Self {
            rows: Vec::new(),
            members: Vec::new(),
            wars: Vec::new(),
            _fee: memory.reserve(512, class)?,
            _vectors: None,
            max_text: 0,
            max_members: 0,
        })
    }
    pub(super) fn push(
        &mut self,
        row: HeldRow,
        work: &mut DiplomacyIndexWorkV1,
    ) -> Result<(), ValueTreeError> {
        work.step()?;
        if self.rows.len() >= work.max_rows {
            return Err(ValueTreeError::ResourceLimited("diplomacy index rows"));
        }
        self.rows.push(row);
        Ok(())
    }
    pub(super) fn finish(
        &mut self,
        work: &mut DiplomacyIndexWorkV1,
        mut cancelled: impl FnMut() -> bool,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<(), ValueTreeError> {
        if self._vectors.is_some() {
            return Err(closure::invalid());
        }
        let count = self.rows.iter().try_fold(0usize, |n, row| {
            if cancelled() {
                return Err(ValueTreeError::Cancelled);
            }
            work.step()?;
            let extra = if row.field == DiplomacyFieldV1::Alliances {
                row.members.len()
            } else if row.active {
                2
            } else {
                0
            };
            add(n, extra)
        })?;
        let bytes = add(
            mul(mul(2, self.rows.len())?, std::mem::size_of::<HeldRow>())?,
            mul(mul(2, count)?, std::mem::size_of::<Relation>())?,
        )?;
        self._vectors = Some(memory.reserve(bytes, class)?);
        sort(&mut self.rows, work, &mut cancelled, |a, b, w| {
            w.step()?;
            Ok(a.key.as_bytes().cmp(b.key.as_bytes()))
        })?;
        for row in &self.rows {
            if cancelled() {
                return Err(ValueTreeError::Cancelled);
            }
            work.step()?;
            self.max_text = self.max_text.max(row.outer.len());
            self.max_members = self.max_members.max(row.members.len());
            for text in row
                .members
                .iter()
                .chain(row.aggressor.iter())
                .chain(row.defender.iter())
            {
                if cancelled() {
                    return Err(ValueTreeError::Cancelled);
                }
                work.step()?;
                self.max_text = self.max_text.max(text.len());
            }
            match row.field {
                DiplomacyFieldV1::Alliances => {
                    for (ordinal, target) in row.members.iter().enumerate() {
                        work.step()?;
                        self.members.push(Relation {
                            target: target.clone(),
                            row: row.clone(),
                            ordinal: Some(ordinal as u64),
                        });
                    }
                }
                DiplomacyFieldV1::Wars if row.active => {
                    for target in [&row.aggressor, &row.defender] {
                        work.step()?;
                        self.wars.push(Relation {
                            target: target.as_ref().ok_or_else(closure::invalid)?.clone(),
                            row: row.clone(),
                            ordinal: None,
                        });
                    }
                }
                _ => {}
            }
        }
        for relations in [&mut self.members, &mut self.wars] {
            sort(relations, work, &mut cancelled, |a, b, w| {
                let cmp = w.compare(&a.target, &b.target)?;
                Ok(if cmp.is_eq() {
                    w.compare(&a.row.outer, &b.row.outer)?
                        .then(a.ordinal.cmp(&b.ordinal))
                } else {
                    cmp
                })
            })?;
        }
        Ok(())
    }
}
fn outer_text<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    entry: &StateEntryV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut DiplomacyIndexWorkV1,
) -> Result<Text, ValueTreeError> {
    ctx.check()?;
    work.step()?;
    let key = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let field = DiplomacyFieldV1::from_name(key.name).ok_or_else(closure::invalid)?;
    let key = WorldFieldKeyV1::new(field.descriptor()?, key.kind, key.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: ctx.semantic_bytes,
        payload_bytes: RECORD_ROOT_BYTES as u64,
        record_bytes: ctx.record_bytes,
    };
    let info = read_member_record_header_v1(
        &entry.value,
        &key,
        &mut ctx.values,
        limits,
        &mut ctx.cancelled,
    )?;
    let n = usize::try_from(info.semantic_bytes).map_err(|_| closure::invalid())?;
    // Semantic framing plus the bounded outer ID; no caller length estimate.
    if n > add(work.max_text_bytes, 128)? {
        return Err(ValueTreeError::ResourceLimited("diplomacy index outer key"));
    }
    let _fee = ctx.values.memory.reserve(n, ctx.values.class)?;
    let mut bytes = Vec::with_capacity(n);
    crate::incremental_field_member::read_member_record_key_v1(
        &entry.value,
        &key,
        &mut ctx.values,
        limits,
        &mut bytes,
        &mut ctx.cancelled,
    )?;
    let crate::incremental_semantic_key::SemanticKeyRefV1::Text(id) =
        crate::incremental_semantic_key::read_semantic_key_v1(&bytes)
            .map_err(ValueTreeError::Invalid)?
    else {
        return Err(closure::invalid());
    };
    let text = hold_text(id, ctx.values.memory, ctx.values.class, work)?;
    ctx.check()?;
    Ok(text)
}
fn text_at<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: DiplomacyFieldV1,
    path: Path,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut DiplomacyIndexWorkV1,
) -> Result<Text, ValueTreeError> {
    ctx.check()?;
    work.step()?;
    let key = path.read(field, ctx)?;
    let value = ctx.lookup(&key.0)?.ok_or_else(closure::invalid)?;
    let info = read_member_record_header_v1(
        &value,
        &key.0,
        &mut ctx.values,
        MemberRecordLimitsV1 {
            semantic_bytes: ctx.semantic_bytes,
            payload_bytes: work.max_text_bytes as u64,
            record_bytes: ctx.record_bytes,
        },
        &mut ctx.cancelled,
    )?;
    let n = usize::try_from(info.payload_bytes).map_err(|_| closure::invalid())?;
    let fee = text_fee(n, ctx.values.memory, ctx.values.class)?;
    let mut bytes = Vec::with_capacity(n);
    let actual = closure::primitive(
        Kind::Text,
        &value,
        &key,
        &mut ctx.records(),
        work.max_text_bytes as u64,
        &mut bytes,
    )?;
    if actual
        != (RecordValueV1::Text {
            utf8_bytes: n as u64,
        })
    {
        return Err(closure::invalid());
    }
    let text = String::from_utf8(bytes).map_err(|_| closure::invalid())?;
    ctx.check()?;
    Ok(Arc::new(fee.retain(text)))
}
fn active_at<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut DiplomacyIndexWorkV1,
) -> Result<bool, ValueTreeError> {
    work.step()?;
    let key = profile::specs(DiplomacyFieldV1::Wars)[7]
        .path
        .read(DiplomacyFieldV1::Wars, ctx)?;
    let value = ctx.lookup(&key.0)?.ok_or_else(closure::invalid)?;
    let RecordValueV1::Bool(active) = closure::primitive(
        Kind::Bool,
        &value,
        &key,
        &mut ctx.records(),
        1,
        &mut std::io::sink(),
    )?
    else {
        return Err(closure::invalid());
    };
    ctx.check()?;
    Ok(active)
}
fn row_fee(
    memory: &MemoryBudget,
    class: MemoryClass,
    n: usize,
) -> Result<MemoryReservation, ValueTreeError> {
    let carrier = add(
        std::mem::size_of::<ReservedValue<Row>>(),
        64 + 4 * std::mem::size_of::<usize>(),
    )?;
    let bytes = add(
        carrier,
        add(
            mul(3, std::mem::size_of::<HeldRow>())?,
            mul(n, std::mem::size_of::<Text>())?,
        )?,
    )?;
    Ok(memory.reserve(bytes, class)?)
}
pub(super) fn genesis_row<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    entry: &StateEntryV1,
    field: DiplomacyFieldV1,
    child: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut DiplomacyIndexWorkV1,
) -> Result<Option<HeldRow>, ValueTreeError> {
    if !matches!(field, DiplomacyFieldV1::Alliances | DiplomacyFieldV1::Wars) {
        return Ok(None);
    }
    let outer = outer_text(entry, ctx, work)?;
    let key = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let key = WorldFieldKeyV1::new(field.descriptor()?, key.kind, key.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    let mut local = WorldFieldReadContextV1 {
        root: child,
        state: ctx.state,
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
    let n = if field == DiplomacyFieldV1::Alliances {
        usize::try_from(closure::length(field, &mut local)?).map_err(|_| closure::invalid())?
    } else {
        0
    };
    if n > work.max_members {
        return Err(ValueTreeError::ResourceLimited("diplomacy indexed members"));
    }
    let fee = row_fee(local.values.memory, local.values.class, n)?;
    let mut members = Vec::with_capacity(n);
    for i in 0..n {
        local.check()?;
        members.push(text_at(
            field,
            Path::Ordinal("members", i as u64),
            &mut local,
            work,
        )?);
    }
    let (active, aggressor, defender) = if field == DiplomacyFieldV1::Wars {
        (
            active_at(&mut local, work)?,
            Some(text_at(
                field,
                profile::specs(field)[3].path,
                &mut local,
                work,
            )?),
            Some(text_at(
                field,
                profile::specs(field)[4].path,
                &mut local,
                work,
            )?),
        )
    } else {
        (false, None, None)
    };
    local.check()?;
    Ok(Some(Arc::new(fee.retain(Row {
        field,
        key,
        member: entry.value,
        outer,
        child: child.clone(),
        members,
        active,
        aggressor,
        defender,
    }))))
}

fn find(
    rows: &[HeldRow],
    field: DiplomacyFieldV1,
    key: &WorldFieldKeyV1,
    work: &mut DiplomacyIndexWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<Option<usize>, ValueTreeError> {
    let (mut lo, mut hi) = (0, rows.len());
    while lo < hi {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        work.step()?;
        let mid = lo + (hi - lo) / 2;
        match rows[mid].key.as_bytes().cmp(key.as_bytes()) {
            std::cmp::Ordering::Less => lo = mid + 1,
            std::cmp::Ordering::Greater => hi = mid,
            std::cmp::Ordering::Equal => {
                if rows[mid].field != field {
                    return Err(closure::invalid());
                }
                return Ok(Some(mid));
            }
        }
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(None)
}
fn changed_row<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    old: Option<&HeldRow>,
    entry: &StateEntryV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut DiplomacyIndexWorkV1,
    max: u64,
) -> Result<HeldRow, ValueTreeError> {
    ctx.check()?;
    let (field, _, child) = super::outer(entry, ctx)?.ok_or_else(closure::invalid)?;
    super::super::receiver::child(
        field,
        old.map(|r| &r.child),
        &child,
        ctx,
        &mut work.cells_remaining,
        max,
    )?;
    let Some(old) = old else {
        return genesis_row(entry, field, &child, ctx, work)?.ok_or_else(closure::invalid);
    };
    let outer = outer_text(entry, ctx, work)?;
    if !work.compare(&outer, &old.outer)?.is_eq() {
        return Err(closure::invalid());
    }
    let mut local = WorldFieldReadContextV1 {
        root: &child,
        state: ctx.state,
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
    let n = if field == DiplomacyFieldV1::Alliances {
        usize::try_from(closure::length(field, &mut local)?).map_err(|_| closure::invalid())?
    } else {
        0
    };
    if n > work.max_members {
        return Err(ValueTreeError::ResourceLimited("diplomacy indexed members"));
    }
    let fee = row_fee(local.values.memory, local.values.class, mul(n, 2)?)?;
    let mut members: Vec<Option<Text>> = Vec::with_capacity(n);
    for i in 0..n {
        local.check()?;
        work.step()?;
        members.push(old.members.get(i).cloned());
    }
    if field == DiplomacyFieldV1::Alliances {
        let state = local.state;
        let error = &mut local.take_state_error;
        let mut records = InventoryValueReader {
            values: &mut local.values,
            key_work: &mut *local.key_work,
            semantic_bytes: local.semantic_bytes,
            record_bytes: local.record_bytes,
            cancelled: &mut local.cancelled,
        };
        let held = work.cells_remaining / 2;
        work.cells_remaining -= held;
        let mut remaining = held;
        let result = crate::incremental_record_diff::visit(
            &old.child,
            &child,
            &mut records,
            &mut remaining,
            crate::incremental_record_diff::RecordDiffLabelsV1 {
                invalid: "diplomacy relation delta",
                cells: "diplomacy relation delta cells",
                stack: "diplomacy relation delta stack",
                tree: inner_tree::LABELS,
            },
            |_, new, records| {
                if let Some(entry) = new {
                    let mut view = WorldFieldReadContextV1 {
                        root: &child,
                        state,
                        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                            source: records.values.source,
                            memory: records.values.memory,
                            class: records.values.class,
                            io: &mut *records.values.io,
                        },
                        key_work: &mut *records.key_work,
                        semantic_bytes: records.semantic_bytes,
                        record_bytes: records.record_bytes,
                        take_state_error: &mut *error,
                        cancelled: &mut *records.cancelled,
                    };
                    let (_, semantic) = closure::identity(field, entry, &mut view, max)?;
                    if let schema::Row::Member(i) = schema::inspect(field, &semantic)? {
                        let i = usize::try_from(i).map_err(|_| closure::invalid())?;
                        let slot = members.get_mut(i).ok_or_else(closure::invalid)?;
                        *slot = Some(text_at(
                            field,
                            Path::Ordinal("members", i as u64),
                            &mut view,
                            work,
                        )?);
                    }
                    view.check()?;
                }
                Ok(())
            },
        );
        work.cells_remaining = work
            .cells_remaining
            .checked_add(remaining)
            .ok_or_else(closure::invalid)?;
        result?;
    }
    let mut complete = Vec::with_capacity(n);
    for member in members {
        local.check()?;
        work.step()?;
        complete.push(member.ok_or_else(closure::invalid)?);
    }
    let (active, aggressor, defender) = if field == DiplomacyFieldV1::Wars {
        (
            active_at(&mut local, work)?,
            Some(text_at(
                field,
                profile::specs(field)[3].path,
                &mut local,
                work,
            )?),
            Some(text_at(
                field,
                profile::specs(field)[4].path,
                &mut local,
                work,
            )?),
        )
    } else {
        (false, None, None)
    };
    local.check()?;
    Ok(Arc::new(fee.retain(Row {
        field,
        key: old.key.clone(),
        member: entry.value,
        outer,
        child: (*child).clone(),
        members: complete,
        active,
        aggressor,
        defender,
    })))
}

pub(super) fn copied_index(
    source: &DiplomacyRelationsIndexV1,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut DiplomacyIndexWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<DiplomacyRelationsIndexV1, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let mut target = DiplomacyRelationsIndexV1::empty(memory, class)?;
    for row in &source.rows {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        work.step()?;
        let fee = row_fee(memory, class, row.members.len())?;
        let mut members = Vec::with_capacity(row.members.len());
        for text in &row.members {
            if cancelled() {
                return Err(ValueTreeError::Cancelled);
            }
            members.push(hold_text(text, memory, class, work)?);
        }
        let outer = hold_text(&row.outer, memory, class, work)?;
        let aggressor = row
            .aggressor
            .as_ref()
            .map(|s| hold_text(s, memory, class, work))
            .transpose()?;
        let defender = row
            .defender
            .as_ref()
            .map(|s| hold_text(s, memory, class, work))
            .transpose()?;
        target.push(
            Arc::new(fee.retain(Row {
                field: row.field,
                key: row.key.clone(),
                member: row.member,
                outer,
                child: row.child.clone(),
                members,
                active: row.active,
                aggressor,
                defender,
            })),
            work,
        )?;
    }
    target.finish(work, &mut cancelled, memory, class)?;
    Ok(target)
}
pub(super) fn readmit_work(
    source: &DiplomacyRelationsIndexV1,
) -> Result<DiplomacyIndexWorkV1, ValueTreeError> {
    // Source is private, complete and finite. This is a checked upper bound for
    // copies plus in-place heapsorts, not a refill of caller Current work.
    let count = add(
        add(source.rows.len(), source.members.len())?,
        add(source.wars.len(), 1)?,
    )?;
    let count = u64::try_from(count).map_err(|_| closure::invalid())?;
    let steps = count
        .checked_mul(count)
        .and_then(|n| n.checked_mul(128))
        .ok_or_else(closure::invalid)?;
    let mut work = DiplomacyIndexWorkV1::from_cells(
        steps,
        u64::try_from(source.max_text.max(1)).map_err(|_| closure::invalid())?,
    )?;
    work.max_rows = source.rows.len();
    work.max_members = source.max_members;
    Ok(work)
}
pub(in crate::incremental_diplomacy) fn accepted_index<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    source: &DiplomacyRelationsIndexV1,
    writes: &[oasis7_formal_protocol::incremental_execution::ExecutionWriteV1],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut DiplomacyIndexWorkV1,
    max: u64,
) -> Result<DiplomacyRelationsIndexV1, ValueTreeError> {
    ctx.check()?;
    let bytes = add(512, mul(source.rows.len(), std::mem::size_of::<HeldRow>())?)?;
    let mut index = DiplomacyRelationsIndexV1 {
        rows: Vec::with_capacity(source.rows.len()),
        members: Vec::new(),
        wars: Vec::new(),
        _fee: ctx.values.memory.reserve(bytes, ctx.values.class)?,
        _vectors: None,
        max_text: 0,
        max_members: 0,
    };
    for row in &source.rows {
        ctx.check()?;
        work.step()?;
        index.rows.push(row.clone());
    }
    for write in writes {
        ctx.check()?;
        work.step()?;
        let Some((field, key, kind)) = current::edge_kind(&write.mutation.key, write.namespace)?
        else {
            continue;
        };
        if kind != WorldFieldRecordKindV1::Entry
            || !matches!(field, DiplomacyFieldV1::Alliances | DiplomacyFieldV1::Wars)
        {
            continue;
        }
        let old = find(&source.rows, field, &key, work, &mut ctx.cancelled)?;
        let old = old.map(|i| &source.rows[i]);
        match (old, write.mutation.before) {
            (Some(row), Some(value)) if row.member == value => {}
            (None, None) => {}
            _ => return Err(closure::invalid()),
        }
        let replacement = if let Some(value) = write.mutation.after {
            Some(changed_row(
                old,
                &StateEntryV1 {
                    namespace: write.namespace,
                    key: write.mutation.key.clone(),
                    value,
                },
                ctx,
                work,
                max,
            )?)
        } else {
            if field != DiplomacyFieldV1::Alliances || old.is_none() {
                return Err(closure::invalid());
            }
            None
        };
        // Source lookup is binary; mutations of the paid output vector are
        // bounded and charged separately, including shifted Arc slots.
        let mut position = None;
        for (i, row) in index.rows.iter().enumerate() {
            ctx.check()?;
            work.step()?;
            if row.key == key {
                position = Some(i);
                break;
            }
        }
        match (position, replacement) {
            (Some(i), Some(row)) => index.rows[i] = row,
            (Some(i), None) => {
                for _ in i..index.rows.len() {
                    ctx.check()?;
                    work.step()?;
                }
                index.rows.remove(i);
            }
            (None, Some(row)) => index.push(row, work)?,
            (None, None) => return Err(closure::invalid()),
        }
    }
    index.finish(
        work,
        &mut ctx.cancelled,
        ctx.values.memory,
        ctx.values.class,
    )?;
    ctx.check()?;
    Ok(index)
}

#[cfg(test)]
#[path = "incremental_diplomacy_index_tests.rs"]
mod tests;

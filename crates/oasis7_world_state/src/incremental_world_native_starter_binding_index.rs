//! Qualified starter binding data only, never authentication or action authority.
use super::*;
use crate::incremental_world_native_gameplay::StarterOcClaimsFieldV1 as Field;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::{StateEntryV1, StateScanContext, StateScanError},
};
use std::{cell::RefCell, sync::Arc};

#[path = "incremental_world_native_starter_binding_index_build.rs"]
mod build;
#[path = "incremental_world_native_starter_binding_index_current.rs"]
mod current;
pub(crate) use build::{accept_starter_binding_index_verified_v1, build_starter_binding_index_v1};
pub use current::{StarterBindingObservationV1, observe_current_starter_binding_v1};

/// All fields are caller-owned continuous allowances; no operation refills them.
pub struct StarterBindingIndexWorkV1 {
    pub max_rows: usize,
    pub max_row_text_bytes: usize,
    pub max_pending_rows: usize,
    pub steps_remaining: u64,
    pub bytes_remaining: u64,
    pub comparison_bytes_remaining: u64,
    pub moved_slots_remaining: u64,
    pub pending_entries_remaining: usize,
}
impl StarterBindingIndexWorkV1 {
    fn spend(&mut self, bytes: usize) -> Result<(), ValueTreeError> {
        self.steps_remaining = self
            .steps_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("starter index steps"))?;
        self.bytes_remaining = self
            .bytes_remaining
            .checked_sub(u64::try_from(bytes).map_err(|_| invalid())?)
            .ok_or(ValueTreeError::ResourceLimited("starter index bytes"))?;
        Ok(())
    }
    fn comparison(&mut self, bytes: usize) -> Result<(), ValueTreeError> {
        self.comparison_bytes_remaining = self
            .comparison_bytes_remaining
            .checked_sub(u64::try_from(bytes).map_err(|_| invalid())?)
            .ok_or(ValueTreeError::ResourceLimited("starter index comparison"))?;
        Ok(())
    }
    fn moved(&mut self, slots: usize) -> Result<(), ValueTreeError> {
        self.moved_slots_remaining = self
            .moved_slots_remaining
            .checked_sub(u64::try_from(slots).map_err(|_| invalid())?)
            .ok_or(ValueTreeError::ResourceLimited("starter index moves"))?;
        Ok(())
    }
}
struct Row {
    outer: String,
    key: WorldFieldKeyV1,
    member: [u8; 32],
    player: String,
    public_key: Option<String>,
    agent: String,
}
type HeldRow = Arc<ReservedValue<Row>>;
/// Private construction requires a complete field proof and real typed records.
/// Rows preserve duplicate bindings and Some("") separately from None.
pub struct StarterBindingIndexV1 {
    roots: [u8; 32],
    class: MemoryClass,
    metadata: [u8; 32],
    by_binding: Vec<HeldRow>,
    by_outer: Vec<HeldRow>,
}
impl StarterBindingIndexV1 {
    pub fn records(&self) -> usize {
        self.by_outer.len()
    }
    /// Aggregate consistency only; pool admission remains with the held owners.
    pub(crate) fn validate_aggregate_binding_v1(
        &self,
        roots: &RootSetV1,
        class: MemoryClass,
        expected_records: u64,
    ) -> Result<(), ValueTreeError> {
        if self.class != class
            || self.roots != roots.digest().map_err(|_| invalid())?
            || u64::try_from(self.by_outer.len()).map_err(|_| invalid())? != expected_records
            || self.by_binding.len() != self.by_outer.len()
        {
            return Err(invalid());
        }
        Ok(())
    }

    fn validate(
        &self,
        roots: &RootSetV1,
        _memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<(), ValueTreeError> {
        if self.class != class
            || self.roots != roots.digest().map_err(|_| invalid())?
            || self.by_binding.len() != self.by_outer.len()
        {
            return Err(invalid());
        }
        Ok(())
    }
}
fn row_text(row: &Row) -> Result<usize, ValueTreeError> {
    row.outer
        .len()
        .checked_add(row.player.len())
        .and_then(|n| n.checked_add(row.agent.len()))
        .and_then(|n| n.checked_add(row.public_key.as_ref().map_or(0, String::len)))
        .ok_or_else(invalid)
}
fn hold_row(
    outer: &str,
    key: WorldFieldKeyV1,
    member: [u8; 32],
    claim: &crate::StarterOcClaimState,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut StarterBindingIndexWorkV1,
) -> Result<HeldRow, ValueTreeError> {
    let text = outer
        .len()
        .checked_add(claim.player_id.len())
        .and_then(|n| n.checked_add(claim.agent_id.len()))
        .and_then(|n| n.checked_add(claim.public_key.as_ref().map_or(0, String::len)))
        .ok_or_else(invalid)?;
    if text > work.max_row_text_bytes {
        return Err(ValueTreeError::ResourceLimited("starter row text"));
    }
    work.spend(text)?;
    let bytes = std::mem::size_of::<ReservedValue<Row>>()
        .checked_add(2 * std::mem::size_of::<usize>())
        .and_then(|n| n.checked_add(std::mem::align_of::<ReservedValue<Row>>()))
        .and_then(|n| n.checked_add(text))
        .ok_or_else(invalid)?;
    let fee = memory.reserve(bytes, class)?;
    Ok(Arc::new(fee.retain(Row {
        outer: outer.to_owned(),
        key,
        member,
        player: claim.player_id.clone(),
        public_key: claim.public_key.clone(),
        agent: claim.agent_id.clone(),
    })))
}
fn entry_kind(
    key: &[u8],
    namespace: StateNamespaceV1,
) -> Result<Option<WorldFieldRecordKindV1>, ValueTreeError> {
    if !key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(None);
    }
    let parsed = WorldFieldKeyV1::from_bytes(key).map_err(ValueTreeError::Invalid)?;
    if parsed.owner != FieldOwner::WorldState || parsed.name != Field::NAME {
        return Ok(None);
    }
    let expected = WorldFieldKeyV1::new(Field::descriptor()?, parsed.kind, parsed.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if expected.as_bytes() != key
        || parsed.namespace != namespace
        || parsed.namespace != Field::profile()?.namespace()?
    {
        return Err(invalid());
    }
    Ok(Some(parsed.kind))
}
fn typed_row(
    entry: &StateEntryV1,
    ctx: &mut WorldFieldReadContextV1<
        '_,
        impl MerkleNodeReader,
        impl ValueNodeReader,
        impl FnMut() -> Option<ValueTreeError>,
        impl FnMut() -> bool,
    >,
    scan: &mut StateScanBudget,
    native: &mut WorldNativeQualificationWorkV1,
    work: &mut StarterBindingIndexWorkV1,
) -> Result<HeldRow, ValueTreeError> {
    read::check(ctx)?;
    native.records_remaining = native
        .records_remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("starter typed records"))?;
    let original = family::starter_original_key_v1(entry, ctx, native.max_key_heap)?;
    let crate::incremental_semantic_key::SemanticKeyRefV1::Text(outer) =
        crate::incremental_semantic_key::read_semantic_key_v1(&original)
            .map_err(ValueTreeError::Invalid)?
    else {
        return Err(invalid());
    };
    let parsed = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let key = WorldFieldKeyV1::new(Field::descriptor()?, parsed.kind, parsed.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    let body = read::read_body_at_entry_v1::<Field, _, _, _, _>(&key, &entry.value, ctx)?;
    let profile = Field::profile()?;
    let decoded: WorldNativeRecoveredRecordV1<crate::StarterOcClaimState> =
        recover_typed_record_body_v1(&body, &profile, ctx, scan, &mut native.typed)?;
    Field::validate_record_binding(
        WorldNativeKeyRefV1::Text(outer),
        &body,
        ctx,
        scan,
        &mut native.typed,
    )?;
    read::check(ctx)?;
    hold_row(
        outer,
        key,
        entry.value,
        decoded.value().value(),
        ctx.values.memory,
        ctx.values.class,
        work,
    )
}
fn binding_cmp(
    a: &Row,
    b: &Row,
    work: &mut StarterBindingIndexWorkV1,
) -> Result<std::cmp::Ordering, ValueTreeError> {
    work.comparison(row_text(a)?.checked_add(row_text(b)?).ok_or_else(invalid)?)?;
    Ok((&a.player, &a.public_key, &a.outer).cmp(&(&b.player, &b.public_key, &b.outer)))
}
fn outer_position(
    rows: &[HeldRow],
    key: &WorldFieldKeyV1,
    work: &mut StarterBindingIndexWorkV1,
    mut cancel: impl FnMut() -> bool,
) -> Result<Result<usize, usize>, ValueTreeError> {
    let (mut lo, mut hi) = (0, rows.len());
    while lo < hi {
        if cancel() {
            return Err(ValueTreeError::Cancelled);
        }
        work.comparison(
            rows[(lo + hi) / 2]
                .key
                .as_bytes()
                .len()
                .checked_add(key.as_bytes().len())
                .ok_or_else(invalid)?,
        )?;
        let mid = (lo + hi) / 2;
        match rows[mid].key.as_bytes().cmp(key.as_bytes()) {
            std::cmp::Ordering::Less => lo = mid + 1,
            std::cmp::Ordering::Greater => hi = mid,
            std::cmp::Ordering::Equal => return Ok(Ok(mid)),
        }
    }
    Ok(Err(lo))
}
fn insert_row(
    index: &mut StarterBindingIndexV1,
    row: HeldRow,
    work: &mut StarterBindingIndexWorkV1,
    mut cancel: impl FnMut() -> bool,
) -> Result<(), ValueTreeError> {
    if index.by_outer.len() >= work.max_rows
        || index.by_outer.len() == index.by_outer.capacity()
        || index.by_binding.len() == index.by_binding.capacity()
    {
        return Err(ValueTreeError::ResourceLimited("starter index rows"));
    }
    let outer = match outer_position(&index.by_outer, &row.key, work, &mut cancel)? {
        Ok(_) => return Err(invalid()),
        Err(pos) => pos,
    };
    let (mut lo, mut hi) = (0, index.by_binding.len());
    while lo < hi {
        if cancel() {
            return Err(ValueTreeError::Cancelled);
        }
        let mid = (lo + hi) / 2;
        if binding_cmp(&index.by_binding[mid], &row, work)?.is_lt() {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    work.moved(index.by_outer.len() - outer + index.by_binding.len() - lo + 2)?;
    index.by_outer.insert(outer, row.clone());
    index.by_binding.insert(lo, row);
    Ok(())
}
fn index_fee(
    memory: &MemoryBudget,
    class: MemoryClass,
    capacity: usize,
) -> Result<oasis7_proto::memory_budget::MemoryReservation, ValueTreeError> {
    memory
        .reserve(
            capacity
                .checked_mul(2 * std::mem::size_of::<HeldRow>())
                .and_then(|n| n.checked_add(std::mem::size_of::<StarterBindingIndexV1>()))
                .ok_or_else(invalid)?,
            class,
        )
        .map_err(Into::into)
}
/// Explicit resource conversion: no Recovery row is implicitly reused in Ordinary.
pub fn readmit_starter_binding_index_ordinary_v1(
    source: &ReservedValue<StarterBindingIndexV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    work: &mut StarterBindingIndexWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<StarterBindingIndexV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let _phase = memory.reserve(32 << 10, MemoryClass::Ordinary)?;
    if !source.uses_budget(memory) {
        return Err(invalid());
    }
    source.validate(roots, memory, MemoryClass::Recovery)?;
    if source.records() > work.max_rows {
        return Err(ValueTreeError::ResourceLimited("starter index rows"));
    }
    let fee = index_fee(memory, MemoryClass::Ordinary, source.records())?;
    let mut index = StarterBindingIndexV1 {
        roots: source.roots,
        class: MemoryClass::Ordinary,
        metadata: source.metadata,
        by_binding: Vec::with_capacity(source.records()),
        by_outer: Vec::with_capacity(source.records()),
    };
    for row in &source.by_outer {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        let text = row_text(row)?;
        if text > work.max_row_text_bytes {
            return Err(ValueTreeError::ResourceLimited("starter row text"));
        }
        work.spend(text)?;
        let bytes = std::mem::size_of::<ReservedValue<Row>>()
            .checked_add(
                2 * std::mem::size_of::<usize>() + std::mem::align_of::<ReservedValue<Row>>(),
            )
            .and_then(|n| n.checked_add(text))
            .ok_or_else(invalid)?;
        let row_fee = memory.reserve(bytes, MemoryClass::Ordinary)?;
        let copy = Arc::new(row_fee.retain(Row {
            outer: row.outer.clone(),
            key: row.key.clone(),
            member: row.member,
            player: row.player.clone(),
            public_key: row.public_key.clone(),
            agent: row.agent.clone(),
        }));
        insert_row(&mut index, copy, work, &mut cancelled)?;
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(fee.retain(index))
}

/// Aggregate compatibility bridge. Limits derive only from its private charged
/// source's exact finite rows; no external size or unlimited work is accepted.
pub(crate) fn readmit_starter_binding_index_ordinary_bounded_v1(
    source: &ReservedValue<StarterBindingIndexV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<StarterBindingIndexV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let _phase = memory.reserve(32 << 10, MemoryClass::Ordinary)?;
    if !source.uses_budget(memory) {
        return Err(invalid());
    }
    source.validate(roots, memory, MemoryClass::Recovery)?;
    let n = source.records();
    let mut bytes = 0usize;
    let mut maximum = 0usize;
    for row in &source.by_outer {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        let text = row_text(row)?;
        bytes = bytes.checked_add(text).ok_or_else(invalid)?;
        maximum = maximum.max(text);
    }
    let squared = n.checked_mul(n).ok_or_else(invalid)?;
    let comparisons = squared
        .checked_mul(
            maximum
                .checked_mul(2)
                .and_then(|v| {
                    v.checked_add(2 * crate::incremental_world_field_key::WORLD_FIELD_KEY_MAX_BYTES)
                })
                .ok_or_else(invalid)?,
        )
        .ok_or_else(invalid)?;
    let moves = squared
        .checked_mul(2)
        .and_then(|v| v.checked_add(n.checked_mul(2)?))
        .ok_or_else(invalid)?;
    let mut work = StarterBindingIndexWorkV1 {
        max_rows: n,
        max_row_text_bytes: maximum,
        max_pending_rows: 0,
        steps_remaining: u64::try_from(n).map_err(|_| invalid())?,
        bytes_remaining: u64::try_from(bytes).map_err(|_| invalid())?,
        comparison_bytes_remaining: u64::try_from(comparisons).map_err(|_| invalid())?,
        moved_slots_remaining: u64::try_from(moves).map_err(|_| invalid())?,
        pending_entries_remaining: 0,
    };
    readmit_starter_binding_index_ordinary_v1(source, roots, memory, &mut work, &mut cancelled)
}

#[cfg(test)]
#[path = "incremental_world_native_starter_binding_index_tests.rs"]
mod tests;

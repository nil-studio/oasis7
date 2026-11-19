//! Actual original GEN/cold field visitor: no World/WorldState serialization.
//! Original semantic-key bytes and typed JSON field order are kept together;
//! ordinary transitions must bind real TX writes before reusing this DATA.
use super::*;
use oasis7_world_state::observation_projection::current_commitment::{
    ProjectionColumnDataV1, ProjectionColumnErrorV1,
};
use oasis7_world_state::{
    incremental_projection::{FieldDestination, FieldShape},
    incremental_semantic_key::{SemanticKeyV1, write_semantic_key_v1},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Write},
    mem::size_of,
};
pub(in crate::world) struct ProjectionOriginalColumnRowV1 {
    /// Exact canonical original key, never reconstructed from its digest.
    semantic: ReservedValue<Vec<u8>>,
    key_json: ReservedValue<Vec<u8>>,
    /// Original typed serde JSON fragment, not sorted JSON/canonical CBOR.
    json: ReservedValue<Vec<u8>>,
    value: ProjectionColumnDataV1,
    agent_electricity: Option<i64>,
    agent_intent: Option<ReservedValue<Vec<u8>>>,
    _fee: MemoryReservation,
}
impl ProjectionOriginalColumnRowV1 {
    pub(in crate::world) fn semantic(&self) -> &[u8] {
        &self.semantic
    }
    pub(in crate::world) fn original_key_json(&self) -> &[u8] {
        &self.key_json
    }
    pub(in crate::world) fn original_json(&self) -> &[u8] {
        &self.json
    }
    pub(in crate::world) fn value(&self) -> &ProjectionColumnDataV1 {
        &self.value
    }
    pub(super) fn agent_intent_v1(&self)->Option<&[u8]>{self.agent_intent.as_deref().map(Vec::as_slice)}
    pub(in crate::world) fn agent_electricity(&self) -> Option<i64> {
        self.agent_electricity
    }
}
pub(in crate::world) struct ProjectionOriginalColumnGroupV1 {
    semantic: ReservedValue<Vec<u8>>,
    key_json: ReservedValue<Vec<u8>>,
    entries: usize,
}
impl ProjectionOriginalColumnGroupV1 {
    pub(in crate::world) fn semantic(&self) -> &[u8] {
        &self.semantic
    }
    pub(in crate::world) fn original_key_json(&self) -> &[u8] {
        &self.key_json
    }
    pub(in crate::world) fn entries(&self) -> usize {
        self.entries
    }
}
pub(in crate::world) struct ProjectionOriginalColumnFieldV1 {
    descriptor: FieldDescriptor,
    declared: Option<usize>,
    groups: Vec<ProjectionOriginalColumnGroupV1>,
    rows: Vec<ProjectionOriginalColumnRowV1>,
    _group_slots: MemoryReservation,
    _row_slots: MemoryReservation,
    _fee: MemoryReservation,
}
impl ProjectionOriginalColumnFieldV1 {
    pub(in crate::world) fn descriptor(&self) -> FieldDescriptor {
        self.descriptor
    }
    pub(in crate::world) fn declared(&self) -> Option<usize> {
        self.declared
    }
    pub(in crate::world) fn groups(&self) -> &[ProjectionOriginalColumnGroupV1] {
        &self.groups
    }
    pub(in crate::world) fn rows(&self) -> &[ProjectionOriginalColumnRowV1] {
        &self.rows
    }
}
/// Only original source DATA. A separate actual qualified-source initializer
/// must bind every descriptor/count/semantic key/fragment to the accepted root.
pub(in crate::world) struct NormalProjectionColumnSeedDataV1 {
    auxiliary: Option<ProjectionColumnDataV1>,
    marketplace_index: Option<std::sync::Arc<super::market_index::ProjectionMarketplaceIndexSeedDataV1>>,
    world_rule_rows: Option<std::sync::Arc<super::world_rule_rows::OriginalWorldRuleRowsDataV1>>,
    state_slots: Vec<emission::ProjectionOriginalStateSlotDataV1>,
    _state_slot_capacity: MemoryReservation,
    emitted: Vec<emission::ProjectionOriginalStateEmissionV1>,
    emitted_count: Option<usize>,
    _emitted_slots: MemoryReservation,
    fields: BTreeMap<(FieldOwner, &'static str), ProjectionOriginalColumnFieldV1>,
    memory: MemoryBudget,
    core: WorkIdentityV1,
    _fees: Vec<MemoryReservation>,
    _slots: MemoryReservation,
    _fee: MemoryReservation,
}
impl NormalProjectionColumnSeedDataV1 {
    pub(super) fn marketplace_index_v1(&self, common: &mut FormalBoundedContextV1<'_>) -> Result<std::sync::Arc<super::market_index::ProjectionMarketplaceIndexSeedDataV1>,BodyMutationError> { self.validate_context(common)?; let value=self.marketplace_index.as_ref().ok_or_else(||invalid("actual marketplace internal index source missing"))?; value.validate_context(common)?; common.consume_steps(1).map_err(paid)?; Ok(std::sync::Arc::clone(value)) }
    pub(super) fn world_rule_rows_v1(
        &self,
    ) -> Option<&std::sync::Arc<super::world_rule_rows::OriginalWorldRuleRowsDataV1>> {
        self.world_rule_rows.as_ref()
    }
    pub(in crate::world) fn fields(
        &self,
    ) -> impl ExactSizeIterator<Item = &ProjectionOriginalColumnFieldV1> {
        self.fields.values()
    }
    pub(in crate::world) fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common.consume_steps(1).map_err(paid)?;
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self._fee.uses_budget(common.memory())
        {
            return Err(invalid("projection column seed context"));
        }
        Ok(())
    }
}
pub(in crate::world) struct NormalProjectionColumnInitializerV1<'a, 'b> {
    common: &'a mut FormalBoundedContextV1<'b>,
    auxiliary: Option<ProjectionColumnDataV1>,
    marketplace_index: Option<std::sync::Arc<super::market_index::ProjectionMarketplaceIndexSeedDataV1>>,
    world_rule_rows: Option<std::sync::Arc<super::world_rule_rows::OriginalWorldRuleRowsDataV1>>,
    state_slots: Vec<emission::ProjectionOriginalStateSlotDataV1>,
    state_slot_capacity: MemoryReservation,
    emitted: Vec<emission::ProjectionOriginalStateEmissionV1>,
    emitted_count: Option<usize>,
    emitted_slots: MemoryReservation,
    fields: BTreeMap<(FieldOwner, &'static str), ProjectionOriginalColumnFieldV1>,
    active: BTreeSet<(FieldOwner, &'static str)>,
    fees: Vec<MemoryReservation>,
    slots: MemoryReservation,
    fee: MemoryReservation,
}
fn reserve(
    common: &mut FormalBoundedContextV1<'_>,
    n: usize,
) -> Result<MemoryReservation, BodyMutationError> {
    common.charge(1, n as u64).map_err(paid)?;
    common
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(|e| ValueTreeError::Memory(e).into())
}
fn grow<T>(
    values: &mut Vec<T>,
    slots: &mut MemoryReservation,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    if values.len() < values.capacity() {
        return Ok(());
    }
    let cap = values
        .capacity()
        .checked_mul(2)
        .unwrap_or(usize::MAX)
        .max(4);
    let bytes = cap
        .checked_mul(size_of::<T>())
        .ok_or_else(|| invalid("projection column vector layout"))?;
    let fee = reserve(common, bytes)?;
    values
        .try_reserve_exact(cap - values.len())
        .map_err(|_| invalid("projection column vector allocation"))?;
    if values.capacity() != cap {
        return Err(invalid("projection column vector capacity"));
    }
    *slots = fee;
    Ok(())
}
pub(super) struct Output<'a, 'b> {
    common: &'a mut FormalBoundedContextV1<'b>,
    bytes: Option<Vec<u8>>,
    count: usize,
    bound: usize,
    first: Option<BodyMutationError>,
}
impl Write for Output<'_, '_> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.first.is_some() {
            return Err(io::ErrorKind::Other.into());
        }
        if let Err(e) = self.common.charge(1, b.len() as u64) {
            self.first = Some(paid(e));
            return Err(io::ErrorKind::Other.into());
        }
        let Some(n) = self.count.checked_add(b.len()).filter(|n| *n <= self.bound) else {
            self.first = Some(invalid("projection original output bound"));
            return Err(io::ErrorKind::Other.into());
        };
        if let Some(bytes) = &mut self.bytes {
            bytes.extend_from_slice(b)
        }
        self.count = n;
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(super) fn capture(
    common: &mut FormalBoundedContextV1<'_>,
    encode: impl FnMut(&mut Output<'_, '_>) -> Result<(), BodyMutationError>,
) -> Result<ReservedValue<Vec<u8>>, BodyMutationError> {
    capture_bounded_v1(common, usize::MAX, encode)
}
pub(super) fn capture_bounded_v1(
    common: &mut FormalBoundedContextV1<'_>,
    max_output_bytes: usize,
    mut encode: impl FnMut(&mut Output<'_, '_>) -> Result<(), BodyMutationError>,
) -> Result<ReservedValue<Vec<u8>>, BodyMutationError> {
    let mut count = Output {
        common,
        bytes: None,
        count: 0,
        bound: max_output_bytes,
        first: None,
    };
    let result = encode(&mut count);
    if let Some(e) = count.first {
        return Err(e);
    }
    result?;
    let n = count.count;
    let fee = reserve(
        count.common,
        size_of::<Vec<u8>>()
            .checked_add(n)
            .ok_or_else(|| invalid("projection original layout"))?,
    )?;
    let bytes = Vec::with_capacity(n);
    if bytes.capacity() != n {
        return Err(invalid("projection original capacity"));
    }
    let mut output = Output {
        common: count.common,
        bytes: Some(bytes),
        count: 0,
        bound: n,
        first: None,
    };
    let result = encode(&mut output);
    if let Some(e) = output.first {
        return Err(e);
    }
    result?;
    if output.count != n {
        return Err(invalid("projection original count changed"));
    }
    output.common.consume_steps(0).map_err(paid)?;
    Ok(fee.retain(
        output
            .bytes
            .ok_or_else(|| invalid("projection original output"))?,
    ))
}
impl<'a, 'b> NormalProjectionColumnInitializerV1<'a, 'b> {
    pub(in crate::world) fn new(
        common: &'a mut FormalBoundedContextV1<'b>,
    ) -> Result<Self, BodyMutationError> {
        let fee = reserve(
            common,
            size_of::<Self>().max(size_of::<NormalProjectionColumnSeedDataV1>()),
        )?;
        let slots = common
            .memory()
            .reserve(0, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let emitted_slots = common
            .memory()
            .reserve(0, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let state_slot_capacity = common
            .memory()
            .reserve(0, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(Self {
            common,
            auxiliary: None,
            marketplace_index: None,
            world_rule_rows: None,
            state_slots: Vec::new(),
            state_slot_capacity,
            emitted: Vec::new(),
            emitted_count: None,
            emitted_slots,
            fields: BTreeMap::new(),
            active: BTreeSet::new(),
            fees: Vec::new(),
            slots,
            fee,
        })
    }
    fn keep(&mut self, fee: MemoryReservation) -> Result<(), BodyMutationError> {
        grow(&mut self.fees, &mut self.slots, self.common)?;
        self.fees.push(fee);
        Ok(())
    }
    fn needed(field: FieldDescriptor) -> bool {
        match field.owner {
            FieldOwner::WorldState | FieldOwner::Cognition => {
                !matches!(field.destination, FieldDestination::Process)
            }
            FieldOwner::Runtime => {
                runtime_projection_links_v1().any(|link| {
                    field.name == link.runtime_field
                        || field
                            .name
                            .strip_prefix(link.runtime_field)
                            .is_some_and(|s| s.starts_with('.'))
                }) || field.name == "journal"
                    || field.name == "next_event_id"
                    || field.name == "next_event_id_era"
            }
        }
    }
    fn key<K: SemanticKeyV1 + ?Sized>(
        &mut self,
        key: &K,
    ) -> Result<ReservedValue<Vec<u8>>, BodyMutationError> {
        capture(self.common, |output| {
            write_semantic_key_v1(key, output).map_err(paid)
        })
    }
    fn row<K: SemanticKeyV1 + ?Sized, V: serde::Serialize + ?Sized>(
        &mut self,
        field: FieldDescriptor,
        key: &K,
        value: &V,
    ) -> Result<(), BodyMutationError> {
        self.common.consume_steps(1).map_err(paid)?;
        if !self.active.contains(&(field.owner, field.name)) {
            return Err(invalid("projection column field framing"));
        }
        if !Self::needed(field) {
            return Ok(());
        }
        let semantic = self.key(key)?;
        let key_json = capture(self.common, |output| {
            serde_json::to_writer(output, key).map_err(paid)
        })?;
        let json = capture(self.common, |output| {
            serde_json::to_writer(output, value).map_err(paid)
        })?;
        self.append_original_encoded_row_v1(field,semantic,key_json,json)
    }

    pub(in crate::world) fn finish(
        self,
    ) -> Result<NormalProjectionColumnSeedDataV1, BodyMutationError> {
        if !self.active.is_empty() {
            return Err(invalid("unfinished projection column"));
        }
        self.common.consume_steps(0).map_err(paid)?;
        Ok(NormalProjectionColumnSeedDataV1 {
            auxiliary: self.auxiliary,
            marketplace_index: self.marketplace_index,
            world_rule_rows: self.world_rule_rows,
            state_slots: self.state_slots,
            _state_slot_capacity: self.state_slot_capacity,
            emitted: self.emitted,
            emitted_count: self.emitted_count,
            _emitted_slots: self.emitted_slots,
            fields: self.fields,
            memory: self.common.memory().clone(),
            core: self.common.work().identity(),
            _fees: self.fees,
            _slots: self.slots,
            _fee: self.fee,
        })
    }
}
impl GenesisFieldVisitor for NormalProjectionColumnInitializerV1<'_, '_> {
    type Error = BodyMutationError;
    fn begin(&mut self, field: FieldDescriptor, entries: Option<usize>) -> Result<(), Self::Error> {
        self.common.consume_steps(1).map_err(paid)?;
        if self.active.contains(&(field.owner, field.name)) {
            return Err(invalid("duplicate open projection column"));
        }
        let open_fee = reserve(
            self.common,
            2 * (11 * size_of::<(FieldOwner, &str)>() + 128),
        )?;
        self.keep(open_fee)?;
        self.active.insert((field.owner, field.name));
        if !Self::needed(field) {
            return Ok(());
        }
        if self.fields.contains_key(&(field.owner, field.name)) {
            return Err(invalid("duplicate projection column"));
        }
        let fee = reserve(
            self.common,
            size_of::<ProjectionOriginalColumnFieldV1>()
                + 2 * (11 * size_of::<((FieldOwner, &str), ProjectionOriginalColumnFieldV1)>()
                    + 256),
        )?;
        let group_slots = reserve(self.common, 0)?;
        let row_slots = reserve(self.common, 0)?;
        self.fields.insert(
            (field.owner, field.name),
            ProjectionOriginalColumnFieldV1 {
                descriptor: field,
                declared: entries,
                groups: Vec::new(),
                rows: Vec::new(),
                _group_slots: group_slots,
                _row_slots: row_slots,
                _fee: fee,
            },
        );
        Ok(())
    }
    fn group<K: SemanticKeyV1 + ?Sized>(
        &mut self,
        field: FieldDescriptor,
        key: &K,
        entries: usize,
    ) -> Result<(), Self::Error> {
        self.common.consume_steps(1).map_err(paid)?;
        if !self.active.contains(&(field.owner, field.name)) {
            return Err(invalid("projection group framing"));
        }
        if !Self::needed(field) {
            return Ok(());
        }
        let semantic = self.key(key)?;
        let key_json = capture(self.common, |output| {
            serde_json::to_writer(output, key).map_err(paid)
        })?;
        let target = self
            .fields
            .get_mut(&(field.owner, field.name))
            .ok_or_else(|| invalid("projection group column"))?;
        grow(&mut target.groups, &mut target._group_slots, self.common)?;
        target.groups.push(ProjectionOriginalColumnGroupV1 {
            semantic,
            key_json,
            entries,
        });
        Ok(())
    }
    fn entry<K: SemanticKeyV1 + ?Sized, V: serde::Serialize + ?Sized>(
        &mut self,
        field: FieldDescriptor,
        key: &K,
        value: &V,
    ) -> Result<(), Self::Error> {
        self.row(field, key, value)
    }
    fn bytes<K: SemanticKeyV1 + ?Sized>(
        &mut self,
        field: FieldDescriptor,
        key: &K,
        value: &[u8],
    ) -> Result<(), Self::Error> {
        self.row(field, key, value)
    }
    fn side<T: serde::Serialize + ?Sized>(
        &mut self,
        field: FieldDescriptor,
        value: &T,
        _entries: Option<u64>,
    ) -> Result<(), Self::Error> {
        self.common.consume_steps(1).map_err(paid)?;
        if !Self::needed(field) {
            return Ok(());
        }
        self.begin(field, Some(1))?;
        self.row(field, &(), value)?;
        self.end(field)
    }
    fn process(&mut self, _: FieldDescriptor) -> Result<(), Self::Error> {
        self.common.consume_steps(1).map_err(paid)
    }
    fn end(&mut self, field: FieldDescriptor) -> Result<(), Self::Error> {
        self.common.consume_steps(1).map_err(paid)?;
        if !self.active.remove(&(field.owner, field.name)) {
            return Err(invalid("projection column end"));
        }
        Ok(())
    }
}

/// Original typed-map ordering over the closed semantic-key grammar. Raw key
/// encodings include text lengths, so byte-order is deliberately not used.
/// This is DATA ordering only; callers still bind actual member originals.
pub(in crate::world) fn compare_projection_original_keys_v1(
    left: &[u8],
    right: &[u8],
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<std::cmp::Ordering, BodyMutationError> {
    use oasis7_world_state::incremental_semantic_key::{
        LedgerKeyRefV1 as L, SemanticKeyRefV1 as K, read_semantic_key_v1,
    };
    use std::cmp::Ordering;
    let length = left
        .len()
        .checked_add(right.len())
        .ok_or_else(|| invalid("projection semantic comparison layout"))?;
    // Recursive pair decoding revalidates bounded borrowed child tokens. Account
    // its complete worst-case depth before any UTF8/parser/string comparison.
    let factor = 2 * oasis7_world_state::incremental_semantic_key::SEMANTIC_KEY_MAX_PAIR_DEPTH + 2;
    common
        .charge(
            factor as u64,
            (length as u64)
                .checked_mul(factor as u64)
                .ok_or_else(|| invalid("projection semantic comparison work"))?,
        )
        .map_err(paid)?;
    fn ledger<'a>(value: L<'a>) -> (u8, &'a str) {
        match value {
            L::World => (0, ""),
            L::Agent(v) => (1, v),
            L::Site(v) => (2, v),
            L::Factory(v) => (3, v),
        }
    }
    fn compare(left: K<'_>, right: K<'_>) -> Result<Ordering, BodyMutationError> {
        Ok(match (left, right) {
            (K::Unit, K::Unit) => Ordering::Equal,
            (K::Text(a), K::Text(b)) => a.cmp(b),
            (K::Unsigned(a), K::Unsigned(b)) => a.cmp(&b),
            (K::Resource(a), K::Resource(b)) => a.cmp(&b),
            (K::Ledger(a), K::Ledger(b)) => ledger(a).cmp(&ledger(b)),
            (K::Pair { left: a, right: b }, K::Pair { left: c, right: d }) => {
                let first = compare(a.decode().map_err(invalid)?, c.decode().map_err(invalid)?)?;
                if first == Ordering::Equal {
                    compare(b.decode().map_err(invalid)?, d.decode().map_err(invalid)?)?
                } else {
                    first
                }
            }
            _ => return Err(invalid("projection semantic key type changed")),
        })
    }
    compare(
        read_semantic_key_v1(left).map_err(invalid)?,
        read_semantic_key_v1(right).map_err(invalid)?,
    )
}

#[path = "incremental_normal_projection_commitment_emission.rs"]
mod emission;

#[path = "incremental_normal_projection_commitment_initial_dedicated.rs"]
mod dedicated;

#[path = "incremental_normal_projection_commitment_initial_auxiliary.rs"]
mod auxiliary;

#[path="incremental_normal_projection_commitment_initial_market.rs"] mod market;

#[path="incremental_normal_projection_commitment_initial_cold_rows.rs"] mod cold_rows;

#[path="incremental_normal_projection_commitment_initial_cold_base.rs"]mod cold_base;

#[path="incremental_normal_projection_commitment_initial_cold_records.rs"] mod cold_records;

//! Bounded data-only index for the fixed normal WorldRules lifecycle selector.
//!
//! Rows are derived from the existing typed WorldRules sources.  They are not
//! business authority and never replace selected-record Current validation.
use crate::{
    incremental_field_member::{FieldOwner, MemberRecordLimitsV1, read_member_record_key_v1},
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_semantic_key::{SemanticKeyRefV1, read_semantic_key_v1},
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
    incremental_world_rule_composite_family::WorldRuleCompositeQualificationV1,
    incremental_world_rule_effects::WorldRuleRemainderQualificationV1,
    incremental_world_rule_parent::descriptor as world_rules_descriptor,
    incremental_world_rule_shipments::{WorldRuleShipmentValueV1, read_world_rule_shipment_v1},
    incremental_world_rules_family::{SupplyFamilyQualificationV1, WorldRulesFamilyWorkV1},
};
use oasis7_formal_protocol::{
    incremental_execution::{RootSetV1, TransitionRecordV1},
    incremental_state::{
        MerkleNodeReader, StateEntryV1, StateNamespaceV1, StateRootV1, StateScanBudget,
        StateScanContext, StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{
    collections::BTreeMap,
    io::{self, Write},
    mem::{align_of, size_of},
    sync::Arc,
};

fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("world rule lifecycle directory")
}
fn limited() -> ValueTreeError {
    ValueTreeError::ResourceLimited("world rule lifecycle directory")
}
fn add(a: usize, b: usize) -> Result<usize, ValueTreeError> {
    a.checked_add(b).ok_or_else(limited)
}
fn string_bytes(s: &str) -> usize {
    s.len()
}
fn row_fee<T>(bytes: usize) -> Result<usize, ValueTreeError> {
    add(
        size_of::<T>(),
        add(
            bytes,
            size_of::<Arc<ReservedValue<T>>>() + 2 * size_of::<usize>() + align_of::<T>(),
        )?,
    )
}

struct PaidTextWriter {
    bytes: Vec<u8>,
    fee: MemoryReservation,
    memory: MemoryBudget,
    class: MemoryClass,
    max_bytes: usize,
}
impl PaidTextWriter {
    fn new(
        memory: &MemoryBudget,
        class: MemoryClass,
        max_bytes: usize,
    ) -> Result<Self, ValueTreeError> {
        let fee = memory.reserve(0, class).map_err(ValueTreeError::Memory)?;
        Ok(Self {
            bytes: Vec::new(),
            fee,
            memory: memory.clone(),
            class,
            max_bytes,
        })
    }
    fn into_text(self) -> Result<(String, MemoryReservation), ValueTreeError> {
        let text = String::from_utf8(self.bytes).map_err(|_| invalid())?;
        if text.capacity() != self.fee.bytes() {
            return Err(invalid());
        }
        Ok((text, self.fee))
    }
}
impl Write for PaidTextWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let next_len = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .filter(|n| *n <= self.max_bytes)
            .ok_or_else(|| io::Error::other("WorldRules shipment output bound"))?;
        if next_len > self.bytes.capacity() {
            let delta = next_len - self.bytes.capacity();
            let mut fee = self
                .memory
                .reserve(delta, self.class)
                .map_err(|_| io::Error::other("WorldRules shipment output budget"))?;
            self.bytes
                .try_reserve_exact(next_len - self.bytes.len())
                .map_err(|_| io::Error::other("WorldRules shipment output allocation"))?;
            if self.bytes.capacity() != next_len {
                return Err(io::Error::other("WorldRules shipment output capacity"));
            }
            self.fee.merge(&mut fee).map_err(io::Error::other)?;
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn shipment_key(semantic: &[u8]) -> Result<Option<(u64, bool)>, ValueTreeError> {
    let decoded = read_semantic_key_v1(semantic).map_err(ValueTreeError::Invalid)?;
    let SemanticKeyRefV1::Pair { left, right } = decoded else {
        return Ok(None);
    };
    let (branch, id, column) = match left.decode().map_err(ValueTreeError::Invalid)? {
        SemanticKeyRefV1::Text(branch) => (
            branch,
            right.decode().map_err(ValueTreeError::Invalid)?,
            None,
        ),
        SemanticKeyRefV1::Pair { left, right: id } => {
            let SemanticKeyRefV1::Text(branch) = left.decode().map_err(ValueTreeError::Invalid)?
            else {
                return Ok(None);
            };
            let SemanticKeyRefV1::Text(column) = right.decode().map_err(ValueTreeError::Invalid)?
            else {
                return Ok(None);
            };
            (
                branch,
                id.decode().map_err(ValueTreeError::Invalid)?,
                Some(column),
            )
        }
        _ => return Ok(None),
    };
    if branch != "supply_shipments" {
        return Ok(None);
    }
    let SemanticKeyRefV1::Unsigned(id) = id else {
        return Err(invalid());
    };
    let marker = match column {
        None => true,
        Some(
            "authorization_id" | "batch_id" | "beneficiary_id" | "lineage_id" | "input_amount"
            | "delivered_amount" | "settled",
        ) => false,
        _ => return Err(invalid()),
    };
    Ok(Some((id, marker)))
}

fn read_shipment_texts<M, R, E, C>(
    id: u64,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_payload: u64,
) -> Result<Option<(crate::world_rules::SupplyShipmentV1, [MemoryReservation; 4])>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let max = usize::try_from(max_payload).map_err(|_| limited())?;
    let mut a = PaidTextWriter::new(ctx.values.memory, ctx.values.class, max)?;
    let mut b = PaidTextWriter::new(ctx.values.memory, ctx.values.class, max)?;
    let mut c = PaidTextWriter::new(ctx.values.memory, ctx.values.class, max)?;
    let mut d = PaidTextWriter::new(ctx.values.memory, ctx.values.class, max)?;
    let value = {
        let mut outputs: [&mut dyn Write; 4] = [&mut a, &mut b, &mut c, &mut d];
        read_world_rule_shipment_v1(id, ctx, max_payload, &mut outputs)?
    };
    let (identity_utf8_bytes, input_amount, delivered_amount, settled) = match value {
        WorldRuleShipmentValueV1::RulesAbsent | WorldRuleShipmentValueV1::ShipmentAbsent => {
            return Ok(None);
        }
        WorldRuleShipmentValueV1::Value {
            identity_utf8_bytes,
            input_amount,
            delivered_amount,
            settled,
        } => (identity_utf8_bytes, input_amount, delivered_amount, settled),
    };
    let (authorization_id, fee_a) = a.into_text()?;
    let (batch_id, fee_b) = b.into_text()?;
    let (beneficiary_id, fee_c) = c.into_text()?;
    let (lineage_id, fee_d) = d.into_text()?;
    let actual = [
        authorization_id.len(),
        batch_id.len(),
        beneficiary_id.len(),
        lineage_id.len(),
    ];
    if actual != identity_utf8_bytes.map(|n| usize::try_from(n).unwrap_or(usize::MAX)) {
        return Err(invalid());
    }
    Ok(Some((
        crate::world_rules::SupplyShipmentV1 {
            authorization_id,
            batch_id,
            beneficiary_id,
            lineage_id,
            input_amount,
            delivered_amount,
            settled,
        },
        [fee_a, fee_b, fee_c, fee_d],
    )))
}

/// Parent outer-key identity and selected fields. `id` is the embedded source
/// value and intentionally remains distinct from `outer_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleSupplyAuthorizationV1 {
    pub outer_id: String,
    pub id: String,
    pub provider_id: String,
    pub active: bool,
}
impl LifecycleSupplyAuthorizationV1 {
    fn bytes(&self) -> Result<usize, ValueTreeError> {
        add(
            string_bytes(&self.outer_id),
            add(string_bytes(&self.id), string_bytes(&self.provider_id))?,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleSupplyBatchV1 {
    pub outer_id: String,
    pub authorization_id: String,
    pub allocated: bool,
    /// Actual allocation entries retained in their original BTree order for
    /// capacity and per-beneficiary work. It is metadata, not an accepted row.
    pub allocation: BTreeMap<String, i64>,
}
impl LifecycleSupplyBatchV1 {
    fn bytes(&self) -> Result<usize, ValueTreeError> {
        let strings = add(
            string_bytes(&self.outer_id),
            string_bytes(&self.authorization_id),
        )?;
        self.allocation.iter().try_fold(strings, |total, (key, _)| {
            let node = size_of::<String>()
                .checked_add(size_of::<i64>())
                .and_then(|n| n.checked_add(5 * size_of::<usize>() + align_of::<String>()))
                .and_then(|n| n.checked_add(key.capacity()))
                .ok_or_else(limited)?;
            add(total, node)
        })
    }
}

/// Exact original shipment DTO, retaining all four text identities, amounts,
/// and settled bit for later selected Current comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleSupplyShipmentV1 {
    pub outer_id: u64,
    pub value: crate::world_rules::SupplyShipmentV1,
}
impl LifecycleSupplyShipmentV1 {
    fn bytes(&self) -> Result<usize, ValueTreeError> {
        let v = &self.value;
        [
            v.authorization_id.capacity(),
            v.batch_id.capacity(),
            v.beneficiary_id.capacity(),
            v.lineage_id.capacity(),
        ]
        .into_iter()
        .try_fold(0usize, |n, m| add(n, m))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleMinimumServiceV1 {
    pub outer_id: String,
    pub operator_agent_id: String,
    pub region_id: String,
    pub registered_at: u64,
}
impl LifecycleMinimumServiceV1 {
    fn bytes(&self) -> Result<usize, ValueTreeError> {
        add(
            string_bytes(&self.outer_id),
            add(
                string_bytes(&self.operator_agent_id),
                string_bytes(&self.region_id),
            )?,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleEmergencyAuthorizationV1 {
    pub outer_id: String,
    pub id: String,
    pub provider_id: String,
    pub region_id: String,
    pub active: bool,
    pub starts_at: u64,
    pub expires_at: u64,
}
impl LifecycleEmergencyAuthorizationV1 {
    fn bytes(&self) -> Result<usize, ValueTreeError> {
        add(
            string_bytes(&self.outer_id),
            add(
                string_bytes(&self.id),
                add(
                    string_bytes(&self.provider_id),
                    string_bytes(&self.region_id),
                )?,
            )?,
        )
    }
}

/// Selector metadata for every real Emergency batch. The full row remains
/// selected through `read_current_emergency_batch_v1`; this projection keeps
/// original outer-key order and does not filter committed batches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleEmergencyBatchV1 {
    pub outer_id: String,
    pub authorization_id: String,
    pub committed_at: Option<u64>,
}
impl LifecycleEmergencyBatchV1 {
    fn bytes(&self) -> Result<usize, ValueTreeError> {
        add(
            string_bytes(&self.outer_id),
            string_bytes(&self.authorization_id),
        )
    }
}

/// Immutable sorted projections retained by the genuine WorldRules
/// qualification. Each row has its own fee, so incremental transitions can
/// share unchanged rows and replace only actual changed entries.
pub struct WorldRuleLifecycleIndexV1 {
    roots: [u8; 32],
    pub(super) supply_authorizations: Vec<Arc<ReservedValue<LifecycleSupplyAuthorizationV1>>>,
    pub(super) supply_batches: Vec<Arc<ReservedValue<LifecycleSupplyBatchV1>>>,
    pub(super) supply_shipments: Vec<Arc<ReservedValue<LifecycleSupplyShipmentV1>>>,
    pub(super) minimum_services: Vec<Arc<ReservedValue<LifecycleMinimumServiceV1>>>,
    pub(super) emergency_authorizations: Vec<Arc<ReservedValue<LifecycleEmergencyAuthorizationV1>>>,
    pub(super) emergency_batches: Vec<Arc<ReservedValue<LifecycleEmergencyBatchV1>>>,
    /// All actual Emergency observation outer keys, in original map order.
    pub(super) observation_keys: Vec<Arc<ReservedValue<String>>>,
    _container_fee: MemoryReservation,
    _authorization_slots_fee: MemoryReservation,
    _batch_slots_fee: MemoryReservation,
    _shipment_slots_fee: MemoryReservation,
    _service_slots_fee: MemoryReservation,
    _emergency_authorization_slots_fee: MemoryReservation,
    _emergency_batch_slots_fee: MemoryReservation,
    _observation_slots_fee: MemoryReservation,
    memory: MemoryBudget,
    class: MemoryClass,
}

impl WorldRuleLifecycleIndexV1 {
    fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        if self.roots != roots.digest().map_err(|_| invalid())? {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn supply_authorizations(
        &self,
    ) -> impl ExactSizeIterator<Item = &LifecycleSupplyAuthorizationV1> {
        self.supply_authorizations.iter().map(|row| &***row)
    }
    pub fn supply_batches(&self) -> impl ExactSizeIterator<Item = &LifecycleSupplyBatchV1> {
        self.supply_batches.iter().map(|row| &***row)
    }
    pub fn supply_shipments(&self) -> impl ExactSizeIterator<Item = &LifecycleSupplyShipmentV1> {
        self.supply_shipments.iter().map(|row| &***row)
    }
    pub fn minimum_services(&self) -> impl ExactSizeIterator<Item = &LifecycleMinimumServiceV1> {
        self.minimum_services.iter().map(|row| &***row)
    }
    pub fn emergency_authorizations(
        &self,
    ) -> impl ExactSizeIterator<Item = &LifecycleEmergencyAuthorizationV1> {
        self.emergency_authorizations.iter().map(|row| &***row)
    }
    pub fn emergency_batches(&self) -> impl ExactSizeIterator<Item = &LifecycleEmergencyBatchV1> {
        self.emergency_batches.iter().map(|row| &***row)
    }
    pub fn observation_key_present(&self, key: &str) -> bool {
        self.observation_keys
            .binary_search_by(|row| row.as_str().cmp(key))
            .is_ok()
    }
    pub fn check_budget(
        &self,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<(), oasis7_formal_protocol::incremental_value::ValueTreeError> {
        if self.class != class || !self.memory.same_pool(memory) {
            return Err(invalid());
        }
        if self
            .supply_authorizations
            .iter()
            .any(|r| !r.uses_budget(memory))
            || self.supply_batches.iter().any(|r| !r.uses_budget(memory))
            || self.supply_shipments.iter().any(|r| !r.uses_budget(memory))
            || self.minimum_services.iter().any(|r| !r.uses_budget(memory))
            || self
                .emergency_authorizations
                .iter()
                .any(|r| !r.uses_budget(memory))
            || self
                .emergency_batches
                .iter()
                .any(|r| !r.uses_budget(memory))
            || self.observation_keys.iter().any(|r| !r.uses_budget(memory))
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub(super) fn share(
        &self,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Self, ValueTreeError> {
        self.check_budget(memory, self.class)?;
        if class != self.class {
            return Err(invalid());
        }
        let slot = size_of::<Arc<ReservedValue<()>>>();
        let slots = [
            self.supply_authorizations.len(),
            self.supply_batches.len(),
            self.supply_shipments.len(),
            self.minimum_services.len(),
            self.emergency_authorizations.len(),
            self.observation_keys.len(),
            self.emergency_batches.len(),
        ]
        .into_iter()
        .try_fold(0usize, |sum, n| {
            add(sum, n.checked_mul(slot).ok_or_else(limited)?)
        })?;
        let fee = memory
            .reserve(add(size_of::<Self>(), add(slots, 2048)?)?, class)
            .map_err(ValueTreeError::Memory)?;
        Ok(Self {
            roots: self.roots,
            supply_authorizations: self.supply_authorizations.clone(),
            supply_batches: self.supply_batches.clone(),
            supply_shipments: self.supply_shipments.clone(),
            minimum_services: self.minimum_services.clone(),
            emergency_authorizations: self.emergency_authorizations.clone(),
            emergency_batches: self.emergency_batches.clone(),
            observation_keys: self.observation_keys.clone(),
            _container_fee: fee,
            _authorization_slots_fee: memory.reserve(0, class).map_err(ValueTreeError::Memory)?,
            _batch_slots_fee: memory.reserve(0, class).map_err(ValueTreeError::Memory)?,
            _shipment_slots_fee: memory.reserve(0, class).map_err(ValueTreeError::Memory)?,
            _service_slots_fee: memory.reserve(0, class).map_err(ValueTreeError::Memory)?,
            _emergency_authorization_slots_fee: memory
                .reserve(0, class)
                .map_err(ValueTreeError::Memory)?,
            _emergency_batch_slots_fee: memory.reserve(0, class).map_err(ValueTreeError::Memory)?,
            _observation_slots_fee: memory.reserve(0, class).map_err(ValueTreeError::Memory)?,
            memory: memory.clone(),
            class,
        })
    }
}

pub(super) struct WorldRuleLifecycleIndexBuilderV1 {
    pub(super) supply_authorizations: Vec<Arc<ReservedValue<LifecycleSupplyAuthorizationV1>>>,
    pub(super) supply_batches: Vec<Arc<ReservedValue<LifecycleSupplyBatchV1>>>,
    pub(super) supply_shipments: Vec<Arc<ReservedValue<LifecycleSupplyShipmentV1>>>,
    pub(super) minimum_services: Vec<Arc<ReservedValue<LifecycleMinimumServiceV1>>>,
    pub(super) emergency_authorizations: Vec<Arc<ReservedValue<LifecycleEmergencyAuthorizationV1>>>,
    pub(super) emergency_batches: Vec<Arc<ReservedValue<LifecycleEmergencyBatchV1>>>,
    pub(super) observation_keys: Vec<Arc<ReservedValue<String>>>,
    authorization_slots_fee: MemoryReservation,
    batch_slots_fee: MemoryReservation,
    shipment_slots_fee: MemoryReservation,
    service_slots_fee: MemoryReservation,
    emergency_authorization_slots_fee: MemoryReservation,
    emergency_batch_slots_fee: MemoryReservation,
    observation_slots_fee: MemoryReservation,
    memory: MemoryBudget,
    class: MemoryClass,
}
impl WorldRuleLifecycleIndexBuilderV1 {
    pub(super) fn new(memory: &MemoryBudget, class: MemoryClass) -> Result<Self, ValueTreeError> {
        Ok(Self {
            supply_authorizations: Vec::new(),
            supply_batches: Vec::new(),
            supply_shipments: Vec::new(),
            minimum_services: Vec::new(),
            emergency_authorizations: Vec::new(),
            observation_keys: Vec::new(),
            emergency_batches: Vec::new(),
            authorization_slots_fee: memory.reserve(0, class)?,
            batch_slots_fee: memory.reserve(0, class)?,
            shipment_slots_fee: memory.reserve(0, class)?,
            service_slots_fee: memory.reserve(0, class)?,
            emergency_authorization_slots_fee: memory.reserve(0, class)?,
            emergency_batch_slots_fee: memory.reserve(0, class)?,
            observation_slots_fee: memory.reserve(0, class)?,
            memory: memory.clone(),
            class,
        })
    }
    fn retain<T>(
        memory: &MemoryBudget,
        class: MemoryClass,
        value: T,
        bytes: usize,
    ) -> Result<Arc<ReservedValue<T>>, ValueTreeError> {
        let fee = memory
            .reserve(row_fee::<T>(bytes)?, class)
            .map_err(ValueTreeError::Memory)?;
        Ok(Arc::new(fee.retain(value)))
    }
    fn push_slot<T>(
        memory: &MemoryBudget,
        class: MemoryClass,
        rows: &mut Vec<T>,
        slots: &mut MemoryReservation,
        value: T,
    ) -> Result<(), ValueTreeError> {
        if rows.len() == rows.capacity() {
            let old = rows.capacity();
            let next = old.max(4).checked_mul(2).ok_or_else(limited)?;
            let bytes = next.checked_mul(size_of::<T>()).ok_or_else(limited)?;
            let fee = memory
                .reserve(bytes, class)
                .map_err(ValueTreeError::Memory)?;
            rows.try_reserve_exact(next.saturating_sub(rows.len()))
                .map_err(|_| limited())?;
            if rows.capacity() > next {
                return Err(invalid());
            }
            *slots = fee;
        }
        rows.push(value);
        Ok(())
    }
    fn copy_row<T: Clone>(
        memory: &MemoryBudget,
        class: MemoryClass,
        source: &T,
        bytes: usize,
    ) -> Result<Arc<ReservedValue<T>>, ValueTreeError> {
        let fee = memory
            .reserve(row_fee::<T>(bytes)?, class)
            .map_err(ValueTreeError::Memory)?;
        Ok(Arc::new(fee.retain(source.clone())))
    }
    pub(super) fn push_supply_authorization(
        &mut self,
        row: LifecycleSupplyAuthorizationV1,
    ) -> Result<(), ValueTreeError> {
        if self
            .supply_authorizations
            .last()
            .is_some_and(|prior| prior.outer_id.as_str() >= row.outer_id.as_str())
        {
            return Err(invalid());
        }
        let memory = self.memory.clone();
        let class = self.class;
        let fee = row.bytes()?;
        let value = Self::retain(&memory, class, row, fee)?;
        Self::push_slot(
            &memory,
            class,
            &mut self.supply_authorizations,
            &mut self.authorization_slots_fee,
            value,
        )
    }
    pub(super) fn push_supply_batch(
        &mut self,
        row: LifecycleSupplyBatchV1,
    ) -> Result<(), ValueTreeError> {
        if self
            .supply_batches
            .last()
            .is_some_and(|prior| prior.outer_id.as_str() >= row.outer_id.as_str())
        {
            return Err(invalid());
        }
        let memory = self.memory.clone();
        let class = self.class;
        let fee = row.bytes()?;
        let value = Self::retain(&memory, class, row, fee)?;
        Self::push_slot(
            &memory,
            class,
            &mut self.supply_batches,
            &mut self.batch_slots_fee,
            value,
        )
    }
    pub(super) fn push_supply_shipment(
        &mut self,
        row: LifecycleSupplyShipmentV1,
    ) -> Result<(), ValueTreeError> {
        if self
            .supply_shipments
            .last()
            .is_some_and(|prior| prior.outer_id >= row.outer_id)
        {
            return Err(invalid());
        }
        let memory = self.memory.clone();
        let class = self.class;
        let fee = row.bytes()?;
        let value = Self::retain(&memory, class, row, fee)?;
        Self::push_slot(
            &memory,
            class,
            &mut self.supply_shipments,
            &mut self.shipment_slots_fee,
            value,
        )
    }
    pub(super) fn push_minimum_service(
        &mut self,
        row: LifecycleMinimumServiceV1,
    ) -> Result<(), ValueTreeError> {
        if self
            .minimum_services
            .last()
            .is_some_and(|prior| prior.outer_id.as_str() >= row.outer_id.as_str())
        {
            return Err(invalid());
        }
        let memory = self.memory.clone();
        let class = self.class;
        let fee = row.bytes()?;
        let value = Self::retain(&memory, class, row, fee)?;
        Self::push_slot(
            &memory,
            class,
            &mut self.minimum_services,
            &mut self.service_slots_fee,
            value,
        )
    }
    pub(super) fn push_emergency_authorization(
        &mut self,
        row: LifecycleEmergencyAuthorizationV1,
    ) -> Result<(), ValueTreeError> {
        if self
            .emergency_authorizations
            .last()
            .is_some_and(|prior| prior.outer_id.as_str() >= row.outer_id.as_str())
        {
            return Err(invalid());
        }
        let memory = self.memory.clone();
        let class = self.class;
        let fee = row.bytes()?;
        let value = Self::retain(&memory, class, row, fee)?;
        Self::push_slot(
            &memory,
            class,
            &mut self.emergency_authorizations,
            &mut self.emergency_authorization_slots_fee,
            value,
        )
    }
    pub(super) fn push_emergency_batch(
        &mut self,
        row: LifecycleEmergencyBatchV1,
    ) -> Result<(), ValueTreeError> {
        if self
            .emergency_batches
            .last()
            .is_some_and(|prior| prior.outer_id.as_str() >= row.outer_id.as_str())
        {
            return Err(invalid());
        }
        let memory = self.memory.clone();
        let class = self.class;
        let fee = row.bytes()?;
        let value = Self::retain(&memory, class, row, fee)?;
        Self::push_slot(
            &memory,
            class,
            &mut self.emergency_batches,
            &mut self.emergency_batch_slots_fee,
            value,
        )
    }
    pub(super) fn push_observation_key(&mut self, row: String) -> Result<(), ValueTreeError> {
        if self
            .observation_keys
            .last()
            .is_some_and(|prior| prior.as_str() >= row.as_str())
        {
            return Err(invalid());
        }
        let memory = self.memory.clone();
        let class = self.class;
        let fee = row.capacity();
        let value = Self::retain(&memory, class, row, fee)?;
        Self::push_slot(
            &memory,
            class,
            &mut self.observation_keys,
            &mut self.observation_slots_fee,
            value,
        )
    }
    pub(super) fn push_supply_authorization_ref(
        &mut self,
        row: Arc<ReservedValue<LifecycleSupplyAuthorizationV1>>,
    ) -> Result<(), ValueTreeError> {
        if self
            .supply_authorizations
            .last()
            .is_some_and(|prior| prior.outer_id.as_str() >= row.outer_id.as_str())
        {
            return Err(invalid());
        }
        let memory = self.memory.clone();
        let class = self.class;
        Self::push_slot(
            &memory,
            class,
            &mut self.supply_authorizations,
            &mut self.authorization_slots_fee,
            row,
        )
    }
    pub(super) fn push_supply_batch_ref(
        &mut self,
        row: Arc<ReservedValue<LifecycleSupplyBatchV1>>,
    ) -> Result<(), ValueTreeError> {
        if self
            .supply_batches
            .last()
            .is_some_and(|prior| prior.outer_id.as_str() >= row.outer_id.as_str())
        {
            return Err(invalid());
        }
        let memory = self.memory.clone();
        let class = self.class;
        Self::push_slot(
            &memory,
            class,
            &mut self.supply_batches,
            &mut self.batch_slots_fee,
            row,
        )
    }
    pub(super) fn push_supply_shipment_ref(
        &mut self,
        row: Arc<ReservedValue<LifecycleSupplyShipmentV1>>,
    ) -> Result<(), ValueTreeError> {
        if self
            .supply_shipments
            .last()
            .is_some_and(|prior| prior.outer_id >= row.outer_id)
        {
            return Err(invalid());
        }
        let memory = self.memory.clone();
        let class = self.class;
        Self::push_slot(
            &memory,
            class,
            &mut self.supply_shipments,
            &mut self.shipment_slots_fee,
            row,
        )
    }
    pub(super) fn push_minimum_service_ref(
        &mut self,
        row: Arc<ReservedValue<LifecycleMinimumServiceV1>>,
    ) -> Result<(), ValueTreeError> {
        if self
            .minimum_services
            .last()
            .is_some_and(|prior| prior.outer_id.as_str() >= row.outer_id.as_str())
        {
            return Err(invalid());
        }
        let memory = self.memory.clone();
        let class = self.class;
        Self::push_slot(
            &memory,
            class,
            &mut self.minimum_services,
            &mut self.service_slots_fee,
            row,
        )
    }
    pub(super) fn push_emergency_authorization_ref(
        &mut self,
        row: Arc<ReservedValue<LifecycleEmergencyAuthorizationV1>>,
    ) -> Result<(), ValueTreeError> {
        if self
            .emergency_authorizations
            .last()
            .is_some_and(|prior| prior.outer_id.as_str() >= row.outer_id.as_str())
        {
            return Err(invalid());
        }
        let memory = self.memory.clone();
        let class = self.class;
        Self::push_slot(
            &memory,
            class,
            &mut self.emergency_authorizations,
            &mut self.emergency_authorization_slots_fee,
            row,
        )
    }
    pub(super) fn push_emergency_batch_ref(
        &mut self,
        row: Arc<ReservedValue<LifecycleEmergencyBatchV1>>,
    ) -> Result<(), ValueTreeError> {
        if self
            .emergency_batches
            .last()
            .is_some_and(|prior| prior.outer_id.as_str() >= row.outer_id.as_str())
        {
            return Err(invalid());
        }
        let memory = self.memory.clone();
        let class = self.class;
        Self::push_slot(
            &memory,
            class,
            &mut self.emergency_batches,
            &mut self.emergency_batch_slots_fee,
            row,
        )
    }
    pub(super) fn push_observation_ref(
        &mut self,
        row: Arc<ReservedValue<String>>,
    ) -> Result<(), ValueTreeError> {
        if self
            .observation_keys
            .last()
            .is_some_and(|prior| prior.as_str() >= row.as_str())
        {
            return Err(invalid());
        }
        let memory = self.memory.clone();
        let class = self.class;
        Self::push_slot(
            &memory,
            class,
            &mut self.observation_keys,
            &mut self.observation_slots_fee,
            row,
        )
    }
    pub(super) fn push_shipment_paid(
        &mut self,
        row: LifecycleSupplyShipmentV1,
        mut text_fees: [MemoryReservation; 4],
    ) -> Result<(), ValueTreeError> {
        if self
            .supply_shipments
            .last()
            .is_some_and(|prior| prior.outer_id >= row.outer_id)
        {
            return Err(invalid());
        }
        let bytes = row.bytes()?;
        let fee_bytes = text_fees
            .iter()
            .try_fold(0usize, |sum, fee| add(sum, fee.bytes()))?;
        if bytes != fee_bytes {
            return Err(invalid());
        }
        let total = row_fee::<LifecycleSupplyShipmentV1>(bytes)?;
        let mut fee = self
            .memory
            .reserve(total.checked_sub(bytes).ok_or_else(limited)?, self.class)
            .map_err(ValueTreeError::Memory)?;
        for text_fee in &mut text_fees {
            fee.merge(text_fee).map_err(ValueTreeError::Invalid)?;
        }
        let value = Arc::new(fee.retain(row));
        let memory = self.memory.clone();
        let class = self.class;
        Self::push_slot(
            &memory,
            class,
            &mut self.supply_shipments,
            &mut self.shipment_slots_fee,
            value,
        )
    }
    pub(super) fn copy_supply_authorization(
        &mut self,
        source: &LifecycleSupplyAuthorizationV1,
    ) -> Result<(), ValueTreeError> {
        if self
            .supply_authorizations
            .last()
            .is_some_and(|prior| prior.outer_id.as_str() >= source.outer_id.as_str())
        {
            return Err(invalid());
        }
        let memory = self.memory.clone();
        let class = self.class;
        let bytes = source.bytes()?;
        let row = Self::copy_row(&memory, class, source, bytes)?;
        Self::push_slot(
            &memory,
            class,
            &mut self.supply_authorizations,
            &mut self.authorization_slots_fee,
            row,
        )
    }
    pub(super) fn copy_supply_batch(
        &mut self,
        source: &LifecycleSupplyBatchV1,
    ) -> Result<(), ValueTreeError> {
        if self
            .supply_batches
            .last()
            .is_some_and(|prior| prior.outer_id.as_str() >= source.outer_id.as_str())
        {
            return Err(invalid());
        }
        let memory = self.memory.clone();
        let class = self.class;
        let bytes = source.bytes()?;
        let row = Self::copy_row(&memory, class, source, bytes)?;
        Self::push_slot(
            &memory,
            class,
            &mut self.supply_batches,
            &mut self.batch_slots_fee,
            row,
        )
    }
    pub(super) fn copy_supply_shipment(
        &mut self,
        source: &LifecycleSupplyShipmentV1,
    ) -> Result<(), ValueTreeError> {
        if self
            .supply_shipments
            .last()
            .is_some_and(|prior| prior.outer_id >= source.outer_id)
        {
            return Err(invalid());
        }
        let memory = self.memory.clone();
        let class = self.class;
        let bytes = source.bytes()?;
        let row = Self::copy_row(&memory, class, source, bytes)?;
        Self::push_slot(
            &memory,
            class,
            &mut self.supply_shipments,
            &mut self.shipment_slots_fee,
            row,
        )
    }
    pub(super) fn copy_minimum_service(
        &mut self,
        source: &LifecycleMinimumServiceV1,
    ) -> Result<(), ValueTreeError> {
        if self
            .minimum_services
            .last()
            .is_some_and(|prior| prior.outer_id.as_str() >= source.outer_id.as_str())
        {
            return Err(invalid());
        }
        let memory = self.memory.clone();
        let class = self.class;
        let bytes = source.bytes()?;
        let row = Self::copy_row(&memory, class, source, bytes)?;
        Self::push_slot(
            &memory,
            class,
            &mut self.minimum_services,
            &mut self.service_slots_fee,
            row,
        )
    }
    pub(super) fn copy_emergency_authorization(
        &mut self,
        source: &LifecycleEmergencyAuthorizationV1,
    ) -> Result<(), ValueTreeError> {
        if self
            .emergency_authorizations
            .last()
            .is_some_and(|prior| prior.outer_id.as_str() >= source.outer_id.as_str())
        {
            return Err(invalid());
        }
        let memory = self.memory.clone();
        let class = self.class;
        let bytes = source.bytes()?;
        let row = Self::copy_row(&memory, class, source, bytes)?;
        Self::push_slot(
            &memory,
            class,
            &mut self.emergency_authorizations,
            &mut self.emergency_authorization_slots_fee,
            row,
        )
    }
    pub(super) fn copy_emergency_batch(
        &mut self,
        source: &LifecycleEmergencyBatchV1,
    ) -> Result<(), ValueTreeError> {
        if self
            .emergency_batches
            .last()
            .is_some_and(|prior| prior.outer_id.as_str() >= source.outer_id.as_str())
        {
            return Err(invalid());
        }
        let memory = self.memory.clone();
        let class = self.class;
        let bytes = source.bytes()?;
        let row = Self::copy_row(&memory, class, source, bytes)?;
        Self::push_slot(
            &memory,
            class,
            &mut self.emergency_batches,
            &mut self.emergency_batch_slots_fee,
            row,
        )
    }
    pub(super) fn copy_observation(&mut self, source: &str) -> Result<(), ValueTreeError> {
        if self
            .observation_keys
            .last()
            .is_some_and(|prior| prior.as_str() >= source)
        {
            return Err(invalid());
        }
        let memory = self.memory.clone();
        let class = self.class;
        let bytes = source.len();
        let fee = memory
            .reserve(row_fee::<String>(bytes)?, class)
            .map_err(ValueTreeError::Memory)?;
        let mut copy = String::new();
        copy.try_reserve_exact(bytes).map_err(|_| limited())?;
        if copy.capacity() != bytes {
            return Err(invalid());
        }
        copy.push_str(source);
        let row = Arc::new(fee.retain(copy));
        Self::push_slot(
            &memory,
            class,
            &mut self.observation_keys,
            &mut self.observation_slots_fee,
            row,
        )
    }
    pub(super) fn finish(
        self,
        roots: [u8; 32],
    ) -> Result<ReservedValue<WorldRuleLifecycleIndexV1>, ValueTreeError> {
        // Each lane already holds the reservation for its actual Vec capacity.
        let storage_fee = self
            .memory
            .reserve(size_of::<WorldRuleLifecycleIndexV1>() + 2048, self.class)
            .map_err(ValueTreeError::Memory)?;
        let fee = self
            .memory
            .reserve(size_of::<WorldRuleLifecycleIndexV1>(), self.class)
            .map_err(ValueTreeError::Memory)?;
        Ok(fee.retain(WorldRuleLifecycleIndexV1 {
            roots,
            supply_authorizations: self.supply_authorizations,
            supply_batches: self.supply_batches,
            supply_shipments: self.supply_shipments,
            minimum_services: self.minimum_services,
            emergency_authorizations: self.emergency_authorizations,
            emergency_batches: self.emergency_batches,
            observation_keys: self.observation_keys,
            _container_fee: storage_fee,
            _authorization_slots_fee: self.authorization_slots_fee,
            _batch_slots_fee: self.batch_slots_fee,
            _shipment_slots_fee: self.shipment_slots_fee,
            _service_slots_fee: self.service_slots_fee,
            _emergency_authorization_slots_fee: self.emergency_authorization_slots_fee,
            _emergency_batch_slots_fee: self.emergency_batch_slots_fee,
            _observation_slots_fee: self.observation_slots_fee,
            memory: self.memory,
            class: self.class,
        }))
    }
}

#[path = "incremental_world_rule_lifecycle_qualification.rs"]
mod qualification;
pub use qualification::{
    accept_world_rule_lifecycle_index_v1, qualify_world_rule_lifecycle_index_v1,
    readmit_world_rule_lifecycle_index_ordinary_v1,
};

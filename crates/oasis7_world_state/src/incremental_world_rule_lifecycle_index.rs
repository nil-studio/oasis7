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

fn debit_outer(work: &mut WorldRulesFamilyWorkV1, bytes: usize) -> Result<(), ValueTreeError> {
    work.outer_records_remaining = work
        .outer_records_remaining
        .checked_sub(1)
        .ok_or_else(limited)?;
    work.remainder.cells_remaining = work
        .remainder
        .cells_remaining
        .checked_sub(u64::try_from(bytes).map_err(|_| limited())?)
        .ok_or_else(limited)?;
    Ok(())
}

fn canonical_world_rules_key(
    raw: &[u8],
) -> Result<Option<(WorldFieldKeyV1, WorldFieldRecordKindV1)>, ValueTreeError> {
    const DOMAIN: &[u8] = b"oasis7.iws.world-field-key.v1\0";
    if !raw.starts_with(DOMAIN) {
        return Ok(None);
    }
    let binding = WorldFieldKeyV1::from_bytes(raw).map_err(ValueTreeError::Invalid)?;
    if binding.owner != FieldOwner::WorldState || binding.name != "world_rules" {
        return Ok(None);
    }
    if binding.namespace != StateNamespaceV1::Authorization {
        return Err(invalid());
    }
    let key = WorldFieldKeyV1::new(
        world_rules_descriptor().map_err(|_| invalid())?,
        binding.kind,
        binding.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != raw {
        return Err(invalid());
    }
    Ok(Some((key, binding.kind)))
}

fn read_member_semantic<M, R, E, C>(
    value: &[u8; 32],
    key: &WorldFieldKeyV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_semantic: usize,
) -> Result<(Vec<u8>, MemoryReservation), ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let reserve = max_semantic
        .checked_add(size_of::<Vec<u8>>() + 64)
        .ok_or_else(limited)?;
    let fee = ctx
        .values
        .memory
        .reserve(reserve, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    let mut semantic = Vec::new();
    semantic
        .try_reserve_exact(max_semantic)
        .map_err(|_| limited())?;
    if semantic.capacity() > max_semantic {
        return Err(invalid());
    }
    read_member_record_key_v1(
        value,
        key,
        &mut ctx.values,
        MemberRecordLimitsV1 {
            semantic_bytes: u64::try_from(max_semantic).map_err(|_| limited())?,
            payload_bytes: ctx.record_bytes,
            record_bytes: ctx.record_bytes,
        },
        &mut semantic,
        &mut ctx.cancelled,
    )?;
    ctx.check()?;
    Ok((semantic, fee))
}

fn sort_ids(
    ids: &mut Vec<u64>,
    work: &mut WorldRulesFamilyWorkV1,
    reject_duplicates: bool,
) -> Result<(), ValueTreeError> {
    let n = ids.len();
    let levels = if n <= 1 {
        0
    } else {
        usize::BITS as usize - (n - 1).leading_zeros() as usize
    };
    let comparisons = n.checked_mul(levels).ok_or_else(limited)?;
    let bytes = comparisons
        .checked_mul(size_of::<u64>())
        .ok_or_else(limited)?;
    work.remainder.cells_remaining = work
        .remainder
        .cells_remaining
        .checked_sub(u64::try_from(comparisons).map_err(|_| limited())?)
        .ok_or_else(limited)?;
    work.outer_records_remaining = work
        .outer_records_remaining
        .checked_sub(u64::try_from(comparisons).map_err(|_| limited())?)
        .ok_or_else(limited)?;
    let _ = bytes;
    ids.sort_unstable();
    if reject_duplicates {
        if ids.windows(2).any(|w| w[0] == w[1]) {
            return Err(invalid());
        }
    } else {
        ids.dedup();
    }
    Ok(())
}

fn collect_initial_shipment_ids<M, R, E, C>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldRulesFamilyWorkV1,
    expected_count: usize,
) -> Result<(Vec<u64>, MemoryReservation), ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let memory = ctx.values.memory.clone();
    let class = ctx.values.class;
    let bytes = expected_count
        .checked_mul(size_of::<u64>())
        .and_then(|n| n.checked_add(size_of::<Vec<u64>>() + 64))
        .ok_or_else(limited)?;
    let fee = memory
        .reserve(bytes, class)
        .map_err(ValueTreeError::Memory)?;
    let mut ids = Vec::new();
    ids.try_reserve_exact(expected_count)
        .map_err(|_| limited())?;
    if ids.capacity() > expected_count {
        return Err(invalid());
    }
    let stop = std::cell::RefCell::new(&mut ctx.cancelled);
    let mut local = WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Authorization),
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
        cancelled: || stop.borrow_mut()(),
    };
    let max_semantic = usize::try_from(local.semantic_bytes)
        .unwrap_or(usize::MAX)
        .min(usize::try_from(work.max_semantic_bytes).unwrap_or(usize::MAX));
    for root in roots
        .state
        .iter()
        .filter(|root| root.namespace == StateNamespaceV1::Authorization)
    {
        let result = StateScanContext {
            source: local.state,
            memory: &memory,
            class,
            io: scan,
        }
        .visit_entries(
            root,
            |entry| {
                local.check()?;
                if entry.namespace != StateNamespaceV1::Authorization {
                    return Err(invalid());
                }
                let Some((key, kind)) = canonical_world_rules_key(&entry.key)? else {
                    return Ok(());
                };
                if key.as_bytes() != entry.key {
                    return Err(invalid());
                }
                if kind != WorldFieldRecordKindV1::Entry {
                    return Ok(());
                }
                debit_outer(work, entry.key.len())?;
                let (semantic, _semantic_fee) =
                    read_member_semantic(&entry.value, &key, &mut local, max_semantic)?;
                if let Some((id, marker)) = shipment_key(&semantic)? {
                    if marker {
                        if ids.len() == expected_count {
                            return Err(invalid());
                        }
                        ids.push(id);
                    }
                }
                local.check()
            },
            || stop.borrow_mut()(),
        );
        match result {
            Ok(_) => (),
            Err(StateScanError::Visitor(e)) => return Err(e),
            Err(StateScanError::Memory(e)) => return Err(ValueTreeError::Memory(e)),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(StateScanError::ResourceLimited) => return Err(limited()),
            Err(_) => return Err(invalid()),
        }
        local.check()?;
    }
    if ids.len() != expected_count {
        return Err(invalid());
    }
    sort_ids(&mut ids, work, true)?;
    Ok((ids, fee))
}

fn collect_changed_shipment_ids<M, R, E, C>(
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldRulesFamilyWorkV1,
) -> Result<(Vec<u64>, MemoryReservation), ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let cap = record.writes.len();
    let bytes = cap
        .checked_mul(size_of::<u64>())
        .and_then(|n| n.checked_add(size_of::<Vec<u64>>() + 64))
        .ok_or_else(limited)?;
    let fee = ctx
        .values
        .memory
        .reserve(bytes, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    let mut ids = Vec::new();
    ids.try_reserve_exact(cap).map_err(|_| limited())?;
    if ids.capacity() > cap {
        return Err(invalid());
    }
    let max_semantic = usize::try_from(ctx.semantic_bytes)
        .unwrap_or(usize::MAX)
        .min(usize::try_from(work.max_semantic_bytes).unwrap_or(usize::MAX));
    for write in &record.writes {
        ctx.check()?;
        if write.namespace != StateNamespaceV1::Authorization {
            continue;
        }
        let Some((key, kind)) = canonical_world_rules_key(&write.mutation.key)? else {
            continue;
        };
        if kind != WorldFieldRecordKindV1::Entry {
            continue;
        }
        debit_outer(work, write.mutation.key.len())?;
        let value = write
            .mutation
            .after
            .or(write.mutation.before)
            .ok_or_else(invalid)?;
        let (semantic, _semantic_fee) = read_member_semantic(&value, &key, ctx, max_semantic)?;
        if let Some((id, _)) = shipment_key(&semantic)? {
            ids.push(id);
        }
    }
    sort_ids(&mut ids, work, false)?;
    Ok((ids, fee))
}

fn copy_supply_rows(
    builder: &mut WorldRuleLifecycleIndexBuilderV1,
    supply: &SupplyFamilyQualificationV1,
    work: &mut WorldRulesFamilyWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), ValueTreeError> {
    let counts = supply.record_counts();
    if usize::try_from(counts[0]).map_err(|_| limited())? != supply.lifecycle_authorizations().len()
        || usize::try_from(counts[1]).map_err(|_| limited())? != supply.lifecycle_batches().len()
    {
        return Err(invalid());
    }
    for row in supply.lifecycle_authorizations() {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        if !row.uses_budget(&builder.memory) {
            return Err(invalid());
        }
        let value = &***row;
        debit_outer(work, value.bytes()?)?;
        builder.push_supply_authorization_ref(Arc::clone(row))?;
    }
    for row in supply.lifecycle_batches() {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        if !row.uses_budget(&builder.memory) {
            return Err(invalid());
        }
        let value = &***row;
        debit_outer(work, value.bytes()?)?;
        builder.push_supply_batch_ref(Arc::clone(row))?;
    }
    Ok(())
}

fn copy_emergency_rows(
    builder: &mut WorldRuleLifecycleIndexBuilderV1,
    emergency: &WorldRuleCompositeQualificationV1<crate::world_rules::EmergencySupplyStateV1>,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut WorldRulesFamilyWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), ValueTreeError> {
    emergency.check_budget(memory, class)?;
    let directory = emergency
        .emergency_lifecycle_directory()
        .ok_or_else(invalid)?;
    for row in directory.minimum_service_rows() {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        debit_outer(work, row.bytes()?)?;
        builder.push_minimum_service_ref(Arc::clone(row))?;
    }
    for row in directory.authorization_rows() {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        debit_outer(work, row.bytes()?)?;
        builder.push_emergency_authorization_ref(Arc::clone(row))?;
    }
    // Keep every row, including committed batches. Original selectors decide
    // whether a selected batch is relevant to a particular command.
    for row in directory.batch_rows() {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        debit_outer(work, row.bytes()?)?;
        builder.push_emergency_batch_ref(Arc::clone(row))?;
    }
    for row in directory.observation_rows() {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        debit_outer(work, row.len())?;
        builder.push_observation_ref(Arc::clone(row))?;
    }
    Ok(())
}

fn validate_source_families<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    supply: &SupplyFamilyQualificationV1,
    remainder: &WorldRuleRemainderQualificationV1,
    emergency: &WorldRuleCompositeQualificationV1<crate::world_rules::EmergencySupplyStateV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    ctx.check()?;
    let digest = roots.digest().map_err(|_| invalid())?;
    if ctx.root != roots.state_root(StateNamespaceV1::Authorization)
        || supply.allocation_class() != ctx.values.class
        || remainder.allocation_class() != ctx.values.class
        || emergency.allocation_class() != ctx.values.class
    {
        return Err(invalid());
    }
    supply.validate_binding(&digest)?;
    if supply
        .lifecycle_authorizations()
        .iter()
        .any(|row| !row.uses_budget(&ctx.values.memory))
        || supply
            .lifecycle_batches()
            .iter()
            .any(|row| !row.uses_budget(&ctx.values.memory))
    {
        return Err(invalid());
    }
    remainder.validate_binding(roots)?;
    emergency.validate_binding(roots)?;
    emergency.check_budget(&ctx.values.memory, ctx.values.class)?;
    if supply.rules_present() != remainder.rules_present()
        || emergency.parent_present() != remainder.rules_present()
    {
        return Err(invalid());
    }
    ctx.check()
}

/// Build a fixed WorldRules lifecycle selector from the exact typed family
/// qualifications. It retains only selector data; actions still reopen their
/// selected records through the original Current codecs.
#[allow(clippy::too_many_arguments)]
pub fn qualify_world_rule_lifecycle_index_v1<M, R, E, C>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    outer_scan: &mut StateScanBudget,
    _inner_scan: &mut StateScanBudget,
    work: &mut WorldRulesFamilyWorkV1,
    supply: &SupplyFamilyQualificationV1,
    remainder: &WorldRuleRemainderQualificationV1,
    emergency: &WorldRuleCompositeQualificationV1<crate::world_rules::EmergencySupplyStateV1>,
) -> Result<ReservedValue<WorldRuleLifecycleIndexV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    validate_source_families(roots, supply, remainder, emergency, ctx)?;
    let digest = roots.digest().map_err(|_| invalid())?;
    let mut builder = WorldRuleLifecycleIndexBuilderV1::new(&ctx.values.memory, ctx.values.class)?;
    copy_supply_rows(&mut builder, supply, work, &mut ctx.cancelled)?;
    copy_emergency_rows(
        &mut builder,
        emergency,
        &ctx.values.memory,
        ctx.values.class,
        work,
        &mut ctx.cancelled,
    )?;

    let expected = usize::try_from(remainder.record_counts()[2]).map_err(|_| limited())?;
    let (ids, _ids_fee) = collect_initial_shipment_ids(roots, ctx, outer_scan, work, expected)?;
    for id in ids {
        ctx.check()?;
        let Some((value, text_fees)) =
            read_shipment_texts(id, ctx, work.remainder.max_payload_bytes)?
        else {
            return Err(invalid());
        };
        let row = LifecycleSupplyShipmentV1 {
            outer_id: id,
            value,
        };
        debit_outer(work, row.bytes()?)?;
        builder.push_shipment_paid(row, text_fees)?;
    }
    if builder.supply_shipments.len() != expected {
        return Err(invalid());
    }
    ctx.check()?;
    builder.finish(digest)
}

/// Rebuild the selector after one verified WorldRules transition. Only IDs
/// found in that transition's authenticated writes are reopened; unchanged
/// shipment rows are shared from the parent projection.
#[allow(clippy::too_many_arguments)]
pub fn accept_world_rule_lifecycle_index_v1<M, R, E, C>(
    parent_index: &ReservedValue<WorldRuleLifecycleIndexV1>,
    parent_supply: &SupplyFamilyQualificationV1,
    parent_remainder: &WorldRuleRemainderQualificationV1,
    parent_emergency: &WorldRuleCompositeQualificationV1<
        crate::world_rules::EmergencySupplyStateV1,
    >,
    supply: &SupplyFamilyQualificationV1,
    remainder: &WorldRuleRemainderQualificationV1,
    emergency: &WorldRuleCompositeQualificationV1<crate::world_rules::EmergencySupplyStateV1>,
    witness: &crate::incremental_verified_world_transition::VerifiedWorldTransitionV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldRulesFamilyWorkV1,
) -> Result<ReservedValue<WorldRuleLifecycleIndexV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let record = witness.record();
    let before_digest = record.before.digest().map_err(|_| invalid())?;
    let after_digest = record.after.digest().map_err(|_| invalid())?;
    if ctx.values.class != MemoryClass::Ordinary
        || parent_index.class != MemoryClass::Ordinary
        || !parent_index.uses_budget(&ctx.values.memory)
        || !ctx.values.memory.same_pool(&parent_index.memory)
        || ctx.root != record.after.state_root(StateNamespaceV1::Authorization)
    {
        return Err(invalid().into());
    }
    witness.check_context(&ctx.values.memory, ctx.values.class, ctx.root)?;
    parent_index.validate_binding(&record.before)?;
    parent_supply.validate_binding(&before_digest)?;
    parent_remainder.validate_binding(&record.before)?;
    parent_emergency.validate_binding(&record.before)?;
    supply.validate_binding(&after_digest)?;
    remainder.validate_binding(&record.after)?;
    emergency.validate_binding(&record.after)?;
    validate_source_families(&record.after, supply, remainder, emergency, ctx)?;

    let mut builder =
        WorldRuleLifecycleIndexBuilderV1::new(&ctx.values.memory, MemoryClass::Ordinary)?;
    copy_supply_rows(&mut builder, supply, work, &mut ctx.cancelled)?;
    copy_emergency_rows(
        &mut builder,
        emergency,
        &ctx.values.memory,
        MemoryClass::Ordinary,
        work,
        &mut ctx.cancelled,
    )?;

    let (changed, _changed_fee) = collect_changed_shipment_ids(record, ctx, work)?;
    let expected = usize::try_from(remainder.record_counts()[2]).map_err(|_| limited())?;
    if expected
        > parent_index
            .supply_shipments
            .len()
            .saturating_add(changed.len())
    {
        return Err(limited().into());
    }
    let mut old_at = 0usize;
    let mut changed_at = 0usize;
    while old_at < parent_index.supply_shipments.len() || changed_at < changed.len() {
        ctx.check()?;
        let old_id = parent_index
            .supply_shipments
            .get(old_at)
            .map(|row| row.outer_id);
        let changed_id = changed.get(changed_at).copied();
        match (old_id, changed_id) {
            (Some(old), Some(new)) if old < new => {
                let row = &parent_index.supply_shipments[old_at];
                if !row.uses_budget(&ctx.values.memory) {
                    return Err(invalid().into());
                }
                debit_outer(work, row.bytes()?)?;
                builder.push_supply_shipment_ref(Arc::clone(row))?;
                old_at += 1;
            }
            (Some(old), Some(new)) if old == new => {
                if let Some((value, text_fees)) =
                    read_shipment_texts(new, ctx, work.remainder.max_payload_bytes)?
                {
                    let row = LifecycleSupplyShipmentV1 {
                        outer_id: new,
                        value,
                    };
                    debit_outer(work, row.bytes()?)?;
                    builder.push_shipment_paid(row, text_fees)?;
                }
                old_at += 1;
                changed_at += 1;
            }
            (_, Some(new)) => {
                if let Some((value, text_fees)) =
                    read_shipment_texts(new, ctx, work.remainder.max_payload_bytes)?
                {
                    let row = LifecycleSupplyShipmentV1 {
                        outer_id: new,
                        value,
                    };
                    debit_outer(work, row.bytes()?)?;
                    builder.push_shipment_paid(row, text_fees)?;
                }
                changed_at += 1;
            }
            (Some(_), None) => {
                let row = &parent_index.supply_shipments[old_at];
                if !row.uses_budget(&ctx.values.memory) {
                    return Err(invalid().into());
                }
                debit_outer(work, row.bytes()?)?;
                builder.push_supply_shipment_ref(Arc::clone(row))?;
                old_at += 1;
            }
            (None, None) => break,
        }
    }
    if builder.supply_shipments.len() != expected {
        return Err(invalid().into());
    }
    ctx.check()?;
    Ok(builder.finish(after_digest)?)
}

/// Admit the immutable selector under Ordinary memory by copying each row
/// only after reserving its concrete storage. The source qualification is not
/// reinterpreted and cancellation is checked throughout the copy.
pub fn readmit_world_rule_lifecycle_index_ordinary_v1(
    source: &ReservedValue<WorldRuleLifecycleIndexV1>,
    memory: &MemoryBudget,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<ReservedValue<WorldRuleLifecycleIndexV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if source.class != MemoryClass::Recovery || !source.uses_budget(memory) {
        return Err(invalid());
    }
    source.check_budget(memory, MemoryClass::Recovery)?;
    let mut builder = WorldRuleLifecycleIndexBuilderV1::new(memory, MemoryClass::Ordinary)?;
    for row in &source.supply_authorizations {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        builder.copy_supply_authorization(&***row)?;
    }
    for row in &source.supply_batches {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        builder.copy_supply_batch(&***row)?;
    }
    for row in &source.supply_shipments {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        builder.copy_supply_shipment(&***row)?;
    }
    for row in &source.minimum_services {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        builder.copy_minimum_service(&***row)?;
    }
    for row in &source.emergency_authorizations {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        builder.copy_emergency_authorization(&***row)?;
    }
    for row in &source.emergency_batches {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        builder.copy_emergency_batch(&***row)?;
    }
    for row in &source.observation_keys {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        builder.copy_observation(&**row)?;
    }
    let output = builder.finish(source.roots)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(output)
}

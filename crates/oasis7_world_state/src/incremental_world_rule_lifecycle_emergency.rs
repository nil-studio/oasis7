//! Emergency-only resident lifecycle rows derived from the closed V1 branch.
//!
//! This is selector data attached to the genuine composite qualification. It
//! does not authorize a command and never replaces a selected Current read.
use super::*;
use crate::{
    incremental_record_columns_read::{ColumnCellV1, ColumnKeyV1},
    incremental_typed_record_diff::{
        ChangeDecoder, TypedCellRefV1, accept_body, key, point_typed_column_v1, root,
        visit_entries,
    },
    incremental_world_field_key::WorldFieldRecordKindV1,
    incremental_world_rules_family::{
        LifecycleEmergencyAuthorizationV1, LifecycleEmergencyBatchV1,
        LifecycleMinimumServiceV1,
    },
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateScanBudget, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{
    MemoryBudget, MemoryClass, MemoryReservation, ReservedValue,
};
use std::{mem::{align_of, size_of}, sync::Arc};
#[path = "incremental_world_rule_emergency_close_inputs.rs"]
mod close_inputs;
pub use close_inputs::EmergencyActiveInputsV1;

type Emergency = crate::world_rules::EmergencySupplyStateV1;
type Authorization = crate::world_rules::EmergencyAuthorizationV1;
type Batch = crate::world_rules::EmergencyBatchV1;
type MinimumService = crate::world_rules::MinimumServiceContractV1;
type Observation = oasis7_agent_api::world_action::EmergencyObservationV1;

fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Emergency lifecycle projection")
}
fn limited() -> ValueTreeError {
    ValueTreeError::ResourceLimited("Emergency lifecycle projection")
}
fn add(a: usize, b: usize) -> Result<usize, ValueTreeError> {
    a.checked_add(b).ok_or_else(limited)
}
fn spend<M, R, E, C>(
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    steps: usize,
    bytes: usize,
) -> Result<(), ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    d.check()?;
    d.scan.nodes_remaining = d.scan.nodes_remaining.checked_sub(u64::try_from(steps).map_err(|_| limited())?)
        .ok_or_else(limited)?;
    d.scan.read_bytes_remaining = d.scan.read_bytes_remaining
        .checked_sub(u64::try_from(bytes).map_err(|_| limited())?)
        .ok_or_else(limited)?;
    d.check()
}
fn string_copy(value: &str) -> Result<String, ValueTreeError> {
    let mut copy = String::new();
    copy.try_reserve_exact(value.len()).map_err(|_| limited())?;
    if copy.capacity() > value.len() { return Err(invalid()); }
    copy.push_str(value);
    Ok(copy)
}
fn strings_copy(values: &[&str]) -> Result<Vec<String>, ValueTreeError> {
    values.iter().map(|value| string_copy(value)).collect()
}
fn emergency_profile() -> TypedRecordProfileV1 {
    WorldRuleCompositeBranchV1::EmergencySupply.profile()
}
fn directory_arc_layout<T>() -> Result<usize, ValueTreeError> {
    size_of::<ReservedValue<T>>()
        .checked_add(2 * size_of::<usize>())
        .and_then(|n| n.checked_add(align_of::<ReservedValue<T>>()))
        .ok_or_else(limited)
}
fn authorization_bytes(row: &LifecycleEmergencyAuthorizationV1) -> Result<usize, ValueTreeError> {
    [row.outer_id.capacity(), row.id.capacity(), row.provider_id.capacity(), row.region_id.capacity()]
        .into_iter().try_fold(0usize, add)
}
fn batch_bytes(row: &LifecycleEmergencyBatchV1) -> Result<usize, ValueTreeError> {
    add(row.outer_id.capacity(), row.authorization_id.capacity())
}
fn minimum_service_bytes(row: &LifecycleMinimumServiceV1) -> Result<usize, ValueTreeError> {
    [row.outer_id.capacity(), row.operator_agent_id.capacity(), row.region_id.capacity()]
        .into_iter().try_fold(0usize, add)
}

/// Sparse Emergency projection retained on the exact branch qualification.
pub struct EmergencyLifecycleDirectoryV1 {
    minimum_services: Vec<Arc<ReservedValue<LifecycleMinimumServiceV1>>>,
    authorizations: Vec<Arc<ReservedValue<LifecycleEmergencyAuthorizationV1>>>,
    batches: Vec<Arc<ReservedValue<LifecycleEmergencyBatchV1>>>,
    observations: Vec<Arc<ReservedValue<String>>>,
    active_inputs: EmergencyActiveInputsV1,
    minimum_service_slots: MemoryReservation,
    authorization_slots: MemoryReservation,
    batch_slots: MemoryReservation,
    observation_slots: MemoryReservation,
    container_fee: MemoryReservation,
    memory: MemoryBudget,
    class: MemoryClass,
}
impl EmergencyLifecycleDirectoryV1 {
    pub(crate) fn minimum_service_rows(&self) -> &[Arc<ReservedValue<LifecycleMinimumServiceV1>>] { &self.minimum_services }
    pub(crate) fn authorization_rows(&self) -> &[Arc<ReservedValue<LifecycleEmergencyAuthorizationV1>>] { &self.authorizations }
    pub(crate) fn batch_rows(&self) -> &[Arc<ReservedValue<LifecycleEmergencyBatchV1>>] { &self.batches }
    pub(crate) fn observation_rows(&self) -> &[Arc<ReservedValue<String>>] { &self.observations }
    pub(crate) fn active_inputs(&self) -> &EmergencyActiveInputsV1 { &self.active_inputs }
    pub fn minimum_services(&self) -> impl ExactSizeIterator<Item = &LifecycleMinimumServiceV1> {
        self.minimum_services.iter().map(|row| &***row)
    }
    pub fn emergency_authorizations(&self) -> impl ExactSizeIterator<Item = &LifecycleEmergencyAuthorizationV1> {
        self.authorizations.iter().map(|row| &***row)
    }
    pub fn emergency_batches(&self) -> impl ExactSizeIterator<Item = &LifecycleEmergencyBatchV1> {
        self.batches.iter().map(|row| &***row)
    }
    pub fn observation_key_present(&self, key: &str) -> bool {
        self.observations.binary_search_by(|row| row.as_str().cmp(key)).is_ok()
    }
    pub fn check_budget(&self, memory: &MemoryBudget, class: MemoryClass) -> Result<(), ValueTreeError> {
        if self.class != class || !self.memory.same_pool(memory)
            || self.minimum_services.iter().any(|r| !r.uses_budget(memory))
            || self.authorizations.iter().any(|r| !r.uses_budget(memory))
            || self.batches.iter().any(|r| !r.uses_budget(memory))
            || self.observations.iter().any(|r| !r.uses_budget(memory))
        { return Err(invalid()); }
        self.active_inputs.check_budget(memory, class)?;
        Ok(())
    }
    fn share(&self, memory: &MemoryBudget, class: MemoryClass, scan: &mut StateScanBudget, cancelled: &mut impl FnMut() -> bool) -> Result<Self, ValueTreeError> {
        self.check_budget(memory, self.class)?;
        if class != self.class { return Err(invalid()); }
        let mut out = EmergencyLifecycleBuilderV1::new(memory, class)?;
        for row in &self.minimum_services { out.push_minimum_service_ref(row.clone(), scan, cancelled)?; }
        for row in &self.authorizations { out.push_authorization_ref(row.clone(), scan, cancelled)?; }
        for row in &self.batches { out.push_batch_ref(row.clone(), scan, cancelled)?; }
        for row in &self.observations { out.push_observation_ref(row.clone(), scan, cancelled)?; }
        let active_inputs = self.active_inputs.share(memory, class, scan, cancelled)?;
        out.finish(active_inputs)
    }
    fn share_readmit(&self, memory: &MemoryBudget, cancelled: &mut impl FnMut() -> bool) -> Result<Self, ValueTreeError> {
        self.check_budget(memory, self.class)?;
        let mut out=EmergencyLifecycleBuilderV1::new(memory,self.class)?;
        for row in &self.minimum_services { if cancelled(){return Err(ValueTreeError::Cancelled);} out.push_minimum_service_ref_readmit(row.clone(),cancelled)?; }
        for row in &self.authorizations { if cancelled(){return Err(ValueTreeError::Cancelled);} out.push_authorization_ref_readmit(row.clone(),cancelled)?; }
        for row in &self.batches { if cancelled(){return Err(ValueTreeError::Cancelled);} out.push_batch_ref_readmit(row.clone(),cancelled)?; }
        for row in &self.observations { if cancelled(){return Err(ValueTreeError::Cancelled);} out.push_observation_ref_readmit(row.clone(),cancelled)?; }
        let active_inputs = self.active_inputs.share_readmit(memory, self.class, cancelled)?;
        out.finish(active_inputs)
    }
    pub(super) fn readmit_ordinary(&self, memory: &MemoryBudget, mut cancelled: impl FnMut() -> bool) -> Result<Self, ValueTreeError> {
        self.check_budget(memory, self.class)?;
        if self.class == MemoryClass::Ordinary {
            return self.share_readmit(memory, &mut cancelled);
        }
        let mut out = EmergencyLifecycleBuilderV1::new(memory, MemoryClass::Ordinary)?;
        for row in &self.minimum_services {
            if cancelled() { return Err(ValueTreeError::Cancelled); }
            out.insert_minimum_service(row.outer_id.as_str(), row.operator_agent_id.as_str(), row.region_id.as_str(), row.registered_at)?;
        }
        for row in &self.authorizations {
            if cancelled() { return Err(ValueTreeError::Cancelled); }
            out.insert_authorization(row.outer_id.as_str(), row.id.as_str(), row.provider_id.as_str(), row.region_id.as_str(), row.active, row.starts_at, row.expires_at)?;
        }
        for row in &self.batches {
            if cancelled() { return Err(ValueTreeError::Cancelled); }
            out.insert_batch(row.outer_id.as_str(), row.authorization_id.as_str(), row.committed_at)?;
        }
        for row in &self.observations {
            if cancelled() { return Err(ValueTreeError::Cancelled); }
            out.insert_observation(row.as_str())?;
        }
        let active_inputs = self.active_inputs.readmit_ordinary(memory, &mut cancelled)?;
        if cancelled() { return Err(ValueTreeError::Cancelled); }
        out.finish(active_inputs)
    }
}

struct EmergencyLifecycleBuilderV1 {
    minimum_services: Vec<Arc<ReservedValue<LifecycleMinimumServiceV1>>>,
    authorizations: Vec<Arc<ReservedValue<LifecycleEmergencyAuthorizationV1>>>,
    batches: Vec<Arc<ReservedValue<LifecycleEmergencyBatchV1>>>,
    observations: Vec<Arc<ReservedValue<String>>>,
    minimum_service_slots: MemoryReservation,
    authorization_slots: MemoryReservation,
    batch_slots: MemoryReservation,
    observation_slots: MemoryReservation,
    container_fee: MemoryReservation,
    memory: MemoryBudget,
    class: MemoryClass,
}
impl EmergencyLifecycleBuilderV1 {
    fn new(memory: &MemoryBudget, class: MemoryClass) -> Result<Self, ValueTreeError> {
        Ok(Self {
            minimum_services: Vec::new(), authorizations: Vec::new(), batches: Vec::new(), observations: Vec::new(),
            minimum_service_slots: memory.reserve(0, class)?,
            authorization_slots: memory.reserve(0, class)?,
            batch_slots: memory.reserve(0, class)?,
            observation_slots: memory.reserve(0, class)?,
            container_fee: memory.reserve(size_of::<EmergencyLifecycleDirectoryV1>() + 256, class)?,
            memory: memory.clone(), class,
        })
    }
    fn ensure<T>(memory: &MemoryBudget, class: MemoryClass, rows: &mut Vec<T>, slots: &mut MemoryReservation, needed: usize) -> Result<(), ValueTreeError> {
        if needed <= rows.capacity() { return Ok(()); }
        let next = rows.capacity().checked_mul(2).ok_or_else(limited)?.max(4).max(needed);
        let bytes = next.checked_mul(size_of::<T>()).ok_or_else(limited)?;
        let fee = memory.reserve(bytes, class)?;
        rows.try_reserve_exact(next.saturating_sub(rows.len())).map_err(|_| limited())?;
        if rows.capacity() != next { return Err(invalid()); }
        *slots = fee;
        Ok(())
    }
    fn push_minimum_service_ref(&mut self, row: Arc<ReservedValue<LifecycleMinimumServiceV1>>, scan: &mut StateScanBudget, cancelled: &mut impl FnMut() -> bool) -> Result<(), ValueTreeError> {
        spend_counts(scan,cancelled,1,size_of::<usize>())?;
        if self.minimum_services.len()==self.minimum_services.capacity() { let cap=self.minimum_services.capacity().checked_mul(2).ok_or_else(limited)?.max(4);spend_counts(scan,cancelled,1,cap.checked_mul(size_of::<Arc<ReservedValue<LifecycleMinimumServiceV1>>>()).ok_or_else(limited)?)?; }
        let needed=self.minimum_services.len().checked_add(1).ok_or_else(limited)?;Self::ensure(&self.memory,self.class,&mut self.minimum_services,&mut self.minimum_service_slots,needed)?;
        self.minimum_services.push(row);Ok(())
    }
    fn push_minimum_service_ref_readmit(&mut self,row:Arc<ReservedValue<LifecycleMinimumServiceV1>>,cancelled:&mut impl FnMut()->bool)->Result<(),ValueTreeError>{if cancelled(){return Err(ValueTreeError::Cancelled);}let needed=self.minimum_services.len().checked_add(1).ok_or_else(limited)?;Self::ensure(&self.memory,self.class,&mut self.minimum_services,&mut self.minimum_service_slots,needed)?;self.minimum_services.push(row);Ok(())}
    fn push_authorization_ref(&mut self, row: Arc<ReservedValue<LifecycleEmergencyAuthorizationV1>>, scan: &mut StateScanBudget, cancelled: &mut impl FnMut() -> bool) -> Result<(), ValueTreeError> {
        spend_counts(scan,cancelled,1,size_of::<usize>())?;
        if self.authorizations.len()==self.authorizations.capacity() { let cap=self.authorizations.capacity().checked_mul(2).ok_or_else(limited)?.max(4);spend_counts(scan,cancelled,1,cap.checked_mul(size_of::<Arc<ReservedValue<LifecycleEmergencyAuthorizationV1>>>()).ok_or_else(limited)?)?; }
        let needed=self.authorizations.len().checked_add(1).ok_or_else(limited)?;Self::ensure(&self.memory,self.class,&mut self.authorizations,&mut self.authorization_slots,needed)?;
        self.authorizations.push(row);
        Ok(())
    }
    fn push_authorization_ref_readmit(&mut self,row:Arc<ReservedValue<LifecycleEmergencyAuthorizationV1>>,cancelled:&mut impl FnMut()->bool)->Result<(),ValueTreeError>{if cancelled(){return Err(ValueTreeError::Cancelled);}let needed=self.authorizations.len().checked_add(1).ok_or_else(limited)?;Self::ensure(&self.memory,self.class,&mut self.authorizations,&mut self.authorization_slots,needed)?;self.authorizations.push(row);Ok(())}
    fn push_batch_ref(&mut self, row: Arc<ReservedValue<LifecycleEmergencyBatchV1>>, scan: &mut StateScanBudget, cancelled: &mut impl FnMut() -> bool) -> Result<(), ValueTreeError> {
        spend_counts(scan,cancelled,1,size_of::<usize>())?;
        if self.batches.len()==self.batches.capacity() { let cap=self.batches.capacity().checked_mul(2).ok_or_else(limited)?.max(4);spend_counts(scan,cancelled,1,cap.checked_mul(size_of::<Arc<ReservedValue<LifecycleEmergencyBatchV1>>>()).ok_or_else(limited)?)?; }
        let needed=self.batches.len().checked_add(1).ok_or_else(limited)?;Self::ensure(&self.memory,self.class,&mut self.batches,&mut self.batch_slots,needed)?;
        self.batches.push(row);
        Ok(())
    }
    fn push_batch_ref_readmit(&mut self,row:Arc<ReservedValue<LifecycleEmergencyBatchV1>>,cancelled:&mut impl FnMut()->bool)->Result<(),ValueTreeError>{if cancelled(){return Err(ValueTreeError::Cancelled);}let needed=self.batches.len().checked_add(1).ok_or_else(limited)?;Self::ensure(&self.memory,self.class,&mut self.batches,&mut self.batch_slots,needed)?;self.batches.push(row);Ok(())}
    fn push_observation_ref(&mut self, row: Arc<ReservedValue<String>>, scan: &mut StateScanBudget, cancelled: &mut impl FnMut() -> bool) -> Result<(), ValueTreeError> {
        spend_counts(scan,cancelled,1,size_of::<usize>())?;
        if self.observations.len()==self.observations.capacity() { let cap=self.observations.capacity().checked_mul(2).ok_or_else(limited)?.max(4);spend_counts(scan,cancelled,1,cap.checked_mul(size_of::<Arc<ReservedValue<String>>>()).ok_or_else(limited)?)?; }
        let needed=self.observations.len().checked_add(1).ok_or_else(limited)?;Self::ensure(&self.memory,self.class,&mut self.observations,&mut self.observation_slots,needed)?;
        self.observations.push(row);
        Ok(())
    }
    fn push_observation_ref_readmit(&mut self,row:Arc<ReservedValue<String>>,cancelled:&mut impl FnMut()->bool)->Result<(),ValueTreeError>{if cancelled(){return Err(ValueTreeError::Cancelled);}let needed=self.observations.len().checked_add(1).ok_or_else(limited)?;Self::ensure(&self.memory,self.class,&mut self.observations,&mut self.observation_slots,needed)?;self.observations.push(row);Ok(())}
    fn find_auth(&self, outer: &str, scan: &mut StateScanBudget, cancelled: &mut impl FnMut() -> bool) -> Result<Result<usize,usize>, ValueTreeError> {
        for (at,row) in self.authorizations.iter().enumerate() {
            spend_counts(scan,cancelled,1,row.outer_id.len().checked_add(outer.len()).ok_or_else(limited)?)?;
            match row.outer_id.as_str().cmp(outer) { std::cmp::Ordering::Less=>{},std::cmp::Ordering::Equal=>return Ok(Ok(at)),std::cmp::Ordering::Greater=>return Ok(Err(at)) }
        }
        Ok(Err(self.authorizations.len()))
    }
    fn find_minimum_service(&self, outer: &str, scan: &mut StateScanBudget, cancelled: &mut impl FnMut() -> bool) -> Result<Result<usize,usize>, ValueTreeError> {
        for (at,row) in self.minimum_services.iter().enumerate() {
            spend_counts(scan,cancelled,1,row.outer_id.len().checked_add(outer.len()).ok_or_else(limited)?)?;
            match row.outer_id.as_str().cmp(outer) { std::cmp::Ordering::Less=>{},std::cmp::Ordering::Equal=>return Ok(Ok(at)),std::cmp::Ordering::Greater=>return Ok(Err(at)) }
        }
        Ok(Err(self.minimum_services.len()))
    }
    fn find_batch(&self, outer: &str, scan: &mut StateScanBudget, cancelled: &mut impl FnMut() -> bool) -> Result<Result<usize,usize>, ValueTreeError> {
        for (at,row) in self.batches.iter().enumerate() {
            spend_counts(scan,cancelled,1,row.outer_id.len().checked_add(outer.len()).ok_or_else(limited)?)?;
            match row.outer_id.as_str().cmp(outer) { std::cmp::Ordering::Less=>{},std::cmp::Ordering::Equal=>return Ok(Ok(at)),std::cmp::Ordering::Greater=>return Ok(Err(at)) }
        }
        Ok(Err(self.batches.len()))
    }
    fn find_observation(&self, outer: &str, scan: &mut StateScanBudget, cancelled: &mut impl FnMut() -> bool) -> Result<Result<usize,usize>, ValueTreeError> {
        for (at,row) in self.observations.iter().enumerate() {
            spend_counts(scan,cancelled,1,row.len().checked_add(outer.len()).ok_or_else(limited)?)?;
            match row.as_str().cmp(outer) { std::cmp::Ordering::Less=>{},std::cmp::Ordering::Equal=>return Ok(Ok(at)),std::cmp::Ordering::Greater=>return Ok(Err(at)) }
        }
        Ok(Err(self.observations.len()))
    }
    fn remove_authorization(&mut self, outer: &str, scan: &mut StateScanBudget, cancelled: &mut impl FnMut() -> bool) -> Result<(), ValueTreeError> {
        let Ok(at)=self.find_auth(outer,scan,cancelled)? else{return Err(invalid());};
        spend_counts(scan,cancelled,1,self.authorizations.len().saturating_sub(at+1).checked_mul(size_of::<Arc<ReservedValue<LifecycleEmergencyAuthorizationV1>>>()).ok_or_else(limited)?)?;
        self.authorizations.remove(at); Ok(())
    }
    fn remove_minimum_service(&mut self, outer: &str, scan: &mut StateScanBudget, cancelled: &mut impl FnMut() -> bool) -> Result<(), ValueTreeError> {
        let Ok(at)=self.find_minimum_service(outer,scan,cancelled)? else{return Err(invalid());};
        spend_counts(scan,cancelled,1,self.minimum_services.len().saturating_sub(at+1).checked_mul(size_of::<Arc<ReservedValue<LifecycleMinimumServiceV1>>>()).ok_or_else(limited)?)?;
        self.minimum_services.remove(at); Ok(())
    }
    fn remove_batch(&mut self, outer: &str, scan: &mut StateScanBudget, cancelled: &mut impl FnMut() -> bool) -> Result<(), ValueTreeError> {
        let Ok(at)=self.find_batch(outer,scan,cancelled)? else{return Err(invalid());};
        spend_counts(scan,cancelled,1,self.batches.len().saturating_sub(at+1).checked_mul(size_of::<Arc<ReservedValue<LifecycleEmergencyBatchV1>>>()).ok_or_else(limited)?)?;
        self.batches.remove(at); Ok(())
    }
    fn remove_observation(&mut self, outer: &str, scan: &mut StateScanBudget, cancelled: &mut impl FnMut() -> bool) -> Result<(), ValueTreeError> {
        let Ok(at)=self.find_observation(outer,scan,cancelled)? else{return Err(invalid());};
        spend_counts(scan,cancelled,1,self.observations.len().saturating_sub(at+1).checked_mul(size_of::<Arc<ReservedValue<String>>>()).ok_or_else(limited)?)?;
        self.observations.remove(at); Ok(())
    }
    fn insert_minimum_service(&mut self, outer: &str, operator: &str, region: &str, registered_at: u64) -> Result<(), ValueTreeError> {
        let bytes = outer.len().checked_add(operator.len()).and_then(|n|n.checked_add(region.len())).ok_or_else(limited)?;
        let fee_bytes=directory_arc_layout::<LifecycleMinimumServiceV1>()?.checked_add(size_of::<LifecycleMinimumServiceV1>()).and_then(|n|n.checked_add(bytes)).ok_or_else(limited)?;
        let fee=self.memory.reserve(fee_bytes,self.class)?;
        let row=LifecycleMinimumServiceV1{outer_id:string_copy(outer)?,operator_agent_id:string_copy(operator)?,region_id:string_copy(region)?,registered_at};
        if minimum_service_bytes(&row)?!=bytes{return Err(invalid());}
        let row=Arc::new(fee.retain(row));
        let pos=self.minimum_services.binary_search_by(|x|x.outer_id.cmp(&row.outer_id)).unwrap_or_else(|i|i);
        let needed=self.minimum_services.len().checked_add(1).ok_or_else(limited)?;Self::ensure(&self.memory,self.class,&mut self.minimum_services,&mut self.minimum_service_slots,needed)?;
        self.minimum_services.insert(pos,row); Ok(())
    }
    fn insert_authorization(&mut self, outer: &str, id: &str, provider: &str, region: &str, active: bool, starts_at: u64, expires_at: u64) -> Result<(), ValueTreeError> {
        let bytes = outer.len().checked_add(id.len()).and_then(|n|n.checked_add(provider.len())).and_then(|n|n.checked_add(region.len())).ok_or_else(limited)?;
        let fee_bytes=directory_arc_layout::<LifecycleEmergencyAuthorizationV1>()?.checked_add(size_of::<LifecycleEmergencyAuthorizationV1>()).and_then(|n|n.checked_add(bytes)).ok_or_else(limited)?;
        let fee=self.memory.reserve(fee_bytes,self.class)?;
        let [outer_id,id,provider_id,region_id] = strings_copy(&[outer,id,provider,region])?.try_into().map_err(|_|invalid())?;
        let row=LifecycleEmergencyAuthorizationV1{outer_id,id,provider_id,region_id,active,starts_at,expires_at};
        let bytes=authorization_bytes(&row)?;
        if bytes != fee_bytes.checked_sub(directory_arc_layout::<LifecycleEmergencyAuthorizationV1>()?).and_then(|n|n.checked_sub(size_of::<LifecycleEmergencyAuthorizationV1>())).ok_or_else(limited)? { return Err(invalid()); }
        let row=Arc::new(fee.retain(row));
        let pos=self.authorizations.binary_search_by(|x|x.outer_id.cmp(&row.outer_id)).unwrap_or_else(|i|i);
        let needed=self.authorizations.len().checked_add(1).ok_or_else(limited)?;Self::ensure(&self.memory,self.class,&mut self.authorizations,&mut self.authorization_slots,needed)?;
        self.authorizations.insert(pos,row); Ok(())
    }
    fn insert_batch(&mut self, outer: &str, authorization_id: &str, committed_at: Option<u64>) -> Result<(), ValueTreeError> {
        let bytes=outer.len().checked_add(authorization_id.len()).ok_or_else(limited)?;
        let fee_bytes=directory_arc_layout::<LifecycleEmergencyBatchV1>()?.checked_add(size_of::<LifecycleEmergencyBatchV1>()).and_then(|n|n.checked_add(bytes)).ok_or_else(limited)?;
        let fee=self.memory.reserve(fee_bytes,self.class)?;
        let row=LifecycleEmergencyBatchV1{outer_id:string_copy(outer)?,authorization_id:string_copy(authorization_id)?,committed_at};
        if batch_bytes(&row)?!=bytes{return Err(invalid());}
        let row=Arc::new(fee.retain(row));
        let pos=self.batches.binary_search_by(|x|x.outer_id.cmp(&row.outer_id)).unwrap_or_else(|i|i);
        let needed=self.batches.len().checked_add(1).ok_or_else(limited)?;Self::ensure(&self.memory,self.class,&mut self.batches,&mut self.batch_slots,needed)?;
        self.batches.insert(pos,row); Ok(())
    }
    fn insert_observation(&mut self, outer: &str) -> Result<(), ValueTreeError> {
        let fee_bytes=directory_arc_layout::<String>()?.checked_add(size_of::<String>()).and_then(|n|n.checked_add(outer.len())).ok_or_else(limited)?;
        let fee=self.memory.reserve(fee_bytes,self.class)?;
        let row=string_copy(outer)?;
        let row=Arc::new(fee.retain(row));
        let pos=self.observations.binary_search_by(|x|x.as_str().cmp(row.as_str())).unwrap_or_else(|i|i);
        let needed=self.observations.len().checked_add(1).ok_or_else(limited)?;Self::ensure(&self.memory,self.class,&mut self.observations,&mut self.observation_slots,needed)?;
        self.observations.insert(pos,row); Ok(())
    }
    fn finish(self, active_inputs: EmergencyActiveInputsV1) -> Result<EmergencyLifecycleDirectoryV1, ValueTreeError> {
        Ok(EmergencyLifecycleDirectoryV1 {
            authorizations:self.authorizations, batches:self.batches, observations:self.observations,
            active_inputs,
            minimum_services:self.minimum_services, minimum_service_slots:self.minimum_service_slots,
            authorization_slots:self.authorization_slots, batch_slots:self.batch_slots,
            observation_slots:self.observation_slots, memory:self.memory, class:self.class,
            container_fee:self.container_fee,
        })
    }
}
fn spend_counts(scan: &mut StateScanBudget, cancelled: &mut impl FnMut() -> bool, steps: usize, bytes: usize) -> Result<(),ValueTreeError> {
    if cancelled() { return Err(ValueTreeError::Cancelled); }
    scan.nodes_remaining=scan.nodes_remaining.checked_sub(u64::try_from(steps).map_err(|_|limited())?).ok_or_else(limited)?;
    scan.read_bytes_remaining=scan.read_bytes_remaining.checked_sub(u64::try_from(bytes).map_err(|_|limited())?).ok_or_else(limited)?;
    Ok(())
}

fn map_root<M,R,E,C>(body: Option<&StateRootV1>, name: &'static str, d: &mut ChangeDecoder<'_, '_, M,R,E,C>) -> Result<ReservedValue<StateRootV1>, ValueTreeError>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool {
    let Some(body)=body else {
        let empty=StateRootV1::empty(StateNamespaceV1::Authorization).map_err(|_|invalid())?;
        return Ok(d.ctx.values.memory.reserve(size_of::<StateRootV1>()+512,d.ctx.values.class)?.retain(empty));
    };
    let p=emergency_profile(); let heap=d.work.max_tree_heap;
    let row=point_typed_column_v1(&p,body,name,d,heap)?;
    let ColumnCellV1::Root(root)=&row.cell else{return Err(invalid());};
    let root=root.clone();
    Ok(d.ctx.values.memory.reserve(size_of::<StateRootV1>()+512,d.ctx.values.class)?.retain(root))
}
fn charge_projection_insert<M,R,E,C,T>(d:&mut ChangeDecoder<'_, '_,M,R,E,C>, count:usize, text_bytes:usize)->Result<(),ValueTreeError>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool {
    let steps=count.checked_add(1).ok_or_else(limited)?;
    let compare=text_bytes.checked_mul(steps).ok_or_else(limited)?;
    let move_bytes=count.checked_mul(size_of::<Arc<ReservedValue<T>>>()).ok_or_else(limited)?;
    spend(d,steps,compare.checked_add(move_bytes).ok_or_else(limited)?)
}
fn outer_key<M,R,E,C>(p:&TypedRecordProfileV1, cell:&TypedCellRefV1<'_>, d:&mut ChangeDecoder<'_, '_,M,R,E,C>) -> Result<Option<ReservedValue<ColumnKeyV1>>,ValueTreeError>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool {
    let parsed=key(p,cell,d)?;
    match &*parsed {
        ColumnKeyV1::Text(name) if name=="_length" => Ok(None),
        ColumnKeyV1::Pair(kind,value) if matches!(&**kind,ColumnKeyV1::Text(name) if name=="entry") => {
            let ColumnKeyV1::Text(_)=&**value else{return Err(invalid());};
            Ok(Some(parsed))
        }
        _=>Err(invalid()),
    }
}
fn outer_text(key:&ColumnKeyV1)->Result<&str,ValueTreeError>{
    let ColumnKeyV1::Pair(kind,value)=key else{return Err(invalid());};
    if !matches!(&**kind,ColumnKeyV1::Text(name) if name=="entry"){return Err(invalid());}
    let ColumnKeyV1::Text(outer)=&**value else{return Err(invalid());};
    Ok(outer)
}
fn index_authorizations<M,R,E,C>(builder:&mut EmergencyLifecycleBuilderV1,before:Option<&StateRootV1>,after:Option<&StateRootV1>,d:&mut ChangeDecoder<'_, '_,M,R,E,C>)->Result<(),ValueTreeError>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool {
    let p=emergency_profile();let old=map_root(before,"authorizations",d)?;let new=map_root(after,"authorizations",d)?;
    visit_entries(&*old,&*new,d,|before,after,d|{
        let cell=after.or(before).ok_or_else(invalid)?;
        let Some(parsed_outer)=outer_key(&p,cell,d)? else{return d.check();};let outer=outer_text(&parsed_outer)?;
        if before.is_some(){builder.remove_authorization(&outer,d.scan,&mut d.ctx.cancelled)?;}
        if let Some(cell)=after {
            let record=root(&p,cell,d)?;
            let value=recover_typed_record_body_v1::<Authorization,_,_,_,_>(&*record,&p,d.ctx,d.scan,d.work)?;
            let v=value.value();
            let text=outer.len().checked_add(v.id.len()).and_then(|n|n.checked_add(v.provider_id.len())).and_then(|n|n.checked_add(v.region_id.len())).ok_or_else(limited)?;
            charge_projection_insert::<_,_,_,_,LifecycleEmergencyAuthorizationV1>(d,builder.authorizations.len(),text)?;
            builder.insert_authorization(&outer,&v.id,&v.provider_id,&v.region_id,v.active,v.starts_at,v.expires_at)?;
        }
        d.check()
    })
}
fn index_batches<M,R,E,C>(builder:&mut EmergencyLifecycleBuilderV1,before:Option<&StateRootV1>,after:Option<&StateRootV1>,d:&mut ChangeDecoder<'_, '_,M,R,E,C>)->Result<(),ValueTreeError>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool {
    let p=emergency_profile();let old=map_root(before,"batches",d)?;let new=map_root(after,"batches",d)?;
    visit_entries(&*old,&*new,d,|before,after,d|{
        let cell=after.or(before).ok_or_else(invalid)?;
        let Some(parsed_outer)=outer_key(&p,cell,d)? else{return d.check();};let outer=outer_text(&parsed_outer)?;
        if before.is_some(){builder.remove_batch(&outer,d.scan,&mut d.ctx.cancelled)?;}
        if let Some(cell)=after {
            let record=root(&p,cell,d)?;
            let value=recover_typed_record_body_v1::<Batch,_,_,_,_>(&*record,&p,d.ctx,d.scan,d.work)?;
            let v=value.value();let text=outer.len().checked_add(v.authorization_id.len()).ok_or_else(limited)?;
            charge_projection_insert::<_,_,_,_,LifecycleEmergencyBatchV1>(d,builder.batches.len(),text)?;
            builder.insert_batch(&outer,&v.authorization_id,v.committed_at)?;
        }
        d.check()
    })
}
fn index_observations<M,R,E,C>(builder:&mut EmergencyLifecycleBuilderV1,before:Option<&StateRootV1>,after:Option<&StateRootV1>,d:&mut ChangeDecoder<'_, '_,M,R,E,C>)->Result<(),ValueTreeError>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool {
    let p=emergency_profile();let old=map_root(before,"observations",d)?;let new=map_root(after,"observations",d)?;
    visit_entries(&*old,&*new,d,|before,after,d|{
        let cell=after.or(before).ok_or_else(invalid)?;
        let Some(parsed_outer)=outer_key(&p,cell,d)? else{return d.check();};let outer=outer_text(&parsed_outer)?;
        if before.is_some(){builder.remove_observation(&outer,d.scan,&mut d.ctx.cancelled)?;}
        if let Some(cell)=after {
            let record=root(&p,cell,d)?;
            let _value=recover_typed_record_body_v1::<Observation,_,_,_,_>(&*record,&p,d.ctx,d.scan,d.work)?;
            charge_projection_insert::<_,_,_,_,String>(d,builder.observations.len(),outer.len())?;
            builder.insert_observation(&outer)?;
        }
        d.check()
    })
}

fn index_minimum_services<M,R,E,C>(builder:&mut EmergencyLifecycleBuilderV1,before:Option<&StateRootV1>,after:Option<&StateRootV1>,d:&mut ChangeDecoder<'_, '_,M,R,E,C>)->Result<(),ValueTreeError>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool {
    let p=emergency_profile();let old=map_root(before,"minimum_services",d)?;let new=map_root(after,"minimum_services",d)?;
    visit_entries(&*old,&*new,d,|before,after,d|{
        let cell=after.or(before).ok_or_else(invalid)?;
        let Some(parsed_outer)=outer_key(&p,cell,d)? else{return d.check();};let outer=outer_text(&parsed_outer)?;
        if before.is_some(){builder.remove_minimum_service(&outer,d.scan,&mut d.ctx.cancelled)?;}
        if let Some(cell)=after {
            let record=root(&p,cell,d)?;
            let value=recover_typed_record_body_v1::<MinimumService,_,_,_,_>(&*record,&p,d.ctx,d.scan,d.work)?;
            let v=value.value();let text=outer.len().checked_add(v.operator_agent_id.len()).and_then(|n|n.checked_add(v.registration.region_id.len())).ok_or_else(limited)?;
            charge_projection_insert::<_,_,_,_,LifecycleMinimumServiceV1>(d,builder.minimum_services.len(),text)?;
            builder.insert_minimum_service(&outer,&v.operator_agent_id,&v.registration.region_id,v.registered_at)?;
        }
        d.check()
    })
}

fn index_changes<M,R,E,C>(parent:Option<&EmergencyLifecycleDirectoryV1>,before:Option<&StateRootV1>,after:Option<&StateRootV1>,ctx:&mut WorldFieldReadContextV1<'_,M,R,E,C>,scan:&mut StateScanBudget,work:&mut TypedRecordReadWorkV1)->Result<EmergencyLifecycleDirectoryV1,ValueTreeError>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool {
    let mut builder=EmergencyLifecycleBuilderV1::new(ctx.values.memory,ctx.values.class)?;
    if let Some(parent)=parent {
        parent.check_budget(ctx.values.memory,ctx.values.class)?;
        for row in &parent.minimum_services { builder.push_minimum_service_ref(row.clone(),scan,&mut ctx.cancelled)?; }
        for row in &parent.authorizations { builder.push_authorization_ref(row.clone(),scan,&mut ctx.cancelled)?; }
        for row in &parent.batches { builder.push_batch_ref(row.clone(),scan,&mut ctx.cancelled)?; }
        for row in &parent.observations { builder.push_observation_ref(row.clone(),scan,&mut ctx.cancelled)?; }
    }
    let p=emergency_profile();let trees=work.max_retained_trees;
    let mut d=ChangeDecoder{ctx,scan,work,profile:p,trees_remaining:trees,tree_depth:0};
    index_minimum_services(&mut builder,before,after,&mut d)?;
    index_authorizations(&mut builder,before,after,&mut d)?;
    index_batches(&mut builder,before,after,&mut d)?;
    index_observations(&mut builder,before,after,&mut d)?;
    let active_inputs = close_inputs::update(parent.map(|directory| &directory.active_inputs), before, after, &mut d)?;
    d.check()?;
    builder.finish(active_inputs)
}

/// Update only the Emergency profile. `accept_body` has already verified the
/// complete original DTO; this child records the three maps consumed by the
/// fixed normal selector and shares unchanged rows on transitions.
pub(super) fn update<T,M,R,E,C>(parent:Option<&EmergencyLifecycleDirectoryV1>,before:Option<&StateRootV1>,after:Option<&StateRootV1>,ctx:&mut WorldFieldReadContextV1<'_,M,R,E,C>,scan:&mut StateScanBudget,work:&mut TypedRecordReadWorkV1)->Result<Option<EmergencyLifecycleDirectoryV1>,ValueTreeError>
where T:CompositeBranchCodecV1,M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool {
    if <T as CompositeBranchCodecV1>::BRANCH != WorldRuleCompositeBranchV1::EmergencySupply { return Ok(None); }
    if before==after {
        if let Some(parent)=parent { return Ok(Some(parent.share(ctx.values.memory,ctx.values.class,scan,&mut ctx.cancelled)?)); }
        if before.is_some() { return Err(invalid()); }
        let active_inputs = close_inputs::EmergencyActiveInputsV1::empty(ctx.values.memory, ctx.values.class)?;
        return Ok(Some(EmergencyLifecycleBuilderV1::new(ctx.values.memory,ctx.values.class)?.finish(active_inputs)?));
    }
    if before.is_some()&&parent.is_none() { return Err(invalid()); }
    Ok(Some(index_changes(parent,before,after,ctx,scan,work)?))
}

/// Fold the transaction's actual Emergency body into the accepted projection.
/// The caller must first bind/validate that body with the closed branch codec;
/// this helper only updates selector data and does not authorize a command.
pub(crate) fn project_current<M, R, E, C>(
    parent: &EmergencyLifecycleDirectoryV1,
    before: Option<&StateRootV1>,
    after: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
) -> Result<EmergencyLifecycleDirectoryV1, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    let directory = index_changes(Some(parent), before, Some(after), ctx, scan, work)?;
    ctx.check()?;
    Ok(directory)
}

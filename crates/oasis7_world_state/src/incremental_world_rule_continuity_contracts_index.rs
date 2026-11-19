//! Complete resident registration->original contract outer-key directory.
//! This is paid metadata, never a settlement/event permission.
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{
    mem::{align_of, size_of},
    sync::Arc,
};
type Fee<'a> = dyn FnMut(u64, u64) -> Result<(), ValueTreeError> + 'a;
fn limit() -> ValueTreeError {
    ValueTreeError::ResourceLimited("continuity contract directory layout")
}
fn add(a: usize, b: usize) -> Result<usize, ValueTreeError> {
    a.checked_add(b).ok_or_else(limit)
}
fn mul(a: usize, b: usize) -> Result<usize, ValueTreeError> {
    a.checked_mul(b).ok_or_else(limit)
}
fn u(n: usize) -> Result<u64, ValueTreeError> {
    u64::try_from(n).map_err(|_| limit())
}
fn arc<T>() -> Result<usize, ValueTreeError> {
    add(
        size_of::<ReservedValue<T>>(),
        add(2 * size_of::<usize>(), 2 * align_of::<ReservedValue<T>>())?,
    )
}
struct Row {
    outer: String,
    organization: String,
    contracts: Vec<String>,
}
type Held = Arc<ReservedValue<Row>>;
struct Backing {
    rows: Vec<Held>,
    _vector: MemoryReservation,
    _carrier: MemoryReservation,
}
pub(crate) struct ContinuityContractIndexV1 {
    data: Arc<ReservedValue<Backing>>,
    memory: MemoryBudget,
    class: MemoryClass,
}
impl std::fmt::Debug for ContinuityContractIndexV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContinuityContractIndexV1")
            .field("registrations", &self.data.rows.len())
            .finish_non_exhaustive()
    }
}
pub(crate) struct Builder {
    rows: Vec<Held>,
    vector: MemoryReservation,
    carrier: MemoryReservation,
    memory: MemoryBudget,
    class: MemoryClass,
}
impl Builder {
    pub(crate) fn new(memory: &MemoryBudget, class: MemoryClass) -> Result<Self, ValueTreeError> {
        Ok(Self {
            rows: Vec::new(),
            vector: memory.reserve(0, class)?,
            carrier: memory.reserve(size_of::<Self>(), class)?,
            memory: memory.clone(),
            class,
        })
    }
    fn capacity(&mut self, n: usize) -> Result<(), ValueTreeError> {
        if n <= self.rows.capacity() {
            return Ok(());
        }
        let cap = self
            .rows
            .capacity()
            .checked_mul(2)
            .ok_or_else(limit)?
            .max(4)
            .max(n);
        let fee = self
            .memory
            .reserve(mul(cap, size_of::<Held>())?, self.class)?;
        self.rows
            .try_reserve_exact(cap - self.rows.len())
            .map_err(|_| limit())?;
        if self.rows.capacity() > cap {
            return Err(limit());
        }
        self.vector = fee;
        Ok(())
    }
    pub(crate) fn remove(&mut self, outer: &str, fee: &mut Fee<'_>) -> Result<(), ValueTreeError> {
        for i in 0..self.rows.len() {
            fee(1, u(add(self.rows[i].outer.len(), outer.len())?)?)?;
            if self.rows[i].outer == outer {
                fee(1, u(mul(self.rows.len() - i, size_of::<Held>())?)?)?;
                self.rows.remove(i);
                break;
            }
        }
        Ok(())
    }
    pub(crate) fn put(
        &mut self,
        outer: &str,
        registration: &crate::world_rules::OrganizationRegistrationV1,
        fee: &mut Fee<'_>,
    ) -> Result<(), ValueTreeError> {
        let mut bytes = add(
            arc::<Row>()?,
            add(outer.len(), registration.organization_id.len())?,
        )?;
        bytes = add(
            bytes,
            mul(registration.activity_sources.len(), size_of::<String>())?,
        )?;
        for id in registration.activity_sources.keys() {
            fee(1, u(id.len())?)?;
            bytes = add(bytes, id.len())?;
        }
        fee(1, u(add(outer.len(), registration.organization_id.len())?)?)?;
        let paid = self.memory.reserve(bytes, self.class)?;
        let mut contracts = Vec::with_capacity(registration.activity_sources.len());
        if contracts.capacity() > registration.activity_sources.len() {
            return Err(limit());
        }
        for id in registration.activity_sources.keys() {
            fee(1, u(id.len())?)?;
            let value = id.clone();
            if value.capacity() > id.len() {
                return Err(limit());
            }
            contracts.push(value);
        }
        let outer_copy = outer.to_owned();
        let organization = registration.organization_id.clone();
        if outer_copy.capacity() > outer.len()
            || organization.capacity() > registration.organization_id.len()
        {
            return Err(limit());
        }
        self.remove(outer, fee)?;
        self.capacity(add(self.rows.len(), 1)?)?;
        self.rows.push(Arc::new(paid.retain(Row {
            outer: outer_copy,
            organization,
            contracts,
        })));
        Ok(())
    }
    pub(crate) fn finish(self) -> Result<ContinuityContractIndexV1, ValueTreeError> {
        let fee = self.memory.reserve(arc::<Backing>()?, self.class)?;
        Ok(ContinuityContractIndexV1 {
            data: Arc::new(fee.retain(Backing {
                rows: self.rows,
                _vector: self.vector,
                _carrier: self.carrier,
            })),
            memory: self.memory,
            class: self.class,
        })
    }
}
impl ContinuityContractIndexV1 {
    pub(crate) fn share(
        &self,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Self, ValueTreeError> {
        if !self.memory.same_pool(memory) || self.class != class || !self.data.uses_budget(memory) {
            return Err(ValueTreeError::Invalid(
                "continuity contract index pool/class",
            ));
        }
        Ok(Self {
            data: self.data.clone(),
            memory: memory.clone(),
            class,
        })
    }
    pub(crate) fn copy_for_changes(
        &self,
        memory: &MemoryBudget,
        class: MemoryClass,
        fee: &mut Fee<'_>,
    ) -> Result<Builder, ValueTreeError> {
        let _proof = self.share(memory, class)?;
        let mut next = Builder::new(memory, class)?;
        next.capacity(self.data.rows.len())?;
        for row in &self.data.rows {
            fee(1, u(size_of::<Held>())?)?;
            next.rows.push(row.clone());
        }
        Ok(next)
    }
    pub(crate) fn readmit_ordinary(
        &self,
        memory: &MemoryBudget,
        fee: &mut Fee<'_>,
    ) -> Result<Self, ValueTreeError> {
        if self.class == MemoryClass::Ordinary {
            return self.share(memory, MemoryClass::Ordinary);
        }
        let _proof = self.share(memory, MemoryClass::Recovery)?;
        let mut out = Builder::new(memory, MemoryClass::Ordinary)?;
        out.capacity(self.data.rows.len())?;
        for row in &self.data.rows {
            let mut bytes = add(arc::<Row>()?, add(row.outer.len(), row.organization.len())?)?;
            bytes = add(bytes, mul(row.contracts.len(), size_of::<String>())?)?;
            for text in &row.contracts {
                fee(1, u(text.len())?)?;
                bytes = add(bytes, text.len())?;
            }
            let paid = memory.reserve(bytes, MemoryClass::Ordinary)?;
            fee(1, u(add(row.outer.len(), row.organization.len())?)?)?;
            let contracts = row.contracts.clone();
            let outer = row.outer.clone();
            let organization = row.organization.clone();
            if contracts.capacity() > row.contracts.len()
                || outer.capacity() > row.outer.len()
                || organization.capacity() > row.organization.len()
                || contracts
                    .iter()
                    .zip(&row.contracts)
                    .any(|(a, b)| a.capacity() > b.len())
            {
                return Err(limit());
            }
            out.rows.push(Arc::new(paid.retain(Row {
                outer,
                organization,
                contracts,
            })));
        }
        out.finish()
    }
    pub(crate) fn matching(
        &self,
        contract: &str,
        memory: &MemoryBudget,
        class: MemoryClass,
        fee: &mut Fee<'_>,
    ) -> Result<ReservedValue<Vec<(String, String)>>, ValueTreeError> {
        let mut count = 0usize;
        let mut payload = 0usize;
        self.visit_matching(contract, memory, class, fee, &mut |outer, organization| {
            count = add(count, 1)?;
            payload = add(payload, add(outer.len(), organization.len())?)?;
            Ok(())
        })?;
        let bytes = add(
            size_of::<Vec<(String, String)>>(),
            add(mul(count, size_of::<(String, String)>())?, payload)?,
        )?;
        let paid = memory.reserve(bytes, class)?;
        let mut rows = Vec::with_capacity(count);
        if rows.capacity() > count {
            return Err(limit());
        }
        self.visit_matching(contract, memory, class, fee, &mut |outer, organization| {
            let a = outer.to_owned();
            let b = organization.to_owned();
            if a.capacity() > outer.len() || b.capacity() > organization.len() {
                return Err(limit());
            }
            rows.push((a, b));
            Ok(())
        })?;
        Ok(paid.retain(rows))
    }
    /// Scans only the resident directory. Selected Current DTO reads happen after
    /// this method, never by traversing all registration pages on each action.
    pub(crate) fn visit_matching(
        &self,
        contract: &str,
        memory: &MemoryBudget,
        class: MemoryClass,
        fee: &mut Fee<'_>,
        visit: &mut dyn FnMut(&str, &str) -> Result<(), ValueTreeError>,
    ) -> Result<(), ValueTreeError> {
        let _proof = self.share(memory, class)?;
        for row in &self.data.rows {
            for id in &row.contracts {
                fee(1, u(add(id.len(), contract.len())?)?)?;
                if id == contract {
                    fee(1, u(add(row.outer.len(), row.organization.len())?)?)?;
                    visit(&row.outer, &row.organization)?;
                    break;
                }
            }
        }
        Ok(())
    }
}

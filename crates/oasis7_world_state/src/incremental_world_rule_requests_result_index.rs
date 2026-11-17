//! Complete resident receipt result->original outer-key directory.
//! This is paid metadata, never a settlement/event permission.
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{
    mem::{align_of, size_of},
    sync::Arc,
};
type Fee<'a> = dyn FnMut(u64, u64) -> Result<(), ValueTreeError> + 'a;
fn limit() -> ValueTreeError {
    ValueTreeError::ResourceLimited("request result directory layout")
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
// Both the exact outer key and its authenticated value remain available to a
// selected Current reader. Neither actor/request nor a result hash substitutes
// for the original map entry.
struct Row {
    outer: String,
    organization: String,
    request: String,
    result: String,
    value: [u8; 32],
}
type Held = Arc<ReservedValue<Row>>;
pub(crate) struct SelectedRequestV1(Held);
impl SelectedRequestV1 {
    pub(crate) fn outer(&self) -> &str {
        &self.0.outer
    }
    pub(crate) fn value(&self) -> [u8; 32] {
        self.0.value
    }
    pub(crate) fn validate_receipt(
        &self,
        receipt: &crate::world_rules::WorldRuleReceiptV1,
        fee: &mut Fee<'_>,
    ) -> Result<(), ValueTreeError> {
        for (expected, actual) in [
            (&self.0.organization, &receipt.actor_id),
            (&self.0.request, &receipt.request_id),
            (&self.0.result, &receipt.result_id),
        ] {
            fee(1, u(add(expected.len(), actual.len())?)?)?;
            if expected != actual {
                return Err(ValueTreeError::Invalid("request result source binding"));
            }
        }
        Ok(())
    }
}
/// The result payload is written directly into its prepaid final allocation.
/// Data precedes the reservation so every failure drops bytes before capacity.
pub(crate) struct PendingRowV1 {
    outer: String,
    organization: String,
    request: String,
    result: Vec<u8>,
    value: [u8; 32],
    charge: MemoryReservation,
}
impl PendingRowV1 {
    pub(crate) fn result_bytes(&mut self) -> &mut [u8] {
        self.result.as_mut_slice()
    }
}
struct Backing {
    rows: Vec<Held>,
    _vector: MemoryReservation,
    _carrier: MemoryReservation,
}
pub(crate) struct RequestsResultIndexV1 {
    data: Arc<ReservedValue<Backing>>,
    memory: MemoryBudget,
    class: MemoryClass,
}
impl std::fmt::Debug for RequestsResultIndexV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RequestsResultIndexV1")
            .field("receipts", &self.data.rows.len())
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
    pub(crate) fn prepare(
        &self,
        outer: &str,
        actor: &str,
        request: &str,
        result_bytes: usize,
        value: [u8; 32],
        fee: &mut Fee<'_>,
    ) -> Result<PendingRowV1, ValueTreeError> {
        fee(1, u(add(actor.len(), request.len())?)?)?;
        let mut bytes = add(arc::<Row>()?, add(outer.len(), actor.len())?)?;
        for length in [result_bytes, request.len()] {
            fee(1, u(length)?)?;
            bytes = add(bytes, length)?;
        }
        fee(1, u(add(outer.len(), actor.len())?)?)?;
        let paid = self.memory.reserve(bytes, self.class)?;
        // Charge the actual payload copy before allocating or reading it. A
        // failed read cannot publish this incomplete row or refund spent work.
        fee(1, u(result_bytes)?)?;
        fee(1, u(request.len())?)?;
        let result = vec![0; result_bytes];
        let outer = outer.to_owned();
        let organization = actor.to_owned();
        let request_copy = request.to_owned();
        if result.capacity() > result_bytes
            || outer.capacity() > outer.len()
            || organization.capacity() > actor.len()
            || request_copy.capacity() > request.len()
        {
            return Err(limit());
        }
        Ok(PendingRowV1 {
            outer,
            organization,
            request: request_copy,
            result,
            value,
            charge: paid,
        })
    }
    pub(crate) fn put(
        &mut self,
        mut row: PendingRowV1,
        fee: &mut Fee<'_>,
    ) -> Result<(), ValueTreeError> {
        fee(1, u(row.result.len())?)?;
        let result = String::from_utf8(std::mem::take(&mut row.result))
            .map_err(|_| ValueTreeError::Invalid("request result UTF8"))?;
        self.remove(&row.outer, fee)?;
        self.capacity(add(self.rows.len(), 1)?)?;
        // Original requests.values() traverses outer BTree keys. Preserve that
        // order after sparse replacement as well as after cold construction.
        let mut position = self.rows.len();
        for (index, existing) in self.rows.iter().enumerate() {
            fee(1, u(add(existing.outer.len(), row.outer.len())?)?)?;
            if existing.outer.as_str() > row.outer.as_str() {
                position = index;
                break;
            }
        }
        fee(1, u(mul(self.rows.len() - position, size_of::<Held>())?)?)?;
        self.rows.insert(
            position,
            Arc::new(row.charge.retain(Row {
                outer: row.outer,
                organization: row.organization,
                request: row.request,
                result,
                value: row.value,
            })),
        );
        Ok(())
    }
    pub(crate) fn finish(self) -> Result<RequestsResultIndexV1, ValueTreeError> {
        let fee = self.memory.reserve(arc::<Backing>()?, self.class)?;
        Ok(RequestsResultIndexV1 {
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
impl RequestsResultIndexV1 {
    pub(crate) fn share(
        &self,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Self, ValueTreeError> {
        if !self.memory.same_pool(memory) || self.class != class || !self.data.uses_budget(memory) {
            return Err(ValueTreeError::Invalid("request result index pool/class"));
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
            for text in [&row.result, &row.request] {
                fee(1, u(text.len())?)?;
                bytes = add(bytes, text.len())?;
            }
            let paid = memory.reserve(bytes, MemoryClass::Ordinary)?;
            fee(1, u(add(row.outer.len(), row.organization.len())?)?)?;
            let result = row.result.clone();
            let request = row.request.clone();
            let outer = row.outer.clone();
            let organization = row.organization.clone();
            if outer.capacity() > row.outer.len()
                || organization.capacity() > row.organization.len()
                || result.capacity() > row.result.len()
                || request.capacity() > row.request.len()
            {
                return Err(limit());
            }
            out.rows.push(Arc::new(paid.retain(Row {
                outer,
                organization,
                request,
                result,
                value: row.value,
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
    ) -> Result<ReservedValue<Vec<SelectedRequestV1>>, ValueTreeError> {
        let mut count = 0usize;
        self.visit_matching(contract, memory, class, fee, &mut |_, _| {
            count = add(count, 1)?;
            Ok(())
        })?;
        let bytes = add(
            size_of::<Vec<SelectedRequestV1>>(),
            mul(count, size_of::<SelectedRequestV1>())?,
        )?;
        let paid = memory.reserve(bytes, class)?;
        let mut rows = Vec::with_capacity(count);
        if rows.capacity() > count {
            return Err(limit());
        }
        self.visit_matching(contract, memory, class, fee, &mut |row, fee| {
            fee(1, u(size_of::<SelectedRequestV1>())?)?;
            rows.push(SelectedRequestV1(row.clone()));
            Ok(())
        })?;
        Ok(paid.retain(rows))
    }
    /// Scans only the resident directory. Selected Current DTO reads happen after
    /// this method, never by traversing all receipt pages on each action.
    fn visit_matching(
        &self,
        contract: &str,
        memory: &MemoryBudget,
        class: MemoryClass,
        fee: &mut Fee<'_>,
        visit: &mut dyn FnMut(&Held, &mut Fee<'_>) -> Result<(), ValueTreeError>,
    ) -> Result<(), ValueTreeError> {
        let _proof = self.share(memory, class)?;
        for row in &self.data.rows {
            fee(1, u(add(row.result.len(), contract.len())?)?)?;
            if row.result == contract {
                fee(1, u(add(row.outer.len(), row.organization.len())?)?)?;
                visit(row, fee)?;
            }
        }
        Ok(())
    }
}

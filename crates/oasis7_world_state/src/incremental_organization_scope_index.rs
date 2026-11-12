//! Paid resident metadata for fixed organization predicates.
//! Parent qualification builds it once; ordinary receivers update only changed
//! records. Iterating this small metadata directory never opens World pages.
use crate::incremental_typed_record_codec::btree_output_heap;
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{
    collections::BTreeMap,
    fmt,
    mem::{align_of, size_of},
    sync::Arc,
};

#[derive(Clone, Copy, Debug)]
pub(crate) enum OrganizationScopeKindV1 {
    Continuity,
    Protection,
    Estate,
}
impl OrganizationScopeKindV1 {
    fn index(self) -> usize {
        self as usize
    }
}
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct OrganizationScopeFlagsV1 {
    pub(crate) active_case: bool,
    pub(crate) risk_frozen: bool,
    pub(crate) undisposed_estate: bool,
}
struct Row {
    organization: String,
    flags: OrganizationScopeFlagsV1,
}
type Table = BTreeMap<String, Row>;
struct Tables {
    rows: [Table; 3],
    max_key_bytes: [usize; 3],
}
type Data = Arc<ReservedValue<Tables>>;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("organization scope metadata")
}
fn bound() -> ValueTreeError {
    ValueTreeError::ResourceLimited("organization scope metadata bound")
}
fn arc_bytes<T>() -> Result<usize, ValueTreeError> {
    size_of::<ReservedValue<T>>()
        .checked_add(2 * size_of::<usize>())
        .and_then(|n| n.checked_add(2 * align_of::<ReservedValue<T>>()))
        .ok_or_else(bound)
}
fn charge_text(
    charge: &mut impl FnMut(u64, u64) -> Result<(), ValueTreeError>,
    steps: usize,
    bytes: usize,
) -> Result<(), ValueTreeError> {
    charge(
        u64::try_from(steps).map_err(|_| bound())?,
        u64::try_from(bytes).map_err(|_| bound())?,
    )
}

/// No public constructor/serde/clone. Possessing this directory alone grants
/// no qualification: its fixed owner also binds the actual source roots.
pub(crate) struct OrganizationScopeIndexV1 {
    data: Data,
    memory: MemoryBudget,
    class: MemoryClass,
    backing_class: MemoryClass,
    ordinary_admission: Option<Arc<ReservedValue<()>>>,
}
impl fmt::Debug for OrganizationScopeIndexV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OrganizationScopeIndexV1")
            .field("records", &self.data.rows.each_ref().map(BTreeMap::len))
            .field("class", &self.class)
            .finish_non_exhaustive()
    }
}
impl OrganizationScopeIndexV1 {
    pub(crate) fn check_budget(
        &self,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<(), ValueTreeError> {
        if self.class != class || !self.memory.same_pool(memory) || !self.data.uses_budget(memory) {
            return Err(invalid());
        }
        if self.backing_class != class {
            let admitted = self.backing_class == MemoryClass::Recovery
                && class == MemoryClass::Ordinary
                && self.ordinary_admission.as_ref().is_some_and(|fee| {
                    fee.uses_budget(memory) && fee.reserved_bytes() >= self.data.reserved_bytes()
                });
            if !admitted {
                return Err(invalid());
            }
        }
        Ok(())
    }
    /// Share an unchanged, already-paid immutable directory, never its balance.
    pub(crate) fn share(
        &self,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Self, ValueTreeError> {
        self.check_budget(memory, class)?;
        Ok(Self {
            data: self.data.clone(),
            memory: memory.clone(),
            class,
            backing_class: self.backing_class,
            ordinary_admission: self.ordinary_admission.clone(),
        })
    }
    /// Constant-time Recovery admission. The immutable backing and its original
    /// fee stay alive; a NEW Ordinary reservation covers its entire retained
    /// upper bound before sharing it. This is deliberately conservative double
    /// admission, not changing a class flag or copying a work balance.
    pub(crate) fn readmit_ordinary(&self, memory: &MemoryBudget) -> Result<Self, ValueTreeError> {
        if self.class == MemoryClass::Ordinary {
            return self.share(memory, MemoryClass::Ordinary);
        }
        self.check_budget(memory, MemoryClass::Recovery)?;
        let bytes = self
            .data
            .reserved_bytes()
            .checked_add(arc_bytes::<()>()?)
            .ok_or_else(bound)?;
        let fee = memory.reserve(bytes, MemoryClass::Ordinary)?;
        let admission = Arc::new(fee.retain(()));
        Ok(Self {
            data: self.data.clone(),
            memory: memory.clone(),
            class: MemoryClass::Ordinary,
            backing_class: self.backing_class,
            ordinary_admission: Some(admission),
        })
    }
    /// Private fixed callers choose the kind. Every comparison spends caller
    /// work and cancellation through the fee-only callback; values are data.
    pub(crate) fn observe(
        &self,
        kind: OrganizationScopeKindV1,
        organization: &str,
        memory: &MemoryBudget,
        class: MemoryClass,
        charge: &mut impl FnMut(u64, u64) -> Result<(), ValueTreeError>,
    ) -> Result<OrganizationScopeFlagsV1, ValueTreeError> {
        self.check_budget(memory, class)?;
        charge(1, 0)?;
        let mut out = OrganizationScopeFlagsV1::default();
        for row in self.data.rows[kind.index()].values() {
            charge_text(
                charge,
                1,
                organization
                    .len()
                    .checked_add(row.organization.len())
                    .ok_or_else(bound)?,
            )?;
            if row.organization == organization {
                out.active_case |= row.flags.active_case;
                out.risk_frozen |= row.flags.risk_frozen;
                out.undisposed_estate |= row.flags.undisposed_estate;
            }
        }
        charge(1, 0)?;
        Ok(out)
    }
    /// Copies resident metadata only when actual typed child roots changed.
    /// No World DTO, quote, history payload or page is loaded here.
    pub(crate) fn copy_for_changes(
        &self,
        memory: &MemoryBudget,
        class: MemoryClass,
        charge: &mut impl FnMut(u64, u64) -> Result<(), ValueTreeError>,
    ) -> Result<OrganizationScopeIndexBuilderV1, ValueTreeError> {
        self.check_budget(memory, class)?;
        let mut next = OrganizationScopeIndexBuilderV1::new(memory, class)?;
        for kind in [
            OrganizationScopeKindV1::Continuity,
            OrganizationScopeKindV1::Protection,
            OrganizationScopeKindV1::Estate,
        ] {
            for (outer, row) in &self.data.rows[kind.index()] {
                next.put(kind, outer, &row.organization, row.flags, charge)?;
            }
        }
        Ok(next)
    }
}

pub(crate) struct OrganizationScopeIndexBuilderV1 {
    // Drop all data before returning its admitted memory.
    tables: Tables,
    charge: MemoryReservation,
    memory: MemoryBudget,
    class: MemoryClass,
}
impl OrganizationScopeIndexBuilderV1 {
    pub(crate) fn new(memory: &MemoryBudget, class: MemoryClass) -> Result<Self, ValueTreeError> {
        if !matches!(class, MemoryClass::Ordinary | MemoryClass::Recovery) {
            return Err(invalid());
        }
        Ok(Self {
            tables: Tables {
                rows: std::array::from_fn(|_| BTreeMap::new()),
                max_key_bytes: [0; 3],
            },
            charge: memory.reserve(0, class)?,
            memory: memory.clone(),
            class,
        })
    }
    fn lookup_work(
        &self,
        kind: usize,
        key: &str,
        charge: &mut impl FnMut(u64, u64) -> Result<(), ValueTreeError>,
    ) -> Result<(), ValueTreeError> {
        // A BTree search visits at most log2(n)+1 levels with at most eleven
        // String comparisons per node. Charge before the library lookup.
        let levels =
            usize::try_from(self.tables.rows[kind].len().max(1).ilog2()).map_err(|_| bound())? + 1;
        let comparisons = levels.checked_mul(11).ok_or_else(bound)?;
        let bytes = key
            .len()
            .checked_add(self.tables.max_key_bytes[kind])
            .and_then(|n| n.checked_mul(comparisons))
            .ok_or_else(bound)?;
        charge_text(charge, comparisons, bytes)
    }
    pub(crate) fn put(
        &mut self,
        kind: OrganizationScopeKindV1,
        outer: &str,
        organization: &str,
        flags: OrganizationScopeFlagsV1,
        charge: &mut impl FnMut(u64, u64) -> Result<(), ValueTreeError>,
    ) -> Result<(), ValueTreeError> {
        let i = kind.index();
        self.lookup_work(i, outer, charge)?;
        self.lookup_work(i, outer, charge)?;
        let fresh = !self.tables.rows[i].contains_key(outer);
        let payload = organization
            .len()
            .checked_add(if fresh { outer.len() } else { 0 })
            .ok_or_else(bound)?;
        let bytes = payload
            .checked_add(if fresh {
                btree_output_heap::<String, Row>(1)?
            } else {
                0
            })
            .ok_or_else(bound)?;
        charge_text(charge, 1, bytes)?;
        let mut fee = self.memory.reserve(bytes, self.class)?;
        self.charge
            .merge(&mut fee)
            .map_err(ValueTreeError::Invalid)?;
        let text = copy_text(organization)?;
        if fresh {
            let key = copy_text(outer)?;
            self.tables.max_key_bytes[i] = self.tables.max_key_bytes[i].max(key.len());
            self.tables.rows[i].insert(
                key,
                Row {
                    organization: text,
                    flags,
                },
            );
        } else {
            let row = self.tables.rows[i].get_mut(outer).ok_or_else(invalid)?;
            *row = Row {
                organization: text,
                flags,
            };
        }
        Ok(())
    }
    pub(crate) fn remove(
        &mut self,
        kind: OrganizationScopeKindV1,
        outer: &str,
        charge: &mut impl FnMut(u64, u64) -> Result<(), ValueTreeError>,
    ) -> Result<(), ValueTreeError> {
        let i = kind.index();
        self.lookup_work(i, outer, charge)?;
        self.tables.rows[i].remove(outer);
        // Keep the old charge/max bound: deletion may retain the empty root
        // node, and no unproven allocator capacity is refunded.
        Ok(())
    }
    pub(crate) fn finish(mut self) -> Result<OrganizationScopeIndexV1, ValueTreeError> {
        let mut fee = self.memory.reserve(arc_bytes::<Tables>()?, self.class)?;
        self.charge
            .merge(&mut fee)
            .map_err(ValueTreeError::Invalid)?;
        let data = Arc::new(self.charge.retain(self.tables));
        Ok(OrganizationScopeIndexV1 {
            data,
            memory: self.memory,
            class: self.class,
            backing_class: self.class,
            ordinary_admission: None,
        })
    }
}
fn copy_text(value: &str) -> Result<String, ValueTreeError> {
    let mut out = String::new();
    out.try_reserve_exact(value.len()).map_err(|_| bound())?;
    if out.capacity() > value.len() {
        return Err(bound());
    }
    out.push_str(value);
    Ok(out)
}

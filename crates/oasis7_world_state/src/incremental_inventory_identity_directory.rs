//! Resident original inventory identities. This is source DATA, not execution
//! permission. GEN/cold walks the qualified source once; Current folds writes.
use super::*;
use crate::MaterialLedgerId;
use crate::incremental_semantic_key::LedgerKeyRefV1;
use std::{
    cmp::Ordering,
    mem::{align_of, size_of},
    sync::Arc,
};
#[path = "incremental_inventory_identity_directory_current.rs"]
mod current;
#[path = "incremental_inventory_identity_directory_tree.rs"]
mod tree;
pub use current::recover_current_inventory_ledger_v1;
pub struct InventoryIdentityDirectoryWorkV1 {
    pub visits_remaining: u64,
    pub comparison_bytes_remaining: u64,
    pub pending_entries_remaining: usize,
    pub max_identity_bytes: usize,
    pub max_output_entries: usize,
}
impl InventoryIdentityDirectoryWorkV1 {
    fn step(
        &mut self,
        bytes: usize,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        self.visits_remaining = self
            .visits_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("inventory identity visits"))?;
        self.comparison_bytes_remaining = self
            .comparison_bytes_remaining
            .checked_sub(bytes as u64)
            .ok_or(ValueTreeError::ResourceLimited(
                "inventory identity comparison bytes",
            ))?;
        Ok(())
    }
}
pub struct InventoryIdentityDirectoryV1 {
    roots: [u8; 32],
    tree: Option<tree::NodeRef>,
}
impl InventoryIdentityDirectoryV1 {
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        if self.roots != roots.digest().map_err(|_| invalid())? {
            return Err(invalid());
        }
        Ok(())
    }
}
#[derive(Clone, Copy)]
struct IdentityRef<'a> {
    ledger: Option<LedgerKeyRefV1<'a>>,
    material: Option<&'a str>,
}
struct Identity {
    ledger: Option<MaterialLedgerId>,
    material: Option<String>,
}
impl Identity {
    fn borrowed(&self) -> IdentityRef<'_> {
        IdentityRef {
            ledger: self.ledger.as_ref().map(ledger_ref),
            material: self.material.as_deref(),
        }
    }
}
fn ledger_ref(ledger: &MaterialLedgerId) -> LedgerKeyRefV1<'_> {
    match ledger {
        MaterialLedgerId::World => LedgerKeyRefV1::World,
        MaterialLedgerId::Agent(s) => LedgerKeyRefV1::Agent(s),
        MaterialLedgerId::Site(s) => LedgerKeyRefV1::Site(s),
        MaterialLedgerId::Factory(s) => LedgerKeyRefV1::Factory(s),
    }
}
fn ledger_parts(ledger: Option<LedgerKeyRefV1<'_>>) -> (u8, &str) {
    match ledger {
        None => (0, ""),
        Some(LedgerKeyRefV1::World) => (1, ""),
        Some(LedgerKeyRefV1::Agent(s)) => (2, s),
        Some(LedgerKeyRefV1::Site(s)) => (3, s),
        Some(LedgerKeyRefV1::Factory(s)) => (4, s),
    }
}
fn compare(
    a: IdentityRef<'_>,
    b: IdentityRef<'_>,
    work: &mut InventoryIdentityDirectoryWorkV1,
    c: &mut impl FnMut() -> bool,
) -> Result<Ordering, ValueTreeError> {
    let (at, as_) = ledger_parts(a.ledger);
    let (bt, bs) = ledger_parts(b.ledger);
    work.step(
        as_.len()
            .min(bs.len())
            .checked_add(
                a.material
                    .unwrap_or("")
                    .len()
                    .min(b.material.unwrap_or("").len()),
            )
            .ok_or_else(invalid)?,
        c,
    )?;
    Ok(at
        .cmp(&bt)
        .then_with(|| as_.cmp(bs))
        .then_with(|| a.material.cmp(&b.material)))
}
fn arc_heap<T>() -> Result<usize, ValueTreeError> {
    size_of::<ReservedValue<T>>()
        .checked_add(2 * size_of::<usize>())
        .and_then(|n| n.checked_add(4 * align_of::<ReservedValue<T>>()))
        .ok_or_else(invalid)
}
fn retain_identity(
    id: IdentityRef<'_>,
    memory: &MemoryBudget,
    work: &mut InventoryIdentityDirectoryWorkV1,
    c: &mut impl FnMut() -> bool,
) -> Result<Arc<ReservedValue<Identity>>, ValueTreeError> {
    let (_, ledger) = ledger_parts(id.ledger);
    let material = id.material.unwrap_or("");
    if ledger.len() > work.max_identity_bytes || material.len() > work.max_identity_bytes {
        return Err(ValueTreeError::ResourceLimited("inventory identity bytes"));
    }
    work.step(
        ledger
            .len()
            .checked_add(material.len())
            .ok_or_else(invalid)?,
        c,
    )?;
    let bytes = arc_heap::<Identity>()?
        .checked_add(ledger.len())
        .and_then(|n| n.checked_add(material.len()))
        .ok_or_else(invalid)?;
    let fee = memory.reserve(bytes, MemoryClass::Ordinary)?;
    fn owned(s: &str) -> Result<String, ValueTreeError> {
        let value = s.to_owned();
        if value.capacity() > s.len() {
            return Err(invalid());
        }
        Ok(value)
    }
    let ledger = match id.ledger {
        None => None,
        Some(LedgerKeyRefV1::World) => Some(MaterialLedgerId::World),
        Some(LedgerKeyRefV1::Agent(s)) => Some(MaterialLedgerId::Agent(owned(s)?)),
        Some(LedgerKeyRefV1::Site(s)) => Some(MaterialLedgerId::Site(owned(s)?)),
        Some(LedgerKeyRefV1::Factory(s)) => Some(MaterialLedgerId::Factory(owned(s)?)),
    };
    let material = id.material.map(owned).transpose()?;
    Ok(Arc::new(fee.retain(Identity { ledger, material })))
}
fn semantic_identity<'a>(
    parsed: crate::incremental_world_field_key::WorldFieldKeyRefV1<'_>,
    semantic: SemanticKeyRefV1<'a>,
) -> Result<Option<IdentityRef<'a>>, ValueTreeError> {
    // The original decoder already verified framing, full closure and the
    // structural/semantic relationship. This only borrows its typed output.
    match (parsed.name, parsed.kind, semantic) {
        ("resources", _, _) => Ok(None),
        ("materials", WorldFieldRecordKindV1::Entry, SemanticKeyRefV1::Text(material)) => {
            Ok(Some(IdentityRef {
                ledger: None,
                material: Some(material),
            }))
        }
        ("material_ledgers", WorldFieldRecordKindV1::Group, SemanticKeyRefV1::Ledger(ledger)) => {
            Ok(Some(IdentityRef {
                ledger: Some(ledger),
                material: None,
            }))
        }
        (
            "material_ledgers",
            WorldFieldRecordKindV1::Entry,
            SemanticKeyRefV1::Pair { left, right },
        ) => {
            let SemanticKeyRefV1::Ledger(ledger) =
                left.decode().map_err(ValueTreeError::Invalid)?
            else {
                return Err(invalid());
            };
            let SemanticKeyRefV1::Text(material) =
                right.decode().map_err(ValueTreeError::Invalid)?
            else {
                return Err(invalid());
            };
            Ok(Some(IdentityRef {
                ledger: Some(ledger),
                material: Some(material),
            }))
        }
        _ => Err(invalid()),
    }
}
#[allow(clippy::too_many_arguments)]
pub fn build_inventory_identity_directory_v1<M, R, E, C>(
    parent: &ReservedValue<InventoryFamilyQualificationV1>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    family: &mut InventoryFamilyWorkV1,
    work: &mut InventoryIdentityDirectoryWorkV1,
) -> Result<ReservedValue<InventoryIdentityDirectoryV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    parent.check(ctx.values.memory, MemoryClass::Ordinary)?;
    parent.validate_binding(roots)?;
    if ctx.values.class != MemoryClass::Ordinary
        || !parent.uses_budget(ctx.values.memory)
        || ctx.state.allocation_class() != Some(MemoryClass::Ordinary)
        || ctx
            .state
            .allocation_budget()
            .is_none_or(|m| !m.same_pool(ctx.values.memory))
    {
        return Err(invalid());
    }
    let memory = ctx.values.memory;
    let fee = memory.reserve(
        size_of::<InventoryIdentityDirectoryV1>() + 128,
        MemoryClass::Ordinary,
    )?;
    let mut tree = None;
    let mut counts = [0u64; 4];
    let mut metadata = [false; 3];
    let stop = std::cell::RefCell::new(&mut ctx.cancelled);
    let mut local = WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Business),
        state: ctx.state,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: ctx.values.source,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: || stop.borrow_mut()(),
    };
    let result = StateScanContext {
        source: local.state,
        memory,
        class: MemoryClass::Ordinary,
        io: scan,
    }
    .visit_entries(
        roots.state_root(StateNamespaceV1::Business),
        |entry| {
            local.check()?;
            let Some(key) = key(&entry.key)? else {
                return Ok(());
            };
            let row = row_with_semantic(
                &key,
                &entry.value,
                &mut local,
                family,
                |parsed, semantic| {
                    if let Some(id) = semantic_identity(parsed, semantic)? {
                        let id = retain_identity(id, memory, work, &mut || stop.borrow_mut()())?;
                        let (after, inserted) =
                            tree::put(&tree, id, memory, work, &mut || stop.borrow_mut()())?;
                        if !inserted {
                            return Err(invalid());
                        }
                        tree = Some(after);
                    }
                    Ok(())
                },
            )?;
            match row {
                Row::Meta(i) => {
                    if metadata[i] {
                        return Err(invalid());
                    }
                    metadata[i] = true;
                }
                Row::Entry(i) => counts[i] = counts[i].checked_add(1).ok_or_else(invalid)?,
                Row::Group(_) => counts[2] = counts[2].checked_add(1).ok_or_else(invalid)?,
                Row::Member(_) => counts[3] = counts[3].checked_add(1).ok_or_else(invalid)?,
            }
            Ok(())
        },
        || stop.borrow_mut()(),
    );
    let original = (local.take_state_error)();
    if let Some(error) = original {
        return Err(error);
    }
    result.map_err(|error| match error {
        StateScanError::Visitor(e) => e,
        StateScanError::Memory(e) => ValueTreeError::Memory(e),
        StateScanError::Cancelled => ValueTreeError::Cancelled,
        StateScanError::ResourceLimited => {
            ValueTreeError::ResourceLimited("inventory identity parent scan")
        }
        StateScanError::Invalid(e) => ValueTreeError::Invalid(e),
        StateScanError::Tree(_) => ValueTreeError::Invalid("inventory identity parent tree"),
    })?;
    local.check()?;
    if metadata != [true; 3] || counts != parent.counts {
        return Err(invalid());
    }
    Ok(fee.retain(InventoryIdentityDirectoryV1 {
        roots: parent.roots,
        tree,
    }))
}

#[cfg(test)]
#[path = "incremental_inventory_identity_directory_tests.rs"]
mod tests;

#[path = "incremental_inventory_identity_directory_freeze.rs"]
mod freeze;
pub use freeze::{
    FrozenInventoryIdentityDirectoryV1, freeze_current_inventory_identity_directory_v1,
};

#[path = "incremental_inventory_identity_directory_total.rs"]
mod total;
pub use total::sum_current_inventory_material_v1;

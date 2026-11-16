//! Explicit authenticated-parent ALL scope, for one cold startup only.
//! Ordinary capture remains the dirty-only producer in the parent module.
use super::*;
use crate::incremental_field_read::WorldFieldReadContextV1;
use oasis7_formal_protocol::incremental_state::{
    StateScanBudget, StateScanContext, StateScanError,
};
pub struct QualifiedIndustryProjectionColumnsV1 {
    counts: [u64; 9],
    rows: Vec<Row>,
    roots: [u8; 32],
    memory: MemoryBudget,
    _slots: MemoryReservation,
    _fee: MemoryReservation,
}
impl QualifiedIndustryProjectionColumnsV1 {
    pub fn record_counts(&self) -> [u64; 9] {
        self.counts
    }
    pub fn validate_context(
        &self,
        roots: &RootSetV1,
        memory: &MemoryBudget,
    ) -> Result<(), ValueTreeError> {
        if self.roots != roots.digest().map_err(|_| bad())?
            || !self.memory.same_pool(memory)
            || !self._fee.uses_budget(memory)
            || !self._slots.uses_budget(memory)
        {
            return Err(bad());
        }
        Ok(())
    }
    pub fn visit_with_work_v1<E>(
        &self,
        work: &mut dyn FnMut(usize) -> Result<(), E>,
        mut emit: impl FnMut(&[&str], &[u8], &[u8], &[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.rows {
            work(0)?;
            emit(
                &[row.field],
                &row.semantic,
                &row.key_json,
                &row.json.as_ref().expect("closed ALL record").bytes,
            )?
        }
        Ok(())
    }
}
struct StateSource<'a, 'b, M> {
    source: &'a M,
    p: &'a Processing<'b>,
    first: RefCell<Option<ValueTreeError>>,
}
impl<M: MerkleNodeReader> MerkleNodeReader for StateSource<'_, '_, M> {
    type Node = M::Node;
    fn allocation_class(&self) -> Option<MemoryClass> {
        self.source.allocation_class()
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        self.source.allocation_budget()
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        if self.first.borrow().is_some() {
            return Err("cold industry processing".into());
        }
        if let Err(e) = self.p.spend(0) {
            *self.first.borrow_mut() = Some(e);
            return Err("cold industry processing".into());
        }
        let value = self.source.read_node(id, max);
        if let Ok(Some(raw)) = &value {
            let paid = raw
                .as_ref()
                .len()
                .checked_mul(16)
                .and_then(|n| n.checked_add(id.len() + 256))
                .ok_or_else(limit)
                .and_then(|n| self.p.spend(n));
            if let Err(e) = paid {
                *self.first.borrow_mut() = Some(e);
                return Err("cold industry processing".into());
            }
        }
        value
    }
}
#[allow(clippy::too_many_arguments)]
pub fn read_qualified_industry_projection_columns_v1<M, R, E, C>(
    roots: &RootSetV1,
    factory: &ReservedValue<crate::incremental_factory_records::FactoryFamilyQualificationV1>,
    industry: &ReservedValue<crate::incremental_industry_jobs::IndustryFamilyQualificationV1>,
    logistics: &ReservedValue<crate::incremental_logistics_records::LogisticsFamilyQualificationV1>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    factory_work: &mut crate::incremental_factory_records::FactoryWorkBudgetV1,
    industry_work: &mut crate::incremental_industry_jobs::IndustryWorkBudgetV1,
    logistics_work: &mut crate::incremental_logistics_records::LogisticsWorkBudgetV1,
    processing: &mut BusinessDiffProcessingWorkV1,
) -> Result<QualifiedIndustryProjectionColumnsV1, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    read.check()?;
    let memory = read.values.memory.clone();
    if read.values.class != MemoryClass::Ordinary
        || !factory.uses_budget(&memory)
        || !industry.uses_budget(&memory)
        || !logistics.uses_budget(&memory)
        || factory.allocation_class() != MemoryClass::Ordinary
        || industry.allocation_class() != MemoryClass::Ordinary
        || logistics.allocation_class() != MemoryClass::Ordinary
    {
        return Err(bad());
    }
    let digest = roots.digest().map_err(|_| bad())?;
    factory.validate_binding(&digest)?;
    industry.validate_binding(&digest)?;
    logistics.validate_binding(&digest)?;
    let stop = RefCell::new(&mut read.cancelled);
    let check = || (stop.borrow_mut())();
    let p = Processing {
        work: RefCell::new(processing),
        cancel: &check,
    };
    let source = StateSource {
        source: read.state,
        p: &p,
        first: RefCell::new(None),
    };
    let values_source = PaidSource {
        source: read.values.source,
        p: &p,
    };
    let mut expected = [0u64; 9];
    expected[..2].copy_from_slice(&factory.record_counts());
    expected[2..7].copy_from_slice(&industry.record_counts());
    expected[7..].copy_from_slice(&logistics.record_counts());
    let capacity = expected.iter().try_fold(0usize, |n, v| {
        n.checked_add(usize::try_from(*v).map_err(|_| limit())?)
            .ok_or_else(limit)
    })?;
    p.spend(capacity.checked_mul(size_of::<Row>()).ok_or_else(limit)?)?;
    let slots = memory.reserve(
        capacity.checked_mul(size_of::<Row>()).ok_or_else(limit)?,
        MemoryClass::Ordinary,
    )?;
    let fee = memory.reserve(
        size_of::<QualifiedIndustryProjectionColumnsV1>(),
        MemoryClass::Ordinary,
    )?;
    let mut rows = Vec::with_capacity(capacity);
    if rows.capacity() != capacity {
        return Err(limit());
    }
    let mut counts = [0u64; 9];
    let fields = [
        "factories",
        "pending_factory_builds",
        "pending_recipe_jobs",
        "pending_material_transits",
        "recipe_completion_receipts",
        "factory_production_failure_dispositions",
        "logistics_settlement_receipts",
        "logistics_routes",
        "completed_logistics_paths",
    ];
    let mut metadata = 0u16;
    for ns in [
        StateNamespaceV1::Business,
        StateNamespaceV1::Scheduler,
        StateNamespaceV1::Authorization,
    ] {
        let result = StateScanContext {
            source: &source,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: scan,
        }
        .visit_entries(
            roots.state_root(ns),
            |entry| {
                p.spend(
                    entry
                        .key
                        .len()
                        .checked_mul(16)
                        .and_then(|n| n.checked_add(256))
                        .ok_or_else(limit)?,
                )?;
                if !entry.key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
                    return Ok(());
                }
                let parsed =
                    WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                let Some(slot) = fields.iter().position(|name| *name == parsed.name) else {
                    return Ok(());
                };
                let field = fields[slot];
                let descriptor = crate::incremental_projection::WORLD_STATE_FIELDS
                    .iter()
                    .copied()
                    .find(|f| {
                        f.name == field
                            && f.owner == crate::incremental_projection::FieldOwner::WorldState
                    })
                    .ok_or_else(bad)?;
                let key = WorldFieldKeyV1::new(descriptor, parsed.kind, parsed.semantic_key)
                    .map_err(ValueTreeError::Invalid)?;
                if key.as_bytes() != entry.key || entry.namespace != ns {
                    return Err(bad());
                }
                let mut values = ValueReadContext {
                    source: &values_source,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut *read.values.io,
                };
                let mut cancelled = || check();
                let mut records = InventoryValueReader {
                    values: &mut values,
                    key_work: &mut *read.key_work,
                    semantic_bytes: read.semantic_bytes,
                    record_bytes: read.record_bytes,
                    cancelled: &mut cancelled,
                };
                if parsed.kind == WorldFieldRecordKindV1::Metadata {
                    if metadata & (1 << slot) != 0 {
                        return Err(bad());
                    }
                    records.values.verify_closure(
                        &entry.value,
                        records.record_bytes,
                        &mut *records.cancelled,
                    )?;
                    records.presence(&entry.value, &key, 0)?;
                    metadata |= 1 << slot;
                    return Ok(());
                }
                if parsed.kind != WorldFieldRecordKindV1::Entry {
                    return Err(bad());
                }
                macro_rules! capture {
                    ($module:ident,$kind:expr,$work:expr) => {{
                        let kind = $kind;
                        let Some(selected) = crate::$module::projection::outer(
                            kind,
                            &key,
                            &entry.value,
                            &mut records,
                            &p,
                        )?
                        else {
                            return Ok(());
                        };
                        let json = crate::$module::projection::project(
                            kind,
                            &selected.child,
                            &mut records,
                            $work,
                            &p,
                        )?;
                        (selected, json)
                    }};
                }
                let (selected, json) = match slot {
                    0 => capture!(
                        incremental_factory_records,
                        crate::incremental_factory_records::FactoryRecordKindV1::Factory,
                        factory_work
                    ),
                    1 => capture!(
                        incremental_factory_records,
                        crate::incremental_factory_records::FactoryRecordKindV1::Build,
                        factory_work
                    ),
                    2 => capture!(
                        incremental_industry_jobs,
                        crate::incremental_industry_jobs::IndustryRecordKindV1::Recipe,
                        industry_work
                    ),
                    3 => capture!(
                        incremental_industry_jobs,
                        crate::incremental_industry_jobs::IndustryRecordKindV1::Transit,
                        industry_work
                    ),
                    4 => capture!(
                        incremental_industry_jobs,
                        crate::incremental_industry_jobs::IndustryRecordKindV1::Completion,
                        industry_work
                    ),
                    5 => capture!(
                        incremental_industry_jobs,
                        crate::incremental_industry_jobs::IndustryRecordKindV1::Failure,
                        industry_work
                    ),
                    6 => capture!(
                        incremental_industry_jobs,
                        crate::incremental_industry_jobs::IndustryRecordKindV1::Settlement,
                        industry_work
                    ),
                    7 => capture!(
                        incremental_logistics_records,
                        crate::incremental_logistics_records::LogisticsRecordKindV1::Route,
                        logistics_work
                    ),
                    8 => capture!(
                        incremental_logistics_records,
                        crate::incremental_logistics_records::LogisticsRecordKindV1::Path,
                        logistics_work
                    ),
                    _ => return Err(bad()),
                };
                counts[slot] = counts[slot].checked_add(1).ok_or_else(limit)?;
                if counts[slot] > expected[slot] || rows.len() == capacity {
                    return Err(bad());
                }
                let semantic = encode(&memory, &p, |mut out| match &selected.key {
                    Key::Text(id) => {
                        crate::incremental_semantic_key::write_semantic_key_v1(id, &mut out)
                            .map_err(|_| bad())
                    }
                    Key::Id(id) => {
                        crate::incremental_semantic_key::write_semantic_key_v1(id, &mut out)
                            .map_err(|_| bad())
                    }
                })?;
                let key_json = match &selected.key {
                    Key::Text(id) => super::json(id, &memory, &p)?,
                    Key::Id(id) => super::json(id, &memory, &p)?,
                };
                rows.push(Row {
                    field,
                    key: selected.key,
                    semantic,
                    key_json,
                    json: Some(json),
                    _fee: selected._fee,
                });
                Ok(())
            },
            || check(),
        );
        if let Some(error) = source.first.borrow_mut().take() {
            return Err(error);
        }
        if let Some(error) = (read.take_state_error)() {
            return Err(error);
        }
        match result {
            Ok(_) => {}
            Err(StateScanError::Memory(e)) => return Err(ValueTreeError::Memory(e)),
            Err(StateScanError::Visitor(e)) => return Err(e),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(StateScanError::ResourceLimited) => return Err(limit()),
            _ => return Err(bad()),
        }
    }
    if metadata != (1 << 9) - 1 || counts != expected {
        return Err(bad());
    }
    for i in 1..rows.len() {
        let mut j = i;
        while j != 0 {
            let cost = match (&rows[j - 1].key, &rows[j].key) {
                (Key::Text(a), Key::Text(b)) => a.len() + b.len(),
                _ => 16,
            };
            p.spend(cost + rows[j - 1].field.len() + rows[j].field.len())?;
            if rows[j - 1]
                .field
                .cmp(rows[j].field)
                .then_with(|| rows[j - 1].key.cmp(&rows[j].key))
                != std::cmp::Ordering::Greater
            {
                break;
            }
            p.spend(2 * size_of::<Row>())?;
            rows.swap(j - 1, j);
            j -= 1
        }
    }
    Ok(QualifiedIndustryProjectionColumnsV1 {
        counts: expected,
        rows,
        roots: digest,
        memory,
        _slots: slots,
        _fee: fee,
    })
}

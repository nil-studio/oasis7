//! Fixed Factory/Industry/Logistics dirty records to original typed JSON.
//! No parent scan, generic CBOR decoder, field-root substitution or permission.
use crate::{
    incremental_field_member::*,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_runtime_revocation::BusinessDiffProcessingWorkV1,
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
    },
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateRootV1},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use serde_json::Value;
use std::{
    cell::RefCell,
    io::{self, Write},
    mem::size_of,
};
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Current industry typed projection")
}
fn limit() -> ValueTreeError {
    ValueTreeError::ResourceLimited("Current industry projection work")
}
pub(crate) struct Processing<'a> {
    work: RefCell<&'a mut BusinessDiffProcessingWorkV1>,
    cancel: &'a dyn Fn() -> bool,
}
impl Processing<'_> {
    pub(crate) fn spend(&self, n: usize) -> Result<(), ValueTreeError> {
        if (self.cancel)() {
            return Err(ValueTreeError::Cancelled);
        }
        let n = u64::try_from(n).map_err(|_| limit())?;
        let steps = n.checked_add(1).ok_or_else(limit)?;
        let mut w = self.work.borrow_mut();
        let old = (w.steps_remaining, w.bytes_remaining);
        w.steps_remaining = w.steps_remaining.saturating_sub(steps);
        w.bytes_remaining = w.bytes_remaining.saturating_sub(n);
        if old.0 < steps || old.1 < n {
            Err(limit())
        } else {
            Ok(())
        }
    }
}
struct PaidSource<'a, 'b, R> {
    source: &'a R,
    p: &'a Processing<'b>,
}
impl<R: ValueNodeReader> ValueNodeReader for PaidSource<'_, '_, R> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<std::sync::Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.p.spend(0)?;
        let node = self.source.read_node(id, max, memory, class)?;
        let n = node
            .len()
            .checked_mul(16)
            .and_then(|n| n.checked_add(32 + 256))
            .ok_or_else(limit)?;
        self.p.spend(n)?;
        Ok(node)
    }
}
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Key {
    Text(String),
    Id(u64),
}
pub(crate) struct Outer {
    pub key: Key,
    pub child: StateRootV1,
    pub _fee: MemoryReservation,
}
pub(crate) struct JsonRecord {
    pub bytes: ReservedValue<Vec<u8>>,
    pub _graph: MemoryReservation,
}
struct Row {
    field: &'static str,
    key: Key,
    semantic: ReservedValue<Vec<u8>>,
    key_json: ReservedValue<Vec<u8>>,
    json: Option<JsonRecord>,
    _fee: MemoryReservation,
}
pub struct CurrentIndustryProjectionDeltaV1 {
    rows: Vec<Row>,
    guard: PreparedGuard,
    parent: [u8; 32],
    memory: MemoryBudget,
    _slots: MemoryReservation,
    _fee: MemoryReservation,
}
impl CurrentIndustryProjectionDeltaV1 {
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_read_source_guard(&self.guard, live)?;
        if !tx.uses_budget(memory)
            || !self.memory.same_pool(memory)
            || !self._fee.uses_budget(memory)
            || tx.parent_fence().parent_roots != self.parent
        {
            return Err(bad().into());
        }
        Ok(())
    }
    pub fn visit_with_work_v1<E>(
        &self,
        work: &mut dyn FnMut(usize) -> Result<(), E>,
        mut visit: impl FnMut(&[&str], &[u8], &[u8], Option<&[u8]>) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.rows {
            work(0)?;
            visit(
                &[row.field],
                &row.semantic,
                &row.key_json,
                row.json.as_ref().map(|v| v.bytes.as_slice()),
            )?
        }
        Ok(())
    }
}
struct Output<'a, 'b> {
    p: &'a Processing<'b>,
    bytes: Option<Vec<u8>>,
    n: usize,
    max: usize,
    first: Option<ValueTreeError>,
}
impl Write for Output<'_, '_> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        for chunk in b.chunks(8192) {
            if self.first.is_some() {
                return Err(io::ErrorKind::Other.into());
            }
            if let Err(e) = self.p.spend(chunk.len()) {
                self.first = Some(e);
                return Err(io::ErrorKind::Other.into());
            }
            let Some(n) = self.n.checked_add(chunk.len()).filter(|n| *n <= self.max) else {
                self.first = Some(limit());
                return Err(io::ErrorKind::Other.into());
            };
            if let Some(out) = &mut self.bytes {
                out.extend_from_slice(chunk)
            }
            self.n = n
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(crate) fn encode(
    memory: &MemoryBudget,
    p: &Processing<'_>,
    mut writer: impl FnMut(&mut dyn Write) -> Result<(), ValueTreeError>,
) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
    let mut count = Output {
        p,
        bytes: None,
        n: 0,
        max: usize::MAX,
        first: None,
    };
    let result = writer(&mut count);
    if let Some(e) = count.first {
        return Err(e);
    }
    result?;
    let n = count.n;
    let fee = memory.reserve(
        n.checked_add(size_of::<Vec<u8>>()).ok_or_else(limit)?,
        MemoryClass::Ordinary,
    )?;
    p.spend(n)?;
    let bytes = Vec::with_capacity(n);
    if bytes.capacity() != n {
        return Err(limit());
    }
    let mut out = Output {
        p,
        bytes: Some(bytes),
        n: 0,
        max: n,
        first: None,
    };
    let result = writer(&mut out);
    if let Some(e) = out.first {
        return Err(e);
    }
    result?;
    if out.n != n {
        return Err(bad());
    }
    Ok(fee.retain(out.bytes.take().ok_or_else(bad)?))
}
pub(crate) fn json<T: serde::Serialize>(
    value: &T,
    memory: &MemoryBudget,
    p: &Processing<'_>,
) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
    encode(memory, p, |out| {
        serde_json::to_writer(out, value).map_err(|_| bad())
    })
}
pub(crate) fn scalar(
    value: crate::incremental_record_value_read::RecordValueV1,
    text: Vec<u8>,
) -> Result<Value, ValueTreeError> {
    use crate::incremental_ledger_value::LedgerValueKindV1 as K;
    use crate::incremental_record_value_read::RecordValueV1 as V;
    Ok(match value {
        V::Unit => Value::Null,
        V::Unsigned(v) => v.into(),
        V::Signed(v) => v.into(),
        V::Bool(v) => v.into(),
        V::Text { utf8_bytes } => {
            if usize::try_from(utf8_bytes).ok() != Some(text.len()) {
                return Err(bad());
            }
            Value::String(String::from_utf8(text).map_err(|_| bad())?)
        }
        V::Ledger(info) => {
            let id = String::from_utf8(text).map_err(|_| bad())?;
            if u64::try_from(id.len()).ok() != Some(info.id_bytes) {
                return Err(bad());
            }
            let ledger = match info.kind {
                K::World => crate::MaterialLedgerId::World,
                K::Agent => crate::MaterialLedgerId::Agent(id),
                K::Site => crate::MaterialLedgerId::Site(id),
                K::Factory => crate::MaterialLedgerId::Factory(id),
            };
            serde_json::to_value(ledger).map_err(|_| bad())?
        }
    })
}
/// Record-local schema assembly only. The final original DTO decoder and its
/// original serializer, rather than this intermediate object, select JSON.
pub(crate) struct Assembly {
    sections: std::collections::BTreeMap<&'static str, serde_json::Map<String, Value>>,
}
impl Assembly {
    pub(crate) fn new() -> Self {
        Self {
            sections: std::collections::BTreeMap::new(),
        }
    }
    pub(crate) fn put(
        &mut self,
        section: &'static str,
        name: &str,
        value: Value,
    ) -> Result<(), ValueTreeError> {
        if self
            .sections
            .entry(section)
            .or_default()
            .insert(name.to_owned(), value)
            .is_some()
        {
            return Err(bad());
        }
        Ok(())
    }
    pub(crate) fn member(
        &mut self,
        section: &'static str,
        name: &str,
        index: Option<u64>,
        part: Option<&str>,
        member: Option<&str>,
        value: Value,
    ) -> Result<(), ValueTreeError> {
        let entry = self
            .sections
            .entry(section)
            .or_default()
            .entry(name.to_owned())
            .or_insert_with(|| {
                if member.is_some() {
                    Value::Object(Default::default())
                } else {
                    Value::Array(Vec::new())
                }
            });
        if let Some(member) = member {
            if entry
                .as_object_mut()
                .ok_or_else(bad)?
                .insert(member.to_owned(), value)
                .is_some()
            {
                return Err(bad());
            }
            return Ok(());
        }
        let index = usize::try_from(index.ok_or_else(bad)?).map_err(|_| limit())?;
        let list = entry.as_array_mut().ok_or_else(bad)?;
        if list.len() <= index {
            list.resize_with(index.checked_add(1).ok_or_else(limit)?, || Value::Null)
        }
        if let Some(part) = part {
            if list[index].is_null() {
                list[index] = Value::Object(Default::default())
            }
            if list[index]
                .as_object_mut()
                .ok_or_else(bad)?
                .insert(part.to_owned(), value)
                .is_some()
            {
                return Err(bad());
            }
        } else {
            if !list[index].is_null() {
                return Err(bad());
            }
            list[index] = value
        }
        Ok(())
    }
    pub(crate) fn collection(
        &mut self,
        section: &'static str,
        name: &str,
        value: Value,
        map: bool,
        leaf_count: u64,
    ) -> Result<(), ValueTreeError> {
        let object = self.sections.entry(section).or_default();
        match value {
            Value::Null => {
                if object.insert(name.to_owned(), Value::Null).is_some() {
                    return Err(bad());
                }
            }
            Value::Number(ref n) => {
                let n = n.as_u64().ok_or_else(bad)?;
                if n > leaf_count {
                    return Err(bad());
                }
                object.entry(name.to_owned()).or_insert_with(|| {
                    if map {
                        Value::Object(Default::default())
                    } else {
                        Value::Array(Vec::new())
                    }
                });
            }
            _ => return Err(bad()),
        }
        Ok(())
    }
    pub(crate) fn take(&mut self, section: &'static str) -> Value {
        Value::Object(self.sections.remove(section).unwrap_or_default())
    }
}
pub fn is_industry_projection_field_v1(field: &str) -> bool {
    matches!(
        field,
        "factories"
            | "pending_factory_builds"
            | "pending_recipe_jobs"
            | "pending_material_transits"
            | "recipe_completion_receipts"
            | "factory_production_failure_dispositions"
            | "logistics_settlement_receipts"
            | "logistics_routes"
            | "completed_logistics_paths"
    )
}
fn classify(raw: &[u8]) -> Result<Option<&'static str>, ValueTreeError> {
    if !raw.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(None);
    }
    let k = WorldFieldKeyV1::from_bytes(raw).map_err(ValueTreeError::Invalid)?;
    let field = match k.name {
        "factories" => "factories",
        "pending_factory_builds" => "pending_factory_builds",
        "pending_recipe_jobs" => "pending_recipe_jobs",
        "pending_material_transits" => "pending_material_transits",
        "recipe_completion_receipts" => "recipe_completion_receipts",
        "factory_production_failure_dispositions" => "factory_production_failure_dispositions",
        "logistics_settlement_receipts" => "logistics_settlement_receipts",
        "logistics_routes" => "logistics_routes",
        "completed_logistics_paths" => "completed_logistics_paths",
        _ => return Ok(None),
    };
    Ok((k.kind == WorldFieldRecordKindV1::Entry).then_some(field))
}

struct Pending {
    field: &'static str,
    key: Vec<u8>,
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
}
#[allow(clippy::too_many_arguments)]
pub fn read_current_industry_projection_delta_v1<M, R, E, C>(
    roots: &RootSetV1,
    factory: &ReservedValue<crate::incremental_factory_records::FactoryFamilyQualificationV1>,
    industry: &ReservedValue<crate::incremental_industry_jobs::IndustryFamilyQualificationV1>,
    logistics: &ReservedValue<crate::incremental_logistics_records::LogisticsFamilyQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    factory_work: &mut crate::incremental_factory_records::FactoryWorkBudgetV1,
    industry_work: &mut crate::incremental_industry_jobs::IndustryWorkBudgetV1,
    logistics_work: &mut crate::incremental_logistics_records::LogisticsWorkBudgetV1,
    processing: &mut BusinessDiffProcessingWorkV1,
    pending_entries: &mut usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<CurrentIndustryProjectionDeltaV1, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let stop = RefCell::new(&mut ctx.cancelled);
    let check = || (stop.borrow_mut())();
    let p = Processing {
        work: RefCell::new(processing),
        cancel: &check,
    };
    let paid = PaidSource {
        source: ctx.values.source,
        p: &p,
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ctx.transaction.validate_read_source_guard(guard, live)?;
        let memory = ctx.values.memory.clone();
        if ctx.values.class != MemoryClass::Ordinary
            || !ctx.transaction.uses_budget(&memory)
            || !factory.uses_budget(&memory)
            || !industry.uses_budget(&memory)
            || !logistics.uses_budget(&memory)
            || factory.allocation_class() != MemoryClass::Ordinary
            || industry.allocation_class() != MemoryClass::Ordinary
            || logistics.allocation_class() != MemoryClass::Ordinary
        {
            return Err(bad().into());
        }
        let root_bytes = roots
            .state
            .iter()
            .try_fold(roots.world_id.len() + 256, |n, r| {
                n.checked_add(r.node_root.len() + 128).ok_or_else(limit)
            })?;
        p.spend(root_bytes.checked_mul(16).ok_or_else(limit)?)?;
        let digest = roots.digest().map_err(|_| bad())?;
        if digest != ctx.transaction.parent_fence().parent_roots {
            return Err(bad().into());
        }
        factory.validate_binding(&digest)?;
        industry.validate_binding(&digest)?;
        logistics.validate_binding(&digest)?;
        let fee = memory.reserve(
            size_of::<CurrentIndustryProjectionDeltaV1>(),
            MemoryClass::Ordinary,
        ).map_err(ValueTreeError::Memory)?;
        let mut n = 0usize;
        let mut bytes = 0usize;
        for ns in [
            StateNamespaceV1::Business,
            StateNamespaceV1::Scheduler,
            StateNamespaceV1::Authorization,
        ] {
            ctx.transaction.visit_pending_writes(
                ns,
                pending_entries,
                || {
                    if (stop.borrow_mut())() {
                        return Err(WorldFieldOverlayErrorV1::from(ValueTreeError::Cancelled));
                    }
                    p.spend(0)?;
                    Ok(())
                },
                |raw, before, after| {
                    p.spend(
                        raw.len()
                            .checked_mul(16)
                            .and_then(|n| n.checked_add(256))
                            .ok_or_else(limit)?,
                    )?;
                    if before != after && classify(raw)?.is_some() {
                        n = n.checked_add(1).ok_or_else(limit)?;
                        bytes = bytes.checked_add(raw.len()).ok_or_else(limit)?
                    }
                    Ok::<_, WorldFieldOverlayErrorV1>(())
                },
            )?;
        }
        let pending_fee = memory.reserve(
            n.checked_mul(size_of::<Pending>())
                .and_then(|n| n.checked_add(bytes))
                .ok_or_else(limit)?,
            MemoryClass::Ordinary,
        ).map_err(ValueTreeError::Memory)?;
        let mut pending = Vec::with_capacity(n);
        if pending.capacity() != n {
            return Err(limit().into());
        }
        for ns in [
            StateNamespaceV1::Business,
            StateNamespaceV1::Scheduler,
            StateNamespaceV1::Authorization,
        ] {
            ctx.transaction.visit_pending_writes(
                ns,
                pending_entries,
                || {
                    if (stop.borrow_mut())() {
                        return Err(WorldFieldOverlayErrorV1::from(ValueTreeError::Cancelled));
                    }
                    p.spend(0)?;
                    Ok(())
                },
                |raw, before, after| {
                    p.spend(
                        raw.len()
                            .checked_mul(16)
                            .and_then(|n| n.checked_add(256))
                            .ok_or_else(limit)?,
                    )?;
                    if before != after {
                        if let Some(field) = classify(raw)? {
                            if pending.len() == n {
                                return Err(bad().into());
                            }
                            pending.push(Pending {
                                field,
                                key: raw.to_vec(),
                                before,
                                after,
                            })
                        }
                    }
                    Ok::<_, WorldFieldOverlayErrorV1>(())
                },
            )?;
        }
        if pending.len() != n {
            return Err(bad().into());
        }
        let mut rows = Vec::with_capacity(n);
        if rows.capacity() != n {
            return Err(limit().into());
        }
        let slots = memory.reserve(
            n.checked_mul(size_of::<Row>()).ok_or_else(limit)?,
            MemoryClass::Ordinary,
        ).map_err(ValueTreeError::Memory)?;
        for pending in pending {
            if check() {
                return Err(ValueTreeError::Cancelled.into());
            }
            p.spend(
                pending
                    .key
                    .len()
                    .checked_mul(16)
                    .and_then(|n| n.checked_add(256))
                    .ok_or_else(limit)?,
            )?;
            let binding =
                WorldFieldKeyV1::from_bytes(&pending.key).map_err(ValueTreeError::Invalid)?;
            if ctx.transaction.read(binding.namespace, &pending.key)? != pending.after {
                return Err(bad().into());
            }
            let key = WorldFieldKeyV1::new(
                crate::incremental_projection::WORLD_STATE_FIELDS
                    .iter()
                    .copied()
                    .find(|f| f.name == pending.field)
                    .ok_or_else(bad)?,
                binding.kind,
                binding.semantic_key,
            )
            .map_err(ValueTreeError::Invalid)?;
            if key.as_bytes() != pending.key {
                return Err(bad().into());
            }
            let mut values = ValueReadContext {
                source: &paid,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut *ctx.values.io,
            };
            let mut cancelled = || check();
            let mut records = InventoryValueReader {
                values: &mut values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut cancelled,
            };
            macro_rules! capture {
                ($module:ident,$kind:expr,$work:expr) => {{
                    let kind = $kind;
                    let before = match pending.before {
                        Some(v) => {
                            crate::$module::projection::outer(kind, &key, &v, &mut records, &p)?
                        }
                        None => None,
                    };
                    let after = match pending.after {
                        Some(v) => {
                            crate::$module::projection::outer(kind, &key, &v, &mut records, &p)?
                        }
                        None => None,
                    };
                    if before.is_none() && after.is_none() {
                        if pending.before.is_none() || pending.after.is_none() {
                            return Err(bad().into());
                        }
                        continue;
                    }
                    if let (Some(before), Some(after)) = (&before, &after) {
                        if before.key != after.key {
                            return Err(bad().into());
                        }
                        if before.child == after.child {
                            continue;
                        }
                    }
                    let is_present = after.is_some();
                    let selected = after.or(before).ok_or_else(bad)?;
                    let json = if is_present {
                        Some(crate::$module::projection::project(
                            kind,
                            &selected.child,
                            &mut records,
                            $work,
                            &p,
                        )?)
                    } else {
                        None
                    };
                    (selected, json)
                }};
            }
            let (selected, json) = match pending.field {
                "factories" => capture!(
                    incremental_factory_records,
                    crate::incremental_factory_records::FactoryRecordKindV1::Factory,
                    factory_work
                ),
                "pending_factory_builds" => capture!(
                    incremental_factory_records,
                    crate::incremental_factory_records::FactoryRecordKindV1::Build,
                    factory_work
                ),
                "pending_recipe_jobs" => capture!(
                    incremental_industry_jobs,
                    crate::incremental_industry_jobs::IndustryRecordKindV1::Recipe,
                    industry_work
                ),
                "pending_material_transits" => capture!(
                    incremental_industry_jobs,
                    crate::incremental_industry_jobs::IndustryRecordKindV1::Transit,
                    industry_work
                ),
                "recipe_completion_receipts" => capture!(
                    incremental_industry_jobs,
                    crate::incremental_industry_jobs::IndustryRecordKindV1::Completion,
                    industry_work
                ),
                "factory_production_failure_dispositions" => capture!(
                    incremental_industry_jobs,
                    crate::incremental_industry_jobs::IndustryRecordKindV1::Failure,
                    industry_work
                ),
                "logistics_settlement_receipts" => capture!(
                    incremental_industry_jobs,
                    crate::incremental_industry_jobs::IndustryRecordKindV1::Settlement,
                    industry_work
                ),
                "logistics_routes" => capture!(
                    incremental_logistics_records,
                    crate::incremental_logistics_records::LogisticsRecordKindV1::Route,
                    logistics_work
                ),
                "completed_logistics_paths" => capture!(
                    incremental_logistics_records,
                    crate::incremental_logistics_records::LogisticsRecordKindV1::Path,
                    logistics_work
                ),
                _ => return Err(bad().into()),
            };
            let semantic = encode(&memory, &p, |mut out| {
                match &selected.key {
                    Key::Text(id) => {
                        crate::incremental_semantic_key::write_semantic_key_v1(id, &mut out)
                    }
                    Key::Id(id) => {
                        crate::incremental_semantic_key::write_semantic_key_v1(id, &mut out)
                    }
                }
                .map_err(|_| bad())
            })?;
            let key_json = match &selected.key {
                Key::Text(id) => self::json(id, &memory, &p)?,
                Key::Id(id) => self::json(id, &memory, &p)?,
            };
            let row = Row {
                field: pending.field,
                key: selected.key,
                semantic,
                key_json,
                json,
                _fee: selected._fee,
            };
            let mut first = None;
            let place = rows.binary_search_by(|old: &Row| {
                if first.is_some() {
                    return std::cmp::Ordering::Equal;
                }
                let cost = match (&old.key, &row.key) {
                    (Key::Text(a), Key::Text(b)) => a.len() + b.len(),
                    _ => 16,
                };
                if let Err(e) = p.spend(cost + old.field.len() + row.field.len()) {
                    first = Some(e);
                    return std::cmp::Ordering::Equal;
                }
                old.field.cmp(row.field).then_with(|| old.key.cmp(&row.key))
            });
            if let Some(e) = first {
                return Err(e.into());
            }
            match place {
                Ok(_) => return Err(bad().into()),
                Err(i) => {
                    p.spend(
                        (rows.len() - i)
                            .checked_mul(size_of::<Row>())
                            .ok_or_else(limit)?,
                    )?;
                    rows.insert(i, row)
                }
            }
        }
        drop(pending_fee);
        ctx.transaction.validate_read_source_guard(guard, live)?;
        let guard = ctx.transaction.guard();
        Ok(CurrentIndustryProjectionDeltaV1 {
            rows,
            guard,
            parent: digest,
            memory,
            _slots: slots,
            _fee: fee,
        })
    }));
    match result {
        Ok(result) => {
            let result = if let Some(error) = (ctx.take_state_error)() {
                Err(error.into())
            } else {
                result
            };
            if result.is_err() {
                let _: Result<(), ExecutionError> =
                    ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            }
            result
        }
        Err(panic) => {
            let _: Result<(), ExecutionError> =
                ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            std::panic::resume_unwind(panic)
        }
    }
}

#[path="incremental_industry_projection_delta_cold.rs"]mod cold;
pub use cold::{QualifiedIndustryProjectionColumnsV1,read_qualified_industry_projection_columns_v1};

//! Qualified ALL native columns, explicit cold/bootstrap scope only.
use super::*;
use oasis7_formal_protocol::incremental_state::StateScanContext;
pub struct QualifiedNativeProjectionColumnsV1 {
    counts: [(&'static str, u64); 24],
    rows: Vec<NativeRow>,
    roots: [u8; 32],
    memory: MemoryBudget,
    fee: MemoryReservation,
}
impl QualifiedNativeProjectionColumnsV1 {
    pub fn record_counts(&self) -> [(&'static str, u64); 24] {
        self.counts
    }
    pub fn validate_context(
        &self,
        roots: &RootSetV1,
        memory: &MemoryBudget,
    ) -> Result<(), ValueTreeError> {
        if self.roots != roots.digest().map_err(|_| bad())?
            || !self.memory.same_pool(memory)
            || !self.fee.uses_budget(memory)
        {
            return Err(bad());
        }
        Ok(())
    }
    pub fn visit_with_work_v1<E>(
        &self,
        work: &mut dyn FnMut(usize) -> Result<(), E>,
        mut emit: impl FnMut(&'static str, &[u8], &[u8], &[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.rows {
            work(0)?;
            emit(
                row.field,
                &row.semantic,
                &row.key_json,
                row.value_json.as_ref().expect("qualified native value"),
            )?
        }
        Ok(())
    }
}
macro_rules! qualified_dispatch{(($name:expr,$entry:expr,$ns:expr,$read:expr,$scan:expr,$work:expr,$p:expr) $($n:literal => $f:ty,)+)=>{match $name{$($n=>one_selection::<$f,_,_,_,_>(NativeSelection::Qualified($entry),$ns,$read,$scan,$work,$p),)+_=>Err(bad())}};}
#[allow(clippy::too_many_arguments)]
pub fn read_qualified_native_projection_columns_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<WorldNativeFieldsQualificationV1>,
    roots: &RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    outer_scan: &mut StateScanBudget,
    typed_scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
    processing: &mut BusinessDiffProcessingWorkV1,
) -> Result<QualifiedNativeProjectionColumnsV1, ValueTreeError> {
    read.check()?;
    let memory = read.values.memory.clone();
    if read.values.class != MemoryClass::Ordinary
        || !parent.uses_budget(&memory)
        || parent.projection_allocation_class_v1() != MemoryClass::Ordinary
    {
        return Err(bad());
    }
    parent.validate_binding(roots)?;
    let counts = parent.projection_record_counts_v1();
    let capacity = counts.iter().try_fold(0usize, |n, (_, v)| {
        n.checked_add(usize::try_from(*v).map_err(|_| limit())?)
            .ok_or_else(limit)
    })?;
    let p = Processing {
        bank: std::cell::RefCell::new(processing),
    };
    let source = ProcessingState {
        source: read.state,
        processing: &p,
        first: std::cell::RefCell::new(None),
    };
    let stop = std::cell::RefCell::new(&mut read.cancelled);
    let mut check = || (stop.borrow_mut())();
    spend(
        &p,
        capacity
            .checked_mul(size_of::<NativeRow>())
            .and_then(|n| n.checked_add(size_of::<QualifiedNativeProjectionColumnsV1>()))
            .ok_or_else(limit)?,
        &mut check,
    )?;
    let fee = memory.reserve(
        capacity
            .checked_mul(size_of::<NativeRow>())
            .and_then(|n| n.checked_add(size_of::<QualifiedNativeProjectionColumnsV1>()))
            .ok_or_else(limit)?,
        MemoryClass::Ordinary,
    )?;
    let mut rows = Vec::with_capacity(capacity);
    if rows.capacity() != capacity {
        return Err(limit());
    }
    let mut observed = [0u64; 24];
    let mut metadata = 0u32;
    for root in &roots.state {
        let result = StateScanContext {
            source: &source,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut *outer_scan,
        }
        .visit_entries(
            root,
            |entry| {
                spend(
                    &p,
                    entry
                        .key
                        .len()
                        .checked_mul(16)
                        .and_then(|n| n.checked_add(256))
                        .ok_or_else(limit)?,
                    &mut || (stop.borrow_mut())(),
                )?;
                if !selected(&entry.key)? {
                    return Ok(());
                }
                let parsed =
                    WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                let slot = counts
                    .iter()
                    .position(|(name, _)| *name == parsed.name)
                    .ok_or_else(bad)?;
                let mut selected = WorldFieldReadContextV1 {
                    root,
                    state: &source,
                    values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                        source: read.values.source,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut *read.values.io,
                    },
                    key_work: &mut *read.key_work,
                    semantic_bytes: read.semantic_bytes,
                    record_bytes: read.record_bytes,
                    take_state_error: || {
                        source
                            .first
                            .borrow_mut()
                            .take()
                            .or_else(|| (read.take_state_error)())
                    },
                    cancelled: || (stop.borrow_mut())(),
                };
                let row = native_fields!(
                    qualified_dispatch,
                    parsed.name,
                    entry,
                    root.namespace,
                    &mut selected,
                    typed_scan,
                    work,
                    &p
                )?;
                if parsed.kind == WorldFieldRecordKindV1::Metadata {
                    if metadata & (1 << slot) != 0 {
                        return Err(bad());
                    }
                    metadata |= 1 << slot;
                } else {
                    observed[slot] = observed[slot].checked_add(1).ok_or_else(limit)?;
                    if observed[slot] > counts[slot].1 {
                        return Err(bad());
                    }
                    let row = row.ok_or_else(bad)?;
                    if rows.len() == capacity || row.value_json.is_none() {
                        return Err(bad());
                    }
                    rows.push(row)
                }
                Ok(())
            },
            || (stop.borrow_mut())(),
        );
        if let Some(error) = source.first.borrow_mut().take() {
            return Err(error);
        }
        if let Some(error) = (read.take_state_error)() {
            return Err(error);
        }
        match result {
            Ok(_) => {}
            Err(oasis7_formal_protocol::incremental_state::StateScanError::Memory(e)) => {
                return Err(ValueTreeError::Memory(e));
            }
            Err(oasis7_formal_protocol::incremental_state::StateScanError::Visitor(e)) => {
                return Err(e);
            }
            Err(oasis7_formal_protocol::incremental_state::StateScanError::Cancelled) => {
                return Err(ValueTreeError::Cancelled);
            }
            Err(oasis7_formal_protocol::incremental_state::StateScanError::ResourceLimited) => {
                return Err(limit());
            }
            _ => return Err(bad()),
        }
    }
    if metadata != (1u32 << 24) - 1
        || observed
            .iter()
            .zip(counts)
            .any(|(actual, (_, expected))| *actual != expected)
    {
        return Err(bad());
    }
    for i in 1..rows.len() {
        let mut j = i;
        while j != 0 {
            spend(
                &p,
                rows[j - 1].semantic.len() + rows[j].semantic.len() + 128,
                &mut || (stop.borrow_mut())(),
            )?;
            let order = rows[j - 1].field.cmp(rows[j].field);
            let order = if order == std::cmp::Ordering::Equal {
                match (
                    read_semantic_key_v1(&rows[j - 1].semantic).map_err(ValueTreeError::Invalid)?,
                    read_semantic_key_v1(&rows[j].semantic).map_err(ValueTreeError::Invalid)?,
                ) {
                    (SemanticKeyRefV1::Unit, SemanticKeyRefV1::Unit) => std::cmp::Ordering::Equal,
                    (SemanticKeyRefV1::Text(a), SemanticKeyRefV1::Text(b)) => a.cmp(b),
                    (SemanticKeyRefV1::Unsigned(a), SemanticKeyRefV1::Unsigned(b)) => a.cmp(&b),
                    _ => return Err(bad()),
                }
            } else {
                order
            };
            if order != std::cmp::Ordering::Greater {
                break;
            }
            spend(&p, 2 * size_of::<NativeRow>(), &mut || {
                (stop.borrow_mut())()
            })?;
            rows.swap(j - 1, j);
            j -= 1
        }
    }
    Ok(QualifiedNativeProjectionColumnsV1 {
        counts,
        rows,
        roots: roots.digest().map_err(|_| bad())?,
        memory,
        fee,
    })
}

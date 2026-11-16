//! Explicit complete qualified primitive columns, not an ordinary dirty path.
use super::*;
use crate::incremental_field_read::WorldFieldReadContextV1;
use oasis7_formal_protocol::incremental_state::{StateScanContext, StateScanError};
pub struct QualifiedPrimitiveProjectionColumnsV1 {
    counts: [u64; FIELDS],
    rows: Vec<Row>,
    roots: [u8; 32],
    memory: MemoryBudget,
    fee: MemoryReservation,
}
impl QualifiedPrimitiveProjectionColumnsV1 {
    pub fn visit_field_counts_with_work_v1<E>(
        &self,
        work: &mut dyn FnMut(usize) -> Result<(), E>,
        mut emit: impl FnMut(FieldDescriptor, u64) -> Result<(), E>,
    ) -> Result<(), E> {
        for (i, field) in WORLD_STATE_FIELDS.iter().copied().enumerate() {
            work(0)?;
            if supports(field) {
                emit(field, self.counts[i])?
            }
        }
        Ok(())
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
                row.value_json.as_ref().expect("qualified primitive value"),
            )?
        }
        Ok(())
    }
}
struct StateSource<'a, 'b, 'c, M, C> {
    source: &'a M,
    processing: &'a std::cell::RefCell<&'b mut BusinessDiffProcessingWorkV1>,
    stop: &'a std::cell::RefCell<&'c mut C>,
    first: std::cell::RefCell<Option<ValueTreeError>>,
}
impl<M: MerkleNodeReader, C: FnMut() -> bool> MerkleNodeReader for StateSource<'_, '_, '_, M, C> {
    type Node = M::Node;
    fn allocation_class(&self) -> Option<MemoryClass> {
        self.source.allocation_class()
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        self.source.allocation_budget()
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        if self.first.borrow().is_some() {
            return Err("cold primitive processing".into());
        }
        if let Err(e) = pay(&mut self.processing.borrow_mut(), 0, &mut || {
            (self.stop.borrow_mut())()
        }) {
            *self.first.borrow_mut() = Some(e);
            return Err("cold primitive processing".into());
        }
        let value = self.source.read_node(id, max);
        if let Ok(Some(raw)) = &value {
            let paid = raw
                .as_ref()
                .len()
                .checked_mul(16)
                .and_then(|n| n.checked_add(id.len() + 256))
                .ok_or_else(limit)
                .and_then(|n| {
                    pay(&mut self.processing.borrow_mut(), n, &mut || {
                        (self.stop.borrow_mut())()
                    })
                });
            if let Err(e) = paid {
                *self.first.borrow_mut() = Some(e);
                return Err("cold primitive processing".into());
            }
        }
        value
    }
}
#[allow(clippy::too_many_arguments)]
pub fn read_qualified_primitive_projection_columns_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<WorldPrimitiveQualificationV1>,
    roots: &RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldPrimitiveFamilyWorkV1,
    processing: &mut BusinessDiffProcessingWorkV1,
) -> Result<QualifiedPrimitiveProjectionColumnsV1, WorldFieldOverlayErrorV1> {
    read.check()?;
    let memory = read.values.memory.clone();
    if read.values.class != MemoryClass::Ordinary
        || !parent.uses_budget(&memory)
        || parent.allocation_class() != MemoryClass::Ordinary
    {
        return Err(bad().into());
    }
    parent.validate_binding(roots).map_err(owner_error)?;
    let capacity = parent.counts.iter().try_fold(0usize, |n, v| {
        n.checked_add(usize::try_from(*v).map_err(|_| limit())?)
            .ok_or_else(limit)
    })?;
    let stop = std::cell::RefCell::new(&mut read.cancelled);
    let processing = std::cell::RefCell::new(processing);
    let source = StateSource {
        source: read.state,
        processing: &processing,
        stop: &stop,
        first: std::cell::RefCell::new(None),
    };
    let n = capacity
        .checked_mul(size_of::<Row>())
        .and_then(|n| n.checked_add(size_of::<QualifiedPrimitiveProjectionColumnsV1>()))
        .ok_or_else(limit)?;
    pay(&mut processing.borrow_mut(), n, &mut || {
        (stop.borrow_mut())()
    })?;
    let fee = memory
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut rows = Vec::with_capacity(capacity);
    if rows.capacity() != capacity {
        return Err(limit().into());
    }
    let mut counts = [0u64; FIELDS];
    let mut seen = [false; FIELDS];
    for root in &roots.state {
        let result = StateScanContext {
            source: &source,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut *scan,
        }
        .visit_entries(
            root,
            |entry| {
                pay(
                    &mut processing.borrow_mut(),
                    entry
                        .key
                        .len()
                        .checked_mul(16)
                        .and_then(|n| n.checked_add(256))
                        .ok_or_else(limit)?,
                    &mut || (stop.borrow_mut())(),
                )?;
                let Some((index, _, key)) =
                    identify(entry.namespace, &entry.key).map_err(owner_error)?
                else {
                    return Ok(());
                };
                let parsed =
                    WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
                let mut cancelled = || (stop.borrow_mut())();
                let mut values = PrimitiveReadContextV1 {
                    values: &mut read.values,
                    key_work: &mut *read.key_work,
                    semantic_bytes: read.semantic_bytes,
                    record_bytes: read.record_bytes,
                    cancelled: &mut cancelled,
                };
                let row = one_selection(
                    PrimitiveSelection::Qualified(entry),
                    roots,
                    &mut values,
                    work,
                    &mut processing.borrow_mut(),
                )?;
                match parsed.kind {
                    WorldFieldRecordKindV1::Metadata => {
                        if std::mem::replace(&mut seen[index], true) {
                            return Err(bad().into());
                        }
                    }
                    WorldFieldRecordKindV1::Entry => {
                        counts[index] = counts[index].checked_add(1).ok_or_else(limit)?;
                        if counts[index] > parent.counts[index] || rows.len() == capacity {
                            return Err(bad().into());
                        }
                        let row = row.ok_or_else(bad)?;
                        if row.value_json.is_none() {
                            return Err(bad().into());
                        }
                        rows.push(row)
                    }
                    _ => return Err(bad().into()),
                }
                Ok::<_, WorldFieldOverlayErrorV1>(())
            },
            || (stop.borrow_mut())(),
        );
        if let Some(e) = source.first.borrow_mut().take() {
            return Err(e.into());
        }
        if let Some(e) = (read.take_state_error)() {
            return Err(e.into());
        }
        match result {
            Ok(_) => {}
            Err(StateScanError::Memory(e)) => return Err(ValueTreeError::Memory(e).into()),
            Err(StateScanError::Visitor(e)) => return Err(e),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled.into()),
            Err(StateScanError::ResourceLimited) => return Err(limit().into()),
            _ => return Err(bad().into()),
        }
    }
    if counts != parent.counts
        || WORLD_STATE_FIELDS
            .iter()
            .enumerate()
            .any(|(i, f)| supports(*f) && !seen[i])
    {
        return Err(bad().into());
    }
    for i in 1..rows.len() {
        let mut j = i;
        while j != 0 {
            pay(
                &mut processing.borrow_mut(),
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
                    _ => return Err(bad().into()),
                }
            } else {
                order
            };
            if order != std::cmp::Ordering::Greater {
                break;
            }
            pay(
                &mut processing.borrow_mut(),
                2 * size_of::<Row>(),
                &mut || (stop.borrow_mut())(),
            )?;
            rows.swap(j - 1, j);
            j -= 1
        }
    }
    Ok(QualifiedPrimitiveProjectionColumnsV1 {
        counts,
        rows,
        roots: roots.digest().map_err(|_| bad())?,
        memory,
        fee,
    })
}

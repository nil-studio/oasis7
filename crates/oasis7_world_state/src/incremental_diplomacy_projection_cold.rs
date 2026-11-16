//! Complete accepted Identity/Proposal/Vote enumeration. Uses the same typed point decoder
//! as ordinary deltas and never creates synthetic pending writes.
use super::*;
use oasis7_formal_protocol::incremental_state::{StateScanContext, StateScanError};
pub struct QualifiedDiplomacyProjectionColumnsV1 {
    counts: [u64; 4],
    data: CurrentDiplomacyProjectionDeltaV1,
}
impl QualifiedDiplomacyProjectionColumnsV1 {
    pub fn record_counts(&self) -> [u64; 4] {
        self.counts
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_bootstrap_read_only_v1(live)?;
        self.data.validate(tx, memory, live)
    }
    pub fn projection_data_v1(&self) -> &CurrentDiplomacyProjectionDeltaV1 {
        &self.data
    }
}
pub fn read_qualified_diplomacy_projection_columns_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    family: &ReservedValue<DiplomacyFamilyQualificationV1>,
    roots: &RootSetV1,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    p: &mut BusinessDiffProcessingWorkV1,
    max_text: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<QualifiedDiplomacyProjectionColumnsV1, BodyMutationError> {
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        ctx.transaction.validate_bootstrap_read_only_v1(live)?;
        let memory = ctx.values.memory.clone();
        if ctx.values.class != MemoryClass::Ordinary
            || !ctx.transaction.uses_budget(&memory)
            || !family.uses_budget(&memory)
            || family.allocation_class() != MemoryClass::Ordinary
        {
            return Err(bad().into());
        }
        family.validate_binding(&ctx.transaction.parent_fence().parent_roots)?;
        if roots.digest()? != ctx.transaction.parent_fence().parent_roots {
            return Err(bad().into());
        }
        let expected = family.record_counts();
        let count = expected.into_iter().try_fold(0usize, |total, n| {
            total
                .checked_add(usize::try_from(n).map_err(|_| limit())?)
                .ok_or_else(limit)
        })?;
        let mut changes_fee = reserve(
            &memory,
            count.checked_mul(size_of::<Change>()).ok_or_else(limit)?,
            p,
            &mut ctx.cancelled,
        )?;
        let mut changes = Vec::with_capacity(count);
        if changes.capacity() != count {
            return Err(limit().into());
        }
        let mut counts = [0u64; 4];
        let mut metadata = [false; 4];
        let mut markers = [false; 4];
        {
            let stop = RefCell::new(&mut ctx.cancelled);
            StateScanContext {
                source: state,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: scan,
            }
            .visit_entries_with_reader(
                roots.state_root(StateNamespaceV1::Business),
                |entry, _| {
                    pay(p, entry.key.len(), &mut **stop.borrow_mut())?;
                    let Some(kind) = identify(&entry.key)? else {
                        return Ok(());
                    };
                    if entry.namespace != StateNamespaceV1::Business {
                        return Err(bad().into());
                    }
                    *cells = cells.checked_sub(1).ok_or_else(limit)?;
                    let index=DiplomacyFieldV1::ALL.into_iter().position(|candidate|candidate==kind).ok_or_else(bad)?;
                    let parsed =
                        WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                    let selected = {
                        let parent_root = roots.state_root(StateNamespaceV1::Business);
                        let mut cancelled = || (stop.borrow_mut())();
                        let mut top = crate::incremental_field_read::WorldFieldReadContextV1 {
                            root: parent_root,
                            state,
                            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                                source: ctx.values.source,
                                memory: &memory,
                                class: MemoryClass::Ordinary,
                                io: &mut *ctx.values.io,
                            },
                            key_work: &mut *ctx.key_work,
                            semantic_bytes: ctx.semantic_bytes,
                            record_bytes: ctx.record_bytes,
                            take_state_error: &mut ctx.take_state_error,
                            cancelled: &mut cancelled,
                        };
                        super::super::family::outer(entry, &mut top)?
                    };
                    if selected.is_none() {
                        let seen = if parsed.kind == WorldFieldRecordKindV1::Metadata {
                            &mut metadata[index]
                        } else {
                            &mut markers[index]
                        };
                        if *seen {
                            return Err(bad().into());
                        }
                        *seen = true;
                        return Ok(());
                    }
                    if changes.len() == count {
                        return Err(bad().into());
                    }
                    counts[index] = counts[index].checked_add(1).ok_or_else(limit)?;
                    let mut fee = reserve(&memory, entry.key.len(), p, &mut **stop.borrow_mut())?;
                    changes_fee
                        .merge(&mut fee)
                        .map_err(ValueTreeError::Invalid)?;
                    let mut key = Vec::with_capacity(entry.key.len());
                    if key.capacity() != entry.key.len() {
                        return Err(limit().into());
                    }
                    for chunk in entry.key.chunks(8192) {
                        pay(p, chunk.len(), &mut **stop.borrow_mut())?;
                        key.extend_from_slice(chunk)
                    }
                    changes.push(Change {
                        kind,
                        key,
                        before: None,
                        after: Some(entry.value),
                    });
                    Ok::<_, BodyMutationError>(())
                },
                || (stop.borrow_mut())(),
            )
            .map_err(|error| match error {
                StateScanError::Visitor(error) => error,
                StateScanError::Memory(error) => ValueTreeError::Memory(error).into(),
                StateScanError::ResourceLimited => limit().into(),
                StateScanError::Cancelled => ValueTreeError::Cancelled.into(),
                StateScanError::Invalid(note) => ValueTreeError::Invalid(note).into(),
                StateScanError::Tree(error) => {
                    BodyMutationError::Original(Box::new(io::Error::other(error)))
                }
            })?;
        }
        if counts != expected
            || metadata != [true; 4]
            || markers != [true; 4]
            || changes.len() != count
        {
            return Err(bad().into());
        }
        let fee = reserve(
            &memory,
            count
                .checked_mul(size_of::<Entity>())
                .and_then(|n| n.checked_add(size_of::<QualifiedDiplomacyProjectionColumnsV1>()))
                .ok_or_else(limit)?,
            p,
            &mut ctx.cancelled,
        )?;
        let mut rows = Vec::with_capacity(count);
        if rows.capacity() != count {
            return Err(limit().into());
        }
        for change in changes {
            let row = decode_projection_entity_v1(
                change, family, roots, state, ctx, scan, cells, p, max_text, live,
            )?
            .ok_or_else(bad)?;
            if !row.after || row.before {
                return Err(bad().into());
            }
            rows.push(row)
        }
        if let Some(error) = (ctx.take_state_error)() {
            return Err(error.into());
        }
        pay(p, 0, &mut ctx.cancelled)?;
        ctx.transaction.validate_bootstrap_read_only_v1(live)?;
        let out = QualifiedDiplomacyProjectionColumnsV1 {
            counts: expected,
            data: CurrentDiplomacyProjectionDeltaV1 {
                rows,
                guard: ctx.transaction.guard(),
                parent: ctx.transaction.parent_fence().parent_roots,
                memory,
                _fee: fee,
            },
        };
        out.validate(ctx.transaction, ctx.values.memory, live)?;
        Ok(out)
    })();
    if let Some(error) = (ctx.take_state_error)() {
        return Err(error.into());
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    result
}

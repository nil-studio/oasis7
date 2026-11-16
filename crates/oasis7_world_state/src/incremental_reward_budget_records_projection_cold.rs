//! Complete accepted Identity/Proposal/Vote enumeration. Uses the same typed point decoder
//! as ordinary deltas and never creates synthetic pending writes.
use super::*;
use oasis7_formal_protocol::incremental_state::{StateScanContext, StateScanError};
pub struct QualifiedRewardProjectionColumnsV1 {
    counts: [u64; 2],
    data: CurrentRewardProjectionDeltaV1,
}
impl QualifiedRewardProjectionColumnsV1 {
    pub fn record_counts(&self) -> [u64; 2] {
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
    pub fn projection_data_v1(&self) -> &CurrentRewardProjectionDeltaV1 {
        &self.data
    }
}
pub fn read_qualified_reward_projection_columns_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    family: &ReservedValue<RewardFamilyQualificationV1>,
    roots: &RootSetV1,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    p: &mut BusinessDiffProcessingWorkV1,
    max_text: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<QualifiedRewardProjectionColumnsV1, BodyMutationError> {
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
                .checked_add(usize::try_from(n).map_err(|_| limited())?)
                .ok_or_else(limited)
        })?;
        let mut changes_fee = reserve(
            &memory,
            count.checked_mul(size_of::<Change>()).ok_or_else(limited)?,
            p,
            &mut ctx.cancelled,
        )?;
        let mut changes = Vec::with_capacity(count);
        if changes.capacity() != count {
            return Err(limited().into());
        }
        let mut counts = [0u64; 2];
        let mut metadata = [false; 2];
        let mut markers = [false; 2];
        let mut length_seen = false;
        let mut ordered = super::super::receiver::OrderChanges::default();
        for root in &roots.state {
            let stop = RefCell::new(&mut ctx.cancelled);
            StateScanContext {
                source: state,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut *scan,
            }
            .visit_entries_with_reader(
                root,
                |entry, _| {
                    pay(p, entry.key.len(), &mut **stop.borrow_mut())?;
                    let Some(kind) = identify(&entry.key)? else {
                        return Ok(());
                    };
                    if entry.namespace != kind.namespace() {
                        return Err(bad().into());
                    }
                    *cells = cells.checked_sub(1).ok_or_else(limited)?;
                    let index = super::super::family::index(kind);
                    let parsed =
                        WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                    let selected = {
                        let parent_root = roots.state_root(kind.namespace());
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
                    match selected {
                        Some(super::super::family::Outer::Metadata(actual)) => {
                            if actual != kind || metadata[index] {
                                return Err(bad().into());
                            }
                            metadata[index] = true;
                            return Ok(());
                        }
                        Some(super::super::family::Outer::Format(actual)) => {
                            if actual != kind || markers[index] {
                                return Err(bad().into());
                            }
                            markers[index] = true;
                            return Ok(());
                        }
                        Some(super::super::family::Outer::Length(length)) => {
                            if kind != RewardRecordKindV1::Mint
                                || length_seen
                                || length != family.mint_length()
                            {
                                return Err(bad().into());
                            }
                            length_seen = true;
                            return Ok(());
                        }
                        Some(super::super::family::Outer::Record(actual, _)) => {
                            if actual.kind != kind {
                                return Err(bad().into());
                            }
                            if kind == RewardRecordKindV1::Mint {
                                ordered.observe(actual.index, false, true)?;
                            }
                        }
                        None => return Err(bad().into()),
                    }
                    if changes.len() == count {
                        return Err(bad().into());
                    }
                    counts[index] = counts[index].checked_add(1).ok_or_else(limited)?;
                    let mut fee = reserve(&memory, entry.key.len(), p, &mut **stop.borrow_mut())?;
                    changes_fee
                        .merge(&mut fee)
                        .map_err(ValueTreeError::Invalid)?;
                    let mut key = Vec::with_capacity(entry.key.len());
                    if key.capacity() != entry.key.len() {
                        return Err(limited().into());
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
                StateScanError::ResourceLimited => limited().into(),
                StateScanError::Cancelled => ValueTreeError::Cancelled.into(),
                StateScanError::Invalid(note) => ValueTreeError::Invalid(note).into(),
                StateScanError::Tree(error) => {
                    BodyMutationError::Original(Box::new(std::io::Error::other(error)))
                }
            })?;
        }
        ordered.check(0, family.mint_length())?;
        if !length_seen
            || counts != expected
            || metadata != [true; 2]
            || markers != [true; 2]
            || changes.len() != count
        {
            return Err(bad().into());
        }
        let fee = reserve(
            &memory,
            count
                .checked_mul(size_of::<Entity>())
                .and_then(|n| n.checked_add(size_of::<QualifiedRewardProjectionColumnsV1>()))
                .ok_or_else(limited)?,
            p,
            &mut ctx.cancelled,
        )?;
        let mut rows = Vec::with_capacity(count);
        if rows.capacity() != count {
            return Err(limited().into());
        }
        for change in changes {
            let row = decode_projection_entity_v1(
                change, family, roots, state, ctx, scan, cells, p, max_text,
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
        let out = QualifiedRewardProjectionColumnsV1 {
            counts: expected,
            data: CurrentRewardProjectionDeltaV1 {
                entities: rows,
                before_length: 0,
                after_length: family.mint_length(),
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

//! Actual complete Controls scopes. Empty subject groups remain present even
//! when they own no members; neither their existence nor counts are guessed.
use super::*;
use oasis7_formal_protocol::incremental_state::{
    StateScanBudget, StateScanContext, StateScanError,
};
pub struct QualifiedCollectionControlsProjectionColumnsV1 {
    counts: [[u64; 2]; 3],
    cursor: u64,
    data: CurrentCollectionControlsProjectionDeltaV1,
}
impl QualifiedCollectionControlsProjectionColumnsV1 {
    pub fn group_member_counts(&self) -> [[u64; 2]; 3] {
        self.counts
    }
    pub fn cursor_entries(&self) -> u64 {
        self.cursor
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
    pub fn visit_with_work_v1<E: From<ValueTreeError>>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(&'static str, &[u8], &[u8], &[u8], bool) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.data.rows {
            work(1, 0)?;
            emit(
                name(row.field)?,
                &row.semantic,
                &row.key,
                row.json.as_deref().map(Vec::as_slice).ok_or_else(bad)?,
                row.field < 2,
            )?
        }
        Ok(())
    }
    pub fn visit_groups_with_work_v1<E: From<ValueTreeError>>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(&'static str, &[u8], &[u8], u64, bool) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.data.groups {
            work(1, 0)?;
            emit(
                name(row.field)?,
                &row.semantic,
                &row.key,
                row.members.ok_or_else(bad)?,
                row.field < 2,
            )?
        }
        Ok(())
    }
}
pub fn read_qualified_collection_controls_projection_columns_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    family: &ReservedValue<WorldCollectionControlsQualificationV1>,
    roots: &RootSetV1,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    controls: &mut WorldCollectionControlsWorkV1,
    p: &mut BusinessDiffProcessingWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<QualifiedCollectionControlsProjectionColumnsV1, BodyMutationError> {
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        ctx.transaction.validate_bootstrap_read_only_v1(live)?;
        let memory = ctx.values.memory.clone();
        if ctx.values.class != MemoryClass::Ordinary
            || !family.uses_budget(&memory)
            || !ctx.transaction.uses_budget(&memory)
        {
            return Err(bad().into());
        }
        family.check(&memory, MemoryClass::Ordinary)?;
        family.validate_binding(roots)?;
        if roots.digest().map_err(|_| bad())? != ctx.transaction.parent_fence().parent_roots {
            return Err(bad().into());
        }
        let expected = family.group_member_counts();
        let cursor = family.cursor_entries();
        let count = expected.iter().flatten().try_fold(
            usize::try_from(cursor).map_err(|_| limit())?,
            |n, c| {
                n.checked_add(usize::try_from(*c).map_err(|_| limit())?)
                    .ok_or_else(limit)
            },
        )?;
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
        let mut observed = [[0u64; 2]; 3];
        let mut observed_cursor = 0u64;
        let mut metadata = [false; 4];
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
                |entry, _reader| {
                    pay(p, entry.key.len(), &mut **stop.borrow_mut())?;
                    let Some((field, key)) = super::super::key(&entry.key)? else {
                        return Ok(());
                    };
                    if entry.namespace != namespace(field) {
                        return Err(bad().into());
                    }
                    let top = roots.state_root(entry.namespace);
                    let mut reader = WorldFieldReadContextV1 {
                        root: top,
                        state,
                        values: ValueReadContext {
                            source: ctx.values.source,
                            memory: &memory,
                            class: MemoryClass::Ordinary,
                            io: &mut *ctx.values.io,
                        },
                        key_work: &mut *ctx.key_work,
                        semantic_bytes: ctx.semantic_bytes,
                        record_bytes: ctx.record_bytes,
                        take_state_error: &mut ctx.take_state_error,
                        cancelled: || (stop.borrow_mut())(),
                    };
                    let row =
                        super::super::row(field, &key, &entry.value, false, &mut reader, controls)?;
                    match row {
                        super::super::Row::Meta(i) => {
                            if metadata[i] {
                                return Err(bad().into());
                            }
                            metadata[i] = true;
                            return Ok(());
                        }
                        super::super::Row::Group(i, _) => {
                            observed[i][0] = observed[i][0].checked_add(1).ok_or_else(limit)?
                        }
                        super::super::Row::Member(i, _) => {
                            observed[i][1] = observed[i][1].checked_add(1).ok_or_else(limit)?
                        }
                        super::super::Row::Cursor => {
                            observed_cursor = observed_cursor.checked_add(1).ok_or_else(limit)?
                        }
                    };
                    if changes.len() == count {
                        return Err(bad().into());
                    }
                    let mut fee = reserve(&memory, entry.key.len(), p, &mut reader.cancelled)?;
                    changes_fee
                        .merge(&mut fee)
                        .map_err(ValueTreeError::Invalid)?;
                    let mut key = Vec::with_capacity(entry.key.len());
                    if key.capacity() != entry.key.len() {
                        return Err(limit().into());
                    }
                    for chunk in entry.key.chunks(8192) {
                        pay(p, chunk.len(), &mut reader.cancelled)?;
                        key.extend_from_slice(chunk);
                    }
                    changes.push(Change {
                        field,
                        key,
                        before: None,
                        after: Some(entry.value),
                    });
                    Ok::<_, BodyMutationError>(())
                },
                || (stop.borrow_mut())(),
            )
            .map_err(|e| match e {
                StateScanError::Visitor(e) => e,
                StateScanError::Memory(e) => ValueTreeError::Memory(e).into(),
                StateScanError::ResourceLimited => limit().into(),
                StateScanError::Cancelled => ValueTreeError::Cancelled.into(),
                StateScanError::Invalid(e) => ValueTreeError::Invalid(e).into(),
                StateScanError::Tree(e) => {
                    BodyMutationError::Original(Box::new(io::Error::other(e)))
                }
            })?;
        }
        if observed != expected
            || observed_cursor != cursor
            || metadata != [true; 4]
            || changes.len() != count
        {
            return Err(bad().into());
        }
        let fee = reserve(
            &memory,
            count
                .checked_mul(size_of::<ValueRow>() + size_of::<GroupRow>())
                .and_then(|n| {
                    n.checked_add(size_of::<QualifiedCollectionControlsProjectionColumnsV1>())
                })
                .ok_or_else(limit)?,
            p,
            &mut ctx.cancelled,
        )?;
        let mut rows = Vec::with_capacity(count);
        let mut groups = Vec::with_capacity(count);
        if rows.capacity() != count || groups.capacity() != count {
            return Err(limit().into());
        }
        for change in changes {
            match decode_projection_change_v1(change, roots, state, ctx, controls, p)? {
                Some(Decoded::Value(row)) => {
                    if row.json.is_none() {
                        return Err(bad().into());
                    }
                    rows.push(row)
                }
                Some(Decoded::Group(mut row)) => {
                    let digest = row.scope;
                    pay(p, 0, &mut ctx.cancelled)?;
                    let mut work = super::super::index::CollectionControlsIndexWorkV1 {
                        steps_remaining: std::mem::take(&mut controls.index_steps_remaining),
                    };
                    let meta = family.groups.get(digest, &mut work, &mut ctx.cancelled);
                    controls.index_steps_remaining = work.steps_remaining;
                    let meta = meta?.ok_or_else(bad)?;
                    if !meta.present {
                        return Err(bad().into());
                    }
                    row.members = Some(meta.members);
                    groups.push(row)
                }
                None => return Err(bad().into()),
            }
        }
        if let Some(error) = (ctx.take_state_error)() {
            return Err(error.into());
        }
        pay(p, 0, &mut ctx.cancelled)?;
        let out = QualifiedCollectionControlsProjectionColumnsV1 {
            counts: expected,
            cursor,
            data: CurrentCollectionControlsProjectionDeltaV1 {
                rows,
                groups,
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

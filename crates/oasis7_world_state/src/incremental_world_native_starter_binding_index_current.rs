use super::*;
/// A bounded observation of actual Current rows. No authentication, admission,
/// nonce, Accepted, durable-history or business permission is issued here.
pub struct StarterBindingObservationV1 {
    count: u64,
    unique: Option<HeldRow>,
    identity: PreparedGuard,
    memory: MemoryBudget,
}
impl StarterBindingObservationV1 {
    pub fn matching_claims(&self) -> u64 {
        self.count
    }
    pub fn unique_agent_id(&self) -> Option<&str> {
        self.unique.as_ref().map(|row| row.agent.as_str())
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &mut ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.contribute(|tx| {
            tx.validate_guard(&self.identity, live)?;
            if !self.memory.same_pool(memory) || !tx.uses_budget(memory) {
                return Err(invalid().into());
            }
            Ok(())
        })
    }
}
struct Pending {
    key: WorldFieldKeyV1,
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
}
fn range(
    rows: &[HeldRow],
    player: &str,
    public_key: &str,
    upper: bool,
    work: &mut StarterBindingIndexWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<usize, ValueTreeError> {
    let (mut lo, mut hi) = (0, rows.len());
    while lo < hi {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        let mid = (lo + hi) / 2;
        let row = &rows[mid];
        work.comparison(
            player
                .len()
                .checked_add(public_key.len())
                .and_then(|n| n.checked_add(row.player.len()))
                .and_then(|n| n.checked_add(row.public_key.as_ref().map_or(0, String::len)))
                .ok_or_else(invalid)?,
        )?;
        let order =
            (row.player.as_str(), row.public_key.as_deref()).cmp(&(player, Some(public_key)));
        if order.is_lt() || (upper && order.is_eq()) {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    Ok(lo)
}
fn count_row(
    row: HeldRow,
    player: &str,
    public_key: &str,
    count: &mut u64,
    unique: &mut Option<HeldRow>,
    work: &mut StarterBindingIndexWorkV1,
) -> Result<(), ValueTreeError> {
    work.comparison(
        row.player
            .len()
            .checked_add(player.len())
            .and_then(|n| n.checked_add(public_key.len()))
            .and_then(|n| n.checked_add(row.public_key.as_ref().map_or(0, String::len)))
            .ok_or_else(invalid)?,
    )?;
    if row.player == player && row.public_key.as_deref() == Some(public_key) {
        *count = count.checked_add(1).ok_or_else(invalid)?;
        if *count == 1 {
            *unique = Some(row);
        } else {
            *unique = None;
        }
    }
    Ok(())
}
/// Query the genuine parent index plus only this transaction's typed deltas.
/// Unchanged parent matching members are independently re-read and decoded;
/// nonmatching old records and the full parent field are never scanned here.
#[allow(clippy::too_many_arguments)]
pub fn observe_current_starter_binding_v1<M, P, R, E, C>(
    player: &str,
    public_key: &str,
    parent: &ReservedValue<StarterBindingIndexV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    native: &mut WorldNativeQualificationWorkV1,
    work: &mut StarterBindingIndexWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<StarterBindingObservationV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    tx.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        read::check(ctx)?;
        let _phase = ctx
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(ctx.values.memory)
            || !parent.uses_budget(ctx.values.memory)
        {
            return Err(invalid().into());
        }
        parent.validate(roots, ctx.values.memory, MemoryClass::Ordinary)?;
        if roots.digest().map_err(|_| invalid())? != tx.parent_fence().parent_roots
            || ctx.root != roots.state_root(Field::profile()?.namespace()?)
        {
            return Err(invalid().into());
        }
        if parent.records() > work.max_rows || work.max_pending_rows == 0 {
            return Err(ValueTreeError::ResourceLimited("starter Current limits").into());
        }
        work.spend(
            player
                .len()
                .checked_add(public_key.len())
                .and_then(|n| n.checked_add(std::mem::size_of::<PreparedGuard>()))
                .ok_or_else(invalid)?,
        )?;
        let namespace = ctx.root.namespace;
        let metadata =
            WorldFieldKeyV1::new(Field::descriptor()?, WorldFieldRecordKindV1::Metadata, None)
                .map_err(ValueTreeError::Invalid)?;
        let value = tx
            .read(namespace, metadata.as_bytes())?
            .ok_or_else(invalid)?;
        read::check(ctx)?;
        if value != parent.metadata {
            return Err(invalid().into());
        }
        ctx.values
            .verify_closure(&value, ctx.record_bytes, &mut ctx.cancelled)?;
        ctx.records().presence(&value, &metadata, 0)?;
        let fee = ctx
            .values
            .memory
            .reserve(
                std::mem::size_of::<StarterBindingObservationV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let pending_bytes = work
            .max_pending_rows
            .checked_mul(std::mem::size_of::<Pending>())
            .ok_or_else(invalid)?;
        let _pending_fee = ctx
            .values
            .memory
            .reserve(pending_bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut pending = Vec::with_capacity(work.max_pending_rows);
        let mut remaining = std::mem::take(&mut work.pending_entries_remaining);
        let inspected = tx.visit_pending_writes::<WorldFieldOverlayErrorV1>(
            namespace,
            &mut remaining,
            || read::check(ctx).map_err(Into::into),
            |key, before, after| {
                let Some(kind) = entry_kind(key, namespace)? else {
                    return Ok(());
                };
                match kind {
                    WorldFieldRecordKindV1::Metadata => {
                        if before != after {
                            return Err(invalid().into());
                        }
                    }
                    WorldFieldRecordKindV1::Entry => {
                        work.spend(key.len())?;
                        if pending.len() == pending.capacity() {
                            return Err(ValueTreeError::ResourceLimited(
                                "starter Current pending rows",
                            )
                            .into());
                        }
                        let parsed =
                            WorldFieldKeyV1::from_bytes(key).map_err(ValueTreeError::Invalid)?;
                        let key =
                            WorldFieldKeyV1::new(Field::descriptor()?, kind, parsed.semantic_key)
                                .map_err(ValueTreeError::Invalid)?;
                        pending.push(Pending { key, before, after });
                    }
                    _ => return Err(invalid().into()),
                }
                Ok(())
            },
        );
        work.pending_entries_remaining = remaining;
        inspected?;
        let mut total = i128::try_from(parent.records()).map_err(|_| invalid())?;
        let (mut count, mut unique) = (0u64, None);
        // Reconcile each changed real outer key with its exact parent member.
        for change in &pending {
            read::check(ctx)?;
            let old = outer_position(&parent.by_outer, &change.key, work, &mut ctx.cancelled)?;
            match (old, change.before) {
                (Ok(pos), Some(before)) if parent.by_outer[pos].member == before => {}
                (Err(_), None) => {}
                _ => return Err(invalid().into()),
            }
            total += i128::from(change.after.is_some()) - i128::from(change.before.is_some());
            if let Some(after) = change.after {
                let actual = tx.read(namespace, change.key.as_bytes())?;
                read::check(ctx)?;
                if actual != Some(after) {
                    return Err(invalid().into());
                }
                let _entry = ctx
                    .values
                    .memory
                    .reserve(
                        change.key.as_bytes().len() + std::mem::size_of::<StateEntryV1>(),
                        MemoryClass::Ordinary,
                    )
                    .map_err(ValueTreeError::Memory)?;
                let row = typed_row(
                    &StateEntryV1 {
                        namespace,
                        key: change.key.as_bytes().to_vec(),
                        value: after,
                    },
                    ctx,
                    scan,
                    native,
                    work,
                )?;
                count_row(row, player, public_key, &mut count, &mut unique, work)?;
            }
        }
        if total < 0 || total > i128::try_from(work.max_rows).map_err(|_| invalid())? {
            return Err(ValueTreeError::ResourceLimited("starter Current row count").into());
        }
        let begin = range(
            &parent.by_binding,
            player,
            public_key,
            false,
            work,
            &mut ctx.cancelled,
        )?;
        let end = range(
            &parent.by_binding,
            player,
            public_key,
            true,
            work,
            &mut ctx.cancelled,
        )?;
        for row in &parent.by_binding[begin..end] {
            read::check(ctx)?;
            let mut changed = false;
            for change in &pending {
                read::check(ctx)?;
                work.comparison(
                    row.key
                        .as_bytes()
                        .len()
                        .checked_add(change.key.as_bytes().len())
                        .ok_or_else(invalid)?,
                )?;
                if row.key == change.key {
                    changed = true;
                    break;
                }
            }
            if changed {
                continue;
            }
            let actual = tx
                .read(namespace, row.key.as_bytes())?
                .ok_or_else(invalid)?;
            read::check(ctx)?;
            if actual != row.member {
                return Err(invalid().into());
            }
            let _entry = ctx
                .values
                .memory
                .reserve(
                    row.key.as_bytes().len() + std::mem::size_of::<StateEntryV1>(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            let decoded = typed_row(
                &StateEntryV1 {
                    namespace,
                    key: row.key.as_bytes().to_vec(),
                    value: actual,
                },
                ctx,
                scan,
                native,
                work,
            )?;
            work.comparison(
                row_text(&decoded)?
                    .checked_add(row_text(row)?)
                    .ok_or_else(invalid)?,
            )?;
            if decoded.outer != row.outer
                || decoded.player != row.player
                || decoded.public_key != row.public_key
                || decoded.agent != row.agent
            {
                return Err(invalid().into());
            }
            count_row(decoded, player, public_key, &mut count, &mut unique, work)?;
        }
        read::check(ctx)?;
        let tail = tx.guard();
        tx.validate_guard(&tail, live)?;
        let result = fee.retain(StarterBindingObservationV1 {
            count,
            unique,
            identity: tail,
            memory: ctx.values.memory.clone(),
        });
        read::check(ctx)?;
        Ok(result)
    })
}

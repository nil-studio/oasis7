use super::*;
use oasis7_world_state::incremental_runtime_history_index::read_runtime_history_index_point_v1;
fn job(job: u64, index: Option<u32>, position: u64) -> tree::Key {
    let mut bytes = [0u8; 13];
    bytes[..8].copy_from_slice(&job.to_be_bytes());
    if let Some(index) = index {
        bytes[8] = 1;
        bytes[9..].copy_from_slice(&index.to_be_bytes());
    }
    tree::Key::new(&bytes, position)
}
pub(super) fn recorded_key(receipt: &ProductValidationReceiptV1, position: u64) -> tree::Key {
    job(receipt.job_id, receipt.validation_index, position)
}
pub(super) fn attempt_key(attempt: &ProductValidationAttemptV1, position: u64) -> tree::Key {
    job(attempt.job_id, attempt.validation_index, position)
}
pub(super) fn lookup(
    requester: &str,
    module: &str,
    kind: &str,
    amount: i64,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<[u8; 32], DeliveryDataErrorV1> {
    let mut hash = blake3::Hasher::new();
    hash.update(b"oasis7.product-delivery.lookup.v1\0");
    for field in [requester, module, kind] {
        common.charge(1, 8)?;
        hash.update(&(field.len() as u64).to_be_bytes());
        for part in field.as_bytes().chunks(16 * 1024) {
            common.charge(1, part.len() as u64)?;
            hash.update(part);
        }
    }
    common.charge(1, 8)?;
    hash.update(&amount.to_be_bytes());
    Ok(*hash.finalize().as_bytes())
}
fn key(slot: &EventSlot) -> Option<(u8, tree::Key)> {
    match &slot.value().body {
        WorldEventBody::Domain(DomainEvent::ProductValidationRecorded { receipt }) => {
            Some((0, recorded_key(receipt, slot.position)))
        }
        WorldEventBody::Domain(DomainEvent::ProductValidationAttemptStarted { attempt }) => {
            Some((1, attempt_key(attempt, slot.position)))
        }
        WorldEventBody::Domain(DomainEvent::ProductValidated { .. }) => {
            Some((2, tree::Key::new(&slot.lookup, slot.position)))
        }
        WorldEventBody::ModuleRuntimeCharged(charge) => {
            Some((3, tree::Key::new(charge.trace_id.as_bytes(), slot.position)))
        }
        _ => None,
    }
}
fn root(trees: &mut Trees, kind: u8) -> &mut Option<tree::NodeRef> {
    match kind {
        0 => &mut trees.recorded,
        1 => &mut trees.attempts,
        2 => &mut trees.deliveries,
        _ => &mut trees.charges,
    }
}
pub(super) fn trim(
    data: &mut Data,
    floor: u64,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), DeliveryDataErrorV1> {
    while let Some(slot) = tree::lower(&data.trees.positions, &tree::Key::new(&[], 0), common)? {
        if slot.position >= floor {
            break;
        }
        let (kind, index_key) =
            key(&slot).ok_or_else(|| invalid("delivery retained source kind"))?;
        let selected = root(&mut data.trees, kind);
        *selected = tree::remove(selected, &index_key, common)?;
        data.trees.positions = tree::remove(
            &data.trees.positions,
            &tree::Key::new(&[], slot.position),
            common,
        )?;
    }
    data.start = floor;
    Ok(())
}
pub(super) fn append(
    data: &mut Data,
    event: ReservedValue<DecodedWorldEventV1>,
    original: [u8; 32],
    encoded: u64,
    current: Option<CountedWorldEventSourceV1>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), DeliveryDataErrorV1> {
    common.consume_steps(1)?;
    if !event.uses_budget(common.memory()) || event.allocation_class() != MemoryClass::Ordinary {
        return Err(invalid("delivery decoded source pool/class"));
    }
    let id = event.event().id;
    if data.last_id == Some(u64::MAX) && id == 1 {
        data.wraps = data.wraps.checked_add(1).ok_or_else(limited)?;
    }
    let position = data.end;
    data.end = data.end.checked_add(1).ok_or_else(limited)?;
    data.last_id = Some(id);
    let hash = match &event.event().body {
        WorldEventBody::Domain(DomainEvent::ProductValidated {
            requester_agent_id,
            module_id,
            stack,
            ..
        }) => lookup(
            requester_agent_id,
            module_id,
            &stack.kind,
            stack.amount,
            common,
        )?,
        _ => [0; 32],
    };
    let relevant = matches!(
        &event.event().body,
        WorldEventBody::Domain(
            DomainEvent::ProductValidated { .. }
                | DomainEvent::ProductValidationRecorded { .. }
                | DomainEvent::ProductValidationAttemptStarted { .. }
        ) | WorldEventBody::ModuleRuntimeCharged(_)
    );
    if relevant {
        let fee = common
            .memory()
            .reserve(arc_heap::<EventSlot>()?, MemoryClass::Ordinary)?;
        let event = Arc::new(fee.retain(EventSlot {
            event,
            original,
            encoded,
            position,
            wraps: data.wraps,
            lookup: hash,
            current,
        }));
        let (kind, key) = key(&event).ok_or_else(|| invalid("delivery source classification"))?;
        let selected = root(&mut data.trees, kind);
        *selected = Some(tree::put(selected, key, event.clone(), common)?);
        data.trees.positions = Some(tree::put(
            &data.trees.positions,
            tree::Key::new(&[], position),
            event,
            common,
        )?);
    }
    Ok(())
}
pub(super) fn point<M, R, E, C>(
    history: &AcceptedRuntimeHistorySourceV1,
    ordinal: u64,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut ProductValidationDeliveryWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<
    oasis7_world_state::incremental_runtime_history_index::RuntimeHistoryIndexRowV1,
    DeliveryDataErrorV1,
>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    common.charge(256, 512)?;
    let (_, meter, stop) = common.parts();
    let child = meter.begin_child()?;
    let steps = child.remaining_steps() / 2;
    let bytes = child.remaining_read_bytes() / 2;
    read.values.io.nodes_remaining = read.values.io.nodes_remaining.min(steps);
    work.index.cells_remaining = work.index.cells_remaining.min(steps);
    read.values.io.read_bytes_remaining = read.values.io.read_bytes_remaining.min(bytes);
    read.key_work.bytes_remaining = read.key_work.bytes_remaining.min(bytes);
    let before = [
        read.values.io.nodes_remaining,
        work.index.cells_remaining,
        read.values.io.read_bytes_remaining,
        read.key_work.bytes_remaining,
    ];
    let result = {
        let mut local = WorldFieldReadContextV1 {
            root: &history.manifest().lanes[RuntimeHistoryLaneV1::Journal.index()].root,
            state: read.state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: read.values.source,
                memory: read.values.memory,
                class: read.values.class,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: read.semantic_bytes,
            record_bytes: read.record_bytes,
            take_state_error: &mut read.take_state_error,
            cancelled: || (read.cancelled)() || stop(),
        };
        read_runtime_history_index_point_v1(
            RuntimeHistoryLaneV1::Journal,
            &history.manifest().lanes[RuntimeHistoryLaneV1::Journal.index()],
            ordinal,
            &mut local,
            &mut work.index,
        )
    };
    let after = [
        read.values.io.nodes_remaining,
        work.index.cells_remaining,
        read.values.io.read_bytes_remaining,
        read.key_work.bytes_remaining,
    ];
    let debit = |mut range: std::ops::Range<usize>| {
        range.try_fold(0u64, |n, i| n.checked_add(before[i].checked_sub(after[i])?))
    };
    let steps = debit(0..2).ok_or_else(limited)?;
    let bytes = debit(2..4).ok_or_else(limited)?;
    let paid = child.consume(steps, bytes);
    child.finish();
    let result = result?;
    paid?;
    common.consume_steps(0)?;
    Ok(result)
}
pub(in crate::world) fn build_product_validation_delivery_index_v1<M, R, E, C, O>(
    head: &VerifiedFormalCommitV1,
    history: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
    roots: &RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    originals: &mut O,
    work: &mut ProductValidationDeliveryWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<ProductValidationDeliveryIndexV1>, DeliverySourceErrorV1<O::Error>>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
{
    let setup = (|| {
        common.charge(1024, 4096)?;
        let occurrence = history.occurrence();
        let window = &history.manifest().lanes[RuntimeHistoryLaneV1::Journal.index()];
        let index = history.history_index(RuntimeHistoryLaneV1::Journal);
        if !history.uses_budget(common.memory())
            || history.allocation_class() != MemoryClass::Ordinary
            || occurrence.kind() != ExecutionFactKindV1::Result
            || occurrence.head_record_id() != head.verified_record_id()
            || occurrence.head_body_id() != occurrence.fact_body_id()
            || occurrence.roots_id() != history.manifest().roots
            || roots.world_id.len() > 256
            || roots.state.len() != 6
            || roots.digest()? != history.manifest().roots
            || index.window() != window
            || index.lane() != RuntimeHistoryLaneV1::Journal
            || !index.uses_budget(common.memory())
            || index.allocation_class() != MemoryClass::Ordinary
            || !read.values.memory.same_pool(common.memory())
            || read.values.class != MemoryClass::Ordinary
        {
            return Err(invalid("delivery accepted Journal scope"));
        }
        let fee = common.memory().reserve(
            size_of::<ProductValidationDeliveryIndexV1>() + 4096,
            MemoryClass::Ordinary,
        )?;
        Ok((fee, window.start, window.end()?))
    })()
    .map_err(DeliverySourceErrorV1::Data)?;
    let (fee, start, end) = setup;
    let mut data = Data {
        trees: Trees::default(),
        start,
        end: start,
        last_id: None,
        wraps: 0,
    };
    for ordinal in start..end {
        let row = point(history, ordinal, read, work, common)?;
        let (event, encoded) = original::read(
            &row.location.original,
            Some(row.encoded_bytes),
            originals,
            work,
            common,
            &mut read.cancelled,
        )?;
        append(
            &mut data,
            event,
            row.location.original,
            encoded,
            None,
            common,
        )?;
    }
    common.consume_steps(0).map_err(DeliveryDataErrorV1::from)?;
    let occurrence = history.occurrence();
    Ok(fee.retain(ProductValidationDeliveryIndexV1 {
        data,
        head: head.verified_record_id(),
        body: occurrence.head_body_id(),
        roots: occurrence.roots_id(),
        parent_original: occurrence.location().original,
    }))
}
impl ProductValidationDeliveryIndexV1 {
    pub(in crate::world) fn begin_current<'a, M: MerkleNodeReader>(
        &'a self,
        owner: &'a ReservedValue<Self>,
        head: &VerifiedFormalCommitV1,
        retention: &'a ReservedValue<WorldJournalRetentionV1>,
        counter: &WorldEventCounterV1,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<CurrentProductValidationDeliveryIndexV1<'a>, DeliveryDataErrorV1> {
        common.consume_steps(1)?;
        counter.validate(tx)?;
        counter.bind_journal_retention_v1(tx, retention)?;
        tx.validate_guard(&tx.guard(), live)?;
        if !std::ptr::eq(self, &**owner)
            || !owner.uses_budget(common.memory())
            || !retention.uses_budget(common.memory())
            || retention.allocation_class() != MemoryClass::Ordinary
            || head.verified_record_id() != self.head
            || self.body != tx.parent_fence().parent_commit
            || self.roots != tx.parent_fence().parent_roots
        {
            return Err(invalid("delivery resident parent binding"));
        }
        let fee = common
            .memory()
            .reserve(
                size_of::<CurrentProductValidationDeliveryIndexV1<'_>>() + 128,
                MemoryClass::Ordinary,
            )?
            .retain(());
        Ok(CurrentProductValidationDeliveryIndexV1 {
            parent: owner,
            retention,
            data: self.data.clone(),
            identity: tx.guard(),
            core: common.work().identity(),
            counter: counter.clone(),
            next_fact: 0,
            appended: 0,
            references: None,
            fee,
        })
    }
}
impl CurrentProductValidationDeliveryIndexV1<'_> {
    pub(super) fn refresh<
        M: MerkleNodeReader,
        O: OriginalObjectStreamReader,
        C: FnMut() -> bool,
    >(
        &mut self,
        tx: &mut ExecutionTransaction<'_, M>,
        originals: &mut O,
        work: &mut ProductValidationDeliveryWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        cancelled: &mut C,
    ) -> Result<(), DeliverySourceErrorV1<O::Error>> {
        self.validate(tx, common)?;
        let parent_hot = self
            .parent
            .data
            .end
            .checked_sub(self.parent.data.start)
            .ok_or_else(limited)?;
        let total = self.counter.journal_append_height_v1(tx)?;
        let new = total
            .checked_sub(parent_hot)
            .and_then(|n| n.checked_sub(self.appended))
            .ok_or_else(limited)?;
        if new == 0 {
            return Ok(());
        }
        let n = usize::try_from(new).map_err(|_| limited())?;
        common
            .charge(
                new,
                new.checked_mul(size_of::<CountedWorldEventSourceV1>() as u64)
                    .ok_or_else(limited)?,
            )
            .map_err(DeliveryDataErrorV1::from)?;
        let _fee = common
            .memory()
            .reserve(
                n.checked_mul(size_of::<CountedWorldEventSourceV1>())
                    .and_then(|n| n.checked_add(size_of::<Vec<CountedWorldEventSourceV1>>()))
                    .ok_or_else(limited)?,
                MemoryClass::Ordinary,
            )
            .map_err(DeliveryDataErrorV1::from)?;
        let mut sources = Vec::new();
        sources.try_reserve_exact(n).map_err(|_| limited())?;
        if sources.capacity() > n {
            return Err(limited().into());
        }
        let mut failure = None;
        let visited = self
            .counter
            .visit_counted_sources_since(tx, self.next_fact, |source| {
                if let Err(error) = common.consume_steps(1) {
                    failure = Some(DeliveryDataErrorV1::from(error));
                    return Err(BodyMutationError::Invalid(
                        "delivery counted traversal stopped",
                    ));
                }
                if sources.len() == n {
                    return Err(BodyMutationError::Invalid(
                        "delivery counted source cardinality",
                    ));
                }
                sources.push(source);
                Ok(())
            });
        if let Some(error) = failure {
            return Err(error.into());
        }
        visited.map_err(DeliveryDataErrorV1::from)?;
        if sources.len() != n {
            return Err(invalid("delivery typed source count closure").into());
        }
        common
            .consume_steps(new)
            .map_err(DeliveryDataErrorV1::from)?;
        sources.reverse();
        let mut data = self.data.clone();
        let mut references = self.references.clone();
        let mut next = self.next_fact;
        for source in sources {
            source.validate(tx)?;
            next = source.ordinal().checked_add(1).ok_or_else(limited)?;
            let id = *source.original();
            let (event, encoded) = original::read(&id, None, originals, work, common, cancelled)?;
            let position = data.end;
            let ordinal = source.ordinal();
            let reference_heap = size_of::<JournalReference>()
                .checked_add(2 * size_of::<usize>())
                .and_then(|n| n.checked_add(4 * align_of::<JournalReference>()))
                .ok_or_else(limited)?;
            let reference_fee = common
                .memory()
                .reserve(reference_heap, MemoryClass::Ordinary)
                .map_err(DeliveryDataErrorV1::from)?;
            references = Some(Arc::new(JournalReference {
                original: id,
                encoded,
                position,
                ordinal,
                previous: references,
                _fee: reference_fee,
            }));
            append(&mut data, event, id, encoded, Some(source), common)?;
            let maximum = u64::try_from(self.retention.max_events()).map_err(|_| limited())?;
            let floor = data.end.saturating_sub(maximum).max(data.start);
            trim(&mut data, floor, common)?;
        }
        common.consume_steps(0).map_err(DeliveryDataErrorV1::from)?;
        self.data = data;
        self.references = references;
        self.next_fact = next;
        self.appended = self.appended.checked_add(new).ok_or_else(limited)?;
        Ok(())
    }
}

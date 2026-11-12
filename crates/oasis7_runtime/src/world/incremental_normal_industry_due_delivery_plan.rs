use super::*;
use crate::world::incremental_body::BodyMutationContext;
use oasis7_world_state::ProductValidationDeliveryCursor;
pub(in crate::world) struct ProductValidationReplayItemV1 {
    slot: EventRef,
    era: u64,
    routed: bool,
    body: [u8; 32],
    roots: [u8; 32],
    accepted_end: u64,
    identity: PreparedGuard,
    core: WorkIdentityV1,
}
impl ProductValidationReplayItemV1 {
    pub(in crate::world) fn event(&self) -> &WorldEvent {
        self.slot.value()
    }
    pub(in crate::world) fn event_id_era(&self) -> u64 {
        self.era
    }
    pub(in crate::world) fn was_routed_at_plan(&self) -> bool {
        self.routed
    }
    fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), DeliveryDataErrorV1> {
        common.consume_steps(1)?;
        tx.validate_transaction_identity(&self.identity)?;
        if self.body != tx.parent_fence().parent_commit
            || self.roots != tx.parent_fence().parent_roots
            || self.core != common.work().identity()
            || !self.slot.uses_budget(common.memory())
        {
            return Err(invalid("delivery exact replay source binding"));
        }
        if self.slot.position >= self.accepted_end {
            self.slot
                .current
                .as_ref()
                .ok_or_else(|| invalid("delivery counted origin missing"))?
                .validate(tx)?;
        }
        Ok(())
    }
}
pub(in crate::world) struct ProductValidationDeliveryPlanV1 {
    items: [Option<ProductValidationReplayItemV1>; 2],
    has_delivery: bool,
    fee: ReservedValue<()>,
}
impl ProductValidationDeliveryPlanV1 {
    pub(in crate::world) fn iter(&self) -> impl Iterator<Item = &ProductValidationReplayItemV1> {
        self.items.iter().flatten()
    }
    pub(in crate::world) fn has_delivery(&self) -> bool {
        self.has_delivery
    }
    pub(super) fn into_replay_items(self) -> ProductValidationReplayIteratorV1 {
        ProductValidationReplayIteratorV1 {
            plan: self,
            next: 0,
        }
    }
}
/// The consumed plan and its actual metadata admission stay owned throughout
/// iteration. Moving a typed item retains its original paid slot/event Arc.
pub(super) struct ProductValidationReplayIteratorV1 {
    plan: ProductValidationDeliveryPlanV1,
    next: usize,
}
impl Iterator for ProductValidationReplayIteratorV1 {
    type Item = ProductValidationReplayItemV1;
    fn next(&mut self) -> Option<Self::Item> {
        while self.next < self.plan.items.len() {
            let at = self.next;
            self.next += 1;
            if let Some(item) = self.plan.items[at].take() {
                return Some(item);
            }
        }
        None
    }
}
/// Sole exact-route input. Its original event/era can be borrowed only after
/// the fresh Current cursor and charged-trace test have completed.
pub(in crate::world) struct PreparedProductValidationReplayV1 {
    item: ProductValidationReplayItemV1,
    guard: PreparedGuard,
    fee: ReservedValue<()>,
}
impl PreparedProductValidationReplayV1 {
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), DeliveryDataErrorV1> {
        self.item.validate(tx, common)?;
        tx.validate_guard(&self.guard, live)?;
        if !self.fee.uses_budget(common.memory()) {
            return Err(invalid("delivery prepared route pool"));
        }
        Ok(())
    }
    pub(super) fn validate_evidence_identity<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        identity: WorkIdentityV1,
    ) -> Result<(), DeliveryDataErrorV1> {
        tx.validate_transaction_identity(&self.item.identity)?;
        if self.item.core != identity
            || self.item.body != tx.parent_fence().parent_commit
            || self.item.roots != tx.parent_fence().parent_roots
            || !self.item.slot.uses_budget(memory)
            || !self.fee.uses_budget(memory)
            || !tx.uses_budget(memory)
            || !World::is_product_validation_event(self.event())
        {
            return Err(invalid("delivery routed origin identity"));
        }
        if self.item.slot.position >= self.item.accepted_end {
            self.item
                .slot
                .current
                .as_ref()
                .ok_or_else(|| invalid("delivery routed counted origin"))?
                .validate(tx)?;
        }
        Ok(())
    }
    pub(in crate::world) fn event(&self) -> &WorldEvent {
        self.item.event()
    }
    pub(in crate::world) fn event_id_era(&self) -> u64 {
        self.item.era
    }
}
fn cursor<M, R, E, C>(
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ProductValidationDeliveryCursor, DeliveryDataErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    common.charge(128, 256)?;
    let (_, meter, stop) = common.parts();
    let child = meter.begin_child()?;
    overlay.values.io.nodes_remaining = overlay
        .values
        .io
        .nodes_remaining
        .min(child.remaining_steps());
    let bytes = child.remaining_read_bytes() / 2;
    overlay.values.io.read_bytes_remaining = overlay.values.io.read_bytes_remaining.min(bytes);
    overlay.key_work.bytes_remaining = overlay.key_work.bytes_remaining.min(bytes);
    let before = [
        overlay.values.io.nodes_remaining,
        overlay.values.io.read_bytes_remaining,
        overlay.key_work.bytes_remaining,
    ];
    let result = {
        let mut local = WorldFieldOverlayContextV1 {
            transaction: &mut *overlay.transaction,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: overlay.values.source,
                memory: overlay.values.memory,
                class: overlay.values.class,
                io: &mut *overlay.values.io,
            },
            key_work: &mut *overlay.key_work,
            semantic_bytes: overlay.semantic_bytes,
            record_bytes: overlay.record_bytes,
            take_state_error: &mut overlay.take_state_error,
            cancelled: || (overlay.cancelled)() || stop(),
        };
        oasis7_world_state::incremental_delivery_cursor::read_delivery_cursor_overlay_v1(&mut local)
    };
    let after = [
        overlay.values.io.nodes_remaining,
        overlay.values.io.read_bytes_remaining,
        overlay.key_work.bytes_remaining,
    ];
    let steps = before[0].checked_sub(after[0]).ok_or_else(limited)?;
    let bytes = before[1]
        .checked_sub(after[1])
        .and_then(|n| n.checked_add(before[2].checked_sub(after[2])?))
        .ok_or_else(limited)?;
    let paid = child.consume(steps, bytes);
    child.finish();
    let value = result?;
    paid?;
    common.consume_steps(0)?;
    Ok(value)
}
fn sequence<M, R, W, C>(
    tx: &mut ExecutionTransaction<'_, M>,
    values: &R,
    body: &mut BodyMutationContext<'_, W, C>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<crate::world::incremental_allocators::RuntimeSequenceV1, DeliveryDataErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    C: FnMut() -> bool,
{
    common.charge(128, 256)?;
    if !body.memory.same_pool(common.memory()) {
        return Err(invalid("delivery allocator pool"));
    }
    let (_, meter, stop) = common.parts();
    let child = meter.begin_child()?;
    body.io.nodes_remaining = body.io.nodes_remaining.min(child.remaining_steps());
    body.io.read_bytes_remaining = body
        .io
        .read_bytes_remaining
        .min(child.remaining_read_bytes());
    let before = (body.io.nodes_remaining, body.io.read_bytes_remaining);
    let result = {
        let mut local = BodyMutationContext {
            sink: &mut *body.sink,
            memory: body.memory,
            io: &mut *body.io,
            cancelled: || (body.cancelled)() || stop(),
        };
        World::read_iws_allocator(
            crate::world::incremental_allocators::RuntimeAllocatorV1::Event,
            tx,
            values,
            &mut local,
        )
    };
    let after = (body.io.nodes_remaining, body.io.read_bytes_remaining);
    let steps = before.0.checked_sub(after.0).ok_or_else(limited)?;
    let bytes = before.1.checked_sub(after.1).ok_or_else(limited)?;
    let paid = child.consume(steps, bytes);
    child.finish();
    let value = result?;
    paid?;
    common.consume_steps(0)?;
    Ok(value)
}
fn routed(
    data: &Data,
    event: &EventSlot,
    era: u64,
    cursor: &ProductValidationDeliveryCursor,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<bool, DeliveryDataErrorV1> {
    common.consume_steps(1)?;
    if cursor.has_routed(era, event.value().id) {
        return Ok(true);
    }
    let fee = common
        .memory()
        .reserve(size_of::<String>() + 128, MemoryClass::Ordinary)?;
    common.charge(128, 128)?;
    let prefix = World::product_validation_event_trace_prefix_v1(era, event.value().id);
    if prefix.capacity() > 128 || prefix.len() > 64 {
        return Err(limited());
    }
    let trace = tree::lower(
        &data.trees.charges,
        &tree::Key::new(prefix.as_bytes(), 0),
        common,
    )?;
    let result=trace.is_some_and(|slot|matches!(&slot.value().body,WorldEventBody::ModuleRuntimeCharged(charge) if charge.trace_id.starts_with(&prefix)));
    drop(fee);
    Ok(result)
}
fn compare_fee(
    slot: &EventSlot,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), DeliveryDataErrorV1> {
    common.charge(
        slot.encoded
            .checked_mul(8)
            .and_then(|n| n.checked_add(512))
            .ok_or_else(limited)?,
        slot.encoded.checked_mul(8).ok_or_else(limited)?,
    )?;
    Ok(())
}
impl CurrentProductValidationDeliveryIndexV1<'_> {
    fn item<M: MerkleNodeReader>(
        &self,
        slot: EventRef,
        tail_era: u64,
        cursor: &ProductValidationDeliveryCursor,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ProductValidationReplayItemV1, DeliveryDataErrorV1> {
        let wraps = self
            .data
            .wraps
            .checked_sub(slot.wraps)
            .ok_or_else(limited)?;
        let era = World::product_validation_era_before_wraps_v1(tail_era, wraps);
        let was_routed = routed(&self.data, &slot, era, cursor, common)?;
        Ok(ProductValidationReplayItemV1 {
            slot,
            era,
            routed: was_routed,
            body: self.parent.body,
            roots: self.parent.roots,
            accepted_end: self.parent.data.end,
            identity: tx.guard(),
            core: common.work().identity(),
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn plan_receipt<M, R, E, C, W, D, O>(
        &mut self,
        receipt: &ProductValidationReceiptV1,
        overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        body: &mut BodyMutationContext<'_, W, D>,
        originals: &mut O,
        work: &mut ProductValidationDeliveryWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<ProductValidationDeliveryPlanV1, DeliverySourceErrorV1<O::Error>>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        D: FnMut() -> bool,
        O: OriginalObjectStreamReader,
    {
        let result = (|| {
            self.refresh(
                overlay.transaction,
                originals,
                work,
                common,
                &mut overlay.cancelled,
            )?;
            let cursor = cursor(overlay, common)?;
            let seq = sequence(overlay.transaction, overlay.values.source, body, common)?;
            let tail = World::product_validation_tail_event_id_era_v1(
                seq.next,
                seq.era,
                self.data.last_id,
            );
            let key = source::recorded_key(receipt, 0);
            let mut recorded = None;
            tree::range(
                &self.data.trees.recorded,
                key.data(),
                self.data.start,
                self.data.end,
                true,
                common,
                &mut |slot, common| {
                    compare_fee(slot, common)?;
                    if matches!(&slot.value().body,WorldEventBody::Domain(DomainEvent::ProductValidationRecorded {receipt:actual}) if actual==receipt)
                    {
                        recorded = Some(slot.clone());
                        return Ok(false);
                    }
                    Ok(true)
                },
            )?;
            let mut items = [None, None];
            let mut has_delivery = false;
            if let Some(recorded) = recorded {
                let after = recorded.position.checked_add(1).ok_or_else(limited)?;
                let next = tree::lower(
                    &self.data.trees.recorded,
                    &source::recorded_key(receipt, after),
                    common,
                )?;
                let end=next.filter(|slot|matches!(&slot.value().body,WorldEventBody::Domain(DomainEvent::ProductValidationRecorded {receipt:r}) if r.job_id==receipt.job_id&&r.validation_index==receipt.validation_index))
                    .map_or(self.data.end,|slot|slot.position);
                items[0] = Some(self.item(recorded, tail, &cursor, overlay.transaction, common)?);
                let lookup = source::lookup(
                    &receipt.requester_agent_id,
                    &receipt.module_id,
                    &receipt.stack.kind,
                    receipt.stack.amount,
                    common,
                )?;
                let mut delivery = None;
                tree::range(
                    &self.data.trees.deliveries,
                    &lookup,
                    after,
                    end,
                    false,
                    common,
                    &mut |slot, common| {
                        compare_fee(slot, common)?;
                        if World::product_validation_delivery_matches(slot.value(), receipt) {
                            delivery = Some(slot.clone());
                            return Ok(false);
                        }
                        Ok(true)
                    },
                )?;
                if let Some(delivery) = delivery {
                    has_delivery = true;
                    items[1] =
                        Some(self.item(delivery, tail, &cursor, overlay.transaction, common)?);
                }
            }
            overlay
                .transaction
                .validate_guard(&overlay.transaction.guard(), live)
                .map_err(DeliveryDataErrorV1::from)?;
            common.consume_steps(0).map_err(DeliveryDataErrorV1::from)?;
            let fee = common
                .memory()
                .reserve(
                    size_of::<ProductValidationDeliveryPlanV1>() + 128,
                    MemoryClass::Ordinary,
                )
                .map_err(DeliveryDataErrorV1::from)?;
            Ok(ProductValidationDeliveryPlanV1 {
                items,
                has_delivery,
                fee: fee.retain(()),
            })
        })();
        if result.is_err() {
            let _: Result<(), ExecutionError> = overlay
                .transaction
                .contribute(|_| Err(ExecutionError::Aborted));
        }
        result
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn prepare_exact_replay<M, R, E, C, O>(
        &mut self,
        item: ProductValidationReplayItemV1,
        overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        originals: &mut O,
        work: &mut ProductValidationDeliveryWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Option<PreparedProductValidationReplayV1>, DeliverySourceErrorV1<O::Error>>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        O: OriginalObjectStreamReader,
    {
        let result = (|| {
            self.refresh(
                overlay.transaction,
                originals,
                work,
                common,
                &mut overlay.cancelled,
            )?;
            item.validate(overlay.transaction, common)?;
            if item.routed {
                return Ok(None);
            }
            let current = cursor(overlay, common)?;
            if routed(&self.data, &item.slot, item.era, &current, common)? {
                return Ok(None);
            }
            let guard = overlay.transaction.guard();
            overlay
                .transaction
                .validate_guard(&guard, live)
                .map_err(DeliveryDataErrorV1::from)?;
            let fee = common
                .memory()
                .reserve(
                    size_of::<PreparedProductValidationReplayV1>() + 128,
                    MemoryClass::Ordinary,
                )
                .map_err(DeliveryDataErrorV1::from)?
                .retain(());
            common.consume_steps(0).map_err(DeliveryDataErrorV1::from)?;
            Ok(Some(PreparedProductValidationReplayV1 { item, guard, fee }))
        })();
        if result.is_err() {
            let _: Result<(), ExecutionError> = overlay
                .transaction
                .contribute(|_| Err(ExecutionError::Aborted));
        }
        result
    }
}

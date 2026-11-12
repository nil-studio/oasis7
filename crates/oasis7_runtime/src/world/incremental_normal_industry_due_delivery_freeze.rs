use super::*;
pub(in crate::world) struct FrozenProductValidationDeliveryIndexV1 {
    data: Data,
    parent_original: [u8; 32],
    references: Option<Arc<JournalReference>>,
    guard: PreparedGuard,
    memory: MemoryBudget,
    core: WorkIdentityV1,
}
impl CurrentProductValidationDeliveryIndexV1<'_> {
    pub(in crate::world) fn freeze<
        M: MerkleNodeReader,
        O: OriginalObjectStreamReader,
        C: FnMut() -> bool,
    >(
        mut self,
        tx: &mut ExecutionTransaction<'_, M>,
        originals: &mut O,
        work: &mut ProductValidationDeliveryWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        cancelled: &mut C,
        live: &ExecutionFence,
    ) -> Result<
        ReservedValue<FrozenProductValidationDeliveryIndexV1>,
        DeliverySourceErrorV1<O::Error>,
    > {
        let result = (|| {
            self.refresh(tx, originals, work, common, cancelled)?;
            self.validate(tx, common)?;
            let guard = tx.guard();
            tx.validate_guard(&guard, live)?;
            let fee = common
                .memory()
                .reserve(
                    size_of::<FrozenProductValidationDeliveryIndexV1>() + 128,
                    MemoryClass::Ordinary,
                )
                .map_err(DeliveryDataErrorV1::from)?;
            common.consume_steps(0).map_err(DeliveryDataErrorV1::from)?;
            Ok(fee.retain(FrozenProductValidationDeliveryIndexV1 {
                data: self.data,
                parent_original: self.parent.parent_original,
                references: self.references,
                guard,
                memory: common.memory().clone(),
                core: common.work().identity(),
            }))
        })();
        if result.is_err() {
            let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
        }
        result
    }
}
impl FrozenProductValidationDeliveryIndexV1 {
    pub(in crate::world) fn validate_guard<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), DeliveryDataErrorV1> {
        common.consume_steps(1)?;
        tx.validate_guard(&self.guard, live)?;
        if !self.memory.same_pool(common.memory()) || self.core != common.work().identity() {
            return Err(invalid("delivery frozen scope"));
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn rebind_actual_accepted_v1<M, R, E, C>(
        &self,
        binding: &SealedExecutionBindingV1,
        head: &VerifiedFormalCommitV1,
        history: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
        roots: &RootSetV1,
        read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
        work: &mut ProductValidationDeliveryWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<ProductValidationDeliveryIndexV1>, DeliveryDataErrorV1>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    {
        common.charge(2048, 8192)?;
        if !self.memory.same_pool(common.memory())
            || !history.uses_budget(common.memory())
            || history.allocation_class() != MemoryClass::Ordinary
            || !read.values.memory.same_pool(common.memory())
            || read.values.class != MemoryClass::Ordinary
        {
            return Err(invalid("delivery after acceptance pool/class"));
        }
        let _phase = common.memory().reserve(16384, MemoryClass::Ordinary)?;
        binding.validate_source_guard(&self.guard, common.memory())?;
        binding.validate_after_roots(roots, common.memory())?;
        let fact = history.occurrence();
        let window = &history.manifest().lanes[RuntimeHistoryLaneV1::Journal.index()];
        if fact.kind() != ExecutionFactKindV1::Result
            || fact.head_record_id() != head.verified_record_id()
            || fact.head_body_id() != fact.fact_body_id()
            || fact.roots_id() != binding.after_roots_digest()
            || history.manifest().roots != binding.after_roots_digest()
            || history.manifest().previous_view != Some(self.parent_original)
            || window.end()? != self.data.end
            || window.start < self.data.start
        {
            return Err(invalid("delivery actual accepted source lineage"));
        }
        let mut reference = self.references.clone();
        while let Some(current) = reference {
            common.consume_steps(1)?;
            if current.position >= window.start {
                let row = source::point(history, current.position, read, work, common)?;
                if row.location.original != current.original
                    || row.encoded_bytes != current.encoded
                    || row.location.height != fact.location().height
                    || row.location.ordinal != current.ordinal as u64
                {
                    return Err(invalid("delivery actual accepted counted locator join"));
                }
            }
            reference = current.previous.clone();
        }
        let mut data = self.data.clone();
        source::trim(&mut data, window.start, common)?;
        let fee = common.memory().reserve(
            size_of::<ProductValidationDeliveryIndexV1>() + 128,
            MemoryClass::Ordinary,
        )?;
        common.consume_steps(0)?;
        Ok(fee.retain(ProductValidationDeliveryIndexV1 {
            data,
            head: head.verified_record_id(),
            body: fact.head_body_id(),
            roots: fact.roots_id(),
            parent_original: fact.location().original,
        }))
    }
}

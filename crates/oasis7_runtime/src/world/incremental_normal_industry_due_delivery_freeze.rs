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

impl CurrentProductValidationDeliveryIndexV1<'_> {
    /// Terminal read-only capture retains the actual paid persistent graph;
    /// it does not reset or replace the ongoing Current delivery view.
    pub(in crate::world) fn capture_normal_resident_v1<M:MerkleNodeReader,O:OriginalObjectStreamReader,C:FnMut()->bool>(
        &mut self,tx:&mut ExecutionTransaction<'_,M>,originals:&mut O,work:&mut ProductValidationDeliveryWorkV1,common:&mut FormalBoundedContextV1<'_>,cancelled:&mut C,live:&ExecutionFence,
    )->Result<ReservedValue<FrozenProductValidationDeliveryIndexV1>,DeliverySourceErrorV1<O::Error>> {
        self.refresh(tx,originals,work,common,cancelled)?;self.validate(tx,common)?;
        common.charge(1,(size_of::<FrozenProductValidationDeliveryIndexV1>()+128)as u64).map_err(DeliveryDataErrorV1::from)?;
        let fee=common.memory().reserve(size_of::<FrozenProductValidationDeliveryIndexV1>()+128,MemoryClass::Ordinary).map_err(DeliveryDataErrorV1::from)?;
        let guard=tx.guard();tx.validate_guard(&guard,live).map_err(DeliveryDataErrorV1::from)?;
        Ok(fee.retain(FrozenProductValidationDeliveryIndexV1{data:self.data.clone(),parent_original:self.parent.parent_original,references:self.references.clone(),guard,memory:common.memory().clone(),core:common.work().identity()}))
    }
}

impl FrozenProductValidationDeliveryIndexV1 {
    /// Preaccept source refinement over the genuinely checked candidate. The
    /// output stays inside PreparedNormalHostResident until durable Finish.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn prepare_checked_candidate_v1<M,R,E,C>(
        &self,terminal:&crate::world::incremental_normal_host_finish::SealedNormalResidentSourceV1,
        core:&ReservedValue<oasis7_formal_protocol::incremental_material::CheckedCommitCore>,
        history:&ReservedValue<oasis7_world_state::incremental_runtime_history_source::CheckedRuntimeHistorySourceV1>,
        head:&VerifiedFormalCommitV1,roots:&RootSetV1,read:&mut WorldFieldReadContextV1<'_,M,R,E,C>,work:&mut ProductValidationDeliveryWorkV1,common:&mut FormalBoundedContextV1<'_>,
    )->Result<ReservedValue<ProductValidationDeliveryIndexV1>,DeliveryDataErrorV1>
    where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool {
        common.charge(2048,8192)?;
        terminal.validate_delivery_source_v1(self,common).map_err(|_|invalid("delivery actual terminal capture"))?;
        let fact=history.occurrence();let window=&history.manifest().lanes[RuntimeHistoryLaneV1::Journal.index()];
        if !core.uses_budget(common.memory())||!history.uses_budget(common.memory())||!self.memory.same_pool(common.memory())||self.core!=common.work().identity()
            ||core.candidate_record_id()!=head.verified_record_id()||fact.candidate_record_id()!=core.candidate_record_id()||fact.body_id()!=core.catalog().entry.commit
            ||fact.roots_id()!=terminal.after_roots()||core.body().roots!=terminal.after_roots()||history.manifest().roots!=terminal.after_roots()
            ||roots.digest()?!=terminal.after_roots()||history.manifest().previous_view!=Some(self.parent_original)||window.end()?!=self.data.end||window.start<self.data.start {
            return Err(invalid("delivery checked candidate lineage"));
        }
        let _phase=common.memory().reserve(16384,MemoryClass::Ordinary)?;
        let mut reference=self.references.clone();
        while let Some(current)=reference {
            common.consume_steps(1)?;
            if current.position>=window.start {
                let row=source::point_window(window,current.position,read,work,common)?;
                if row.location.original!=current.original||row.encoded_bytes!=current.encoded||row.location.height!=fact.location().height||row.location.ordinal!=current.ordinal as u64{return Err(invalid("delivery checked counted locator join"));}
            }
            reference=current.previous.clone();
        }
        let mut data=self.data.clone();source::trim(&mut data,window.start,common)?;
        let fee=common.memory().reserve(size_of::<ProductValidationDeliveryIndexV1>()+128,MemoryClass::Ordinary)?;
        common.consume_steps(0)?;
        Ok(fee.retain(ProductValidationDeliveryIndexV1{data,head:head.verified_record_id(),body:fact.body_id(),roots:terminal.after_roots(),parent_original:fact.location().original}))
    }
}
impl ProductValidationDeliveryIndexV1 {
    pub(in crate::world) fn share_normal_ordinary_v1(&self,common:&mut FormalBoundedContextV1<'_>)->Result<ReservedValue<Self>,DeliveryDataErrorV1>{
        common.consume_steps(1)?;
        let fee=common.memory().reserve(size_of::<Self>()+128,MemoryClass::Ordinary)?;
        Ok(fee.retain(Self{data:self.data.clone(),head:self.head,body:self.body,roots:self.roots,parent_original:self.parent_original}))
    }
}

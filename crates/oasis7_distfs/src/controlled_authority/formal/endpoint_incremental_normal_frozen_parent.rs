//! Borrow the genuine accepted parent while the dual Frozen owner retains both
//! original endpoint/FD holders. This does not recreate a live Host slot.
use super::super::super::parent_view::{
    HostAcceptedParentReadViewV1, HostParentSourceRefV1, with_parent_source_bounded,
};
use super::*;
impl PaidNormalFrozenPairV1 {
    fn validate_parent_scope(
        &self,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut impl FnMut() -> bool,
    ) -> Result<(), FormalError> {
        let mut ctx = work.bounded_context(memory, stop);
        ctx.consume_steps(1)?;
        if !self.uses_budget(memory)
            || self.owned.value.primary.owned.value.source.identity != ctx.work().identity()
            || self.owned.value.replica.owned.value.source.identity != ctx.work().identity()
            || self.owned.value.primary.owned.value.source.lease.is_none()
            || self.owned.value.replica.owned.value.source.lease.is_none()
        {
            return Err(invalid(
                "Frozen parent scope requires actual dual live holders",
            ));
        }
        Ok(())
    }
    pub fn with_primary_parent_read_view_bounded<T, E>(
        &self,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut impl FnMut() -> bool,
        consume: impl FnOnce(HostAcceptedParentReadViewV1<'_>) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<FormalError>,
    {
        self.validate_parent_scope(memory, work, stop)?;
        let source = &self.owned.value.primary.owned.value.source;
        let lease = source
            .lease
            .as_ref()
            .ok_or_else(|| invalid("Frozen primary lease missing"))?;
        with_parent_source_bounded(
            HostParentSourceRefV1 {
                endpoint: &source.endpoint,
                lease,
                fence: source.fence,
                record: source.record,
                page_namespace: source.page_namespace,
                native_namespace: source.native_namespace,
            },
            memory,
            work,
            stop,
            consume,
        )
    }
    pub fn with_replica_parent_read_view_bounded<T, E>(
        &self,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut impl FnMut() -> bool,
        consume: impl FnOnce(HostAcceptedParentReadViewV1<'_>) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<FormalError>,
    {
        self.validate_parent_scope(memory, work, stop)?;
        let source = &self.owned.value.replica.owned.value.source;
        let lease = source
            .lease
            .as_ref()
            .ok_or_else(|| invalid("Frozen replica lease missing"))?;
        with_parent_source_bounded(
            HostParentSourceRefV1 {
                endpoint: &source.endpoint,
                lease,
                fence: source.fence,
                record: source.record,
                page_namespace: source.page_namespace,
                native_namespace: source.native_namespace,
            },
            memory,
            work,
            stop,
            consume,
        )
    }
    pub fn with_primary_native_worker_bounded<T, E>(
        &self,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut impl FnMut() -> bool,
        consume: impl FnOnce(super::super::super::closure::NativeWorkerLeaseV1<'_>) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<FormalError>,
    {
        self.validate_parent_scope(memory, work, stop)?;
        let source = &self.owned.value.primary.owned.value.source;
        let parent = source
            .endpoint
            .parent
            .as_ref()
            .ok_or_else(|| invalid("Frozen actual parent missing"))?;
        source
            .endpoint
            .with_iws_native_worker_v1(parent, memory, stop, consume)
    }
}

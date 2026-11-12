//! Private consumed source for the complete two-Hot Frozen transaction.
//! An error retains recovery ownership, never returns an apparently live GEN.
use super::super::store::PreparedPendingRequestUpdate;
use super::*;

pub(super) struct NormalFrozenTransactionV1 {
    pub(super) endpoint: Box<FileFormalEndpoint>,
    pub(super) endpoint_fee: ReservedValue<()>,
    pub(super) lease: Option<PaidRemoteLeaseV1>,
    pub(super) identity:
        oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
    pub(super) fence: ExecutionFence,
    pub(super) record: [u8; 32],
    pub(super) environment: [u8; 32],
    pub(super) page_namespace: (u64, u64),
    pub(super) native_namespace: (u64, u64),
    pub(super) _outer: ReservedValue<()>,
}
pub(super) struct NormalFrozenRecoveryV1 {
    pub(super) source: NormalFrozenTransactionV1,
    pub(super) cause: BoundedAcquireErrorV1,
}
impl PaidAcceptedGenesisHostSourceV1 {
    pub(super) fn consume_normal_frozen_source(
        self,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut impl FnMut() -> bool,
    ) -> Result<NormalFrozenTransactionV1, BoundedAcquireErrorV1> {
        {
            let mut ctx = work.bounded_context(memory, stop);
            self.validate_context(&mut ctx)?;
        }
        let fee = memory
            .reserve(
                size_of::<NormalFrozenTransactionV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid("normal Frozen source admission"))?;
        work.consume_steps(1, stop)?;
        let Self {
            endpoint,
            endpoint_fee,
            lease,
            identity,
            fence,
            record,
            environment,
            page_namespace,
            native_namespace,
        } = self;
        Ok(NormalFrozenTransactionV1 {
            endpoint,
            endpoint_fee,
            lease: Some(lease),
            identity,
            fence,
            record,
            environment,
            page_namespace,
            native_namespace,
            _outer: fee.retain(()),
        })
    }
}
impl NormalFrozenTransactionV1 {
    pub(super) fn publish(
        mut self,
        update: PreparedPendingRequestUpdate,
        provider: &RemoteMonotonicEndpointProvider,
        policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<Self, NormalFrozenRecoveryV1> {
        let result = (|| {
            work.consume_steps(1, stop)?;
            if !self.endpoint_fee.uses_budget(memory)
                || !self._outer.uses_budget(memory)
                || self.identity != work.bounded_context(memory, stop).work().identity()
            {
                return Err(invalid("normal Frozen source pool/Core").into());
            }
            let lease = self
                .lease
                .take()
                .ok_or_else(|| invalid("normal Frozen source requires recovery"))?;
            let endpoint = &mut *self.endpoint;
            let instance = match endpoint.role {
                EndpointRole::Primary => &endpoint.authority.definition().primary.instance,
                EndpointRole::Replica => &endpoint.authority.definition().replica.instance,
            };
            let lease = endpoint.store.publish_pending_update_with_lease_bounded(
                update, lease, provider, instance, policy, memory, work, stop,
            )?;
            self.lease = Some(lease);
            Ok(())
        })();
        match result {
            Ok(()) => Ok(self),
            Err(cause) => {
                // Pre-admission errors must close an FD too. Unknown post-Begin
                // outcomes can only continue through actual disk reconciliation.
                drop(self.lease.take());
                Err(NormalFrozenRecoveryV1 {
                    source: self,
                    cause,
                })
            }
        }
    }
}

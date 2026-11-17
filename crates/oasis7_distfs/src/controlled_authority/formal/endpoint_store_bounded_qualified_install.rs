//! One real native atomic+provider Finish kernel for typed GEN/normal sources.
use super::super::FormalArchiveReadWorkV1;
use super::*;
use crate::controlled_authority::fencing::{
    BoundedAcquireErrorV1, EndpointCheckpointBindingV1, EndpointInstanceBindingV1,
    RemoteMonotonicEndpointProvider, TrustedFencingPolicyV1, acquire_provider_lease_bounded_v1,
    begin_signed_native_checkpoint_bounded_v1, sign_native_checkpoint_bounded_v1,
};
use oasis7_proto::memory_budget::{MemoryBudget, ReservedValue};
pub(super) struct InstalledQualifiedDraft<T> {
    pub value: T,
    pub lease: crate::controlled_authority::fencing::PaidRemoteLeaseV1,
    pub body: HotBody,
    pub body_fee: ReservedValue<()>,
    pub checkpoint: EndpointCheckpointBindingV1,
    pub checkpoint_fee: ReservedValue<()>,
}
#[allow(clippy::too_many_arguments)]
pub(super) fn install_signed_qualified_draft_bounded<'a, T>(
    store: &'a Store,
    draft: super::bounded_native_draft::PaidNativeCheckpointDraftV1<'a>,
    provider: &RemoteMonotonicEndpointProvider,
    instance: &EndpointInstanceBindingV1,
    policy: &TrustedFencingPolicyV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
    consume: impl FnOnce(
        super::bounded_native_draft::PaidNativeCheckpointDraftV1<'a>,
    ) -> Result<T, FormalError>,
) -> Result<InstalledQualifiedDraft<T>, BoundedAcquireErrorV1> {
    let lease = {
        let mut ctx = work.bounded_context(memory, stop);
        acquire_provider_lease_bounded_v1(provider, instance, &store.checkpoint, policy, &mut ctx)?
    };
    install_signed_qualified_draft_with_lease_bounded(
        store, draft, lease, provider, instance, policy, memory, work, stop, consume,
    )
}
#[allow(clippy::too_many_arguments)]
pub(super) fn install_signed_qualified_draft_with_lease_bounded<'a, T>(
    store: &'a Store,
    draft: super::bounded_native_draft::PaidNativeCheckpointDraftV1<'a>,
    lease: crate::controlled_authority::fencing::PaidRemoteLeaseV1,
    provider: &RemoteMonotonicEndpointProvider,
    instance: &EndpointInstanceBindingV1,
    policy: &TrustedFencingPolicyV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
    consume: impl FnOnce(
        super::bounded_native_draft::PaidNativeCheckpointDraftV1<'a>,
    ) -> Result<T, FormalError>,
) -> Result<InstalledQualifiedDraft<T>, BoundedAcquireErrorV1> {
    let (begun, body, body_fee) = {
        let mut ctx = work.bounded_context(memory, stop);
        lease.validate_current_binding(instance, &store.checkpoint, policy, &mut ctx)?;
        let signed = sign_native_checkpoint_bounded_v1(provider, lease, draft, &mut ctx)?;
        let checkpoint =
            super::bounded_signed_checkpoint::prepare_signed_native_checkpoint_bounded_v1(
                signed, &mut ctx,
            )?;
        let begun = begin_signed_native_checkpoint_bounded_v1(provider, checkpoint, &mut ctx)?;
        let (body, body_fee) = begun.draft().paid_cache_copy(&mut ctx)?;
        (begun, body, body_fee)
    };
    let (binding, binding_fee) =
        super::bounded_transaction::copy_checkpoint(begun.next_checkpoint(), memory, work, stop)?;
    #[cfg(test)]
    if store.fault == Some(1) {
        return Err(FormalError::Io("injected after provider intent".into()).into());
    }
    store.atomic_hot_bounded(begun.native_bytes(), memory, work, stop)?;
    #[cfg(test)]
    if store.fault == Some(2) {
        return Err(FormalError::Io("injected after native checkpoint".into()).into());
    }
    let mut ctx = work.bounded_context(memory, stop);
    let finished = begun.finish_bounded(provider, &mut ctx)?;
    let (value, lease) = finished.consume_draft_and_lease(|draft, lease| (consume(draft), lease));
    let value = value?;
    Ok(InstalledQualifiedDraft {
        value,
        lease,
        body,
        body_fee,
        checkpoint: binding,
        checkpoint_fee: binding_fee,
    })
}

//! One paid actual Store/monotonic-checkpoint read, retaining the native
//! decoder phase through genuine head/predecessor restoration and Host use.
use super::*;
use crate::controlled_authority::fencing::BoundedAcquireErrorV1;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1;
use oasis7_formal_protocol::controlled_authority::formal::{
    bounded_context::WorkIdentityV1,
    proof::{PaidNativeCheckpointErrorV1, PaidNativeCheckpointVerdictV1},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};

#[derive(Debug)]
pub(super) enum ColdCheckpointErrorV1 {
    Formal(FormalError),
    Acquire(BoundedAcquireErrorV1),
    Native(PaidNativeCheckpointErrorV1),
}
impl From<FormalError> for ColdCheckpointErrorV1 {
    fn from(e: FormalError) -> Self {
        Self::Formal(e)
    }
}
impl From<BoundedAcquireErrorV1> for ColdCheckpointErrorV1 {
    fn from(e: BoundedAcquireErrorV1) -> Self {
        Self::Acquire(e)
    }
}
pub(super) struct AuthenticatedColdCheckpointV1 {
    head: VerifiedFormalCommitV1,
    predecessor: Option<VerifiedFormalCommitV1>,
    native: PaidNativeCheckpointVerdictV1,
    store: super::store::cold_parent::PaidColdStoreHeadV1,
    _authority_id: ReservedValue<String>,
    _restore_wire: ReservedValue<Vec<u8>>,
    outer: ReservedValue<()>,
    identity: WorkIdentityV1,
}
impl AuthenticatedColdCheckpointV1 {
    pub(super) fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.native.uses_budget(memory)
            && self.outer.uses_budget(memory)
            && self.store.uses_budget(memory)
            && self._authority_id.uses_budget(memory)
            && self._restore_wire.uses_budget(memory)
    }
    pub(super) fn head(&self) -> &VerifiedFormalCommitV1 {
        &self.head
    }
    pub(super) fn predecessor(&self) -> Option<&VerifiedFormalCommitV1> {
        self.predecessor.as_ref()
    }
    pub(super) fn native(&self) -> &NativeCheckpointVerdictV1 {
        self.native.value()
    }
    pub(super) fn state(&self) -> &FormalRequestStateV1 {
        self.store.state()
    }
    pub(super) fn validate_material_binding_v1(
        &self,
        memory: &MemoryBudget,
        core: &oasis7_formal_protocol::incremental_material::CheckedCommitCore,
    ) -> Result<(), FormalError> {
        if !self.native.uses_budget(memory)
            || !self.outer.uses_budget(memory)
            || !self.store.uses_budget(memory)
            || self.head.verified_record_id() != core.candidate_record_id()
            || self.native.value().proof().decision.record != *self.head.record()
            || self.native.value().head().position != self.head.record().position
            || self.native.value().authority_id() != self.head.finalizing_authority_id()
        {
            return Err(invalid("cold execution material authenticated source"));
        }
        Ok(())
    }
    pub(super) fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        self.native.validate_context(common)?;
        if self.identity != common.work().identity()
            || !self.outer.uses_budget(common.memory())
            || !self.store.uses_budget(common.memory())
        {
            return Err(invalid("cold checkpoint foreign context"));
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn recover_authenticated_checkpoint_bounded_v1(
    endpoint: &FileFormalEndpoint,
    lease: &crate::controlled_authority::fencing::PaidRemoteLeaseV1,
    pinned: &fencing::TrustedFencingPolicyV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<AuthenticatedColdCheckpointV1, ColdCheckpointErrorV1> {
    let mut common = work.bounded_context(memory, stop);
    let instance = match endpoint.role {
        EndpointRole::Primary => &endpoint.authority.definition().primary.instance,
        EndpointRole::Replica => &endpoint.authority.definition().replica.instance,
    };
    lease.validate_current_binding(instance, &endpoint.store.checkpoint, pinned, &mut common)?;
    let signed = lease
        .native_checkpoint_bytes()
        .ok_or_else(|| invalid("cold checkpoint requires actual protected native bytes"))?;
    let authority_id = oasis7_formal_protocol::controlled_authority::formal::normal_frozen_bounded::normal_frozen_hash_bounded(
        oasis7_formal_protocol::controlled_authority::formal::record::AUTHORITY_ID_DOMAIN_V1,
        endpoint.authority.definition(), &mut common)?;
    let native = NativeCheckpointVerdictV1::verify_bounded(
        signed,
        &endpoint.authority,
        &instance.endpoint_public_key,
        &endpoint.store.checkpoint,
        lease
            .evidence()
            .ok_or_else(|| invalid("cold checkpoint trusted provider evidence required"))?,
        &endpoint.store.body.head,
        &endpoint.store.body.head,
        &authority_id,
        |id, common| {
            let (memory, _, stop) = common.parts();
            let raw = endpoint
                .store
                .raw_object_bounded(id, 64 << 20, memory, work, stop)?
                .ok_or_else(|| invalid("native checkpoint object missing"))?;
            raw.into_native_bytes_v1(common)
        },
        &mut common,
    )
    .map_err(ColdCheckpointErrorV1::Native)?;
    // The actual selected Store state is read by the original complete decoder;
    // its candidate payload cannot be replaced by an archive locator or Bool.
    let store = {
        let (memory, _, stop) = common.parts();
        endpoint
            .store
            .read_cold_qualified_head_bounded_v1(memory, work, stop)?
    };
    let FormalRequestStateV1::Qualified {
        candidate, proof, ..
    } = store.state()
    else {
        return Err(invalid("formal checkpoint qualified entry").into());
    };
    let restore_wire =
        oasis7_formal_protocol::controlled_authority::formal::context_wire::encode_paid(
            &(proof, &candidate.candidate.record),
            64 << 20,
            &mut common,
            "formal encoding exceeds byte bound",
            "cold native restore",
        )?;
    let actual =
        u64::try_from(restore_wire.len()).map_err(|_| invalid("cold restore visit layout"))?;
    common.charge(
        actual
            .checked_mul(16)
            .ok_or_else(|| invalid("cold restore visits"))?,
        actual,
    )?;
    if !oasis7_formal_protocol::controlled_authority::formal::incremental::is_iws_record(
        proof.decision.record.schema_version,
    ) || proof.decision.record != candidate.candidate.record
        || native.value().proof() != proof
    {
        return Err(invalid("native checked checkpoint context differs from held storage").into());
    }
    // A schema7/8 IWS record deliberately cannot enter legacy Snapshot material
    // validation. The same original package codec is bounded here; actual typed
    // record/Core/Current/Result qualification follows before a Host source.
    candidate
        .material
        .validate_iws_bounded(&mut common)
        .map_err(BoundedAcquireErrorV1::from)?;
    if let Some(parent) = store.parent_state() {
        let FormalRequestStateV1::Qualified { proof, .. } = parent else {
            return Err(invalid("formal checkpoint ancestor qualified entry").into());
        };
        if native.value().verified_parent_record() != Some(&proof.decision.record)
            || native.value().verified_parent_finalizing_authority_id()
                != Some(proof.decision.primary.body.finalizing_authority_id.as_str())
        {
            return Err(
                invalid("native checked checkpoint parent differs from held storage").into(),
            );
        }
    }
    let original = if proof.decision.branch == FormalCompletionBranchV1::Normal {
        &endpoint.authority
    } else {
        &endpoint.original
    };
    let recovery = (proof.decision.branch != FormalCompletionBranchV1::Normal)
        .then_some(endpoint.recovery.as_ref())
        .flatten();
    let (head, predecessor) = restore_formal_commit_and_parent_from_checkpoint(
        proof,
        original,
        recovery,
        native.value(),
    )?;
    let fee = memory
        .reserve(
            std::mem::size_of::<AuthenticatedColdCheckpointV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("cold checkpoint owner admission"))?;
    Ok(AuthenticatedColdCheckpointV1 {
        head,
        predecessor,
        native,
        store,
        _authority_id: authority_id,
        _restore_wire: restore_wire,
        outer: fee.retain(()),
        identity: common.work().identity(),
    })
}

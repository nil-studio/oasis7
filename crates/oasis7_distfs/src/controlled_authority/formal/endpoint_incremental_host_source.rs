//! Fresh accepted GEN or normal ownership, not a historical proof or WorldReady grant.
use super::genesis_receipts::ProviderPolicyRefs;
use super::*;
use crate::controlled_authority::fencing::{
    BoundedAcquireErrorV1, PaidRemoteLeaseV1, ProtectedSignerCustodyV1,
    RemoteMonotonicEndpointProvider, TrustedFencingPolicyV1, acquire_provider_lease_bounded_v1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::{context_wire, incremental::authority_binding_bounded},
    incremental_execution::ExecutionFence,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::mem::size_of;

/// The original endpoint, locks, accepted carriers and both namespace leases
/// move together. No constructor, Clone, mutable endpoint or detached parts.
pub struct PaidAcceptedGenesisHostSourceV1 {
    endpoint: Box<FileFormalEndpoint>,
    endpoint_fee: ReservedValue<()>,
    identity: oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
    lease: PaidRemoteLeaseV1,
    fence: ExecutionFence,
    record: [u8; 32],
    environment: [u8; 32],
    page_namespace: (u64, u64),
    native_namespace: (u64, u64),
    predecessor: Option<[u8; 32]>,
    predecessor_fence: Option<ExecutionFence>,
}
impl PaidAcceptedGenesisHostSourceV1 {
    pub fn endpoint_role(&self) -> EndpointRole {
        self.endpoint.role
    }
    pub fn validate_context(
        &self,
        ctx:&mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        ctx.consume_steps(1)?;
        if !self.uses_budget(ctx.memory()) || self.identity != ctx.work().identity() {
            return Err(invalid("Host accepted source pool/Core"));
        }
        Ok(())
    }
    pub fn predecessor_record_id(&self) -> Option<[u8; 32]> {
        self.predecessor
    }
    pub fn predecessor_fence(&self) -> Option<ExecutionFence> {
        self.predecessor_fence
    }
    pub fn fence(&self) -> ExecutionFence {
        self.fence
    }
    pub fn record_id(&self) -> [u8; 32] {
        self.record
    }
    pub fn execution_environment(&self) -> [u8; 32] {
        self.environment
    }
    pub fn page_namespace_identity(&self) -> (u64, u64) {
        self.page_namespace
    }
    pub fn native_namespace_identity(&self) -> (u64, u64) {
        self.native_namespace
    }
    pub fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.endpoint_fee.uses_budget(memory) && self.lease.uses_budget(memory)
    }
    /// The extra owner/Box fee. Original resident source fees remain owned by
    /// the moved endpoint and are not counted as newly allocated copies.
    pub fn reserved_bytes(&self) -> usize {
        self.endpoint_fee.reserved_bytes() + self.lease.reserved_bytes()
    }
    pub fn commit(&self) -> Result<&VerifiedFormalCommitV1, FormalError> {
        self.endpoint
            .parent
            .as_ref()
            .ok_or_else(|| invalid("Host accepted source parent"))
    }
    pub fn authority(&self) -> &VerifiedAuthorityV1 {
        &self.endpoint.authority
    }
    pub fn with_native_worker<T, E>(
        &self,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut impl FnMut() -> bool,
        consume: impl FnOnce(closure::NativeWorkerLeaseV1<'_>) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<FormalError>,
    {
        {
            let mut ctx = work.bounded_context(memory, stop);
            self.validate_context(&mut ctx)?;
        }
        self.endpoint
            .with_iws_native_worker_v1(self.commit()?, memory, stop, consume)
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum AcceptedHostKind {
    Genesis,
    Normal,
    AuthenticatedCold,
}
enum LiveQuorum<'a> {
    AuthenticatedCold(&'a super::cold_install::AuthenticatedColdCandidateV1),
    Genesis(&'a oasis7_formal_protocol::controlled_authority::formal::PaidVerifiedGenesisCommitV1),
    Normal(&'a oasis7_formal_protocol::controlled_authority::formal::PaidVerifiedNormalCommitV1),
}
impl LiveQuorum<'_> {
    fn verified(&self) -> &VerifiedFormalCommitV1 {
        match self {
            Self::AuthenticatedCold(q) => q.checkpoint().head(),
            Self::Genesis(q) => q.verified(),
            Self::Normal(q) => q.verified(),
        }
    }
    fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        match self {
            Self::AuthenticatedCold(q) => q.checkpoint().uses_budget(memory),
            Self::Genesis(q) => q.uses_budget(memory),
            Self::Normal(q) => q.uses_budget(memory),
        }
    }
    fn validate_context(
        &self,
        ctx:&mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        match self {
            Self::AuthenticatedCold(q) => q.checkpoint().validate_context(ctx),
            Self::Genesis(q) => q.validate_context(ctx),
            Self::Normal(q) => q.validate_context(ctx),
        }
    }
}
impl FileFormalEndpoint {
    pub(super) fn into_authenticated_cold_host_source_with_lease_bounded(
        self,
        lease: PaidRemoteLeaseV1,
        provider: &RemoteMonotonicEndpointProvider,
        pinned: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<PaidAcceptedGenesisHostSourceV1, BoundedAcquireErrorV1> {
        self.into_accepted_host_source_bounded(
            ProviderPolicyRefs {
                provider,
                policy: pinned,
            },
            memory,
            work,
            stop,
            AcceptedHostKind::AuthenticatedCold,
            Some(lease),
        )
    }
    pub fn into_accepted_genesis_host_source_bounded(
        self,
        provider: &RemoteMonotonicEndpointProvider,
        pinned: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<PaidAcceptedGenesisHostSourceV1, BoundedAcquireErrorV1> {
        self.into_accepted_host_source_bounded(
            ProviderPolicyRefs {
                provider,
                policy: pinned,
            },
            memory,
            work,
            stop,
            AcceptedHostKind::Genesis,
            None,
        )
    }
    /// Same complete source/namespace/lease gate, with an actual retained normal
    /// quorum. No GEN, cold, staged or caller-metadata owner can select this lane.
    pub fn into_accepted_normal_host_source_bounded(
        self,
        provider: &RemoteMonotonicEndpointProvider,
        pinned: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<PaidAcceptedGenesisHostSourceV1, BoundedAcquireErrorV1> {
        self.into_accepted_host_source_bounded(
            ProviderPolicyRefs {
                provider,
                policy: pinned,
            },
            memory,
            work,
            stop,
            AcceptedHostKind::Normal,
            None,
        )
    }
    pub(in crate::controlled_authority::formal::endpoint) fn into_accepted_normal_host_source_with_lease_bounded(
        self,
        lease: PaidRemoteLeaseV1,
        provider: &RemoteMonotonicEndpointProvider,
        pinned: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<PaidAcceptedGenesisHostSourceV1, BoundedAcquireErrorV1> {
        self.into_accepted_host_source_bounded(
            ProviderPolicyRefs {
                provider,
                policy: pinned,
            },
            memory,
            work,
            stop,
            AcceptedHostKind::Normal,
            Some(lease),
        )
    }
}
fn charge(
    work: &FormalArchiveReadWorkV1,
    n: usize,
    stop: &mut dyn FnMut() -> bool,
) -> Result<(), FormalError> {
    work.charge(
        1,
        u64::try_from(n).map_err(|_| invalid("Host source layout"))?,
        stop,
    )
}
impl FileFormalEndpoint {
    /// Only the live, current, complete GEN cache can cross this boundary.
    /// A retained Qualified receipt, cold metadata or a staged job cannot.
    fn into_accepted_host_source_bounded(
        self,
        fencing: ProviderPolicyRefs<'_, '_>,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
        kind: AcceptedHostKind,
        supplied_lease: Option<PaidRemoteLeaseV1>,
    ) -> Result<PaidAcceptedGenesisHostSourceV1, BoundedAcquireErrorV1> {
        let ProviderPolicyRefs {
            provider,
            policy: pinned,
        } = fencing;
        charge(work, 0, stop)?;
        if self.poisoned || (self.recovery.is_some() && kind != AcceptedHostKind::AuthenticatedCold)
        {
            return Err(invalid("Host fresh GEN endpoint").into());
        }
        let bytes = size_of::<FileFormalEndpoint>()
            .checked_add(size_of::<ReservedValue<Box<FileFormalEndpoint>>>())
            .and_then(|n| n.checked_add(size_of::<PaidAcceptedGenesisHostSourceV1>()))
            .ok_or_else(|| invalid("Host source layout"))?;
        let fee = memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(|_| invalid("Host source owner admission"))?;
        let commit = self
            .parent
            .as_ref()
            .ok_or_else(|| invalid("Host GEN accepted parent missing"))?;
        let capability = self
            .iws_page_store
            .as_ref()
            .ok_or_else(|| invalid("Host GEN page capability missing"))?;
        let slot = capability.jobs.parent()?;
        let cache = slot
            .as_ref()
            .ok_or_else(|| invalid("Host GEN accepted cache missing"))?;
        use oasis7_world_state::incremental_world_storage_source_cold::AuthenticatedWorldStorageParentSourceV1;
        let (core, storage): (_, &dyn AuthenticatedWorldStorageParentSourceV1) =
            match (&cache.current_owner, kind) {
                (
                    provisional::AcceptedCurrentOwner::Candidate(value),
                    AcceptedHostKind::Genesis | AcceptedHostKind::Normal,
                ) => (&value.core, &value.source),
                (
                    provisional::AcceptedCurrentOwner::AuthenticatedCold(value),
                    AcceptedHostKind::AuthenticatedCold,
                ) => {
                    let mut ctx = work.bounded_context(memory, stop);
                    value.validate_context(cache, &mut ctx)?;
                    (value.core(), value.source())
                }
                _ => {
                    return Err(
                        invalid("Host requires exact corresponding accepted source kind").into(),
                    );
                }
            };
        let quorum = match kind {
            AcceptedHostKind::AuthenticatedCold => LiveQuorum::AuthenticatedCold(
                cache
                    .authenticated_cold()
                    .ok_or_else(|| invalid("Host authenticated cold source missing"))?,
            ),
            AcceptedHostKind::Genesis => LiveQuorum::Genesis(
                cache
                    .genesis_quorum
                    .as_ref()
                    .ok_or_else(|| invalid("Host GEN durable quorum missing"))?,
            ),
            AcceptedHostKind::Normal => LiveQuorum::Normal(
                cache
                    .normal_quorum
                    .as_ref()
                    .ok_or_else(|| invalid("Host normal durable quorum missing"))?,
            ),
        };
        let (predecessor, predecessor_fence) = match kind {
            // Cold is an independently authenticated startup, not a claimed
            // successor installation in an existing Host slot.
            AcceptedHostKind::AuthenticatedCold => (None, None),
            AcceptedHostKind::Genesis => {
                if core.body().height != 0
                    || core.body().parent_commit.is_some()
                    || core.transition().is_some()
                    || core.parent_record_id().is_some()
                    || commit.record().position != 1
                    || commit.record().kind != FormalRecordKindV1::InitialActivation
                {
                    return Err(invalid("Host GEN exact initial source").into());
                }
                (None, None)
            }
            AcceptedHostKind::Normal => {
                if core.body().height==0||core.transition().is_none()||!oasis7_formal_protocol::controlled_authority::formal::incremental::is_iws_record(commit.record().schema_version)||commit.record().kind!=FormalRecordKindV1::Execution||commit.record().position<=1{return Err(invalid("Host normal exact Execution source").into());}
                let previous = core
                    .parent_record_id()
                    .ok_or_else(|| invalid("Host normal actual predecessor missing"))?;
                charge(work, commit.record().parent_record_id.len(), stop)?;
                let mut actual = [0; 32];
                hex::decode_to_slice(&commit.record().parent_record_id, &mut actual)
                    .map_err(|_| invalid("Host normal predecessor encoding"))?;
                if actual != previous {
                    return Err(invalid("Host normal quorum/Core predecessor mismatch").into());
                }
                let body = core
                    .parent_body()
                    .ok_or_else(|| invalid("Host normal original parent body missing"))?;
                let parent_commit = core
                    .body()
                    .parent_commit
                    .ok_or_else(|| invalid("Host normal parent body ID missing"))?;
                (
                    Some(previous),
                    Some(ExecutionFence {
                        parent_commit,
                        parent_roots: body.roots,
                        authority: body.authority,
                    }),
                )
            }
        };
        let data = cache
            .complete_data()
            .ok_or_else(|| invalid("Host GEN complete data missing"))?;
        let local = cache.local_closure.as_ref();
        if kind != AcceptedHostKind::AuthenticatedCold && local.is_none() {
            return Err(invalid("Host GEN local closure missing").into());
        }
        if kind == AcceptedHostKind::AuthenticatedCold
            && data
                .pages()
                .authenticated_cold_native_binding_v1()
                .is_none()
        {
            return Err(invalid("Host cold actual local namespace witness missing").into());
        }
        let record = commit.verified_record_id();
        // Every logical carrier was produced from the private original core.
        // These are immutable holders, not caller metadata claims.
        charge(work, 32 * 24 + 8 * 4, stop)?;
        if !core.uses_budget(memory)
            || !storage.uses_budget(memory)
            || !cache.current().uses_budget(memory)
            || !cache.roots.uses_budget(memory)
            || !cache.cognition.uses_budget(memory)
            || !cache.runtime.uses_budget(memory)
            || !capability.pages.memory_budget().same_pool(memory)
            || !capability.jobs.uses_budget(memory)
            || !data.uses_budget(memory)
            || !data.material().uses_budget(memory)
            || !data.pages().uses_budget(memory)
            || !quorum.uses_budget(memory)
            || cache.current().allocation_class() != MemoryClass::Ordinary
            || cache.runtime.allocation_class() != MemoryClass::Ordinary
            || record != cache.commit_record
            || record != core.candidate_record_id()
            || record != quorum.verified().verified_record_id()
            || record != data.candidate_record_id()
            || local.is_some_and(|local| record != local.candidate_record_id())
            || record != cache.runtime.occurrence().head_record_id()
            || record != cache.cognition.current().head_record_id()
            || data.body_id() != cache.runtime.occurrence().head_body_id()
            || data.body_id() != cache.cognition.selected().occurrence().head_body_id()
            || data.roots_id() != core.body().roots
            || data.roots_id() != cache.cognition.current().roots_id()
            || data.manifest_id() != core.execution_artifacts().manifest
            || record != data.pages().candidate_record_id()
            || data.body_id() != data.pages().body_id()
            || data.roots_id() != data.pages().roots_id()
            || data.page_namespace_identity() != data.material().page_namespace_identity()
            || data.page_namespace_identity() != data.pages().page_namespace_identity()
        {
            return Err(invalid("Host GEN complete accepted source binding").into());
        }
        // Compare the original fixed six-namespace type itself, charging all
        // scanned strings and fixed fields before its original PartialEq.
        let a = &*cache.roots;
        let b = core.roots();
        let mut root_bytes = a
            .world_id
            .len()
            .checked_add(b.world_id.len())
            .and_then(|n| {
                n.checked_add(
                    2 * (32
                        + 2 * 8
                        + 6 * (size_of::<
                            oasis7_formal_protocol::incremental_state::StateNamespaceV1,
                        >() + 8)),
                )
            })
            .ok_or_else(|| invalid("Host roots layout"))?;
        for (left, right) in a.state.iter().zip(&b.state) {
            root_bytes = root_bytes
                .checked_add(left.node_root.len())
                .and_then(|n| n.checked_add(right.node_root.len()))
                .ok_or_else(|| invalid("Host roots layout"))?;
        }
        charge(work, root_bytes, stop)?;
        if a != b {
            return Err(invalid("Host current roots binding").into());
        }
        let authority = {
            let mut ctx = work.bounded_context(memory, stop);
            quorum.validate_context(&mut ctx)?;
            authority_binding_bounded(&self.authority, &mut ctx)?
        };
        charge(work, 40, stop)?;
        if authority != core.body().authority {
            return Err(invalid("Host actual authority/body binding").into());
        }
        let local_reference = match kind {
            AcceptedHostKind::AuthenticatedCold => cache
                .authenticated_cold()
                .and_then(|p| p.checkpoint().native().local_data_closure_reference())
                .ok_or_else(|| invalid("Host authenticated cold local reference missing"))?,
            _ => local
                .ok_or_else(|| invalid("Host local closure missing"))?
                .reference_for(commit)?,
        };
        let instance = match self.role {
            EndpointRole::Primary => &self.authority.definition().primary.instance,
            EndpointRole::Replica => &self.authority.definition().replica.instance,
        };
        self.store.validate_host_current_bounded(
            &instance.endpoint_public_key,
            commit,
            local_reference,
            memory,
            work,
            stop,
        )?;
        let (lease, identity) = {
            let mut ctx = work.bounded_context(memory, stop);
            // Pinned caller configuration must equal the actual VerifiedAuthority,
            // not merely the provider's self-reported policy.
            let policy = self.authority.policy();
            {
                let mut compare = |a: &str, b: &str| -> Result<(), FormalError> {
                    ctx.charge(
                        1,
                        u64::try_from(
                            a.len()
                                .checked_add(b.len())
                                .ok_or_else(|| invalid("Host policy layout"))?,
                        )
                        .map_err(|_| invalid("Host policy layout"))?,
                    )?;
                    if a != b {
                        return Err(FormalError::FencingUnproven);
                    }
                    Ok(())
                };
                compare(
                    &policy.fencing_provider_public_key,
                    &pinned.provider_public_key,
                )?;
                if policy.authorized_fencing_resource_bindings.len()
                    != pinned.authorized_resource_bindings.len()
                {
                    return Err(FormalError::FencingUnproven.into());
                }
                for (a, b) in policy
                    .authorized_fencing_resource_bindings
                    .iter()
                    .zip(&pinned.authorized_resource_bindings)
                {
                    compare(a, b)?;
                }
            }
            let lease = match supplied_lease {
                Some(lease) => lease,
                None => acquire_provider_lease_bounded_v1(
                    provider,
                    instance,
                    &self.store.checkpoint,
                    pinned,
                    &mut ctx,
                )?,
            };
            lease.validate_current_binding(instance, &self.store.checkpoint, pinned, &mut ctx)?;
            let a = context_wire::encode_paid(
                lease.checkpoint(),
                usize::MAX,
                &mut ctx,
                "Host checkpoint wire limit",
                "Host checkpoint encoding",
            )?;
            let b = context_wire::encode_paid(
                &self.store.checkpoint,
                usize::MAX,
                &mut ctx,
                "Host checkpoint wire limit",
                "Host checkpoint encoding",
            )?;
            ctx.charge(1, a.len().max(b.len()) as u64)?;
            if a.as_slice() != b.as_slice() {
                return Err(FormalError::FencingUnproven.into());
            }
            ctx.charge(1, 32 * 4)?;
            if lease.pending().is_some()
                || lease.seal().is_some()
                || lease.protected_custody()
                    != Some(&ProtectedSignerCustodyV1::NonexportableLeaseBoundV1)
                || lease
                    .durable_set_state()
                    .is_some_and(|v| v.activation_digest.is_none())
            {
                return Err(FormalError::FencingUnproven.into());
            }
            let native = lease
                .native_checkpoint_bytes()
                .ok_or(FormalError::FencingUnproven)?;
            let mut hash = blake3::Hasher::new();
            for chunk in native.chunks(8192) {
                ctx.charge(1, chunk.len() as u64)?;
                hash.update(chunk);
            }
            let mut digest = [0; 64];
            hex::encode_to_slice(hash.finalize().as_bytes(), &mut digest)
                .map_err(|_| invalid("Host native digest"))?;
            ctx.charge(1, 64)?;
            if lease.checkpoint().commitment.as_bytes() != digest {
                return Err(FormalError::FencingUnproven.into());
            }
            let identity = ctx.work().identity();
            (lease, identity)
        };
        let environment = core.body().execution_environment;
        let fence = ExecutionFence {
            parent_commit: data.body_id(),
            parent_roots: data.roots_id(),
            authority,
        };
        let page_namespace = data.page_namespace_identity();
        let native_namespace = data.native_namespace_identity();
        charge(work, 0, stop)?;
        drop(slot);
        Ok(PaidAcceptedGenesisHostSourceV1 {
            endpoint: Box::new(self),
            endpoint_fee: fee.retain(()),
            identity,
            lease,
            fence,
            record,
            environment,
            page_namespace,
            native_namespace,
            predecessor,
            predecessor_fence,
        })
    }
}

#[cfg(test)]
#[path = "endpoint_incremental_host_source_tests.rs"]
mod tests;

#[path = "endpoint_incremental_host_parent.rs"]
mod parent_view;
pub use parent_view::{HostAcceptedParentReadViewV1, PaidHostQualifiedParentProofV1};

#[path = "endpoint_incremental_normal_frozen.rs"]
mod normal_frozen;
pub(in crate::controlled_authority) use normal_frozen::PaidNormalFrozenReceiptSourceV1;
pub use normal_frozen::{
    NormalFrozenFailureV1, NormalFrozenPairFailureV1, NormalReadyCandidateReadViewV1,
    NormalReadyPairFailureV1, NormalReadyPublicationFailureV1, PaidNormalAcceptedPairV1,
    PaidNormalFrozenEndpointV1, PaidNormalFrozenPairV1, PaidNormalReadyPairV1,
    join_normal_frozen_endpoints_bounded,
};

#[cfg(not(target_arch = "wasm32"))]
#[path = "endpoint_incremental_host_cold.rs"]
mod cold;

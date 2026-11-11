//! GEN-only typed admission and final installation. Ordinary guards remain closed.
//! Complete staged bytes alone cannot construct an install permit: the SAME
//! dual-quorum proof and actual durable local Ready/ProofComplete are required.
#[path = "endpoint_incremental_genesis_stage.rs"]
mod stage;
use super::super::FormalArchiveReadWorkV1;
use super::*;
use crate::controlled_authority::fencing::{
    BoundedAcquireErrorV1, RemoteMonotonicEndpointProvider, TrustedFencingPolicyV1,
};
use oasis7_formal_protocol::controlled_authority::formal::{
    PaidVerifiedGenesisCommitV1, verify_genesis_qualified_bounded,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
pub(in crate::controlled_authority::formal::endpoint) struct GenesisValidatedInputs {
    quorum: PaidVerifiedGenesisCommitV1,
    qualified: store::PaidRequestStateV1,
}
impl GenesisValidatedInputs {
    pub(in crate::controlled_authority::formal::endpoint) fn verified(
        &self,
    ) -> &VerifiedFormalCommitV1 {
        self.quorum.verified()
    }
    pub(in crate::controlled_authority::formal::endpoint) fn state(
        &self,
    ) -> &store::PaidRequestStateV1 {
        &self.qualified
    }
    pub(in crate::controlled_authority::formal::endpoint) fn uses_budget(
        &self,
        memory: &MemoryBudget,
    ) -> bool {
        self.quorum.uses_budget(memory) && self.qualified.uses_budget(memory)
    }
    pub(in crate::controlled_authority::formal::endpoint) fn reserved_bytes(&self) -> usize {
        self.quorum.reserved_bytes() + self.qualified.reserved_bytes()
    }
    pub(in crate::controlled_authority::formal::endpoint) fn validate_context(
        &self,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<(), FormalError> {
        let mut ctx = work.bounded_context(memory, stop);
        self.quorum.validate_context(&mut ctx)?;
        if !self.qualified.uses_budget(memory) {
            return Err(invalid("GEN Qualified foreign pool"));
        }
        Ok(())
    }
    pub(in crate::controlled_authority::formal::endpoint) fn into_quorum(
        self,
    ) -> PaidVerifiedGenesisCommitV1 {
        self.quorum
    }
}
pub(in crate::controlled_authority::formal::endpoint) struct GenesisInstallPermit<'a> {
    job: jobs::GenesisJobLease<'a>,
    inputs: GenesisValidatedInputs,
}
impl<'a> GenesisInstallPermit<'a> {
    pub(in crate::controlled_authority::formal::endpoint) fn split(
        self,
    ) -> (jobs::GenesisJobLease<'a>, GenesisValidatedInputs) {
        (self.job, self.inputs)
    }
}
fn same<T: serde::Serialize + PartialEq>(
    a: &T,
    b: &T,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<bool, FormalError> {
    store::bounded_transaction::same(a, b, memory, work, stop)
}
fn qualified_state(
    candidate: &FormalCandidateMaterialV1,
    quorum: &PaidVerifiedGenesisCommitV1,
    closure: &str,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<store::PaidRequestStateV1, FormalError> {
    let mut ctx = work.bounded_context(memory, stop);
    let c = store::context_wire::encode_paid(
        candidate,
        64 * 1024 * 1024,
        &mut ctx,
        "formal object byte limit",
        "GEN candidate sizing",
    )?;
    let p = store::context_wire::encode_paid(
        quorum.proof(),
        64 * 1024 * 1024,
        &mut ctx,
        "formal object byte limit",
        "GEN proof sizing",
    )?;
    let n = c
        .len()
        .checked_add(p.len())
        .and_then(|n| n.checked_add(closure.len()))
        .and_then(|n| n.checked_add(128))
        .ok_or_else(|| invalid("GEN Qualified clone layout"))?;
    // Reuse the proved COMPLETE RequestState heap bound. The original clone
    // constructs a strict subset of that same reachable DTO graph: no parser
    // scratch or diagnostic buffers are needed, so this deliberately overpays.
    let fee = memory
        .reserve(
            store::bounded_request::decode_workspace(n)?,
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("GEN Qualified clone admission"))?;
    ctx.charge(1, n as u64)?;
    let state = FormalRequestStateV1::Qualified {
        candidate: candidate.clone(),
        proof: quorum.proof().clone(),
        data_closure: Some(closure.to_owned()),
    };
    let state = fee.retain(state);
    let wire = store::context_wire::encode_paid(
        &*state,
        64 * 1024 * 1024,
        &mut ctx,
        "formal object byte limit",
        "GEN Qualified encoding",
    )?;
    drop(ctx);
    store::bounded_request::decode_request_bytes_bounded_v1(&wire, memory, work, stop)
}
pub struct PaidGenesisAcceptanceV1 {
    head: ReservedValue<FormalHeadRefV1>,
}
impl PaidGenesisAcceptanceV1 {
    pub fn head(&self) -> &FormalHeadRefV1 {
        &self.head
    }
    pub fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.head.uses_budget(memory)
    }
    pub fn reserved_bytes(&self) -> usize {
        self.head.reserved_bytes()
    }
}
fn paid_parent_copy(
    quorum: &PaidVerifiedGenesisCommitV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<(VerifiedFormalCommitV1, ReservedValue<()>), FormalError> {
    let mut ctx = work.bounded_context(memory, stop);
    let wire = store::context_wire::encode_paid(
        quorum.verified().record(),
        64 * 1024 * 1024,
        &mut ctx,
        "formal encoding exceeds byte bound",
        "GEN parent sizing",
    )?;
    let n = wire
        .len()
        .checked_add(quorum.verified().finalizing_authority_id().len())
        .and_then(|n| n.checked_add(std::mem::size_of::<VerifiedFormalCommitV1>()))
        .and_then(|n| {
            n.checked_add(
                store::bounded_native_draft::role_map_nodes(
                    quorum.verified().record().artifact_roots.len(),
                )
                .ok()?,
            )
        })
        .ok_or_else(|| invalid("GEN formal parent layout"))?;
    let fee = memory
        .reserve(n, MemoryClass::Ordinary)
        .map_err(|_| invalid("GEN formal parent admission"))?;
    ctx.charge(1, n as u64)?;
    Ok((quorum.verified().clone(), fee.retain(())))
}
impl FileFormalEndpoint {
    /// Final GEN lane. Receipt preparation is a separate, mandatory producer:
    /// absent durable local Ready or real dual ProofComplete this fails closed.
    pub fn publish_genesis_qualified_bounded(
        &mut self,
        candidate: &FormalCandidateMaterialV1,
        proof: &FormalQualifiedCommitProofV1,
        context: &VerifiedCandidateContextV1,
        provider: &RemoteMonotonicEndpointProvider,
        policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<PaidGenesisAcceptanceV1, BoundedAcquireErrorV1> {
        work.consume_steps(1, stop)?;
        // Durable exact retries precede fresh-head and staged-source admission.
        // They return the original public proof result without signing, native
        // writes, cache replacement or a new source/hash counter allocation.
        if self.authority.definition().profile_version==oasis7_formal_protocol::controlled_authority::formal::incremental::IWS_AUTHORITY_PROFILE_VERSION && candidate.candidate.record.schema_version==7 && candidate.candidate.record.kind==FormalRecordKindV1::InitialActivation{
            if let Some(existing)=self.store.read_request_bounded(&candidate.candidate.record.request_digest,memory,work,stop)?{
                if let FormalRequestStateV1::Qualified{candidate:old,proof:installed,..}=existing.state(){
                    if !same(old,candidate,memory,work,stop)? || !same(installed,proof,memory,work,stop)? || !same(context.finalizing_authority().definition(),self.authority.definition(),memory,work,stop)? || !same(&context.finalizing_authority().activation_ref(),&self.authority.activation_ref(),memory,work,stop)?{return Err(FormalError::Conflict("GEN retained Qualified exact original proof/material".into()).into());}
                    if self.parent.is_some(){
                        let mut ctx=work.bounded_context(memory,stop);let quorum=verify_genesis_qualified_bounded(proof,&candidate.candidate,context,&mut ctx)?;
                        let fee=memory.reserve(std::mem::size_of::<PaidGenesisAcceptanceV1>()+std::mem::size_of::<FormalHeadRefV1>()+64,MemoryClass::Ordinary).map_err(|_|invalid("GEN retry head admission"))?;ctx.charge(1,64)?;
                        return Ok(PaidGenesisAcceptanceV1{head:fee.retain(FormalHeadRefV1{position:quorum.verified().record().position,record_id:hex::encode(quorum.verified().verified_record_id())})});
                    }
                }
            }
        }
        if self.authority.definition().profile_version!=oasis7_formal_protocol::controlled_authority::formal::incremental::IWS_AUTHORITY_PROFILE_VERSION || self.parent.is_some() || self.recovery.is_some() || candidate.candidate.record.schema_version!=7 || candidate.candidate.record.kind!=FormalRecordKindV1::InitialActivation || candidate.branch!=FormalCompletionBranchV1::Normal || candidate.frozen.is_some() || candidate.frozen_material.is_some(){return Err(invalid("GEN exact initial authority/candidate required").into());}
        if !same(
            context.finalizing_authority().definition(),
            self.authority.definition(),
            memory,
            work,
            stop,
        )? || !same(
            &context.finalizing_authority().activation_ref(),
            &self.authority.activation_ref(),
            memory,
            work,
            stop,
        )? || !same(
            &policy.provider_public_key,
            &self.authority.policy().fencing_provider_public_key,
            memory,
            work,
            stop,
        )? || !same(
            &policy.authorized_resource_bindings,
            &self.authority.policy().authorized_fencing_resource_bindings,
            memory,
            work,
            stop,
        )? {
            return Err(FormalError::FencingUnproven.into());
        }
        let instance = match self.role {
            EndpointRole::Primary => &self.authority.definition().primary.instance,
            EndpointRole::Replica => &self.authority.definition().replica.instance,
        };
        self.store.reconcile_pending_update_with_scope(
            provider,
            instance,
            policy,
            memory,
            work,
            stop,
            &store::bounded_transaction::RecoveryScope::Cold {
                authority: &self.authority,
                role: self.role,
            },
        )?;
        let capability = self
            .iws_page_store
            .as_ref()
            .ok_or_else(|| invalid("GEN real page capability required"))?;
        if !capability.jobs.matches(candidate, &mut || stop())? {
            return Err(invalid("GEN complete exact material job required").into());
        }
        let mut ctx = work.bounded_context(memory, stop);
        let quorum =
            verify_genesis_qualified_bounded(proof, &candidate.candidate, context, &mut ctx)?;
        drop(ctx);
        let job = capability.jobs.genesis_job()?;
        let closure = job.reference(quorum.verified(), memory)?;
        let request = &proof.decision.record.request_digest;
        let ready = self
            .store
            .read_request_bounded(request, memory, work, stop)?
            .ok_or_else(|| invalid("GEN requires durable local Ready"))?;
        match ready.state() {
            FormalRequestStateV1::Ready {
                candidate: local,
                decision,
                complete: Some(complete),
                ..
            } => {
                let expected = match self.role {
                    EndpointRole::Primary => &proof.primary_complete,
                    EndpointRole::Replica => &proof.replica_complete,
                };
                if !same(local, candidate, memory, work, stop)?
                    || !same(decision, &proof.decision, memory, work, stop)?
                    || !same(complete, expected, memory, work, stop)?
                {
                    return Err(invalid("GEN exact locally installed decision/complete").into());
                }
            }
            FormalRequestStateV1::Qualified {
                candidate: local,
                proof: installed,
                data_closure,
            } => {
                if !same(local, candidate, memory, work, stop)?
                    || !same(installed, proof, memory, work, stop)?
                    || !same(&data_closure.as_deref(), &Some(closure), memory, work, stop)?
                {
                    return Err(invalid("GEN exact durable Qualified recovery").into());
                }
            }
            _ => return Err(invalid("GEN requires durable local ProofComplete").into()),
        }
        let (parent, parent_fee) = paid_parent_copy(&quorum, memory, work, stop)?;
        let output_fee = memory
            .reserve(
                std::mem::size_of::<PaidGenesisAcceptanceV1>()
                    + std::mem::size_of::<FormalHeadRefV1>()
                    + 64,
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid("GEN head output admission"))?;
        work.charge(1, 64, stop)?;
        let head = output_fee.retain(FormalHeadRefV1 {
            position: quorum.verified().record().position,
            record_id: hex::encode(quorum.verified().verified_record_id()),
        });
        let qualified = qualified_state(candidate, &quorum, closure, memory, work, stop)?;
        let permit = GenesisInstallPermit {
            job,
            inputs: GenesisValidatedInputs { quorum, qualified },
        };
        let instance = match self.role {
            EndpointRole::Primary => &self.authority.definition().primary.instance,
            EndpointRole::Replica => &self.authority.definition().replica.instance,
        };
        let publication = self.store.publish_genesis_install_bounded(
            permit, provider, instance, policy, memory, work, stop,
        )?;
        // Only a successful real native atomic+Finish can create publication.
        // It retains the private job lease and quorum until this promotion.
        let (quorum, job) = publication.into_parts();
        drop(job);
        capability.jobs.accepted_genesis(quorum, parent_fee)?;
        self.parent = Some(parent);
        Ok(PaidGenesisAcceptanceV1 { head })
    }
}

impl FileFormalEndpoint {
    /// Read-only observation of the genuine retained GEN cache, never a permit.
    pub fn accepted_genesis_record_id(&self) -> Result<Option<[u8; 32]>, FormalError> {
        let Some(capability) = &self.iws_page_store else {
            return Ok(None);
        };
        let slot = capability.jobs.parent()?;
        let Some(cache) = slot.as_ref() else {
            return Ok(None);
        };
        let Some(quorum) = cache.genesis_quorum.as_ref() else {
            return Ok(None);
        };
        let Some(data) = cache.complete_data() else {
            return Err(invalid("GEN accepted complete evidence missing"));
        };
        if cache.commit_record != quorum.verified().verified_record_id()
            || data.candidate_record_id() != cache.commit_record
            || data.body_id() != cache.runtime.occurrence().head_body_id()
        {
            return Err(invalid("GEN accepted retained source/quorum mismatch"));
        }
        Ok(Some(cache.commit_record))
    }
}

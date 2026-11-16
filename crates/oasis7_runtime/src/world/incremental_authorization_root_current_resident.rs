//! A paid, immutable end-of-tick projection. The graph is shared only after
//! the original registered-write correspondence and actual Current SHA agree.
use super::*;
use oasis7_formal_protocol::incremental_execution::SealedExecutionBindingV1;
pub(in crate::world) struct FrozenNormalAuthorizationProjectionV1 {
    data: ProvisionalAuthorizationProjectionV1,
    expected: [u8;64],
    revocation_body: ReservedValue<oasis7_formal_protocol::incremental_state::StateRootV1>,
    guard: PreparedGuard,
    core: oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
    parent_record: [u8;32],
    fee: ReservedValue<()>,
}
impl CurrentAuthorizationProjectionV1<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn capture_normal_resident_v1<M,P,R,E,C,V,ER,CR>(
        &self, qualified:&ReservedValue<RuntimeTypedCompleteQualificationV1>, roots:&RootSetV1,
        overlay:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,
        read:&mut WorldFieldReadContextV1<'_,P,V,ER,CR>,
        work:&mut AuthorizationRootSourceReadWorkV1, remaining:&mut usize,
        common:&mut FormalBoundedContextV1<'_>, live:&ExecutionFence,
    )->Result<ReservedValue<FrozenNormalAuthorizationProjectionV1>,SourceError>
    where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,V:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,ER:FnMut()->Option<ValueTreeError>,CR:FnMut()->bool {
        self.validate(overlay.transaction,common)?;
        self.validate_registered_writes(overlay.transaction,remaining,common)?;
        let mut actual=[0u8;64];
        let length=read::paid(roots,overlay,read,work,common,|overlay,_,_|{
            let guard=overlay.transaction.guard();
            oasis7_world_state::incremental_runtime_authorization_root::read_current_authorization_root_value_v1(qualified.authorization_root(),roots,overlay,64,&guard,live,&mut &mut actual[..]).map_err(SourceError::from)
        })?;
        let hash=self.data.hash(common)?;
        common.charge(1,128)?;
        if length!=64 || actual!=self.expected || hash.as_bytes()!=actual {
            return Err(invalid("normal resident actual authorization root").into());
        }
        let fee=common.memory().reserve(std::mem::size_of::<FrozenNormalAuthorizationProjectionV1>(),MemoryClass::Ordinary)?.retain(());
        let revocation_body=paid_body(&self.revocation_body,common)?;
        let outer=common.memory().reserve(std::mem::size_of::<FrozenNormalAuthorizationProjectionV1>(),MemoryClass::Ordinary)?;
        let guard=overlay.transaction.guard();overlay.transaction.validate_guard(&guard,live).map_err(SourceError::Execution)?;
        Ok(outer.retain(FrozenNormalAuthorizationProjectionV1{
            data:ProvisionalAuthorizationProjectionV1{root:self.data.root.clone(),class:self.data.class,limits:self.data.limits},
            expected:actual,revocation_body,guard,core:common.work().identity(),parent_record:self.parent.head,fee,
        }))
    }
}
impl FrozenNormalAuthorizationProjectionV1 {
    /// Reuse the exact root verification performed by this private owner’s
    /// sole mint above: registered writes, resident seven-map canonical hash,
    /// and the actual physical root agreed before the immutable graph moved.
    /// This is not a caller-supplied hash or predicate result.
    pub(in crate::world) fn verify_projection_root_data_v1<M:MerkleNodeReader>(
        &self,tx:&oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_,M>,
        common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence,
    )->Result<(),SourceError> {
        common.charge(2,64)?;
        tx.validate_read_source_guard(&self.guard,live).map_err(SourceError::Execution)?;
        self.data.check_pool(common)?;
        if self.core!=common.work().identity() || !self.fee.uses_budget(common.memory())
            || !self.revocation_body.uses_budget(common.memory()) {
            return Err(invalid("projection verified authorization data context").into());
        }
        Ok(())
    }

    /// Actual published authorization-root scalar for the readonly projection.
    /// Read stability permits later source reads, never a different mutation.
    pub(in crate::world) fn projection_root_hash_v1<'a, M:MerkleNodeReader>(
        &'a self,
        parent:&crate::world::incremental_normal_sources::BoundNormalTickParentV1<'_>,
        tx:&oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_,M>,
        common:&mut FormalBoundedContextV1<'_>,
        live:&ExecutionFence,
    )->Result<&'a str,SourceError>{
        parent.validate_context(common)?;
        common.charge(2,96)?;
        tx.validate_read_source_guard(&self.guard,live).map_err(SourceError::Execution)?;
        self.data.check_pool(common)?;
        if self.core!=common.work().identity()||!self.fee.uses_budget(common.memory())
            ||!self.revocation_body.uses_budget(common.memory())
            ||self.parent_record!=parent.refs().head.verified_record_id()
            ||tx.parent_fence()!=parent.fence(){return Err(invalid("projection actual authorization source").into())}
        std::str::from_utf8(&self.expected).map_err(|_|invalid("projection authorization root UTF8").into())
    }

    /// The terminal witness is obtained only from the fixed finish owner after
    /// its write/fact cut equality check. An older strict guard is never refreshed.
    pub(in crate::world) fn prepare_checked_candidate_v1(
        &self,terminal:&crate::world::incremental_normal_host_finish::SealedNormalResidentSourceV1,
        core:&ReservedValue<oasis7_formal_protocol::incremental_material::CheckedCommitCore>,head:&VerifiedFormalCommitV1,history:&ReservedValue<oasis7_world_state::incremental_runtime_history_source::CheckedRuntimeHistorySourceV1>,roots:&RootSetV1,
        common:&mut FormalBoundedContextV1<'_>,
    )->Result<AcceptedAuthorizationProjectionV1,SourceError>{
        common.charge(16,8192)?;
        terminal.validate_authorization_source_v1(self,common).map_err(|_|SourceError::from(invalid("normal resident terminal source")))?;
        read::metadata(roots,common)?;
        if roots.digest().map_err(SourceError::Execution)?!=terminal.after_roots(){return Err(invalid("normal resident sealed roots").into());}
        let occurrence=history.occurrence();
        let mut parent=[0u8;32];hex::decode_to_slice(&head.record().parent_record_id,&mut parent).map_err(|_|invalid("normal authorization parent id"))?;
        if self.core!=common.work().identity()||!self.fee.uses_budget(common.memory())||!history.uses_budget(common.memory())
            ||occurrence.candidate_record_id()!=head.verified_record_id()||core.candidate_record_id()!=head.verified_record_id()||occurrence.body_id()!=core.catalog().entry.commit||core.body().roots!=terminal.after_roots()
            ||occurrence.roots_id()!=terminal.after_roots()||history.manifest().roots!=terminal.after_roots()
            ||parent!=self.parent_record {
            return Err(invalid("normal resident accepted authorization lineage").into());
        }
        let fee=common.memory().reserve(std::mem::size_of::<AcceptedAuthorizationProjectionV1>(),MemoryClass::Ordinary)?.retain(());
        let revocation_body=paid_body(&self.revocation_body,common)?;
        self.data.check_pool(common)?;
        Ok(AcceptedAuthorizationProjectionV1{
            data:ProvisionalAuthorizationProjectionV1{root:self.data.root.clone(),class:self.data.class,limits:self.data.limits},
            head:head.verified_record_id(),body:occurrence.body_id(),roots:terminal.after_roots(),revocation_body,expected:self.expected,fee,
        })
    }
}
impl AcceptedAuthorizationProjectionV1 {
    pub(in crate::world) fn share_normal_ordinary_v1(&self,common:&mut FormalBoundedContextV1<'_>)->Result<Self,SourceError>{
        common.consume_steps(1)?;self.data.check_pool(common)?;
        let fee=common.memory().reserve(std::mem::size_of::<Self>(),MemoryClass::Ordinary)?.retain(());
        Ok(Self{data:ProvisionalAuthorizationProjectionV1{root:self.data.root.clone(),class:self.data.class,limits:self.data.limits},head:self.head,body:self.body,roots:self.roots,revocation_body:paid_body(&self.revocation_body,common)?,expected:self.expected,fee})
    }
}

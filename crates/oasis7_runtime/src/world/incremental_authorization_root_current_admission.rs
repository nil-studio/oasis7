//! Read-only admission closure of the actual registered seven-source stage.
use super::*;
impl CurrentAuthorizationProjectionV1<'_>{
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn module_admission_grant_presence_v1<M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,V:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,ER:FnMut()->Option<ValueTreeError>,CR:FnMut()->bool>(
        &self,key:&str,qualified:&ReservedValue<RuntimeTypedCompleteQualificationV1>,roots:&RootSetV1,overlay:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,V,ER,CR>,work:&mut AuthorizationRootSourceReadWorkV1,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence,
    )->Result<ReservedValue<oasis7_world_state::incremental_runtime_authorization_grants_family::CurrentAuthorizationGrantPresenceV1>,SourceError>{
        self.validate(overlay.transaction,common)?;
        read::paid(roots,overlay,parent,work,common,|ctx,parent,_|{
            let guard=ctx.transaction.guard();oasis7_world_state::incremental_runtime_authorization_grants_family::read_current_authorization_grant_presence_v1(qualified.grants(),roots,key,ctx.transaction,parent,&guard,live).map_err(SourceError::from)
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn read_module_admission_legacy_v1<M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,V:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,ER:FnMut()->Option<ValueTreeError>,CR:FnMut()->bool>(
        &self,key:&str,family:&ReservedValue<oasis7_world_state::incremental_runtime_module_authorization::ModuleAuthorizationFamilyQualificationV1>,roots:&RootSetV1,overlay:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,V,ER,CR>,work:&mut AuthorizationRootSourceReadWorkV1,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence,
    )->Result<Option<ReservedValue<oasis7_world_state::effect::CapabilityGrant>>,SourceError>{
        self.validate(overlay.transaction,common)?;
        read::paid(roots,overlay,parent,work,common,|ctx,_,work|{
            let max=usize::try_from(work.max_text_bytes).map_err(|_|limited())?;
            let width=max.max(16).checked_add(std::mem::size_of::<String>()).ok_or_else(limited)?;
            let kinds=work.max_selected_heap.checked_div(width).unwrap_or(0).saturating_sub(2);
            let guard=ctx.transaction.guard();
            crate::world::incremental_module_execution_authorization::read_root_module_capability_v1(key,family,ctx,&mut work.module_authorization,&guard,live,&crate::world::incremental_module_execution_authorization::RootModuleAuthorizationLimitsV1{max_text_bytes:max,max_effect_kinds:kinds}).map_err(SourceError::from)
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn read_module_admission_grant_v1<M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,V:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,ER:FnMut()->Option<ValueTreeError>,CR:FnMut()->bool>(
        &self,key:&str,qualified:&ReservedValue<RuntimeTypedCompleteQualificationV1>,roots:&RootSetV1,overlay:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,V,ER,CR>,work:&mut AuthorizationRootSourceReadWorkV1,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence,
    )->Result<Option<ReservedValue<serde_json::Value>>,SourceError>{
        self.validate(overlay.transaction,common)?;
        read::paid(roots,overlay,parent,work,common,|ctx,parent,work|{
            let mut parent=WorldFieldReadContextV1{root:roots.state_root(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization),state:parent.state,values:oasis7_formal_protocol::incremental_value::ValueReadContext{source:parent.values.source,memory:parent.values.memory,class:parent.values.class,io:&mut*parent.values.io},key_work:&mut*parent.key_work,semantic_bytes:parent.semantic_bytes,record_bytes:parent.record_bytes,take_state_error:&mut parent.take_state_error,cancelled:&mut parent.cancelled};
            let guard=ctx.transaction.guard();oasis7_world_state::incremental_runtime_authorization_grants_family::recover_current_authorization_grant_v1(qualified.grants(),roots,key,ctx.transaction,&mut parent,&mut work.json,work.max_selected_heap,&guard,live).map_err(SourceError::from)
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn read_module_admission_revocation_v1<M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,V:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,ER:FnMut()->Option<ValueTreeError>,CR:FnMut()->bool>(
        &self,request:oasis7_world_state::incremental_runtime_revocation::ModuleAdmissionRevocationRequestV1<'_>,qualified:&ReservedValue<RuntimeTypedCompleteQualificationV1>,roots:&RootSetV1,overlay:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,V,ER,CR>,work:&mut AuthorizationRootSourceReadWorkV1,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence,
    )->Result<ReservedValue<oasis7_world_state::incremental_runtime_revocation::ModuleAdmissionRevocationValueV1>,SourceError>{
        self.validate(overlay.transaction,common)?;
        read::paid(roots,overlay,parent,work,common,|ctx,parent,work|{
            let guard=ctx.transaction.guard();oasis7_world_state::incremental_runtime_revocation::read_current_module_admission_revocation_v1(request,qualified.revocation(),ctx,parent,&mut work.scan,&mut work.typed,&guard,live).map_err(SourceError::from)
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn validate_module_admission_root_v1<M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,V:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,ER:FnMut()->Option<ValueTreeError>,CR:FnMut()->bool>(
        &self,qualified:&ReservedValue<RuntimeTypedCompleteQualificationV1>,roots:&RootSetV1,
        overlay:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,V,ER,CR>,work:&mut AuthorizationRootSourceReadWorkV1,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence,
    )->Result<Result<(),crate::WorldError>,SourceError>{
        self.validate(overlay.transaction,common)?;read::metadata(roots,common)?;
        qualified.validate_binding(roots)?;
        if !qualified.uses_budget(common.memory())||qualified.allocation_class()!=MemoryClass::Ordinary{return Err(invalid("module admission typed source").into())}
        self.validate_registered_writes(overlay.transaction,&mut work.pending_entries,common)?;
        let actual=read::paid(roots,overlay,parent,work,common,|ctx,parent,work|{
            let guard=ctx.transaction.guard();
            oasis7_world_state::incremental_runtime_revocation::read_current_module_admission_revocation_v1(
                oasis7_world_state::incremental_runtime_revocation::ModuleAdmissionRevocationRequestV1::Body,qualified.revocation(),ctx,parent,&mut work.scan,&mut work.typed,&guard,live).map_err(SourceError::from)
        })?;
        let oasis7_world_state::incremental_runtime_revocation::ModuleAdmissionRevocationValueV1::Body(body)=&*actual else{return Err(invalid("module admission revocation observation").into())};
        if body!=&*self.revocation_body{return Err(invalid("module admission stale revocation projection").into())}
        let expected=self.data.hash(common)?;
        struct Compare<'a>{expected:&'a[u8],offset:usize,equal:bool}
        impl std::io::Write for Compare<'_>{fn write(&mut self,b:&[u8])->std::io::Result<usize>{let end=self.offset.checked_add(b.len()).ok_or_else(||std::io::Error::other("authorization root length"))?;self.equal&=self.expected.get(self.offset..end)==Some(b);self.offset=end;Ok(b.len())}fn flush(&mut self)->std::io::Result<()>{Ok(())}}
        let mut compare=Compare{expected:expected.as_bytes(),offset:0,equal:true};
        read::paid(roots,overlay,parent,work,common,|ctx,_,work|{
            let guard=ctx.transaction.guard();oasis7_world_state::incremental_runtime_authorization_root::read_current_authorization_root_value_v1(qualified.authorization_root(),roots,ctx,work.max_text_bytes,&guard,live,&mut compare).map_err(SourceError::from)
        })?;
        overlay.transaction.validate_guard(&overlay.transaction.guard(),live).map_err(SourceError::Execution)?;
        Ok(if compare.equal&&compare.offset==expected.len(){Ok(())}else{Err(crate::world::capability_authorization::deny("capability authorization root does not match durable state"))})
    }
}

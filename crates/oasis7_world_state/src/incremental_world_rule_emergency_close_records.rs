//! Fixed original record selection for Active/Observe/Close. Child of the
//! genuine Current Emergency source; neither DTO nor reader grants authority.
use super::*;
pub struct CurrentEmergencyEvidenceV1 {core:CurrentEmergencyRecordCoreV1,value:Option<RecoveredTypedRecordV1<oasis7_agent_api::world_action::EmergencyEvidenceV1>>}
impl CurrentEmergencyEvidenceV1 {pub fn outer_id(&self)->&str{&self.core.outer_id}pub fn value(&self)->Option<&oasis7_agent_api::world_action::EmergencyEvidenceV1>{self.value.as_ref().map(RecoveredTypedRecordV1::value)}}
pub struct CurrentEmergencyServiceV1 {core:CurrentEmergencyRecordCoreV1,value:Option<RecoveredTypedRecordV1<oasis7_agent_api::world_action::EmergencyServiceRegistrationV1>>}
impl CurrentEmergencyServiceV1 {pub fn outer_id(&self)->&str{&self.core.outer_id}pub fn value(&self)->Option<&oasis7_agent_api::world_action::EmergencyServiceRegistrationV1>{self.value.as_ref().map(RecoveredTypedRecordV1::value)}}
pub struct CurrentEmergencyMinimumServiceV1 {core:CurrentEmergencyRecordCoreV1,value:Option<RecoveredTypedRecordV1<crate::world_rules::MinimumServiceContractV1>>}
impl CurrentEmergencyMinimumServiceV1 {pub fn outer_id(&self)->&str{&self.core.outer_id}pub fn value(&self)->Option<&crate::world_rules::MinimumServiceContractV1>{self.value.as_ref().map(RecoveredTypedRecordV1::value)}}
read_api!(read_current_emergency_evidence_v1,CurrentEmergencyEvidenceV1,oasis7_agent_api::world_action::EmergencyEvidenceV1,"evidence");
read_api!(read_current_emergency_service_v1,CurrentEmergencyServiceV1,oasis7_agent_api::world_action::EmergencyServiceRegistrationV1,"services");
read_api!(read_current_emergency_minimum_service_v1,CurrentEmergencyMinimumServiceV1,crate::world_rules::MinimumServiceContractV1,"minimum_services");

fn validate_selected_current<M,R,E,C>(source:&ReservedValue<CurrentEmergencyLifecycleSourceV1>,selected:&CurrentEmergencyRecordCoreV1,collection:&'static str,parent:&ReservedValue<WorldRuleCompositeQualificationV1<Emergency>>,roots:&RootSetV1,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,work:&TypedRecordReadWorkV1,guard:&PreparedGuard,live:&ExecutionFence)->Result<(),WorldFieldOverlayErrorV1>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool{
 validate_source(source,parent,roots,ctx,guard,live)?;ctx.transaction.validate_transaction_identity(&selected.identity)?;
 if selected.parent!=source.parent||selected.collection!=collection||!selected.memory.same_pool(ctx.values.memory)||selected._fee.bytes()==0{return Err(bad().into());}
 let nested=collection_root(source.overlay.body(),collection,ctx,work)?;let p=WorldRuleCompositeBranchV1::EmergencySupply.profile();
 let mut records=InventoryValueReader {values:&mut ctx.values,key_work:&mut *ctx.key_work,semantic_bytes:ctx.semantic_bytes,record_bytes:ctx.record_bytes,cancelled:&mut ctx.cancelled};
 let key=records.key(p.field,WorldFieldRecordKindV1::Entry,&("entry",selected.outer_id.as_str()))?.0;
 if crate::incremental_record_tree::lookup(&nested,&key,&mut records,p.labels)?!=selected.expected{return Err(bad().into());}
 if (records.cancelled)(){return Err(ValueTreeError::Cancelled.into());}Ok(())
}
macro_rules! validate_selection {
 ($name:ident,$wrapper:ty,$collection:literal)=>{
  pub fn $name<M,R,E,C>(source:&ReservedValue<CurrentEmergencyLifecycleSourceV1>,selected:&ReservedValue<$wrapper>,parent:&ReservedValue<WorldRuleCompositeQualificationV1<Emergency>>,roots:&RootSetV1,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,work:&TypedRecordReadWorkV1,guard:&PreparedGuard,live:&ExecutionFence)->Result<(),WorldFieldOverlayErrorV1>
  where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool{
   let result=(||{if !selected.uses_budget(ctx.values.memory)||selected.reserved_bytes()<size_of::<$wrapper>(){return Err(bad().into());}validate_selected_current(source,&selected.core,$collection,parent,roots,ctx,work,guard,live)})();if result.is_err(){let _:Result<(),ExecutionError>=ctx.transaction.contribute(|_|Err(ExecutionError::Aborted));}result
  }
 };
}
validate_selection!(validate_current_emergency_service_selection_v1,CurrentEmergencyServiceV1,"services");
validate_selection!(validate_current_emergency_minimum_selection_v1,CurrentEmergencyMinimumServiceV1,"minimum_services");

/// Original services-map materialization used by Observe. The registration is
/// borrowed from a genuine selected source; this structural DATA writer does
/// not grant observation authority. Actual key/count CAS remain the sole core.
#[allow(clippy::too_many_arguments)]
pub fn replace_current_emergency_service_v1<M,S,R,E,C,W,CW>(source:&ReservedValue<CurrentEmergencyLifecycleSourceV1>,selected:&ReservedValue<CurrentEmergencyServiceV1>,after:Option<&oasis7_agent_api::world_action::EmergencyServiceRegistrationV1>,parent:&ReservedValue<WorldRuleCompositeQualificationV1<Emergency>>,roots:&RootSetV1,state:&S,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,write:&mut MemberRecordWriteContext<'_,W,CW>,scan:&mut StateScanBudget,read_work:&mut WorldRuleCompositeReadWorkV1,write_work:&mut WorldRuleCompositeWriteWorkV1,guard:&PreparedGuard,live:&ExecutionFence)->Result<Option<ReservedValue<EmergencyOverlay>>,WorldFieldOverlayErrorV1>
where M:MerkleNodeReader,S:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,W:ValueNodeReader+ValueNodeWriter+StatePageWriter,CW:FnMut()->bool{
 let result=if selected.core.collection!="services"||!selected.uses_budget(ctx.values.memory)||selected.reserved_bytes()<size_of::<CurrentEmergencyServiceV1>(){Err(bad().into())}else{super::patch_selected::<oasis7_agent_api::world_action::EmergencyServiceRegistrationV1,_,_,_,_,_,_,_>(source,&selected.core,after,parent,roots,state,ctx,write,scan,read_work,write_work,guard,live)};
 if result.is_err(){let _:Result<(),ExecutionError>=ctx.transaction.contribute(|_|Err(ExecutionError::Aborted));}result
}

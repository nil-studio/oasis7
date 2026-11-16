//! Fixed activity-close batch. The next registration observes the previous
//! publication, while module routing remains deferred until the batch end.
use super::*;
use crate::world::{incremental_module_invocation::*,
 incremental_body_event::{AgentEventOriginalV1,materials::CorePolicyMaterialContextV1},
 incremental_normal_industry_due::deferred::DueEventQueueV1,
 incremental_normal_tail::NormalTailWorkV1};
use oasis7_formal_protocol::{incremental_material::OriginalObjectStreamReader,
 incremental_schedule::SchedulePageWriter,incremental_state::StatePageWriter,
 incremental_value::{ValueNodeReader,ValueNodeWriter,ValueTreeError}};
use oasis7_world_state::{incremental_field_read::WorldFieldReadContextV1,
 incremental_world_rule_continuity::apply_current_organization_continuity_system_v1,
 world_rules::{prepare_lifecycle_commands_selected_from_current_v1,apply_system_lifecycle_command_from_current_v1}};
impl<'a,'t,'s,'v,M,R,E,C,W,F,O> RootModuleExecutionContextV1<'a,'t,'s,'v,M,R,E,C,W,F,O>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,
 W:ValueNodeReader+ValueNodeWriter+StatePageWriter+SchedulePageWriter,
 F:FnMut(&mut dyn AgentEventOriginalV1)->Result<(),BodyMutationError>,O:OriginalObjectStreamReader {
 #[allow(clippy::too_many_arguments)]
 pub(in crate::world) fn execute_normal_organization_continuity_v1<P,CE,CC,PE,PC,ME>(
 &mut self,parent_source:&ReservedValue<BoundNormalTickParentV1<'_>>,sources:&NormalTailSourcesV1<'_,'_>,
 economic:&crate::world::incremental_normal_world_rule_lifecycle::NormalWorldRuleLifecycleSourcesV1<'_>,
 work:&mut NormalOrganizationTailWorkV1,materials:&mut CorePolicyMaterialContextV1<'_,M,ME>,
 current:&mut WorldFieldReadContextV1<'_,P,R,CE,CC>,parent:&mut WorldFieldReadContextV1<'_,P,R,PE,PC>,
 normal:&mut NormalTailWorkV1<'_,'_>,directory:&ReservedValue<AcceptedInvocationDirectoryV1>,
 enumeration:&mut CurrentInvocationEnumerationWorkV1,live_owner:&mut dyn FnMut()->ExecutionFence,
 )->Result<(),RootModuleExecutionErrorV1<O::Error>>
 where P:MerkleNodeReader,CE:FnMut()->Option<ValueTreeError>,CC:FnMut()->bool,
 PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool,ME:FnMut()->Option<ValueTreeError> {
 let result=(|| {
 macro_rules! common {()=>{normal.common_mut().map_err(RootModuleExecutionErrorV1::Body)?}}
 normal.validate_parent(parent_source).map_err(RootModuleExecutionErrorV1::Body)?;
 let now=self.now;
 let state=self.state;
 let initial=super::activity_io::run(self.ctx,parent,self.members,materials,work,common!(),|ctx,_parent,_write,_materials,work,tail|CurrentOrganizationContinuityOwnerV1::open(parent_source,sources,tail,state,ctx,&mut work.scan,&mut work.read,&live_owner()))
 .map_err(RootModuleExecutionErrorV1::Body)?;
 let Some(value)=initial.value() else {return Ok(())};
 let source_bytes=super::activity_io::run(self.ctx,parent,self.members,materials,work,common!(),|ctx,_parent,_write,_materials,work,_tail|initial.current.retained_allocation_bytes_v1(&mut work.cells_remaining,&mut ctx.cancelled).map_err(Into::into)).map_err(RootModuleExecutionErrorV1::Body)?;
 let snapshot_bytes=value.registrations.len().checked_mul(size_of::<ReservedValue<Vec<(String,String,oasis7_agent_api::world_action::OrganizationContinuityCommandV1)>>>()).ok_or(ValueTreeError::ResourceLimited("organization command vector"))?;
 let snapshot_fee=common!().memory().reserve(snapshot_bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 common!().charge(value.registrations.len() as u64,snapshot_bytes as u64).map_err(|e|RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e))))?;
 let mut commands=Vec::with_capacity(value.registrations.len());
 for registration in value.registrations.values(){
 let facts=super::activity_io::run(self.ctx,parent,self.members,materials,work,common!(),|ctx,parent,_write,_materials,work,tail|super::continuity_facts::read_complete_facts_v1(registration,sources,economic.orders,state,ctx,parent,work,super::continuity_facts::LifecycleFactScopeV1::Lifecycle,tail,&live_owner())).map_err(RootModuleExecutionErrorV1::Body)?;
 let row_bytes=source_bytes.checked_mul(4).and_then(|n|n.checked_add(facts.clone_allocation_bytes().checked_mul(4)?)).and_then(|n|n.checked_add(4096)).ok_or(ValueTreeError::ResourceLimited("organization command snapshot copy"))?;
 let row_fee=common!().memory().reserve(row_bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 common!().charge(1,row_bytes as u64).map_err(|e|RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e))))?;
 let facts_snapshot=std::collections::BTreeMap::from([(registration.organization_id.clone(),facts.value_for_lifecycle().map_err(RootModuleExecutionErrorV1::Body)?.clone())]);
 let row=prepare_lifecycle_commands_selected_from_current_v1(value,&facts_snapshot,now,Some(&registration.organization_id)).map_err(|error|RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(OrganizationContinuityInfraErrorV1(error)))))?;
 commands.push(row_fee.retain(row));
 }
 let commands=snapshot_fee.retain(commands);
 drop(initial);
 let mut queue=DueEventQueueV1::new(self.ctx.transaction,common!(),now,&live_owner()).map_err(RootModuleExecutionErrorV1::Body)?;
 for selected_command in commands.iter().flat_map(|row|row.iter()) {
 let live=live_owner();
 let owner=super::activity_io::run(self.ctx,parent,self.members,materials,work,common!(),|ctx,_parent,_write,_materials,work,tail|CurrentOrganizationContinuityOwnerV1::open(parent_source,sources,tail,state,ctx,&mut work.scan,&mut work.read,&live))
 .map_err(RootModuleExecutionErrorV1::Body)?;
 let Some(before)=owner.value() else {continue};
 use oasis7_agent_api::world_action::OrganizationContinuityCommandV1 as Command;
 let organization_id=match &selected_command.2 {
 Command::FinalizeAvailabilityUnknown{organization_id,..}|Command::OpenDormancyCase{organization_id,..}=>Some(organization_id.as_str()),
 Command::PublishNotice{case_id,..}|Command::AdvancePhase{case_id,..}=>before.cases.get(case_id).map(|case|case.organization_id.as_str()),
 _=>return Err(ValueTreeError::Invalid("non-system continuity command snapshot").into()),
 };
 let Some(registration)=organization_id.and_then(|id|before.registrations.get(id)) else {continue};
 let guard=self.ctx.transaction.guard();
 let binding=super::activity_io::run(self.ctx,parent,self.members,materials,work,common!(),|ctx,parent,_write,_materials,work,_tail|crate::world::incremental_industrial_eligibility::read_current_world_binding_v1(sources.runtime_state().resources(),ctx,parent,&mut work.eligibility,&guard,&live).map_err(|e|BodyMutationError::Original(Box::new(OrganizationContinuityInfraErrorV1(e)))))
 .map_err(|e|RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e))))?;
 let epoch_length=super::activity_io::run(self.ctx,parent,self.members,materials,work,common!(),|ctx,_parent,_write,_materials,work,_tail|{let guard=ctx.transaction.guard();oasis7_world_state::incremental_runtime_governance::read_current_governance_epoch_length_v1(economic.governance,sources.roots(),state,ctx,&mut work.scan,work.read.max_tree_heap,&guard,&live).map_err(|e|BodyMutationError::Original(Box::new(e)))})
 .map_err(RootModuleExecutionErrorV1::Body)?;
 let facts=super::activity_io::run(self.ctx,parent,self.members,materials,work,common!(),|ctx,parent,_write,_materials,work,tail|super::continuity_facts::read_complete_facts_v1(registration,sources,economic.orders,state,ctx,parent,work,super::continuity_facts::LifecycleFactScopeV1::Lifecycle,tail,&live))
 .map_err(RootModuleExecutionErrorV1::Body)?;
 let phase=PreparedNormalOrganizationEventV1::reserve_event_v1(common!(),self.events.max_event_bytes).map_err(RootModuleExecutionErrorV1::Body)?;
 let command=selected_command.clone();
 let world_command=oasis7_agent_api::world_action::WorldRuleCommandV1::OrganizationContinuity{command:command.2.clone()};
 let Some(header)=super::activity_io::run(self.ctx,parent,self.members,materials,work,common!(),|ctx,parent,_write,_materials,work,tail|super::header::prepare_system_header_v1(&command.0,&command.1,&world_command,binding.as_deref(),sources,economic.orders,state,ctx,parent,work,tail,&live)).map_err(RootModuleExecutionErrorV1::Body)? else {continue};
 let retained_bytes=super::activity_io::run(self.ctx,parent,self.members,materials,work,common!(),|ctx,_parent,_write,_materials,work,_tail|owner.current.retained_allocation_bytes_v1(&mut work.cells_remaining,&mut ctx.cancelled).map_err(Into::into)).map_err(RootModuleExecutionErrorV1::Body)?;
 // The closed derived-Clone continuity DTO contains only the original
 // String/Vec/BTree containers. Four original decoder allocation envelopes
 // cover the state copy plus the fixed reducer's new case/notice copies.
 let clone_bytes=retained_bytes.checked_mul(4).and_then(|n|n.checked_add(facts.clone_allocation_bytes())).and_then(|n|n.checked_add(4096)).ok_or(ValueTreeError::ResourceLimited("organization reducer copy envelope"))?;
 let clone_fee=common!().memory().reserve(clone_bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 common!().charge(1,clone_bytes as u64).map_err(|e|RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e))))?;
 let mut after=before.clone();
 let result_id=if header.retry{None}else{
 let key_fee=common!().memory().reserve(512,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 common!().charge(1,(command.0.len()+command.1.len()) as u64).map_err(|e|RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e))))?;
 let key=oasis7_world_state::world_rules::request_key(&command.0,&command.1);
 let result=apply_system_lifecycle_command_from_current_v1(&mut after,facts.value_for_lifecycle().map_err(RootModuleExecutionErrorV1::Body)?,&command.0,&key,&command.2,now,binding.as_deref().unwrap_or(""));
 drop(key_fee);
 match result{Ok(id)=>Some(id),Err(_)=>continue}
 };
 let guard=self.ctx.transaction.guard();
 let overlay=if header.retry{None}else{super::activity_io::run(self.ctx,parent,self.members,materials,work,common!(),|ctx,_parent,write,_materials,work,_tail|apply_current_organization_continuity_system_v1(sources.native().rules().continuity(),sources.roots(),&owner.current,&after,state,ctx,write,&mut work.scan,&mut work.read,&mut work.write,&guard,&live).map_err(|e|BodyMutationError::Original(Box::new(e))))
 .map_err(|e|RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e))))?};
 if !header.retry&&!world_command.is_runtime_observation(){
 let receipt=oasis7_world_state::world_rules::WorldRuleReceiptV1{actor_id:command.0.clone(),request_id:command.1.clone(),command:world_command.clone(),committed_at:now,result_id:result_id.ok_or_else(||RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid("organization receipt result missing")))?};
 let guard=self.ctx.transaction.guard();
 let _request_overlay=super::activity_io::run(self.ctx,parent,self.members,materials,work,common!(),|ctx,_parent,write,_materials,work,_tail|oasis7_world_state::incremental_world_rule_requests_current::insert_current_world_rule_request_v1(&header.request,&receipt,sources.native().rules().requests(),sources.roots(),state,ctx,write,&mut work.scan,&mut work.read,&mut work.write,&guard,&live).map_err(|e|BodyMutationError::Original(Box::new(e)))).map_err(RootModuleExecutionErrorV1::Body)?;
 }
 let event_id=crate::world::incremental_normal_tail::event_preview::preview_current_normal_event_id_v1(self.ctx,common!()).map_err(RootModuleExecutionErrorV1::Body)?;
 let event=WorldEvent{id:event_id,time:now,caused_by:None,body:WorldEventBody::Domain(crate::DomainEvent::WorldRuleCommitted{
 action_id:0,actor_id:command.0,request_id:command.1,command:world_command,governance_epoch:now/epoch_length.max(1),world_binding:binding.as_ref().cloned()})};
 let mut prepared=PreparedNormalOrganizationEventV1::after_current_write(event,&owner,self.ctx.transaction,common!(),now,&live_owner(),phase).map_err(RootModuleExecutionErrorV1::Body)?;
 prepared.header=Some(header);
 prepared.activity_inputs=Some((facts,None,clone_fee.retain(after),owner.current,binding));
 let paid=self.persist_due_organization_tail_v1(prepared,materials,common!(),live_owner)?;
 queue.push(paid,self.ctx.transaction,common!(),now,&live_owner()).map_err(RootModuleExecutionErrorV1::Body)?;
 drop(overlay);
 }
 self.flush_due_events_v1(&mut queue,common!(),directory,enumeration,live_owner)?;
 Ok(())
 })();
 if result.is_err(){let _:Result<(),ExecutionError>=self.ctx.transaction.contribute(|_|Err(ExecutionError::Aborted));}
 result
 }
}

#[derive(Debug)]struct OrganizationContinuityInfraErrorV1<T:std::fmt::Debug>(T);
impl<T:std::fmt::Debug>std::fmt::Display for OrganizationContinuityInfraErrorV1<T>{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{write!(f,"organization continuity infrastructure: {:?}",self.0)}}
impl<T:std::fmt::Debug>std::error::Error for OrganizationContinuityInfraErrorV1<T>{}

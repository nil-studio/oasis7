//! Fixed Current apply preconditions, preserving the original first business
//! error. Completion of this prefix is not finality or publication authority.
use super::*;
use super::instance_current::{CurrentModuleInstanceFactsV1,ModuleInstanceFactsFeesV1};
use crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1;
use crate::world::incremental_module_invocation::AcceptedInvocationDirectoryV1;
use oasis7_formal_protocol::incremental_value::ValueNodeReader;
use oasis7_world_state::{incremental_runtime_proposal::CurrentRuntimeProposalV1,incremental_field_overlay::WorldFieldOverlayContextV1,incremental_field_read::WorldFieldReadContextV1};

pub(super) struct PreparedCurrentModuleApplyPrefixV1{
 pub(super) proposal:ReservedValue<CurrentRuntimeProposalV1>,
 pub(super) applied_manifest:crate::Manifest,pub(super) changes:Option<crate::ModuleChangeSet>,
 pub(super) applied_hash:ReservedValue<String>,pub(super) epoch:u64,
 pub(super) facts:ModuleInstanceFactsFeesV1,pub(super) fee:MemoryReservation,
 guard:PreparedGuard,core:WorkIdentityV1,now:u64,
}
pub(super) enum ModuleApplyPrefixDecisionV1{
 Ready(PreparedCurrentModuleApplyPrefixV1),
 Rejected{error:ReservedValue<crate::WorldError>,_facts:Option<ModuleInstanceFactsFeesV1>},
}
impl PreparedCurrentModuleApplyPrefixV1{
 pub(super) fn validate_strict_v1<M:MerkleNodeReader>(&self,tx:&ExecutionTransaction<'_,M>,common:&FormalBoundedContextV1<'_>,now:u64,live:&ExecutionFence)->Result<(),BodyMutationError>{
 tx.validate_guard(&self.guard,live)?;
 self.proposal.validate_provenance(tx,common.memory(),live).map_err(overlay_error)?;
 if self.core!=common.work().identity()||self.now!=now||!self.fee.uses_budget(common.memory())||!self.applied_hash.uses_budget(common.memory()){return Err(invalid())}Ok(())
 }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_current_module_apply_prefix_v1<M,P,R,E,C,PE,PC>(
 proposal:ReservedValue<CurrentRuntimeProposalV1>,sources:&ModuleLifecycleSourcesV1<'_>,
 ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,R,PE,PC>,
 factory:&mut FactoryScheduleWorkV1,module:&mut ModuleLifecycleWorkV1<'_>,common:&mut FormalBoundedContextV1<'_>,
 directory:&ReservedValue<AcceptedInvocationDirectoryV1>,now:u64,live:&ExecutionFence,
)->Result<ModuleApplyPrefixDecisionV1,FactoryScheduleReadErrorV1>
where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool{
 proposal.validate_strict(ctx.transaction,common.memory(),live).map_err(overlay_error)?;
 let visits=proposal.output_reservation_count_v1()?;pay(common,visits.checked_mul(std::mem::size_of::<MemoryReservation>()).ok_or_else(invalid)?)?;
 let heap=proposal.output_reserved_bytes_v1()?.checked_mul(8).and_then(|n|n.checked_add(16384)).ok_or_else(invalid)?;
 pay(common,heap)?;let fee=common.memory().reserve(heap,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let id=proposal.original_id();let Some(value)=proposal.value()else{return Ok(ModuleApplyPrefixDecisionV1::Rejected{error:fee.retain(crate::WorldError::ProposalNotFound{proposal_id:id}),_facts:None})};
 let approved_hash=match &value.status{oasis7_world_state::ProposalStatus::Approved{manifest_hash,..}=>manifest_hash,_=>return Ok(ModuleApplyPrefixDecisionV1::Rejected{error:fee.retain(crate::WorldError::ProposalInvalidState{proposal_id:id,expected:"approved".to_owned(),found:value.status.label()}),_facts:None})};
 let(brake,policy)=crate::world::incremental_normal_action_factory_schedule::with_module_lifecycle_read_io_v1(sources.families.roots,ctx,parent,factory,common,module,|ctx,parent,_,module|{
  use oasis7_world_state::incremental_runtime_controls::{RuntimeControlFieldV1,RuntimeControlReadV1,read_runtime_control_overlay_v1};
  let brake=match read_runtime_control_overlay_v1(RuntimeControlFieldV1::GovernanceEmergencyBrakeUntilTick,ctx,0,&mut std::io::sink()).map_err(overlay_error)?{RuntimeControlReadV1::Unit=>None,RuntimeControlReadV1::Unsigned(v)=>Some(v),_=>return Err(invalid().into())};
  let guard=ctx.transaction.guard();
  let policy=oasis7_world_state::incremental_runtime_governance::read_current_governance_execution_policy_v1(sources.governance,sources.families.roots,parent.state,ctx,module.proposal_scan,module.max_selected_heap,&guard,live).map_err(overlay_error)?;
  Ok((brake,policy))
 })?;
 let epoch=now/policy.epoch_length_ticks.max(1);
 let rejection=if brake.is_some_and(|until|now<until){Some(crate::WorldError::GovernancePolicyInvalid{reason:format!("governance apply blocked by emergency brake until_tick={}",brake.unwrap_or(now))})}
 else if value.not_before_tick.is_some_and(|until|now<until){Some(crate::WorldError::GovernancePolicyInvalid{reason:format!("proposal_id={} timelock pending current_tick={} not_before_tick={}",id,now,value.not_before_tick.unwrap_or(now))})}
 else if value.activate_epoch.is_some_and(|until|epoch<until){Some(crate::WorldError::GovernancePolicyInvalid{reason:format!("proposal_id={} activation epoch pending current_epoch={} activate_epoch={}",id,epoch,value.activate_epoch.unwrap_or(epoch))})}else{None};
 if let Some(error)=rejection{return Ok(ModuleApplyPrefixDecisionV1::Rejected{error:fee.retain(error),_facts:None})}
 let changes=match value.manifest.module_changes(){Ok(v)=>v,Err(error)=>return Ok(ModuleApplyPrefixDecisionV1::Rejected{error:fee.retain(error),_facts:None})};
 let mut source=CurrentModuleInstanceFactsV1::new(sources,ctx,parent,factory,module,common,live,directory)?;
 let result=if let Some(changes)=&changes{super::registry_rules::validate_module_changes_from_source_v1(&mut source,changes)}else{Ok(())};
 let facts=source.into_parts();
 if let Err(error)=result{let error=match error{super::registry_rules::ModuleRegistryRuleErrorV1::World(e)=>e,super::registry_rules::ModuleRegistryRuleErrorV1::Source(FactoryScheduleReadErrorV1::World(e))=>e,super::registry_rules::ModuleRegistryRuleErrorV1::Source(e)=>return Err(e)};return Ok(ModuleApplyPrefixDecisionV1::Rejected{error:fee.retain(error),_facts:Some(facts)})}
 let applied_manifest=match if changes.is_some(){value.manifest.without_module_changes()}else{Ok(value.manifest.clone())}{Ok(v)=>v,Err(error)=>return Ok(ModuleApplyPrefixDecisionV1::Rejected{error:fee.retain(error),_facts:Some(facts)})};
 let proposal_hash=crate::world::incremental_normal_action_diplomacy::fixed_json_digest_v1(&value.manifest,common)?;
 pay(common,proposal_hash.len().min(approved_hash.len()))?;
 if proposal_hash.as_str()!=approved_hash{return Ok(ModuleApplyPrefixDecisionV1::Rejected{error:fee.retain(crate::WorldError::GovernanceFinalityInvalid{reason:"approved manifest hash drift".to_owned()}),_facts:Some(facts)})}
 let applied_hash=crate::world::incremental_normal_action_diplomacy::fixed_json_digest_v1(&applied_manifest,common)?;
 Ok(ModuleApplyPrefixDecisionV1::Ready(PreparedCurrentModuleApplyPrefixV1{proposal,applied_manifest,changes,applied_hash,epoch,facts,fee,guard:ctx.transaction.guard(),core:common.work().identity(),now}))
}

/// Only the fixed prefix plus the actual certificate-source verifier may mint
/// this owner. Neither a decoded certificate nor the prefix alone authorizes an
/// Apply mutation. The writer first checks the final strict cut, then retains
/// this owner as historical evidence while its exact Current CASs advance TX.
pub(super) struct ValidatedCurrentModuleApplyV1{
 pub(super) prefix:PreparedCurrentModuleApplyPrefixV1,
 pub(super) certificate:ReservedValue<crate::GovernanceFinalityCertificate>,
 pub(super) finality:crate::world::PaidCurrentModuleGovernanceFinalityV1,
 guard:PreparedGuard,core:WorkIdentityV1,now:u64,_fee:MemoryReservation,
 local:Option<crate::world::PaidCurrentLocalModuleGovernanceFinalityV1>,
}
impl ValidatedCurrentModuleApplyV1{
 pub(super) fn validate_current_v1<M:MerkleNodeReader>(&self,tx:&ExecutionTransaction<'_,M>,tail:&crate::world::incremental_normal_tail::NormalTailWorkV1<'_, '_>,now:u64,live:&ExecutionFence)->Result<(),BodyMutationError>{
 tx.validate_guard(&self.guard,live)?;self.validate_provenance_v1(tx,tail,now,live)
 }
 pub(super) fn validate_provenance_v1<M:MerkleNodeReader>(&self,tx:&ExecutionTransaction<'_,M>,tail:&crate::world::incremental_normal_tail::NormalTailWorkV1<'_, '_>,now:u64,live:&ExecutionFence)->Result<(),BodyMutationError>{
 tx.validate_transaction_identity(&self.guard)?;tx.validate_guard(&tx.guard(),live)?;
 self.prefix.proposal.validate_provenance(tx,tail.memory(),live).map_err(overlay_error)?;
 let proposal=self.prefix.proposal.value().ok_or_else(invalid)?;
 let oasis7_world_state::ProposalStatus::Approved{manifest_hash,..}=&proposal.status else{return Err(invalid())};
 if self.core!=tail.identity()||self.now!=now||!self._fee.uses_budget(tail.memory())||!self.certificate.uses_budget(tail.memory())||!self.prefix.fee.uses_budget(tail.memory())||!self.prefix.applied_hash.uses_budget(tail.memory()){return Err(invalid())}
 if let Some(local)=&self.local{local.validate_provenance(tx,tail.memory(),tail.identity(),now,self.prefix.proposal.original_id(),live)?;}
 self.finality.validate_provenance(tx,tail.memory(),tail.identity(),now,self.prefix.proposal.original_id(),manifest_hash,self.prefix.epoch,live)
 }
}

pub(super) enum CurrentModuleApplyValidationV1{
 Ready(ValidatedCurrentModuleApplyV1),
 Rejected{error:crate::WorldError,_fees:crate::world::ModuleCapabilityAdmissionFeesV1,_prefix:PreparedCurrentModuleApplyPrefixV1,_certificate:ReservedValue<crate::GovernanceFinalityCertificate>,_local:Option<crate::world::PaidCurrentLocalModuleGovernanceFinalityV1>},
}
struct CertificateCopyCauseV1{error:serde_json::Error,_fee:MemoryReservation}
impl std::fmt::Debug for CertificateCopyCauseV1{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{std::fmt::Debug::fmt(&self.error,f)}}
impl std::fmt::Display for CertificateCopyCauseV1{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{std::fmt::Display::fmt(&self.error,f)}}
impl std::error::Error for CertificateCopyCauseV1{}
#[allow(clippy::too_many_arguments)]
pub(super) fn validate_current_module_apply_v1<M,P,R,E,C,PE,PC>(
 prefix:PreparedCurrentModuleApplyPrefixV1,certificate:&crate::GovernanceFinalityCertificate,
 sources:&ModuleLifecycleSourcesV1<'_>,ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,R,PE,PC>,
 factory:&mut FactoryScheduleWorkV1,module:&mut ModuleLifecycleWorkV1<'_>,common:&mut FormalBoundedContextV1<'_>,now:u64,live:&ExecutionFence,
)->Result<CurrentModuleApplyValidationV1,BodyMutationError>
where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool{
 prefix.validate_strict_v1(ctx.transaction,common,now,live)?;
 let counted=match super::governance_json::module_json_value_paid_v1(certificate,common){Ok(v)=>v,Err(super::governance_json::ModuleGovernanceJsonErrorV1::Body(e))=>return Err(e),Err(super::governance_json::ModuleGovernanceJsonErrorV1::Json{error,_fee})=>return Err(BodyMutationError::Original(Box::new(CertificateCopyCauseV1{error,_fee})))};
 let bytes=counted.fee.bytes().checked_mul(2).and_then(|n|n.checked_add(std::mem::size_of::<crate::GovernanceFinalityCertificate>())).ok_or_else(invalid)?;
 pay(common,bytes)?;let certificate_fee=common.memory().reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let certificate=certificate_fee.retain(certificate.clone());drop(counted);
 let proposal=prefix.proposal.value().ok_or_else(invalid)?;
 let oasis7_world_state::ProposalStatus::Approved{manifest_hash,..}=&proposal.status else{return Err(invalid())};
 let finality=match crate::world::validate_current_module_governance_finality_v1(prefix.proposal.original_id(),manifest_hash,prefix.epoch,&certificate,sources,ctx,parent,factory,module,common,live){
 Ok(v)=>v,Err(crate::world::CurrentModuleGovernanceFinalityErrorV1::Body(e))=>return Err(e),
 Err(crate::world::CurrentModuleGovernanceFinalityErrorV1::World{error,fees})=>return Ok(CurrentModuleApplyValidationV1::Rejected{error,_fees:fees,_prefix:prefix,_certificate:certificate,_local:None}),};
 finality.validate_provenance(ctx.transaction,common.memory(),common.work().identity(),now,prefix.proposal.original_id(),manifest_hash,prefix.epoch,live)?;
 pay(common,std::mem::size_of::<ValidatedCurrentModuleApplyV1>())?;
 let fee=common.memory().reserve(std::mem::size_of::<ValidatedCurrentModuleApplyV1>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 Ok(CurrentModuleApplyValidationV1::Ready(ValidatedCurrentModuleApplyV1{prefix,certificate,finality,guard:ctx.transaction.guard(),core:common.work().identity(),now,_fee:fee,local:None}))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_current_local_module_apply_v1<M,P,R,E,C,PE,PC>(
 prefix:PreparedCurrentModuleApplyPrefixV1,local:crate::world::PaidCurrentLocalModuleGovernanceFinalityV1,
 sources:&ModuleLifecycleSourcesV1<'_>,ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,R,PE,PC>,
 factory:&mut FactoryScheduleWorkV1,module:&mut ModuleLifecycleWorkV1<'_>,common:&mut FormalBoundedContextV1<'_>,now:u64,live:&ExecutionFence,
)->Result<CurrentModuleApplyValidationV1,BodyMutationError>
where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool{
 local.validate_provenance(ctx.transaction,common.memory(),common.work().identity(),now,prefix.proposal.original_id(),live)?;
 let result=validate_current_module_apply_v1(prefix,local.certificate(),sources,ctx,parent,factory,module,common,now,live)?;
 Ok(match result{
 CurrentModuleApplyValidationV1::Ready(mut owner)=>{owner.local=Some(local);CurrentModuleApplyValidationV1::Ready(owner)},
 CurrentModuleApplyValidationV1::Rejected{error,_fees,_prefix,_certificate,..}=>CurrentModuleApplyValidationV1::Rejected{error,_fees,_prefix,_certificate,_local:Some(local)},
 })
}

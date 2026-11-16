//! Complete borrowed normal-action sources from one genuine bound parent.
//! This private assembler never opens Current business data before its action.
use crate::world::{
 incremental_body::BodyMutationError,
 incremental_normal_sources::BoundNormalTickParentV1,
 incremental_normal_action_work::NormalActionSourcesV1,
 incremental_normal_action_factory_schedule::FactoryScheduleSourcesV1,
 incremental_action_factory_lifecycle::FactoryLifecycleSourcesV1,
 incremental_action_crisis::CrisisSourcesV1,
 incremental_action_contract_accept::ContractAcceptSourcesV1,
 incremental_action_meta_progress::MetaProgressSourcesV1,
 incremental_normal_action_diplomacy::{AllianceSourcesV1,contracts::ContractSourcesV1},
 incremental_normal_action_profile::ProfileGovernanceSourcesV1,
 incremental_normal_action_module_lifecycle::ModuleLifecycleSourcesV1,
 incremental_module_invocation::{AcceptedInvocationDirectoryV1,RootModuleExecutionFamiliesV1},
 incremental_factory_directory_orders::AcceptedFactoryOrderDirectoryV1,
 incremental_authorization_root_current::CurrentAuthorizationProjectionV1,
 ModuleCapabilityAdmissionSourcesV1,
};
use oasis7_formal_protocol::{controlled_authority::formal::bounded_context::{FormalBoundedContextV1,WorkIdentityV1},incremental_value::ValueTreeError};
use oasis7_proto::memory_budget::{MemoryBudget,MemoryClass,MemoryReservation,ReservedValue};
use oasis7_world_state::{incremental_factory_schedule_index::FactoryScheduleIndexV1,incremental_factory_schedule_receipts::FactoryScheduleReceiptIndexV1,incremental_inventory::InventoryIdentityDirectoryV1};
/// Only the original opaque accepted producers may supply these mandatory
/// caches. A missing cache is an upstream construction obligation, never None.
pub(in crate::world) struct NormalActionParentCachesV1<'a>{
 pub builds:&'a ReservedValue<FactoryScheduleIndexV1>,
 pub receipts:&'a ReservedValue<FactoryScheduleReceiptIndexV1>,
 pub inventory_identity:&'a ReservedValue<InventoryIdentityDirectoryV1>,
 pub invocations:&'a ReservedValue<AcceptedInvocationDirectoryV1>,
 pub orders:&'a ReservedValue<AcceptedFactoryOrderDirectoryV1>,
}
pub(in crate::world) struct BoundNormalActionSourceParentsV1<'a,'parent>{parent:&'a BoundNormalTickParentV1<'parent>,cache:NormalActionParentCachesV1<'a>,core:WorkIdentityV1,memory:MemoryBudget}
fn bad()->BodyMutationError{BodyMutationError::Invalid("normal complete action source parent")}
fn pay(common:&mut FormalBoundedContextV1<'_>,steps:u64,bytes:usize)->Result<(),BodyMutationError>{common.charge(steps,u64::try_from(bytes).map_err(|_|bad())?).map_err(|e|BodyMutationError::Original(Box::new(e)))}
pub(in crate::world) fn bind_normal_action_source_parents_v1<'a,'parent>(parent:&'a BoundNormalTickParentV1<'parent>,cache:NormalActionParentCachesV1<'a>,common:&mut FormalBoundedContextV1<'_>)->Result<ReservedValue<BoundNormalActionSourceParentsV1<'a,'parent>>,BodyMutationError>{
 parent.validate_context(common).map_err(|e|BodyMutationError::Original(Box::new(e)))?;
 let refs=parent.refs();let current=refs.storage.current();let native=current.native();
 let steps=native.factory().record_counts()[1].checked_add(native.industry().record_counts()[2]).and_then(|n|n.checked_add(32)).ok_or_else(bad)?;
 let bytes=crate::world::incremental_normal_sources::root_bytes(refs.roots,refs.roots).map_err(|e|BodyMutationError::Original(Box::new(e)))?;
 let bytes=usize::try_from(bytes).map_err(|_|bad())?.checked_mul(4).and_then(|n|n.checked_add(std::mem::size_of::<BoundNormalActionSourceParentsV1<'_, '_>>())).ok_or_else(bad)?;
 pay(common,steps,bytes)?;
 if !cache.builds.uses_budget(common.memory())||!cache.receipts.uses_budget(common.memory())||!cache.inventory_identity.uses_budget(common.memory())||!cache.invocations.uses_budget(common.memory())||!cache.orders.uses_budget(common.memory()){return Err(bad())}
 cache.builds.validate_binding(native.factory(),refs.roots,common.memory(),MemoryClass::Ordinary)?;
 cache.receipts.validate_binding(native.industry(),refs.roots,common.memory(),MemoryClass::Ordinary)?;
 cache.inventory_identity.validate_binding(refs.roots)?;
 cache.invocations.validate_action_source_v1(&parent.fence(),common.memory())?;
 cache.orders.validate_action_source_v1(&parent.fence(),common.memory())?;
 let fee=common.memory().reserve(std::mem::size_of::<BoundNormalActionSourceParentsV1<'_, '_>>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 Ok(fee.retain(BoundNormalActionSourceParentsV1{parent,cache,core:common.work().identity(),memory:common.memory().clone()}))
}
impl BoundNormalActionSourceParentsV1<'_, '_>{
 pub(in crate::world) fn validate_tick_parent_v1(&self,parent:&BoundNormalTickParentV1<'_>,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{
  self.parent.validate_context(common).map_err(|error|BodyMutationError::Original(Box::new(error)))?;
  if !std::ptr::eq(self.parent,parent)||self.core!=common.work().identity()||!self.memory.same_pool(common.memory()){return Err(bad())}Ok(())
 }

 /// All nested references live solely in this Runtime callback. No caller can
 /// retain a self-referential view or use the callback to mint a dispatch cap.
 pub(in crate::world) fn with_sources_v1<T>(&self,authorization:&CurrentAuthorizationProjectionV1<'_>,compiler:&crate::module_source_compiler::RuntimeSourceCompiler,common:&mut FormalBoundedContextV1<'_>,consume:impl FnOnce(&NormalActionSourcesV1<'_>,&mut FormalBoundedContextV1<'_>)->T)->Result<T,BodyMutationError>{
  self.parent.validate_context(common).map_err(|e|BodyMutationError::Original(Box::new(e)))?;
  if self.core!=common.work().identity()||!self.memory.same_pool(common.memory()){return Err(bad())}
  let families=self.parent.module_execution_families_v1(common).map_err(|e|BodyMutationError::Original(Box::new(e)))?;
  let bytes=std::mem::size_of::<CrisisSourcesV1<'_>>()+std::mem::size_of::<ContractAcceptSourcesV1<'_>>()+std::mem::size_of::<MetaProgressSourcesV1<'_>>()+std::mem::size_of::<FactoryLifecycleSourcesV1<'_>>()+std::mem::size_of::<FactoryScheduleSourcesV1<'_>>()+std::mem::size_of::<ModuleCapabilityAdmissionSourcesV1<'_>>()+std::mem::size_of::<ModuleLifecycleSourcesV1<'_>>()+std::mem::size_of::<AllianceSourcesV1<'_, '_>>()+std::mem::size_of::<ContractSourcesV1<'_, '_>>()+std::mem::size_of::<ProfileGovernanceSourcesV1<'_>>()+std::mem::size_of::<NormalActionSourcesV1<'_>>();
  pay(common,16,bytes)?;let _layout=common.memory().reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let refs=self.parent.refs();let current=refs.storage.current();let native=current.native();let raw=current.raw();let runtime=current.runtime();
  let factory_lifecycle=||FactoryLifecycleSourcesV1{agents:raw.agents(),factories:native.factory(),industry:native.industry(),index:native.factory_lifecycle_index(),inventory:native.inventory(),primitive:native.primitive(),native:native.native()};
  let factory=factory_lifecycle();
  let schedule=FactoryScheduleSourcesV1{roots:refs.roots,lifecycle:factory_lifecycle(),builds:self.cache.builds,receipts:self.cache.receipts,inventory_directory:self.cache.inventory_identity,factory_authority:native.factory_authority(),agent_authority:native.agent_authority(),logistics:native.logistics(),material_profiles:native.material(),artifact_config:runtime.config(),module_instances:raw.instances(),invocation_directory:self.cache.invocations};
  let capability=ModuleCapabilityAdmissionSourcesV1{current:authorization,typed:runtime,governance:runtime.governance(),proposals:runtime.proposals(),native:native.native(),roots:refs.roots,legacy:runtime.module_authorization(),primitive:native.primitive()};
  let module=ModuleLifecycleSourcesV1{capability:&capability,controls:native.controls(),proposals:runtime.proposals(),families:RootModuleExecutionFamiliesV1{roots:families.roots,parent:families.parent,head:families.head,instances:families.instances,config:families.config,content:families.content,agents:families.agents,authorization:families.authorization,effects:families.effects,state:families.state,agent_authority:families.agent_authority,marketplace:families.marketplace},factory:&schedule,governance:runtime.governance(),compiler};
  let crisis=CrisisSourcesV1{agents:raw.agents(),diplomacy:native.diplomacy()};
  let contract=ContractAcceptSourcesV1{agents:raw.agents(),diplomacy:native.diplomacy()};
  let meta=MetaProgressSourcesV1{agents:raw.agents(),native:native.native()};
  let alliance=AllianceSourcesV1{agents:raw.agents(),diplomacy:native.diplomacy(),continuity:native.rules().continuity(),organizations:native.rules().organization(),native:native.native(),orders:self.cache.orders,rules:native.rules(),roots:refs.roots};
  let contracts=ContractSourcesV1{shared:&alliance,artifacts:runtime.resources()};
  let profile=ProfileGovernanceSourcesV1{agents:raw.agents(),native:native.native(),proposals:runtime.proposals(),materials:native.material()};
  let sources=NormalActionSourcesV1{module_lifecycle:&module,crisis_sources:&crisis,contract_sources:&contract,meta_sources:&meta,factory_sources:&factory,logistics:native.logistics(),materials:native.material(),controls:native.controls(),starter_bindings:native.native().starter_binding_index(),factory_schedule:&schedule,contracts:&contracts,alliance:&alliance,profile:&profile};
  Ok(consume(&sources,common))
 }
}

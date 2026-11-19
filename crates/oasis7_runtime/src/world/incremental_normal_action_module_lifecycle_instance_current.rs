//! Fixed Current observations for registry/manifest admission and M1 migration.
use super::*;
use super::{registry_rules::*,manifest_rules::*,m1_rules::*};
use crate::{ModuleChangeSet,ModuleLimits,ModuleManifest,WorldError};
use crate::world::{incremental_body::BodyMutationError,
    incremental_normal_action_factory_schedule::{FactoryScheduleReadErrorV1,with_module_lifecycle_read_io_v1},
    incremental_module_invocation::AcceptedInvocationDirectoryV1};
use oasis7_formal_protocol::{incremental_value::{ValueNodeReader,ValueReadContext},incremental_state::MerkleNodeReader};
use oasis7_world_state::{incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,incremental_runtime_artifact_config::*};
type Error=FactoryScheduleReadErrorV1;

/// Retained clone/error/output graph admission; move this owner into the final
/// Action source on both successful and rejected business results.
pub(in crate::world) struct ModuleInstanceFactsFeesV1 {
    capabilities:Vec<crate::world::ModuleCapabilityAdmissionFeesV1>,
    fees:Vec<MemoryReservation>,slots:MemoryReservation,
}
pub(in crate::world) struct CurrentModuleInstanceFactsV1<'a,'s,'t,'v,'m,'p,'b,'mw,M,P,R,E,C,PE,PC>{
    sources:&'a ModuleLifecycleSourcesV1<'s>,
    ctx:&'a mut WorldFieldOverlayContextV1<'t,'v,'m,M,R,E,C>,
    parent:&'a mut WorldFieldReadContextV1<'p,P,R,PE,PC>,
    factory:&'a mut FactoryScheduleWorkV1,module:&'a mut ModuleLifecycleWorkV1<'mw>,
    common:&'a mut FormalBoundedContextV1<'b>,live:&'a ExecutionFence,
    directory:&'a ReservedValue<AcceptedInvocationDirectoryV1>,held:ModuleInstanceFactsFeesV1,
}
impl<'a,'s,'t,'v,'m,'p,'b,'mw,M,P,R,E,C,PE,PC> CurrentModuleInstanceFactsV1<'a,'s,'t,'v,'m,'p,'b,'mw,M,P,R,E,C,PE,PC>
where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool {
    pub(super) fn common_v1(&mut self) -> &mut FormalBoundedContextV1<'b> { &mut *self.common }
    pub(super) fn world_manifest_v1(&mut self) -> Result<super::governance_manifest::CurrentModuleWorldManifestV1, Error> {
        with_module_lifecycle_read_io_v1(self.sources.families.roots,self.ctx,self.parent,self.factory,self.common,self.module,|ctx,parent,factory,module|Ok(super::governance_manifest::read_current_module_world_manifest_v1(self.sources,ctx,parent,factory,module,self.live)?))
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn new(sources:&'a ModuleLifecycleSourcesV1<'s>,ctx:&'a mut WorldFieldOverlayContextV1<'t,'v,'m,M,R,E,C>,parent:&'a mut WorldFieldReadContextV1<'p,P,R,PE,PC>,factory:&'a mut FactoryScheduleWorkV1,module:&'a mut ModuleLifecycleWorkV1<'mw>,common:&'a mut FormalBoundedContextV1<'b>,live:&'a ExecutionFence,directory:&'a ReservedValue<AcceptedInvocationDirectoryV1>)->Result<Self,Error>{
        if !ctx.values.memory.same_pool(common.memory()){return Err(invalid().into())}
        let slots=common.memory().reserve(32*std::mem::size_of::<MemoryReservation>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
        let capslots=common.memory().reserve(4*std::mem::size_of::<crate::world::ModuleCapabilityAdmissionFeesV1>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
        let mut fees=Vec::with_capacity(32);fees.push(capslots);
        Ok(Self{sources,ctx,parent,factory,module,common,live,directory,held:ModuleInstanceFactsFeesV1{fees,slots,capabilities:Vec::with_capacity(4)}})
    }
    pub(in crate::world) fn into_parts(self)->ModuleInstanceFactsFeesV1{self.held}
    fn hold(&mut self,fee:MemoryReservation)->Result<(),Error>{
        if self.held.fees.len()==self.held.fees.capacity(){
            let cap=self.held.fees.capacity().checked_mul(2).ok_or_else(invalid)?;
            let bytes=cap.checked_mul(std::mem::size_of::<MemoryReservation>()).ok_or_else(invalid)?;
            pay(self.common,bytes)?;
            let next=self.common.memory().reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
            self.held.fees.reserve_exact(cap-self.held.fees.len());
            if self.held.fees.capacity()>cap{return Err(invalid().into())}
            self.held.slots=next;
        }
        self.held.fees.push(fee);Ok(())
    }
    fn work(&mut self,bytes:usize)->Result<(),Error>{pay(self.common,bytes)?;Ok(())}
    fn retain(&mut self,bytes:usize)->Result<(),Error>{
        self.work(bytes)?;
        let fee=self.common.memory().reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;self.hold(fee)
    }
    fn selected(&mut self,key:&str)->Result<Option<ReservedValue<SelectedModuleManifestV1>>,Error>{
        let sources=self.sources;
        with_module_lifecycle_read_io_v1(sources.families.roots,self.ctx,self.parent,self.factory,self.common,self.module,|ctx,parent,factory,module|{
            let body=open_artifact_config_body_overlay_v1(ArtifactConfigSelectedBodyRequestV1{field:ArtifactConfigFieldV1::RegistryManifest,key:ArtifactConfigRecordKeyV1::Member(key),family:sources.families.config,parent_roots:sources.families.roots,current:None},parent.state,ctx,&mut factory.config_scan,&mut factory.config).map_err(overlay_error)?;
            let Some(body)=body else{return Ok(None)};
            let mut read=WorldFieldReadContextV1{root:body.root(),state:parent.state,
                values:ValueReadContext{source:ctx.values.source,memory:ctx.values.memory,class:ctx.values.class,io:&mut*ctx.values.io},key_work:&mut*ctx.key_work,semantic_bytes:ctx.semantic_bytes,record_bytes:ctx.record_bytes,take_state_error:&mut ctx.take_state_error,cancelled:&mut ctx.cancelled};
            Ok(Some(read_selected_module_manifest_v1(&body,&mut read,&mut factory.config,&mut factory.config_scan,&SelectedModuleManifestLimitsV1{heap_bytes:module.max_selected_heap,json_heap_bytes:module.max_selected_heap,max_collection_items:module.max_selected_heap as u64})?))
        })
    }
    fn active(&mut self,id:&str)->Result<Option<ReservedValue<String>>,Error>{
        let sources=self.sources;let live=self.live;
        with_module_lifecycle_read_io_v1(sources.families.roots,self.ctx,self.parent,self.factory,self.common,self.module,|ctx,parent,factory,module|{
            let body=open_artifact_config_body_overlay_v1(ArtifactConfigSelectedBodyRequestV1{field:ArtifactConfigFieldV1::RegistryActive,key:ArtifactConfigRecordKeyV1::Member(id),family:sources.families.config,parent_roots:sources.families.roots,current:None},parent.state,ctx,&mut factory.config_scan,&mut factory.config).map_err(overlay_error)?;
            let Some(body)=body else{return Ok(None)};
            let n=module.max_selected_heap;let fee=ctx.values.memory.reserve(n.checked_add(std::mem::size_of::<String>()).ok_or_else(invalid)?,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
            let mut out=Vec::with_capacity(n);let guard=ctx.transaction.guard();
            let value=read_qualified_artifact_config_overlay_v1(ArtifactConfigCellRequestV1{field:ArtifactConfigFieldV1::RegistryActive,key:ArtifactConfigRecordKeyV1::Member(id),address:address("active","value"),family:sources.families.config,qualification:&body,max_payload_bytes:n as u64},ctx,&guard,live,&mut out).map_err(overlay_error)?;
            if !matches!(*value,ArtifactConfigValueV1::Text{utf8_bytes}if utf8_bytes==out.len() as u64){return Err(invalid().into())}
            Ok(Some(fee.retain(String::from_utf8(out).map_err(|_|invalid())?)))
        })
    }
}
fn address(section:&'static str,column:&'static str)->ArtifactConfigAddressV1<'static>{ArtifactConfigAddressV1{section,ordinal:None,inner_ordinal:None,member:None,column}}
fn flatten<T>(result:Result<T,ModuleRegistryRuleErrorV1<Error>>)->Result<T,Error>{match result{Ok(v)=>Ok(v),Err(ModuleRegistryRuleErrorV1::World(e))=>Err(e.into()),Err(ModuleRegistryRuleErrorV1::Source(e))=>Err(e)}}
impl<M,P,R,E,C,PE,PC> manifest_rules::sealed::Sealed for CurrentModuleInstanceFactsV1<'_,'_,'_,'_,'_,'_,'_,'_,M,P,R,E,C,PE,PC>{}
impl<M,P,R,E,C,PE,PC> registry_rules::sealed::Sealed for CurrentModuleInstanceFactsV1<'_,'_,'_,'_,'_,'_,'_,'_,M,P,R,E,C,PE,PC>{}
impl<M,P,R,E,C,PE,PC> m1_rules::sealed::Sealed for CurrentModuleInstanceFactsV1<'_,'_,'_,'_,'_,'_,'_,'_,M,P,R,E,C,PE,PC>{}

impl<'s,M,P,R,E,C,PE,PC> M1StorageFactsV1<'s> for CurrentModuleInstanceFactsV1<'_,'_,'_,'_,'_,'_,'_,'_,M,P,R,E,C,PE,PC>
where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool {
    type Error=Error;
    fn keys(&mut self)->Result<ModuleStorageKeysV1<'s>,Error>{
        let sources=self.sources;let live=self.live;
        let keys=with_module_lifecycle_read_io_v1(sources.families.roots,self.ctx,self.parent,self.factory,self.common,self.module,|ctx,parent,_,module|{
            let guard=ctx.transaction.guard();Ok(crate::world::read_current_module_state_keys_v1(sources.families.state,sources.families.roots,parent.state,ctx,module.instance_scan,module.instance_cells,module.pending_entries,module.max_selected_heap,&guard,live)?)
        })?;Ok(ModuleStorageKeysV1::Current(keys))
    }
    fn bytes(&mut self,key:&str)->Result<ModuleStorageBytesV1<'s>,Error>{
        let sources=self.sources;
        with_module_lifecycle_read_io_v1(sources.families.roots,self.ctx,self.parent,self.factory,self.common,self.module,|ctx,_,_,module|{
            let mut count=ByteCount(0);let first=crate::world::incremental_module_state::read_iws_module_state(sources.families.state,key,ctx.transaction,&mut ctx.values,&mut ctx.cancelled,module.max_selected_heap as u64,&mut count)?;
            let Some(n)=first else{return Err(invalid().into())};
            let n=usize::try_from(n).map_err(|_|invalid())?;
            if n!=count.0{return Err(invalid().into())}
            let fee=ctx.values.memory.reserve(n.checked_add(std::mem::size_of::<Vec<u8>>()).ok_or_else(invalid)?,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
            let mut bytes=Vec::with_capacity(n);
            let second=crate::world::incremental_module_state::read_iws_module_state(sources.families.state,key,ctx.transaction,&mut ctx.values,&mut ctx.cancelled,module.max_selected_heap as u64,&mut bytes)?;
            if first!=second||bytes.len()!=n{return Err(invalid().into())}
            Ok(ModuleStorageBytesV1::Current(fee.retain(bytes)))
        })
    }
    fn instance_module(&mut self,key:&str)->Result<Option<ModuleTextRecordV1<'s>>,Error>{
        use oasis7_world_state::incremental_module_instance_fields::{ModuleInstanceFieldV1 as F,ModuleInstanceScalarRefV1 as S};
        let sources=self.sources;
        with_module_lifecycle_read_io_v1(sources.families.roots,self.ctx,self.parent,self.factory,self.common,self.module,|ctx,_,_,module|{
            let mut read=crate::world::incremental_module_read::ModuleReadContext{qualification:sources.families.instances,values:ctx.values.source,memory:ctx.values.memory,io:&mut*ctx.values.io,max_value_bytes:module.max_selected_heap as u64,cancelled:&mut ctx.cancelled};
            read.check(ctx.transaction)?;
            // The state-key family allows a few more UTF-8 bytes than the
            // independently closed instance-key family. Such a state key is a
            // genuine non-instance entry, as with the legacy map lookup.
            if oasis7_world_state::incremental_module_instance_fields::ModuleInstanceKeyV1::new(key,F::ModuleId).is_err(){return Ok(None)}
            let Some(record)=read.read_instance(key,ctx.transaction)? else{return Ok(None)};
            let S::Text(id)=record.field(F::ModuleId)else{return Err(invalid().into())};
            let n=id.len().checked_add(std::mem::size_of::<String>()).ok_or_else(invalid)?;
            ctx.key_work.bytes_remaining=ctx.key_work.bytes_remaining.checked_sub(n as u64).ok_or(ValueTreeError::ResourceLimited("M1 instance module copy"))?;
            let fee=ctx.values.memory.reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
            Ok(Some(ModuleTextRecordV1::Current(fee.retain(id.to_owned()))))
        })
    }
    fn position(&mut self,id:&str)->Result<Option<oasis7_agent_api::world_action::GeoPos>,Error>{
        use oasis7_world_state::{incremental_agent_subtree::*,incremental_agent_fields::AgentFieldV1 as F};
        let sources=self.sources;let live=self.live;
        with_module_lifecycle_read_io_v1(sources.families.roots,self.ctx,self.parent,self.factory,self.common,self.module,|ctx,parent,factory,_|{
            let guard=ctx.transaction.guard();
            if read_current_agent_presence_v1(id,sources.factory.lifecycle.agents,sources.families.roots,ctx,parent,&mut factory.lifecycle.pending_entries_remaining,&guard,live).map_err(overlay_error)?==CurrentAgentPresenceV1::Absent{return Ok(None)}
            let mut xyz=[0;3];
            for (n,field) in [F::PositionX,F::PositionY,F::PositionZ].into_iter().enumerate(){let guard=ctx.transaction.guard();let value=read_agent_subtree_scalar_overlay_v1(AgentSubtreeQualifiedRequestV1{cell:AgentSubtreeScalarRequestV1{agent:id,field,ordinal:0},qualification:sources.factory.lifecycle.agents},ctx,64,&mut std::io::sink(),&guard,live).map_err(overlay_error)?;
                let Some(AgentSubtreeValueV1::I64(value))=value else{return Err(invalid().into())};xyz[n]=value;
            }Ok(Some(oasis7_agent_api::world_action::GeoPos::new(xyz[0],xyz[1],xyz[2])))
        })
    }
    fn work(&mut self,bytes:usize)->Result<(),Error>{CurrentModuleInstanceFactsV1::work(self,bytes)}
    fn reserve_decode(&mut self,bytes:usize)->Result<Option<MemoryReservation>,Error>{
        let heap=bytes.checked_mul(1024).and_then(|n|n.checked_add(65536)).ok_or_else(invalid)?;
        self.common.charge(bytes as u64,heap as u64).map_err(|e|BodyMutationError::Original(Box::new(e)))?;
        Ok(Some(self.common.memory().reserve(heap,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?))
    }
}
struct ByteCount(usize);impl std::io::Write for ByteCount{fn write(&mut self,b:&[u8])->std::io::Result<usize>{self.0=self.0.checked_add(b.len()).ok_or_else(||std::io::Error::other("module state length"))?;Ok(b.len())}fn flush(&mut self)->std::io::Result<()>{Ok(())}}

impl<M,P,R,E,C,PE,PC> ModuleManifestFactsV1 for CurrentModuleInstanceFactsV1<'_,'_,'_,'_,'_,'_,'_,'_,M,P,R,E,C,PE,PC>
where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool {
    type Error=Error;
    fn reserve_manifest(&mut self,module:&ModuleManifest)->Result<(),Error>{
        // The existing exhaustive visitor measures all borrowed Text/JSON,
        // including declaration/subscription data before the fixed predicates.
        let mut bytes=std::mem::size_of::<ModuleManifest>()+16384;
        visit_artifact_module_manifest_v1(module,|_,payload|->Result<(),BodyMutationError>{
            let n=match payload{ArtifactConfigPayloadV1::Primitive(oasis7_world_state::incremental_field_member::MemberPayloadRefV1::Text(s))=>s.len(),ArtifactConfigPayloadV1::Json(v)=>{
                struct Count<'a,'b>{n:usize,common:&'a mut FormalBoundedContextV1<'b>,first:Option<BodyMutationError>}
                impl std::io::Write for Count<'_,'_>{fn write(&mut self,b:&[u8])->std::io::Result<usize>{
                    if let Err(e)=pay(self.common,b.len()){self.first=Some(e);return Err(std::io::Error::other("manifest work"))}
                    self.n=self.n.checked_add(b.len()).ok_or_else(||std::io::Error::other("manifest size"))?;Ok(b.len())
                }fn flush(&mut self)->std::io::Result<()>{Ok(())}}
                // Only traversal for accounting; actual JSON recovery remains
                // the original typed column codec.
                let mut count=Count{n:0,common:self.common,first:None};let encoded=serde_json::to_writer(&mut count,v);
                if let Some(error)=count.first{return Err(error)}encoded.map_err(|_|invalid())?;count.n
            },_=>64};
            bytes=bytes.checked_add(n.checked_mul(32).ok_or_else(invalid)?).ok_or_else(invalid)?;Ok(())
        })?;
        self.retain(bytes)
    }
    fn artifact_registered(&mut self,hash:&str)->Result<bool,Error>{
        let sources=self.sources;
        with_module_lifecycle_read_io_v1(sources.families.roots,self.ctx,self.parent,self.factory,self.common,self.module,|ctx,_,_,_|Ok(super::current::read_artifact_registered_v1(hash,ctx)?))
    }
    fn identity(&mut self,module:&ModuleManifest)->Result<(),Error>{
        crate::world::validate_current_module_artifact_identity_v1(module,self.sources,self.ctx,self.parent,self.factory,self.module,self.common,self.live)
    }
    fn abi(&mut self,module:&ModuleManifest)->Result<(),Error>{
        validate_module_abi_shape_v1(module)?;
        crate::world::World::validate_gameplay_contract_for_manifest_data_v1(module)?;Ok(())
    }
    fn limits(&mut self,id:&str,limits:&ModuleLimits)->Result<(),Error>{
        let sources=self.sources;let live=self.live;
        let max=with_module_lifecycle_read_io_v1(sources.families.roots,self.ctx,self.parent,self.factory,self.common,self.module,|ctx,parent,factory,_|{
            let body=open_artifact_config_body_overlay_v1(ArtifactConfigSelectedBodyRequestV1{field:ArtifactConfigFieldV1::Limits,key:ArtifactConfigRecordKeyV1::Global,family:sources.families.config,parent_roots:sources.families.roots,current:None},parent.state,ctx,&mut factory.config_scan,&mut factory.config).map_err(overlay_error)?.ok_or_else(invalid)?;
            let mut values=[0u64;6];
            for (n,column) in ["max_mem_bytes","max_gas","max_call_rate","max_output_bytes","max_effects","max_emits"].into_iter().enumerate(){
                let guard=ctx.transaction.guard();let value=read_qualified_artifact_config_overlay_v1(ArtifactConfigCellRequestV1{field:ArtifactConfigFieldV1::Limits,key:ArtifactConfigRecordKeyV1::Global,address:address("limits",column),family:sources.families.config,qualification:&body,max_payload_bytes:8},ctx,&guard,live,&mut std::io::sink()).map_err(overlay_error)?;
                let ArtifactConfigValueV1::Unsigned(value)=*value else{return Err(invalid().into())};values[n]=value;
            }
            Ok(ModuleLimits{max_mem_bytes:values[0],max_gas:values[1],max_call_rate:u32::try_from(values[2]).map_err(|_|invalid())?,max_output_bytes:values[3],max_effects:u32::try_from(values[4]).map_err(|_|invalid())?,max_emits:u32::try_from(values[5]).map_err(|_|invalid())?})
        })?;
        validate_module_limits_from_records_v1(id,limits,&max)?;Ok(())
    }
    fn capability(&mut self,cap:&str)->Result<(),Error>{
        // The capability family's fixed Current consumer preserves durable-v2
        // preference and every original issuer/revocation/parent-chain gate.
        let result=crate::world::validate_current_module_required_capability_v1(cap,self.sources,self.ctx,self.parent,self.factory,self.module,self.common,self.live)?;
        result.validate(self.ctx.transaction,self.common,self.live)?;
        let (result,fees)=result.into_parts();
        if self.held.capabilities.len()==self.held.capabilities.capacity(){
            let cap=self.held.capabilities.capacity().checked_mul(2).ok_or_else(invalid)?;
            self.retain(cap.checked_mul(std::mem::size_of::<crate::world::ModuleCapabilityAdmissionFeesV1>()).ok_or_else(invalid)?)?;
            self.held.capabilities.reserve_exact(cap-self.held.capabilities.len());
            if self.held.capabilities.capacity()>cap{return Err(invalid().into())}
        }
        self.held.capabilities.push(fees);result.map_err(Into::into)
    }
}

impl<'s,M,P,R,E,C,PE,PC> ModuleRegistryFactsV1<'s> for CurrentModuleInstanceFactsV1<'_,'_,'_,'_,'_,'_,'_,'_,M,P,R,E,C,PE,PC>
where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool {
    type Error=Error;
    fn reserve_changes(&mut self,changes:&ModuleChangeSet)->Result<(),Error>{
        let mut bytes=16384usize;
        for module in &changes.register{self.reserve_manifest(module)?;bytes=bytes.checked_add(module.module_id.len().checked_mul(32).ok_or_else(invalid)?).ok_or_else(invalid)?;}
        for upgrade in &changes.upgrade{self.reserve_manifest(&upgrade.manifest)?;for s in [&upgrade.module_id,&upgrade.from_version,&upgrade.to_version,&upgrade.wasm_hash]{bytes=bytes.checked_add(s.len().checked_mul(32).ok_or_else(invalid)?).ok_or_else(invalid)?;}}
        for activate in &changes.activate{bytes=bytes.checked_add((activate.module_id.len()+activate.version.len()).checked_mul(32).ok_or_else(invalid)?).ok_or_else(invalid)?;}
        for deactivate in &changes.deactivate{bytes=bytes.checked_add(deactivate.module_id.len().checked_mul(32).ok_or_else(invalid)?).ok_or_else(invalid)?;}
        self.retain(bytes)
    }
    fn admission(&mut self,module:&ModuleManifest)->Result<(),Error>{
        use oasis7_world_state::{incremental_world_native::{world_native_field_descriptors_v1,recover_current_module_admission_freeze_v1},
            incremental_field_member::FieldDestination};
        let key_bytes=module.module_id.len().checked_add(module.version.len()).and_then(|n|n.checked_add(module.wasm_hash.len())).and_then(|n|n.checked_mul(8)).and_then(|n|n.checked_add(16384)).ok_or_else(invalid)?;
        self.retain(key_bytes)?;
        let key=oasis7_world_state::state::ModuleAdmissionFreeze::key(&module.module_id,&module.version,&module.wasm_hash);
        self.common.consume_steps(24).map_err(|e|BodyMutationError::Original(Box::new(e)))?;
        let descriptor=world_native_field_descriptors_v1()?.into_iter().find(|d|d.name=="module_admission_freezes").ok_or_else(invalid)?;
        let FieldDestination::State(ns)=descriptor.destination else{return Err(invalid().into())};
        let sources=self.sources;let live=self.live;
        let freeze=with_module_lifecycle_read_io_v1(sources.families.roots,self.ctx,self.parent,self.factory,self.common,self.module,|ctx,parent,factory,_|{
            let mut read=WorldFieldReadContextV1{root:sources.families.roots.state_root(ns),state:parent.state,
                values:ValueReadContext{source:ctx.values.source,memory:ctx.values.memory,class:ctx.values.class,io:&mut*ctx.values.io},key_work:&mut*ctx.key_work,semantic_bytes:ctx.semantic_bytes,record_bytes:ctx.record_bytes,take_state_error:&mut ctx.take_state_error,cancelled:&mut ctx.cancelled};
            let guard=ctx.transaction.guard();
            Ok(recover_current_module_admission_freeze_v1(&key,sources.factory.lifecycle.native,sources.families.roots,ctx.transaction,&mut read,&mut factory.lifecycle.native_scan,&mut factory.lifecycle.native_read,&guard,live).map_err(overlay_error)?)
        })?;
        if let Some(marker)=freeze {let marker=marker.value().value();self.retain(marker.reason.len().checked_mul(4).and_then(|n|n.checked_add(key.len()+2048)).ok_or_else(invalid)?)?;return Err(WorldError::ModuleChangeInvalid{reason:format!("module admission frozen: {} scope={} rollback_proposal_id={}",marker.reason,key,marker.rollback_proposal_id)}.into())}
        Ok(())
    }
    fn storage_activation(&mut self,id:&str,version:&str)->Result<(),Error>{flatten(validate_m1_storage_activation_from_source_v1(self,id,version))}
    fn record_exists(&mut self,key:&str)->Result<bool,Error>{
        let sources=self.sources;
        with_module_lifecycle_read_io_v1(sources.families.roots,self.ctx,self.parent,self.factory,self.common,self.module,|ctx,parent,factory,_|{
            Ok(open_artifact_config_body_overlay_v1(ArtifactConfigSelectedBodyRequestV1{field:ArtifactConfigFieldV1::RegistryManifest,key:ArtifactConfigRecordKeyV1::Member(key),family:sources.families.config,parent_roots:sources.families.roots,current:None},parent.state,ctx,&mut factory.config_scan,&mut factory.config).map_err(overlay_error)?.is_some())
        })
    }
    fn manifest(&mut self,key:&str)->Result<Option<ModuleManifestRecordV1<'s>>,Error>{Ok(self.selected(key)?.map(ModuleManifestRecordV1::Current))}
    fn active_version(&mut self,id:&str)->Result<Option<ModuleTextRecordV1<'s>>,Error>{Ok(self.active(id)?.map(ModuleTextRecordV1::Current))}
    fn gameplay_conflicts(&mut self,changes:&ModuleChangeSet)->Result<(),Error>{self.current_gameplay_conflicts(changes)}
}

// The same closed resolver/order/slot predicate serves the legacy World and
// Current sources. No outside callback can provide an admission decision.
trait GameplayFacts<'s>{
    type Error;
    fn active_entries(&mut self)->Result<std::collections::BTreeMap<String,String>,Self::Error>;
    fn stored_manifest(&mut self,key:&str)->Result<Option<ModuleManifestRecordV1<'s>>,Self::Error>;
    fn output(&mut self,bytes:usize)->Result<(),Self::Error>;
}
fn gameplay<'s,S:GameplayFacts<'s>>(source:&mut S,changes:&ModuleChangeSet)->Result<(),ModuleRegistryRuleErrorV1<S::Error>>{
    let mut active=source.active_entries().map_err(ModuleRegistryRuleErrorV1::Source)?;
    for deactivation in &changes.deactivate{active.remove(&deactivation.module_id);}
    for upgrade in &changes.upgrade{
        source.output(upgrade.module_id.len()+upgrade.to_version.len()+256).map_err(ModuleRegistryRuleErrorV1::Source)?;
        if active.get(&upgrade.module_id).is_some_and(|version|version==&upgrade.from_version){active.insert(upgrade.module_id.clone(),upgrade.to_version.clone());}
    }
    for activation in &changes.activate{
        source.output(activation.module_id.len()+activation.version.len()+256).map_err(ModuleRegistryRuleErrorV1::Source)?;
        active.insert(activation.module_id.clone(),activation.version.clone());
    }
    let mut slots=std::collections::BTreeMap::<(String,oasis7_wasm_abi::GameplayModuleKind),String>::new();
    for (id,version) in &active{
        source.output((id.len()+version.len()).saturating_mul(4)+1024).map_err(ModuleRegistryRuleErrorV1::Source)?;
        let borrowed=changes.register.iter().find(|m|m.module_id==*id&&m.version==*version)
            .or_else(||changes.upgrade.iter().find(|u|u.module_id==*id&&u.to_version==*version).map(|u|&u.manifest));
        let stored;
        let manifest=if let Some(manifest)=borrowed{manifest}else{
            let key=crate::ModuleRegistry::record_key(id,version);
            stored=source.stored_manifest(&key).map_err(ModuleRegistryRuleErrorV1::Source)?;
            stored.as_ref().ok_or_else(||WorldError::ModuleChangeInvalid{reason:format!("module record missing for activation {key}")})?.manifest()
        };
        if manifest.role!=oasis7_wasm_abi::ModuleRole::Gameplay{continue}
        let Some(gameplay)=&manifest.abi_contract.gameplay else{continue};
        for mode in &gameplay.game_modes{
            source.output((mode.len()+id.len()).saturating_mul(8)+4096).map_err(ModuleRegistryRuleErrorV1::Source)?;
            let slot=(mode.clone(),gameplay.kind);
            match slots.get(&slot){
                Some(existing)if existing!=id=>return Err(WorldError::ModuleChangeInvalid{reason:format!("gameplay slot conflict mode='{}' kind='{}': {} vs {}",slot.0,slot.1.as_str(),existing,id)}.into()),
                Some(_)=>{},None=>{slots.insert(slot,id.clone());},
            }
        }
    }Ok(())
}
struct LegacyGameplay<'s>(&'s crate::world::World);
impl<'s> GameplayFacts<'s> for LegacyGameplay<'s>{
    type Error=WorldError;
    fn active_entries(&mut self)->Result<std::collections::BTreeMap<String,String>,WorldError>{Ok(self.0.module_registry.active.clone())}
    fn stored_manifest(&mut self,key:&str)->Result<Option<ModuleManifestRecordV1<'s>>,WorldError>{Ok(self.0.module_registry.records.get(key).map(|r|ModuleManifestRecordV1::Legacy(&r.manifest)))}
    fn output(&mut self,_:usize)->Result<(),WorldError>{Ok(())}
}
pub(in crate::world) fn validate_gameplay_activation_conflicts_from_legacy_v1(world:&crate::world::World,changes:&ModuleChangeSet)->Result<(),WorldError>{
    match gameplay(&mut LegacyGameplay(world),changes){Ok(v)=>Ok(v),Err(ModuleRegistryRuleErrorV1::World(e)|ModuleRegistryRuleErrorV1::Source(e))=>Err(e)}
}
impl<'s,M,P,R,E,C,PE,PC> GameplayFacts<'s> for CurrentModuleInstanceFactsV1<'_,'_,'_,'_,'_,'_,'_,'_,M,P,R,E,C,PE,PC>
where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool {
    type Error=Error;
    fn active_entries(&mut self)->Result<std::collections::BTreeMap<String,String>,Error>{
        let sources=self.sources;let live=self.live;let directory=self.directory;
        let ids=with_module_lifecycle_read_io_v1(sources.families.roots,self.ctx,self.parent,self.factory,self.common,self.module,|ctx,parent,factory,_|{
            let guard=ctx.transaction.guard();
            let keys=crate::world::incremental_module_invocation::current_registry_active_keys_v1(directory,sources.families.roots,parent.state,ctx,&mut factory.invocation,&guard,live).map_err(enumeration_error)?;
            let bytes=keys.len().checked_mul(std::mem::size_of::<ReservedValue<String>>()).ok_or_else(invalid)?;
            let fee=ctx.values.memory.reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
            let mut ids=Vec::with_capacity(keys.len());
            for key in keys.iter(){let guard=ctx.transaction.guard();ids.push(read_current_registry_active_id_v1(sources.families.config,key,ctx,factory.invocation.max_text_bytes,&guard,live).map_err(overlay_error)?);}
            Ok(fee.retain(ids))
        })?;
        let mut active=std::collections::BTreeMap::new();
        for id in ids.iter(){
            let Some(version)=self.active(id)?else{return Err(invalid().into())};
            let bytes=id.len().checked_add(version.len()).and_then(|n|n.checked_add(256)).ok_or_else(invalid)?;
            self.retain(bytes.checked_mul(8).ok_or_else(invalid)?)?;
            if active.insert(id.to_string(),version.to_string()).is_some(){return Err(invalid().into())}
        }Ok(active)
    }
    fn stored_manifest(&mut self,key:&str)->Result<Option<ModuleManifestRecordV1<'s>>,Error>{Ok(self.selected(key)?.map(ModuleManifestRecordV1::Current))}
    fn output(&mut self,bytes:usize)->Result<(),Error>{
        // Covers bounded BTree comparisons/copies (usize depth at most 64),
        // independently of the retained output allocation charged below.
        let work=bytes.checked_mul(256).ok_or_else(invalid)?;
        self.common.charge(256,work as u64).map_err(|e|BodyMutationError::Original(Box::new(e)))?;
        self.retain(bytes)
    }
}
impl<M,P,R,E,C,PE,PC> CurrentModuleInstanceFactsV1<'_,'_,'_,'_,'_,'_,'_,'_,M,P,R,E,C,PE,PC>
where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool {
    fn current_gameplay_conflicts(&mut self,changes:&ModuleChangeSet)->Result<(),Error>{flatten(gameplay(self,changes))}
}
fn enumeration_error(e:crate::world::incremental_module_invocation::CurrentInvocationEnumerationErrorV1)->BodyMutationError{
    use crate::world::incremental_module_invocation::CurrentInvocationEnumerationErrorV1 as E;
    match e{E::Execution(e)=>e.into(),E::Value(e)=>e.into(),E::Body(e)=>e,E::Overlay(e)=>overlay_error(e)}
}

impl<M,P,R,E,C,PE,PC> super::instance_rules::sealed::Sealed for CurrentModuleInstanceFactsV1<'_,'_,'_,'_,'_,'_,'_,'_,M,P,R,E,C,PE,PC>{}
impl<'s,M,P,R,E,C,PE,PC> super::instance_rules::ModuleInstallFactsV1<'s> for CurrentModuleInstanceFactsV1<'_,'_,'_,'_,'_,'_,'_,'_,M,P,R,E,C,PE,PC>
where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool {
    type Error=Error;
    fn reserve_install(&mut self,manifest:&ModuleManifest,actor:&str)->Result<(),Error>{
        <Self as ModuleManifestFactsV1>::reserve_manifest(self,manifest)?;
        self.retain(actor.len().checked_mul(32).and_then(|n|n.checked_add(4096)).ok_or_else(invalid)?)
    }
    fn frozen_admission(&mut self,manifest:&ModuleManifest)->Result<(),Error>{
        <Self as ModuleRegistryFactsV1<'s>>::admission(self,manifest)
    }
    fn agent_exists(&mut self,id:&str)->Result<bool,Error>{
        use oasis7_world_state::incremental_agent_subtree::{read_current_agent_presence_v1,CurrentAgentPresenceV1};
        let sources=self.sources;let live=self.live;
        with_module_lifecycle_read_io_v1(sources.families.roots,self.ctx,self.parent,self.factory,self.common,self.module,|ctx,parent,factory,_|{
            let guard=ctx.transaction.guard();
            Ok(read_current_agent_presence_v1(id,sources.factory.lifecycle.agents,sources.families.roots,ctx,parent,&mut factory.lifecycle.pending_entries_remaining,&guard,live).map_err(overlay_error)?==CurrentAgentPresenceV1::Existing)
        })
    }
    fn owner(&mut self,hash:&str)->Result<Option<String>,Error>{
        let sources=self.sources;
        let owner=with_module_lifecycle_read_io_v1(sources.families.roots,self.ctx,self.parent,self.factory,self.common,self.module,|ctx,_,_,module|{
            Ok(super::current::read_artifact_owner_v1(hash,ctx,module.max_selected_heap)?)
        })?;
        let (value,fee)=owner.into_value_v1();self.hold(fee)?;Ok(value)
    }
    fn balance(&mut self,id:&str,kind:oasis7_agent_api::world_action::ResourceKind)->Result<i64,Error>{
        let sources=self.sources;let live=self.live;
        with_module_lifecycle_read_io_v1(sources.families.roots,self.ctx,self.parent,self.factory,self.common,self.module,|ctx,_,_,_|{
            Ok(super::current::read_market_balance_v1(id,kind,sources,ctx,live)?)
        })
    }
    fn registry_manifest(&mut self,key:&str)->Result<Option<ModuleManifestRecordV1<'s>>,Error>{
        Ok(self.selected(key)?.map(ModuleManifestRecordV1::Current))
    }
    fn active_version(&mut self,id:&str)->Result<Option<ModuleTextRecordV1<'s>>,Error>{
        Ok(self.active(id)?.map(ModuleTextRecordV1::Current))
    }
}

impl<'s,M,P,R,E,C,PE,PC> super::upgrade_rules::ModuleUpgradeFactsV1<'s> for CurrentModuleInstanceFactsV1<'_,'_,'_,'_,'_,'_,'_,'_,M,P,R,E,C,PE,PC>
where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool {
 fn reserve_upgrade(&mut self,manifest:&ModuleManifest,actor:&str,instance_id:&str,from:&str)->Result<(),Error>{
   <Self as super::instance_rules::ModuleInstallFactsV1<'s>>::reserve_install(self,manifest,actor)?;
   let n=instance_id.len().checked_add(from.len()).and_then(|n|n.checked_mul(64)).and_then(|n|n.checked_add(16384)).ok_or_else(invalid)?;self.retain(n)
 }
 fn instance(&mut self,id:&str)->Result<Option<super::upgrade_rules::ModuleInstanceRecordV1<'s>>,Error>{
   let sources=self.sources;
   with_module_lifecycle_read_io_v1(sources.families.roots,self.ctx,self.parent,self.factory,self.common,self.module,|ctx,_,_,module|{
     let mut read=crate::world::incremental_module_read::ModuleReadContext{qualification:sources.families.instances,values:ctx.values.source,memory:ctx.values.memory,io:&mut*ctx.values.io,max_value_bytes:module.max_selected_heap as u64,cancelled:&mut ctx.cancelled};
     Ok(read.read_instance(id,ctx.transaction)?.map(super::upgrade_rules::ModuleInstanceRecordV1::Current))
   })
 }
 fn reserve_instance(&mut self,instance:&super::upgrade_rules::ModuleInstanceRecordV1<'_>)->Result<(),Error>{
   let n=instance.module_id().len().checked_add(instance.version().len()).and_then(|n|n.checked_add(instance.owner().len())).and_then(|n|n.checked_mul(64)).and_then(|n|n.checked_add(16384)).ok_or_else(invalid)?;self.retain(n)
 }
}

impl<'s,M,P,R,E,C,PE,PC> super::rollback_rules::ModuleRollbackFactsV1<'s> for CurrentModuleInstanceFactsV1<'_,'_,'_,'_,'_,'_,'_,'_,M,P,R,E,C,PE,PC>
where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool {
 fn reserve_rollback_query(&mut self,actor:&str,id:&str,target:&str)->Result<(),Error>{
   let n=actor.len().checked_add(id.len()).and_then(|n|n.checked_add(target.len())).and_then(|n|n.checked_mul(64)).and_then(|n|n.checked_add(16384)).ok_or_else(invalid)?;self.retain(n)
 }
 fn unique_applied_release(&mut self,module_id:&str,version:&str,hash:&str)->Result<Option<u64>,Error>{
   let directory=super::release_current::read_current_module_release_requests_v1(self.sources,self.ctx,self.parent,self.factory,self.module,self.common,self.live)?;
   let sources=self.sources;let live=self.live;
   with_module_lifecycle_read_io_v1(sources.families.roots,self.ctx,self.parent,self.factory,self.common,self.module,|ctx,_,_,module|{
     directory.validate(ctx.transaction,ctx.values.memory,live).map_err(overlay_error)?;
     enum Scan{Second,Value(ValueTreeError)}
     impl From<ValueTreeError> for Scan{fn from(e:ValueTreeError)->Self{Self::Value(e)}}
     let mut found=None;
     let mut compare=|a:&str,b:&str|->Result<bool,Scan>{
       let bytes=a.len().checked_add(b.len()).and_then(|n|u64::try_from(n).ok()).ok_or(Scan::Value(ValueTreeError::Invalid("release comparison size")))?;
       ctx.key_work.bytes_remaining=ctx.key_work.bytes_remaining.checked_sub(bytes).ok_or(Scan::Value(ValueTreeError::ResourceLimited("release comparison bytes")))?;Ok(a==b)
     };
     let result=directory.visit(module.instance_cells,&mut ctx.cancelled,&mut|_,row,_|{
       if row.status==oasis7_world_state::state::ModuleReleaseRequestStatus::Applied&&compare(&row.manifest.module_id,module_id)?&&compare(&row.manifest.version,version)?&&compare(&row.manifest.wasm_hash,hash)? {
         if found.is_some(){return Err(Scan::Second)}found=Some(row.request_id);
       }Ok(())
     });
     match result{Ok(())=>Ok(found),Err(Scan::Second)=>Ok(None),Err(Scan::Value(e))=>Err(e.into())}
   })
 }
}

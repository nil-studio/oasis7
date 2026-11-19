//! Unique catalog audience rules. Readonly source callbacks expose actual
//! records; the source cannot bypass any target/authority/binding predicate.
use crate::world::{World,WorldError};
use crate::capability_authorization::CapabilityAuthorityRecord;
use oasis7_world_state::ModuleInstanceState;
use oasis7_wasm_abi::CapabilityGrantV2;
use serde_json::Value;
pub(in crate::world)mod sealed{pub trait Sources{}}
pub(in crate::world)trait SourcesV1:sealed::Sources{
 type Error:From<WorldError>;
 fn instance<T>(&self,id:&str,visit:impl FnOnce(Option<&ModuleInstanceState>)->Result<T,Self::Error>)->Result<T,Self::Error>;
 fn authority<T>(&self,id:&str,visit:impl FnOnce(Option<&CapabilityAuthorityRecord>)->Result<T,Self::Error>)->Result<T,Self::Error>;
 fn chain_world<T>(&self,visit:impl FnOnce(&str)->Result<T,Self::Error>)->Result<T,Self::Error>;
 fn binding<T>(&self,visit:impl FnOnce(Option<&Value>)->Result<T,Self::Error>)->Result<T,Self::Error>;
 fn finality(&self,epoch:u64)->Result<(),Self::Error>;
 fn work(&self,s:u64,b:usize)->Result<(),Self::Error>;
}
fn denied(reason:&str)->WorldError{WorldError::CapabilityAuthorizationDenied{reason:reason.into()}}
pub(in crate::world)fn validate_from_sources_v1<S:SourcesV1>(source:&S,grant:&CapabilityGrantV2)->Result<(),S::Error>{
 let audience=&grant.audience;source.work(1,audience.target_kind.len())?;
 match audience.target_kind.as_str(){
  "world" if audience.target_id.is_none()=>{},
  "module_instance"=>{let target=audience.target_id.as_deref().ok_or_else(||denied("capability module-instance target id is required"))?;let live=source.instance(target,|value|{let Some(instance)=value else{return Ok(false)};source.work(1,instance.instance_id.len()+target.len()+instance.module_id.len()+grant.scope.module_id.len()+instance.module_version.len()+grant.scope.module_version.len())?;Ok(instance.active&&instance.instance_id==target&&instance.module_id==grant.scope.module_id&&instance.module_version==grant.scope.module_version)})?;if !live{return Err(denied("capability module-instance target is not live").into())}},
  _=>return Err(denied("capability audience target is not live").into())
 }
 source.authority(&grant.issuer.issuer_id,|value|{let authority=value.ok_or_else(||denied("capability audience authority is not installed"))?;source.work(1,authority.finality_status.len()+authority.branch_id.len()+audience.branch_id.len())?;if authority.finality_status!="finalized"||authority.branch_id!=audience.branch_id||authority.finality_epoch!=audience.finality_epoch{return Err(denied("capability audience is not bound to live finality").into())}Ok(())})?;
 source.chain_world(|world|{source.work(1,world.len()+audience.world_id.len())?;if world!="unbound"&&world!=audience.world_id{return Err(denied("capability audience world does not match live world").into())}Ok(())})?;
 source.binding(|binding|{if let Some(binding)=binding{source.work(3,audience.world_id.len()+audience.branch_id.len())?;if binding.get("world_id").and_then(Value::as_str)!=Some(audience.world_id.as_str())||binding.get("branch_id").and_then(Value::as_str)!=Some(audience.branch_id.as_str())||binding.get("finality_epoch").and_then(Value::as_u64)!=Some(audience.finality_epoch){return Err(denied("capability audience conflicts with Runtime binding").into())}}Ok(())})?;
 source.finality(audience.finality_epoch)
}
struct Legacy<'a>(&'a World);
impl sealed::Sources for Legacy<'_>{}
impl SourcesV1 for Legacy<'_>{
 type Error=WorldError;
 fn instance<T>(&self,id:&str,visit:impl FnOnce(Option<&ModuleInstanceState>)->Result<T,WorldError>)->Result<T,WorldError>{visit(self.0.state.module_instances.get(id))}
 fn authority<T>(&self,id:&str,visit:impl FnOnce(Option<&CapabilityAuthorityRecord>)->Result<T,WorldError>)->Result<T,WorldError>{visit(self.0.capability_revocation_state.authority_records.get(id))}
 fn chain_world<T>(&self,visit:impl FnOnce(&str)->Result<T,WorldError>)->Result<T,WorldError>{visit(&self.0.chain_resource_manifest.world_id)}
 fn binding<T>(&self,visit:impl FnOnce(Option<&Value>)->Result<T,WorldError>)->Result<T,WorldError>{visit(self.0.cognition.get("runtime_binding"))}
 fn work(&self,_:u64,_:usize)->Result<(),WorldError>{Ok(())}
 fn finality(&self,epoch:u64)->Result<(),WorldError>{if let Some(record)=self.0.tick_consensus_records.last(){let hash=record.block.block_hash();if record.certificate.block_hash!=hash||record.certificate.threshold==0{return Err(denied("capability live finality record is invalid"))}
   if let Some(chain_epoch)=record.block.header.chain_epoch&&chain_epoch<epoch{return Err(denied("capability finality epoch is ahead of live chain"))}}Ok(())}
}
pub(super)fn legacy(world:&World,grant:&CapabilityGrantV2)->Result<(),WorldError>{validate_from_sources_v1(&Legacy(world),grant)}

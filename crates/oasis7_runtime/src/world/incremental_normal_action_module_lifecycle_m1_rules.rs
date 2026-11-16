//! Original M1 storage historical-state admission. The Source is sealed to the
//! legacy actual World and fixed Current readers; keys are complete DATA only.
use super::*;
use super::registry_rules::{ModuleRegistryRuleErrorV1,ModuleTextRecordV1};
use oasis7_agent_api::world_action::GeoPos;
use std::collections::BTreeMap;
pub(in crate::world) mod sealed{pub trait Sealed{}}
pub(in crate::world) enum ModuleStorageKeysV1<'s>{Legacy(&'s BTreeMap<String,Vec<u8>>),Current(ReservedValue<crate::world::PaidCurrentModuleStateKeysV1>)}
pub(in crate::world) enum ModuleStorageBytesV1<'s>{Legacy(&'s[u8]),Current(ReservedValue<Vec<u8>>)}
impl ModuleStorageBytesV1<'_>{pub(in crate::world) fn bytes(&self)->&[u8]{match self{Self::Legacy(b)=>b,Self::Current(b)=>b}}}
pub(in crate::world) trait M1StorageFactsV1<'s>:sealed::Sealed{
 type Error;
 fn keys(&mut self)->Result<ModuleStorageKeysV1<'s>,Self::Error>;
 fn bytes(&mut self,key:&str)->Result<ModuleStorageBytesV1<'s>,Self::Error>;
 fn instance_module(&mut self,key:&str)->Result<Option<ModuleTextRecordV1<'s>>,Self::Error>;
 fn position(&mut self,id:&str)->Result<Option<GeoPos>,Self::Error>;
 fn work(&mut self,bytes:usize)->Result<(),Self::Error>;
 fn reserve_decode(&mut self,bytes:usize)->Result<Option<MemoryReservation>,Self::Error>;
}
pub(in crate::world) fn validate_m1_storage_activation_from_source_v1<'s,S:M1StorageFactsV1<'s>>(source:&mut S,id:&str,version:&str)->Result<(),ModuleRegistryRuleErrorV1<S::Error>>{
 if id!=crate::M1_STORAGE_POWER_MODULE_ID||version!=crate::M1_STORAGE_POWER_MODULE_VERSION{return Ok(())}
 let keys=source.keys().map_err(ModuleRegistryRuleErrorV1::Source)?;
 match keys {
  ModuleStorageKeysV1::Legacy(keys)=>for key in keys.keys(){validate_one(source,key)?},
  ModuleStorageKeysV1::Current(keys)=>for key in keys.keys(){validate_one(source,key)?},
 }Ok(())
}
fn validate_one<'s,S:M1StorageFactsV1<'s>>(source:&mut S,key:&str)->Result<(),ModuleRegistryRuleErrorV1<S::Error>>{
 source.work(key.len()).map_err(ModuleRegistryRuleErrorV1::Source)?;
 let belongs=if key==crate::M1_STORAGE_POWER_MODULE_ID{true}else{
  source.instance_module(key).map_err(ModuleRegistryRuleErrorV1::Source)?.is_some_and(|v|v.as_str()==crate::M1_STORAGE_POWER_MODULE_ID)
 };
 if !belongs{return Ok(())}
 let bytes=source.bytes(key).map_err(ModuleRegistryRuleErrorV1::Source)?;
 if bytes.bytes().is_empty(){return Ok(())}
 let _decode=source.reserve_decode(bytes.bytes().len()).map_err(ModuleRegistryRuleErrorV1::Source)?;
 // The original serde_json::Value visitor grows from actual decoded values;
 // it does not reserve CBOR size hints. All scratch/decoded graph peaks are
 // admitted by the concrete Current source before invoking this same decoder.
 let mut compatible=false;
 if let Ok(state)=serde_cbor::from_slice::<serde_json::Value>(bytes.bytes()) {
  let marker=state.get("last_settled_event_id");
  if marker.is_some_and(|value|value.is_null()||value.as_u64().is_some()) {
   if let Some(agents)=state.get("agents").and_then(serde_json::Value::as_object){
    compatible=true;
    for (id,battery) in agents {
     source.work(id.len()).map_err(ModuleRegistryRuleErrorV1::Source)?;
     let Some(position)=source.position(id).map_err(ModuleRegistryRuleErrorV1::Source)? else{compatible=false;break};
     let position_matches=battery.get("pos")==serde_json::to_value(position).ok().as_ref();
     let bounded_level=battery.get("level").and_then(serde_json::Value::as_i64).is_some_and(|level|(0..=12).contains(&level));
     if !(position_matches&&bounded_level){compatible=false;break}
    }
   }
  }
 }
 if !compatible{return Err(crate::WorldError::ModuleChangeInvalid{reason:"M1 storage 0.2.0 activation requires explicit migration of historical battery state".into()}.into())}
 Ok(())
}

pub(in crate::world) struct LegacyM1StorageFactsV1<'s>(&'s crate::world::World);
impl<'s> LegacyM1StorageFactsV1<'s>{pub(in crate::world) fn new(world:&'s crate::world::World)->Self{Self(world)}}
impl sealed::Sealed for LegacyM1StorageFactsV1<'_>{}
impl<'s> M1StorageFactsV1<'s> for LegacyM1StorageFactsV1<'s>{
 type Error=crate::WorldError;
 fn keys(&mut self)->Result<ModuleStorageKeysV1<'s>,Self::Error>{Ok(ModuleStorageKeysV1::Legacy(&self.0.state.module_states))}
 fn bytes(&mut self,key:&str)->Result<ModuleStorageBytesV1<'s>,Self::Error>{self.0.state.module_states.get(key).map(|b|ModuleStorageBytesV1::Legacy(b)).ok_or_else(||crate::WorldError::ModuleChangeInvalid{reason:"M1 storage state key closure".into()})}
 fn instance_module(&mut self,key:&str)->Result<Option<ModuleTextRecordV1<'s>>,Self::Error>{Ok(self.0.state.module_instances.get(key).map(|i|ModuleTextRecordV1::Legacy(&i.module_id)))}
 fn position(&mut self,id:&str)->Result<Option<GeoPos>,Self::Error>{Ok(self.0.state.agents.get(id).map(|a|a.state.pos))}
 fn work(&mut self,_:usize)->Result<(),Self::Error>{Ok(())}
 fn reserve_decode(&mut self,_:usize)->Result<Option<MemoryReservation>,Self::Error>{Ok(None)}
}

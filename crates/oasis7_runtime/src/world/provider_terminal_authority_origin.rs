//! The original local-origin probe. Every durable grant is decoded even when
//! a local marker or invocation already matched; malformed tails still fail.
use super::*;
use std::collections::BTreeMap;
pub(in crate::world) mod sealed{pub trait Sources{}}
pub(in crate::world) trait LocalOriginSourcesV1:sealed::Sources{
 type Error:From<WorldError>;
 fn marker_actor(&self)->Result<Option<&str>,Self::Error>;
 fn contexts(&self)->Result<&BTreeMap<String,crate::CapabilityInvocationContext>,Self::Error>;
 fn grants(&self)->Result<&BTreeMap<String,serde_json::Value>,Self::Error>;
 fn work(&self,s:u64,b:usize)->Result<(),Self::Error>;
 fn before_value<T:serde::Serialize>(&self,v:&T)->Result<(),Self::Error>;
}
pub(in crate::world) fn has_local_origin_from_sources_v1<S:LocalOriginSourcesV1>(source:&S,agent:&str)->Result<bool,S::Error>{
 let module=crate::world::local_test_provider_bootstrap::LOCAL_TEST_PROVIDER_MODULE_ID;
 let marker=source.marker_actor()?;source.work(1,marker.map_or(0,str::len)+agent.len())?;
 let mut local=marker==Some(agent);
 if !local{for context in source.contexts()?.values(){source.work(1,context.module_id.len()+module.len()+agent.len())?;if context.module_id==module&&matches!(&context.subject,CapabilitySubject::Agent{agent_id,..}if agent_id==agent){local=true;break}}}
 for encoded in source.grants()?.values(){
  source.before_value(encoded)?;
  let grant:CapabilityGrantV2=serde_json::from_value(encoded.clone()).map_err(|_|deny("durable provider grant malformed"))?;
  source.work(1,grant.issuer.issuer_id.len()+grant.scope.module_id.len()+agent.len())?;
  local|=grant.issuer.issuer_id==crate::world::local_test_provider_bootstrap::LOCAL_TEST_PROVIDER_ISSUER_ID&&grant.scope.module_id==module&&matches!(&grant.subject,CapabilitySubject::Agent{agent_id,..}if agent_id==agent);
 }
 Ok(local)
}
struct Legacy<'a>(&'a World);
impl sealed::Sources for Legacy<'_>{}
impl LocalOriginSourcesV1 for Legacy<'_>{
 type Error=WorldError;
 fn marker_actor(&self)->Result<Option<&str>,WorldError>{Ok(self.0.manifest.content.get("local_test_provider_authority").and_then(|v|v.get("actor")).and_then(serde_json::Value::as_str))}
 fn contexts(&self)->Result<&BTreeMap<String,crate::CapabilityInvocationContext>,WorldError>{Ok(&self.0.capability_invocation_contexts)}
 fn grants(&self)->Result<&BTreeMap<String,serde_json::Value>,WorldError>{Ok(&self.0.capability_grants_v2)}
 fn work(&self,_:u64,_:usize)->Result<(),WorldError>{Ok(())}
 fn before_value<T:serde::Serialize>(&self,_:&T)->Result<(),WorldError>{Ok(())}
}
pub(super) fn legacy(world:&World,agent:&str)->Result<bool,WorldError>{has_local_origin_from_sources_v1(&Legacy(world),agent)}

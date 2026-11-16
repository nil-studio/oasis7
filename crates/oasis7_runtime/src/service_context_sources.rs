//! Unique readonly provider selection/context rules. Sources are sealed DATA
//! readers; the outer error exclusively carries IO/work/cancellation failure.
use super::*;
use std::collections::BTreeMap;
pub(crate) mod sealed{pub trait Sources{}}
pub(crate) trait ServiceContextSourcesV1:sealed::Sources{
 type Error;
 fn contexts(&self)->Result<&BTreeMap<String,CapabilityInvocationContext>,Self::Error>;
 fn local_origin(&self,agent:&str)->Result<Result<bool,crate::WorldError>,Self::Error>;
 fn binding(&self)->Result<Result<RuntimeBindingV1,crate::WorldError>,Self::Error>;
 fn tick(&self)->u64;
 fn catalog(&self,agent:&str,presenter:CapabilityPresenter,nonce:String)->Result<Result<(CapabilityCatalogSnapshot,CapabilityInvocationContext),crate::WorldError>,Self::Error>;
 fn availability(&self,agent:&str)->Result<Result<crate::world::ProviderCapabilityAvailability,crate::WorldError>,Self::Error>;
 fn root(&self)->Result<String,Self::Error>;
 fn work(&self,steps:u64,bytes:usize)->Result<(),Self::Error>;
 fn before_value<T:serde::Serialize+?Sized>(&self,value:&T)->Result<(),Self::Error>;
 fn copy(&self,text:&str)->Result<String,Self::Error>;
 fn format(&self,args:std::fmt::Arguments<'_>)->Result<String,Self::Error>;
 fn digest(&self,domain:&str,value:&str)->Result<String,Self::Error>;
 fn payer(&self,owner:&str)->Result<serde_json::Value,Self::Error>;
}
pub(crate) fn optional_from_sources_v1<S:ServiceContextSourcesV1>(source:&S,agent:&str)->Result<Result<Option<WorldServiceAgentContext>,String>,S::Error>{
 let mut any=false;for context in source.contexts()?.values(){source.work(1,agent.len()+context.presenter.presenter_kind.len())?;if is_provider_context_for_agent(context,agent){any=true;break}}
 if !any{return match source.local_origin(agent)?{Err(error)=>Ok(Err(source.format(format_args!("{error:?}"))?)),Ok(true)=>Ok(Err(source.copy("canonical local provider invocation is missing or malformed")?)),Ok(false)=>Ok(Ok(None))}}
 match canonical_from_sources_v1(source,agent)?{Ok(context)=>Ok(Ok(Some(context))),Err(original)=>match source.availability(agent)?{Ok(crate::world::ProviderCapabilityAvailability::ExpiredOnly{..})=>Ok(Ok(None)),_=>Ok(Err(original))}}
}
pub(crate) fn canonical_from_sources_v1<S:ServiceContextSourcesV1>(source:&S,agent:&str)->Result<Result<WorldServiceAgentContext,String>,S::Error>{
 let binding=match source.binding()?{Ok(v)=>v,Err(error)=>return Ok(Err(source.format(format_args!("{error:?}"))?))};
 let context=match provider_from_sources_v1(source,&binding,agent,0)?{Ok(v)=>v,Err(error)=>return Ok(Err(error))};
 let CapabilitySubject::Agent{owner_binding,..}=&context.invocation.subject else{return Ok(Err(source.copy("canonical provider subject is not an Agent")?))};
 let root=source.root()?;
 let output=WorldServiceAgentContext{agent_id:source.copy(agent)?,capability_authorization_root:source.copy(&root)?,capability_snapshot_hash:source.digest("oasis7.runtime.manifest.v1",&root)?,authority_context_hash:source.digest("oasis7.runtime.authority-context.v1",&root)?,payer_binding:source.payer(owner_binding)?,capability_catalog:context.catalog,capability_invocation_context:context.invocation};
 Ok(Ok(output))
}
pub(crate) fn provider_from_sources_v1<S:ServiceContextSourcesV1>(source:&S,binding:&RuntimeBindingV1,agent:&str,sequence:u64)->Result<Result<ProviderCapabilityContext,String>,S::Error>{
 let mut found=None;for context in source.contexts()?.values(){source.work(1,agent.len()+context.audience.world_id.len()+binding.world_id.len()+context.audience.branch_id.len()+binding.branch_id.len())?;if is_provider_context_for_agent(context,agent)&&context.audience.world_id==binding.world_id&&context.audience.branch_id==binding.branch_id&&context.audience.finality_epoch==binding.finality_epoch{source.before_value(context)?;found=Some(context.clone());break}}
 let Some(invocation)=found else{return Ok(Err(source.format(format_args!("Runtime capability invocation context is unavailable for provider agent {agent}"))?))};
 source.before_value(&invocation.presenter)?;let presenter=invocation.presenter.clone();
 let nonce=source.format(format_args!("runtime-live:{}:{}:{}",agent,source.tick(),sequence))?;
 let(catalog,invocation)=match source.catalog(agent,presenter,nonce)?{Ok(v)=>v,Err(error)=>return Ok(Err(source.format(format_args!("Runtime capability context unavailable: {error:?}"))?))};
 source.work(4,catalog.world_id.len()+binding.world_id.len()+catalog.branch_id.len()+binding.branch_id.len())?;
 if catalog.world_id!=binding.world_id||catalog.branch_id!=binding.branch_id||catalog.finality_epoch!=binding.finality_epoch||catalog.logical_tick!=binding.base_tick{return Ok(Err(source.copy("Runtime capability context is not bound to the current cognition binding")?))}
 let Some(session)=invocation.presenter.session_id.as_deref().filter(|id|!id.trim().is_empty())else{return Ok(Err(source.format(format_args!("Runtime provider presenter session identity is unavailable for agent {agent}"))?))};
 let _session_id=source.copy(session)?;
 Ok(Ok(ProviderCapabilityContext{catalog,invocation}))
}
pub(super) struct Legacy<'a>(pub &'a World);
impl sealed::Sources for Legacy<'_>{}
impl ServiceContextSourcesV1 for Legacy<'_>{
 type Error=std::convert::Infallible;
 fn contexts(&self)->Result<&BTreeMap<String,CapabilityInvocationContext>,Self::Error>{Ok(self.0.capability_invocation_contexts())}
 fn local_origin(&self,a:&str)->Result<Result<bool,crate::WorldError>,Self::Error>{Ok(self.0.has_local_provider_origin(a))}
 fn binding(&self)->Result<Result<RuntimeBindingV1,crate::WorldError>,Self::Error>{Ok(self.0.current_cognition_runtime_binding())}
 fn tick(&self)->u64{self.0.state().time}
 fn catalog(&self,a:&str,p:CapabilityPresenter,n:String)->Result<Result<(CapabilityCatalogSnapshot,CapabilityInvocationContext),crate::WorldError>,Self::Error>{Ok(self.0.capability_context_for_agent(a,p,n))}
 fn availability(&self,a:&str)->Result<Result<crate::world::ProviderCapabilityAvailability,crate::WorldError>,Self::Error>{Ok(self.0.local_provider_capability_availability(a))}
 fn root(&self)->Result<String,Self::Error>{Ok(self.0.capability_authorization_root().to_owned())}
 fn work(&self,_:u64,_:usize)->Result<(),Self::Error>{Ok(())}
 fn before_value<T:serde::Serialize+?Sized>(&self,_:&T)->Result<(),Self::Error>{Ok(())}
 fn copy(&self,s:&str)->Result<String,Self::Error>{Ok(s.to_owned())}
 fn format(&self,a:std::fmt::Arguments<'_>)->Result<String,Self::Error>{Ok(format!("{a}"))}
 fn digest(&self,d:&str,s:&str)->Result<String,Self::Error>{Ok(h_v1(d,&s).0)}
 fn payer(&self,s:&str)->Result<serde_json::Value,Self::Error>{Ok(serde_json::json!({"owner_binding":s}))}
}

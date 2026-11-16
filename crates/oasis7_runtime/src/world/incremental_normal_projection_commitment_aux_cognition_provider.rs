//! Fixed Current implementation of the original provider catalog and terminal
//! rules. Lazy graphs are opened at their original business access points.
use super::*;
use std::cell::OnceCell;
use oasis7_agent_api::{CapabilityInvocationContext,RuntimeBindingV1};
use oasis7_wasm_abi::{CapabilityGrantV2,CapabilityCatalogSnapshot,CapabilityPresenter,CapabilityCanonicalWorkV1,CanonicalCborAllocator,CanonicalCborStreamError};
use crate::world::{capability_catalog::rules as catalog,provider_terminal_authority_rules as terminal,incremental_module_capability_admission_rules as admission};
use crate::service_context::sources::{self,ServiceContextSourcesV1};
#[path="incremental_normal_projection_commitment_aux_cognition_provider_ops.rs"]mod ops;
#[path="incremental_normal_projection_commitment_aux_cognition_provider_admission.rs"]mod issuer;
#[derive(Debug)]enum Error{World(crate::WorldError),PaidWorld(ReservedValue<crate::WorldError>),Body(BodyMutationError)}
impl From<crate::WorldError>for Error{fn from(e:crate::WorldError)->Self{Self::World(e)}}
impl From<BodyMutationError>for Error{fn from(e:BodyMutationError)->Self{Self::Body(e)}}
fn deny(text:impl Into<String>)->crate::WorldError{crate::WorldError::CapabilityAuthorizationDenied{reason:text.into()}}
struct Provider<'v,'f,'a,'s,'c,'k,M>{
 view:&'v values::View<'f,'a,'s,'c,M>,binding:Option<&'v ReservedValue<RuntimeBindingV1>>,canonical:RefCell<&'k mut oasis7_wasm_abi::CanonicalCborWorkV1>,first:RefCell<Option<BodyMutationError>>,
 contexts:OnceCell<BTreeMap<String,CapabilityInvocationContext>>,grants:OnceCell<BTreeMap<String,serde_json::Value>>,registry:OnceCell<crate::modules::ModuleRegistry>,policies:OnceCell<oasis7_world_state::policy::PolicySet>,
 production_hardened:bool,allow_local:bool,held_errors:RefCell<Vec<ReservedValue<crate::WorldError>>>,
}
impl<M:MerkleNodeReader>Provider<'_, '_, '_, '_, '_, '_,M>{
 fn check(&self)->Result<(),BodyMutationError>{if let Some(e)=self.first.borrow_mut().take(){return Err(e)}self.view.facts.check()}
 fn split<T>(&self,value:Result<T,Error>)->Result<Result<T,crate::WorldError>,BodyMutationError>{self.check()?;match value{Ok(v)=>Ok(Ok(v)),Err(Error::World(e))=>Ok(Err(e)),Err(Error::PaidWorld(e))=>{let crate::WorldError::CapabilityAuthorizationDenied{reason}=&*e else{return Err(invalid("actual catalog finality error kind"))};let reason=self.view.copy(reason)?;self.view.facts.before_owned(size_of::<crate::WorldError>())?;values::push(self.view.facts,&mut self.held_errors.borrow_mut(),e)?;Ok(Err(deny(reason)))},Err(Error::Body(e))=>Err(e)}}
 fn load<'b,T:serde::de::DeserializeOwned>(&'b self,slot:&'b OnceCell<T>,path:&[&str])->Result<&'b T,BodyMutationError>{if slot.get().is_none(){let value=self.view.typed(path)?;slot.set(value).map_err(|_|invalid("Cognition provider typed source reentry"))?}slot.get().ok_or_else(||invalid("Cognition provider typed source missing"))}
 fn contexts_data(&self)->Result<&BTreeMap<String,CapabilityInvocationContext>,BodyMutationError>{self.load(&self.contexts,&["capability_invocation_contexts"])}
 fn grants_data(&self)->Result<&BTreeMap<String,serde_json::Value>,BodyMutationError>{self.load(&self.grants,&["capability_grants_v2"])}
 fn registry_data(&self)->Result<&crate::modules::ModuleRegistry,BodyMutationError>{self.load(&self.registry,&["module_registry"])}
 fn policies_data(&self)->Result<&oasis7_world_state::policy::PolicySet,BodyMutationError>{self.load(&self.policies,&["policies"])}
 fn note(&self,args:std::fmt::Arguments<'_>)->Result<String,BodyMutationError>{oasis7_world_state::world_rules::format_readonly_projection_data_v1(args,&mut|s,b,h|{self.view.facts.work(s,b)?;if h>0{self.view.facts.before_owned(h)?}Ok(())})}
 fn selected<T:serde::de::DeserializeOwned>(&self,path:&[&str],id:&str)->Result<Option<T>,BodyMutationError>{let parent=self.view.required(path)?;let Some(row)=self.view.member_like_json_get(&parent,id)?else{return Ok(None)};self.view.decode(&row)?.map(Some).map_err(|_|invalid("Cognition provider actual selected codec"))}
 fn native<T:serde::de::DeserializeOwned,K:oasis7_world_state::incremental_semantic_key::SemanticKeyV1+?Sized>(&self,field:&str,key:&K)->Result<Option<T>,BodyMutationError>{match self.view.facts.point(field,key)?{None=>Ok(None),Some(row)=>self.view.decode_original(self.view.facts.raw(row)?).map(Some)}}
 fn scalar<T:serde::de::DeserializeOwned>(&self,field:&str)->Result<T,BodyMutationError>{let raw=self.view.facts.columns.scalar_original_json_data_v1(field,&mut self.view.facts.common.borrow_mut())?;self.view.decode_original(raw)}
 fn epoch(&self)->Result<u64,BodyMutationError>{let policy:crate::governance::GovernanceExecutionPolicy=self.view.typed(&["governance_execution_policy"])?;Ok(self.view.facts.clock/policy.epoch_length_ticks.max(1))}
 fn retain<T>(&self,value:T)->Result<ReservedValue<T>,BodyMutationError>{self.view.facts.work(1,size_of::<T>())?;Ok(self.view.facts.common.borrow().memory().reserve(size_of::<T>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?.retain(value))}
}
impl<M:MerkleNodeReader>sources::sealed::Sources for Provider<'_, '_, '_, '_, '_, '_,M>{}
impl<M:MerkleNodeReader>ServiceContextSourcesV1 for Provider<'_, '_, '_, '_, '_, '_,M>{
 type Error=BodyMutationError;
 fn contexts(&self)->Result<&BTreeMap<String,CapabilityInvocationContext>,Self::Error>{self.contexts_data()}
 fn local_origin(&self,agent:&str)->Result<Result<bool,crate::WorldError>,Self::Error>{self.split(issuer::local_origin(self,agent))}
 fn binding(&self)->Result<Result<RuntimeBindingV1,crate::WorldError>,Self::Error>{let binding=self.binding.ok_or_else(||invalid("Cognition observation actual Runtime binding missing"))?;if !binding.uses_budget(self.view.facts.common.borrow().memory()){return Err(invalid("Cognition observation Runtime binding pool"))}self.view.before_clone(&**binding)?;Ok(Ok((**binding).clone()))}
 fn tick(&self)->u64{self.view.facts.clock}
 fn catalog(&self,a:&str,p:CapabilityPresenter,n:String)->Result<Result<(CapabilityCatalogSnapshot,CapabilityInvocationContext),crate::WorldError>,Self::Error>{self.view.facts.before_owned(8192)?;self.split(catalog::capability_context_from_sources_v1(self,&mut ops::Ops(self),a,p,n))}
 fn availability(&self,a:&str)->Result<Result<crate::world::ProviderCapabilityAvailability,crate::WorldError>,Self::Error>{self.view.facts.before_owned(8192)?;self.split(terminal::local_provider_capability_availability_from_sources_v1(self,&mut ops::Ops(self),a))}
 fn root(&self)->Result<String,Self::Error>{self.view.typed(&["capability_authorization_root"])}
 fn work(&self,s:u64,b:usize)->Result<(),Self::Error>{self.view.facts.work(s,b)}
 fn before_value<T:serde::Serialize+?Sized>(&self,v:&T)->Result<(),Self::Error>{self.view.before_clone(v)}
 fn copy(&self,s:&str)->Result<String,Self::Error>{self.view.copy(s)}
 fn format(&self,a:std::fmt::Arguments<'_>)->Result<String,Self::Error>{self.note(a)}
 fn digest(&self,d:&str,v:&str)->Result<String,Self::Error>{let digest=oasis7_agent_api::world_cognition::cognition_digest_with_work_v1(d,&v,self);self.check()?;let digest=digest.map_err(paid)?;self.view.copy(digest.as_str())}
 fn payer(&self,owner:&str)->Result<serde_json::Value,Self::Error>{self.view.facts.before_owned(11*(size_of::<String>()+size_of::<serde_json::Value>())+owner.len()+256)?;self.view.facts.work(1,owner.len())?;Ok(serde_json::json!({"owner_binding":owner}))}
}
pub(super) fn optional_context<M:MerkleNodeReader>(view:&values::View<'_, '_, '_, '_,M>,binding:Option<&ReservedValue<RuntimeBindingV1>>,agent:&str,canonical:&mut oasis7_wasm_abi::CanonicalCborWorkV1)->Result<Option<WorldServiceAgentContext>,BodyMutationError>{
 let process=view.facts.process_facts_v1()?;
 let source=Provider{view,binding,canonical:RefCell::new(canonical),first:RefCell::new(None),contexts:OnceCell::new(),grants:OnceCell::new(),registry:OnceCell::new(),policies:OnceCell::new(),production_hardened:process.production_hardened(),allow_local:process.allow_local_finality_signing(),held_errors:RefCell::new(Vec::new())};
 let result=sources::optional_from_sources_v1(&source,agent)?;source.check()?;match result{Ok(v)=>Ok(v),Err(note)=>Err(view.business(format_args!("{note}")))}
}

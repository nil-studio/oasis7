//! Fixed recipe observations over the genuine readonly projection columns.
//! These DTOs feed the original evaluator and confer no execution authority.
use super::*;
use crate::world::event_processing::{factory_schedule_fixed as fixed,factory_schedule_market_fixed as market,factory_schedule_product_fixed as product};
use fixed::{FactoryRecipeFactsV1,FactoryScheduleRecordV1,RecipeFactoryV1};
use oasis7_world_state::{MaterialLedgerId,MaterialStack,gameplay_capability::GameplayCapabilityReadonlyFactsV1};
use std::{borrow::Cow,ops::ControlFlow};
pub(super) struct RecipeFacts<'v,'f,'a,'s,'c,M>(pub(super) &'v values::View<'f,'a,'s,'c,M>);
fn limited()->BodyMutationError{ValueTreeError::ResourceLimited("Current native recipe facts").into()}
impl<M:MerkleNodeReader> RecipeFacts<'_, '_, '_, '_, '_,M>{
 pub(super) fn eval(&mut self,action_id:crate::ActionId,action:&crate::Action)->Result<crate::WorldEventBody,NativeError>{
  self.0.facts.check()?;let result=match action{crate::Action::ScheduleRecipe{requester_agent_id,factory_id,recipe_id,plan,logistics_route_ids,logistics_path_ids}=>fixed::evaluate_schedule_recipe_v1(self,action_id,requester_agent_id,factory_id,recipe_id,plan,logistics_route_ids,logistics_path_ids),_=>Err(invalid("native recipe evaluator action binding").into())};
  if matches!(&result,Err(NativeError::Body(_))){return result}self.0.facts.check()?;result
 }
 fn charge(&self,n:usize)->Result<(),NativeError>{self.0.facts.work(1,n)?;Ok(())}
 fn ownership(&self,n:usize)->Result<(),NativeError>{self.0.facts.before_owned(n)?;Ok(())}
 fn quantity(&self,ledger:&MaterialLedgerId,kind:&str)->Result<i64,NativeError>{self.charge(kind.len())?;let Some(group)=self.0.facts.point("material_ledgers",ledger)?else{return Ok(0)};let rows=group.nested_rows().ok_or_else(||invalid("actual material ledger row kind"))?;let semantic=self.0.facts.key(kind)?;let row=rows.row_data_v1(&semantic,&mut self.0.facts.common.borrow_mut())?;match row{None=>Ok(0),Some(row)=>Ok(super::super::super::super::fact_decode::with_i64_data_v1(self.0.facts.raw(row)?,&self.0.facts.common,|value|Ok(*value))?)}}
 fn original_string(&self,row:&ProjectionColumnRefV1<'_>)->Result<String,NativeError>{self.0.decode(row)?.map_err(|_|NativeError::Body(invalid("actual native recipe String codec")))}
}
impl<M> fixed::sealed::Sealed for RecipeFacts<'_, '_, '_, '_, '_,M>{}
impl<'source,M:MerkleNodeReader> FactoryRecipeFactsV1<'source> for RecipeFacts<'_, '_, '_, '_, '_,M>{
 type Error=NativeError;
 fn charge(&mut self,n:usize)->Result<(),NativeError>{RecipeFacts::charge(self,n)}
 fn recipe_work(&mut self,s:u64,b:usize,h:usize)->Result<(),NativeError>{self.0.facts.work(s,b)?;if h>0{self.0.facts.before_owned(h)?;}Ok(())}
 fn reserve_recipe_output(&mut self,requester:&str,factory:&str,recipe:&str,plan:&oasis7_wasm_abi::RecipeExecutionPlan,routes:&[String],paths:&[String])->Result<(),NativeError>{self.0.before_clone(&(requester,factory,recipe,plan,routes,paths))?;self.ownership(65536)?;Ok(())}
 fn now(&self)->u64{self.0.facts.clock}
 fn agent_exists(&mut self,id:&str)->Result<bool,NativeError>{Ok(self.0.facts.owner_agent_present_data(id)?)}
 fn factory(&mut self,id:&str)->Result<Option<RecipeFactoryV1<'source>>,NativeError>{self.charge(id.len())?;let Some(row)=self.0.facts.point("factories",id)?else{return Ok(None)};let value=super::super::super::super::fact_decode::owned_factory_data_v1(self.0.facts.raw(row)?,&self.0.facts.common)?;self.0.before_clone(&*value)?;Ok(Some(RecipeFactoryV1::Projection(value)))}
 fn pending_recipes_for_factory(&mut self,id:&str)->Result<usize,NativeError>{self.charge(id.len())?;let mut count=0usize;self.0.facts.quote_visit_jobs_data(|job|{self.0.facts.work(1,job.factory_id.len().checked_add(id.len()).ok_or_else(limited)?)?;if job.factory_id==id{count=count.checked_add(1).ok_or_else(limited)?;}Ok(())})?;Ok(count)}
 fn output_stack_rejection(&mut self,plan:&oasis7_wasm_abi::RecipeExecutionPlan)->Result<Option<crate::RejectReason>,NativeError>{product::recipe_output_stack_limit_rejection_v1(self,plan)}
 fn stage(&mut self)->Result<crate::IndustryStage,NativeError>{Ok(self.0.facts.with_industry_progress(|value|Ok(value.stage))?)}
 fn recipe_profile(&mut self,id:&str)->Result<Option<FactoryScheduleRecordV1<'source,crate::RecipeProfileV1>>,NativeError>{self.charge(id.len())?;let Some(row)=self.0.facts.point("recipe_profiles",id)?else{return Ok(None)};let value=super::super::super::super::fact_decode::owned_recipe_profile_data_v1(self.0.facts.raw(row)?,&self.0.facts.common)?;self.0.before_clone(&*value)?;Ok(Some(FactoryScheduleRecordV1::Current(value)))}
 fn product_profile(&mut self,id:&str)->Result<Option<FactoryScheduleRecordV1<'source,crate::ProductProfileV1>>,NativeError>{self.charge(id.len())?;let Some(row)=self.0.facts.point("product_profiles",id)?else{return Ok(None)};let value=super::super::super::super::fact_decode::owned_product_profile_data_v1(self.0.facts.raw(row)?,&self.0.facts.common)?;self.0.before_clone(&*value)?;Ok(Some(FactoryScheduleRecordV1::Current(value)))}
 fn effective_consume(&mut self,consume:&[MaterialStack],produce:&[MaterialStack])->Result<Vec<MaterialStack>,NativeError>{let source=RefCell::new(self);oasis7_world_state::state::apply_domain_event_industry_helpers::recipe_consume_with_maintenance_from_profiles_v1(consume,produce,&mut|kind|FactoryRecipeFactsV1::product_profile(&mut**source.borrow_mut(),kind),&mut|n|{let mut borrowed=source.borrow_mut();let source=&mut**borrowed;RecipeFacts::charge(source,n)?;RecipeFacts::ownership(source,n)})}
 fn route(&mut self,id:&str)->Result<Option<FactoryScheduleRecordV1<'source,oasis7_world_state::LogisticsRouteV1>>,NativeError>{self.charge(id.len())?;let Some(row)=self.0.facts.point("logistics_routes",id)?else{return Ok(None)};let value=super::super::super::super::fact_decode::owned_route_data_v1(self.0.facts.raw(row)?,&self.0.facts.common)?;self.0.before_clone(&*value)?;Ok(Some(FactoryScheduleRecordV1::Current(value)))}
 fn route_completed(&mut self,id:&str)->Result<bool,NativeError>{self.charge(id.len())?;Ok(self.0.facts.point("completed_logistics_route_ids",id)?.is_some())}
 fn path_id(&mut self,routes:&[String])->Result<Option<String>,NativeError>{self.0.before_clone(routes)?;if routes.is_empty(){Ok(None)}else{Err(invalid("native builtin quote has no nonempty route-plan source").into())}}
 fn allocate_paths(&mut self,_ledger:&MaterialLedgerId,routes:&[String],paths:&[String],consume:&[MaterialStack])->Result<Result<(),String>,NativeError>{self.0.before_clone(&(routes,paths,consume))?;if routes.is_empty()&&paths.is_empty(){Ok(Ok(()))}else{Err(invalid("native builtin quote has no nonempty path-allocation source").into())}}
 fn material(&mut self,ledger:&MaterialLedgerId,kind:&str)->Result<i64,NativeError>{self.quantity(ledger,kind)}
 fn electricity(&mut self,id:&str)->Result<i64,NativeError>{Ok(self.0.facts.quote_agent_electricity_data(id)?.unwrap_or(0))}
 fn market_quotes(&mut self,ledger:&MaterialLedgerId,consume:&[MaterialStack])->Result<Vec<oasis7_world_state::MaterialMarketQuote>,NativeError>{market::market_quotes_from_facts_v1(self,ledger,consume)}
 fn bottleneck_tags(&mut self,profile:Option<&crate::RecipeProfileV1>,consume:&[MaterialStack])->Result<Vec<String>,NativeError>{fixed::resolve_recipe_bottleneck_tags_with_work_v1(profile,consume,&mut|s,b,h|{self.0.facts.work(s,b)?;if h>0{self.0.facts.before_owned(h)?;}Ok::<_,NativeError>(())})}
 fn scarcity_delay(&mut self,preferred:&MaterialLedgerId,actual:&MaterialLedgerId,consume:&[MaterialStack],tags:&[String])->Result<u32,NativeError>{market::scarcity_delay_from_facts_v1(self,preferred,actual,consume,tags)}
}
impl<M> market::sealed::Sealed for RecipeFacts<'_, '_, '_, '_, '_,M>{}
impl<M:MerkleNodeReader> market::FactoryMarketFactsV1 for RecipeFacts<'_, '_, '_, '_, '_,M>{
 type Error=NativeError;
 fn charge(&mut self,n:usize)->Result<(),NativeError>{RecipeFacts::charge(self,n)}
 fn reserve_work(&mut self,consume:&[MaterialStack],tags:&[String])->Result<(),NativeError>{self.0.before_clone(&(consume,tags))?;Ok(())}
 fn material(&mut self,ledger:&MaterialLedgerId,kind:&str)->Result<i64,NativeError>{self.quantity(ledger,kind)}
 fn transit_loss_bps(&mut self,kind:&str)->Result<i64,NativeError>{RecipeFacts::charge(self,kind.len())?;let class=match self.0.facts.point("material_profiles",kind)?{None=>None,Some(row)=>Some(super::super::super::super::fact_decode::with_material_profile_data_v1(self.0.facts.raw(row)?,&self.0.facts.common,|value|Ok(value.transport_loss_class))?)};Ok(oasis7_world_state::world::pure_helpers::material_transit_loss_bps_from_transport_class_v1(class))}
 fn governance_tax_bps(&mut self)->Result<u16,NativeError>{Ok(self.0.facts.with_gameplay_policy(|policy|Ok(policy.electricity_tax_bps.saturating_add(policy.data_tax_bps).min(10_000)))?)}
}
impl<M> product::sealed::Sealed for RecipeFacts<'_, '_, '_, '_, '_,M>{}
impl<M:MerkleNodeReader> product::ProductStackLimitFactsV1 for RecipeFacts<'_, '_, '_, '_, '_,M>{
 type Error=NativeError;
 fn charge(&mut self,n:usize)->Result<(),NativeError>{RecipeFacts::charge(self,n)}
 fn reserve_result(&mut self,kind:&str)->Result<(),NativeError>{let n=kind.len().checked_mul(24).and_then(|n|n.checked_add(4*("recipe produce byproduct stack limit exceeded: kind= amount= limit=; reduce batches and request a new quote".len()+2*20))).ok_or_else(limited)?;self.ownership(n)}
 fn resolve_product_module(&mut self,kind:&str)->Result<Option<String>,NativeError>{let active=self.0.required(&["module_registry","active"])?;let memory=self.0.facts.common.borrow().memory().clone();let mut graph=|error:ProjectionColumnErrorV1<NativeError>|match error{ProjectionColumnErrorV1::Work(e)=>e,ProjectionColumnErrorV1::Memory(e)=>NativeError::Body(ValueTreeError::Memory(e).into()),ProjectionColumnErrorV1::Invalid(e)=>NativeError::Body(invalid(e))};if active.share_data(&memory,&mut|s,b|{self.0.facts.work(s,usize::try_from(b).map_err(|_|limited())?)?;Ok::<_,NativeError>(())}).map_err(&mut graph)?.root_kind()!="object"{return Err(invalid("actual active module registry kind").into())}
  crate::world::economy::readonly_recipe_modules::select_product_module_v1(kind,crate::world::World::builtin_product_module_for_kind(kind),&mut|id|Ok(self.0.member_like_json_get(&active,id)?.is_some()),&mut|emit|{active.object_members_control_data_v1(&memory,&mut|s,b|{self.0.facts.work(s,usize::try_from(b).map_err(|_|limited())?)?;Ok::<_,NativeError>(())},&mut|id,_row,_work|emit(id).map_err(ProjectionColumnErrorV1::Work)).map_err(&mut graph)?;Ok(())},&mut|s,b,h|{self.0.facts.work(s,b)?;if h>0{self.0.facts.before_owned(h)?;}Ok::<_,NativeError>(())})
 }
 fn active_module_hash(&mut self,id:&str)->Result<Cow<'_,str>,NativeError>{RecipeFacts::charge(self,id.len())?;let version=match self.0.optional(&["module_registry","active",id])?{Some(value)=>value,None=>return Err(crate::WorldError::ModuleChangeInvalid{reason:self.0.format(format_args!("module not active {id}"))?}.into())};let version=self.original_string(&version)?;let key=self.0.format(format_args!("{id}@{version}"))?;let record=self.0.optional(&["module_registry","records",&key])?;let Some(record)=record else{return Err(crate::WorldError::ModuleChangeInvalid{reason:self.0.format(format_args!("module record missing {key}"))?}.into())};let manifest=self.0.member_like_json_get(&record,"manifest")?.ok_or_else(||invalid("actual module record manifest missing"))?;let hash=self.0.member_like_json_get(&manifest,"wasm_hash")?.ok_or_else(||invalid("actual module record wasm hash missing"))?;Ok(Cow::Owned(self.original_string(&hash)?))}
}

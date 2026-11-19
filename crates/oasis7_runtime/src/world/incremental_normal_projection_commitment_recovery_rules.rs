//! One original Runtime recovery-assessment assembly over a real quote source.
use oasis7_world_state::gameplay_capability::{GameplayCapabilityQuery,RecoveryAssessment,CURRENT_INDUSTRIAL_CAPABILITY_GOAL};
/// Work controls only copying and allocation. The fixed callers choose the
/// original builtin-quote source and original JSON codec.
pub(crate) trait RecoveryAssemblyOpsV1{
 type Error;
 fn text(&self,value:&str)->Result<String,Self::Error>;
 fn before_push(&self,old_len:usize,old_capacity:usize)->Result<(),Self::Error>;
 fn quote(&self,owner:&str,factory:&str,recipe:&str)->Result<Option<(bool,serde_json::Value,serde_json::Value)>,Self::Error>;
 fn compare(&self,a:&str,b:&str)->Result<bool,Self::Error>;
}
pub(crate) fn append_recipe_recovery_data_v1<S:RecoveryAssemblyOpsV1>(source:&S,query:&mut GameplayCapabilityQuery,goal:&str,execution_actor:Option<&str>)->Result<(),S::Error>{
 if !source.compare(goal,CURRENT_INDUSTRIAL_CAPABILITY_GOAL)?{return Ok(())}
 let mut candidate=RecoveryAssessment{kind:source.text("current_recipe_run")?,status:source.text("unknown")?,reason_code:source.text("current_line_evidence_unavailable")?,action:None,evidence:None,recheck:Some(source.text("fresh_authenticated_read_after_authority_resource_or_line_change")?)};
 if let(Some(owner),Some(factory),Some(recipe))=(query.subject.owner(),query.primary_factory_id.as_deref(),query.current_recipe_id.as_deref()){
  let authorized=match execution_actor{None=>false,Some(actor)=>source.compare(actor,owner)?};
  if !authorized{candidate.reason_code=source.text("current_execution_authority_unavailable")?}else{
   match source.quote(owner,factory,recipe)?{Some((rejected,action,evidence))=>{candidate.status=source.text(if rejected{"blocked"}else{"available"})?;candidate.reason_code=source.text(if rejected{"canonical_recipe_admission_rejected"}else{"canonical_recipe_quote_available"})?;candidate.action=Some(action);candidate.evidence=Some(evidence)},None=>candidate.reason_code=source.text("canonical_recipe_quote_unavailable")?}
  }
 }
 source.before_push(query.recovery.len(),query.recovery.capacity())?;query.recovery.push(candidate);Ok(())
}

pub(crate) struct LegacyRecoveryV1<'a>(pub(crate) &'a crate::World);
impl RecoveryAssemblyOpsV1 for LegacyRecoveryV1<'_>{type Error=std::convert::Infallible;
 fn text(&self,v:&str)->Result<String,Self::Error>{Ok(v.into())}
 fn before_push(&self,_:usize,_:usize)->Result<(),Self::Error>{Ok(())}
 fn compare(&self,a:&str,b:&str)->Result<bool,Self::Error>{Ok(a==b)}
 fn quote(&self,owner:&str,factory:&str,recipe:&str)->Result<Option<(bool,serde_json::Value,serde_json::Value)>,Self::Error>{Ok(match self.0.builtin_recipe_quote(owner,factory,recipe,1){Err(_)=>None,Ok(quote)=>{let rejected=quote.rejection.is_some();let action=serde_json::to_value(&quote.action).expect("serializable recipe action");let evidence=serde_json::json!({"quoted_at_tick":quote.quoted_at_tick,"rejection":quote.rejection,"preview":quote.preview,"authorization":quote.authorization});Some((rejected,action,evidence))}})}
}

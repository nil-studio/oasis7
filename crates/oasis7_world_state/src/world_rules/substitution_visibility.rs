//! Original current-owner substitution observation predicates.
use oasis7_agent_api::world_action::*;
pub fn substitution_authority_visible_with_work_v1<E>(value:&SubstitutionExecutionAuthorityV1,actor:&str,binding:&str,work:&mut impl FnMut(u64,usize)->Result<(),E>,root:&mut impl FnMut(&IndustrialRefV1)->Result<bool,E>)->Result<bool,E>{work(1,value.owner_id.len()+actor.len())?;if value.owner_id!=actor{return Ok(false)}work(1,value.owner_binding_digest.len()+binding.len())?;if value.owner_binding_digest!=binding{return Ok(false)}root(&value.root_ref)}
pub fn substitution_decision_visible_with_work_v1<E>(value:&SubstitutionDecisionV1,actor:&str,binding:&str,work:&mut impl FnMut(u64,usize)->Result<(),E>,root:&mut impl FnMut(&IndustrialRefV1)->Result<bool,E>)->Result<bool,E>{work(1,value.owner_id.len()+actor.len())?;if value.owner_id!=actor{return Ok(false)}work(1,value.candidate.authority.owner_binding_digest.len()+binding.len())?;if value.candidate.authority.owner_binding_digest!=binding{return Ok(false)}root(&value.intent.preview.request.root_ref)}

/// Original private receipt predicate after current-owner filtering.
pub fn current_rule_request_visible_with_work_v1<E>(receipt:&super::WorldRuleReceiptV1,actor:Option<&str>,binding:Option<&str>,work:&mut impl FnMut(u64,usize)->Result<(),E>,root:&mut impl FnMut(&IndustrialRefV1)->Result<bool,E>,lot:&mut impl FnMut(&IndustrialRefV1)->Result<bool,E>,decision:&mut impl FnMut(&IndustrialRefV1)->Result<bool,E>)->Result<bool,E>{
 work(1,receipt.actor_id.len()+actor.map_or(0,str::len))?;if actor!=Some(receipt.actor_id.as_str()){return Ok(false)}
 let Some(binding)=binding else{return Ok(!matches!(receipt.command,WorldRuleCommandV1::IndustrialWork{..}|WorldRuleCommandV1::RecipeSubstitution{..}))};
 match &receipt.command{
 WorldRuleCommandV1::IndustrialWork{command}=>match command.as_ref(){
 IndustrialWorkCommandV1::ImportQualifiedLot{facts,..}=>{work(1,facts.owner_binding_digest.len()+binding.len())?;Ok(facts.owner_binding_digest==binding)},
 IndustrialWorkCommandV1::AcceptRoot{authority,..}=>{work(1,authority.owner_binding_digest.len()+binding.len())?;Ok(authority.owner_binding_digest==binding)},
 IndustrialWorkCommandV1::ReleaseLot{lot_ref}=>lot(lot_ref),
 IndustrialWorkCommandV1::StartRoot{root_ref,..}|IndustrialWorkCommandV1::TerminateRoot{root_ref,..}=>root(root_ref)},
 WorldRuleCommandV1::RecipeSubstitution{command}=>match command.as_ref(){
 RecipeSubstitutionCommandV1::PublishAuthority{authority,..}=>{work(1,authority.owner_binding_digest.len()+binding.len())?;Ok(authority.owner_binding_digest==binding)},
 RecipeSubstitutionCommandV1::Confirm{confirmation}=>root(&confirmation.preview.request.root_ref),
 RecipeSubstitutionCommandV1::Convert{decision_ref}|RecipeSubstitutionCommandV1::ApplyBinding{decision_ref}|RecipeSubstitutionCommandV1::Cutover{decision_ref,..}=>decision(decision_ref)},
 _=>Ok(true)}
}

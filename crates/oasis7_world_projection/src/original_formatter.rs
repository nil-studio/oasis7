//! Original service projection field assembly. These are observation DATA
//! operations: they never validate a source, audience, seal or permission.
//! Fixed World/Current callers compute each field from their actual sources.
use crate::{WorldServiceProjection,WorldServiceFeedbackHistory};
use oasis7_agent_api::RuntimeBindingV1;
use oasis7_proto::viewer::CanonicalAgentOwnerFenceV1;
use oasis7_client_api::world_service::FormalProjectionCommitHeaderV1;
use oasis7_world_state::WorldEvent;

pub fn base_data_v1<S>(state:S,world_rule_status:Option<serde_json::Value>,canonical_agent_owner:Option<CanonicalAgentOwnerFenceV1>,feedback_history:Option<WorldServiceFeedbackHistory>,runtime_binding:Option<RuntimeBindingV1>)->WorldServiceProjection<S>{
 WorldServiceProjection{
  world_rule_status,canonical_agent_owner,canonical_agent_chat:None,
  game_task_control:None,native_delegation_authorizations:Vec::new(),
  feedback_history,state,events:Vec::new(),formal_commit_header:None,
  runtime_binding,agent_context:None,scheduler_wakes:Vec::new(),
  continuation_contexts:Default::default(),continuations:Vec::new(),cognition_leases:Vec::new(),
 }
}
/// The exact owner-only chat key predicate. Authentication of the actual key
/// source remains mandatory in the fixed caller.
pub fn owner_key_matches_data_v1(owner_key:&str,caller_key:&str)->bool{owner_key==caller_key}
pub fn attach_gameplay_query_data_v1<S>(projection:&mut WorldServiceProjection<S>,query:serde_json::Value){
 let status=projection.world_rule_status.get_or_insert_with(||serde_json::json!({}));
 status["gameplay_capability"]=query;
}
pub fn attach_formal_header_data_v1<S>(projection:&mut WorldServiceProjection<S>,header:FormalProjectionCommitHeaderV1,current_era_events:Vec<WorldEvent>){
 projection.events=current_era_events;projection.formal_commit_header=Some(header);
}

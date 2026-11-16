//! Independent IWS Current commitment and tick-record DATA grammar. Parsing or
//! validating these values never mints Node, execution, Current, acceptance or
//! legacy TickConsensusRecord qualification.
use serde::{Serialize,Deserialize};
pub const IWS_CURRENT_COMMITMENT_DOMAIN_V1:&str="oasis7.iws.normal-current-commitment.v1";
pub const IWS_TICK_CONSENSUS_DOMAIN_V1:&str="oasis7.iws.tick-consensus-record.v1";
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IwsNodeCommitContextV1{pub world_id:String,pub node_id:String,pub proposer_id:String,pub height:u64,pub slot:u64,pub epoch:u64,pub node_block_hash:String,pub action_root:String,pub committed_at_unix_ms:i64}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IwsCurrentCommitmentFrameV1{
 pub domain:String,pub version:u32,pub parent_record:[u8;32],pub parent_body:[u8;32],pub parent_roots:[u8;32],
 pub runtime_original:[u8;32],pub runtime_ordinal:u64,pub cognition_original:[u8;32],pub cognition_ordinal:u64,
 pub current_roots:Vec<u8>,pub manifest_hash:String,pub policy_namespace_root:Vec<u8>,pub frozen_material:[u8;32],pub input_original:[u8;32],pub input_bytes:u64,pub policy_original:[u8;32],pub policy_bytes:u64,pub context:IwsNodeCommitContextV1,
}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IwsTickEventIdentityV1{pub event_id:u64,pub original:[u8;32]}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IwsTickConsensusRecordV1{
 pub domain:String,pub version:u32,pub consensus_height:u64,pub tick:u64,
 pub authority_source:String,pub parent_tick_original:Option<[u8;32]>,
 pub current:IwsCurrentCommitmentFrameV1,pub current_digest:[u8;32],
 pub ordered_events:Vec<IwsTickEventIdentityV1>,pub ordered_action_ids:Vec<u64>,
}
impl IwsCurrentCommitmentFrameV1{
 /// Structural validation only. Authenticity of this historical cut requires
 /// the fixed producer/Original occurrence and its actual source binding.
 pub fn validate_data(&self)->Result<(),oasis7_formal_protocol::incremental_execution::ExecutionError>{
  use oasis7_formal_protocol::{incremental_execution::{ExecutionError,RootSetV1},incremental_state::StateNamespaceV1};
  let invalid=||ExecutionError::Invalid("IWS Current commitment frame");
  if self.domain!=IWS_CURRENT_COMMITMENT_DOMAIN_V1||self.version!=1||self.current_roots.len()>1024{return Err(invalid())}
  let roots=RootSetV1::from_canonical_bytes(&self.current_roots)?;
  if roots.world_id!=self.context.world_id||roots.height!=self.context.height||roots.state_root(StateNamespaceV1::Authorization).canonical_bytes().map_err(ExecutionError::Tree)?!=self.policy_namespace_root||self.manifest_hash.len()!=64||!self.manifest_hash.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)){return Err(invalid())}Ok(())
 }
}
impl IwsTickConsensusRecordV1{
 pub fn validate_data(&self)->Result<(),oasis7_formal_protocol::incremental_execution::ExecutionError>{
  use oasis7_formal_protocol::incremental_execution::{ExecutionError,RootSetV1};
  self.current.validate_data()?;
  let roots=RootSetV1::from_canonical_bytes(&self.current.current_roots)?;
  if self.domain!=IWS_TICK_CONSENSUS_DOMAIN_V1||self.version!=1||self.consensus_height==0||self.tick!=roots.simulation_tick||self.authority_source.trim().is_empty(){return Err(ExecutionError::Invalid("IWS tick consensus data"))}Ok(())
 }
}

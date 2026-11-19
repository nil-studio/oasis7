use super::*;
use crate::incremental_runtime_history_decode::*;
use oasis7_formal_protocol::{incremental_execution::RootSetV1,incremental_state::StateNamespaceV1};
use oasis7_proto::{memory_budget::{MemoryBudget,MemoryClass},cbor_decode_preflight::CborPreflightWorkV1};
use oasis7_wasm_abi::CanonicalCborWorkV1;
fn fixture()->IwsTickConsensusRecordV1{
 let mut roots=RootSetV1::empty("world/真实".into(),[7;32]).unwrap();roots.height=3;roots.simulation_tick=91;
 IwsTickConsensusRecordV1{domain:IWS_TICK_CONSENSUS_DOMAIN_V1.into(),version:1,consensus_height:1,tick:91,authority_source:"authority".into(),parent_tick_original:None,current:IwsCurrentCommitmentFrameV1{domain:IWS_CURRENT_COMMITMENT_DOMAIN_V1.into(),version:1,parent_record:[1;32],parent_body:[2;32],parent_roots:[3;32],runtime_original:[4;32],runtime_ordinal:5,cognition_original:[6;32],cognition_ordinal:7,current_roots:roots.canonical_bytes().unwrap(),manifest_hash:"ab".repeat(32),policy_namespace_root:roots.state_root(StateNamespaceV1::Authorization).canonical_bytes().unwrap(),frozen_material:[8;32],input_original:[9;32],input_bytes:101,policy_original:[10;32],policy_bytes:102,context:IwsNodeCommitContextV1{world_id:roots.world_id,node_id:"node".into(),proposer_id:"proposer".into(),height:3,slot:4,epoch:5,node_block_hash:"block".into(),action_root:"actions".into(),committed_at_unix_ms:19}},current_digest:[11;32],ordered_events:vec![IwsTickEventIdentityV1{event_id:u64::MAX,original:[12;32]},IwsTickEventIdentityV1{event_id:1,original:[13;32]}],ordered_action_ids:vec![8,4]}
}
fn work()->RuntimeHistoryDecodeWorkV1{RuntimeHistoryDecodeWorkV1{structure:CborPreflightWorkV1{bytes_remaining:8<<20,values_remaining:1_000_000,max_depth:64},canonical:CanonicalCborWorkV1{visits_remaining:1_000_000,bytes_remaining:8<<20,max_depth:64,max_map_keys:1024,max_key_bytes:1024},max_original_bytes:1<<20,max_heap_bytes:8<<20}}
#[derive(serde::Serialize)]
struct Envelope<'a,T>{domain:&'a str,schema:&'a str,value:T}
#[test]
fn iws_tick_profile_roundtrips_its_distinct_envelope_and_preserves_rollover_order(){
 let memory=MemoryBudget::new(32<<20,0).unwrap();let _fixture_fee=memory.reserve(1<<20,MemoryClass::Ordinary).unwrap();let value=fixture();value.validate_data().unwrap();
 let raw=oasis7_wasm_abi::encode_canonical_cbor(&Envelope{domain:"oasis7.runtime.history.original.v1",schema:"iws-tick-consensus-record.v1",value:&value}).unwrap();
 for kind in[RuntimeHistoryDecodeKindV1::IwsTick,RuntimeHistoryDecodeKindV1::TickFamily]{
  let decoded=decode_runtime_history_original_v1(kind,&raw,&memory,MemoryClass::Ordinary,&mut work(),||false).unwrap();
  assert!(matches!(&*decoded,DecodedRuntimeHistoryOriginalV1::IwsTick(actual)if actual==&value));
 }
 assert!(decode_runtime_history_original_v1(RuntimeHistoryDecodeKindV1::Tick,&raw,&memory,MemoryClass::Ordinary,&mut work(),||false).is_err());
}
#[test]
fn iws_current_frame_does_not_accept_changed_domain_namespace_or_tick(){
 let mut value=fixture();value.current.domain="legacy-state-root".into();assert!(value.validate_data().is_err());
 let mut value=fixture();value.current.policy_namespace_root[0]^=1;assert!(value.validate_data().is_err());
 let mut value=fixture();value.tick+=1;assert!(value.validate_data().is_err());
}
#[test]
fn iws_tick_family_dispatch_observes_cancellation_and_separate_header_budget(){
 let memory=MemoryBudget::new(32<<20,0).unwrap();let _fixture_fee=memory.reserve(1<<20,MemoryClass::Ordinary).unwrap();
 let raw=oasis7_wasm_abi::encode_canonical_cbor(&Envelope{domain:"oasis7.runtime.history.original.v1",schema:"iws-tick-consensus-record.v1",value:fixture()}).unwrap();
 assert!(matches!(decode_runtime_history_original_v1(RuntimeHistoryDecodeKindV1::TickFamily,&raw,&memory,MemoryClass::Ordinary,&mut work(),||true),Err(oasis7_formal_protocol::incremental_value::ValueTreeError::Cancelled)));
 let mut limited=work();limited.structure.values_remaining=1;
 assert!(matches!(decode_runtime_history_original_v1(RuntimeHistoryDecodeKindV1::TickFamily,&raw,&memory,MemoryClass::Ordinary,&mut limited,||false),Err(oasis7_formal_protocol::incremental_value::ValueTreeError::ResourceLimited(_))));
}

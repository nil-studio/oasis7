//! Structural Cognition seed from one genuine qualified parent. Enabled
//! fields come exclusively from the later actual State/Proof/History folds;
//! Disabled serializes the original null, never a hashed/default envelope.
use crate::world::{incremental_body::BodyMutationError,incremental_normal_sources::{BoundNormalTickParentV1,root_bytes}};
use oasis7_formal_protocol::{controlled_authority::formal::bounded_context::FormalBoundedContextV1,incremental_value::ValueTreeError};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::observation_projection::current_commitment::{ProjectionColumnDataV1,ProjectionColumnErrorV1};
use oasis7_world_state::incremental_cognition_current_complete_v3::CognitionCurrentPresenceV3;
fn bad()->BodyMutationError{BodyMutationError::Invalid("qualified Cold Cognition profile")}
pub(in crate::world)fn prepare_qualified_cognition_projection_profile_v1(parent:&BoundNormalTickParentV1<'_>,common:&mut FormalBoundedContextV1<'_>)->Result<ProjectionColumnDataV1,BodyMutationError>{
 parent.validate_context(common).map_err(|e|BodyMutationError::Original(Box::new(e)))?;
 let refs=parent.refs();let current=refs.storage.current().cognition();let history=refs.cognition.selected();
 common.charge(1,96).map_err(|e|BodyMutationError::Original(Box::new(e)))?;
 if !current.uses_budget(common.memory())||current.allocation_class()!=MemoryClass::Ordinary||!refs.cognition.uses_budget(common.memory())||refs.cognition.selected().allocation_class()!=MemoryClass::Ordinary||refs.cognition.current().head_record_id()!=refs.head.verified_record_id()||history.manifest().roots!=parent.fence().parent_roots{return Err(bad())}
 // The original binding method encodes/hashes this real RootSet. Its fixed
 // source layout is charged before that operation and kept through validation.
 let roots_bytes=root_bytes(refs.roots,refs.roots).map_err(|e|BodyMutationError::Original(Box::new(e)))?;
 let phase_bytes=usize::try_from(roots_bytes).ok().and_then(|n|n.checked_mul(16)).and_then(|n|n.checked_add(4096)).ok_or_else(bad)?;
 common.charge(phase_bytes as u64,phase_bytes as u64).map_err(|e|BodyMutationError::Original(Box::new(e)))?;
 let _binding_phase=common.memory().reserve(phase_bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 current.validate_binding(refs.roots)?;
 let enabled=match current.presence(){CognitionCurrentPresenceV3::Enabled=>true,CognitionCurrentPresenceV3::Disabled=>false};
 if enabled!=history.manifest().enabled{return Err(bad())}
 // One original fixed field key and one raw Map node; no Default/codec/hash
 // of a Cognition record is executed here. Column owns its own resulting fees.
 let raw_bytes=11*(std::mem::size_of::<String>()+std::mem::size_of::<serde_json::Value>())+13*std::mem::size_of::<usize>()+std::mem::size_of::<serde_json::Map<String,serde_json::Value>>()+"cognition".len()+256;
 common.charge(1,raw_bytes as u64).map_err(|e|BodyMutationError::Original(Box::new(e)))?;
 let memory=common.memory().clone();let _raw_phase=memory.reserve(raw_bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let mut object=serde_json::Map::new();object.insert("cognition".to_owned(),if enabled{serde_json::Value::Object(serde_json::Map::new())}else{serde_json::Value::Null});
 ProjectionColumnDataV1::from_json_data(serde_json::Value::Object(object),&memory,&mut|s,b|common.charge(s,b)).map_err(|e|match e{ProjectionColumnErrorV1::Work(e)=>BodyMutationError::Original(Box::new(e)),ProjectionColumnErrorV1::Memory(e)=>ValueTreeError::Memory(e).into(),ProjectionColumnErrorV1::Invalid(_)=>bad()})
}

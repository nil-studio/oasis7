//! Fixed old projection domains, canonical encoder and BLAKE3 implementation.
//! Inputs here are DATA only; source/audience selection remains the producer.
use super::*;
use oasis7_client_api::formal_projection::{ProjectionAudienceV1,projection_leaf_with_work_v1,projection_tree_with_work_v1,projection_root_with_work_v1};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use std::mem::size_of;
fn copy_digest(value:&str,common:&mut FormalBoundedContextV1<'_>)->Result<ReservedValue<String>,BodyMutationError>{
 common.charge(1,value.len() as u64).map_err(paid)?;if value.len()!=64||!value.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)){return Err(invalid("original projection digest encoding"))}
 let fee=common.memory().reserve(size_of::<String>()+64,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;let text=value.to_owned();common.consume_steps(0).map_err(paid)?;Ok(fee.retain(text))
}
pub(super) fn leaf_data_v1<T:serde::Serialize+?Sized>(audience:&ProjectionAudienceV1,value:&T,common:&mut FormalBoundedContextV1<'_>,canonical:&mut CanonicalCborWorkV1)->Result<ReservedValue<String>,BodyMutationError>{
 let digest=crate::world::normal_bootstrap_current::hash::with_current_meter_v1(common,canonical,|meter|projection_leaf_with_work_v1(audience,value,meter))?.map_err(paid)?;
 copy_digest(digest.as_str(),common)
}
/// The fixed audience assembler holds all actual input-graph/Vec fees across
/// this consuming call; the shared tree kernel owns its new levels' fee.
pub(super) fn tree_root_data_v1(leaves:Vec<(ProjectionAudienceV1,String)>,common:&mut FormalBoundedContextV1<'_>,canonical:&mut CanonicalCborWorkV1)->Result<ReservedValue<String>,BodyMutationError>{
 let tree=crate::world::normal_bootstrap_current::hash::with_current_meter_v1(common,canonical,|meter|projection_tree_with_work_v1(leaves,meter))?.map_err(paid)?;
 let root=crate::world::normal_bootstrap_current::hash::with_current_meter_v1(common,canonical,|meter|projection_root_with_work_v1(tree.value(),meter))?.map_err(paid)?;
 copy_digest(root.as_str(),common)
}

//! Original nested-map/map-set DATA with independent group presence. Empty
//! groups are actual rows; absence is never inferred from child cardinality.
use super::*;
use super::original_rows::{OriginalProjectionRowsDataV1,OriginalProjectionRowDataV1};
use std::{mem::size_of,sync::Arc};
use oasis7_world_state::incremental_semantic_key::{read_semantic_key_v1,SemanticKeyRefV1,SemanticKeyTokenV1,SEMANTIC_KEY_DOMAIN};
fn bytes_copy(bytes:&[u8],common:&mut FormalBoundedContextV1<'_>)->Result<ReservedValue<Vec<u8>>,BodyMutationError>{
 let n=bytes.len().checked_add(size_of::<Vec<u8>>()).ok_or_else(||invalid("nested original bytes layout"))?;common.charge(1,n as u64).map_err(paid)?;let fee=common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;let mut out=Vec::with_capacity(bytes.len());if out.capacity()!=bytes.len(){return Err(invalid("nested original bytes capacity"))}for part in bytes.chunks(8192){common.charge(1,part.len() as u64).map_err(paid)?;out.extend_from_slice(part)}Ok(fee.retain(out))
}
fn token_root(token:SemanticKeyTokenV1<'_>,common:&mut FormalBoundedContextV1<'_>)->Result<ReservedValue<Vec<u8>>,BodyMutationError>{
 let token=token.original_token_bytes_v1();let len=SEMANTIC_KEY_DOMAIN.len().checked_add(token.len()).ok_or_else(||invalid("nested original key layout"))?;common.charge(1,len as u64).map_err(paid)?;let fee=common.memory().reserve(size_of::<Vec<u8>>().checked_add(len).ok_or_else(||invalid("nested original key layout"))?,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;let mut out=Vec::with_capacity(len);if out.capacity()!=len{return Err(invalid("nested original key capacity"))}out.extend_from_slice(SEMANTIC_KEY_DOMAIN);for part in token.chunks(8192){common.charge(1,part.len() as u64).map_err(paid)?;out.extend_from_slice(part)}Ok(fee.retain(out))
}
#[derive(Debug)]struct PaidNestedKeyCodecError{error:serde_json::Error,_fee:MemoryReservation}
impl std::fmt::Display for PaidNestedKeyCodecError{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{std::fmt::Display::fmt(&self.error,f)}}
impl std::error::Error for PaidNestedKeyCodecError{fn source(&self)->Option<&(dyn std::error::Error+'static)>{Some(&self.error)}}
fn pair_key_json<'a>(bytes:&'a[u8],common:&mut FormalBoundedContextV1<'_>)->Result<(&'a serde_json::value::RawValue,&'a serde_json::value::RawValue),BodyMutationError>{
 let n=bytes.len().checked_mul(2).and_then(|n|n.checked_add(32768)).ok_or_else(||invalid("nested original key refusal layout"))?;common.charge(1,n as u64).map_err(paid)?;let phase=common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 common.charge(bytes.len() as u64,bytes.len() as u64).map_err(paid)?;let decoded=serde_json::from_slice::<(&serde_json::value::RawValue,&serde_json::value::RawValue)>(bytes);common.consume_steps(0).map_err(paid)?;
 match decoded{Ok(pair)=>Ok(pair),Err(error)=>Err(paid(PaidNestedKeyCodecError{error,_fee:phase}))}
}
pub(super) fn initialize_original_nested_rows_data_v1(field:&ProjectionOriginalColumnFieldV1,common:&mut FormalBoundedContextV1<'_>)->Result<OriginalProjectionRowsDataV1,BodyMutationError>{
 use oasis7_world_state::incremental_projection::FieldShape;
 let is_set=matches!(field.descriptor().shape,FieldShape::MapSet);if !is_set&&!matches!(field.descriptor().shape,FieldShape::NestedMap){return Err(invalid("nested original field profile"))}
 let mut groups=OriginalProjectionRowsDataV1::empty_data_v1(common)?;let mut offset=0usize;
 for group in field.groups(){
  common.charge(1,group.semantic().len() as u64).map_err(paid)?;let expected=read_semantic_key_v1(group.semantic()).map_err(invalid)?;
  let end=offset.checked_add(group.entries()).filter(|end|*end<=field.rows().len()).ok_or_else(||invalid("nested original group count"))?;
  let mut children=OriginalProjectionRowsDataV1::empty_data_v1(common)?;
  for row in &field.rows()[offset..end]{
   common.charge(1,row.semantic().len().checked_mul(32).ok_or_else(||invalid("nested key comparison work"))? as u64).map_err(paid)?;
   let SemanticKeyRefV1::Pair{left,..}=read_semantic_key_v1(row.semantic()).map_err(invalid)?else{return Err(invalid("nested original pair key"))};
   if left.decode().map_err(invalid)?!=expected{return Err(invalid("nested original group key mismatch"))}
   let (_,key)=pair_key_json(row.original_key_json(),common)?;
   let value=OriginalProjectionRowDataV1::from_original_encoded_data_v1(bytes_copy(row.semantic(),common)?,bytes_copy(key.get().as_bytes(),common)?,bytes_copy(row.original_json(),common)?,common)?;
   children=children.with_row_data_v1(value,common)?;
  }
  offset=end;
  let row=OriginalProjectionRowDataV1::from_group_data_v1(bytes_copy(group.semantic(),common)?,bytes_copy(group.original_key_json(),common)?,Arc::new(children),is_set,common)?;
  groups=groups.with_row_data_v1(row,common)?;
 }
 if offset!=field.rows().len(){return Err(invalid("nested original ungrouped row"))}Ok(groups)
}
/// A genuine delta's explicit group operation. It checks the resident before
/// presence and preserves an empty after group independently of its children.
pub(super) fn with_original_group_presence_data_v1(groups:&OriginalProjectionRowsDataV1,semantic:&[u8],key_json:&[u8],before_present:bool,after_present:bool,is_set:bool,common:&mut FormalBoundedContextV1<'_>)->Result<OriginalProjectionRowsDataV1,BodyMutationError>{
 let before=groups.row_data_v1(semantic,common)?;
 if before.is_some()!=before_present{return Err(invalid("nested original group before presence"))}
 if !after_present{if let Some(row)=before{let children=row.nested_rows().ok_or_else(||invalid("removed original group source kind"))?;common.consume_steps(1).map_err(paid)?;if children.len()!=0{return Err(invalid("removed original group retains members"))}}return groups.without_row_data_v1(semantic,common)}
 let children=match before{Some(row)=>row.nested_rows().ok_or_else(||invalid("nested original group type"))?.share_data_v1(common)?,None=>OriginalProjectionRowsDataV1::empty_data_v1(common)?};
 let row=OriginalProjectionRowDataV1::from_group_data_v1(bytes_copy(semantic,common)?,bytes_copy(key_json,common)?,Arc::new(children),is_set,common)?;
 groups.with_row_data_v1(row,common)
}
/// Applies a real nested member operation. `allow_removed_group` is limited
/// to a deletion whose genuine enclosing delta removed that same group.
pub(super) fn with_original_nested_member_data_v1(groups:&OriginalProjectionRowsDataV1,semantic:&[u8],pair_json:&[u8],after:Option<&[u8]>,is_set:bool,allow_removed_group:bool,common:&mut FormalBoundedContextV1<'_>)->Result<OriginalProjectionRowsDataV1,BodyMutationError>{
 common.charge(1,semantic.len().checked_mul(32).ok_or_else(||invalid("nested original pair work"))? as u64).map_err(paid)?;
 let SemanticKeyRefV1::Pair{left,..}=read_semantic_key_v1(semantic).map_err(invalid)?else{return Err(invalid("nested original member pair"))};
 let group_key=token_root(left,common)?;let (outer_json,inner_json)=pair_key_json(pair_json,common)?;
 let Some(group)=groups.row_data_v1(&group_key,common)?else{if after.is_none()&&allow_removed_group{return groups.share_data_v1(common)}return Err(invalid("nested original member group absent"))};
 let before=group.nested_rows().ok_or_else(||invalid("nested original group type"))?;
 let children=if let Some(value)=after{let row=OriginalProjectionRowDataV1::from_original_encoded_data_v1(bytes_copy(semantic,common)?,bytes_copy(inner_json.get().as_bytes(),common)?,bytes_copy(value,common)?,common)?;before.with_row_data_v1(row,common)?}else{before.without_row_data_v1(semantic,common)?};
 let row=OriginalProjectionRowDataV1::from_group_data_v1(group_key,bytes_copy(outer_json.get().as_bytes(),common)?,Arc::new(children),is_set,common)?;
 groups.with_row_data_v1(row,common)
}

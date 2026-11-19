//! Fold only the exact original diplomacy-cell changes. Counts are checked
//! after all member patches, independently of sparse-tree visitation order.
use super::*;
use super::{
    diplomacy_rows::OriginalDiplomacyRecordDataV1, original_rows::OriginalProjectionRowDataV1,
    state_columns::OriginalStateColumnsDataV1,
};
use oasis7_world_state::incremental_diplomacy::{
    CurrentDiplomacyProjectionDeltaV1, DiplomacyProjectionPathRefV1 as Path,
};
use oasis7_formal_protocol::{incremental_execution::{ExecutionTransaction,ExecutionFence},incremental_state::MerkleNodeReader};
use std::cell::RefCell;
fn bytes(value:&[u8],common:&mut FormalBoundedContextV1<'_>)->Result<ReservedValue<Vec<u8>>,BodyMutationError>{super::initial::capture(common,|out|std::io::Write::write_all(out,value).map_err(paid))}
fn key<K:oasis7_world_state::incremental_semantic_key::SemanticKeyV1+?Sized>(value:&K,common:&mut FormalBoundedContextV1<'_>)->Result<(ReservedValue<Vec<u8>>,ReservedValue<Vec<u8>>),BodyMutationError>{let semantic=super::initial::capture(common,|out|oasis7_world_state::incremental_semantic_key::write_semantic_key_v1(value,out).map_err(paid))?;let json=super::capture_projection_json_data_v1(value,common)?;Ok((semantic,json))}
pub(super) fn fold_diplomacy_v1<M:MerkleNodeReader>(out:&mut OriginalStateColumnsDataV1,source:&CurrentDiplomacyProjectionDeltaV1,tx:&ExecutionTransaction<'_,M>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<(),BodyMutationError>{
 fold_rows_v1(out,source,tx,common,live)
}
pub(super) fn fold_rows_v1<M:MerkleNodeReader,T:super::cold_record_maps::RecordMapsV1>(out:&mut T,source:&CurrentDiplomacyProjectionDeltaV1,tx:&ExecutionTransaction<'_,M>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<(),BodyMutationError>{
 source.validate(tx,common.memory(),live)?;let common=RefCell::new(common);
 source.visit_entities_with_work_v1(|s,b|common.borrow_mut().charge(s,b).map_err(paid),|kind,semantic,key_json,before,after|{let mut common=common.borrow_mut();let field=kind.name();let rows=out.map_data_v1(field,&mut common)?.ok_or_else(||invalid("diplomacy original field missing"))?;let old=rows.row_data_v1(semantic,&mut common)?;if old.is_some()!=before{return Err(invalid("diplomacy original entity before presence"))}
 if !after{let rows=rows.without_row_data_v1(semantic,&mut common)?;return out.replace_original_map_data_v1(field,rows,&mut common)}
 if !before{let value=OriginalDiplomacyRecordDataV1::empty_for_new_actual_entity_v1(kind,&mut common)?;let row=OriginalProjectionRowDataV1::from_diplomacy_data_v1(bytes(semantic,&mut common)?,bytes(key_json,&mut common)?,value,&mut common)?;let rows=rows.with_row_data_v1(row,&mut common)?;out.replace_original_map_data_v1(field,rows,&mut common)?;}else if old.and_then(|v|v.diplomacy_record_v1()).is_none(){return Err(invalid("diplomacy resident parent kind"))}Ok(())})?;
 source.visit_patches_with_work_v1(|s,b|common.borrow_mut().charge(s,b).map_err(paid),|kind,semantic,key_json,path,after|{let mut common=common.borrow_mut();if matches!(path,Path::Length(..)){return Ok(())}let field=kind.name();let rows=out.map_data_v1(field,&mut common)?.ok_or_else(||invalid("diplomacy original field missing"))?;let record=rows.row_data_v1(semantic,&mut common)?.and_then(|v|v.diplomacy_record_v1()).ok_or_else(||invalid("diplomacy current entity missing"))?;
 let value=match path{Path::Scalar(name)=>record.with_scalar_v1(name,after.ok_or_else(||invalid("diplomacy required logical scalar removed"))?,&mut common)?,Path::Member(index)=>{let(k,j)=key(&index,&mut common)?;let after=after.map(|v|bytes(v,&mut common)).transpose()?;record.with_member_v1(k,j,after,&mut common)?},Path::Length(..)=>unreachable!()};
 let row=OriginalProjectionRowDataV1::from_diplomacy_data_v1(bytes(semantic,&mut common)?,bytes(key_json,&mut common)?,value,&mut common)?;let rows=rows.with_row_data_v1(row,&mut common)?;out.replace_original_map_data_v1(field,rows,&mut common)
 })?;
 source.visit_patches_with_work_v1(|s,b|common.borrow_mut().charge(s,b).map_err(paid),|kind,semantic,_key,path,_after|{let Path::Length(count)=path else{return Ok(())};let mut common=common.borrow_mut();let rows=out.map_data_v1(kind.name(),&mut common)?.ok_or_else(||invalid("diplomacy original field missing"))?;let record=rows.row_data_v1(semantic,&mut common)?.and_then(|v|v.diplomacy_record_v1()).ok_or_else(||invalid("diplomacy count entity missing"))?;if u64::try_from(record.members_v1(&mut common)?.len()).map_err(|_|invalid("diplomacy resident count layout"))?!=count{return Err(invalid("diplomacy resident actual count mismatch"))}Ok(())})?;
 source.visit_entities_with_work_v1(|s,b|common.borrow_mut().charge(s,b).map_err(paid),|kind,semantic,_key,_before,after|{if !after{return Ok(())}let mut common=common.borrow_mut();let rows=out.map_data_v1(kind.name(),&mut common)?.ok_or_else(||invalid("diplomacy original field missing"))?;let record=rows.row_data_v1(semantic,&mut common)?.and_then(|v|v.diplomacy_record_v1()).ok_or_else(||invalid("diplomacy final entity missing"))?;record.validate_complete_v1(&mut common)})?;
 let common=common.into_inner();source.validate(tx,common.memory(),live)
}

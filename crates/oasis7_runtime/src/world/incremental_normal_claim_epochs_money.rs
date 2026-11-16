//! Selected fixed-economy and account state for claim epoch committers.
use super::*;
use crate::world::incremental_normal_world_rule_lifecycle::delivery::{IndustrialDeliveryWorkV1,IndustrialDeliveryErrorV1};
use crate::world::incremental_normal_tail::NormalTailWorkV1;
use oasis7_formal_protocol::{incremental_value::{ValueNodeReader,ValueNodeWriter,ValueTreeError},incremental_state::StatePageWriter};
use oasis7_world_state::{MainTokenAccountBalance,MainTokenSupplyState,incremental_field_overlay::WorldFieldOverlayContextV1,incremental_field_member::MemberRecordWriteContext,incremental_fixed_economy_records::*,incremental_world_balances::*,incremental_world_number_maps::*};
use std::collections::BTreeMap;
use oasis7_proto::memory_budget::MemoryClass;
pub(super) struct SelectedClaimMoneyV1{
 pub before:Option<MainTokenAccountBalance>,pub after:Option<MainTokenAccountBalance>,
 pub supply_before:MainTokenSupplyState,pub supply_after:MainTokenSupplyState,
 pub treasury_before:BTreeMap<String,u64>,pub treasury_after:BTreeMap<String,u64>,
 supply:ReservedValue<FixedEconomyRecordQualificationV1>,_account:MemoryReservation,_clone:MemoryReservation,_phase:MemoryReservation,
}
#[allow(clippy::too_many_arguments)]
pub(super) fn read<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(actor:&str,bucket:Option<&str>,sources:&ClaimEpochStateSourcesV1<'_>,state:&M,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,work:&mut IndustrialDeliveryWorkV1,tail:&mut NormalTailWorkV1<'_,'_>,live:&ExecutionFence)->Result<SelectedClaimMoneyV1,IndustrialDeliveryErrorV1>{
 let payload=actor.len().checked_mul(3).and_then(|n|n.checked_add(bucket.map_or(0,str::len)*3)).and_then(|n|n.checked_add(8192)).ok_or(BodyMutationError::Invalid("claim monetary layout"))?;
 tail.charge(payload)?;let phase=tail.reserve(payload,MemoryClass::Ordinary)?;
 let(before,account)=crate::world::incremental_normal_world_rule_lifecycle::delivery::economy::balance(actor,ctx,work.max_identity_bytes,tail)?;
 let clone_bytes=before.as_ref().map_or(0,|v|v.account_id.len()).checked_mul(8).ok_or(BodyMutationError::Invalid("claim account clone layout"))?;tail.charge(clone_bytes)?;let clone_fee=tail.reserve(clone_bytes,MemoryClass::Ordinary)?;
 let guard=ctx.transaction.guard();let supply=open_current_industrial_main_token_supply_v1(sources.fixed,sources.roots,ctx,state,&mut work.fixed_scan,&mut work.fixed_cells_remaining,&guard,live)?;
 let mut value=[0;4];for(i,column)in[FixedEconomyColumnV1::TotalSupply,FixedEconomyColumnV1::CirculatingSupply,FixedEconomyColumnV1::TotalIssued,FixedEconomyColumnV1::TotalBurned].into_iter().enumerate(){let guard=ctx.transaction.guard();let FixedEconomyValueV1::U64(v)=read_qualified_fixed_economy_overlay_v1(FixedEconomyCellRequestV1{column,qualification:&supply},ctx,&guard,live)?else{return Err(BodyMutationError::Invalid("claim supply column").into())};value[i]=v;}
 let supply_before=MainTokenSupplyState{total_supply:value[0],circulating_supply:value[1],total_issued:value[2],total_burned:value[3]};
 let mut treasury_before=BTreeMap::new();for key in [Some(oasis7_world_state::MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL),Some(oasis7_world_state::MAIN_TOKEN_TREASURY_BUCKET_SLASH),bucket].into_iter().flatten(){tail.charge(key.len()*3)?;if treasury_before.contains_key(key){continue}match read_world_number_map_overlay_v1(WorldNumberMapFieldV1::MainTokenTreasuryBalances,WorldNumberMapKeyRefV1::Text(key),ctx)?{Some(WorldNumberV1::Unsigned(v))=>{treasury_before.insert(key.to_owned(),v);},None=>{},_=>return Err(BodyMutationError::Invalid("claim treasury value").into())}}
 Ok(SelectedClaimMoneyV1{after:before.clone(),before,supply_after:supply_before.clone(),supply_before,treasury_before,treasury_after:BTreeMap::new(),supply,_account:account,_clone:clone_fee,_phase:phase})
}
#[allow(clippy::too_many_arguments)]
pub(super) fn commit<M:MerkleNodeReader,R:ValueNodeReader,W:ValueNodeReader+ValueNodeWriter+StatePageWriter,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,CW:FnMut()->bool>(source:&SelectedClaimMoneyV1,actor:&str,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,write:&mut MemberRecordWriteContext<'_,W,CW>,work:&mut IndustrialDeliveryWorkV1,live:&ExecutionFence)->Result<(),IndustrialDeliveryErrorV1>{
 let mut after_q=None;for(column,before,after)in[(FixedEconomyColumnV1::TotalSupply,source.supply_before.total_supply,source.supply_after.total_supply),(FixedEconomyColumnV1::CirculatingSupply,source.supply_before.circulating_supply,source.supply_after.circulating_supply),(FixedEconomyColumnV1::TotalIssued,source.supply_before.total_issued,source.supply_after.total_issued),(FixedEconomyColumnV1::TotalBurned,source.supply_before.total_burned,source.supply_after.total_burned)]{let guard=ctx.transaction.guard();let next=change_qualified_fixed_economy_cell_v1(FixedEconomyCellChangeV1{column,qualification:after_q.as_ref().unwrap_or(&source.supply),expected:FixedEconomyValueV1::U64(before),after:FixedEconomyValueV1::U64(after),max_pending_node_bytes:work.max_pending_node_bytes},ctx,write,&guard,live)?;if next.is_some(){after_q=next;}}
 if let Some(after)=source.after.as_ref(){let guard=ctx.transaction.guard();change_world_balance_overlay_v1(WorldBalanceChangeV1{field:WorldBalanceFieldV1::MainToken,key:actor,expected:source.before.as_ref().map(WorldBalanceRefV1::from),after:Some(WorldBalanceRefV1::from(after)),max_current_identity_bytes:work.max_identity_bytes as u64},ctx,write,&guard,live)?;}
 for(key,after)in &source.treasury_after{let guard=ctx.transaction.guard();change_world_number_map_overlay_v1(WorldNumberMapChangeV1{field:WorldNumberMapFieldV1::MainTokenTreasuryBalances,key:WorldNumberMapKeyRefV1::Text(key),expected:source.treasury_before.get(key).copied().map(WorldNumberV1::Unsigned),after:Some(WorldNumberV1::Unsigned(*after))},ctx,write,&guard,live)?;}
 Ok(())
}

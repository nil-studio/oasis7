//! Complete expiry of one genuine selected Current restricted starter grant.
use super::*;
use crate::world::{incremental_normal_tail::NormalTailWorkV1,incremental_normal_world_rule_lifecycle::delivery::{IndustrialDeliveryWorkV1,IndustrialDeliveryErrorV1},incremental_body_event::materials::CorePolicyMaterialContextV1,incremental_body::BodyMutationContext};
use oasis7_formal_protocol::{incremental_value::{ValueNodeReader,ValueNodeWriter,ValueTreeError},incremental_state::StatePageWriter};
use oasis7_world_state::{RestrictedStarterClaimGrantState,RestrictedStarterClaimGrantStatus,incremental_field_overlay::WorldFieldOverlayContextV1,incremental_field_member::MemberRecordWriteContext,incremental_main_token_policy_records::*};
use oasis7_proto::memory_budget::MemoryClass;
use std::collections::BTreeMap;
pub(super) struct CommittedGrantEpochStateV1{_before:ReservedValue<CurrentRestrictedClaimGrantV1>,_after:RestrictedStarterClaimGrantState,_after_q:Option<ReservedValue<MainTokenPolicyRecordQualificationV1>>,_money:super::money::SelectedClaimMoneyV1}
#[allow(clippy::too_many_arguments)]
pub(super) fn apply_current_grant_expiry_v1<M:MerkleNodeReader,R:ValueNodeReader,W:ValueNodeReader+ValueNodeWriter+StatePageWriter,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,CW:FnMut()->bool,ME:FnMut()->Option<ValueTreeError>>(id:&str,epoch:u64,policy:&ReservedValue<MainTokenPolicyFamilyQualificationV1>,sources:&ClaimEpochStateSourcesV1<'_>,state:&M,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,write:&mut MemberRecordWriteContext<'_,W,CW>,materials:&mut CorePolicyMaterialContextV1<'_,M,ME>,work:&mut IndustrialDeliveryWorkV1,tail:&mut NormalTailWorkV1<'_,'_>,now:u64,live:&ExecutionFence)->Result<Option<PreparedClaimEpochMutationV1>,IndustrialDeliveryErrorV1>{
 tail.charge(id.len().checked_add(8192).ok_or(BodyMutationError::Invalid("grant metadata budget"))?)?;
 let guard=ctx.transaction.guard();let before=read_current_restricted_claim_grant_v1(id,policy,sources.roots,state,ctx,&mut work.fixed_scan,&mut work.fixed_cells_remaining,work.max_identity_bytes as u64,&guard,live)?;
 let Some(grant)=before.value()else{return Ok(None)};if grant.status!=RestrictedStarterClaimGrantStatus::Issued||epoch<grant.expires_at_epoch{return Ok(None)}
 let bytes=[id,grant.beneficiary_account_id.as_str(),&grant.issuer_id,&grant.issuance_reason,&grant.spend_scope,&grant.source_treasury_bucket_id,grant.status_reason.as_deref().unwrap_or("")].into_iter().try_fold(8192usize,|n,s|n.checked_add(s.len().checked_mul(8)?)).ok_or(BodyMutationError::Invalid("grant expiry clone layout"))?;
 tail.charge(bytes)?;let phase=tail.reserve(bytes,MemoryClass::Ordinary)?;
 let mut money=super::money::read(id,Some(&grant.source_treasury_bucket_id),sources,state,ctx,work,tail,live)?;
 let amount=money.before.as_ref().map_or(0,|v|v.restricted_starter_claim_balance);
 let event=evaluate::grant_expiry_v1(id,grant,epoch,amount);
 // The shared original fixed kernel sees only this actual selected account
 // and the three possible actual treasury points, never a World DTO.
 let mut balances=BTreeMap::new();if let Some(account)=&money.before{balances.insert(id.to_owned(),account.clone());}
 let(after,balances,treasury,supply)=oasis7_world_state::world::main_token_restricted_claim_publication::prepare_selected_restricted_grant_terminal_v1(grant.clone(),&balances,&money.treasury_before,&money.supply_before,id,&grant.issuer_id,&grant.issuance_reason,&grant.spend_scope,&grant.source_treasury_bucket_id,grant.issued_amount,amount,grant.issued_at_epoch,epoch,grant.expires_at_epoch,None)?;
 money.after=balances.get(id).cloned().or_else(||money.before.clone());money.treasury_after=treasury;money.supply_after=supply;
 {let mut mutation=BodyMutationContext{sink:&mut*write.sink,memory:write.memory,io:&mut*write.io,cancelled:&mut write.cancelled};let guard=ctx.transaction.guard();materials.contribute(ctx.transaction,ctx.values.source,&mut mutation,&guard,live)?;}
 super::money::commit(&money,id,ctx,write,work,live)?;
 let mut after_q=None;
 for(cell,expected,value)in[(MainTokenPolicyColumnV1::Status,MainTokenPolicyValueRefV1::Text("issued"),MainTokenPolicyValueRefV1::Text("expired")),(MainTokenPolicyColumnV1::StatusUpdatedAtEpoch,grant.status_updated_at_epoch.map_or(MainTokenPolicyValueRefV1::Unit,MainTokenPolicyValueRefV1::U64),MainTokenPolicyValueRefV1::U64(epoch)),(MainTokenPolicyColumnV1::StatusReason,grant.status_reason.as_deref().map_or(MainTokenPolicyValueRefV1::Unit,MainTokenPolicyValueRefV1::Text),MainTokenPolicyValueRefV1::Text("expired"))]{let guard=ctx.transaction.guard();let next=change_qualified_main_token_policy_cell_v1(MainTokenPolicyCellChangeV1{key:MainTokenPolicyKeyV1::Grant(id),cell:MainTokenPolicyCellV1::Column(cell),qualification:after_q.as_ref().or_else(||before.qualification()).ok_or(BodyMutationError::Invalid("grant expiry selected qualification"))?,expected,after:value,max_current_payload_bytes:work.max_identity_bytes as u64,max_pending_node_bytes:work.max_pending_node_bytes},ctx,write,&guard,live)?;if next.is_some(){after_q=next;}}
 let committed=CommittedGrantEpochStateV1{_before:before,_after:after,_after_q:after_q,_money:money};
 Ok(Some(PreparedClaimEpochMutationV1{event:Some(event),guard:ctx.transaction.guard(),identity:tail.identity(),memory:tail.memory().clone(),now,preserved_last_active:None,_phase:phase,_state:CommittedEpochStateV1::Grant(committed)}))
}

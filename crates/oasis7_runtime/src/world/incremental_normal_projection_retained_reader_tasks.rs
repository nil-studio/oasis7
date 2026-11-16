//! Original legacy-index/complete-chain traversal over authenticated paid leaves.
//! The complete SDK fields and signed lookup scope are checked before a row
//! reaches its consumer. This module neither selects a current task nor admits a reader.
use super::*;
use super::decode::{self, PaidDecodedRetainedServiceDataV1};
use crate::world::incremental_body::BodyMutationError;
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::MemoryReservation;
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_world_state::formal_retained_history::{FormalRetainedHistoryRootV1,GameTaskControlLookupV1,RetainedHistoryLeafV1};
use std::mem::size_of;
fn original<E:std::error::Error+Send+Sync+'static>(e:E)->BodyMutationError{BodyMutationError::Original(Box::new(e))}
fn reserve(n:usize,common:&mut FormalBoundedContextV1<'_>)->Result<MemoryReservation,BodyMutationError>{common.charge(1,n as u64).map_err(original)?;common.memory().reserve(n,MemoryClass::Ordinary).map_err(|e|ValueTreeError::Memory(e).into())}
fn text(s:&str,common:&mut FormalBoundedContextV1<'_>)->Result<ReservedValue<String>,BodyMutationError>{let n=size_of::<String>().checked_add(s.len()).ok_or(BodyMutationError::Invalid("retained task cursor layout"))?;let fee=reserve(n,common)?;let mut value=Vec::with_capacity(s.len());if value.capacity()!=s.len(){return Err(BodyMutationError::Invalid("retained task cursor capacity"))}for part in s.as_bytes().chunks(8192){common.charge(1,part.len()as u64).map_err(original)?;value.extend_from_slice(part)}Ok(fee.retain(String::from_utf8(value).map_err(original)?))}
// All task references and IDs are ASCII digests. Arbitrary UTF-8 task scope
// strings are never copied through this cursor helper.
fn same(a:&str,b:&str,common:&mut FormalBoundedContextV1<'_>)->Result<bool,BodyMutationError>{common.charge(1,(a.len()+b.len())as u64).map_err(original)?;Ok(a==b)}
fn matches_scope(result:&crate::service_admission::CanonicalIntentResultV1,lookup:&GameTaskControlLookupV1,common:&mut FormalBoundedContextV1<'_>,canonical:&mut CanonicalCborWorkV1)->Result<bool,BodyMutationError>{
 common.consume_steps(1).map_err(original)?;if result.rejected.is_some(){return Ok(false)}
 let Some((agent,owner,generation,key,world,genesis,branch,epoch))=oasis7_world_state::formal_retained_history::game_task_result_scope_data_v1(result)else{return Ok(false)};
 // game_task_result_lookup originally hashes the signed subject even when
 // another scope field differs. Preserve that evaluation order.
 let encoded=super::canonical::encode_original_canonical_data_v1(&("owner-public-key",key),common,canonical)?;
 let digest=decode::hash(&encoded,common)?;
 for(a,b)in [(agent,lookup.agent_id.as_str()),(owner,lookup.owner_binding.as_str())]{if !same(a,b,common)?{return Ok(false)}}
 common.consume_steps(1).map_err(original)?;if generation!=lookup.owner_generation{return Ok(false)}
 for(a,b)in [(digest.as_str(),lookup.owner_public_key_digest.as_str()),(world,lookup.world_id.as_str()),(genesis,lookup.genesis_digest.as_str()),(branch,lookup.branch_id.as_str())]{if !same(a,b,common)?{return Ok(false)}}
 common.consume_steps(1).map_err(original)?;Ok(epoch==lookup.reorg_epoch)
}
/// The complete original result order: legacy ordered references first, then
/// newest-to-oldest linked revisions. Every row has passed the original SDK
/// fields and scope predicates. An error never returns a successful prefix.
pub(super) fn visit_task_results_data_v1<R:PaidProjectionRetainedOriginalReaderV1>(
 root:&FormalRetainedHistoryRootV1,lookup:&GameTaskControlLookupV1,reader:&mut R,
 common:&mut FormalBoundedContextV1<'_>,canonical:&mut CanonicalCborWorkV1,
 mut visit:impl FnMut(PaidDecodedRetainedServiceDataV1,&mut FormalBoundedContextV1<'_>,&mut CanonicalCborWorkV1)->Result<(),BodyMutationError>,
)->Result<(),BodyMutationError>where R::RetainedError:std::error::Error+Send+Sync+'static{
 decode::validate_root(root,common,canonical)?;
 // Original canonicalized() performs these exact flat string checks and clone.
 // Keep its actual clone fee alive for the whole lookup and preserve its error.
 let mut n=size_of::<GameTaskControlLookupV1>();for s in [&lookup.agent_id,&lookup.owner_binding,&lookup.owner_public_key_digest,&lookup.world_id,&lookup.genesis_digest,&lookup.branch_id]{common.charge(1,s.len()as u64).map_err(original)?;n=n.checked_add(s.len()).ok_or(BodyMutationError::Invalid("retained task lookup layout"))?;}
 let lookup_fee=reserve(n+size_of::<oasis7_world_state::formal_retained_history::RetainedHistoryReadError>()+128,common)?;
 let lookup=match lookup.canonicalized(){Ok(value)=>lookup_fee.retain(value),Err(error)=>return Err(BodyMutationError::Original(Box::new(LookupFailure{error,_fee:lookup_fee})))};
 let key=decode::key("game-task-control",&*lookup,common,canonical)?;
 if let Some(leaf)=decode::read_leaf(root,&key,reader,common,canonical)?{let RetainedHistoryLeafV1::GameTaskControlIndex(entry)=leaf.leaf()else{return Err(decode::invalid("legacy task index leaf type mismatch",common))};
  common.consume_steps(1).map_err(original)?;
  // Original short-circuit checks: count, order, then each digest shape.
  if entry.result_key_digests.is_empty()||entry.result_key_digests.len()>2{return Err(decode::invalid("retained result index references are malformed",common))}
  for pair in entry.result_key_digests.windows(2){common.charge(1,(pair[0].len()+pair[1].len())as u64).map_err(original)?;if pair[0]>=pair[1]{return Err(decode::invalid("retained result index references are malformed",common))}}
  for reference in &entry.result_key_digests{common.charge(1,reference.len()as u64).map_err(original)?;if !oasis7_world_state::formal_retained_history::is_correlation_key_digest(reference){return Err(decode::invalid("retained result index references are malformed",common))}}
  for reference in &entry.result_key_digests{common.consume_steps(1).map_err(original)?;let result=decode::read_result_data(root,reference,reader,common,canonical)?.ok_or_else(||decode::invalid("legacy task index result absent",common))?;if !matches_scope(result.result(),&lookup,common,canonical)?{return Err(decode::invalid("legacy task index scope mismatch",common))}visit(result,common,canonical)?;}
 }
 let key=decode::key("game-task-control-chain-v1",&*lookup,common,canonical)?;
 let Some(head)=decode::read_leaf(root,&key,reader,common,canonical)?else{return Ok(())};
 let RetainedHistoryLeafV1::GameTaskControlChainIndex(entry)=head.leaf()else{return Err(decode::invalid("retained task chain head type mismatch",common))};
 common.consume_steps(1).map_err(original)?;if entry.count==0{return Err(decode::invalid("retained task chain is empty",common))}if entry.count>4096{return Err(decode::unavailable("retained task chain exceeds query budget",common))}
 let count=entry.count as usize;let n=size_of::<Vec<ReservedValue<String>>>().checked_add(count.checked_mul(size_of::<ReservedValue<String>>()).ok_or(BodyMutationError::Invalid("retained task seen layout"))?).ok_or(BodyMutationError::Invalid("retained task seen layout"))?;
 let _seen_fee=reserve(n,common)?;let mut seen:Vec<ReservedValue<String>>=Vec::with_capacity(count);if seen.capacity()!=count{return Err(BodyMutationError::Invalid("retained task seen capacity"))}
 let mut cursor=Some(text(&entry.head,common)?);let mut actual=0;
 while let Some(reference)=cursor{common.consume_steps(1).map_err(original)?;if actual>=count{return Err(decode::invalid("retained task chain count/cycle/reference mismatch",common))}
  for previous in &seen{if same(previous,&reference,common)?{return Err(decode::invalid("retained task chain count/cycle/reference mismatch",common))}}
  common.charge(1,reference.len()as u64).map_err(original)?;if !oasis7_world_state::formal_retained_history::is_correlation_key_digest(&reference){return Err(decode::invalid("retained task chain count/cycle/reference mismatch",common))}
  let key=decode::key("game-task-control-link",reference.as_str(),common,canonical)?;
  let link=decode::read_leaf(root,&key,reader,common,canonical)?.ok_or_else(||decode::invalid("retained task chain link absent",common))?;
  let RetainedHistoryLeafV1::GameTaskControlLink(link)=link.leaf()else{return Err(decode::invalid("retained task chain link absent",common))};
  if !same(&link.result_key_digest,&reference,common)?{return Err(decode::invalid("retained task chain link identity mismatch",common))}
  let result=decode::read_result_data(root,&reference,reader,common,canonical)?.ok_or_else(||decode::invalid("retained task chain result absent",common))?;
  if !matches_scope(result.result(),&lookup,common,canonical)?{return Err(decode::invalid("retained task chain result scope mismatch",common))}
  visit(result,common,canonical)?;
  cursor=link.previous.as_deref().map(|s|text(s,common)).transpose()?;seen.push(reference);actual+=1;
 }
 if actual!=count{return Err(decode::invalid("retained task chain incomplete",common))}Ok(())
}
#[derive(Debug)]struct LookupFailure{error:oasis7_world_state::formal_retained_history::RetainedHistoryReadError,_fee:MemoryReservation}
impl std::fmt::Display for LookupFailure{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{std::fmt::Display::fmt(&self.error,f)}}impl std::error::Error for LookupFailure{}

/// Query-local authenticated result DATA, retaining both original leaf and
/// decoder graph charges. It is not a current-control/audience receipt.
pub(in crate::world) struct PaidRetainedTaskResultsDataV1 {
 values:Vec<PaidDecodedRetainedServiceDataV1>,slots:MemoryReservation,_fee:MemoryReservation,
}
impl PaidRetainedTaskResultsDataV1 {
 pub(in crate::world) fn into_values_data_v1(self)->impl Iterator<Item=PaidDecodedRetainedServiceDataV1>{
  // Keep the query slot fee with the iterator until every moved row owner has
  // left it. No original/decoder fee is detached from a row.
  struct Values{iter:std::vec::IntoIter<PaidDecodedRetainedServiceDataV1>,_slots:MemoryReservation,_fee:MemoryReservation}
  impl Iterator for Values{type Item=PaidDecodedRetainedServiceDataV1;fn next(&mut self)->Option<Self::Item>{self.iter.next()}}
  Values{iter:self.values.into_iter(),_slots:self.slots,_fee:self._fee}
 }
 pub(in crate::world) fn results(&self)->impl Iterator<Item=&crate::service_admission::CanonicalIntentResultV1>{self.values.iter().map(|v|v.result())}
}
pub(in crate::world) fn read_task_results_data_v1<R:PaidProjectionRetainedOriginalReaderV1>(root:&FormalRetainedHistoryRootV1,lookup:&GameTaskControlLookupV1,reader:&mut R,common:&mut FormalBoundedContextV1<'_>,canonical:&mut CanonicalCborWorkV1)->Result<PaidRetainedTaskResultsDataV1,BodyMutationError>where R::RetainedError:std::error::Error+Send+Sync+'static{
 let fee=reserve(size_of::<PaidRetainedTaskResultsDataV1>(),common)?;let slots=reserve(0,common)?;
 let mut output=PaidRetainedTaskResultsDataV1{values:Vec::new(),slots,_fee:fee};
 visit_task_results_data_v1(root,lookup,reader,common,canonical,|value,common,_|{
  common.consume_steps(1).map_err(original)?;
  if output.values.len()==output.values.capacity(){let capacity=output.values.capacity().checked_mul(2).unwrap_or(usize::MAX).max(4);let n=capacity.checked_mul(size_of::<PaidDecodedRetainedServiceDataV1>()).ok_or(BodyMutationError::Invalid("retained task result slots layout"))?;let fee=reserve(n,common)?;common.charge(1,(output.values.len()*size_of::<PaidDecodedRetainedServiceDataV1>())as u64).map_err(original)?;output.values.try_reserve_exact(capacity-output.values.len()).map_err(|_|BodyMutationError::Invalid("retained task result slots allocation"))?;if output.values.capacity()!=capacity{return Err(BodyMutationError::Invalid("retained task result slots capacity"))}output.slots=fee;}
  output.values.push(value);Ok(())
 })?;Ok(output)
}

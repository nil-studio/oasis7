//! Complete original AgentChat retained-index read over authenticated leaves.
use super::*;
use super::decode::{self,PaidDecodedRetainedServiceDataV1};
use crate::world::incremental_body::BodyMutationError;
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::MemoryReservation;
use oasis7_world_state::formal_retained_history::{AgentChatIntentLookupV1,FormalRetainedHistoryRootV1,RetainedHistoryLeafV1};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use std::mem::size_of;
fn original<E:std::error::Error+Send+Sync+'static>(e:E)->BodyMutationError{BodyMutationError::Original(Box::new(e))}
fn reserve(n:usize,c:&mut FormalBoundedContextV1<'_>)->Result<MemoryReservation,BodyMutationError>{c.charge(1,n as u64).map_err(original)?;c.memory().reserve(n,MemoryClass::Ordinary).map_err(|e|ValueTreeError::Memory(e).into())}
pub(in crate::world) struct PaidRetainedChatResultsDataV1{values:Vec<PaidDecodedRetainedServiceDataV1>,_slots:MemoryReservation,_fee:MemoryReservation}
impl PaidRetainedChatResultsDataV1{pub(in crate::world) fn into_values_data_v1(self)->impl Iterator<Item=PaidDecodedRetainedServiceDataV1>{struct Values{iter:std::vec::IntoIter<PaidDecodedRetainedServiceDataV1>,_slots:MemoryReservation,_fee:MemoryReservation}impl Iterator for Values{type Item=PaidDecodedRetainedServiceDataV1;fn next(&mut self)->Option<Self::Item>{self.iter.next()}}Values{iter:self.values.into_iter(),_slots:self._slots,_fee:self._fee}}}
pub(in crate::world) fn read_chat_results_data_v1<R:PaidProjectionRetainedOriginalReaderV1>(root:&FormalRetainedHistoryRootV1,lookup:&AgentChatIntentLookupV1,reader:&mut R,common:&mut FormalBoundedContextV1<'_>,canonical:&mut CanonicalCborWorkV1)->Result<PaidRetainedChatResultsDataV1,BodyMutationError>where R::RetainedError:std::error::Error+Send+Sync+'static{
 decode::validate_root(root,common,canonical)?;let mut n=size_of::<AgentChatIntentLookupV1>();for value in [&lookup.agent_id,&lookup.intent_id,&lookup.owner_public_key_digest,&lookup.player_id,&lookup.world_id,&lookup.branch_id,&lookup.authority_scope]{for part in value.as_bytes().chunks(8192){common.charge(1,part.len()as u64).map_err(original)?;}n=n.checked_add(value.len()).ok_or(BodyMutationError::Invalid("retained chat lookup layout"))?;}
 let fee=reserve(n.checked_add(size_of::<LookupFailure>()+128).ok_or(BodyMutationError::Invalid("retained chat lookup error layout"))?,common)?;
 let lookup=match lookup.canonicalized(){Ok(value)=>fee.retain(value),Err(error)=>return Err(original(LookupFailure{error,_fee:fee}))};
 let key=decode::key("agent-chat-intent",&*lookup,common,canonical)?;let leaf=decode::read_leaf(root,&key,reader,common,canonical)?;
 let refs=match &leaf{None=>None,Some(leaf)=>match leaf.leaf(){RetainedHistoryLeafV1::AgentChatIntentIndex(entry)=>Some(&entry.result_key_digests),_=>return Err(decode::invalid("retained AgentChat index leaf kind mismatch",common))}};
 let Some(refs)=refs else{let fee=reserve(size_of::<PaidRetainedChatResultsDataV1>(),common)?;let slots=reserve(0,common)?;return Ok(PaidRetainedChatResultsDataV1{values:Vec::new(),_slots:slots,_fee:fee})};
 common.consume_steps(1).map_err(original)?;if refs.is_empty()||refs.len()>2{return Err(decode::invalid("retained result index references are malformed",common))}
 for pair in refs.windows(2){common.charge(1,(pair[0].len()+pair[1].len())as u64).map_err(original)?;if pair[0]>=pair[1]{return Err(decode::invalid("retained result index references are malformed",common))}}
 for reference in refs{common.charge(1,reference.len()as u64).map_err(original)?;if !oasis7_world_state::formal_retained_history::is_correlation_key_digest(reference){return Err(decode::invalid("retained result index references are malformed",common))}}
 let count=refs.len();let fee=reserve(size_of::<PaidRetainedChatResultsDataV1>(),common)?;let slots=reserve(count.checked_mul(size_of::<PaidDecodedRetainedServiceDataV1>()).ok_or(BodyMutationError::Invalid("retained chat result slots"))?,common)?;let mut output=PaidRetainedChatResultsDataV1{values:Vec::with_capacity(count),_slots:slots,_fee:fee};if output.values.capacity()!=count{return Err(BodyMutationError::Invalid("retained chat result capacity"))}
 for reference in refs{common.consume_steps(1).map_err(original)?;let result=decode::read_result_data(root,reference,reader,common,canonical)?.ok_or_else(||decode::invalid("retained chat index points to an absent result",common))?;
  let parts=oasis7_world_state::formal_retained_history::retained_chat_scope_data_with_work_v1(result.result(),&lookup.intent_id,&mut|s,b|common.charge(s,b).map_err(original))?;let Some(parts)=parts else{return Err(decode::invalid("retained chat index points to a nonmatching result",common))};
  let digest=decode::canonical_task_owner_digest_data_v1(parts.public_key(),common,canonical)?;
  if !parts.matches_with_digest_data_v1(&lookup,&digest,&mut|s,b|common.charge(s,b).map_err(original))?{return Err(decode::invalid("retained chat index points to a nonmatching result",common))}
  output.values.push(result);
 }Ok(output)
}
#[derive(Debug)]struct LookupFailure{error:oasis7_world_state::formal_retained_history::RetainedHistoryReadError,_fee:MemoryReservation}
impl std::fmt::Display for LookupFailure{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{std::fmt::Display::fmt(&self.error,f)}}impl std::error::Error for LookupFailure{}

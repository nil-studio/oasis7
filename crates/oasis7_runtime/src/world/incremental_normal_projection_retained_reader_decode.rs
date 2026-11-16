//! Paid original retained leaf/result codecs. The root is DATA supplied only
//! by the fixed actual-Current fact reader; no accepted source is minted here.
use super::*;
use super::{get_retained_index_leaf_bounded_v1,canonical::encode_original_canonical_data_v1};
use crate::world::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{controlled_authority::formal::{bounded_cbor_data::{self as cbor,CborDataLayoutV1,CborDataLimitsV1,PaidCborDataV1},business_index::{PaidBusinessIndexLeafV1,validate_business_index_root_bounded_v1,BUSINESS_INDEX_MAX_LEAF_BYTES}},incremental_value::ValueTreeError};
use oasis7_proto::memory_budget::MemoryReservation;
use oasis7_world_state::formal_retained_history::{FormalRetainedHistoryRootV1,RetainedHistoryLeafV1,RetainedHistoryReadError};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use std::{borrow::Borrow,mem::size_of};
fn original<E:std::error::Error+Send+Sync+'static>(error:E)->BodyMutationError{BodyMutationError::Original(Box::new(error))}
#[derive(Debug)]struct Protocol{error:RetainedHistoryReadError,_fee:MemoryReservation}
impl std::fmt::Display for Protocol{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{std::fmt::Display::fmt(&self.error,f)}}impl std::error::Error for Protocol{}
pub(super) fn invalid(note:&'static str,common:&mut FormalBoundedContextV1<'_>)->BodyMutationError{
 // Admission failures remain typed resource errors; no protocol string is
 // allocated before its own ordinary fee exists.
 if let Err(error)=common.charge(1,note.len()as u64){return original(error)}
 match common.memory().reserve(size_of::<Protocol>()+note.len(),MemoryClass::Ordinary){Ok(fee)=>original(Protocol{error:RetainedHistoryReadError::Invalid(note.into()),_fee:fee}),Err(error)=>ValueTreeError::Memory(error).into()}
}
pub(super) fn unavailable(note:&'static str,common:&mut FormalBoundedContextV1<'_>)->BodyMutationError{
 if let Err(error)=common.charge(1,note.len()as u64){return original(error)}
 match common.memory().reserve(size_of::<Protocol>()+note.len(),MemoryClass::Ordinary){Ok(fee)=>original(Protocol{error:RetainedHistoryReadError::Unavailable(note.into()),_fee:fee}),Err(error)=>ValueTreeError::Memory(error).into()}
}
#[derive(Debug)]struct CodecFailure{kind:CodecKind,_fee:MemoryReservation}
enum CodecKind{Refusal(cbor::PaidCborRefusalV1),Resource(cbor::BoundedCborDataErrorV1)}
impl std::fmt::Debug for CodecKind{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{match self{Self::Refusal(e)=>f.debug_tuple("RetainedCodec").field(&e.message()).finish(),Self::Resource(e)=>std::fmt::Debug::fmt(e,f)}}}
impl std::fmt::Display for CodecFailure{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{match &self.kind{CodecKind::Refusal(e)=>write!(f,"invalid retained history: {}",e.message()),CodecKind::Resource(e)=>std::fmt::Display::fmt(e,f)}}}impl std::error::Error for CodecFailure{}
fn decode_with_wire_limit<T:serde::de::DeserializeOwned>(raw:&[u8],layout:CborDataLayoutV1,max_wire_bytes:usize,common:&mut FormalBoundedContextV1<'_>)->Result<PaidCborDataV1<T>,BodyMutationError>{
 common.charge(1,size_of::<CodecFailure>()as u64).map_err(original)?;let error_fee=common.memory().reserve(size_of::<CodecFailure>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let limits=CborDataLimitsV1{max_wire_bytes,max_graph_bytes:usize::MAX,max_depth:usize::MAX};
 match cbor::decode_closed_cbor_data_v1(raw,layout,&limits,common){Ok(Ok(value))=>Ok(value),Ok(Err(error))=>Err(original(CodecFailure{kind:CodecKind::Refusal(error),_fee:error_fee})),Err(error)=>Err(original(CodecFailure{kind:CodecKind::Resource(error),_fee:error_fee}))}
}
fn decode<T:serde::de::DeserializeOwned>(raw:&[u8],layout:CborDataLayoutV1,common:&mut FormalBoundedContextV1<'_>)->Result<PaidCborDataV1<T>,BodyMutationError>{decode_with_wire_limit(raw,layout,BUSINESS_INDEX_MAX_LEAF_BYTES,common)}
fn convert(v:oasis7_client_api::consensus_action_wire::ConsensusPayloadOwnedLayoutV1)->CborDataLayoutV1{CborDataLayoutV1{root:v.root,sequence_element:v.sequence_element,map_pair:v.map_pair,set_element:v.set_element,box_pointee:v.box_pointee,default_heap_per_value:v.default_heap_per_value,symbols_bytes:v.symbols_bytes,visits:v.visits,missing:v.missing}}
pub(super) fn hash(raw:&[u8],common:&mut FormalBoundedContextV1<'_>)->Result<ReservedValue<String>,BodyMutationError>{
 common.charge(1,(size_of::<String>()+64)as u64).map_err(original)?;let fee=common.memory().reserve(size_of::<String>()+64,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;let mut digest=blake3::Hasher::new();for part in raw.chunks(8192){common.charge(1+(part.len()as u64).div_ceil(64),part.len()as u64).map_err(original)?;digest.update(part);}common.charge(1,32).map_err(original)?;let digest=digest.finalize();let mut text=String::with_capacity(64);if text.capacity()!=64{return Err(invalid("retained digest capacity",common))}const HEX:&[u8;16]=b"0123456789abcdef";for byte in digest.as_bytes(){common.charge(1,2).map_err(original)?;text.push(HEX[(byte>>4)as usize]as char);text.push(HEX[(byte&15)as usize]as char)}Ok(fee.retain(text))
}
fn equal(a:&[u8],b:&[u8],common:&mut FormalBoundedContextV1<'_>)->Result<bool,BodyMutationError>{common.consume_steps(1).map_err(original)?;if a.len()!=b.len(){return Ok(false)}for(a,b)in a.chunks(8192).zip(b.chunks(8192)){common.charge(1,(a.len()+b.len())as u64).map_err(original)?;if a!=b{return Ok(false)}}Ok(true)}
/// Original validation order: schema -> paid original index validation ->
/// root digest shape -> original ABI tuple encoding/hash -> exact comparison.
pub(super) fn validate_root(root:&FormalRetainedHistoryRootV1,common:&mut FormalBoundedContextV1<'_>,canonical:&mut CanonicalCborWorkV1)->Result<(),BodyMutationError>{
 common.consume_steps(1).map_err(original)?;if root.schema_version!=1{return Err(invalid("unsupported formal retained-history root schema",common))}
 validate_business_index_root_bounded_v1(&root.index_root,common).map_err(original)?;
 if root.root_digest.len()!=64{return Err(invalid("formal retained-history root digest mismatch",common))}for byte in root.root_digest.bytes(){common.charge(1,1).map_err(original)?;if !byte.is_ascii_hexdigit(){return Err(invalid("formal retained-history root digest mismatch",common))}}
 let bytes=encode_original_canonical_data_v1(&("oasis7.formal.retained-history.root.v1",root.schema_version,&root.index_root),common,canonical)?;let actual=hash(&bytes,common)?;
 if !equal(root.root_digest.as_bytes(),actual.as_bytes(),common)?{return Err(invalid("formal retained-history root digest mismatch",common))}Ok(())
}
pub(super) struct PaidRetainedLeafDataV1{value:PaidCborDataV1<RetainedHistoryLeafV1>,_index:PaidBusinessIndexLeafV1}
impl PaidRetainedLeafDataV1{pub(super) fn leaf(&self)->&RetainedHistoryLeafV1{self.value.borrow()}}
pub(super) fn read_leaf<R:PaidProjectionRetainedOriginalReaderV1>(root:&FormalRetainedHistoryRootV1,key:&str,reader:&mut R,common:&mut FormalBoundedContextV1<'_>,canonical:&mut CanonicalCborWorkV1)->Result<Option<PaidRetainedLeafDataV1>,BodyMutationError>where R::RetainedError:std::error::Error+Send+Sync+'static{
 let Some(index)=get_retained_index_leaf_bounded_v1(&root.index_root,key,reader,common).map_err(original)?else{return Ok(None)};index.validate_context(common).map_err(original)?;
 let leaf=index.leaf();common.charge(1,(leaf.key_digest.len()+key.len())as u64).map_err(original)?;if leaf.key_digest!=key{return Err(invalid("retained-history leaf key mismatch",common))}
 let layout=oasis7_world_state::formal_retained_history::retained_history_leaf_owned_layout_v1(&mut||common.consume_steps(1).map_err(original))?;
 let value=decode::<RetainedHistoryLeafV1>(&leaf.canonical_value,convert(layout),common)?;value.validate_context(common).map_err(original)?;
 let decoded:&RetainedHistoryLeafV1=value.borrow();common.consume_steps(1).map_err(original)?;
 if leaf.kind!=oasis7_world_state::formal_retained_history::retained_leaf_kind(decoded){return Err(invalid("retained-history leaf kind does not match its value type",common))}
 let encoded=encode_original_canonical_data_v1(decoded,common,canonical)?;
 if !equal(&encoded,&leaf.canonical_value,common)?{return Err(invalid("retained-history leaf is not canonical",common))}
 Ok(Some(PaidRetainedLeafDataV1{value,_index:index}))
}
/// Original namespace and exact ABI canonical key preimage. Output is not an
/// archive read permission; the fixed caller binds its actual Current root.
pub(super) fn key<T:serde::Serialize+?Sized>(namespace:&str,value:&T,common:&mut FormalBoundedContextV1<'_>,canonical:&mut CanonicalCborWorkV1)->Result<ReservedValue<String>,BodyMutationError>{
 let bytes=encode_original_canonical_data_v1(&("oasis7.formal.retained-history.key.v1",namespace,value),common,canonical)?;hash(&bytes,common)
}
fn encode_result_limit<T:serde::Serialize>(value:&T,max_wire_bytes:usize,common:&mut FormalBoundedContextV1<'_>)->Result<PaidCborDataV1<Vec<u8>>,BodyMutationError>{
 common.charge(1,size_of::<CodecFailure>()as u64).map_err(original)?;let error_fee=common.memory().reserve(size_of::<CodecFailure>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let limits=CborDataLimitsV1{max_wire_bytes,max_graph_bytes:usize::MAX,max_depth:usize::MAX};
 match cbor::encode_closed_cbor_data_v1(value,&limits,common){Ok(Ok(value))=>Ok(value),Ok(Err(error))=>Err(original(CodecFailure{kind:CodecKind::Refusal(error),_fee:error_fee})),Err(error)=>Err(original(CodecFailure{kind:CodecKind::Resource(error),_fee:error_fee}))}
}
fn encode_result<T:serde::Serialize>(value:&T,common:&mut FormalBoundedContextV1<'_>)->Result<PaidCborDataV1<Vec<u8>>,BodyMutationError>{encode_result_limit(value,BUSINESS_INDEX_MAX_LEAF_BYTES,common)}
/// Authenticated canonical result DATA, including the unique complete SDK
/// correlation/receipt predicates. Current owner/task selection remains separate.
pub(in crate::world) struct PaidDecodedRetainedServiceDataV1{result:PaidCborDataV1<crate::service_admission::CanonicalIntentResultV1>,_leaf:PaidRetainedLeafDataV1}
impl PaidDecodedRetainedServiceDataV1{pub(in crate::world) fn result(&self)->&crate::service_admission::CanonicalIntentResultV1{self.result.borrow()}}
pub(super) fn read_result_data<R:PaidProjectionRetainedOriginalReaderV1>(root:&FormalRetainedHistoryRootV1,correlation_key:&str,reader:&mut R,common:&mut FormalBoundedContextV1<'_>,canonical:&mut CanonicalCborWorkV1)->Result<Option<PaidDecodedRetainedServiceDataV1>,BodyMutationError>where R::RetainedError:std::error::Error+Send+Sync+'static{
 common.charge(1,correlation_key.len()as u64).map_err(original)?;
 if !oasis7_world_state::formal_retained_history::is_correlation_key_digest(correlation_key){return Err(invalid("WorldService correlation key digest is malformed",common))}
 let selected=key("world-service-result",&correlation_key,common,canonical)?;
 let Some(leaf)=read_leaf(root,&selected,reader,common,canonical)?else{return Ok(None)};
 let RetainedHistoryLeafV1::WorldServiceResult(entry)=leaf.leaf()else{return Err(invalid("retained WorldService result leaf kind mismatch",common))};
 common.charge(1,(entry.correlation_key_digest.len()+correlation_key.len())as u64).map_err(original)?;
 if entry.correlation_key_digest!=correlation_key{return Err(invalid("retained WorldService result key mismatch",common))}
 let layout=oasis7_client_api::consensus_action_wire::canonical_intent_result_owned_layout_v1(&mut||common.consume_steps(1).map_err(original))?;
 let result=decode::<crate::service_admission::CanonicalIntentResultV1>(&entry.canonical_result_cbor,convert(layout),common)?;result.validate_context(common).map_err(original)?;
 let value:&crate::service_admission::CanonicalIntentResultV1=result.borrow();let exact=encode_result(value,common)?;
 let exact_bytes:&Vec<u8>=exact.borrow();
 if !equal(exact_bytes,&entry.canonical_result_cbor,common)?{return Err(invalid("retained WorldService result bytes are not canonical",common))}
 let digest=hash(exact_bytes,common)?;if !equal(digest.as_bytes(),entry.canonical_result_digest.as_bytes(),common)?{return Err(invalid("retained WorldService result bytes are not canonical",common))}
 let derived=crate::world::normal_bootstrap_current::hash::with_current_meter_v1(common,canonical,|meter|oasis7_client_api::world_service::request_digest_with_work_v1("request-key",&value.request.correlation.key,meter))?.map_err(original)?;
 if !equal(derived.as_str().as_bytes(),correlation_key.as_bytes(),common)?{return Err(invalid("retained WorldService result correlation mismatch",common))}
 // The shared SDK owns the original request.validate -> derive correlation ->
 // key digest -> committed height -> receipt order for every payload variant.
 crate::world::normal_bootstrap_current::hash::with_current_meter_v1(common,canonical,|meter|
  oasis7_client_api::legacy_world_service::validate_canonical_result_fields_with_work_v1(correlation_key,value,meter)
 )?.map_err(original)?;
 Ok(Some(PaidDecodedRetainedServiceDataV1{result,_leaf:leaf}))
}

struct JsonInput<'a,'b>{raw:&'a[u8],position:usize,common:&'a mut FormalBoundedContextV1<'b>,first:Option<BodyMutationError>}
impl std::io::Read for JsonInput<'_, '_>{fn read(&mut self,out:&mut[u8])->std::io::Result<usize>{if self.first.is_some(){return Err(std::io::ErrorKind::Other.into())}let n=out.len().min(8192).min(self.raw.len()-self.position);if let Err(e)=self.common.charge(1,n as u64){self.first=Some(original(e));return Err(std::io::ErrorKind::Other.into())}out[..n].copy_from_slice(&self.raw[self.position..self.position+n]);self.position+=n;Ok(n)}}
/// Fixed hot-row codec. The row bytes come from the actual retained auxiliary
/// column; decoding them creates DATA only. The temporary JSON Value has no
/// serde defaults, so one original header per wire byte bounds its full graph.
fn decode_original_json_data<T:serde::de::DeserializeOwned>(raw:&[u8],layout:CborDataLayoutV1,common:&mut FormalBoundedContextV1<'_>)->Result<PaidCborDataV1<T>,BodyMutationError>{
 let n=raw.len().checked_add(1).and_then(|n|n.checked_mul(size_of::<serde_json::Value>().max(size_of::<String>()))).and_then(|n|n.checked_mul(8)).and_then(|n|n.checked_add(raw.len().checked_mul(2)?)).and_then(|n|n.checked_add(32768)).ok_or(BodyMutationError::Invalid("hot service JSON graph layout"))?;
 common.charge(n as u64,n as u64).map_err(original)?;let input_phase=common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let mut input=JsonInput{raw,position:0,common,first:None};let value=serde_json::from_reader::<_,serde_json::Value>(&mut input);if let Some(e)=input.first.take(){return Err(e)}
 let value=match value{Ok(v)=>v,Err(error)=>return Err(original(HotJsonFailure{error,_fee:input_phase}))};
 let common=&mut *input.common;
 let limits=CborDataLimitsV1{max_wire_bytes:usize::MAX,max_graph_bytes:usize::MAX,max_depth:usize::MAX};
 common.charge(1,size_of::<CodecFailure>()as u64).map_err(original)?;let error_fee=common.memory().reserve(size_of::<CodecFailure>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let result=match cbor::decode_closed_json_value_data_v1::<T>(value,layout,&limits,common){Ok(Ok(value))=>value,Ok(Err(error))=>return Err(original(CodecFailure{kind:CodecKind::Refusal(error),_fee:error_fee})),Err(error)=>return Err(original(CodecFailure{kind:CodecKind::Resource(error),_fee:error_fee}))};
 result.validate_context(common).map_err(original)?;
 drop(input_phase);Ok(result)
}
pub(in crate::world) fn decode_hot_service_result_json_data_v1(key:&str,raw:&[u8],common:&mut FormalBoundedContextV1<'_>,canonical:&mut CanonicalCborWorkV1)->Result<PaidCborDataV1<crate::service_admission::CanonicalIntentResultV1>,BodyMutationError>{let layout=oasis7_client_api::consensus_action_wire::canonical_intent_result_owned_layout_v1(&mut||common.consume_steps(1).map_err(original))?;let result=decode_original_json_data(raw,convert(layout),common)?;crate::world::normal_bootstrap_current::hash::with_current_meter_v1(common,canonical,|meter|oasis7_client_api::legacy_world_service::validate_canonical_result_fields_with_work_v1(key,result.borrow(),meter))?.map_err(original)?;Ok(result)}
pub(in crate::world) fn decode_current_task_terminal_record_data_v2(raw:&[u8],common:&mut FormalBoundedContextV1<'_>)->Result<PaidCborDataV1<oasis7_world_state::TaskTerminalRecordV2>,BodyMutationError>{let layout=oasis7_world_state::task_terminal_record_owned_layout_v2(&mut||common.consume_steps(1).map_err(original))?;decode_original_json_data(raw,convert(layout),common)}

#[derive(Debug)]struct HotJsonFailure{error:serde_json::Error,_fee:MemoryReservation}
impl std::fmt::Display for HotJsonFailure{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{std::fmt::Display::fmt(&self.error,f)}}impl std::error::Error for HotJsonFailure{}

pub(in crate::world) fn canonical_task_owner_digest_data_v1(owner:&str,common:&mut FormalBoundedContextV1<'_>,canonical:&mut CanonicalCborWorkV1)->Result<ReservedValue<String>,BodyMutationError>{let bytes=encode_original_canonical_data_v1(&("owner-public-key",owner),common,canonical)?;hash(&bytes,common)}

pub(in crate::world) fn is_canonical_empty_retained_root_data_v1(root:&FormalRetainedHistoryRootV1,common:&mut FormalBoundedContextV1<'_>,canonical:&mut CanonicalCborWorkV1)->Result<bool,BodyMutationError>{
 let empty=oasis7_formal_protocol::controlled_authority::formal::business_index::empty_business_index_root_bounded_v1(common).map_err(original)?;
 let raw=encode_original_canonical_data_v1(&("oasis7.formal.retained-history.root.v1",1u32,&*empty),common,canonical)?;let digest=hash(&raw,common)?;
 common.charge(1,(root.index_root.node_root.len()+empty.node_root.len())as u64).map_err(original)?;
 if root.schema_version!=1||root.index_root!=*empty{return Ok(false)}equal(root.root_digest.as_bytes(),digest.as_bytes(),common)
}

pub(in crate::world) fn equal_canonical_service_result_data_v1(a:&crate::service_admission::CanonicalIntentResultV1,b:&crate::service_admission::CanonicalIntentResultV1,common:&mut FormalBoundedContextV1<'_>,canonical:&mut CanonicalCborWorkV1)->Result<bool,BodyMutationError>{
 // Encoding is a work census only. Preserve the original typed equality,
 // including original JSON Number and ordered receipt-map semantics.
 let left=encode_original_canonical_data_v1(a,common,canonical)?;let right=encode_original_canonical_data_v1(b,common,canonical)?;let n=left.len().checked_add(right.len()).ok_or(BodyMutationError::Invalid("canonical result equality layout"))?;common.charge(n as u64+1,n as u64).map_err(original)?;Ok(a==b)
}

pub(in crate::world) fn decode_current_task_terminal_index_data_v2(raw:&[u8],common:&mut FormalBoundedContextV1<'_>)->Result<PaidCborDataV1<oasis7_world_state::TaskTerminalIndexV2>,BodyMutationError>{let layout=oasis7_world_state::task_terminal_index_owned_layout_v2(&mut||common.consume_steps(1).map_err(original))?;decode_original_json_data(raw,convert(layout),common)}

#[derive(Debug)]struct TerminalWitnessCodecFailure{cause:BodyMutationError,_fee:MemoryReservation}
impl std::fmt::Display for TerminalWitnessCodecFailure{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{f.write_str("invalid terminal Stop witness codec")}}impl std::error::Error for TerminalWitnessCodecFailure{}
#[derive(Debug)]struct PlainProtocolNote{note:&'static str,_fee:MemoryReservation}
impl std::fmt::Display for PlainProtocolNote{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{f.write_str(self.note)}}impl std::error::Error for PlainProtocolNote{}
fn plain(note:&'static str,common:&mut FormalBoundedContextV1<'_>)->BodyMutationError{if let Err(e)=common.charge(1,size_of::<PlainProtocolNote>()as u64){return original(e)}match common.memory().reserve(size_of::<PlainProtocolNote>(),MemoryClass::Ordinary){Ok(fee)=>original(PlainProtocolNote{note,_fee:fee}),Err(e)=>ValueTreeError::Memory(e).into()}}
pub(in crate::world) fn decode_terminal_stop_witness_data_v2(raw:&[u8],common:&mut FormalBoundedContextV1<'_>)->Result<PaidCborDataV1<crate::service_admission::CanonicalIntentResultV1>,BodyMutationError>{
 common.charge(1,size_of::<TerminalWitnessCodecFailure>()as u64).map_err(original)?;let error_fee=common.memory().reserve(size_of::<TerminalWitnessCodecFailure>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let layout=oasis7_client_api::consensus_action_wire::canonical_intent_result_owned_layout_v1(&mut||common.consume_steps(1).map_err(original))?;
 let result=match decode_with_wire_limit::<crate::service_admission::CanonicalIntentResultV1>(raw,convert(layout),raw.len(),common){Ok(value)=>value,Err(error)=>{let refusal=match &error{BodyMutationError::Original(cause)=>cause.downcast_ref::<CodecFailure>().is_some_and(|e|matches!(&e.kind,CodecKind::Refusal(_))),_=>false};if refusal{return Err(original(TerminalWitnessCodecFailure{cause:error,_fee:error_fee}))}return Err(error)}};
 let exact=encode_result_limit(std::borrow::Borrow::<crate::service_admission::CanonicalIntentResultV1>::borrow(&result),usize::MAX,common)?;let bytes:&Vec<u8>=exact.borrow();if !equal(bytes,raw,common)?{return Err(plain("terminal Stop witness is not exact canonical result bytes",common))}Ok(result)
}
pub(in crate::world) fn charge_terminal_record_data_v2(record:&oasis7_world_state::TaskTerminalRecordV2,common:&mut FormalBoundedContextV1<'_>,canonical:&mut CanonicalCborWorkV1)->Result<(),BodyMutationError>{let raw=encode_original_canonical_data_v1(record,common,canonical)?;common.charge(raw.len()as u64+1,raw.len()as u64).map_err(original)}

pub(in crate::world) fn blake3_raw_data_v1(raw:&[u8],common:&mut FormalBoundedContextV1<'_>)->Result<ReservedValue<String>,BodyMutationError>{hash(raw,common)}

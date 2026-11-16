//! Work-only operators for the unique original feedback/readback rules.
//! They encode/hash themselves; callers cannot supply a digest or verdict.
use super::*;
use oasis7_wasm_abi::{CapabilityCanonicalWorkV1,CanonicalCborStreamError as WorkError};
use std::io::{self,Write};
#[derive(Debug)]pub enum FeedbackReadErrorV1{Business(CognitionRecoveryError),Work(WorkError)}
impl From<CognitionRecoveryError>for FeedbackReadErrorV1{fn from(e:CognitionRecoveryError)->Self{Self::Business(e)}}
impl From<WorkError>for FeedbackReadErrorV1{fn from(e:WorkError)->Self{Self::Work(e)}}
pub(crate) fn legacy<T>(v:Result<T,FeedbackReadErrorV1>)->Result<T,CognitionRecoveryError>{match v{Ok(v)=>Ok(v),Err(FeedbackReadErrorV1::Business(e))=>Err(e),Err(FeedbackReadErrorV1::Work(_))=>unreachable!("legacy feedback work cannot fail")}}
pub fn phase<T:Serialize+?Sized,A:CapabilityCanonicalWorkV1>(value:&T,source:&A)->Result<Option<A::Charge>,WorkError>{if !source.is_metered(){return Ok(None)}let n=measure(value,source)?;let heap=n.checked_mul(2048).and_then(|n|n.checked_add(65536)).ok_or(WorkError::ResourceLimited("feedback typed graph"))?;source.charge_work(1+32*n as u64,n as u64)?;Ok(Some(source.reserve(heap)?))}
pub fn measure<T:Serialize+?Sized,A:CapabilityCanonicalWorkV1>(value:&T,source:&A)->Result<usize,WorkError>{match crate::util::measure_json_with_work_v1(value,&mut|s,b|source.charge_work(s,b)){Ok(n)=>Ok(n),Err(crate::util::JsonHashWorkErrorV1::Work(e))=>Err(e),Err(crate::util::JsonHashWorkErrorV1::Codec(_))=>Err(WorkError::Invalid("feedback original JSON codec"))}}
pub struct PaidJsonBytesV1<C>{pub bytes:Vec<u8>,_fee:Option<C>}
pub fn json_bytes<T:Serialize+?Sized,A:CapabilityCanonicalWorkV1>(value:&T,source:&A)->Result<PaidJsonBytesV1<A::Charge>,FeedbackReadErrorV1>{
 if !source.is_metered(){return serde_json::to_vec(value).map(|bytes|PaidJsonBytesV1{bytes,_fee:None}).map_err(|_|CognitionRecoveryError::new("runtime_feedback_payload_invalid").into())}
 let n=measure(value,source)?;let fee=source.reserve(n+std::mem::size_of::<Vec<u8>>())?;
 struct Out<'a,A>{source:&'a A,bytes:Vec<u8>,n:usize,first:Option<WorkError>}
 impl<A:CapabilityCanonicalWorkV1>Write for Out<'_,A>{fn write(&mut self,b:&[u8])->io::Result<usize>{if b.len()>self.n-self.bytes.len(){return Err(io::ErrorKind::Other.into())}if let Err(e)=self.source.charge_work(1,b.len()as u64){self.first=Some(e);return Err(io::ErrorKind::Other.into())}self.bytes.extend_from_slice(b);Ok(b.len())}fn flush(&mut self)->io::Result<()>{Ok(())}}
 let mut out=Out{source,bytes:Vec::with_capacity(n),n,first:None};if out.bytes.capacity()!=n{return Err(WorkError::ResourceLimited("feedback JSON capacity").into())}let result=serde_json::to_writer(&mut out,value);if let Some(e)=out.first{return Err(e.into())}result.map_err(|_|CognitionRecoveryError::new("runtime_feedback_payload_invalid"))?;if out.bytes.len()!=n{return Err(WorkError::Invalid("feedback JSON changed").into())}Ok(PaidJsonBytesV1{bytes:out.bytes,_fee:Some(fee)})
}
pub struct PaidDigestV1<C>(oasis7_agent_api::world_cognition::PaidCognitionDigestDataV1<C>);
impl<C>PaidDigestV1<C>{pub fn as_str(&self)->&str{self.0.as_str()}}
impl<C>Serialize for PaidDigestV1<C>{fn serialize<S:serde::Serializer>(&self,s:S)->Result<S::Ok,S::Error>{s.serialize_str(self.as_str())}}
impl<C>PartialEq<PaidDigestV1<C>>for String{fn eq(&self,r:&PaidDigestV1<C>)->bool{self==r.as_str()}}
pub fn digest<T:Serialize+?Sized,A:CapabilityCanonicalWorkV1>(domain:&str,value:&T,source:&A)->Result<PaidDigestV1<A::Charge>,WorkError>{oasis7_agent_api::world_cognition::cognition_digest_with_work_v1(domain,value,source).map(PaidDigestV1)}

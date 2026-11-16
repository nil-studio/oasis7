//! Original typed serde-JSON bytes -> SHA256 plain64, work-only callbacks.
//! The caller retains the actual source graph and the output/error allocation
//! admission. No callback selects a digest, changes JSON or grants authority.
use serde::Serialize;
use sha2::{Digest,Sha256};
use std::io::{self,Write};
#[derive(Debug)]pub enum JsonHashWorkErrorV1<E>{Work(E),Codec(serde_json::Error)}
impl<E:std::fmt::Display> std::fmt::Display for JsonHashWorkErrorV1<E>{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{match self{Self::Work(e)=>std::fmt::Display::fmt(e,f),Self::Codec(e)=>std::fmt::Display::fmt(e,f)}}}
impl<E:std::error::Error+'static> std::error::Error for JsonHashWorkErrorV1<E>{}
struct HashWriter<'a,E>{hash:Option<Sha256>,n:usize,work:&'a mut dyn FnMut(u64,u64)->Result<(),E>,first:Option<E>}
impl<E> Write for HashWriter<'_,E>{
 fn write(&mut self,bytes:&[u8])->io::Result<usize>{if self.first.is_some(){return Err(io::ErrorKind::Other.into())}for chunk in bytes.chunks(8192){if let Err(error)=(self.work)(1+if self.hash.is_some(){(chunk.len()as u64).div_ceil(64)}else{chunk.len()as u64},chunk.len()as u64){self.first=Some(error);return Err(io::ErrorKind::Other.into())}self.n=self.n.checked_add(chunk.len()).expect("original JSON stream length overflow");if let Some(hash)=&mut self.hash{hash.update(chunk)}}Ok(bytes.len())}
 fn flush(&mut self)->io::Result<()>{Ok(())}
}
pub fn hash_json_with_work_v1<T:Serialize+?Sized,E>(value:&T,work:&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<String,JsonHashWorkErrorV1<E>>{
 work(1,0).map_err(JsonHashWorkErrorV1::Work)?;
 let mut writer=HashWriter{hash:Some(Sha256::new()),n:0,work,first:None};let result=serde_json::to_writer(&mut writer,value);
 if let Some(error)=writer.first{return Err(JsonHashWorkErrorV1::Work(error))}result.map_err(JsonHashWorkErrorV1::Codec)?;
 (writer.work)(0,0).map_err(JsonHashWorkErrorV1::Work)?;
 // SHA finalization plus the exact original lowercase hex bytes. The output
 // owner must already hold its 64-byte String admission in a bounded caller.
 (writer.work)(33,64).map_err(JsonHashWorkErrorV1::Work)?;let digest=writer.hash.expect("hash writer mode").finalize();let mut text=String::with_capacity(64);const HEX:&[u8;16]=b"0123456789abcdef";for byte in digest{text.push(HEX[(byte>>4)as usize]as char);text.push(HEX[(byte&15)as usize]as char)}Ok(text)
}
/// Exact original typed serde-JSON length, using the very same streaming
/// writer as the hash path. No buffer, alternative grammar or predicate is
/// introduced. The fixed caller still applies its original post-codec limit.
pub fn measure_json_with_work_v1<T:Serialize+?Sized,E>(value:&T,work:&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<usize,JsonHashWorkErrorV1<E>>{
 work(1,0).map_err(JsonHashWorkErrorV1::Work)?;let mut writer=HashWriter{hash:None,n:0,work,first:None};let result=serde_json::to_writer(&mut writer,value);if let Some(error)=writer.first{return Err(JsonHashWorkErrorV1::Work(error))}result.map_err(JsonHashWorkErrorV1::Codec)?;(writer.work)(0,0).map_err(JsonHashWorkErrorV1::Work)?;Ok(writer.n)
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn original_json_unicode_and_large_string_bytes_are_unchanged(){let value=serde_json::json!({"unicode":"原始字段\n","large":"q".repeat(8193),"numeric":{"2":2,"10":10}});let expected=hex::encode(Sha256::digest(serde_json::to_vec(&value).unwrap()));let mut bytes=0u64;let actual=hash_json_with_work_v1(&value,&mut|_,n|{bytes+=n;Ok::<(),()>(())}).unwrap();assert_eq!(actual,expected);assert!(bytes>8193);}
 #[test]fn typed_first_stream_error_is_preserved_and_no_more_callbacks_run(){let mut calls=0;let result=hash_json_with_work_v1(&"x".repeat(17000),&mut|_,_|{calls+=1;if calls==3{Err("stopped")}else{Ok(())}});assert!(matches!(result,Err(JsonHashWorkErrorV1::Work("stopped"))));assert_eq!(calls,3);}
}

#[cfg(test)]mod measure_tests{
 use super::*;
 #[test]fn measured_wire_is_exact_original_json_for_escape_unicode_and_boundaries(){for n in [0,8191,8192,8193]{let value=serde_json::json!({"escaped":"原始\n\"", "payload":"q".repeat(n),"numbers":[0,1,-1]});let expected=serde_json::to_vec(&value).unwrap().len();let actual=measure_json_with_work_v1(&value,&mut|_,_|Ok::<(),()>(())).unwrap();assert_eq!(actual,expected);}}
 #[test]fn length_cancellation_keeps_typed_first_cause(){let mut calls=0;let result=measure_json_with_work_v1(&"x".repeat(17000),&mut|_,_|{calls+=1;if calls==3{Err("first stop")}else{Ok(())}});assert!(matches!(result,Err(JsonHashWorkErrorV1::Work("first stop"))));assert_eq!(calls,3);}
}

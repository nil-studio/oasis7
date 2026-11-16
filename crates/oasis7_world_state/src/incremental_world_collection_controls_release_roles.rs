//! Paid resident original release-role strings. No role normalization is introduced.
use super::*;
use std::{sync::Arc,collections::{BTreeMap,BTreeSet}};
use oasis7_proto::memory_budget::MemoryReservation;
struct RoleSetV1{members:BTreeSet<String>,bytes:usize}
struct RoleMapV1{groups:BTreeMap<String,Arc<ReservedValue<RoleSetV1>>>}
#[derive(Clone)]
pub(super) struct ReleaseRolesDirectoryV1{map:Arc<ReservedValue<RoleMapV1>>,outer_bytes:usize,graph_bytes:usize,count:u64,class:MemoryClass,memory:MemoryBudget}
impl ReleaseRolesDirectoryV1{
 pub fn new(memory:&MemoryBudget,class:MemoryClass)->Result<Self,ValueTreeError>{let n=std::mem::size_of::<RoleMapV1>()+128;let fee=memory.reserve(n,class)?;Ok(Self{map:Arc::new(fee.retain(RoleMapV1{groups:BTreeMap::new()})),outer_bytes:n,graph_bytes:n,count:0,class,memory:memory.clone()})}
 pub fn check(&self,memory:&MemoryBudget,class:MemoryClass,coverage:Option<&MemoryReservation>)->Result<(),ValueTreeError>{if !self.memory.same_pool(memory)||!self.map.uses_budget(memory){return Err(invalid())}if self.class!=class&&!(self.class==MemoryClass::Recovery&&class==MemoryClass::Ordinary&&coverage.is_some_and(|f|f.uses_budget(memory)&&f.bytes()>=self.graph_bytes)){return Err(invalid())}Ok(())}
 pub fn coverage(&self,memory:&MemoryBudget)->Result<Option<MemoryReservation>,ValueTreeError>{if !self.memory.same_pool(memory){return Err(invalid())}if self.class==MemoryClass::Ordinary{Ok(None)}else{Ok(Some(memory.reserve(self.graph_bytes,MemoryClass::Ordinary)?))}}
 pub fn members(&self,subject:&str)->Option<&BTreeSet<String>>{self.map.groups.get(subject).map(|g|&g.members)}
 pub fn count(&self)->u64{self.count}
 pub fn member_bytes(&self,subject:&str)->usize{self.map.groups.get(subject).map_or(0,|g|g.bytes)}
 pub fn selected<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(&self,subject:&str,ctx:&mut WorldFieldReadContextV1<'_,M,R,E,C>,work:&mut WorldCollectionControlsWorkV1)->Result<Self,ValueTreeError>{
   work.records_remaining=work.records_remaining.checked_sub(128).ok_or(ValueTreeError::ResourceLimited("release role selected lookup"))?;
   let bytes=subject.len().checked_add(1).and_then(|n|n.checked_mul(128)).ok_or_else(invalid)?;
   ctx.key_work.bytes_remaining=ctx.key_work.bytes_remaining.checked_sub(bytes as u64).ok_or(ValueTreeError::ResourceLimited("release role selected bytes"))?;
   let outer=std::mem::size_of::<RoleMapV1>().checked_add(subject.len()+4096).ok_or_else(invalid)?;
   let fee=ctx.values.memory.reserve(outer,ctx.values.class)?;let mut groups=BTreeMap::new();
   let mut count=0;let mut inner=0;
   if let Some(group)=self.map.groups.get(subject){count=group.members.len()as u64;inner=group.bytes;groups.insert(subject.to_owned(),group.clone());}
   Ok(Self{map:Arc::new(fee.retain(RoleMapV1{groups})),outer_bytes:outer,graph_bytes:outer.checked_add(inner).ok_or_else(invalid)?,count,class:self.class,memory:ctx.values.memory.clone()})
 }
 pub fn edit<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(&mut self,subject:&str,member:&str,add:bool,ctx:&mut WorldFieldReadContextV1<'_,M,R,E,C>,work:&mut WorldCollectionControlsWorkV1)->Result<(),ValueTreeError>{
   ctx.check()?;if !self.memory.same_pool(ctx.values.memory){return Err(invalid())}
   work.records_remaining=work.records_remaining.checked_sub(128).ok_or(ValueTreeError::ResourceLimited("release roles lookup steps"))?;
   let probe=subject.len().checked_add(member.len()).and_then(|n|n.checked_add(1)).and_then(|n|n.checked_mul(128)).ok_or_else(invalid)?;
   ctx.key_work.bytes_remaining=ctx.key_work.bytes_remaining.checked_sub(probe as u64).ok_or(ValueTreeError::ResourceLimited("release roles lookup bytes"))?;
   let old=self.map.groups.get(subject);
   if old.is_some_and(|g|g.members.contains(member))==add{return Err(invalid())}
   let count=self.map.groups.len().checked_add(old.map_or(0,|g|g.members.len())).and_then(|n|n.checked_add(2)).ok_or_else(invalid)?;
   work.records_remaining=work.records_remaining.checked_sub(count as u64).ok_or(ValueTreeError::ResourceLimited("release role directory copies"))?;
   let outer=self.outer_bytes.checked_add(subject.len()+4096).ok_or_else(invalid)?;
   let inner=old.map_or(0,|g|g.bytes).checked_add(member.len()+4096).and_then(|n|n.checked_add(std::mem::size_of::<RoleSetV1>()+128)).ok_or_else(invalid)?;
   let bytes=outer.checked_add(inner).and_then(|n|n.checked_mul(64)).ok_or_else(invalid)?;
   ctx.key_work.bytes_remaining=ctx.key_work.bytes_remaining.checked_sub(bytes as u64).ok_or(ValueTreeError::ResourceLimited("release role directory bytes"))?;
   let outer_fee=ctx.values.memory.reserve(outer,ctx.values.class)?;
   let inner_fee=ctx.values.memory.reserve(inner,ctx.values.class)?;
   let mut groups=self.map.groups.clone();let mut members=old.map_or_else(BTreeSet::new,|g|g.members.clone());
   if add{members.insert(member.to_owned());}else{members.remove(member);}
   let oldbytes=old.map_or(0,|g|g.bytes);
   let innerbytes=if members.is_empty(){groups.remove(subject);0}else{groups.insert(subject.to_owned(),Arc::new(inner_fee.retain(RoleSetV1{members,bytes:inner})));inner};
   self.graph_bytes=self.graph_bytes.checked_sub(self.outer_bytes).and_then(|n|n.checked_sub(oldbytes)).and_then(|n|n.checked_add(outer)).and_then(|n|n.checked_add(innerbytes)).ok_or_else(invalid)?;
   self.count=if add{self.count.checked_add(1)}else{self.count.checked_sub(1)}.ok_or_else(invalid)?;
   self.outer_bytes=outer;self.map=Arc::new(outer_fee.retain(RoleMapV1{groups}));Ok(())
 }
 pub fn row<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(&mut self,key:&WorldFieldKeyV1,value:&[u8;32],add:bool,ctx:&mut WorldFieldReadContextV1<'_,M,R,E,C>,work:&mut WorldCollectionControlsWorkV1)->Result<(),ValueTreeError>{
   self.row_selected(key,value,add,None,ctx,work)
 }
 pub fn row_selected<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(&mut self,key:&WorldFieldKeyV1,value:&[u8;32],add:bool,target:Option<&str>,ctx:&mut WorldFieldReadContextV1<'_,M,R,E,C>,work:&mut WorldCollectionControlsWorkV1)->Result<(),ValueTreeError>{
   let parsed=WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
   if parsed.kind!=WorldFieldRecordKindV1::Entry{return Ok(())}
   let limits=MemberRecordLimitsV1{semantic_bytes:work.max_record_bytes.min(ctx.record_bytes),payload_bytes:0,record_bytes:work.max_record_bytes.min(ctx.record_bytes)};
   let info=read_member_record_header_v1(value,key,&mut ctx.values,limits,&mut ctx.cancelled)?;
   let n=usize::try_from(info.semantic_bytes).map_err(|_|invalid())?;
   work.step(&mut ctx.cancelled)?;
   ctx.key_work.bytes_remaining=ctx.key_work.bytes_remaining.checked_sub((n as u64).checked_mul(4).ok_or_else(invalid)?).ok_or(ValueTreeError::ResourceLimited("release role original key decoding"))?;
   let _fee=ctx.values.memory.reserve(n.checked_add(4096).ok_or_else(invalid)?,ctx.values.class)?;
   let mut bytes=vec![0;n];read_member_record_key_v1(value,key,&mut ctx.values,limits,&mut std::io::Cursor::new(bytes.as_mut_slice()),&mut ctx.cancelled)?;
   use crate::incremental_semantic_key::{read_semantic_key_v1,SemanticKeyRefV1 as K};
   let K::Pair{left,right}=read_semantic_key_v1(&bytes).map_err(ValueTreeError::Invalid)?else{return Err(invalid())};
   let(K::Text(subject),K::Text(member))=(left.decode().map_err(ValueTreeError::Invalid)?,right.decode().map_err(ValueTreeError::Invalid)?)else{return Err(invalid())};
   if let Some(target)=target{work.step(&mut ctx.cancelled)?;ctx.key_work.bytes_remaining=ctx.key_work.bytes_remaining.checked_sub((target.len()as u64).checked_add(subject.len()as u64).ok_or_else(invalid)?).ok_or(ValueTreeError::ResourceLimited("release role subject comparison"))?;if target!=subject{return Ok(())}}
   self.edit(subject,member,add,ctx,work)
 }
}

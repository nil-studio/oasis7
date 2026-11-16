//! Immutable observation DATA; never WorldState, a read permission or a commit.
//! Trees retain their own Ordinary allocation charges. Successors share the
//! unchanged graph and path-copy only actual changed JSON/column paths.
use oasis7_proto::memory_budget::{MemoryBudget,MemoryClass,MemoryReservation,MemoryLimitExceeded};
use serde::{Serialize,Serializer,ser::{SerializeMap,SerializeSeq}};
use serde_json::Value;
use std::{cmp::Ordering,sync::Arc};
#[derive(Debug)]pub enum ProjectionColumnErrorV1<E>{Work(E),Memory(MemoryLimitExceeded),Invalid(&'static str)}
impl<E:std::fmt::Display> std::fmt::Display for ProjectionColumnErrorV1<E>{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{match self{Self::Work(e)=>e.fmt(f),Self::Memory(e)=>e.fmt(f),Self::Invalid(s)=>f.write_str(s)}}}
impl<E:std::error::Error+'static> std::error::Error for ProjectionColumnErrorV1<E>{}
type J=Arc<JsonNode>;
type T=Option<Arc<TreeNode>>;
struct KeyText{text:String,_fee:MemoryReservation}
enum Key{Text(Arc<KeyText>),Index(usize)}
impl Key{fn shared(&self)->Self{match self{Self::Text(v)=>Self::Text(Arc::clone(v)),Self::Index(v)=>Self::Index(*v)}}}
struct TreeNode{key:Key,value:J,left:T,right:T,height:u32,count:usize,_fee:MemoryReservation}
enum Kind{Null,Bool(bool),Number(serde_json::Number),Text(String),Array{start:usize,len:usize,items:T},TextSet(T),Object(T)}
struct JsonNode{kind:Kind,_fee:MemoryReservation}
/// Only a serialized/read-only column graph. Runtime separately binds the
/// actual qualified source, original semantic keys and transaction revision.
pub struct ProjectionColumnDataV1{root:J,_fee:MemoryReservation}
struct Build<'a,'b,E>{memory:&'a MemoryBudget,work:&'b mut dyn FnMut(u64,u64)->Result<(),E>}
impl<E> Build<'_,'_,E>{
 fn pay(&mut self,s:u64,b:usize)->Result<(),ProjectionColumnErrorV1<E>>{(self.work)(s,b as u64).map_err(ProjectionColumnErrorV1::Work)}
 fn reserve(&mut self,n:usize)->Result<MemoryReservation,ProjectionColumnErrorV1<E>>{self.pay(1,n)?;self.memory.reserve(n,MemoryClass::Ordinary).map_err(ProjectionColumnErrorV1::Memory)}
 fn key(&mut self,text:String)->Result<Key,ProjectionColumnErrorV1<E>>{let fee=self.reserve(std::mem::size_of::<KeyText>()+text.capacity()+2*std::mem::size_of::<usize>())?;Ok(Key::Text(Arc::new(KeyText{text,_fee:fee})))}
 fn cmp(&mut self,a:&Key,b:&Key)->Result<Ordering,ProjectionColumnErrorV1<E>>{let bytes=match(a,b){(Key::Text(a),Key::Text(b))=>a.text.len().saturating_add(b.text.len()),_=>0};self.pay(1,bytes)?;Ok(match(a,b){(Key::Text(a),Key::Text(b))=>a.text.cmp(&b.text),(Key::Index(a),Key::Index(b))=>a.cmp(b),(Key::Text(_),Key::Index(_))=>Ordering::Less,_=>Ordering::Greater})}
 fn node(&mut self,key:Key,value:J,left:T,right:T)->Result<Arc<TreeNode>,ProjectionColumnErrorV1<E>>{let count=1usize.checked_add(size(&left)).and_then(|v|v.checked_add(size(&right))).ok_or(ProjectionColumnErrorV1::Invalid("projection column tree count"))?;let height=1+h(&left).max(h(&right));let fee=self.reserve(std::mem::size_of::<TreeNode>()+2*std::mem::size_of::<usize>())?;Ok(Arc::new(TreeNode{key,value,left,right,height,count,_fee:fee}))}
 fn balance(&mut self,key:Key,value:J,left:T,right:T)->Result<Arc<TreeNode>,ProjectionColumnErrorV1<E>>{
  self.pay(1,0)?;
  if h(&left)>h(&right)+1{let l=left.as_ref().ok_or(ProjectionColumnErrorV1::Invalid("projection left rotation"))?;if h(&l.left)>=h(&l.right){let nr=self.node(key,value,l.right.clone(),right)?;return self.node(l.key.shared(),Arc::clone(&l.value),l.left.clone(),Some(nr))}let lr=l.right.as_ref().ok_or(ProjectionColumnErrorV1::Invalid("projection left inner rotation"))?;let nl=self.node(l.key.shared(),Arc::clone(&l.value),l.left.clone(),lr.left.clone())?;let nr=self.node(key,value,lr.right.clone(),right)?;return self.node(lr.key.shared(),Arc::clone(&lr.value),Some(nl),Some(nr))}
  if h(&right)>h(&left)+1{let r=right.as_ref().ok_or(ProjectionColumnErrorV1::Invalid("projection right rotation"))?;if h(&r.right)>=h(&r.left){let nl=self.node(key,value,left,r.left.clone())?;return self.node(r.key.shared(),Arc::clone(&r.value),Some(nl),r.right.clone())}let rl=r.left.as_ref().ok_or(ProjectionColumnErrorV1::Invalid("projection right inner rotation"))?;let nl=self.node(key,value,left,rl.left.clone())?;let nr=self.node(r.key.shared(),Arc::clone(&r.value),rl.right.clone(),r.right.clone())?;return self.node(rl.key.shared(),Arc::clone(&rl.value),Some(nl),Some(nr))}
  self.node(key,value,left,right)
 }
 fn put(&mut self,tree:&T,key:Key,value:J)->Result<T,ProjectionColumnErrorV1<E>>{self.pay(1,0)?;let Some(old)=tree else{return self.node(key,value,None,None).map(Some)};match self.cmp(&key,&old.key)?{Ordering::Equal=>self.node(old.key.shared(),value,old.left.clone(),old.right.clone()).map(Some),Ordering::Less=>{let left=self.put(&old.left,key,value)?;self.balance(old.key.shared(),Arc::clone(&old.value),left,old.right.clone()).map(Some)},Ordering::Greater=>{let right=self.put(&old.right,key,value)?;self.balance(old.key.shared(),Arc::clone(&old.value),old.left.clone(),right).map(Some)}}}
 fn compare_text(&mut self,key:&str,node:&Key)->Result<Ordering,ProjectionColumnErrorV1<E>>{let Key::Text(node)=node else{return Err(ProjectionColumnErrorV1::Invalid("projection object key"))};self.pay(1,key.len().saturating_add(node.text.len()))?;Ok(key.cmp(&node.text))}
 fn remove_text(&mut self,tree:&T,key:&str)->Result<T,ProjectionColumnErrorV1<E>>{
  self.pay(1,0)?;let Some(old)=tree else{return Ok(None)};
  match self.compare_text(key,&old.key)?{
   Ordering::Less=>{let left=self.remove_text(&old.left,key)?;self.balance(old.key.shared(),Arc::clone(&old.value),left,old.right.clone()).map(Some)},
   Ordering::Greater=>{let right=self.remove_text(&old.right,key)?;self.balance(old.key.shared(),Arc::clone(&old.value),old.left.clone(),right).map(Some)},
   Ordering::Equal=>match(&old.left,&old.right){(None,_)=>Ok(old.right.clone()),(_,None)=>Ok(old.left.clone()),(Some(_),Some(right))=>{let mut successor=right;while let Some(left)=&successor.left{self.pay(1,0)?;successor=left}let Key::Text(successor_key)=&successor.key else{return Err(ProjectionColumnErrorV1::Invalid("projection object successor"))};let after=self.remove_text(&old.right,&successor_key.text)?;self.balance(successor.key.shared(),Arc::clone(&successor.value),old.left.clone(),after).map(Some)}}
  }
 }

 fn keep_array_suffix(&mut self,tree:&T,first:usize)->Result<T,ProjectionColumnErrorV1<E>>{self.pay(1,0)?;let Some(node)=tree else{return Ok(None)};let Key::Index(index)=&node.key else{return Err(ProjectionColumnErrorV1::Invalid("projection array key"))};if *index<first{return self.keep_array_suffix(&node.right,first)}let left=self.keep_array_suffix(&node.left,first)?;self.join_array(node.key.shared(),Arc::clone(&node.value),left,node.right.clone()).map(Some)}
 // A split may remove several levels. Join descends the taller side instead
 // of applying a single AVL rotation valid only for one-element edits.
 fn join_array(&mut self,key:Key,value:J,left:T,right:T)->Result<Arc<TreeNode>,ProjectionColumnErrorV1<E>>{self.pay(1,0)?;if h(&left)>h(&right)+1{let node=left.as_ref().ok_or(ProjectionColumnErrorV1::Invalid("projection array join left"))?;let after=self.join_array(key,value,node.right.clone(),right)?;return self.balance(node.key.shared(),Arc::clone(&node.value),node.left.clone(),Some(after))}if h(&right)>h(&left)+1{let node=right.as_ref().ok_or(ProjectionColumnErrorV1::Invalid("projection array join right"))?;let after=self.join_array(key,value,left,node.left.clone())?;return self.balance(node.key.shared(),Arc::clone(&node.value),Some(after),node.right.clone())}self.node(key,value,left,right)}

 fn json(&mut self,value:Value)->Result<J,ProjectionColumnErrorV1<E>>{self.pay(1,0)?;let extra=match &value{Value::String(v)=>v.capacity(),_=>0};let fee=self.reserve(std::mem::size_of::<JsonNode>()+extra+2*std::mem::size_of::<usize>())?;let kind=match value{Value::Null=>Kind::Null,Value::Bool(v)=>Kind::Bool(v),Value::Number(v)=>Kind::Number(v),Value::String(v)=>Kind::Text(v),Value::Array(values)=>{let len=values.len();let mut items=None;for(i,v)in values.into_iter().enumerate(){let value=self.json(v)?;items=self.put(&items,Key::Index(i),value)?}Kind::Array{start:0,len,items}},Value::Object(values)=>{let mut items=None;for(k,v)in values{let key=self.key(k)?;let value=self.json(v)?;items=self.put(&items,key,value)?}Kind::Object(items)}};Ok(Arc::new(JsonNode{kind,_fee:fee}))}
}
fn h(t:&T)->u32{t.as_ref().map_or(0,|v|v.height)}fn size(t:&T)->usize{t.as_ref().map_or(0,|v|v.count)}
impl ProjectionColumnDataV1{
 /// This is a DATA conversion, not authentication. Caller retains the actual
 /// source graph fee until this consuming conversion returns or unwinds.
 pub fn from_json_data<E>(value:Value,memory:&MemoryBudget,work:&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<Self,ProjectionColumnErrorV1<E>>{let mut build=Build{memory,work};let fee=build.reserve(std::mem::size_of::<Self>())?;let root=build.json(value)?;Ok(Self{root,_fee:fee})}
 pub fn uses_budget(&self,memory:&MemoryBudget)->bool{self._fee.uses_budget(memory)&&self.root._fee.uses_budget(memory)}
 pub fn share_data<E>(&self,memory:&MemoryBudget,work:&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<Self,ProjectionColumnErrorV1<E>>{if !self.uses_budget(memory){return Err(ProjectionColumnErrorV1::Invalid("projection column pool"))}let fee=Build{memory,work}.reserve(std::mem::size_of::<Self>())?;Ok(Self{root:Arc::clone(&self.root),_fee:fee})}
 /// Borrow an actual column member without cloning or rebuilding its graph.
 /// This only observes DATA; the Runtime source owns the root/transaction proof.
 pub fn object_member_data<'a,E>(&'a self,key:&str,memory:&MemoryBudget,work:&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<Option<ProjectionColumnRefV1<'a>>,ProjectionColumnErrorV1<E>>{
  if !self.uses_budget(memory){return Err(ProjectionColumnErrorV1::Invalid("projection column pool"))}
  let Kind::Object(root)=&self.root.kind else{return Err(ProjectionColumnErrorV1::Invalid("projection column is not object"))};
  let mut next=root.as_ref();let mut build=Build{memory,work};while let Some(node)=next{match build.compare_text(key,&node.key)?{Ordering::Less=>next=node.left.as_ref(),Ordering::Greater=>next=node.right.as_ref(),Ordering::Equal=>return Ok(Some(ProjectionColumnRefV1{node:&node.value}))}}Ok(None)
 }
 /// Incremental immutable DATA successor. The supplied member is already
 /// paid in this same pool; unchanged branches retain their original charges.
 /// No key, root or value passed here grants a Current/accepted qualification.
 pub fn with_object_member_data<E>(&self,key:&str,after:Option<&Self>,memory:&MemoryBudget,work:&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<Self,ProjectionColumnErrorV1<E>>{
  if !self.uses_budget(memory)||after.is_some_and(|v|!v.uses_budget(memory)){return Err(ProjectionColumnErrorV1::Invalid("projection column pool"))}
  let Kind::Object(items)=&self.root.kind else{return Err(ProjectionColumnErrorV1::Invalid("projection column is not object"))};let mut build=Build{memory,work};let fee=build.reserve(std::mem::size_of::<Self>())?;
  let items=if let Some(after)=after{let key_fee=build.reserve(std::mem::size_of::<KeyText>()+key.len()+2*std::mem::size_of::<usize>())?;let text=key.to_owned();if text.capacity()!=key.len(){return Err(ProjectionColumnErrorV1::Invalid("projection key capacity"))}let key=Key::Text(Arc::new(KeyText{text,_fee:key_fee}));build.put(items,key,Arc::clone(&after.root))?}else{build.remove_text(items,key)?};
  let root_fee=build.reserve(std::mem::size_of::<JsonNode>()+2*std::mem::size_of::<usize>())?;Ok(Self{root:Arc::new(JsonNode{kind:Kind::Object(items),_fee:root_fee}),_fee:fee})
 }

 pub fn root_data(&self)->ProjectionColumnRefV1<'_>{ProjectionColumnRefV1{node:&self.root}}

 pub fn array_len_data_v1<E>(&self,memory:&MemoryBudget,work:&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<usize,ProjectionColumnErrorV1<E>>{work(1,0).map_err(ProjectionColumnErrorV1::Work)?;if !self.uses_budget(memory){return Err(ProjectionColumnErrorV1::Invalid("projection array pool"))}match &self.root.kind{Kind::Array{len,..}=>Ok(*len),Kind::TextSet(items)=>Ok(size(items)),_=>Err(ProjectionColumnErrorV1::Invalid("projection column is not array"))}}
 pub fn object_len_data_v1<E>(&self,memory:&MemoryBudget,work:&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<usize,ProjectionColumnErrorV1<E>>{work(1,0).map_err(ProjectionColumnErrorV1::Work)?;if !self.uses_budget(memory){return Err(ProjectionColumnErrorV1::Invalid("projection object pool"))}match &self.root.kind{Kind::Object(items)=>Ok(size(items)),_=>Err(ProjectionColumnErrorV1::Invalid("projection column is not object"))}}
 pub fn root_kind(&self)->&'static str{match &self.root.kind{Kind::Null=>"null",Kind::Bool(_)=>"bool",Kind::Number(_)=>"number",Kind::Text(_)=>"text",Kind::Array{..}|Kind::TextSet(_)=>"array",Kind::Object(_)=>"object"}}
}
fn visit_map<E>(tree:&T,emit:&mut impl FnMut(&Key,&J)->Result<(),E>)->Result<(),E>{if let Some(node)=tree{visit_map(&node.left,emit)?;emit(&node.key,&node.value)?;visit_map(&node.right,emit)?}Ok(())}
impl Serialize for JsonNode{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{match &self.kind{Kind::Null=>s.serialize_unit(),Kind::Bool(v)=>s.serialize_bool(*v),Kind::Number(v)=>v.serialize(s),Kind::Text(v)=>s.serialize_str(v),Kind::TextSet(items)=>{let mut out=s.serialize_seq(Some(size(items)))?;visit_map(items,&mut|_,value|out.serialize_element(&**value))?;out.end()},Kind::Object(items)=>{let mut out=s.serialize_map(Some(size(items)))?;visit_map(items,&mut|key,value|{let Key::Text(key)=key else{return Err(serde::ser::Error::custom("projection object key"))};out.serialize_entry(&key.text,&**value)})?;out.end()},Kind::Array{start,len,items}=>{let mut out=s.serialize_seq(Some(*len))?;let mut next=*start;visit_map(items,&mut|key,value|{if !matches!(key,Key::Index(i)if *i==next){return Err(serde::ser::Error::custom("projection array ordinal"))}next+=1;out.serialize_element(&**value)})?;out.end()}}}}
impl Serialize for ProjectionColumnDataV1{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{self.root.serialize(s)}}

/// A read-only borrow cannot escape its actual paid column owner.
pub struct ProjectionColumnRefV1<'a>{node:&'a J}
impl Serialize for ProjectionColumnRefV1<'_>{fn serialize<S:Serializer>(&self,s:S)->Result<S::Ok,S::Error>{self.node.serialize(s)}}

/// Fixed formatter predicates shared with the original World wrapper. These
/// consume only original entity facts, never an authorization verdict.
pub fn projection_location_visible_v1(outer:&str,inner:&str,reader:Option<&str>)->bool{reader==Some(outer)&&reader==Some(inner)}
pub fn projection_site_visible_v1<'a>(owner:&str,members:impl IntoIterator<Item=&'a str>,reader:Option<&str>)->bool{reader.is_some_and(|reader|owner==reader||members.into_iter().any(|id|id==reader))}
pub fn projection_site_member_visible_v1(member:&str,reader:Option<&str>)->bool{reader==Some(member)}
pub fn projection_site_owner_visible_v1(owner:&str,reader:Option<&str>)->bool{reader==Some(owner)}
pub fn projection_build_visible_v1(builder:&str,reader:Option<&str>)->bool{reader==Some(builder)}
pub fn projection_intent_visible_v1(agent:&str,reader:Option<&str>)->bool{reader==Some(agent)}

impl<'a> ProjectionColumnRefV1<'a>{
 pub fn unsigned_data(&self)->Option<u64>{match &self.node.kind{Kind::Number(value)=>value.as_u64(),_=>None}}
 pub fn bool_data(&self)->Option<bool>{match &self.node.kind{Kind::Bool(value)=>Some(*value),_=>None}}

 pub fn text_data(&self)->Option<&'a str>{match &self.node.kind{Kind::Text(value)=>Some(value),_=>None}}
 pub fn share_data<E>(&self,memory:&MemoryBudget,work:&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<ProjectionColumnDataV1,ProjectionColumnErrorV1<E>>{if !self.node._fee.uses_budget(memory){return Err(ProjectionColumnErrorV1::Invalid("projection member pool"))}let fee=Build{memory,work}.reserve(std::mem::size_of::<ProjectionColumnDataV1>())?;Ok(ProjectionColumnDataV1{root:Arc::clone(self.node),_fee:fee})}
}

impl<'a> ProjectionColumnRefV1<'a>{
 pub fn object_members_data<E>(&self,memory:&MemoryBudget,work:&mut dyn FnMut(u64,u64)->Result<(),E>,emit:&mut impl FnMut(&'a str,ProjectionColumnRefV1<'a>,&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<(),ProjectionColumnErrorV1<E>>)->Result<(),ProjectionColumnErrorV1<E>>{
  if !self.node._fee.uses_budget(memory){return Err(ProjectionColumnErrorV1::Invalid("projection member pool"))}let Kind::Object(items)=&self.node.kind else{return Err(ProjectionColumnErrorV1::Invalid("projection member is not object"))};
  fn walk<'a,E>(tree:&'a T,work:&mut dyn FnMut(u64,u64)->Result<(),E>,emit:&mut impl FnMut(&'a str,ProjectionColumnRefV1<'a>,&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<(),ProjectionColumnErrorV1<E>>)->Result<(),ProjectionColumnErrorV1<E>>{if let Some(node)=tree{work(1,0).map_err(ProjectionColumnErrorV1::Work)?;walk(&node.left,work,emit)?;let Key::Text(key)=&node.key else{return Err(ProjectionColumnErrorV1::Invalid("projection object key"))};emit(&key.text,ProjectionColumnRefV1{node:&node.value},work)?;walk(&node.right,work,emit)?}Ok(())}walk(items,work,emit)
 }
 pub fn object_member_data<E>(&self,key:&str,memory:&MemoryBudget,work:&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<Option<ProjectionColumnRefV1<'a>>,ProjectionColumnErrorV1<E>>{
  if !self.node._fee.uses_budget(memory){return Err(ProjectionColumnErrorV1::Invalid("projection member pool"))}let Kind::Object(root)=&self.node.kind else{return Err(ProjectionColumnErrorV1::Invalid("projection member is not object"))};let mut next=root.as_ref();let mut build=Build{memory,work};while let Some(node)=next{match build.compare_text(key,&node.key)?{Ordering::Less=>next=node.left.as_ref(),Ordering::Greater=>next=node.right.as_ref(),Ordering::Equal=>return Ok(Some(ProjectionColumnRefV1{node:&node.value}))}}Ok(None)
 }
}

impl ProjectionColumnDataV1{
 pub fn array_text_contains_data<E>(&self,expected:&str,memory:&MemoryBudget,work:&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<bool,ProjectionColumnErrorV1<E>>{
  if !self.uses_budget(memory){return Err(ProjectionColumnErrorV1::Invalid("projection column pool"))}
  if let Kind::TextSet(items)=&self.root.kind{let mut build=Build{memory,work};let mut next=items.as_ref();while let Some(node)=next{match build.compare_text(expected,&node.key)?{Ordering::Less=>next=node.left.as_ref(),Ordering::Greater=>next=node.right.as_ref(),Ordering::Equal=>return Ok(true)}}return Ok(false)}
  let Kind::Array{items,..}=&self.root.kind else{return Err(ProjectionColumnErrorV1::Invalid("projection member is not array"))};fn contains<E>(tree:&T,expected:&str,work:&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<bool,ProjectionColumnErrorV1<E>>{let Some(node)=tree else{return Ok(false)};work(1,0).map_err(ProjectionColumnErrorV1::Work)?;if contains(&node.left,expected,work)?{return Ok(true)}let Kind::Text(text)=&node.value.kind else{return Err(ProjectionColumnErrorV1::Invalid("projection authority member is not text"))};work(1,text.len().saturating_add(expected.len())as u64).map_err(ProjectionColumnErrorV1::Work)?;if text==expected{return Ok(true)}contains(&node.right,expected,work)}contains(items,expected,work)
 }
 /// Exact original site-membership retain: preserves duplicate matching IDs
 /// and shares actual text nodes instead of cloning authority strings.
 pub fn visible_site_members_data_v1<E>(&self,reader:Option<&str>,memory:&MemoryBudget,work:&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<Self,ProjectionColumnErrorV1<E>>{
  if !self.uses_budget(memory){return Err(ProjectionColumnErrorV1::Invalid("projection column pool"))}let Kind::Array{items,..}=&self.root.kind else{return Err(ProjectionColumnErrorV1::Invalid("projection member is not array"))};let mut build=Build{memory,work};let fee=build.reserve(std::mem::size_of::<Self>())?;let root_fee=build.reserve(std::mem::size_of::<JsonNode>()+2*std::mem::size_of::<usize>())?;let mut after=None;let mut len=0usize;
  visit_map(items,&mut|_,value|{let Kind::Text(text)=&value.kind else{return Err(ProjectionColumnErrorV1::Invalid("projection authority member is not text"))};build.pay(1,text.len().saturating_add(reader.map_or(0,str::len)))?;if projection_site_member_visible_v1(text,reader){after=build.put(&after,Key::Index(len),Arc::clone(value))?;len=len.checked_add(1).ok_or(ProjectionColumnErrorV1::Invalid("projection visible membership count"))?}Ok(())})?;Ok(Self{root:Arc::new(JsonNode{kind:Kind::Array{start:0,len,items:after},_fee:root_fee}),_fee:fee})
 }
}

/// Apply the original Agent observation masking to one resident column. No
/// entity DTO or WorldState is cloned; changed paths share paid immutable DATA.
pub fn project_agent_cells_data_v1<E>(actual:&ProjectionColumnDataV1,reader:Option<&str>,memory:&MemoryBudget,work:&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<ProjectionColumnDataV1,ProjectionColumnErrorV1<E>>{
 let empty_mailbox=ProjectionColumnDataV1::from_json_data(Value::Array(Vec::new()),memory,work)?;let mut result=actual.share_data(memory,work)?;
 actual.root_data().object_members_data(memory,work,&mut|id,row,work|{let original=row.share_data(memory,work)?;let mut masked=original.with_object_member_data("mailbox",Some(&empty_mailbox),memory,work)?;if !projection_intent_visible_v1(id,reader){masked=masked.with_object_member_data("intent",None,memory,work)?}result=result.with_object_member_data(id,Some(&masked),memory,work)?;Ok(())})?;Ok(result)
}
fn text_field<'a,E>(row:&ProjectionColumnRefV1<'a>,name:&str,memory:&MemoryBudget,work:&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<&'a str,ProjectionColumnErrorV1<E>>{row.object_member_data(name,memory,work)?.and_then(|v|v.text_data()).ok_or(ProjectionColumnErrorV1::Invalid("projection original text field"))}
/// Fixed original location outer/inner check and construction-builder filter.
/// The caller selects only these original schema columns, never a predicate.
pub fn project_location_authorities_data_v1<E>(actual:&ProjectionColumnDataV1,reader:Option<&str>,memory:&MemoryBudget,work:&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<ProjectionColumnDataV1,ProjectionColumnErrorV1<E>>{
 let mut result=actual.share_data(memory,work)?;actual.root_data().object_members_data(memory,work,&mut|outer,row,work|{let inner=text_field(&row,"agent_id",memory,work)?;work(1,(outer.len()+inner.len()+reader.map_or(0,str::len))as u64).map_err(ProjectionColumnErrorV1::Work)?;if !projection_location_visible_v1(outer,inner,reader){result=result.with_object_member_data(outer,None,memory,work)?}Ok(())})?;Ok(result)
}
pub fn project_pending_builds_data_v1<E>(actual:&ProjectionColumnDataV1,reader:Option<&str>,memory:&MemoryBudget,work:&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<ProjectionColumnDataV1,ProjectionColumnErrorV1<E>>{
 let mut result=actual.share_data(memory,work)?;actual.root_data().object_members_data(memory,work,&mut|outer,row,work|{let builder=text_field(&row,"builder_agent_id",memory,work)?;work(1,(builder.len()+reader.map_or(0,str::len))as u64).map_err(ProjectionColumnErrorV1::Work)?;if !projection_build_visible_v1(builder,reader){result=result.with_object_member_data(outer,None,memory,work)?}Ok(())})?;Ok(result)
}
pub fn project_site_authorities_data_v1<E>(actual:&ProjectionColumnDataV1,reader:Option<&str>,memory:&MemoryBudget,work:&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<ProjectionColumnDataV1,ProjectionColumnErrorV1<E>>{
 let empty_owner=ProjectionColumnDataV1::from_json_data(Value::String(String::new()),memory,work)?;let mut result=actual.share_data(memory,work)?;
 actual.root_data().object_members_data(memory,work,&mut|outer,row,work|{let owner=text_field(&row,"owner_agent_id",memory,work)?;let members=row.object_member_data("authorized_agent_ids",memory,work)?.ok_or(ProjectionColumnErrorV1::Invalid("projection original authority members"))?.share_data(memory,work)?;work(1,(owner.len()+reader.map_or(0,str::len))as u64).map_err(ProjectionColumnErrorV1::Work)?;
  // Original OR short-circuit: owner visibility never needs membership search.
  let visible=if projection_site_owner_visible_v1(owner,reader){true}else if let Some(reader)=reader{members.array_text_contains_data(reader,memory,work)?}else{false};
  if !visible{result=result.with_object_member_data(outer,None,memory,work)?;return Ok(())}
  let filtered=members.visible_site_members_data_v1(reader,memory,work)?;let original=row.share_data(memory,work)?;let mut masked=original.with_object_member_data("authorized_agent_ids",Some(&filtered),memory,work)?;if !projection_site_owner_visible_v1(owner,reader){masked=masked.with_object_member_data("owner_agent_id",Some(&empty_owner),memory,work)?}result=result.with_object_member_data(outer,Some(&masked),memory,work)?;Ok(())})?;Ok(result)
}
pub fn project_claim_sequences_data_v1<E>(actual:&ProjectionColumnDataV1,claims:&ProjectionColumnDataV1,memory:&MemoryBudget,work:&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<ProjectionColumnDataV1,ProjectionColumnErrorV1<E>>{
 let mut result=actual.share_data(memory,work)?;actual.root_data().object_members_data(memory,work,&mut|target,_,work|{if claims.object_member_data(target,memory,work)?.is_none(){result=result.with_object_member_data(target,None,memory,work)?}Ok(())})?;Ok(result)
}

#[cfg(test)]
mod tests{
 use super::*;
 fn budget()->MemoryBudget{MemoryBudget::new(8*1024*1024,0).unwrap()}
 fn work(_:u64,_:u64)->Result<(),&'static str>{Ok(())}
 #[test]fn successor_retains_shared_graph_after_parent_drop(){
  let memory=budget();let first=ProjectionColumnDataV1::from_json_data(serde_json::json!({"a":{"private":"old"},"b":[1,2,3],"c":true}),&memory,&mut work).unwrap();let value=ProjectionColumnDataV1::from_json_data(serde_json::json!({"next":7}),&memory,&mut work).unwrap();let second=first.with_object_member_data("a",Some(&value),&memory,&mut work).unwrap().with_object_member_data("c",None,&memory,&mut work).unwrap();drop(first);drop(value);assert!(memory.used()>0);assert_eq!(serde_json::to_value(&second).unwrap(),serde_json::json!({"a":{"next":7},"b":[1,2,3]}));drop(second);assert_eq!(memory.used(),0);
 }
 #[test]fn site_filter_preserves_original_duplicate_retain_and_owner_mask(){
  let memory=budget();let actual=ProjectionColumnDataV1::from_json_data(serde_json::json!({"one":{"owner_agent_id":"a","authorized_agent_ids":["b","b","c"]},"two":{"owner_agent_id":"c","authorized_agent_ids":[]}}),&memory,&mut work).unwrap();let visible=project_site_authorities_data_v1(&actual,Some("b"),&memory,&mut work).unwrap();assert_eq!(serde_json::to_value(&visible).unwrap(),serde_json::json!({"one":{"owner_agent_id":"","authorized_agent_ids":["b","b"]}}));assert_eq!(serde_json::to_value(&actual).unwrap()["one"]["owner_agent_id"],"a");
 }
 #[test]fn failed_pathcopy_preserves_parent_and_releases_attempt(){
  let memory=budget();let actual=ProjectionColumnDataV1::from_json_data(serde_json::json!({"a":1,"b":2,"c":3}),&memory,&mut work).unwrap();let before=memory.used();let mut calls=0;let result=actual.with_object_member_data("b",None,&memory,&mut|_,_|{calls+=1;if calls==3{Err("cancelled")}else{Ok(())}});assert!(matches!(result,Err(ProjectionColumnErrorV1::Work("cancelled"))));assert_eq!(memory.used(),before);assert_eq!(serde_json::to_value(&actual).unwrap(),serde_json::json!({"a":1,"b":2,"c":3}));
 }
 #[test]fn event_array_retention_keeps_original_ordinals_and_shares_successor(){
  let memory=budget();let mut events=ProjectionColumnDataV1::from_json_data(serde_json::json!([0,1,2,3,4,5,6,7,8,9]),&memory,&mut work).unwrap();let parent=events.share_data(&memory,&mut work).unwrap();events=events.without_array_prefix_data_v1(7,&memory,&mut work).unwrap();let next=ProjectionColumnDataV1::from_json_data(serde_json::json!(10),&memory,&mut work).unwrap();events=events.with_array_append_data_v1(&next,&memory,&mut work).unwrap();drop(parent);drop(next);assert_eq!(serde_json::to_value(&events).unwrap(),serde_json::json!([7,8,9,10]));drop(events);assert_eq!(memory.used(),0);
 }
}

impl ProjectionColumnDataV1{
 /// Actual append DATA successor; ordinal keys remain stable through prefix
 /// retention so normal updates never renumber or copy unchanged events.
 pub fn with_array_append_data_v1<E>(&self,value:&Self,memory:&MemoryBudget,work:&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<Self,ProjectionColumnErrorV1<E>>{
  if !self.uses_budget(memory)||!value.uses_budget(memory){return Err(ProjectionColumnErrorV1::Invalid("projection array pool"))}let Kind::Array{start,len,items}=&self.root.kind else{return Err(ProjectionColumnErrorV1::Invalid("projection column is not array"))};let next=start.checked_add(*len).ok_or(ProjectionColumnErrorV1::Invalid("projection array ordinal overflow"))?;let len=len.checked_add(1).ok_or(ProjectionColumnErrorV1::Invalid("projection array length overflow"))?;start.checked_add(len).ok_or(ProjectionColumnErrorV1::Invalid("projection array end overflow"))?;let mut build=Build{memory,work};let fee=build.reserve(std::mem::size_of::<Self>())?;let items=build.put(items,Key::Index(next),Arc::clone(&value.root))?;let root_fee=build.reserve(std::mem::size_of::<JsonNode>()+2*std::mem::size_of::<usize>())?;Ok(Self{root:Arc::new(JsonNode{kind:Kind::Array{start:*start,len,items},_fee:root_fee}),_fee:fee})
 }
 pub fn without_array_prefix_data_v1<E>(&self,removed:usize,memory:&MemoryBudget,work:&mut dyn FnMut(u64,u64)->Result<(),E>)->Result<Self,ProjectionColumnErrorV1<E>>{
  if !self.uses_budget(memory){return Err(ProjectionColumnErrorV1::Invalid("projection array pool"))}let Kind::Array{start,len,items}=&self.root.kind else{return Err(ProjectionColumnErrorV1::Invalid("projection column is not array"))};let len=len.checked_sub(removed).ok_or(ProjectionColumnErrorV1::Invalid("projection array retention count"))?;let start=start.checked_add(removed).ok_or(ProjectionColumnErrorV1::Invalid("projection array retention ordinal"))?;let mut build=Build{memory,work};let fee=build.reserve(std::mem::size_of::<Self>())?;let items=build.keep_array_suffix(items,start)?;let root_fee=build.reserve(std::mem::size_of::<JsonNode>()+2*std::mem::size_of::<usize>())?;Ok(Self{root:Arc::new(JsonNode{kind:Kind::Array{start,len,items},_fee:root_fee}),_fee:fee})
 }
}

#[path="current_commitment_path.rs"]
mod data_path;
pub use data_path::ProjectionDataPathRefV1;

#[path="original_formatter_rows.rs"] mod original_formatter_rows;

#[path="current_commitment_text_set.rs"]
mod text_set;

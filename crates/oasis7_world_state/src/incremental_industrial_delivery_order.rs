//! Fixed two-leaf Delivered publication using genuine Current order closure.
//! The operation is structural DATA; Runtime owns all delivery business checks.
use super::*;
use crate::incremental_world_rule_composite::{WorldRuleCompositeReadWorkV1,WorldRuleCompositeWriteWorkV1};
use crate::{incremental_world_rule_receipts::{AgentOrderEditV1,AgentOrdersQualificationV1,AgentOrdersOverlayV1,patch_world_rule_agent_orders_v1},incremental_world_rule_composite_overlay::bind_current_world_rule_branch_v1,incremental_typed_record_diff::point_typed_column_v1};
pub fn mark_current_industrial_order_delivered_v1<M,S,R,E,C,W,CW>(parent:&ReservedValue<AgentOrdersQualificationV1>,source:&ReservedValue<CurrentAgentOrdersV1>,roots:&RootSetV1,id:&str,now:u64,state:&S,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,write:&mut MemberRecordWriteContext<'_,W,CW>,scan:&mut StateScanBudget,read_work:&mut WorldRuleCompositeReadWorkV1,write_work:&mut WorldRuleCompositeWriteWorkV1,guard:&PreparedGuard,live:&ExecutionFence)->Result<ReservedValue<AgentOrdersOverlayV1>,WorldFieldOverlayErrorV1>
where M:MerkleNodeReader,S:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,W:ValueNodeReader+StatePageWriter,CW:FnMut()->bool {
 ctx.transaction.contribute(|tx|{
  // Reborrow the same transaction; this fixed operation cannot accept a raw
  // branch root, an expected digest supplied by caller or a caller permission.
  let mut local=WorldFieldOverlayContextV1 {transaction:tx,values:ValueReadContext {source:ctx.values.source,memory:ctx.values.memory,class:ctx.values.class,io:&mut *ctx.values.io},key_work:&mut *ctx.key_work,semantic_bytes:ctx.semantic_bytes,record_bytes:ctx.record_bytes,take_state_error:&mut ctx.take_state_error,cancelled:&mut ctx.cancelled};
  validate(source,&mut local,guard,live)?;let p=profile();
  let _phase=write.memory.reserve(32<<10,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let (_,order)=selected_order_column(source,id,&mut local,read_work.max_tree_heap)?;
  let ColumnCellV1::Root(body)=&order.cell else{return Err(bad().into());};
  let (phase_leaf,delivered_leaf)={
   let mut read=WorldFieldReadContextV1 {root:body,state,values:ValueReadContext {source:local.values.source,memory:local.values.memory,class:local.values.class,io:&mut *local.values.io},key_work:&mut *local.key_work,semantic_bytes:local.semantic_bytes,record_bytes:local.record_bytes,take_state_error:&mut local.take_state_error,cancelled:&mut local.cancelled};
   let work=&mut *read_work;let trees=work.max_retained_trees;let max=work.max_tree_heap;
   let mut d=ChangeDecoder {ctx:&mut read,scan,work,profile:p,trees_remaining:trees,tree_depth:0};
   let phase=point_typed_column_v1(&p,body,"phase",&mut d,max)?;let ColumnCellV1::Root(phase_root)=&phase.cell else{return Err(bad().into());};
   let actual=recover_typed_record_body_v1::<crate::world_rules::AgentIndustrialPhaseV1,_,_,_,_>(phase_root,&p,d.ctx,d.scan,d.work)?;
   let delivered=point_typed_column_v1(&p,body,"delivered_at",&mut d,max)?;
   if *actual.value()!=crate::world_rules::AgentIndustrialPhaseV1::AwaitingDelivery||!matches!(&delivered.cell,ColumnCellV1::Unit){return Err(bad().into());}
   d.check()?;drop(d);
   let mut records=read.records();let key=records.key(p.field,WorldFieldRecordKindV1::Entry,"phase")?.0;
   let phase_leaf=incremental_record_tree::lookup(body,&key,&mut records,p.labels)?.ok_or_else(bad)?;
   let key=records.key(p.field,WorldFieldRecordKindV1::Entry,"delivered_at")?.0;
   let delivered_leaf=incremental_record_tree::lookup(body,&key,&mut records,p.labels)?.ok_or_else(bad)?;(phase_leaf,delivered_leaf)
  };
  let guard=local.transaction.guard();
  let current=bind_current_world_rule_branch_v1(parent,roots,state,&mut local,scan,&mut *read_work,&guard,live)?;
  let guard=local.transaction.guard();
  patch_world_rule_agent_orders_v1(parent,roots,Some(&current),&[
   AgentOrderEditV1::Phase {key:id,expected:phase_leaf,after:crate::world_rules::AgentIndustrialPhaseV1::Delivered},
   AgentOrderEditV1::DeliveredAt {key:id,expected:delivered_leaf,after:Some(now)},
  ],state,&mut local,write,scan,read_work,write_work,&guard,live)?.ok_or_else(||bad().into())
 })
}

fn selected_order_column<M,R,E,C>(source:&ReservedValue<CurrentAgentOrdersV1>,id:&str,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,max_heap:usize)->Result<([u8;32],ReservedValue<crate::incremental_record_columns_read::ColumnRowV1>),WorldFieldOverlayErrorV1>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool {
 let p=profile();let root=source.body.as_deref().ok_or_else(bad)?;
 let mut records=InventoryValueReader {values:&mut ctx.values,key_work:&mut *ctx.key_work,semantic_bytes:ctx.semantic_bytes,record_bytes:ctx.record_bytes,cancelled:&mut ctx.cancelled};
 let(key,_)=records.key(p.field,WorldFieldRecordKindV1::Entry,&("entry",id))?;
 let value=incremental_record_tree::lookup(root,&key,&mut records,p.labels)?.ok_or_else(bad)?;
 let row=crate::incremental_record_columns_read::read_column_row_v1(ColumnReadProfileV1 {field:p.field,schema:p.schema,groups:&[],reference_schema:p.reference_schema},&StateEntryV1 {namespace:root.namespace,key:key.as_bytes().to_vec(),value},&mut records,max_heap)?;
 if !matches!(&row.cell,ColumnCellV1::Root(_)){return Err(bad().into());}Ok((value,row))
}
/// Replace only one real selected Current Order. Runtime owns the original
/// phase/refund reducer and the actual-shape admission of its after DTO. No
/// caller-supplied expected root or embedded-ID equality grants authority.
pub fn replace_current_industrial_order_v1<M,S,R,E,C,W,CW>(parent:&ReservedValue<AgentOrdersQualificationV1>,source:&ReservedValue<CurrentAgentOrdersV1>,roots:&RootSetV1,id:&str,after:&crate::world_rules::AgentIndustrialOrderV1,state:&S,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,write:&mut MemberRecordWriteContext<'_,W,CW>,scan:&mut StateScanBudget,read_work:&mut WorldRuleCompositeReadWorkV1,write_work:&mut WorldRuleCompositeWriteWorkV1,guard:&PreparedGuard,live:&ExecutionFence)->Result<ReservedValue<AgentOrdersOverlayV1>,WorldFieldOverlayErrorV1>
where M:MerkleNodeReader,S:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,W:ValueNodeReader+StatePageWriter,CW:FnMut()->bool {
 ctx.transaction.contribute(|tx|{
  let mut local=WorldFieldOverlayContextV1 {transaction:tx,values:ValueReadContext {source:ctx.values.source,memory:ctx.values.memory,class:ctx.values.class,io:&mut *ctx.values.io},key_work:&mut *ctx.key_work,semantic_bytes:ctx.semantic_bytes,record_bytes:ctx.record_bytes,take_state_error:&mut ctx.take_state_error,cancelled:&mut ctx.cancelled};
  validate(source,&mut local,guard,live)?;
  if local.values.class!=MemoryClass::Ordinary||!local.values.memory.same_pool(write.memory)||!local.transaction.uses_budget(write.memory){return Err(bad().into());}
  let _phase=write.memory.reserve(32<<10,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let (expected,_record)=selected_order_column(source,id,&mut local,read_work.max_tree_heap)?;
  let guard=local.transaction.guard();let current=bind_current_world_rule_branch_v1(parent,roots,state,&mut local,scan,read_work,&guard,live)?;
  let guard=local.transaction.guard();
  patch_world_rule_agent_orders_v1(parent,roots,Some(&current),&[AgentOrderEditV1::Put {key:id,expected:Some(expected),order:after}],state,&mut local,write,scan,read_work,write_work,&guard,live)?.ok_or_else(||bad().into())
 })
}

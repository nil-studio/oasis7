//! Small fixed field store shared by the original delta folds and qualified
//! initial folds; this contains original rows, never execution State.
use super::*;
use super::original_rows::OriginalProjectionRowsDataV1 as Rows;
use std::sync::Arc;
pub(super) trait RecordMapsV1{
 fn map_data_v1(&self,name:&str,common:&mut FormalBoundedContextV1<'_>)->Result<Option<&Arc<Rows>>,BodyMutationError>;
 fn sequence_data_v1(&self,name:&str,common:&mut FormalBoundedContextV1<'_>)->Result<&Arc<Rows>,BodyMutationError>{self.map_data_v1(name,common)?.ok_or_else(||invalid("qualified sequence field missing"))}
 fn replace_original_sequence_data_v1(&mut self,name:&str,rows:Rows,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{self.replace_original_map_data_v1(name,rows,common)}
 fn replace_original_map_data_v1(&mut self,name:&str,rows:Rows,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>;
}
impl RecordMapsV1 for super::state_columns::OriginalStateColumnsDataV1{
 fn sequence_data_v1(&self,n:&str,c:&mut FormalBoundedContextV1<'_>)->Result<&Arc<Rows>,BodyMutationError>{self.sequence_data_v1(n,c)}
 fn replace_original_sequence_data_v1(&mut self,n:&str,r:Rows,c:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{self.replace_original_sequence_data_v1(n,r,c)}

 fn map_data_v1(&self,n:&str,c:&mut FormalBoundedContextV1<'_>)->Result<Option<&Arc<Rows>>,BodyMutationError>{self.map_data_v1(n,c)}
 fn replace_original_map_data_v1(&mut self,n:&str,r:Rows,c:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{self.replace_original_map_data_v1(n,r,c)}
}
pub(super) struct QualifiedRecordMapsV1<const N:usize>{fields:[&'static str;N],rows:[Arc<Rows>;N],_fee:MemoryReservation}
impl<const N:usize> QualifiedRecordMapsV1<N>{
 pub(super) fn empty_v1(fields:[&'static str;N],common:&mut FormalBoundedContextV1<'_>)->Result<Self,BodyMutationError>{
 let n=std::mem::size_of::<Self>().checked_add(N.checked_mul(std::mem::size_of::<Rows>()+2*std::mem::size_of::<usize>()).ok_or_else(||invalid("qualified field layout"))?).ok_or_else(||invalid("qualified field layout"))?;common.charge(N as u64,n as u64).map_err(paid)?;let fee=common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let phase=common.memory().reserve(N.checked_mul(std::mem::size_of::<Arc<Rows>>()).ok_or_else(||invalid("qualified field layout"))?,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;let mut temporary=Vec::with_capacity(N);if temporary.capacity()!=N{return Err(invalid("qualified field capacity"))}for _ in 0..N{common.consume_steps(1).map_err(paid)?;temporary.push(Arc::new(Rows::empty_data_v1(common)?))}let rows=temporary.try_into().map_err(|_|invalid("qualified fixed field array"))?;drop(phase);Ok(Self{fields,rows,_fee:fee})
 }
}
impl<const N:usize> RecordMapsV1 for QualifiedRecordMapsV1<N>{
 fn map_data_v1(&self,name:&str,common:&mut FormalBoundedContextV1<'_>)->Result<Option<&Arc<Rows>>,BodyMutationError>{if !self._fee.uses_budget(common.memory()){return Err(invalid("qualified field pool"))}for(i,field)in self.fields.iter().enumerate(){common.charge(1,(name.len()+field.len())as u64).map_err(paid)?;if *field==name{self.rows[i].validate(common)?;return Ok(Some(&self.rows[i]))}}Ok(None)}
 fn replace_original_map_data_v1(&mut self,name:&str,rows:Rows,common:&mut FormalBoundedContextV1<'_>)->Result<(),BodyMutationError>{rows.validate(common)?;for(i,field)in self.fields.iter().enumerate(){common.charge(1,(name.len()+field.len())as u64).map_err(paid)?;if *field==name{let n=std::mem::size_of::<Rows>()+2*std::mem::size_of::<usize>();common.charge(1,n as u64).map_err(paid)?;let fee=common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;self.rows[i]=Arc::new(rows);self._fee.merge(&mut{fee}).map_err(invalid)?;return Ok(())}}Err(invalid("qualified fixed field missing"))}
}

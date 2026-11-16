//! Sparse Active predicate inputs derived only after the complete original
//! Emergency codec. These metadata rows are DATA, not authorization results.
//! Full records are selected again from actual Current by the Runtime owner.
use super::*;
#[derive(Debug)]
pub struct EmergencyEvidenceMetadataV1 {pub outer_id:String,pub region_id:String,pub region_budget_lineage:String,pub end:Option<u64>}
#[derive(Debug)]
pub struct EmergencyServiceMetadataV1 {pub outer_id:String,pub obligation_id:String,pub region_id:String,pub registered_at:u64}
#[derive(Debug)]
pub struct EmergencyOverflowMetadataV1 {pub region_id:String,pub starts_at:u64}
pub struct EmergencyActiveInputsV1 {
    evidence:Vec<Arc<ReservedValue<EmergencyEvidenceMetadataV1>>>,services:Vec<Arc<ReservedValue<EmergencyServiceMetadataV1>>>,overflow:Vec<Arc<ReservedValue<EmergencyOverflowMetadataV1>>>,
    _evidence_slots:MemoryReservation,_service_slots:MemoryReservation,_overflow_slots:MemoryReservation,_fee:MemoryReservation,memory:MemoryBudget,class:MemoryClass,
}
impl EmergencyActiveInputsV1 {
    pub fn evidence(&self)->impl ExactSizeIterator<Item=&EmergencyEvidenceMetadataV1>{self.evidence.iter().map(|r|&***r)}
    pub fn services(&self)->impl ExactSizeIterator<Item=&EmergencyServiceMetadataV1>{self.services.iter().map(|r|&***r)}
    pub fn overflow_rows(&self)->impl ExactSizeIterator<Item=&EmergencyOverflowMetadataV1>{self.overflow.iter().map(|r|&***r)}
    pub fn overflowed(&self,region:&str,starts_at:u64)->bool{self.overflow.binary_search_by(|r|(r.region_id.as_str(),r.starts_at).cmp(&(region,starts_at))).is_ok()}
    pub(super) fn check_budget(&self,memory:&MemoryBudget,class:MemoryClass)->Result<(),ValueTreeError>{
        if self.class!=class||!self.memory.same_pool(memory)||self.evidence.iter().any(|r|!r.uses_budget(memory))||self.services.iter().any(|r|!r.uses_budget(memory))||self.overflow.iter().any(|r|!r.uses_budget(memory)){return Err(invalid());}Ok(())
    }
    /// Only used at the closed profile's genuinely absent before/after edge.
    pub(super) fn empty(memory:&MemoryBudget,class:MemoryClass)->Result<Self,ValueTreeError>{
        Ok(Self {evidence:Vec::new(),services:Vec::new(),overflow:Vec::new(),_evidence_slots:memory.reserve(0,class)?,_service_slots:memory.reserve(0,class)?,_overflow_slots:memory.reserve(0,class)?,_fee:memory.reserve(size_of::<Self>(),class)?,memory:memory.clone(),class})
    }
    /// Resource-only readmission follows the existing lifecycle directory's
    /// MemoryBudget/cancellation ABI. It performs no original IO, qualification
    /// scan or fresh/reset Work: only paid immutable metadata containers/copies.
    pub(super) fn readmit_ordinary(&self,memory:&MemoryBudget,mut cancelled:impl FnMut()->bool)->Result<Self,ValueTreeError>{self.share_readmit(memory,MemoryClass::Ordinary,&mut cancelled)}
    pub(super) fn share_readmit(&self,memory:&MemoryBudget,class:MemoryClass,cancelled:&mut impl FnMut()->bool)->Result<Self,ValueTreeError>{
        self.check_budget(memory,self.class)?;let mut out=Self::empty(memory,class)?;
        for row in &self.evidence{if cancelled(){return Err(ValueTreeError::Cancelled);}let copy=if class==self.class{row.clone()}else{evidence_row(&row.outer_id,&row.region_id,&row.region_budget_lineage,row.end,memory,class)?};push(&mut out.evidence,copy,&mut out._evidence_slots,memory,class)?;}
        for row in &self.services{if cancelled(){return Err(ValueTreeError::Cancelled);}let copy=if class==self.class{row.clone()}else{service_row(&row.outer_id,&row.obligation_id,&row.region_id,row.registered_at,memory,class)?};push(&mut out.services,copy,&mut out._service_slots,memory,class)?;}
        for row in &self.overflow{if cancelled(){return Err(ValueTreeError::Cancelled);}let copy=if class==self.class{row.clone()}else{overflow_row(&row.region_id,row.starts_at,memory,class)?};push(&mut out.overflow,copy,&mut out._overflow_slots,memory,class)?;}
        if cancelled(){return Err(ValueTreeError::Cancelled);}Ok(out)
    }
    pub(super) fn share(&self,memory:&MemoryBudget,class:MemoryClass,scan:&mut StateScanBudget,cancelled:&mut impl FnMut()->bool)->Result<Self,ValueTreeError>{
        self.check_budget(memory,self.class)?;let mut out=Self::empty(memory,class)?;
        for row in &self.evidence {spend_counts(scan,cancelled,1,row.outer_id.len()+row.region_id.len()+row.region_budget_lineage.len())?;let copy=if self.class==class{row.clone()}else{evidence_row(&row.outer_id,&row.region_id,&row.region_budget_lineage,row.end,memory,class)?};push(&mut out.evidence,copy,&mut out._evidence_slots,memory,class)?;}
        for row in &self.services {spend_counts(scan,cancelled,1,row.outer_id.len()+row.obligation_id.len()+row.region_id.len())?;let copy=if self.class==class{row.clone()}else{service_row(&row.outer_id,&row.obligation_id,&row.region_id,row.registered_at,memory,class)?};push(&mut out.services,copy,&mut out._service_slots,memory,class)?;}
        for row in &self.overflow {spend_counts(scan,cancelled,1,row.region_id.len())?;let copy=if self.class==class{row.clone()}else{overflow_row(&row.region_id,row.starts_at,memory,class)?};push(&mut out.overflow,copy,&mut out._overflow_slots,memory,class)?;}
        Ok(out)
    }
}
fn row_fee<T>(memory:&MemoryBudget,class:MemoryClass,bytes:usize)->Result<MemoryReservation,ValueTreeError>{memory.reserve(directory_arc_layout::<T>()?.checked_add(bytes).ok_or_else(limited)?,class).map_err(Into::into)}
fn evidence_row(outer:&str,region:&str,lineage:&str,end:Option<u64>,memory:&MemoryBudget,class:MemoryClass)->Result<Arc<ReservedValue<EmergencyEvidenceMetadataV1>>,ValueTreeError>{
 let bytes=outer.len().checked_add(region.len()).and_then(|n|n.checked_add(lineage.len())).ok_or_else(limited)?;let fee=row_fee::<EmergencyEvidenceMetadataV1>(memory,class,bytes)?;
 Ok(Arc::new(fee.retain(EmergencyEvidenceMetadataV1 {outer_id:string_copy(outer)?,region_id:string_copy(region)?,region_budget_lineage:string_copy(lineage)?,end})))
}
fn service_row(outer:&str,id:&str,region:&str,registered_at:u64,memory:&MemoryBudget,class:MemoryClass)->Result<Arc<ReservedValue<EmergencyServiceMetadataV1>>,ValueTreeError>{
 let bytes=outer.len().checked_add(id.len()).and_then(|n|n.checked_add(region.len())).ok_or_else(limited)?;let fee=row_fee::<EmergencyServiceMetadataV1>(memory,class,bytes)?;
 Ok(Arc::new(fee.retain(EmergencyServiceMetadataV1 {outer_id:string_copy(outer)?,obligation_id:string_copy(id)?,region_id:string_copy(region)?,registered_at})))
}
fn overflow_row(region:&str,starts_at:u64,memory:&MemoryBudget,class:MemoryClass)->Result<Arc<ReservedValue<EmergencyOverflowMetadataV1>>,ValueTreeError>{let fee=row_fee::<EmergencyOverflowMetadataV1>(memory,class,region.len())?;Ok(Arc::new(fee.retain(EmergencyOverflowMetadataV1 {region_id:string_copy(region)?,starts_at})))}
fn push<T>(out:&mut Vec<Arc<ReservedValue<T>>>,value:Arc<ReservedValue<T>>,fee:&mut MemoryReservation,memory:&MemoryBudget,class:MemoryClass)->Result<(),ValueTreeError>{
 if out.len()==out.capacity(){let cap=out.capacity().checked_mul(2).ok_or_else(limited)?.max(4);let mut next=memory.reserve(cap.checked_mul(size_of::<Arc<ReservedValue<T>>>()).ok_or_else(limited)?,class)?;fee.merge(&mut next).map_err(ValueTreeError::Invalid)?;out.try_reserve_exact(cap-out.len()).map_err(|_|limited())?;if out.capacity()>cap{return Err(limited());}}out.push(value);Ok(())
}
fn remove<T>(rows:&mut Vec<Arc<ReservedValue<T>>>,at:Result<usize,usize>){if let Ok(at)=at{rows.remove(at);}}
fn insert_at<T>(rows:&mut Vec<Arc<ReservedValue<T>>>,row:Arc<ReservedValue<T>>,at:usize,fee:&mut MemoryReservation,memory:&MemoryBudget,class:MemoryClass)->Result<(),ValueTreeError>{push(rows,row,fee,memory,class)?;rows[at..].rotate_right(1);Ok(())}
fn project_evidence<M,R,E,C>(out:&mut EmergencyActiveInputsV1,before:Option<&StateRootV1>,after:Option<&StateRootV1>,d:&mut ChangeDecoder<'_,'_,M,R,E,C>)->Result<(),ValueTreeError>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool {
 let p=emergency_profile();let old=map_root(before,"evidence",d)?;let new=map_root(after,"evidence",d)?;
 visit_entries(&old,&new,d,|before,after,d|{let cell=after.or(before).ok_or_else(invalid)?;let Some(key)=outer_key(&p,cell,d)? else{return d.check()};let id=outer_text(&key)?;
  let text=id.len().checked_mul(out.evidence.len()+1).ok_or_else(limited)?;spend(d,out.evidence.len()+1,text.checked_add(out.evidence.len()*size_of::<Arc<ReservedValue<EmergencyEvidenceMetadataV1>>>()).ok_or_else(limited)?)?;
  if before.is_some(){let at=out.evidence.binary_search_by(|r|r.outer_id.as_str().cmp(id));if at.is_err(){return Err(invalid());}remove(&mut out.evidence,at);}
  if let Some(cell)=after {let body=root(&p,cell,d)?;let record=recover_typed_record_body_v1::<oasis7_agent_api::world_action::EmergencyEvidenceV1,_,_,_,_>(&body,&p,d.ctx,d.scan,d.work)?;let value=record.value();
   spend(d,1,id.len().checked_add(value.region_id.len()).and_then(|n|n.checked_add(value.region_budget_lineage.len())).ok_or_else(limited)?)?;
   let row=evidence_row(id,&value.region_id,&value.region_budget_lineage,crate::world_rules::evidence_end(value),d.ctx.values.memory,d.ctx.values.class)?;
   let at=out.evidence.binary_search_by(|r|r.outer_id.as_str().cmp(id)).err().ok_or_else(invalid)?;insert_at(&mut out.evidence,row,at,&mut out._evidence_slots,d.ctx.values.memory,d.ctx.values.class)?;
  }d.check()
 })
}
fn project_services<M,R,E,C>(out:&mut EmergencyActiveInputsV1,before:Option<&StateRootV1>,after:Option<&StateRootV1>,d:&mut ChangeDecoder<'_,'_,M,R,E,C>)->Result<(),ValueTreeError>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool {
 let p=emergency_profile();let old=map_root(before,"services",d)?;let new=map_root(after,"services",d)?;
 visit_entries(&old,&new,d,|before,after,d|{let cell=after.or(before).ok_or_else(invalid)?;let Some(key)=outer_key(&p,cell,d)? else{return d.check()};let id=outer_text(&key)?;
  spend(d,out.services.len()+1,id.len().checked_mul(out.services.len()+1).and_then(|n|n.checked_add(out.services.len()*size_of::<Arc<ReservedValue<EmergencyServiceMetadataV1>>>())).ok_or_else(limited)?)?;
  if before.is_some(){let at=out.services.binary_search_by(|r|r.outer_id.as_str().cmp(id));if at.is_err(){return Err(invalid());}remove(&mut out.services,at);}
  if let Some(cell)=after{let body=root(&p,cell,d)?;let record=recover_typed_record_body_v1::<oasis7_agent_api::world_action::EmergencyServiceRegistrationV1,_,_,_,_>(&body,&p,d.ctx,d.scan,d.work)?;let value=record.value();
   spend(d,1,id.len().checked_add(value.obligation_id.len()).and_then(|n|n.checked_add(value.region_id.len())).ok_or_else(limited)?)?;
   let row=service_row(id,&value.obligation_id,&value.region_id,value.registered_at,d.ctx.values.memory,d.ctx.values.class)?;
   let at=out.services.binary_search_by(|r|r.outer_id.as_str().cmp(id)).err().ok_or_else(invalid)?;insert_at(&mut out.services,row,at,&mut out._service_slots,d.ctx.values.memory,d.ctx.values.class)?;
  }d.check()
 })
}
fn project_overflow<M,R,E,C>(out:&mut EmergencyActiveInputsV1,before:Option<&StateRootV1>,after:Option<&StateRootV1>,d:&mut ChangeDecoder<'_,'_,M,R,E,C>)->Result<(),ValueTreeError>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool {
 let p=emergency_profile();let old=map_root(before,"overflowed_windows",d)?;let new=map_root(after,"overflowed_windows",d)?;
 visit_entries(&old,&new,d,|before,after,d|{let cell=after.or(before).ok_or_else(invalid)?;let key=key(&p,cell,d)?;
  if matches!(&*key,ColumnKeyV1::Text(s) if s=="_length"){return d.check();}
  let ColumnKeyV1::Pair(tag,tuple)=&*key else{return Err(invalid())};if !matches!(&**tag,ColumnKeyV1::Text(s) if s=="entry"){return Err(invalid());}
  // This only projects the original already-closed CollectionKey grammar;
  // semantic bytes are parsed exclusively by the shared key() decoder.
  let ColumnKeyV1::Pair(region,window)=&**tuple else{return Err(invalid())};let (ColumnKeyV1::Text(region),ColumnKeyV1::Ordinal(window))=(&**region,&**window) else{return Err(invalid())};
  spend(d,out.overflow.len()+1,region.len().checked_mul(out.overflow.len()+1).and_then(|n|n.checked_add(out.overflow.len()*size_of::<Arc<ReservedValue<EmergencyOverflowMetadataV1>>>())).ok_or_else(limited)?)?;
  if before.is_some(){let at=out.overflow.binary_search_by(|r|(r.region_id.as_str(),r.starts_at).cmp(&(region.as_str(),*window)));if at.is_err(){return Err(invalid());}remove(&mut out.overflow,at);}
  if after.is_some(){let row=overflow_row(region,*window,d.ctx.values.memory,d.ctx.values.class)?;let at=out.overflow.binary_search_by(|r|(r.region_id.as_str(),r.starts_at).cmp(&(region.as_str(),*window))).err().ok_or_else(invalid)?;insert_at(&mut out.overflow,row,at,&mut out._overflow_slots,d.ctx.values.memory,d.ctx.values.class)?;}d.check()
 })
}
/// Private hook: caller has completed the original Emergency accept_body.
/// All GEN/cold/accepted/Current lifecycle entrypoints share this same delta.
pub(super) fn update<M,R,E,C>(parent:Option<&EmergencyActiveInputsV1>,before:Option<&StateRootV1>,after:Option<&StateRootV1>,d:&mut ChangeDecoder<'_,'_,M,R,E,C>)->Result<EmergencyActiveInputsV1,ValueTreeError>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool {
 if before.is_some()&&parent.is_none(){return Err(invalid());}
 let mut out=match parent{Some(p)=>p.share(d.ctx.values.memory,d.ctx.values.class,d.scan,&mut d.ctx.cancelled)?,None=>EmergencyActiveInputsV1::empty(d.ctx.values.memory,d.ctx.values.class)?};
 if before!=after{project_evidence(&mut out,before,after,d)?;project_services(&mut out,before,after,d)?;project_overflow(&mut out,before,after,d)?;}d.check()?;Ok(out)
}

//! Fixed three-profile Current source and original-codec CAS. DATA only.
use super::*;
use crate::incremental_world_native_industry::{ProductProfilesFieldV1,RecipeProfilesFieldV1,FactoryProfilesFieldV1};
use oasis7_formal_protocol::incremental_state::StatePageWriter;
macro_rules! profile {
 ($read:ident,$write:ident,$slot:ident,$field:ty,$value:ty)=>{
  pub fn $read<M,P,R,E,C>(id:&str,parent:&ReservedValue<WorldNativeFieldsQualificationV1>,roots:&RootSetV1,tx:&mut ExecutionTransaction<'_,M>,ctx:&mut WorldFieldReadContextV1<'_,P,R,E,C>,scan:&mut StateScanBudget,work:&mut WorldNativeSelectedReadWorkV1,guard:&PreparedGuard,live:&ExecutionFence)->Result<ReservedValue<CurrentFactoryNativeRecordV1<$value>>,WorldFieldOverlayErrorV1>
  where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool {
   if !parent.uses_budget(ctx.values.memory)||parent.class!=MemoryClass::Ordinary{return tx.contribute(|_|Err(invalid().into()))}
   selected::<$field,_,_,_,_,_>(SelectedKey::Receipt(id),&parent.$slot,roots,tx,ctx,scan,work,guard,live)
  }
  pub fn $write<M,P,R,E,C,W,CW>(expected:&ReservedValue<CurrentFactoryNativeRecordV1<$value>>,id:&str,after:&$value,tx:&mut ExecutionTransaction<'_,M>,ctx:&mut WorldFieldReadContextV1<'_,P,R,E,C>,write:&mut MemberRecordWriteContext<'_,W,CW>,records:&mut u64,work:&mut TypedRecordWriteWorkV1,live:&ExecutionFence)->Result<(),WorldFieldOverlayErrorV1>
  where M:MerkleNodeReader,P:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,W:ValueNodeReader+StatePageWriter,CW:FnMut()->bool {
   tx.contribute(|tx|{
    tx.validate_guard(&expected.guard,live)?;
    if !expected.uses_budget(write.memory)||!expected.memory.same_pool(write.memory)||!tx.uses_budget(write.memory)||ctx.values.class!=MemoryClass::Ordinary{return Err(invalid().into())}
    read::check(ctx)?;
    let ns=<$field>::profile()?.namespace()?;
    if tx.read(ns,expected.key.as_bytes())?!=expected.value_root{return Err(invalid().into())}
    let _phase=write.memory.reserve(id.len().checked_add(32<<10).ok_or_else(invalid)?,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
    let mut row=None;
    prepare_world_native_record_v1::<$field>(&id.to_owned(),after,write,records,work,&mut |r|{if row.is_some(){return Err(invalid())}row=Some(r);Ok(())})?;
    let row=row.ok_or_else(invalid)?;
    if row.namespace!=ns||row.mutation.key!=expected.key.as_bytes()||row.mutation.after.is_none(){return Err(invalid().into())}
    read::check(ctx)?;if(write.cancelled)(){return Err(ValueTreeError::Cancelled.into())}
    let guard=tx.guard();tx.validate_guard(&guard,live)?;
    tx.write(ns,expected.key.as_bytes(),row.mutation.after)?;
    read::check(ctx)?;if(write.cancelled)(){return Err(ValueTreeError::Cancelled.into())}Ok(())
   })
  }
 }
}
profile!(read_current_governed_product_profile_v1,write_current_governed_product_profile_v1,product_profiles,ProductProfilesFieldV1,oasis7_wasm_abi::ProductProfileV1);
profile!(read_current_governed_recipe_profile_v1,write_current_governed_recipe_profile_v1,recipe_profiles,RecipeProfilesFieldV1,oasis7_wasm_abi::RecipeProfileV1);
profile!(read_current_governed_factory_profile_v1,write_current_governed_factory_profile_v1,factory_profiles,FactoryProfilesFieldV1,oasis7_wasm_abi::FactoryProfileV1);

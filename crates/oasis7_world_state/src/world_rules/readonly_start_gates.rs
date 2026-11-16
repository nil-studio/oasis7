//! Original lazy StartRoot selection and per-parent gates. Returned references
//! are DATA only; source authentication and the execution transaction remain
//! with the fixed caller.
use super::invalid;
use crate::{WorldError,MaterialLedgerId};
use oasis7_agent_api::world_action::*;
use std::mem::size_of;
fn reference_bytes(r:&IndustrialRefV1)->usize{r.world_binding.len()+r.domain.len()+r.schema.len()+r.id.len()+r.digest.len()+size_of::<u64>()}
fn ledger_bytes(v:&MaterialLedgerId)->usize{match v{MaterialLedgerId::World=>1,MaterialLedgerId::Agent(v)|MaterialLedgerId::Site(v)|MaterialLedgerId::Factory(v)=>v.len()+1}}
fn refusal<T,E>(reason:&str,work:&mut impl FnMut(u64,usize,usize)->Result<(),E>)->Result<Result<T,WorldError>,E>{work(1,reason.len(),reason.len()+size_of::<WorldError>())?;Ok(Err(invalid(reason)))}
pub(super) fn root<'a,E>(value:Option<&'a RootOperationV1>,root_ref:&IndustrialRefV1,actor:&str,world:&str,work:&mut impl FnMut(u64,usize,usize)->Result<(),E>)->Result<Result<&'a RootOperationV1,WorldError>,E>{
    if let Some(root)=value {work(1,reference_bytes(&root.root_ref)+reference_bytes(root_ref)+root.owner_id.len()+actor.len()+root_ref.world_binding.len()+world.len(),0)?;if root.root_ref==*root_ref&&root.owner_id==actor&&root_ref.world_binding==world&&!root.first_sink{return Ok(Ok(root));}}
    refusal("industrial_start_root_unavailable",work)
}
pub(super) fn lot<'a,E>(value:Option<&'a IndustrialLotV1>,parent:&IndustrialParentV1,input_ledger:&MaterialLedgerId,work:&mut impl FnMut(u64,usize,usize)->Result<(),E>)->Result<Result<&'a IndustrialLotV1,WorldError>,E>{
    if let Some(lot)=value {work(1,reference_bytes(&lot.lot_ref)+reference_bytes(&parent.lot_ref)+ledger_bytes(&lot.source_ledger)+ledger_bytes(input_ledger),0)?;if lot.lot_ref==parent.lot_ref&&lot.source_ledger==*input_ledger{return Ok(Ok(lot));}}
    refusal("industrial_start_source_not_arrived",work)
}
pub(super) fn authority<E>(value:Option<&IndustrialLotAuthorityV1>,slot:&IndustrialInputSlotV1,owner_binding:&str,now:u64,work:&mut impl FnMut(u64,usize,usize)->Result<(),E>)->Result<Result<(),WorldError>,E>{
    let Some(authority)=value else{return refusal("industrial_start_source_authority_missing",work)};
    work(1,authority.owner_binding_digest.len()+owner_binding.len()+authority.material_profile.id.len()+slot.material_profile.id.len()+authority.material_profile.version.len()+slot.material_profile.version.len()+authority.material_profile.digest.len()+slot.material_profile.digest.len(),0)?;
    if authority.owner_binding_digest!=owner_binding||authority.expires_at<now||authority.material_profile!=slot.material_profile{return refusal("industrial_start_lot_authority_stale",work);}Ok(Ok(()))
}

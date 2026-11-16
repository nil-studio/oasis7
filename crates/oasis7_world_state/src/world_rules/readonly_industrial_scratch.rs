//! Lazy selected source records for one local StartRoot rehearsal. These maps
//! are ordinary paid DATA, never an independently admitted Current branch.
use super::readonly_owner::IndustrialOwnerReadonlyFactsV1;
use oasis7_agent_api::world_action::*;
pub trait IndustrialScratchReadonlyFactsV1:IndustrialOwnerReadonlyFactsV1 {
    fn industrial_with_lot<T>(&self,id:&str,visit:impl FnOnce(Option<&IndustrialLotV1>)->Result<T,Self::Error>)->Result<T,Self::Error>;
    fn industrial_with_lot_authority<T>(&self,id:&str,visit:impl FnOnce(Option<&IndustrialLotAuthorityV1>)->Result<T,Self::Error>)->Result<T,Self::Error>;
    fn industrial_with_receipt<T>(&self,id:&str,visit:impl FnOnce(Option<&IndustrialReceiptV1>)->Result<T,Self::Error>)->Result<T,Self::Error>;
    fn industrial_with_disposition<T>(&self,id:&str,visit:impl FnOnce(Option<&DispositionPlanV1>)->Result<T,Self::Error>)->Result<T,Self::Error>;
    fn industrial_sink_present(&self,key:&str)->Result<bool,Self::Error>;
}
impl IndustrialScratchReadonlyFactsV1 for crate::WorldState {
 fn industrial_with_lot<T>(&self,id:&str,visit:impl FnOnce(Option<&IndustrialLotV1>)->Result<T,Self::Error>)->Result<T,Self::Error>{visit(self.world_rules.as_ref().and_then(|r|r.industrial_work.lots.get(id)))}
 fn industrial_with_lot_authority<T>(&self,id:&str,visit:impl FnOnce(Option<&IndustrialLotAuthorityV1>)->Result<T,Self::Error>)->Result<T,Self::Error>{visit(self.world_rules.as_ref().and_then(|r|r.industrial_work.imported_lot_authorities.get(id)))}
 fn industrial_with_receipt<T>(&self,id:&str,visit:impl FnOnce(Option<&IndustrialReceiptV1>)->Result<T,Self::Error>)->Result<T,Self::Error>{visit(self.world_rules.as_ref().and_then(|r|r.industrial_work.receipts.get(id)))}
 fn industrial_with_disposition<T>(&self,id:&str,visit:impl FnOnce(Option<&DispositionPlanV1>)->Result<T,Self::Error>)->Result<T,Self::Error>{visit(self.world_rules.as_ref().and_then(|r|r.industrial_work.dispositions.get(id)))}
 fn industrial_sink_present(&self,key:&str)->Result<bool,Self::Error>{Ok(self.world_rules.as_ref().is_some_and(|r|r.industrial_work.sink_keys.contains(key)))}
}

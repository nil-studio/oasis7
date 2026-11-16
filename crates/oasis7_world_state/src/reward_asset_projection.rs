//! Original pool-budget wire emitter over borrowed member maps.
use super::*;
use serde::{Serialize, Serializer, ser::SerializeStruct};
pub struct SystemOrderPoolBudgetProjectionFieldsV1<'a, C: Serialize + ?Sized, A: Serialize + ?Sized>
{
    pub epoch_index: u64,
    pub total_credit_budget: u64,
    pub remaining_credit_budget: u64,
    pub node_credit_caps: &'a C,
    pub node_credit_allocated: &'a A,
}
impl<C: Serialize + ?Sized, A: Serialize + ?Sized> Serialize
    for SystemOrderPoolBudgetProjectionFieldsV1<'_, C, A>
{
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut out = s.serialize_struct("SystemOrderPoolBudget", 5)?;
        out.serialize_field("epoch_index", &self.epoch_index)?;
        out.serialize_field("total_credit_budget", &self.total_credit_budget)?;
        out.serialize_field("remaining_credit_budget", &self.remaining_credit_budget)?;
        out.serialize_field("node_credit_caps", self.node_credit_caps)?;
        out.serialize_field("node_credit_allocated", self.node_credit_allocated)?;
        out.end()
    }
}
impl Serialize for SystemOrderPoolBudget {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self {
            epoch_index,
            total_credit_budget,
            remaining_credit_budget,
            node_credit_caps,
            node_credit_allocated,
        } = self;
        SystemOrderPoolBudgetProjectionFieldsV1 {
            epoch_index: *epoch_index,
            total_credit_budget: *total_credit_budget,
            remaining_credit_budget: *remaining_credit_budget,
            node_credit_caps,
            node_credit_allocated,
        }
        .serialize(s)
    }
}

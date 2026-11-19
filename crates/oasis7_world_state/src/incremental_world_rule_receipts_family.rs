//! Fixed original receipt/order consumers of the common branch family.
use super::*;
use crate::incremental_world_rule_composite_family::{
    WorldRuleCompositeQualificationV1, accept_world_rule_branch_transition_v1,
    qualify_world_rule_branch_v1, readmit_world_rule_branch_ordinary_v1,
};
use oasis7_formal_protocol::incremental_execution::{
    RootSetV1, TransitionRecordV1, TransitionVerifier,
};
pub type WorldRuleRequestsQualificationV1 = WorldRuleCompositeQualificationV1<
    std::collections::BTreeMap<String, crate::world_rules::WorldRuleReceiptV1>,
>;
pub type AgentOrdersQualificationV1 = WorldRuleCompositeQualificationV1<
    std::collections::BTreeMap<String, crate::world_rules::AgentIndustrialOrderV1>,
>;
macro_rules! owner {
    ($qualify:ident,$accept:ident,$readmit:ident,$ty:ty) => {
        pub fn $readmit(
            parent: &ReservedValue<WorldRuleCompositeQualificationV1<$ty>>,
            roots: &RootSetV1,
            memory: &oasis7_proto::memory_budget::MemoryBudget,
            cancelled: impl FnMut() -> bool,
        ) -> Result<ReservedValue<WorldRuleCompositeQualificationV1<$ty>>, ValueTreeError> {
            readmit_world_rule_branch_ordinary_v1(parent, roots, memory, cancelled)
        }
        pub fn $qualify<
            M: MerkleNodeReader,
            R: ValueNodeReader,
            E: FnMut() -> Option<ValueTreeError>,
            C: FnMut() -> bool,
        >(
            roots: &RootSetV1,
            ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
            scan: &mut StateScanBudget,
            work: &mut WorldRuleCompositeReadWorkV1,
        ) -> Result<ReservedValue<WorldRuleCompositeQualificationV1<$ty>>, ValueTreeError> {
            qualify_world_rule_branch_v1::<$ty, _, _, _, _>(roots, ctx, scan, work)
        }
        pub fn $accept<
            M: MerkleNodeReader,
            P: MerkleNodeReader,
            R: ValueNodeReader,
            E: FnMut() -> Option<ValueTreeError>,
            C: FnMut() -> bool,
        >(
            parent: &ReservedValue<WorldRuleCompositeQualificationV1<$ty>>,
            verifier: &TransitionVerifier<'_, P>,
            record: &TransitionRecordV1,
            ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
            scan: &mut StateScanBudget,
            work: &mut WorldRuleCompositeReadWorkV1,
        ) -> Result<
            ReservedValue<WorldRuleCompositeQualificationV1<$ty>>,
            crate::incremental_field_overlay::WorldFieldOverlayErrorV1,
        > {
            accept_world_rule_branch_transition_v1::<$ty, _, _, _, _, _>(
                parent, verifier, record, ctx, scan, work,
            )
        }
    };
}
owner!(
    qualify_world_rule_requests_v1,
    accept_world_rule_requests_transition_v1,
    readmit_world_rule_requests_ordinary_v1,
    std::collections::BTreeMap<String,crate::world_rules::WorldRuleReceiptV1>
);
owner!(
    qualify_world_rule_agent_orders_v1,
    accept_world_rule_agent_orders_transition_v1,
    readmit_world_rule_agent_orders_ordinary_v1,
    std::collections::BTreeMap<String,crate::world_rules::AgentIndustrialOrderV1>
);

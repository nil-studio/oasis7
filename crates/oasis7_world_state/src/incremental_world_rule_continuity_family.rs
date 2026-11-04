//! Fixed original continuity/emergency consumers of the common branch family.
use super::*;
use crate::incremental_world_rule_composite_family::{
    WorldRuleCompositeQualificationV1, accept_world_rule_branch_transition_v1,
    qualify_world_rule_branch_v1,
};
use oasis7_formal_protocol::incremental_execution::{
    RootSetV1, TransitionRecordV1, TransitionVerifier,
};
pub type OrganizationContinuityQualificationV1 =
    WorldRuleCompositeQualificationV1<crate::world_rules::OrganizationContinuityStateV1>;
pub type EmergencySupplyQualificationV1 =
    WorldRuleCompositeQualificationV1<crate::world_rules::EmergencySupplyStateV1>;
macro_rules! owner {
    ($qualify:ident,$accept:ident,$ty:ty) => {
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
    qualify_world_rule_continuity_v1,
    accept_world_rule_continuity_transition_v1,
    crate::world_rules::OrganizationContinuityStateV1
);
owner!(
    qualify_world_rule_emergency_v1,
    accept_world_rule_emergency_transition_v1,
    crate::world_rules::EmergencySupplyStateV1
);

//! Internal field access after a version-specific opaque schedule was admitted.
//! This view never constructs or converts authority capabilities.
use oasis7_formal_protocol::bft::{
    BftHeaderHeightScheduleV3, BftHeightProposerScheduleV2, BftSchedulePolicyV2, BftValidatorSetV2,
};
use oasis7_proto::distributed_bft::v2::{BftScopeV2, BftValidatorTransitionRefV2};
pub(super) trait ExecutionSchedule {
    fn scope(&self) -> &BftScopeV2;
    fn height(&self) -> u64;
    fn parent_tick(&self) -> u64;
    fn parent_commit_ref(&self) -> &str;
    fn parent_state_root(&self) -> &str;
    fn policy(&self) -> &BftSchedulePolicyV2;
    fn policy_hash(&self) -> &str;
    fn active_set(&self) -> &BftValidatorSetV2;
    fn pending_transition(&self) -> Option<&BftValidatorTransitionRefV2>;
}
macro_rules! schedule_view {
    ($schedule:ty) => {
        impl ExecutionSchedule for $schedule {
            fn scope(&self) -> &BftScopeV2 {
                self.scope()
            }
            fn height(&self) -> u64 {
                self.height()
            }
            fn parent_tick(&self) -> u64 {
                self.parent_tick()
            }
            fn parent_commit_ref(&self) -> &str {
                self.parent_commit_ref()
            }
            fn parent_state_root(&self) -> &str {
                self.parent_state_root()
            }
            fn policy(&self) -> &BftSchedulePolicyV2 {
                self.policy()
            }
            fn policy_hash(&self) -> &str {
                self.policy_hash()
            }
            fn active_set(&self) -> &BftValidatorSetV2 {
                self.active_set()
            }
            fn pending_transition(&self) -> Option<&BftValidatorTransitionRefV2> {
                self.pending_transition()
            }
        }
    };
}
schedule_view!(BftHeightProposerScheduleV2);
schedule_view!(BftHeaderHeightScheduleV3);

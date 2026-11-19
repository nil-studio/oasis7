//! Fixed activity-close batch. The next registration observes the previous
//! publication, while module routing remains deferred until the batch end.
use super::*;
use crate::world::{
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_module_invocation::*,
    incremental_normal_tail::NormalTailWorkV1,
};
use oasis7_formal_protocol::{incremental_material::OriginalObjectStreamReader,
 incremental_schedule::SchedulePageWriter,incremental_state::StatePageWriter,
 incremental_value::{ValueNodeReader,ValueNodeWriter,ValueTreeError}};
use oasis7_world_state::incremental_field_read::WorldFieldReadContextV1;
impl<'a,'t,'s,'v,M,R,E,C,W,F,O> RootModuleExecutionContextV1<'a,'t,'s,'v,M,R,E,C,W,F,O>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,
 W:ValueNodeReader+ValueNodeWriter+StatePageWriter+SchedulePageWriter,
 F:FnMut(&mut dyn AgentEventOriginalV1)->Result<(),BodyMutationError>,O:OriginalObjectStreamReader {
 #[allow(clippy::too_many_arguments)]
 pub(in crate::world) fn execute_normal_organization_tail_v1<P,CE,CC,PE,PC,ME>(
 &mut self,parent_source:&ReservedValue<BoundNormalTickParentV1<'_>>,sources:&NormalTailSourcesV1<'_,'_>,
 economic:&crate::world::incremental_normal_world_rule_lifecycle::NormalWorldRuleLifecycleSourcesV1<'_>,
 work:&mut NormalOrganizationTailWorkV1,materials:&mut CorePolicyMaterialContextV1<'_,M,ME>,
 current:&mut WorldFieldReadContextV1<'_,P,R,CE,CC>,parent:&mut WorldFieldReadContextV1<'_,P,R,PE,PC>,
 normal:&mut NormalTailWorkV1<'_,'_>,directory:&ReservedValue<AcceptedInvocationDirectoryV1>,
 enumeration:&mut CurrentInvocationEnumerationWorkV1,live_owner:&mut dyn FnMut()->ExecutionFence,
 )->Result<(),RootModuleExecutionErrorV1<O::Error>>
 where P:MerkleNodeReader,CE:FnMut()->Option<ValueTreeError>,CC:FnMut()->bool,
 PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool,ME:FnMut()->Option<ValueTreeError> {
 self.execute_normal_organization_activity_close_v1(parent_source,sources,economic,work,materials,current,parent,normal,directory,enumeration,live_owner)?;
 self.execute_normal_organization_continuity_v1(parent_source,sources,economic,work,materials,current,parent,normal,directory,enumeration,live_owner)
 }
}

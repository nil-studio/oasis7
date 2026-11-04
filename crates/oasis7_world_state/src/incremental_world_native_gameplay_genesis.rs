//! Original native fields delegate only their envelopes to the common framework.
use super::*;
use crate::{
    WorldState,
    incremental_world_native::{
        WorldNativeFieldSpecV1, prepare_world_native_metadata_v1, prepare_world_native_record_v1,
    },
};
use oasis7_formal_protocol::incremental_execution::ExecutionWriteV1;
use oasis7_proto::memory_budget::ReservedValue;
macro_rules! field {
    ($marker:ident,$key:ty,$value:ty,$name:literal,$columns:literal,$shape:ident) => {
        pub struct $marker;
        impl WorldNativeFieldSpecV1 for $marker {
            type Key = $key;
            type Value = $value;
            const NAME: &'static str = $name;
            const COLUMNS: &'static str = $columns;
            const SHAPE: FieldShape = FieldShape::$shape;
        }
    };
}
field!(
    GameplayPolicyFieldV1,
    (),
    crate::GameplayPolicyState,
    "gameplay_policy",
    "gameplay_policy.columns",
    Value
);
field!(
    AgentClaimsFieldV1,
    String,
    crate::AgentClaimState,
    "agent_claims",
    "agent_claims.columns",
    Map
);
field!(
    StarterOcClaimsFieldV1,
    String,
    crate::StarterOcClaimState,
    "starter_oc_claims",
    "starter_oc_claims.columns",
    Map
);
field!(
    MetaProgressFieldV1,
    String,
    crate::MetaProgressState,
    "meta_progress",
    "meta_progress.columns",
    Map
);
field!(
    ModuleAdmissionFreezesFieldV1,
    String,
    crate::state::ModuleAdmissionFreeze,
    "module_admission_freezes",
    "module_admission_freezes.columns",
    Map
);
field!(
    ModuleReleaseRequestsFieldV1,
    u64,
    crate::state::ModuleReleaseRequestState,
    "module_release_requests",
    "module_release_requests.columns",
    Map
);
field!(
    ModuleReleaseManifestMappingsFieldV1,
    u64,
    crate::state::ModuleReleaseManifestMappingState,
    "module_release_manifest_mappings",
    "module_release_manifest_mappings.columns",
    Map
);
/// Source map identities are borrowed literally, independently of inner DTO IDs.
/// The caller owns all work and the provisional sink; no record Vec is staged.
pub fn prepare_world_native_gameplay_genesis_v1<
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &WorldState,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    work: &mut TypedRecordWriteWorkV1,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    prepare_world_native_metadata_v1::<GameplayPolicyFieldV1>(ctx, records, sink)?;
    prepare_world_native_record_v1::<GameplayPolicyFieldV1>(
        &(),
        &source.gameplay_policy,
        ctx,
        records,
        work,
        sink,
    )?;
    macro_rules! map {
        ($marker:ty,$field:ident) => {
            prepare_world_native_metadata_v1::<$marker>(ctx, records, sink)?;
            for (key, value) in &source.$field {
                prepare_world_native_record_v1::<$marker>(key, value, ctx, records, work, sink)?;
            }
        };
    }
    map!(AgentClaimsFieldV1, agent_claims);
    map!(StarterOcClaimsFieldV1, starter_oc_claims);
    map!(MetaProgressFieldV1, meta_progress);
    map!(ModuleAdmissionFreezesFieldV1, module_admission_freezes);
    map!(ModuleReleaseRequestsFieldV1, module_release_requests);
    map!(
        ModuleReleaseManifestMappingsFieldV1,
        module_release_manifest_mappings
    );
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}

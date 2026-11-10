//! Native original governance and monetary record adapters. Every Map keeps
//! its original outer ID; the two optional Value fields retain the Unit key.
use crate::{
    WorldState,
    incremental_typed_record_codec::*,
    incremental_world_native::{
        WorldNativeFieldSpecV1, WorldNativeKeyRefV1, prepare_world_native_metadata_v1,
        prepare_world_native_record_v1,
    },
};
use oasis7_formal_protocol::incremental_execution::ExecutionWriteV1;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("native governance/money record")
}
#[path = "incremental_world_native_governance_binding.rs"]
mod binding;
#[path = "incremental_world_native_governance_bootstrap.rs"]
mod bootstrap;
#[path = "incremental_world_native_governance_bootstrap_binding.rs"]
mod bootstrap_binding;
#[path = "incremental_world_native_governance_dto.rs"]
mod dto;
#[path = "incremental_world_native_governance_enums.rs"]
mod enums;
#[path = "incremental_world_native_governance_rotation.rs"]
mod rotation;
macro_rules! spec {
    ($marker:ident,$name:literal,$columns:literal,$key:ty,$value:ty,$shape:ident) => {
        #[derive(Clone, Copy, Debug, Default)]
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
spec!(
    GovernanceFinalitySignerRegistryFieldV1,
    "governance_finality_signer_registry",
    "governance_finality_signer_registry.columns",
    (),
    Option<crate::governance::GovernanceFinalitySignerRegistry>,
    Value
);
spec!(
    GovernanceValidatorAdmissionsFieldV1,
    "governance_validator_admissions",
    "governance_validator_admissions.columns",
    String,
    crate::governance::GovernanceValidatorAdmissionRecord,
    Map
);
spec!(
    GovernanceMainTokenControllerRegistryFieldV1,
    "governance_main_token_controller_registry",
    "governance_main_token_controller_registry.columns",
    (),
    Option<crate::governance::GovernanceMainTokenControllerRegistry>,
    Value
);
spec!(
    MainTokenNodePointsBridgeRecordsFieldV1,
    "main_token_node_points_bridge_records",
    "main_token_node_points_bridge_records.columns",
    u64,
    crate::main_token::MainTokenNodePointsBridgeEpochRecord,
    Map
);
spec!(
    MainTokenTreasuryDistributionRecordsFieldV1,
    "main_token_treasury_distribution_records",
    "main_token_treasury_distribution_records.columns",
    String,
    crate::main_token::MainTokenTreasuryDistributionRecord,
    Map
);
spec!(
    RestrictedStarterClaimLiveopsPoolTopUpRecordsFieldV1,
    "restricted_starter_claim_liveops_pool_top_up_records",
    "restricted_starter_claim_liveops_pool_top_up_records.columns",
    String,
    crate::main_token::RestrictedStarterClaimLiveopsPoolTopUpRecord,
    Map
);
#[derive(Clone, Copy, Debug, Default)]
pub struct GovernanceFinalityKeyRotationsFieldV1;
impl WorldNativeFieldSpecV1 for GovernanceFinalityKeyRotationsFieldV1 {
    type Key = String;
    type Value = crate::governance::FinalityKeyRotationRecordV1;
    const NAME: &'static str = "governance_finality_key_rotations";
    const COLUMNS: &'static str = "governance_finality_key_rotations.columns";
    const SHAPE: FieldShape = FieldShape::Map;
    const MAX_RECORDS: Option<u64> =
        Some(crate::governance::MAX_FINALITY_KEY_ROTATION_RECORDS_V1 as u64);
    fn validate_source_record(
        key: &Self::Key,
        value: &Self::Value,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<(), ValueTreeError> {
        binding::source(key, value, memory, class)
    }
    fn validate_record_binding<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        key: WorldNativeKeyRefV1<'_>,
        body: &StateRootV1,
        ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
        scan: &mut StateScanBudget,
        work: &mut TypedRecordReadWorkV1,
    ) -> Result<(), ValueTreeError> {
        binding::persisted(key, body, ctx, scan, work)
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct GovernanceBootstrapFinalityKeyRotationsFieldV1;
impl WorldNativeFieldSpecV1 for GovernanceBootstrapFinalityKeyRotationsFieldV1 {
    type Key = String;
    type Value = crate::governance::BootstrapFinalityKeyRotationRecordV1;
    const NAME: &'static str = "governance_bootstrap_finality_key_rotations";
    const COLUMNS: &'static str = "governance_bootstrap_finality_key_rotations.columns";
    const SHAPE: FieldShape = FieldShape::Map;
    const MAX_RECORDS: Option<u64> =
        Some(crate::governance::MAX_FINALITY_KEY_ROTATION_RECORDS_V1 as u64);
    fn validate_source_record(
        key: &Self::Key,
        value: &Self::Value,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<(), ValueTreeError> {
        bootstrap_binding::source(key, value, memory, class)
    }
    fn validate_record_binding<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        key: WorldNativeKeyRefV1<'_>,
        body: &StateRootV1,
        ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
        scan: &mut StateScanBudget,
        work: &mut TypedRecordReadWorkV1,
    ) -> Result<(), ValueTreeError> {
        bootstrap_binding::persisted(key, body, ctx, scan, work)
    }
}
/// This contributes exactly eight original field families, including both None
/// Value records. It grants neither complete World nor History readiness.
pub fn prepare_world_native_governance_genesis_v1<
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &WorldState,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    typed_work: &mut TypedRecordWriteWorkV1,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    crate::governance::validate_finality_key_rotation_history_capacity(
        source.governance_finality_key_rotations.len(),
        source.governance_bootstrap_finality_key_rotations.len(),
    )
    .map_err(|_| invalid())?;
    macro_rules! value {
        ($spec:ty,$field:ident) => {
            prepare_world_native_metadata_v1::<$spec>(ctx, records, sink)?;
            prepare_world_native_record_v1::<$spec>(
                &(),
                &source.$field,
                ctx,
                records,
                typed_work,
                sink,
            )?;
        };
    }
    macro_rules! map {
        ($spec:ty,$field:ident) => {
            prepare_world_native_metadata_v1::<$spec>(ctx, records, sink)?;
            for (key, value) in &source.$field {
                prepare_world_native_record_v1::<$spec>(
                    key, value, ctx, records, typed_work, sink,
                )?;
            }
        };
    }
    value!(
        GovernanceFinalitySignerRegistryFieldV1,
        governance_finality_signer_registry
    );
    map!(
        GovernanceValidatorAdmissionsFieldV1,
        governance_validator_admissions
    );
    map!(
        GovernanceFinalityKeyRotationsFieldV1,
        governance_finality_key_rotations
    );
    map!(
        GovernanceBootstrapFinalityKeyRotationsFieldV1,
        governance_bootstrap_finality_key_rotations
    );
    value!(
        GovernanceMainTokenControllerRegistryFieldV1,
        governance_main_token_controller_registry
    );
    map!(
        MainTokenNodePointsBridgeRecordsFieldV1,
        main_token_node_points_bridge_records
    );
    map!(
        MainTokenTreasuryDistributionRecordsFieldV1,
        main_token_treasury_distribution_records
    );
    map!(
        RestrictedStarterClaimLiveopsPoolTopUpRecordsFieldV1,
        restricted_starter_claim_liveops_pool_top_up_records
    );
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "incremental_world_native_governance_fixture.rs"]
pub(crate) mod fixture;
#[cfg(test)]
#[path = "incremental_world_native_governance_genesis_tests.rs"]
mod genesis_tests;
#[cfg(test)]
#[path = "incremental_world_native_governance_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "incremental_world_native_governance_bootstrap_tests.rs"]
mod bootstrap_tests;

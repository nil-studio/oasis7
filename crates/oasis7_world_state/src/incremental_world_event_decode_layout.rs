use core::mem::size_of;
#[derive(Clone, Copy)]
pub(crate) struct WorldEventOwnedLayoutsV1 {
    pub root: usize,
    pub seq_element: usize,
    pub map_key: usize,
    pub map_value: usize,
    pub map_pair: usize,
    pub set_element: usize,
    pub box_pointee: usize,
    pub content: usize,
    pub content_pair: usize,
    pub flatten_element: usize,
    pub json_value: usize,
}
const fn maximum<const N: usize>(xs: [usize; N]) -> usize {
    let mut out = 0;
    let mut i = 0;
    while i < N {
        if xs[i] > out {
            out = xs[i];
        }
        i += 1;
    }
    out
}
pub(crate) const fn world_event_owned_layouts_v1() -> WorldEventOwnedLayoutsV1 {
    WorldEventOwnedLayoutsV1 {
        root: size_of::<crate::WorldEvent>(),
        seq_element: maximum([
            size_of::<
                Option<(
                    serde::__private229::de::Content<'static>,
                    serde::__private229::de::Content<'static>,
                )>,
            >(),
            size_of::<String>(),
            size_of::<crate::LogisticsOwnerPayout>(),
            size_of::<crate::MainTokenGenesisAllocationBucketState>(),
            size_of::<crate::MainTokenNodePointsBridgeDistribution>(),
            size_of::<crate::ManifestPatchOp>(),
            size_of::<crate::MaterialMarketQuote>(),
            size_of::<crate::ReceiptParticipantSignature>(),
            size_of::<crate::WarParticipantOutcome>(),
            size_of::<oasis7_agent_api::world_action::EmergencyNeedV1>(),
            size_of::<oasis7_agent_api::world_action::EmergencyObservationV1>(),
            size_of::<oasis7_agent_api::world_action::GeoPos>(),
            size_of::<oasis7_agent_api::world_action::IndustrialDispositionV1>(),
            size_of::<oasis7_agent_api::world_action::IndustrialParentV1>(),
            size_of::<oasis7_agent_api::world_action::IndustrialRefV1>(),
            size_of::<oasis7_agent_api::world_action::MainTokenGenesisAllocationPlan>(),
            size_of::<oasis7_agent_api::world_action::MainTokenTreasuryDistribution>(),
            size_of::<oasis7_agent_api::world_action::NodeRewardMintRecord>(),
            size_of::<oasis7_agent_api::world_action::NodeSettlement>(),
            size_of::<oasis7_agent_api::world_action::ObservedAgent>(),
            size_of::<oasis7_agent_api::world_action::SubstitutionCandidateV1>(),
            size_of::<
                oasis7_client_api::world_service::SignedReadRequest<
                    crate::service_admission::participant_policy::ParticipantMigrationBindingV1,
                >,
            >(),
            size_of::<oasis7_wasm_abi::FactoryProfileV1>(),
            size_of::<oasis7_wasm_abi::MaterialStack>(),
            size_of::<oasis7_wasm_abi::ModuleCommandDeclaration>(),
            size_of::<oasis7_wasm_abi::ModuleSubscription>(),
            size_of::<oasis7_wasm_abi::ProductProfileV1>(),
            size_of::<oasis7_wasm_abi::RecipeProfileV1>(),
            size_of::<oasis7_wasm_abi::SubstitutionProfileRefV1>(),
            size_of::<oasis7_wasm_abi::SubstitutionRuleV1>(),
            size_of::<serde::__private229::de::Content<'static>>(),
            size_of::<serde_json::Value>(),
            size_of::<u8>(),
        ]),
        map_key: maximum([
            size_of::<String>(),
            size_of::<oasis7_agent_api::world_action::ResourceKind>(),
        ]),
        map_value: maximum([
            size_of::<String>(),
            size_of::<Vec<u8>>(),
            size_of::<crate::CapabilityEffectReceiptLink>(),
            size_of::<i64>(),
            size_of::<oasis7_agent_api::world_action::IndustrialInputBaselineV1>(),
            size_of::<oasis7_agent_api::world_action::IndustrialInputSlotV1>(),
            size_of::<serde_json::Value>(),
            size_of::<u16>(),
            size_of::<u64>(),
        ]),
        map_pair: maximum([
            size_of::<(String, String)>(),
            size_of::<(String, Vec<u8>)>(),
            size_of::<(String, crate::CapabilityEffectReceiptLink)>(),
            size_of::<(String, i64)>(),
            size_of::<(
                String,
                oasis7_agent_api::world_action::IndustrialInputBaselineV1,
            )>(),
            size_of::<(
                String,
                oasis7_agent_api::world_action::IndustrialInputSlotV1,
            )>(),
            size_of::<(String, serde_json::Value)>(),
            size_of::<(String, u16)>(),
            size_of::<(String, u64)>(),
            size_of::<(oasis7_agent_api::world_action::ResourceKind, i64)>(),
            size_of::<(
                serde::__private229::de::Content<'static>,
                serde::__private229::de::Content<'static>,
            )>(),
        ]),
        set_element: maximum([
            size_of::<String>(),
            size_of::<oasis7_agent_api::world_action::OrganizationEstateScopeV1>(),
        ]),
        box_pointee: maximum([
            size_of::<crate::TaskTerminalRecordV2>(),
            size_of::<oasis7_agent_api::game::GameOwnStopReferenceV2>(),
            size_of::<oasis7_agent_api::world_action::Action>(),
            size_of::<oasis7_agent_api::world_action::IndustrialBaselineAuthorityV1>(),
            size_of::<oasis7_agent_api::world_action::IndustrialWorkCommandV1>(),
            size_of::<oasis7_agent_api::world_action::QualifiedIndustrialLotFactsV1>(),
            size_of::<oasis7_agent_api::world_action::RecipeSubstitutionCommandV1>(),
            size_of::<oasis7_agent_api::world_action::RootOperationV1>(),
            size_of::<oasis7_agent_api::world_action::SubstitutionExecutionAuthorityV1>(),
            size_of::<serde::__private229::de::Content<'static>>(),
        ]),
        content: size_of::<serde::__private229::de::Content<'static>>(),
        content_pair: size_of::<(
            serde::__private229::de::Content<'static>,
            serde::__private229::de::Content<'static>,
        )>(),
        flatten_element: size_of::<
            Option<(
                serde::__private229::de::Content<'static>,
                serde::__private229::de::Content<'static>,
            )>,
        >(),
        json_value: size_of::<serde_json::Value>(),
    }
}

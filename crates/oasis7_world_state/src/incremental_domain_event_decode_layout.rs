//! Closed bare-DomainEvent reachable owned layouts; not a WorldEvent graph.
//! serde 1.0.229 is pinned in Cargo.lock; __private229 deliberately fails to
//! resolve on upgrade so Content's changed representation must be re-audited.
use core::mem::size_of;
#[derive(Clone, Copy)]
pub(crate) struct DomainEventOwnedLayoutsV1 {
    pub root: usize,
    pub seq_element: usize,
    pub map_key: usize,
    pub map_value: usize,
    pub map_pair: usize,
    pub set_element: usize,
    pub box_pointee: usize,
    pub content: usize,
    pub content_pair: usize,
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
pub(crate) const fn domain_event_owned_layouts_v1() -> DomainEventOwnedLayoutsV1 {
    DomainEventOwnedLayoutsV1 {
        root: size_of::<crate::DomainEvent>(),
        seq_element: maximum([
            size_of::<String>(),
            size_of::<crate::events::LogisticsOwnerPayout>(),
            size_of::<crate::events::MaterialMarketQuote>(),
            size_of::<crate::gameplay_state::WarParticipantOutcome>(),
            size_of::<crate::main_token::MainTokenGenesisAllocationBucketState>(),
            size_of::<crate::main_token::MainTokenNodePointsBridgeDistribution>(),
            size_of::<oasis7_agent_api::world_action::EmergencyNeedV1>(),
            size_of::<oasis7_agent_api::world_action::EmergencyObservationV1>(),
            size_of::<oasis7_agent_api::world_action::IndustrialDispositionV1>(),
            size_of::<oasis7_agent_api::world_action::IndustrialParentV1>(),
            size_of::<oasis7_agent_api::world_action::IndustrialRefV1>(),
            size_of::<oasis7_agent_api::world_action::MainTokenTreasuryDistribution>(),
            size_of::<oasis7_agent_api::world_action::NodeRewardMintRecord>(),
            size_of::<oasis7_agent_api::world_action::NodeSettlement>(),
            size_of::<oasis7_agent_api::world_action::ObservedAgent>(),
            size_of::<oasis7_agent_api::world_action::SubstitutionCandidateV1>(),
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
            size_of::<i64>(),
            size_of::<oasis7_agent_api::world_action::IndustrialInputBaselineV1>(),
            size_of::<oasis7_agent_api::world_action::IndustrialInputSlotV1>(),
            size_of::<serde_json::Value>(),
            size_of::<u16>(),
            size_of::<u64>(),
        ]),
        map_pair: maximum([
            size_of::<(String, String)>(),
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
        json_value: size_of::<serde_json::Value>(),
    }
}

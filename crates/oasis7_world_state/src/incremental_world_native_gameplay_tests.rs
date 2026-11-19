use super::*;
use crate::incremental_typed_record_test_support::{roundtrip, source_admission};
use crate::incremental_world_native::WorldNativeFieldSpecV1;
fn profile<F: WorldNativeFieldSpecV1>() -> TypedRecordProfileV1 {
    F::profile().unwrap()
}

#[test]
fn native_gameplay_claim_meta_and_freeze_preserve_original_nonempty_dtos() {
    let _source = source_admission();
    let mut world = crate::WorldState::default();
    fixture::populate_world(&mut world);
    roundtrip(&world.gameplay_policy, profile::<GameplayPolicyFieldV1>());
    roundtrip(
        &world.agent_claims["outer-claim"],
        profile::<AgentClaimsFieldV1>(),
    );
    roundtrip(
        &world.starter_oc_claims["outer-starter"],
        profile::<StarterOcClaimsFieldV1>(),
    );
    roundtrip(
        &world.meta_progress["outer-meta"],
        profile::<MetaProgressFieldV1>(),
    );
    roundtrip(
        &world.module_admission_freezes["outer-freeze"],
        profile::<ModuleAdmissionFreezesFieldV1>(),
    );
}
#[test]
fn native_release_request_preserves_manifest_and_original_nested_maps() {
    let _source = source_admission();
    roundtrip(
        &fixture::release(),
        profile::<ModuleReleaseRequestsFieldV1>(),
    );
}
#[test]
fn native_release_mapping_preserves_all_original_status_variants() {
    use crate::state::ModuleReleaseRequestStatus as S;
    let _source = source_admission();
    for status in [
        S::Requested,
        S::Shadowed,
        S::PartiallyApproved,
        S::Approved,
        S::Rejected,
        S::Applied,
    ] {
        let mut mapping = fixture::mapping();
        mapping.status = status;
        roundtrip(&mapping, profile::<ModuleReleaseManifestMappingsFieldV1>());
    }
}
#[test]
fn native_module_enum_codecs_preserve_all_original_variants() {
    use oasis7_agent_api::world_action::ModuleInstallTarget;
    use oasis7_wasm_abi::*;
    let _source = source_admission();
    let profile = profile::<ModuleReleaseRequestsFieldV1>();
    for value in [ModuleKind::Reducer, ModuleKind::Pure] {
        roundtrip(&value, profile);
    }
    for value in [
        ModuleRole::Rule,
        ModuleRole::Domain,
        ModuleRole::Gameplay,
        ModuleRole::Body,
        ModuleRole::AgentInternal,
    ] {
        roundtrip(&value, profile);
    }
    for value in [
        ModuleSubscriptionStage::PreAction,
        ModuleSubscriptionStage::PostAction,
        ModuleSubscriptionStage::PostEvent,
        ModuleSubscriptionStage::Tick,
    ] {
        roundtrip(&value, profile);
    }
    for value in [
        GameplayModuleKind::War,
        GameplayModuleKind::Governance,
        GameplayModuleKind::Crisis,
        GameplayModuleKind::Economic,
        GameplayModuleKind::Meta,
    ] {
        roundtrip(&value, profile);
    }
    for value in [
        ModuleInstallTarget::SelfAgent,
        ModuleInstallTarget::LocationInfrastructure {
            location_id: "".into(),
        },
    ] {
        roundtrip(&value, profile);
    }
}

#[path = "incremental_world_native_gameplay_bits_tests.rs"]
mod bits;

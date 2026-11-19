//! Bounded literal original DTOs, including independent outer/inner identities.
use crate::{WorldState, gameplay_state::*, state::*};
use oasis7_agent_api::world_action::{ModuleInstallTarget, ModuleProfileChanges};
use oasis7_wasm_abi::*;
use std::collections::BTreeMap;
pub(crate) fn manifest() -> ModuleManifest {
    ModuleManifest {
        module_id: "inner-module".into(),
        name: "名字".into(),
        version: "".into(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::AgentInternal,
        wasm_hash: "raw-hash".into(),
        interface_version: "v".into(),
        exports: vec!["repeat".into(), "repeat".into()],
        subscriptions: vec![
            ModuleSubscription {
                event_kinds: vec!["event".into(), "event".into()],
                action_kinds: vec![],
                stage: None,
                filters: Some(serde_json::Value::Null),
            },
            ModuleSubscription {
                event_kinds: vec![],
                action_kinds: vec!["action".into()],
                stage: Some(ModuleSubscriptionStage::Tick),
                filters: Some(
                    serde_json::json!({"negative_zero":-0.0,"unsigned":u64::MAX,"ordered":[null,-7,"é"]}),
                ),
            },
        ],
        required_caps: vec!["cap".into(), "cap".into()],
        abi_contract: ModuleAbiContract {
            abi_version: Some(0),
            input_schema: Some(String::new()),
            output_schema: None,
            cap_slots: BTreeMap::from([("".into(), "raw".into()), ("slot".into(), "cap".into())]),
            policy_hooks: vec!["hook".into(), "hook".into()],
            gameplay: Some(GameplayContract {
                kind: GameplayModuleKind::Meta,
                game_modes: vec!["mode".into(), "mode".into()],
                min_players: 0,
                max_players: Some(u16::MAX),
            }),
            declarations: ModuleSchemaDeclarations {
                commands: vec![ModuleCommandDeclaration {
                    namespace: "command".into(),
                    name: "run".into(),
                    schema_version: u32::MAX,
                    schema_hash: "raw".into(),
                    max_payload_bytes: u64::MAX,
                }],
            },
        },
        artifact_identity: Some(ModuleArtifactIdentity {
            source_hash: "source".into(),
            build_manifest_hash: "build".into(),
            signer_node_id: "signer".into(),
            signature_scheme: "scheme".into(),
            artifact_signature: "signature".into(),
        }),
        limits: ModuleLimits {
            max_mem_bytes: u64::MAX,
            max_gas: 0,
            max_call_rate: u32::MAX,
            max_output_bytes: 1,
            max_effects: 0,
            max_emits: u32::MAX,
        },
    }
}
pub(crate) fn release() -> ModuleReleaseRequestState {
    ModuleReleaseRequestState {
        request_id: 77,
        requester_agent_id: "inner-requester".into(),
        manifest: manifest(),
        activate: true,
        install_target: ModuleInstallTarget::LocationInfrastructure {
            location_id: "位置".into(),
        },
        profile_changes: ModuleProfileChanges {
            product_profiles: vec![
                crate::incremental_world_native_industry::test_profiles::product_profile(),
            ],
            recipe_profiles: vec![
                crate::incremental_world_native_industry::test_profiles::recipe_profile(),
            ],
            factory_profiles: vec![
                crate::incremental_world_native_industry::test_profiles::factory_profile(),
            ],
        },
        required_roles: vec!["role".into(), "role".into()],
        role_approvals: BTreeMap::from([
            ("outer-role".into(), "inner-agent".into()),
            ("".into(), "".into()),
        ]),
        attestations: BTreeMap::from([(
            "outer-attestation".into(),
            ModuleReleaseAttestationState {
                request_id: u64::MAX,
                signer_node_id: "inner-signer".into(),
                platform: "a".into(),
                submitted_by_agent_id: "actor".into(),
                build_manifest_hash: "b".into(),
                source_hash: "s".into(),
                wasm_hash: "w".into(),
                proof_cid: "p".into(),
                builder_image_digest: "image".into(),
                container_platform: "container".into(),
                canonicalizer_version: "canonical".into(),
                submitted_at: 0,
            },
        )]),
        status: ModuleReleaseRequestStatus::Approved,
        shadow_manifest_hash: Some(String::new()),
        applied_manifest_hash: None,
        applied_proposal_id: Some(u64::MAX),
        rejected_reason: Some("literal historical note".into()),
        created_at: 0,
        updated_at: u64::MAX,
    }
}
pub(crate) fn mapping() -> ModuleReleaseManifestMappingState {
    ModuleReleaseManifestMappingState {
        request_id: 88,
        release_id: "inner-release".into(),
        module_id: "inner-module".into(),
        attestation_count: u32::MAX,
        release_wasm_hash: Some(String::new()),
        release_source_hash: None,
        release_build_manifest_hash: Some("build".into()),
        release_builder_image_digest: None,
        release_container_platform: Some("platform".into()),
        release_canonicalizer_version: None,
        attestation_platforms: vec!["repeat".into(), "repeat".into()],
        attestation_proof_cids: vec!["second".into(), "first".into()],
        receipt_evidence_conflict: true,
        shadow_manifest_hash: None,
        applied_manifest_hash: Some("applied".into()),
        applied_proposal_id: Some(0),
        status: ModuleReleaseRequestStatus::Applied,
        created_at: u64::MAX,
        updated_at: 0,
    }
}
pub(crate) fn populate_world(world: &mut WorldState) {
    world.gameplay_policy = GameplayPolicyState {
        electricity_tax_bps: u16::MAX,
        data_tax_bps: 0,
        power_trade_fee_bps: 10001,
        max_open_contracts_per_agent: 0,
        blocked_agents: vec!["重复".into(), "重复".into()],
        forbidden_location_ids: vec!["".into(), "site".into()],
        updated_at: u64::MAX,
    };
    world.agent_claims.insert(
        "outer-claim".into(),
        AgentClaimState {
            control_source: Some(AgentClaimControlSourceV1 {
                acquisition_sequence: u64::MAX,
                root_agent_id: "root".into(),
                player_id: "player".into(),
                root_claimed_at: 0,
                acquired_event_digest: "digest".into(),
            }),
            target_agent_id: "inner-target".into(),
            claim_owner_id: "inner-owner".into(),
            reputation_tier: u8::MAX,
            slot_index: 0,
            activation_fee_amount: 1,
            activation_fee_burn_amount: 2,
            activation_fee_treasury_amount: 3,
            claim_bond_amount: 4,
            locked_bond_amount: 5,
            upfront_restricted_spent_amount: 6,
            upfront_liquid_spent_amount: 7,
            claim_bond_locked_restricted_amount: 8,
            claim_bond_locked_liquid_amount: 9,
            claim_bond_restricted_source_treasury_bucket_id: Some(String::new()),
            upkeep_per_epoch: u64::MAX,
            release_cooldown_epochs: 11,
            grace_epochs: 12,
            idle_warning_epochs: 13,
            forced_idle_reclaim_epochs: 14,
            forced_reclaim_penalty_bps: u16::MAX,
            claimed_at_epoch: 15,
            upkeep_paid_through_epoch: 16,
            delinquent_since_epoch: Some(0),
            grace_deadline_epoch: None,
            release_requested_at_epoch: Some(u64::MAX),
            release_ready_at_epoch: None,
            idle_warning_emitted_at_epoch: Some(17),
        },
    );
    world.starter_oc_claims.insert(
        "outer-starter".into(),
        StarterOcClaimState {
            agent_id: "inner-agent".into(),
            player_id: "player".into(),
            public_key: Some(String::new()),
            amount: u64::MAX,
            claimed_at: 0,
            source_treasury_bucket_id: None,
        },
    );
    world.meta_progress.insert(
        "outer-meta".into(),
        MetaProgressState {
            agent_id: "inner-meta".into(),
            track_points: BTreeMap::from([("".into(), i64::MIN), ("track".into(), i64::MAX)]),
            total_points: i64::MIN,
            achievements: vec!["repeat".into(), "repeat".into(), "".into()],
            unlocked_tiers: BTreeMap::from([
                ("empty".into(), vec![]),
                ("track".into(), vec!["repeat".into(), "repeat".into()]),
            ]),
            last_granted_at: u64::MAX,
        },
    );
    world.module_admission_freezes.insert(
        "outer-freeze".into(),
        ModuleAdmissionFreeze {
            module_id: "inner-module".into(),
            module_version: "".into(),
            wasm_hash: "raw".into(),
            rollback_proposal_id: u64::MAX,
            source_release_request_id: Some(0),
            reason: "literal reason".into(),
        },
    );
    world.module_release_requests.insert(1001, release());
    world
        .module_release_manifest_mappings
        .insert(1002, mapping());
}

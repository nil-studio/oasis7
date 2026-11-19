//! Nonempty originals: outer map IDs intentionally differ where the original
//! contract permits it; rotation retains its mandatory exact ID relation.
use std::collections::{BTreeMap, BTreeSet};
pub(crate) fn signer() -> crate::governance::GovernanceFinalitySignerRegistry {
    crate::governance::GovernanceFinalitySignerRegistry {
        slot_id: "slot".into(),
        threshold: 2,
        threshold_bps: 6700,
        signer_bindings: BTreeMap::from([
            ("node-a".into(), "a".repeat(64)),
            ("node-b".into(), "b".repeat(64)),
        ]),
        validator_stakes: BTreeMap::from([("node-a".into(), 100), ("node-b".into(), 200)]),
    }
}
pub(crate) fn controller() -> crate::governance::GovernanceMainTokenControllerRegistry {
    crate::governance::GovernanceMainTokenControllerRegistry {
        genesis_controller_account_id: "controller".into(),
        treasury_bucket_controller_slots: BTreeMap::from([("bucket".into(), "slot".into())]),
        restricted_starter_claim_admin_account_ids: BTreeSet::from(["admin".into()]),
        controller_signer_policies: BTreeMap::from([(
            "slot".into(),
            crate::governance::GovernanceThresholdSignerPolicy {
                threshold: 2,
                allowed_public_keys: BTreeSet::from(["a".repeat(64), "b".repeat(64)]),
            },
        )]),
    }
}
pub(crate) fn admission() -> crate::governance::GovernanceValidatorAdmissionRecord {
    crate::governance::GovernanceValidatorAdmissionRecord {
        candidate_id: "inner-candidate".into(),
        node_id: "node-a".into(),
        finality_signer_public_key: "a".repeat(64),
        stake: 100,
        operator_owner: "operator".into(),
        public_manifest_hash: "1".repeat(64),
        requested_at_epoch: 3,
        last_transition_tick: 4,
        approved_at_epoch: Some(5),
        activation_epoch: Some(6),
        status: crate::governance::GovernanceValidatorAdmissionStatus::Revoked,
        revoked_at_epoch: Some(7),
        revocation_reason: Some(String::new()),
    }
}
pub(crate) fn bridge() -> crate::main_token::MainTokenNodePointsBridgeEpochRecord {
    crate::main_token::MainTokenNodePointsBridgeEpochRecord {
        epoch_index: 17,
        settlement_hash: "settlement".into(),
        total_amount: 300,
        distributions: vec![
            crate::main_token::MainTokenNodePointsBridgeDistribution {
                node_id: "node-a".into(),
                account_id: "account-a".into(),
                amount: 100,
            },
            crate::main_token::MainTokenNodePointsBridgeDistribution {
                node_id: "node-b".into(),
                account_id: "account-b".into(),
                amount: 200,
            },
        ],
    }
}
pub(crate) fn treasury() -> crate::main_token::MainTokenTreasuryDistributionRecord {
    crate::main_token::MainTokenTreasuryDistributionRecord {
        proposal_id: 44,
        distribution_id: "inner-distribution".into(),
        bucket_id: "bucket".into(),
        total_amount: 300,
        distributions: vec![
            oasis7_agent_api::world_action::MainTokenTreasuryDistribution {
                account_id: "account-a".into(),
                amount: 100,
            },
            oasis7_agent_api::world_action::MainTokenTreasuryDistribution {
                account_id: "account-b".into(),
                amount: 200,
            },
        ],
        distributed_epoch: 18,
    }
}
pub(crate) fn top_up() -> crate::main_token::RestrictedStarterClaimLiveopsPoolTopUpRecord {
    crate::main_token::RestrictedStarterClaimLiveopsPoolTopUpRecord {
        controller_account_id: "controller".into(),
        top_up_id: "inner-top-up".into(),
        source_treasury_bucket_id: "source".into(),
        target_treasury_bucket_id: "target".into(),
        amount: 7,
        topped_up_at_epoch: 8,
    }
}
pub(crate) fn rotation() -> crate::governance::FinalityKeyRotationRecordV1 {
    use oasis7_agent_api::world_action::*;
    let mut authorization = GovernanceAuthorizationV1 {
        schema_version: 1,
        world_id: "world".into(),
        chain_id: "chain".into(),
        genesis_digest: "a".repeat(64),
        parent_position: 1,
        parent_record_id: "b".repeat(64),
        parent_state_root: "c".repeat(64),
        policy_digest: "d".repeat(64),
        controller_account_id: "controller".into(),
        nonce: 1,
        valid_from_position: 2,
        valid_through_position: 3,
        signatures: BTreeMap::new(),
    };
    let body = FinalityKeyRotationBodyV1 {
        schema_version: 1,
        authorization: authorization.clone(),
        rotation_id: "rotation".into(),
        candidate_id: "candidate".into(),
        node_id: "node".into(),
        expected_generation: 0,
        old_public_key_hex: "1".repeat(64),
        new_public_key_hex: "2".repeat(64),
        decision_height: 2,
        activation_height: 4,
        target_epoch: 1,
    };
    authorization
        .signatures
        .insert("a".repeat(64), "b".repeat(128));
    let record = crate::governance::FinalityKeyRotationRecordV1 {
        authorized_action: GovernanceAuthorizedActionV1 {
            authorization,
            action: Box::new(Action::RotateGovernanceValidatorFinalityKeyV1 {
                controller_account_id: "controller".into(),
                body,
                old_key_proof: FinalityKeyRotationProofV1 {
                    schema_version: 1,
                    role: FinalityKeyRotationRoleV1::OldKeyAuthorization,
                    signature_hex: "a".repeat(128),
                },
                new_key_proof: FinalityKeyRotationProofV1 {
                    schema_version: 1,
                    role: FinalityKeyRotationRoleV1::NewKeyPossession,
                    signature_hex: "b".repeat(128),
                },
            }),
        },
        applied_at_tick: 2,
    };
    record.validate().unwrap();
    record
}
pub(crate) fn populate_world(world: &mut crate::WorldState) {
    world.governance_finality_signer_registry = Some(signer());
    world.governance_main_token_controller_registry = Some(controller());
    world
        .governance_validator_admissions
        .insert("outer-candidate".into(), admission());
    world
        .governance_finality_key_rotations
        .insert("rotation".into(), rotation());
    world
        .main_token_node_points_bridge_records
        .insert(101, bridge());
    world
        .main_token_treasury_distribution_records
        .insert("outer-distribution".into(), treasury());
    world
        .restricted_starter_claim_liveops_pool_top_up_records
        .insert("outer-top-up".into(), top_up());
}

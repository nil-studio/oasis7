use crate::{
    CapabilityRevocationState, TaskTerminalIndexV2, TaskTerminalRecordV2,
    capability_authorization::*,
    service_admission::{
        AgentSignerDelegationChangeV1, SignedReadRequest, participant::*, participant_policy::*,
    },
};
use oasis7_agent_api::game::*;
use std::collections::{BTreeMap, BTreeSet};
fn ns() -> GameIdentityNamespaceV1 {
    GameIdentityNamespaceV1 {
        service_id: "service".into(),
        environment_id: "dev".into(),
        instance_epoch: DecimalU64(u64::MAX),
    }
}
fn registration() -> GameParticipantRegisterV1 {
    GameParticipantRegisterV1 {
        api_rev: 1,
        client_operation_id: "register".into(),
        identity_namespace: ns(),
        account_id: "account".into(),
        world_id: "world".into(),
        genesis_digest: "genesis".into(),
        owner_public_key: "ownerkey".into(),
        owner_key_version: DecimalU64(1),
    }
}
fn receipt() -> GameParticipantReceiptV1 {
    GameParticipantReceiptV1 {
        api_rev: 1,
        identity_namespace: ns(),
        account_id: "account".into(),
        world_id: "world".into(),
        genesis_digest: "genesis".into(),
        agent_id: "agent".into(),
        owner_public_key: "key".into(),
        owner_binding: "binding".into(),
        owner_generation: DecimalU64(1),
        client_operation_id: "register".into(),
        business_digest: "digest".into(),
        registered_tick: DecimalU64(11),
        imported: true,
    }
}
fn certificate() -> crate::governance::GovernanceFinalityCertificate {
    crate::governance::GovernanceFinalityCertificate {
        proposal_id: 17,
        manifest_hash: "manifest".into(),
        consensus_height: 4,
        epoch_id: 5,
        validator_set_hash: "validators".into(),
        stake_root: "stake".into(),
        threshold_bps: u16::MAX,
        min_unique_signers: 2,
        threshold: 3,
        signatures: BTreeMap::from([("signer".into(), "signature".into())]),
    }
}
fn fence() -> GameTaskFenceV1 {
    GameTaskFenceV1 {
        world_id: "world".into(),
        genesis_digest: "genesis".into(),
        branch_id: "branch".into(),
        reorg_epoch: DecimalU64(0),
        agent_id: "agent".into(),
        owner_binding: "owner".into(),
        owner_generation: DecimalU64(1),
        owner_public_key: "public".into(),
        task_id: "task".into(),
        task_revision: DecimalU64(3),
        task_status: GameTaskStatusV1::Active,
        latest_goal_revision: DecimalU64(2),
        active_goal_revision: DecimalU64(1),
        active_goal_reference: Some(GameGoalReferenceV1 {
            goal_revision: DecimalU64(1),
            goal_ref: "goal".into(),
            prepare_operation_id: "prepare".into(),
            publish_operation_id: "publish".into(),
            activation_committed_height: DecimalU64(3),
        }),
        executor_epoch: DecimalU64(4),
        executor_binding: Some(GameExecutorBindingV1 {
            executor_id: "executor".into(),
            executor_public_key: "key".into(),
            delegation_generation: DecimalU64(1),
            executor_epoch: DecimalU64(4),
        }),
    }
}
fn signed<T>(request: T) -> SignedReadRequest<T> {
    SignedReadRequest {
        request,
        subject_public_key: "subject".into(),
        signature_hex: "exact signed bytes".into(),
    }
}
pub(super) fn value() -> CapabilityRevocationState {
    let policy = ParticipantEntryPolicyV1 {
        api_rev: 1,
        world_id: "world".into(),
        genesis_digest: "genesis".into(),
        identity_namespace: ns(),
        issuer_public_key: "issuer".into(),
        generation: 1,
        max_grant_ticks: u64::MAX,
        max_participants: 200,
        registry_complete: true,
        enabled: false,
    };
    let migration = ParticipantMigrationBindingV1 {
        registration: registration(),
        agent_id: "agent".into(),
        owner_binding: "binding".into(),
        owner_generation: 2,
    };
    let manifest = ParticipantMigrationManifestV1 {
        api_rev: 1,
        policy_digest: "policy".into(),
        world_id: "world".into(),
        genesis_digest: "genesis".into(),
        branch_id: "branch".into(),
        reorg_epoch: 0,
        identity_namespace: ns(),
        source_snapshot_epoch: 10,
        source_watermark: "watermark".into(),
        source_snapshot_digest: "snapshot".into(),
        complete: true,
        previous_registry_digest: "previous".into(),
        legacy_owner_state_digest: "legacy".into(),
        entries: vec![signed(migration.clone())],
    };
    let record = CapabilityAuthorityRecord {
        issuer_id: "inner issuer".into(),
        issuer_kind: "governance".into(),
        key_id: "key".into(),
        public_key_hex: "public".into(),
        issuer_key_epoch: 1,
        governance_epoch: 2,
        finalized_receipt_id: "receipt".into(),
        authority_rotation_receipt_id: Some(String::new()),
        world_id: "world".into(),
        branch_id: "branch".into(),
        finality_epoch: 4,
        finality_block_hash: "block".into(),
        finality_status: "finalized".into(),
        revocation_epoch: 5,
        revoked_grant_ids: BTreeSet::from(["revoked".into()]),
        superseded_by: BTreeMap::from([("old".into(), "new".into())]),
    };
    let binding = CapabilityAuthorityFinalityBinding {
        authority_record_hash: "hash".into(),
        issuer_id: "issuer".into(),
        issuer_kind: "governance".into(),
        key_id: "key".into(),
        issuer_public_key_hex: "public".into(),
        issuer_key_epoch: 1,
        governance_epoch: 2,
        finalized_receipt_id: "receipt".into(),
        authority_rotation_receipt_id: None,
        world_id: "world".into(),
        branch_id: "branch".into(),
        finality_epoch: 3,
        finality_block_hash: "block".into(),
        finality_status: "finalized".into(),
    };
    let before = fence();
    let close = GameTaskRevokeAndCloseV2 {
        api_revision: 2,
        client_operation_id: "close".into(),
        custody_account_id: "custody".into(),
        termination_plan_digest: "plan".into(),
        permitted_branches: GameTaskCloseConditionsV2 {
            exact_initial_active: before.clone(),
            own_stop_intent: Some(GameOwnStopIntentV2 {
                operation_id: "stop".into(),
                intent_digest: "intent".into(),
            }),
        },
        signer_public_key: "signer".into(),
        signer_generation: DecimalU64(1),
        signer_nonce: DecimalU64(2),
        revoke_generation: DecimalU64(3),
        revoke_nonce: DecimalU64(u64::MAX),
        cleanup_until_ms: DecimalU64(1000),
    };
    let closed = GameTaskClosedV2 {
        schema_revision: 2,
        close_operation_id: "close".into(),
        close_original_digest: "original".into(),
        termination_plan_digest: "plan".into(),
        initial_fence: before.clone(),
        actual_before_fence: before.clone(),
        selected_predicate: GameTaskClosePredicateV2::OwnStopSuspended(Box::new(
            GameOwnStopReferenceV2 {
                operation_id: "stop".into(),
                original_digest: "stop original".into(),
                canonical_result_digest: "stop result".into(),
                post_fence: before,
            },
        )),
        revoked_signer_public_key: "signer".into(),
        revoked_generation: DecimalU64(3),
        permanent_nonce_high_water: DecimalU64(u64::MAX),
    };
    let mut value = CapabilityRevocationState {
        participants: ParticipantRegistryV1 {
            policy: Some(policy.clone()),
            policy_history: vec![ParticipantPolicyAuthorizationV1 {
                policy: policy.clone(),
                proof: ParticipantPolicyProofV1 {
                    manifest: signed(manifest.clone()),
                    certificate: certificate(),
                    signatures: BTreeMap::from([(
                        "policy signer".into(),
                        "policy signature".into(),
                    )]),
                },
            }],
            migrated_bindings: BTreeMap::from([("outer migration".into(), migration)]),
            devlocal_policy_history: vec![signed(DevLocalParticipantPolicyAuthorizationV1 {
                policy,
                manifest: signed(manifest),
            })],
            bindings: BTreeMap::from([("outer binding".into(), receipt())]),
            operations: BTreeMap::from([(
                "outer op".into(),
                ParticipantOperationV1 {
                    request: registration(),
                    receipt: receipt(),
                },
            )]),
        },
        epoch: u64::MAX,
        revoked_grant_ids: BTreeSet::from([String::new(), "_length".into()]),
        superseded_by: BTreeMap::from([("_length".into(), "replacement".into())]),
        authority_records: BTreeMap::from([("outer issuer".into(), record)]),
        authority_finality_proofs: BTreeMap::from([(
            "outer proof".into(),
            CapabilityAuthorityFinalityProof {
                proof_version: 1,
                certificate: certificate(),
                binding,
                signatures: BTreeMap::from([(
                    "authority signer".into(),
                    "authority signature".into(),
                )]),
            },
        )]),
        finalized_receipt_id: Some(String::new()),
        agent_identities: BTreeMap::from([(
            "agent".into(),
            CapabilityAgentIdentity {
                owner_binding: "owner".into(),
                generation: 7,
            },
        )]),
        manufactured_identity_acquisition_sequences: BTreeMap::from([(
            "manufactured".into(),
            u64::MAX,
        )]),
        agent_signer_delegations: BTreeMap::from([(
            "outer delegation".into(),
            AgentSignerDelegationChangeV1 {
                world_id: "world".into(),
                branch_id: "branch".into(),
                agent_id: "agent".into(),
                owner_binding: "owner".into(),
                agent_identity_generation: 1,
                generation: 2,
                delegate_public_key: "delegate".into(),
                revoked: true,
                nonce: 4,
            },
        )]),
        world_service_results: BTreeMap::from([(
            "result".into(),
            serde_json::json!({"original":[null,-3,u64::MAX,true,""]}),
        )]),
        task_terminals_v2: Some(TaskTerminalIndexV2 {
            schema_revision: 2,
            records: BTreeMap::from([(
                "task".into(),
                TaskTerminalRecordV2 {
                    schema_revision: 2,
                    original: signed(close),
                    committed_height: 11,
                    committed_at_unix_ms: 22,
                    stop_predecessor_canonical_cbor: Some(vec![0, 255, 1, 0]),
                    closed,
                },
            )]),
        }),
        safe_alternatives: BTreeMap::from([(
            "safe".into(),
            serde_json::json!({"unaltered":{"nested":"proof"},"negative_zero":-0.0}),
        )]),
        system_identities: BTreeMap::from([("system".into(), 99)]),
    };
    let terminals = value.task_terminals_v2.as_mut().unwrap();
    let mut initial = terminals.records["task"].clone();
    initial.stop_predecessor_canonical_cbor = None;
    initial.closed.selected_predicate = GameTaskClosePredicateV2::InitialActive;
    terminals
        .records
        .insert("initial-active".into(), initial.clone());
    initial.stop_predecessor_canonical_cbor = Some(Vec::new());
    terminals
        .records
        .insert("empty-predecessor".into(), initial);
    value
}

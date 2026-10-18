use super::super::*;
use super::pos;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const LOCAL_FINALITY_SIGNER_1: (&str, &str) = (
    "governance.local.finality.signer.1",
    "oasis7-governance-local-finality-signer-1-v1",
);
const LOCAL_FINALITY_SIGNER_2: (&str, &str) = (
    "governance.local.finality.signer.2",
    "oasis7-governance-local-finality-signer-2-v1",
);
const ROTATED_FINALITY_SIGNER_3: (&str, &str) = (
    "governance.test.finality.signer.3",
    "oasis7-governance-test-finality-signer-3-v1",
);

#[path = "governance_finality_tests.rs"]
mod finality_tests;

fn local_guardians() -> Vec<String> {
    vec![
        LOCAL_FINALITY_SIGNER_1.0.to_string(),
        LOCAL_FINALITY_SIGNER_2.0.to_string(),
    ]
}

fn manifest_path(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_string()).collect()
}

fn finality_signing_key(seed_label: &str) -> SigningKey {
    let seed = util::sha256_hex(seed_label.as_bytes());
    let seed_bytes = hex::decode(seed).expect("decode governance finality seed");
    let private_key_bytes: [u8; 32] = seed_bytes
        .as_slice()
        .try_into()
        .expect("governance finality seed is 32 bytes");
    SigningKey::from_bytes(&private_key_bytes)
}

fn bind_finality_signer_with_seed(world: &mut World, node_id: &str, seed_label: &str) {
    let signing_key = finality_signing_key(seed_label);
    let public_key_hex = hex::encode(signing_key.verifying_key().to_bytes());
    world
        .bind_node_identity(node_id, public_key_hex.as_str())
        .expect("bind governance finality signer identity");
}

fn build_finality_certificate_with_signers(
    world: &World,
    proposal_id: ProposalId,
    signer_specs: &[(&str, &str)],
) -> GovernanceFinalityCertificate {
    let mut certificate = world
        .build_local_finality_certificate(proposal_id)
        .expect("build local finality certificate");
    let mut signatures = BTreeMap::new();
    let min_unique_signers = certificate.effective_min_unique_signers();
    for (node_id, seed_label) in signer_specs {
        let payload = GovernanceFinalityCertificate::signing_payload_v1(
            proposal_id,
            certificate.manifest_hash.as_str(),
            certificate.consensus_height,
            certificate.epoch_id,
            certificate.validator_set_hash.as_str(),
            certificate.stake_root.as_str(),
            certificate.threshold_bps,
            min_unique_signers,
            node_id,
        );
        let signing_key = finality_signing_key(seed_label);
        let signature = signing_key.sign(payload.as_slice());
        signatures.insert(
            (*node_id).to_string(),
            format!(
                "{}{}",
                GovernanceFinalityCertificate::SIGNATURE_PREFIX_ED25519_V1,
                hex::encode(signature.to_bytes())
            ),
        );
    }
    certificate.signatures = signatures;
    certificate
}

#[test]
fn governance_event_validator_admission_submitted_defaults_legacy_stake() {
    let value = json!({
        "type": "ValidatorAdmissionSubmitted",
        "data": {
            "controller_account_id": "msig.genesis.v1",
            "candidate_id": "candidate-legacy",
            "node_id": "validator-legacy",
            "finality_signer_public_key": "1111111111111111111111111111111111111111111111111111111111111111",
            "operator_owner": "ops.legacy",
            "public_manifest_hash": "manifest-legacy",
            "requested_at_epoch": 7
        }
    });

    let decoded: GovernanceEvent =
        serde_json::from_value(value).expect("deserialize legacy validator admission event");
    match decoded {
        GovernanceEvent::ValidatorAdmissionSubmitted { stake, .. } => assert_eq!(stake, 100),
        other => panic!("expected validator admission event: {other:?}"),
    }
}

fn register_agent(world: &mut World, agent_id: &str, x: i64, y: i64) {
    world.submit_action(Action::RegisterAgent {
        agent_id: agent_id.to_string(),
        pos: pos(x, y),
    });
    world.step().unwrap();
}

fn set_main_token_controller_registry_for_tests(
    world: &mut World,
    policy_account_ids: &[&str],
    restricted_admin_account_ids: &[&str],
) {
    let ecosystem_controller_account_id = policy_account_ids
        .first()
        .copied()
        .unwrap_or("msig.ecosystem_governance.v1");
    let mut controller_signer_policies = BTreeMap::from([(
        "msig.genesis.v1".to_string(),
        GovernanceThresholdSignerPolicy {
            threshold: 1,
            allowed_public_keys: BTreeSet::from([
                "6249e5a58278dbc4e629a16b5d33f6b84c39e3ceeb10e963bb9ef64ea4daac30".to_string(),
            ]),
        },
    )]);
    for (index, account_id) in policy_account_ids.iter().enumerate() {
        controller_signer_policies.insert(
            account_id.to_string(),
            GovernanceThresholdSignerPolicy {
                threshold: 1,
                allowed_public_keys: BTreeSet::from([format!("{:064x}", index + 3)]),
            },
        );
    }
    world
        .set_governance_main_token_controller_registry(GovernanceMainTokenControllerRegistry {
            genesis_controller_account_id: "msig.genesis.v1".to_string(),
            treasury_bucket_controller_slots: BTreeMap::from([(
                MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL.to_string(),
                ecosystem_controller_account_id.to_string(),
            )]),
            restricted_starter_claim_admin_account_ids: restricted_admin_account_ids
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            controller_signer_policies,
        })
        .expect("set controller registry for tests");
}

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("duration")
        .as_nanos();
    std::env::temp_dir().join(format!("oasis7-governance-{prefix}-{unique}"))
}

#[test]
fn governance_finality_registry_roundtrip_persists_and_drives_epoch_snapshot() {
    let temp_dir = temp_dir("registry-roundtrip");
    let mut world = World::new();
    world
        .set_governance_finality_signer_registry(GovernanceFinalitySignerRegistry {
            slot_id: "governance.finality.v1".to_string(),
            threshold: 2,
            threshold_bps: 0,
            signer_bindings: BTreeMap::from([
                (
                    "governance.finality.v1.signer01".to_string(),
                    "54e7a02919fff2d49a9c325def8cb0211ea7f7a75a9011b9d0678b9e2a7af6bc".to_string(),
                ),
                (
                    "governance.finality.v1.signer02".to_string(),
                    "38dac17ff403cc19de033e47be7cf7b5354635fbc5c1976d7c532e20494aace4".to_string(),
                ),
                (
                    "governance.finality.v1.signer03".to_string(),
                    "e22bd5029176296712fb1a477f91c15775e5ab858181cb4172839ced526f12c8".to_string(),
                ),
            ]),
            validator_stakes: BTreeMap::from([
                ("governance.finality.v1.signer01".to_string(), 50),
                ("governance.finality.v1.signer02".to_string(), 30),
                ("governance.finality.v1.signer03".to_string(), 20),
            ]),
        })
        .expect("set finality registry");
    world
        .set_governance_main_token_controller_registry(GovernanceMainTokenControllerRegistry {
            genesis_controller_account_id: "msig.genesis.v1".to_string(),
            treasury_bucket_controller_slots: BTreeMap::from([(
                "staking_reward_pool".to_string(),
                "msig.staking_governance.v1".to_string(),
            )]),
            restricted_starter_claim_admin_account_ids: BTreeSet::from([
                "msig.staking_governance.v1".to_string(),
            ]),
            controller_signer_policies: BTreeMap::from([
                (
                    "msig.genesis.v1".to_string(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 2,
                        allowed_public_keys: BTreeSet::from([
                            "6249e5a58278dbc4e629a16b5d33f6b84c39e3ceeb10e963bb9ef64ea4daac30"
                                .to_string(),
                            "7014e88a6336ec91fc7e6ffb044b50232e4411ec403f90123fa8a202a3420a04"
                                .to_string(),
                        ]),
                    },
                ),
                (
                    "msig.staking_governance.v1".to_string(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 2,
                        allowed_public_keys: BTreeSet::from([
                            "13c160fc0f516b9a5663aa00c2a5446be6467f68ce341fdd79cdb64224dffd20"
                                .to_string(),
                            "10fa4d90abf753ec1aa54aee3ea53bab25f43e7078897e1fb6a3777af2255bcb"
                                .to_string(),
                        ]),
                    },
                ),
            ]),
        })
        .expect("set controller registry");
    world.save_to_dir(&temp_dir).expect("save world");

    let restored = World::load_from_dir(&temp_dir).expect("restore world");
    let snapshot = restored.governance_effective_finality_epoch_snapshot(7);
    assert_eq!(snapshot.epoch_id, 7);
    assert_eq!(snapshot.threshold, 2);
    assert_eq!(snapshot.signer_node_ids.len(), 3);
    assert!(!snapshot.stake_root.is_empty());
    assert_eq!(
        restored.node_identity_public_key("governance.finality.v1.signer01"),
        Some("54e7a02919fff2d49a9c325def8cb0211ea7f7a75a9011b9d0678b9e2a7af6bc")
    );
    assert_eq!(
        restored
            .governance_main_token_controller_registry()
            .and_then(|registry| registry.controller_signer_policies.get("msig.genesis.v1"))
            .map(|policy| policy.threshold),
        Some(2)
    );
    assert_eq!(
        restored
            .governance_main_token_controller_registry()
            .map(|registry| {
                registry
                    .restricted_starter_claim_admin_account_ids
                    .contains("msig.staking_governance.v1")
            }),
        Some(true)
    );
}

#[test]
fn governance_controller_registry_rejects_restricted_grant_admin_without_policy() {
    let mut world = World::new();
    let err = world
        .set_governance_main_token_controller_registry(GovernanceMainTokenControllerRegistry {
            genesis_controller_account_id: "msig.genesis.v1".to_string(),
            treasury_bucket_controller_slots: BTreeMap::new(),
            restricted_starter_claim_admin_account_ids: BTreeSet::from(["liveops".to_string()]),
            controller_signer_policies: BTreeMap::from([(
                "msig.genesis.v1".to_string(),
                GovernanceThresholdSignerPolicy {
                    threshold: 1,
                    allowed_public_keys: BTreeSet::from([
                        "6249e5a58278dbc4e629a16b5d33f6b84c39e3ceeb10e963bb9ef64ea4daac30"
                            .to_string(),
                    ]),
                },
            )]),
        })
        .expect_err("missing admin policy should be rejected");
    assert!(matches!(err, WorldError::GovernancePolicyInvalid { .. }));
}

#[test]
fn update_restricted_claim_admin_registry_rejects_controller_account_outside_ecosystem_slot() {
    let mut world = World::new();
    set_main_token_controller_registry_for_tests(&mut world, &["liveops"], &["liveops"]);
    let journal_len_before = world.journal().events.len();

    world.submit_action(Action::UpdateRestrictedStarterClaimAdminRegistry {
        controller_account_id: "msig.wrong_controller.v1".to_string(),
        next_admin_account_ids: vec!["ops_backup".to_string()],
    });
    world
        .step()
        .expect("reject wrong controller slot registry update");

    let rejection = world.journal().events[journal_len_before..]
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => Some(reason),
            _ => None,
        })
        .expect("registry update rejection");
    match rejection {
        RejectReason::RuleDenied { notes } => {
            assert!(
                notes
                    .iter()
                    .any(|note| note.contains("ecosystem treasury controller slot"))
            );
        }
        other => panic!("expected rule denied, got {other:?}"),
    }
}

#[test]
fn update_restricted_claim_admin_registry_rejects_account_without_signer_policy() {
    let mut world = World::new();
    set_main_token_controller_registry_for_tests(&mut world, &["liveops"], &["liveops"]);
    let journal_len_before = world.journal().events.len();

    world.submit_action(Action::UpdateRestrictedStarterClaimAdminRegistry {
        controller_account_id: "liveops".to_string(),
        next_admin_account_ids: vec!["ops_backup".to_string()],
    });
    world.step().expect("reject policy-missing registry update");

    let rejection = world.journal().events[journal_len_before..]
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => Some(reason),
            _ => None,
        })
        .expect("registry update rejection");
    match rejection {
        RejectReason::RuleDenied { notes } => {
            assert!(
                notes
                    .iter()
                    .any(|note| { note.contains("missing restricted grant admin signer policy") })
            );
        }
        other => panic!("expected rule denied, got {other:?}"),
    }
}

#[test]
fn update_restricted_claim_admin_registry_applies_governance_event() {
    let mut world = World::new();
    set_main_token_controller_registry_for_tests(
        &mut world,
        &["liveops", "ops_backup"],
        &["liveops"],
    );

    world.submit_action(Action::UpdateRestrictedStarterClaimAdminRegistry {
        controller_account_id: "liveops".to_string(),
        next_admin_account_ids: vec!["liveops".to_string(), "ops_backup".to_string()],
    });
    world.step().expect("apply registry update");

    let registry = world
        .governance_main_token_controller_registry()
        .expect("registry after update");
    assert_eq!(
        registry
            .restricted_starter_claim_admin_account_ids
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["liveops".to_string(), "ops_backup".to_string()]
    );
    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Governance(
            GovernanceEvent::RestrictedStarterClaimAdminRegistryUpdated {
                controller_account_id,
                previous_admin_account_ids,
                next_admin_account_ids,
            },
        ) => {
            assert_eq!(controller_account_id, "liveops");
            assert_eq!(previous_admin_account_ids, &vec!["liveops".to_string()]);
            assert_eq!(
                next_admin_account_ids,
                &vec!["liveops".to_string(), "ops_backup".to_string()]
            );
        }
        other => panic!("expected governance registry update event, got {other:?}"),
    }
}

#[test]
fn governance_flow_applies_manifest() {
    let mut world = World::new();
    let manifest = Manifest {
        version: 2,
        content: json!({ "name": "demo" }),
    };

    let proposal_id = world
        .propose_manifest_update(manifest.clone(), "alice")
        .unwrap();
    let shadow_hash = world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();
    let applied_hash = world.apply_proposal(proposal_id).unwrap();

    assert_eq!(shadow_hash, applied_hash);
    assert_eq!(world.manifest().version, 2);
    assert_eq!(world.manifest().content, manifest.content);
}

#[test]
fn governance_policy_blocks_local_apply_proposal_path() {
    let mut world = World::new();
    world.enable_production_release_policy();
    let manifest = Manifest {
        version: 2,
        content: json!({ "name": "external-finality-only" }),
    };

    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();

    let err = world.apply_proposal(proposal_id).unwrap_err();
    assert!(matches!(err, WorldError::GovernancePolicyInvalid { .. }));

    let certificate = world.build_local_finality_certificate(proposal_id).unwrap();
    world
        .apply_proposal_with_finality(proposal_id, &certificate)
        .expect("apply with explicit finality cert");
}

#[test]
fn governance_patch_updates_manifest() {
    let mut world = World::new();
    let base_hash = world.current_manifest_hash().unwrap();
    let patch = ManifestPatch {
        base_manifest_hash: base_hash,
        ops: vec![ManifestPatchOp::Set {
            path: vec!["settings".to_string(), "mode".to_string()],
            value: json!("fast"),
        }],
        new_version: Some(3),
    };

    let proposal_id = world.propose_manifest_patch(patch, "alice").unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();
    world.apply_proposal(proposal_id).unwrap();

    assert_eq!(world.manifest().version, 3);
    assert_eq!(
        world.manifest().content,
        json!({ "settings": { "mode": "fast" } })
    );
}

#[test]
fn manifest_diff_and_merge() {
    let base = Manifest {
        version: 1,
        content: json!({ "a": 1, "b": { "a_base_only": 1, "m_both": 2 }, "c_base_only": true }),
    };
    let target = Manifest {
        version: 2,
        content: json!({ "a": 1, "b": { "b_target_only": 4, "m_both": 3 }, "d_target_only": false }),
    };

    let patch = diff_manifest(&base, &target).unwrap();
    assert_eq!(
        patch.ops,
        vec![
            ManifestPatchOp::Remove {
                path: manifest_path(&["b", "a_base_only"])
            },
            ManifestPatchOp::Set {
                path: manifest_path(&["b", "b_target_only"]),
                value: json!(4),
            },
            ManifestPatchOp::Set {
                path: manifest_path(&["b", "m_both"]),
                value: json!(3),
            },
            ManifestPatchOp::Remove {
                path: manifest_path(&["c_base_only"])
            },
            ManifestPatchOp::Set {
                path: manifest_path(&["d_target_only"]),
                value: json!(false),
            },
        ]
    );
    let applied = apply_manifest_patch(&base, &patch).unwrap();
    assert_eq!(applied, target);

    let base_hash = util::hash_json(&base).unwrap();
    let patch1 = ManifestPatch {
        base_manifest_hash: base_hash.clone(),
        ops: vec![ManifestPatchOp::Set {
            path: manifest_path(&["b", "m_both"]),
            value: json!(3),
        }],
        new_version: Some(2),
    };
    let patch2 = ManifestPatch {
        base_manifest_hash: base_hash,
        ops: vec![ManifestPatchOp::Set {
            path: manifest_path(&["e"]),
            value: json!(5),
        }],
        new_version: Some(3),
    };

    let merged = merge_manifest_patches(&base, &[patch1, patch2]).unwrap();
    let merged_applied = apply_manifest_patch(&base, &merged).unwrap();
    let expected = Manifest {
        version: 3,
        content: json!({ "a": 1, "b": { "a_base_only": 1, "m_both": 3 }, "c_base_only": true, "e": 5 }),
    };
    assert_eq!(merged_applied, expected);
}

#[test]
fn merge_reports_conflicts() {
    let base = Manifest {
        version: 1,
        content: json!({ "a": { "b": 1 }, "x": 1 }),
    };
    let base_hash = util::hash_json(&base).unwrap();
    let patch1 = ManifestPatch {
        base_manifest_hash: base_hash.clone(),
        ops: vec![ManifestPatchOp::Set {
            path: vec!["a".to_string(), "b".to_string()],
            value: json!(2),
        }],
        new_version: None,
    };
    let patch2 = ManifestPatch {
        base_manifest_hash: base_hash,
        ops: vec![ManifestPatchOp::Set {
            path: vec!["a".to_string()],
            value: json!({ "b": 3 }),
        }],
        new_version: None,
    };

    let result = merge_manifest_patches_with_conflicts(&base, &[patch1, patch2]).unwrap();
    assert_eq!(result.conflicts.len(), 1);
    assert_eq!(result.conflicts[0].path, vec!["a".to_string()]);
    assert_eq!(result.conflicts[0].kind, ConflictKind::PrefixOverlap);
    assert_eq!(result.conflicts[0].patches, vec![0, 1]);
    assert_eq!(result.conflicts[0].ops.len(), 2);
}

#[test]
fn governance_apply_with_finality_rejects_threshold_mismatch() {
    let mut world = World::new();
    let manifest = Manifest {
        version: 2,
        content: json!({ "name": "demo" }),
    };
    let proposal_id = world
        .propose_manifest_update(manifest.clone(), "alice")
        .unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();

    let mut certificate = world.build_local_finality_certificate(proposal_id).unwrap();
    certificate.min_unique_signers = certificate.effective_min_unique_signers().saturating_add(1);
    certificate.threshold = certificate.min_unique_signers;
    let err = world
        .apply_proposal_with_finality(proposal_id, &certificate)
        .unwrap_err();
    assert!(matches!(err, WorldError::GovernanceFinalityInvalid { .. }));
}

#[test]
fn governance_apply_with_finality_rejects_validator_set_hash_mismatch() {
    let mut world = World::new();
    let manifest = Manifest {
        version: 2,
        content: json!({ "name": "validator-set-hash-mismatch" }),
    };
    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();

    let mut certificate = world.build_local_finality_certificate(proposal_id).unwrap();
    certificate.validator_set_hash = "deadbeef".to_string();
    let err = world
        .apply_proposal_with_finality(proposal_id, &certificate)
        .unwrap_err();
    let WorldError::GovernanceFinalityInvalid { reason } = err else {
        panic!("expected GovernanceFinalityInvalid");
    };
    assert!(reason.contains("validator_set_hash mismatch"));
}

#[test]
fn governance_apply_with_finality_rejects_stake_root_mismatch() {
    let mut world = World::new();
    let manifest = Manifest {
        version: 2,
        content: json!({ "name": "stake-root-mismatch" }),
    };
    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();

    let mut certificate = world.build_local_finality_certificate(proposal_id).unwrap();
    certificate.stake_root = "deadbeef".to_string();
    let err = world
        .apply_proposal_with_finality(proposal_id, &certificate)
        .unwrap_err();
    let WorldError::GovernanceFinalityInvalid { reason } = err else {
        panic!("expected GovernanceFinalityInvalid");
    };
    assert!(reason.contains("stake_root mismatch"));
}

#[test]
fn governance_apply_with_finality_rejects_signer_outside_epoch_snapshot() {
    let mut world = World::new();
    bind_finality_signer_with_seed(
        &mut world,
        ROTATED_FINALITY_SIGNER_3.0,
        ROTATED_FINALITY_SIGNER_3.1,
    );
    world
        .set_governance_finality_epoch_snapshot(GovernanceFinalityEpochSnapshot {
            epoch_id: 0,
            threshold: 2,
            signer_node_ids: vec![
                LOCAL_FINALITY_SIGNER_1.0.to_string(),
                ROTATED_FINALITY_SIGNER_3.0.to_string(),
            ],
            ..GovernanceFinalityEpochSnapshot::default()
        })
        .expect("set epoch snapshot");

    let manifest = Manifest {
        version: 2,
        content: json!({ "name": "epoch-signer-check" }),
    };
    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();

    let stale_certificate = world.build_local_finality_certificate(proposal_id).unwrap();
    let err = world
        .apply_proposal_with_finality(proposal_id, &stale_certificate)
        .unwrap_err();
    let WorldError::GovernanceFinalityInvalid { reason } = err else {
        panic!("expected GovernanceFinalityInvalid");
    };
    assert!(reason.contains("not part of finality epoch snapshot"));
    assert!(reason.contains(LOCAL_FINALITY_SIGNER_2.0));
}

#[cfg(feature = "test_tier_full")]
#[test]
fn governance_finality_epoch_snapshot_rotation_rejects_stale_signers_and_accepts_rotated_set() {
    let mut world = World::new();
    world
        .set_governance_execution_policy(GovernanceExecutionPolicy {
            epoch_length_ticks: 2,
            ..GovernanceExecutionPolicy::default()
        })
        .expect("set governance policy");
    bind_finality_signer_with_seed(
        &mut world,
        ROTATED_FINALITY_SIGNER_3.0,
        ROTATED_FINALITY_SIGNER_3.1,
    );
    world
        .set_governance_finality_epoch_snapshot(GovernanceFinalityEpochSnapshot {
            epoch_id: 0,
            threshold: 2,
            signer_node_ids: vec![
                LOCAL_FINALITY_SIGNER_1.0.to_string(),
                LOCAL_FINALITY_SIGNER_2.0.to_string(),
            ],
            ..GovernanceFinalityEpochSnapshot::default()
        })
        .expect("set epoch 0 snapshot");
    world
        .set_governance_finality_epoch_snapshot(GovernanceFinalityEpochSnapshot {
            epoch_id: 1,
            threshold: 2,
            signer_node_ids: vec![
                LOCAL_FINALITY_SIGNER_1.0.to_string(),
                ROTATED_FINALITY_SIGNER_3.0.to_string(),
            ],
            ..GovernanceFinalityEpochSnapshot::default()
        })
        .expect("set epoch 1 snapshot");
    for _ in 0..2 {
        world.step().expect("advance governance epoch");
    }

    let manifest = Manifest {
        version: 2,
        content: json!({ "name": "epoch-signer-rotation" }),
    };
    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();

    let stale_certificate = build_finality_certificate_with_signers(
        &world,
        proposal_id,
        &[LOCAL_FINALITY_SIGNER_1, LOCAL_FINALITY_SIGNER_2],
    );
    let err = world
        .apply_proposal_with_finality(proposal_id, &stale_certificate)
        .unwrap_err();
    let WorldError::GovernanceFinalityInvalid { reason } = err else {
        panic!("expected GovernanceFinalityInvalid");
    };
    assert!(reason.contains("not part of finality epoch snapshot"));
    assert!(reason.contains(LOCAL_FINALITY_SIGNER_2.0));

    let rotated_certificate = build_finality_certificate_with_signers(
        &world,
        proposal_id,
        &[LOCAL_FINALITY_SIGNER_1, ROTATED_FINALITY_SIGNER_3],
    );
    world
        .apply_proposal_with_finality(proposal_id, &rotated_certificate)
        .expect("apply with rotated signer set");
}

#[test]
fn governance_apply_emits_manifest_updated_before_applied() {
    let mut world = World::new();
    let manifest = Manifest {
        version: 2,
        content: json!({ "name": "demo" }),
    };
    let proposal_id = world
        .propose_manifest_update(manifest.clone(), "alice")
        .unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();
    let certificate = world.build_local_finality_certificate(proposal_id).unwrap();

    world
        .apply_proposal_with_finality(proposal_id, &certificate)
        .unwrap();

    let mut manifest_updated_idx = None;
    let mut applied_idx = None;
    for (idx, event) in world.journal().events.iter().enumerate() {
        match &event.body {
            WorldEventBody::ManifestUpdated(_) => manifest_updated_idx = Some(idx),
            WorldEventBody::Governance(GovernanceEvent::Applied {
                proposal_id: pid, ..
            }) if *pid == proposal_id => applied_idx = Some(idx),
            _ => {}
        }
    }
    let manifest_updated_idx = manifest_updated_idx.expect("manifest updated event");
    let applied_idx = applied_idx.expect("applied event");
    assert!(
        manifest_updated_idx < applied_idx,
        "manifest must be updated before applied marker"
    );
}

#[test]
fn governance_timelock_blocks_early_apply() {
    let mut world = World::new();
    world
        .set_governance_execution_policy(GovernanceExecutionPolicy {
            timelock_ticks: 3,
            ..GovernanceExecutionPolicy::default()
        })
        .unwrap();

    let manifest = Manifest {
        version: 2,
        content: json!({ "name": "timelock" }),
    };
    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();

    let err = world.apply_proposal(proposal_id).unwrap_err();
    assert!(matches!(err, WorldError::GovernancePolicyInvalid { .. }));

    for _ in 0..3 {
        world.step().unwrap();
    }
    world.apply_proposal(proposal_id).unwrap();
}

#[test]
fn governance_epoch_gate_blocks_early_apply() {
    let mut world = World::new();
    world
        .set_governance_execution_policy(GovernanceExecutionPolicy {
            epoch_length_ticks: 5,
            activation_delay_epochs: 1,
            ..GovernanceExecutionPolicy::default()
        })
        .unwrap();

    let manifest = Manifest {
        version: 2,
        content: json!({ "name": "epoch" }),
    };
    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();

    let err = world.apply_proposal(proposal_id).unwrap_err();
    assert!(matches!(err, WorldError::GovernancePolicyInvalid { .. }));

    for _ in 0..5 {
        world.step().unwrap();
    }
    world.apply_proposal(proposal_id).unwrap();
}

#[test]
fn governance_emergency_brake_and_release_gate_apply() {
    let mut world = World::new();
    let manifest = Manifest {
        version: 2,
        content: json!({ "name": "brake" }),
    };
    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();

    world
        .activate_emergency_brake("guardian-1", "incident", 4, local_guardians())
        .unwrap();
    let err = world.apply_proposal(proposal_id).unwrap_err();
    assert!(matches!(err, WorldError::GovernancePolicyInvalid { .. }));

    world
        .release_emergency_brake("guardian-2", "incident mitigated", local_guardians())
        .unwrap();
    world.apply_proposal(proposal_id).unwrap();
}

#[test]
fn governance_emergency_veto_rejects_queued_proposal() {
    let mut world = World::new();
    let manifest = Manifest {
        version: 2,
        content: json!({ "name": "veto" }),
    };
    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();

    world
        .emergency_veto_proposal(
            proposal_id,
            "guardian-1",
            "unsafe parameter drift",
            local_guardians(),
        )
        .unwrap();
    let proposal = world.proposals().get(&proposal_id).unwrap();
    assert!(matches!(proposal.status, ProposalStatus::Rejected { .. }));
    let ProposalStatus::Rejected { reason } = &proposal.status else {
        panic!("proposal should be rejected");
    };
    assert!(reason.contains("emergency_veto"));

    let err = world.apply_proposal(proposal_id).unwrap_err();
    assert!(matches!(err, WorldError::ProposalInvalidState { .. }));
}

#[test]
fn governance_emergency_controls_reject_invalid_guardian_signatures() {
    let mut world = World::new();
    let manifest = Manifest {
        version: 2,
        content: json!({ "name": "guardian-check" }),
    };
    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();

    let below_threshold = world
        .activate_emergency_brake(
            "guardian-1",
            "threshold check",
            3,
            vec!["governance.local.finality.signer.1".to_string()],
        )
        .unwrap_err();
    assert!(matches!(
        below_threshold,
        WorldError::GovernancePolicyInvalid { .. }
    ));

    let untrusted_signer = world
        .emergency_veto_proposal(
            proposal_id,
            "guardian-2",
            "untrusted signer",
            vec![
                "governance.local.finality.signer.1".to_string(),
                "governance.unknown.signer".to_string(),
            ],
        )
        .unwrap_err();
    assert!(matches!(
        untrusted_signer,
        WorldError::GovernancePolicyInvalid { .. }
    ));
}

#[test]
fn governance_identity_penalty_freezes_and_slashes_profile() {
    let mut world = World::new();
    register_agent(&mut world, "agent-1", 0, 0);
    world
        .set_governance_identity_profile("agent-1", 100, 0, GovernanceIdentityStatus::Active)
        .unwrap();

    let penalty_id = world
        .apply_identity_penalty(
            "agent-1",
            "evidence.sybil.cluster",
            "suspected sybil coordination",
            40,
            10,
            "guardian-1",
            local_guardians(),
        )
        .unwrap();

    let profile = world.governance_identity_profile("agent-1").unwrap();
    assert_eq!(profile.status, GovernanceIdentityStatus::Frozen);
    assert_eq!(profile.stake_locked, 60);
    assert_eq!(profile.slash_count, 1);

    let record = world
        .governance_identity_penalties()
        .get(&penalty_id)
        .unwrap();
    assert_eq!(record.status, GovernanceIdentityPenaltyStatus::Applied);
    assert_eq!(record.slash_stake, 40);
    assert_eq!(record.target_agent_id, "agent-1");
}

#[test]
fn governance_identity_penalty_appeal_accept_restores_profile() {
    let mut world = World::new();
    register_agent(&mut world, "agent-1", 0, 0);
    world
        .set_governance_identity_profile("agent-1", 50, 0, GovernanceIdentityStatus::Active)
        .unwrap();

    let penalty_id = world
        .apply_identity_penalty(
            "agent-1",
            "evidence.fp.case",
            "potential false positive",
            20,
            10,
            "guardian-1",
            local_guardians(),
        )
        .unwrap();
    world
        .appeal_identity_penalty(penalty_id, "agent-1", "request review")
        .unwrap();
    world
        .resolve_identity_penalty_appeal(penalty_id, "committee", true, "appeal accepted")
        .unwrap();

    let profile = world.governance_identity_profile("agent-1").unwrap();
    assert_eq!(profile.status, GovernanceIdentityStatus::Active);
    assert_eq!(profile.stake_locked, 50);

    let record = world
        .governance_identity_penalties()
        .get(&penalty_id)
        .unwrap();
    assert_eq!(
        record.status,
        GovernanceIdentityPenaltyStatus::AppealAccepted
    );
    assert_eq!(record.resolved_by.as_deref(), Some("committee"));
}

#[test]
fn governance_identity_penalty_appeal_respects_deadline() {
    let mut world = World::new();
    register_agent(&mut world, "agent-1", 0, 0);
    world
        .set_governance_identity_profile("agent-1", 30, 0, GovernanceIdentityStatus::Active)
        .unwrap();
    let penalty_id = world
        .apply_identity_penalty(
            "agent-1",
            "evidence.deadline.case",
            "deadline check",
            10,
            1,
            "guardian-1",
            local_guardians(),
        )
        .unwrap();

    for _ in 0..2 {
        world.step().unwrap();
    }
    let err = world
        .appeal_identity_penalty(penalty_id, "agent-1", "too late")
        .unwrap_err();
    assert!(matches!(err, WorldError::GovernancePolicyInvalid { .. }));
}

#[test]
fn governance_identity_penalty_rejects_duplicate_incident_signature() {
    let mut world = World::new();
    register_agent(&mut world, "agent-1", 0, 0);
    world
        .set_governance_identity_profile("agent-1", 80, 0, GovernanceIdentityStatus::Active)
        .unwrap();

    world
        .apply_identity_penalty(
            "agent-1",
            "evidence.sybil.replay",
            "first signal",
            10,
            10,
            "guardian-1",
            local_guardians(),
        )
        .unwrap();
    let err = world
        .apply_identity_penalty(
            "agent-1",
            "evidence.sybil.replay",
            "duplicate signal",
            10,
            10,
            "guardian-1",
            local_guardians(),
        )
        .unwrap_err();
    assert!(matches!(err, WorldError::GovernancePolicyInvalid { .. }));
    let WorldError::GovernancePolicyInvalid { reason } = err else {
        panic!("expected governance policy invalid");
    };
    assert!(reason.contains("duplicate identity penalty incident"));
}

#[test]
fn governance_identity_penalty_evidence_chain_tracks_appeal_and_resolution() {
    let mut world = World::new();
    register_agent(&mut world, "agent-1", 0, 0);
    world
        .set_governance_identity_profile("agent-1", 60, 0, GovernanceIdentityStatus::Active)
        .unwrap();

    let penalty_id = world
        .apply_identity_penalty(
            "agent-1",
            "evidence.sybil.chain",
            "chain seed",
            20,
            10,
            "guardian-1",
            local_guardians(),
        )
        .unwrap();
    let root_chain_hash = world
        .governance_identity_penalties()
        .get(&penalty_id)
        .unwrap()
        .evidence_chain_hash
        .clone();
    assert!(!root_chain_hash.is_empty());

    world
        .appeal_identity_penalty(penalty_id, "agent-1", "provide counter evidence")
        .unwrap();
    let appealed = world
        .governance_identity_penalties()
        .get(&penalty_id)
        .unwrap();
    assert!(appealed.appeal_evidence_hash.is_some());
    assert_ne!(appealed.evidence_chain_hash, root_chain_hash);
    let appeal_chain_hash = appealed.evidence_chain_hash.clone();

    world
        .resolve_identity_penalty_appeal(penalty_id, "committee", false, "appeal rejected")
        .unwrap();
    let resolved = world
        .governance_identity_penalties()
        .get(&penalty_id)
        .unwrap();
    assert!(resolved.resolution_evidence_hash.is_some());
    assert_ne!(resolved.evidence_chain_hash, appeal_chain_hash);
}

#[test]
fn governance_identity_penalty_monitor_reports_false_positive_and_open_risk() {
    let mut world = World::new();
    register_agent(&mut world, "agent-1", 0, 0);
    world
        .set_governance_identity_profile("agent-1", 100, 0, GovernanceIdentityStatus::Active)
        .unwrap();

    let restored_penalty = world
        .apply_identity_penalty(
            "agent-1",
            "evidence.fp.monitor",
            "possible false positive",
            10,
            10,
            "guardian-1",
            local_guardians(),
        )
        .unwrap();
    world
        .appeal_identity_penalty(restored_penalty, "agent-1", "counter evidence provided")
        .unwrap();
    world
        .resolve_identity_penalty_appeal(
            restored_penalty,
            "committee",
            true,
            "counter evidence accepted",
        )
        .unwrap();

    world
        .apply_identity_penalty(
            "agent-1",
            "evidence.sybil.open",
            "still under review",
            5,
            10,
            "guardian-2",
            local_guardians(),
        )
        .unwrap();

    let stats = world.governance_identity_penalty_monitor_stats(0);
    assert_eq!(stats.total_penalties, 2);
    assert_eq!(stats.appealed_penalties, 1);
    assert_eq!(stats.resolved_appeals, 1);
    assert_eq!(stats.appeal_accepted_penalties, 1);
    assert_eq!(stats.high_risk_open_penalties, 1);
    assert_eq!(stats.false_positive_rate_bps, 10_000);
}

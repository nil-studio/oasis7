use super::*;
use oasis7_runtime::service_admission::participant_policy::*;
use oasis7_runtime::{
    GovernanceFinalityCertificate, GovernanceFinalityEpochSnapshot, Manifest, ProposalDecision,
    ProposalStatus,
};
use std::collections::BTreeMap;
const GOVERNORS: [&str; 2] = ["participant-governor-a", "participant-governor-b"];
fn governor(index: usize) -> SigningKey {
    SigningKey::from_bytes(&[91 + index as u8; 32])
}
pub(super) fn install(
    world: &mut RuntimeWorld,
    identity: &WorldIdentity,
    generation: u64,
    enabled: bool,
) {
    let mut current_namespace = namespace();
    current_namespace.instance_epoch = DecimalU64(generation);
    install_for_identity(world, identity, generation, enabled, current_namespace, 71);
}
pub(super) fn install_for_identity(
    world: &mut RuntimeWorld,
    identity: &WorldIdentity,
    generation: u64,
    enabled: bool,
    current_namespace: GameIdentityNamespaceV1,
    issuer_seed: u8,
) {
    if generation == 1 {
        for (index, node) in GOVERNORS.iter().enumerate() {
            world
                .bind_node_identity(
                    node,
                    &hex::encode(governor(index).verifying_key().to_bytes()),
                )
                .unwrap();
        }
        world
            .set_governance_finality_epoch_snapshot(GovernanceFinalityEpochSnapshot {
                epoch_id: 0,
                threshold: 2,
                min_unique_signers: 2,
                threshold_bps: 10_000,
                signer_node_ids: GOVERNORS.map(str::to_owned).to_vec(),
                validator_stakes: GOVERNORS
                    .map(|node| (node.to_owned(), 100))
                    .into_iter()
                    .collect(),
                ..Default::default()
            })
            .unwrap();
    }
    let policy = ParticipantEntryPolicyV1 {
        api_rev: 1,
        world_id: identity.world_id.clone(),
        genesis_digest: identity.genesis_digest.clone(),
        identity_namespace: current_namespace.clone(),
        issuer_public_key: sign_read_request("fixture", (), &hex::encode([issuer_seed; 32]))
            .unwrap()
            .subject_public_key,
        generation,
        max_grant_ticks: 256,
        max_participants: 8,
        registry_complete: true,
        enabled,
    };
    let binding = world.current_cognition_runtime_binding().unwrap();
    let manifest = ParticipantMigrationManifestV1 {
        api_rev: 1,
        policy_digest: policy_digest(&policy).unwrap(),
        world_id: identity.world_id.clone(),
        genesis_digest: identity.genesis_digest.clone(),
        branch_id: binding.branch_id,
        reorg_epoch: binding.reorg_epoch,
        identity_namespace: current_namespace,
        source_snapshot_epoch: generation,
        source_watermark: format!("private-fixture-snapshot-{generation}"),
        source_snapshot_digest: format!("blake3:{}", "a".repeat(64)),
        complete: true,
        previous_registry_digest: world.participant_registry_digest().unwrap(),
        legacy_owner_state_digest: world.participant_legacy_owner_state_digest().unwrap(),
        entries: Vec::new(),
    };
    let manifest = sign_read_request(
        PARTICIPANT_MIGRATION_MANIFEST_DOMAIN,
        manifest,
        &hex::encode([issuer_seed; 32]),
    )
    .unwrap();
    let proposal=world.propose_manifest_update(Manifest {version:world.manifest().version+1,content:serde_json::json!({"participant_policy_authorization":policy_proposal_commitment(&policy,&manifest).unwrap()})},"private-fixture").unwrap();
    world.shadow_proposal(proposal).unwrap();
    world
        .approve_proposal(proposal, "private-fixture", ProposalDecision::Approve)
        .unwrap();
    let hash = match &world.proposals().get(&proposal).unwrap().status {
        ProposalStatus::Approved { manifest_hash, .. } => manifest_hash.clone(),
        _ => unreachable!(),
    };
    let snapshot = world.governance_finality_epoch_snapshots().get(&0).unwrap();
    let height = world.journal().events.len() as u64 + 1;
    let mut certificate = GovernanceFinalityCertificate {
        proposal_id: proposal,
        manifest_hash: hash,
        consensus_height: height,
        epoch_id: 0,
        validator_set_hash: snapshot.validator_set_hash.clone(),
        stake_root: snapshot.stake_root.clone(),
        threshold_bps: snapshot.threshold_bps,
        min_unique_signers: 2,
        threshold: 2,
        signatures: BTreeMap::new(),
    };
    for (index, node) in GOVERNORS.iter().enumerate() {
        let payload = GovernanceFinalityCertificate::signing_payload_v1(
            proposal,
            &certificate.manifest_hash,
            height,
            0,
            &certificate.validator_set_hash,
            &certificate.stake_root,
            certificate.threshold_bps,
            2,
            node,
        );
        certificate.signatures.insert(
            (*node).into(),
            format!(
                "{}{}",
                GovernanceFinalityCertificate::SIGNATURE_PREFIX_ED25519_V1,
                hex::encode(governor(index).sign(&payload).to_bytes())
            ),
        );
    }
    let mut proof = ParticipantPolicyProofV1 {
        manifest,
        certificate,
        signatures: BTreeMap::new(),
    };
    let payload = policy_signing_payload(&policy, &proof);
    let domain = if generation == 1 {
        PARTICIPANT_POLICY_INSTALL_DOMAIN
    } else {
        PARTICIPANT_POLICY_UPDATE_DOMAIN
    };
    for (index, node) in GOVERNORS.iter().enumerate() {
        let signed = sign_read_request(
            domain,
            payload.clone(),
            &hex::encode(governor(index).to_bytes()),
        )
        .unwrap();
        proof
            .signatures
            .insert((*node).into(), signed.signature_hex);
    }
    world
        .install_participant_policy_with_finality_proof(policy, proof)
        .unwrap();
}

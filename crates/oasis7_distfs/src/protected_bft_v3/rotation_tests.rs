//! Real signed qualified-H protocol fixtures and native authenticated TCP.
//! The backend/clock below are explicit memory doubles, not World execution or
//! physical custody evidence. World must still authorize the complete action.
use super::*;
use crate::protected_bft_v3::*;
use ed25519_dalek::{Signer, SigningKey};
use oasis7_agent_api::world_action::GovernanceAuthorizationV1;
use oasis7_formal_protocol::{
    bft::{BftProfileTargetV2, BftSchedulePolicyV2, verify_formal_bft_handoff_v2},
    controlled_authority::formal::{
        FormalCompletionBranchV1, InitialActivationTrustV1, ProfileHandoffV1,
        VerifiedFormalReadAnchorV1, test_fixtures as fixtures, verify_formal_qualified,
    },
};
use oasis7_proto::distributed_bft::v2::{BftScopeV2, BftValidatorV2};
use std::{
    collections::BTreeMap,
    net::{SocketAddr, TcpListener},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};
fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}
fn public(n: u8) -> String {
    hex::encode(key(n).verifying_key().to_bytes())
}
fn hash(n: u8) -> String {
    hex::encode([n; 32])
}
fn binding() -> BftSignerBindingV3 {
    let (certificate, authority, initial_proof, initial_commit) = fixtures::initial_with_profile(2);
    let trust = InitialActivationTrustV1 {
        issuer_public_key: fixtures::public(9),
        world: certificate.body.authority.world.clone(),
        initial_state_root: certificate.body.initial_state_root.clone(),
        governing_manifest_digest: certificate.body.governing_manifest_digest.clone(),
        recovery_policy_digest: certificate.body.recovery_policy.digest().unwrap(),
    };
    let anchor =
        VerifiedFormalReadAnchorV1::from_initial(&certificate, &trust, &initial_proof).unwrap();
    let scope = BftScopeV2 {
        world_id: trust.world.world_id.clone(),
        chain_id: trust.world.chain_id.clone(),
        genesis_hash: trust.world.genesis_digest.clone(),
    };
    let target = BftProfileTargetV2::new(
        scope,
        2,
        vec![BftValidatorV2 {
            validator_id: "validator-a".into(),
            public_key_hex: public(31),
            voting_power: 1,
        }],
        BftSchedulePolicyV2::new(5, "bft").unwrap(),
    )
    .unwrap();
    let handoff = ProfileHandoffV1 {
        next_profile: "bft".into(),
        next_profile_version: 1,
        next_authority_digest: target.target_digest().unwrap(),
        valid_from_position: 3,
    };
    let frozen = fixtures::frozen(&authority, &initial_commit, Some(handoff));
    let record = fixtures::execution(&authority, &frozen);
    let proof = fixtures::proof(
        record.clone(),
        &authority,
        FormalCompletionBranchV1::Normal,
        None,
        Some(frozen),
        4,
        5,
        6,
    );
    let committed =
        verify_formal_qualified(&proof, &authority, Some(&initial_commit), None).unwrap();
    let anchor = anchor.after_verified_commit(authority, committed).unwrap();
    let handoff = verify_formal_bft_handoff_v2(&anchor, target).unwrap();
    let schedule = handoff
        .initial_schedule_after_handoff_replay(
            &record.after_state_root,
            &record.governing_manifest_digest,
        )
        .unwrap();
    let mut provisioned = vec![
        BftSignerKeyIdentityV3 {
            validator_id: "validator-a".into(),
            public_key_hex: public(31),
        },
        BftSignerKeyIdentityV3 {
            validator_id: "validator-a".into(),
            public_key_hex: public(32),
        },
    ];
    provisioned.sort_by(|a, b| a.public_key_hex.cmp(&b.public_key_hex));
    BftSignerBindingV3::new(
        handoff,
        schedule,
        hash(70),
        "validator-a".into(),
        provisioned,
        oasis7_consensus::tendermint_v1::BftTimeoutsV1 {
            propose_ms: 100,
            prevote_ms: 100,
            precommit_ms: 100,
            round_delta_ms: 10,
        },
        "rotation-native-runtime".into(),
        public(34),
    )
    .unwrap()
}
fn body(binding: &BftSignerBindingV3) -> FinalityKeyRotationBodyV1 {
    let schedule = binding.initial_schedule();
    FinalityKeyRotationBodyV1 {
        schema_version: 1,
        authorization: GovernanceAuthorizationV1 {
            schema_version: 1,
            world_id: binding.scope.world_id.clone(),
            chain_id: binding.scope.chain_id.clone(),
            genesis_digest: binding.scope.genesis_hash.clone(),
            parent_position: schedule.parent_tick(),
            parent_record_id: schedule.parent_commit_ref().into(),
            parent_state_root: schedule.parent_state_root().into(),
            policy_digest: hash(71),
            controller_account_id: "world-must-check-controller".into(),
            nonce: 1,
            valid_from_position: schedule.height(),
            valid_through_position: schedule.height(),
            signatures: BTreeMap::new(),
        },
        rotation_id: "original-rotation".into(),
        candidate_id: "world-must-check-admission".into(),
        node_id: binding.local_validator_id.clone(),
        expected_generation: 1,
        old_public_key_hex: public(31),
        new_public_key_hex: public(32),
        decision_height: schedule.height(),
        activation_height: schedule.height() + 2,
        target_epoch: 1,
    }
}
struct Handle {
    key: SigningKey,
    disabled: Option<Arc<AtomicUsize>>,
}
impl BftSigningKeyHandleV3 for Handle {
    fn public_key_hex(&self) -> String {
        hex::encode(self.key.verifying_key().to_bytes())
    }
    fn sign_preimage(&self, bytes: &[u8]) -> Result<[u8; 64], BftSignerErrorV3> {
        if let Some(calls) = &self.disabled {
            calls.fetch_add(1, Ordering::SeqCst);
            return Err(BftSignerErrorV3("test original key unavailable".into()));
        }
        Ok(self.key.sign(bytes).to_bytes())
    }
}
struct Backend {
    binding: String,
    snapshot: Arc<Mutex<BftSignerBackendSnapshotV3>>,
}
impl Backend {
    fn check(
        snapshot: &BftSignerBackendSnapshotV3,
        generation: u64,
        token: &str,
    ) -> Result<(), BftSignerErrorV3> {
        if snapshot.lease_generation != generation
            || token.is_empty()
            || snapshot.lease_token_digest != token
        {
            Err(BftSignerErrorV3("test lease fenced".into()))
        } else {
            Ok(())
        }
    }
}
impl BftSignerBackendV3 for Backend {
    fn load_provisioned(
        &mut self,
        binding: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        if binding != self.binding {
            return Err(BftSignerErrorV3("test binding differs".into()));
        }
        Ok(self.snapshot.lock().unwrap().clone())
    }
    fn acquire_lease(
        &mut self,
        binding: &str,
        generation: u64,
        token: &str,
    ) -> Result<BftSignerAcquireOutcomeV3, BftSignerErrorV3> {
        self.load_provisioned(binding)?;
        let mut s = self.snapshot.lock().unwrap();
        if s.lease_generation != generation || !s.lease_token_digest.is_empty() {
            return Ok(BftSignerAcquireOutcomeV3::Conflict(s.clone()));
        }
        s.lease_generation += 1;
        s.lease_token_digest = token.into();
        Ok(BftSignerAcquireOutcomeV3::Acquired(s.clone()))
    }
    fn release_lease(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        let mut s = self.snapshot.lock().unwrap();
        Self::check(&s, generation, token)?;
        s.lease_generation += 1;
        s.lease_token_digest.clear();
        Ok(s.clone())
    }
    fn read_for_lease(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        let s = self.snapshot.lock().unwrap();
        Self::check(&s, generation, token)?;
        Ok(s.clone())
    }
    fn protected_time_ms(&mut self, generation: u64, token: &str) -> Result<i64, BftSignerErrorV3> {
        let mut s = self.snapshot.lock().unwrap();
        Self::check(&s, generation, token)?;
        s.protected_time_floor_ms += 1;
        Ok(s.protected_time_floor_ms)
    }
    fn compare_and_swap(
        &mut self,
        generation: u64,
        token: &str,
        revision: u64,
        digest: &str,
        event: Option<i64>,
        next: BftSignerStateV3,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        let mut s = self.snapshot.lock().unwrap();
        Self::check(&s, generation, token)?;
        if s.state.revision != revision
            || s.state.state_digest != digest
            || next.revision != revision + 1
        {
            return Err(BftSignerErrorV3("test exact CAS differs".into()));
        }
        if let Some(t) = event {
            s.protected_time_floor_ms = s.protected_time_floor_ms.max(t);
        }
        s.state = next;
        Ok(s.clone())
    }
}
fn native(
    binding: &BftSignerBindingV3,
    snapshot: &Arc<Mutex<BftSignerBackendSnapshotV3>>,
    disabled: Option<Arc<AtomicUsize>>,
) -> (RemoteBftSignerProviderV3, thread::JoinHandle<()>) {
    let service = BftSignerServiceV3::open(
        binding.clone(),
        Box::new(Handle {
            key: key(33),
            disabled: None,
        }),
        vec![
            Box::new(Handle {
                key: key(31),
                disabled: disabled.clone(),
            }),
            Box::new(Handle {
                key: key(32),
                disabled,
            }),
        ],
        Box::new(Backend {
            binding: binding.digest().unwrap(),
            snapshot: snapshot.clone(),
        }),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address: SocketAddr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        service.serve_connection(&mut stream).unwrap();
    });
    let client = RemoteBftSignerProviderV3::new(
        address,
        Duration::from_secs(20),
        binding.clone(),
        key(34),
        BftSignerTrustV3 {
            schema_version: 3,
            provider_public_key: public(33),
            binding_digest: binding.digest().unwrap(),
        },
    )
    .unwrap();
    (client, server)
}
#[test]
fn native_typed_rotation_proofs_are_archived_before_release_and_cold_retry_never_resigns() {
    let binding = binding();
    let body = body(&binding);
    let snapshot = Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
        state: BftSignerStateV3::from_handoff(&binding).unwrap(),
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 100,
    }));
    let (provider, server) = native(&binding, &snapshot, None);
    let mut lease = provider.acquire(&binding).unwrap();
    let original = lease.checkpoint().unwrap();
    assert!(
        lease
            .sign_finality_rotation_proof(
                original.state.revision,
                &original.state.state_digest,
                &body,
                FinalityKeyRotationRoleV1::OldKeyAuthorization
            )
            .is_err()
    );
    assert_eq!(lease.checkpoint().unwrap().state, original.state);
    let enabled = lease
        .enable_membership(original.state.revision, &original.state.state_digest)
        .unwrap();
    let (old, after_old) = lease
        .sign_finality_rotation_proof(
            enabled.state.revision,
            &enabled.state.state_digest,
            &body,
            FinalityKeyRotationRoleV1::OldKeyAuthorization,
        )
        .unwrap();
    let (new, completed) = lease
        .sign_finality_rotation_proof(
            after_old.state.revision,
            &after_old.state.state_digest,
            &body,
            FinalityKeyRotationRoleV1::NewKeyPossession,
        )
        .unwrap();
    assert_eq!(
        completed
            .state
            .finality_rotation_proofs
            .as_ref()
            .unwrap()
            .records
            .len(),
        2
    );
    assert_eq!(completed.state.journal, enabled.state.journal);
    assert_eq!(completed.state.history, enabled.state.history);
    assert_eq!(
        completed.state.membership_install,
        enabled.state.membership_install
    );
    verify_proof(&body, &old, FinalityKeyRotationRoleV1::OldKeyAuthorization).unwrap();
    verify_proof(&body, &new, FinalityKeyRotationRoleV1::NewKeyPossession).unwrap();
    assert!(verify_proof(&body, &old, FinalityKeyRotationRoleV1::NewKeyPossession).is_err());
    let signature: [u8; 64] = hex::decode(&old.signature_hex).unwrap().try_into().unwrap();
    assert!(
        key(31)
            .verifying_key()
            .verify_strict(
                &body
                    .signing_bytes(FinalityKeyRotationRoleV1::NewKeyPossession)
                    .unwrap(),
                &ed25519_dalek::Signature::from_bytes(&signature)
            )
            .is_err(),
        "old consent signature cannot cross the new-possession domain even under the same verification key"
    );
    assert_eq!(
        completed
            .state
            .journal
            .as_ref()
            .unwrap()
            .local_signing_public_key(),
        public(31)
    );

    let mut corrupt = completed.state.clone();
    corrupt.finality_rotation_proofs.as_mut().unwrap().records[0]
        .proof
        .signature_hex = hash(90).repeat(2);
    assert!(validate_archive(&binding, &corrupt).is_err());
    let mut changed = body.clone();
    changed.authorization.nonce += 1;
    assert!(
        lease
            .sign_finality_rotation_proof(
                completed.state.revision,
                &completed.state.state_digest,
                &changed,
                FinalityKeyRotationRoleV1::OldKeyAuthorization
            )
            .is_err()
    );
    let mut bad = body.clone();
    bad.rotation_id = "wrong-parent".into();
    bad.authorization.parent_record_id = hash(91);
    assert!(
        lease
            .sign_finality_rotation_proof(
                completed.state.revision,
                &completed.state.state_digest,
                &bad,
                FinalityKeyRotationRoleV1::OldKeyAuthorization
            )
            .is_err()
    );
    let mut bad = body.clone();
    bad.rotation_id = "unprovisioned".into();
    bad.new_public_key_hex = public(40);
    assert!(
        lease
            .sign_finality_rotation_proof(
                completed.state.revision,
                &completed.state.state_digest,
                &bad,
                FinalityKeyRotationRoleV1::NewKeyPossession
            )
            .is_err()
    );
    let mut erased = completed.state.candidate();
    erased.finality_rotation_proofs = None;
    assert!(
        lease
            .advance(
                completed.state.revision,
                &completed.state.state_digest,
                erased
            )
            .is_err()
    );
    assert_eq!(lease.checkpoint().unwrap().state, completed.state);
    drop(lease);
    server.join().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let (provider, server) = native(&binding, &snapshot, Some(calls.clone()));
    let mut lease = provider.acquire(&binding).unwrap();
    for (role, expected) in [
        (FinalityKeyRotationRoleV1::OldKeyAuthorization, old),
        (FinalityKeyRotationRoleV1::NewKeyPossession, new),
    ] {
        let (proof, checkpoint) = lease
            .sign_finality_rotation_proof(
                enabled.state.revision,
                &enabled.state.state_digest,
                &body,
                role,
            )
            .unwrap();
        assert_eq!(proof, expected);
        assert_eq!(checkpoint.state, completed.state);
    }
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "original result must not call either unavailable key"
    );
    let mut fresh = body;
    fresh.rotation_id = "another-consent-candidate".into();
    assert!(
        lease
            .sign_finality_rotation_proof(
                completed.state.revision,
                &completed.state.state_digest,
                &fresh,
                FinalityKeyRotationRoleV1::OldKeyAuthorization
            )
            .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(lease.checkpoint().unwrap().state, completed.state);
    drop(lease);
    server.join().unwrap();
}
#[test]
fn rotation_archive_rejects_declared_65_entries_before_allocating_records() {
    let mut bytes = vec![0xa1];
    bytes.extend(serde_cbor::to_vec(&"records").unwrap());
    bytes.extend([0x98, 0x41]);
    let error = serde_cbor::from_slice::<BftFinalityRotationProofArchiveV3>(&bytes)
        .unwrap_err()
        .to_string();
    assert!(error.contains("record count exceeds bound"), "{error}");
}

//! Real authenticated TCP against a software transaction adapter.
use super::*;

pub(super) fn configured_source(
    binding: &BftSignerBindingV3,
) -> (
    BftPrivateRetentionCreationMarkerV1,
    BftPrivateRetentionAttestorV1,
) {
    let marker = BftPrivateRetentionCreationMarkerV1 {
        schema_version: 1,
        creation_identity: hash(75),
        source: BftPrivateRetentionSourceV1 {
            binding_digest: binding.digest().unwrap(),
            handoff_digest: binding.handoff_record_id.clone(),
            material_source_digest: binding.material_identity.clone(),
        },
        ledger_origin_digest: hash(76),
        activation_digest: binding.activation_digest.clone(),
        initial_state_digest: BftSignerStateV3::from_handoff(binding)
            .unwrap()
            .state_digest,
        initial_height: binding.initial_schedule().height(),
    };
    let role = BftPrivateRetentionAttestorV1 {
        schema_version: 1,
        provider_public_key: public(33),
        source: marker.source.clone(),
        creation_marker_digest: marker.digest().unwrap(),
        coverage_start_height: marker.initial_height,
        allow_authenticated_successor_origins: false,
    };
    (marker, role)
}

struct FixedSource {
    marker: BftPrivateRetentionCreationMarkerV1,
    permit: bool,
}
impl BftFreshSourceVerifierV1 for FixedSource {
    fn verify_original_runtime_source(
        &self,
        binding: &BftSignerBindingV3,
        marker: &BftPrivateRetentionCreationMarkerV1,
    ) -> Result<(), BftSignerErrorV3> {
        if !self.permit
            || self.marker != *marker
            || binding.digest()? != marker.source.binding_digest
        {
            return Err(invalid("test original source verifier rejected activation"));
        }
        Ok(())
    }
}

struct FreshProviderKey(Arc<AtomicUsize>);
impl BftSigningKeyHandleV3 for FreshProviderKey {
    fn public_key_hex(&self) -> String {
        public(33)
    }

    fn sign_preimage(&self, bytes: &[u8]) -> Result<[u8; 64], BftSignerErrorV3> {
        let activation_domain = b"oasis7.bft.private-retention.v1.head-receipt";
        if bytes
            .windows(activation_domain.len())
            .any(|part| part == activation_domain)
        {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
        Ok(key(33).sign(bytes).to_bytes())
    }
}

pub(super) fn service(
    binding: &BftSignerBindingV3,
    snapshot: &Arc<Mutex<BftSignerBackendSnapshotV3>>,
    slot: &Arc<Mutex<FreshSlot>>,
    marker: &BftPrivateRetentionCreationMarkerV1,
    role: &BftPrivateRetentionAttestorV1,
    permit: bool,
) -> Result<BftSignerServiceV3, BftSignerErrorV3> {
    BftSignerServiceV3::open_with_fresh_coverage(
        binding.clone(),
        Box::new(FreshProviderKey(
            slot.lock().unwrap().activation_signs.clone(),
        )),
        [31, 32]
            .into_iter()
            .map(|n| {
                Box::new(Handle {
                    key: key(n),
                    disabled: None,
                }) as Box<dyn BftSigningKeyHandleV3>
            })
            .collect(),
        Box::new(Backend {
            binding: binding.digest()?,
            snapshot: snapshot.clone(),
            fresh: Some(slot.clone()),
        }),
        BftFreshCoverageSupportV1 {
            attestor: role.clone(),
            marker: marker.clone(),
            verifier: Box::new(FixedSource {
                marker: marker.clone(),
                permit,
            }),
        },
    )
}

fn verify_uncovered_checkpoint_legacy_encoding(checkpoint: &BftSignerCheckpointV3) {
    let state = &checkpoint.state;
    let old_input = serde_json::json!({
        "schema_version": state.schema_version,
        "binding_digest": state.binding_digest,
        "revision": state.revision,
        "history": state.history,
        "journal": state.journal,
        "completed_journals": state.completed_journals,
        "pending_driver_apply": state.pending_driver_apply,
        "applied_installs": state.applied_installs,
    });
    use crate::protected_bft_v3::crypto::{digest_hex, verify_signature};
    assert_eq!(
        digest_hex("oasis7.bft.protected-signer.v3.state", &old_input).unwrap(),
        state.state_digest
    );
    let mut old_state = old_input;
    old_state["state_digest"] = serde_json::json!(state.state_digest);
    assert_eq!(
        oasis7_cas::to_canonical_cbor(state).unwrap(),
        oasis7_cas::to_canonical_cbor(&old_state).unwrap()
    );
    let old_body = serde_json::json!({
        "state": old_state,
        "lease_generation": checkpoint.lease_generation,
        "challenge": checkpoint.challenge,
        "request_digest": checkpoint.request_digest,
    });
    verify_signature(
        "oasis7.bft.protected-signer.v3.checkpoint",
        &old_body,
        &checkpoint.provider_signature_hex,
        &public(33),
    )
    .unwrap();
}

#[test]
fn native_fresh_retention_activates_atomically_and_cold_retry_returns_original() {
    let binding = binding();
    let (marker, role) = configured_source(&binding);
    let snapshot = Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
        state: BftSignerStateV3::from_handoff(&binding).unwrap(),
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 100,
    }));
    let slot = Arc::new(Mutex::new(FreshSlot {
        marker: marker.clone(),
        original: None,
        reservation: None,
        reservation_hold: None,
        reserved_response_bytes: 0,
        mismatched_readback: false,
        lose_commit_acknowledgement: false,
        activation_signs: Arc::new(AtomicUsize::new(0)),
    }));

    let (provider, server) = native(&binding, &snapshot, None);
    let mut lease = provider.acquire(&binding).unwrap();
    assert!(lease.provision_private_retention(&role, &marker).is_err());
    let uncovered = lease.checkpoint().unwrap();
    assert_eq!(uncovered.state.revision, 0);
    verify_uncovered_checkpoint_legacy_encoding(&uncovered);
    drop(lease);
    server.join().unwrap();

    let denied = service(&binding, &snapshot, &slot, &marker, &role, false).unwrap();
    let (provider, server) = native_service(&binding, denied);
    let mut lease = provider.acquire(&binding).unwrap();
    assert!(lease.start_height_timer(0).is_err());
    assert!(lease.provision_private_retention(&role, &marker).is_err());
    assert!(slot.lock().unwrap().original.is_none());
    drop(lease);
    server.join().unwrap();

    let writer = service(&binding, &snapshot, &slot, &marker, &role, true).unwrap();
    let (provider, server) = native_service(&binding, writer);
    let mut lease = provider.acquire(&binding).unwrap();
    let original = lease.provision_private_retention(&role, &marker).unwrap();
    let reply = serde_json::to_value(
        crate::protected_bft_v3::wire::BftSignerReplyKindV3::PrivateRetention {
            ledger: Box::new(
                crate::protected_bft_v3::wire::BftRetentionReplyLedgerV1::Activation(
                    original.clone(),
                ),
            ),
            latest_seal: None,
            reserved_response_bytes: slot.lock().unwrap().reserved_response_bytes,
        },
    )
    .unwrap();
    assert!(reply["body"].get("latest_seal").is_none());
    assert!(reply["body"]["ledger"].get("selection_index").is_none());
    let activated = lease.checkpoint().unwrap();
    assert_eq!(activated.state.revision, 1);
    assert_eq!(
        activated.state.private_retention.as_ref(),
        Some(&original.receipt.body.head)
    );
    assert_eq!(
        original.receipt.body.original_lease_generation,
        activated.lease_generation
    );
    original
        .verify(
            &binding,
            &role,
            &marker,
            &activated.state,
            activated.lease_generation,
        )
        .unwrap();
    let old_spend = {
        let persisted = slot.lock().unwrap();
        assert_eq!(persisted.original.as_ref(), Some(&original));
        assert!(persisted.reserved_response_bytes > 0);
        persisted.reserved_response_bytes
    };
    assert!(lease.start_height_timer(1).is_err());
    drop(lease);
    server.join().unwrap();

    // The original live verifier is now unavailable. Readback relies on the
    // committed receipt and independently pinned marker across the new lease.
    let reader = service(&binding, &snapshot, &slot, &marker, &role, false).unwrap();
    let (provider, server) = native_service(&binding, reader);
    let mut lease = provider.acquire(&binding).unwrap();
    let recovered = lease.provision_private_retention(&role, &marker).unwrap();
    assert_eq!(recovered, original);
    let current = lease.checkpoint().unwrap();
    assert!(current.lease_generation > original.receipt.body.original_lease_generation);
    assert_eq!(current.state, activated.state);
    let persisted = slot.lock().unwrap();
    assert!(persisted.reserved_response_bytes > old_spend);
    assert_eq!(persisted.activation_signs.load(Ordering::SeqCst), 1);
    drop(persisted);
    drop(lease);
    server.join().unwrap();
}

#[test]
fn native_fresh_retention_mismatched_atomic_readback_never_releases_receipt() {
    let binding = binding();
    let (marker, role) = configured_source(&binding);
    let snapshot = Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
        state: BftSignerStateV3::from_handoff(&binding).unwrap(),
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 100,
    }));
    let slot = Arc::new(Mutex::new(FreshSlot {
        marker: marker.clone(),
        original: None,
        reservation: None,
        reservation_hold: None,
        reserved_response_bytes: 0,
        mismatched_readback: true,
        lose_commit_acknowledgement: false,
        activation_signs: Arc::new(AtomicUsize::new(0)),
    }));
    let writer = service(&binding, &snapshot, &slot, &marker, &role, true).unwrap();
    let (provider, server) = native_service_expected(&binding, writer, true);
    let mut lease = provider.acquire(&binding).unwrap();
    let failure = lease
        .provision_private_retention(&role, &marker)
        .unwrap_err();
    assert!(failure.to_string().contains("commit/readback uncertain"));
    assert!(lease.checkpoint().is_err());
    drop(lease);
    server.join().unwrap();

    let persisted = slot.lock().unwrap();
    let original = persisted.original.as_ref().unwrap();
    let after = snapshot.lock().unwrap();
    assert_eq!(
        after.state.private_retention.as_ref(),
        Some(&original.receipt.body.head)
    );
    assert!(!after.lease_token_digest.is_empty());
    assert!(persisted.reserved_response_bytes > 0);
}

#[test]
fn native_fresh_retention_lost_commit_acknowledgement_recovers_only_after_fence() {
    let binding = binding();
    let (marker, role) = configured_source(&binding);
    let snapshot = Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
        state: BftSignerStateV3::from_handoff(&binding).unwrap(),
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 100,
    }));
    let slot = Arc::new(Mutex::new(FreshSlot {
        marker: marker.clone(),
        original: None,
        reservation: None,
        reservation_hold: None,
        reserved_response_bytes: 0,
        mismatched_readback: false,
        lose_commit_acknowledgement: true,
        activation_signs: Arc::new(AtomicUsize::new(0)),
    }));
    let writer = service(&binding, &snapshot, &slot, &marker, &role, true).unwrap();
    let (provider, server) = native_service_expected(&binding, writer, true);
    let mut lease = provider.acquire(&binding).unwrap();
    let floor = lease.protected_time_ms().unwrap();
    assert_eq!(floor, 101);
    assert!(lease.provision_private_retention(&role, &marker).is_err());
    drop(lease);
    server.join().unwrap();

    let fenced = snapshot.lock().unwrap().clone();
    assert_eq!(fenced.protected_time_floor_ms, floor);
    assert!(!fenced.lease_token_digest.is_empty());
    let original = slot.lock().unwrap().original.clone().unwrap();

    let reader = service(&binding, &snapshot, &slot, &marker, &role, false).unwrap();
    let (provider, server) = native_service(&binding, reader);
    assert!(provider.acquire(&binding).is_err());
    server.join().unwrap();
    assert_eq!(*snapshot.lock().unwrap(), fenced);

    // The lost writer has returned. The software adapter positively fences
    // that exact old owner before the new service can recover its receipt.
    let released = Backend {
        binding: binding.digest().unwrap(),
        snapshot: snapshot.clone(),
        fresh: Some(slot.clone()),
    }
    .release_lease(fenced.lease_generation, &fenced.lease_token_digest)
    .unwrap();
    assert_eq!(released.lease_generation, fenced.lease_generation + 1);
    assert!(released.lease_token_digest.is_empty());

    let reader = service(&binding, &snapshot, &slot, &marker, &role, false).unwrap();
    let (provider, server) = native_service(&binding, reader);
    let mut lease = provider.acquire(&binding).unwrap();
    assert_eq!(
        lease.provision_private_retention(&role, &marker).unwrap(),
        original
    );
    let checkpoint = lease.checkpoint().unwrap();
    assert!(checkpoint.lease_generation > original.receipt.body.original_lease_generation);
    assert_eq!(checkpoint.state, fenced.state);
    assert_eq!(snapshot.lock().unwrap().protected_time_floor_ms, floor);
    assert_eq!(
        slot.lock().unwrap().activation_signs.load(Ordering::SeqCst),
        1
    );
    drop(lease);
    server.join().unwrap();
}

#[path = "first_reservation_tests.rs"]
mod first_reservation_tests;

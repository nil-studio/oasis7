use super::*;
fn h(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}
fn source() -> BftSuccessorWindowSourceV1 {
    BftSuccessorWindowSourceV1 {
        scope: BftScopeV2 {
            world_id: "world".into(),
            chain_id: "chain".into(),
            genesis_hash: h(1),
        },
        binding_digest: h(2),
        handoff_position: 2,
        handoff_record_id: h(3),
        handoff_snapshot_root: h(4),
        target_digest: h(5),
        activation_digest: h(6),
        material_identity: h(7),
    }
}
fn reference(n: u8) -> BftSuccessorWindowArchiveRefV1 {
    BftSuccessorWindowArchiveRefV1 {
        digest: h(n),
        bytes: 1,
    }
}
fn body() -> BftSuccessorWindowCheckpointBodyV1 {
    let source = source();
    BftSuccessorWindowCheckpointBodyV1 {
        schema_version: 1,
        predecessor_checkpoint_digest: source.origin_digest().unwrap(),
        source,
        request_id: "original-request".into(),
        sequence: 1,
        cut: BftSuccessorWindowCutV1 {
            height: 34,
            header_id: h(8),
            certificate_digest: h(9),
            execution_commitment_digest: h(10),
            snapshot_ref: reference(11),
            journal_ref: reference(12),
            semantic_state_root: h(13),
            post_manifest_hash: h(14),
            business_result_root: h(15),
            driver_record_digest: h(16),
            driver_install_digest: h(17),
        },
        // A well-formed canonical empty map is only shape data, never a real
        // schedule. bind_verified_state must compare genuine opaque bytes.
        successor_schedule_cbor: vec![0xd9, 0xd9, 0xf7, 0xa0],
        successor_schedule_digest: h(18),
        current_journal_ref: None,
        current_journal_digest: None,
        public_archive_root: h(19),
        execution_archive_root: h(20),
        evidence_refs: vec![],
        usage: BftSuccessorWindowArchiveUsageV1 {
            segments: 1,
            objects: 2,
            total_bytes: 2,
        },
    }
}
#[test]
fn cumulative_budget_does_not_reset_at_a_window_and_overflow_never_wraps() {
    let full = BftSuccessorWindowArchiveUsageV1 {
        segments: 64,
        objects: 1,
        total_bytes: 4 * 1024 * 1024 * 1024,
    };
    full.validate().unwrap();
    assert!(
        full.checked_add(BftSuccessorWindowArchiveUsageV1 {
            segments: 1,
            objects: 1,
            total_bytes: 1
        })
        .is_err()
    );
    assert!(
        BftSuccessorWindowArchiveUsageV1 {
            segments: u64::MAX,
            objects: 0,
            total_bytes: 0
        }
        .validate()
        .is_err()
    );
    assert!(
        BftSuccessorWindowArchiveUsageV1 {
            segments: 1,
            objects: 1,
            total_bytes: u64::MAX
        }
        .validate()
        .is_err()
    );
    assert!(
        BftSuccessorWindowArchiveUsageV1 {
            segments: 1,
            objects: 1,
            total_bytes: 1
        }
        .validate_after(&full)
        .is_err()
    );
}
#[test]
fn original_h_request_schedule_journal_and_manifest_are_digest_bound() {
    let original = body();
    original.validate().unwrap();
    let digest = original.digest().unwrap();
    let mut variants = Vec::new();
    let mut changed = original.clone();
    changed.request_id.push('x');
    variants.push(changed);
    let mut changed = original.clone();
    changed.successor_schedule_cbor.push(0);
    variants.push(changed);
    let mut changed = original.clone();
    changed.cut.journal_ref.digest = h(30);
    variants.push(changed);
    let mut changed = original.clone();
    changed.public_archive_root = h(31);
    variants.push(changed);
    for changed in variants {
        assert_ne!(changed.digest().unwrap(), digest);
    }
    let mut changed = original.clone();
    changed.source.scope.genesis_hash = h(30);
    assert!(
        changed.validate().is_err(),
        "first predecessor must retain original H source"
    );
    let mut changed = original.clone();
    changed.current_journal_ref = Some(reference(30));
    assert!(
        changed.validate().is_err(),
        "journal ref/digest must appear together"
    );
    let mut changed = original;
    changed.current_journal_digest = Some(h(30));
    assert!(changed.validate().is_err());
}
#[test]
fn strict_hashes_canonical_roundtrip_and_domain_prefix_are_explicit() {
    for value in ["A".repeat(64), "0".repeat(63), "g".repeat(64)] {
        assert!(hash(&value).is_err());
    }
    let original = body();
    let bytes = encode_bounded(&original, BFT_SUCCESSOR_WINDOW_METADATA_MAX_BYTES_V1).unwrap();
    assert_eq!(
        BftSuccessorWindowCheckpointBodyV1::decode_canonical(&bytes).unwrap(),
        original
    );
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(BftSuccessorWindowCheckpointBodyV1::decode_canonical(&trailing).is_err());
    assert!(BftSuccessorWindowCheckpointBodyV1::decode_canonical(&[0xbf, 0xff]).is_err());
    let preimage = original.signing_bytes().unwrap();
    let expected_domain = b"oasis7.bft.successor-window.v1.checkpoint";
    assert_eq!(
        &preimage[..8],
        &(expected_domain.len() as u64).to_be_bytes()
    );
    assert_eq!(&preimage[8..8 + expected_domain.len()], expected_domain);
    assert_ne!(
        original.digest().unwrap(),
        digest_hex(BFT_SUCCESSOR_WINDOW_ARCHIVE_DOMAIN_V1, &original).unwrap()
    );
    let mut huge = original;
    huge.successor_schedule_cbor = vec![0; BFT_SUCCESSOR_WINDOW_METADATA_MAX_BYTES_V1 + 1];
    assert!(huge.validate().is_err());
}
#[test]
fn minimum_rejects_same_height_forks_and_skipped_sequences() {
    let original = body().minimum().unwrap();
    original.validate_after(&original).unwrap();
    let mut fork = original.clone();
    fork.header_id = h(22);
    assert!(fork.validate_after(&original).is_err());
    let mut gap = original.clone();
    gap.sequence += 2;
    gap.height += 1;
    assert!(gap.validate_after(&original).is_err());
    let mut next = original.clone();
    next.sequence += 1;
    next.height += 1;
    next.header_id = h(23);
    next.checkpoint_digest = h(24);
    next.validate_after(&original).unwrap();
    let mut foreign = next;
    foreign.source.scope.world_id = "foreign".into();
    assert!(foreign.validate_after(&original).is_err());
}
#[test]
fn object_membership_rejects_changed_bytes_duplicate_entries_and_origin_conflicts() {
    let bytes = b"original archive object";
    let object = BftSuccessorWindowArchiveRefV1 {
        digest: blake3::hash(bytes).to_hex().to_string(),
        bytes: bytes.len() as u64,
    };
    object.validate_bytes(bytes).unwrap();
    assert!(object.validate_bytes(b"changed archive object").is_err());
    let mut manifest = BftSuccessorWindowArchiveManifestV1 {
        schema_version: 1,
        source: source(),
        kind: BftSuccessorWindowArchiveKindV1::Public,
        segment_sequence: 1,
        predecessor_root: None,
        first_height: 3,
        last_height: 34,
        entries: vec![BftSuccessorWindowArchiveEntryV1 {
            identity_digest: h(5),
            object,
        }],
        usage: BftSuccessorWindowArchiveUsageV1 {
            segments: 1,
            objects: 1,
            total_bytes: bytes.len() as u64,
        },
    };
    manifest.validate().unwrap();
    manifest.entries.push(manifest.entries[0].clone());
    assert!(manifest.validate().is_err());
    manifest.entries.pop();
    manifest.predecessor_root = Some(h(30));
    assert!(manifest.validate().is_err());
}
#[test]
fn prepared_cas_consumes_a_revision_before_commit() {
    let intent = BftSuccessorWindowInstallIntentBodyV1 {
        schema_version: 1,
        checkpoint: body(),
        expected_lease_generation: 7,
        expected_revision: 11,
        expected_state_digest: h(23),
        driver_evidence_digest: h(24),
    };
    assert_eq!(intent.minimum_prepared_revision().unwrap(), 12);
    let mut overflow = intent;
    overflow.expected_revision = u64::MAX - 1;
    assert!(overflow.validate().is_err());
}

struct ProviderKey(ed25519_dalek::SigningKey);
impl BftSigningKeyHandleV3 for ProviderKey {
    fn public_key_hex(&self) -> String {
        hex::encode(self.0.verifying_key().to_bytes())
    }
    fn sign_preimage(&self, bytes: &[u8]) -> Result<[u8; 64], BftSignerErrorV3> {
        use ed25519_dalek::Signer;
        Ok(self.0.sign(bytes).to_bytes())
    }
}
#[test]
fn signed_original_receipt_retains_historical_generation_and_rejects_domain_or_body_tamper() {
    let key = ProviderKey(ed25519_dalek::SigningKey::from_bytes(&[7; 32]));
    let intent = BftSuccessorWindowInstallIntentV1::sign(
        BftSuccessorWindowInstallIntentBodyV1 {
            schema_version: 1,
            checkpoint: body(),
            expected_lease_generation: 7,
            expected_revision: 11,
            expected_state_digest: h(23),
            driver_evidence_digest: h(24),
        },
        &key,
    )
    .unwrap();
    intent.verify_signature(&key.public_key_hex()).unwrap();
    // These signatures bind shape fixtures, not a qualified execution token.
    let receipt = BftSuccessorWindowInstallReceiptV1::sign(
        BftSuccessorWindowInstallReceiptBodyV1 {
            schema_version: 1,
            source: intent.body.checkpoint.source.clone(),
            request_id: "original-request".into(),
            checkpoint_digest: intent.body.checkpoint.digest().unwrap(),
            intent_digest: intent.digest().unwrap(),
            minimum: intent.body.checkpoint.minimum().unwrap(),
            lease_generation: 9,
            committed_revision: 13,
            driver_install_digest: h(17),
        },
        &key,
    )
    .unwrap();
    receipt.verify_for(&intent, &key.public_key_hex()).unwrap();
    assert_eq!(
        receipt.body.lease_generation, 9,
        "receipt is a historical commit fact, not the querier's lease"
    );
    let mut stale = receipt.clone();
    stale.body.committed_revision = 12;
    assert!(stale.verify_for(&intent, &key.public_key_hex()).is_err());
    let mut wrong_domain = receipt.clone();
    wrong_domain.provider_signature_hex = sign_preimage(
        &key,
        BFT_SUCCESSOR_WINDOW_INTENT_DOMAIN_V1,
        &wrong_domain.body,
    )
    .unwrap();
    assert!(
        wrong_domain
            .verify_for(&intent, &key.public_key_hex())
            .is_err()
    );
    let mut tampered = intent.clone();
    tampered.body.checkpoint.request_id = "different-request".into();
    assert!(
        receipt
            .verify_for(&tampered, &key.public_key_hex())
            .is_err()
    );
    let mut request = BftSuccessorWindowPersistedRequestV1 {
        intent,
        status: BftSuccessorWindowStatusV1::Prepared,
        receipt: None,
        adopted_evidence: None,
        superseded_reason: None,
    };
    request.validate(&key.public_key_hex()).unwrap();
    request.status = BftSuccessorWindowStatusV1::InstalledPending;
    assert!(request.validate(&key.public_key_hex()).is_err());
    request.receipt = Some(receipt);
    request.validate(&key.public_key_hex()).unwrap();
    request.status = BftSuccessorWindowStatusV1::Adopted;
    assert!(
        request.validate(&key.public_key_hex()).is_err(),
        "cannot adopt without readback evidence"
    );
    request.status = BftSuccessorWindowStatusV1::Superseded;
    request.superseded_reason = Some("known-conflict".into());
    assert!(
        request.validate(&key.public_key_hex()).is_err(),
        "committed minimum cannot be marked uncommitted"
    );
}

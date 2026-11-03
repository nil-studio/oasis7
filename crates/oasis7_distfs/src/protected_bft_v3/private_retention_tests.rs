use super::private_retention::*;
use super::private_retention_backend::*;
use super::*;
use ed25519_dalek::{Signer, SigningKey};

fn source() -> BftPrivateRetentionSourceV1 {
    BftPrivateRetentionSourceV1 {
        binding_digest: "01".repeat(32),
        handoff_digest: "02".repeat(32),
        material_source_digest: "03".repeat(32),
    }
}

pub(super) fn input_for(
    source: &BftPrivateRetentionSourceV1,
    key: &BftPrivateRetentionKeyV1,
) -> std::sync::Arc<BftPrivateRetentionAdmissionInputV1> {
    let mut objects = [
        (
            BftPrivateRetentionInputObjectKindV1::OriginalData,
            b"original-private-data".as_slice(),
        ),
        (
            BftPrivateRetentionInputObjectKindV1::PublicationMetadata,
            b"original-publication-metadata".as_slice(),
        ),
    ]
    .into_iter()
    .map(|(kind, bytes)| BftPrivateRetentionInputObjectV1 {
        kind,
        reference: BftSuccessorWindowArchiveRefV1 {
            digest: blake3::hash(bytes).to_hex().to_string(),
            bytes: bytes.len() as u64,
        },
        bytes: bytes.to_vec(),
    })
    .collect::<Vec<_>>();
    objects.sort_by(|a, b| a.reference.digest.cmp(&b.reference.digest));
    std::sync::Arc::new(BftPrivateRetentionAdmissionInputV1 {
        schema_version: 1,
        source: source.clone(),
        key: key.clone(),
        publication_reservation_bytes: 123,
        objects,
    })
}
pub(super) fn fixture_inputs(
    map: &BftPrivateRetentionMapV1,
) -> BftPrivateRetentionInputInventoryV1 {
    map.entries
        .iter()
        .map(|entry| {
            let input = input_for(&map.source, &entry.key);
            (input.admission_digest().unwrap(), input)
        })
        .collect()
}

pub(super) fn fixture() -> (
    BftPrivateRetentionAttestorV1,
    BftPrivateRetentionMapV1,
    BftPrivateRetentionReceiptV1,
) {
    let key = SigningKey::from_bytes(&[71; 32]);
    let role = BftPrivateRetentionAttestorV1 {
        schema_version: 1,
        provider_public_key: hex::encode(key.verifying_key().to_bytes()),
        source: source(),
        creation_marker_digest: "0c".repeat(32),
        coverage_start_height: 1,
        allow_authenticated_successor_origins: true,
    };
    let mut map = BftPrivateRetentionMapV1 {
        schema_version: 1,
        source: source(),
        coverage_start_height: 1,
        previous_seal_receipt_digest: None,
        entries: vec![BftPrivateRetentionEntryV1 {
            key: BftPrivateRetentionKeyV1 {
                ledger_origin_digest: "04".repeat(32),
                height: 1,
                header_id: "05".repeat(32),
            },
            fact: BftPrivateRetentionFactV1::Some {
                original_data_ref: "06".repeat(32),
                metadata_ref: "07".repeat(32),
                admission_digest: "08".repeat(32),
                input_ref: None,
            },
            lifecycle: BftPrivateRetentionLifecycleV1::ReservedPending,
        }],
    };
    map.entries[0].fact = input_for(&map.source, &map.entries[0].key).fact().unwrap();
    let mut head = BftPrivateRetentionHeadV1 {
            selection_commitment: None,
        schema_version: 2,
        source: source(),
        capability_digest: role.digest().unwrap(),
        sequence: 1,
        map_digest: map.digest().unwrap(),
        map_byte_hash: blake3::hash(&map.canonical_bytes().unwrap())
            .to_hex()
            .to_string(),
        entry_count: 1,
        previous_receipt_digest: None,
        retained_material_bytes: None,
    };
    head.retained_material_bytes = Some(
        super::private_retention_backend::retention_material_bound(
            &snapshot(10, "0a".repeat(32)),
            None,
            &map,
            &head,
            &fixture_inputs(&map),
        )
        .unwrap(),
    );
    let after_state = snapshot(10, "0a".repeat(32))
        .state
        .with_private_retention_head(head.clone())
        .unwrap();
    let body = BftPrivateRetentionReceiptBodyV1 {
        head,
        operation_digest: "09".repeat(32),
        original_lease_generation: 7,
        before_revision: 10,
        before_state_digest: "0a".repeat(32),
        after_revision: 11,
        after_state_digest: after_state.state_digest,
    };
    let bytes = super::crypto::framed_cbor(
        RECEIPT_DOMAIN,
        &body,
        super::crypto::BFT_SIGNER_MAX_SIGNING_BYTES_V3,
    )
    .unwrap();
    let receipt = BftPrivateRetentionReceiptV1 {
        body,
        signature_hex: hex::encode(key.sign(&bytes).to_bytes()),
    };
    (role, map, receipt)
}

#[test]
fn private_retention_original_receipt_rejects_omission_downgrade_and_self_selected_role() {
    let (role, map, receipt) = fixture();
    receipt.verify_map(&role, &map).unwrap();
    let mut omitted = map.clone();
    omitted.entries.clear();
    assert!(receipt.verify_map(&role, &omitted).is_err());
    let mut downgraded = map.clone();
    downgraded.entries[0].fact = BftPrivateRetentionFactV1::ExplicitLegacyNone;
    assert!(receipt.verify_map(&role, &downgraded).is_err());
    let mut foreign = role.clone();
    foreign.source.material_source_digest = "aa".repeat(32);
    assert!(receipt.verify_map(&foreign, &map).is_err());
    foreign = role;
    foreign.provider_public_key =
        hex::encode(SigningKey::from_bytes(&[72; 32]).verifying_key().to_bytes());
    assert!(receipt.verify_map(&foreign, &map).is_err());
}

#[test]
fn private_retention_map_rejects_noncanonical_duplicate_and_oversized_inventory() {
    let (_, map, _) = fixture();
    let bytes = map.canonical_bytes().unwrap();
    assert_eq!(
        BftPrivateRetentionMapV1::from_canonical_bytes(&bytes).unwrap(),
        map
    );
    let ordinary = serde_cbor::to_vec(&map).unwrap();
    assert_ne!(bytes, ordinary);
    assert!(BftPrivateRetentionMapV1::from_canonical_bytes(&ordinary).is_err());
    let mut duplicate = map;
    duplicate.entries.push(duplicate.entries[0].clone());
    assert!(duplicate.validate().is_err());
    // Declared Array(257), without a first entry: reject before allocation/read.
    let mut declared = vec![0xa1];
    declared.extend(serde_cbor::to_vec(&"entries").unwrap());
    declared.extend([0x99, 0x01, 0x01]);
    let error = serde_cbor::from_slice::<BftPrivateRetentionMapV1>(&declared).unwrap_err();
    assert!(error.to_string().contains("retention entry bound"));
}

pub(super) fn snapshot(revision: u64, digest: String) -> BftSignerBackendSnapshotV3 {
    let state = serde_json::from_value(serde_json::json!({
        "schema_version":3,"binding_digest":source().binding_digest,"revision":revision,
        "history":[],"journal":null,"completed_journals":[],"pending_driver_apply":null,
        "applied_installs":[],"state_digest":digest,
    }))
    .unwrap();
    BftSignerBackendSnapshotV3 {
        state,
        successor_archive_transfer: None,
        lease_generation: 7,
        lease_token_digest: "ff".repeat(32),
        protected_time_floor_ms: 123,
    }
}

#[test]
fn private_retention_atomic_readback_checks_map_receipt_state_and_preserved_fields() {
    let (role, map, receipt) = fixture();
    let request = BftPrivateRetentionCasV1 {
        before: snapshot(10, "0a".repeat(32)),
        expected_retention: None,
        after: BftPrivateRetentionSnapshotV1 {
            inputs: fixture_inputs(&map),
            signer: {
                let mut snapshot = snapshot(10, "0a".repeat(32));
                snapshot.state = snapshot
                    .state
                    .with_private_retention_head(receipt.body.head.clone())
                    .unwrap();
                snapshot
            },
            retention: BftPrivateRetentionLedgerV1 {
            selection_index: None, map, receipt },
        },
    };
    request.validate_envelope(&role).unwrap();
    request.validate_exact_readback(&request.after).unwrap();
    for field in 0..5 {
        let mut changed = request.after.clone();
        match field {
            0 => changed.retention.map.entries.clear(),
            1 => changed.retention.receipt.body.operation_digest = "cc".repeat(32),
            2 => changed.signer.state.revision += 1,
            3 => changed.signer.protected_time_floor_ms += 1,
            _ => changed.signer.lease_generation += 1,
        }
        assert!(request.validate_exact_readback(&changed).is_err());
    }
}

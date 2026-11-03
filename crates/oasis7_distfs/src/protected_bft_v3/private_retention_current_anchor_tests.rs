use super::super::private_retention::{BftPrivateRetentionLifecycleV1, RECEIPT_DOMAIN};
use super::super::private_retention_backend::BftPrivateRetentionLedgerV1;
use super::super::private_retention_seal::{
    BftPrivateRetentionSealBodyV1, BftPrivateRetentionSealV1,
};
use super::super::successor_window::BftSuccessorWindowArchiveRefV1;
use super::*;
use ed25519_dalek::{Signer, SigningKey};
fn sign_preimage<T: serde::Serialize>(
    key: &SigningKey,
    domain: &str,
    body: &T,
) -> Result<String, BftSignerErrorV3> {
    let bytes = super::super::crypto::framed_cbor(
        domain,
        body,
        super::super::crypto::BFT_SIGNER_MAX_SIGNING_BYTES_V3,
    )?;
    Ok(hex::encode(key.sign(&bytes).to_bytes()))
}

fn fixture() -> (
    BftPrivateRetentionAttestorV1,
    BftPrivateRetentionSealV1,
    BftPrivateRetentionLedgerV1,
) {
    let (role, mut map, mut receipt) = super::super::private_retention_tests::fixture();
    let key = SigningKey::from_bytes(&[71; 32]);
    map.entries[0].lifecycle = BftPrivateRetentionLifecycleV1::Ready;
    let mut unsigned = map.entries[0].clone();
    unsigned.key.height = 2;
    unsigned.fact = super::super::private_retention_tests::input_for(&map.source, &unsigned.key)
        .fact()
        .unwrap();
    map.entries.push(unsigned);
    receipt.body.head.map_digest = map.digest().unwrap();
    receipt.body.head.map_byte_hash = blake3::hash(&map.canonical_bytes().unwrap())
        .to_hex()
        .to_string();
    receipt.body.head.entry_count = 2;
    receipt.signature_hex = sign_preimage(&key, RECEIPT_DOMAIN, &receipt.body).unwrap();
    let hash = "ab".repeat(32);
    let cut = BftSuccessorWindowCutV1 {
        height: 1,
        header_id: hash.clone(),
        certificate_digest: hash.clone(),
        execution_commitment_digest: hash.clone(),
        snapshot_ref: BftSuccessorWindowArchiveRefV1 {
            digest: hash.clone(),
            bytes: 1,
        },
        journal_ref: BftSuccessorWindowArchiveRefV1 {
            digest: hash.clone(),
            bytes: 1,
        },
        semantic_state_root: hash.clone(),
        post_manifest_hash: hash.clone(),
        business_result_root: hash.clone(),
        driver_record_digest: hash.clone(),
        driver_install_digest: hash,
    };
    let body = BftPrivateRetentionSealBodyV1::from_original(
        BftPrivateRetentionLedgerV1 {
            selection_index: None,
            map,
            receipt,
        },
        cut,
    )
    .unwrap();
    let anchor_body = BftPrivateRetentionSealAnchorBodyV1::from_original(&body).unwrap();
    let seal = BftPrivateRetentionSealV1 {
        signature_hex: sign_preimage(&key, SEAL_ANCHOR_DOMAIN, &anchor_body).unwrap(),
        body,
    };
    let map = seal.body.carry_map().unwrap();
    let mut receipt = seal.body.original.receipt.clone();
    receipt.body.head.sequence += 1;
    receipt.body.head.previous_receipt_digest = Some(
        super::super::crypto::digest_hex(
            "oasis7.bft.private-retention.v1.receipt",
            &seal.body.original.receipt,
        )
        .unwrap(),
    );
    receipt.body.head.map_digest = map.digest().unwrap();
    receipt.body.head.map_byte_hash = blake3::hash(&map.canonical_bytes().unwrap())
        .to_hex()
        .to_string();
    receipt.body.head.entry_count = map.entries.len() as u32;
    receipt.body.before_revision = receipt.body.after_revision;
    receipt.body.after_revision += 1;
    receipt.signature_hex = sign_preimage(&key, RECEIPT_DOMAIN, &receipt.body).unwrap();
    (
        role,
        seal,
        BftPrivateRetentionLedgerV1 {
            selection_index: None,
            map,
            receipt,
        },
    )
}

#[test]
fn compact_current_anchor_authenticates_new_floor_without_relaxing_ordinary_map() {
    let (role, seal, ledger) = fixture();
    let anchor = seal.anchor().unwrap();
    assert!(encode_bounded(&anchor, MAX_ANCHOR).unwrap().len() < MAX_ANCHOR);
    verify_current_anchor(&role, &ledger, Some(&anchor)).unwrap();
    assert!(ledger.receipt.verify_map(&role, &ledger.map).is_err());
    assert!(verify_current_anchor(&role, &ledger, None).is_err());
    let mut other_role = role.clone();
    other_role.creation_marker_digest = "ff".repeat(32);
    assert!(verify_current_anchor(&other_role, &ledger, Some(&anchor)).is_err());
}
#[test]
fn compact_current_anchor_rejects_signed_carry_omission() {
    let (role, seal, mut ledger) = fixture();
    ledger.map.entries.clear();
    ledger.receipt.body.head.map_digest = ledger.map.digest().unwrap();
    ledger.receipt.body.head.map_byte_hash = blake3::hash(&ledger.map.canonical_bytes().unwrap())
        .to_hex()
        .to_string();
    ledger.receipt.body.head.entry_count = 0;
    ledger.receipt.signature_hex = sign_preimage(
        &SigningKey::from_bytes(&[71; 32]),
        RECEIPT_DOMAIN,
        &ledger.receipt.body,
    )
    .unwrap();
    let error = verify_current_anchor(&role, &ledger, Some(&seal.anchor().unwrap())).unwrap_err();
    assert!(
        error.to_string().contains("omitted original carry"),
        "{error}"
    );
}
#[test]
fn full_seal_signature_rejects_rehashed_original_t_plus_one_omission() {
    let (role, mut seal, _) = fixture();
    seal.verify(&role).unwrap();
    seal.body.original.map.entries.pop();
    seal.body.carry_keys.clear();
    // The full seal retains a provider-signed digest of the original complete
    // body. Rehashing caller-owned map bytes does not produce another seal.
    let map = &seal.body.original.map;
    seal.body.original.receipt.body.head.map_digest = map.digest().unwrap();
    seal.body.original.receipt.body.head.map_byte_hash =
        blake3::hash(&map.canonical_bytes().unwrap())
            .to_hex()
            .to_string();
    seal.body.original.receipt.body.head.entry_count = 1;
    // Make the replacement receipt independently valid so this regression
    // specifically exercises the unchanged original seal signature.
    seal.body.original.receipt.signature_hex = sign_preimage(
        &SigningKey::from_bytes(&[71; 32]),
        RECEIPT_DOMAIN,
        &seal.body.original.receipt.body,
    )
    .unwrap();
    assert!(seal.verify(&role).is_err());
}

#[test]
fn compact_anchor_maximum_carry_fits_existing_bounded_metadata() {
    let (role, seal, _) = fixture();
    let mut body = seal.anchor().unwrap().body;
    let original = body.carry_entries[0].clone();
    body.carry_entries = (0..BFT_PRIVATE_RETENTION_MAX_ALTERNATIVES_V1)
        .map(|index| {
            let mut entry = original.clone();
            entry.key.header_id = format!("{index:064x}");
            entry
        })
        .collect();
    body.original_head.entry_count = body.carry_entries.len() as u32 + 1;
    let anchor = BftPrivateRetentionSealAnchorV1 {
        signature_hex: sign_preimage(
            &SigningKey::from_bytes(&[71; 32]),
            SEAL_ANCHOR_DOMAIN,
            &body,
        )
        .unwrap(),
        body,
    };
    anchor.verify(&role).unwrap();
    assert!(encode_bounded(&anchor, MAX_ANCHOR).unwrap().len() < MAX_ANCHOR);
    let mut excess = anchor;
    let mut seventeenth = original;
    seventeenth.key.header_id = "ff".repeat(32);
    excess.body.carry_entries.push(seventeenth);
    excess.body.original_head.entry_count += 1;
    assert!(excess.body.validate_shape().is_err());
    let bytes = encode_bounded(&excess, MAX_ANCHOR).unwrap();
    assert!(
        super::super::crypto::decode_bounded::<BftPrivateRetentionSealAnchorV1>(&bytes, MAX_ANCHOR)
            .is_err()
    );
}

#[test]
fn private_retention_sealed_reopen_chain_uses_original_role_and_signed_anchor() {
    // Cryptographic chain component fixture, not an applied World/Seal service
    // transaction. Actual service reopen is covered by the Active integration.
    let (role, seal, ledger) = fixture();
    let verify =
        |role: &BftPrivateRetentionAttestorV1, seals, ledger: &BftPrivateRetentionLedgerV1| {
            super::super::private_retention_seal::ValidatedBftPrivateRetentionSealChainV1::verify(
                role, seals, ledger,
            )
        };
    verify(&role, vec![std::sync::Arc::new(seal.clone())], &ledger).unwrap();
    assert!(
        verify(&role, vec![], &ledger).is_err(),
        "missing seal cannot authorize advanced floor"
    );
    let mut foreign = role.clone();
    foreign.source.binding_digest = "fe".repeat(32);
    assert!(verify(&foreign, vec![std::sync::Arc::new(seal.clone())], &ledger).is_err());
    let mut changed = ledger.clone();
    changed.map.previous_seal_receipt_digest = None;
    assert!(
        changed.receipt.verify_map(&role, &changed.map).is_err(),
        "deleting anchor does not select an authenticated unsealed map"
    );
    assert!(verify(&role, vec![std::sync::Arc::new(seal.clone())], &changed).is_err());
    let mut bad_seal = seal.clone();
    bad_seal.signature_hex = "00".repeat(64);
    assert!(verify(&role, vec![std::sync::Arc::new(bad_seal)], &ledger).is_err());
    assert_eq!(
        role.coverage_start_height, 1,
        "original role is never rewritten"
    );
    assert_eq!(ledger.map.coverage_start_height, 2);
}

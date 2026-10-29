use super::*;
use crate::world_action::{Action, governance_validator_admission_auth_required};
use std::collections::BTreeMap;
fn body() -> BootstrapFinalityKeyRotationBodyV1 {
    BootstrapFinalityKeyRotationBodyV1 {
        schema_version: 1,
        authorization: GovernanceAuthorizationV1 {
            schema_version: 1,
            world_id: "w".into(),
            chain_id: "c".into(),
            genesis_digest: "0".repeat(64),
            parent_position: 1,
            parent_record_id: "1".repeat(64),
            parent_state_root: "2".repeat(64),
            policy_digest: "3".repeat(64),
            controller_account_id: "a".into(),
            nonce: 1,
            valid_from_position: 2,
            valid_through_position: 3,
            signatures: BTreeMap::new(),
        },
        rotation_id: "r".into(),
        bootstrap_registry_digest_hex: "a".repeat(64),
        bootstrap_public_key_hex: "b".repeat(64),
        node_id: "n".into(),
        expected_generation: 1,
        old_public_key_hex: "4".repeat(64),
        new_public_key_hex: "5".repeat(64),
        decision_height: 2,
        activation_height: 4,
        target_epoch: 2,
    }
}
fn proof(role: BootstrapFinalityKeyRotationRoleV1) -> BootstrapFinalityKeyRotationProofV1 {
    BootstrapFinalityKeyRotationProofV1 {
        schema_version: 1,
        role,
        signature_hex: "6".repeat(128),
    }
}
fn action(body: BootstrapFinalityKeyRotationBodyV1) -> Action {
    Action::RotateGovernanceBootstrapFinalityKeyV1 {
        controller_account_id: "a".into(),
        body,
        old_key_proof: proof(BootstrapFinalityKeyRotationRoleV1::OldKeyAuthorization),
        new_key_proof: proof(BootstrapFinalityKeyRotationRoleV1::NewKeyPossession),
    }
}
// Independent minimal CBOR writer: no serde encoding is used for this vector.
fn uint(out: &mut Vec<u8>, major: u8, value: u64) {
    if value < 24 {
        out.push((major << 5) | value as u8);
    } else if value < 256 {
        out.extend_from_slice(&[(major << 5) | 24, value as u8]);
    } else {
        panic!("vector only uses short lengths");
    }
}
fn text(out: &mut Vec<u8>, value: &str) {
    uint(out, 3, value.len() as u64);
    out.extend_from_slice(value.as_bytes());
}
fn number(out: &mut Vec<u8>, field: &str, value: u64) {
    text(out, field);
    uint(out, 0, value);
}
fn field(out: &mut Vec<u8>, name: &str, value: &str) {
    text(out, name);
    text(out, value);
}
fn independent_body(b: &BootstrapFinalityKeyRotationBodyV1) -> Vec<u8> {
    let mut o = vec![0xac]; // twelve fields, in the fixed typed map order
    number(&mut o, "schema_version", 1);
    text(&mut o, "authorization");
    o.push(0xad); // thirteen context fields
    let a = &b.authorization;
    number(&mut o, "schema_version", 1);
    field(&mut o, "world_id", &a.world_id);
    field(&mut o, "chain_id", &a.chain_id);
    field(&mut o, "genesis_digest", &a.genesis_digest);
    number(&mut o, "parent_position", a.parent_position);
    field(&mut o, "parent_record_id", &a.parent_record_id);
    field(&mut o, "parent_state_root", &a.parent_state_root);
    field(&mut o, "policy_digest", &a.policy_digest);
    field(&mut o, "controller_account_id", &a.controller_account_id);
    number(&mut o, "nonce", a.nonce);
    number(&mut o, "valid_from_position", a.valid_from_position);
    number(&mut o, "valid_through_position", a.valid_through_position);
    text(&mut o, "signatures");
    o.push(0xa0);
    field(&mut o, "rotation_id", &b.rotation_id);
    field(
        &mut o,
        "bootstrap_registry_digest_hex",
        &b.bootstrap_registry_digest_hex,
    );
    field(
        &mut o,
        "bootstrap_public_key_hex",
        &b.bootstrap_public_key_hex,
    );
    field(&mut o, "node_id", &b.node_id);
    number(&mut o, "expected_generation", b.expected_generation);
    field(&mut o, "old_public_key_hex", &b.old_public_key_hex);
    field(&mut o, "new_public_key_hex", &b.new_public_key_hex);
    number(&mut o, "decision_height", b.decision_height);
    number(&mut o, "activation_height", b.activation_height);
    number(&mut o, "target_epoch", b.target_epoch);
    o
}
#[test]
fn independent_role_framing_and_complete_controller_action() {
    let b = body();
    let encoded = independent_body(&b);
    assert_eq!(serde_cbor::to_vec(&b).unwrap(), encoded);
    assert_eq!(
        BootstrapFinalityKeyRotationBodyV1::decode_canonical(&encoded).unwrap(),
        b
    );
    for role in [
        BootstrapFinalityKeyRotationRoleV1::OldKeyAuthorization,
        BootstrapFinalityKeyRotationRoleV1::NewKeyPossession,
    ] {
        let domain = role.domain().as_bytes();
        let mut expected = Vec::new();
        expected.extend_from_slice(&(domain.len() as u64).to_be_bytes());
        expected.extend_from_slice(domain);
        expected.extend_from_slice(&(encoded.len() as u64).to_be_bytes());
        expected.extend_from_slice(&encoded);
        assert_eq!(b.signing_bytes(role).unwrap(), expected);
    }
    assert_ne!(
        b.signing_bytes(BootstrapFinalityKeyRotationRoleV1::OldKeyAuthorization)
            .unwrap(),
        b.signing_bytes(BootstrapFinalityKeyRotationRoleV1::NewKeyPossession)
            .unwrap()
    );
    let mut outer = b.authorization.clone();
    let mut a = action(b.clone());
    assert!(governance_validator_admission_auth_required(&a));
    let original = outer.signing_bytes(&a).unwrap();
    outer.signatures.insert("7".repeat(64), "8".repeat(128));
    assert_eq!(outer.signing_bytes(&a).unwrap(), original);
    if let Action::RotateGovernanceBootstrapFinalityKeyV1 {
        ref mut new_key_proof,
        ..
    } = a
    {
        new_key_proof.signature_hex = "9".repeat(128);
    }
    assert_ne!(outer.signing_bytes(&a).unwrap(), original);
    assert_eq!(b.next_generation().unwrap(), 2);
}
#[test]
fn binds_every_outer_context_field_and_rejects_signature_cycle() {
    let b = body();
    let a = action(b.clone());
    let mut changes = Vec::new();
    macro_rules! change {
        ($field:ident, $value:expr) => {{
            let mut c = b.authorization.clone();
            c.$field = $value;
            changes.push(c);
        }};
    }
    change!(schema_version, 2);
    change!(world_id, "other".into());
    change!(chain_id, "other".into());
    change!(genesis_digest, "a".repeat(64));
    change!(parent_position, 2);
    change!(parent_record_id, "b".repeat(64));
    change!(parent_state_root, "c".repeat(64));
    change!(policy_digest, "d".repeat(64));
    change!(controller_account_id, "other".into());
    change!(nonce, 2);
    change!(valid_from_position, 1);
    change!(valid_through_position, 4);
    for outer in changes {
        assert!(b.validate_authorization(&outer).is_err());
        assert!(outer.signing_bytes(&a).is_err());
    }
    let mut cyclic = b;
    cyclic
        .authorization
        .signatures
        .insert("a".repeat(64), "b".repeat(128));
    assert!(cyclic.validate().is_err());
}
#[test]
fn rejects_bad_boundaries_keys_proof_roles_and_noncanonical_wire() {
    let b = body();
    for field in 0..8 {
        let mut bad = b.clone();
        match field {
            0 => bad.schema_version = 2,
            1 => bad.expected_generation = u64::MAX,
            2 => bad.target_epoch = 0,
            3 => bad.new_public_key_hex = bad.old_public_key_hex.clone(),
            4 => bad.old_public_key_hex = "A".repeat(64),
            5 => bad.activation_height += 1,
            6 => bad.decision_height += 1,
            _ => bad.rotation_id = "x".repeat(257),
        }
        assert!(bad.validate().is_err());
    }
    let mut initial = b.clone();
    initial.expected_generation = 0;
    assert_eq!(initial.next_generation().unwrap(), 1);
    let old = proof(BootstrapFinalityKeyRotationRoleV1::OldKeyAuthorization);
    assert!(
        old.validate_for(&b, BootstrapFinalityKeyRotationRoleV1::NewKeyPossession)
            .is_err()
    );
    let mut upper = old;
    upper.signature_hex = "A".repeat(128);
    assert!(
        upper
            .validate_for(&b, BootstrapFinalityKeyRotationRoleV1::OldKeyAuthorization)
            .is_err()
    );
    let bytes = serde_cbor::to_vec(&b).unwrap();
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(BootstrapFinalityKeyRotationBodyV1::decode_canonical(&trailing).is_err());
    let mut noncanonical = bytes; // schema_version=1, represented with an unnecessary extra byte
    let key = b"schema_version";
    let offset = noncanonical
        .windows(key.len())
        .position(|w| w == key)
        .unwrap()
        + key.len();
    assert_eq!(noncanonical[offset], 1);
    noncanonical.insert(offset, 0x18);
    assert!(BootstrapFinalityKeyRotationBodyV1::decode_canonical(&noncanonical).is_err());
    assert!(
        BootstrapFinalityKeyRotationBodyV1::decode_canonical(&vec![
            0;
            BOOTSTRAP_FINALITY_KEY_ROTATION_MAX_BYTES_V1
                + 1
        ])
        .is_err()
    );
    let raw = serde_json::to_string(&b).unwrap();
    let duplicated = raw.replacen(
        "\"rotation_id\":\"r\"",
        "\"rotation_id\":\"r\",\"rotation_id\":\"other\"",
        1,
    );
    assert!(serde_json::from_str::<BootstrapFinalityKeyRotationBodyV1>(&duplicated).is_err());
}

#[test]
fn bootstrap_source_and_initial_key_are_bound_and_admitted_domains_stay_separate() {
    let b = body();
    let old = BootstrapFinalityKeyRotationRoleV1::OldKeyAuthorization;
    let original = b.signing_bytes(old).unwrap();
    for field in 0..2 {
        let mut changed = b.clone();
        if field == 0 {
            changed.bootstrap_registry_digest_hex = "c".repeat(64);
        } else {
            changed.bootstrap_public_key_hex = "d".repeat(64);
        }
        assert_ne!(changed.signing_bytes(old).unwrap(), original);
        assert_ne!(
            b.authorization.signing_bytes(&action(b.clone())).unwrap(),
            changed
                .authorization
                .signing_bytes(&action(changed.clone()))
                .unwrap()
        );
        if field == 0 {
            changed.bootstrap_registry_digest_hex = "A".repeat(64);
        } else {
            changed.bootstrap_public_key_hex = "A".repeat(64);
        }
        assert!(changed.validate().is_err());
    }
    assert_ne!(
        old.domain(),
        crate::world_action::FINALITY_KEY_ROTATION_OLD_DOMAIN_V1
    );
    assert_ne!(
        BootstrapFinalityKeyRotationRoleV1::NewKeyPossession.domain(),
        crate::world_action::FINALITY_KEY_ROTATION_NEW_DOMAIN_V1
    );
    let encoded = serde_cbor::to_vec(&b).unwrap();
    assert!(crate::world_action::FinalityKeyRotationBodyV1::decode_canonical(&encoded).is_err());
    let wire = serde_json::to_string(&action(b.clone())).unwrap();
    assert!(wire.contains("RotateGovernanceBootstrapFinalityKeyV1"));
    assert!(!wire.contains("candidate_id"));
    let decoded: Action = serde_json::from_str(&wire).unwrap();
    assert_eq!(
        b.authorization.signing_bytes(&decoded).unwrap(),
        b.authorization.signing_bytes(&action(b.clone())).unwrap()
    );
}

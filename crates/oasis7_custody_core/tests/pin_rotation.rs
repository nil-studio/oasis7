use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, Signer, SigningKey};
use oasis7_custody_core::{DecimalU64 as D, native_binding::*};
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|v| format!("{v:02x}")).collect()
}
fn manifest() -> OwnerPinRotationManifestV1 {
    OwnerPinRotationManifestV1 {
        api_version: 1,
        from_ledger_digest: "12".repeat(32),
        identity_namespace: "native.test".into(),
        identity_instance_epoch: D(9_007_199_254_740_993),
        old_pin: NativeBindingPinV1 {
            key_id: "owner-old".into(),
            public_key: hex(&SigningKey::from_bytes(&[51; 32]).verifying_key().to_bytes()),
            environment: "test".into(),
            service_identity: "custody-owner".into(),
            service_epoch: D(3),
        },
        new_pin: NativeBindingPinV1 {
            key_id: "owner-new".into(),
            public_key: hex(&SigningKey::from_bytes(&[52; 32]).verifying_key().to_bytes()),
            environment: "test".into(),
            service_identity: "custody-owner".into(),
            service_epoch: D(4),
        },
        operation_id: "rotation-operation".into(),
        issued_at: D(1000),
        expires_at: D(301_000),
    }
}
#[test]
fn rotation_is_exactly_ledger_namespace_and_epoch_bound() {
    let m = manifest();
    assert!(
        m.validate_for(
            &m.from_ledger_digest,
            &m.old_pin,
            &m.identity_namespace,
            m.identity_instance_epoch,
            1000
        )
        .is_ok()
    );
    assert!(
        m.validate_for(
            &"13".repeat(32),
            &m.old_pin,
            &m.identity_namespace,
            m.identity_instance_epoch,
            1000
        )
        .is_err()
    );
    assert!(
        m.validate_for(
            &m.from_ledger_digest,
            &m.new_pin,
            &m.identity_namespace,
            m.identity_instance_epoch,
            1000
        )
        .is_err()
    );
    assert!(
        m.validate_for(
            &m.from_ledger_digest,
            &m.old_pin,
            "another.namespace",
            m.identity_instance_epoch,
            1000
        )
        .is_err()
    );
    assert!(
        m.validate_for(
            &m.from_ledger_digest,
            &m.old_pin,
            &m.identity_namespace,
            D(1),
            1000
        )
        .is_err()
    );
}
#[test]
fn only_monotone_distinct_same_service_pins_are_eligible() {
    let original = manifest();
    for mode in 0..6 {
        let mut m = original.clone();
        match mode {
            0 => m.new_pin.service_epoch = m.old_pin.service_epoch,
            1 => m.new_pin.key_id = m.old_pin.key_id.clone(),
            2 => m.new_pin.public_key = m.old_pin.public_key.clone(),
            3 => m.new_pin.environment = "other".into(),
            4 => m.new_pin.service_identity = "other".into(),
            _ => m.new_pin.service_epoch = D(2),
        }
        assert!(m.validate().is_err());
    }
}
#[test]
fn time_window_cannot_be_future_expired_or_more_than_five_minutes() {
    let mut m = manifest();
    assert!(m.validate_at(999).is_err());
    assert!(m.validate_at(301_000).is_err());
    assert!(m.validate_at(300_999).is_ok());
    m.expires_at = D(301_001);
    assert!(m.validate().is_err());
    m.issued_at = D(u64::MAX);
    m.expires_at = D(1);
    assert!(m.validate().is_err());
}
#[test]
fn root_and_possession_domains_cover_every_manifest_byte() {
    let m = manifest();
    let root = SigningKey::from_bytes(&[53; 32]);
    let owner = SigningKey::from_bytes(&[52; 32]);
    let bytes = m.signing_bytes().unwrap();
    let pop = m.possession_signing_bytes().unwrap();
    assert_ne!(bytes, pop);
    let root_signature = root.sign(&bytes);
    let owner_signature = owner.sign(&pop);
    assert!(
        root.verifying_key()
            .verify_strict(&bytes, &root_signature)
            .is_ok()
    );
    assert!(
        owner
            .verifying_key()
            .verify_strict(&pop, &owner_signature)
            .is_ok()
    );
    assert!(
        root.verifying_key()
            .verify_strict(&pop, &root_signature)
            .is_err()
    );
    let mut changed = m.clone();
    changed.operation_id.push('x');
    assert_ne!(m.digest().unwrap(), changed.digest().unwrap());
    assert!(
        root.verifying_key()
            .verify_strict(&changed.signing_bytes().unwrap(), &root_signature)
            .is_err()
    );
    assert!(
        owner
            .verifying_key()
            .verify_strict(
                &changed.possession_signing_bytes().unwrap(),
                &owner_signature
            )
            .is_err()
    );
    let signed = SignedOwnerPinRotationManifestV1 {
        manifest: m,
        native_root_key_id: "native-fixed-root".into(),
        native_root_signature: URL_SAFE_NO_PAD.encode(root_signature.to_bytes()),
        new_owner_signature: URL_SAFE_NO_PAD.encode(owner_signature.to_bytes()),
    };
    assert!(signed.validate_root_key_id("native-fixed-root").is_ok());
    assert!(
        signed
            .validate_root_key_id("manifest-selected-root")
            .is_err()
    );
    let decoded = URL_SAFE_NO_PAD
        .decode(&signed.native_root_signature)
        .unwrap();
    assert!(Signature::from_slice(&decoded).is_ok());
}
#[test]
fn wire_is_closed_lowercase_and_full_width_without_json_number_coercion() {
    let m = manifest();
    let mut wire = serde_json::to_value(&m).unwrap();
    assert_eq!(
        wire["identity_instance_epoch"],
        serde_json::json!("9007199254740993")
    );
    wire["identity_instance_epoch"] = serde_json::json!(9007199254740993u64);
    assert!(serde_json::from_value::<OwnerPinRotationManifestV1>(wire).is_err());
    let mut wire = serde_json::to_value(&m).unwrap();
    wire["unknown"] = serde_json::json!(true);
    assert!(serde_json::from_value::<OwnerPinRotationManifestV1>(wire).is_err());
    let mut m = m;
    m.from_ledger_digest = "AB".repeat(32);
    assert!(m.validate().is_err());
    let mut signed = SignedOwnerPinRotationManifestV1 {
        manifest: manifest(),
        native_root_key_id: "native-fixed-root".into(),
        native_root_signature: "A".repeat(86),
        new_owner_signature: "A".repeat(86),
    };
    assert!(signed.validate().is_ok());
    signed.new_owner_signature.replace_range(85.., "B");
    assert!(signed.validate().is_err());
    signed.new_owner_signature = "A".repeat(86);
    signed.native_root_signature.push('=');
    assert!(signed.validate().is_err());
    let mut wire = serde_json::to_value(&signed).unwrap();
    wire["public_key"] = serde_json::json!("self-reported");
    assert!(serde_json::from_value::<SignedOwnerPinRotationManifestV1>(wire).is_err());
}

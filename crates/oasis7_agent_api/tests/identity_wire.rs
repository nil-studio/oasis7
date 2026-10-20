use oasis7_agent_api::identity::{NativeIdentityChallengeV1, NativeIdentityPurposeV1};

#[test]
fn identity_codec_matches_independent_cross_language_vectors() {
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/native_identity_v1.json")).unwrap();
    for v in vectors.as_array().unwrap() {
        let purpose = match v["purpose"].as_str().unwrap() {
            "login" => NativeIdentityPurposeV1::Login,
            "bind" => NativeIdentityPurposeV1::Bind,
            "reauth" => NativeIdentityPurposeV1::Reauthenticate,
            "register" => NativeIdentityPurposeV1::Register,
            "recovery_candidate" => NativeIdentityPurposeV1::RecoveryCandidate,
            _ => panic!("unknown vector purpose"),
        };
        let key = hex::decode(v["public_key"].as_str().unwrap()).unwrap();
        let nonce = hex::decode(v["nonce_hex"].as_str().unwrap()).unwrap();
        let challenge = NativeIdentityChallengeV1 {
            identity_service_id: v["identity_service_id"].as_str().unwrap(),
            environment_id: v["environment_id"].as_str().unwrap(),
            audience: v["audience"].as_str().unwrap(),
            purpose,
            account_context: v["account_context"].as_str().unwrap(),
            operation_id: v["operation_id"].as_str().unwrap(),
            public_key: key.as_slice().try_into().unwrap(),
            nonce: nonce.as_slice().try_into().unwrap(),
            issued_at: v["issued_at"].as_u64().unwrap(),
            expires_at: v["expires_at"].as_u64().unwrap(),
        };
        assert_eq!(
            hex::encode(challenge.signing_bytes().unwrap()),
            v["signed_bytes_hex"].as_str().unwrap()
        );
    }
}

#[test]
fn every_identity_binding_and_field_boundary_changes_signed_bytes() {
    let mut c = NativeIdentityChallengeV1 {
        identity_service_id: "service",
        environment_id: "test",
        audience: "https://localhost",
        purpose: NativeIdentityPurposeV1::Login,
        account_context: "",
        operation_id: "op",
        public_key: &[1; 32],
        nonce: &[2; 32],
        issued_at: 100,
        expires_at: 120100,
    };
    let original = c.signing_bytes().unwrap();
    macro_rules! changed {
        ($field:ident, $value:expr) => {{
            let saved = c.$field;
            c.$field = $value;
            assert_ne!(original, c.signing_bytes().unwrap(), stringify!($field));
            c.$field = saved;
        }};
    }
    changed!(identity_service_id, "other");
    changed!(environment_id, "other");
    changed!(audience, "https://other");
    changed!(purpose, NativeIdentityPurposeV1::Register);
    changed!(account_context, "account");
    changed!(operation_id, "other");
    changed!(public_key, &[3; 32]);
    changed!(nonce, &[3; 32]);
    changed!(issued_at, u64::MAX);
    changed!(expires_at, u64::MAX);
    c.identity_service_id = "ab";
    c.environment_id = "c";
    let partition = c.signing_bytes().unwrap();
    c.identity_service_id = "a";
    c.environment_id = "bc";
    assert_ne!(partition, c.signing_bytes().unwrap());
}

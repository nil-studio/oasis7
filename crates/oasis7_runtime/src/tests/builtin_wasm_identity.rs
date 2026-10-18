use super::super::{
    WorldError, m1_builtin_manifest_hash_tokens, m1_builtin_module_artifact_identity,
    m4_builtin_manifest_hash_tokens, m4_builtin_module_artifact_identity,
    m5_builtin_manifest_hash_tokens, m5_builtin_module_artifact_identity,
};

fn first_hash_token_value(tokens: &[String]) -> &str {
    tokens
        .first()
        .map(String::as_str)
        .and_then(|token| token.split_once('=').map(|(_, hash)| hash).or(Some(token)))
        .expect("builtin manifest hash token")
}

#[test]
fn builtin_identity_manifest_resolves_m1_entry() {
    let hashes = m1_builtin_manifest_hash_tokens("m1.rule.move").expect("m1 hash tokens");
    let identity =
        m1_builtin_module_artifact_identity("m1.rule.move", first_hash_token_value(&hashes))
            .expect("resolve m1 identity");
    assert!(identity.is_complete());
    assert_eq!(identity.signer_node_id, "builtin.module.release.signer");
    match identity.signature_scheme.as_str() {
        "ed25519" => {
            assert!(
                identity
                    .artifact_signature
                    .starts_with("modsig:ed25519:v1:"),
                "unexpected signature: {}",
                identity.artifact_signature
            );
        }
        "identity_hash_v1" => {
            assert!(
                identity.artifact_signature.starts_with("idhash:"),
                "unexpected signature: {}",
                identity.artifact_signature
            );
        }
        other => panic!("unexpected signature scheme: {other}"),
    }
}

#[test]
fn builtin_identity_manifest_resolves_m4_entry() {
    let hashes = m4_builtin_manifest_hash_tokens("m4.factory.miner.mk1").expect("m4 hash tokens");
    let identity = m4_builtin_module_artifact_identity(
        "m4.factory.miner.mk1",
        first_hash_token_value(&hashes),
    )
    .expect("resolve m4 identity");
    assert!(identity.is_complete());
    assert_eq!(identity.signer_node_id, "builtin.module.release.signer");
    match identity.signature_scheme.as_str() {
        "ed25519" => {
            assert!(
                identity
                    .artifact_signature
                    .starts_with("modsig:ed25519:v1:"),
                "unexpected signature: {}",
                identity.artifact_signature
            );
        }
        "identity_hash_v1" => {
            assert!(
                identity.artifact_signature.starts_with("idhash:"),
                "unexpected signature: {}",
                identity.artifact_signature
            );
        }
        other => panic!("unexpected signature scheme: {other}"),
    }
}

#[test]
fn builtin_identity_manifest_resolves_m5_entry() {
    let hashes = m5_builtin_manifest_hash_tokens("m5.gameplay.war.core").expect("m5 hash tokens");
    let identity = m5_builtin_module_artifact_identity(
        "m5.gameplay.war.core",
        first_hash_token_value(&hashes),
    )
    .expect("resolve m5 identity");
    assert!(identity.is_complete());
    assert_eq!(identity.signer_node_id, "builtin.module.release.signer");
    match identity.signature_scheme.as_str() {
        "ed25519" => {
            assert!(
                identity
                    .artifact_signature
                    .starts_with("modsig:ed25519:v1:"),
                "unexpected signature: {}",
                identity.artifact_signature
            );
        }
        "identity_hash_v1" => {
            assert!(
                identity.artifact_signature.starts_with("idhash:"),
                "unexpected signature: {}",
                identity.artifact_signature
            );
        }
        other => panic!("unexpected signature scheme: {other}"),
    }
}

#[test]
fn builtin_identity_manifest_rejects_missing_module() {
    let hashes = m1_builtin_manifest_hash_tokens("m1.rule.move").expect("m1 hash tokens");
    let err =
        m1_builtin_module_artifact_identity("m1.rule.missing", first_hash_token_value(&hashes))
            .expect_err("missing module should fail");

    match err {
        WorldError::ModuleChangeInvalid { reason } => {
            assert!(
                reason.contains("missing module_id=m1.rule.missing"),
                "unexpected reason: {reason}"
            );
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

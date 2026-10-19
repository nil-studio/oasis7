use super::*;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../../doc/testing/templates/network-tier-persistent-preview-planned.example.json"
    ))
    .unwrap()
}

fn load_modified(source: &Value) -> Result<LoadedNetworkTierManifest, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = loop {
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let candidate = std::env::temp_dir().join(format!(
            "oasis7-planned-policy-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        match fs::create_dir(&candidate) {
            Ok(()) => break candidate,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("create exclusive planned policy directory: {error}"),
        }
    };
    let mut source = source.clone();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for name in ["genesis_ref", "bootstrap_peer_ref"] {
        let fixture_path = root
            .join("doc/testing/templates")
            .join(source["runtime_refs"][name].as_str().unwrap());
        source["runtime_refs"][name] = json!(fixture_path.to_string_lossy());
    }
    let path = dir.join("manifest.json");
    fs::write(&path, serde_json::to_vec(&source).unwrap()).unwrap();
    let result = LoadedNetworkTierManifest::load(&path);
    fs::remove_dir_all(dir).unwrap();
    result
}

#[test]
fn planned_persistent_preview_is_schema_valid_but_cannot_start_prototype() {
    let loaded = load_modified(&fixture()).expect("explicit preview persistent declaration");
    assert_eq!(
        loaded.manifest.world_policy.as_ref().unwrap().retention,
        "persistent"
    );
    assert_eq!(loaded.manifest.token_policy.value_semantics, "preview");
    assert!(loaded.manifest.token_policy.reset_policy.is_none());
    let roundtrip = serde_json::to_value(&loaded.manifest).unwrap();
    assert!(roundtrip["token_policy"].get("reset_policy").is_none());
    assert_eq!(
        serde_json::from_value::<NetworkTierManifest>(roundtrip).unwrap(),
        loaded.manifest
    );

    let error = loaded
        .validate_runtime_support()
        .expect_err("planned is not activation");
    assert!(error.contains("cannot start the compatibility prototype"));
}

#[test]
fn legacy_schema_never_silently_ignores_new_authority_or_world_policies() {
    for field in ["release_policy", "world_policy", "authority_policy"] {
        for value in [Value::Null, fixture()[field].clone()] {
            let mut source = fixture();
            source["schema_version"] = json!(NETWORK_TIER_MANIFEST_SCHEMA_V1);
            for other in ["release_policy", "world_policy", "authority_policy"] {
                source.as_object_mut().unwrap().remove(other);
            }
            source[field] = value;
            let error = load_modified(&source).expect_err("cannot discard v2 policy");
            assert!(
                error.contains("v1 network tier manifest forbids"),
                "{error}"
            );
        }
    }
}

#[test]
fn planned_schema_rejects_activation_value_reset_and_identity_drift() {
    let cases = [
        ("/status", json!("live")),
        ("/tier", json!("mainnet")),
        ("/release_policy/stage", json!("production")),
        ("/world_policy/retention", json!("resettable")),
        ("/world_policy/reset_policy", json!("resettable")),
        ("/world_policy/world_id", json!("another-world")),
        ("/chain_id", json!("another-chain")),
        ("/authority_policy/profile", json!("bft")),
        ("/authority_policy/profile_version", json!(2)),
        ("/authority_policy/profile_version", json!(true)),
        ("/authority_policy/activation", json!("active")),
        ("/token_policy/value_semantics", json!("production")),
        ("/token_policy/faucet_mode", json!("guarded_testnet_faucet")),
    ];
    for (pointer, value) in cases {
        let mut source = fixture();
        *source.pointer_mut(pointer).unwrap() = value;
        assert!(load_modified(&source).is_err(), "must reject {pointer}");
    }
    for value in [Value::Null, json!("frozen")] {
        let mut source = fixture();
        source["token_policy"]["reset_policy"] = value;
        assert!(
            load_modified(&source)
                .unwrap_err()
                .contains("v2 forbids token_policy.reset_policy")
        );
    }
    for field in ["release_policy", "world_policy", "authority_policy"] {
        let mut source = fixture();
        source.as_object_mut().unwrap().remove(field);
        assert!(load_modified(&source).is_err());
        source[field] = Value::Null;
        assert!(load_modified(&source).is_err());
    }
}

#[test]
fn planned_preview_cannot_allow_active_claims_even_when_denied() {
    for claim in [
        "mainnet_live",
        "PRODUCTION_OC_SETTLEMENT",
        "persistent_world_live",
        "controlled_single_authority_live",
        "distributed_finality",
    ] {
        let mut source = fixture();
        source["claims_policy"]["allowed_claims"]
            .as_array_mut()
            .unwrap()
            .push(json!(claim));
        assert!(
            load_modified(&source)
                .unwrap_err()
                .contains("forbids allowed claim")
        );
    }
}

use super::*;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("duration")
        .as_nanos();
    std::env::temp_dir().join(format!("oasis7-governance-import-{prefix}-{unique}"))
}

fn governed_manifest(on_call_key: &str, governance_key: &str) -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({"slot_id":"governance.finality.v1","signer_id":"validator01","scheme":"ed25519","threshold":1,"public_key_hex":"54e7a02919fff2d49a9c325def8cb0211ea7f7a75a9011b9d0678b9e2a7af6bc"}),
        serde_json::json!({"slot_id":"msig.genesis.v1","signer_id":"controller01","scheme":"ed25519","threshold":1,"public_key_hex":"6249e5a58278dbc4e629a16b5d33f6b84c39e3ceeb10e963bb9ef64ea4daac30"}),
        serde_json::json!({"slot_id":"msig.staking_governance.v1","signer_id":"staking01","scheme":"ed25519","threshold":1,"public_key_hex":"13c160fc0f516b9a5663aa00c2a5446be6467f68ce341fdd79cdb64224dffd20"}),
        serde_json::json!({"slot_id":"msig.ecosystem_governance.v1","signer_id":"ecosystem01","scheme":"ed25519","threshold":1,"public_key_hex":"0241f2e23305407676f2a5cec6d154da74944b2a366b2b2b6913cb746d402d0e"}),
        serde_json::json!({"slot_id":"msig.security_council.v1","signer_id":"security01","scheme":"ed25519","threshold":1,"public_key_hex":"d09de9413371ae42f643e4f8f31e2139611d1617809375b1ad884df3fb089448"}),
        serde_json::json!({"slot_id":"ops.rollback.on_call.v1","signer_id":"rollback-on-call-01","scheme":"ed25519","threshold":1,"public_key_hex":on_call_key}),
        serde_json::json!({"slot_id":"governance.rollback.v1","signer_id":"rollback-governance-01","scheme":"ed25519","threshold":1,"public_key_hex":governance_key}),
    ]
}

fn expected_rollback_registry(
    on_call_key: &str,
    governance_key: &str,
) -> RollbackAuthorityRegistry {
    RollbackAuthorityRegistry::new([
        RollbackAuthorityRecord {
            authority_id: "rollback-on-call-01".into(),
            role: RollbackAuthorityRole::OnCall,
            public_key_hex: on_call_key.into(),
            active: true,
        },
        RollbackAuthorityRecord {
            authority_id: "rollback-governance-01".into(),
            role: RollbackAuthorityRole::Governance,
            public_key_hex: governance_key.into(),
            active: true,
        },
    ])
    .expect("valid expected rollback registry")
}

fn write_manifest(path: &std::path::Path, entries: &[serde_json::Value]) {
    std::fs::write(
        path,
        serde_json::to_vec_pretty(entries).expect("encode manifest"),
    )
    .expect("write manifest");
}

#[test]
fn import_bootstraps_persists_and_rotates_fixed_rollback_authority_slots() {
    const OLD_ON_CALL: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const OLD_GOVERNANCE: &str = "2222222222222222222222222222222222222222222222222222222222222222";
    const NEW_ON_CALL: &str = "3333333333333333333333333333333333333333333333333333333333333333";
    const NEW_GOVERNANCE: &str = "4444444444444444444444444444444444444444444444444444444444444444";
    let root = temp_dir("rollback-authority-rotation");
    std::fs::create_dir_all(&root).expect("create root");
    let world_dir = root.join("world");
    let manifest_path = root.join("public_manifest.json");
    write_manifest(
        &manifest_path,
        &governed_manifest(OLD_ON_CALL, OLD_GOVERNANCE),
    );
    run_import(CliOptions {
        world_dir: world_dir.clone(),
        public_manifest: manifest_path.clone(),
        finality_slot_id: "governance.finality.v1".into(),
        default_threshold: 1,
    })
    .expect("bootstrap governed rollback authorities");
    let bootstrapped =
        oasis7::runtime_host::world::load_from_dir(&world_dir).expect("reload bootstrap");
    assert_eq!(
        bootstrapped.snapshot().rollback_authority_registry,
        expected_rollback_registry(OLD_ON_CALL, OLD_GOVERNANCE)
    );

    write_manifest(
        &manifest_path,
        &governed_manifest(NEW_ON_CALL, NEW_GOVERNANCE),
    );
    run_import(CliOptions {
        world_dir: world_dir.clone(),
        public_manifest: manifest_path,
        finality_slot_id: "governance.finality.v1".into(),
        default_threshold: 1,
    })
    .expect("rotate governed rollback authorities");
    let rotated = oasis7::runtime_host::world::load_from_dir(world_dir).expect("reload rotation");
    assert_eq!(
        rotated.snapshot().rollback_authority_registry,
        expected_rollback_registry(NEW_ON_CALL, NEW_GOVERNANCE)
    );
}

#[test]
fn invalid_rollback_authority_manifest_is_rejected_without_world_overwrite() {
    const ON_CALL: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const GOVERNANCE: &str = "2222222222222222222222222222222222222222222222222222222222222222";
    let root = temp_dir("rollback-authority-invalid-atomic");
    std::fs::create_dir_all(&root).expect("create root");
    let world_dir = root.join("world");
    let manifest_path = root.join("public_manifest.json");
    let mut baseline = oasis7::runtime_host::world::new_production_hardened();
    baseline
        .set_rollback_authority_registry(expected_rollback_registry(ON_CALL, GOVERNANCE))
        .expect("seed registry");
    baseline.save_to_dir(&world_dir).expect("save baseline");
    let baseline_snapshot = baseline.snapshot();

    let mut cases = Vec::new();
    let mut missing = governed_manifest(ON_CALL, GOVERNANCE);
    missing.retain(|entry| entry["slot_id"] != "governance.rollback.v1");
    cases.push(("missing fixed governance slot", missing));
    let mut wrong_scheme = governed_manifest(ON_CALL, GOVERNANCE);
    wrong_scheme[5]["scheme"] = serde_json::json!("secp256k1");
    cases.push(("wrong signature scheme", wrong_scheme));
    let mut wrong_threshold = governed_manifest(ON_CALL, GOVERNANCE);
    wrong_threshold[5]["threshold"] = serde_json::json!(2);
    cases.push(("wrong per-role threshold", wrong_threshold));
    let mut duplicate = governed_manifest(ON_CALL, GOVERNANCE);
    duplicate[6]["signer_id"] = duplicate[5]["signer_id"].clone();
    duplicate[6]["public_key_hex"] = duplicate[5]["public_key_hex"].clone();
    cases.push(("cross-role duplicate signer and key", duplicate));

    let mut unexpectedly_accepted = Vec::new();
    for (label, entries) in cases {
        baseline.save_to_dir(&world_dir).expect("restore baseline");
        write_manifest(&manifest_path, &entries);
        if run_import(CliOptions {
            world_dir: world_dir.clone(),
            public_manifest: manifest_path.clone(),
            finality_slot_id: "governance.finality.v1".into(),
            default_threshold: 1,
        })
        .is_ok()
        {
            unexpectedly_accepted.push(label);
        }
        if oasis7::runtime_host::world::load_from_dir(&world_dir)
            .expect("reload baseline")
            .snapshot()
            != baseline_snapshot
        {
            unexpectedly_accepted.push(label);
        }
    }
    assert!(
        unexpectedly_accepted.is_empty(),
        "invalid manifests accepted or overwrote world state: {unexpectedly_accepted:?}"
    );
}

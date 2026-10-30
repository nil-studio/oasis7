//! Shipping CLI with fixed original identity and authenticated active material.
use super::*;
use std::os::unix::fs::PermissionsExt;
#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    h: &QualifiedH<BftProfileTargetV2>,
    root: &Scratch,
    store: &LocalCasStore,
    binding: &BftSignerBindingV3,
    pin: &str,
    address: SocketAddr,
    archive: &LocalBftSuccessorArchiveStoreV1,
) {
    let directory = root.path().join("shipping-active-cli");
    fs::create_dir(&directory).unwrap();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    let replication = directory.join("replication");
    fs::create_dir(&replication).unwrap();
    fs::set_permissions(&replication, fs::Permissions::from_mode(0o700)).unwrap();
    let trust = directory.join("trust.json");
    let material = directory.join("H.cbor");
    let key = directory.join("requester.hex");
    let config = directory.join("node.json");
    let input = directory.join("actions.cbor");
    private(&trust, &serde_json::to_vec(&h.trust).unwrap());
    private(
        &material,
        &oasis7_wasm_abi::encode_canonical_cbor(&h.material).unwrap(),
    );
    private(&key, hex::encode(fixtures::key(117).to_bytes()).as_bytes());
    let actions = vec![
        oasis7_node::NodeConsensusAction::from_payload(
            98_001,
            validator_id(0),
            payload("native-active-start-nonempty-agent"),
        )
        .unwrap(),
    ];
    private(&input, &serde_cbor::to_vec(&actions).unwrap());
    let value = serde_json::json!({"schema_version":1,"trust_config_path":trust,"target":h.target,
        "handoff_material_path":material,"cas_directory":store.root(),"records_directory":root.path().join("ledger"),"replication_directory":replication,
        "validator_id":binding.local_validator_id,"player_id":validator_id(0),"operator_instance_id":binding.operator_instance_id,
        "provisioned_signing_keys":binding.provisioned_signing_keys,"timeouts":binding.timeouts,"provider_address":address,
        "provider_trust":{"schema_version":3,"provider_public_key":pin,"binding_digest":binding.digest().unwrap()},
        "requester_signing_key_path":key,"provider_timeout_ms":60000,"completion_timeout_ms":120000,
        "successor_private_archive_directory":archive.root()});
    private(&config, &serde_json::to_vec(&value).unwrap());
    crate::identity_receipt::dispatch(
        [
            "bft-header",
            "node",
            "--config",
            config.to_str().unwrap(),
            "--actions",
            input.to_str().unwrap(),
        ]
        .into_iter(),
    )
    .unwrap()
    .expect("shipping CLI must reopen authenticated active execution");
}
fn private(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

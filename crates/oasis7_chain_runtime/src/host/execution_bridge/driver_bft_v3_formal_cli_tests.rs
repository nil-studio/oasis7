//! Real offline command consumer: exact H, private replay inputs and next header.
use super::super::super::formal_header::FormalBftHeaderExecutionV3;
use super::execution_tests::{certify, private_actions};
use super::*;
use oasis7_runtime::LocalCasStore;
use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[test]
fn wire3_formal_read_cli_previews_from_qualified_h_and_actual_private_history() {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(cli_case)
        .unwrap()
        .join()
        .unwrap();
}

fn file(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

fn names(path: &Path) -> BTreeSet<String> {
    fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect()
}

fn cli_case() {
    let h = qualified_handoff_v3();
    let mut execution = FormalBftHeaderExecutionV3::from_handoff(
        h.anchor.clone(),
        h.target.clone(),
        &h.material,
        h.reader.clone(),
        Vec::new(),
        false,
    )
    .unwrap();
    let root = Scratch::new();
    let cas = root.path().join("cas");
    fs::create_dir(&cas).unwrap();
    fs::set_permissions(&cas, fs::Permissions::from_mode(0o700)).unwrap();
    let store = LocalCasStore::new(&cas);
    for (digest, bytes) in &h.reader.0 {
        assert_eq!(store.put_bytes(bytes).unwrap(), *digest);
    }
    let trust = root.path().join("trust.json");
    let material = root.path().join("H.cbor");
    let config = root.path().join("preview.json");
    let actions = root.path().join("actions.cbor");
    let history = root.path().join("private-history.cbor");
    file(&trust, &serde_json::to_vec(&h.trust).unwrap());
    file(
        &material,
        &oasis7_wasm_abi::encode_canonical_cbor(&h.material).unwrap(),
    );
    file(
        &config,
        &serde_json::to_vec(&serde_json::json!({
            "schema_version":1, "trust_config_path":trust, "target":h.target,
            "handoff_material_path":material, "cas_directory":cas,
        }))
        .unwrap(),
    );
    file(&actions, EMPTY_ACTIONS_CBOR);

    let expected_initial = execution.build_candidate(EMPTY_ACTIONS_CBOR).unwrap();
    let initial =
        crate::formal_bft_header_cli::preview_from_paths(&config, &actions, None).unwrap();
    let initial = serde_json::to_value(initial).unwrap();
    assert_eq!(
        initial["header"],
        serde_json::to_value(&expected_initial.header).unwrap()
    );
    assert_eq!(
        initial["execution_commitment"],
        serde_json::to_value(&expected_initial.commitment).unwrap()
    );

    let mut entries = Vec::new();
    for bytes in [private_actions(), EMPTY_ACTIONS_CBOR.to_vec()] {
        let candidate = execution.build_candidate(&bytes).unwrap();
        let certificate = certify(execution.schedule(), &candidate);
        execution.replay_next(&certificate, &bytes).unwrap();
        entries.push(serde_json::json!({"certificate":certificate, "actions_cbor":bytes}));
    }
    let history_value = serde_json::json!({"schema_version":1, "entries":entries});
    file(
        &history,
        &oasis7_wasm_abi::encode_canonical_cbor(&history_value).unwrap(),
    );
    let before_names = names(root.path());
    let expected = execution.build_candidate(EMPTY_ACTIONS_CBOR).unwrap();
    let preview =
        crate::formal_bft_header_cli::preview_from_paths(&config, &actions, Some(&history))
            .expect("the real CLI replays both certified private transitions from H");
    let preview = serde_json::to_value(preview).unwrap();
    assert_eq!(
        preview["schema_version"],
        "oasis7.formal_bft_header_preview.v1"
    );
    assert_eq!(
        preview["header"],
        serde_json::to_value(&expected.header).unwrap()
    );
    assert_eq!(
        preview["execution_commitment"],
        serde_json::to_value(&expected.commitment).unwrap()
    );
    assert_eq!(
        names(root.path()),
        before_names,
        "preview creates no ledger or receipt"
    );
    assert!(preview.get("actions_cbor").is_none());
    assert!(preview.get("snapshot").is_none());
    assert!(preview.get("committed").is_none());
    let args = [
        "bft-header",
        "preview",
        "--config",
        config.to_str().unwrap(),
        "--actions",
        actions.to_str().unwrap(),
        "--history",
        history.to_str().unwrap(),
    ];
    crate::identity_receipt::dispatch(args.into_iter())
        .unwrap()
        .unwrap();
    assert_eq!(names(root.path()), before_names);

    fs::set_permissions(&actions, fs::Permissions::from_mode(0o666)).unwrap();
    assert!(
        crate::formal_bft_header_cli::preview_from_paths(&config, &actions, None).is_err(),
        "private inputs cannot be read from a permissive file"
    );
    fs::set_permissions(&actions, fs::Permissions::from_mode(0o600)).unwrap();
    let linked_actions = root.path().join("linked-actions.cbor");
    std::os::unix::fs::symlink(&actions, &linked_actions).unwrap();
    assert!(
        crate::formal_bft_header_cli::preview_from_paths(&config, &linked_actions, None).is_err()
    );

    // File syntax is valid and the certificates are real, but the original
    // first-height private bytes are replaced by a different canonical vector.
    let mut mixed = history_value;
    mixed["entries"][0]["actions_cbor"] = serde_json::to_value(EMPTY_ACTIONS_CBOR).unwrap();
    file(
        &history,
        &oasis7_wasm_abi::encode_canonical_cbor(&mixed).unwrap(),
    );
    assert!(
        crate::formal_bft_header_cli::preview_from_paths(&config, &actions, Some(&history),)
            .is_err(),
        "a history cannot mix a genuine QC with other private inputs"
    );
}

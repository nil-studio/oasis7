use super::*;

#[test]
fn parse_options_rejects_network_tier_manifest_when_runtime_bundle_hash_mismatches_current_binary()
{
    let (dir, manifest_path) = write_test_network_tier_manifest(
        "0000000000000000000000000000000000000000000000000000000000000000",
    );
    let err = parse_options(
        [
            "--network-tier-manifest",
            manifest_path.to_string_lossy().as_ref(),
        ]
        .into_iter(),
    )
    .expect_err("parse should fail on runtime bundle drift");
    assert!(
        err.contains("network tier runtime bundle hash mismatch"),
        "unexpected mismatch error: {err}"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn parse_options_rejects_network_tier_manifest_when_runtime_bundle_hash_is_malformed() {
    let (dir, manifest_path) = write_test_network_tier_manifest("not-a-sha256");
    let err = parse_options(
        [
            "--network-tier-manifest",
            manifest_path.to_string_lossy().as_ref(),
        ]
        .into_iter(),
    )
    .expect_err("parse should fail on malformed runtime bundle hash");
    assert!(
        err.contains("invalid runtime_build.sha256"),
        "unexpected malformed hash error: {err}"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn parse_options_rejects_public_testnet_bundle_without_generated_world_sidecar() {
    let runtime_sha256 = current_test_binary_sha256();
    let (dir, manifest_path) = write_test_network_tier_manifest(runtime_sha256.as_str());
    let bundle_path = dir.join("public-testnet.bundle.json");
    let mut bundle: serde_json::Value =
        serde_json::from_slice(fs::read(&bundle_path).expect("read bundle").as_slice())
            .expect("parse bundle");
    bundle
        .as_object_mut()
        .expect("bundle object")
        .remove("generated_world_sidecar");
    fs::write(
        &bundle_path,
        serde_json::to_vec_pretty(&bundle).expect("encode bundle"),
    )
    .expect("write bundle");

    let err = parse_options(
        [
            "--network-tier-manifest",
            manifest_path.to_string_lossy().as_ref(),
        ]
        .into_iter(),
    )
    .expect_err("parse should fail without generated sidecar");
    assert!(
        err.contains("missing generated_world_sidecar"),
        "unexpected missing sidecar error: {err}"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn parse_options_rejects_network_tier_manifest_when_generated_world_sidecar_drifts() {
    let runtime_sha256 = current_test_binary_sha256();
    let (dir, manifest_path) = write_test_network_tier_manifest(runtime_sha256.as_str());
    fs::write(
        dir.join("generated-scenario-world").join("snapshot.json"),
        "{\"model\":{\"locations\":[{\"id\":\"drifted\"}]}}\n",
    )
    .expect("drift sidecar snapshot");

    let err = parse_options(
        [
            "--network-tier-manifest",
            manifest_path.to_string_lossy().as_ref(),
        ]
        .into_iter(),
    )
    .expect_err("parse should fail on generated sidecar drift");
    assert!(
        err.contains("generated_world_sidecar drift detected"),
        "unexpected generated sidecar drift error: {err}"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn parse_options_rejects_network_tier_manifest_when_generated_world_sidecar_journal_missing() {
    let runtime_sha256 = current_test_binary_sha256();
    let (dir, manifest_path) = write_test_network_tier_manifest(runtime_sha256.as_str());
    fs::remove_file(dir.join("generated-scenario-world").join("journal.json"))
        .expect("remove sidecar journal");

    let err = parse_options(
        [
            "--network-tier-manifest",
            manifest_path.to_string_lossy().as_ref(),
        ]
        .into_iter(),
    )
    .expect_err("parse should fail on incomplete generated sidecar");
    assert!(
        err.contains("generated_world_sidecar missing journal.json"),
        "unexpected generated sidecar missing journal error: {err}"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn parse_options_rejects_network_tier_manifest_when_generation_provenance_drifts() {
    let runtime_sha256 = current_test_binary_sha256();
    let (dir, manifest_path) = write_test_network_tier_manifest(runtime_sha256.as_str());
    fs::write(
        dir.join("world-generation-provenance.json"),
        r#"{"scenario_id":"different"}"#,
    )
    .expect("drift provenance");

    let err = parse_options(
        [
            "--network-tier-manifest",
            manifest_path.to_string_lossy().as_ref(),
        ]
        .into_iter(),
    )
    .expect_err("parse should fail on provenance drift");
    assert!(
        err.contains("world_generation_provenance drift detected"),
        "unexpected provenance drift error: {err}"
    );

    let _ = fs::remove_dir_all(dir);
}

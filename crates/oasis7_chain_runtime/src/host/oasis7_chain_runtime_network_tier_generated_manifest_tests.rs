use super::*;
use std::process::Command;

#[test]
fn generated_manifest_is_admitted_by_runtime_parser() {
    let dir = temp_dir("generated-manifest-admission");
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repository root");
    let sidecar_dir = dir.join("generated-world");
    let provenance_path = dir.join("world-generation-provenance.json");
    fs::create_dir_all(&sidecar_dir).expect("create generated world sidecar");
    fs::write(sidecar_dir.join("snapshot.json"), b"{}\n").expect("write sidecar snapshot");
    fs::write(sidecar_dir.join("journal.json"), b"{}\n").expect("write sidecar journal");
    fs::write(
        &provenance_path,
        br#"{"scenario_id":"network-tier-admission-test"}"#,
    )
    .expect("write generation provenance");

    let sidecar_meta = test_dir_tree_meta(sidecar_dir.as_path());
    let provenance_sha256 = sha256_hex(
        fs::read(&provenance_path)
            .expect("read generation provenance")
            .as_slice(),
    );
    let bundle_dir = repo_root
        .join("target/oasis7-chain-runtime-tests")
        .join(dir.file_name().expect("temporary directory name"));
    fs::create_dir_all(&bundle_dir).expect("create repository-relative bundle directory");
    let bundle_path = bundle_dir.join("release-candidate.json");
    fs::write(
        &bundle_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "runtime_build": {"sha256": current_test_binary_sha256()},
            "generated_world_sidecar": {
                "kind": "directory",
                "resolved_path": sidecar_dir,
                "sha256_tree": sidecar_meta.sha256_tree,
                "file_count": sidecar_meta.file_count,
                "total_bytes": sidecar_meta.total_bytes
            },
            "world_generation_provenance": {
                "kind": "file",
                "resolved_path": provenance_path,
                "sha256": provenance_sha256
            }
        }))
        .expect("encode release candidate bundle"),
    )
    .expect("write release candidate bundle");

    let bundle_ref = bundle_path
        .strip_prefix(&repo_root)
        .expect("bundle under repository root");
    let manifest_dir = dir.join("manifests");
    fs::create_dir_all(&manifest_dir).expect("create manifest directory");
    let manifest_path = manifest_dir.join("generated-network-tier.json");
    let genesis_template =
        repo_root.join("doc/testing/templates/public-testnet-genesis.example.json");
    let bootstrap_template =
        repo_root.join("doc/testing/templates/public-testnet-bootstrap.example.txt");
    let generated = Command::new(repo_root.join("scripts/network-tier-manifest.sh"))
        .current_dir(&dir)
        .args([
            "create",
            "--manifest",
            manifest_path.to_string_lossy().as_ref(),
            "--tier",
            "public_testnet",
            "--status",
            "rehearsal",
            "--network-id",
            "generated-admission-network",
            "--chain-id",
            "generated-admission-chain",
            "--release-candidate-bundle-ref",
            bundle_ref.to_string_lossy().as_ref(),
            "--genesis-ref",
            genesis_template.to_string_lossy().as_ref(),
            "--bootstrap-peer-ref",
            bootstrap_template.to_string_lossy().as_ref(),
            "--rpc-ref",
            "https://public-testnet.example.invalid/rpc",
            "--explorer-ref",
            "https://public-testnet.example.invalid/explorer",
            "--faucet-ref",
            "https://public-testnet.example.invalid/faucet",
            "--governance-mode",
            "shared_ops",
            "--validator-admission",
            "shared_allowlist",
            "--target-validator-count",
            "2",
            "--allow-observer-nodes",
            "true",
            "--token-symbol",
            "OC",
            "--faucet-mode",
            "guarded_testnet_faucet",
            "--reset-policy",
            "resettable",
            "--value-semantics",
            "testnet",
            "--promote-from",
            "local_devnet",
            "--require-gate",
            "runtime_bootstrap",
            "--allowed-claim",
            "public_testnet",
            "--denied-claim",
            "mainnet_live",
            "--denied-claim",
            "production_oc_settlement",
            "--evidence-ref",
            "generated-admission-test",
        ])
        .output()
        .expect("run network tier manifest generator");
    assert!(
        generated.status.success(),
        "manifest generation failed: stdout={} stderr={}",
        String::from_utf8_lossy(&generated.stdout),
        String::from_utf8_lossy(&generated.stderr)
    );

    let options = parse_options(
        [
            "--network-tier-manifest",
            manifest_path.to_string_lossy().as_ref(),
        ]
        .into_iter(),
    );
    assert!(
        options.is_ok(),
        "runtime must admit the manifest generated by the production generator: {options:?}"
    );

    let _ = fs::remove_dir_all(bundle_dir);
    let _ = fs::remove_dir_all(dir);
}

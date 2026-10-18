use super::*;
use std::{collections::BTreeMap, path::{Path, PathBuf}, process::{Command, Output}};
use super::super::execution_bridge_real_tests::real_execution_bridge::controlled_bootstrap_anchor::tests::{original_cli_fixture, absent_genesis_cli_fixture};
const CHILD: &str = "controlled_bootstrap_cli::tests::offline_dispatch_child";
#[test]
#[ignore = "internal subprocess helper; ordinary parent invokes it"]
fn offline_dispatch_child() {
    let args: Vec<String> =
        serde_json::from_str(&std::env::var("OASIS7_BOOTSTRAP_PROBE_ARGS").unwrap()).unwrap();
    let expected = std::env::var("OASIS7_BOOTSTRAP_PROBE_SUCCESS").unwrap() == "true";
    let result = super::super::identity_receipt::dispatch(args.iter().map(String::as_str))
        .expect("offline command must dispatch before startup options");
    assert_eq!(result.is_ok(), expected, "{result:?}");
}
fn fingerprint(dir: &Path) -> BTreeMap<PathBuf, String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            assert!(!p.is_dir(), "offline verifier created a directory");
            let value = if p.is_symlink() {
                format!("link:{}", std::fs::read_link(&p).unwrap().display())
            } else {
                oasis7_runtime::blake3_hex(&std::fs::read(&p).unwrap())
            };
            (p, value)
        })
        .collect()
}
fn invoke(root: &Path, config: &Path, proof: &Path, expected: bool, real: Option<&Path>) -> Output {
    let args = vec![
        "verify-controlled-bootstrap".to_string(),
        "--trusted-config".into(),
        config.display().to_string(),
        "--evidence".into(),
        proof.display().to_string(),
    ];
    let mut cmd = if let Some(binary) = real {
        let mut c = Command::new(binary);
        c.args(&args);
        c
    } else {
        let mut c = Command::new(std::env::current_exe().unwrap());
        c.args(["--ignored", "--exact", CHILD, "--nocapture"]);
        c.env(
            "OASIS7_BOOTSTRAP_PROBE_ARGS",
            serde_json::to_string(&args).unwrap(),
        );
        c.env("OASIS7_BOOTSTRAP_PROBE_SUCCESS", expected.to_string());
        c
    };
    let out = cmd.current_dir(root).output().unwrap();
    if real.is_some() {
        assert_eq!(
            out.status.success(),
            expected,
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    } else {
        assert!(
            out.status.success(),
            "{} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    out
}
#[test]
fn offline_bootstrap_process_success_and_failures_preserve_original_inputs() {
    let root = std::env::temp_dir().join(format!(
        "bootstrap-cli-process-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let (config, proof) = original_cli_fixture();
    let config_path = root.join("trust.json");
    let proof_path = root.join("proof.json");
    std::fs::write(&config_path, &config).unwrap();
    std::fs::write(&proof_path, &proof).unwrap();
    let mut cases = Vec::new();
    for case in 0..8 {
        let mut value: serde_json::Value = serde_json::from_slice(&config).unwrap();
        match case {
            0 => value["unknown"] = true.into(),
            1 => value["release_security_policy"]["unknown"] = true.into(),
            2 => value["issuer_policy"]["trust"]["unknown"] = true.into(),
            3 => value["schema_version"] = 2.into(),
            4 => value["minimum_head"]["position"] = 1.into(),
            5 => value["issuer_policy"]["issuer_public_key"] = "0".repeat(64).into(),
            6 => value["issuer_policy"]["profile"] = "bft".into(),
            _ => value["issuer_policy"]["execution_manifest_root"] = "0".repeat(64).into(),
        }
        let p = root.join(format!("bad-config-{case}.json"));
        std::fs::write(&p, serde_json::to_vec(&value).unwrap()).unwrap();
        cases.push((p, proof_path.clone()));
    }
    let oversized = root.join("oversized-config.json");
    std::fs::write(&oversized, vec![b' '; 65537]).unwrap();
    cases.push((oversized, proof_path.clone()));
    let mut bad: serde_json::Value = serde_json::from_slice(&proof).unwrap();
    bad["primary_receipt"]["signature_hex"] = "0".repeat(128).into();
    let bad_proof = root.join("bad-proof.json");
    std::fs::write(&bad_proof, serde_json::to_vec(&bad).unwrap()).unwrap();
    cases.push((config_path.clone(), bad_proof));
    let trailing = root.join("trailing-proof.json");
    let mut bytes = proof.clone();
    bytes.extend(b" null");
    std::fs::write(&trailing, bytes).unwrap();
    cases.push((config_path.clone(), trailing));
    let link = root.join("symlink-config.json");
    std::os::unix::fs::symlink(&config_path, &link).unwrap();
    cases.push((link, proof_path.clone()));
    let (absent_config, absent_proof) = absent_genesis_cli_fixture();
    let absent_config_path = root.join("absent-genesis-config.json");
    let absent_proof_path = root.join("absent-genesis-proof.json");
    std::fs::write(&absent_config_path, absent_config).unwrap();
    std::fs::write(&absent_proof_path, absent_proof).unwrap();
    cases.push((absent_config_path, absent_proof_path));
    let original = fingerprint(&root);
    let real = std::env::var_os("OASIS7_BOOTSTRAP_CLI_TEST_BINARY").map(PathBuf::from);
    for binary in [None, real.as_deref()]
        .into_iter()
        .take(if real.is_some() { 2 } else { 1 })
    {
        let output = invoke(&root, &config_path, &proof_path, true, binary);
        let stdout = String::from_utf8(output.stdout).unwrap();
        let summary: serde_json::Value = stdout
            .lines()
            .find_map(|line| {
                serde_json::from_str::<serde_json::Value>(line)
                    .ok()
                    .filter(|v| v.get("scope").is_some())
            })
            .expect("actual verified summary");
        assert_eq!(summary["execution_height"], 41);
        assert_eq!(summary["simulation_tick"], 7);
        assert_eq!(
            summary["scope"],
            "offline_certificate_bound_no_state_change_bootstrap_prerequisite"
        );
        for (cfg, evidence) in &cases {
            invoke(&root, cfg, evidence, false, binary);
        }
        assert_eq!(fingerprint(&root), original);
    }
    if let Some(destination) = std::env::var_os("OASIS7_BOOTSTRAP_CLI_TEST_EVIDENCE") {
        let destination = PathBuf::from(destination);
        std::fs::create_dir_all(&destination).unwrap();
        std::fs::write(destination.join("original-config.json"), config).unwrap();
        std::fs::write(destination.join("original-proof.json"), proof).unwrap();
        std::fs::write(
            destination.join("process-input-hashes.json"),
            serde_json::to_vec_pretty(
                &original
                    .iter()
                    .map(|(p, h)| (p.file_name().unwrap().to_string_lossy().to_string(), h))
                    .collect::<BTreeMap<_, _>>(),
            )
            .unwrap(),
        )
        .unwrap();
    }
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn offline_bootstrap_arguments_reject_duplicates_and_startup_flags() {
    assert!(run(["--trusted-config"].into_iter()).is_err());
    assert!(run(["--evidence", "a", "--evidence", "b"].into_iter()).is_err());
    assert!(run(["--world-id", "new-world"].into_iter()).is_err());
}

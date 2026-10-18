#![allow(
    dead_code,
    reason = "Shared module release fixture helpers preserve existing compiler integration cases"
)]
use crate::runtime::*;
fn pos(x: i64, y: i64) -> oasis7_agent_api::world_action::GeoPos {
    oasis7_agent_api::world_action::GeoPos::new(x, y, 0)
}
use ed25519_dalek::{Signer, SigningKey};
use oasis7_agent_api::world_action::ResourceKind;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const SOURCE_COMPILER_ENV: &str = "OASIS7_MODULE_SOURCE_COMPILER";
const SOURCE_MAX_FILES_ENV: &str = "OASIS7_MODULE_SOURCE_MAX_FILES";
const SOURCE_COMPILE_TIMEOUT_MS_ENV: &str = "OASIS7_MODULE_SOURCE_COMPILE_TIMEOUT_MS";
const SOURCE_SANDBOX_SECRET_ENV: &str = "OASIS7_SOURCE_SANDBOX_TEST_SECRET";
pub(crate) static SOURCE_COMPILER_ENV_LOCK: Mutex<()> = Mutex::new(());
const TEST_FINALITY_SIGNER_NODE_1: &str = "governance.local.finality.signer.1";
const TEST_FINALITY_SIGNER_SEED_1: &str = "oasis7-governance-local-finality-signer-1-v1";
const TEST_FINALITY_SIGNER_NODE_2: &str = "governance.local.finality.signer.2";
const TEST_FINALITY_SIGNER_SEED_2: &str = "oasis7-governance-local-finality-signer-2-v1";

fn removed_old_brand_module_source_env(suffix: &str) -> String {
    ["AGENT", "WORLD", "MODULE", "SOURCE", suffix].join("_")
}

fn register_agent(world: &mut World, agent_id: &str) {
    world.submit_action(Action::RegisterAgent {
        agent_id: agent_id.to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register agent");
    world
        .set_agent_resource_balance(agent_id, ResourceKind::Electricity, 128)
        .expect("seed electricity");
    world
        .set_agent_resource_balance(agent_id, ResourceKind::Data, 64)
        .expect("seed data");
}

fn set_agent_resource(world: &mut World, agent_id: &str, kind: ResourceKind, amount: i64) {
    world
        .set_agent_resource_balance(agent_id, kind, amount)
        .expect("set agent resource balance");
}

fn base_manifest(module_id: &str, version: &str, wasm_hash: &str) -> ModuleManifest {
    ModuleManifest {
        module_id: module_id.to_string(),
        name: format!("module-{module_id}"),
        version: version.to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.to_string(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        artifact_identity: Some(signed_test_artifact_identity(wasm_hash)),
        limits: ModuleLimits::default(),
    }
}

fn set_test_governance_finality_epoch_snapshot(
    world: &mut World,
    threshold: u16,
    signer_node_ids: &[&str],
) {
    let epoch_len = world
        .governance_execution_policy()
        .epoch_length_ticks
        .max(1);
    let epoch_id = world.state().time / epoch_len;
    world
        .set_governance_finality_epoch_snapshot(GovernanceFinalityEpochSnapshot {
            epoch_id,
            threshold,
            signer_node_ids: signer_node_ids
                .iter()
                .map(|signer| signer.to_string())
                .collect(),
            ..GovernanceFinalityEpochSnapshot::default()
        })
        .expect("set test finality epoch snapshot");
}

fn test_finality_signing_key(seed_label: &str) -> SigningKey {
    let seed = sha256_hex(seed_label.as_bytes());
    let seed_bytes = hex::decode(seed).expect("decode test finality seed");
    let private_key_bytes: [u8; 32] = seed_bytes
        .as_slice()
        .try_into()
        .expect("test finality seed is 32 bytes");
    SigningKey::from_bytes(&private_key_bytes)
}

fn test_finality_seed_label(node_id: &str) -> &'static str {
    match node_id {
        TEST_FINALITY_SIGNER_NODE_1 => TEST_FINALITY_SIGNER_SEED_1,
        TEST_FINALITY_SIGNER_NODE_2 => TEST_FINALITY_SIGNER_SEED_2,
        _ => panic!("missing test finality seed for signer {node_id}"),
    }
}

fn extract_latest_module_apply_proposal_id(world: &World) -> ProposalId {
    world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ModuleInstalled { proposal_id, .. })
            | WorldEventBody::Domain(DomainEvent::ModuleUpgraded { proposal_id, .. })
            | WorldEventBody::Domain(DomainEvent::ModuleRollbackApplied { proposal_id, .. }) => {
                Some(*proposal_id)
            }
            _ => None,
        })
        .expect("module apply proposal id")
}

fn extract_queued_manifest_hash(world: &World, proposal_id: ProposalId) -> String {
    world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Governance(GovernanceEvent::Queued {
                proposal_id: event_proposal_id,
                manifest_hash,
                ..
            }) if *event_proposal_id == proposal_id => Some(manifest_hash.clone()),
            _ => None,
        })
        .expect("governance queued manifest hash")
}

fn extract_governance_applied_consensus_height(world: &World, proposal_id: ProposalId) -> u64 {
    world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Governance(GovernanceEvent::Applied {
                proposal_id: event_proposal_id,
                consensus_height: Some(consensus_height),
                ..
            }) if *event_proposal_id == proposal_id => Some(*consensus_height),
            _ => None,
        })
        .expect("governance applied consensus height")
}

fn build_external_finality_certificate(
    world: &World,
    proposal_id: ProposalId,
    manifest_hash: &str,
    consensus_height: u64,
) -> GovernanceFinalityCertificate {
    let epoch_len = world
        .governance_execution_policy()
        .epoch_length_ticks
        .max(1);
    let epoch_id = world.state().time / epoch_len;
    let snapshot = world
        .governance_finality_epoch_snapshots()
        .get(&epoch_id)
        .cloned()
        .expect("governance finality snapshot for current epoch");
    let min_unique_signers = snapshot.effective_min_unique_signers();
    let mut signatures = BTreeMap::new();
    for node_id in &snapshot.signer_node_ids {
        let payload = GovernanceFinalityCertificate::signing_payload_v1(
            proposal_id,
            manifest_hash,
            consensus_height,
            epoch_id,
            snapshot.validator_set_hash.as_str(),
            snapshot.stake_root.as_str(),
            snapshot.threshold_bps,
            min_unique_signers,
            node_id.as_str(),
        );
        let signing_key = test_finality_signing_key(test_finality_seed_label(node_id.as_str()));
        let signature = signing_key.sign(payload.as_slice());
        signatures.insert(
            node_id.clone(),
            format!(
                "{}{}",
                GovernanceFinalityCertificate::SIGNATURE_PREFIX_ED25519_V1,
                hex::encode(signature.to_bytes())
            ),
        );
    }

    GovernanceFinalityCertificate {
        proposal_id,
        manifest_hash: manifest_hash.to_string(),
        consensus_height,
        epoch_id,
        validator_set_hash: snapshot.validator_set_hash,
        stake_root: snapshot.stake_root,
        threshold_bps: snapshot.threshold_bps,
        min_unique_signers,
        threshold: min_unique_signers,
        signatures,
    }
}

fn derive_module_action_finality_certificate(
    world: &World,
    submit_action: impl FnOnce(&mut World),
) -> GovernanceFinalityCertificate {
    let mut simulated = world.clone();
    if !simulated
        .release_security_policy()
        .allow_local_finality_signing
    {
        let mut policy = simulated.release_security_policy().clone();
        policy.allow_local_finality_signing = true;
        simulated.set_release_security_policy(policy);
    }
    submit_action(&mut simulated);
    simulated.step().expect("simulate module action finality");
    let proposal_id = extract_latest_module_apply_proposal_id(&simulated);
    let manifest_hash = extract_queued_manifest_hash(&simulated, proposal_id);
    let consensus_height = extract_governance_applied_consensus_height(&simulated, proposal_id);
    build_external_finality_certificate(world, proposal_id, &manifest_hash, consensus_height)
}

fn assert_last_rejection_note(world: &World, action_id: ActionId, expected: &str) {
    let event = world.journal().events.last().expect("last event");
    let WorldEventBody::Domain(DomainEvent::ActionRejected {
        action_id: rejected_action_id,
        reason: RejectReason::RuleDenied { notes },
    }) = &event.body
    else {
        panic!(
            "expected action rejected rule denied event: {:?}",
            event.body
        );
    };
    assert_eq!(*rejected_action_id, action_id);
    assert!(
        notes.iter().any(|note| note.contains(expected)),
        "missing expected note `{expected}` in {notes:?}"
    );
}

fn sample_module_source_package() -> ModuleSourcePackage {
    ModuleSourcePackage {
        manifest_path: "Cargo.toml".to_string(),
        files: BTreeMap::from([
            (
                "Cargo.toml".to_string(),
                br#"[package]
name = "sample_module"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]
"#
                .to_vec(),
            ),
            (
                "src/lib.rs".to_string(),
                b"#[unsafe(no_mangle)] pub extern \"C\" fn reduce() {}".to_vec(),
            ),
        ]),
    }
}

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("duration since epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "oasis7-runtime-tests-{prefix}-{}-{unique}",
        std::process::id()
    ))
}

fn write_fake_source_compiler(script_path: &Path, produced_wasm_bytes: &str) {
    let script = format!(
        "#!/usr/bin/env bash\nset -euo pipefail\nout_path=\"$4\"\nprintf '%s' '{produced_wasm_bytes}' > \"$out_path\"\n"
    );
    write_executable_script(script_path, script.as_str());
}

fn write_executable_script(script_path: &Path, script: &str) {
    fs::write(script_path, script).expect("write fake source compiler");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(script_path, fs::Permissions::from_mode(0o755))
            .expect("chmod fake source compiler");
    }
}

struct EnvVarGuard {
    key: String,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn capture(key: impl Into<String>) -> Self {
        let key = key.into();
        Self {
            previous: std::env::var(key.as_str()).ok(),
            key,
        }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => {
                // SAFETY: This test/setup code mutates process environment in a controlled scope.
                unsafe {
                    crate::env_mut::set_var(self.key.as_str(), value);
                }
            }
            None => {
                // SAFETY: This test/setup code mutates process environment in a controlled scope.
                unsafe {
                    crate::env_mut::remove_var(self.key.as_str());
                }
            }
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(bytes))
}
const TEST_MODULE_ARTIFACT_SIGNER_NODE_ID: &str = "test.module.release.signer";

fn signed_test_artifact_identity(wasm_hash: &str) -> ModuleArtifactIdentity {
    let source_hash = sha256_hex(format!("test-src:{wasm_hash}").as_bytes());
    let build_manifest_hash = sha256_hex(b"test-build-manifest-v1");
    let payload = ModuleArtifactIdentity::signing_payload_v1(
        wasm_hash,
        source_hash.as_str(),
        build_manifest_hash.as_str(),
        TEST_MODULE_ARTIFACT_SIGNER_NODE_ID,
    );
    let signing_key = test_module_artifact_signing_key();
    let signature = signing_key.sign(payload.as_slice());
    ModuleArtifactIdentity {
        source_hash,
        build_manifest_hash,
        signer_node_id: TEST_MODULE_ARTIFACT_SIGNER_NODE_ID.to_string(),
        signature_scheme: ModuleArtifactIdentity::SIGNATURE_SCHEME_ED25519.to_string(),
        artifact_signature: format!(
            "{}{}",
            ModuleArtifactIdentity::SIGNATURE_PREFIX_ED25519_V1,
            hex::encode(signature.to_bytes())
        ),
    }
}

fn test_module_artifact_signing_key() -> SigningKey {
    // Deterministic testing key; production verification only trusts its public key.
    let seed = sha256_hex(b"oasis7-test-module-artifact-signer-v1");
    let seed_bytes = hex::decode(seed).expect("decode test module signing seed");
    let private_key_bytes: [u8; 32] = seed_bytes
        .as_slice()
        .try_into()
        .expect("test module signing seed is 32 bytes");
    SigningKey::from_bytes(&private_key_bytes)
}

#[test]
fn compile_module_artifact_from_source_registers_compiled_artifact() {
    let _env_lock = SOURCE_COMPILER_ENV_LOCK.lock().expect("lock compile env");
    let _env_guard = EnvVarGuard::capture(SOURCE_COMPILER_ENV);
    let removed_old_brand_compiler = removed_old_brand_module_source_env("COMPILER");
    let _removed_old_brand_env_guard = EnvVarGuard::capture(removed_old_brand_compiler.as_str());
    let temp_root = temp_dir("compile-module-artifact");
    fs::create_dir_all(&temp_root).expect("create temp dir");
    let compiler_script = temp_root.join("compiler.sh");
    let produced_wasm_bytes = "compiled-from-source-runtime";
    write_fake_source_compiler(compiler_script.as_path(), produced_wasm_bytes);
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        crate::env_mut::set_var(SOURCE_COMPILER_ENV, compiler_script.as_os_str());
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        crate::env_mut::remove_var(removed_old_brand_compiler.as_str());
    }

    let mut world = crate::runtime_host::world::new();
    register_agent(&mut world, "publisher-1");

    world.submit_action(Action::CompileModuleArtifactFromSource {
        publisher_agent_id: "publisher-1".to_string(),
        module_id: "m.loop.source.compile".to_string(),
        source_package: sample_module_source_package(),
    });
    world.step().expect("compile from source action");

    let wasm_bytes = produced_wasm_bytes.as_bytes().to_vec();
    let wasm_hash = sha256_hex(&wasm_bytes);
    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Domain(DomainEvent::ModuleArtifactDeployed {
            publisher_agent_id,
            wasm_hash: event_hash,
            bytes_len,
            ..
        })) if publisher_agent_id == "publisher-1" && event_hash == &wasm_hash && *bytes_len == wasm_bytes.len() as u64
    ));
    assert_eq!(
        world.state().module_artifact_owners.get(&wasm_hash),
        Some(&"publisher-1".to_string())
    );
    assert_eq!(
        world
            .load_module(&wasm_hash)
            .expect("load compiled module")
            .bytes,
        wasm_bytes.into()
    );

    let _ = fs::remove_dir_all(temp_root);
}

#[test]
fn compile_module_artifact_from_source_rejects_removed_old_brand_compiler_env() {
    let _env_lock = SOURCE_COMPILER_ENV_LOCK.lock().expect("lock compile env");
    let _env_guard = EnvVarGuard::capture(SOURCE_COMPILER_ENV);
    let removed_old_brand_compiler = removed_old_brand_module_source_env("COMPILER");
    let _removed_old_brand_env_guard = EnvVarGuard::capture(removed_old_brand_compiler.as_str());

    let temp_root = temp_dir("compile-module-artifact-prefers-oasis7-env");
    fs::create_dir_all(&temp_root).expect("create temp dir");
    let primary_script = temp_root.join("compiler-primary.sh");
    let removed_old_brand_script = temp_root.join("compiler-removed-old-brand.sh");
    write_fake_source_compiler(primary_script.as_path(), "compiled-from-oasis7-env");
    write_fake_source_compiler(
        removed_old_brand_script.as_path(),
        "compiled-from-removed-old-brand-env",
    );
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        crate::env_mut::set_var(SOURCE_COMPILER_ENV, primary_script.as_os_str());
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        crate::env_mut::set_var(
            removed_old_brand_compiler.as_str(),
            removed_old_brand_script.as_os_str(),
        );
    }

    let mut world = crate::runtime_host::world::new();
    register_agent(&mut world, "publisher-1");

    world.submit_action(Action::CompileModuleArtifactFromSource {
        publisher_agent_id: "publisher-1".to_string(),
        module_id: "m.loop.source.compile.prefers-oasis7".to_string(),
        source_package: sample_module_source_package(),
    });
    world.step().expect("compile from source action");

    let wasm_bytes = b"compiled-from-oasis7-env".to_vec();
    let wasm_hash = sha256_hex(&wasm_bytes);
    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Domain(DomainEvent::ModuleArtifactDeployed {
            publisher_agent_id,
            wasm_hash: event_hash,
            bytes_len,
            ..
        })) if publisher_agent_id == "publisher-1" && event_hash == &wasm_hash && *bytes_len == wasm_bytes.len() as u64
    ));

    let _ = fs::remove_dir_all(temp_root);
}

#[test]
fn compile_module_artifact_from_source_rejects_in_production_release_policy() {
    let _env_lock = SOURCE_COMPILER_ENV_LOCK.lock().expect("lock compile env");
    let _env_guard = EnvVarGuard::capture(SOURCE_COMPILER_ENV);
    let removed_old_brand_compiler = removed_old_brand_module_source_env("COMPILER");
    let _removed_old_brand_env_guard = EnvVarGuard::capture(removed_old_brand_compiler.as_str());

    let temp_root = temp_dir("compile-module-artifact-production-disabled");
    fs::create_dir_all(&temp_root).expect("create temp dir");
    let compiler_script = temp_root.join("compiler.sh");
    write_fake_source_compiler(compiler_script.as_path(), "compiled-in-production");
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        crate::env_mut::set_var(SOURCE_COMPILER_ENV, compiler_script.as_os_str());
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        crate::env_mut::remove_var(removed_old_brand_compiler.as_str());
    }

    let mut world = crate::runtime_host::world::new();
    world.enable_production_release_policy();
    register_agent(&mut world, "publisher-1");

    let action_id = world.submit_action(Action::CompileModuleArtifactFromSource {
        publisher_agent_id: "publisher-1".to_string(),
        module_id: "m.loop.source.production-disabled".to_string(),
        source_package: sample_module_source_package(),
    });
    world
        .step()
        .expect("compile source rejected by production policy");

    assert_last_rejection_note(
        &world,
        action_id,
        "runtime source compile is disabled by production release policy",
    );

    let _ = fs::remove_dir_all(temp_root);
}

#[test]
fn compile_module_artifact_from_source_rejects_when_manifest_path_missing_in_files() {
    let mut world = crate::runtime_host::world::new();
    register_agent(&mut world, "publisher-1");

    let action_id = world.submit_action(Action::CompileModuleArtifactFromSource {
        publisher_agent_id: "publisher-1".to_string(),
        module_id: "m.loop.source.invalid".to_string(),
        source_package: ModuleSourcePackage {
            manifest_path: "Cargo.toml".to_string(),
            files: BTreeMap::from([(
                "src/lib.rs".to_string(),
                b"#[unsafe(no_mangle)] pub extern \"C\" fn reduce() {}".to_vec(),
            )]),
        },
    });
    world.step().expect("compile invalid source action");

    assert_last_rejection_note(&world, action_id, "manifest path missing");
}

#[test]
fn compile_module_artifact_from_source_rejects_when_file_count_exceeds_limit() {
    let _env_lock = SOURCE_COMPILER_ENV_LOCK.lock().expect("lock compile env");
    let _env_guard = EnvVarGuard::capture(SOURCE_MAX_FILES_ENV);
    let removed_old_brand_max_files = removed_old_brand_module_source_env("MAX_FILES");
    let _removed_old_brand_env_guard = EnvVarGuard::capture(removed_old_brand_max_files.as_str());
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        crate::env_mut::set_var(SOURCE_MAX_FILES_ENV, "1");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        crate::env_mut::remove_var(removed_old_brand_max_files.as_str());
    }

    let mut world = crate::runtime_host::world::new();
    register_agent(&mut world, "publisher-1");

    let action_id = world.submit_action(Action::CompileModuleArtifactFromSource {
        publisher_agent_id: "publisher-1".to_string(),
        module_id: "m.loop.source.too-many-files".to_string(),
        source_package: sample_module_source_package(),
    });
    world.step().expect("compile source with too many files");

    assert_last_rejection_note(&world, action_id, "source file count exceeds limit");
}

#[test]
fn compile_module_artifact_from_source_rejects_when_compiler_times_out() {
    let _env_lock = SOURCE_COMPILER_ENV_LOCK.lock().expect("lock compile env");
    let _compiler_guard = EnvVarGuard::capture(SOURCE_COMPILER_ENV);
    let removed_old_brand_compiler = removed_old_brand_module_source_env("COMPILER");
    let _removed_old_brand_compiler_guard =
        EnvVarGuard::capture(removed_old_brand_compiler.as_str());
    let _timeout_guard = EnvVarGuard::capture(SOURCE_COMPILE_TIMEOUT_MS_ENV);

    let temp_root = temp_dir("compile-module-artifact-timeout");
    fs::create_dir_all(&temp_root).expect("create temp dir");
    let compiler_script = temp_root.join("compiler-timeout.sh");
    write_executable_script(
        compiler_script.as_path(),
        "#!/usr/bin/env bash\nset -euo pipefail\nsleep 1\nprintf '%s' 'late-module' > \"$4\"\n",
    );
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        crate::env_mut::set_var(SOURCE_COMPILER_ENV, compiler_script.as_os_str());
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        crate::env_mut::remove_var(removed_old_brand_compiler.as_str());
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        crate::env_mut::set_var(SOURCE_COMPILE_TIMEOUT_MS_ENV, "20");
    }

    let mut world = crate::runtime_host::world::new();
    register_agent(&mut world, "publisher-1");

    let action_id = world.submit_action(Action::CompileModuleArtifactFromSource {
        publisher_agent_id: "publisher-1".to_string(),
        module_id: "m.loop.source.timeout".to_string(),
        source_package: sample_module_source_package(),
    });
    world.step().expect("compile timeout action");

    assert_last_rejection_note(&world, action_id, "compiler timed out");
    let _ = fs::remove_dir_all(temp_root);
}

#[test]
fn compile_module_artifact_from_source_sanitizes_env_and_isolates_tmpdir() {
    let _env_lock = SOURCE_COMPILER_ENV_LOCK.lock().expect("lock compile env");
    let _compiler_guard = EnvVarGuard::capture(SOURCE_COMPILER_ENV);
    let removed_old_brand_compiler = removed_old_brand_module_source_env("COMPILER");
    let _removed_old_brand_compiler_guard =
        EnvVarGuard::capture(removed_old_brand_compiler.as_str());
    let _secret_guard = EnvVarGuard::capture(SOURCE_SANDBOX_SECRET_ENV);

    let temp_root = temp_dir("compile-module-artifact-sandbox-env");
    fs::create_dir_all(&temp_root).expect("create temp dir");
    let compiler_script = temp_root.join("compiler-sandbox-env.sh");
    write_executable_script(
        compiler_script.as_path(),
        format!(
            "#!/usr/bin/env bash\nset -euo pipefail\nworkspace=\"$2\"\nif [[ -n \"${{{SOURCE_SANDBOX_SECRET_ENV}:-}}\" ]]; then\n  exit 17\nfi\nif [[ \"${{TMPDIR:-}}\" != \"$workspace/tmp\" ]]; then\n  exit 19\nfi\nprintf '%s' 'compiled-sandbox-env' > \"$4\"\n"
        )
        .as_str(),
    );
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        crate::env_mut::set_var(SOURCE_COMPILER_ENV, compiler_script.as_os_str());
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        crate::env_mut::remove_var(removed_old_brand_compiler.as_str());
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        crate::env_mut::set_var(SOURCE_SANDBOX_SECRET_ENV, "must-not-leak");
    }

    let mut world = crate::runtime_host::world::new();
    register_agent(&mut world, "publisher-1");

    world.submit_action(Action::CompileModuleArtifactFromSource {
        publisher_agent_id: "publisher-1".to_string(),
        module_id: "m.loop.source.sandbox-env".to_string(),
        source_package: sample_module_source_package(),
    });
    world.step().expect("compile source with sandbox env");

    let wasm_bytes = b"compiled-sandbox-env".to_vec();
    let wasm_hash = sha256_hex(&wasm_bytes);
    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Domain(DomainEvent::ModuleArtifactDeployed {
            publisher_agent_id,
            wasm_hash: event_hash,
            ..
        })) if publisher_agent_id == "publisher-1" && event_hash == &wasm_hash
    ));

    let _ = fs::remove_dir_all(temp_root);
}

#[test]
fn compiler_capability_requires_host_assembly_after_snapshot_and_serde() {
    fn assert_host_installed(mut world: World) {
        register_agent(&mut world, "capability-publisher");
        let action = world.submit_action(Action::CompileModuleArtifactFromSource {
            publisher_agent_id: "capability-publisher".into(),
            module_id: "host.capability.probe".into(),
            source_package: ModuleSourcePackage {
                manifest_path: String::new(),
                files: Default::default(),
            },
        });
        world.step().unwrap();
        assert_last_rejection_note(&world, action, "manifest_path is empty");
    }
    let host = crate::runtime_host::world::new();
    assert_host_installed(host.clone());
    assert_host_installed(
        crate::runtime_host::world::from_snapshot(host.snapshot(), host.journal().clone()).unwrap(),
    );
    let encoded = serde_json::to_vec(&host).unwrap();
    let decoded: World = serde_json::from_slice(&encoded).unwrap();
    let decoded = crate::runtime_host::install_runtime_capabilities(decoded);
    assert_eq!(encoded, serde_json::to_vec(&decoded).unwrap());
    assert_host_installed(decoded);
}

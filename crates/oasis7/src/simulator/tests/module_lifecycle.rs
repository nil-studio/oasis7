use super::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

const SOURCE_COMPILER_ENV: &str = "OASIS7_MODULE_SOURCE_COMPILER";

fn source_compiler_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct EnvVarGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn capture(key: &'static str) -> Self {
        Self {
            key,
            previous: std::env::var(key).ok(),
        }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => {
                // SAFETY: This test/setup code mutates process environment in a controlled scope.
                unsafe {
                    oasis7::env_mut::set_var(self.key, value);
                }
            }
            None => {
                // SAFETY: This test/setup code mutates process environment in a controlled scope.
                unsafe {
                    oasis7::env_mut::remove_var(self.key);
                }
            }
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn setup_kernel_with_agent(agent_id: &str) -> WorldKernel {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-home".to_string(),
        name: "home".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: agent_id.to_string(),
        location_id: "loc-home".to_string(),
    });
    kernel.step_until_empty();
    kernel
}

fn write_fake_source_compiler(script_path: &Path, output_wasm_text: &str) {
    let script = format!(
        "#!/usr/bin/env bash\nset -euo pipefail\nout_path=\"$4\"\nmkdir -p \"$(dirname \"$out_path\")\"\nprintf '%s' '{}' > \"$out_path\"\n",
        output_wasm_text
    );
    fs::write(script_path, script).expect("write fake compiler script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(script_path)
            .expect("script metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(script_path, permissions).expect("set executable permission");
    }
}

#[test]
fn module_lifecycle_compile_from_source_deploys_artifact() {
    let _lock = source_compiler_env_lock().lock().expect("env lock");
    let _guard = EnvVarGuard::capture(SOURCE_COMPILER_ENV);
    let temp_root = std::env::temp_dir().join(format!(
        "oasis7-sim-compile-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&temp_root).expect("create temp root");

    let script_path = temp_root.join("compiler.sh");
    let compiled_text = "sim-compiled-wasm";
    write_fake_source_compiler(script_path.as_path(), compiled_text);
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(SOURCE_COMPILER_ENV, script_path.as_os_str());
    }

    let mut kernel = setup_kernel_with_agent("agent-1");
    kernel.submit_action(Action::CompileModuleArtifactFromSource {
        publisher_agent_id: "agent-1".to_string(),
        module_id: "m.sim.compile".to_string(),
        manifest_path: "Cargo.toml".to_string(),
        source_files: BTreeMap::from([
            (
                "Cargo.toml".to_string(),
                br#"[package]
name = \"m_sim_compile\"
version = \"0.1.0\"
edition = \"2024\""#
                    .to_vec(),
            ),
            (
                "src/lib.rs".to_string(),
                b"#[unsafe(no_mangle)] pub extern \"C\" fn tick() {}".to_vec(),
            ),
        ]),
    });

    let event = kernel.step().expect("compile event");
    let expected_bytes = compiled_text.as_bytes().to_vec();
    let expected_hash = sha256_hex(expected_bytes.as_slice());
    match event.kind {
        WorldEventKind::ModuleArtifactDeployed {
            publisher_agent_id,
            wasm_hash,
            bytes_len,
            module_id_hint,
            wasm_bytes,
        } => {
            assert_eq!(publisher_agent_id, "agent-1");
            assert_eq!(wasm_hash, expected_hash);
            assert_eq!(bytes_len, expected_bytes.len() as u64);
            assert_eq!(module_id_hint.as_deref(), Some("m.sim.compile"));
            assert_eq!(wasm_bytes, expected_bytes);
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let _ = fs::remove_dir_all(temp_root);
}

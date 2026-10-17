//! Preserve build inputs in the executable rather than consulting a new checkout at recovery.
use std::{collections::BTreeMap, env, fs, path::PathBuf, process::Command};
fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let mut values = BTreeMap::new();
    for key in [
        "TARGET",
        "CARGO_CFG_TARGET_OS",
        "CARGO_CFG_TARGET_ARCH",
        "CARGO_CFG_TARGET_FAMILY",
        "CARGO_CFG_TARGET_FEATURE",
        "PROFILE",
        "DEBUG",
        "OPT_LEVEL",
    ] {
        values.insert(key.to_string(), env::var(key).expect(key));
    }
    // Cargo supplies the effective flags with a unit separator. Preserve them
    // exactly alongside target/features, including otherwise invisible cfgs.
    for key in [
        "CARGO_ENCODED_RUSTFLAGS",
        "RUSTC_WORKSPACE_WRAPPER",
        "RUSTC_WRAPPER",
    ] {
        if let Ok(value) = env::var(key) {
            values.insert(key.to_string(), value);
        }
        println!("cargo:rerun-if-env-changed={key}");
    }
    for (key, value) in env::vars().filter(|(key, _)| key.starts_with("CARGO_FEATURE_")) {
        values.insert(key, value);
    }
    let metadata = values
        .iter()
        .map(|(key, value)| format!("{key}={value}\n"))
        .collect::<String>();
    fs::write(out.join("native-build-metadata.txt"), metadata).expect("write build metadata");
    let compiler = Command::new(env::var_os("RUSTC").expect("RUSTC"))
        .args(["--version", "--verbose"])
        .output()
        .expect("compiler version");
    assert!(compiler.status.success(), "compiler version failed");
    fs::write(out.join("native-toolchain.txt"), compiler.stdout).expect("write compiler metadata");
    let lock = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"))
        .join("../../Cargo.lock");
    println!("cargo:rerun-if-changed={}", lock.display());
    fs::copy(lock, out.join("native-Cargo.lock")).expect("preserve Cargo.lock");
    for key in ["TARGET", "CARGO_CFG_TARGET_OS", "CARGO_CFG_TARGET_ARCH"] {
        println!(
            "cargo:rustc-env=NATIVE_BUILD_{key}={}",
            env::var(key).expect(key)
        );
    }
}

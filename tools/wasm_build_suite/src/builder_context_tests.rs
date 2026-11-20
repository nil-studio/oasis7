//! The Docker builder image uses its own standalone workspace under
//! `docker/wasm-builder/` so root `Cargo.toml` / `Cargo.lock` changes never
//! leak into WASM producer recipes or `.wasm` bytecode.
const DOCKERFILE: &str = include_str!("../../../docker/wasm-builder/Dockerfile");
const CONTEXT: &str = include_str!("../../../.dockerignore");
const BUILDER_MANIFEST: &str = include_str!("../../../docker/wasm-builder/Cargo.toml");
const SDK_MANIFEST: &str = include_str!("../../../crates/oasis7_wasm_sdk/Cargo.toml");
const ABI_MANIFEST: &str = include_str!("../../../crates/oasis7_wasm_abi/Cargo.toml");
const BUILD_MANIFEST: &str = include_str!("../../../crates/oasis7_wasm_build/Cargo.toml");
const SUITE_MANIFEST: &str = include_str!("../../../tools/wasm_build_suite/Cargo.toml");

#[test]
fn builder_context_includes_only_the_shared_native_linker_installer() {
    let context: Vec<_> = CONTEXT.lines().collect();
    for required in [
        "!.github/",
        "!.github/actions/",
        "!.github/actions/setup-linux-mold/",
        "!.github/actions/setup-linux-mold/setup.py",
    ] {
        assert!(
            context.contains(&required),
            "missing installer input {required}"
        );
    }
    for broad in [
        "!.github/**",
        "!.github/actions/**",
        "!.github/actions/setup-linux-mold/**",
    ] {
        assert!(
            !context.contains(&broad),
            "unrelated workflow inputs must stay excluded"
        );
    }
    assert!(DOCKERFILE.lines().any(|line| line == "COPY --chmod=0755 .github/actions/setup-linux-mold/setup.py /opt/oasis7/.github/actions/setup-linux-mold/setup.py"));
}

#[test]
fn builder_context_uses_standalone_workspace_and_excludes_root_cargo_and_unrelated_crates() {
    let context: Vec<_> = CONTEXT.lines().collect();
    for required in [
        "!docker/wasm-builder/Dockerfile",
        "!docker/wasm-builder/Cargo.toml",
        "!docker/wasm-builder/Cargo.lock",
        "!docker/wasm-builder/src/**",
        "!crates/oasis7_wasm_build/**",
        "!tools/wasm_build_suite/**",
        "!scripts/build-wasm-module.sh",
    ] {
        assert!(
            context.contains(&required),
            "missing standalone builder context entry {required}"
        );
    }
    for forbidden in [
        "!Cargo.toml",
        "!Cargo.lock",
        "!crates/**",
        "!crates/*/Cargo.toml",
        "!tools/**",
        "!tools/*/Cargo.toml",
        "!docker/wasm-builder/prepare-workspace.sh",
    ] {
        assert!(
            !context.contains(&forbidden),
            "root workspace or unrelated crate entry must stay excluded: {forbidden}"
        );
    }
    assert!(
        !DOCKERFILE
            .lines()
            .any(|line| line.contains(" Cargo.toml ") || line.contains(" Cargo.lock ")),
        "Dockerfile must not copy root Cargo.toml/Cargo.lock"
    );
    assert!(BUILDER_MANIFEST.contains("[workspace]"));
    for excluded_target in [
        "crates/**/target",
        "crates/**/target/**",
        "tools/**/target",
        "tools/**/target/**",
        "docker/wasm-builder/target",
        "docker/wasm-builder/target/**",
    ] {
        assert!(
            context.contains(&excluded_target),
            "build outputs must remain excluded: {excluded_target}"
        );
    }
}

#[test]
fn wasm_sdk_and_producer_manifests_do_not_inherit_root_workspace() {
    for (name, manifest) in [
        ("crates/oasis7_wasm_sdk/Cargo.toml", SDK_MANIFEST),
        ("crates/oasis7_wasm_abi/Cargo.toml", ABI_MANIFEST),
        ("crates/oasis7_wasm_build/Cargo.toml", BUILD_MANIFEST),
        ("tools/wasm_build_suite/Cargo.toml", SUITE_MANIFEST),
    ] {
        assert!(
            !manifest.contains("workspace = true") && !manifest.contains(".workspace = true"),
            "{name} must stay self-contained without root workspace inheritance"
        );
    }
}

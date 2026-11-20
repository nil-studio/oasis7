use super::*;

const MODULE: &str = "crates/oasis7_builtin_wasm_modules/m4_factory_miner_mk1";
const SDK: &str = "crates/oasis7_wasm_sdk";
const TEMPLATE: &str =
    "crates/oasis7_builtin_wasm_modules/_templates/m4_factory_module_template.rs";

fn real_m4_fixture() -> Fixture {
    let fixture = Fixture::new();
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    for path in [
        format!("{MODULE}/Cargo.toml"),
        format!("{MODULE}/Cargo.lock"),
        format!("{MODULE}/src/lib.rs"),
        format!("{SDK}/Cargo.toml"),
        format!("{SDK}/src/lib.rs"),
        TEMPLATE.into(),
    ] {
        fixture.write(&path, &fs::read_to_string(repository.join(&path)).unwrap());
    }
    // Preserve the real wrapper's crate-relative declaration and standalone
    // workspace. The SDK belongs to a different workspace without inheriting it.
    fixture.write(
        "Cargo.toml",
        "[workspace]\nmembers=[\"crates/oasis7_wasm_sdk\"]\nresolver=\"2\"\n",
    );
    fixture.write(
        "Cargo.lock",
        &fs::read_to_string(fixture.0.join(format!("{MODULE}/Cargo.lock"))).unwrap(),
    );
    fixture
}

fn m4_inputs(fixture: &Fixture) -> SourceInputs {
    compute_source_inputs(
        &fixture.0.join(format!("{MODULE}/Cargo.toml")),
        DEFAULT_WASM_TARGET,
    )
    .unwrap()
}

#[test]
fn standalone_m4_inputs_ignore_unconsumed_root_manifest_and_lockfile() {
    let fixture = real_m4_fixture();
    let baseline = m4_inputs(&fixture);
    for path in ["Cargo.toml", "Cargo.lock"] {
        let original = fs::read_to_string(fixture.0.join(path)).unwrap();
        fixture.write(
            path,
            &format!("{original}\n# unrelated root workspace edit\n"),
        );
        assert_eq!(
            baseline,
            m4_inputs(&fixture),
            "an unconsumed {path} must not invalidate standalone module evidence"
        );
        fixture.write(path, &original);
    }
    assert!(
        baseline
            .source_inputs
            .iter()
            .all(|input| !matches!(input.path.as_str(), "Cargo.toml" | "Cargo.lock"))
    );
    for path in [
        format!("{MODULE}/Cargo.toml"),
        format!("{MODULE}/Cargo.lock"),
        format!("{SDK}/src/lib.rs"),
        TEMPLATE.into(),
    ] {
        assert!(
            baseline
                .source_inputs
                .iter()
                .any(|input| input.path == path),
            "source-root labels must retain the original repository path {path}"
        );
    }
}

#[test]
fn standalone_m4_inputs_track_sdk_template_and_active_build_workspace() {
    let fixture = real_m4_fixture();
    let baseline = m4_inputs(&fixture);
    for (path, addition) in [
        (
            format!("{SDK}/src/lib.rs"),
            "\npub const SOURCE_INPUT_FIXTURE_VALUE: u32 = 1;\n",
        ),
        (
            TEMPLATE.into(),
            "\nconst SOURCE_INPUT_FIXTURE_VALUE: u32 = 1;\n",
        ),
        (
            format!("{MODULE}/Cargo.toml"),
            "\n[profile.release]\nopt-level = \"s\"\n",
        ),
        (
            format!("{MODULE}/Cargo.lock"),
            "\n# active build workspace lockfile edit\n",
        ),
    ] {
        let original = fs::read_to_string(fixture.0.join(&path)).unwrap();
        fixture.write(&path, &format!("{original}{addition}"));
        assert_ne!(
            baseline.source_hash,
            m4_inputs(&fixture).source_hash,
            "the actual build input {path} must invalidate source identity"
        );
        fixture.write(&path, &original);
        assert_eq!(baseline, m4_inputs(&fixture));
    }
}

#[test]
fn active_virtual_workspace_profile_is_an_input_without_package_inheritance() {
    let fixture = Fixture::new();
    let baseline = fixture.inputs().unwrap();
    let original = fs::read_to_string(fixture.0.join("Cargo.toml")).unwrap();
    fixture.write(
        "Cargo.toml",
        &format!("{original}\n[profile.release]\nopt-level = \"s\"\n"),
    );
    let changed = fixture.inputs().unwrap();
    assert!(
        changed
            .source_inputs
            .iter()
            .any(|input| input.path == "Cargo.toml")
    );
    assert_ne!(baseline.source_hash, changed.source_hash);
}

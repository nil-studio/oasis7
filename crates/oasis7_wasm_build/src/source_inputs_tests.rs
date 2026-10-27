use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "oasis7-source-v2-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let f = Self(root.canonicalize().unwrap());
        f.write(
            "Cargo.toml",
            "[workspace]\nmembers=[\"module\"]\nresolver=\"2\"\n",
        );
        f.write(
            "module/Cargo.toml",
            "[package]\nname=\"fixture\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
        );
        f.write(
            "module/src/lib.rs",
            "include!(\"../../templates/value.rs\");",
        );
        f.write("templates/value.rs", "pub const VALUE:u32=1;");
        f.declare(
            vec![Edge {
                from: "module/src/lib.rs".into(),
                to: "templates/value.rs".into(),
            }],
            BTreeMap::new(),
        );
        f
    }
    fn write(&self, name: &str, text: &str) {
        let p = self.0.join(name);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    fn declare(&self, includes: Vec<Edge>, build_inputs: BTreeMap<String, Vec<String>>) {
        let manifest = self.0.join("module/Cargo.toml");
        let old = fs::read_to_string(&manifest).unwrap();
        let prefix = old
            .split("[package.metadata.oasis7_source_inputs]")
            .next()
            .unwrap();
        let mut text = format!(
            "{}\n[package.metadata.oasis7_source_inputs]\nrule_version=2\nincludes=[",
            prefix.trim_end()
        );
        for edge in includes {
            text.push_str(&format!(
                "{{from={},to={}}},",
                serde_json::to_string(&edge.from).unwrap(),
                serde_json::to_string(&edge.to).unwrap()
            ));
        }
        text.push_str("]\n[package.metadata.oasis7_source_inputs.build_inputs]\n");
        for (from, inputs) in build_inputs {
            text.push_str(&format!(
                "{}={}\n",
                serde_json::to_string(&from).unwrap(),
                serde_json::to_string(&inputs).unwrap()
            ));
        }
        fs::write(manifest, text).unwrap();
    }

    fn inputs(&self) -> Result<SourceInputs, SourceHashError> {
        compute_source_inputs(&self.0.join("module/Cargo.toml"), DEFAULT_WASM_TARGET)
    }
    fn error(&self) -> String {
        self.inputs().unwrap_err().to_string()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn transitive_literals_and_declared_build_data_are_inputs() {
    let f = Fixture::new();
    f.write("templates/value.rs", "pub const TEXT:&str=include_str!(\"nested/data.txt\"); pub const BYTES:&[u8]=include_bytes!(\"nested/data.txt\");");
    f.write("templates/nested/data.txt", "one");
    f.write(
        "module/build.rs",
        "fn main(){ let _=std::fs::read_to_string(\"../build-data.txt\").unwrap(); }",
    );
    f.write("build-data.txt", "before");
    f.declare(
        vec![
            Edge {
                from: "module/src/lib.rs".into(),
                to: "templates/value.rs".into(),
            },
            Edge {
                from: "templates/value.rs".into(),
                to: "templates/nested/data.txt".into(),
            },
        ],
        BTreeMap::from([("module/build.rs".into(), vec!["build-data.txt".into()])]),
    );
    let first = f.inputs().unwrap();
    f.write("templates/nested/data.txt", "two");
    let second = f.inputs().unwrap();
    assert_ne!(first.source_hash, second.source_hash);
    f.write("build-data.txt", "after");
    assert_ne!(second.source_hash, f.inputs().unwrap().source_hash);
    assert_eq!(
        first
            .source_inputs
            .iter()
            .filter(|r| r.path == "templates/nested/data.txt")
            .count(),
        1
    );
}

#[test]
fn unknown_dynamic_missing_and_undeclared_inputs_fail_closed() {
    let f = Fixture::new();
    for source in [
        "include!(concat!(\"../../templates/\",\"value.rs\"));",
        "include!(env!(\"SOURCE\"));",
        "macro_rules! hidden {()=>{include!(\"../../templates/value.rs\");}}",
        "#[path=\"../../templates/value.rs\"] mod hidden;",
    ] {
        f.write("module/src/lib.rs", source);
        assert!(f.error().contains("unsupported-input"));
    }
    f.write(
        "module/src/lib.rs",
        "include!(\"../../templates/value.rs\");",
    );
    f.declare(vec![], BTreeMap::new());
    assert!(f.error().contains("undeclared"));
    f.write("module/src/lib.rs", "pub fn value(){}");
    f.write(
        "module/build.rs",
        "fn main(){let _=std::fs::read_to_string(\"../build-data.txt\");}",
    );
    assert!(f.error().contains("No such file") || f.error().contains("os error"));
    f.write("build-data.txt", "data");
    assert!(f.error().contains("build.rs requires"));
    f.write(
        "module/build.rs",
        "fn main(){let _=std::env::var(\"INPUT\");}",
    );
    assert!(f.error().contains("build script supports"));
}

#[test]
fn cycles_escape_missing_symlinks_and_non_utf8_fail_closed() {
    let f = Fixture::new();
    f.write("templates/value.rs", "include!(\"value.rs\");");
    f.declare(
        vec![
            Edge {
                from: "module/src/lib.rs".into(),
                to: "templates/value.rs".into(),
            },
            Edge {
                from: "templates/value.rs".into(),
                to: "templates/value.rs".into(),
            },
        ],
        BTreeMap::new(),
    );
    assert!(f.error().contains("cycle"));
    f.write("templates/value.rs", "pub const VALUE:u32=1;");
    f.write("module/src/lib.rs", "include!(\"../../../escape.rs\");");
    assert!(f.error().contains("escapes"));
    f.write("module/src/lib.rs", "include!(\"/absolute.rs\");");
    assert!(f.error().contains("absolute"));
    f.write(
        "module/src/lib.rs",
        "include!(\"../../templates/value.rs\");",
    );
    fs::remove_file(f.0.join("templates/value.rs")).unwrap();
    assert!(f.inputs().is_err());
    #[cfg(unix)]
    {
        use std::os::unix::{ffi::OsStringExt, fs::symlink};
        symlink(
            f.0.join("module/src/lib.rs"),
            f.0.join("templates/value.rs"),
        )
        .unwrap();
        assert!(f.error().contains("symlink"));
        fs::remove_file(f.0.join("templates/value.rs")).unwrap();
        f.write("templates/value.rs", "pub const VALUE:u32=1;");
        let invalid =
            f.0.join("module/src")
                .join(std::ffi::OsString::from_vec(vec![255]));
        assert!(
            label(&f.0, &invalid)
                .unwrap_err()
                .to_string()
                .contains("non-UTF-8")
        );
        #[cfg(target_os = "linux")]
        {
            fs::write(invalid, b"data").unwrap();
            assert!(f.error().contains("non-UTF-8"));
        }
    }
}

#[test]
fn declaration_order_and_checkout_location_do_not_change_identity() {
    let f = Fixture::new();
    let original = f.inputs().unwrap();
    let other = Fixture::new();
    assert_eq!(original, other.inputs().unwrap());
    // cwd does not enter the API; an absolute manifest remains independent of it.
    let result = Command::new("cargo")
        .current_dir(&other.0)
        .args(["locate-project", "--manifest-path"])
        .arg(f.0.join("module/Cargo.toml"))
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(original, f.inputs().unwrap());
}

#[test]
fn legacy_unknown_and_version_only_patches_do_not_validate() {
    let f = Fixture::new();
    let evidence = f.inputs().unwrap();
    assert!(
        validate_source_evidence(None, None, "old", &evidence)
            .unwrap_err()
            .to_string()
            .contains("legacy")
    );
    assert!(
        validate_source_evidence(
            Some(99),
            Some(&evidence.source_inputs),
            &evidence.source_hash,
            &evidence
        )
        .is_err()
    );
    assert!(validate_source_evidence(Some(2), None, "old", &evidence).is_err());
    assert!(
        validate_source_evidence(Some(2), Some(&evidence.source_inputs), "old", &evidence).is_err()
    );
    validate_source_evidence(
        Some(2),
        Some(&evidence.source_inputs),
        &evidence.source_hash,
        &evidence,
    )
    .unwrap();
    f.write("templates/value.rs", "pub const VALUE:u32=2;");
    assert!(
        validate_source_evidence(
            Some(2),
            Some(&evidence.source_inputs),
            &evidence.source_hash,
            &f.inputs().unwrap()
        )
        .is_err()
    );
}

#[test]
fn fixed_binary_encoding_vector() {
    let records = vec![SourceInputRecord {
        kind: "source-file".into(),
        path: "src/a\nβ.rs".into(),
        sha256: "00".repeat(32),
    }];
    // Independent Python hashlib vector: fixed domain, BE-u64 byte lengths/count, raw digest.
    assert_eq!(
        hash_records("module/Cargo.toml", "wasm32-unknown-unknown", &records).unwrap(),
        "b8cd4c44cb2622d204afe4ea9febc868157a7b38a82d8e8d1bbd9e2bb18d9905"
    );
}

#[test]
fn all_22_real_wrapper_declarations_track_only_their_template() {
    let f = Fixture::new();
    fs::remove_dir_all(f.0.join("module")).unwrap();
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let modules = repository.join("crates/oasis7_builtin_wasm_modules");
    let mut wrappers = Vec::new();
    for dir in fs::read_dir(&modules).unwrap() {
        let dir = dir.unwrap().path();
        let manifest = dir.join("Cargo.toml");
        let Ok(text) = fs::read_to_string(&manifest) else {
            continue;
        };
        let Some((_, declaration)) = text.split_once("[package.metadata.oasis7_source_inputs]")
        else {
            continue;
        };
        let rel = dir.strip_prefix(repository).unwrap();
        let name = dir.file_name().unwrap().to_str().unwrap();
        f.write(&format!("{}/Cargo.toml",rel.display()),&format!("[package]\nname=\"{name}\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[package.metadata.oasis7_source_inputs]{declaration}"));
        let source = fs::read_to_string(dir.join("src/lib.rs")).unwrap();
        f.write(&format!("{}/src/lib.rs", rel.display()), &source);
        let literal = source
            .lines()
            .find(|line| line.starts_with("include!("))
            .unwrap()
            .split('"')
            .nth(1)
            .unwrap();
        let template = dir
            .join("src")
            .join(literal)
            .canonicalize()
            .unwrap()
            .strip_prefix(repository)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        wrappers.push((rel.to_path_buf(), template));
    }
    assert_eq!(wrappers.len(), 22);
    f.write(
        "unrelated/Cargo.toml",
        "[package]\nname=\"unrelated\"\nversion=\"0.1.0\"\n",
    );
    f.write("unrelated/src/lib.rs", "pub fn value(){}");
    f.write(
        "Cargo.toml",
        &format!(
            "[workspace]\nresolver=\"2\"\nmembers=[{},\"unrelated\"]\n",
            wrappers
                .iter()
                .map(|(p, _)| format!("{:?}", p.to_str().unwrap()))
                .collect::<Vec<_>>()
                .join(",")
        ),
    );
    let templates = wrappers
        .iter()
        .map(|(_, v)| v.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(templates.len(), 3);
    for template in &templates {
        f.write(
            template,
            &fs::read_to_string(repository.join(template)).unwrap(),
        );
    }
    let hash = |p: &Path| {
        compute_source_hash(&f.0.join(p).join("Cargo.toml"), DEFAULT_WASM_TARGET).unwrap()
    };
    let before = wrappers.iter().map(|(p, _)| hash(p)).collect::<Vec<_>>();
    let unrelated = hash(Path::new("unrelated"));
    for template in templates {
        let source = fs::read_to_string(f.0.join(&template)).unwrap();
        f.write(
            &template,
            &format!("{source}\n// one changed template input\n"),
        );
        for ((path, dependency), old) in wrappers.iter().zip(&before) {
            assert_eq!(
                hash(path) != *old,
                *dependency == template,
                "{}",
                path.display()
            );
        }
        assert_eq!(unrelated, hash(Path::new("unrelated")));
        f.write(&template, &source);
    }
}

#[test]
fn data_with_rs_suffix_is_not_parsed_as_rust_and_macro_inputs_are_rejected() {
    let f = Fixture::new();
    f.write("module/src/lib.rs", "pub const DATA:&[u8]=include_bytes!(\"data.rs\"); pub const TEXT:&str=include_str!(\"text.rs\");");
    fs::write(f.0.join("module/src/data.rs"), [0xff, 0, 1]).unwrap();
    f.write("module/src/text.rs", "this is plain text, not Rust!");
    f.declare(vec![], BTreeMap::new());
    f.inputs().unwrap();
    for source in [
        "opaque!(include_str!(\"data.rs\"));",
        "opaque!(include_bytes!(concat!(\"data\",\".rs\")));",
        "opaque!(env!(\"INPUT\"));",
        "opaque!(\"some input\");",
    ] {
        f.write("module/src/lib.rs", source);
        assert!(f.error().contains("unsupported"));
    }
    f.write("module/src/lib.rs", "pub fn value(){}");
    f.declare(vec![], BTreeMap::from([("module/build.rs".into(), vec![])]));
    for source in [
        "fn main(){unknown_macro!(std::fs::read(\"../secret\"));}",
        "fn main(){println!(\"{:?}\",std::fs::read(\"../secret\"));}",
        "fn main(){assert!(helper_reads_env());}",
    ] {
        f.write("module/build.rs", source);
        assert!(
            f.inputs().is_err(),
            "hidden build input must reject: {source}"
        );
    }
}

#[test]
fn actual_repository_wrapper_closures_support_the_real_local_dependency_lane() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut count = 0;
    for dir in fs::read_dir(repository.join("crates/oasis7_builtin_wasm_modules")).unwrap() {
        let dir = dir.unwrap().path();
        if !fs::read_to_string(dir.join("Cargo.toml"))
            .is_ok_and(|text| text.contains("[package.metadata.oasis7_source_inputs]"))
        {
            continue;
        }
        let evidence = compute_source_inputs(&dir.join("Cargo.toml"), DEFAULT_WASM_TARGET).unwrap();
        assert!(
            evidence
                .source_inputs
                .iter()
                .any(|r| r.path == "crates/oasis7_wasm_sdk/src/lib.rs")
        );
        assert_eq!(
            evidence
                .source_inputs
                .iter()
                .filter(|r| r.path.contains("/_templates/"))
                .count(),
            1
        );
        count += 1;
    }
    assert_eq!(count, 22);
}

#[cfg(unix)]
#[test]
fn symlink_parent_traversal_cannot_hash_different_bytes_from_the_compiler() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    f.write("outside/subdir/marker", "x");
    f.write("outside/value.rs", "pub const VALUE:u32=99;");
    f.write("module/src/value.rs", "pub const VALUE:u32=1;");
    symlink(f.0.join("outside/subdir"), f.0.join("module/src/link")).unwrap();
    f.write("module/src/lib.rs", "include!(\"link/../value.rs\");");
    f.declare(vec![], BTreeMap::new());
    assert!(f.error().contains("symlink"));
    fs::remove_file(f.0.join("module/src/link")).unwrap();
    symlink(f.0.join("module"), f.0.join("module-link")).unwrap();
    assert!(
        compute_source_inputs(&f.0.join("module-link/Cargo.toml"), DEFAULT_WASM_TARGET)
            .unwrap_err()
            .to_string()
            .contains("symlink")
    );
}

#[test]
fn hash_from_other_cwd_child() {
    let Ok(manifest) = std::env::var("OASIS7_SOURCE_INPUT_TEST_MANIFEST") else {
        return;
    };
    let expected = std::env::var("OASIS7_SOURCE_INPUT_TEST_HASH").unwrap();
    assert_eq!(
        compute_source_hash(Path::new(&manifest), DEFAULT_WASM_TARGET).unwrap(),
        expected
    );
}

#[test]
fn input_order_duplicates_and_actual_cwd_are_stable() {
    let f = Fixture::new();
    f.write(
        "templates/value.rs",
        "pub const TEXT:&str=include_str!(\"data.txt\");",
    );
    f.write("templates/data.txt", "data");
    let first = Edge {
        from: "module/src/lib.rs".into(),
        to: "templates/value.rs".into(),
    };
    let second = Edge {
        from: "templates/value.rs".into(),
        to: "templates/data.txt".into(),
    };
    f.declare(vec![first.clone(), second.clone()], BTreeMap::new());
    let before = f.inputs().unwrap();
    f.declare(vec![second, first.clone(), first], BTreeMap::new());
    let after = f.inputs().unwrap();
    let declaration_digest = |inputs: &SourceInputs| {
        inputs
            .source_inputs
            .iter()
            .find(|record| record.kind == "input-declaration")
            .unwrap()
            .sha256
            .clone()
    };
    assert_eq!(declaration_digest(&before), declaration_digest(&after));
    // The containing Cargo.toml is also an entity input: editing its bytes is a real source change.
    assert_ne!(before.source_hash, after.source_hash);
    let before = after;
    let cwd = Fixture::new();
    let child = Command::new(std::env::current_exe().unwrap())
        .current_dir(&cwd.0)
        .args(["source_inputs::tests::hash_from_other_cwd_child", "--exact"])
        .env(
            "OASIS7_SOURCE_INPUT_TEST_MANIFEST",
            f.0.join("module/Cargo.toml"),
        )
        .env("OASIS7_SOURCE_INPUT_TEST_HASH", before.source_hash)
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "{}",
        String::from_utf8_lossy(&child.stdout)
    );
}

#[test]
fn custom_crate_root_and_non_build_test_targets_keep_their_boundaries() {
    let f = Fixture::new();
    f.write("module/Cargo.toml","[package]\nname=\"fixture\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[lib]\npath=\"src/custom.rs\"\n");
    f.write(
        "module/src/custom.rs",
        "mod other; pub fn value(){other::value();}",
    );
    f.write("module/src/other.rs", "pub fn value(){}");
    f.write("module/tests/unused.rs", "#[test] fn example(){}");
    f.declare(vec![], BTreeMap::new());
    let evidence = f.inputs().unwrap();
    assert!(
        evidence
            .source_inputs
            .iter()
            .any(|r| r.path == "module/src/other.rs")
    );
    assert!(
        !evidence
            .source_inputs
            .iter()
            .any(|r| r.path == "module/tests/unused.rs")
    );
}

#[test]
fn shared_rust_helper_is_checked_again_in_its_build_script_context() {
    let f = Fixture::new();
    f.write(
        "Cargo.toml",
        "[workspace]\nmembers=[\"module\",\"a_shared\"]\nresolver=\"2\"\n",
    );
    f.write(
        "a_shared/Cargo.toml",
        "[package]\nname=\"fixture_shared\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
    );
    f.write("a_shared/src/lib.rs", "include!(\"common.rs\");");
    f.write(
        "a_shared/src/common.rs",
        "fn main(){let _=std::fs::read(\"input.txt\").unwrap();}",
    );
    f.write("module/Cargo.toml","[package]\nname=\"fixture\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[dependencies]\nfixture_shared={path=\"../a_shared\"}\n");
    f.write("module/src/lib.rs", "pub fn value(){}");
    f.write(
        "module/build.rs",
        "include!(\"../a_shared/src/common.rs\");",
    );
    f.write("module/input.txt", "real build input");
    f.declare(vec![], BTreeMap::from([("module/build.rs".into(), vec![])]));
    let build = Command::new("cargo")
        .env_remove("RUSTC_WRAPPER")
        .args(["build", "--offline", "--manifest-path"])
        .arg(f.0.join("module/Cargo.toml"))
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "actual Cargo fixture must be supported: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(
        f.inputs().is_err(),
        "normal-library visit must not hide undeclared build input"
    );
    f.declare(
        vec![],
        BTreeMap::from([("module/build.rs".into(), vec!["module/input.txt".into()])]),
    );
    let before = f.inputs().unwrap();
    assert!(
        before
            .source_inputs
            .iter()
            .any(|r| r.path == "module/input.txt")
    );
    f.write("module/input.txt", "changed real build input");
    assert_ne!(before.source_hash, f.inputs().unwrap().source_hash);
}

#[test]
fn included_build_helper_reads_from_package_working_directory() {
    let f = Fixture::new();
    f.write("module/src/lib.rs", "pub fn value(){}");
    f.write("module/build.rs", "include!(\"src/helper.rs\");");
    f.write(
        "module/src/helper.rs",
        "fn main(){let data=std::fs::read(\"input.txt\").unwrap(); assert_eq!(data,b\"real\");}",
    );
    f.write("module/input.txt", "real");
    f.write("module/src/input.txt", "decoy");
    let build = Command::new("cargo")
        .env_remove("RUSTC_WRAPPER")
        .args(["build", "--offline", "--manifest-path"])
        .arg(f.0.join("module/Cargo.toml"))
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "Cargo must read package/input.txt: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    f.declare(
        vec![],
        BTreeMap::from([
            ("module/build.rs".into(), vec![]),
            (
                "module/src/helper.rs".into(),
                vec!["module/src/input.txt".into()],
            ),
        ]),
    );
    assert!(
        f.inputs().is_err(),
        "helper-relative decoy must not certify actual package-root read"
    );
    f.declare(
        vec![],
        BTreeMap::from([("module/build.rs".into(), vec!["module/input.txt".into()])]),
    );
    let before = f.inputs().unwrap();
    f.write("module/input.txt", "changed");
    assert_ne!(before.source_hash, f.inputs().unwrap().source_hash);
}

#[test]
fn dependency_named_fs_cannot_impersonate_standard_build_input_reads() {
    let f = Fixture::new();
    f.write(
        "Cargo.toml",
        "[workspace]\nmembers=[\"module\",\"helper\"]\nresolver=\"2\"\n",
    );
    f.write(
        "helper/Cargo.toml",
        "[package]\nname=\"input_helper\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
    );
    f.write(
        "helper/src/lib.rs",
        "pub mod fs {pub fn read(_: &str)->Vec<u8>{std::fs::read(\"hidden.txt\").unwrap()}}",
    );
    f.write("module/Cargo.toml","[package]\nname=\"fixture\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[build-dependencies]\ninput_helper={path=\"../helper\"}\n");
    f.write("module/src/lib.rs", "pub fn value(){}");
    f.write(
        "module/build.rs",
        "use input_helper::fs; fn main(){let _=fs::read(\"declared.txt\");}",
    );
    f.write("module/declared.txt", "decoy");
    f.write("module/hidden.txt", "actual");
    f.declare(
        vec![],
        BTreeMap::from([("module/build.rs".into(), vec!["module/declared.txt".into()])]),
    );
    let build = Command::new("cargo")
        .env_remove("RUSTC_WRAPPER")
        .args(["build", "--offline", "--manifest-path"])
        .arg(f.0.join("module/Cargo.toml"))
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(
        f.inputs().is_err(),
        "external fs alias must not certify decoy instead of actual build input"
    );
}

#[test]
fn cargo_build_target_metadata_supplies_the_package_cwd_for_nested_scripts() {
    let f = Fixture::new();
    f.write("module/Cargo.toml","[package]\nname=\"fixture\"\nversion=\"0.1.0\"\nedition=\"2024\"\nbuild=\"src/setup.rs\"\n");
    f.write("module/src/lib.rs", "pub fn value(){}");
    f.write(
        "module/src/setup.rs",
        "fn main(){let data=std::fs::read(\"input.txt\").unwrap(); assert_eq!(data,b\"real\");}",
    );
    f.write("module/input.txt", "real");
    f.write("module/src/input.txt", "decoy");
    f.declare(
        vec![],
        BTreeMap::from([(
            "module/src/setup.rs".into(),
            vec!["module/input.txt".into()],
        )]),
    );
    let build = Command::new("cargo")
        .env_remove("RUSTC_WRAPPER")
        .args(["build", "--offline", "--manifest-path"])
        .arg(f.0.join("module/Cargo.toml"))
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let before = f.inputs().unwrap();
    f.write("module/input.txt", "changed");
    assert_ne!(before.source_hash, f.inputs().unwrap().source_hash);
}

#[test]
fn one_helper_is_checked_for_each_distinct_cargo_build_package() {
    let f = Fixture::new();
    f.write(
        "Cargo.toml",
        "[workspace]\nmembers=[\"module\",\"a_shared\",\"z_other\"]\nresolver=\"2\"\n",
    );
    f.write(
        "a_shared/Cargo.toml",
        "[package]\nname=\"fixture_shared\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
    );
    f.write("a_shared/src/lib.rs", "include!(\"common.rs\");");
    f.write(
        "a_shared/src/common.rs",
        "fn main(){let _=std::fs::read(\"input.txt\").unwrap();}",
    );
    f.write(
        "z_other/Cargo.toml",
        "[package]\nname=\"fixture_other\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
    );
    f.write("z_other/src/lib.rs", "pub fn value(){}");
    f.write(
        "z_other/build.rs",
        "include!(\"../a_shared/src/common.rs\");",
    );
    f.write("z_other/input.txt", "other package");
    f.write("module/Cargo.toml","[package]\nname=\"fixture\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[dependencies]\nfixture_shared={path=\"../a_shared\"}\nfixture_other={path=\"../z_other\"}\n");
    f.write("module/src/lib.rs", "pub fn value(){}");
    f.write(
        "module/build.rs",
        "include!(\"../a_shared/src/common.rs\");",
    );
    f.write("module/input.txt", "module package");
    f.declare(
        vec![],
        BTreeMap::from([
            ("module/build.rs".into(), vec!["module/input.txt".into()]),
            ("z_other/build.rs".into(), vec!["z_other/input.txt".into()]),
        ]),
    );
    let before = f.inputs().unwrap();
    assert!(
        before
            .source_inputs
            .iter()
            .any(|record| record.path == "z_other/input.txt")
    );
    f.write("z_other/input.txt", "changed other input");
    assert_ne!(before.source_hash, f.inputs().unwrap().source_hash);
}

#[test]
fn local_procedural_attribute_cannot_hide_compile_time_file_inputs() {
    let f = Fixture::new();
    f.write(
        "Cargo.toml",
        "[workspace]\nmembers=[\"module\",\"input_macro\"]\nresolver=\"2\"\n",
    );
    f.write("input_macro/Cargo.toml","[package]\nname=\"input_macro\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[lib]\nproc-macro=true\n");
    f.write("input_macro/src/lib.rs","extern crate proc_macro; use proc_macro::TokenStream; #[proc_macro_attribute] pub fn external_input(_:TokenStream,_:TokenStream)->TokenStream {let value=std::fs::read_to_string(\"hidden.txt\").unwrap(); format!(\"pub const VALUE:u32={};\",value).parse().unwrap()}");
    f.write("module/Cargo.toml","[package]\nname=\"fixture\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[dependencies]\ninput_macro={path=\"../input_macro\"}\n");
    f.write(
        "module/src/lib.rs",
        "use input_macro::external_input; #[external_input] pub const VALUE:u32=0;",
    );
    for path in ["hidden.txt", "module/hidden.txt", "input_macro/hidden.txt"] {
        f.write(path, "1");
    }
    f.declare(vec![], BTreeMap::new());
    let build = Command::new("cargo")
        .current_dir(&f.0)
        .env_remove("RUSTC_WRAPPER")
        .args(["build", "--offline", "--manifest-path"])
        .arg(f.0.join("module/Cargo.toml"))
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "actual local proc-macro fixture must compile: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(
        f.inputs().is_err(),
        "local proc-macro hidden input must not receive a complete v2 closure"
    );
}

#[test]
fn cfg_attr_path_cannot_select_an_unaccounted_wasm_module() {
    let f = Fixture::new();
    f.write("module/src/lib.rs","#[cfg_attr(target_arch=\"wasm32\",path=\"../../templates/external.rs\")] mod payload; pub fn value()->u32{payload::VALUE}");
    f.write("module/src/payload.rs", "pub const VALUE:u32=0;");
    f.write("templates/external.rs", "pub const VALUE:u32=1;");
    f.declare(vec![], BTreeMap::new());
    let build = Command::new("cargo")
        .env_remove("RUSTC_WRAPPER")
        .args([
            "build",
            "--offline",
            "--target",
            DEFAULT_WASM_TARGET,
            "--manifest-path",
        ])
        .arg(f.0.join("module/Cargo.toml"))
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "actual wasm cfg_attr fixture must compile: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(
        f.inputs().is_err(),
        "cfg_attr path must not silently hash the inactive default module"
    );
}

#[test]
fn build_stdout_cannot_introduce_undeclared_linker_or_generated_inputs() {
    let f = Fixture::new();
    f.write("module/src/lib.rs", "pub fn value(){}");
    f.declare(vec![], BTreeMap::from([("module/build.rs".into(), vec![])]));
    for body in [
        r#"println!("cargo:rustc-link-search=../../external"); println!("cargo:rustc-link-lib=static=payload");"#,
        r#"println!("cargo::rustc-link-arg=@../../response");"#,
        r#"println!("cargo:rustc-flags=-L../../external");"#,
        r#"println!("{}", "cargo:rustc-link-search=../../external");"#,
    ] {
        f.write("module/build.rs", &format!("fn main(){{{body}}}"));
        assert!(
            f.inputs().is_err(),
            "unsupported Cargo stdout must not certify closure: {body}"
        );
    }
    f.write(
        "module/build.rs",
        r#"fn main(){println!("ordinary static diagnostic"); eprintln!("stderr diagnostic");}"#,
    );
    f.inputs().unwrap();
}

#[test]
fn unknown_attributes_derives_and_macro_definitions_fail_closed() {
    let f = Fixture::new();
    f.declare(vec![], BTreeMap::new());
    for source in [
        "#[external_input] pub fn value(){}",
        "#[derive(UnknownInput)] pub struct Value;",
        "#[cfg_attr(target_arch=\"wasm32\", external_input)] pub fn value(){}",
        "macro_rules! export_wasm_module {()=>{unknown_generator!();}}",
        "macro_rules! export_wasm_module {()=>{#[path=\"external.rs\"] mod payload;}}",
    ] {
        f.write("module/src/lib.rs", source);
        assert!(
            f.inputs().is_err(),
            "unsupported compile-time shape: {source}"
        );
    }
}

#[test]
fn an_unrelated_workspace_proc_macro_does_not_expand_this_closure() {
    let f = Fixture::new();
    f.write(
        "Cargo.toml",
        "[workspace]\nmembers=[\"module\",\"unused_macro\"]\nresolver=\"2\"\n",
    );
    f.write("unused_macro/Cargo.toml","[package]\nname=\"unused_macro\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[lib]\nproc-macro=true\n");
    f.write("unused_macro/src/lib.rs","extern crate proc_macro; #[proc_macro] pub fn unused(_:proc_macro::TokenStream)->proc_macro::TokenStream {\"\".parse().unwrap()}");
    let before = f.inputs().unwrap();
    f.write(
        "unused_macro/src/lib.rs",
        "compile_error!(\"not a dependency of this target\");",
    );
    assert_eq!(before, f.inputs().unwrap());
}

#[cfg(unix)]
#[test]
fn an_empty_whitelist_root_symlink_is_rejected_before_directory_enumeration() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    fs::create_dir_all(f.0.join("outside-empty")).unwrap();
    symlink(f.0.join("outside-empty"), f.0.join("module/assets")).unwrap();
    assert!(
        f.inputs().is_err(),
        "empty assets symlink must not be followed and silently omitted"
    );
}

#[test]
fn macro_generated_modules_and_unknown_macro_owners_are_not_implicit_inputs() {
    let f = Fixture::new();
    f.write("module/src/lib.rs","macro_rules! export_wasm_module {()=>{mod payload;}} export_wasm_module!(); pub fn value()->u32{payload::VALUE}");
    f.write(
        "module/src/payload.rs",
        "include!(\"../../templates/external.rs\");",
    );
    f.write("templates/external.rs", "pub const VALUE:u32=1;");
    f.declare(vec![], BTreeMap::new());
    let build = Command::new("cargo")
        .env_remove("RUSTC_WRAPPER")
        .args(["build", "--offline", "--manifest-path"])
        .arg(f.0.join("module/Cargo.toml"))
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "actual macro-generated module fixture must compile: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    assert!(
        f.inputs().is_err(),
        "module generation must not bypass the static module graph"
    );
    for source in [
        "use external::format; pub fn value(){let _=format!(\"value\");}",
        "pub fn value(){let _=external::format!(\"value\");}",
        "use external::unknown as r#format; pub fn value(){let _=format!(\"value\");}",
    ] {
        f.write("module/src/lib.rs", source);
        assert!(f.inputs().is_err());
    }
}

#[test]
fn include_mod_context_is_rejected_without_guessing_a_sibling_directory() {
    let f = Fixture::new();
    f.write("module/src/lib.rs","include!(\"header.rs\"); #[cfg(test)] mod tests {#[test] fn actual(){assert_eq!(super::value(),11);}}");
    f.write(
        "module/src/header.rs",
        "mod sub; pub fn value()->u32{sub::VALUE}",
    );
    f.write(
        "module/src/sub.rs",
        "include!(\"../../templates/external.rs\");",
    );
    f.write("module/src/header/sub.rs", "pub const VALUE:u32=22;");
    f.write("templates/external.rs", "pub const VALUE:u32=11;");
    f.declare(vec![], BTreeMap::new());
    let actual = Command::new("cargo")
        .env_remove("RUSTC_WRAPPER")
        .args(["test", "--offline", "--lib", "--manifest-path"])
        .arg(f.0.join("module/Cargo.toml"))
        .output()
        .unwrap();
    assert!(
        actual.status.success(),
        "actual include module resolution must compile and return 11: {}",
        String::from_utf8_lossy(&actual.stderr)
    );
    assert!(
        f.inputs().is_err(),
        "included non-inline mod must not hash the inactive header/sub.rs instead of the active external input"
    );
    let ordinary = Fixture::new();
    ordinary.write("module/src/lib.rs","mod header; #[cfg(test)] mod tests{#[test] fn actual(){assert_eq!(super::header::value(),22);}}");
    ordinary.write(
        "module/src/header.rs",
        "mod sub; pub fn value()->u32{sub::VALUE}",
    );
    ordinary.write("module/src/header/sub.rs", "pub const VALUE:u32=22;");
    ordinary.declare(vec![], BTreeMap::new());
    ordinary.inputs().unwrap();
    let actual = Command::new("cargo")
        .env_remove("RUSTC_WRAPPER")
        .args(["test", "--offline", "--lib", "--manifest-path"])
        .arg(ordinary.0.join("module/Cargo.toml"))
        .output()
        .unwrap();
    assert!(actual.status.success());
}

#[test]
fn include_mod_context_is_not_skipped_after_an_ordinary_module_visit() {
    let f = Fixture::new();
    f.write("module/Cargo.toml","[package]\nname=\"fixture\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[[bin]]\nname=\"fixture_bin\"\npath=\"src/z_bin.rs\"\n");
    f.write(
        "module/src/lib.rs",
        "mod header; pub fn ordinary()->u32{header::value()}",
    );
    f.write(
        "module/src/z_bin.rs",
        "include!(\"header.rs\"); fn main(){assert_eq!(value(),11);}",
    );
    f.write(
        "module/src/header.rs",
        "mod sub; pub fn value()->u32{sub::VALUE}",
    );
    f.write(
        "module/src/sub.rs",
        "include!(\"../../templates/external.rs\");",
    );
    f.write("module/src/header/sub.rs", "pub const VALUE:u32=22;");
    f.write("templates/external.rs", "pub const VALUE:u32=11;");
    f.declare(vec![], BTreeMap::new());
    let actual = Command::new("cargo")
        .env_remove("RUSTC_WRAPPER")
        .args([
            "run",
            "--offline",
            "--bin",
            "fixture_bin",
            "--manifest-path",
        ])
        .arg(f.0.join("module/Cargo.toml"))
        .output()
        .unwrap();
    assert!(
        actual.status.success(),
        "real lib+bin shared module/include fixture: {}",
        String::from_utf8_lossy(&actual.stderr)
    );
    assert!(
        f.inputs().is_err(),
        "ordinary module cache must not waive a later include! context check"
    );
}

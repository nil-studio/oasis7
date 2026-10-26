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
        self.write(
            "module/source-inputs.json",
            &serde_json::to_string(&Declaration {
                rule_version: 2,
                includes,
                build_inputs,
            })
            .unwrap(),
        );
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
    let declaration = fs::read_to_string(f.0.join("module/source-inputs.json")).unwrap();
    f.write(
        "module/source-inputs.json",
        &serde_json::to_string_pretty(
            &serde_json::from_str::<serde_json::Value>(&declaration).unwrap(),
        )
        .unwrap(),
    );
    assert_eq!(original, f.inputs().unwrap());
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
        let declaration = dir.join(DECLARATION);
        if !declaration.is_file() {
            continue;
        }
        let rel = dir.strip_prefix(repository).unwrap();
        let name = dir.file_name().unwrap().to_str().unwrap();
        f.write(
            &format!("{}/Cargo.toml", rel.display()),
            &format!("[package]\nname=\"{name}\"\nversion=\"0.1.0\"\nedition=\"2024\"\n"),
        );
        f.write(
            &format!("{}/src/lib.rs", rel.display()),
            &fs::read_to_string(dir.join("src/lib.rs")).unwrap(),
        );
        f.write(
            &format!("{}/{DECLARATION}", rel.display()),
            &fs::read_to_string(declaration).unwrap(),
        );
        let d: Declaration =
            serde_json::from_str(&fs::read_to_string(dir.join(DECLARATION)).unwrap()).unwrap();
        assert_eq!(d.includes.len(), 1);
        wrappers.push((rel.to_path_buf(), d.includes[0].to.clone()));
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
        if !dir.join(DECLARATION).is_file() {
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
    assert_eq!(before, f.inputs().unwrap());
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

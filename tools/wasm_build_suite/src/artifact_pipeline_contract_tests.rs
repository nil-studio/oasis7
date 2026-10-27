//! Exercise the scripts that select and hydrate the builder's Runtime artifacts.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const SCOPE: &str = include_str!("../../../scripts/plan-wasm-determinism-scope.sh");
const SCOPE_TEST: &str = include_str!("../../../scripts/plan-wasm-determinism-scope.test.sh");
const SYNC_M1: &str = include_str!("../../../scripts/sync-m1-builtin-wasm-artifacts.sh");
const SYNC_TEST: &str = include_str!("../../../scripts/sync-m1-builtin-wasm-artifacts.test.sh");
const SYNC_M4: &str = include_str!("../../../scripts/sync-m4-builtin-wasm-artifacts.sh");
const SYNC_M5: &str = include_str!("../../../scripts/sync-m5-builtin-wasm-artifacts.sh");
const PACKAGE_SUMMARY: &str = include_str!("../../../scripts/package-wasm-summary-bundle.sh");
const STAGE_SUMMARY: &str = include_str!("../../../scripts/stage-wasm-summary-imports.sh");
const VERIFY_SUMMARY: &str = include_str!("../../../scripts/ci-verify-m1-wasm-summaries.py");

#[test]
fn summary_verifier_requires_current_input_evidence_for_every_module_set() {
    assert_compiled_input("scripts/ci-verify-m1-wasm-summaries.py", VERIFY_SUMMARY);
    assert_compiled_input("scripts/package-wasm-summary-bundle.sh", PACKAGE_SUMMARY);
    assert_compiled_input("scripts/stage-wasm-summary-imports.sh", STAGE_SUMMARY);
    let scratch = Scratch::new();
    assert_success(Command::new("python3").args(["-c", r#"
import copy, json, pathlib, subprocess, sys
repo, root = map(pathlib.Path, sys.argv[1:])
script = repo / 'scripts/ci-verify-m1-wasm-summaries.py'
def summary(module, runner):
    return dict(schema_version=1, purpose='candidate-build', baseline_sha256='0'*64, module_set=module, runner=runner,
        current_platform='linux-x86_64', host_platform=runner, canonical_platform='linux-x86_64', module_count=1,
        module_hashes={module+'.module':'wasm'}, manifest_platform_hashes={module+'.module':'wasm'},
        identity_hashes={module+'.module':'identity'},
        identity_build_recipe=dict(builder_image_digest='builder', container_platform='linux-x86_64', canonicalizer_version='v1'),
        receipt_evidence={module+'.module':dict(source_input_rule_version=2,
            source_inputs=[dict(kind='source-file',path='module/src/lib.rs',sha256='0'*64)],
            source_hash='source',build_manifest_hash='build',wasm_hash='wasm',builder_image_digest='builder',container_platform='linux-x86_64',canonicalizer_version='v1')})
def run(module, directory):
    return subprocess.run([sys.executable,str(script),'--module-set',module,'--summary-dir',str(directory)],capture_output=True,text=True)
for module in ('m1','m4','m5'):
    directory=root/module; directory.mkdir()
    for runner in ('linux-x86_64','darwin-arm64'):
        (directory/(runner+'.json')).write_text(json.dumps(summary(module,runner)))
    result=run(module,directory); assert result.returncode==0,result.stderr
original=summary('m1','linux-x86_64')
for purpose,digest in [('unknown','0'*64),('candidate-build',''),('release-build','')]:
    changed=summary('m1','linux-x86_64'); changed['purpose']=purpose; changed['baseline_sha256']=digest
    (root/'m1/linux-x86_64.json').write_text(json.dumps(changed))
    assert run('m1',root/'m1').returncode!=0,'unknown/mixed purpose'

for mutation in ('legacy','unknown','missing-details','invalid-path','invalid-digest'):
    candidate=copy.deepcopy(original); evidence=candidate['receipt_evidence']['m1.module']
    if mutation=='legacy': del evidence['source_input_rule_version']
    elif mutation=='unknown': evidence['source_input_rule_version']=99
    elif mutation=='missing-details': evidence['source_inputs']=[]
    elif mutation=='invalid-path': evidence['source_inputs'][0]['path']='../escape'
    else: evidence['source_inputs'][0]['sha256']='not-a-digest'
    (root/'m1/linux-x86_64.json').write_text(json.dumps(candidate))
    result=run('m1',root/'m1'); assert result.returncode!=0,mutation
for module in ('m1','m4','m5'):
    (root/module/'linux-x86_64.json').write_text(json.dumps(summary(module,'linux-x86_64')))
bundle=root/'bundle'
result=subprocess.run(['bash',str(repo/'scripts/package-wasm-summary-bundle.sh'),'--out-dir',str(bundle),'--runner-label','darwin-arm64','--source-summary-root',str(root)],capture_output=True,text=True)
assert result.returncode==0,result.stderr
for mixed in (False,True):
    local=root/'local'; local.mkdir(exist_ok=True)
    payload=summary('m1','linux-x86_64')
    if mixed: payload['purpose']='release-build'; payload['baseline_sha256']=''
    (local/'linux-x86_64.json').write_text(json.dumps(payload))
    result=subprocess.run(['bash',str(repo/'scripts/stage-wasm-summary-imports.sh'),'--module-set','m1','--local-summary-dir',str(local),'--out-dir',str(root/('mixed' if mixed else 'staged')),'--external-summary-bundle',str(bundle)],capture_output=True,text=True)
    assert (result.returncode!=0)==mixed,result.stderr

"#]).arg(repo_root()).arg(&scratch.0).output().expect("run actual summary verifier positive/negative vectors"));
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assert_compiled_input(relative: &str, compiled: &str) {
    assert_eq!(
        fs::read_to_string(repo_root().join(relative)).expect("read script input"),
        compiled,
        "executed script must equal the compiled test input: {relative}"
    );
}

fn assert_success(output: Output) -> String {
    assert!(
        output.status.success(),
        "script failed: {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("script stdout UTF-8")
}

#[test]
fn artifact_scope_executes_fail_closed_and_owner_path_contracts() {
    assert_compiled_input("scripts/plan-wasm-determinism-scope.sh", SCOPE);
    assert_compiled_input("scripts/plan-wasm-determinism-scope.test.sh", SCOPE_TEST);
    let stdout = assert_success(
        Command::new("bash")
            .arg(repo_root().join("scripts/plan-wasm-determinism-scope.test.sh"))
            .current_dir(repo_root())
            .output()
            .expect("execute actual scope regression suite"),
    );
    assert!(stdout.contains("plan-wasm-determinism-scope.test: OK"));
}

#[test]
fn artifact_sync_executes_cli_no_write_and_distfs_layout_contracts() {
    assert_compiled_input("scripts/sync-m1-builtin-wasm-artifacts.sh", SYNC_M1);
    assert_compiled_input("scripts/sync-m1-builtin-wasm-artifacts.test.sh", SYNC_TEST);
    assert_success(
        Command::new("bash")
            .arg(repo_root().join("scripts/sync-m1-builtin-wasm-artifacts.test.sh"))
            .current_dir(repo_root())
            .output()
            .expect("execute actual sync CLI fixture suite"),
    );
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "oasis7-artifact-wrapper-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("unix epoch")
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("scripts")).expect("create wrapper fixture");
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn artifact_sync_wrappers_forward_exact_runtime_owner_paths_and_caller_arguments() {
    for (module, source) in [("m4", SYNC_M4), ("m5", SYNC_M5)] {
        let relative = format!("scripts/sync-{module}-builtin-wasm-artifacts.sh");
        assert_compiled_input(&relative, source);
        let scratch = Scratch::new();
        let wrapper = scratch.0.join(&relative);
        fs::write(&wrapper, source).expect("write actual wrapper");
        let delegate = scratch.0.join("scripts/sync-m1-builtin-wasm-artifacts.sh");
        fs::write(
            &delegate,
            "#!/usr/bin/env bash\nset -euo pipefail\nprintf '%s\\n' \"$@\"\n",
        )
        .expect("write argument capture delegate");
        fs::set_permissions(&delegate, fs::Permissions::from_mode(0o755))
            .expect("make capture delegate executable");
        let output_dir = scratch.0.join("output with spaces");
        let stdout = assert_success(
            Command::new("bash")
                .arg(&wrapper)
                .arg("--check")
                .arg("--out-dir")
                .arg(&output_dir)
                .output()
                .expect("execute actual wrapper"),
        );
        let artifacts = scratch.0.join("crates/oasis7_runtime/src/world/artifacts");
        let expected = vec![
            "--module-ids-path".to_string(),
            artifacts
                .join(format!("{module}_builtin_module_ids.txt"))
                .display()
                .to_string(),
            "--hash-path".to_string(),
            artifacts
                .join(format!("{module}_builtin_modules.sha256"))
                .display()
                .to_string(),
            "--check".to_string(),
            "--out-dir".to_string(),
            output_dir.display().to_string(),
        ];
        assert_eq!(stdout.lines().collect::<Vec<_>>(), expected);
        let help = assert_success(
            Command::new("bash")
                .arg(&wrapper)
                .arg("--help")
                .output()
                .expect("wrapper help"),
        );
        assert!(help.contains(&format!(
            "crates/oasis7_runtime/src/world/artifacts/{module}_builtin_module_ids.txt"
        )));
        assert!(help.contains(&format!(
            "crates/oasis7_runtime/src/world/artifacts/{module}_builtin_modules.sha256"
        )));
        assert!(
            !output_dir.exists(),
            "wrapper contract must not perform build writes"
        );
    }
}

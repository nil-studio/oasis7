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

const IMPORT_SUMMARIES: &str = include_str!("../../../scripts/stage-wasm-summary-imports.sh");
const ATTESTATION_FLOW: &str =
    include_str!("../../../scripts/module-release-node-attestation-flow.sh");
const EVIDENCE_REPORT: &str = include_str!("../../../scripts/wasm-release-evidence-report.sh");
const BUNDLE_SMOKE: &str = include_str!("../../../scripts/wasm-summary-bundle-smoke.sh");

#[test]
fn registered_evidence_report_executes_full_pending_and_mismatch_contracts() {
    assert_compiled_input("scripts/wasm-release-evidence-report.sh", EVIDENCE_REPORT);
    let scratch = Scratch::new();
    assert_success(Command::new("python3").args(["-c", r#"
import json, pathlib, subprocess, sys
repo, root = map(pathlib.Path, sys.argv[1:])
def summary(runner):
    return dict(schema_version=1,module_set='m1',runner=runner,current_platform='linux-x86_64',host_platform=runner,canonical_platform='linux-x86_64',module_count=1,
      module_hashes={'m1.module':'wasm'},manifest_platform_hashes={'m1.module':'wasm'},identity_hashes={'m1.module':'identity'},
      identity_build_recipe=dict(builder_image_digest='builder',container_platform='linux-x86_64',canonicalizer_version='v1'),
      receipt_evidence={'m1.module':dict(source_hash='source',build_manifest_hash='build',wasm_hash='wasm',builder_image_digest='builder',container_platform='linux-x86_64',canonicalizer_version='v1')})
for case in ('closed','pending','mismatch'):
    imports=root/case/'imports'; imports.mkdir(parents=True)
    for runner in (('linux-x86_64',) if case=='pending' else ('linux-x86_64','darwin-arm64')):
        value=summary(runner)
        if case=='mismatch' and runner=='darwin-arm64': value['module_hashes']['m1.module']='different-wasm'
        (imports/(runner+'.json')).write_text(json.dumps(value))
    out=root/case/'report'
    result=subprocess.run(['bash',str(repo/'scripts/wasm-release-evidence-report.sh'),'--out-dir',str(out),'--module-sets','m1','--runner-label','linux-x86_64','--skip-collect','--summary-import-dir',str(imports),'--required-runners','linux-x86_64','--expected-runners','linux-x86_64,darwin-arm64'],capture_output=True,text=True)
    files=list(out.glob('*/summary.json'));assert len(files)==1,result.stderr
    report=json.loads(files[0].read_text())
    expected={'closed':'cross-host-closed','pending':'conditional-go','mismatch':'no-go'}[case]
    assert report['gate_result']==expected,(report,result.stderr)
    assert report['cross_host_closed']==(case=='closed'),(case,report,result.stdout,result.stderr)
    assert report['cross_host_evidence_pending']==(case=='pending'),(case,report,result.stdout,result.stderr)
    assert result.returncode==0,(case,result.stderr) # report generation succeeds; the recorded gate is authoritative
    assert report['module_sets'][0]['verify_status']==('failed' if case=='mismatch' else 'passed')
"#]).arg(repo_root()).arg(&scratch.0).output().expect("execute actual offline evidence report matrix"));
}

#[test]
fn registered_manual_bundle_smoke_executes_its_real_local_fixtures() {
    assert_compiled_input("scripts/wasm-summary-bundle-smoke.sh", BUNDLE_SMOKE);
    let scratch = Scratch::new();
    let tools = scratch.0.join("no-ripgrep");
    fs::create_dir(&tools).expect("create sentinel tools directory");
    let sentinel = tools.join("rg");
    fs::write(
        &sentinel,
        "#!/bin/sh\necho ripgrep-sentinel-invoked >&2\nexit 127\n",
    )
    .expect("write failing rg sentinel");
    fs::set_permissions(&sentinel, fs::Permissions::from_mode(0o755))
        .expect("make sentinel executable");
    let path = std::env::join_paths(std::iter::once(tools).chain(std::env::split_paths(
        &std::env::var_os("PATH").expect("test PATH"),
    )))
    .expect("sentinel PATH");
    let stdout = assert_success(
        Command::new("bash")
            .arg(repo_root().join("scripts/wasm-summary-bundle-smoke.sh"))
            .env("PATH", path)
            .current_dir(repo_root())
            .output()
            .expect("execute actual local bundle smoke"),
    );
    assert!(stdout.contains("wasm summary bundle smoke: OK"));
}

#[test]
fn registered_wasm_consumers_execute_import_and_offline_request_contracts() {
    assert_compiled_input("scripts/stage-wasm-summary-imports.sh", IMPORT_SUMMARIES);
    assert_compiled_input(
        "scripts/module-release-node-attestation-flow.sh",
        ATTESTATION_FLOW,
    );
    let scratch = Scratch::new();
    assert_success(Command::new("python3").args(["-c", r#"
import json, pathlib, subprocess, sys
repo, root = map(pathlib.Path, sys.argv[1:])
bundle=root/'bundle'; bundle.mkdir()
summary=dict(schema_version=1,module_set='m1',runner='darwin-arm64',host_platform='darwin-arm64',canonical_platform='linux-x86_64',identity_build_recipe=dict(container_platform='linux-x86_64'))
(bundle/'summary.json').write_text(json.dumps(summary))
(bundle/'bundle_manifest.json').write_text(json.dumps(dict(schema_version=1,runner_label='darwin-arm64',host_platform='darwin-arm64',canonical_platforms=['linux-x86_64'],summary_files=dict(m1='summary.json'))))
local=root/'local';local.mkdir();(local/'linux-x86_64.json').write_text('{}')
def stage(out):
    return subprocess.run(['bash',str(repo/'scripts/stage-wasm-summary-imports.sh'),'--module-set','m1','--local-summary-dir',str(local),'--out-dir',str(out),'--external-summary-bundle',str(bundle)],capture_output=True,text=True)
result=stage(root/'valid');assert result.returncode==0,result.stderr
assert json.loads((root/'valid/darwin-arm64.json').read_text())==summary
summary['host_platform']='linux-x86_64';(bundle/'summary.json').write_text(json.dumps(summary))
result=stage(root/'invalid');assert result.returncode!=0 and 'host_platform mismatch' in result.stderr,result.stderr
flow=['bash',str(repo/'scripts/module-release-node-attestation-flow.sh'),'--out-dir',str(root/'flow'),'--module-sets','m1','--skip-local-collect','--summary-import-dir',str(root/'missing-imports')]
for key,value in [('request-id','1'),('operator-agent-id','1'),('signer-node-id','fixture'),('build-manifest-hash','build'),('source-hash','source'),('wasm-hash','wasm'),('builder-image-digest','builder'),('container-platform','linux-x86_64'),('canonicalizer-version','v1')]:flow += ['--'+key,value]
result=subprocess.run(flow,capture_output=True,text=True)
assert result.returncode!=0 and 'summary import dir not found' in result.stderr,result.stderr
assert not list((root/'flow').rglob('proof_payload.json')) and not list((root/'flow').rglob('submit_request.json')), 'invalid imports must not be packaged or submitted'
"#]).arg(repo_root()).arg(&scratch.0).output().expect("execute registered consumer contracts without signing or network"));
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

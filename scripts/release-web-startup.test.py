#!/usr/bin/env python3
"""Behavioral startup regressions; no Cargo or browser dependencies."""
import pathlib
import subprocess
import time
import tempfile
import json
import os
import shutil
import http.server
import threading
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]

class StartupTests(unittest.TestCase):
    def test_launcher_exit_interrupts_primary_port_wait(self):
        source = (ROOT / "scripts/viewer-primary-web-entry-regression.sh").read_text()
        functions = source[source.index("wait_for_port() {"):source.index("normalize_eval_token() {")]
        library = ROOT / "scripts/release-web-fixture-lib.sh"
        script = f"source {library};\n" if library.exists() else ""
        script += functions + "\n( exit 37 ) & launcher_pid=$!; launcher_log=/dev/null; sleep .1; wait_for_port 127.0.0.1 1 5"
        started = time.monotonic()
        result = subprocess.run(["bash", "-c", script], capture_output=True, text=True, timeout=8)
        self.assertLess(time.monotonic() - started, 3, "dead launcher consumed readiness deadline")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("status=37", result.stderr)

    def test_fixture_early_exit_is_reported(self):
        library = ROOT / "scripts/release-web-fixture-lib.sh"
        result = subprocess.run(["bash", "-c", f"source {library}; (exit 23) & release_web_fixture_pid=$!; sleep .1; release_web_check_startup"], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("fixture process exited: status=23", result.stderr)

    def test_configuration_rejects_unsafe_scope_and_permissions_without_key_leak(self):
        library = ROOT / "scripts/release-web-fixture-lib.sh"
        names = ('ENDPOINT', 'PUBLIC_KEY', 'WORLD_ID', 'GENESIS_DIGEST', 'SCOPE', 'READ_PRIVATE_KEY')
        data = {'OASIS7_WORLD_SERVICE_' + key: 'secret-test-value' for key in names}
        data['OASIS7_WORLD_SERVICE_ENDPOINT'] = 'http://127.0.0.1:1234'
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / 'config.json'
            for mode, scope in [(0o644, 'public'), (0o600, 'engineering')]:
                data['OASIS7_WORLD_SERVICE_SCOPE'] = scope
                path.write_text(json.dumps(data))
                path.chmod(mode)
                result = subprocess.run(['bash', '-c', f'source {library}; release_web_fixture_import "$1"', 'bash', str(path)], capture_output=True, text=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn('secret-test-value', result.stdout + result.stderr)
            data['OASIS7_WORLD_SERVICE_SCOPE'] = 'public'
            path.write_text(json.dumps(data))
            path.chmod(0o600)
            result = subprocess.run(['bash', '-c', f'source {library}; release_web_fixture_import "$1"; [[ "$OASIS7_WORLD_SERVICE_SCOPE" == public ]]', 'bash', str(path)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout, '')

    def test_owned_process_deadline_terminates_hung_child(self):
        with tempfile.TemporaryDirectory() as directory:
            pidfile = pathlib.Path(directory) / 'pid'
            stop = pathlib.Path(directory) / 'stop'
            code = "import os,signal,time,pathlib; pathlib.Path(%r).write_text(str(os.getpid())); signal.signal(signal.SIGTERM,signal.SIG_IGN); time.sleep(60)" % str(pidfile)
            started = time.monotonic()
            result = subprocess.run(['python3', str(ROOT / 'scripts/release-web-fixture-process.py'), str(stop), '1', 'python3', '-c', code], capture_output=True, text=True, timeout=12)
            self.assertLess(time.monotonic() - started, 10)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('process deadline exceeded', result.stderr)
            with self.assertRaises(ProcessLookupError):
                os.kill(int(pidfile.read_text()), 0)

    def test_deadline_is_failure_even_when_child_stops_cleanly(self):
        with tempfile.TemporaryDirectory() as directory:
            stop = pathlib.Path(directory) / 'stop'
            code = "import pathlib,time; p=pathlib.Path(%r); exec('while not p.exists(): time.sleep(.05)')" % str(stop)
            result = subprocess.run(['python3', str(ROOT / 'scripts/release-web-fixture-process.py'), str(stop), '1', 'python3', '-c', code], capture_output=True, text=True, timeout=5)
            self.assertEqual(result.returncode, 124)

    def test_fixture_rejects_external_url(self):
        result = subprocess.run(['bash', str(ROOT / 'scripts/viewer-software-safe-step-regression.sh'), '--release-web-fixture', '--url', 'http://127.0.0.1:1'], capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn('cannot be used with --url', result.stderr)

    def test_fixture_rejects_external_chain_before_runtime(self):
        result = subprocess.run(['bash', str(ROOT / 'scripts/viewer-software-safe-step-regression.sh'), '--release-web-fixture', '--chain-status-bind', '127.0.0.1:1'], capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn('incompatible with the isolated signed Web QA fixture', result.stderr)

    def test_primary_rejects_scenario_before_runtime(self):
        result = subprocess.run(['bash', str(ROOT / 'scripts/viewer-primary-web-entry-regression.sh'), '--scenario', 'llm_bootstrap'], capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn('unknown option: --scenario', result.stderr)

    def test_default_artifact_build_failure_prevents_process_start(self):
        self.check_primary_artifact_build(custom=False)

    def test_explicit_artifact_directory_is_preserved(self):
        self.check_primary_artifact_build(custom=True)

    def test_canonical_index_alias_preserves_built_visual_test_bundle(self):
        self.check_primary_artifact_build(custom=False, succeed=True)

    def check_primary_artifact_build(self, custom, succeed=False):
        with tempfile.TemporaryDirectory() as directory:
            repo = pathlib.Path(directory)
            scripts = repo / 'scripts'
            scripts.mkdir()
            for name in ('viewer-primary-web-entry-regression.sh', 'release-web-fixture-lib.sh', 'viewer-web-dist-contract.sh'):
                shutil.copyfile(ROOT / 'scripts' / name, scripts / name)
            (scripts / 'agent-browser-lib.sh').write_text('ab_require() { :; }\nab_cmd() { :; }\nresolve_viewer_static_dir_for_web_closure() { printf "%s\\n" "$2"; }\n')
            (scripts / 'cargo-dev-lib.sh').write_text('oasis7_cargo_dev() { echo cargo >> "$TEST_TRACE_PATH"; return 42; }\n')
            (scripts / 'hosted-login-gate-env-lib.sh').write_text('oasis7_hosted_login_gate_env_defaults() { :; }\n')
            (scripts / 'viewer-dependency-preflight.sh').write_text('viewer_dependency_preflight() { echo preflight >> "$TEST_TRACE_PATH"; }\n')
            ensure = scripts / 'ensure-wasm-bindgen-cli.sh'
            ensure.write_text('#!/bin/sh\necho ensure >> "$TEST_TRACE_PATH"\necho /fixture/wasm-bindgen\n')
            ensure.chmod(0o700)
            bin_dir = repo / 'bin'
            bin_dir.mkdir()
            npm = bin_dir / 'npm'
            npm.write_text('#!/bin/sh\necho "npm $*" >> "$TEST_TRACE_PATH"\necho "controlled real-builder command failure" >&2\nexit 42\n')
            if succeed:
                npm.write_text('#!/bin/sh\necho "npm $*" >> "$TEST_TRACE_PATH"\nmkdir -p "$FAKE_VIEWER_OUTPUT"\nprintf "canonical HTML" > "$FAKE_VIEWER_OUTPUT/viewer.html"\nprintf "compiled visual-test JS" > "$FAKE_VIEWER_OUTPUT/viewer.js"\n')
            npm.chmod(0o700)
            trace = repo / 'trace'
            output = repo / 'evidence'
            command = ['bash', str(scripts / 'viewer-primary-web-entry-regression.sh'), '--out-dir', str(output)]
            if custom:
                artifact = repo / 'caller-artifacts'
                artifact.mkdir()
                command += ['--viewer-static-dir', str(artifact)]
            env = dict(os.environ, TEST_TRACE_PATH=str(trace), FAKE_VIEWER_OUTPUT=str(repo / 'crates/oasis7_viewer/.viewer-test-dist'), PATH=str(bin_dir) + os.pathsep + os.environ['PATH'])
            result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=8)
            self.assertNotEqual(result.returncode, 0)
            commands = trace.read_text().splitlines()
            if custom:
                self.assertEqual(commands, ['cargo'], 'caller artifacts unexpectedly rebuilt or replaced')
            else:
                self.assertEqual(commands[:2], ['preflight', 'ensure'])
                self.assertIn('run build:viewer:visual-test', commands[2])
                if succeed:
                    self.assertEqual(commands[-1], 'cargo')
                    output_dist = repo / 'crates/oasis7_viewer/.viewer-test-dist'
                    self.assertEqual((output_dist / 'index.html').read_bytes(), (output_dist / 'viewer.html').read_bytes())
                    self.assertEqual((output_dist / 'viewer.js').read_text(), 'compiled visual-test JS')
                else:
                    self.assertNotIn('cargo', commands, 'artifact build failure reached runtime prebuild/start')
                    self.assertIn('visual-test artifact build failed', result.stderr)
            self.assertEqual(list(output.glob('*/signed-world-service-fixture.log')), [])

    def test_release_fixture_public_projection_rejects_wrong_identity_or_renderer(self):
        source = (ROOT / 'scripts/viewer-software-safe-step-regression.sh').read_text()
        predicate = source.split("# Hosted guests see", 1)[1].split('wait_for_js_true "', 1)[1].split('" 6000', 1)[0]
        predicate = predicate.replace('${agent_id_json}', '"agent-a"')
        import json
        state = dict(selectedKind='agent', selectedId='agent-a', selectedAgentDebug=dict(environment_class='runtime_live', provider_config_ref='provider://loopback-http/runtime-live/agent-a'), pixelWorldRuntimeStatus='ready', pixelWorldRuntimeSource='wasm_bindgen_runtime')
        for field, value in ((None, None), ('selectedId', 'foreign'), ('pixelWorldRuntimeStatus', 'unavailable'), ('pixelWorldRuntimeSource', 'fixture_mock')):
            candidate = dict(state)
            if field:
                candidate[field] = value
            script = 'global.window={__AW_TEST__:{getState:()=>('+json.dumps(candidate)+')}}; process.exit(('+predicate+') === '+str(field is None).lower()+' ? 0 : 1)'
            result = subprocess.run(['node', '-e', script], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_owned_http_404_interrupts_readiness_deadline(self):
        class NotFound(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                self.send_response(404)
                self.end_headers()
            def log_message(self, *args):
                pass
        server = http.server.HTTPServer(('127.0.0.1', 0), NotFound)
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        try:
            source = (ROOT / 'scripts/viewer-primary-web-entry-regression.sh').read_text()
            functions = source[source.index('wait_for_http() {'):source.index('print_launcher_failure_context() {')]
            script = f'source {ROOT / "scripts/release-web-fixture-lib.sh"};\n' + functions + '\nwait_for_http "$1" 5'
            started = time.monotonic()
            result = subprocess.run(['bash', '-c', script, 'bash', f'http://127.0.0.1:{server.server_port}/'], capture_output=True, text=True, timeout=8)
            self.assertLess(time.monotonic() - started, 3)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('HTTP 404', result.stderr)
        finally:
            server.shutdown()
            server.server_close()
            worker.join(timeout=2)

if __name__ == "__main__":
    unittest.main()

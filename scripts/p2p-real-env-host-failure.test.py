#!/usr/bin/env python3
"""Exercise the actual wrapper using isolated local-probe and SSH stubs."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def run_case(failures, samples=2):
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        scripts = root / "scripts"
        scripts.mkdir()
        for name in ("p2p-real-env-host-monitor.sh", "p2p-real-env-host-summary.py"):
            shutil.copy(ROOT / "scripts" / name, scripts / name)
        values = {"hostname": "test-host", "cpu_cores": "4", "load1": "0.2", "load5": "0.1", "load15": "0.1", "mem_total_bytes": "1000", "mem_available_bytes": "800", "storage_path": "/data", "storage_total_bytes": "1000", "storage_used_bytes": "100", "storage_available_bytes": "900", "storage_used_percent": "10", "service_active_state": "active", "service_sub_state": "running", "service_main_pid": "0"}
        for prefix in ("wrapper", "runtime"):
            for key in ("pid", "pcpu", "pmem", "elapsed_secs", "nlwp", "psr"):
                if prefix == "runtime" or key != "pid": values[prefix + "_" + key] = ""
        (root / "values.json").write_text(json.dumps(values))
        probe = scripts / "p2p-real-env-node-host-sample.sh"
        probe.write_text("#!/usr/bin/env python3\n" + "import json,os,pathlib,sys,shlex\nr=pathlib.Path(os.environ['STUB_ROOT']); label=sys.argv[1].split('.')[0]; p=r/(label+'.count'); n=int(p.read_text())+1 if p.exists() else 1; p.write_text(str(n)); mode=json.loads(os.environ['FAILURES']).get(label+':'+str(n));\nif mode=='empty': sys.exit(0)\nif mode=='duplicate': print('cpu_cores=4')\nif mode=='unknown': print('unexpected=value')\nif mode=='fetch': print('secret-password',file=sys.stderr); sys.exit(7)\nv=json.loads((r/'values.json').read_text()); v['storage_path']=sys.argv[2]\nif mode=='path_mismatch': v['storage_path']='/wrong-path'\nif mode=='decimal_pid': v['service_main_pid']='4.5'\nif mode=='decimal_cpu': v['cpu_cores']='4.5'\nif mode=='malformed': v['cpu_cores']='invalid'\nif mode=='overflow': v['cpu_cores']='9'*400\nif mode=='missing': del v['cpu_cores']\nfor k,value in v.items(): print(k+'='+shlex.quote(value))\n")
        # The wrapper invokes bash for the probe; intercept only that invocation.
        bindir = root / "bin"; bindir.mkdir()
        bash = shutil.which("bash")
        (bindir / "bash").write_text(f'#!/bin/sh\ncase "$1" in *p2p-real-env-node-host-sample.sh) exec python3 "$@";; esac\nexec {bash} "$@"\n')
        (bindir / "ssh").write_text('#!/usr/bin/env python3\nimport os,shlex,subprocess,sys\nargs=shlex.split(sys.argv[-1])[3:]\nsys.exit(subprocess.call(["python3",os.environ["STUB_ROOT"]+"/scripts/p2p-real-env-node-host-sample.sh",*args]))\n')
        (bindir / "sleep").write_text('#!/bin/sh\nexit 0\n')
        for p in bindir.iterdir(): p.chmod(0o755)
        env = dict(os.environ, PATH=str(bindir)+os.pathsep+os.environ['PATH'], STUB_ROOT=str(root), FAILURES=json.dumps(failures))
        env.pop("P2PARCH6_SEQ_SSH_PASSWORD", None)
        env.pop("P2PARCH6_STORAGE_SSH_PASSWORD", None)
        result = subprocess.run([bash, str(scripts / "p2p-real-env-host-monitor.sh"), "--samples", str(samples), "--interval-secs", "1", "--out-dir", str(root / "out"), "--local-service", "local_node.service", "--sequencer-service", "sequencer_ecs.service", "--storage-service", "storage_ecs.service"], env=env, capture_output=True, text=True)
        assert result.returncode == bool(failures), (failures, result.returncode, result.stderr)
        summary_path = root / "out/latest_summary.json"
        assert summary_path.is_file(), (failures, result.stderr)
        summary = json.loads(summary_path.read_text())
        assert summary['aggregate']['expected_sample_count'] == samples * 3
        assert summary['aggregate']['attempted_count'] == samples * 3
        assert summary['aggregate']['failed_sample_count'] == len(failures)
        assert summary['aggregate']['complete'] == (not failures)
        assert 'secret-password' not in ''.join(p.read_text() for p in (root/'out').rglob('*') if p.is_file())
        for label, node in summary['nodes'].items():
            latest_failed = label+':'+str(samples) in failures
            assert node['available'] == (not latest_failed)
            if latest_failed: assert node['latest'] == {}
            assert node['sample_count'] == samples
            assert node['success_sample_count'] + node['failed_sample_count'] == samples
        records = [json.loads(line) for line in next((root / 'out').glob('*/samples.ndjson')).read_text().splitlines()]
        for row in records:
            mode = failures.get(row['label'] + ':' + str(row['sample_index']))
            if mode in ('empty', 'duplicate', 'unknown', 'missing', 'malformed', 'overflow', 'decimal_pid', 'decimal_cpu', 'path_mismatch'):
                assert row['fetch_error'] == 'invalid_sample_payload'
                assert row['stage'] == 'parse'
        assert (root/'out/latest_summary.md').is_file()
        return summary


def parser_checks():
    # Use the real Bash producer's quoting, including ANSI-C control escapes.
    value = "é host with spaces \\ quote'\nline\tend"
    encoded = subprocess.check_output(["bash", "-c", "printf 'hostname=%q\\n' \"$1\"", "bash", value], text=True)
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        payload = root / "sample.env"
        marker = root / "executed"
        payload.write_text(encoded + "cpu_cores=$(touch " + str(marker) + ")\n")
        # No shell evaluation, even when rejecting an invalid or unknown field.
        result = subprocess.run(["python3", str(ROOT / "scripts/p2p-real-env-host-summary.py"), "--sample-env", str(payload), "local_node", "1", "unit", "/data", "", ""], capture_output=True, text=True)
        assert result.returncode == 1
        assert json.loads(result.stdout)["fetch_error"] == "invalid_sample_payload"
        assert not marker.exists()
        values = {key: "0" for key in ("cpu_cores", "load1", "load5", "load15", "mem_total_bytes", "mem_available_bytes", "service_main_pid")}
        values.update(hostname=value, storage_path="/data", service_active_state="active", service_sub_state="running", runtime_pid="")
        values.update({"storage_" + key: "" for key in ("total_bytes", "used_bytes", "available_bytes", "used_percent")})
        values.update({prefix + "_" + key: "" for prefix in ("wrapper", "runtime") for key in ("pcpu", "pmem", "elapsed_secs", "nlwp", "psr")})
        encoded = subprocess.check_output(["bash", "-c", 'LC_ALL=C; for entry; do printf "%s=%q\\n" "${entry%%=*}" "${entry#*=}"; done', "bash", *[key + "=" + item for key, item in values.items()]], text=True)
        payload.write_text(encoded)
        result = subprocess.run(["python3", str(ROOT / "scripts/p2p-real-env-host-summary.py"), "--sample-env", str(payload), "local_node", "1", "unit", "/data", "", ""], capture_output=True, text=True)
        assert result.returncode == 0, result.stdout
        assert json.loads(result.stdout)["host"]["hostname"] == value



if __name__ == '__main__':
    parser_checks()
    for failures in ({'local_node:1':'fetch'}, {'storage_ecs:2':'fetch'}, {f'{label}:{i}':'fetch' for label in ('local_node','sequencer_ecs','storage_ecs') for i in (1,2)}, {'sequencer_ecs:2':'malformed'}, {'storage_ecs:1':'missing'}, {'local_node:2':'overflow'}, {'local_node:1':'empty'}, {'sequencer_ecs:1':'duplicate'}, {'storage_ecs:2':'unknown'}, {'local_node:1':'decimal_pid'}, {'storage_ecs:2':'decimal_cpu'}, {'sequencer_ecs:1':'path_mismatch'}, {}):
        run_case(failures)
    failed = run_case({'sequencer_ecs:2':'fetch'})['nodes']['sequencer_ecs']
    assert failed['last_success']['sample_index'] == 1
    recovered = run_case({'local_node:1':'fetch'})['nodes']['local_node']
    assert recovered['latest_attempt']['sample_fetch_ok'] is True
    print('host failure wrapper regressions passed')

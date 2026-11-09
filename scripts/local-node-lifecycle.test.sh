#!/usr/bin/env bash
set -euo pipefail
# Run against a real compiled runtime, without starting a competing Cargo build.
lifecycle_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
lifecycle_bin=${OASIS7_CHAIN_RUNTIME_BIN:?set OASIS7_CHAIN_RUNTIME_BIN to the compiled runtime}
lifecycle_fixture_bin=${OASIS7_NODE_LIFECYCLE_TEST_BIN:?set OASIS7_NODE_LIFECYCLE_TEST_BIN to the compiled runtime unit-test executable}
lifecycle_tmp=$(mktemp -d "${TMPDIR:-/tmp}/oasis7-native-node.XXXXXX")
lifecycle_tmp=$(cd "$lifecycle_tmp" && pwd -P)
cleanup() {
  local lifecycle_outcome=$?
  if [[ -f "$lifecycle_tmp/node/node.json" ]]; then
    if ! "$lifecycle_bin" node stop --data-dir "$lifecycle_tmp/node" --timeout-ms 5000 >"$lifecycle_tmp/cleanup.json" 2>"$lifecycle_tmp/cleanup.stderr"; then
      echo "preserved unresolved native test instance: $lifecycle_tmp" >&2; return
    fi
    if ! python3 - "$lifecycle_tmp/cleanup.json" <<'PYTHON'
import json,sys
s=json.load(open(sys.argv[1]));assert s['instance'] is None or (s['instance']['state'] in ('stopped','failed') and s['instance']['runtime_pid'] is None)
PYTHON
    then echo "preserved pending native test instance: $lifecycle_tmp" >&2; return; fi
  fi
  if (( lifecycle_outcome == 0 )); then rm -rf "$lifecycle_tmp";
  else echo "preserved native lifecycle failure evidence: $lifecycle_tmp" >&2; fi
}
trap cleanup EXIT
trap 'printf "native lifecycle step failed: line=%s status=%s command=%s\n" "$LINENO" "$?" "$BASH_COMMAND" >&2' ERR
env OASIS7_NODE_EXISTING_WORLD_FIXTURE_OUT="$lifecycle_tmp/generated-world" "$lifecycle_fixture_bin" --exact node_lifecycle::tests::local_node_export_existing_world_fixture --nocapture >"$lifecycle_tmp/fixture.log" 2>&1
python3 - "$lifecycle_root" "$lifecycle_tmp" "$lifecycle_bin" <<'PYTHON'
import hashlib,json,pathlib,sys,socket
root,temp,binary=map(pathlib.Path,sys.argv[1:]);source=root/'doc/testing/evidence'
def write(name,value):
    path=temp/name;path.write_text(json.dumps(value,indent=2)+'\n');return path
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
genesis=json.loads((source/'public-testnet-governed-bootstrap-genesis-2026-06-06.json').read_text())
registry=json.loads((source/'public-testnet-governed-bootstrap-validator-registry-2026-06-06.json').read_text())
manifest=json.loads((source/'public-testnet-governed-bootstrap-manifest-2026-06-06.json').read_text())
g=write('genesis.json',genesis);r=write('registry.json',registry)
(temp/'peers.txt').write_text('/ip4/127.0.0.1/tcp/1/p2p/12D3KooWAuNCCEDu7CdUUDwALuAhuLekZHgVWxAYp4Ag5ti79fJj\n')
sidecar=temp/'generated-world';assert (sidecar/'snapshot.json').is_file() and (sidecar/'journal.json').is_file()
provenance=write('world-generation-provenance.json',{'generated':True})
tree=hashlib.sha256();total=0
files=sorted(p for p in sidecar.rglob('*') if p.is_file())
for path in files:
    size=path.stat().st_size;total+=size
    tree.update(f'{path.relative_to(sidecar).as_posix()}\0{sha(path)}\0{size}\n'.encode())
bundle=write('bundle.json',{'schema_version':'oasis7.release_candidate_bundle.v1','runtime_build':{'sha256':sha(binary)},'generated_world_sidecar':{'kind':'directory','ref':str(sidecar),'resolved_path':str(sidecar),'sha256_tree':tree.hexdigest(),'file_count':len(files),'total_bytes':total},'world_generation_provenance':{'kind':'file','ref':str(provenance),'resolved_path':str(provenance),'sha256':sha(provenance)}})
canonical={'signer_bindings':{f"governance.finality.v1.{v['node_id']}":v['finality_signer_public_key'].lower() for v in registry['validators']},'slot_id':registry['slot_id'],'threshold':registry['threshold'],'threshold_bps':registry['threshold_bps'],'validator_stakes':{f"governance.finality.v1.{v['node_id']}":v['stake'] for v in registry['validators']}}
manifest['deployment_validator_registry']={'ref':'registry.json','sha256':sha(r),'semantic_sha256':hashlib.sha256(json.dumps(canonical,sort_keys=True,separators=(',',':')).encode()).hexdigest()}
manifest['runtime_refs']={'release_candidate_bundle_ref':str(bundle),'genesis_ref':str(g),'bootstrap_peer_ref':str(temp/'peers.txt'),'generated_world_sidecar_ref':str(sidecar),'world_generation_provenance_ref':str(provenance)}
m=write('manifest.json',manifest)
write('chain.json',{'api_version':'oasis7.chain-config/v1','runtime_profile':'compatibility_observer_v1','world_id':genesis['world_id'],'chain_id':genesis['chain_id'],'genesis_sha256':sha(g),'network_tier_manifest':str(m),'manifest_sha256':sha(m)})
s=socket.socket();s.bind(('127.0.0.1',0));(temp/'port').write_text(str(s.getsockname()[1]));s.close()
PYTHON
"$lifecycle_bin" node init --chain "$lifecycle_tmp/chain.json" --data-dir "$lifecycle_tmp/node" --node-id native-e2e-observer --rpc-bind "127.0.0.1:$(cat "$lifecycle_tmp/port")" >"$lifecycle_tmp/init.json"
[[ ! -e "$lifecycle_tmp/node/state" ]]
key_before=$(shasum -a 256 "$lifecycle_tmp/node/keys/node-keypair.toml")
"$lifecycle_bin" node start --config "$lifecycle_tmp/node/node.json" >"$lifecycle_tmp/start.json"
wait_ready() {
  local required=$1
  local observed=0
  for ((attempt=0;attempt<100;attempt++)); do
    observed=$(grep -c '^oasis7_chain_runtime ready\.' "$lifecycle_tmp/node/logs/runtime.log" || true)
    if [[ "$observed" -ge "$required" ]]; then return; fi
    sleep 0.1
  done
  printf 'native lifecycle readiness failed: required=%s observed=%s\n' "$required" "$observed" >&2
  python3 - "$lifecycle_tmp" <<'PYTHON' >&2 || true
import json,pathlib,sys
root=pathlib.Path(sys.argv[1])
for name in ('managed-stop.json','recovery-replacement.json','restart-result.json'):
    path=root/name
    if not path.is_file() or path.stat().st_size>131072:
        continue
    value=json.loads(path.read_text())
    instance=value.get('instance') or {}
    print(json.dumps({'evidence':name,**{k:value.get(k) for k in ('state','stage','revision','local_generation','blocker')},'instance':{k:instance.get(k) for k in ('generation','state','runtime_pid')}}),flush=True)
PYTHON
  cat "$lifecycle_tmp/node/logs/runtime.log" >&2; return 1
}
node_execution_closure_digest() {
  python3 - "$1" <<'PYTHON'
import hashlib,json,os,pathlib,stat,sys
state=pathlib.Path(sys.argv[1])
entries=[]
for name in ('world','records','cas'):
    root=state/name
    if not os.path.lexists(root):
        entries.append([name,'missing'])
        continue
    pending=[root]
    while pending:
        path=pending.pop()
        relative=path.relative_to(state).as_posix()
        metadata=path.lstat()
        if stat.S_ISLNK(metadata.st_mode):
            entries.append([relative,'symlink',os.readlink(path)])
        elif stat.S_ISDIR(metadata.st_mode):
            entries.append([relative,'directory'])
            pending.extend(sorted(path.iterdir(),reverse=True))
        elif stat.S_ISREG(metadata.st_mode):
            digest=hashlib.sha256(path.read_bytes()).hexdigest()
            entries.append([relative,'file',metadata.st_size,digest])
        else:
            raise SystemExit(f'unexpected node execution-closure entry type: {path}')
encoded=json.dumps(sorted(entries),separators=(',',':')).encode()
print(hashlib.sha256(encoded).hexdigest())
PYTHON
}
wait_ready 1
"$lifecycle_bin" node status --data-dir "$lifecycle_tmp/node" >"$lifecycle_tmp/status.json"
python3 - "$lifecycle_tmp/status.json" <<'PYTHON'
import json,sys
s=json.load(open(sys.argv[1]));assert s['instance']['state']=='running';assert s['formal_readiness'] is False;assert s['verified_sync_readiness'] is None
PYTHON
"$lifecycle_bin" node stop --data-dir "$lifecycle_tmp/node" --timeout-ms 5000 >"$lifecycle_tmp/stop.json"
python3 - "$lifecycle_tmp/stop.json" <<'PYTHON'
import json,sys
s=json.load(open(sys.argv[1]));assert s['instance']['state']=='stopped';assert s['blocker'] is None
PYTHON
[[ "$key_before" == "$(shasum -a 256 "$lifecycle_tmp/node/keys/node-keypair.toml")" ]]
[[ -d "$lifecycle_tmp/node/state" ]]
# Exercise the optional real provider, including both restart steps and receipt
# retries, against the same world and retained node identity.
python3 - "$lifecycle_tmp/stop.json" "$lifecycle_tmp/node/node.json" "$lifecycle_tmp/start-request.json" <<'PYTHON'
import json,sys
s=json.load(open(sys.argv[1]));json.dump({'api_version':'oasis7.local-operation/v1','operation_id':'managed-start','world_id':s['chain']['world_id'],'chain_id':s['chain']['chain_id'],'kind':'start','config':sys.argv[2],'expected_generation':s['local_generation'],'expected_config_digest':s['config_digest'],'expected_instance':None},open(sys.argv[3],'w'))
PYTHON
"$lifecycle_bin" op plan --request "$lifecycle_tmp/start-request.json" >"$lifecycle_tmp/plan.json"
"$lifecycle_bin" op submit --request "$lifecycle_tmp/start-request.json" --store "$lifecycle_tmp/domain" >"$lifecycle_tmp/submitted.json"
"$lifecycle_bin" op resume --store "$lifecycle_tmp/domain" --id managed-start --revision 1 >"$lifecycle_tmp/managed-start.json"
wait_ready 2
"$lifecycle_bin" node status --data-dir "$lifecycle_tmp/node" >"$lifecycle_tmp/managed-status.json"
python3 - "$lifecycle_tmp/managed-start.json" "$lifecycle_tmp/managed-status.json" "$lifecycle_tmp/node/node.json" "$lifecycle_tmp/restart-request.json" <<'PYTHON'
import json,sys
op=json.load(open(sys.argv[1]));s=json.load(open(sys.argv[2]));assert op['state']=='succeeded';assert s['instance']['generation']==2
json.dump({'api_version':'oasis7.local-operation/v1','operation_id':'managed-restart','world_id':s['chain']['world_id'],'chain_id':s['chain']['chain_id'],'kind':'restart','config':sys.argv[3],'expected_generation':s['local_generation'],'expected_config_digest':s['config_digest'],'expected_instance':s['instance']},open(sys.argv[4],'w'))
PYTHON
"$lifecycle_bin" op submit --request "$lifecycle_tmp/restart-request.json" --store "$lifecycle_tmp/domain" >"$lifecycle_tmp/restart-submitted.json"
"$lifecycle_bin" op resume --store "$lifecycle_tmp/domain" --id managed-restart --revision 1 >"$lifecycle_tmp/restart-stop.json"
revision=$(python3 - "$lifecycle_tmp/restart-stop.json" <<'PYTHON'
import json,sys
s=json.load(open(sys.argv[1]));assert s['stage']=='stop_verified';print(s['revision'])
PYTHON
)
"$lifecycle_bin" op resume --store "$lifecycle_tmp/domain" --id managed-restart --revision "$revision" >"$lifecycle_tmp/restart-result.json"
wait_ready 3
"$lifecycle_bin" op submit --request "$lifecycle_tmp/start-request.json" --store "$lifecycle_tmp/domain" >"$lifecycle_tmp/original-start-retry.json"
"$lifecycle_bin" node status --data-dir "$lifecycle_tmp/node" >"$lifecycle_tmp/replaced-status.json"
python3 - "$lifecycle_tmp/restart-result.json" "$lifecycle_tmp/original-start-retry.json" "$lifecycle_tmp/replaced-status.json" <<'PYTHON'
import json,sys
restart,old,current=(json.load(open(p)) for p in sys.argv[1:]);assert restart['state']=='succeeded';assert len(restart['results'])==2;assert old['results'][0]['target']['generation']==2;assert current['instance']['generation']==3;assert current['instance']['state']=='running';assert current['formal_readiness'] is False
PYTHON
# Lose only this test's management ledger after a completed managed stop.
# Recovery must query the old generation and leave its running replacement alone.
python3 - "$lifecycle_tmp/replaced-status.json" "$lifecycle_tmp/node/node.json" "$lifecycle_tmp/stop-request.json" <<'PYTHON'
import json,sys
s=json.load(open(sys.argv[1]));json.dump({'api_version':'oasis7.local-operation/v1','operation_id':'managed-stop','world_id':s['chain']['world_id'],'chain_id':s['chain']['chain_id'],'kind':'stop','config':sys.argv[2],'expected_generation':s['local_generation'],'expected_config_digest':s['config_digest'],'expected_instance':s['instance']},open(sys.argv[3],'w'))
PYTHON
"$lifecycle_bin" op submit --request "$lifecycle_tmp/stop-request.json" --store "$lifecycle_tmp/domain" >"$lifecycle_tmp/stop-submitted.json"
"$lifecycle_bin" op resume --store "$lifecycle_tmp/domain" --id managed-stop --revision 1 >"$lifecycle_tmp/managed-stop.json"
rm "$lifecycle_tmp/domain/operations.json"
"$lifecycle_bin" node start --config "$lifecycle_tmp/node/node.json" >"$lifecycle_tmp/recovery-replacement.json"
wait_ready 4
"$lifecycle_bin" op recover --request "$lifecycle_tmp/stop-request.json" --store "$lifecycle_tmp/domain" >"$lifecycle_tmp/recovered-stop.json"
"$lifecycle_bin" op resume --store "$lifecycle_tmp/domain" --id managed-stop --revision 1 >"$lifecycle_tmp/recovered-stop-retry.json"
"$lifecycle_bin" node status --data-dir "$lifecycle_tmp/node" >"$lifecycle_tmp/after-recovery.json"
python3 - "$lifecycle_tmp/recovered-stop.json" "$lifecycle_tmp/recovered-stop-retry.json" "$lifecycle_tmp/after-recovery.json" <<'PYTHON'
import json,sys
recovered,repeated,current=(json.load(open(p)) for p in sys.argv[1:]);assert recovered['recovered'] and recovered['state']=='succeeded';assert recovered['results'][0]['target']['generation']==3;assert repeated['results'][0]['target']['generation']==3;assert current['instance']['generation']==4 and current['instance']['state']=='running'
PYTHON
# Build a different-SHA package candidate bound to this executable. This local
# compatibility-observer fixture has no committed execution record/CAS closure,
# so package admission must refuse before writing an OperationRecord or stopping
# the running owner. A successful different-SHA package upgrade still needs a
# genuinely qualified execution-history fixture; this case must not simulate it.
native_package_root="$lifecycle_tmp/native-package"
native_artifact_dir="$lifecycle_tmp/native-artifact"
native_runtime="$native_artifact_dir/oasis7_chain_runtime"
native_buildinfo="$native_artifact_dir/BUILDINFO"
mkdir -p "$native_package_root/releases/old/bin" "$native_artifact_dir"
native_prior_runtime="$native_package_root/releases/old/bin/oasis7_chain_runtime"
cp "$lifecycle_bin" "$native_prior_runtime"
chmod 755 "$native_prior_runtime"
native_source_sha256=$(shasum -a 256 "$lifecycle_bin" | awk '{print $1}')
native_prior_runtime_sha256=$(shasum -a 256 "$native_prior_runtime" | awk '{print $1}')
[[ "$native_prior_runtime_sha256" == "$native_source_sha256" ]]
native_prior_package_version='0.0.0+local-native-prior.1'
native_prior_commit='0123456789abcdef0123456789abcdef01234567'
native_prior_run_id='local-native-prior'
cat >"$native_package_root/releases/old/BUILDINFO" <<EOF
workflow=Local Native Package
commit=$native_prior_commit
package_version=$native_prior_package_version
run_id=$native_prior_run_id
platform=local-native
runtime_sha256=$native_prior_runtime_sha256
EOF
ln -s "$native_package_root/releases/old" "$native_package_root/current"
printf '%s\n' "$native_prior_package_version" >"$native_package_root/CURRENT_VERSION"
cp "$native_package_root/releases/old/BUILDINFO" "$native_package_root/DEPLOYED_BUILDINFO"
# Exercise a different-SHA release replacement with an operator-pinned
# candidate chain config. The node/world/registry/peer pins stay unchanged;
# only the runtime_build subtree and bundle reference move to this artifact.
cp "$lifecycle_bin" "$native_runtime"
printf 'oasis7-local-native-candidate-runtime-v1\n' >>"$native_runtime"
chmod 755 "$native_runtime"
native_runtime_sha256=$(shasum -a 256 "$native_runtime" | awk '{print $1}')
[[ "$native_runtime_sha256" != "$native_source_sha256" ]]
native_package_version='0.0.0+local-native-e2e.1'
native_commit='abcdef0123456789abcdef0123456789abcdef01'
native_run_id='local-native-e2e'
cat >"$native_buildinfo" <<EOF
workflow=Local Native Package
commit=$native_commit
package_version=$native_package_version
run_id=$native_run_id
platform=local-native
runtime_sha256=$native_runtime_sha256
EOF
native_buildinfo_sha256=$(shasum -a 256 "$native_buildinfo" | awk '{print $1}')
native_candidate_bundle="$lifecycle_tmp/candidate-e2e-bundle.json"
native_candidate_manifest="$lifecycle_tmp/candidate-e2e-manifest.json"
native_candidate_chain="$lifecycle_tmp/candidate-e2e-chain.json"
python3 - "$lifecycle_tmp/chain.json" "$lifecycle_tmp/manifest.json" "$lifecycle_tmp/bundle.json" \
  "$native_runtime" "$native_runtime_sha256" "$native_package_version" "$native_commit" "$native_run_id" \
  "$native_candidate_bundle" "$native_candidate_manifest" "$native_candidate_chain" <<'PYTHON'
import copy,hashlib,json,pathlib,sys
chain_path,manifest_path,bundle_path=map(pathlib.Path,sys.argv[1:4])
runtime=pathlib.Path(sys.argv[4]);runtime_sha,version,commit,run_id=sys.argv[5:9]
bundle_out,manifest_out,chain_out=map(pathlib.Path,sys.argv[9:12])
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
bundle=json.loads(bundle_path.read_text())
bundle['runtime_build']={
  'commit':commit,'git_commit':commit,'kind':'file','path':str(runtime),
  'resolved_path':str(runtime),'ref':f'local-native:{version}:{run_id}',
  'sha256':runtime_sha,'size_bytes':runtime.stat().st_size,
  'package_version':version,'run_id':run_id,
  'updated_by':f'local-node-lifecycle.test.sh {version}',
}
bundle_out.write_text(json.dumps(bundle,indent=2,sort_keys=True)+'\n')
manifest=json.loads(manifest_path.read_text())
manifest['runtime_refs']['release_candidate_bundle_ref']=str(bundle_out)
manifest_out.write_text(json.dumps(manifest,indent=2,sort_keys=True)+'\n')
chain=json.loads(chain_path.read_text())
chain['network_tier_manifest']=str(manifest_out)
chain['manifest_sha256']=sha(manifest_out)
chain_out.write_text(json.dumps(chain,indent=2,sort_keys=True)+'\n')
PYTHON
native_candidate_chain_sha256=$(shasum -a 256 "$native_candidate_chain" | awk '{print $1}')
native_helper_sha256=$(shasum -a 256 "$lifecycle_root/scripts/p2p-public-testnet-package-node-upgrade.sh" | awk '{print $1}')
python3 - "$lifecycle_tmp/after-recovery.json" "$lifecycle_tmp/node/node.json" "$native_package_root" \
  "$lifecycle_root/scripts/p2p-public-testnet-package-node-upgrade.sh" "$native_helper_sha256" \
  "$native_runtime" "$native_runtime_sha256" "$native_buildinfo" "$native_buildinfo_sha256" \
  "$native_package_version" "$native_commit" "$native_run_id" "$native_candidate_chain" "$native_candidate_chain_sha256" \
  "$lifecycle_tmp/package-upgrade-request.json" <<'PYTHON'
import json,sys
status=json.load(open(sys.argv[1]))
request={
  'api_version':'oasis7.local-operation/v1',
  'operation_id':'local-native-package-upgrade',
  'world_id':status['chain']['world_id'],
  'chain_id':status['chain']['chain_id'],
  'kind':'package_upgrade',
  'config':sys.argv[2],
  'expected_generation':status['local_generation'],
  'expected_config_digest':status['config_digest'],
  'expected_instance':status['instance'],
  'package_upgrade':{
    'api_version':'oasis7.local-package-upgrade-request/v1',
    'profile':'local_native_package_v1',
    'node_root':sys.argv[3],
    'helper_script':sys.argv[4],
    'helper_sha256':sys.argv[5],
    'native_artifact':{
      'runtime_binary':sys.argv[6],
      'runtime_sha256':sys.argv[7],
      'buildinfo':sys.argv[8],
      'buildinfo_sha256':sys.argv[9],
    },
    'candidate_chain_config':{
      'api_version':'oasis7.operator-pinned-candidate-chain-config/v1',
      'path':sys.argv[13],
      'sha256':sys.argv[14],
    },
    'package_version':sys.argv[10],
    'commit':sys.argv[11],
    'run_id':sys.argv[12],
    'artifact_ref':None,
  },
}
json.dump(request,open(sys.argv[15],'w'),sort_keys=True,separators=(',',':'))
PYTHON
# Package compatibility admission binds current_exe() to the exact release
# selected by node_root/current. Planning is deliberately pure and can describe
# this candidate without probing runtime compatibility. The real refusal occurs
# at stage, before the helper writes a package transaction or the owner is stopped.
native_selected_release=$(cd -P "$native_package_root/current" && pwd)
native_selected_runtime="$native_selected_release/bin/oasis7_chain_runtime"
[[ "$native_selected_runtime" == "$native_prior_runtime" ]]
[[ -x "$native_selected_runtime" ]]
"$lifecycle_bin" node status --data-dir "$lifecycle_tmp/node" >"$lifecycle_tmp/before-package-admission.json"
package_config_sha256=$(shasum -a 256 "$lifecycle_tmp/node/node.json" | awk '{print $1}')
package_binding_sha256=$(shasum -a 256 "$lifecycle_tmp/node/node-binding.json" | awk '{print $1}')
package_current_target=$(readlink "$native_package_root/current")
package_prior_runtime_sha256=$(shasum -a 256 "$native_prior_runtime" | awk '{print $1}')
package_prior_buildinfo_sha256=$(shasum -a 256 "$native_package_root/releases/old/BUILDINFO" | awk '{print $1}')
cp "$lifecycle_tmp/domain/operations.json" "$lifecycle_tmp/package-domain-before.json"
package_execution_closure_before=$(node_execution_closure_digest "$lifecycle_tmp/node/state")
"$native_selected_runtime" op plan --request "$lifecycle_tmp/package-upgrade-request.json" \
  >"$lifecycle_tmp/package-upgrade-plan.json"
"$native_selected_runtime" op submit --request "$lifecycle_tmp/package-upgrade-request.json" \
  --store "$lifecycle_tmp/domain" >"$lifecycle_tmp/package-upgrade-submitted.json"
package_submit_revision=$(python3 - "$lifecycle_tmp/package-upgrade-submitted.json" <<'PYTHON'
import json,sys
record=json.load(open(sys.argv[1]))
assert record['request']['operation_id']=='local-native-package-upgrade'
assert record['state']=='queued' and record['stage']=='prepared'
assert record['revision']==1
print(record['revision'])
PYTHON
)
"$native_selected_runtime" op resume --store "$lifecycle_tmp/domain" \
  --id local-native-package-upgrade --revision "$package_submit_revision" \
  >"$lifecycle_tmp/package-upgrade-result.json"
python3 - "$lifecycle_tmp/package-upgrade-plan.json" "$lifecycle_tmp/package-upgrade-result.json" \
  "$lifecycle_tmp/package-upgrade-request.json" <<'PYTHON'
import json,sys
plan,result,request=(json.load(open(path)) for path in sys.argv[1:4])
assert plan['state']=='queued' and plan['stage']=='prepared'
assert plan['request']['operation_id']==request['operation_id']
assert result['request']['operation_id']==request['operation_id']
assert result['state']=='reconciling'
assert result['stage']=='package_stage_unresolved'
blocker=result.get('blocker') or ''
assert blocker.startswith('ORIGINAL_OUTCOME_UNKNOWN: '),blocker
assert 'materialized World has no exact latest execution record and CAS root; compatibility is unproven' in blocker,blocker
progress=result['package_upgrade_progress']
assert progress.get('stage_receipt') is None
assert progress.get('promotion_receipt') is None
assert progress.get('rollback_receipt') is None
assert result['results']==[]
assert result['action_attempts']==['op:local-native-package-upgrade:stage']
PYTHON
"$lifecycle_bin" node status --data-dir "$lifecycle_tmp/node" >"$lifecycle_tmp/after-package-admission-refusal.json"
package_execution_closure_after=$(node_execution_closure_digest "$lifecycle_tmp/node/state")
python3 - "$lifecycle_tmp/before-package-admission.json" "$lifecycle_tmp/after-package-admission-refusal.json" \
  "$lifecycle_tmp/package-domain-before.json" "$lifecycle_tmp/domain/operations.json" \
  "$lifecycle_tmp/package-upgrade-request.json" <<'PYTHON'
import json,sys
before,after=(json.load(open(path)) for path in sys.argv[1:3])
assert before['instance']['state']=='running' and before['instance']['runtime_pid'] is not None
assert before['instance']==after['instance']
assert before['local_generation']==after['local_generation']
assert before['config_digest']==after['config_digest']
assert before['chain']==after['chain']
before_store,after_store=(json.load(open(path)) for path in sys.argv[3:5])
request=json.load(open(sys.argv[5]))
before_records={record['request']['operation_id']:record for record in before_store['records']}
after_records={record['request']['operation_id']:record for record in after_store['records']}
operation_id=request['operation_id']
assert operation_id not in before_records
assert set(after_records)==set(before_records)|{operation_id}
for old_id,old_record in before_records.items():
    assert after_records[old_id]==old_record,old_id
new_record=after_records[operation_id]
assert new_record['request']==request
assert new_record['state']=='reconciling'
assert new_record['stage']=='package_stage_unresolved'
assert new_record['package_upgrade_progress'].get('stage_receipt') is None
assert new_record['package_upgrade_progress'].get('promotion_receipt') is None
assert new_record['package_upgrade_progress'].get('rollback_receipt') is None
assert new_record['results']==[]
assert new_record['action_attempts']==['op:local-native-package-upgrade:stage']
PYTHON
assert_preimage_same() {
  local label=$1 expected=$2 actual=$3
  if [[ "$expected" != "$actual" ]]; then
    printf 'package stage refusal changed %s (expected=%q actual=%q)\n' \
      "$label" "$expected" "$actual" >&2
    return 1
  fi
}
assert_path_absent() {
  local label=$1 path=$2
  if [[ -e "$path" ]]; then
    printf 'package stage refusal unexpectedly created %s: %s\n' "$label" "$path" >&2
    return 1
  fi
}
assert_preimage_same 'node config SHA256' "$package_config_sha256" \
  "$(shasum -a 256 "$lifecycle_tmp/node/node.json" | awk '{print $1}')"
assert_preimage_same 'node binding SHA256' "$package_binding_sha256" \
  "$(shasum -a 256 "$lifecycle_tmp/node/node-binding.json" | awk '{print $1}')"
assert_preimage_same 'current symlink target' "$package_current_target" \
  "$(readlink "$native_package_root/current")"
assert_preimage_same 'prior runtime SHA256' "$package_prior_runtime_sha256" \
  "$(shasum -a 256 "$native_prior_runtime" | awk '{print $1}')"
assert_preimage_same 'prior BUILDINFO SHA256' "$package_prior_buildinfo_sha256" \
  "$(shasum -a 256 "$native_package_root/releases/old/BUILDINFO" | awk '{print $1}')"
assert_preimage_same 'World/records/CAS closure digest' "$package_execution_closure_before" \
  "$package_execution_closure_after"
assert_path_absent 'package operation directory' \
  "$native_package_root/package-upgrade-operations/local-native-package-upgrade"
assert_path_absent 'stage receipt' \
  "$native_package_root/package-upgrade-operations/local-native-package-upgrade/stage-receipt.json"
assert_path_absent 'promotion receipt' \
  "$native_package_root/package-upgrade-operations/local-native-package-upgrade/promotion-receipt.json"
assert_path_absent 'runtime compatibility receipt' \
  "$lifecycle_tmp/node/state/package-runtime-compatibility/local-native-package-upgrade.json"

# Build a synthetic planning-only candidate to confirm the request can carry a
# distinct operator-pinned artifact identity. It exits if executed and is not
# runtime-compatibility or failed-start evidence.
failed_runtime="$native_artifact_dir/oasis7_chain_runtime-plan-smoke"
cat >"$failed_runtime" <<'SH'
#!/bin/sh
exit 42
SH
chmod 755 "$failed_runtime"
failed_runtime_sha256=$(shasum -a 256 "$failed_runtime" | awk '{print $1}')
failed_buildinfo="$native_artifact_dir/BUILDINFO-plan-smoke"
failed_package_version='0.0.0+local-native-plan-smoke.1'
failed_commit='abcdef0123456789abcdef0123456789abcdef02'
cat >"$failed_buildinfo" <<EOF
workflow=Local Native Package
commit=$failed_commit
package_version=$failed_package_version
run_id=local-native-plan-smoke
platform=local-native
runtime_sha256=$failed_runtime_sha256
EOF
failed_buildinfo_sha256=$(shasum -a 256 "$failed_buildinfo" | awk '{print $1}')
failed_candidate_bundle="$lifecycle_tmp/candidate-plan-smoke-bundle.json"
failed_candidate_manifest="$lifecycle_tmp/candidate-plan-smoke-manifest.json"
failed_candidate_chain="$lifecycle_tmp/candidate-plan-smoke-chain.json"
python3 - "$native_candidate_chain" "$native_candidate_manifest" "$native_candidate_bundle" \
  "$failed_runtime" "$failed_runtime_sha256" "$failed_package_version" "$failed_commit" \
  'local-native-plan-smoke' "$failed_candidate_bundle" "$failed_candidate_manifest" "$failed_candidate_chain" <<'PYTHON'
import copy,hashlib,json,pathlib,sys
chain_path,manifest_path,bundle_path=map(pathlib.Path,sys.argv[1:4])
runtime=pathlib.Path(sys.argv[4]);runtime_sha,version,commit,run_id=sys.argv[5:9]
bundle_out,manifest_out,chain_out=map(pathlib.Path,sys.argv[9:12])
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
bundle=json.loads(bundle_path.read_text())
bundle['runtime_build']={
  'commit':commit,'git_commit':commit,'kind':'file','path':str(runtime),
  'resolved_path':str(runtime),'ref':f'local-native:{version}:{run_id}',
  'sha256':runtime_sha,'size_bytes':runtime.stat().st_size,
  'package_version':version,'run_id':run_id,
  'updated_by':f'local-node-lifecycle.test.sh {version}',
}
bundle_out.write_text(json.dumps(bundle,indent=2,sort_keys=True)+'\n')
manifest=json.loads(manifest_path.read_text())
manifest['runtime_refs']['release_candidate_bundle_ref']=str(bundle_out)
manifest_out.write_text(json.dumps(manifest,indent=2,sort_keys=True)+'\n')
chain=json.loads(chain_path.read_text())
chain['network_tier_manifest']=str(manifest_out)
chain['manifest_sha256']=sha(manifest_out)
chain_out.write_text(json.dumps(chain,indent=2,sort_keys=True)+'\n')
PYTHON
failed_candidate_chain_sha256=$(shasum -a 256 "$failed_candidate_chain" | awk '{print $1}')
# Each package operation must be planned and advanced by the exact runtime
# selected when that operation began. The current symlink still selects the
# original prior release because the first candidate was refused during stage.
native_selected_release=$(cd -P "$native_package_root/current" && pwd)
native_selected_runtime="$native_selected_release/bin/oasis7_chain_runtime"
[[ -x "$native_selected_runtime" ]]
"$lifecycle_bin" node status --data-dir "$lifecycle_tmp/node" >"$lifecycle_tmp/before-plan-smoke.json"
python3 - "$lifecycle_tmp/before-plan-smoke.json" "$lifecycle_tmp/node/node.json" "$native_package_root" \
  "$lifecycle_root/scripts/p2p-public-testnet-package-node-upgrade.sh" "$native_helper_sha256" \
  "$failed_runtime" "$failed_runtime_sha256" "$failed_buildinfo" "$failed_buildinfo_sha256" \
  "$failed_package_version" "$failed_commit" "$failed_candidate_chain" "$failed_candidate_chain_sha256" \
  "$lifecycle_tmp/plan-smoke-package-upgrade-request.json" <<'PYTHON'
import json,sys
status=json.load(open(sys.argv[1]))
request={
  'api_version':'oasis7.local-operation/v1',
  'operation_id':'local-native-package-plan-smoke',
  'world_id':status['chain']['world_id'],
  'chain_id':status['chain']['chain_id'],
  'kind':'package_upgrade',
  'config':sys.argv[2],
  'expected_generation':status['local_generation'],
  'expected_config_digest':status['config_digest'],
  'expected_instance':status['instance'],
  'package_upgrade':{
    'api_version':'oasis7.local-package-upgrade-request/v1',
    'profile':'local_native_package_v1',
    'node_root':sys.argv[3],
    'helper_script':sys.argv[4],
    'helper_sha256':sys.argv[5],
    'native_artifact':{
      'runtime_binary':sys.argv[6],
      'runtime_sha256':sys.argv[7],
      'buildinfo':sys.argv[8],
      'buildinfo_sha256':sys.argv[9],
    },
    'candidate_chain_config':{
      'api_version':'oasis7.operator-pinned-candidate-chain-config/v1',
      'path':sys.argv[12],
      'sha256':sys.argv[13],
    },
    'package_version':sys.argv[10],
    'commit':sys.argv[11],
    'run_id':'local-native-plan-smoke',
    'artifact_ref':None,
  },
}
json.dump(request,open(sys.argv[14],'w'),sort_keys=True,separators=(',',':'))
PYTHON
python3 - "$lifecycle_tmp/plan-smoke-package-upgrade-request.json" "$lifecycle_tmp/refused-package-upgrade-request.json" <<'PYTHON'
import json,sys
request=json.load(open(sys.argv[1]))
request['operation_id']='local-native-package-refused-candidate'
request['package_upgrade']['candidate_chain_config']['sha256']='0'*64
json.dump(request,open(sys.argv[2],'w'),sort_keys=True,separators=(',',':'))
PYTHON
if "$native_selected_runtime" op plan --request "$lifecycle_tmp/refused-package-upgrade-request.json" \
  >"$lifecycle_tmp/refused-package-upgrade-plan.stdout" 2>"$lifecycle_tmp/refused-package-upgrade-plan.stderr"; then
  echo 'candidate chain config with a mismatched operator pin unexpectedly passed planning' >&2
  exit 1
fi
if ! grep -Fq 'operator-pinned candidate chain config SHA256 mismatch' \
  "$lifecycle_tmp/refused-package-upgrade-plan.stdout" \
  "$lifecycle_tmp/refused-package-upgrade-plan.stderr"; then
  echo 'candidate-chain pin refusal diagnostic missing from stdout and stderr' >&2
  cat "$lifecycle_tmp/refused-package-upgrade-plan.stdout" \
    "$lifecycle_tmp/refused-package-upgrade-plan.stderr" >&2
  exit 1
fi
"$lifecycle_bin" node status --data-dir "$lifecycle_tmp/node" >"$lifecycle_tmp/after-refused-package.json"
python3 - "$lifecycle_tmp/before-plan-smoke.json" "$lifecycle_tmp/after-refused-package.json" \
  "$native_package_root/current" "$native_package_root/package-upgrade-operations/local-native-package-refused-candidate" <<'PYTHON'
import json,pathlib,sys
before,after=(json.load(open(path)) for path in sys.argv[1:3])
for key in ('incarnation','generation','config_digest','state','runtime_pid','action_id'):
    assert before['instance'][key]==after['instance'][key],key
assert after['instance']['state']=='running'
assert before['config_digest']==after['config_digest']
assert pathlib.Path(sys.argv[3]).is_symlink()
assert not pathlib.Path(sys.argv[4]).exists()
assert not pathlib.Path(sys.argv[3]+'/package-upgrade-operations/local-native-package-refused-candidate').exists()
PYTHON

# This synthetic exit-42 file is only a planning smoke: planning binds the
# operator-pinned request but does not execute candidate compatibility probes.
# Do not submit/resume it or count this as descriptor or failed-start evidence.
package_domain_ledger_before_fake_plan=$(shasum -a 256 "$lifecycle_tmp/domain/operations.json" | awk '{print $1}')
if ! "$native_selected_runtime" op plan --request "$lifecycle_tmp/plan-smoke-package-upgrade-request.json" \
  >"$lifecycle_tmp/plan-smoke-package-upgrade-plan.stdout" 2>"$lifecycle_tmp/plan-smoke-package-upgrade-plan.stderr"; then
  cat "$lifecycle_tmp/plan-smoke-package-upgrade-plan.stdout" "$lifecycle_tmp/plan-smoke-package-upgrade-plan.stderr" >&2
  echo 'synthetic candidate planning smoke failed before producing its pure plan' >&2
  exit 1
fi
python3 - "$lifecycle_tmp/plan-smoke-package-upgrade-plan.stdout" \
  "$lifecycle_tmp/plan-smoke-package-upgrade-request.json" <<'PYTHON'
import json,sys
plan=json.load(open(sys.argv[1]));request=json.load(open(sys.argv[2]))
assert plan['state']=='queued' and plan['stage']=='prepared'
assert plan['request']['operation_id']==request['operation_id']
assert plan['package_upgrade_progress'].get('stage_receipt') is None
assert plan['package_upgrade_progress'].get('promotion_receipt') is None
assert plan.get('action_attempts',[])==[]
assert plan.get('results',[])==[]
PYTHON
"$lifecycle_bin" node status --data-dir "$lifecycle_tmp/node" >"$lifecycle_tmp/after-plan-smoke.json"
python3 - "$lifecycle_tmp/before-plan-smoke.json" "$lifecycle_tmp/after-plan-smoke.json" \
  "$native_package_root" "$native_package_version" <<'PYTHON'
import json,pathlib,sys
before,after=(json.load(open(path)) for path in sys.argv[1:3])
for key in ('incarnation','generation','config_digest','state','runtime_pid','action_id'):
    assert before['instance'][key]==after['instance'][key],key
assert after['instance']['state']=='running'
assert before['config_digest']==after['config_digest']
root=pathlib.Path(sys.argv[3])
assert (root/'current').is_symlink()
assert (root/'current').resolve(strict=True)==root/'releases'/'old'
assert not (root/'package-upgrade-operations/local-native-package-plan-smoke').exists()
PYTHON
assert_preimage_same 'operation ledger SHA256 after plan smoke' \
  "$package_domain_ledger_before_fake_plan" \
  "$(shasum -a 256 "$lifecycle_tmp/domain/operations.json" | awk '{print $1}')"
assert_preimage_same 'World/records/CAS closure digest after plan smoke' \
  "$package_execution_closure_before" \
  "$(node_execution_closure_digest "$lifecycle_tmp/node/state")"
"$lifecycle_bin" node stop --data-dir "$lifecycle_tmp/node" --timeout-ms 5000 >"$lifecycle_tmp/final-stop.json"
assert_preimage_same 'node keypair SHA256' "$key_before" \
  "$(shasum -a 256 "$lifecycle_tmp/node/keys/node-keypair.toml")"
echo 'ok: native node lifecycle + recovery, stage-time refusal for unqualified package history, wrong-pin refusal, and pure-plan candidate smoke; qualified different-SHA upgrade remains TODO'

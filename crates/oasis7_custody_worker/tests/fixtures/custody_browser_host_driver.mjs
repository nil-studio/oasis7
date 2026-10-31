// Trusted local fixture supervisor. Never mounted as HTTP or an M tool.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {createInterface} from 'node:readline';
import {isAbsolute,resolve} from 'node:path';
import {existsSync} from 'node:fs';
export function actualHostDriver({directory,fixtureSource,port}){
  const image=process.env.OASIS7_HOST_COMPONENT_IMAGE,artifact=process.env.OASIS7_HOST_COMPONENT_CLI,skillBundle=process.env.OASIS7_HOST_COMPONENT_SKILL_BUNDLE;
  assert.match(image??'',/^sha256:[0-9a-f]{64}$/,'trusted immutable local Linux image required');
  assert.ok(isAbsolute(artifact??'')&&existsSync(artifact),'actual canonical Linux Host CLI required');
  assert.ok(isAbsolute(skillBundle??'')&&existsSync(resolve(skillBundle,'manifest.json')),'actual same-source formal Skill bundle required');
  assert.ok(Number.isInteger(port)&&port>=1024&&port<=65535);
  const child=spawn('rtk',['proxy','docker','run','--rm','--pull=never','--read-only',
    '--cap-drop=ALL',...['CHOWN','SETUID','SETGID','SETPCAP','KILL'].map(v=>'--cap-add='+v),
    '--security-opt=no-new-privileges','--tmpfs','/tmp:rw,nosuid,nodev,mode=1777',
    '--tmpfs','/var/lib/oasis7-custody:rw,nosuid,nodev,mode=0755',
    '--mount',`type=bind,src=${artifact},dst=/artifacts/host-cli,readonly`,
    '--mount',`type=bind,src=${fixtureSource},dst=/fixtures,readonly`,
    '--mount',`type=bind,src=${skillBundle},dst=/formal-skill,readonly`,
    '--mount',`type=bind,src=${resolve(directory,'localhost.crt')},dst=/fixture-ca/localhost.crt,readonly`,
    '--env',`OASIS7_LOCAL_RP_PORT=${port}`,image,'python3','/fixtures/host_supervisor.py'],
    {stdio:['pipe','pipe','ignore']});
  const lines=createInterface({input:child.stdout});let pending=null,exited=false;
  lines.on('line',line=>{if(!pending)return;const callback=pending;pending=null;
    try{const response=JSON.parse(line);assert.equal(response.ok,true,'actual Host fixture rejected');callback.resolve(response.result);}
    catch{callback.reject(Error('actual Host component fixture rejected'));}});
  child.once('exit',()=>{exited=true;pending?.reject(Error('Host component fixture exited'));pending=null;});
  const rpc=async(operation,input)=>{assert.equal(pending,null,'serial fixture RPC');assert.equal(exited,false);
    return new Promise((resolve,reject)=>{const timer=setTimeout(()=>{pending=null;reject(Error('Host fixture RPC timeout'));},45000);
      pending={resolve:v=>{clearTimeout(timer);resolve(v);},reject:e=>{clearTimeout(timer);reject(e);}};
      child.stdin.write(JSON.stringify({operation,input})+'\n');});};
  return {rpc,async initialize({target,origin}){
    const descriptor={api_version:1,environment:'test',transport_profile:'native-broker-custody-v1',tool_profile:'custody-native-v1',
      host_instance_id:'actual-linux-host-v2',client_public_key:'',build_digest:'',skill_manifest_digest:'',tool_manifest_digest:'',
      https_origin:'https://host.example',restricted_runtime_identity:'local-component-not-model-lifecycle-qualified',
      isolation_mechanism:'separate_os_principal',isolation_acceptance_refs:['component-not-CT008']};
    const result=await rpc('initialize',{request_id:'actual-host-v2-init',identity:{api_version:1,enabled:true,service_origin:origin,
      skill_manifest_path:'/replaced-by-trusted-supervisor',descriptor,target,isolation:{host_uid:21001,model_uid:21002,native:{kind:'remote',https_origin:origin}}}});
    assert.equal(result.deployment_accepted,false);return result.descriptor;
  },async exercise({host_binding,counters,setStage,nativeCounts}){
    setStage('v2-actual-host-bind');await rpc('bind',host_binding);
    const enrollment=host_binding.pairing.enrollment_id,session=host_binding.session_id;
    setStage('v2-actual-host-private-claim');const before=counters();const nativeBefore=Object.values(nativeCounts).reduce((a,b)=>a+b,0);
    const claim=await rpc('claim',{api_version:1,enrollment_id:enrollment,test_session_id:session,request_id:'actual-host-v2-joint-claim',
      purposes:['execute','read'],replace:false,expected_generation:null});
    assert.equal(claim.state,'credential_reply_retained');const claimed=counters();assert.equal(claimed.claims,before.claims+1);
    setStage('v2-actual-host-os-serve');const isolation=await rpc('serve',{});
    assert.equal(isolation.ready,true);assert.equal(isolation.private_denied_m,true);assert.equal(isolation.private_denied_n_witness,true);
    setStage('v2-actual-formal-skill-m-launch');
    const probe=await rpc('model_probe',{resource_id:host_binding.resource_id});
    assert.equal(probe.kind,'deterministic_component_not_real_model');
    assert.equal(probe.formal_skill_bytes_loaded,true);assert.equal(probe.model_namespace_retired,true);
    assert.deepEqual(probe.replies.map(v=>v.method),['initialize','heartbeat','tools_list','request_finish','request_abort']);
    assert.ok(probe.replies.every(v=>v.accepted));
    const after=counters();assert.equal(before.native-after.native,20,'actual Native160 ledger must match 20 observed Native HTTP');for(const purpose of ['execution','reconcile_read'])assert.equal(after.purposes[purpose].transport,before.purposes[purpose].transport,'closed Control must not borrow purpose transport');assert.equal(after.model_controls,5);assert.equal(after.closing_reason,'finished');assert.equal(Object.values(nativeCounts).reduce((a,b)=>a+b,0)-nativeBefore,20,'actual joint claim4 plus closed five Control16 Native HTTP');assert.equal(after.execution_book_absent,true);assert.equal(after.old_v1_sessions,0);
    return {actual_linux_h_m_kernel_cli:true,native_placement:'actual-remote-https',n_local_negative_witness:true,
      model_lifecycle:'owned_fixture_pid_namespace_retired_only',runtime_skill_loading:'actual_bytes_in_deterministic_m_process_not_real_model',world_closed:'not_claimed',actual_host_control_methods:5};
  },async close(){if(exited)return;child.stdin.end();await new Promise(resolve=>{const timer=setTimeout(()=>{child.kill('SIGTERM');resolve();},7000);child.once('exit',()=>{clearTimeout(timer);resolve();});});lines.close();}};
}

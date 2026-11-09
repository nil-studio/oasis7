// Trusted local fixture supervisor. Never mounted as HTTP or an M tool.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {createInterface} from 'node:readline';
import {isAbsolute,resolve} from 'node:path';
import {existsSync} from 'node:fs';
export function hostControlDriver({directory,fixtureSource,port}){
  const image=process.env.OASIS7_HOST_COMPONENT_IMAGE,artifact=process.env.OASIS7_HOST_COMPONENT_CLI,skillBundle=process.env.OASIS7_HOST_COMPONENT_SKILL_BUNDLE;
  assert.match(image??'',/^sha256:[0-9a-f]{64}$/,'trusted immutable local Linux image required');
  assert.ok(isAbsolute(artifact??'')&&existsSync(artifact),'actual canonical Linux Host CLI required');
  assert.ok(isAbsolute(skillBundle??'')&&existsSync(resolve(skillBundle,'manifest.json')),'actual same-source formal Skill bundle required');
  for(const name of ['host_control_supervisor.py','host_control_relay.py'])assert.ok(existsSync(resolve(fixtureSource,name)),'fixed Host fixture script required');
  assert.ok(Number.isInteger(port)&&port>=1024&&port<=65535);
  const child=spawn('rtk',['proxy','docker','run','--interactive','--rm','--pull=never','--read-only',
    '--cap-drop=ALL',...['CHOWN','SETUID','SETGID','SETPCAP','KILL','NET_BIND_SERVICE'].map(v=>'--cap-add='+v),
    '--security-opt=no-new-privileges','--tmpfs','/tmp:rw,nosuid,nodev,mode=1777',
    '--tmpfs','/var/lib/oasis7-custody:rw,nosuid,nodev,mode=0755',
    '--mount',`type=bind,src=${artifact},dst=/artifacts/host-cli,readonly`,
    '--mount',`type=bind,src=${resolve(fixtureSource,'host_control_supervisor.py')},dst=/fixtures/host_control_supervisor.py,readonly`,
    '--mount',`type=bind,src=${resolve(fixtureSource,'host_control_relay.py')},dst=/fixtures/host_control_relay.py,readonly`,
    '--mount',`type=bind,src=${skillBundle},dst=/formal-skill,readonly`,
    '--mount',`type=bind,src=${resolve(directory,'localhost.crt')},dst=/fixture-ca/localhost.crt,readonly`,
    '--env',`OASIS7_LOCAL_RP_PORT=${port}`,image,'python3','/fixtures/host_control_supervisor.py'],
    {stdio:['pipe','pipe','pipe']});
  const lines=createInterface({input:child.stdout});const errors=createInterface({input:child.stderr});
  let pending=null,exited=false,ready=false,lastStage=0,lastErrno=null,stderrBytes=0;
  let startResolve,startReject;
  const started=new Promise((resolve,reject)=>{startResolve=resolve;startReject=reject;});
  // Attach immediately so a failure before the first RPC never creates an unhandled rejection.
  started.catch(()=>{});
  const failure=()=>Error(`Host fixture stage=${lastStage} errno=${lastErrno??0} stderr_bytes=${stderrBytes}`);
  const timer=setTimeout(()=>{startReject(failure());},15000);
  errors.on('line',line=>{stderrBytes+=Buffer.byteLength(line)+1;if(line.length>4096)return;
    try{const value=JSON.parse(line);if(value.event==='startup_failed'&&Number.isInteger(value.stage)){lastStage=value.stage;lastErrno=Number.isInteger(value.errno)?value.errno:null;startReject(failure());}}catch{}
  });
  lines.on('line',line=>{if(line.length>65536){pending?.reject(failure());pending=null;return;}
    let response;try{response=JSON.parse(line);}catch{pending?.reject(failure());pending=null;return;}
    if(response.event==='startup_ready'&&Number.isInteger(response.stage)){lastStage=response.stage;ready=true;clearTimeout(timer);startResolve();return;}
    if(!pending)return;const callback=pending;pending=null;
    if(response.ok!==true){lastStage=Number.isInteger(response.stage)?response.stage:lastStage;lastErrno=Number.isInteger(response.errno)?response.errno:null;if(response.cli&&typeof response.cli==='object'){const c=response.cli;console.error(JSON.stringify({host_cli_failure:Object.fromEntries(['stage','elapsed_ms','exit_code','stdout_bytes','stderr_bytes'].filter(k=>Number.isInteger(c[k])).map(k=>[k,c[k]]))}));}callback.reject(failure());return;}
    callback.resolve(response.result);
  });
  child.once('error',()=>{exited=true;clearTimeout(timer);startReject(failure());pending?.reject(failure());pending=null;});
  child.once('exit',(code,signal)=>{exited=true;clearTimeout(timer);const error=Error(`Host fixture exited code=${Number.isInteger(code)?code:-1} signal=${signal==='SIGTERM'?15:signal==='SIGKILL'?9:0} stage=${lastStage} errno=${lastErrno??0} stderr_bytes=${stderrBytes}`);if(!ready)startReject(error);pending?.reject(error);pending=null;});
  const rpc=async(operation,input)=>{await started;assert.equal(pending,null,'serial fixture RPC');if(exited)throw failure();
    return new Promise((resolve,reject)=>{const timer=setTimeout(()=>{pending=null;reject(failure());},45000);
      pending={resolve:v=>{clearTimeout(timer);resolve(v);},reject:e=>{clearTimeout(timer);reject(e);}};
      child.stdin.write(JSON.stringify({operation,input})+'\n');});};
  return {rpc,async close(){if(exited)return;child.stdin.end();await new Promise(resolve=>{const timer=setTimeout(()=>{child.kill('SIGTERM');resolve();},7000);child.once('exit',()=>{clearTimeout(timer);resolve();});});lines.close();errors.close();}};
}

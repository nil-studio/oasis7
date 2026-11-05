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
  assert.ok(Number.isInteger(port)&&port>=1024&&port<=65535);
  const child=spawn('rtk',['proxy','docker','run','--rm','--pull=never','--read-only',
    '--cap-drop=ALL',...['CHOWN','SETUID','SETGID','SETPCAP','KILL','NET_BIND_SERVICE'].map(v=>'--cap-add='+v),
    '--security-opt=no-new-privileges','--tmpfs','/tmp:rw,nosuid,nodev,mode=1777',
    '--tmpfs','/var/lib/oasis7-custody:rw,nosuid,nodev,mode=0755',
    '--mount',`type=bind,src=${artifact},dst=/artifacts/host-cli,readonly`,
    '--mount',`type=bind,src=${fixtureSource},dst=/fixtures,readonly`,
    '--mount',`type=bind,src=${skillBundle},dst=/formal-skill,readonly`,
    '--mount',`type=bind,src=${resolve(directory,'localhost.crt')},dst=/fixture-ca/localhost.crt,readonly`,
    '--env',`OASIS7_LOCAL_RP_PORT=${port}`,image,'python3','/fixtures/host_control_supervisor.py'],
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
  return {rpc,async close(){if(exited)return;child.stdin.end();await new Promise(resolve=>{const timer=setTimeout(()=>{child.kill('SIGTERM');resolve();},7000);child.once('exit',()=>{clearTimeout(timer);resolve();});});lines.close();}};
}

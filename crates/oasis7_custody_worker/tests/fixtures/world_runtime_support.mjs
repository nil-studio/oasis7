// A separate real Rust NodeRuntime executes World intents; Node never manufactures receipts.
import {spawn} from 'node:child_process';
import {once} from 'node:events';
import {existsSync,readFileSync,writeFileSync} from 'node:fs';
import {resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {setTimeout as delay} from 'node:timers/promises';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'../..');
export async function worldAuthority(directory) {
  const ready=resolve(directory,'world-ready.json'),stop=resolve(directory,'world-stop');
  const binary=readFileSync(resolve(root,'artifacts/world-test-binary.txt'),'utf8').trim();
  const child=spawn(binary,['serve_custody_world_http_fixture','--ignored','--nocapture','--test-threads=1'],{
    env:{...Object.fromEntries(Object.entries(process.env).filter(([key])=>!key.startsWith('OASIS7_'))),
      OASIS7_CUSTODY_WORLD_HTTP_READY:ready,OASIS7_CUSTODY_WORLD_HTTP_STOP:stop},
    stdio:['ignore','pipe','pipe']});
  let exited=false,output='';
  for(const stream of [child.stdout,child.stderr]) stream.on('data',chunk=>{output=(output+chunk.toString()).slice(-12000);});
  const exit=once(child,'exit').then(()=>{exited=true;});
  const close=async()=>{writeFileSync(stop,'stop');await Promise.race([exit,delay(5000,undefined,{ref:false})]);
    if(!exited){child.kill('SIGTERM');await exit;}};
  for(let i=0;i<3000;i++) {
    if(exited) throw new Error('real World fixture exited before readiness: '+output);
    if(existsSync(ready)) {try {return {data:JSON.parse(readFileSync(ready,'utf8')),close};}catch{/* ready write in progress */}}
    await delay(20);
  }
  await close();throw new Error('real World fixture readiness timed out: '+output);
}

// A separate real Rust NodeRuntime executes World intents; Node never manufactures receipts.
import {spawn} from 'node:child_process';
import {once} from 'node:events';
import {existsSync,readFileSync,writeFileSync,rmSync} from 'node:fs';
import {resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {setTimeout as delay} from 'node:timers/promises';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'../..');
export async function worldAuthority(directory,{liveContext=false,settleSubmits=false,settleBaseline=false,physicalPorts=false}={}) {
  const ready=resolve(directory,'world-ready.json'),stop=resolve(directory,'world-stop');
  const physicalFile=resolve(directory,'world-physical-ports.jsonl');
  const physicalPortCounts=()=>{
    if(!physicalPorts)throw new Error('physical receiver evidence was not enabled');
    const counts={};if(!existsSync(physicalFile))return counts;
    const rows=readFileSync(physicalFile,'utf8').trim().split('\n').filter(Boolean);
    if(rows.length>4096)throw new Error('physical receiver evidence exceeded bound');
    const ports=new Set(['submit','lookup','view','policy','owner_view','starter','goal_prepare','goal_current','other']);
    for(const row of rows){const value=JSON.parse(row);if(Object.keys(value).sort().join(',')!=='method,port'||!['get','post','other'].includes(value.method)||!ports.has(value.port))throw new Error('invalid fixed physical receiver evidence');const key=value.method+':'+value.port;counts[key]=(counts[key]??0)+1;}
    return counts;
  };
  const binary=readFileSync(resolve(root,'artifacts/world-test-binary.txt'),'utf8').trim();
  const baselineMarker=resolve(directory,"baseline-world-settlement");
  if(settleBaseline)writeFileSync(baselineMarker,"baseline-only",{flag:"wx",mode:0o600});
  const disableBaselineSettlement=()=>{if(settleBaseline)rmSync(baselineMarker,{force:true});};
  const child=spawn(binary,[liveContext?'serve_live_context_world_http_fixture':'serve_custody_world_http_fixture','--ignored','--nocapture','--test-threads=1'],{
    env:{...Object.fromEntries(Object.entries(process.env).filter(([key])=>!key.startsWith('OASIS7_'))),
      OASIS7_CUSTODY_WORLD_HTTP_READY:ready,OASIS7_CUSTODY_WORLD_HTTP_STOP:stop,
      ...(settleSubmits?{OASIS7_CUSTODY_WORLD_SETTLE_SUBMITS:'true'}:{}),
      ...(settleBaseline?{OASIS7_CUSTODY_WORLD_SETTLE_BASELINE_FILE:baselineMarker}:{}),
      ...(physicalPorts?{OASIS7_CUSTODY_WORLD_PHYSICAL_PORTS:physicalFile}:{}),
      ...(liveContext?{OASIS7_LIVE_CONTEXT_ADVANCE:resolve(directory,'advance'),OASIS7_LIVE_CONTEXT_NEXT:resolve(directory,'next.json'),OASIS7_LIVE_CONTEXT_STATS_REQUEST:resolve(directory,'stats-request'),OASIS7_LIVE_CONTEXT_STATS:resolve(directory,'stats.json')}: {})},
    stdio:['ignore','pipe','pipe']});
  let exited=false,output='';
  for(const stream of [child.stdout,child.stderr]) stream.on('data',chunk=>{output=(output+chunk.toString()).slice(-12000);});
  const exit=once(child,'exit').then(()=>{exited=true;});
  const close=async()=>{disableBaselineSettlement();writeFileSync(stop,'stop');await Promise.race([exit,delay(5000,undefined,{ref:false})]);
    if(!exited){child.kill('SIGTERM');await exit;}};
  for(let i=0;i<3000;i++) {
    if(exited) throw new Error('real World fixture exited before readiness: '+output);
    if(existsSync(ready)) {try {return {data:JSON.parse(readFileSync(ready,'utf8')),close,disableBaselineSettlement,physicalPortCounts};}catch{/* ready write in progress */}}
    await delay(20);
  }
  await close();throw new Error('real World fixture readiness timed out: '+output);
}

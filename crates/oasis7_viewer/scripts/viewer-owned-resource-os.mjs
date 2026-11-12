import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {verifyOwnedResourceSnapshot} from './viewer-owned-resource-policy.mjs';
import {readOwnedStartupProfile} from './viewer-owned-startup-context.mjs';
import {withObservationCleanup} from './world-view-frame-capability.mjs';
const execute=promisify(execFile);
const ps=async args=>(await execute('/bin/ps',args,{timeout:3000,maxBuffer:1024*1024,encoding:'utf8'})).stdout;
async function bounded(operation){let timer;try{return await Promise.race([operation,new Promise((_,no)=>{timer=setTimeout(()=>no(Error('owned resource CDP command exceeded 3s')),3000);})]);}finally{clearTimeout(timer);}}
// Inventory contains identity/parent metadata only, never another process's
// command line or memory. Memory is read only after the owned subtree is proven.
export async function readProcessIdentities(){
 const text=await ps(['-axo','pid=,ppid=,lstart=']);
 return text.trim().split('\n').map(line=>{
  const match=line.match(/^\s*(\d+)\s+(\d+)\s+(.+?)\s*$/);
  if(!match)throw Error('process identity inventory malformed');
  return {pid:Number(match[1]),ppid:Number(match[2]),birth:match[3]};
 });
}
export async function readOwnedProcessRss(context,expectedUrl){
 if(process.platform!=='darwin')throw Error('RSS capability probe is registered for Darwin only');
 const profile=readOwnedStartupProfile(context);
 if(context.pages().length!==1||context.pages()[0].url()!==expectedUrl)throw Error('owned single page changed');
 const cdp=await bounded(context.browser().newBrowserCDPSession());
 const send=method=>{
  if(readOwnedStartupProfile(context)!==profile||context.pages().length!==1||context.pages()[0].url()!==expectedUrl)throw Error('owned page proof invalidated');
  return bounded(cdp.send(method));
 };
 const proof=await withObservationCleanup(async()=>{
  const commandLine=(await send('Browser.getBrowserCommandLine')).arguments;
  const cdpProcesses=(await send('SystemInfo.getProcessInfo')).processInfo;
  const roots=cdpProcesses.filter(row=>row.type==='browser');
  if(roots.length!==1)throw Error('owned native browser PID ambiguous');
  return {profile,browserPid:roots[0].id,commandLine,cdpProcesses,expectedUrl,pageUrls:context.pages().map(page=>page.url())};
 },()=>bounded(cdp.detach()),{});
 const before=await readProcessIdentities(), owned=new Set([proof.browserPid]);
 verifyOwnedResourceSnapshot({...proof,before:before.map(row=>({...row,rssKiB:0})),after:before.map(row=>({...row,rssKiB:0}))});
 if(!before.some(row=>row.pid===proof.browserPid))throw Error('owned browser root unavailable');
 let changed=true;
 while(changed){changed=false;for(const row of before)if(!owned.has(row.pid)&&owned.has(row.ppid)){owned.add(row.pid);changed=true;}}
 const rss=new Map();
 for(const pid of owned){
  const text=(await ps(['-p',String(pid),'-o','rss='])).trim();
  if(!/^\d+$/.test(text))throw Error('owned process RSS unavailable');
  rss.set(pid,Number(text));
 }
 const after=(await readProcessIdentities()).map(row=>({...row,rssKiB:rss.get(row.pid)}));
 if(readOwnedStartupProfile(context)!==profile)throw Error('owned context lease changed');
 proof.pageUrls=context.pages().map(page=>page.url());
 return verifyOwnedResourceSnapshot({...proof,before,after});
}

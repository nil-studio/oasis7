import {readOwnedStartupProfile} from './viewer-owned-startup-context.mjs';
import {readOwnedProcessRss} from './viewer-owned-resource-os.mjs';
import {readOwnedMemory} from './viewer-owned-cdp-memory.mjs';
import {withObservationCleanup} from './world-view-frame-capability.mjs';
import {performance} from 'node:perf_hooks';
import {densityResourceStampScript,validateDensityResourceStamp,matchDensityResourceStamps} from './viewer-density-resource-stamp.mjs';
async function bounded(operation){let timer;try{return await Promise.race([operation,new Promise((_,no)=>{timer=setTimeout(()=>no(Error('Viewer resource read exceeded 10s')),10000);})]);}finally{clearTimeout(timer);}}
// Caller supplies only its factory-created context, not a PID or CDP endpoint.
// All partial results survive in report even if a phase or cleanup guard fails.
export async function observeOwnedViewerResources({context,url,world,cut,report}) {
 const profile=readOwnedStartupProfile(context),page=context.pages()[0],expected={url,world,cut};
 const owned=()=>{if(readOwnedStartupProfile(context)!==profile||context.pages().length!==1||context.pages()[0]!==page||page.url()!==url)throw Error('Viewer resource owned page changed');};
 owned();
 const read=()=>{owned();return bounded(page.evaluate('('+densityResourceStampScript(cut)+')()'));};
 report.before=await read();validateDensityResourceStamp(report.before,expected);
 const session=await bounded(context.newCDPSession(page));
 await withObservationCleanup(async()=>{
  const memoryMethods=new Set(['Runtime.getHeapUsage','Memory.getDOMCounters']);
  const heapStarted=performance.now();
  const memory=await readOwnedMemory({page:method=>{owned();if(!memoryMethods.has(method))throw Error('resource observation method outside readonly contract');return bounded(session.send(method));}});
  report.jsAndDomReadMs=performance.now()-heapStarted;
  report.jsAndDom=Object.fromEntries(['jsUsedHeapBytes','jsTotalHeapBytes','domNodes','documents','listeners'].map(key=>[key,memory[key]]));
  report.jsAndDom.capabilityReasons={heap:memory.capabilityReasons.heap??null,dom:memory.capabilityReasons.dom??null};
  const rssStarted=performance.now();
  report.rss=await readOwnedProcessRss(context,url);report.rssReadMs=performance.now()-rssStarted;
  report.after=await read();
  report.phase=matchDensityResourceStamps(report.before,report.after,expected);
  report.gpuAllocatedBytes=null;
  report.applicationCacheBytes=null;
  report.semantics='short owned-browser process RSS, page JS heap and mounted WASM capacity components; overlapping components must not be summed; not used WASM heap, total GPU memory or P6';
 },()=>bounded(session.detach()),report);
 return report;
}

import {compareRetentionComponent} from './viewer-retention-components.mjs';

// Local identity only. This object never accepts a caller-supplied PID or URL.
export function createRetentionOwnedAnchor(context, url) {
 const pages=context.pages(),page=pages[0];
 if(pages.length!==1||page.url()!==url)throw Error('retention requires one owned page');
 let root=null,lease=null;
 return (rss,wasm)=>{
  if(context.pages().length!==1||context.pages()[0]!==page||page.url()!==url)throw Error('retention owned page changed');
  const row=rss?.components?.find(x=>x.pid===rss.browserPid);
  if(!row||typeof row.birth!=='string'||!row.birth)throw Error('retention RSS root proof missing');
  const current={pid:row.pid,birth:row.birth};
  if(root&&(root.pid!==current.pid||root.birth!==current.birth))throw Error('retention native browser changed');
  if(wasm?.status!=='observed'||!Number.isSafeInteger(wasm.instanceSequence)||wasm.instanceSequence<=0||!Number.isSafeInteger(wasm.linearCapacityBytes)||wasm.linearCapacityBytes<0)throw Error('retention mounted WASM capacity unavailable');
  if(lease!==null&&lease!==wasm.instanceSequence)throw Error('retention mounted WASM lease changed');
  root??=current;lease??=wasm.instanceSequence;
  return {rootPid:root.pid,rootBirth:root.birth,wasmLease:lease};
 };
}

const identity=row=>JSON.stringify([row.pid,row.ppid,row.birth,row.cdpType]);
// RSS components overlap JS/WASM and may include shared pages. Never add them
// to JS or linear capacity, or relabel process RSS as attributable cache heap.
export function summarizeOwnedRetention(first,last,{eligible}) {
 const unknown=reason=>({status:'unknown',reason});
 if(!eligible||first.length!==6||last.length!==6)return {rss:unknown('short_or_overlapping_method_probe'),processes:[],totalApplicationCacheBytes:null,gpuAllocatedBytes:null};
 if([...first,...last].some(row=>row.memoryPhaseMatched!==true||!row.rss?.components))return {rss:unknown('unmatched_or_missing_endpoint'),processes:[],totalApplicationCacheBytes:null,gpuAllocatedBytes:null};
 const groups=[...first,...last].map(row=>new Map(row.rss.components.map(p=>[identity(p),p])));
 const keys=[...groups[0].keys()].sort();
 const same=groups.every(g=>JSON.stringify([...g.keys()].sort())===JSON.stringify(keys));
 const median=values=>values.sort((a,b)=>a-b)[Math.floor(values.length/2)];
 const processes=keys.filter(key=>groups.every(g=>g.has(key))).map(key=>({identity:JSON.parse(key),comparison:compareRetentionComponent(median(groups.slice(0,6).map(g=>g.get(key).rssBytes)),median(groups.slice(6).map(g=>g.get(key).rssBytes)))}));
 return {rss:same?compareRetentionComponent(median(first.map(r=>r.rss.rawRssSumBytes)),median(last.map(r=>r.rss.rawRssSumBytes))):unknown('endpoint_process_set_changed'),processes,totalApplicationCacheBytes:null,gpuAllocatedBytes:null,semantics:'owned process RSS only; overlapping components, not cache allocation or used WASM heap'};
}

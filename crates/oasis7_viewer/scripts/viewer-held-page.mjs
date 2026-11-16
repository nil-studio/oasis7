import {performance} from 'node:perf_hooks';
export async function waitHeldFinalPage({held,expected,currentGeneration,now=()=>performance.now(),sleep=ms=>new Promise(r=>setTimeout(r,ms)),timeoutMs=10000}) {
 if(!Number.isFinite(timeoutMs)||timeoutMs<=0||timeoutMs>10000)throw Error('held page wait must be bounded');
 for(const key of ['connection','generation','world','scope','cut'])if(typeof expected[key]!=='string'||!expected[key])throw Error('missing held page binding: '+key);
 const end=now()+timeoutMs;
 while(now()<end){
  if(currentGeneration(expected.connection)!==expected.generation)throw Error('held page query generation changed');
  const page=held().find(p=>{const v=p.response?.view,q=p.request?.query;return p.connection===expected.connection&&q?.cursor&&String(q.request_generation).split(':')[0]===expected.generation&&String(v?.request_generation).split(':')[0]===expected.generation&&v.world_id===expected.world&&v.scope_id===expected.scope&&v.source_cut===expected.cut&&v.delivery==='baseline'&&v.completeness==='complete'&&!v.next_cursor&&v.entities?.length===200;});
  if(page)return page;
  await sleep(Math.min(25,end-now()));
 }
 throw Error('matching final200 page was not held before deadline');
}

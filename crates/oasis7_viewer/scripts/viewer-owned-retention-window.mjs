// Fixed owned-context consumer of the existing 5617 workload. Not P5/full cache.
import {createRequire} from 'node:module';
import {readFileSync,writeFileSync,appendFileSync,mkdirSync} from 'node:fs';
import {resolve,dirname,join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {performance} from 'node:perf_hooks';
import {startOwnedViewerFixtureServer} from './viewer-owned-fixture-server.mjs';
import {createStartupProfile,launchOwnedStartupContext,readOwnedStartupProfile} from './viewer-owned-startup-context.mjs';
import {identifyOwnedStartupContext,closeOwnedStartupContext,boundedStartupOperation as bounded} from './viewer-owned-startup-lifecycle.mjs';
import {captureOwnedViewerAtlas,readOwnedViewerGpuAfterAtlas,assertViewerScene} from './viewer-product-startup-endpoints.mjs';
import {readOwnedProcessRss} from './viewer-owned-resource-os.mjs';
import {readOwnedMemory} from './viewer-owned-cdp-memory.mjs';
import {measureWorldViewRetention} from './world-view-retention-memory.mjs';
import {createRetentionOwnedInput} from './viewer-retention-owned-input.mjs';
import {prepareOwnedRetentionCamera} from './viewer-retention-owned-prepare.mjs';
import {createRetentionOwnedAnchor,summarizeOwnedRetention} from './viewer-retention-owned-components.mjs';
import {withObservationCleanup} from './world-view-frame-capability.mjs';
const here=dirname(fileURLToPath(import.meta.url)),repo=resolve(here,'../../..'),sha=b=>createHash('sha256').update(b).digest('hex');
for(const key of ['RETENTION_OWNED_ARTIFACT_ROOT','RETENTION_OWNED_SOURCE_HEAD','RETENTION_OWNED_MANIFEST','RETENTION_OWNED_OUTPUT','RETENTION_OWNED_MS'])if(!process.env[key])throw Error('declare '+key);
const durationMs=Number(process.env.RETENTION_OWNED_MS);if(!Number.isFinite(durationMs)||durationMs<=0||durationMs>1800000)throw Error('owned retention duration outside registered bound');
const source=execFileSync('git',['rev-parse',process.env.RETENTION_OWNED_SOURCE_HEAD+'^{commit}'],{cwd:repo,encoding:'utf8'}).trim(),manifest=JSON.parse(readFileSync(process.env.RETENTION_OWNED_MANIFEST));if(manifest.source!==source)throw Error('owned retention source mismatch');
const assets=Array.isArray(manifest.files)?Object.fromEntries(manifest.files.map(r=>[r.path,{bytes:r.size,sha256:r.sha256}])):manifest.files;
if(Object.keys(assets??{}).length!==18||(Array.isArray(manifest.files)&&manifest.files.length!==18))throw Error('owned retention needs unique18 assets');
const out=resolve(process.env.RETENTION_OWNED_OUTPUT);mkdirSync(dirname(out),{recursive:true});mkdirSync(out);
const names=['viewer-owned-retention-window.mjs','viewer-retention-owned-prepare.mjs','viewer-retention-owned-components.mjs','viewer-retention-owned-input.mjs','world-view-retention-memory.mjs','viewer-retention-components.mjs','viewer-retention-focus.mjs','viewer-owned-fixture-server.mjs','world-view-density-fixture.mjs','viewer-bounded-wire-audit.mjs','viewer-static-files.mjs','viewer-owned-startup-context.mjs','viewer-owned-startup-lifecycle.mjs','viewer-owned-resource-os.mjs','viewer-owned-resource-policy.mjs','viewer-owned-cdp-memory.mjs','viewer-owned-cdp-input.mjs','viewer-owned-cdp-session.mjs','viewer-product-startup-endpoints.mjs','viewer-startup-atlas-oracle.py','world-view-frame-capability.mjs'];
const summary={source,runnerHead:execFileSync('git',['rev-parse','HEAD'],{cwd:repo,encoding:'utf8'}).trim(),scripts:Object.fromEntries(names.map(n=>[n,sha(readFileSync(join(here,n)))])),assets,durationMs,formalP6:false,totalApplicationCacheBytes:null,gpuAllocatedBytes:null,status:'running',first:[],last:[],limitations:['WASM is mounted linear capacity, not allocator used heap.','Process RSS overlaps JS/WASM/shared pages; components must not be summed.','Current/retired inflight statistics are scalar coverage, not retained heap attribution.','No forced GC, cache clear, reload or sample retries; polling crossings remain unknown.']};
const modules=process.env.OASIS7_PLAYWRIGHT_NODE_MODULES||'/Users/scc/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules', {chromium}=createRequire(join(modules,'owned-retention.cjs'))('playwright');
let server,context,memorySession;
const timer=setTimeout(()=>{summary.status='failed';summary.error='owned retention hard deadline exceeded; cleanup unproven';try{writeFileSync(join(out,'summary.json'),JSON.stringify(summary,null,2));}finally{process.exit(1);}},durationMs+180000);
await withObservationCleanup(async()=>{
 try{
 server=await startOwnedViewerFixtureServer({artifactRoot:process.env.RETENTION_OWNED_ARTIFACT_ROOT,source,out,expectedAssets:assets,retentionStreaming:true});
 context=await launchOwnedStartupContext(chromium,process.env.OASIS7_PLAYWRIGHT_CHROME_PATH||'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',createStartupProfile());
 summary.ownedIdentity=await identifyOwnedStartupContext(context,summary);summary.browserVersion=context.browser().version();
 const page=context.pages()[0];await page.goto(server.url,{waitUntil:'domcontentloaded',timeout:30000});
 const expected=server.fixture.description(),input=createRetentionOwnedInput(context,server.url);
 summary.preparation={};summary.initialScene=await prepareOwnedRetentionCamera({page,expected,input,report:summary.preparation});
 const atlasPath=join(out,'source-agent.png');writeFileSync(atlasPath,execFileSync('git',['show',source+':doc/world-simulator/viewer/assets/ui-ux/pixel-world/agent.png'],{cwd:repo}));
 summary.atlas={};await captureOwnedViewerAtlas({page,expected,out,atlasPath,report:summary.atlas,prefix:'initial'});summary.gpu=await readOwnedViewerGpuAfterAtlas(page);assertViewerScene({...summary.atlas.matched.after,gpu:summary.gpu},{...expected,requireGpu:true,requireLabels:true});
 const profile=readOwnedStartupProfile(context),anchor=createRetentionOwnedAnchor(context,server.url);
 await input.evaluate(`(async()=>{const m=await import('/pixel-world-bridge/pixel_world_bridge.js');window.__QA_READ_METRICS__=m.readPixelWorldDisplayMetrics;window.__QA_READ_WASM_CAPACITY__=m.readPixelWorldLinearMemoryCapacity;return true})()`);
 memorySession=await context.newCDPSession(page);
 const owned=()=>{if(readOwnedStartupProfile(context)!==profile||context.pages().length!==1||context.pages()[0]!==page||page.url()!==server.url)throw Error('owned retention observation identity changed');};
 let observationIndex=0;
 const readMemory=async()=>{
  const report={index:observationIndex++,startModuleMs:performance.now()},start=performance.now();
  return withObservationCleanup(async()=>{
   try{
   owned();const memory=await readOwnedMemory({page:method=>{owned();if(!['Runtime.getHeapUsage','Memory.getDOMCounters'].includes(method))throw Error('retention CDP method rejected');return bounded(memorySession.send(method),'retention readonly memory');}});
   report.jsAndDom=Object.fromEntries(['jsUsedHeapBytes','jsTotalHeapBytes','domNodes','documents','listeners'].map(key=>[key,memory[key]]));
   const rssStarted=performance.now();report.rss=memory.rss=await readOwnedProcessRss(context,server.url);report.rssReadMs=memory.rssReadMs=performance.now()-rssStarted;
   report.wasm=await input.evaluate('window.__QA_READ_WASM_CAPACITY__()');report.anchor=memory.ownedAnchor=anchor(memory.rss,report.wasm);memory.ownedObservationMs=performance.now()-start;
   memory.ownedObservationIndex=report.index;memory.ownedRssBytes=null;return memory;
   }catch(error){report.error=String(error);throw error;}
  },()=>{report.elapsedMs=performance.now()-start;appendFileSync(join(out,'retention-owned-observations.jsonl'),JSON.stringify(report)+'\n');},report);
 };
 summary.retention=await measureWorldViewRetention({...input,fixture:server.fixture,out,durationMs,overview:()=>input.run(['click','[data-query-overview]']),beginStreamingWire:()=>server.assertHealthy(),readWireSummary:expected=>server.readWireSummary(expected),readMemory,
  onMemoryRecord(record,memory,{inflight}){const row={timeMs:record.timeMs,phase:record.phase,memoryPhaseMatched:record.memoryPhaseMatched,rss:memory.rss,rssReadMs:memory.rssReadMs,ownedObservationMs:memory.ownedObservationMs,observationIndex:memory.ownedObservationIndex,anchor:memory.ownedAnchor,inflight};appendFileSync(join(out,'retention-owned-rss.jsonl'),JSON.stringify(row)+'\n');if(row.phase==='home'){if(summary.first.length<6)summary.first.push(row);summary.last.push(row);if(summary.last.length>6)summary.last.shift();}},
 });
 summary.components=summarizeOwnedRetention(summary.first,summary.last,{eligible:summary.retention.longStabilityEligible});
 summary.status='observed_components_only';
 if(summary.components.rss.status==='over_budget'||summary.components.processes.some(r=>r.comparison.status==='over_budget'))throw Error('known owned RSS component growth exceeds10%');
 }catch(error){summary.status='failed';summary.error=String(error);throw error;}
},[
 async()=>{if(memorySession)await bounded(memorySession.detach(),'detach retention memory');},
 async()=>{if(context)summary.close=await closeOwnedStartupContext(context);},
 ()=>{if(server){summary.server=server.statistics();server.verifyAssets();}},
 async()=>{if(server)await bounded(server.close(),'close retention server');},
 ()=>{try{if(summary.cleanupErrors?.length)summary.status='failed_cleanup';writeFileSync(join(out,'summary.json'),JSON.stringify(summary,null,2));}finally{clearTimeout(timer);}},
],summary);

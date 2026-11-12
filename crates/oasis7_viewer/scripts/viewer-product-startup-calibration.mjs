// Exactly three product calibration cases, not20/30 or formal A/B/P6.
import {createRequire} from 'node:module';
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {resolve,dirname,join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import os from 'node:os';
import {startOwnedViewerFixtureServer} from './viewer-owned-fixture-server.mjs';
import {createStartupProfile,launchOwnedStartupContext} from './viewer-owned-startup-context.mjs';
import {identifyOwnedStartupContext,closeOwnedStartupContext,boundedStartupOperation as bounded} from './viewer-owned-startup-lifecycle.mjs';
import {waitOwnedViewerScene,captureOwnedViewerAtlas,readOwnedViewerGpuAfterAtlas,assertViewerScene,nativeSelectOwnedViewer} from './viewer-product-startup-endpoints.mjs';
import {withObservationCleanup} from './world-view-frame-capability.mjs';
const here=dirname(fileURLToPath(import.meta.url)),repo=resolve(here,'../../..'),sha=b=>createHash('sha256').update(b).digest('hex');
for(const key of ['VIEWER_STARTUP_ARTIFACT_ROOT','VIEWER_STARTUP_SOURCE_HEAD','VIEWER_STARTUP_MANIFEST','VIEWER_STARTUP_OUTPUT'])if(!process.env[key])throw Error('declare '+key);
const source=execFileSync('git',['rev-parse',process.env.VIEWER_STARTUP_SOURCE_HEAD+'^{commit}'],{cwd:repo,encoding:'utf8'}).trim();
const input=JSON.parse(readFileSync(process.env.VIEWER_STARTUP_MANIFEST));if(input.source!==source)throw Error('artifact source mismatch');
const assets=Array.isArray(input.files)?Object.fromEntries(input.files.map(r=>[r.path,{bytes:r.size,sha256:r.sha256}])):input.files;
if(!assets||Object.keys(assets).length!==18||(Array.isArray(input.files)&&input.files.length!==18))throw Error('expected complete unique18 artifact manifest');
const out=resolve(process.env.VIEWER_STARTUP_OUTPUT);mkdirSync(dirname(out),{recursive:true});mkdirSync(out);
const atlas=execFileSync('git',['show',source+':doc/world-simulator/viewer/assets/ui-ux/pixel-world/agent.png'],{cwd:repo});const atlasPath=join(out,'source-agent.png');writeFileSync(atlasPath,atlas);
const scripts=Object.fromEntries(['viewer-product-startup-calibration.mjs','viewer-owned-fixture-server.mjs','viewer-owned-startup-context.mjs','viewer-owned-startup-lifecycle.mjs','viewer-product-startup-endpoints.mjs','viewer-startup-atlas-oracle.py','world-view-density-fixture.mjs','viewer-static-files.mjs','viewer-bounded-wire-audit.mjs','viewer-owned-cdp-input.mjs','viewer-owned-cdp-session.mjs','world-view-frame-capability.mjs'].map(n=>[n,sha(readFileSync(join(here,n)))]));
const summary={source,runnerHead:execFileSync('git',['rev-parse','HEAD'],{cwd:repo,encoding:'utf8'}).trim(),assets,scripts,atlasSha256:sha(atlas),cases:[],hardware:{platform:process.platform,arch:process.arch,os:os.release(),cpu:os.cpus()[0]?.model,totalMemory:os.totalmem(),refreshRate:'unknown'},controlledTransport:true,realServiceAuthority:false,
 limitations:['M3/local loopback observer fixture, not authenticated player/A-B/50k/P6.','Delivery remains actual visual distribution no-store; warm does not promise HTTP or compiled-WASM cache reuse.','Atlas image is a captured observable upper endpoint; screenshot passivity/physical presentation unproven.','No20cold/30warm statistics; observer/RPC/PNG/Python costs included, never subtracted.'],status:'running'};
const timer=setTimeout(()=>{summary.status='failed';summary.error='180s hard stop; cleanup unproven';try{writeFileSync(join(out,'summary.json'),JSON.stringify(summary,null,2));}finally{process.exit(1);}},180000);
const modules=process.env.OASIS7_PLAYWRIGHT_NODE_MODULES||'/Users/scc/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules';
const {chromium}=createRequire(join(modules,'startup-product.cjs'))('playwright');
let server;
await withObservationCleanup(async()=>{
 try{
  server=await startOwnedViewerFixtureServer({artifactRoot:process.env.VIEWER_STARTUP_ARTIFACT_ROOT,source,out,expectedAssets:assets});
  const profileA=createStartupProfile(),profileB=createStartupProfile();summary.profiles=[profileA.path,profileB.path];summary.url=server.url;
  for(const [name,profile] of [['cold-first',profileA],['warm-same-profile',profileA],['cold-independent',profileB]]){
    const row={name,profile:profile.path,atlas:{},native:{},network:[],status:'running'};summary.cases.push(row);server.beginCase(name);
    const context=await launchOwnedStartupContext(chromium,process.env.OASIS7_PLAYWRIGHT_CHROME_PATH||'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',profile);
    await withObservationCleanup(async()=>{
      try{
      row.ownedIdentity=await identifyOwnedStartupContext(context,row);
      row.browserVersion=context.browser().version();
      if(context.pages().length!==1||context.pages()[0].url()!=='about:blank')throw Error('fresh process did not start in one blank page');
      const page=context.pages()[0],cdp=await context.newCDPSession(page);await cdp.send('Network.enable');
      cdp.on('Network.responseReceived',e=>{if(e.response.url.endsWith('.wasm'))row.network.push({url:e.response.url,status:e.response.status,fromDiskCache:!!e.response.fromDiskCache,fromServiceWorker:!!e.response.fromServiceWorker});});
      row.nodeNavigationStart=performance.now();await page.goto(server.url,{waitUntil:'domcontentloaded',timeout:30000});
      row.navigation=await page.evaluate(()=>({now:performance.now(),origin:performance.timeOrigin,navigation:performance.getEntriesByType('navigation')[0]?.toJSON()}));
      await page.waitForFunction(()=>document.querySelector('.pixel-world-canvas')?.dataset.rendererReady==='true',null,{timeout:30000});
      row.bridgeReadyObserved=await page.evaluate(()=>({now:performance.now(),origin:performance.timeOrigin}));
      const expected=server.fixture.description();row.completeScene=await waitOwnedViewerScene(page,expected);
      await captureOwnedViewerAtlas({page,expected,out,atlasPath,report:row.atlas,prefix:name});
      row.gpu=await readOwnedViewerGpuAfterAtlas(page);assertViewerScene({...row.atlas.matched.after,gpu:row.gpu},{...expected,requireGpu:true});
      await nativeSelectOwnedViewer(context,page,expected,row.native);
      row.resources=await page.evaluate(()=>({now:performance.now(),origin:performance.timeOrigin,wasm:performance.getEntriesByType('resource').filter(r=>r.name.endsWith('.wasm')).map(r=>({url:r.name,duration:r.duration,transferSize:r.transferSize,encodedBodySize:r.encodedBodySize,decodedBodySize:r.decodedBodySize}))}));
      if(row.navigation.origin!==row.resources.origin||row.native.after.origin!==row.navigation.origin)throw Error('startup page clock/origin changed');
      row.observedEndpoints={bridgeReadyUpperMs:row.bridgeReadyObserved.now,completeSceneUpperMs:row.completeScene.now,atlasImageUpperMs:row.atlas.observedPageUpperMs,nativeSelectedDomUpperMs:row.native.after.now,physicalFirstFrame:null,coreGoalUi:null};
      server.assertHealthy();server.verifyAssets();row.status='passed_calibration_only';
      }catch(error){row.status='failed';row.error=String(error);throw error;}
    },async()=>{row.close=await closeOwnedStartupContext(context);},row);
  }
  summary.status='passed_product_calibration_only';
 }catch(error){summary.status='failed';summary.error=String(error);throw error;}
},[
 ()=>{if(server)summary.server=server.statistics();},
 ()=>{if(server)server.verifyAssets();},
 async()=>{if(server)await bounded(server.close(),'close owned Viewer server');},
 ()=>{try{for(const row of summary.cases)if(row.cleanupErrors?.length)row.status='failed_cleanup';if(summary.cleanupErrors?.length){summary.status='failed';summary.error??='owned cleanup failed';}writeFileSync(join(out,'summary.json'),JSON.stringify(summary,null,2));}finally{clearTimeout(timer);}},
],summary);
console.log(JSON.stringify({status:summary.status,cases:summary.cases.map(r=>({name:r.name,status:r.status,endpoints:r.observedEndpoints}))}));

// One bounded real-Viewer resource sample, not a 30-minute or P6 acceptance run.
import {createRequire} from 'node:module';
import {mkdirSync,readFileSync,writeFileSync} from 'node:fs';
import {resolve,dirname,join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {performance} from 'node:perf_hooks';
import {createStartupProfile,launchOwnedStartupContext} from './viewer-owned-startup-context.mjs';
import {identifyOwnedStartupContext,closeOwnedStartupContext} from './viewer-owned-startup-lifecycle.mjs';
import {startOwnedViewerFixtureServer} from './viewer-owned-fixture-server.mjs';
import {prepareOwnedViewerDensity,captureOwnedViewerAtlas,readOwnedViewerGpuAfterAtlas} from './viewer-product-startup-endpoints.mjs';
import {observeOwnedViewerResources} from './viewer-density-resource-observation.mjs';
import {validateDensityResourceWire} from './viewer-density-resource-stamp.mjs';
import {withObservationCleanup} from './world-view-frame-capability.mjs';
const here=dirname(fileURLToPath(import.meta.url)),repo=resolve(here,'../../..');
const assert=(value,reason)=>{if(!value)throw Error(reason);},sha=bytes=>createHash('sha256').update(bytes).digest('hex');
assert(process.env.VIEWER_DENSITY_RSS_MANIFEST&&process.env.VIEWER_DENSITY_RSS_OUTPUT,'declare canonical immutable manifest and fresh output');
const manifest=JSON.parse(readFileSync(process.env.VIEWER_DENSITY_RSS_MANIFEST));
assert(/^[0-9a-f]{40}$/.test(manifest.source)&&Array.isArray(manifest.files)&&manifest.files.length===18,'declare actual built source and canonical18 manifest');
const assets={};for(const row of manifest.files){assert(typeof row.path==='string'&&!Object.hasOwn(assets,row.path)&&Number.isSafeInteger(row.size)&&row.size>=0&&/^[0-9a-f]{64}$/.test(row.sha256),'invalid or duplicate asset receipt');assets[row.path]={bytes:row.size,sha256:row.sha256};}
execFileSync('git',['cat-file','-e',manifest.source+'^{commit}'],{cwd:repo});
const root=resolve(process.env.VIEWER_DENSITY_RSS_ARTIFACT_ROOT||manifest.root),out=resolve(process.env.VIEWER_DENSITY_RSS_OUTPUT);mkdirSync(dirname(out),{recursive:true});mkdirSync(out);
const modules=process.env.OASIS7_PLAYWRIGHT_NODE_MODULES||'/Users/scc/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules';
const {chromium}=createRequire(join(modules,'density-resource.cjs'))('playwright');
const started=performance.now(),summary={builtSource:manifest.source,runnerHead:execFileSync('git',['rev-parse','HEAD'],{cwd:repo,encoding:'utf8'}).trim(),assets,scripts:{},resource:{},controlledTransport:true,realServiceAuthority:false,hardStopMs:120000,limits:['single short controlled observer/debug scene; not genuine hosted player/P5','RSS/JS/WASM capacity overlap and must not be summed','GPU total, complete cache, 30min/8GiB/A-B/P6 remain unaccepted']};
for(const name of ['viewer-density-owned-resource.mjs','viewer-density-resource-observation.mjs','viewer-density-resource-stamp.mjs','viewer-owned-resource-os.mjs','viewer-owned-resource-policy.mjs','viewer-owned-startup-context.mjs','viewer-owned-startup-lifecycle.mjs','viewer-owned-fixture-server.mjs','viewer-product-startup-endpoints.mjs','viewer-startup-atlas-oracle.py','world-view-density-fixture.mjs','viewer-owned-cdp-memory.mjs','world-view-frame-capability.mjs','viewer-bounded-wire-audit.mjs','viewer-static-files.mjs','viewer-owned-cdp-input.mjs'])summary.scripts[name]=sha(readFileSync(join(here,name)));
const timer=setTimeout(()=>{summary.error='120s whole-case deadline; cleanup unproven';summary.elapsedMs=performance.now()-started;try{writeFileSync(join(out,'summary.json'),JSON.stringify(summary,null,2));}catch(error){console.error(error);}process.exit(1);},120000);
let server,context;
await withObservationCleanup(async()=>{
 try{
  server=await startOwnedViewerFixtureServer({artifactRoot:root,source:manifest.source,out,expectedAssets:assets});server.beginCase('real-viewer-rss');
  const lease=createStartupProfile();summary.profile=lease.path;
  context=await launchOwnedStartupContext(chromium,process.env.OASIS7_PLAYWRIGHT_CHROME_PATH||'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',lease);
  summary.nativeIdentity=await identifyOwnedStartupContext(context,summary);
  assert(context.pages().length===1,'fresh owned Viewer page count invalid');const page=context.pages()[0];
  await page.goto(server.url,{waitUntil:'load',timeout:15000});
  const expected=server.fixture.description();summary.prepare={};summary.prepared=await prepareOwnedViewerDensity(context,page,expected,summary.prepare);server.assertHealthy();
  const asset='doc/world-simulator/viewer/assets/ui-ux/pixel-world/agent.png',png=execFileSync('git',['show',manifest.source+':'+asset],{cwd:repo}),atlasPath=join(out,'built-source-agent.png');writeFileSync(atlasPath,png);
  summary.atlas={sourcePngSha256:sha(png),observerCost:'normal screenshots/opaque-source oracle precede RSS; not deducted'};
  Object.assign(summary.atlas,await captureOwnedViewerAtlas({page,expected,out,atlasPath,prefix:'pre-rss',report:summary.atlas}));
  summary.gpuIdentity=await readOwnedViewerGpuAfterAtlas(page);assert(typeof summary.gpuIdentity==='string'&&!/swiftshader|llvmpipe|softpipe|software/i.test(summary.gpuIdentity),'native GPU identity unavailable');
  summary.wireBefore=server.statistics().latestView;
  await observeOwnedViewerResources({context,url:server.url,world:expected.world,cut:expected.cut,report:summary.resource});
  summary.wireAfter=server.statistics().latestView;server.assertHealthy();
  validateDensityResourceWire(summary.wireBefore,summary.resource.before,expected);validateDensityResourceWire(summary.wireAfter,summary.resource.after,expected);
  assert(summary.wireBefore.connection===summary.wireAfter.connection&&summary.wireBefore.generation===summary.wireAfter.generation,'resource observation crossed actual WS connection/query');
  summary.status='controlled_viewer_component_observation_only';
 }catch(error){summary.status='failed';summary.error=String(error);throw error;}
},[
 async()=>{if(context)summary.closed=await closeOwnedStartupContext(context);},
 ()=>{if(server){summary.server=server.statistics();server.assertHealthy();server.verifyAssets();}},
 async()=>{if(server)await server.close();},
 ()=>{summary.elapsedMs=performance.now()-started;if(summary.cleanupErrors?.length)summary.status='failed_cleanup';try{writeFileSync(join(out,'summary.json'),JSON.stringify(summary,null,2));}finally{clearTimeout(timer);}}
],summary);

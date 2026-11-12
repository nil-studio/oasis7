// Three short groups in one exclusive, sandboxed, disposable browser profile.
// Requested texture storage is a diagnostic input, never measured GPU memory.
import {createServer} from 'node:http';
import {createRequire} from 'node:module';
import {mkdirSync,writeFileSync,readFileSync} from 'node:fs';
import {resolve,dirname,join} from 'node:path';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {createStartupProfile,launchOwnedStartupContext} from './viewer-owned-startup-context.mjs';
import {readOwnedProcessRss} from './viewer-owned-resource-os.mjs';
import {withObservationCleanup} from './world-view-frame-capability.mjs';
const modules=process.env.OASIS7_PLAYWRIGHT_NODE_MODULES||'/Users/scc/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules';
const {chromium}=createRequire(join(modules,'resource-capability.cjs'))('playwright');
if(!process.env.VIEWER_RESOURCE_OUTPUT)throw Error('declare fresh VIEWER_RESOURCE_OUTPUT');
const out=resolve(process.env.VIEWER_RESOURCE_OUTPUT);mkdirSync(dirname(out),{recursive:true});mkdirSync(out);
const summary={source:execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim(),groups:[],limitations:['Owned process RSS sum may count shared pages repeatedly.','No application cache, GPU total allocation, physical presentation or P6 verdict.'],scripts:{}};
for(const file of ['viewer-owned-resource-capability.mjs','viewer-owned-resource-os.mjs','viewer-owned-resource-policy.mjs','viewer-owned-startup-context.mjs','world-view-frame-capability.mjs'])summary.scripts[file]=createHash('sha256').update(readFileSync(new URL(file,import.meta.url))).digest('hex');
const timer=setTimeout(()=>{summary.error='120s hard stop; owned cleanup unproven';try{writeFileSync(join(out,'summary.json'),JSON.stringify(summary,null,2));}catch(error){console.error(error);}process.exit(1);},120000);
let server,context;
const boundedClose=async()=>{let timer;try{await Promise.race([context.close(),new Promise((_,no)=>{timer=setTimeout(()=>no(Error('owned browser close exceeded 10s')),10000);})]);}finally{clearTimeout(timer);}};
await withObservationCleanup(async()=>{
 try {
 server=createServer((req,res)=>{res.writeHead(200,{'Content-Type':'text/html','Cache-Control':'no-store'});res.end('<!doctype html><title>Owned resource capability</title><canvas id="probe" width="64" height="64"></canvas>');});
 await new Promise((yes,no)=>{server.once('error',no);server.listen(0,'127.0.0.1',yes);});
 const url=`http://127.0.0.1:${server.address().port}/`;
 const profile=createStartupProfile();summary.profile=profile.path;
 context=await launchOwnedStartupContext(chromium,process.env.OASIS7_PLAYWRIGHT_CHROME_PATH||'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',profile);
 if(context.pages().length!==1)throw Error('fresh owned page count invalid');
 const page=context.pages()[0];await page.goto(url,{timeout:10000});
 summary.groups.push({name:'blank',rss:await readOwnedProcessRss(context,url)});
 const texture=await page.evaluate(()=>{
  const gl=document.querySelector('#probe').getContext('webgl2');if(!gl)throw Error('WebGL2 unavailable');
  const textures=[];for(let i=0;i<2;i++){const texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,texture);gl.texStorage2D(gl.TEXTURE_2D,1,gl.RGBA8,1024,1024);textures.push(texture);}
  window.__OWNED_RESOURCE_TEXTURES__={gl,textures};return {requestedStorageBytes:2*1024*1024*4,error:gl.getError(),semantics:'requested_RGBA8_storage_not_physical_GPU_allocation'};
 });
 if(texture.error!==0)throw Error('controlled texture allocation rejected');
 summary.groups.push({name:'controlled-textures',texture,rss:await readOwnedProcessRss(context,url),totalGpuAllocatedBytes:null});
 const extra=await context.newPage();
 let refused=false;try{await readOwnedProcessRss(context,url);}catch(error){refused=/owned single page changed/.test(String(error));summary.groups.push({name:'extra-owned-page-negative',refused,reason:String(error)});}
 await extra.close();if(!refused)throw Error('multiple-page resource read was not refused before RSS');
 await page.evaluate(()=>{const value=window.__OWNED_RESOURCE_TEXTURES__;for(const texture of value.textures)value.gl.deleteTexture(texture);delete window.__OWNED_RESOURCE_TEXTURES__;});
 summary.status='component_capability_only';
 }catch(error){summary.error=String(error);throw error;}
},[
 async()=>{
  if(!context)return;
  await boundedClose();
  const pid=summary.groups.find(row=>row.rss)?.rss.browserPid;
  if(!pid)throw Error('owned browser native exit identity unavailable');
  const end=performance.now()+2000;
  while(performance.now()<end){try{process.kill(pid,0);}catch(error){if(error.code==='ESRCH'){summary.processExited=true;return;}throw error;}await new Promise(yes=>setTimeout(yes,50));}
  throw Error('owned native browser exit unproven');
 },
 async()=>{if(server)await new Promise((yes,no)=>server.close(error=>error?no(error):yes()));},
 ()=>{try{writeFileSync(join(out,'summary.json'),JSON.stringify(summary,null,2));}finally{clearTimeout(timer);}}
],summary);

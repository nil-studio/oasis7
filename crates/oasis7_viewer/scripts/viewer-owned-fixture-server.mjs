// Shared controlled transport for product-startup and owned-RSS probes, not P5.
import {createServer} from 'node:http';
import {WebSocketServer} from 'ws';
import {readdirSync,appendFileSync,writeFileSync} from 'node:fs';
import {resolve,relative,extname,join} from 'node:path';
import {createHash,randomUUID} from 'node:crypto';
import {isDeepStrictEqual} from 'node:util';
import {createDensityFixture} from './world-view-density-fixture.mjs';
import {readRegularFile} from './viewer-static-files.mjs';
import {createBoundedWireAudit} from './viewer-bounded-wire-audit.mjs';
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
export function createOwnedViewerEvidence(handlers) {
  const writer=createBoundedWireAudit({limitBytes:64*1024*1024,...handlers});
  return {write(row){if(!writer.record(row))throw writer.failure();}};
}
export function fingerprintViewerAssets(root) {
  const walk=dir=>readdirSync(dir,{withFileTypes:true}).flatMap(e=>e.isDirectory()?walk(join(dir,e.name)):[join(dir,e.name)]);
  return Object.fromEntries(walk(root).sort().map(p=>{const b=readRegularFile(p);return [relative(root,p),{bytes:b.length,sha256:sha(b)}];}));
}
export async function startOwnedViewerFixtureServer({artifactRoot,source,out,expectedAssets}) {
  const root=resolve(artifactRoot),assets=fingerprintViewerAssets(root),fixture=createDensityFixture();
  if(expectedAssets&&!isDeepStrictEqual(expectedAssets,assets))throw Error('immutable Viewer asset manifest mismatch');
  let bytes=0,rows=0,inputBytes=0,closed=false,caseId=null,failed=false,fatalError,latestView=null,requestCount=0;
  const http=[],wire=[];
  const markFailure=error=>{if(!failed){failed=true;fatalError=error;}};
  const stopSockets=()=>{for(const socket of wss.clients)socket.terminate();};
  const evidence=createOwnedViewerEvidence({append:line=>appendFileSync(join(out,'server.jsonl'),line),persistFailure:row=>writeFileSync(join(out,'server-evidence-failure.json'),JSON.stringify(row)),stopSource:stopSockets});
  const audit=(kind,row)=>{
    if(failed)throw fatalError;
    const item={caseId,wall:Date.now(),...row},line=JSON.stringify({kind,...item})+'\n';
    bytes+=Buffer.byteLength(line);if(++rows>10000){markFailure(Error('owned Viewer server row limit exceeded'));stopSockets();throw fatalError;}
    try{evidence.write({kind,...item});}catch(error){markFailure(error);throw error;}(kind==='http'?http:wire).push(item);
  };
  const server=createServer((req,res)=>{
    let pathname;
    if(failed){res.writeHead(503);res.end();return;}
    try{pathname=decodeURIComponent(new URL(req.url,'http://localhost').pathname);}catch{res.writeHead(400);res.end();return;}
    if(req.method!=='GET'){res.writeHead(405);res.end();return;}
    const p=resolve(root,'.'+(pathname==='/'?'/viewer.html':pathname)),rel=relative(root,p);
    if(!rel||rel.startsWith('..')){res.writeHead(403);res.end();return;}
    try{
      let body=readRegularFile(p);
      if(extname(p)==='.html'){
        const html=body.toString(),pattern=/<script\b[^>]*id=["']oasis7-viewer-runtime-config["'][^>]*>[\s\S]*?<\/script>/gi;
        const count=(html.match(pattern)||[]).length;if(count>1)throw Error('duplicate trusted runtime config');
        const config={deploymentMode:'trusted_local_only',viewerWsEndpoint:`ws://density-qa.localhost:${server.address().port}/qa-world-ws`};
        const tag=`<script id="oasis7-viewer-runtime-config" type="application/json">${JSON.stringify(config)}</script>`;
        body=Buffer.from(count?html.replace(pattern,tag):html.replace('</head>',tag+'</head>'));
      }
      // Preserve the existing visual distribution's no-store delivery. In
      // particular never rewrite unhashed WASM into a fictional cacheable URL.
      const type={'.html':'text/html','.js':'text/javascript','.css':'text/css; charset=utf-8','.wasm':'application/wasm'}[extname(p)]||'application/octet-stream';
      audit('http',{method:req.method,path:pathname,status:200,bytes:body.length,sha256:sha(body),cacheControl:'no-store',contentType:type});
      res.writeHead(200,{'Content-Type':type,'Cache-Control':'no-store'});res.end(body);
    }catch(error){try{audit('http',{method:req.method,path:pathname,status:404,error:String(error)});}catch(secondary){markFailure(secondary);}res.writeHead(failed?500:404);res.end();}
  });
  const wss=new WebSocketServer({server,path:'/qa-world-ws',maxPayload:512*1024});
  wss.on('connection',socket=>{
    const connection=randomUUID();
    socket.on('error',error=>{markFailure(error);stopSockets();});
    socket.on('message',data=>{
      try{
        if(failed)throw fatalError;
        inputBytes+=data.length;if(inputBytes>64*1024*1024){markFailure(Error('owned Viewer WS input capacity exceeded'));stopSockets();throw fatalError;}
        const message=JSON.parse(data.toString());
        if(message.type==='request_world_view')requestCount++;
        audit('wire',{direction:'in',connection,type:message.type,generation:message.query?.request_generation??message.request_generation??null});
        const response=fixture.reply(message,connection);if(!response)return;
        const view=response.view;
        if(view)latestView={connection,generation:view.request_generation,world:view.world_id,scope:view.scope_id,cut:view.source_cut,completeness:view.completeness,delivery:view.delivery};
        audit('wire',{direction:'out',connection,type:response.type,generation:view?.request_generation??null,world:view?.world_id??null,scope:view?.scope_id??null,cut:view?.source_cut??null,completeness:view?.completeness??null,entities:view?.entities?.length??null});
        socket.send(JSON.stringify(response));
      }catch(error){markFailure(error);socket.close(1008,'controlled read-only fixture rejected request');}
    });
  });
  await new Promise((yes,no)=>{server.once('error',no);server.listen(0,'127.0.0.1',yes);});
  return {source,assets,fixture,url:`http://density-qa.localhost:${server.address().port}/viewer.html?test_api=1&connect=1&hosted_bootstrap=0&locale=en`,
    beginCase(id){caseId=id;},assertHealthy(){if(failed)throw fatalError;},statistics(){return {http:structuredClone(http),wire:structuredClone(wire),bytes,inputBytes,rows,requestCount,latestView:structuredClone(latestView),error:failed?String(fatalError):null};},
    verifyAssets(){if(!isDeepStrictEqual(assets,fingerprintViewerAssets(root)))throw Error('Viewer artifacts changed during probe');},
    async close(){if(closed)return;closed=true;for(const socket of wss.clients)socket.terminate();await new Promise(yes=>wss.close(yes));await new Promise((yes,no)=>server.close(error=>error?no(error):yes()));},
  };
}

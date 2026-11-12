import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtempSync,writeFileSync,mkdirSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {startOwnedViewerFixtureServer,fingerprintViewerAssets} from './viewer-owned-fixture-server.mjs';
const setup=async()=>{const dir=mkdtempSync(join(tmpdir(),'viewer-product-server-'));const root=join(dir,'assets'),out=join(dir,'evidence');mkdirSync(root);mkdirSync(out);writeFileSync(join(root,'viewer.html'),'<head></head>');writeFileSync(join(root,'test.wasm'),Buffer.from([0,97,115,109,1,0,0,0]));return{root,server:await startOwnedViewerFixtureServer({artifactRoot:root,source:'test',out,expectedAssets:fingerprintViewerAssets(root)})};};
test('same owned origin serves real bytes/no-store and refuses mutation/path escape',async()=>{
 const {server}=await setup();const url=server.url.replace('density-qa.localhost','127.0.0.1');
 try{const response=await fetch(url);const html=await response.text();assert.match(html,/trusted_local_only/);assert.match(html,/density-qa.localhost/);const wasm=await fetch(new URL('/test.wasm',url));assert.equal(wasm.headers.get('cache-control'),'no-store');assert.equal((await wasm.arrayBuffer()).byteLength,8);assert.equal((await fetch(url,{method:'POST'})).status,405);assert.equal((await fetch(new URL('/%2e%2e%2foutside',url))).status,403);server.verifyAssets();server.assertHealthy();}finally{await server.close();}
});
test('standard WS pages prove1000 same-cut records; snapshot stays empty and writes fail closed',async()=>{
 const {server}=await setup(),url=new URL(server.url);url.hostname='127.0.0.1';url.protocol='ws:';url.pathname='/qa-world-ws';url.search='';const socket=new WebSocket(url);
 const receive=()=>new Promise((yes,no)=>{const timer=setTimeout(()=>no(Error('WS test timeout')),2000);socket.addEventListener('message',e=>{clearTimeout(timer);yes(JSON.parse(e.data));},{once:true});});
 try{await new Promise((yes,no)=>{socket.addEventListener('open',yes,{once:true});socket.addEventListener('error',no,{once:true});});let next=receive();socket.send(JSON.stringify({type:'request_snapshot'}));assert.deepEqual((await next).snapshot.model.agents,{});let cursor=null,count=0;
 do{next=receive();socket.send(JSON.stringify({type:'request_world_view',query:{request_generation:'1:1',page_size:200,...(cursor?{cursor}:{})}}));const response=await next;assert.equal(response.view.source_cut,server.fixture.description().cut);count+=response.view.entities.length;cursor=response.view.next_cursor;}while(cursor);assert.equal(count,1000);server.assertHealthy();const closed=new Promise(yes=>socket.addEventListener('close',yes,{once:true}));socket.send(JSON.stringify({type:'Command'}));await closed;assert.throws(()=>server.assertHealthy(),/non-read/);}finally{socket.close();await server.close();}
});
test('asset mutation cannot inherit the original immutable proof',async()=>{const {server,root}=await setup();try{writeFileSync(join(root,'test.wasm'),'changed');assert.throws(()=>server.verifyAssets(),/changed/);}finally{await server.close();}});

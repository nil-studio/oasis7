import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,readFileSync,writeFileSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {startOwnedViewerFixtureServer} from './viewer-owned-fixture-server.mjs';
test('explicit retention mode streams full protocol rows with bounded scalar query summary',async()=>{
 const dir=mkdtempSync(join(tmpdir(),'retention-server-')),root=join(dir,'assets');mkdirSync(root);writeFileSync(join(root,'viewer.html'),'<head></head>');
 const server=await startOwnedViewerFixtureServer({artifactRoot:root,source:'test',out:dir,retentionStreaming:true}),url=new URL(server.url);url.hostname='127.0.0.1';url.protocol='ws:';url.pathname='/qa-world-ws';url.search='';const socket=new WebSocket(url);
 try{
 await new Promise((yes,no)=>{socket.addEventListener('open',yes,{once:true});socket.addEventListener('error',no,{once:true});});
 const receive=new Promise((yes,no)=>{const timer=setTimeout(()=>no(Error('bounded WS test')),2000);socket.addEventListener('message',e=>{clearTimeout(timer);yes(JSON.parse(e.data));},{once:true});});
 socket.send(JSON.stringify({type:'request_world_view',query:{request_generation:'1:1',page_size:200,search:'qa-agent-049'}}));const result=await receive;
 const summary=server.readWireSummary({search:'qa-agent-049'});assert.equal(summary.latestSearchMatches,true);assert.equal(summary.limitBytes,512*1024*1024);assert.equal(summary.requestCount,1);assert.equal(server.statistics().wire.length,0);
 const raw=readFileSync(join(dir,'server.jsonl')),rows=raw.toString().trim().split('\n').map(JSON.parse);assert.equal(summary.recordedBytes,raw.length);assert.equal(summary.recordedRows,rows.length);assert.equal(rows[0].message.query.search,'qa-agent-049');assert.deepEqual(rows[1].message,result);
 assert.equal(server.readWireSummary({search:''}).latestSearchMatches,false);
 }finally{socket.close();await server.close();}
});

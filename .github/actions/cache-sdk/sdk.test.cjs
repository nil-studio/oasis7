'use strict';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const os=require('node:os');
const {execFileSync}=require('node:child_process');
test('actual published SDK archive versions and four directional tar handoffs', async () => {
  const temporary=fs.mkdtempSync(path.join(os.tmpdir(),'portable-sdk-'));
  const oldEnv=process.env.GITHUB_WORKSPACE;
  try {
    console.log('cache-sdk-fixture phase=load-sdk');
    const patched=await import('../../../vendor/actions-cache-explicit-context/index.mjs');
    console.log('cache-sdk-fixture phase=tar-handoffs');
    const original=patched; // No options invokes the byte-preserved upstream defaults.
    const root=path.join(temporary,'home/runner/.cache/oasis7/worker-build/0.7.5');
    const host=path.join(temporary,'home/runner/work/oasis7/oasis7');
    const container=path.join(temporary,'__w/oasis7/oasis7');
    fs.mkdirSync(host,{recursive:true});fs.mkdirSync(container,{recursive:true});
    const historical='/home/runner/.cache/oasis7/worker-build/0.7.5';
    assert.equal(patched.utils.getCacheVersion([historical],'zstd-without-long',false),'057d5ed12ddf291714b0e15f44112c6469c9ec0a79a74779a48613846a0198fc');
    assert.equal(patched.utils.getCacheVersion([historical],'zstd-without-long',false),original.utils.getCacheVersion([historical],'zstd-without-long',false));
    assert.equal(patched.utils.getCacheVersion(['/home/runner/work/_temp/oasis7-browser-archives/fonts-freefont-ttf-039c03e75c206d2555b0c40b0f2fe0861a318ade4117c5404c533eb77c81ab5d'],'zstd-without-long',false),'e336ca23d329e223a6fd89d1cdb9e790d7a74a70081146644002cc77f91aca35');
    // Old SDK under container workspace demonstrably produces the wrong member.
    fs.mkdirSync(path.join(root,'bin'),{recursive:true});fs.writeFileSync(path.join(root,'bin/worker-build'),'portable-payload');
    process.env.GITHUB_WORKSPACE=container;
    assert.notDeepEqual(await original.utils.resolvePaths([root]), await patched.utils.resolvePaths([root],host));
    const expected=path.relative(host,root);
    for (const [writer,reader] of [['host','container'],['container','host'],['container','container'],['host','host']]) {
      const archive=fs.mkdtempSync(path.join(temporary,'archive-'));
      process.env.GITHUB_WORKSPACE=writer==='host'?host:container;
      const sdk=writer==='host'?original:patched;
      const cwd=writer==='host'?undefined:host;
      const members=await sdk.utils.resolvePaths([root],cwd);
      assert.deepEqual(members,[expected]);
      await sdk.tar.createTar(archive,members,'zstd-without-long',cwd);
      const listing=execFileSync(process.platform === 'darwin' ? 'gtar' : 'tar',['-P','--use-compress-program','unzstd','-tf',path.join(archive,sdk.utils.getCacheFileName('zstd-without-long'))],{encoding:'utf8'});
      assert.ok(listing.includes(expected+'/bin/worker-build'));
      fs.rmSync(root,{recursive:true});
      process.env.GITHUB_WORKSPACE=reader==='host'?host:container;
      await (reader==='host'?original:patched).tar.extractTar(path.join(archive,sdk.utils.getCacheFileName('zstd-without-long')),'zstd-without-long',reader==='host'?undefined:host);
      assert.equal(fs.readFileSync(path.join(root,'bin/worker-build'),'utf8'),'portable-payload');
    }
    const cache=patched.sdk;
    const prior=process.env.ACTIONS_CACHE_SERVICE_V2;delete process.env.ACTIONS_CACHE_SERVICE_V2;
    await assert.rejects(cache.restoreCache(['/unused'],'key',[],{archiveWorkingDirectory:host}), /requires cache service v2/);
    process.env.ACTIONS_CACHE_SERVICE_V2='True';
    for (const cwd of ['relative', '/tmp/../escape', '/tmp/with space', '/tmp/quote;command'])
      await assert.rejects(cache.restoreCache(['/unused'],'key',[],{archiveWorkingDirectory:cwd}), /absolute directory/);
    await assert.rejects(cache.restoreCache(['/unused'],'key',[],{archiveWorkingDirectory:host,beforeExtract:'not a function'}), /must be a function/);
    if(prior===undefined)delete process.env.ACTIONS_CACHE_SERVICE_V2;else process.env.ACTIONS_CACHE_SERVICE_V2=prior;
  } finally {
    if(oldEnv===undefined)delete process.env.GITHUB_WORKSPACE;else process.env.GITHUB_WORKSPACE=oldEnv;
    fs.rmSync(temporary,{recursive:true,force:true});
  }
});

test('real V2 HTTP download invokes fatal validation before SDK extraction', {timeout:10000}, async () => {
  const http=require('node:http');
  const temporary=fs.mkdtempSync(path.join(os.tmpdir(),'portable-http-'));
  const variables=['GITHUB_WORKSPACE','ACTIONS_CACHE_SERVICE_V2','ACTIONS_RESULTS_URL','ACTIONS_RUNTIME_TOKEN','GITHUB_SERVER_URL','RUNNER_TEMP'];
  const old=Object.fromEntries(variables.map(key=>[key,process.env[key]]));
  console.log('cache-sdk-fixture phase=http-loopback');
  const {utils,tar,sdk}=await import('../../../vendor/actions-cache-explicit-context/index.mjs');
  const workspace=path.join(temporary,'workspace'), root=path.join(temporary,'root'), archive=path.join(temporary,'archive');
  for(const directory of [workspace,root,archive])fs.mkdirSync(directory,{recursive:true});
  fs.writeFileSync(path.join(root,'payload'),'actual SDK download');
  let server;
  try {
    process.env.GITHUB_WORKSPACE=workspace;
    await tar.createTar(archive,await utils.resolvePaths([root],workspace),'zstd-without-long',workspace);
    const bytes=fs.readFileSync(path.join(archive,utils.getCacheFileName('zstd-without-long')));
    fs.rmSync(root,{recursive:true});
    const requests=[];
    server=http.createServer((request,response)=>{
      if(request.url==='/archive'){response.writeHead(200,{'content-length':bytes.length});response.end(bytes);return;}
      assert.equal(request.headers['user-agent'],'@actions/cache-6.1.0');
      let body='';request.on('data',chunk=>body+=chunk);request.on('end',()=>{
        requests.push(JSON.parse(body));response.setHeader('content-type','application/json');
        response.end(JSON.stringify({ok:true,signedDownloadUrl:`http://127.0.0.1:${server.address().port}/archive`,matchedKey:'fixed'}));
      });
    });
    await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
    process.env.ACTIONS_CACHE_SERVICE_V2='True';process.env.ACTIONS_RESULTS_URL=`http://127.0.0.1:${server.address().port}`;
    process.env.ACTIONS_RUNTIME_TOKEN='local-fixture-only';process.env.GITHUB_SERVER_URL='https://github.com';process.env.RUNNER_TEMP=temporary;
    let callbacks=0;
    console.log('cache-sdk-fixture phase=reject-before-extract');
    await assert.rejects(sdk.restoreCache([root],'fixed',[],{archiveWorkingDirectory:workspace,beforeExtract:async(file,compression,matched)=>{
      callbacks++;assert.equal(matched,'fixed');assert.equal(compression,'zstd-without-long');assert.deepEqual(fs.readFileSync(file),bytes);
      assert.equal(fs.existsSync(root),false);throw Error('malicious fixture rejected');
    }}), {name:'ValidationError',message:'Explicit archive validation rejected downloaded cache'});
    assert.equal(callbacks,1);assert.equal(fs.existsSync(root),false);
    console.log('cache-sdk-fixture phase=accept-before-extract');
    const match=await sdk.restoreCache([root],'fixed',[],{archiveWorkingDirectory:workspace,beforeExtract:async()=>{callbacks++;assert.equal(fs.existsSync(root),false);}});
    assert.equal(match,'fixed');assert.equal(callbacks,2);assert.equal(fs.readFileSync(path.join(root,'payload'),'utf8'),'actual SDK download');
    assert.equal(requests.length,2);
    for(const request of requests) assert.equal(request.version,utils.getCacheVersion([root],'zstd-without-long',false));
  } finally {
    if(server)await new Promise(resolve=>server.close(resolve));
    for(const [key,value]of Object.entries(old)){if(value===undefined)delete process.env[key];else process.env[key]=value;}
    fs.rmSync(temporary,{recursive:true,force:true});
  }
});

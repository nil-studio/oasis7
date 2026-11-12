import {test} from 'node:test';
import assert from 'node:assert/strict';
import {nativeHttp} from '../platform/native.mjs';
test('native HTTP uses canonical identity prefix and CSRF header; never follows redirects',async()=>{
 const originalFetch=globalThis.fetch;let observed;
 globalThis.fetch=async(url,options)=>{observed={url,options};return new Response('{"ok":true}');};
 try{
  await nativeHttp('https://authority.example.test','/entry-executor/control','POST','__Host-oasis7_sid=test','a'.repeat(64),'{"integer":18446744073709551615}',Date.now()+1000,1024);
  assert.equal(observed.url,'https://authority.example.test/api/public/hosted-account/v2/entry-executor/control');
  assert.equal(observed.options.headers['X-Oasis7-Csrf'],'a'.repeat(64));
  assert.equal(observed.options.body,'{"integer":18446744073709551615}');
  assert.equal(observed.options.redirect,'manual');
  globalThis.fetch=async()=>new Response(null,{status:302,headers:{Location:'https://foreign.example.test'}});
  await assert.rejects(nativeHttp('https://authority.example.test','/session','GET','private','','',Date.now()+1000,1024));
 }finally{globalThis.fetch=originalFetch;}
});

test('explicit trusted alias preserves native audience without redirecting cookie destination',async()=>{
 const originalFetch=globalThis.fetch;let observed;
 globalThis.fetch=async(url,options)=>{observed={url,options};return new Response('{}');};
 try{
  await nativeHttp('https://transport.example.test','/entry-executor/control','POST','__Host-oasis7_sid=test','a'.repeat(64),'{}',Date.now()+1000,1024,'https://original.example.test');
  assert.equal(observed.url,'https://transport.example.test/api/public/hosted-account/v2/entry-executor/control');
  assert.equal(observed.options.headers.Origin,'https://original.example.test');
  assert.equal(observed.options.headers.Cookie,'__Host-oasis7_sid=test');
 }finally{globalThis.fetch=originalFetch;}
});

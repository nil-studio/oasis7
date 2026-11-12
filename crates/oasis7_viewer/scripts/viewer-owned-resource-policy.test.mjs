import test from 'node:test';
import assert from 'node:assert/strict';
import {verifyOwnedResourceSnapshot as verify} from './viewer-owned-resource-policy.mjs';
const row=(pid,ppid,rssKiB=10,birth='birth-A')=>({pid,ppid,rssKiB,birth});
const fixture=()=>({profile:'/tmp/owned-A',browserPid:100,commandLine:['--user-data-dir=/tmp/owned-A'],pageUrls:['http://127.0.0.1:1/'],expectedUrl:'http://127.0.0.1:1/',before:[row(100,1),row(101,100),row(999,1,99999)],after:[row(100,1),row(101,100),row(999,1,99999)],cdpProcesses:[{id:100,type:'browser'},{id:101,type:'GPU'}]});
test('only proven descendants contribute RSS; sum remains a component diagnostic',()=>{const result=verify(fixture());assert.equal(result.rawRssSumBytes,20480);assert.equal(result.components.length,2);assert.equal(result.gpuAllocatedBytes,null);});
test('foreign CDP process, root reuse and child races fail closed',()=>{for(const change of [f=>f.cdpProcesses.push({id:999,type:'GPU'}),f=>f.after[0].birth='birth-B',f=>f.after[1].ppid=1]){const f=fixture();change(f);assert.throws(()=>verify(f));}});
test('extra pages and profile prefix/duplicate arguments cannot establish ownership',()=>{for(const change of [f=>f.pageUrls.push('about:blank'),f=>f.commandLine=['--user-data-dir=/tmp/owned-AB'],f=>f.commandLine.push('--user-data-dir=/tmp/other')]){const f=fixture();change(f);assert.throws(()=>verify(f));}});

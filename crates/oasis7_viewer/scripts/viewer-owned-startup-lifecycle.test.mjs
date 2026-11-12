import test from 'node:test';
import assert from 'node:assert/strict';
import {EventEmitter} from 'node:events';
import {createStartupProfile,launchOwnedStartupContext} from './viewer-owned-startup-context.mjs';
import {identifyOwnedStartupContext,closeOwnedStartupContext} from './viewer-owned-startup-lifecycle.mjs';
async function contextFor(args) {
  const lease=createStartupProfile(),context=new EventEmitter();context.close=async()=>context.emit('close');
  context.browser=()=>({newBrowserCDPSession:async()=>({send:async method=>method==='Browser.getBrowserCommandLine'?{arguments:args??['--user-data-dir='+lease.path]}:{processInfo:[{type:'browser',id:765432}]},detach:async()=>{}})});
  return launchOwnedStartupContext({launchPersistentContext:async()=>context},'/chrome',lease);
}
test('early product failure still has pre-navigation native identity and verified close',async t=>{
  const context=await contextFor(),identity=await identifyOwnedStartupContext(context);assert.equal(identity.nativePid,765432);
  t.mock.method(process,'kill',()=>{const error=Error('gone');error.code='ESRCH';throw error;});
  assert.equal((await closeOwnedStartupContext(context)).processExited,true);
});
test('foreign profile or disabled sandbox cannot acquire cleanup identity',async()=>{
  for(const args of [['--user-data-dir=/foreign'],['--no-sandbox']]){const context=await contextFor(args);await assert.rejects(identifyOwnedStartupContext(context),/identity/);await assert.rejects(closeOwnedStartupContext(context),/identity/);}
});

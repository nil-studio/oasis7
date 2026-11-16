import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
const source=readFileSync(new URL('./world-view-retention-memory.mjs',import.meta.url),'utf8');
function harness(rows,clicked=true,late=false,lateOverview=false){let now=0,reads=0,clicks=0;const context={performance:{now:()=>now},fixture:{description:()=>({cut:'cut'})},read:async()=>{reads++;if(late)now+=13000;return rows[Math.min(reads-1,rows.length-1)]},run:async()=>{now+=50},nativeCount:async()=>clicks,overview:async()=>{if(lateOverview)now+=13000;if(clicked)clicks++},assert:(v,m)=>{if(!v)throw Error(m)}};vm.createContext(context);vm.runInContext(source.slice(source.indexOf('  const settle ='),source.indexOf('  const disclosure='))+source.slice(source.indexOf('  const home ='),source.indexOf('  const cycle ='))+';globalThis.home=home;',context);return{home:()=>context.home(),time:()=>now,reads:()=>reads};}
const row=(bodies=1000,labels=200,status=null)=>({stats:{loading:false,stale:false,hasError:false,published:{queryGenerationMatches:true,sourceCutMatches:true,completeness:'complete',entities:1000}},metrics:{status:'observed',cut:'cut',bodies,labels},overviewStatus:status});
test('home waits for physical density and applied overview rather than complete DTO alone',async()=>{const h=harness([row(800),row(1000,199),row(1000,200,'applying'),row()]);await h.home();assert.equal(h.reads(),4);});
test('home nested settle shares the same twelve second deadline',async()=>{const h=harness([row(800)]);await assert.rejects(h.home(),/did not settle/);assert.equal(h.time(),12000);});
test('complete but unavailable overview cannot become home',async()=>{const h=harness([row(1000,200,'unavailable')]);await assert.rejects(h.home(),/overview unavailable/);});

test('no-op overview cannot certify applied home',async()=>{const h=harness([row()],false);await assert.rejects(h.home(),/trusted button click/);});

test('late asynchronous read cannot pass after the shared deadline',async()=>{const h=harness([row()],true,true);await assert.rejects(h.home(),/deadline elapsed/);});

test('late overview callback cannot restart the home deadline',async()=>{const h=harness([row()],true,false,true);await assert.rejects(h.home(),/deadline elapsed/);});

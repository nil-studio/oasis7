import {writeFileSync,appendFileSync,mkdirSync} from 'node:fs';
import {resolve} from 'node:path';
import {createOwnedCdpSession} from './viewer-owned-cdp-session.mjs';

export function summarizeFrameEvents(events) {
 const counts={};for(const event of events)counts[event.name]=(counts[event.name]||0)+1;
 return {eventCounts:counts,screenshots:events.filter(e=>e.name==='Screenshot').length,
  worldPassSubmission:null,worldGpuCompletion:null,worldPresentedFrames:null,frameBudget:'unknown',
  reason:'Browser image/swap records do not map the active world render version/pass through GPU completion to physical presentation; UI redraw and reused world textures are not new world frames.'};
}
export async function withObservationCleanup(body,cleanup,report) {
 let original,value;
 try { value=await body(); } catch(error) { original=error; }
 try { await cleanup(); } catch(error) { (report.cleanupErrors??=[]).push(String(error));if(!original)original=error; }
 if(original) { if(report.cleanupErrors?.length) { try { original.observationCleanupErrors=[...report.cleanupErrors]; } catch {} console.error('observation cleanup failed: '+JSON.stringify(report.cleanupErrors)); } throw original; }return value;
}
// Page-target images only. No browser-global trace is started, including when
// one page happens to exist: another page could be created while tracing.
export async function probeWorldFrameCapabilities({run,evaluate,out,durationMs=3000,action}) {
 if(!Number.isFinite(durationMs)||durationMs<1000||durationMs>10000)throw Error('frame probe duration must be 1..10 seconds');
 const session=await createOwnedCdpSession(run,evaluate),report={durationMs,scope:'bounded page image capability; not P6 acceptance',browserTrace:'not started: session-tab ownership does not prove exclusive browser process ownership'};
 return withObservationCleanup(async()=>{
 try {
  report.initialPageCount=session.pageCount;
  try { report.browser=await session.browser('Browser.getVersion');report.traceCategories=await session.browser('Tracing.getCategories'); }
  catch(error) { report.browserInventory={status:'unavailable',reason:String(error)}; }
  report.pageImages=await observePageImages({session,run,out,durationMs,action,report});
  report.observation=summarizeFrameEvents([]);report.status='page_image_capability_only';return report;
 }catch(error){report.status='failed';report.error=String(error);throw error;}
 },async()=>{
  session.close();writeFileSync(resolve(out,'frame-capability.json'),JSON.stringify(report,null,2));
 },report);
}
async function observePageImages({session,run,out,durationMs,action,report}) {
 const dir=resolve(out,'page-frame-images');mkdirSync(dir);let count=0,bytes=0,overflow=false,error=null;const pending=new Set();
 const anchor=async()=>{const r=await session.page('Runtime.evaluate',{expression:'({now:performance.now(),origin:performance.timeOrigin,wall:Date.now()})',returnByValue:true});return r.result.value;};
 const before=await anchor();
 const remove=session.onPageEvent(event=>{
  if(event.method!=='Page.screencastFrame')return;
  const row=event.params;count++;const data=Buffer.from(row.data,'base64');bytes+=data.length;
  if(count>600||bytes>64*1024*1024)overflow=true;
  else try {writeFileSync(resolve(dir,String(count).padStart(4,'0')+'.jpg'),data);appendFileSync(resolve(out,'page-frame-metadata.jsonl'),JSON.stringify({index:count,bytes:data.length,metadata:row.metadata,sessionId:row.sessionId,receivedWall:Date.now()})+'\n');} catch(e) { error=String(e); }
  const ack=session.page('Page.screencastFrameAck',{sessionId:row.sessionId}).catch(e=>{error=String(e);}).finally(()=>pending.delete(ack));pending.add(ack);
 });
 await withObservationCleanup(async()=>{
  await session.page('Page.startScreencast',{format:'jpeg',quality:80,everyNthFrame:1});
  if(action)await action();await run(['wait',String(durationMs)]);
 },async()=>{
  try {await session.page('Page.stopScreencast');} finally {remove();await Promise.all(pending);report.pageImageCollection={count,bytes,overflow,error};}
 },report);
 const after=await anchor();if(count===0)throw Error('page image capability returned no frames');if(overflow||error)throw Error('page image observation incomplete: '+JSON.stringify({overflow,error,count,bytes}));
 return {count,bytes,before,after,clockMapping:'raw frame-swap metadata and page performance/timeOrigin/wall anchors; no calibrated per-input visible ACK timing yet',coverage:'page-target screencast; not every-frame delivery, physical presentation, GPU completion or new world texture proof'};
}

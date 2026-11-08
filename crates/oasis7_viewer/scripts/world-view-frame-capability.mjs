import {writeFileSync,openSync,writeSync,closeSync,appendFileSync,mkdirSync} from 'node:fs';
import {resolve} from 'node:path';
import {createOwnedCdpSession} from './viewer-owned-cdp-session.mjs';

export function summarizeFrameEvents(events) {
 const counts={};for(const event of events)counts[event.name]=(counts[event.name]||0)+1;
 return {eventCounts:counts,screenshots:events.filter(e=>e.name==='Screenshot').length,
  worldPassSubmission:null,worldGpuCompletion:null,worldPresentedFrames:null,frameBudget:'unknown',
  reason:'Browser trace events do not establish a mapping from the active world render version/pass through GPU completion to a presented frame; UI redraw and reused world textures are not new world frames.'};
}

// Capability probe only: no forced frame, no artificial paint fence and no FPS gate.
export async function probeWorldFrameCapabilities({run,evaluate,out,durationMs=3000,action}) {
 if(!Number.isFinite(durationMs)||durationMs<1000||durationMs>10000)throw Error('frame probe duration must be 1..10 seconds');
 const session=await createOwnedCdpSession(run,evaluate),report={durationMs,scope:'bounded browser frame observation capability; not P6 acceptance'},path=resolve(out,'frame-capability.json');
 let tracing=false,fd=null,unsubscribe=()=>{};
 try {
  report.initialPageCount=session.pageCount;
  try { report.browser=await session.browser('Browser.getVersion'); } catch(error) {
    if (!String(error.message).startsWith('browser-wide observation requires one owned page')) throw error;
    report.browserTrace={status:'unavailable',reason:String(error.message)};
    report.pageImages=await observePageImages({session,run,out,durationMs,action});
    report.observation=summarizeFrameEvents([]);report.status='page_image_capability_only';return report;
  }
  const available=(await session.browser('Tracing.getCategories')).categories;
  if(!Array.isArray(available))throw Error('missing trace category capability response');
  const desired=['devtools.timeline','blink.user_timing','disabled-by-default-devtools.screenshot','cc','benchmark'];
  report.availableCategories=available;report.requestedCategories=desired.filter(c=>available.includes(c));report.unsupportedCategories=desired.filter(c=>!available.includes(c));
  if(!report.requestedCategories.length)throw Error('no supported frame observation categories');
  let complete;const completed=new Promise(resolve=>complete=resolve);
  unsubscribe=session.onEvent(event=>{
   if(event.method==='Tracing.bufferUsage'&&event.params?.percentFull>=.95)report.bufferFull=true;
   if(event.method==='Tracing.tracingComplete')complete(event.params);
  });
  await session.browser('Tracing.start',{transferMode:'ReturnAsStream',streamFormat:'json',bufferUsageReportingInterval:250,traceConfig:{recordMode:'recordUntilFull',traceBufferSizeInKb:16384,includedCategories:report.requestedCategories}});tracing=true;
  const mark=async name=>{const before=await evaluate('performance.now()');await session.page('Runtime.evaluate',{expression:`performance.mark(${JSON.stringify(name)})`,returnByValue:true});const after=await evaluate('performance.now()');return{name,before,after};};
  report.startAnchor=await mark('oasis7-frame-probe-start');
  if(action)await action();
  await run(['wait',String(durationMs)]);
  report.endAnchor=await mark('oasis7-frame-probe-end');
  await session.browser('Tracing.end');tracing=false;
  let timer;const done=await Promise.race([completed,new Promise((_,reject)=>timer=setTimeout(()=>reject(Error('trace completion timed out')),10000))]).finally(()=>clearTimeout(timer));
  report.dataLossOccurred=done.dataLossOccurred===true;
  if(!done.stream)throw Error('trace completed without raw stream');
  const stream=done.stream;fd=openSync(resolve(out,'frame-trace.json'),'wx');let bytes=0;const chunks=[];
  try {
   const deadline=Date.now()+15000;
   while(true){if(Date.now()>deadline)throw Error('trace stream time budget exceeded');const row=await session.browser('IO.read',{handle:stream,size:65536});const data=Buffer.from(row.data||'',row.base64Encoded?'base64':'utf8');writeSync(fd,data);bytes+=data.length;
    if(bytes>32*1024*1024)throw Error('trace stream byte budget exceeded; partial raw preserved');chunks.push(data);if(row.eof)break;}
  }finally{closeSync(fd);fd=null;await session.browser('IO.close',{handle:stream});report.rawBytes=bytes;}
  const raw=JSON.parse(Buffer.concat(chunks).toString('utf8'));report.observation=summarizeFrameEvents(raw.traceEvents||[]);
  report.anchorEvents=(raw.traceEvents||[]).filter(e=>e.name==='oasis7-frame-probe-start'||e.name==='oasis7-frame-probe-end').map(({name,ts,pid,tid,ph})=>({name,ts,pid,tid,ph}));
  if(report.bufferFull||report.dataLossOccurred)throw Error('trace overflow/data loss; capability evidence incomplete');
  report.status='capabilities_observed_only';return report;
 }catch(error){report.status='failed';report.error=String(error);throw error;}
 finally{if(fd!==null)closeSync(fd);if(tracing)try{await session.browser('Tracing.end');}catch{}unsubscribe();session.close();writeFileSync(path,JSON.stringify(report,null,2));}
}

// Page-target stream, independent of browser-global tracing. These images are not
// a promise of every presentation or a world-pass/version fence.
async function observePageImages({session,run,out,durationMs,action}) {
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
 try {await session.page('Page.startScreencast',{format:'jpeg',quality:80,everyNthFrame:1});if(action)await action();await run(['wait',String(durationMs)]);}
 finally {try{await session.page('Page.stopScreencast');}finally{remove();await Promise.all(pending);}}
 const after=await anchor();if(overflow||error)throw Error('page image observation incomplete: '+JSON.stringify({overflow,error,count,bytes}));
 return {count,bytes,before,after,clockMapping:'raw frame-swap timestamp metadata retained; page performance/timeOrigin/wall anchors retained; no calibrated per-input visible ACK timing yet',coverage:'page-target screencast only; not every-frame delivery, physical display presentation, GPU completion or new world texture proof'};
}

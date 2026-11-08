import {writeFileSync,openSync,writeSync,closeSync} from 'node:fs';
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
  report.browser=await session.browser('Browser.getVersion');
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

import {readOwnedStartupProfile} from './viewer-owned-startup-context.mjs';
import {withObservationCleanup} from './world-view-frame-capability.mjs';
const identities=new WeakMap();
export async function boundedStartupOperation(operation,label,milliseconds=10000) {
  let timer;try{return await Promise.race([operation,new Promise((_,no)=>{timer=setTimeout(()=>no(Error(label+' exceeded budget')),milliseconds);})]);}finally{clearTimeout(timer);}
}
export async function identifyOwnedStartupContext(context,report={}) {
  const profile=readOwnedStartupProfile(context),cdp=await boundedStartupOperation(context.browser().newBrowserCDPSession(),'owned browser identity attach');
  return withObservationCleanup(async()=>{
    const command=await boundedStartupOperation(cdp.send('Browser.getBrowserCommandLine'),'owned browser command line');
    if(command.arguments.filter(a=>a==='--user-data-dir='+profile).length!==1||command.arguments.filter(a=>a.startsWith('--user-data-dir=')).length!==1||command.arguments.includes('--no-sandbox'))throw Error('owned browser profile/sandbox identity unproven');
    const processes=await boundedStartupOperation(cdp.send('SystemInfo.getProcessInfo'),'owned native PID');
    const roots=processes.processInfo.filter(p=>p.type==='browser');
    if(roots.length!==1||!Number.isSafeInteger(roots[0].id)||roots[0].id<=0)throw Error('owned native browser PID ambiguous');
    const value=Object.freeze({profile,nativePid:roots[0].id});identities.set(context,value);return value;
  },()=>boundedStartupOperation(cdp.detach(),'detach browser identity'),report);
}
export async function closeOwnedStartupContext(context) {
  const identity=identities.get(context); // Capture before the factory close hook retires ownership.
  await boundedStartupOperation(context.close(),'close owned browser');
  if(!identity)throw Error('owned native browser cleanup identity unavailable');
  const end=performance.now()+2000;
  while(performance.now()<end){
    try{process.kill(identity.nativePid,0);}catch(error){if(error.code==='ESRCH'){identities.delete(context);return {...identity,processExited:true};}throw error;}
    await new Promise(yes=>setTimeout(yes,50));
  }
  throw Error('owned browser native PID remains after close');
}

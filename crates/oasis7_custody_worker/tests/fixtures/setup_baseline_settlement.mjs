// Happy-baseline observation only. Never approve/reconcile or replace an intent.
import {setTimeout as delay} from 'node:timers/promises';

export async function continuePreparedBaseline(runtime, plan, {now=Date.now,pause=delay}={}) {
  const deadline=Number(plan.deadline);
  if(!Number.isSafeInteger(deadline)) throw new Error('baseline needs the original setup deadline');
  let last;
  for(let observation=0;observation<16;observation++) {
    if(now()>=deadline) break;
    last=await runtime.continueSetup(plan.setup_id);
    if(!last.ok) return last;
    const progress=await last.clone().json();
    if(progress.stage!=='ready_for_setup') return last;
    // A durable terminal/permission failure is not a baseline timing race.
    if((progress.blocked_operations??[]).some(item=>item.code!=='outcome_unknown')) return last;
    const remaining=deadline-now();
    if(remaining<=0) break;
    await pause(Math.min(20,remaining));
  }
  if(!last) throw new Error('baseline setup deadline elapsed before observation');
  // Preserve pending/unknown for the caller's existing diagnostic assertion.
  return last;
}

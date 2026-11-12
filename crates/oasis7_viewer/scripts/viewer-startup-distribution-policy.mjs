// Fixed local browser-profile distribution; no physical-frame or A/B claim.
export function startupCasePlan(distribution=false){
 return distribution ? [...Array.from({length:20},(_,i)=>({name:`cold-${String(i+1).padStart(2,'0')}`,profile:i,cohort:'cold'})),...Array.from({length:30},(_,i)=>({name:`warm-${String(i+1).padStart(2,'0')}`,profile:0,cohort:'warm'}))] : [{name:'cold-first',profile:0,cohort:'calibration'},{name:'warm-same-profile',profile:0,cohort:'calibration'},{name:'cold-independent',profile:1,cohort:'calibration'}];
}
export function startupDistribution(cases){
 const expected=startupCasePlan(true);if(cases.length!==50||cases.some((r,i)=>r.name!==expected[i].name||r.status!=='passed_calibration_only'||!r.close?.processExited))throw Error('complete ordered50 successful owned samples required');
 return Object.fromEntries(['cold','warm'].map(cohort=>[cohort,Object.fromEntries(['bridgeReadyUpperMs','completeSceneUpperMs','atlasImageUpperMs','nativeSelectedDomUpperMs'].map(key=>{const values=cases.filter((_,i)=>expected[i].cohort===cohort).map(r=>r.observedEndpoints[key]);if(values.some(x=>!Number.isFinite(x)||x<0))throw Error('unknown endpoint');const sorted=[...values].sort((a,b)=>a-b);return[key,{n:values.length,raw:values,p50:sorted[Math.ceil(values.length*.5)-1],p95:sorted[Math.ceil(values.length*.95)-1],max:sorted.at(-1),method:'nearest_rank_observed_upper_not_physical'}];}))]));
}

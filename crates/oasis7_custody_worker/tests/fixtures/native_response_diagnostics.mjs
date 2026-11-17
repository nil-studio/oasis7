// Fixture-only metadata. Never retain request/response bodies, headers or errors.
const routes=['/capabilities','/native-key/options','/native-key/complete','/custody/managed-bindings/session-status'];
const phases=['started','headers','body','transport_error'];
const classes=['1xx','2xx','3xx','4xx','5xx','unknown'];
const shapes=['valid','missing','invalid','invalid_json','oversize','not_observed'];
const max=16;
const count=n=>Number.isSafeInteger(n)&&n>=0?n:null;
export function projectNativeResponses(value) {
  return {omitted:count(value?.omitted),events:Array.isArray(value?.events)?value.events.slice(-max).map(e=>({
    route:routes.includes(e?.route)?e.route:'unknown',phase:phases.includes(e?.phase)?e.phase:'unknown',
    status_class:classes.includes(e?.status_class)?e.status_class:'unknown',elapsed_ms:count(e?.elapsed_ms),
    csrf_shape:shapes.includes(e?.csrf_shape)?e.csrf_shape:'not_observed',
  })):[]};
}
export function createNativeResponses(clock=()=>performance.now()) {
  const facts={omitted:0,events:[]};
  return {snapshot:()=>projectNativeResponses(facts),begin(route) {
    if(!routes.includes(route))return {headers(){},body(){},failed(){}};
    const started=clock();
    const event={route,phase:'started',status_class:'unknown',elapsed_ms:0,csrf_shape:'not_observed'};
    if(facts.events.length===max){facts.events.shift();facts.omitted++;}
    facts.events.push(event);
    const elapsed=()=>{const n=Math.floor(clock()-started);event.elapsed_ms=count(n);};
    return {
      headers(status){event.phase='headers';event.status_class=Number.isInteger(status)&&status>=100&&status<600?`${Math.floor(status/100)}xx`:'unknown';elapsed();},
      failed(){event.phase='transport_error';elapsed();},
      body(text){event.phase='body';elapsed();if(route!=='/capabilities')return;
        // Parse only the existing fixture body, with a separate diagnostic cap.
        if(typeof text!=='string'||text.length>32768){event.csrf_shape='oversize';return;}
        try{const value=JSON.parse(text);const token=value?.csrf_token;
          event.csrf_shape=token===undefined?'missing':typeof token==='string'&&/^[0-9a-f]{64}$/.test(token)?'valid':'invalid';
        }catch{event.csrf_shape='invalid_json';}
      },
    };
  }};
}

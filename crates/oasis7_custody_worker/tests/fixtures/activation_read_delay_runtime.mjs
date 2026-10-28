// Controlled readonly batching delay, never an authority or a synthetic reply.
import deployed,{AccountAuthority as ActualAccount} from './native_owner_result_loss.mjs';
export * from './native_owner_result_loss.mjs';
export default deployed;
function batch(mode){
  const facts={history_reads:0,enrollment_reads:0,history_completed:0,history_rejected:false,released:null,elapsed_ms:0};
  let resolve,started=null,timer;
  const ready=new Promise(done=>{resolve=done;});
  const release=reason=>{if(facts.released!==null)return;facts.released=reason;facts.elapsed_ms=Date.now()-started;clearTimeout(timer);resolve();};
  return {facts,async arrive(name){
    facts[name]++;
    if(started===null){started=Date.now();timer=setTimeout(()=>release('bounded_timeout'),5100);}
    if(mode!=='over_window'&&facts.history_reads>0&&facts.enrollment_reads>0)release('independent_reads_started');
    await ready;
  }};
}
function bound(target,name){const value=Reflect.get(target,name,target);return typeof value==='function'&&name!=='constructor'?value.bind(target):value;}
export class AccountAuthority extends ActualAccount {
  constructor(state,env){
    const control={active:null,mode:env.FIXTURE_ACTIVATION_READ_MODE};
    const wrapped=new Proxy(env,{get(target,name){
      if(name==='TEST_SESSION_PRIVATE_HISTORY')return new Proxy(target[name],{get(service,method){
        if(method==='attest')return async raw=>{if(control.active)await control.active.arrive('history_reads');const result=await service.attest(raw);if(control.active){control.active.facts.history_completed++;if(env.FIXTURE_ACTIVATION_READ_MODE==='reject_history'){control.active.facts.history_rejected=true;throw new Error('fixed test rejects completed readonly input');}}return result;};
        return bound(service,method);
      }});
      if(name==='CLIENT_ENROLLMENTS')return new Proxy(target[name],{get(namespace,method){
        if(method==='get')return (...args)=>new Proxy(namespace.get(...args),{get(stub,operation){
          if(operation==='fetch')return async(...input)=>{
            const path=new URL(typeof input[0]==='string'?input[0]:input[0].url).pathname;
            if(control.active&&path==='/internal/read-proved')await control.active.arrive('enrollment_reads');
            return stub.fetch(...input);
          };
          return bound(stub,operation);
        }});
        return bound(namespace,method);
      }});
      return bound(target,name);
    }});
    super(state,wrapped);this.control=control;this.enabled=env.FIXTURE_ACTIVATION_READ_DELAY==='true';
  }
  async fetch(request){
    const url=new URL(request.url);
    if(this.enabled&&request.method==='POST'&&url.origin==='https://account.internal'&&url.pathname==='/fixture/activation-read-delay-status'&&!url.search&&!url.hash)
      return Response.json(this.control.active?.facts??{history_reads:0,enrollment_reads:0,history_completed:0,history_rejected:false,released:null,elapsed_ms:0});
    if(this.enabled&&request.method==='POST'&&url.pathname==='/internal/test-session-identity'){
      const input=await request.clone().json();
      if(input.action?.kind==='activate')this.control.active=batch(this.control.mode);
    }
    return super.fetch(request);
  }
}

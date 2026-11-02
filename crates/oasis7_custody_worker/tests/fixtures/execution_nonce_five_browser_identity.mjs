// Trusted browser provider wrapper retained; no response/authority substitutes.
import deployed,{ClientEnrollmentAuthority as RealAuthority} from './custody_browser_provider_adapter.mjs';
export * from './custody_browser_provider_adapter.mjs';
export default {async fetch(request,env,ctx){
  if(request.method==='POST'&&new URL(request.url).pathname.endsWith('/client/proof-challenges')){
    let body;try{body=await request.clone().json();}catch{}
    if(body?.kind?.kind==='tool')console.warn('fixture.tool_nonce.public');
  }
  return deployed.fetch(request,env,ctx);
}};
export class ClientEnrollmentAuthority extends RealAuthority {
  constructor(state,env){super(state,env);this.fixturePublicOrigin=env.CUSTODY_PUBLIC_ORIGIN;}
  async fetch(request){
    const prove=request.method==='POST'&&new URL(request.url).pathname==='/internal/prove';
    if(prove){
      console.warn('fixture.enrollment_actor.received');
      let q;try{q=await request.clone().json();}catch{}
      if(q&&Array.isArray(q.headers)&&typeof q.body==='string'){
        const values=name=>q.headers.filter(h=>typeof h?.name==='string'&&h.name.toLowerCase()===name).map(h=>h.value);
        const hosts=values('host');let authority=null;try{authority=new URL(this.fixturePublicOrigin).host;}catch{}
        console.warn('fixture.enrollment_actor.'+(hosts.length===0?'host_absent':hosts.length>1?'host_ambiguous':authority!==null&&hosts[0]===authority?'host_expected':'host_unexpected'));
        for(const [name,tag] of [['transfer-encoding','transfer_encoding'],['content-encoding','content_encoding'],['trailer','trailer']])if(values(name).length)console.warn('fixture.enrollment_actor.'+tag+'_present');
        const length=values('content-length');
        if(length.length)console.warn('fixture.enrollment_actor.'+(length.length===1&&length[0]===String(new TextEncoder().encode(q.body).byteLength)?'length_expected':'length_unexpected'));
      }
    }
    if(request.method==='POST'&&new URL(request.url).pathname==='/internal/abuse'){
      let body;try{body=await request.clone().json();}catch{}
      if(body?.kind==='client_http'&&['provider','realm'].includes(body.scope))console.warn(`fixture.tool_nonce.${body.scope}`);
    }
    let response;try{response=await super.fetch(request);}catch(error){if(prove)console.warn('fixture.enrollment_actor.status_threw');throw error;}
    if(prove)console.warn('fixture.enrollment_actor.'+(response.status===200?'status_200':response.status===409?'status_409':'status_other'));
    return response;
  }
}

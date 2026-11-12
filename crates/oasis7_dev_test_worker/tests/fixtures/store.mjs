import {DurableObject} from 'cloudflare:workers';
import {loadRecord,commitRecord,commitLedger} from '../../platform/store.mjs';
import {readNativeRecord,putNativeRecord} from '../../platform/native.mjs';
export class StoreHarness extends DurableObject {
 async fetch(request){
  const cmd=await request.json(),s=this.ctx;
  try{
   if(cmd.op==='load')return Response.json({value:loadRecord(s,cmd.key)});
   if(cmd.op==='meta')commitRecord(s,'policy-meta',cmd.old,cmd.next);
   if(cmd.op==='ledger')commitLedger(s,cmd.metaOld,cmd.metaNext,cmd.key,cmd.old,cmd.next);
   if(cmd.op==='private')await putNativeRecord(s,this.env,cmd.key,cmd.next);
   if(cmd.op==='readPrivate')return Response.json({value:await readNativeRecord(s,this.env,cmd.key)});
   if(cmd.op==='delete')s.storage.sql.exec('DELETE FROM dev_test_records WHERE id = ?',cmd.key);
   return Response.json({ok:true});
  }catch{return Response.json({unknown:true},{status:409});}
 }
}
export default {fetch:()=>new Response('fixture only',{status:404})};

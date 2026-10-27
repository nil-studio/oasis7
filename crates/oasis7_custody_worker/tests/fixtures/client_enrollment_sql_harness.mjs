// SQLite transaction contract fixture only; never a production authority.
import {DurableObject} from 'cloudflare:workers';
import {readEnrollment,enrollmentTransition} from '../../platform/client_enrollment_sql.mjs';
export class EnrollmentSqlHarness extends DurableObject {
  constructor(ctx,env){super(ctx,env);this.ctx=ctx;}
  async fetch(request){
    const path=new URL(request.url).pathname;
    let observed=null;
    if(path==='/create') enrollmentTransition(this.ctx,'enrollment',(old,inventory)=>{
      if(old!==null) throw new Error('expected absent record');
      observed=JSON.parse(inventory);
      return ['original','1','created'];
    });
    if(path==='/empty') this.ctx.storage.sql.exec('UPDATE custody_records SET payload = ? WHERE id = ?','','enrollment');
    if(path==='/fault'){
      this.ctx.storage.sql.exec("CREATE TRIGGER fail_final BEFORE UPDATE ON custody_records BEGIN SELECT RAISE(ABORT, 'fixture-final-write'); END");
      let failed=false;
      try{enrollmentTransition(this.ctx,'enrollment',()=>['changed','2','must-rollback']);}catch(error){
        if(!error.message.includes('fixture-final-write')) throw error;
        failed=true;
      }finally{this.ctx.storage.sql.exec('DROP TRIGGER fail_final');}
      if(!failed) throw new Error('final write fault did not fire');
    }
    const payload=readEnrollment(this.ctx,'enrollment');
    const tables=this.ctx.storage.sql.exec("SELECT name FROM sqlite_master WHERE type='table' AND name IN ('custody_records','custody_audit') ORDER BY name").toArray();
    const audit=tables.some(row=>row.name==='custody_audit')?this.ctx.storage.sql.exec('SELECT sequence,payload FROM custody_audit ORDER BY sequence').toArray():[];
    return Response.json({payload,tables,audit,observed});
  }
}
export default {fetch(){return new Response(null,{status:404});}};

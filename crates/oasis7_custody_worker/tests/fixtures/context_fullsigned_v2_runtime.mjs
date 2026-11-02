// Isolated synthetic fixture only; actual Rust Actor PUT/read remain untouched.
import actual, {ContextBody as ActualContextBody} from '../../artifacts/native-harness/entry.mjs';
export {ContextFullSignedV2Harness} from '../../artifacts/native-harness/entry.mjs';
export default actual;
export class FullSignedFixtureActor extends ActualContextBody {
  constructor(state,env) { super(state,env); this.fixtureEnv=env; this.fixtureState=state; this.puts=0; }
  async fetch(request) {
    const path=new URL(request.url).pathname;
    if (path==='/fixture/puts') {
      const audit=this.fixtureState.storage.sql.exec('SELECT COUNT(*) AS n FROM custody_audit').one().n;
      return Response.json({puts:this.puts,audit});
    }
    const response=await super.fetch(request);
    if (path==='/internal/fullsigned-v2/put') {
      this.puts++;
      if (response.status===200 && this.puts===1 && this.fixtureEnv.SYNTHETIC_DROP_FULLSIGNED_REPLY==='true') {
        await response.arrayBuffer();
        throw new Error('synthetic committed fullsigned reply lost');
      }
    }
    return response;
  }
}

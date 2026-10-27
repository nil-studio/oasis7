// Test-only fault after Identity has durably retained its complete Root fact.
import deployed, {OwnerNativePermits as RealOwnerNativePermits} from '../../artifacts/owner/entry.mjs';
export * from '../../artifacts/owner/entry.mjs';
export default deployed;
export class OwnerNativePermits extends RealOwnerNativePermits {
  async call(raw) {
    const rpc=JSON.parse(raw);
    if(rpc.action.kind==='record_result' && (
      this.env.FIXTURE_DROP_NATIVE_RESULT==='true' && rpc.action.result.kind==='session_status'
      || this.env.FIXTURE_DROP_MISSING_RESULT==='true' && rpc.action.result.kind==='explicit_login_required')) {
      throw new Error('fixture lost Account submission before commit');
    }
    const result=await super.call(raw);
    if(this.env.FIXTURE_STAGE_GAP==='true' && rpc.action.kind==='record_result' && rpc.action.result.kind==='binding_receipt') {
      await fetch('https://fixture.test/_after-binding-commit',{method:'POST'});
    }
    return result;
  }
}

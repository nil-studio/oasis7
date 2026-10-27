// Test-only fault after Identity has durably retained its complete Root fact.
import deployed, {OwnerNativePermits as RealOwnerNativePermits} from '../../artifacts/owner/entry.mjs';
export * from '../../artifacts/owner/entry.mjs';
export default deployed;
export class OwnerNativePermits extends RealOwnerNativePermits {
  async ownerSetup(raw) {
    const rpc=JSON.parse(raw);
    if(this.env.FIXTURE_SETUP_ARCHIVE_EXPIRY==='final_observation' && rpc.stage==='final_observation' && rpc.action.kind==='record_result') {
      throw new Error('fixture loses original final observation result before Account commit');
    }
    const result=await super.ownerSetup(raw);
    if(rpc.stage===this.env.FIXTURE_SETUP_ARCHIVE_EXPIRY &&
        rpc.action.kind==='archive_canonical' && rpc.action.fact.source==='submit') {
      const response=await fetch('https://fixture.test/_after-setup-submit-archive',{method:'POST'});
      if(response.status!==204) throw new Error('fixture archive expiry failed');
    }
    return result;
  }
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

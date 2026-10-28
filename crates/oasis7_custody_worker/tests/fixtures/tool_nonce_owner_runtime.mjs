// Observe only the fixed metadata named RPC and actual Account route. No fake replies.
import deployed, {OwnerNativePermits as RealOwnerNativePermits,AccountAuthority as RealAccountAuthority} from './native_owner_result_loss.mjs';
export * from './native_owner_result_loss.mjs';
export default deployed;
export class OwnerNativePermits extends RealOwnerNativePermits {
  async clientToolNonce(raw){console.warn('fixture.tool_nonce.owner');return super.clientToolNonce(raw);}
}
export class AccountAuthority extends RealAccountAuthority {
  async fetch(request){
    if(request.method==='POST'&&new URL(request.url).pathname==='/internal/test-session-client-tool-nonce')console.warn('fixture.tool_nonce.account');
    return super.fetch(request);
  }
}

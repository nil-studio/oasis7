// Test-only probes run through actual named service bindings before the legal
// participant workflow consumes the same oracle/callback capacity.
import deployed, {OwnerNativePermits as RealOwnerNativePermits} from '../../artifacts/owner/entry.mjs';
export * from '../../artifacts/owner/entry.mjs';
const observations = {
  identity_sign_bundle_attempts: 0,
  identity_sign_bundle_rejected: false,
  owner_record_policy_attempts: 0,
  owner_record_policy_rejected: false,
};
// This class inherits the production named-entry method without interception.
export class ParticipantIdentityReal extends RealOwnerNativePermits {}
export class OwnerNativePermits extends RealOwnerNativePermits {
  async participant(raw) {
    const rpc = JSON.parse(raw);
    if (rpc.action.kind === 'get_permit' && rpc.action.step === 'policy'
        && observations.identity_sign_bundle_attempts === 0) {
      observations.identity_sign_bundle_attempts++;
      const wrong = JSON.stringify({request:rpc.request,action:{kind:'get_permit',step:'sign_bundle'}});
      try { await this.env.PARTICIPANT_IDENTITY_REAL.participant(wrong); }
      catch { observations.identity_sign_bundle_rejected = true; }
    }
    if (rpc.action.kind === 'record_policy' && observations.owner_record_policy_attempts === 0) {
      observations.owner_record_policy_attempts++;
      // Reuse the complete, real signed policy and current session evidence. A
      // malformed DTO cannot accidentally stand in for the fixed-role rejection.
      try { await this.env.ACCOUNT_KEY_PERMITS.verifyParticipantPermit(raw); }
      catch { observations.owner_record_policy_rejected = true; }
    }
    return super.participant(raw);
  }
}
export default {
  fetch(request, env, ctx) {
    if (new URL(request.url).pathname === '/_fixture/participant-role-probes') {
      return Response.json(observations);
    }
    return deployed.fetch(request, env, ctx);
  },
};

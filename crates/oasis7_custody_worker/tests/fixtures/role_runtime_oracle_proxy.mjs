// Test-only faults around the actual Account oracle. No deployment uses this worker.
import { WorkerEntrypoint } from 'cloudflare:workers';
const reads = new Map();
export class IdentityPermitProxy extends WorkerEntrypoint {
  async verifyKeyPermit(raw) {
    const value = JSON.parse(await this.env.IDENTITY_REAL.verifyKeyPermit(raw));
    if (this.env.FIXTURE_MODE === 'wrong-role') value.scope.context.identity.role = 'executor';
    if (this.env.FIXTURE_MODE === 'wrong-digest') value.request_digest = '00'.repeat(32);
    const permit = JSON.parse(raw).permit_id;
    const count = (reads.get(permit) ?? 0) + 1;
    reads.set(permit, count);
    if (this.env.FIXTURE_MODE === 'freeze-after-seal' && count === 3) {
      await this.env.FENCE.fetch('https://fixture.test/freeze', {method:'POST'});
    }
    return JSON.stringify(value);
  }
}
export default {
  async fetch(request, env) {
    const path = new URL(request.url).pathname;
    const target = path === '/identity-permit' ? env.IDENTITY_REAL : path === '/executor-permit' ? env.EXECUTOR_REAL : undefined;
    if (!target) return new Response(null, {status:404});
    try {
      return new Response(await target.verifyKeyPermit(await request.text()), {headers:{'Content-Type':'application/json'}});
    } catch {
      return Response.json({rejected:true}, {status:409});
    }
  },
};

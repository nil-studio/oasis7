// Test-only trigger of the actual production scheduled handler. Never deployed.
import { WorkerEntrypoint } from 'cloudflare:workers';
import deployed from '../../artifacts/identity/entry.mjs';
export { IdentityNativeApproval, NativeSession } from '../../artifacts/identity/entry.mjs';
export default deployed;
export class ScheduledFixture extends WorkerEntrypoint {
  async tick() { await deployed.scheduled({}, this.env, this.ctx); return 'completed'; }
}

// Test-only trigger of the actual production scheduled handler. Never deployed.
import { WorkerEntrypoint } from 'cloudflare:workers';
import deployed, { RoleKeyProvision as DeployedRoleKeyProvision } from '../../artifacts/identity/entry.mjs';
export { IdentityNativeApproval, IdentityNativeProvision, NativeSession, RoleKeyVault } from '../../artifacts/identity/entry.mjs';
export default deployed;
export class ScheduledFixture extends WorkerEntrypoint {
  async tick() { await deployed.scheduled({}, this.env, this.ctx); return 'completed'; }
}

// Fault injection happens only after the actual Rust vault has committed.
export class RoleKeyProvision extends DeployedRoleKeyProvision {
  async ensureKey(raw) {
    const result = await super.ensureKey(raw);
    if (this.env.FIXTURE_DROP_ROLE_RESPONSE === 'true') throw new Error('fixture lost committed role response');
    return result;
  }
}

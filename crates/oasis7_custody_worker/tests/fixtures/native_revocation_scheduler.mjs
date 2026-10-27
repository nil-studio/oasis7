// Test-only trigger of the actual production scheduled handler. Never deployed.
import { WorkerEntrypoint } from 'cloudflare:workers';
import deployed, { RoleKeyProvision as DeployedRoleKeyProvision, IdentityNativeProvision as DeployedIdentityNativeProvision } from '../../artifacts/identity/entry.mjs';
export { IdentityNativeApproval, NativeProvisionHistory, NativeProvisionRecovery, NativeSession, RoleKeyVault } from '../../artifacts/identity/entry.mjs';
export default deployed;
export class ScheduledFixture extends WorkerEntrypoint {
  async tick() { await deployed.scheduled({}, this.env, this.ctx); return 'completed'; }
}
export class IdentityNativeProvision extends DeployedIdentityNativeProvision {
  async ownerSetup(raw) {
    const result=await super.ownerSetup(raw);
    const selector=JSON.parse(raw),progress=JSON.parse(result);
    if(this.env.FIXTURE_SETUP_STAGE_GAP && selector.stage==='create_task' &&
        progress.stage==='ready_for_setup' && progress.pending_operations.length===0 && progress.blocked_operations.length===0) {
      const reply=await fetch('https://fixture.test/_after-setup-create-commit',{method:'POST'});
      if(reply.status!==204) throw new Error('fixture setup gap failed');
    }
    return result;
  }
}

// Fault injection happens only after the actual Rust vault has committed.
export class RoleKeyProvision extends DeployedRoleKeyProvision {
  async ensureKey(raw) {
    const result = await super.ensureKey(raw);
    if (this.env.FIXTURE_DROP_ROLE_RESPONSE === 'true') throw new Error('fixture lost committed role response');
    return result;
  }
}

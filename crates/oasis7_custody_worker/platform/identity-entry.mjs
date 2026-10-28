import { WorkerEntrypoint } from 'cloudflare:workers';
import RustWorker from './index.js';
export class IdentityNativeProvision extends WorkerEntrypoint {
  async ownerSetup(raw) { return new RustWorker(this.ctx, this.env).identity_run_owner_setup(this.env, raw); }
  async ownerSetupReconcile(raw) { return new RustWorker(this.ctx, this.env).identity_run_owner_setup_reconcile(this.env, raw); }
  async participant(raw) { return new RustWorker(this.ctx, this.env).identity_run_participant(this.env, raw); }
  async run(raw) { return new RustWorker(this.ctx, this.env).identity_run_native(this.env, raw); }
}
export class NativeProvisionHistory extends WorkerEntrypoint {
  async read(raw) { return new RustWorker(this.ctx, this.env).identity_read_native_history(this.env, raw); }
  async missing(raw) { return new RustWorker(this.ctx, this.env).identity_read_missing_native_session(this.env, raw); }
}
export class NativeProvisionRecovery extends WorkerEntrypoint {
  async recover(raw) { return new RustWorker(this.ctx, this.env).identity_recover_native_status(this.env, raw); }
}
export { NativeSession, RoleKeyVault, ClientEnrollmentAuthority } from './index.js';
// Fixed fetch/scheduled callbacks keep generated Rust RPC methods off the default entrypoint.
export default {
  fetch(request, env, ctx) { return new RustWorker(ctx, env).fetch(request); },
  scheduled(controller, env, ctx) { return new RustWorker(ctx, env).native_pump_revocations(env); },
};
export class IdentityNativeApproval extends WorkerEntrypoint {
  async verifyApproval(raw) { return new RustWorker(this.ctx, this.env).native_verify_approval(this.env, raw); }
}
export class RoleKeyProvision extends WorkerEntrypoint {
  async ensureKey(raw) { return new RustWorker(this.ctx, this.env).ensure_role_key(this.env, raw); }
}

export class IdentityClientEnrollments extends WorkerEntrypoint {
  async readProved(enrollmentId) { return new RustWorker(this.ctx,this.env).identity_read_client_enrollment(this.env,enrollmentId); }
}

export class TestSessionPrivateHistory extends WorkerEntrypoint {
  async attest(raw) { return new RustWorker(this.ctx,this.env).identity_attest_test_session(this.env,raw); }
}
export class IdentityTestSessions extends WorkerEntrypoint {
  async preflight(raw) { return new RustWorker(this.ctx,this.env).identity_preflight_test_session(this.env,raw); }
}

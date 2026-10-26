import { WorkerEntrypoint } from 'cloudflare:workers';
import RustWorker from './index.js';
export { NativeSession, RoleKeyVault } from './index.js';
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

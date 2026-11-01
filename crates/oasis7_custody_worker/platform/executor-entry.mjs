import { WorkerEntrypoint } from 'cloudflare:workers';
import RustWorker from './index.js';
export { ExecutorKeys, RoleKeyVault } from './index.js';
export default { fetch(request, env, ctx) { return new RustWorker(ctx, env).fetch(request); } };
export class RoleKeyProvision extends WorkerEntrypoint {
  async executionPrepareV2(raw) { return new RustWorker(this.ctx, this.env).execution_prepare_role_v2(this.env, raw); }
  async ensureKey(raw) { return new RustWorker(this.ctx, this.env).ensure_role_key(this.env, raw); }
}

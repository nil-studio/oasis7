import { WorkerEntrypoint } from 'cloudflare:workers';
import RustWorker from './index.js';
export { ExecutorKeys, RoleKeyVault } from './index.js';
export default { fetch(request, env, ctx) { return new RustWorker(ctx, env).fetch(request); } };
export class RoleKeyProvision extends WorkerEntrypoint {
  async ensureKey(raw) { return new RustWorker(this.ctx, this.env).ensure_role_key(this.env, raw); }
}

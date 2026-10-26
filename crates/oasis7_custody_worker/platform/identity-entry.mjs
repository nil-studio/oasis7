import { WorkerEntrypoint } from 'cloudflare:workers';
import RustWorker from './index.js';
export { NativeSession } from './index.js';
// A fetch-only object keeps generated Rust RPC methods off the default entrypoint.
export default { fetch(request, env, ctx) { return new RustWorker(ctx, env).fetch(request); } };
export class IdentityNativeApproval extends WorkerEntrypoint {
  async verifyApproval(raw) { return new RustWorker(this.ctx, this.env).native_verify_approval(this.env, raw); }
}

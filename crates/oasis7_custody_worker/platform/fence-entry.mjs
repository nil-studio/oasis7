import { WorkerEntrypoint } from 'cloudflare:workers';
import RustWorker from './index.js';
export { RecoveryFence } from './index.js';
export default { fetch(request, env, ctx) { return new RustWorker(ctx, env).fetch(request); } };
// Read only. There is no activation, epoch selection or request-selected namespace.
export class RecoveryFenceReader extends WorkerEntrypoint {
  async read() { return new RustWorker(this.ctx, this.env).read_recovery_fence(this.env); }
}

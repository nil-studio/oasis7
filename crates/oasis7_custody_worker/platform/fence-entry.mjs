import { WorkerEntrypoint } from 'cloudflare:workers';
import RustWorker from './index.js';
export { RecoveryFence } from './index.js';
export default { fetch(request, env, ctx) { return new RustWorker(ctx, env).fetch(request); } };
// Read only. There is no activation, epoch selection or request-selected namespace.
export class RecoveryFenceReader extends WorkerEntrypoint {
  async read() { return new RustWorker(this.ctx, this.env).read_recovery_fence(this.env); }
}
// No ordinary role binds this capability. It additionally requires fixed offline quorum proofs.
export class RecoveryFenceControl extends WorkerEntrypoint {
  async describe() { return new RustWorker(this.ctx,this.env).describe_recovery_fence(this.env); }
  async activate(raw) { return new RustWorker(this.ctx,this.env).activate_recovery_fence(this.env,raw); }
  async disable() { return new RustWorker(this.ctx,this.env).disable_recovery_fence(this.env); }
}

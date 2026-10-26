import { WorkerEntrypoint } from 'cloudflare:workers';
import RustWorker from './index.js';
export { AccountAuthority, ManagerAssociations } from './index.js';
// Default ingress exposes fetch only, never the generated Rust management RPCs.
export default { fetch(request, env, ctx) { return new RustWorker(ctx, env).fetch(request); } };
export class OwnerManagement extends WorkerEntrypoint {
  async reserveDraft(raw) { return new RustWorker(this.ctx, this.env).owner_reserve_draft(this.env, raw); }
  async getDraft(raw) { return new RustWorker(this.ctx, this.env).owner_read_draft(this.env, raw); }
  async freezeManager(raw) { return new RustWorker(this.ctx, this.env).owner_freeze_manager(this.env, raw); }
}

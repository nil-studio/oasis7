import { WorkerEntrypoint } from 'cloudflare:workers';
import RustWorker from './index.js';
export class OwnerNativePermits extends WorkerEntrypoint {
  async call(raw) { return new RustWorker(this.ctx, this.env).owner_native_call(this.env, raw); }
}
export { AccountAuthority, ManagerAssociations, RoleKeyVault } from './index.js';
// Default ingress exposes fetch only, never the generated Rust management RPCs.
export default { fetch(request, env, ctx) { return new RustWorker(ctx, env).fetch(request); } };
export class OwnerManagement extends WorkerEntrypoint {
  async reserveDraft(raw) { return new RustWorker(this.ctx, this.env).owner_reserve_draft(this.env, raw); }
  async getDraft(raw) { return new RustWorker(this.ctx, this.env).owner_read_draft(this.env, raw); }
  async freezeManager(raw) { return new RustWorker(this.ctx, this.env).owner_freeze_manager(this.env, raw); }
  async approveProvision(raw) { return new RustWorker(this.ctx, this.env).owner_approve_provision(this.env, raw); }
  async approveManagedLogin(raw) { return new RustWorker(this.ctx, this.env).owner_approve_managed_login(this.env, raw); }
  async continueManagedLogin(raw) { return new RustWorker(this.ctx, this.env).owner_continue_managed_login(this.env, raw); }
  async advanceProvision(raw) { return new RustWorker(this.ctx, this.env).owner_advance_provision(this.env, raw); }
  async getProvisionProgress(raw) { return new RustWorker(this.ctx, this.env).owner_read_provision_progress(this.env, raw); }
}

export class OwnerRevocations extends WorkerEntrypoint {
  async closeSourceEvent(raw) { return new RustWorker(this.ctx, this.env).owner_close_source_event(this.env, raw); }
}
export class RoleKeyProvision extends WorkerEntrypoint {
  async ensureKey(raw) { return new RustWorker(this.ctx, this.env).ensure_role_key(this.env, raw); }
}
export class IdentityKeyPermits extends WorkerEntrypoint {
  async verifyKeyPermit(raw) { return new RustWorker(this.ctx, this.env).owner_identity_key_permit(this.env, raw); }
}
export class OwnerKeyPermits extends WorkerEntrypoint {
  async verifyKeyPermit(raw) { return new RustWorker(this.ctx, this.env).owner_owner_key_permit(this.env, raw); }
}
export class ExecutorKeyPermits extends WorkerEntrypoint {
  async verifyKeyPermit(raw) { return new RustWorker(this.ctx, this.env).owner_executor_key_permit(this.env, raw); }
}

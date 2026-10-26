// Fixed service-binding transport. Rust owns all DTOs, authentication and policy.
export async function reserveOwnerDraft(env, raw) { return env.OWNER_MANAGEMENT.reserveDraft(raw); }
export async function readOwnerDraft(env, raw) { return env.OWNER_MANAGEMENT.getDraft(raw); }
export async function freezeOwnerManager(env, raw) { return env.OWNER_MANAGEMENT.freezeManager(raw); }
export async function approveOwnerProvision(env, raw) { return env.OWNER_MANAGEMENT.approveProvision(raw); }
export async function approveOwnerManagedLogin(env, raw) { return env.OWNER_MANAGEMENT.approveManagedLogin(raw); }
export async function continueOwnerManagedLogin(env, raw) { return env.OWNER_MANAGEMENT.continueManagedLogin(raw); }
export async function advanceOwnerProvision(env, raw) { return env.OWNER_MANAGEMENT.advanceProvision(raw); }
export async function readOwnerProvisionProgress(env, raw) { return env.OWNER_MANAGEMENT.getProvisionProgress(raw); }

export async function closeSourceEvent(env, raw) { return env.OWNER_REVOCATIONS.closeSourceEvent(raw); }

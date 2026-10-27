// Fixed service-binding transport. Rust owns all DTOs, authentication and policy.
export async function reserveOwnerDraft(env, raw) { return env.OWNER_MANAGEMENT.reserveDraft(raw); }
export async function readOwnerDraft(env, raw) { return env.OWNER_MANAGEMENT.getDraft(raw); }
export async function freezeOwnerManager(env, raw) { return env.OWNER_MANAGEMENT.freezeManager(raw); }
export async function approveOwnerProvision(env, raw) { return env.OWNER_MANAGEMENT.approveProvision(raw); }
export async function approveOwnerManagedLogin(env, raw) { return env.OWNER_MANAGEMENT.approveManagedLogin(raw); }
export async function continueOwnerManagedLogin(env, raw) { return env.OWNER_MANAGEMENT.continueManagedLogin(raw); }
export async function approveOwnerSetup(env, raw) { return env.OWNER_MANAGEMENT.approveOwnerSetup(raw); }
export async function continueOwnerSetup(env, raw) { return env.OWNER_MANAGEMENT.continueOwnerSetup(raw); }
export async function ownerSetupBasis(env, raw) { return env.OWNER_MANAGEMENT.ownerSetupBasis(raw); }
export async function prepareOwnerSetup(env, raw) { return env.OWNER_MANAGEMENT.prepareOwnerSetup(raw); }
export async function advanceOwnerProvision(env, raw) { return env.OWNER_MANAGEMENT.advanceProvision(raw); }
export async function readOwnerProvisionProgress(env, raw) { return env.OWNER_MANAGEMENT.getProvisionProgress(raw); }

export async function closeSourceEvent(env, raw) { return env.OWNER_REVOCATIONS.closeSourceEvent(raw); }

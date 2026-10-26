// Fixed service-binding transport. Rust owns all DTOs, authentication and policy.
export async function reserveOwnerDraft(env, raw) { return env.OWNER_MANAGEMENT.reserveDraft(raw); }
export async function readOwnerDraft(env, raw) { return env.OWNER_MANAGEMENT.getDraft(raw); }
export async function freezeOwnerManager(env, raw) { return env.OWNER_MANAGEMENT.freezeManager(raw); }

// Each binding has one role-specific ensure method; no caller-selected URL or role.
export async function ensureIdentityKey(env, raw) { return env.IDENTITY_PROVISION_KEYS.ensureKey(raw); }
export async function ensureOwnerKey(env, raw) { return env.OWNER_PROVISION_KEYS.ensureKey(raw); }
export async function ensureExecutorKey(env, raw) { return env.EXECUTOR_PROVISION_KEYS.ensureKey(raw); }

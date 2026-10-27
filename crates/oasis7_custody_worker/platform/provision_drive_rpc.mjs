// Each binding has one role-specific ensure method; no caller-selected URL or role.
export async function ensureIdentityKey(env, raw) { return env.IDENTITY_PROVISION_KEYS.ensureKey(raw); }
export async function ensureOwnerKey(env, raw) { return env.OWNER_PROVISION_KEYS.ensureKey(raw); }
export async function ensureExecutorKey(env, raw) { return env.EXECUTOR_PROVISION_KEYS.ensureKey(raw); }
export async function runNativeStage(env, raw) { return env.IDENTITY_NATIVE_PROVISION.run(raw); }
export async function runParticipantWorkflow(env, raw) { return env.IDENTITY_NATIVE_PROVISION.participant(raw); }
export async function readNativeHistory(env, raw) { return env.IDENTITY_NATIVE_HISTORY.read(raw); }
export async function readMissingNativeSession(env, raw) { return env.IDENTITY_NATIVE_HISTORY.missing(raw); }
export async function recoverNativeStatus(env, raw) { return env.IDENTITY_NATIVE_STATUS_RECOVERY.recover(raw); }
export async function runOwnerSetupStage(env, raw) { return env.IDENTITY_NATIVE_PROVISION.ownerSetup(raw); }
export async function runOwnerSetupReconcile(env, raw) { return env.IDENTITY_NATIVE_PROVISION.ownerSetupReconcile(raw); }

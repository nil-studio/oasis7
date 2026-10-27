// Fixed named capabilities only. The caller selects an Account-registered stage.
export async function ownerSetupKeyCall(env, raw) { return env.ACCOUNT_KEY_PERMITS.verifyOwnerSetupPermit(raw); }
export async function ownerSetupIdentityCall(env, raw) { return env.OWNER_NATIVE_PERMITS.ownerSetup(raw); }
export async function ownerSetupSign(env, raw) { return env.OWNER_NATIVE_PERMITS.signOwnerSetup(raw); }
export async function waitOwnerSetupCommit() { await new Promise(resolve=>setTimeout(resolve,250)); }

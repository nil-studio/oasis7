// Independent read-only recovery capabilities; no normal setup write port.
export async function ownerSetupReconcileKeyCall(env, raw) {
  return env.ACCOUNT_KEY_PERMITS.verifyOwnerSetupReconcilePermit(raw);
}
export async function ownerSetupReconcileIdentityCall(env, raw) {
  return env.OWNER_NATIVE_PERMITS.reconcileOwnerSetup(raw);
}
export async function ownerSetupReconcileSign(env, raw) {
  return env.OWNER_NATIVE_PERMITS.signOwnerSetupReconcile(raw);
}

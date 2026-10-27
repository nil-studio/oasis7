// Static bindings only. Rust checks complete stored permits and role/fence context.
export async function verifyKeyPermit(env, raw) {
  return env.ACCOUNT_KEY_PERMITS.verifyKeyPermit(raw);
}
export async function readRecoveryFence(env) {
  return env.RECOVERY_FENCE.read();
}

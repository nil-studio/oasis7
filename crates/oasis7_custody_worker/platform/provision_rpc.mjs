// Fixed transport only. Rust constructs/checks all authority DTOs and pins.
export async function verifyNativeApproval(env, raw) {
  return env.IDENTITY_NATIVE_APPROVAL.verifyApproval(raw);
}
export async function readRecoveryFence(env) {
  return env.RECOVERY_FENCE.read();
}

export async function verifyNativeApprovalTraced(env, raw, stage) { return env.IDENTITY_NATIVE_APPROVAL.verifyApprovalTraced(raw, stage); }

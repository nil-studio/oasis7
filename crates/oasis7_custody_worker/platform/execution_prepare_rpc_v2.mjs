// Existing fixed Executor service binding; no caller-selected URL or retry.
export async function executionPrepareV2(env, raw) {
  return env.EXECUTOR_PROVISION_KEYS.executionPrepareV2(raw);
}

export async function executionDispatchGateV2(env, raw) {
  return env.OWNER_NATIVE_PERMITS.executionDispatchClaimV2(raw);
}

export async function nativeExecutionDispatchGateV2(env,raw) {
  return env.OWNER_NATIVE_PERMITS.nativeExecutionDispatchClaimV2(raw);
}

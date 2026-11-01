// Existing fixed Executor service binding; no caller-selected URL or retry.
export async function executionPrepareV2(env, raw) {
  return env.EXECUTOR_PROVISION_KEYS.executionPrepareV2(raw);
}

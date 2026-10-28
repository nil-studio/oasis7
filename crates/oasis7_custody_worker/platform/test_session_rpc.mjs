// Fixed private capabilities. No caller-selected service or method name.
export async function readTestSessionAttestation(env, raw) {
  if (!env.TEST_SESSION_PRIVATE_HISTORY) throw new Error('test session private history unavailable');
  return env.TEST_SESSION_PRIVATE_HISTORY.attest(raw);
}
export async function runTestSessionPreflight(env, raw) {
  if (!env.IDENTITY_TEST_SESSIONS) throw new Error('test session preflight unavailable');
  return env.IDENTITY_TEST_SESSIONS.preflight(raw);
}
export async function ownerTestSessionCall(env, raw) {
  if (!env.OWNER_NATIVE_PERMITS) throw new Error('test session authority unavailable');
  return env.OWNER_NATIVE_PERMITS.testSession(raw);
}
export async function ownerTestSessionSign(env, raw) {
  if (!env.OWNER_NATIVE_PERMITS) throw new Error('test session signer unavailable');
  return env.OWNER_NATIVE_PERMITS.signTestSessionPreflight(raw);
}
export async function ownerTestSessionKeyCall(env, raw) {
  if (!env.ACCOUNT_KEY_PERMITS) throw new Error('test session key authority unavailable');
  return env.ACCOUNT_KEY_PERMITS.verifyTestSessionPermit(raw);
}

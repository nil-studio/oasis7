// Static capability only. Rust owns all policy, parsing and authority checks.
export async function ownerNativeCall(env, raw) {
  return env.OWNER_NATIVE_PERMITS.call(raw);
}

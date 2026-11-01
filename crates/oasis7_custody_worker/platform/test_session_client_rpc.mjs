// This check compares the actual executing Durable Object ID, not caller JSON.
export function clientAccountActorMatches(state, env, account) {
  if (typeof account !== "string" || account.length === 0 || account.length > 256) return false;
  return state.id.toString() === env.ACCOUNTS.idFromName(account).toString();
}

// Rust callers select from this fixed operation table; no URL or actor name is accepted.
export async function ownerClientCall(env, operation, raw) {
  const binding = env.OWNER_NATIVE_PERMITS;
  switch (operation) {
    case "challenge": return binding.clientChallenge(raw);
    case "tool-nonce": return binding.clientToolNonce(raw);
    case "claim": return binding.clientClaim(raw);
    case "initialize": return binding.clientInitialize(raw);
    case "tools-list": return binding.clientToolsList(raw);
    case "heartbeat": return binding.clientHeartbeat(raw);
    case "finish": return binding.clientFinish(raw);
    case "abort": return binding.clientAbort(raw);
    default: throw new Error("test session client operation unavailable");
  }
}

// Fixed V2 continuation only; public capture and first Admit are unchanged.
export async function ownerCredentialV2Call(env, operation, raw) {
  switch (operation) {
    case "challenge": return env.OWNER_NATIVE_PERMITS.clientV2CredentialChallenge(raw);
    case "claim": return env.OWNER_NATIVE_PERMITS.clientV2CredentialClaim(raw);
    default: throw new Error("test session V2 credentials unavailable");
  }
}

// Fixed paid V2 Control continuation; no URL or role is selected by the caller.
export async function ownerControlV2Call(env, operation, raw) {
 switch(operation){
 case "challenge": return env.OWNER_NATIVE_PERMITS.clientV2ControlChallenge(raw);
 case "initialize": return env.OWNER_NATIVE_PERMITS.clientV2ControlInitialize(raw);
 case "tools-list": return env.OWNER_NATIVE_PERMITS.clientV2ControlToolsList(raw);
 case "heartbeat": return env.OWNER_NATIVE_PERMITS.clientV2ControlHeartbeat(raw);
 default: throw new Error("test session V2 control unavailable");
 }
}

// New N-query/H-Execution profile only; no caller-selected operation or URL.
export async function ownerNativeModelReservationV2(env, raw) {
  return env.OWNER_NATIVE_PERMITS.nativeModelReservationV2(raw);
}

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
    case "claim": return binding.clientClaim(raw);
    case "initialize": return binding.clientInitialize(raw);
    case "tools-list": return binding.clientToolsList(raw);
    case "heartbeat": return binding.clientHeartbeat(raw);
    case "finish": return binding.clientFinish(raw);
    case "abort": return binding.clientAbort(raw);
    default: throw new Error("test session client operation unavailable");
  }
}

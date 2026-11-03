// Harness-only fixed Owner RPC. Actual Account Actor remains behind this hop.
export async function callPendingExecutionComponent(env,raw){
 if(typeof raw!=='string'||new TextEncoder().encode(raw).byteLength>65536)throw new Error('component bound');
 return env.OWNER_NATIVE_PERMITS.pendingExecutionFirst(raw);
}

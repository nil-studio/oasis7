import { beforeEach, expect, it, vi } from "vitest";
vi.mock("./viewer_auth_crypto.js", async () => ({ ...(await vi.importActual("./viewer_auth_crypto.js")), signAuthPayload: vi.fn(async () => `awviewauth:v1:${"cd".repeat(64)}`) }));
import { signAuthPayload } from "./viewer_auth_crypto.js";
import { createSafeAlternativeModule } from "./viewer_safe_alternative_module.js";
beforeEach(() => { sessionStorage.clear(); vi.clearAllMocks(); });
function fixture() {
 const state = { auth: { publicKey: "ab".repeat(32), playerId:"p", boundAgentId:"a" }, worldFeed:{worldId:"w",reorgEpoch:0,stale:false}, viewerProtocol:{capabilities:["canonical_agent_chat_v1"]}, canonicalOwnerWorldIdentity:{world_id:"w",genesis_digest:"g"} };
 state.canonicalAgentOwnerView={agent_id:"a",player_id:"p",public_key:state.auth.publicKey,world_id:"w",reorg_epoch:0,canonical_authority:{branch_id:"b",agent_identity_generation:1}};
 const sendJson=vi.fn(); const module=createSafeAlternativeModule({state,sendJson,render:vi.fn(),refreshOwner:vi.fn(async()=>{})}); return {state,sendJson,module};
}
const propose={operation:"propose",data:{request_id:"r",original:{goal:"via route",waypoints:[{x_cm:1,y_cm:0,z_cm:0}],arrival_tick:10},valid_until_tick:20}};
it("unknown blocks new consent; reconnect recovers the exact signed original",async()=>{
 const f=fixture(); await f.module.command(propose); const sent=f.sendJson.mock.calls[0][0]; expect(sent.request.signature_hex).toBe("cd".repeat(64));
 await expect(f.module.command({operation:"confirm",data:{}})).rejects.toThrow("Recover"); expect(signAuthPayload).toHaveBeenCalledTimes(1);
 const re=fixture(); re.module.recover(); expect(re.sendJson).toHaveBeenCalledWith({...sent,recover:true}); await expect(re.module.command(propose)).rejects.toThrow("Recover");
});
it("invalid coordinates stop before signing; a proposal receipt never sends confirmation",async()=>{
 const f=fixture(); await expect(f.module.command({...propose,data:{...propose.data,original:{...propose.data.original,waypoints:[{x_cm:1.5,y_cm:0,z_cm:0}]}}})).rejects.toThrow("coordinates"); expect(signAuthPayload).not.toHaveBeenCalled();
 await f.module.command(propose); const sent=f.sendJson.mock.calls[0][0];
 f.module.receive({request:sent.request,result:{outcome:{status:"committed",receipt:{request_id:"r",agent_id:"a",state:"awaiting_confirmation",candidates:[{candidate_id:"c"}]}}}});
 expect(f.state.safeAlternative.status).toBe("awaiting_confirmation"); expect(f.sendJson).toHaveBeenCalledTimes(1); expect(signAuthPayload).toHaveBeenCalledTimes(1);
});

it("never displays a late result after signer or world authority changes", async () => {
 const f=fixture(); await f.module.command(propose); const sent=f.sendJson.mock.calls[0][0];
 f.state.auth.publicKey="ef".repeat(32);
 f.module.receive({request:sent.request,result:{outcome:{status:"committed",receipt:{request_id:"r",agent_id:"a",state:"applied"}}}});
 expect(f.state.safeAlternative.record).toBeNull(); expect(f.state.safeAlternative.status).toBe("unknown");
});
it("retries exactly the original without re-signing", async () => {
 const f=fixture(); await f.module.command(propose); const sent=f.sendJson.mock.calls[0][0];
 f.module.recover(true); expect(f.sendJson.mock.calls[1][0]).toEqual({...sent,recover:false});
 expect(signAuthPayload).toHaveBeenCalledTimes(1);
});

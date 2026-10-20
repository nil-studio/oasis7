import { afterEach, describe, expect, it, vi } from "vitest";
import { cborCanonicalEncode } from "./viewer_auth_crypto.js";
import { canonicalAgentChatView, createCanonicalOwnerReadModule } from "./viewer_canonical_owner_read_module.js";
afterEach(() => vi.useRealTimers());
const bytesHex = value => Array.from(cborCanonicalEncode(value), x => x.toString(16).padStart(2, "0")).join("");
function fixture() {
  const state = {auth:{available:true,registrationStatus:"registered",boundAgentId:"a",playerId:"p",publicKey:"ab".repeat(32)},
    worldFeed:{worldId:"w",reorgEpoch:2,stale:false},viewerProtocol:{capabilities:["canonical_agent_chat_v1"]},
    snapshot:{player_gameplay:{canonical_agent_chat:{goal:{message:"untrusted public goal"}}}}};
  const fence={agent_id:"a",player_id:"p",public_key:state.auth.publicKey,world_id:"w",reorg_epoch:2,canonical_authority:{branch_id:"b",agent_identity_generation:3},current_intent_id:null};
  const commit={world:{world_id:"w",genesis_digest:"opaque-genesis-ref"},binding:{branch_id:"b",reorg_generation:2,provider_world_id:"w",finality_ref:"f",governing_manifest_ref:"m",authority_generation:1,permission_generation:1},position:12,execution_block_hash:"block",state_root_ref:"root"};
  const request={contract_version:1,world:commit.world,scope_id:"agent:a",min_commit:commit,fixed_commit:null,deadline_unix_ms:null};
  const context={request,fence,expected:{...fence,branch_id:fence.canonical_authority.branch_id},signing_domain:"/v1/world/view",signing_bytes_hex:bytesHex(["oasis7.world-service.v1","/v1/world/view",request])};
  const sendJson=vi.fn();const signAuthPayload=vi.fn(async()=>`awviewauth:v1:${"cd".repeat(64)}`);
  const module=createCanonicalOwnerReadModule({state,sendJson,signAuthPayload,render:vi.fn(),getSocket:()=>({readyState:WebSocket.OPEN})});
  return {state,fence,commit,context,module,sendJson,signAuthPayload};
}
describe("browser-signed canonical owner read",()=>{
  it.each([2, "2"])("uses original browser signing identity with feed epoch %s and publishes only the correlated private read",async epoch=>{
    const f=fixture();f.state.worldFeed.reorgEpoch=epoch;const pending=f.module.refresh("a");
    expect(canonicalAgentChatView(f.state)).toBeNull();
    expect(f.sendJson).toHaveBeenCalledWith({type:"request_canonical_agent_owner_read_context",agent_id:"a"});
    await f.module.handleContext(f.context);
    const sent=f.sendJson.mock.calls[1][0];
    expect(sent.request.subject_public_key).toBe(f.state.auth.publicKey);
    expect(sent.request.signature_hex).toBe("cd".repeat(64));
    expect(bytesHex(f.context.request)).toBe(bytesHex(sent.request.request));
    const view={...f.fence,goal:{intent_id:"intent",message:"owner-private goal"}};
    f.module.handleView({request:sent.request,request_digest:"blake3:"+"ef".repeat(32),view,version:{commit:f.commit}});
    expect((await pending).goal.message).toBe("owner-private goal");
    expect(canonicalAgentChatView(f.state).goal.message).toBe("owner-private goal");
    expect(f.state.snapshot.player_gameplay.canonical_agent_chat.goal.message).toBe("untrusted public goal");
    f.module.reset();expect(canonicalAgentChatView(f.state)).toBeNull();
  });
  it("bootstraps from a public commit without inventing owner generation, then accepts the verified private generation",async()=>{
    const f=fixture();f.context.fence=null;
    delete f.context.expected.canonical_authority;
    delete f.context.expected.current_intent_id;
    const pending=f.module.refresh("a");await f.module.handleContext(f.context);
    const sent=f.sendJson.mock.calls[1][0];
    expect(sent.request.request).toEqual(f.context.request);
    expect(JSON.stringify(sent)).not.toContain("agent_identity_generation");
    f.module.handleView({request:sent.request,request_digest:"blake3:"+"ef".repeat(32),view:{...f.fence,goal:null},version:{commit:f.commit}});
    expect((await pending).canonical_authority.agent_identity_generation).toBe(3);
  });
  it("rejects a private zero-generation or wrong-branch response after public-context bootstrap",async()=>{
    for(const mutation of [v=>{v.canonical_authority.agent_identity_generation=0},v=>{v.canonical_authority.branch_id="foreign"}]) {
      const f=fixture();f.context.fence=null;const pending=f.module.refresh("a");const rejected=expect(pending).rejects.toThrow();
      await f.module.handleContext(f.context);const sent=f.sendJson.mock.calls[1][0];const view=structuredClone({...f.fence,goal:null});mutation(view);
      f.module.handleView({request:sent.request,request_digest:"blake3:"+"ef".repeat(32),view,version:{commit:f.commit}});
      await rejected;expect(canonicalAgentChatView(f.state)).toBeNull();
    }
  });
  it("does not sign foreign context, arbitrary signing bytes, or stale identity",async()=>{
    for(const mutation of [f=>{f.context.fence.public_key="other"},f=>{f.context.signing_bytes_hex="00"},f=>{f.state.auth.playerId="changed"}]) {
      const f=fixture();const pending=f.module.refresh("a");const rejected=expect(pending).rejects.toThrow();mutation(f);
      await f.module.handleContext(f.context);await rejected;expect(f.signAuthPayload).not.toHaveBeenCalled();
    }
  });
  it("rejects old signed envelope echoes and below-minimum commits",async()=>{
    for(const mutation of [m=>{m.request.signature_hex="ff".repeat(64)},m=>{m.version.commit.position=11}]) {
      const f=fixture();const pending=f.module.refresh("a");const rejected=expect(pending).rejects.toThrow();
      await f.module.handleContext(f.context);const signed=f.sendJson.mock.calls[1][0].request;
      const message=structuredClone({request:signed,request_digest:"blake3:"+"ef".repeat(32),view:{...f.fence,goal:null},version:{commit:f.commit}});mutation(message);
      f.module.handleView(message);await rejected;expect(canonicalAgentChatView(f.state)).toBeNull();
    }
  });
  it.each([null, "", "1e0", "0x2", -1, "9007199254740993"])("rejects invalid or unsafe feed epoch %s before signing", async epoch => {
    const f=fixture();f.state.worldFeed.reorgEpoch=epoch;
    await expect(f.module.refresh("a")).rejects.toThrow();
    expect(f.signAuthPayload).not.toHaveBeenCalled();
    f.state.canonicalAgentOwnerView={...f.fence};
    expect(canonicalAgentChatView(f.state)).toBeNull();
  });

});

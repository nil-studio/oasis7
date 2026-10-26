import { describe, expect, it } from "vitest";
import { fixture, sessionFailure } from "./test_support/browser_owner_entry_fixture.js";

const goal = "Build the original workshop";
const mutationCalls = (f) => f.calls.filter(c => c.path === "/participant/submit" || c.body?.operation?.kind === "submit");
describe("explicit continuation of definitely unsent entry originals", () => {
 it.each(["registrationRaw", "taskRaw", "publishRaw"])("restores %s and reaches a ready task with identical originals", async slot => {
  const f = fixture(), entry = f.create({ fetchImpl: sessionFailure(f, slot) });
  expect(await entry.prepare({ goal, consent: true })).toMatchObject({ state: "unsent" });
  const record = [...f.records.values()].find(r => r[slot]);
  const original = record[slot], count = mutationCalls(f).length;
  expect(await f.create().inspect()).toMatchObject({ state: "unsent" });
  expect(await f.create().prepare({ goal, consent: true })).toMatchObject({ state: "unsent" });
  expect(mutationCalls(f)).toHaveLength(count);
  await expect(f.create().continueOriginal({ goal })).rejects.toMatchObject({ code: "owner_consent_required" });
  const ready = await f.create().continueOriginal({ goal, consent: true });
  expect(ready).toMatchObject({ state: "ready", agentId: "canonical-agent", taskId: "canonical-task" });
  expect([...f.records.values()].find(r => r[slot])[slot]).toBe(original);
  const calls = mutationCalls(f);
  expect(calls).toHaveLength(3);
  const sent = slot === "registrationRaw" ? calls[0] : calls[slot === "taskRaw" ? 1 : 2];
  expect(sent.rawBody).toContain(slot === "registrationRaw" ? original : `"request":${original}`);
  // The next entry action is available without creating another participant/task.
  expect(await f.create().inspect()).toMatchObject({ state: "ready" });
  expect(await f.create().prepare({ goal: "Do not replace", consent: true })).toMatchObject({ state: "ready" });
  expect(mutationCalls(f)).toHaveLength(3);
 });
});

it.each(["registrationRaw", "taskRaw", "publishRaw"])("keeps legacy/possibly sent %s lookup-only, including explicit continuation", async slot=>{
 const f=fixture();await f.create({fetchImpl:sessionFailure(f,slot)}).prepare({goal,consent:true});
 const record=[...f.records.values()].find(r=>r[slot]);record.schema=1;
 const before=mutationCalls(f).length;
 expect(await f.create().inspect()).toMatchObject({state:"pending"});
 expect(await f.create().continueOriginal({goal,consent:true})).toMatchObject({state:"pending"});
 expect(mutationCalls(f)).toHaveLength(before);
});
it.each(["registrationRaw", "taskRaw", "publishRaw"])("atomically claims %s once across competing tabs", async slot=>{
 const f=fixture();await f.create({fetchImpl:sessionFailure(f,slot)}).prepare({goal,consent:true});
 const results=await Promise.allSettled([f.create().continueOriginal({goal,consent:true}),f.create().continueOriginal({goal,consent:true})]);
 expect(results.some(r=>r.status==="fulfilled"&&r.value.state==="ready")).toBe(true);
 expect(mutationCalls(f)).toHaveLength(3);
 expect(await f.create().inspect()).toMatchObject({state:"ready"});
});
it.each(["registrationRaw", "taskRaw", "publishRaw"])("keeps %s possibly sent after Stop immediately after the durable claim", async slot=>{
 const f=fixture();await f.create({fetchImpl:sessionFailure(f,slot)}).prepare({goal,consent:true});
 const abort=new AbortController(), count=mutationCalls(f).length;
 const field=slot==="registrationRaw"?"registrationSendState":`${slot}SendState`;
 const storage={get:f.storage.get,update:async(key,mutate)=>{const result=await f.storage.update(key,mutate);if(result[field]==="may_have_sent")abort.abort();return result;}};
 await expect(f.create({storage,signal:abort.signal}).continueOriginal({goal,consent:true})).rejects.toMatchObject({code:"participant_stopped"});
 expect(mutationCalls(f)).toHaveLength(count);
 expect([...f.records.values()].find(r=>r[slot])[field]).toBe("may_have_sent");
 expect(await f.create().inspect()).toMatchObject({state:"pending"});
 expect(await f.create().continueOriginal({goal,consent:true})).toMatchObject({state:"pending"});
 expect(mutationCalls(f)).toHaveLength(count);
});
it("does not claim an original when the authenticated account changes at the session gate",async()=>{
 const f=fixture();await f.create({fetchImpl:sessionFailure(f,"registrationRaw")}).prepare({goal,consent:true});
 const fetchImpl=(url,options)=>url.endsWith("/session")?Promise.resolve(new Response(JSON.stringify({api_version:2,ok:true,account:{account_id:"another-account"},authentication:{state:"authenticated"},csrf_token:"aa".repeat(32)}))):f.fetchImpl(url,options);
 await expect(f.create({fetchImpl}).continueOriginal({goal,consent:true})).rejects.toMatchObject({code:"participant_account_changed"});
 expect(mutationCalls(f)).toHaveLength(0);
 expect(await f.create().inspect()).toMatchObject({state:"unsent"});
});

it("does not create a fresh participant or task from a stale Continue action",async()=>{
 const f=fixture();
 expect(await f.create().continueOriginal({goal,consent:true})).toMatchObject({state:"needs_participant"});
 expect(mutationCalls(f)).toHaveLength(0);
 expect([...f.records.values()].some(r=>r.registrationRaw||r.taskRaw)).toBe(false);
});
it.each(["registrationRaw","taskRaw","publishRaw"])("rejects contradictory/invalid durable %s markers",async slot=>{
 const f=fixture();await f.create({fetchImpl:sessionFailure(f,slot)}).prepare({goal,consent:true});
 const record=[...f.records.values()].find(r=>r[slot]);
 if(slot==="registrationRaw")record.sent=true;else record[`${slot}SendState`]="sent_maybe_no";
 const count=mutationCalls(f).length;
 const code=slot==="registrationRaw"?"participant_storage_corrupt":"owner_storage_corrupt";
 await expect(f.create().inspect()).rejects.toMatchObject({code});
 await expect(f.create().continueOriginal({goal,consent:true})).rejects.toMatchObject({code});
 expect(mutationCalls(f)).toHaveLength(count);
});
it.each(["registrationRaw","taskRaw","publishRaw"])("keeps missing %s phase conservative rather than offering Continue",async slot=>{
 const f=fixture();await f.create({fetchImpl:sessionFailure(f,slot)}).prepare({goal,consent:true});
 const record=[...f.records.values()].find(r=>r[slot]);delete record[slot==="registrationRaw"?"registrationSendState":`${slot}SendState`];
 const count=mutationCalls(f).length;
 expect(await f.create().inspect()).toMatchObject({state:"pending"});
 expect(await f.create().continueOriginal({goal,consent:true})).toMatchObject({state:"pending"});
 expect(mutationCalls(f)).toHaveLength(count);
});
it.each(["registrationRaw","taskRaw","publishRaw"])("keeps %s unsent after Stop during the session gate and permits an explicit new-generation continuation",async slot=>{
 const f=fixture();await f.create({fetchImpl:sessionFailure(f,slot)}).prepare({goal,consent:true});
 const abort=new AbortController();
 const fetchImpl=async(url,options)=>{if(url.endsWith("/session"))abort.abort();return f.fetchImpl(url,options);};
 const count=mutationCalls(f).length;
 await expect(f.create({fetchImpl,signal:abort.signal}).continueOriginal({goal,consent:true})).rejects.toMatchObject({code:"participant_stopped"});
 expect(mutationCalls(f)).toHaveLength(count);
 expect(await f.create().inspect()).toMatchObject({state:"unsent"});
 expect(await f.create().continueOriginal({goal,consent:true})).toMatchObject({state:"ready"});
});

it.each(["registrationRaw","taskRaw","publishRaw"])("does not POST %s when the durable send claim fails",async slot=>{
 const f=fixture();await f.create({fetchImpl:sessionFailure(f,slot)}).prepare({goal,consent:true});
 const field=slot==="registrationRaw"?"registrationSendState":`${slot}SendState`,count=mutationCalls(f).length;
 const storage={get:f.storage.get,update:(key,mutate)=>f.storage.update(key,r=>{const next=mutate(r);if(next[field]==="may_have_sent")throw new Error("private storage detail");return next;})};
 expect(await f.create({storage}).continueOriginal({goal,consent:true})).toMatchObject({state:"unsent"});
 expect(mutationCalls(f)).toHaveLength(count);
 expect([...f.records.values()].find(r=>r[slot])[field]).toBe("not_sent");
 expect(await f.create().continueOriginal({goal,consent:true})).toMatchObject({state:"ready"});
});
it.each(["registrationRaw","taskRaw","publishRaw"])("does not continue %s from a late scope generation",async slot=>{
 const f=fixture();await f.create({fetchImpl:sessionFailure(f,slot)}).prepare({goal,consent:true});
 let current=true;const count=mutationCalls(f).length;
 const fetchImpl=async(url,options)=>{if(url.endsWith("/session"))current=false;return f.fetchImpl(url,options);};
 await expect(f.create({fetchImpl,isCurrent:()=>current}).continueOriginal({goal,consent:true})).rejects.toMatchObject({code:"participant_stopped"});
 expect(mutationCalls(f)).toHaveLength(count);
 expect(await f.create().inspect()).toMatchObject({state:"unsent"});
});
it.each(["game_participant_register","game_task_create","game_goal_publish"])("reconciles a lost %s submit response without resending",async kind=>{
 const f=fixture();if(kind==="game_participant_register")f.setUnknown(true);else f.setOwnerUnknown(kind);
 const fetchImpl=async(url,options)=>{const response=await f.fetchImpl(url,options);const body=options.body?JSON.parse(options.body):{};
  if((url.endsWith("/participant/submit")&&kind==="game_participant_register")||(body.operation?.kind==="submit"&&body.operation.request.signed_payload.kind===kind))throw new Error("response lost");return response;};
 expect(await f.create({fetchImpl}).prepare({goal,consent:true})).toMatchObject({state:"pending"});
 const count=mutationCalls(f).length;
 expect(await f.create().continueOriginal({goal,consent:true})).toMatchObject({state:"pending"});
 expect(mutationCalls(f)).toHaveLength(count);
 f.setUnknown(false);f.setOwnerUnknown(null);
 const recovered=await f.create().inspect();expect(["needs_task","needs_goal","ready"]).toContain(recovered.state);
 expect(mutationCalls(f)).toHaveLength(count);
});

it("never recreates a participant original removed between continuation reads",async()=>{
 const f=fixture();await f.create({fetchImpl:sessionFailure(f,"registrationRaw")}).prepare({goal,consent:true});
 let reads=0,signatures=0;
 const storage={update:f.storage.update,get:async key=>{const record=await f.storage.get(key);if(key.startsWith("participant:")&&++reads===2)f.records.delete(key);return record;}};
 const ownerSigner={publicKeyHex:"44".repeat(32),sign:async()=>{signatures++;return new Uint8Array(64);}};
 await expect(f.create({storage,ownerSigner}).continueOriginal({goal,consent:true})).rejects.toMatchObject({code:"participant_original_missing"});
 expect(signatures).toBe(0);expect(mutationCalls(f)).toHaveLength(0);
 expect([...f.records.values()].some(r=>r.registrationRaw)).toBe(false);
});

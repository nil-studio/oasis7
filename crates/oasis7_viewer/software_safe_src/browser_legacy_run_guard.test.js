import { describe, expect, it, vi } from "vitest";
import { assertNoLegacyRun } from "./browser_legacy_run_guard.js";
const client = (cap = { ok:true,enabled:true,authenticated:true,legacy_execution_policy:"management_only" }, items = []) => ({capabilities:vi.fn(async()=>cap),list:vi.fn(async()=>items)});
describe("legacy execution migration guard", () => {
 it("allows only explicit deployment disablement without listing", async()=>{
  const api=client({ok:true,enabled:false,reason:"deployment_unavailable"}); await assertNoLegacyRun({client:api}); expect(api.list).not.toHaveBeenCalled();
 });
 it("requires a complete enabled authenticated list", async()=>{
  const api=client(undefined,[{state:"ready",run_state:"stopped",execution:{phase:"finished"}}]); await assertNoLegacyRun({client:api}); expect(api.list).toHaveBeenCalledOnce();
  for(const cap of [{ok:true,enabled:false},{ok:true,enabled:true,authenticated:false},{enabled:false}]) await expect(assertNoLegacyRun({client:client(cap)})).rejects.toMatchObject({code:"legacy_state_unknown"});
 });
 it("blocks both live run projections", async()=>{
  for(const item of [{state:"ready",run_state:"running"},{state:"ready",run_state:"stopped",execution:{phase:"running"}}]) await expect(assertNoLegacyRun({client:client(undefined,[item])})).rejects.toMatchObject({code:"legacy_run_active"});
 });
 it("does not interpret network/list corruption as no legacy run", async()=>{
  const api=client(); api.list.mockRejectedValue(new Error("network")); await expect(assertNoLegacyRun({client:api})).rejects.toMatchObject({code:"legacy_state_unknown"});
  api.capabilities.mockRejectedValue(new Error("network")); await expect(assertNoLegacyRun({client:api})).rejects.toMatchObject({code:"legacy_state_unknown"});
  for(const items of [null,[{}],[{state:"ready",run_state:"unknown"}]]) await expect(assertNoLegacyRun({client:client(undefined,items)})).rejects.toMatchObject({code:"legacy_state_unknown"});
 });
});

it("rejects an empty session list when the server can still start legacy workers", async () => {
 for (const policy of ["session_execution", "unrecognized", undefined]) {
  const api=client({ok:true,enabled:true,authenticated:true,legacy_execution_policy:policy},[]);
  await expect(assertNoLegacyRun({client:api})).rejects.toMatchObject({code:"legacy_migration_required"});
  expect(api.list).not.toHaveBeenCalled();
 }
});

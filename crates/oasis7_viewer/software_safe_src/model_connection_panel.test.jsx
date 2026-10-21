import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ModelConnectionPanel } from "./model_connection_panel.jsx";
function capabilities() { return { ok: true, enabled: true, authenticated: true, trusted_executor: { trusted: true, display_name: "Trusted game executor", location: "Game deployment", origin: location.origin }, presets: [{ id: "deepseek-v1", provider_id: "deepseek", model_id: "deepseek-chat", protocol: "chat_completions" }], executor_available: false, limits: { max_calls: 3, max_tokens: 1000, max_input_tokens: 500, max_output_tokens: 500, max_concurrent: 1, lease_ms: 120000, absolute_ms: 7200000, renew_interval_ms: 30000 } }; }
const connection = () => ({ id: "connection-1", version: 1, preset_id: "deepseek-v1", state: "ready", run_state: "idle", run_generation: 0, expires_at_ms: Date.now() + 120000, absolute_expires_at_ms: Date.now() + 7200000, calls_spent: 0, tokens_reserved: 0 });
function client(items = [connection()], cap = capabilities()) { return { capabilities: vi.fn(async () => cap), list: vi.fn(async () => items), create: vi.fn(async () => connection()), probe: vi.fn(async (value) => ({ ...value, version: value.version + 1 })), activate: vi.fn(), lease: vi.fn(async (value) => ({ ...value, version: value.version + 1, expires_at_ms: Date.now() + 120000 })), stop: vi.fn(async (value) => ({ ...value, version: value.version + 1, run_state: "stopped" })), forget: vi.fn(async (value) => ({ ...value, version: value.version + 1, state: "revoked", run_state: "stopped" })) }; }
function panel(api, locale = "en") { const result = render(() => <ModelConnectionPanel client={api} locale={locale} agentId="a" canControl={true} />); result.container.querySelector("details").open = true; return result; }
afterEach(() => { cleanup(); vi.useRealTimers(); });
describe("model connection panel", () => {
 it("does not mount a key field for disabled, unauthenticated or untrusted deployments", async () => {
  for (const patch of [{ enabled: false }, { authenticated: false }, { trusted_executor: { trusted: false } }]) {
   const api = client([], { ...capabilities(), ...patch }); const result = panel(api);
   await waitFor(() => expect(api.capabilities).toHaveBeenCalled());
   expect(result.container.querySelector("input[type=password]")).toBeNull();
   expect(api.list).not.toHaveBeenCalled(); cleanup();
  }
 });
 it("separates ready from running and blocks start when no game executor exists", async () => {
  const api = client(); const result = panel(api);
  await waitFor(() => expect(result.container.textContent).toContain("Model ready"));
  expect(result.getByText(/No game executor is available/)).toBeTruthy();
  expect(result.getByRole("button", { name: "Confirm start" }).disabled).toBe(true);
  expect(result.getByRole("button", { name: "Test connection" }).disabled).toBe(true);
  fireEvent.click(result.getByLabelText(/Consent to a bounded/));
  fireEvent.click(result.getByRole("button", { name: "Test connection" }));
  await waitFor(() => expect(api.probe).toHaveBeenCalledTimes(1));
  expect(api.activate).not.toHaveBeenCalled();
 });
 it("shows an active run without also claiming it has not started", async () => {
  const api = client([{ ...connection(), run_state: "running", execution: { phase: "running" } }], { ...capabilities(), executor_available: true });
  const result = panel(api);
  await waitFor(() => expect(result.container.textContent).toContain("Executor running"));
  expect(result.container.textContent).toContain("Run in progress");
  expect(result.container.textContent).not.toContain("not started");
 });
 it("clears the password synchronously and never renders a failed key or upstream error", async () => {
  const api = client([]); api.create.mockRejectedValue(new Error("canary-private-key")); const result = panel(api);
  await waitFor(() => expect(result.getByLabelText("Inference API key")).toBeTruthy());
  const input = result.getByLabelText("Inference API key");
  fireEvent.change(result.getByLabelText("Provider and model"), { target: { value: "deepseek-v1" } });
  fireEvent.input(input, { target: { value: "canary-private-key" } });
  fireEvent.click(result.getByLabelText(/I understand the recipient/));
  fireEvent.submit(result.container.querySelector("form"));
  expect(input.value).toBe("");
  await waitFor(() => expect(api.create).toHaveBeenCalledTimes(1));
  expect(api.probe).not.toHaveBeenCalled(); expect(api.activate).not.toHaveBeenCalled();
  await waitFor(() => expect(result.getByRole("alert").textContent).toContain("connection service"));
  expect(result.container.textContent).not.toContain("canary-private-key");
 });
 it("keeps Stop and Forget distinct and refreshes CAS conflicts without automatic reactivation", async () => {
  const api = client(); api.stop.mockRejectedValueOnce({ code: "version_conflict" }); const result = panel(api, "zh");
  await waitFor(() => expect(result.getByRole("button", { name: "停止新决策" })).toBeTruthy());
  fireEvent.click(result.getByRole("button", { name: "停止新决策" }));
  await waitFor(() => expect(api.list).toHaveBeenCalledTimes(2));
  expect(result.getByRole("alert").textContent).toContain("其他页面");
  expect(api.activate).not.toHaveBeenCalled(); expect(api.forget).not.toHaveBeenCalled();
  fireEvent.click(result.getByRole("button", { name: "断开并忘记" }));
  await waitFor(() => expect(api.forget).toHaveBeenCalledTimes(1));
  expect(api.forget.mock.calls[0][0]).toMatchObject({ id: "connection-1", version: 1 });
  await waitFor(() => expect(result.container.textContent).toContain("已断开并忘记 · 已停止新决策"));
  expect(result.container.textContent).not.toContain("会话已过期");
 });
 it("does not renew or revive a session that expired while the page was hidden", async () => {
  vi.useFakeTimers(); vi.setSystemTime(new Date("2026-10-16T00:00:00Z"));
  const descriptor = Object.getOwnPropertyDescriptor(document, "visibilityState");
  Object.defineProperty(document, "visibilityState", { configurable: true, value: "hidden" });
  const api = client(); const result = panel(api);
  await vi.advanceTimersByTimeAsync(0);
  await vi.advanceTimersByTimeAsync(121000);
  expect(api.lease).not.toHaveBeenCalled();
  Object.defineProperty(document, "visibilityState", { configurable: true, value: "visible" });
  document.dispatchEvent(new Event("visibilitychange"));
  await vi.advanceTimersByTimeAsync(0); await vi.advanceTimersByTimeAsync(1000);
  expect(api.lease).not.toHaveBeenCalled(); expect(api.activate).not.toHaveBeenCalled();
  expect(result.container.textContent).toContain("Session expired");
  if (descriptor) Object.defineProperty(document, "visibilityState", descriptor); else delete document.visibilityState;
 });
});

it("shows safe bilingual output-limit recovery without exposing diagnostic codes", async () => {
 for (const [locale, label, guidance] of [["en", "Model output reached its limit", "Truncated output is not an executable action"], ["zh", "模型输出达到上限", "截断输出不能作为可执行动作"]]) {
  const api = client([{ ...connection(), run_state: "stopped", execution: { phase: "finished", reason: "model_failed", model_failure: "output_limit" } }]);
  const result = panel(api, locale);
  await waitFor(() => expect(result.container.textContent).toContain(label));
  expect(result.container.textContent).toContain(guidance);
  expect(result.container.textContent).not.toContain("output_limit");
  expect(api.activate).not.toHaveBeenCalled(); cleanup();
 }
});
it("keeps legacy failures generic and Stop ahead of stale model diagnostics", async () => {
 for (const execution of [{ phase: "finished", reason: "model_failed" }, { phase: "finished", reason: "model_failed", model_failure: "private-canary" }, { phase: "stopped", reason: "control_changed", model_failure: "output_limit" }]) {
  const result = panel(client([{ ...connection(), run_state: execution.phase === "stopped" ? "stopped" : "idle", execution }]));
  await waitFor(() => expect(result.container.textContent).toContain(execution.phase === "stopped" ? "Authority or run state changed" : "Model call did not complete"));
  expect(result.container.textContent).not.toContain("Model output reached its limit");
  expect(result.container.textContent).not.toContain("private-canary"); cleanup();
 }
});

it("hides stale failure details in revoked, expired or stopped-phase snapshots", async () => {
 for (const state of ["revoked", "expired"]) {
  const result = panel(client([{ ...connection(), state, run_state: "stopped", execution: { phase: "finished", reason: "model_failed", model_failure: "output_limit" } }]));
  await waitFor(() => expect(result.container.textContent).toContain(state === "revoked" ? "Disconnected and forgotten" : "Session expired"));
  expect(result.container.textContent).not.toContain("Model output reached its limit"); cleanup();
 }
 const result = panel(client([{ ...connection(), run_state: "stopped", execution: { phase: "stopped", reason: "control_changed", model_failure: "output_limit" } }]));
 await waitFor(() => expect(result.container.textContent).toContain("Authority or run state changed"));
 expect(result.container.textContent).not.toContain("Model output reached its limit");
});

it("reconciles a failed lease to the finished version without restarting or resetting budget", async () => {
 vi.useFakeTimers(); vi.setSystemTime(new Date("2026-10-16T00:00:00Z"));
 const descriptor=Object.getOwnPropertyDescriptor(document,"visibilityState"); Object.defineProperty(document,"visibilityState",{configurable:true,value:"visible"});
 const initial={...connection(),run_state:"running",calls_spent:2};
 const finished={...initial,version:3,run_state:"stopped",execution:{phase:"finished",reason:"completed"}};
 const api=client([initial]); api.lease.mockRejectedValueOnce({code:"version_conflict"}); api.read=vi.fn(async()=>finished);
 const result=panel(api); await vi.advanceTimersByTimeAsync(0); await vi.advanceTimersByTimeAsync(31000);
 expect(api.read).toHaveBeenCalledWith(initial); expect(result.container.textContent).not.toContain("Executor running");
 expect(api.activate).not.toHaveBeenCalled(); expect(api.create).not.toHaveBeenCalled();
 await vi.advanceTimersByTimeAsync(1000); expect(api.lease.mock.calls.at(-1)[0]).toMatchObject({version:3,calls_spent:2,absolute_expires_at_ms:initial.absolute_expires_at_ms});
 if(descriptor) Object.defineProperty(document,"visibilityState",descriptor); else delete document.visibilityState;
});

it("keeps a known connection manageable when paginated history changes", async () => {
 const api=client(); api.read=vi.fn(async(value)=>({...value,version:2}));
 const result=panel(api); await waitFor(()=>expect(result.container.textContent).toContain("Model ready"));
 api.list.mockRejectedValueOnce({code:"list_changed"});
 fireEvent.click(result.getByRole("button",{name:"Refresh connection status"}));
 await waitFor(()=>expect(api.read).toHaveBeenCalledTimes(1));
 expect(result.getByRole("alert").textContent).toContain("list changed");
 fireEvent.click(result.getByRole("button",{name:"Disconnect and forget"}));
 await waitFor(()=>expect(api.forget).toHaveBeenCalledWith(expect.objectContaining({version:2})));
});

it("drops a last-known connection after an authoritative ownership denial", async () => {
 const api=client(); api.read=vi.fn(async()=>{throw {status:404,code:"request_failed"};});
 const result=panel(api); await waitFor(()=>expect(result.container.textContent).toContain("Model ready"));
 api.list.mockRejectedValueOnce({code:"list_changed"});
 fireEvent.click(result.getByRole("button",{name:"Refresh connection status"}));
 await waitFor(()=>expect(api.read).toHaveBeenCalledTimes(1));
 expect(result.container.textContent).not.toContain("Model ready");
 expect(result.queryByRole("button",{name:"Disconnect and forget"})).toBeNull();
});

it("ignores a delayed lease after a newer management refresh", async () => {
 vi.useFakeTimers();vi.setSystemTime(new Date("2026-10-16T00:00:00Z"));
 const descriptor=Object.getOwnPropertyDescriptor(document,"visibilityState");Object.defineProperty(document,"visibilityState",{configurable:true,value:"visible"});
 let finish;const old={...connection(),run_state:"running"};const api=client([old]);api.read=vi.fn();api.lease.mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;}));
 const result=panel(api);await vi.advanceTimersByTimeAsync(0);await vi.advanceTimersByTimeAsync(30000);
 expect(finish).toBeTypeOf("function");api.list.mockResolvedValueOnce([]);
 fireEvent.click(result.getByRole("button",{name:"Refresh connection status"}));await vi.advanceTimersByTimeAsync(0);
 finish({...old,version:2});await vi.advanceTimersByTimeAsync(0);
 expect(result.container.textContent).not.toContain("Executor running");expect(api.read).not.toHaveBeenCalled();
 if(descriptor)Object.defineProperty(document,"visibilityState",descriptor);else delete document.visibilityState;
});

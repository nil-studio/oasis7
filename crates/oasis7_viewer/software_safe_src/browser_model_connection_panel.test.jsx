import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, expect, it, vi } from "vitest";
import { BrowserModelConnectionPanel } from "./browser_model_connection_panel.jsx";
afterEach(cleanup);
function fixture() {
  let value = { revision: 0, hasKey: false, checking: false, checked: false, callsReserved: 0, tokensReserved: 0 };
  const api = {
    snapshot: () => value, subscribe: () => () => {}, dispose: vi.fn(),
    save: vi.fn(async () => { value = { ...value, revision: 1, hasKey: true }; }),
    probe: vi.fn(async () => { value = { ...value, revision: 2, checked: true, callsReserved: 2 }; }),
    stop: vi.fn(async () => {}), forget: vi.fn(async () => { value = { ...value, hasKey: false, checked: false }; }), logout: vi.fn(async () => {}),
  };
  return api;
}
it("fails closed without a browser Task executor and never instantiates credential storage", () => {
  const createSession = vi.fn();
  const view = render(() => <BrowserModelConnectionPanel accountId="owner" locale="en" createSession={createSession} />);
  expect(view.container.textContent).toContain("browser task is not prepared");
  expect(view.queryByLabelText("Inference API key")).toBeNull();
  expect(createSession).not.toHaveBeenCalled();
  expect(view.queryByRole("button", { name: "Confirm start" })).toBeNull();
});
it("requires save and probe consent independently, clears raw input and never auto-probes", async () => {
  const api = fixture();
  const view = render(() => <BrowserModelConnectionPanel accountId="owner" executionReady={true} locale="en" createSession={() => api} />);
  const save = view.getByRole("button", { name: "Save key" });
  expect(save.disabled).toBe(true);
  const input = view.getByLabelText("Inference API key");
  fireEvent.input(input, { target: { value: "canary-ui-secret" } });
  fireEvent.click(view.getByLabelText(/I agree to save/));
  fireEvent.submit(view.container.querySelector("form"));
  expect(input.value).toBe("");
  await waitFor(() => expect(api.save).toHaveBeenCalledWith("canary-ui-secret", 0));
  await waitFor(() => expect(view.getByLabelText(/Consent to one bounded/).disabled).toBe(false));
  expect(api.probe).not.toHaveBeenCalled();
  expect(view.container.textContent).not.toContain("canary-ui-secret");
  fireEvent.click(view.getByLabelText(/Consent to one bounded/));
  fireEvent.click(view.getByRole("button", { name: "Test connection" }));
  await waitFor(() => expect(api.probe).toHaveBeenCalledWith(1));
  await waitFor(() => expect(view.container.textContent).toContain("Tool connectivity check passed"));
  expect(view.container.textContent).toContain("Agent has not started");
});
it("clears the old account credentials on an account switch and does not accept late UI results", async () => {
  const first = fixture(), second = fixture();
  const [account, setAccount] = createSignal("owner-a");
  const view = render(() => <BrowserModelConnectionPanel accountId={account} executionReady={true} locale="en" createSession={({ accountId }) => accountId === "owner-a" ? first : second} />);
  setAccount("owner-b");
  await waitFor(() => expect(first.logout).toHaveBeenCalledTimes(1));
  expect(first.dispose).toHaveBeenCalledTimes(1);
  expect(second.probe).not.toHaveBeenCalled();
  setAccount(null);
  await waitFor(() => expect(second.logout).toHaveBeenCalledTimes(1));
  expect(view.queryByLabelText("Inference API key")).toBeNull();
});
it("renders fixed errors instead of provider or credential text", async () => {
  const api = fixture(); api.save.mockRejectedValue(new Error("private-key upstream request body"));
  const view = render(() => <BrowserModelConnectionPanel accountId="owner" executionReady={true} locale="zh" createSession={() => api} />);
  fireEvent.input(view.getByLabelText("推理 API Key"), { target: { value: "canary" } });
  fireEvent.click(view.getByLabelText(/我同意保存/));
  fireEvent.submit(view.container.querySelector("form"));
  await waitFor(() => expect(view.getByRole("alert")).toBeTruthy());
  expect(view.container.textContent).not.toContain("private-key");
});

it("distinguishes probe allowance from the shared execution budget", () => {
 const api=fixture(); api.snapshot=()=>({revision:1,hasKey:true,checking:false,checked:false,callsReserved:7,tokensReserved:90000,probeCallsReserved:2,probeTokensReserved:48});
 const view=render(()=><BrowserModelConnectionPanel accountId="owner" executionReady={true} locale="en" createSession={()=>api}/>);
 expect(view.container.textContent).toContain("Check calls used / tokens reserved: 2/4 · 48/2048");
 expect(view.container.textContent).toContain("Shared check and execution calls / tokens reserved: 7/8 · 90000/131072");
});

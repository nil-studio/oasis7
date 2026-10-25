import { createSignal } from "solid-js";
import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
vi.mock("./model_connection_panel.jsx", () => ({ ModelConnectionPanel: (props) => <p>{props.managementOnly ? "Legacy management only" : "Legacy enabled"}</p> }));
import { BrowserByokSurface } from "./browser_byok_surface.jsx";
afterEach(cleanup);
const ownerProps = { protocolLoader: async () => ({}), createOwnerEntry: () => ({ inspect: async () => ({ state: "ready", agentId: "agent", taskId: "task" }) }) };
it("requires read-only inspection before authorizing the actual executor fence", async () => {
  let value = { phase: "unprepared", ready: false }, listener;
  const fence = { world_id: "test-world", owner_binding: "owner", agent_id: "agent", task_id: "task", active_goal_reference: { goal_ref: "goal" } };
  const modelSession = { subscribe: () => () => {}, snapshot: () => ({ hasKey: false }), stop: vi.fn() };
  const session = { modelSession, subscribe: (fn) => { listener = fn; return () => {}; }, snapshot: () => value,
    prepare: vi.fn(async (options) => { value = { phase: options.inspectOnly ? "inspected" : "ready", ready: !options.inspectOnly, fence, goal: "Make a product" }; listener(); }), dispose: vi.fn(), stop: vi.fn(), start: vi.fn() };
  const view = render(() => <BrowserByokSurface {...ownerProps} config={{ servicePublicKey: "key" }} accountId="account" agentId="agent" ownerSigner={{ publicKey: "owner", sign: vi.fn() }} locale="en" createSession={() => session} />);
  await waitFor(() => expect(view.getByRole("button", { name: "Check task and register executor" }).disabled).toBe(true));
  expect(view.queryByLabelText("Inference API key")).toBeNull();
  fireEvent.click(view.getByRole("button", { name: "Inspect current task" }));
  await waitFor(() => expect(view.container.textContent).toContain("Task=task"));
  expect(session.prepare).toHaveBeenCalledWith({ inspectOnly: true });
  fireEvent.click(view.getByLabelText(/Confirm the existing task/));
  fireEvent.click(view.getByRole("button", { name: "Check task and register executor" }));
  await waitFor(() => expect(session.prepare).toHaveBeenCalledWith({ takeover: true, confirmedFence: fence }));
  expect(session.start).not.toHaveBeenCalled();
  expect(view.container.textContent).toContain("Legacy management only");
});
it("accepts no key and creates no session when deployment trust is absent", () => {
  const createSession = vi.fn();
  const view = render(() => <BrowserByokSurface accountId="account" agentId="agent" ownerSigner={{}} createSession={createSession} locale="en" />);
  expect(createSession).not.toHaveBeenCalled();
  expect(view.queryByLabelText("Inference API key")).toBeNull();
});

it("prepares a canonical account entry before exposing executor or model controls, without selecting a fabricated projection", async () => {
  const ownerEntry = { inspect: vi.fn(async () => ({ state: "needs_participant" })), prepare: vi.fn(async () => ({ state: "ready", agentId: "real-agent", taskId: "real-task" })) };
  const createSession = vi.fn(() => ({ snapshot: () => ({ phase: "unprepared", ready: false }), subscribe: () => () => {}, dispose: vi.fn(), modelSession: { subscribe: () => () => {}, snapshot: () => ({ hasKey: false }) } }));
  const select = vi.fn(() => false);
  const view = render(() => <BrowserByokSurface config={{ servicePublicKey: "key" }} accountId="account" ownerSigner={{ publicKey: "owner", sign: vi.fn() }} locale="en" protocolLoader={async () => ({})} createOwnerEntry={() => ownerEntry} createSession={createSession} onCanonicalAgent={select} />);
  await waitFor(() => expect(view.container.textContent).toContain("Account binding needs verification"));
  expect(createSession).not.toHaveBeenCalled();
  expect(view.queryByLabelText("Inference API key")).toBeNull();
  const button = view.getByRole("button", { name: "Prepare player, Agent and Task" });
  expect(button.disabled).toBe(true);
  fireEvent.input(view.getByLabelText("Task goal"), { target: { value: "Make one product" } });
  expect(button.disabled).toBe(true);
  fireEvent.click(view.getByLabelText(/Confirm registration or verification/));
  fireEvent.click(button);
  await waitFor(() => expect(ownerEntry.prepare).toHaveBeenCalledWith({ goal: "Make one product", consent: true }));
  await waitFor(() => expect(createSession).toHaveBeenCalledWith(expect.objectContaining({ agentId: "real-agent" })));
  expect(select).toHaveBeenCalledWith("real-agent");
  expect(view.container.textContent).toContain("Viewer projection is still syncing");
  expect(view.queryByLabelText("Inference API key")).toBeNull();
});
it("ignores a late account preparation result after the installed owner changes", async () => {
  let finish;
  const first = { inspect: async () => ({ state: "needs_participant" }), prepare: () => new Promise((resolve) => { finish = resolve; }) };
  const second = { inspect: async () => ({ state: "owner_mismatch" }) };
  const createSession = vi.fn();
  const [owner, setOwner] = createSignal({ publicKey: "first-owner", sign: vi.fn() });
  const view = render(() => <BrowserByokSurface config={{ servicePublicKey: "key" }} accountId="account" ownerSigner={owner} locale="en" protocolLoader={async () => ({})} createOwnerEntry={({ ownerSigner }) => ownerSigner.publicKeyHex === "first-owner" ? first : second} createSession={createSession} />);
  await waitFor(() => expect(view.container.textContent).toContain("Account binding needs verification"));
  fireEvent.input(view.getByLabelText("Task goal"), { target: { value: "Make one product" } });
  fireEvent.click(view.getByLabelText(/Confirm registration or verification/));
  fireEvent.click(view.getByRole("button", { name: "Prepare player, Agent and Task" }));
  await waitFor(() => expect(finish).toBeTypeOf("function"));
  setOwner({ publicKey: "second-owner", sign: vi.fn() });
  await waitFor(() => expect(view.container.textContent).toContain("Owner identity mismatch"));
  finish({ state: "ready", agentId: "old-agent", taskId: "old-task" });
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(createSession).not.toHaveBeenCalled();
  expect(view.queryByText(/old-agent/)).toBeNull();
  expect(view.getByRole("button", { name: "Prepare player, Agent and Task" }).disabled).toBe(true);
});

it("disposes executor confirmation when the Viewer target changes", async () => {
  const sessions = [];
  const [selected, setSelected] = createSignal("agent");
  const createSession = vi.fn(() => { const value = { snapshot: () => ({ phase: "unprepared", ready: false }), subscribe: () => () => {}, dispose: vi.fn(), modelSession: { subscribe: () => () => {}, snapshot: () => ({ hasKey: false }) } }; sessions.push(value); return value; });
  render(() => <BrowserByokSurface {...ownerProps} config={{ servicePublicKey: "key" }} accountId="account" agentId={selected} ownerSigner={{ publicKey: "owner", sign: vi.fn() }} locale="en" createSession={createSession} />);
  await waitFor(() => expect(sessions).toHaveLength(1));
  setSelected("another-visible-agent");
  await waitFor(() => expect(sessions).toHaveLength(2));
  expect(sessions[0].dispose).toHaveBeenCalledOnce();
  expect(createSession.mock.calls[1][0].agentId).toBe("agent");
});

it("bounds a multibyte goal by UTF-8 bytes before authorizing task preparation", async () => {
  const prepare = vi.fn();
  const view = render(() => <BrowserByokSurface config={{ servicePublicKey: "key" }} accountId="account" ownerSigner={{ publicKey: "owner", sign: vi.fn() }} locale="en" protocolLoader={async () => ({})} createOwnerEntry={() => ({ inspect: async () => ({ state: "needs_task" }), prepare })} />);
  await waitFor(() => expect(view.getByLabelText("Task goal")).toBeTruthy());
  fireEvent.input(view.getByLabelText("Task goal"), { target: { value: "中".repeat(2731) } });
  fireEvent.click(view.getByLabelText(/Confirm registration or verification/));
  expect(view.container.textContent).toContain("8193 / 8192");
  expect(view.getByRole("button", { name: "Prepare player, Agent and Task" }).disabled).toBe(true);
  expect(prepare).not.toHaveBeenCalled();
});

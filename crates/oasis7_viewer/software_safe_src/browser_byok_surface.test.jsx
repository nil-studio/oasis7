import { cleanup, fireEvent, render, waitFor } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
vi.mock("./model_connection_panel.jsx", () => ({ ModelConnectionPanel: (props) => <p>{props.managementOnly ? "Legacy management only" : "Legacy enabled"}</p> }));
import { BrowserByokSurface } from "./browser_byok_surface.jsx";
afterEach(cleanup);
it("requires read-only inspection before authorizing the actual executor fence", async () => {
  let value = { phase: "unprepared", ready: false }, listener;
  const fence = { world_id: "test-world", owner_binding: "owner", agent_id: "agent", task_id: "task", active_goal_reference: { goal_ref: "goal" } };
  const modelSession = { subscribe: () => () => {}, snapshot: () => ({ hasKey: false }), stop: vi.fn() };
  const session = { modelSession, subscribe: (fn) => { listener = fn; return () => {}; }, snapshot: () => value,
    prepare: vi.fn(async (options) => { value = { phase: options.inspectOnly ? "inspected" : "ready", ready: !options.inspectOnly, fence, goal: "Make a product" }; listener(); }), dispose: vi.fn(), stop: vi.fn(), start: vi.fn() };
  const view = render(() => <BrowserByokSurface config={{ servicePublicKey: "key" }} accountId="account" agentId="agent" ownerSigner={{ publicKey: "owner", sign: vi.fn() }} locale="en" createSession={() => session} />);
  expect(view.getByRole("button", { name: "Check task and register executor" }).disabled).toBe(true);
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

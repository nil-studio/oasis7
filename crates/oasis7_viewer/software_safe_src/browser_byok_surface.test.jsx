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

it("stops first entry preparation and reopens a fresh generation for read-only recovery", async () => {
  let finish, firstSignal;
  const first = { inspect: vi.fn(async () => ({ state: "needs_participant" })), prepare: vi.fn(() => new Promise((resolve) => { finish = resolve; })), dispose: vi.fn() };
  const recovered = { inspect: vi.fn(async () => ({ state: "needs_goal", agentId: "actual-agent", taskId: "actual-task" })), prepare: vi.fn(), dispose: vi.fn() };
  const createSession = vi.fn();
  let count = 0;
  const view = render(() => <BrowserByokSurface config={{ servicePublicKey: "key" }} accountId="account" ownerSigner={{ publicKey: "owner", sign: vi.fn() }} locale="en" protocolLoader={async () => ({})} createOwnerEntry={({ signal }) => { count++; if (count === 1) { firstSignal = signal; return first; } return recovered; }} createSession={createSession} />);
  await waitFor(() => expect(view.getByLabelText("Task goal")).toBeTruthy());
  fireEvent.input(view.getByLabelText("Task goal"), { target: { value: "Make one product" } });
  fireEvent.click(view.getByLabelText(/Confirm registration or verification/));
  fireEvent.click(view.getByRole("button", { name: "Prepare player, Agent and Task" }));
  await waitFor(() => expect(finish).toBeTypeOf("function"));
  const stop = view.getByRole("button", { name: "Stop entry preparation" });
  expect(stop.disabled).toBe(false);
  fireEvent.click(stop);
  expect(firstSignal.aborted).toBe(true);
  expect(first.dispose).toHaveBeenCalledOnce();
  finish({ state: "ready", agentId: "late-agent", taskId: "late-task" });
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(createSession).not.toHaveBeenCalled();
  expect(view.queryByText(/late-agent/)).toBeNull();
  expect(view.container.textContent).toContain("Already submitted originals may still commit");
  fireEvent.click(view.getByRole("button", { name: "Reopen and inspect entry originals" }));
  await waitFor(() => expect(recovered.inspect).toHaveBeenCalledOnce());
  expect(first.prepare).toHaveBeenCalledOnce();
  expect(recovered.prepare).not.toHaveBeenCalled();
  expect(createSession).not.toHaveBeenCalled();
  expect(view.getByLabelText(/Confirm registration or verification/).checked).toBe(false);
  expect(view.getByRole("button", { name: "Prepare player, Agent and Task" }).disabled).toBe(true);
});

it("explains that missing server migration policy requires the deployment operator", async () => {
 const fence={world_id:"world",agent_id:"agent",task_id:"task"};
 const session={snapshot:()=>({phase:"inspected",ready:false,fence}),subscribe:()=>()=>{},dispose:vi.fn(),prepare:vi.fn(async()=>{throw Object.assign(new Error(),{code:"legacy_migration_required"});}),modelSession:{subscribe:()=>()=>{},snapshot:()=>({hasKey:false})}};
 const view=render(()=><BrowserByokSurface {...ownerProps} config={{servicePublicKey:"key"}} accountId="account" agentId="agent" ownerSigner={{publicKey:"owner",sign:vi.fn()}} locale="en" createSession={()=>session}/>);
 await waitFor(()=>expect(view.getByRole("button",{name:"Check task and register executor"})).toBeTruthy());
 fireEvent.click(view.getByLabelText(/Confirm the existing task/));
 fireEvent.click(view.getByRole("button",{name:"Check task and register executor"}));
 await waitFor(()=>expect(view.getByRole("alert").textContent).toContain("Ask the deployment operator to complete migration"));
 expect(view.getByRole("alert").textContent).toContain("unknown original result");
 expect(view.getByRole("alert").textContent).toContain("legacy_migration_required");
});

it("requires a separate explicit continuation for a reliably unsent original", async () => {
 const continueOriginal=vi.fn(async()=>({state:"ready",agentId:"agent",taskId:"task"}));
 const entry={inspect:vi.fn(async()=>({state:"unsent",continuation:"publishRaw",goal:"Retained goal"})),prepare:vi.fn(),continueOriginal};
 const createSession=vi.fn(()=>({snapshot:()=>({phase:"unprepared",ready:false}),subscribe:()=>()=>{},dispose:vi.fn(),modelSession:{subscribe:()=>()=>{},snapshot:()=>({hasKey:false})}}));
 const view=render(()=><BrowserByokSurface config={{servicePublicKey:"key"}} accountId="account" ownerSigner={{publicKey:"owner",sign:vi.fn()}} locale="en" protocolLoader={async()=>({})} createOwnerEntry={()=>entry} createSession={createSession}/>);
 await waitFor(()=>expect(view.getByRole("button",{name:"Continue sending the retained original"})).toBeTruthy());
 const button=view.getByRole("button",{name:"Continue sending the retained original"});
 expect(button.disabled).toBe(true);
 expect(view.getByLabelText("Task goal").value).toBe("Retained goal");
 fireEvent.click(view.getByLabelText(/Confirm registration or verification/));
 fireEvent.click(button);
 await waitFor(()=>expect(continueOriginal).toHaveBeenCalledWith({goal:"Retained goal",consent:true}));
 await waitFor(()=>expect(createSession).toHaveBeenCalledOnce());
 expect(entry.prepare).not.toHaveBeenCalled();
 expect(view.container.textContent).toContain("Account task ready");
 expect(view.getByRole("button",{name:"Inspect current task"}).disabled).toBe(false);
});

it.each(["owner","world"])("isolates a late explicit continuation after %s switches",async change=>{
 let finish,oldSignal,count=0;
 const first={inspect:async()=>({state:"unsent",continuation:"publishRaw",goal:"Retained goal"}),continueOriginal:()=>new Promise(resolve=>{finish=resolve;}),dispose:vi.fn()};
 const second={inspect:async()=>({state:"owner_mismatch"}),dispose:vi.fn()};
 const [owner,setOwner]=createSignal({publicKey:"owner-a",sign:vi.fn()});
 const [config,setConfig]=createSignal({servicePublicKey:"key",world:{world_id:"world-a"}});
 const createSession=vi.fn(),onCanonicalAgent=vi.fn();
 const view=render(()=><BrowserByokSurface config={config} accountId="account" ownerSigner={owner} locale="en" protocolLoader={async()=>({})} createOwnerEntry={({signal})=>{count++;if(count===1){oldSignal=signal;return first;}return second;}} createSession={createSession} onCanonicalAgent={onCanonicalAgent}/>);
 await waitFor(()=>expect(view.getByRole("button",{name:"Continue sending the retained original"})).toBeTruthy());
 fireEvent.click(view.getByLabelText(/Confirm registration or verification/));
 fireEvent.click(view.getByRole("button",{name:"Continue sending the retained original"}));
 await waitFor(()=>expect(finish).toBeTypeOf("function"));
 if(change==="owner")setOwner({publicKey:"owner-b",sign:vi.fn()});else setConfig({servicePublicKey:"key",world:{world_id:"world-b"}});
 await waitFor(()=>expect(view.container.textContent).toContain("Owner identity mismatch"));
 expect(oldSignal.aborted).toBe(true);expect(first.dispose).toHaveBeenCalledOnce();
 finish({state:"ready",agentId:"late-agent",taskId:"late-task"});
 await new Promise(resolve=>setTimeout(resolve,0));
 expect(createSession).not.toHaveBeenCalled();expect(onCanonicalAgent).not.toHaveBeenCalled();
 expect(view.queryByText(/late-agent/)).toBeNull();
});

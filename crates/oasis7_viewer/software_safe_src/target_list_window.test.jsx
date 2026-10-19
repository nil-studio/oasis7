import { fireEvent, render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { describe, expect, it } from "vitest";
import { TargetListWindow } from "./target_list_window.jsx";

describe("bounded complete Targets lists", () => {
  const source = () => Array.from({ length: 10000 }, (_, index) => ({ id: `agent-${String(index).padStart(5, "0")}`, name: `Agent ${index}` }));
  it("mounts at most 50 rows while every matching target remains searchable and selectable", () => {
    const entries = source();
    const [filter, setFilter] = createSignal("");
    const [selected, setSelected] = createSignal(null);
    const view = render(() => <TargetListWindow items={entries.filter((entry) => entry.id.includes(filter()))} filterKey={filter()} scopeKey="player-a" locale="en" kind="agent">
      {(entry, index) => <button data-target-row data-index={index()} aria-pressed={selected() === entry().id} onClick={() => setSelected(entry().id)}>{entry().id}</button>}
    </TargetListWindow>);
    expect(view.container.querySelectorAll("[data-target-row]")).toHaveLength(50);
    expect(view.getByRole("status")).toHaveTextContent("1–50 / 10000");
    for (let page = 0; page < 199; page += 1) fireEvent.click(view.getByRole("button", { name: "Next agents page" }));
    expect(view.container.querySelectorAll("[data-target-row]")).toHaveLength(50);
    expect(view.getByRole("status")).toHaveTextContent("9951–10000 / 10000");
    fireEvent.click(view.getByRole("button", { name: "agent-09999" }));
    expect(selected()).toBe("agent-09999");
    setFilter("09999");
    expect(view.container.querySelectorAll("[data-target-row]")).toHaveLength(1);
    expect(view.getByRole("status")).toHaveTextContent("1–1 / 1");
    expect(view.getByRole("button", { name: "agent-09999" })).toHaveAttribute("aria-pressed", "true");
    setFilter("");
    expect(view.container.querySelector("[data-target-row]")).toHaveAttribute("data-index", "0");
    expect(selected()).toBe("agent-09999");
  });

  it("clamps removed pages, resets scope, removes revoked rows and preserves focused identities across snapshots", () => {
    const [entries, setEntries] = createSignal(source());
    const [scope, setScope] = createSignal("player-a");
    const view = render(() => <TargetListWindow items={entries()} filterKey="" scopeKey={scope()} locale="en" kind="agent">
      {(entry) => <button data-target-row>{entry().name}</button>}
    </TargetListWindow>);
    fireEvent.click(view.getByRole("button", { name: "Next agents page" }));
    expect(view.getByRole("status")).toHaveTextContent("51–100 / 10000");
    const focused = view.getByRole("button", { name: "Agent 50" });
    focused.focus();
    setEntries(entries().map((entry) => ({ ...entry, name: `Updated ${entry.name}` })));
    expect(view.getByRole("button", { name: "Updated Agent 50" })).toBe(focused);
    expect(document.activeElement).toBe(focused);
    setEntries(entries().slice(0, 10));
    expect(view.getByRole("status")).toHaveTextContent("1–10 / 10");
    expect(view.queryByRole("button", { name: "Updated Agent 50" })).toBeNull();
    setEntries(source());
    fireEvent.click(view.getByRole("button", { name: "Next agents page" }));
    setScope("player-b");
    setEntries([{ id: "agent-allowed-b", name: "Player B target" }]);
    expect(view.container.querySelectorAll("[data-target-row]")).toHaveLength(1);
    expect(view.getByRole("button", { name: "Player B target" })).toBeInTheDocument();
    expect(view.queryByRole("button", { name: "Agent 50" })).toBeNull();
    setEntries([]);
    expect(view.container.querySelectorAll("[data-target-row]")).toHaveLength(0);
    expect(view.getByRole("status")).toHaveTextContent("0–0 / 0");
  });
});

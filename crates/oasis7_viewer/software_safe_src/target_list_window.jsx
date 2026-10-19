import { createEffect, createMemo, createSignal, For, Show } from "solid-js";

const PAGE_SIZE = 50;

// Window only the DOM. Authorization and search operate on the complete list
// upstream, and changing pages never changes the current world selection.
export function TargetListWindow(props) {
  const zh = () => String(props.locale || "").startsWith("zh");
  const noun = () => props.kind === "location" ? (zh() ? "地点" : "locations") : (zh() ? "行动体" : "agents");
  const items = createMemo(() => props.items || []);
  const resetKey = createMemo(() => JSON.stringify([props.filterKey || "", props.scopeKey || ""]));
  const [requested, setRequested] = createSignal({ key: resetKey(), page: 0 });
  const lastPage = createMemo(() => Math.max(0, Math.ceil(items().length / PAGE_SIZE) - 1));
  const page = createMemo(() => requested().key === resetKey() ? Math.min(requested().page, lastPage()) : 0);
  // The derived page resets immediately, before effects or DOM consumers run.
  // Persist the clamp too so later source expansion cannot resurrect an old page.
  createEffect(() => {
    const key = resetKey();
    const current = page();
    if (requested().key !== key || requested().page !== current) setRequested({ key, page: current });
  });
  const entities = createMemo(() => new Map(items().slice(page() * PAGE_SIZE, (page() + 1) * PAGE_SIZE).map((entry) => [entry.id, entry])));
  const keys = createMemo(() => [...entities().keys()]);
  const changePage = (next) => setRequested({ key: resetKey(), page: Math.max(0, Math.min(next, lastPage())) });
  return <>
    <div class="toolbar" aria-label={zh() ? `${noun()}分页` : `${noun()} pagination`}>
      <span role="status" aria-live="polite">{zh() ? "获准匹配目标" : "Authorized matches"}: {items().length ? page() * PAGE_SIZE + 1 : 0}–{Math.min((page() + 1) * PAGE_SIZE, items().length)} / {items().length}</span>
      <Show when={items().length > PAGE_SIZE}>
        <button type="button" disabled={page() === 0} aria-label={zh() ? `上一页${noun()}` : `Previous ${noun()} page`} onClick={() => changePage(page() - 1)}>{zh() ? "上一页" : "Previous"}</button>
        <button type="button" disabled={page() >= lastPage()} aria-label={zh() ? `下一页${noun()}` : `Next ${noun()} page`} onClick={() => changePage(page() + 1)}>{zh() ? "下一页" : "Next"}</button>
      </Show>
    </div>
    <For each={keys()}>{(key, index) => {
      const entry = createMemo((previous) => entities().get(key) || previous);
      return props.children(entry, () => page() * PAGE_SIZE + index());
    }}</For>
  </>;
}

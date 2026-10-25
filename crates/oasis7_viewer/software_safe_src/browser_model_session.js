import { createBrowserModelStore, BrowserModelStoreError } from "./browser_model_store.js";
import { probeBrowserModel } from "./browser_model_provider.js";

// A connection check never has a world port. Task execution is a separate gate.
export function createBrowserModelSession({ accountId, storage, locks, fetchImpl,
  windowRef = globalThis.window, documentRef = globalThis.document,
  navigatorRef = globalThis.navigator, now = Date.now,
  monotonicNow = () => globalThis.performance.now() } = {}) {
  const store = createBrowserModelStore({ accountId, storage, locks });
  const listeners = new Set();
  let controller, generation = 0, disposed = false, checking = false, verifiedRevision = null;
  let lastWall = now(), lastMonotonic = monotonicNow();
  function sampleClock() {
    const wall = now(), monotonic = monotonicNow();
    const gap = wall - lastWall > 10000 || monotonic - lastMonotonic > 10000
      || wall < lastWall || monotonic < lastMonotonic;
    lastWall = wall; lastMonotonic = monotonic;
    return gap;
  }
  const notify = () => { for (const listener of listeners) listener(); };
  const cancel = () => { generation++; controller?.abort(); controller = null; verifiedRevision = null; };
  const unsubscribe = store.subscribe(() => {
    // Local writes and storage notifications carry no credential material.
    if (controller && store.snapshot().revision !== controller.revision) cancel();
    notify();
  });
  async function stop() {
    cancel();
    return store.stop();
  }
  const hidden = () => {
    if (documentRef?.visibilityState !== "visible" || navigatorRef?.onLine === false) {
      void stop().catch(() => notify());
    }
  };
  const pagehide = () => { void stop().catch(() => notify()); };
  const wake = () => { sampleClock(); pagehide(); };
  const clockTimer = setInterval(() => {
    if (sampleClock() && checking) pagehide();
  }, 1000);
  const storageChanged = (event) => {
    if (event.key === null || event.key === store.storageKey) {
      cancel();
      try { store.refresh(); } catch { notify(); }
    }
  };
  documentRef?.addEventListener("visibilitychange", hidden);
  documentRef?.addEventListener("freeze", wake);
  documentRef?.addEventListener("resume", wake);
  windowRef?.addEventListener("offline", hidden);
  windowRef?.addEventListener("pagehide", pagehide);
  windowRef?.addEventListener("storage", storageChanged);
  return {
    snapshot() { return { ...store.snapshot(), checking, checked: verifiedRevision === store.snapshot().revision }; },
    subscribe(listener) { listeners.add(listener); return () => listeners.delete(listener); },
    async save(key, revision) { cancel(); return store.save(key, revision); },
    stop,
    async forget(revision) { cancel(); return store.forget(revision); },
    async logout() { cancel(); return store.clear(); },
    async probe(revision) {
      if (disposed || checking) throw new BrowserModelStoreError("probe_busy");
      if (documentRef?.visibilityState !== "visible" || navigatorRef?.onLine === false) throw new BrowserModelStoreError("page_inactive");
      checking = true; notify();
      lastWall = now(); lastMonotonic = monotonicNow();
      const currentGeneration = ++generation;
      try {
        const started = await store.beginProbe(revision);
        if (disposed || currentGeneration !== generation) throw new BrowserModelStoreError("stopped");
        const active = new AbortController();
        active.revision = started.revision;
        controller = active;
        const assertCurrent = () => {
          if (sampleClock()) { void stop().catch(() => notify()); }
          if (disposed || active.signal.aborted || generation !== currentGeneration
            || documentRef?.visibilityState !== "visible" || navigatorRef?.onLine === false) throw new BrowserModelStoreError("stopped");
        };
        const result = await probeBrowserModel({ signal: active.signal, fetchImpl,
          beforeSend: ({ inputBytes, maxOutputTokens }) => {
            assertCurrent();
            // One token per UTF-8 byte is a conservative bound, not a usage estimate.
            return store.reserve({ revision: started.revision, maxInputTokens: inputBytes, maxOutputTokens });
          },
          getCredential: async () => { assertCurrent(); const key = await store.credential(started.revision); assertCurrent(); return key; },
          afterResponse: ({ reservation, usage }) => store.settle(reservation, usage ? { inputTokens: usage.prompt_tokens, outputTokens: usage.completion_tokens } : null),
        });
        assertCurrent();
        if (store.snapshot().revision !== started.revision) throw new BrowserModelStoreError("stopped");
        verifiedRevision = started.revision;
        return result;
      } finally {
        controller?.abort(); controller = null; checking = false; notify();
      }
    },
    dispose() {
      disposed = true; cancel(); unsubscribe(); listeners.clear();
      clearInterval(clockTimer);
      documentRef?.removeEventListener("visibilitychange", hidden);
      documentRef?.removeEventListener("freeze", wake);
      documentRef?.removeEventListener("resume", wake);
      windowRef?.removeEventListener("offline", hidden);
      windowRef?.removeEventListener("pagehide", pagehide);
      windowRef?.removeEventListener("storage", storageChanged);
    },
  };
}

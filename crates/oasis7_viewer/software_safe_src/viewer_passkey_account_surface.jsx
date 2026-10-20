import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { accountErrorNotice, createPasskeyAccountClient, mountPasskeyAccount } from "./viewer_passkey_account_module.js";

export function PasskeyAccountSurface(props) {
  let root;
  let surface;
  let alive = true;
  const [legacy, setLegacy] = createSignal(false);
  const [error, setError] = createSignal("");
  function focusReadySurface() {
    queueMicrotask(() => {
      const dialog = root?.closest('[role="dialog"]');
      if (alive && dialog && document.activeElement === dialog) {
        dialog.querySelector('input:not([disabled]),button:not([disabled])')?.focus();
      }
    });
  }
  onMount(async () => {
    const client = createPasskeyAccountClient();
    try {
      await client.capabilities();
      if (!alive) return;
      props.onV2Detected?.();
      surface = mountPasskeyAccount(root, { client,
        onJoinWorld: props.onJoinWorld,
        onSessionInvalidated: props.onSessionInvalidated,
      });
      focusReadySurface();
    } catch (failure) {
      if (!alive) return;
      if (failure.status === 404) { setLegacy(true); focusReadySurface(); }
      else setError(accountErrorNotice(failure, "能力检查"));
    }
  });
  onCleanup(() => { alive = false; surface?.dispose(); });
  return <div class="stack">
    <div ref={root} class="stack hosted-passkey-account" />
    <Show when={error()}><div role="status" class="empty">{error()}</div></Show>
    <Show when={legacy()}>{props.legacyForm()}</Show>
  </div>;
}

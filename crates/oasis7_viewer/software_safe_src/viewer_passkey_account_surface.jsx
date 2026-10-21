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
        onAccountState: props.onAccountState,
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

export function hostedAccountGateCopy(signedIn, locale) {
  const zh = String(locale || "").startsWith("zh");
  if (signedIn) return {
    title: zh ? "进入世界开始游戏" : "Enter the World to Play",
    badge: zh ? "账号已登录，等待入场" : "Signed in · world entry pending",
    summary: zh ? "账号已登录。请选择进入世界；世界控制确认后才能聊天与探索。" : "Your account is signed in. Enter the world to establish control before chatting and exploring.",
  };
  return {
    title: zh ? "登录后进入游戏" : "Sign In to Play",
    badge: zh ? "尚未登录" : "Not signed in",
    summary: zh ? "先登录或创建账号，再进入世界，开始聊天与探索。" : "Sign in or create an account, then enter the world to chat and explore.",
  };
}

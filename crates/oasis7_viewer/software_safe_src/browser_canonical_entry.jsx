import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { BrowserByokSurface } from "./browser_byok_surface.jsx";
import { createBrowserOwnerIdentity } from "./browser_game_identity.js";
import { createPasskeyAccountClient } from "./viewer_passkey_account_module.js";

// Canonical entry is rooted in the authenticated account and pinned WorldService.
// It can open before the legacy Viewer has any player, Agent or world snapshot.
export function BrowserCanonicalEntry(props) {
 const read = (value) => typeof value === "function" ? value() : value;
 const tr = (cn, en) => String(read(props.locale) || "en").startsWith("zh") ? cn : en;
 const [accountId, setAccountId] = createSignal(null), [owner, setOwner] = createSignal(null);
 const [agentId, setAgentId] = createSignal(null), [busy, setBusy] = createSignal(false), [error, setError] = createSignal("");
 const identities = props.identityStore || createBrowserOwnerIdentity();
 const client = props.accountClient || createPasskeyAccountClient();
 let alive = true, generation = 0;
 async function refresh() {
  const expected = ++generation; setBusy(true); setError("");
  try {
   const session = await client.session();
   if (!alive || generation !== expected) return;
   const account = session.authentication?.state === "authenticated" ? session.account?.account_id : null;
   if (typeof account !== "string" || !account) { setAccountId(null); setOwner(null); setAgentId(null); return; }
   const config = read(props.config);
   if (!config?.world?.world_id || !config.world.genesis_digest) throw new Error("game_configuration_invalid");
   const previous = read(props.legacyOwnerSigner);
   const identity = read(props.legacyAccountId) === account && previous?.publicKey && typeof previous.sign === "function" ? previous
    : await identities.getOrCreate({ worldId: config.world.world_id, genesisDigest: config.world.genesis_digest, accountId: account });
   if (!alive || generation !== expected) return;
   if (accountId() !== account) setAgentId(null);
   setOwner(identity.publicKeyHex ? { publicKey: identity.publicKeyHex, identityId: identity.publicKeyHex, sign: identity.sign } : identity);
   setAccountId(account);
  } catch { if (alive && expected === generation) { setOwner(null); setAccountId(null); setAgentId(null);
   setError(tr("未能核对登录或恢复本账号的本机玩家身份。请检查登录与浏览器存储后重试。", "Account or browser owner identity could not be verified. Check sign-in and browser storage, then retry.")); } }
  finally { if (alive && expected === generation) setBusy(false); }
 }
 onMount(() => { void refresh(); });
 onCleanup(() => { alive = false; generation += 1; });
 return <section class="stack" aria-label={tr("普通账号首局入口", "Ordinary account first session")}>
  <h2>{tr("开始第一座工厂", "Start your first factory")}</h2>
  <p>{tr("先登录账号，再核对本世界的玩家身份。首局会带你读资源、确认授权、让 Agent 代办一批生产，然后自己选择下一步。", "Sign in, then verify your player identity in this world. Read resources, confirm authority, delegate one production batch to your Agent, then choose your next step.")}</p>
  <button type="button" disabled={busy()} onClick={() => void refresh()}>{tr("核对登录与玩家身份", "Verify account and player identity")}</button>
  <Show when={!accountId() && !busy()}><p role="status">{tr("在账号面板登录后，点击上方按钮继续。", "Sign in through the account panel, then use the button above to continue.")}</p></Show>
  <Show when={error()}><p role="alert">{error()}</p></Show>
  <Show when={accountId() && owner()}>
   <BrowserByokSurface {...props.surfaceProps} firstSession={true} locale={props.locale} config={props.config}
    accountId={accountId} ownerSigner={owner} agentId={agentId}
    onCanonicalAgent={(id) => { setAgentId(id); return true; }} />
  </Show>
 </section>;
}

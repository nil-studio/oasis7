import { createEffect, createSignal, onCleanup, onMount, Show } from "solid-js";
import * as core from "./legacy_core.js";
import { Badge, EmptyState } from "./viewer_ui_primitives.jsx";
import { HostedTestLoginOptIn } from "./viewer_hosted_test_login_view.jsx";
import { PasskeyAccountSurface, hostedAccountGateCopy } from "./viewer_passkey_account_surface.jsx";
import { keepFocusInDialog } from "./viewer_modal_focus.js";
import { isHostedPublicJoinDeploymentMode } from "./software_safe_constants.js";
import {
  gameplayActionButtonBusyAttrs,
  gameplayActionButtonClass,
  gameplayActionButtonDisabled,
  gameplayActionDisabledReason,
  gameplayActionDisplayLabel,
  gameplayActionTestId,
} from "./viewer_gameplay_action_helpers.js";
import { pixelWorldBlockerPresentation } from "./pixel_world_presentation.js";

function uiLocale() {
  return core.state.uiLocale;
}

function tr(locale, zh, en) {
  return core.isLocaleZh(locale) ? zh : en;
}

function localeCode(locale) {
  return core.isLocaleZh(locale) ? "zh" : "en";
}

export function HostedLoginForm(props) {
  return <PasskeyAccountSurface onAccountState={props.onAccountState} onJoinWorld={(account, options) => core.joinHostedAccountWorld(account, options)} onV2Detected={() => core.disableLegacyHostedSession()}
    onSessionInvalidated={() => core.invalidateHostedAccountSession()}
    legacyForm={() => <LegacyHostedLoginForm {...props} />} />;
}

function LegacyHostedLoginForm(props) {
  const locale = () => props.locale ?? uiLocale();
  const clearHostedLoginError = () => {
    if (core.state.hostedLogin.error != null || core.state.hostedLogin.retryAfterSeconds != null) {
      core.state.hostedLogin.error = null;
      core.state.hostedLogin.retryAfterSeconds = null;
      core.requestRender();
    }
  };
  return (
    <div class="stack">
      <div class="control-grid">
        <div class="field">
          <label for={props.handleId ?? "hosted-login-handle"}>
            {tr(locale(), "邮箱", "Email")}
          </label>
          <input
            id={props.handleId ?? "hosted-login-handle"}
            type="email"
            autocomplete="email"
            value={core.state.hostedLogin.handle}
            onInput={(event) => {
              core.state.hostedLogin.handle = String(event.currentTarget.value || "");
              clearHostedLoginError();
            }}
          />
        </div>
      </div>
      <div class="toolbar">
        <button
          data-auth-action="start-login"
          disabled={core.state.hostedLogin.startInFlight}
          onClick={() => {
            void core.startHostedAccountLogin();
          }}
        >
          {tr(locale(), "请求登录验证码", "Request Login Code")}
        </button>
      </div>
      <Show when={core.state.hostedLogin.challengeId}>
        <div class="badge-row">
          <Badge>{`challenge=${core.state.hostedLogin.challengeId}`}</Badge>
          <Badge>{`target=${core.state.hostedLogin.maskedLoginHint || "-"}`}</Badge>
          <Badge>{`delivery=${core.state.hostedLogin.deliveryMode || "-"}`}</Badge>
          <Badge>{core.state.hostedLogin.accountExists ? "account=existing" : "account=new"}</Badge>
        </div>
        <div class="field">
          <label for={props.codeId ?? "hosted-login-code"}>
            {tr(locale(), "验证码", "Verification Code")}
          </label>
          <input
            id={props.codeId ?? "hosted-login-code"}
            type="text"
            autocomplete="off"
            value={core.state.hostedLogin.code}
            onInput={(event) => {
              core.state.hostedLogin.code = String(event.currentTarget.value || "");
              clearHostedLoginError();
            }}
          />
        </div>
        <div class="toolbar">
          <button
            data-auth-action="complete-login"
            disabled={core.state.hostedLogin.completeInFlight || core.state.auth.issueInFlight}
            onClick={() => {
              void core.completeHostedAccountLogin();
            }}
          >
            {tr(locale(), "登录并领取玩家会话", "Sign In and Acquire Player Session")}
          </button>
        </div>
      </Show>
      <Show when={core.state.hostedLogin.error}>
        <div class="stack">
          <EmptyState>{core.state.hostedLogin.error}</EmptyState>
          <Show when={core.state.hostedLogin.retryAfterSeconds != null}>
            <div class="badge-row">
              <Badge>{`retry_after=${core.state.hostedLogin.retryAfterSeconds}s`}</Badge>
            </div>
          </Show>
        </div>
      </Show>
    </div>
  );
}

export function shouldShowHostedLoginGate() {
  return !core.state.auth.available
    && isHostedPublicJoinDeploymentMode(core.state.hostedAccess?.deployment_mode);
}

function focusableElements(root) {
  return [...root.querySelectorAll(
    [
      "a[href], summary",
      "button:not([disabled])",
      "input:not([disabled])",
      "select:not([disabled])",
      "textarea:not([disabled])",
      "[tabindex]:not([tabindex='-1'])",
    ].join(","),
  )].filter((element) => !element.hasAttribute("aria-hidden") && (!element.closest("details:not([open])") || element.tagName === "SUMMARY"));
}

export function HostedLoginGate(props) {
  const { observeViewerStateRevision } = props;
  const [accountSignedIn, setAccountSignedIn] = createSignal(false);
  const accountCopy = () => hostedAccountGateCopy(accountSignedIn(), uiLocale());
  observeViewerStateRevision();
  const locale = () => uiLocale();
  let dialogRef;
  let previousFocus = null;
  let releaseFocusGuard = () => {};

  onMount(() => {
    previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    releaseFocusGuard = keepFocusInDialog(() => dialogRef);
    queueMicrotask(() => {
      const firstFocusable = dialogRef ? focusableElements(dialogRef)[0] : null;
      (firstFocusable || dialogRef)?.focus();
    });
  });

  onCleanup(() => {
    releaseFocusGuard();
    if (previousFocus && document.contains(previousFocus)) {
      previousFocus.focus();
    }
  });

  const trapDialogFocus = (event) => {
    if (event.key !== "Tab" || !dialogRef) {
      return;
    }
    const focusables = focusableElements(dialogRef);
    if (focusables.length === 0) {
      event.preventDefault();
      dialogRef.focus();
      return;
    }
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };

  return (
    <Show when={shouldShowHostedLoginGate()}>
      <div
        class="auth-gate"
        data-viewer-fixture-state="hosted_login_gate"
        role="dialog"
        aria-modal="true"
        aria-labelledby="hosted-login-gate-title"
        tabIndex="-1"
        ref={dialogRef}
        onKeyDown={trapDialogFocus}
      >
        <div class="auth-gate__dialog">
          <div class="auth-gate__header">
            <div>
              <div class="panel__eyebrow">{tr(locale(), "欢迎来到 Oasis7", "Welcome to Oasis7")}</div>
              <h1 id="hosted-login-gate-title" class="auth-gate__title">
                {accountCopy().title}
              </h1>
            </div>
            <Badge class="badge badge--warn">{accountCopy().badge}</Badge>
          </div>
          <div class="feedback-summary">{accountCopy().summary}</div>
          <HostedLoginForm
            onAccountState={({ signedIn }) => setAccountSignedIn(signedIn)}
            locale={locale()}
            handleId="gate-hosted-login-handle"
            codeId="gate-hosted-login-code"
          />
          <HostedTestLoginOptIn core={core} locale={locale} tr={tr} />
          <Show when={core.state.auth.rebindNotice || core.state.auth.error}>
            <EmptyState>{core.state.auth.rebindNotice || core.state.auth.error}</EmptyState>
          </Show>
        </div>
      </div>
    </Show>
  );
}

export function EmptyEntityRecoveryCard(props) {
  const { CalloutCard } = props;
  const locale = () => props.locale ?? uiLocale();
  const gameplay = () => (typeof props.gameplay === "function" ? props.gameplay() : props.gameplay);
  const blockerPresentation = () => pixelWorldBlockerPresentation(gameplay()?.blockerKind, locale());
  const firstAgentClaimAction = () =>
    (gameplay()?.availableActions || []).find((action) => action.actionId === "claim_first_agent");
  const firstAgentClaimDisabledReason = () =>
    gameplayActionDisabledReason(firstAgentClaimAction(), gameplay(), locale());

  return (
    <CalloutCard
      class="empty-entity-recovery"
      kind="empty_world_recovery"
      title={props.title ?? tr(locale(), "认领第一个 Agent", "Claim Your First Agent")}
      badge={blockerPresentation().label}
      badgeClass={firstAgentClaimAction() && !firstAgentClaimDisabledReason() ? "badge badge--good" : "badge badge--warn"}
      variant={firstAgentClaimAction() && !firstAgentClaimDisabledReason() ? null : "warn"}
    >
      <div class="feedback-summary">
        {firstAgentClaimDisabledReason() || blockerPresentation().reason}
      </div>
      <div class="feedback-detail">{gameplay()?.nextStepHint || blockerPresentation().nextAction}</div>
      <Show when={gameplay()?.entityCounts}>
        <div class="badge-row">
          <Badge>{`agents=${gameplay().entityCounts.agents}`}</Badge>
          <Badge>{`locations=${gameplay().entityCounts.locations}`}</Badge>
        </div>
      </Show>
      <div class="feedback-detail">
        {firstAgentClaimAction()
          ? tr(
              locale(),
              "认领提交后等待链上提交与快照同步；同步完成后第一个 Agent 会出现在世界里。",
              "After submitting the claim, wait for chain submission and snapshot sync; the first Agent appears once the committed world updates.",
            )
          : tr(
              locale(),
              "如果页面仍提供“刷新快照”动作，先从当前入口重拉一次；如果数量仍然是 0，就需要修复或重启运行时世界引导流程。",
              "If the page still exposes a refresh action, pull a fresh snapshot from the current entry first. If the counts stay at 0, repair or restart the runtime world bootstrap.",
            )}
      </div>
    </CalloutCard>
  );
}

export function ViewerEntryMenu(props) {
  const { observeViewerStateRevision, buildViewerEntryUrls } = props;
  const locale = () => uiLocale();
  const viewerEntryUrls = () => buildViewerEntryUrls(locale());

  return (
    <details class="entry-menu">
      <summary class="entry-menu__toggle">{tr(locale(), "入口", "Entry")}</summary>
      <div class="entry-menu__panel stack">
        <div>
          <div class="panel__title panel__title--spaced">
            {tr(locale(), "语言与账号入口", "Language and Account Entry")}
          </div>
          <div class="feedback-detail">
            {tr(
              locale(),
              "主玩法继续留在当前页面；在这里切换语言或管理账号安全。",
              "Primary gameplay stays on this page. Change language or manage account security here.",
            )}
          </div>
        </div>
        <div class="toolbar">
          <button
            data-locale="zh"
            disabled={locale() === "zh"}
            onClick={() => core.setViewerLocale("zh")}
          >
            中文
          </button>
          <button
            data-locale="en"
            disabled={locale() === "en"}
            onClick={() => core.setViewerLocale("en")}
          >
            English
          </button>
        </div>
        <div class="badge-row">
          <Badge>{`locale=${localeCode(locale())}`}</Badge>
        </div>
        <div class="feedback-detail">{viewerEntryUrls().softwareSafeUrl}</div>
        <Show when={isHostedPublicJoinDeploymentMode(core.state.hostedAccess?.deployment_mode) && core.state.auth.available}>
          <details aria-label="账号与安全">
            <summary>{tr(locale(), "账号与安全", "Account and Security")}</summary>
            <div style={{ "max-height": "60vh", overflow: "auto" }}><HostedLoginForm locale={locale()} /></div>
          </details>
        </Show>
      </div>
    </details>
  );
}

import { For, Show } from "solid-js";
import * as core from "./legacy_core.js";
import { Badge, EmptyState } from "./viewer_ui_primitives.jsx";
import { EventCard, JsonBlock, MetricCard, PanelSection } from "./viewer_ui_cards.jsx";
import { HostedLoginForm } from "./viewer_hosted_login_gate.jsx";
import {
  LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE,
  isHostedPublicJoinDeploymentMode,
} from "./software_safe_constants.js";

function tr(locale, zh, en) {
  return core.isLocaleZh(locale) ? zh : en;
}

export function WorldDiagnosticsSection(props) {
  const state = core.state;
  const locale = () => props.locale();
  const authSurface = () => props.authSurface();
  const hostedRecoveryHint = () => props.hostedRecoveryHint();
  const showRebindNotice = () => props.showRebindNotice();
  const hostedActionMatrixView = () => props.hostedActionMatrixView();
  const tierBadgeClass = (status) => props.tierBadgeClass(status);
  const promptFeedback = () => props.promptFeedback();
  const promptFeedbackDisplay = () => props.promptFeedbackDisplay();
  const chatFeedback = () => props.chatFeedback();
  const chatFeedbackDisplay = () => props.chatFeedbackDisplay();

  return (
    <>
          <div class="badge-row">
            <Badge class={state.auth.available ? "badge badge--good" : "badge badge--warn"}>
              {`auth=${state.auth.available ? state.auth.registrationStatus || "ready" : "missing"}`}
            </Badge>
            <Badge class="badge badge--accent">{`tier=${authSurface().currentTier}`}</Badge>
            <Badge>{`source=${authSurface().source}`}</Badge>
            <Badge>{`deploymentHint=${authSurface().deploymentHint}`}</Badge>
            <Badge>{`player=${state.auth.playerId || "-"}`}</Badge>
            <Badge>{`pubkey=${state.auth.publicKey ? `${state.auth.publicKey.slice(0, 10)}…` : "-"}`}</Badge>
            <Badge>{`epoch=${state.auth.sessionEpoch == null ? "-" : state.auth.sessionEpoch}`}</Badge>
            <Badge>{`runtime=${state.auth.runtimeStatus || "-"}`}</Badge>
            <Badge>{`boundAgent=${state.auth.boundAgentId || "-"}`}</Badge>
            <Badge>{`requestedAgent=${state.auth.pendingRequestedAgentId || "-"}`}</Badge>
            <Badge>{state.auth.pendingForceRebind ? "rebind=forcing" : "rebind=idle"}</Badge>
          </div>
          <div class="toolbar">
            <Show when={state.auth.available && state.auth.source !== LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE}>
              <button
                data-auth-action="logout"
                onClick={() => {
                  void core.logoutHostedPlayerSession();
                }}
              >
                {tr(locale(), "释放玩家会话", "Release Player Session")}
              </button>
            </Show>
          </div>
          <Show when={hostedRecoveryHint()}>
            {(hint) => <EmptyState>{hint().detail}</EmptyState>}
          </Show>
          <Show
            when={
              !state.auth.available
              && isHostedPublicJoinDeploymentMode(state.hostedAccess?.deployment_mode)
            }
          >
            <HostedLoginForm
              locale={locale()}
              channelId="diag-hosted-login-channel"
              handleId="diag-hosted-login-handle"
              codeId="diag-hosted-login-code"
            />
          </Show>
          <Show when={state.auth.recoveryErrorCode || state.auth.recoveryErrorMessage}>
            <div class="badge-row">
              <Badge class="badge badge--warn">{`recoveryError=${state.auth.recoveryErrorCode || "-"}`}</Badge>
              <Badge>{state.auth.recoveryErrorMessage || "-"}</Badge>
            </div>
          </Show>
          <Show when={showRebindNotice()}>
            <div class="badge-row">
              <Badge class="badge badge--accent">rebind</Badge>
              <Badge>{`target=${state.auth.pendingRequestedAgentId || "-"}`}</Badge>
              <Badge>{state.auth.pendingForceRebind ? "mode=force_rebind" : "mode=awaiting_retry"}</Badge>
            </div>
            <EmptyState>
              {tr(
                locale(),
                "玩家会话正在切换到请求的行动体；注册成功后，当前动作会继续执行。",
                "Player session is switching to the requested agent and the current action will continue after registration succeeds.",
              )}
            </EmptyState>
          </Show>
          <Show when={state.hostedAdmission}>
            {(admission) => (
              <div class="badge-row">
                <Badge>{`activeSlots=${admission().active_player_sessions}/${admission().max_player_sessions}`}</Badge>
                <Badge>
                  {`effectiveSlots=${
                    admission().effective_player_sessions == null
                      ? "-"
                      : `${admission().effective_player_sessions}/${admission().max_player_sessions}`
                  }`}
                </Badge>
                <Badge>{`runtimeBound=${admission().runtime_bound_player_sessions ?? "-"}`}</Badge>
                <Badge>{`runtimeOnly=${admission().runtime_only_player_sessions ?? "-"}`}</Badge>
                <Badge>{`runtimeProbe=${admission().runtime_probe_status || "-"}`}</Badge>
                <Badge>{`issueBudget=${admission().remaining_issue_budget}`}</Badge>
                <Badge>{`leaseTTL=${admission().slot_lease_ttl_ms}`}</Badge>
                <Badge>{`issued=${admission().issued_players_total}`}</Badge>
                <Badge>{`released=${admission().released_players_total}`}</Badge>
              </div>
            )}
          </Show>
          <Show when={state.hostedAdmission?.runtime_probe_error}>
            <div class="badge-row">
              <Badge class="badge badge--warn">{`runtimeProbeError=${state.hostedAdmission.runtime_probe_error}`}</Badge>
            </div>
          </Show>
            <PanelSection title={tr(locale(), "会话阶梯", "Session Ladder")}>
            <EmptyState>{authSurface().currentTierReason}</EmptyState>
            <div class="event-list">
              <For each={authSurface().tiers}>
                {(tier) => (
                  <EventCard title={tier.label} badge={tier.status} badgeClass={tierBadgeClass(tier.status)} meta={tier.reason} />
                )}
              </For>
            </div>
            <div class="badge-row">
              <Badge class={authSurface().capabilities.prompt_control.enabled ? "badge badge--good" : "badge badge--warn"}>
                {`prompt=${
                  authSurface().capabilities.prompt_control.enabled
                    ? "enabled"
                    : authSurface().capabilities.prompt_control.code
                }`}
              </Badge>
              <Badge class={authSurface().capabilities.agent_chat.enabled ? "badge badge--good" : "badge badge--warn"}>
                {`chat=${
                  authSurface().capabilities.agent_chat.enabled
                    ? "enabled"
                    : authSurface().capabilities.agent_chat.code
                }`}
              </Badge>
              <Badge class="badge badge--warn">
                {`mainToken=${authSurface().capabilities.main_token_transfer.code}`}
              </Badge>
            </div>
            <EmptyState>{authSurface().reconnect}</EmptyState>
          </PanelSection>
          <Show when={hostedActionMatrixView().length > 0}>
            <PanelSection title={tr(locale(), "托管动作矩阵", "Hosted Action Matrix")}>
              <EmptyState>
                {tr(
                  locale(),
                  "这里是启动器导出的托管公开加入真值面。质检应直接读取这些动作编号，而不是只靠按钮状态推断。",
                  "This is the hosted public-join truth surface exported by the launcher. QA should read these action ids directly instead of inferring from button state alone.",
                )}
              </EmptyState>
              <div class="event-list">
                <For each={hostedActionMatrixView()}>
                  {(item) => (
                    <EventCard
                      title={item.actionId}
                      badge={item.enabled ? "enabled" : item.code || "blocked"}
                      badgeClass={item.enabled ? "badge badge--good" : "badge badge--warn"}
                      meta={`required_auth=${item.requiredAuth} · availability=${item.availability}`}
                    >
                      <EmptyState>{item.reason || "-"}</EmptyState>
                      <Show when={item.capabilityReason && item.capabilityReason !== item.reason}>
                        <EmptyState>{`viewer=${item.capabilityReason}`}</EmptyState>
                      </Show>
                    </EventCard>
                  )}
                </For>
              </div>
            </PanelSection>
          </Show>
          <div class="summary-grid">
            <MetricCard label={tr(locale(), "提示词反馈", "Prompt Feedback")} value={promptFeedback()?.stage || "idle"}>
              <Show when={promptFeedbackDisplay()}>
                <Badge class={promptFeedbackDisplay().badgeClass}>
                  {promptFeedbackDisplay().label}
                </Badge>
              </Show>
            </MetricCard>
            <MetricCard label={tr(locale(), "聊天反馈", "Chat Feedback")} value={chatFeedback()?.stage || "idle"}>
              <Show when={chatFeedbackDisplay()}>
                <Badge class={chatFeedbackDisplay().badgeClass}>
                  {chatFeedbackDisplay().label}
                </Badge>
              </Show>
            </MetricCard>
          </div>
          <div>
            <div class="panel__title panel__title--spaced">{tr(locale(), "最近事件", "Recent Events")}</div>
            <div class="event-list">
              <Show when={state.recentEvents.length > 0} fallback={<EmptyState>{tr(locale(), "等待实时事件…", "Waiting for live events…")}</EmptyState>}>
                <For each={state.recentEvents}>
                  {(event) => (
                    <EventCard
                      title={core.summarizeEventTitle(event)}
                      badge={`#${Number(event.id || 0)}`}
                      meta={`time=${Number(event.time || 0)}`}
                    >
                      <JsonBlock value={event.kind} />
                    </EventCard>
                  )}
                </For>
              </Show>
            </div>
          </div>
    </>
  );
}

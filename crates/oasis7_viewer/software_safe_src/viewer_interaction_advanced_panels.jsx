import { Show } from "solid-js";
import * as core from "./legacy_core.js";
import { Badge, EmptyState } from "./viewer_ui_primitives.jsx";
import { isHostedPublicJoinDeploymentMode } from "./software_safe_constants.js";

function tr(locale, zh, en) {
  return core.isLocaleZh(locale) ? zh : en;
}

export function InteractionAdvancedPanels(props) {
  const {
    locale,
    PanelSection,
    FeedbackCard,
    promptVersionState,
    promptOverridesVisible,
    promptSettingsSummary,
    canControlSelectedAgent,
    promptSettingsButtonLabel,
    promptControlsEnabled,
    promptControlDisabledReason,
    authSurface,
    promptFeedback,
    promptFeedbackDisplay,
    promptRecoveryRequired,
    mainTokenTransferCapability,
    assetLaneStatusText,
    mainTokenTransferPolicy,
    assetLaneDetail,
  } = props;
  return (
    <>
      <details class="diagnostic command-surface__advanced-details">
        <summary>{tr(locale(), "高级提示词设置", "Advanced Prompt Settings")}</summary>
        <PanelSection class="command-surface__advanced-panel" title={tr(locale(), "高级控制", "Advanced Controls")}>
        <div class="badge-row">
          <Badge>{`activePrompt=v${promptVersionState().currentVersion}`}</Badge>
          <Badge>{`nextRollback=v${promptVersionState().nextRollbackTargetVersion}`}</Badge>
          <Show when={promptVersionState().restoredFromVersion != null}>
            <Badge>{`restoredFrom=v${promptVersionState().restoredFromVersion}`}</Badge>
          </Show>
          <Badge class={promptOverridesVisible() ? "badge badge--good" : "badge"}>
            {promptOverridesVisible()
              ? tr(locale(), "状态=已展开", "state=expanded")
              : tr(locale(), "状态=默认收起", "state=hidden_by_default")}
          </Badge>
          <Badge>{tr(locale(), "本地设置持久化", "locally persisted")}</Badge>
        </div>
        <EmptyState>{promptSettingsSummary()}</EmptyState>
        <div class="toolbar">
            <button
              data-prompt-visibility-toggle="1"
              disabled={!canControlSelectedAgent()}
              onClick={() => core.togglePromptOverridesVisible()}
            >
            {promptSettingsButtonLabel()}
          </button>
        </div>
        </PanelSection>
      <Show when={promptOverridesVisible()}>
        <PanelSection class="command-surface__prompt-panel" title={tr(locale(), "提示词覆盖", "Prompt Overrides")}>
          <div class="feedback-detail">{promptVersionState().summary}</div>
          <div class="feedback-detail">{promptVersionState().detail}</div>
          <Show when={!promptControlsEnabled()}>
            <div class="feedback-detail" data-prompt-readiness="blocked" role="status" aria-live="polite">{promptControlDisabledReason()}</div>
          </Show>
          <Show
            when={
              authSurface().capabilities.prompt_control.enabled
              && isHostedPublicJoinDeploymentMode(core.state.hostedAccess?.deployment_mode)
            }
          >
            <div class="field">
              <label for="strong-auth-approval-code">{tr(locale(), "后端审批码", "Backend Approval Code")}</label>
              <input
                id="strong-auth-approval-code"
                type="password"
                autocomplete="off"
                value={core.state.strongAuth.approvalCode || ""}
                onInput={(event) => {
                  core.state.strongAuth.approvalCode = String(event.currentTarget.value || "");
                }}
              />
            </div>
          </Show>
          <div class="field">
            <label for="prompt-system">{tr(locale(), "系统提示词覆盖", "System Prompt Override")}</label>
            <textarea
                id="prompt-system"
                rows="4"
                disabled={!promptControlsEnabled()}
                value={core.state.promptDraft.systemPrompt}
              onInput={(event) => {
                core.state.promptDraft.systemPrompt = String(event.currentTarget.value || "");
                core.state.promptDraft.dirty = true;
              }}
            />
          </div>
          <div class="field">
            <label for="prompt-short">{tr(locale(), "短期目标覆盖", "Short-Term Goal Override")}</label>
            <textarea
                id="prompt-short"
                rows="3"
                disabled={!promptControlsEnabled()}
                value={core.state.promptDraft.shortTermGoal}
              onInput={(event) => {
                core.state.promptDraft.shortTermGoal = String(event.currentTarget.value || "");
                core.state.promptDraft.dirty = true;
              }}
            />
          </div>
          <div class="field">
            <label for="prompt-long">{tr(locale(), "长期目标覆盖", "Long-Term Goal Override")}</label>
            <textarea
                id="prompt-long"
                rows="3"
                disabled={!promptControlsEnabled()}
                value={core.state.promptDraft.longTermGoal}
              onInput={(event) => {
                core.state.promptDraft.longTermGoal = String(event.currentTarget.value || "");
                core.state.promptDraft.dirty = true;
              }}
            />
          </div>
          <div class="toolbar">
              <button
                data-prompt-action="preview"
                disabled={!promptControlsEnabled()}
                onClick={() => core.sendPromptControl("preview", null)}
            >
              {tr(locale(), "预览提示词", "Preview Prompt")}
            </button>
              <button
                data-prompt-action="apply"
                disabled={!promptControlsEnabled()}
                onClick={() => core.sendPromptControl("apply", null)}
            >
              {tr(locale(), "应用提示词", "Apply Prompt")}
            </button>
          </div>
          <div class="toolbar command-surface__rollback-actions">
            <div class="field field--inline-flex">
              <label for="prompt-rollback-version">{tr(locale(), "下一次回滚目标版本", "Next Rollback Target Version")}</label>
              <input
                id="prompt-rollback-version"
                  type="number"
                  min="0"
                  step="1"
                  disabled={!promptControlsEnabled()}
                value={Number(core.state.promptDraft.rollbackTargetVersion || 0)}
                onInput={(event) => {
                  const nextValue = Number(event.currentTarget.value || 0);
                  core.state.promptDraft.rollbackTargetVersion = Math.max(0, Math.floor(nextValue || 0));
                  core.requestRender();
                }}
              />
            </div>
              <button
                data-prompt-action="rollback"
                disabled={!promptControlsEnabled()}
                onClick={() => {
                core.sendPromptControl("rollback", {
                  toVersion: Number(core.state.promptDraft.rollbackTargetVersion || 0),
                });
              }}
            >
              {tr(locale(), "回滚提示词", "Rollback Prompt")}
            </button>
          </div>
          <Show when={promptFeedback()} fallback={<EmptyState>{tr(locale(), "还没有提示词反馈。", "No prompt feedback yet.")}</EmptyState>}>
            {(feedback) => <FeedbackCard feedback={feedback()} display={promptFeedbackDisplay()} />}
          </Show>
          <Show when={promptRecoveryRequired()}>
            <div class="toolbar" data-prompt-recovery="binding"><button type="button" data-testid="prompt-recovery-cta" data-prompt-recovery-action="refresh-binding" onClick={() => void core.refreshPromptControlBinding()}>{tr(locale(), "刷新权限与 Agent 绑定", "Refresh authority and Agent binding")}</button></div>
          </Show>
          <Show when={core.state.strongAuth.lastGrantActionId}>
            <EmptyState>
              {`lastGrant=${core.state.strongAuth.lastGrantActionId} expiresAt=${
                core.state.strongAuth.lastGrantExpiresAtUnixMs || "-"
              }`}
            </EmptyState>
          </Show>
          <Show when={core.state.strongAuth.lastGrantError}>
            <EmptyState class="empty--danger">{core.state.strongAuth.lastGrantError}</EmptyState>
          </Show>
        </PanelSection>
      </Show>
      </details>
      <details class="diagnostic command-surface__asset-details">
        <summary>{tr(locale(), "资产 / 治理通道", "Asset / Governance Lane")}</summary>
        <PanelSection class="command-surface__asset-panel" title={tr(locale(), "后置能力", "Deferred Surface")}>
        <div class="badge-row">
          <Badge class={mainTokenTransferCapability().enabled ? "badge badge--good" : "badge badge--warn"}>
            {`main_token_transfer=${assetLaneStatusText()}`}
          </Badge>
          <Badge>{`required_auth=${mainTokenTransferPolicy()?.required_auth || "-"}`}</Badge>
          <Badge>{`availability=${mainTokenTransferPolicy()?.availability || "-"}`}</Badge>
        </div>
        <EmptyState>{assetLaneDetail()}</EmptyState>
        <EmptyState>
          {mainTokenTransferPolicy()?.reason
            || tr(locale(), "当前通道没有 main_token_transfer 的托管动作策略。", "No hosted action policy is available for main_token_transfer on this lane.")}
        </EmptyState>
        <div class="toolbar">
          <button disabled>{tr(locale(), "主代币转账（这里暂未开放）", "Main Token Transfer (Not Exposed Here Yet)")}</button>
        </div>
        </PanelSection>
      </details>
    </>
  );
}

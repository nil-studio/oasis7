import { projectPixelWorldRenderInput } from "./pixel_world_render_input.js";
import { createPixelWorldSpatialController } from "./pixel_world_spatial_controller.js";
import { PixelWorldSpatialExplorer } from "./pixel_world_spatial_explorer.jsx";
import { createPixelWorldScopeGuard } from "./pixel_world_scope.js";
import { createPixelWorldHostAdapter } from "./pixel_world_host_adapter.js";
import { createEffect, createMemo, createSignal, For, Index, Show, onCleanup, onMount, untrack } from "solid-js";
import * as core from "./legacy_core.js";
import { createPixelWorldRuntimeBridge, probePixelWorldWebgl2Surface } from "./pixel_world_runtime_loader.js";
import { installPixelWorldHotspotPointerProbe } from "./pixel_world_hotspot_probe.js"; import { createPixelWorldFocusController } from "./pixel_world_focus_controller.js";
import { applyPixelWorldRendererRoute, createPixelWorldRendererRouteSignals, resolvePixelWorldRendererRoute } from "./pixel_world_renderer_route.js";
import { installPixelWorldRenderDtoProbe, installPixelWorldVisualFixtureHook, pixelWorldTestApiEnabled } from "./pixel_world_visual_fixture.js";
import { pixelWorldSelectedBlockerVisualFixture } from "./pixel_world_visual_fixture_data.js";
import { pixelWorldReadableAgentLabel, pixelWorldReadableEntityText, pixelWorldSelectedEntityLabel } from "./pixel_world_identity.js";
import { PixelWorldCanvasAgentHitTargets, PixelWorldCanvasLegend, PixelWorldHostVisualLayer, PixelWorldSparseSceneGuidance, arrayField, fieldValue, pixelWorldVisualState } from "./pixel_world_visual_clarity.jsx";
import { applyPixelWorldMobileSelectionSafeArea, installPixelWorldMobileSelectionSafeArea } from "./pixel_world_mobile_safe_area.js";
import { createHotspotFocusRestoration, PixelWorldHotspot, PixelWorldHotspotTooltip } from "./pixel_world_hotspot.jsx";
import { resolvePixelWorldReadoutStatus } from "./pixel_world_readout.js";
import { pixelWorldBlockerPresentation, pixelWorldConnectionPresentation, pixelWorldFeedFreshnessPresentation } from "./pixel_world_presentation.js";
import { pixelWorldHotspotGlyphSize, pixelWorldHotspotStyle } from "./pixel_world_hotspot_projection.js";
import { PixelWorldRendererTargets } from './pixel_world_renderer_targets.jsx';
import { focusViewerPanel } from "./viewer_navigation.jsx";
export { pixelWorldSelectedBlockerVisualFixture };
function tr(locale, zh, en) { return core.isLocaleZh(locale) ? zh : en; }
async function waitForRuntimeCanvasAttachment(canvas) {
  for (let attempt = 0; attempt < 12; attempt += 1) {
    if (canvas?.isConnected && document.getElementById(PIXEL_WORLD_RUNTIME_CANVAS_ID) === canvas) {
      return true;
    }
    await new Promise((resolve) => requestAnimationFrame(() => resolve()));
  }
  return false;
}
function pixelWorldAgentIdentity(agent, fallbackId = "", isLocaleZh = false) {
  return pixelWorldReadableAgentLabel(agent, fallbackId, isLocaleZh) || "Agent";
}
const PIXEL_WORLD_RUNTIME_CANVAS_ID = "pixel-world-embedded-runtime-canvas"; const PIXEL_WORLD_RENDERER_UNAVAILABLE_MESSAGE_ID = "pixel-world-renderer-unavailable-message";
const pixelWorldFocusUiSessionState = {
  focusMode: false,
  commandDrawerOpen: false,
  diagnosticsDrawerOpen: false,
  maximized: false,
};
function PixelWorldHostHotspotLayer(props) {
  const visualState = () => pixelWorldVisualState(props.renderState());
  return <Index each={visualState().visualHotspots.slice(0, 8)}>{(hotspot, index) => (
    <PixelWorldHotspot locale={props.locale()} hotspot={hotspot()}
      style={pixelWorldHotspotStyle(hotspot(), visualState().worldBounds, index, props.cameraState?.(), props.stageSize?.())}
      rendererProjection={props.rendererProjection?.()}
      glyphSize={pixelWorldHotspotGlyphSize(hotspot())}
      onHover={props.onHover}
      onHotspotInspect={props.onHotspotInspect} onHotspotClear={props.onHotspotClear}
      onHotspotHoverIntent={props.onHotspotHoverIntent}
      onHotspotTrigger={props.onHotspotTrigger} onHotspotRestoreFocus={props.onHotspotRestoreFocus} />
  )}</Index>;
}
export function buildPixelWorldRenderInput(locale = core.state.uiLocale) {
  const worldScaleSurface = core.buildWorldScaleSurface(locale);
  return {
    locale,
    snapshot: core.state.snapshot,
    lists: core.modelLists(),
    gameplay: core.buildGameplaySummary(locale),
    selected: core.state.selectedObject,
    selectedKind: core.state.selectedKind,
    selectedId: core.state.selectedId,
    presentation: {
      world_bounds_label: worldScaleSurface.physicalTruth.worldBoundsLabel,
      marker_truth_note: worldScaleSurface.presentationScale.markerTruthNote,
    },
  };
}
export function PixelWorldCanvasRenderer(props) {
  let canvasRef;
  const [stageSize, setStageSize] = createSignal({ width: 960, height: 540 });
  const [rendererDimensions, setRendererDimensions] = createSignal({ width: 960, height: 540 });
  let metricRefreshGeneration = 0;
  const updateCanvasMetrics = () => {
    const rect = canvasRef?.getBoundingClientRect();
    if (!rect?.width || !rect?.height) return;
    const nextStage = { width: rect.width, height: rect.height };
    const nextRenderer = {
      width: Number(canvasRef?.width) || rect.width,
      height: Number(canvasRef?.height) || rect.height,
    };
    setStageSize((previous) => previous.width === nextStage.width && previous.height === nextStage.height ? previous : nextStage);
    setRendererDimensions((previous) => previous.width === nextRenderer.width && previous.height === nextRenderer.height ? previous : nextRenderer);
  };
  const scheduleCanvasMetricsRefresh = () => {
    const generation = ++metricRefreshGeneration;
    let remainingFrames = 2;
    const refresh = () => {
      if (generation !== metricRefreshGeneration) return;
      updateCanvasMetrics();
      remainingFrames -= 1;
      if (remainingFrames > 0) requestAnimationFrame(refresh);
    };
    requestAnimationFrame(refresh);
  };
  const rendererSize = () => rendererDimensions();
  onMount(() => {
    updateCanvasMetrics();
    const frame = requestAnimationFrame(updateCanvasMetrics);
    let resizeObserver;
    if (typeof ResizeObserver !== 'undefined') {
      resizeObserver = new ResizeObserver(updateCanvasMetrics);
      resizeObserver.observe(canvasRef);
    }
    const mutationObserver = typeof MutationObserver === 'undefined' ? null : new MutationObserver(updateCanvasMetrics);
    mutationObserver?.observe(canvasRef, { attributes: true, attributeFilter: ['width', 'height'] });
    onCleanup(() => {
      metricRefreshGeneration += 1;
      cancelAnimationFrame(frame);
      resizeObserver?.disconnect();
      mutationObserver?.disconnect();
    });
  });
  const hotspotFocus = createHotspotFocusRestoration();
  const visualState = () => pixelWorldVisualState(props.renderState());
  const [inspectedHotspot, setInspectedHotspot] = createSignal(null);
  const [hoverDismissed, setHoverDismissed] = createSignal(false);
  const activeHotspot = () => {
    const inspected = inspectedHotspot();
    if (inspected?.kind === "hotspot") {
      return visualState().visualHotspots.find((hotspot) => hotspot.id === inspected.id) || null;
    }
    return hoverDismissed() ? null : props.hoveredHotspot?.() || null;
  };
  const selectedEntityLabel = () => pixelWorldSelectedEntityLabel(visualState(), visualState().selection, core.isLocaleZh(props.locale()));
  onMount(() => onCleanup(installPixelWorldMobileSelectionSafeArea(() => canvasRef?.closest(".pixel-world-canvas"))));
  createEffect(() => {
    props.cameraState?.();
    props.rendererStatus?.();
    props.renderState?.();
    stageSize();
    updateCanvasMetrics();
    scheduleCanvasMetricsRefresh();
    requestAnimationFrame(() => applyPixelWorldMobileSelectionSafeArea(canvasRef?.closest('.pixel-world-canvas')));
  });
  createEffect(() => {
    if (!canvasRef) {
      return;
    }
    props.onCanvasMount?.(canvasRef);
  });
  createEffect(() => {
    props.renderInput?.();
    if (!canvasRef) {
      return;
    }
    props.onCanvasUpdate?.();
    requestAnimationFrame(() => applyPixelWorldMobileSelectionSafeArea(canvasRef?.closest(".pixel-world-canvas")));
  });
  return (
    <div class="pixel-world-canvas pixel-world-canvas--rendered" data-renderer-projection={props.rendererProjection?.() ? 'true' : undefined} data-renderer-ready={props.rendererStatus?.() === "ready" ? "true" : undefined}>
      <canvas
        ref={canvasRef}
        id={PIXEL_WORLD_RUNTIME_CANVAS_ID}
        class="pixel-world-canvas__surface"
        tabIndex="0"
        role="img"
        aria-label={tr(props.locale(), "世界 Canvas 概览", "World canvas overview")}
        aria-describedby="pixel-world-canvas-accessible-summary"
        width="960"
        height="540"
      />
      <div id="pixel-world-canvas-accessible-summary" class="sr-only">
        {tr(
          props.locale(),
          "Canvas 提供当前世界的只读概览；相邻 HUD、焦点栏和命令抽屉提供当前 Agent、阻塞、回执与命令路径。",
          "The canvas provides a read-only overview of the current world; adjacent HUD, focus rail, and command drawer expose the current agent, blocker, receipt, and command path.",
        )}
      </div>
      <div class="pixel-world-canvas__overlay">
        <Show when={props.rendererProjection?.()} fallback={<>
        <PixelWorldCanvasAgentHitTargets
          locale={props.locale} renderState={props.renderState} selection={props.selection}
          hovered={props.hoveredEntity}
          onSelect={props.onSelect}
          onHover={props.onHover}
        />
        <PixelWorldHostVisualLayer
          enabled={props.visualOverlayEnabled}
          locale={props.locale}
          renderState={props.renderState}
          selection={props.selection}
          hovered={props.hoveredEntity}
          onSelect={props.onSelect}
          onHover={props.onHover}
        />
        </>}>
          <PixelWorldRendererTargets locale={props.locale} renderState={props.renderState} selection={props.selection} cameraState={props.cameraState} stageSize={stageSize} rendererSize={rendererSize} onSelect={props.onSelect} onHover={props.onHover} />
        </Show>
        <PixelWorldHostHotspotLayer locale={props.locale} renderState={props.renderState} cameraState={props.cameraState}
          rendererProjection={props.rendererProjection} stageSize={() => props.rendererProjection?.() ? stageSize() : undefined}
          onHover={props.onHover} onHotspotInspect={(selection) => { setHoverDismissed(false); setInspectedHotspot(selection); }}
          onHotspotHoverIntent={() => setHoverDismissed(false)}
          onHotspotClear={() => { setHoverDismissed(true); setInspectedHotspot(null); }}
          onHotspotTrigger={hotspotFocus.remember} onHotspotRestoreFocus={hotspotFocus.restore} />
        <Show when={visualState().goalHighlight}>
          <div class="pixel-world-canvas__callout pixel-world-canvas__callout--goal">
            {`${tr(props.locale(), "目标", "Goal")}: ${visualState().goalHighlight.title}`}
          </div>
        </Show>
        <Show when={visualState().blockerHighlight}>
          <div class="pixel-world-canvas__callout pixel-world-canvas__callout--blocker">
            {`${tr(props.locale(), "阻塞", "Blocker")}: ${pixelWorldBlockerPresentation(visualState().blockerHighlight.kind, props.locale()).label}`}
          </div>
        </Show>
        <Show when={activeHotspot()}>
            <PixelWorldHotspotTooltip
              locale={props.locale()}
              hotspot={activeHotspot()}
              onHoverLeave={() => props.onHover(null)}
              onClose={() => {
                setHoverDismissed(true);
                setInspectedHotspot(null);
                props.onHover(null);
                hotspotFocus.restore();
              }}
            />
        </Show>
      </div>
      <Show when={visualState().selection}>
        <div class="pixel-world-canvas__selection">
          {`${tr(props.locale(), "已选中", "Selected")}: ${selectedEntityLabel()}`}
        </div>
      </Show>
    </div>
  );
}
function PixelWorldActionReceipt(props) {
  const receipt = () => props.surface().action_receipt;
  const receiptChanges = () => [
    receipt().delta_logical_time == null ? null : `${tr(props.locale(), "世界 Tick 变化", "World tick change")} ${receipt().delta_logical_time}`,
    receipt().delta_event_seq == null ? null : `${tr(props.locale(), "事件序列变化", "Event sequence change")} ${receipt().delta_event_seq}`,
  ].filter(Boolean);
  return (
    <div
      id={props.id}
      class={`pixel-world-action-receipt pixel-world-action-receipt--${receipt().state || "unknown"} pixel-world-action-receipt--${receipt().confidence || "none"} ${props.class ?? ""}`} data-viewer-overlay="receipt"
      data-receipt-present={receipt().present ? "true" : "false"}
      data-receipt-state={receipt().state}
      data-receipt-confidence={receipt().confidence}
    >
      <div class="pixel-world-action-receipt__label">
        {tr(props.locale(), "行动回执", "Action Receipt")}
      </div>
      <div class="pixel-world-action-receipt__body">
        <div class="pixel-world-action-receipt__title">
          {receipt().title}
        </div>
        <div class="pixel-world-action-receipt__summary">
          {receipt().summary}
        </div>
        <Show when={receipt().detail}>
          <div class="pixel-world-action-receipt__detail">
            {receipt().detail}
          </div>
        </Show>
        <Show when={receiptChanges().length > 0}>
          <div class="pixel-world-action-receipt__changes" data-receipt-changes="true">
            {receiptChanges().join(" · ")}
          </div>
        </Show>
      </div>
      <Show when={receipt().present}>
        <div class="pixel-world-action-receipt__meta">
          <span>{receiptConfidenceLabel(receipt().confidence, props.locale(), receipt().state)}</span>
          <Show when={receipt().target_agent_id}>
            <span>{`${tr(props.locale(), "行动体", "Agent")} ${String(receipt().target_agent_id).replace(/^agent[-_]/i, "")}`}</span>
          </Show>
        </div>
      </Show>
    </div>
  );
}
function receiptConfidenceLabel(confidence, locale, state) {
  const value = String(confidence || "").trim().toLowerCase(); const receiptState = String(state || "").trim().toLowerCase();
  if (receiptState === "blocked") return tr(locale, "行动被阻塞", "Action blocked");
  if (receiptState === "rejected") return tr(locale, "行动已拒绝", "Action rejected");
  return value === "world_delta" ? tr(locale, "世界变化已确认", "World change confirmed") : value === "accepted_intent" ? tr(locale, "行动已接受", "Action accepted") : value === "none" ? tr(locale, "等待确认", "Waiting for confirmation") : tr(locale, "状态已记录", "Status recorded");
}
function worldReadoutStatus(locale, renderState) {
  renderState?.();
  return resolvePixelWorldReadoutStatus(locale, core.state.connectionStatus, core.state.worldFeed);
}
const DIRECT_PIXEL_WORLD_NEXT_MOVE_KINDS = new Set(["claim_first_agent", "claim_starter_oc"]);
const PIXEL_WORLD_PENDING_GAMEPLAY_STAGES = new Set(["accepted", "submitted", "queued", "ack", "registering", "signing", "sent"]);
const PIXEL_WORLD_BUSY_GAMEPLAY_STAGES = new Set(["queued", "registering", "signing", "sent"]);
function pixelWorldGameplayActionKey(action) { if (!action) return ""; const actionId = String(action.actionId || action.action_id || action.protocolAction || action.protocol_action || action.executeKind || "").trim().toLowerCase(); const targetAgentId = String(action.targetAgentId || action.target_agent_id || action.actorAgentId || action.actor_agent_id || "").trim().toLowerCase(); return `${actionId}::${targetAgentId}`; }
function pixelWorldGameplayActionMatches(action, feedback) { if (!action || !feedback || feedback.kind !== "gameplay_action") return false; const actionId = String(action.actionId || action.action_id || action.protocolAction || action.protocol_action || action.executeKind || "").trim().toLowerCase(); const feedbackAction = String(feedback.action || "").trim().toLowerCase(); const targetAgentId = String(action.targetAgentId || action.target_agent_id || action.actorAgentId || action.actor_agent_id || "").trim().toLowerCase(); const feedbackAgentId = String(feedback.agentId || feedback.targetAgentId || "").trim().toLowerCase(); return Boolean(actionId && feedbackAction && feedbackAction.includes(actionId) && (!targetAgentId || !feedbackAgentId || targetAgentId === feedbackAgentId)); }
function pixelWorldNextMoveDisabledReason(action, gameplay, locale) {
  if (!action) return null; if (action.disabledReason) return action.disabledReason;
  const feedback = core.snapshotSemanticFeedback(core.state.lastGameplayActionFeedback) || gameplay?.recentFeedback; const feedbackStage = String(feedback?.stage || "").trim().toLowerCase();
  if (pixelWorldGameplayActionMatches(action, feedback) && PIXEL_WORLD_PENDING_GAMEPLAY_STAGES.has(feedbackStage)) return tr(locale, "动作已提交，正在等待世界确认。", "Action submitted; waiting for world confirmation.");
  const actionId = String(action.actionId || action.action_id || "").trim().toLowerCase(); const protocolAction = String(action.protocolAction || action.protocol_action || "").trim().toLowerCase(); const targetAgentId = String(action.targetAgentId || action.target_agent_id || "").trim(); const boundAgentId = String(core.state.auth.boundAgentId || "").trim();
  if (targetAgentId && actionId !== "claim_first_agent" && !["request_snapshot", "live_control.step", "live_control.play"].includes(protocolAction)) { if (!boundAgentId) return tr(locale, "当前账号尚未绑定 Agent，不能执行共享快照动作。", "The current account has no bound Agent for this shared-snapshot action."); if (targetAgentId !== boundAgentId) return tr(locale, `这个动作目标是 ${targetAgentId}，但当前账号绑定的是 ${boundAgentId}。`, `This action targets ${targetAgentId}, but the current account is bound to ${boundAgentId}.`); }
  return null;
}
function pixelWorldNextMovePending(action, gameplay) { if (!action) return false; const feedback = core.snapshotSemanticFeedback(core.state.lastGameplayActionFeedback) || gameplay?.recentFeedback; const feedbackStage = String(feedback?.stage || "").trim().toLowerCase(); const key = pixelWorldGameplayActionKey(action); return Boolean((key && core.state.gameplayActionPending?.actionKey === key) || (pixelWorldGameplayActionMatches(action, feedback) && PIXEL_WORLD_BUSY_GAMEPLAY_STAGES.has(feedbackStage))); }
export function resolvePixelWorldDirectNextMoveAction(gameplay, executeKind, targetAgentId = gameplay?.recommendedAction?.targetAgentId ?? null) {
  if (!DIRECT_PIXEL_WORLD_NEXT_MOVE_KINDS.has(executeKind)) {
    return null;
  }
  const actions = Array.isArray(gameplay?.availableActions) ? gameplay.availableActions : [];
  return actions.find((action) => (
    action?.executeKind === executeKind
    && (action.targetAgentId ?? action.target_agent_id ?? null) === targetAgentId
    && !action?.disabledReason
  )) || actions.find((action) => (
    String(action?.actionId || action?.action_id || "") === executeKind
    && (action.targetAgentId ?? action.target_agent_id ?? null) === targetAgentId
    && !action?.disabledReason
  )) || null;
}
function PixelWorldCommercialHud(props) {
  const surface = () => props.renderState().commercial_surface; const readoutStatus = () => worldReadoutStatus(props.locale(), props.renderState);
  const readoutFeedStatus = () => String(core.state.worldFeed?.status || "loading").trim().toLowerCase() || "loading";
  const worldConnection = () => pixelWorldConnectionPresentation(core.state.connectionStatus, props.locale());
  const feedFreshness = () => pixelWorldFeedFreshnessPresentation(readoutFeedStatus(), core.state.worldFeed?.stale, props.locale());
  const playerBlockerLabel = () => {
    const candidate = String(surface().blocker?.label || "").trim();
    const kind = String(gameplay()?.blockerKind || "").trim();
    if (kind) return pixelWorldBlockerPresentation(kind, props.locale()).label;
    return candidate || null;
  };
  const activeAgentId = () => String(surface()?.active_agent_id || "").trim(); const activeAgent = () => props.renderState().agents.find((agent) => agent.id === activeAgentId()); const activeAgentLabel = () => pixelWorldReadableAgentLabel(activeAgent(), activeAgentId(), core.isLocaleZh(props.locale())) || tr(props.locale(), "未选择 Agent", "No Agent selected");
  const executableNextMoveKinds = new Set([
    "gameplay_action",
    "claim_first_agent",
    "claim_starter_oc",
    "step",
    "play",
    "request_snapshot",
  ]);
  const nextMoveRoutesToGameplayDetails = () => executableNextMoveKinds.has(surface().next_action.execute_kind);
  const nextMoveRoute = () => nextMoveRoutesToGameplayDetails() ? "gameplay_details" : "command";
  const nextMoveHref = () => nextMoveRoutesToGameplayDetails() ? "#viewer-gameplay-details" : "#viewer-details-panel";
  const directNextMoveAction = () => resolvePixelWorldDirectNextMoveAction(
    core.buildGameplaySummary(props.locale()),
    surface().next_action.execute_kind,
    surface().next_action.target_agent_id ?? null,
  ) || (surface().next_action.execute_kind === "gameplay_action"
    ? core.buildGameplaySummary(props.locale()).availableActions.find((action) => (
      DIRECT_PIXEL_WORLD_NEXT_MOVE_KINDS.has(action?.actionId)
      && (action.targetAgentId ?? action.target_agent_id ?? null) === (surface().next_action.target_agent_id ?? null)
      && action?.label === surface().next_action.label
      && !action?.disabledReason
    ))
    : null);
  const gameplay = () => core.buildGameplaySummary(props.locale());
  const nextMoveDisabledReason = () => pixelWorldNextMoveDisabledReason(directNextMoveAction(), gameplay(), props.locale());
  const nextMovePending = () => pixelWorldNextMovePending(directNextMoveAction(), gameplay());
  const openGameplayDetails = () => {
    if (!nextMoveRoutesToGameplayDetails()) {
      return;
    }
    const details = document.getElementById("viewer-gameplay-details");
    if (details) {
      details.open = true;
    }
  };
  const activateNextMove = (event) => {
    event?.preventDefault?.();
    event?.stopPropagation?.();
    if (nextMoveDisabledReason()) {
      return;
    }
    openGameplayDetails();
    const action = directNextMoveAction();
    if (action) {
      core.sendGameplayAction(action);
    } else if (nextMoveHref().startsWith("#")) {
      window.location.hash = nextMoveHref();
    }
  };
  return (
    <Show when={surface()}>
      <div id="viewer-decision-area" class="pixel-world-decision-area" data-viewer-decision-area="true">
        <div
          class="pixel-world-command-strip" data-viewer-overlay="next-move"
          data-active-agent={activeAgentId()}
          data-leverage-state={surface().player_leverage.state}
        >
        <div
          class="pixel-world-command-cell pixel-world-command-cell--next pixel-world-shell-region pixel-world-shell-region--primary"
          data-shell-region="next-move-primary"
          data-next-move-route={nextMoveRoute()}
          data-execute-kind={surface().next_action.execute_kind || "none"}
          data-blocker-present={playerBlockerLabel() ? "true" : "false"}
        >
          <div class="pixel-world-command-cell__header">
            <div class="pixel-world-command-cell__label">
              {tr(props.locale(), "下一步", "Next Move")}
            </div>
            <Show when={playerBlockerLabel()}>
              <span class="pixel-world-command-cell__blocker-chip">
                {`${tr(props.locale(), "阻塞", "Blocker")}: ${playerBlockerLabel()}`}
              </span>
            </Show>
          </div>
          <div class="pixel-world-command-cell__value">{surface().next_action.label}</div>
          <Show when={surface().next_action.detail}>
            <div class="pixel-world-command-cell__detail">{surface().next_action.detail}</div>
          </Show>
          <a
            class="pixel-world-command-cell__action"
            data-primary-action="true"
            href={nextMoveHref()}
            aria-label={`${tr(props.locale(), "下一步", "Next Move")}: ${surface().next_action.label}`}
            aria-disabled={nextMoveDisabledReason() ? "true" : undefined}
            aria-busy={nextMovePending() ? "true" : "false"}
            aria-live="polite"
            tabIndex={nextMoveDisabledReason() ? -1 : undefined}
            onClick={activateNextMove}
          >
            {directNextMoveAction()
              ? surface().next_action.label
              : nextMoveRoutesToGameplayDetails()
                ? tr(props.locale(), "打开玩法明细", "Open Gameplay Details")
              : tr(props.locale(), "去指挥面板", "Go to Command")}
          </a>
          <Show when={nextMovePending() || surface().action_receipt?.present}>
            <div class="pixel-world-command-cell__feedback" data-primary-action-feedback="true" aria-live="polite">
              {nextMovePending()
                ? tr(props.locale(), "动作已提交，等待世界确认。", "Action submitted; waiting for world confirmation.")
                : surface().action_receipt?.title}
            </div>
          </Show>
        </div>
        <div class="pixel-world-command-cell pixel-world-shell-region pixel-world-shell-region--supporting" data-shell-region="supporting-context">
          <div class="pixel-world-shell-context-group pixel-world-shell-context-group--objective">
            <div class="pixel-world-command-cell__label">{tr(props.locale(), "目标", "Objective")}</div>
            <div class="pixel-world-command-cell__value">{surface().objective.title}</div><div class="pixel-world-command-cell__detail">{surface().objective.detail}</div>
          </div>
          <div class="pixel-world-shell-context-group pixel-world-shell-context-group--leverage">
            <div class="pixel-world-command-cell__label">{tr(props.locale(), "玩家杠杆", "Player Leverage")}</div>
            <div class="pixel-world-command-cell__value">{pixelWorldReadableEntityText(surface().player_leverage.summary, props.renderState(), core.isLocaleZh(props.locale()))}</div><div class="pixel-world-command-cell__detail">{surface().player_leverage.label}</div>
            <div class="pixel-world-shell-context-group__agent" data-selected-agent-label={activeAgentLabel()}><span class="pixel-world-command-cell__label">{tr(props.locale(), "当前 Agent", "Selected Agent")}</span><strong>{activeAgentLabel()}</strong></div>
          </div>
        </div>
        </div>
        <Show when={!props.focusMode?.()}><PixelWorldActionReceipt id="viewer-action-receipt" locale={props.locale} surface={surface} /></Show>
        <div class="pixel-world-readout badge-row" aria-label={tr(props.locale(), "世界连接与动态新鲜度", "World connection and feed freshness")}>
          <span class={worldConnection().className} data-world-connection-status={worldConnection().state}>{worldConnection().label}</span>
          <span class={feedFreshness().className} data-world-feed-readout-status={readoutFeedStatus()}>{feedFreshness().label}</span>
          <Show when={surface().world_read.tick !== null && surface().world_read.tick !== undefined}>
            <span class="badge badge--accent" data-world-tick={String(surface().world_read.tick)}>{`tick=${surface().world_read.tick}`}</span>
          </Show>
          <span class="badge badge--accent">{`agents=${surface().world_read.agents}`}</span>
        </div>
        <PixelWorldSparseSceneGuidance locale={props.locale} renderState={props.renderState} />
        <PixelWorldCanvasLegend locale={props.locale} />
      </div>
    </Show>
  );
}
function PixelWorldFocusHud(props) {
  const surface = () => props.renderState().commercial_surface;
  return (
    <Show when={surface()}>
      <div class="pixel-world-focus-hud" data-focus-hud="true">
        <div class="pixel-world-focus-hud__identity">
          <div class="pixel-world-focus-hud__eyebrow">
            {tr(props.locale(), "电影视图", "Cinematic View")}
          </div>
          <div class="pixel-world-focus-hud__title">
            {tr(props.locale(), "世界指挥棋盘", "World Command Board")}
          </div>
        </div>
        <div class="pixel-world-focus-hud__cell pixel-world-focus-hud__cell--prompt">
          <span>{tr(props.locale(), "当前目标", "Current Objective")}</span>
          <strong>{surface().objective.title}</strong>
          <em>{surface().next_action.label}</em>
        </div>
        <div class="pixel-world-focus-hud__cell pixel-world-focus-hud__cell--mission">
          <span>{tr(props.locale(), "任务进度", "Mission Progress")}</span>
          <strong>
            {surface().objective.progress_percent == null
              ? tr(props.locale(), "进行中", "In Progress")
              : `${surface().objective.progress_percent}%`}
          </strong>
          <em>{surface().next_action.detail || surface().objective.detail}</em>
        </div>
        <Show when={surface().world_read.tick !== null && surface().world_read.tick !== undefined}>
          <div
            class="pixel-world-focus-hud__cell pixel-world-focus-hud__cell--tick"
            data-world-tick={String(surface().world_read.tick)}
            data-hud-priority="telemetry"
          >
            <span>{tr(props.locale(), "世界 Tick", "World Tick")}</span>
            <strong>{surface().world_read.tick}</strong>
            <em>{`tick=${surface().world_read.tick}`}</em>
          </div>
        </Show>
        <div
          class="pixel-world-focus-hud__cell pixel-world-focus-hud__cell--blocker"
          data-blocker-present={surface().blocker.label ? "true" : "false"}
          data-hud-priority={surface().blocker.label ? "critical" : "clear"}
        >
          <span>{tr(props.locale(), "阻塞", "Blocker")}</span>
          <strong>{surface().blocker.label || tr(props.locale(), "暂无阻塞", "No blocker")}</strong>
        </div>
        <div class="pixel-world-focus-controls" aria-label={tr(props.locale(), "电影视图控制", "Cinematic controls")}>
          <button type="button" class="pixel-world-focus-control pixel-world-focus-control--primary" onClick={props.onOpenCommand}>
            {tr(props.locale(), "命令与目标", "Command & Target")}
          </button>
          <button id="viewer-focus-exit" type="button" class="pixel-world-focus-control pixel-world-focus-control--quiet" onClick={props.onExit}>{tr(props.locale(), "退出电影视图", "Exit Cinematic")}</button>
          <details class="pixel-world-focus-more-controls">
            <summary>{tr(props.locale(), "更多控制", "More controls")}</summary>
            <button type="button" class="pixel-world-focus-control pixel-world-focus-control--secondary" onClick={props.onOpenDiagnostics}>{tr(props.locale(), "世界状态", "World Status")}</button>
            <button type="button" class="pixel-world-focus-control pixel-world-focus-control--secondary" onClick={props.onToggleMaximized}>
              {props.maximized()
                ? tr(props.locale(), "还原布局", "Restore Layout")
                : tr(props.locale(), "最大化", "Maximize")}
            </button>
          </details>
        </div>
      </div>
    </Show>
  );
}
function PixelWorldFocusCinematicBanner(props) {
  const surface = () => props.renderState().commercial_surface;
  return (
    <Show when={surface()}>
      <div class="pixel-world-focus-cinematic" data-focus-cinematic="true">
        <div class="pixel-world-focus-cinematic__eyebrow">
          {tr(props.locale(), "电影化首屏", "Cinematic Opening")}
        </div>
        <div class="pixel-world-focus-cinematic__title">
          {tr(props.locale(), "工业世界指挥台", "Industrial World Command Board")}
        </div>
        <div class="pixel-world-focus-cinematic__body">
          {surface().objective.detail}
        </div>
        <div class="badge-row">
          <span class="badge badge--accent">{surface().objective.title}</span>
          <Show when={surface().blocker.label}>
            <span class="badge badge--warn">{surface().blocker.label}</span>
          </Show>
        </div>
      </div>
    </Show>
  );
}
function shouldShowFocusCinematic(renderState) {
  const surface = renderState?.commercial_surface;
  if (!surface) {
    return false;
  }
  const hasComparableFocusState = Boolean(
    renderState.selection
      || surface.active_agent_id
      || renderState.links?.length
      || renderState.fragment_terrain?.length
      || surface.blocker?.label
      || surface.action_receipt?.present,
  );
  return !hasComparableFocusState;
}
function PixelWorldFocusRail(props) {
  const surface = () => props.renderState().commercial_surface;
  const selected = () => props.renderState().selection;
  const activeAgent = () => surface()?.active_agent_id || props.renderState().agents[0]?.id || null;
  const activeAgentEntity = () => props.renderState().agents.find((agent) => agent.id === activeAgent());
  const activeAgentName = () => pixelWorldAgentIdentity(activeAgentEntity(), activeAgent(), core.isLocaleZh(props.locale()));
  const selectedName = () => pixelWorldSelectedEntityLabel(props.renderState(), selected(), core.isLocaleZh(props.locale()));
  const routeCount = () => props.renderState().links.length;
  const hasFocusItems = () => Boolean(activeAgent() || selected() || routeCount() > 0);
  return (
    <Show when={surface() && hasFocusItems()}>
      <div class="pixel-world-focus-rail" data-focus-rail="true">
        <div class="pixel-world-focus-rail__label">
          {tr(props.locale(), "焦点", "Focus")}
        </div>
        <Show when={surface()?.blocker.label}>
          <div class="pixel-world-focus-rail__item pixel-world-focus-rail__item--blocker" data-focus-priority="blocker">
            <span>{tr(props.locale(), "阻塞", "Blocker")}</span>
            <strong>{surface().blocker.label}</strong>
          </div>
        </Show>
        <Show when={activeAgent()}>
          <div class="pixel-world-focus-rail__item">
            <span>{tr(props.locale(), "Agent", "Agent")}</span>
            <strong>{activeAgentName()}</strong>
          </div>
        </Show>
        <Show when={selected()}>
          <div class="pixel-world-focus-rail__item">
            <span>{tr(props.locale(), "选中", "Selected")}</span>
            <strong>{selectedName()}</strong>
          </div>
        </Show>
        <Show when={routeCount() > 0}>
          <div class="pixel-world-focus-rail__item">
            <span>{tr(props.locale(), "路线", "Routes")}</span>
            <strong>{routeCount()}</strong>
          </div>
        </Show>
      </div>
    </Show>
  );
}
function PixelWorldFocusMinimapCard(props) {
  const surface = () => props.renderState().commercial_surface;
  const selected = () => props.renderState().selection;
  const activeAgent = () => surface()?.active_agent_id || props.renderState().agents[0]?.id || null;
  const activeAgentEntity = () => props.renderState().agents.find((agent) => agent.id === activeAgent());
  const activeAgentName = () => pixelWorldAgentIdentity(activeAgentEntity(), activeAgent(), core.isLocaleZh(props.locale()));
  const selectedName = () => pixelWorldSelectedEntityLabel(props.renderState(), selected(), core.isLocaleZh(props.locale()));
  const primaryLocation = () => props.renderState().locations[0] || null;
  return (
    <Show when={surface()}>
      <div
        class="pixel-world-focus-minimap"
        data-focus-minimap="true"
      >
        <div class="pixel-world-focus-minimap__label">
          {tr(props.locale(), "任务地图", "Mission Map")}
        </div>
        <Show when={primaryLocation()}>
          <span class="sr-only">
            {`${tr(props.locale(), "参照", "Reference")}: ${primaryLocation().label || primaryLocation().id}`}
          </span>
        </Show>
        <div class="pixel-world-focus-minimap__grid" />
        <div class="pixel-world-focus-minimap__route" data-routes={props.renderState().links.length} />
        <div class="pixel-world-focus-minimap__node pixel-world-focus-minimap__node--target">
          <span>{tr(props.locale(), "目标", "Target")}</span>
          <strong>{surface().next_action.label}</strong>
        </div>
        <div class="pixel-world-focus-minimap__node pixel-world-focus-minimap__node--agent">
          <span>{tr(props.locale(), "Agent", "Agent")}</span>
          <strong>{activeAgent() ? activeAgentName() : tr(props.locale(), "待分配", "Unassigned")}</strong>
        </div>
        <Show when={selected()}>
          <div
            class="pixel-world-focus-minimap__node pixel-world-focus-minimap__node--selected"
            data-selected="true"
          >
            <span>{tr(props.locale(), "选中", "Selected")}</span>
            <strong>{selectedName()}</strong>
          </div>
        </Show>
        <div class="pixel-world-focus-minimap__meta" aria-label={tr(props.locale(), "世界摘要", "World summary")}>
          <span>{`agents=${props.renderState().agents.length}`}</span>
          <span>{`targets=${props.renderState().locations.length}`}</span>
          <span>{`routes=${props.renderState().links.length}`}</span>
          <span>{`fragments=${props.renderState().fragment_terrain.length}`}</span>
        </div>
      </div>
    </Show>
  );
}
function chatEntryTitle(entry, locale) {
  if (entry.source === "error") {
    return `${entry.targetAgentId || entry.agentId || "agent"} ${tr(locale, "回复失败", "reply failed")}`;
  }
  if (entry.source === "player") {
    return `${tr(locale, "玩家", "Player")} -> ${entry.targetAgentId || entry.agentId || "agent"}`;
  }
  return `${entry.agentId || "agent"} ${tr(locale, "已发言", "spoke")}`;
}
function chatEntryCardClass(entry) {
  if (entry.source === "error") return "event-card event-card--chat-error";
  if (entry.source === "player") return "event-card event-card--chat-player";
  return "event-card event-card--chat-agent";
}
function chatEntryMeta(entry, locale) {
  if (entry.source === "error") {
    const code = entry.code ? ` · code=${entry.code}` : "";
    return `${entry.speaker || "runtime"}${code} · tick=${Number(entry.tick || 0)}`;
  }
  const speaker = entry.speaker || entry.playerId || tr(locale, "未知发言者", "unknown speaker");
  const location = entry.locationId || tr(locale, "未知地点", "unknown location");
  return `${speaker} · ${location} · tick=${Number(entry.tick || 0)}`;
}
function PixelRawDiagnostics(props) {
  const locale = () => props.locale();
  const [open, setOpen] = createSignal(false);
  const value = () => (typeof props.value === "function" ? props.value() : props.value);
  return (
    <details class="diagnostic" onToggle={(event) => setOpen(event.currentTarget.open)}>
      <summary>{tr(locale(), "原始诊断", "Raw diagnostics")}</summary>
      <Show when={open()}>
        <pre class="json">{JSON.stringify(value(), null, 2)}</pre>
      </Show>
    </details>
  );
}
function PixelWorldFocusCommandSurface(props) {
  const locale = () => props.locale();
  const agentId = () => {
    const id = String(core.selectedAgentId() || "").trim();
    return id && core.isAgentVisibleToCurrentSession(id) ? id : null;
  };
  const agentEntity = () => core.state.snapshot?.model?.agents?.[agentId()] || null;
  const agentName = () => pixelWorldAgentIdentity(agentEntity(), agentId(), core.isLocaleZh(locale()));
  const authSurface = () => core.buildAuthSurfaceModel();
  const chatCapability = () => authSurface().capabilities.agent_chat;
  const binding = () => core.selectedAgentBindingInfo();
  const chatFeedback = () => core.snapshotSemanticFeedback(core.state.lastChatFeedback);
  const chatFeedbackDisplay = () => core.describeSemanticFeedback(chatFeedback(), locale());
  const chatControlsEnabled = () => chatCapability().enabled && !core.isAgentChatInFlight();
  const gameplaySummary = () => core.buildGameplaySummary(locale());
  const blockerLabel = () => {
    const kind = gameplaySummary()?.blockerKind;
    if (!kind) return tr(locale(), "无阻塞", "No blocker");
    return pixelWorldBlockerPresentation(kind, locale()).label;
  };
  const receiptLabel = () =>
    gameplaySummary()?.executionStateLabel
      || gameplaySummary()?.recentFeedback?.stage
      || tr(locale(), "等待回执", "Waiting");
  const chatHistory = () =>
    core.state.chatHistory
      .filter((entry) => entry.agentId === agentId() || entry.targetAgentId === agentId())
      .slice(0, 12);
  return (
    <div id="viewer-command-console" class="pixel-world-focus-command-surface stack">
      <Show
        when={agentId()}
        fallback={<div class="empty">{tr(locale(), "先选中一个行动体，才能在电影视图里直接下指令。", "Select an agent to issue direct commands in Cinematic View.")}</div>}
      >
        <div class="badge-row">
          <span class="badge badge--accent">{tr(locale(), "当前交互目标", "Current Target")}</span>
          <span class="badge">{agentName()}</span>
          <Show when={binding()?.playerId}>
            <span class="badge">{`boundPlayer=${binding().playerId}`}</span>
          </Show>
          <span class={chatCapability().enabled ? "badge badge--good" : "badge badge--warn"}>
            {chatCapability().enabled ? tr(locale(), "聊天可用", "Chat Ready") : tr(locale(), "聊天受限", "Chat Limited")}
          </span>
        </div>
        <div class="pixel-world-focus-command-tray" data-chat-ready={chatControlsEnabled() ? "true" : "false"}>
          <div class="pixel-world-focus-command-chip pixel-world-focus-command-chip--target">
            <span>{tr(locale(), "目标", "Target")}</span>
            <strong>{agentName()}</strong>
            <em>{tr(locale(), "可交互行动体", "Interactive agent")}</em>
          </div>
          <div class="pixel-world-focus-command-chip pixel-world-focus-command-chip--blocker" data-blocker-present={blockerLabel() !== tr(locale(), "无阻塞", "No blocker") ? "true" : "false"}>
            <span>{tr(locale(), "阻塞", "Blocker")}</span>
            <strong>{blockerLabel()}</strong>
          </div>
          <div class="pixel-world-focus-command-chip pixel-world-focus-command-chip--receipt">
            <span>{tr(locale(), "回执", "Receipt")}</span>
            <strong>{receiptLabel()}</strong>
          </div>
          <button
            type="button"
            class="pixel-world-focus-command-chip pixel-world-focus-command-chip--primary"
            data-chat-send="1"
            disabled={!chatControlsEnabled()}
            onClick={() => core.sendAgentChat(agentId(), core.state.chatDraft.message)}
          >
            {tr(locale(), "发送聊天", "Send Chat")}
          </button>
        </div>
        <Show when={!chatCapability().enabled}>
          <div class="empty">{chatCapability().reason}</div>
        </Show>
        <div class="panel panel--nested">
          <div class="panel__header">
            <div class="stack stack--compact">
              <div class="panel__eyebrow">{tr(locale(), "指挥面板", "Command Surface")}</div>
              <div class="panel__title">{tr(locale(), "行动体聊天", "Agent Chat")}</div>
              <div class="panel__meta-copy">
                {tr(
                  locale(),
                  "给当前目标发消息并读取反馈。",
                  "Message the current target and read feedback.",
                )}
              </div>
            </div>
          </div>
          <div class="panel__body stack">
            <div class="field">
              <label for="agent-chat-message">{tr(locale(), "消息", "Message")}</label>
              <textarea
                id="agent-chat-message"
                rows="2"
                placeholder={tr(locale(), "给当前选中的行动体发一条消息", "Send a message to the selected agent")}
                disabled={!chatControlsEnabled()}
                value={core.state.chatDraft.message}
                onInput={(event) => {
                  core.state.chatDraft.message = String(event.currentTarget.value || "");
                  core.state.chatDraft.dirty = true;
                }}
              />
            </div>
            <div class="toolbar">
              <button
                type="button"
                data-chat-send="1"
                disabled={!chatControlsEnabled()}
                onClick={() => core.sendAgentChat(agentId(), core.state.chatDraft.message)}
              >
                {tr(locale(), "发送聊天", "Send Chat")}
              </button>
            </div>
            <Show when={chatFeedback()} fallback={<div class="empty">{tr(locale(), "还没有聊天反馈。", "No chat feedback yet.")}</div>}>
              {(feedback) => (
                <div class="feedback-card">
                  <div class="badge-row">
                    <span class={chatFeedbackDisplay().badgeClass}>{chatFeedbackDisplay().label}</span>
                    <Show when={chatFeedbackDisplay().code}>
                      <span class="badge">{`code=${chatFeedbackDisplay().code}`}</span>
                    </Show>
                  </div>
                  <div class="feedback-summary">{chatFeedbackDisplay().summary}</div>
                  <Show when={chatFeedbackDisplay().detail}>
                    <div class="feedback-detail">{chatFeedbackDisplay().detail}</div>
                  </Show>
                  <PixelRawDiagnostics locale={locale} value={feedback} />
                </div>
              )}
            </Show>
            <div>
              <div class="panel__title panel__title--spaced">{tr(locale(), "消息流", "Message Flow")}</div>
              <div class="event-list">
                <Show when={chatHistory().length > 0} fallback={<div class="empty">{tr(locale(), "这个行动体还没有聊天历史。", "No chat history for this agent yet.")}</div>}>
                  <For each={chatHistory()}>
                    {(entry) => (
                      <div class={chatEntryCardClass(entry)}>
                        <div class="event-card__title">
                          <span>{chatEntryTitle(entry, locale())}</span>
                        </div>
                        <div class="event-card__meta">{chatEntryMeta(entry, locale())}</div>
                        <div class="feedback-summary">{entry.message || tr(locale(), "没有消息正文。", "No message body.")}</div>
                        <PixelRawDiagnostics locale={locale} value={entry} />
                      </div>
                    )}
                  </For>
                </Show>
              </div>
            </div>
          </div>
        </div>
      </Show>
    </div>
  );
}
export function PixelWorldHost(props) {
  const locale = () => props.locale ?? core.state.uiLocale;
  const visualFixtureName = installPixelWorldVisualFixtureHook();
  const rendererRoute = resolvePixelWorldRendererRoute();
  const [coreRevision, setCoreRevision] = createSignal(0);
  const [worldRevision, setWorldRevision] = createSignal(0);
  const selectedEntity = () => {
    coreRevision();
    return spatial.selection();
  };
  const spatial = createPixelWorldSpatialController({core,enabled:()=>core.state.viewerProtocol?.capabilities?.includes("world_view"),
    getLocale:locale,getRenderState:()=>rustRenderState()});
  const worldViewState = spatial.state, navigation = spatial.navigation;
  const spatialSupported = () => {
    coreRevision();
    return core.state.viewerProtocol?.capabilities?.includes("world_view");
  };
  const renderInput = createMemo(() => {
    worldRevision();
    const spatialState = worldViewState();
    const currentLocale = locale();
    const currentSelection = spatial.selection();
    const currentNavigation = navigation();
    try { return untrack(() => projectPixelWorldRenderInput({...buildPixelWorldRenderInput(currentLocale),
      ...(spatialState.view ? {worldView:spatialState.view,worldViewStale:spatialState.stale} : {}),
      ...(currentSelection ? {selectedKind:currentSelection.kind,selectedId:currentSelection.id} : {}),
      navigation:currentNavigation}));
    } catch (error) {
      return {fatal:{code:"pixel_world_display_capacity",message:String(error?.message || error)}};
    }
  });
  const scopeGuard = createPixelWorldScopeGuard(core.state);
  const [retainedSurface, setRetainedSurface] = createSignal(null);
  const [rustRenderState, setRustRenderState] = createSignal(null);
  const [resourceStage, setResourceStage] = createSignal("descriptor");
  const renderState = () => rustRenderState();
  const visualState = () => pixelWorldVisualState(renderState());
  const [rendererStatus, setRendererStatus, rendererFatal, setRendererFatal, runtimeSource, setRuntimeSource] = createPixelWorldRendererRouteSignals(rendererRoute, createSignal);
  const [hoverSelection, setHoverSelection] = createSignal(null);
  const [cameraState, setCameraState] = createSignal(null);
  const [renderDtoOpen, setRenderDtoOpen] = createSignal(false);
  const [focusMode, setFocusMode] = createSignal(pixelWorldFocusUiSessionState.focusMode);
  const [commandDrawerOpen, setCommandDrawerOpen] = createSignal(pixelWorldFocusUiSessionState.commandDrawerOpen);
  const [diagnosticsDrawerOpen, setDiagnosticsDrawerOpen] = createSignal(pixelWorldFocusUiSessionState.diagnosticsDrawerOpen);
  const [maximized, setMaximized] = createSignal(pixelWorldFocusUiSessionState.maximized);
  installPixelWorldRenderDtoProbe(visualFixtureName, renderState, onCleanup);
  const visualOverlayEnabled = () => {
    coreRevision();
    return Boolean(visualFixtureName || document.body?.getAttribute("data-viewer-visual-fixture"));
  };
  const hoveredHotspot = () => {
    const hover = hoverSelection();
    if (hover?.kind !== "hotspot") {
      return null;
    }
    return visualState().visualHotspots.find((hotspot) => hotspot.id === hover.id) || null;
  };
  function setPersistentFocusMode(next) {
    pixelWorldFocusUiSessionState.focusMode = next;
    setFocusMode(next);
  }
  function setPersistentCommandDrawerOpen(next) {
    pixelWorldFocusUiSessionState.commandDrawerOpen = next;
    setCommandDrawerOpen(next);
  }
  function setPersistentDiagnosticsDrawerOpen(next) {
    pixelWorldFocusUiSessionState.diagnosticsDrawerOpen = next;
    setDiagnosticsDrawerOpen(next);
  }
  function setPersistentMaximized(next) {
    pixelWorldFocusUiSessionState.maximized = next;
    setMaximized(next);
  }
  function toggleMaximized() {
    setPersistentMaximized(!maximized());
  }
  const focusController = createPixelWorldFocusController({
    focusMode,
    commandDrawerOpen,
    diagnosticsDrawerOpen,
    setFocusMode: setPersistentFocusMode,
    setCommandDrawerOpen: setPersistentCommandDrawerOpen,
    setDiagnosticsDrawerOpen: setPersistentDiagnosticsDrawerOpen,
    setMaximized: setPersistentMaximized,
  });
  const adapter = createMemo(() => createPixelWorldHostAdapter({
    onResourceStage: setResourceStage,
    onSelectEntity(selection) {
      const applied = spatial.select(selection);
      if (applied?.kind === "module_visual") {
        focusViewerPanel("viewer-details-panel");
      }
    },
    onHoverEntity(selection) {
      setHoverSelection(selection);
    },
    onFatal(fatal, nextCameraState) {
      if (nextCameraState) {
        setCameraState(nextCameraState);
        spatial.cameraChanged(nextCameraState);
        core.updatePixelWorldRuntimeMeta({
          runtimeStatus: rendererStatus(),
          runtimeSource: runtimeSource(),
          runtimeModuleUrl: adapter().runtimeModuleUrl(),
          camera: nextCameraState,
          fatal: rendererFatal(),
        });
        return;
      }
      setRetainedSurface(previous => untrack(renderState)?.commercial_surface || previous);
      rendererAttemptGeneration += 1;
      adapter().unmount();
      setHoverSelection(null);
      setRendererFatal(fatal);
      setRendererStatus("unavailable");
      setRuntimeSource(fatal?.code === "pixel_world_webgl2_unavailable" ? "surface_unavailable" : runtimeSource());
      setRustRenderState(null);
      core.updatePixelWorldRuntimeMeta({
        runtimeStatus: "unavailable",
        runtimeSource: runtimeSource(),
        runtimeModuleUrl: adapter().runtimeModuleUrl(),
        camera: cameraState(),
        fatal,
      });
      core.reportFatalError(fatal.message, "pixel_world_host");
    },
  }));
  let mountedCanvas = null;
  let removeContextListener = () => {};
  let scopeReadPending = null;
  let hostActive = true;
  function attachContextListener(canvas) {
    removeContextListener();
    const lost = (event) => {
      event.preventDefault();
      // Stop the old session and its hit regions before exposing recovery.
      adapter().simulateFatal({ code: "pixel_world_context_lost", message: "World graphics interrupted. Retry to restore the authorized view." });
    };
    let start = null;
    const stopFollow = spatial.pauseFollow;
    const down = event => { start = {id:event.pointerId,x:event.clientX,y:event.clientY}; };
    const move = event => {
      if (start && start.id === event.pointerId && Math.hypot(event.clientX-start.x,event.clientY-start.y)>4) {
        start=null; stopFollow();
      }
    };
    const up = () => { start=null; };
    const events = [["webglcontextlost",lost],["pointerdown",down],["pointermove",move],["pointerup",up],["pointercancel",up],["wheel",stopFollow]];
    for (const [type,listener] of events) canvas.addEventListener(type,listener);
    removeContextListener = () => { for (const [type,listener] of events) canvas.removeEventListener(type,listener); };
  }
  let rendererAttemptGeneration = 0;
  let rendererUpdatePending = false;
  const rendererAttemptIsCurrent = (canvas, generation) => generation === rendererAttemptGeneration && mountedCanvas === canvas;
  let rendererUpdateFrame = null;
  function scheduleRendererUpdate() {
    if (rendererUpdateFrame !== null) return;
    const generation = rendererAttemptGeneration;
    rendererUpdateFrame = requestAnimationFrame(() => {
      rendererUpdateFrame = null;
      if (!hostActive || generation !== rendererAttemptGeneration || scopeGuard.check().blocked) return;
      setWorldRevision(revision => revision + 1);
      if (rendererStatus() === "ready") applyRendererUpdate();
      else rendererUpdatePending = true;
    });
  }
  function applyRendererUpdate() {
    if (rendererStatus() === "unavailable") {
      return;
    }
    const generation = rendererAttemptGeneration;
    const result = adapter().update(renderInput());
    if (generation !== rendererAttemptGeneration) return;
    if (result?.fatal) {
      setRendererFatal(result.fatal);
    }
    if (result?.renderState) setRetainedSurface(null);
    setRustRenderState(result?.renderState || null);
    setRendererStatus(result?.status || "ready");
    setRuntimeSource(result?.runtimeSource || adapter().runtimeSource());
    core.updatePixelWorldRuntimeMeta({
      runtimeStatus: result?.status || "ready",
      runtimeSource: result?.runtimeSource || adapter().runtimeSource(),
      runtimeModuleUrl: result?.runtimeModuleUrl || adapter().runtimeModuleUrl(),
      camera: cameraState(),
      fatal: result?.fatal || rendererFatal(),
    });
  }
  async function setReadyMode(canvas = mountedCanvas, generation = rendererAttemptGeneration) {
    if (scopeGuard.check().blocked) return;
    if (!rendererAttemptIsCurrent(canvas, generation)) {
      return;
    }
    if (!canvas) {
      const fatal = {
        code: "pixel_world_renderer_mount_missing_canvas",
        message: "pixel world canvas is not mounted yet",
      };
      setRendererFatal(fatal);
      setRendererStatus("unavailable");
      setRuntimeSource("detached");
      setRustRenderState(null);
      core.updatePixelWorldRuntimeMeta({
        runtimeStatus: "unavailable",
        runtimeSource: "detached",
        runtimeModuleUrl: null,
        camera: null,
        fatal,
      });
      return;
    }
    setRendererFatal(null);
    setRendererStatus("booting");
    setRuntimeSource("loading");
    const attached = await waitForRuntimeCanvasAttachment(canvas);
    if (!rendererAttemptIsCurrent(canvas, generation)) {
      return;
    }
    if (!attached) {
      const fatal = {
        code: "pixel_world_renderer_canvas_detached",
        message: "pixel world runtime canvas never became queryable in document",
      };
      setRendererFatal(fatal);
      setRendererStatus("unavailable");
      setRuntimeSource("detached");
      setRustRenderState(null);
      core.updatePixelWorldRuntimeMeta({
        runtimeStatus: "unavailable",
        runtimeSource: "detached",
        runtimeModuleUrl: null,
        camera: cameraState(),
        fatal,
      });
      return;
    }
    const surfaceFatal = probePixelWorldWebgl2Surface(canvas);
    if (surfaceFatal) {
      adapter().simulateFatal(surfaceFatal);
      return;
    }
    const result = await adapter().mount(canvas, renderInput());
    if (!rendererAttemptIsCurrent(canvas, generation)) {
      return;
    }
    if (result?.fatal) {
      setRendererFatal(result.fatal);
    }
    if (result?.renderState) setRetainedSurface(null);
    setRustRenderState(result?.renderState || null);
    setRendererStatus(result?.status || "ready");
    setRuntimeSource(result?.runtimeSource || adapter().runtimeSource());
    core.updatePixelWorldRuntimeMeta({
      runtimeStatus: result?.status || "ready",
      runtimeSource: result?.runtimeSource || adapter().runtimeSource(),
      runtimeModuleUrl: result?.runtimeModuleUrl || adapter().runtimeModuleUrl(),
      camera: cameraState(),
      fatal: result?.fatal || null,
    });
    if (rendererUpdatePending) {
      rendererUpdatePending = false;
      applyRendererUpdate();
    }
  }
  function requestReadyMode() {
    if (scopeGuard.check().blocked) { scopeReadPending = null; core.requestRender(); return; }
    const canvas = mountedCanvas?.isConnected ? mountedCanvas : null;
    mountedCanvas = canvas;
    if (rendererUpdateFrame !== null) cancelAnimationFrame(rendererUpdateFrame);
    rendererUpdateFrame = null;
    const generation = ++rendererAttemptGeneration;
    setRendererFatal(null);
    setRendererStatus("booting");
    setRuntimeSource("loading");
    if (canvas) {
      void setReadyMode(canvas, generation);
    }
  }
  function simulateFatal() {
    adapter().simulateFatal("simulated embedded renderer fatal");
  }
  onMount(() => {
    applyPixelWorldRendererRoute(rendererRoute, core.updatePixelWorldRuntimeMeta);
    function handleKeyDown(event) {
      focusController.handleKeyDown(event);
    }
    window.addEventListener("keydown", handleKeyDown);
    onCleanup(() => window.removeEventListener("keydown", handleKeyDown));
    const unsubscribeRenderHook = core.subscribeRenderHook?.((invalidation) => {
      const scope = scopeGuard.check();
      if (scope.scope !== spatial.scope()) {
        if (scope.changed) { setRendererStatus("unavailable"); setRustRenderState(null); }
        spatial.resetScope();
      }
      if (scope.changed) {
        // Selection is display state. Feedback projection is scope-masked without
        // deleting pending operation identity or replaying the operation.
        core.state.selectedKind = null;
        core.state.selectedId = null;
        core.state.selectedObject = null;
        adapter().simulateFatal({code:"pixel_world_scope_changed",message:"Display authorization changed. Waiting for a new authorized snapshot."});
        setRetainedSurface(null);
        setCameraState(null);
      }
      setCoreRevision((revision) => revision + 1);
      if (scope.blocked) {
        if (core.state.viewerProtocol?.capabilities?.includes("world_view") && scopeReadPending !== scope.scope) {
          scopeReadPending = scope.scope;
          const expected = scope.scope;
          void core.requestScopeSnapshot().then(snapshot => {
            if (!hostActive || !scopeGuard.authorize(expected, scope.generation)) return;
            scopeReadPending = null;
            core.acceptScopeSnapshot(snapshot);
            requestReadyMode();
          }).catch(() => { /* Keep the view locked; retry is explicit. */ });
        }
        return;
      }
      spatial.refresh(core.state.snapshot);
      // An absent hint remains conservatively dirty, including mutable state.
      if (invalidation?.pixelWorldChanged === false) return;
      scheduleRendererUpdate();
    });
    onCleanup(unsubscribeRenderHook);
    if (pixelWorldTestApiEnabled()) {
      onCleanup(installPixelWorldHotspotPointerProbe({ fixtureName: visualFixtureName, getCanvas: () => mountedCanvas, getRendererStatus: rendererStatus, getHotspotHitTargets: () => adapter().hotspotTestHitTargets(), getLocationHitTargets: () => adapter().locationTestHitTargets(), getHoverSelection: hoverSelection, getHoveredHotspot: hoveredHotspot }));
    }
  });
  createEffect(() => {
    document.body.classList.toggle("pixel-world-focus-active", focusMode());
    document.body.classList.toggle("pixel-world-focus-maximized", focusMode() && maximized());
  });
  onCleanup(() => {
    document.body.classList.remove("pixel-world-focus-active");
    document.body.classList.remove("pixel-world-focus-maximized");
    hostActive = false;
    if (rendererUpdateFrame !== null) cancelAnimationFrame(rendererUpdateFrame);
    rendererUpdateFrame = null;
    rendererAttemptGeneration += 1;
    removeContextListener();
    adapter().unmount();
    core.updatePixelWorldRuntimeMeta({
      runtimeStatus: "detached",
      runtimeSource: "detached",
      runtimeModuleUrl: null,
      camera: null,
      fatal: null,
    });
  });
  createEffect(() => {
    worldViewState(); navigation(); spatial.selection();
    untrack(() => { if (mountedCanvas && rendererStatus() === "ready") scheduleRendererUpdate(); });
  });
  return (
    <div
      class={`pixel-world-host stack ${focusMode() ? "pixel-world-host--focus" : ""} ${focusMode() && maximized() ? "pixel-world-host--focus-maximized" : ""}`}
      data-viewer-overlay="world-hud"
      data-world-focus={focusMode() ? "true" : "false"}
      data-world-focus-maximized={focusMode() && maximized() ? "true" : "false"}
      data-visual-fixture={visualFixtureName || ""}
      data-focus-comparable={shouldShowFocusCinematic(renderState()) ? "false" : "true"}
    >
      <Show when={rendererStatus() !== "ready" && rendererStatus() !== "unavailable"}>
        <div role="status" aria-live="polite" data-world-resource-stage={resourceStage()}>
          {{descriptor:tr(locale(),"正在查找世界画面资源…","Finding world graphics…"),
            download:tr(locale(),"正在下载世界画面资源…","Downloading world graphics…"),
            verify:tr(locale(),"正在核对画面资源完整性…","Verifying world graphics…"),
            prepare:tr(locale(),"正在准备世界画面…","Preparing world graphics…"),
            ready:tr(locale(),"正在连接世界画面…","Attaching world graphics…")}[resourceStage()] || tr(locale(),"正在准备世界画面…","Preparing world graphics…")}
        </div>
      </Show>
      <Show when={!focusMode() || !maximized()}>
        <div class="pixel-world-host__summary">
          <div class="pixel-world-host__summary-copy">
            <div class="pixel-world-host__headline">
              {tr(locale(), "世界指挥棋盘", "World Command Board")}
            </div>
            <div class="feedback-detail">
              {renderState()?.commercial_surface?.objective?.detail
                || tr(locale(), "等待 Rust bridge 生成世界显示状态。", "Waiting for the Rust bridge to derive the world display state.")}
            </div>
          </div>
        </div>
      </Show>
      <Show when={spatialSupported()}><details class="pixel-world-exploration-disclosure">
        <summary>{tr(locale(), "探索世界", "Explore world")}</summary>
        <PixelWorldSpatialExplorer controller={spatial} locale={locale()} /></details></Show>
      <div
        class="pixel-world-focus-entry"
        data-viewer-overlay="cinematic-entry"
        hidden={focusMode()}
      >
          <div id="pixel-world-focus-entry-hint" class="pixel-world-focus-entry__hint">
            {tr(locale(), "拖动、缩放并检查世界", "Pan, zoom, and inspect the world")}
          </div>
          <button
            type="button"
            class="pixel-world-focus-entry__button"
            disabled={!renderState()}
            onClick={focusController.enterFocusMode}
            aria-pressed="false"
            aria-describedby={rendererStatus() === "unavailable" ? PIXEL_WORLD_RENDERER_UNAVAILABLE_MESSAGE_ID : "pixel-world-focus-entry-hint"}
          >
            {rendererStatus() === "unavailable"
              ? tr(locale(), "电影视图（当前不可用）", "Cinematic View (unavailable)")
              : tr(locale(), "电影视图", "Cinematic View")}
          </button>
      </div>
      <Show when={focusMode() && renderState()}>
        <Show when={!maximized() && shouldShowFocusCinematic(renderState())}>
          <PixelWorldFocusCinematicBanner
            locale={locale}
            renderState={renderState}
          />
        </Show>
        <PixelWorldFocusHud
          locale={locale}
          renderState={renderState}
          onExit={focusController.exitFocusMode}
          onOpenCommand={focusController.openCommandDrawer}
          onOpenDiagnostics={focusController.openDiagnosticsDrawer}
          onToggleMaximized={toggleMaximized}
          maximized={maximized}
        />
        <Show when={!maximized()}>
          <PixelWorldFocusRail
            locale={locale}
            renderState={renderState}
          />
          <PixelWorldFocusMinimapCard
            locale={locale}
            renderState={renderState}
            variant="immersive"
          />
        </Show>
      </Show>
      <Show when={renderState()}>
        <PixelWorldCommercialHud locale={locale} renderState={renderState} focusMode={focusMode}/>
      </Show>
      <Show when={rendererStatus() !== "fallback" && rendererStatus() !== "unavailable"}>
        <PixelWorldCanvasRenderer
          locale={locale}
          rendererStatus={rendererStatus}
          rendererProjection={() => runtimeSource() === 'wasm_bindgen_runtime' && cameraState() !== null}
          cameraState={cameraState}
          renderInput={renderInput}
          renderState={renderState}
          selection={selectedEntity}
          hoveredEntity={hoverSelection}
          visualOverlayEnabled={visualOverlayEnabled}
          onSelect={(selection) => adapter().simulateSelect(selection)}
          onHover={(selection) => adapter().simulateHover(selection)}
          hoveredHotspot={hoveredHotspot}
          onFatal={(message) => adapter().simulateFatal(message)}
          onCanvasMount={(canvas) => {
            if (mountedCanvas === canvas) return;
            mountedCanvas = canvas;
            attachContextListener(canvas);
            if (rendererUpdateFrame !== null) cancelAnimationFrame(rendererUpdateFrame);
    rendererUpdateFrame = null;
    const generation = ++rendererAttemptGeneration;
            if (untrack(rendererStatus) !== "ready") {
              void setReadyMode(canvas, generation);
            }
          }}
        />
      </Show>
      <Show when={!renderState()}>
        <div
          id={rendererStatus() === "unavailable" ? PIXEL_WORLD_RENDERER_UNAVAILABLE_MESSAGE_ID : undefined}
          class="empty pixel-world-render-unavailable"
          data-viewer-overlay="renderer-unavailable"
          data-renderer-state={rendererStatus()}
        >
          <Show when={retainedSurface()}>{(surface) => <>
            <p>{surface().objective?.title}</p><p>{surface().objective?.detail}</p>
            <PixelWorldActionReceipt locale={locale} surface={surface} />
          </>}</Show>
          {rendererStatus() === "unavailable" ? <><div>{rendererFatal()?.code === "pixel_world_scope_changed" ? tr(locale(), "正在重新确认可见范围；旧内容已清除。", "Reconfirming visible scope; previous content cleared.") : tr(locale(), "此浏览器中的图形不可用", "Graphics unavailable in this browser")}</div><button type="button" class="pixel-world-render-unavailable__retry" onClick={requestReadyMode}>{tr(locale(), "重试 Renderer", "Retry Renderer")}</button></> : tr(locale(), "Rust bridge 正在生成世界显示状态。", "Rust bridge is deriving the world display state.")}
        </div>
      </Show>
      <Show when={focusMode() && renderState()?.commercial_surface}>
        <div class="pixel-world-focus-receipt">
          <PixelWorldActionReceipt
            class="pixel-world-action-receipt--focus-compact"
            locale={locale}
            surface={() => renderState().commercial_surface}
          />
        </div>
      </Show>
      <Show when={focusMode()}>
        <details
          id="viewer-focus-command-drawer"
          class="pixel-world-focus-drawer pixel-world-focus-drawer--command"
          open={commandDrawerOpen()}
          onToggle={(event) => {
            const open = event.currentTarget.open;
            if (open) {
              setPersistentCommandDrawerOpen(true);
            } else {
              focusController.closeCommandDrawer({ returnFocus: true });
            }
          }}
        >
          <summary>{tr(locale(), "命令与目标", "Command and Target")}</summary>
          <div class="pixel-world-focus-drawer__body">
            <PixelWorldFocusCommandSurface locale={locale} />
          </div>
        </details>
      </Show>
      <Show when={!focusMode() || !maximized()}>
        <details class="diagnostic pixel-world-render-diagnostics" data-renderer-state={rendererStatus()}>
          <summary>{tr(locale(), "Renderer 诊断", "Renderer Diagnostics")}</summary>
          <div class="pixel-world-host__toolbar badge-row">
            <span class="badge badge--accent">{`locations=${visualState().locations.length}`}</span>
            <span class="badge badge--accent">{`fragments=${visualState().fragmentTerrain.length}`}</span>
            <span class="badge badge--accent">{`agents=${visualState().agents.length}`}</span>
            <Show when={renderState()?.world_tick !== null && renderState()?.world_tick !== undefined}>
              <span class="badge badge--accent" data-world-tick={String(renderState()?.world_tick)}>{`tick=${renderState()?.world_tick}`}</span>
            </Show>
            <span class="badge">{`links=${visualState().links.length}`}</span>
            <span class="badge">{`hotspots=${arrayField(renderState(), "visual_hotspots", "visualHotspots").length}`}</span>
            <span class="badge">{`derived_positions=${visualState().agents.filter((agent) => agent.position_source === "location_derived").length}`}</span>
            <span class="badge">{visualState().worldBounds ? "world_bounds=ready" : "world_bounds=missing"}</span>
            <span class="badge">{`renderer=${rendererStatus()}`}</span>
            <span class="badge">{`runtime=${runtimeSource()}`}</span>
            <Show when={cameraState()}>
              <span class="badge">{`zoom=${cameraState().zoom.toFixed(2)}`}</span>
            </Show>
            <Show when={cameraState()}>
              <span class="badge">{`pan=${cameraState().pan_x_px},${cameraState().pan_y_px}`}</span>
            </Show>
            <Show when={hoverSelection()}>
              <span class="badge">{`hover=${hoverSelection().kind}/${hoverSelection().id}`}</span>
            </Show>
            <button type="button" onClick={requestReadyMode}>
              {tr(locale(), "重新挂载嵌入式 Renderer", "Reattach Embedded Renderer")}
            </button>
            <button type="button" onClick={simulateFatal}>
              {tr(locale(), "模拟 Renderer Fatal", "Simulate Renderer Fatal")}
            </button>
            <div class="feedback-detail">
              {tr(
                locale(),
                "当前世界舞台只依赖 wasm/Rust bridge、嵌入式 canvas、轻量拖拽缩放和事件回传。",
                "The world stage depends only on the wasm/Rust bridge, embedded canvas, light pan-zoom interaction, and event callbacks.",
              )}
            </div>
            <Show when={rendererStatus() === "unavailable" && rendererFatal()}>
              <div class="feedback-detail" data-renderer-fatal>{`${rendererFatal().code}: ${rendererFatal().message}`}</div>
            </Show>
          </div>
        </details>
      </Show>
      <Show when={focusMode() && renderState()}>
        <details
          id="viewer-focus-diagnostics-drawer"
          class="pixel-world-focus-drawer pixel-world-focus-drawer--diagnostics"
          open={diagnosticsDrawerOpen()}
          onToggle={(event) => {
            const open = event.currentTarget.open;
            if (open) {
              setPersistentDiagnosticsDrawerOpen(true);
            } else {
              focusController.closeDiagnosticsDrawer({ returnFocus: true });
            }
          }}
        >
          <summary>{tr(locale(), "电影视图诊断", "Cinematic Diagnostics")}</summary>
          <div class="pixel-world-focus-drawer__body">
            <div class="badge-row">
              <Show when={renderState().world_tick !== null && renderState().world_tick !== undefined}>
                <span class="badge badge--accent" data-world-tick={String(renderState().world_tick)}>{`tick=${renderState().world_tick}`}</span>
              </Show>
              <span class="badge">{`renderer=${rendererStatus()}`}</span>
              <span class="badge">{`runtime=${runtimeSource()}`}</span>
              <span class="badge">{`derived_positions=${renderState().agents.filter((agent) => agent.position_source === "location_derived").length}`}</span>
              <Show when={rendererFatal()}>
                <span class="badge badge--warn">{rendererFatal().code}</span>
              </Show>
            </div>
            <div class="toolbar toolbar--spaced">
              <button type="button" onClick={requestReadyMode}>
                {tr(locale(), "重新挂载嵌入式 Renderer", "Reattach Embedded Renderer")}
              </button>
            </div>
          </div>
        </details>
      </Show>
      <details
        class="diagnostic"
        onToggle={(event) => setRenderDtoOpen(event.currentTarget.open)}
      >
        <summary>{tr(locale(), "展开 Render DTO", "Expand Render DTO")}</summary>
        <Show when={renderDtoOpen()}>
          <div class="stack flow-top">
            <pre class="json">{JSON.stringify(renderState(), null, 2)}</pre>
          </div>
        </Show>
      </details>
    </div>
  );
}

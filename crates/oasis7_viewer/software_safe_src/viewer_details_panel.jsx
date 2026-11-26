import { Show } from "solid-js";
import * as core from "./legacy_core.js";
import { BrowserCanonicalEntry } from "./browser_canonical_entry.jsx";
import { Badge, EmptyState } from "./viewer_ui_primitives.jsx";
import { viewerRuntimeConfig } from "./viewer_runtime_config_module.js";
import { authSigningIdentity } from "./viewer_auth_session_module.js";
import { pixelWorldReadableModuleLabel, pixelWorldSelectedEntityLabel } from "./pixel_world_identity.js";

function uiLocale() {
  return core.state.uiLocale;
}

function tr(locale, zh, en) {
  return core.isLocaleZh(locale) ? zh : en;
}

export function DetailsPanel(props) {
  const { observeViewerStateRevision, InteractionPanel, EmptyEntityRecoveryCard, DiagnosticDetails } = props;
  observeViewerStateRevision();
  const locale = () => uiLocale();
  const gameplaySummary = () => core.buildGameplaySummary(locale());
  const worldScaleSurface = () => core.buildWorldScaleSurface(locale());
  const worldMetaSummary = () => {
    const physicalTruth = worldScaleSurface().physicalTruth;
    const segments = [];
    if (physicalTruth.worldBoundsLabel) {
      segments.push(
        tr(locale(), "世界边界", "World Bounds") + ` ${physicalTruth.worldBoundsLabel}`,
      );
    }
    const nearestLocation = physicalTruth.nearestLocations[0];
    if (nearestLocation?.distanceLabel) {
      segments.push(
        tr(locale(), "最近距离", "Nearest") + ` ${nearestLocation.distanceLabel}`,
      );
    }
    return segments.length > 0 ? segments.join(" · ") : tr(locale(), "当前未发布世界尺度摘要。", "No world scale summary is published yet.");
  };
  const selectedLabel = () => {
    if (!core.state.selectedKind || !core.state.selectedId || hiddenSelectedAgent()) {
      return tr(locale(), "未选择", "nothing selected");
    }
    if (core.state.selectedKind === "module_visual") {
      return pixelWorldSelectedEntityLabel(
        core.entityCollections(),
        { kind: core.state.selectedKind, id: core.state.selectedId },
        core.isLocaleZh(locale()),
      );
    }
    return `${core.state.selectedKind}:${core.state.selectedId}`;
  };
  const hiddenSelectedAgent = () =>
    core.state.selectedKind === "agent"
    && core.state.selectedId
    && !core.isAgentVisibleToCurrentSession(core.state.selectedId);
  const hasVisibleSelectedObject = () => core.state.selectedObject && !hiddenSelectedAgent();
  const selectedModule = () => core.state.selectedKind === "module_visual" ? core.state.selectedObject : null;
  const selectedModuleAnchor = () => {
    const anchor = selectedModule()?.anchor;
    if (!anchor || typeof anchor !== "object") return tr(locale(), "锚点不可用", "Anchor unavailable");
    const type = String(anchor.type || "anchor").trim();
    const data = anchor.data || {};
    if (type === "absolute") {
      const position = data.pos || data;
      return `${type} · ${core.formatWorldPositionCm(position)}`;
    }
    const id = data.agent_id || data.location_id || data.id;
    return id ? `${type} · ${id}` : type;
  };
  const snapshotSummary = () => ({
    config: core.state.snapshot?.config || null,
    counts: {
      agents: Object.keys(core.state.snapshot?.model?.agents || {}).length,
      locations: Object.keys(core.state.snapshot?.model?.locations || {}).length,
      promptProfiles: Object.keys(core.state.snapshot?.model?.agent_prompt_profiles || {}).length,
      executionDebugContexts: Object.keys(core.state.snapshot?.model?.agent_execution_debug_contexts || {}).length,
    },
    metrics: core.state.metrics,
    hostedAccess: core.clone(core.state.hostedAccess),
  });
  const snapshotCounts = () => ({
    agents: Object.keys(core.state.snapshot?.model?.agents || {}).length,
    locations: Object.keys(core.state.snapshot?.model?.locations || {}).length,
    promptProfiles: Object.keys(core.state.snapshot?.model?.agent_prompt_profiles || {}).length,
    executionDebugContexts: Object.keys(core.state.snapshot?.model?.agent_execution_debug_contexts || {}).length,
  });
  const hasSnapshotDiagnostics = () =>
    !!core.state.snapshot || !!core.state.metrics || !!core.state.hostedAccess;
  return (
    <div class="stack">
      <div class="badge-row">
        <Badge class="badge badge--accent">{tr(locale(), "当前命令目标", "Current Command Target")}</Badge>
        <Badge>{selectedLabel()}</Badge>
      </div>
      <Show when={viewerRuntimeConfig().browserGameClient}>
        <BrowserCanonicalEntry locale={locale} config={() => viewerRuntimeConfig().browserGameClient} legacyOwnerSigner={() => { observeViewerStateRevision(); return core.state.auth.available ? authSigningIdentity(core.state.auth) : null; }} legacyAccountId={() => { observeViewerStateRevision(); return core.state.auth.available ? core.state.auth.hostedAccountId : null; }} />
      </Show>
      <Show
        when={!hiddenSelectedAgent() && core.state.selectedKind !== "module_visual"}
        fallback={
          <Show when={core.state.selectedKind !== "module_visual"}>
            <EmptyState>
              {tr(
                locale(),
                "当前账号还没有可控 Agent。请先完成认领或等待自己的 Agent 绑定同步。",
                "The current account has no controllable Agent yet. Claim one or wait for your own Agent binding to sync.",
              )}
            </EmptyState>
          </Show>
        }
      >
        <InteractionPanel />
      </Show>
      <Show
        when={hasVisibleSelectedObject()}
        fallback={
          gameplaySummary()?.blockerKind === "runtime_snapshot_empty_entities"
            ? (
              <EmptyEntityRecoveryCard
                locale={locale()}
                gameplay={gameplaySummary}
                title={tr(locale(), "对象明细暂时不可用", "Object Details Are Temporarily Unavailable")}
              />
            )
            : <EmptyState>{tr(locale(), "请先从 Targets 选择一个行动体或地点。", "Select an agent or location from Targets first.")}</EmptyState>
        }
      >
        {(selected) => (
          <DiagnosticDetails
            locale={locale()}
            label={tr(locale(), "展开对象原始明细", "Expand Raw Object Details")}
            note={tr(
              locale(),
              "默认只保留交互面；只有在核查快照字段或诊断对象结构时再展开原始 JSON。",
              "The interaction surface stays in front by default. Expand raw JSON only when you need to inspect snapshot fields or diagnose object shape.",
            )}
            value={() => core.clone(selected())}
          />
        )}
      </Show>
      <Show when={selectedModule()}>
        {(module) => (
          <div class="viewer-module-details" data-viewer-module-details="true">
            <div class="panel__title panel__title--spaced">{tr(locale(), "模块明细", "Module Details")}</div>
            <div class="badge-row">
              <Badge class="badge badge--accent">{pixelWorldReadableModuleLabel(module(), module().id, core.isLocaleZh(locale()))}</Badge>
              <Badge>{`module=${module().module_id || "-"}`}</Badge>
            </div>
            <div class="feedback-detail"><strong>{tr(locale(), "类型", "Kind")}</strong>: {module().kind || "artifact"}</div>
            <div class="feedback-detail"><strong>{tr(locale(), "标签", "Label")}</strong>: {pixelWorldReadableModuleLabel(module(), module().id, core.isLocaleZh(locale()))}</div>
            <div class="feedback-detail"><strong>{tr(locale(), "锚点", "Anchor")}</strong>: {selectedModuleAnchor()}</div>
          </div>
        )}
      </Show>
      <div>
        <div class="panel__title panel__title--spaced">{tr(locale(), "世界规模", "World Scale")}</div>
        <div class="badge-row">
          <Badge>{`agents=${snapshotCounts().agents}`}</Badge>
          <Badge>{`locations=${snapshotCounts().locations}`}</Badge>
          <Badge>{`promptProfiles=${snapshotCounts().promptProfiles}`}</Badge>
          <Badge>{`debugContexts=${snapshotCounts().executionDebugContexts}`}</Badge>
          <Badge>{tr(locale(), "snapshot.config.space", "snapshot.config.space")}</Badge>
        </div>
        <div class="feedback-detail flow-top">{worldMetaSummary()}</div>
        <Show when={hasSnapshotDiagnostics()}>
          <DiagnosticDetails
            locale={locale()}
            label={tr(locale(), "展开原始快照诊断", "Expand Raw Snapshot Diagnostics")}
            note={tr(
              locale(),
              "只在需要排查快照结构或托管接入原始字段时展开。",
              "Expand only when you need to inspect the raw snapshot shape or hosted access fields.",
            )}
            value={snapshotSummary}
          />
        </Show>
      </div>
      <Show when={core.state.lastError}>
        <div>
          <div class="panel__title panel__title--spaced panel__title--danger">{tr(locale(), "最近错误", "Last Error")}</div>
          <pre class="json">{core.state.lastError}</pre>
        </div>
      </Show>
    </div>
  );
}

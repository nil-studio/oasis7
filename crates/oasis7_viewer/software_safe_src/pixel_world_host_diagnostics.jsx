import { Show } from "solid-js";
import * as core from "./legacy_core.js";
import { arrayField } from "./pixel_world_visual_clarity.jsx";

function tr(locale, zh, en) { return core.isLocaleZh(locale) ? zh : en; }

export function PixelWorldHostDiagnostics(props) {
  const locale = () => props.locale();
  const renderState = () => props.renderState();
  const visualState = () => props.visualState();
  const rendererStatus = () => props.rendererStatus();
  const runtimeSource = () => props.runtimeSource();
  const cameraState = () => props.cameraState();
  const hoverSelection = () => props.hoverSelection();
  const rendererFatal = () => props.rendererFatal();
  const rendererNeedsReload = () => props.rendererNeedsReload();
  const focusMode = () => props.focusMode();
  const maximized = () => props.maximized();
  return (
    <>
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
            <Show when={cameraState()}><span class="badge">{`zoom=${cameraState().zoom.toFixed(2)}`}</span></Show>
            <Show when={cameraState()}><span class="badge">{`pan=${cameraState().pan_x_px},${cameraState().pan_y_px}`}</span></Show>
            <Show when={hoverSelection()}><span class="badge">{`hover=${hoverSelection().kind}/${hoverSelection().id}`}</span></Show>
            <button type="button" onClick={props.requestReadyMode}>
              {rendererNeedsReload() ? props.rendererReloadLabel() : tr(locale(), "重新挂载嵌入式 Renderer", "Reattach Embedded Renderer")}
            </button>
            <button type="button" onClick={props.simulateFatal}>{tr(locale(), "模拟 Renderer Fatal", "Simulate Renderer Fatal")}</button>
            <div class="feedback-detail">
              {tr(locale(), "当前世界舞台只依赖 wasm/Rust bridge、嵌入式 canvas、轻量拖拽缩放和事件回传。", "The world stage depends only on the wasm/Rust bridge, embedded canvas, light pan-zoom interaction, and event callbacks.")}
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
          open={props.diagnosticsDrawerOpen()}
          onToggle={(event) => {
            if (event.currentTarget.open) props.setPersistentDiagnosticsDrawerOpen(true);
            else props.focusController.closeDiagnosticsDrawer({ returnFocus: true });
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
              <Show when={rendererFatal()}><span class="badge badge--warn">{rendererFatal().code}</span></Show>
            </div>
            <div class="toolbar toolbar--spaced">
              <button type="button" onClick={props.requestReadyMode}>
                {rendererNeedsReload() ? props.rendererReloadLabel() : tr(locale(), "重新挂载嵌入式 Renderer", "Reattach Embedded Renderer")}
              </button>
            </div>
          </div>
        </details>
      </Show>
      <details class="diagnostic" onToggle={(event) => props.setRenderDtoOpen(event.currentTarget.open)}>
        <summary>{tr(locale(), "展开 Render DTO", "Expand Render DTO")}</summary>
        <Show when={props.renderDtoOpen()}>
          <div class="stack flow-top"><pre class="json">{JSON.stringify(renderState(), null, 2)}</pre></div>
        </Show>
      </details>
    </>
  );
}

import { Show } from "solid-js";

export function LiveControlStatus(props) {
  const locale = () => props.locale();
  const observation = () => props.state.liveControlState;
  return <Show when={props.state.controlProfile === "live"}>
    <aside role="status" aria-label={props.tr(locale(), "Viewer 决策调度", "Viewer decision scheduling")}
      data-viewer-overlay="control-state" class="panel"
      style={{ position: "fixed", top: "4.5rem", right: "1rem", "z-index": "25", "max-width": "min(25rem, calc(100vw - 2rem))" }}>
      <div class="panel__body">
        <strong>{!observation()?.verified
          ? props.tr(locale(), "Viewer 调度状态未确认", "Viewer scheduling state unconfirmed")
          : observation().paused
            ? props.tr(locale(), "Viewer 新决策已暂停", "Viewer new decisions paused")
            : props.tr(locale(), "Viewer 新决策已启用", "Viewer new decisions enabled")}</strong>
        <Show when={props.state.liveControlPendingRequestId != null}>
          <div>{props.tr(locale(), "等待 Viewer 确认", "Awaiting Viewer confirmation")}</div>
        </Show>
        <div class="feedback-detail">{props.tr(locale(), "世界作业与已在途的请求仍可能继续。", "World jobs and requests already in flight may continue")}</div>
      </div>
    </aside>
  </Show>;
}

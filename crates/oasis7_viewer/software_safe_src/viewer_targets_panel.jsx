import { Show } from "solid-js";
import * as core from "./legacy_core.js";
import { Badge, EmptyState } from "./viewer_ui_primitives.jsx";
import { CalloutCard, EntityListPendingState } from "./viewer_ui_cards.jsx";
import { TargetListWindow, createTargetListProjection } from "./target_list_window.jsx";
import { AgentActivitySurface } from "./agent_activity_surface.jsx";
import {
  gameplayActionButtonBusyAttrs,
  gameplayActionButtonClass,
  gameplayActionButtonDisabled,
  gameplayActionDisabledReason,
  gameplayActionDisplayLabel,
} from "./viewer_gameplay_action_helpers.js";

function uiLocale() { return core.state.uiLocale; }
function tr(locale, zh, en) {
  return core.isLocaleZh(locale) ? zh : en;
}

export function TargetsPanel(props) {
  const { lists, filterKey, scopeKey } = createTargetListProjection(core, props.observeViewerStateRevision);
  const locale = () => uiLocale();
  const gameplaySummary = () => core.buildGameplaySummary(locale());
  const firstAgentClaimAction = () =>
    (gameplaySummary()?.availableActions || []).find((action) => action.actionId === "claim_first_agent");
  const firstAgentClaimWaiting = () => Boolean(gameplayActionDisabledReason(firstAgentClaimAction(), gameplaySummary(), locale()));
  const hasSnapshot = () => Boolean(core.state.snapshot);
  const selectedLabel = () => {
    props.observeViewerStateRevision();
    if (!core.state.selectedKind || !core.state.selectedId) {
      return null;
    }
    if (core.state.selectedKind === "agent" && !core.isAgentVisibleToCurrentSession(core.state.selectedId)) {
      return null;
    }
    return `${core.state.selectedKind}:${core.state.selectedId}`;
  };
  const isSelectedTarget = (kind, id) => {
    props.observeViewerStateRevision();
    return core.state.selectedKind === kind && core.state.selectedId === id;
  };

  return (
    <div class="stack">
      <Show when={selectedLabel()}>
        {(selected) => (
          <div class="badge-row">
            <Badge class="badge badge--accent">{tr(locale(), "已锁定目标", "Locked Target")}</Badge>
            <Badge>{selected()}</Badge>
          </div>
        )}
      </Show>
      <EmptyState>
        {tr(
          locale(),
          "先在 Targets 中锁定一个行动体或地点，再回到世界查看局势；指挥面板只处理当前选中的目标。",
          "Lock onto an agent or location in Targets first, then read the world; the command surface handles only the selected target.",
        )}
      </EmptyState>
      <Show when={firstAgentClaimAction()}>
        {(action) => (
          <CalloutCard
            title={tr(locale(), "认领第一个 Agent", "Claim Your First Agent")}
            badge={firstAgentClaimWaiting() ? "waiting" : "ready"}
            badgeClass={firstAgentClaimWaiting() ? "badge badge--accent" : "badge badge--good"}
            variant={firstAgentClaimWaiting() ? "warn" : null}
          >
            <div class="feedback-summary">
              {gameplayActionDisabledReason(action(), gameplaySummary(), locale())
                || tr(
                  locale(),
                  "当前账户尚未绑定个人 Agent；可使用已发布的认领入口，提交并同步后核对个人归属。",
                  "This account has no bound personal Agent. Use the published claim action, then verify personal ownership after submission and sync.",
                )}
            </div>
            <Show when={firstAgentClaimWaiting()}>
              <props.StarterOcOnboardingPanel
                gameplay={gameplaySummary()}
                locale={locale()}
                waitingForFirstAgent={true}
              />
            </Show>
            <div class="toolbar">
              <button
                class={gameplayActionButtonClass(action())}
                aria-busy={gameplayActionButtonBusyAttrs(action())}
                disabled={gameplayActionButtonDisabled(action(), gameplaySummary(), locale())}
                onClick={() => props.renderGameplayAction(action())}
              >
                {gameplayActionDisplayLabel(action(), locale())}
              </button>
            </div>
          </CalloutCard>
        )}
      </Show>
      <div class="field">
        <label for="entity-search">{tr(locale(), "筛选目标", "Filter targets")}</label>
        <input
          id="entity-search"
          type="search"
          placeholder={tr(locale(), "搜索行动体或地点", "Search agents or locations")}
          value={core.getSelectedSearch()}
          onInput={(event) => core.setSelectedSearch(event.currentTarget.value)}
        />
      </div>
      <div>
        <div class="panel__title panel__title--spaced">{tr(locale(), "行动体", "Agents")}</div>
        <div class="list">
          <Show
            when={lists().agents.length > 0}
            fallback={
              hasSnapshot()
                ? <EmptyState>{tr(locale(), "当前快照里没有行动体。", "No agents in current snapshot.")}</EmptyState>
                : <EntityListPendingState locale={locale()} label={tr(locale(), "行动体", "agents")} />
            }
          >
            <TargetListWindow items={lists().agents} filterKey={filterKey()} scopeKey={scopeKey()} locale={locale()} kind="agent">
              {(agent, index) => {
                const status = () => props.describeAgentSessionStatus(agent().id, locale());
                const displayName = () => agent().name || agent().label || agent().id;
                return (
                  <button
                    class="list-item"
                    data-testid={index() === 0 ? "viewer-playthrough-select-agent" : `viewer-select-agent-${agent().id}`}
                    data-select-kind="agent"
                    data-select-id={agent().id}
                    data-agent-session-status={status().kind}
                    data-selected={isSelectedTarget("agent", agent().id)}
                    aria-pressed={isSelectedTarget("agent", agent().id) ? "true" : "false"}
                    onClick={() => core.applySelection({ kind: "agent", id: agent().id })}
                  >
                    <div class="list-item__header">
                      <div class="list-item__title">{displayName()}</div>
                      <Show when={isSelectedTarget("agent", agent().id)}>
                        <span class="list-item__selected-label">{tr(locale(), "已选中", "Selected")}</span>
                      </Show>
                    </div>
                    <div class="list-item__meta"><span class="entity-id">{agent().id}</span></div>
                    <div class="badge-row">
                      <Badge class={status().badgeClass}>{status().badge}</Badge>
                      <Show when={status().binding.playerId}>
                        <Badge>{`boundPlayer=${status().binding.playerId}`}</Badge>
                      </Show>
                    </div>
                    <div class="list-item__meta">
                      {`${tr(locale(), "地点", "location")}=${agent().location_id} · ${tr(locale(), "资源", "resources")}=${props.renderResourceSummary(agent().resources)}`}
                    </div>
                    <AgentActivitySurface activity={agent().activity} locale={locale()} />
                    <div class="list-item__meta">{status().detail}</div>
                  </button>
                );
              }}
            </TargetListWindow>
          </Show>
        </div>
      </div>
      <div>
        <div class="panel__title panel__title--spaced">{tr(locale(), "地点", "Locations")}</div>
        <div class="list">
          <Show
            when={lists().locations.length > 0}
            fallback={
              hasSnapshot()
                ? <EmptyState>{tr(locale(), "当前快照里没有地点。", "No locations in current snapshot.")}</EmptyState>
                : <EntityListPendingState locale={locale()} label={tr(locale(), "地点", "locations")} />
            }
          >
            <TargetListWindow items={lists().locations} filterKey={filterKey()} scopeKey={scopeKey()} locale={locale()} kind="location">
              {(location) => (
                <button
                  class="list-item"
                  data-testid={`viewer-select-location-${location().id}`}
                  data-select-kind="location"
                  data-select-id={location().id}
                  data-selected={isSelectedTarget("location", location().id)}
                  aria-pressed={isSelectedTarget("location", location().id) ? "true" : "false"}
                  onClick={() => core.applySelection({ kind: "location", id: location().id })}
                >
                  <div class="list-item__header">
                    <div class="list-item__title">{location().name || location().id}</div>
                    <Show when={isSelectedTarget("location", location().id)}>
                      <span class="list-item__selected-label">{tr(locale(), "已选中", "Selected")}</span>
                    </Show>
                  </div>
                  <div class="list-item__meta">
                    {`id=${location().id} · ${tr(locale(), "半径", "radius")}=${
                      core.formatPhysicalDistanceCm(location().profile?.radius_cm, locale()) || "-"
                    } · ${tr(locale(), "资源", "resources")}=${props.renderResourceSummary(location().resources)}`}
                  </div>
                </button>
              )}
            </TargetListWindow>
          </Show>
        </div>
      </div>
    </div>
  );
}

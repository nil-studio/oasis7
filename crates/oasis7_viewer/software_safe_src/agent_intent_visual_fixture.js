export const AGENT_INTENT_STATUSES = [
  "proposed",
  "submitted",
  "accepted",
  "blocked",
  "completed",
  "rejected",
  "expired",
  "cancelled",
  "superseded",
];

export const AGENT_ACTIVITY_STATUSES = ["idle", "executing", "blocked", "waiting", "unavailable", "missing"];
export const AGENT_INTENT_FRESHNESS = ["current", "stale", "conflict", "reconnecting"];
export const AGENT_INTENT_CONTROL_STATES = ["controllable", "control_lost", "read_only", "unauthorized"];
export const AGENT_INTENT_RECEIPT_STATES = ["valid", "missing"];
export const AGENT_INTENT_VARIANTS = ["normal", "duplicate", "replacement"];

function oneOf(value, choices, fallback) {
  return choices.includes(value) ? value : fallback;
}

export function buildAgentIntentFixtureState(search = window.location.search || "") {
  const params = new URLSearchParams(search);
  const value = (name, fallback) => String(params.get(name) || fallback).trim().toLowerCase();
  const status = oneOf(value("intent_status", "accepted"), [...AGENT_INTENT_STATUSES, "missing"], "accepted");
  const freshness = oneOf(value("intent_freshness", "current"), AGENT_INTENT_FRESHNESS, "current");
  const controlState = oneOf(value("intent_control", "controllable"), AGENT_INTENT_CONTROL_STATES, "controllable");
  const activityStatus = oneOf(value("activity_status", "executing"), AGENT_ACTIVITY_STATUSES, "executing");
  const receiptState = oneOf(value("intent_receipt", value("receipt", status === "completed" ? "valid" : "missing")), AGENT_INTENT_RECEIPT_STATES, status === "completed" ? "valid" : "missing");
  const variant = oneOf(value("intent_variant", "normal"), AGENT_INTENT_VARIANTS, "normal");
  return { status, freshness, controlState, activityStatus, receiptState, variant, connectionStatus: "connected" };
}

function activityFor(status) {
  if (status === "missing") {
    return null;
  }
  if (status === "unavailable") {
    return { status: "unavailable" };
  }
  return {
    status,
    operation: status === "idle" ? null : "resource_recovery",
    target: status === "idle" ? null : "factory-activity-target",
    reason: status === "blocked" ? "upstream material is not ready" : null,
    updated_at: 7,
  };
}

export function buildAgentIntentFixtureSnapshot(viewerFixtureBaseSnapshot, state) {
  const {
    status,
    freshness,
    controlState,
    activityStatus,
    receiptState,
    variant,
  } = state;
  const base = viewerFixtureBaseSnapshot();
  const intentId = "agent-intent-v2:headed-matrix";
  const worldId = "live-formal-release-default";
  const intent = status === "missing"
    ? null
    : {
      schema_version: 2,
      intent_id: intentId,
      status,
      message: AGENT_INTENT_SUMMARIES[status],
      resume_required: status === "blocked",
      source_class: "runtime_projection",
      freshness,
      control_state: controlState,
      agent_id: "agent-0",
      world_id: worldId,
      reorg_epoch: 0,
      logical_time: 7,
      updated_at: 7,
      event_seq: "42",
      reason_code: status === "blocked"
        ? "missing_material"
        : status === "rejected"
          ? "permission_changed"
          : status === "expired"
            ? "precondition_changed"
            : status === "cancelled"
              ? "ownership_changed"
              : status === "superseded"
                ? "superseded_by_replacement"
                : variant === "duplicate"
                  ? "duplicate_request"
                  : null,
      reason_summary: status === "blocked"
        ? "World prerequisites changed before execution."
        : status === "rejected"
          ? "The requested operation is no longer authorized."
          : status === "expired"
            ? "The world position changed before execution."
            : null,
      next_step: status === "blocked" ? "Recheck runtime state before resuming." : null,
      receipt_ref: status === "completed" && receiptState === "valid"
        ? {
          intent_id: intentId,
          world_id: worldId,
          reorg_epoch: 0,
          logical_time: 7,
          event_seq: "42",
          receipt_id: "world-event:43",
        }
        : null,
      replaced_by: variant === "replacement" || status === "superseded" ? "agent-intent-v2:replacement" : null,
      duplicate: variant === "duplicate",
    };
  return {
    ...base,
    model: {
      ...base.model,
      agents: {
        ...base.model.agents,
        "agent-0": {
          ...base.model.agents["agent-0"],
          activity: activityFor(activityStatus),
        },
      },
    },
    player_gameplay: {
      ...base.player_gameplay,
      primary_intent: intent,
    },
  };
}

export function installAgentIntentV2VisualFixture(
  fixtures,
  { core, setFixturePlayerAuth, viewerFixtureBaseSnapshot },
) {
  fixtures.agent_intent_v2 = () => {
    const state = buildAgentIntentFixtureState();
    core.injectSnapshot(buildAgentIntentFixtureSnapshot(viewerFixtureBaseSnapshot, state), { returnState: false });
    core.state.connectionStatus = state.connectionStatus;
    core.state.lastError = null;
    core.applySelection({ kind: "agent", id: "agent-0" });
    setFixturePlayerAuth();
    document.body.setAttribute("data-agent-intent-fixture-connection", state.connectionStatus);
    document.body.setAttribute("data-agent-intent-fixture-status", state.status);
    document.body.setAttribute("data-agent-intent-fixture-activity", state.activityStatus);
    document.body.setAttribute("data-agent-intent-fixture-freshness", state.freshness);
    document.body.setAttribute("data-agent-intent-fixture-control", state.controlState);
    document.body.setAttribute("data-agent-intent-fixture-receipt", state.receiptState);
    document.body.setAttribute("data-agent-intent-fixture-variant", state.variant);
    core.requestRender();
  };
}
import { AGENT_INTENT_SUMMARIES } from "./agent_intent_display_model.js";

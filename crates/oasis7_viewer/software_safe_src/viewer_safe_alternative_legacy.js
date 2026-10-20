import { alternativeStatusLabel, alternativePosition, alternativeScopeLabel, alternativeRiskLabel, alternativeAuthorityLabel, alternativeOutcomeLabel, alternativeBlockerLabel } from "./safe_alternative_display.js";
import { safeAlternativeStyles } from "./safe_alternative_styles.js";
export function safeAlternativeMarkup(state, escape) {
  const ui = state.safeAlternative || {};
  const record = ui.record;
  const candidates = (record?.candidates || []).map((c, index) => `<article><h4>${escape(c.goal)}</h4><p>Target: ${escape(alternativePosition(c.to))}</p><p>Scope: ${escape(alternativeScopeLabel(c.scope))}</p><p>Cost upper bound: ${escape(String(c.cost_upper_bound))}</p><p>Risk: ${escape(alternativeRiskLabel(c.risk))}</p><p>Consequences: ${escape(c.consequences)}</p><p>Withdrawal: ${escape(c.retractability)}</p><p>Reason: ${escape(c.recommendation)}</p><p>Authority: ${escape(alternativeAuthorityLabel(c.required_authority))}</p><button data-safe-alt="confirm" data-index="${index}" ${record.state !== "awaiting_confirmation" || ui.status === "unknown" ? "disabled" : ""}>Confirm exact candidate</button></article>`).join("");
  return `<details class="safe-alternative-panel"><style>${safeAlternativeStyles}</style><summary>Safe movement alternatives</summary><p>Waypoint/timed requests are unsupported. Preview and confirmation have no world effect. Confirm and execute separately.</p><label>Original goal<input id="safe-alt-goal"></label><label>Waypoint coordinates<textarea id="safe-alt-points" placeholder='[{"x_cm":0,"y_cm":0,"z_cm":0}]'></textarea></label><label>Requested arrival tick (optional)<input id="safe-alt-arrival" type="number"></label><button data-safe-alt="propose" ${ui.status === "unknown" ? "disabled" : ""}>Compare alternatives</button><button data-safe-alt="recover">Recover original command</button><button data-safe-alt="retry">Retry exact original</button><p role="status">${escape(alternativeStatusLabel(ui.status))} ${escape(ui.error || "")}</p>${record ? `<p>Original goal: ${escape(record.original?.goal || "")}</p><p>Requested waypoints: ${escape((record.original?.waypoints || []).map(alternativePosition).join(" → "))}</p><p>Requested arrival tick: ${escape(String(record.original?.arrival_tick ?? "No timing constraint"))}</p>${record.blocker ? `<p role="status">${escape(alternativeBlockerLabel(record.blocker))}</p>` : ""}${candidates}<button data-safe-alt="execute" ${record.authorization?.state !== "available" || ui.status === "unknown" ? "disabled" : ""}>Execute confirmed choice</button><button data-safe-alt="reevaluate">Reevaluate explicitly</button><button data-safe-alt="read">Read current state</button><button data-safe-alt="cancel">Decline or cancel</button><p>Actual outcome: ${escape(alternativeOutcomeLabel(record.outcome))}</p>` : ""}</details>`;
}
export function bindSafeAlternativeLegacy(state, api, render) {
  document.querySelectorAll("[data-safe-alt]").forEach(button => button.addEventListener("click", async () => {
    try {
      const operation = button.dataset.safeAlt;
      if (operation === "recover" || operation === "retry") return api.recover(operation === "retry");
      const record = state.safeAlternative.record;
      const data = { request_id: record?.request_id };
      if (operation === "propose") Object.assign(data, { request_id: crypto.randomUUID(), original: {
        goal: document.getElementById("safe-alt-goal").value,
        waypoints: JSON.parse(document.getElementById("safe-alt-points").value), arrival_tick: document.getElementById("safe-alt-arrival").value.trim() ? Number(document.getElementById("safe-alt-arrival").value) : null }, valid_until_tick: state.logicalTime + 100 });
      if (operation === "confirm") {
        const candidate = record.candidates[Number(button.dataset.index)];
        Object.assign(data, { generation: record.generation, candidate_id: candidate.candidate_id,
          candidate_digest: candidate.candidate_digest, confirmation_id: crypto.randomUUID(), valid_until_tick: record.valid_until_tick });
      }
      if (operation === "reevaluate") data.valid_until_tick = state.logicalTime + 100;
      if (operation === "execute") Object.assign(data, { authorization_id: record.authorization.authorization_id, execution_id: crypto.randomUUID() });
      await api.command({ operation, data });
    } catch (error) { state.safeAlternative.error = String(error); render(); }
  }));
}

import {
  DRAFT_TEXT,
  helloAck,
  promptSnapshot,
} from "./browser_functional_regression_fixture_server.mjs";
import {
  armSigningGate,
  beginRollbackRequest,
  bootstrapPromptScenario,
  clickApply,
  connectionGeneration,
  countOutbound,
  currentSocketId,
  deliver,
  deliverToSocket,
  disposeViewer,
  ensurePromptEditorVisible,
  expirePendingPromptAck,
  mountViewer,
  openAdvancedPromptDetails,
  openPromptEditor,
  promptControlApplied,
  promptControlError,
  promptFeedbackState,
  promptResultState,
  releaseSigning,
  remountState,
  requestSnapshotCount,
  socketInfo,
  typeDraft,
  waitForBrowserState,
  waitForSigning,
  waitForStableSockets,
  waitForState,
  waitOutbound,
} from "./browser_functional_regression_driver.mjs";

function assert(condition, message, details = undefined) {
  if (!condition) {
    const suffix = details === undefined ? "" : ` | ${JSON.stringify(details)}`;
    throw new Error(`${message}${suffix}`);
  }
}

export const RECOVERY_CASES = {
  "p03-control-lost": async (context) => {
    const { driver, phaseLog, evidence, artifacts, trace } = context;
    await bootstrapPromptScenario(driver, phaseLog, trace);
    await openPromptEditor(driver, trace);
    await typeDraft(driver, DRAFT_TEXT, trace);
    const before = await countOutbound(driver, {});
    await clickApply(driver, trace);
    const sent = await waitOutbound(driver, before.total, { type: "prompt_control", mode: "apply" });
    const request = sent.entry.message.command.request;
    await artifacts.screenshot("pending");
    await deliver(driver, {
      type: "prompt_control_ack",
      request_auth_nonce: request.auth.nonce,
      ack: {
        request_id: request.request_id,
        agent_id: "agent-0",
        operation: "apply",
        status: "blocked",
        reason_code: "control_lost",
        value_visibility: "hidden",
        next_step: "reauthenticate_and_refresh_binding",
        message: "Agent control was lost; re-authenticate and refresh the current binding before retrying.",
      },
    }, trace);
    const state = await waitForState(driver, (state) => state.lastPromptFeedback?.stage === "blocked", "prompt request blocked", 4000);
    const dom = await driver.evalJson(`(() => {
      const feedback = window.__AW_TEST__.getState().lastPromptFeedback;
      const response = feedback?.response || {};
      return JSON.stringify({
        label: document.querySelector(".command-surface__prompt-panel .feedback-card .badge")?.textContent?.trim() || null,
        text: document.querySelector(".command-surface__prompt-panel")?.innerText || "",
        draft: document.querySelector("#prompt-system")?.value ?? null,
        applyDisabled: document.querySelector('[data-prompt-action="apply"]')?.disabled ?? null,
        promptPanel: Boolean(document.querySelector(".command-surface__prompt-panel")),
        recoveryCta: Boolean(document.querySelector('[data-prompt-recovery-action="refresh-binding"]')),
        recoveryText: document.querySelector('[data-prompt-recovery-action="refresh-binding"]')?.textContent?.trim() || null,
        responseKeys: Object.keys(response).sort(),
        responseVisibility: response.value_visibility ?? null,
        responseHasPromptValue: Object.prototype.hasOwnProperty.call(response, "system_prompt_override"),
        bodyMentionsTypedDraft: document.querySelector("#viewer-details-panel")?.innerText.includes(${JSON.stringify(DRAFT_TEXT)}) ?? null,
      });
    })()`);
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 500));
    const totals = await countOutbound(driver, { type: "prompt_control" });
    evidence.observations.controlLost = { stage: state.lastPromptFeedback.stage, dom, promptRequestCount: totals.count };
    phaseLog.push("correlated control-lost receipt rendered its safe reason, next step and recovery action");
    assert(dom.label === "Blocked", "control-lost receipt was not rendered as blocked", dom.label);
    assert(dom.promptPanel === true, "control-lost receipt removed the prompt surface", dom.promptPanel);
    assert(dom.draft === DRAFT_TEXT, "control-lost receipt cleared the unsubmitted draft", dom.draft);
    assert(dom.applyDisabled === false, "control-lost receipt left the Apply control stuck", dom.applyDisabled);
    assert(dom.recoveryCta === true, "control-lost receipt did not expose the binding recovery action", dom.recoveryText);
    assert(/Next step.*Re-authenticate and refresh the current binding/i.test(dom.text), "control-lost receipt did not surface the safe next step", dom.text.slice(0, 500));
    assert(!/Prompt applied to the current runtime/i.test(dom.text), "control-lost path showed applied before any result", dom.text.slice(0, 400));
    assert(dom.responseVisibility === "hidden", "control-lost response did not mark hidden value visibility", dom.responseKeys);
    assert(dom.responseHasPromptValue === false, "control-lost response leaked the submitted prompt value", dom.responseKeys);
    assert(dom.bodyMentionsTypedDraft === false, "control-lost diagnostics rendered the submitted prompt value outside the draft editor", dom.bodyMentionsTypedDraft);
    assert(totals.count === 1, "control-lost path retried the prompt request", totals);
    await artifacts.screenshot("final");
  },

  // Second control-loss shape. The enhanced runtime converts a lost
  // authority/binding into a hidden blocked error (`prompt_control_result_unknown`)
  // instead of a bare agent_control_forbidden legacy error; the player surface
  // must still keep the draft, keep the control lane usable, show the safe next
  // step and never leak submitted values.
  "p03-control-lost-unknown": async (context) => {
    const { driver, phaseLog, evidence, artifacts, trace } = context;
    await bootstrapPromptScenario(driver, phaseLog, trace);
    await openPromptEditor(driver, trace);
    await typeDraft(driver, DRAFT_TEXT, trace);
    const before = await countOutbound(driver, {});
    await clickApply(driver, trace);
    const sent = await waitOutbound(driver, before.total, { type: "prompt_control", mode: "apply" });
    const request = sent.entry.message.command.request;
    await artifacts.screenshot("pending");
    await deliver(driver, {
      type: "prompt_control_error",
      request_auth_nonce: request.auth.nonce,
      error: {
        request_id: request.request_id,
        agent_id: "agent-0",
        operation: "apply",
        code: "prompt_control_result_unknown",
        reason_code: "result_unknown",
        status: "blocked",
        value_visibility: "hidden",
        next_step: "refresh_authority_and_retry_with_new_request_id",
        message: "prompt control result is unknown for the current authority epoch",
        authority_epoch: "stale-authority-0",
        session_epoch: 99,
        binding_epoch: 99,
        current_version: 9,
        system_prompt_override: "runtime secret prompt value",
      },
    }, trace);
    const state = await waitForState(driver, (state) => state.lastPromptFeedback?.stage === "blocked", "prompt request blocked", 4000);
    const dom = await driver.evalJson(`(() => {
      const value = window.__AW_TEST__.getState();
      const feedback = value.lastPromptFeedback;
      const response = feedback?.response || {};
      return JSON.stringify({
        label: document.querySelector(".command-surface__prompt-panel .feedback-card .badge")?.textContent?.trim() || null,
        text: document.querySelector(".command-surface__prompt-panel")?.innerText || "",
        draft: document.querySelector("#prompt-system")?.value ?? null,
        applyDisabled: document.querySelector('[data-prompt-action="apply"]')?.disabled ?? null,
        promptPanel: Boolean(document.querySelector(".command-surface__prompt-panel")),
        selectedKind: value.selectedKind,
        selectedId: value.selectedId,
        responseKeys: Object.keys(response).sort(),
        responseVisibility: response.value_visibility ?? null,
        leaksPromptValue: JSON.stringify(response).includes("runtime secret prompt value"),
        leaksEpoch: JSON.stringify(response).includes("99"),
        bodyLeaksPromptValue: (document.getElementById("viewer-details-panel")?.innerText || "").includes("runtime secret prompt value"),
      });
    })()`);
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 500));
    const totals = await countOutbound(driver, { type: "prompt_control" });
    evidence.observations.controlLostUnknown = { stage: state.lastPromptFeedback.stage, dom, promptRequestCount: totals.count };
    phaseLog.push("hidden blocked runtime error kept selection, draft and usable controls");
    assert(dom.label === "Blocked", "hidden blocked error was not rendered as blocked", dom.label);
    assert(dom.selectedKind === "agent" && dom.selectedId === "agent-0", "hidden blocked error cleared the selected Agent", { kind: dom.selectedKind, id: dom.selectedId });
    assert(dom.promptPanel === true, "hidden blocked error removed the prompt surface", dom.promptPanel);
    assert(dom.draft === DRAFT_TEXT, "hidden blocked error cleared the unsubmitted draft", dom.draft);
    assert(dom.applyDisabled === false, "hidden blocked error left the Apply control stuck", dom.applyDisabled);
    assert(/Refresh authority, then retry with a new request id/i.test(dom.text), "hidden blocked error did not surface the safe next step", dom.text.slice(0, 500));
    assert(!/Prompt applied to the current runtime/i.test(dom.text), "hidden blocked error showed applied before any result", dom.text.slice(0, 400));
    assert(dom.responseVisibility === "hidden", "hidden blocked response did not retain hidden visibility", dom.responseKeys);
    assert(dom.leaksPromptValue === false, "hidden blocked response leaked the runtime prompt value", dom.responseKeys);
    assert(dom.leaksEpoch === false, "hidden blocked response leaked stale binding/session epochs", dom.responseKeys);
    assert(dom.bodyLeaksPromptValue === false, "hidden blocked diagnostics rendered the runtime prompt value", dom.responseKeys);
    assert(totals.count === 1, "hidden blocked path retried the prompt request", totals);
    await artifacts.screenshot("final");
  },

  // P04: the old request is terminated while the new request is fixed in its
  // real signing stage. A late result for A must not touch B's timer, request
  // identity, feedback or transport. The deferred signer only delays the real
  // WebCrypto signature result; it never fabricates or edits the payload.
  "p04-signing-deferred": async (context) => {
    const { driver, phaseLog, evidence, artifacts, trace } = context;
    await bootstrapPromptScenario(driver, phaseLog, trace);
    await openPromptEditor(driver, trace);
    const rollback = await beginRollbackRequest(driver, trace);
    phaseLog.push("old A rollback request observed once through real UI controls");
    const terminated = await expirePendingPromptAck(driver, trace);
    assert(terminated.stage === "error", "old A request did not terminate before B started", terminated);
    const armed = await armSigningGate(driver);
    evidence.observations.signingGate = armed;
    await typeDraft(driver, DRAFT_TEXT, trace);
    const before = await countOutbound(driver, {});
    await clickApply(driver, trace);
    const signing = await waitForSigning(driver);
    const signingFeedback = await promptFeedbackState(driver);
    evidence.observations.signing = { signing, feedback: signingFeedback };
    assert(signingFeedback.stage === "signing", "B was not fixed in its signing stage", signingFeedback);
    const outboundWhileSigning = await countOutbound(driver, { type: "prompt_control" });
    assert(outboundWhileSigning.count === 1, "B reached the transport before its real signature completed", outboundWhileSigning);
    await deliver(driver, promptControlError(rollback, "late error for terminated request A"), trace);
    const afterLate = await promptFeedbackState(driver);
    evidence.observations.feedbackAfterLateA = afterLate;
    assert(afterLate.stage === "signing", "a late result for A moved B out of signing", { before: signingFeedback, after: afterLate });
    assert(afterLate.requestId === signingFeedback.requestId && afterLate.requestAuthNonce === signingFeedback.requestAuthNonce, "a late result for A rewrote B request identity", { before: signingFeedback, after: afterLate });
    const released = await releaseSigning(driver, trace);
    evidence.observations.released = released;
    const sent = await waitOutbound(driver, before.total, { type: "prompt_control", mode: "apply" }, 8000, trace);
    const requestB = sent.entry.message.command.request;
    evidence.observations.requestA = { requestId: rollback.request_id, nonce: rollback.auth.nonce, toVersion: rollback.to_version };
    evidence.observations.requestB = {
      requestId: requestB.request_id,
      nonce: requestB.auth.nonce,
      expectedVersion: requestB.expected_version,
      sessionEpoch: requestB.session_epoch,
      bindingEpoch: requestB.binding_epoch,
    };
    assert(requestB.request_id && requestB.request_id !== rollback.request_id, "B reused the terminated A request identity", { A: rollback.request_id, B: requestB.request_id });
    assert(requestB.auth.nonce !== rollback.auth.nonce, "B reused the terminated A auth nonce", { A: rollback.auth.nonce, B: requestB.auth.nonce });
    assert(requestB.expected_version === 3, "B request did not carry the visible prompt version", requestB.expected_version);
    const sentFeedback = await promptFeedbackState(driver);
    assert(sentFeedback.stage === "sent" && sentFeedback.requestId === requestB.request_id, "B feedback did not follow its real send", sentFeedback);
    await deliver(driver, promptControlError(rollback, "late error for terminated request A after B sent"), trace);
    const afterSentLate = await promptFeedbackState(driver);
    evidence.observations.feedbackAfterSentLateA = afterSentLate;
    assert(afterSentLate.stage === "sent", "a late result for A changed B after B was sent", afterSentLate);
    assert(afterSentLate.requestId === requestB.request_id && afterSentLate.requestAuthNonce === requestB.auth.nonce, "a late result for A rewrote B identity after B was sent", afterSentLate);
    const timer = await expirePendingPromptAck(driver, trace);
    evidence.observations.timerOwnership = timer;
    assert(timer.result === true, "B lost its own acknowledgement timer to the late A result", timer);
    const totals = await countOutbound(driver, { type: "prompt_control" });
    assert(totals.count === 2, "A/B scenario sent an unexpected prompt request count", totals);
    await artifacts.screenshot("final");
  },

  // P04 variant: B is already sent when A's late error arrives. B must keep its
  // own pending state and only its own correlated applied result may settle it.
  "p04-sent-late-error": async (context) => {
    const { driver, phaseLog, evidence, artifacts, trace } = context;
    await bootstrapPromptScenario(driver, phaseLog, trace);
    await openPromptEditor(driver, trace);
    const rollback = await beginRollbackRequest(driver, trace);
    const terminated = await expirePendingPromptAck(driver, trace);
    assert(terminated.stage === "error", "old A request did not terminate", terminated);
    await typeDraft(driver, DRAFT_TEXT, trace);
    const before = await countOutbound(driver, {});
    await clickApply(driver, trace);
    const sent = await waitOutbound(driver, before.total, { type: "prompt_control", mode: "apply" });
    const requestB = sent.entry.message.command.request;
    evidence.observations.requestA = { requestId: rollback.request_id, nonce: rollback.auth.nonce };
    evidence.observations.requestB = { requestId: requestB.request_id, nonce: requestB.auth.nonce };
    await deliver(driver, promptControlError(rollback, "late error for terminated request A"), trace);
    const afterLate = await promptFeedbackState(driver);
    evidence.observations.feedbackAfterLateA = afterLate;
    assert(afterLate.stage === "sent", "a late result for A settled B after B was sent", afterLate);
    assert(afterLate.requestId === requestB.request_id && afterLate.requestAuthNonce === requestB.auth.nonce, "a late result for A rewrote the sent B identity", afterLate);
    await deliver(driver, promptControlApplied(requestB), trace);
    const applied = await waitForState(driver, (state) => state.lastPromptFeedback?.stage === "applied", "prompt result applied");
    evidence.observations.applied = { stage: applied.lastPromptFeedback.stage, requestId: applied.lastPromptFeedback.requestId };
    assert(applied.lastPromptFeedback.requestId === requestB.request_id, "applied result was not correlated with B", applied.lastPromptFeedback.requestId);
    const totals = await countOutbound(driver, { type: "prompt_control" });
    assert(totals.count === 2, "late A error caused an extra prompt request", totals);
    await artifacts.screenshot("final");
  },

  // P05: duplicate and out-of-order results after B settled must not create a
  // second refresh/version advance or let the old A result overwrite B. A
  // locally timed-out request is not authoritative: its correlated server
  // result must still settle the feedback, once, without a later duplicate
  // re-triggering the refresh.
  "p05-duplicate-out-of-order": async (context) => {
    const { driver, phaseLog, evidence, artifacts, trace } = context;
    await bootstrapPromptScenario(driver, phaseLog, trace);
    await openPromptEditor(driver, trace);
    const rollback = await beginRollbackRequest(driver, trace);
    const terminated = await expirePendingPromptAck(driver, trace);
    assert(terminated.stage === "error", "old A request did not terminate", terminated);
    await typeDraft(driver, DRAFT_TEXT, trace);
    const before = await countOutbound(driver, {});
    await clickApply(driver, trace);
    const sent = await waitOutbound(driver, before.total, { type: "prompt_control", mode: "apply" });
    const requestB = sent.entry.message.command.request;
    evidence.observations.requestA = { requestId: rollback.request_id, nonce: rollback.auth.nonce };
    evidence.observations.requestB = { requestId: requestB.request_id, nonce: requestB.auth.nonce };
    await deliver(driver, {
      type: "prompt_control_ack",
      request_auth_nonce: requestB.auth.nonce,
      ack: { request_id: requestB.request_id, agent_id: "agent-0", operation: "apply", status: "accepted" },
    }, trace);
    await waitForState(driver, (state) => state.lastPromptFeedback?.stage === "accepted", "prompt request accepted");
    await deliver(driver, promptControlApplied(requestB), trace);
    await waitForState(driver, (state) => state.lastPromptFeedback?.stage === "applied", "prompt result applied");
    await waitOutbound(driver, 0, { type: "request_snapshot" }, 8000, trace);
    await deliver(driver, { type: "snapshot", snapshot: promptSnapshot({ version: 4, override: DRAFT_TEXT, time: 8 }) }, trace);
    await waitForState(driver, (state) => state.selectedPromptVersion === 4, "prompt version 4 adopted");
    phaseLog.push("B applied result and its authoritative version 4 snapshot were adopted");
    const beforeDuplicate = await requestSnapshotCount(driver);
    await deliver(driver, promptControlApplied(requestB), trace);
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 300));
    const afterDuplicate = await requestSnapshotCount(driver);
    const duplicateResult = await promptResultState(driver);
    evidence.observations.duplicate = { beforeDuplicate, afterDuplicate, result: duplicateResult };
    assert(afterDuplicate.count === beforeDuplicate.count, "a duplicate applied result triggered another snapshot refresh/commit", { beforeDuplicate, afterDuplicate });
    assert(duplicateResult.selectedPromptVersion === 4, "a duplicate applied result advanced or reset the visible version", duplicateResult);
    assert(duplicateResult.draftSystemPrompt === DRAFT_TEXT, "a duplicate applied result rewrote the applied draft value", duplicateResult);
    await deliver(driver, promptControlError(rollback, "old A error after B applied"), trace);
    await deliver(driver, promptControlApplied(rollback), trace);
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 300));
    const outOfOrder = await promptResultState(driver);
    evidence.observations.outOfOrder = outOfOrder;
    assert(outOfOrder.lastPromptFeedback?.stage === "applied", "an old A result overwrote the applied B feedback", outOfOrder);
    assert(outOfOrder.lastPromptFeedback?.requestId === requestB.request_id, "an old A result replaced the current B request id", outOfOrder);
    assert(outOfOrder.selectedPromptVersion === 4, "an old A result reverted the visible prompt version", outOfOrder);
    const LATE_TEXT = "late authoritative prompt";
    await typeDraft(driver, LATE_TEXT, trace);
    const beforeC = await countOutbound(driver, {});
    await clickApply(driver, trace);
    const sentC = await waitOutbound(driver, beforeC.total, { type: "prompt_control", mode: "apply" }, 8000, trace);
    const requestC = sentC.entry.message.command.request;
    evidence.observations.requestC = { requestId: requestC.request_id, nonce: requestC.auth.nonce };
    const timedOut = await expirePendingPromptAck(driver, trace);
    const timedOutState = await promptFeedbackState(driver);
    evidence.observations.timedOutC = { timer: timedOut, feedback: timedOutState };
    assert(timedOutState.stage === "error" && timedOutState.requestId === requestC.request_id, "C did not settle locally as timed out", timedOutState);
    const beforeLateAck = await requestSnapshotCount(driver);
    await deliver(driver, promptControlApplied(requestC, 5, LATE_TEXT), trace);
    const lateSettled = await waitForState(driver, (state) => state.lastPromptFeedback?.stage === "applied" && state.lastPromptFeedback?.requestId === requestC.request_id, "late result applied to the timed-out request");
    await waitOutbound(driver, beforeLateAck.total, { type: "request_snapshot" }, 8000, trace);
    await deliver(driver, { type: "snapshot", snapshot: promptSnapshot({ version: 5, override: LATE_TEXT, time: 9 }) }, trace);
    await waitForState(driver, (state) => state.selectedPromptVersion === 5, "prompt version 5 adopted");
    const lateResult = await promptResultState(driver);
    evidence.observations.lateAuthoritative = { settled: lateSettled.lastPromptFeedback, result: lateResult };
    assert(lateResult.lastPromptFeedback?.stage === "applied" && lateResult.lastPromptFeedback?.requestId === requestC.request_id, "the authoritative result did not settle the locally timed-out request", lateResult);
    assert(lateResult.selectedPromptVersion === 5, "the authoritative result did not refresh the timed-out request version", lateResult);
    const beforeLateDuplicate = await requestSnapshotCount(driver);
    await deliver(driver, promptControlApplied(requestC, 5, LATE_TEXT), trace);
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 300));
    const afterLateDuplicate = await requestSnapshotCount(driver);
    evidence.observations.lateDuplicate = { beforeDuplicate: beforeLateDuplicate, afterDuplicate: afterLateDuplicate };
    assert(afterLateDuplicate.count === beforeLateDuplicate.count, "a duplicate late result re-triggered the settled request refresh", { beforeLateDuplicate, afterLateDuplicate });
    const totals = await countOutbound(driver, { type: "prompt_control" });
    assert(totals.count === 3, "duplicate/out-of-order or late-result handling caused an extra prompt request", totals);
    await artifacts.screenshot("final");
  },

  // P06: a held request is stranded by the disconnect; the next connection gets
  // its own generation, old-socket events cannot negotiate or overwrite the new
  // snapshot, submission stays closed until the new binding is restored, and a
  // new visible request settles only on its own correlated result.
  "p06-reconnect-recovery": async (context) => {
    const { driver, phaseLog, evidence, artifacts, trace } = context;
    await bootstrapPromptScenario(driver, phaseLog, trace);
    await openPromptEditor(driver, trace);
    await typeDraft(driver, DRAFT_TEXT, trace);
    const before = await countOutbound(driver, {});
    await clickApply(driver, trace);
    const sent = await waitOutbound(driver, before.total, { type: "prompt_control", mode: "apply" });
    const requestA = sent.entry.message.command.request;
    evidence.observations.heldRequest = { requestId: requestA.request_id, nonce: requestA.auth.nonce, sessionEpoch: requestA.session_epoch, bindingEpoch: requestA.binding_epoch };
    const generationBefore = await connectionGeneration(driver);
    const oldSocketId = await currentSocketId(driver);
    const afterCloseIndex = (await countOutbound(driver, {})).total;
    await driver.evalJson("JSON.stringify(window.__BFR__.closeSocket())");
    trace.push("close-socket", { socketId: oldSocketId });
    const socketList = await waitForStableSockets(driver, 2);
    const newSocketId = await currentSocketId(driver);
    await waitOutbound(driver, afterCloseIndex, { type: "hello_v2" }, 12000, trace);
    const generationAfter = await connectionGeneration(driver);
    evidence.observations.connection = { generationBefore, generationAfter, oldSocketId, newSocketId, sockets: socketList };
    assert(newSocketId !== oldSocketId, "reconnect reused the previous socket", { oldSocketId, newSocketId });
    assert(generationAfter > generationBefore, "reconnect did not advance the auth connection generation", { generationBefore, generationAfter });
    // Old-socket events are queued after the new socket exists, then released.
    await deliverToSocket(driver, oldSocketId, helloAck, trace);
    await deliverToSocket(driver, oldSocketId, { type: "snapshot", snapshot: promptSnapshot({ version: 9, override: "old-connection-secret", time: 99 }) }, trace);
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 300));
    const afterOldHandshake = await driver.evalJson(`(() => JSON.stringify({
      state: window.__AW_TEST__.getState(),
      selectedPromptVersion: window.__AW_TEST__.getState().selectedPromptVersion,
    }))()`);
    evidence.observations.afterOldHandshake = afterOldHandshake;
    assert(afterOldHandshake.state.viewerProtocol?.negotiated === false, "an old-connection hello negotiated the new connection", afterOldHandshake.state.viewerProtocol);
    assert(afterOldHandshake.selectedPromptVersion !== 9, "an old-connection snapshot was adopted as the current world state", afterOldHandshake);
    const recoveryFrom = (await countOutbound(driver, {})).total;
    await deliver(driver, helloAck, trace);
    await waitOutbound(driver, recoveryFrom, { type: "authoritative_recovery", mode: "reconnect_sync" }, 8000, trace);
    const unready = await driver.evalJson(`(() => {
      const apply = document.querySelector('[data-prompt-action="apply"]');
      return JSON.stringify({
        applyFound: Boolean(apply),
        applyEnabled: apply ? apply.disabled === false : false,
        registration: window.__AW_TEST__.getState().authRegistrationStatus,
        sessionEpoch: window.__AW_TEST__.getState().authSessionEpoch,
      });
    })()`);
    evidence.observations.beforeRebind = unready;
    assert(unready.applyEnabled === false, "prompt submission opened before the new connection restored the session binding", unready);
    const identity = await driver.evalJson("JSON.stringify(window.__BFR__.identity())");
    await deliver(driver, {
      type: "authoritative_recovery_ack",
      ack: { status: "catch_up_ready", player_id: identity.playerId, session_pubkey: identity.publicKey, agent_id: "agent-0", session_epoch: 13, binding_epoch: 10 },
    }, trace);
    await waitForState(driver, (state) => state.authRegistrationStatus === "registered" && state.authBindingEpoch === 10 && state.authSessionEpoch === 13, "reconnected fixture auth binding registered");
    await deliver(driver, { type: "snapshot", snapshot: promptSnapshot() }, trace);
    await waitForState(driver, (state) => state.selectedKind === "agent" && state.selectedId === "agent-0", "fixture agent selected");
    phaseLog.push("new connection restored hello, session/binding epochs and canonical snapshot");
    await deliverToSocket(driver, oldSocketId, { type: "snapshot", snapshot: promptSnapshot({ version: 9, override: "old-connection-secret", time: 99 }) }, trace);
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 300));
    const afterOldSnapshot = await promptResultState(driver);
    evidence.observations.afterOldSocketSnapshot = afterOldSnapshot;
    assert(afterOldSnapshot.selectedPromptVersion === 3, "an old-connection snapshot overwrote the new connection profile", afterOldSnapshot);
    assert(afterOldSnapshot.draftSystemPrompt !== "old-connection-secret", "an old-connection snapshot leaked into the current prompt draft", afterOldSnapshot);
    // The reconnect re-render recreates the advanced details element closed;
    // re-open it through the real summary control before submitting.
    await openAdvancedPromptDetails(driver, trace);
    await waitForBrowserState(driver, `() => document.querySelector('[data-prompt-action="apply"]')?.disabled === false`, "prompt apply control enabled", 8000);
    await typeDraft(driver, DRAFT_TEXT, trace);
    const beforeB = await countOutbound(driver, {});
    await clickApply(driver, trace);
    const sentB = await waitOutbound(driver, beforeB.total, { type: "prompt_control", mode: "apply" });
    const requestB = sentB.entry.message.command.request;
    evidence.observations.recoveredRequest = { requestId: requestB.request_id, nonce: requestB.auth.nonce, sessionEpoch: requestB.session_epoch, bindingEpoch: requestB.binding_epoch };
    assert(requestB.request_id !== requestA.request_id, "new connection reused the stranded request identity", { A: requestA.request_id, B: requestB.request_id });
    assert(requestB.session_epoch === 13 && requestB.binding_epoch === 10, "new request did not use the recovered session/binding epochs", requestB);
    const sentFeedback = await promptFeedbackState(driver);
    evidence.observations.feedbackAfterSend = sentFeedback;
    assert(sentFeedback.stage === "sent" && sentFeedback.requestId === requestB.request_id, "new request feedback did not follow its real send", sentFeedback);
    await deliverToSocket(driver, oldSocketId, promptControlError(requestA, "old-connection error for the pre-disconnect request"), trace);
    const afterOldError = await promptFeedbackState(driver);
    evidence.observations.afterOldSocketError = afterOldError;
    assert(afterOldError.stage === "sent" && afterOldError.requestId === requestB.request_id, "an old-connection result consumed the new request wait", afterOldError);
    await deliver(driver, promptControlApplied(requestB), trace);
    await waitForState(driver, (state) => state.lastPromptFeedback?.stage === "applied", "prompt result applied");
    phaseLog.push("old-connection late events were ignored and the new request settled only on its own result");
    await artifacts.screenshot("final");
  },

  // P07: the real disposer stops DOM updates without logging out or closing the
  // shared transport; remounting reuses that transport and one visible click
  // still sends exactly one request with one prompt surface.
  "p07-unmount-remount": async (context) => {
    const { driver, phaseLog, evidence, artifacts, trace } = context;
    await bootstrapPromptScenario(driver, phaseLog, trace);
    await openPromptEditor(driver, trace);
    const initial = await remountState(driver);
    evidence.observations.initial = initial;
    assert(initial.available === true && initial.mountCount === 1, "visual-test page did not expose the initial mount lifecycle", initial);
    assert(initial.detailsPanel === true && initial.applyControls === 1, "initial mount did not render one prompt surface", initial);
    const socketBefore = await currentSocketId(driver);
    const socketsBefore = await socketInfo(driver);
    await disposeViewer(driver, trace);
    const disposed = await remountState(driver);
    evidence.observations.disposed = disposed;
    assert(disposed.rootChildren === 0 && disposed.viewerShells === 0 && disposed.detailsPanel === false, "real disposer left a rendered viewer tree mounted", disposed);
    await deliver(driver, { type: "snapshot", snapshot: promptSnapshot({ version: 4, override: "post-dispose-prompt", time: 9 }) }, trace);
    await waitForState(driver, (state) => state.selectedPromptVersion === 4, "prompt version 4 adopted", 6000);
    const disposedAfterUpdate = await remountState(driver);
    evidence.observations.disposedAfterUpdate = disposedAfterUpdate;
    assert(disposedAfterUpdate.viewerShells === 0 && disposedAfterUpdate.detailsPanel === false, "a disposed viewer updated DOM after a transport message", disposedAfterUpdate);
    const mountCount = await mountViewer(driver, trace);
    assert(mountCount === 2, "real remount did not run through the viewer mount entry", mountCount);
    await waitForBrowserState(driver, `() => Boolean(document.querySelector(".viewer-shell"))`, "viewer shell remounted", 8000);
    // The pixel-world focus UI session outlives the component lifecycle, so a
    // remount restores cinematic focus. Leave it through the visible control
    // before exercising the command surface.
    // A replaced route target keeps the old URL fragment but Chrome does not
    // re-resolve :target for the recreated element, so the command route stays
    // hidden after a real remount. Re-enter the same in-page route the way the
    // shell's links do; this is hash navigation, not a reload.
    const routeState = await driver.evalJson(`(() => {
      window.location.hash = "#viewer-stage-panel";
      window.location.hash = "#viewer-details-panel";
      return JSON.stringify({ hash: window.location.hash, route: document.getElementById("viewer-details-panel")?.getAttribute("data-route-open") ?? null });
    })()`);
    evidence.observations.routeAfterRemount = routeState;
    await waitForBrowserState(driver, `() => getComputedStyle(document.getElementById("viewer-details-panel")).display !== "none"`, "viewer details panel visible", 5000);
    await ensurePromptEditorVisible(driver, trace);
    await ensurePromptEditorVisible(driver, trace);
    await ensurePromptEditorVisible(driver, trace);
    const remounted = await remountState(driver);
    evidence.observations.remounted = remounted;
    assert(remounted.viewerShells === 1 && remounted.promptPanels === 1 && remounted.applyControls === 1, "remount duplicated the viewer or prompt surface", remounted);
    assert(remounted.mountCount === 2, "remount count did not advance exactly once", remounted);
    const socketAfter = await currentSocketId(driver);
    const socketsAfter = await socketInfo(driver);
    evidence.observations.transport = { socketBefore, socketAfter, socketsBefore, socketsAfter };
    assert(socketAfter === socketBefore, "dispose/remount replaced the shared transport socket", { socketBefore, socketAfter });
    const currentAfter = (socketsAfter || []).find((entry) => entry.id === socketAfter);
    assert(currentAfter?.readyState === 1, "dispose/remount closed the current shared transport socket", socketsAfter);
    await typeDraft(driver, DRAFT_TEXT, trace);
    const before = await countOutbound(driver, {});
    await clickApply(driver, trace);
    const sent = await waitOutbound(driver, before.total, { type: "prompt_control", mode: "apply" });
    const request = sent.entry.message.command.request;
    evidence.observations.request = { requestId: request.request_id, nonce: request.auth.nonce, expectedVersion: request.expected_version };
    assert(request.expected_version === 4, "remounted visible form did not carry the current prompt version", request.expected_version);
    const totals = await countOutbound(driver, { type: "prompt_control" });
    assert(totals.count === 1, "one visible click after remount produced duplicate requests", totals);
    await deliver(driver, promptControlApplied(request), trace);
    await waitForState(driver, (state) => state.lastPromptFeedback?.stage === "applied", "prompt result applied");
    phaseLog.push("single visible click after remount sent one request and settled on its correlated result");
    await artifacts.screenshot("final");
  },
};

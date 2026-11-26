import { requestGameplayResult, receiveGameplayResult } from "./viewer_gameplay_result_recovery.js";
import { getGameplayRecoveryScope, gameplaySendContextCurrent } from "./viewer_gameplay_recovery_scope.js";
import { normalizeGameplayActionRequest } from "./viewer_feedback_actions.js";
import { createLiveControlStateController, LIVE_CONTROL_STATE_CAPABILITY } from "./viewer_live_control_state.js";
import { matchesGameplayReply, recoverGameplayFeedback, settleGameplayOriginal } from "./viewer_gameplay_receipt_feedback.js";
import { quoteDependencyFingerprint, coordinateDependentQuotes } from "./viewer_quote_dependencies.js";
import { createSafeAlternativeModule } from "./viewer_safe_alternative_module.js";
import { bindFeedbackScope } from "./viewer_feedback_scope.js";
import { createScopeSnapshotTransport } from "./pixel_world_scope_snapshot.js";
import { createWorldViewTransport, worldViewScopeKey } from "./pixel_world_world_view.js";
import { createHostedWorldAccount } from "./viewer_hosted_world_account_module.js";
import { createCanonicalOwnerReadModule } from "./viewer_canonical_owner_read_module.js";
import { createViewerHostedAccountLoginIssuer } from "./viewer_hosted_account_login_issuer.js";
import { invalidateAuthConnection, authConnectionGeneration, authCredentials, installSession, clearSession, hasSigningIdentity, updateRegistrationGrant, captureSessionContext, isSessionContextCurrent } from "./viewer_auth_session_module.js";
import { viewerRuntimeConfig, resolveViewerEndpoint } from "./viewer_runtime_config_module.js";
import { validateRuntimeAckIdentity } from "./viewer_runtime_ack_identity.js";
import { createViewerAuthSurfaceModule } from "./viewer_auth_surface_module.js";
import { createViewerFeedbackModule } from "./viewer_feedback_module.js";
import { createViewerHostedAuthStateModule } from "./viewer_hosted_auth_state_module.js";
import { createViewerHostedTestLoginModule } from "./viewer_hosted_test_login_module.js";
import { scheduleCanonicalGoalPending, applyCommittedChatAck, canonicalAckMatches } from "./viewer_canonical_goal_module.js";
import { createViewerHostedLoginRegistrationBridge } from "./viewer_hosted_login_registration_bridge.js";
import { createViewerAgentChatAuthModule } from "./viewer_agent_chat_auth_module.js";
import { createViewerHostedSessionRefreshModule } from "./viewer_hosted_session_refresh_module.js";
import { createViewerHostedSessionReconnectModule } from "./viewer_hosted_session_reconnect_module.js";
import { createViewerPromptControlModule } from "./viewer_prompt_control_module.js";
import { createViewerControlLossModule } from "./viewer_control_loss_module.js";
import { resetHostedLoginChallenge as resetHostedLoginChallengeState } from "./viewer_hosted_login_state_module.js";
import { createViewerLocalePreferencesModule } from "./viewer_locale_preferences_module.js";
import { createViewerBrowserPersistenceModule } from "./viewer_browser_persistence_module.js";
import { createViewerBrowserRaceIdentityTestApi } from "./viewer_browser_race_identity_test_api_module.js";
import { createViewerWorldScaleModule } from "./viewer_world_scale_module.js";
import { createRefineQuotePreflightStateModule } from "./refine_quote_preflight_state.js";
import { createProductValidationQuoteIntegration } from "./product_validation_quote_integration.js";
import { createPowerSurvivalQuoteIntegration } from "./power_survival_quote_integration.js"; import { createScheduleRecipeQuoteIntegration } from "./schedule_recipe_quote_integration.js"; import { createTransferMaterialQuoteIntegration } from "./transfer_material_quote_integration.js";
import { createPowerSaleQuoteIntegration } from "./power_sale_quote_integration.js";
import { createFragmentRefillPreviewIntegration } from "./fragment_refill_preview_integration.js";
import { createGovernanceVoteQuoteIntegration } from "./governance_vote_quote_integration.js";
import { createWarDeclarationQuoteIntegration } from "./war_declaration_quote_integration.js";
import { createMarketQuoteDecisionIntegration } from "./market_quote_decision_integration.js";
import { createViewerQuoteProtocolFacade } from "./viewer_quote_protocol_facade.js";
import { createViewerApplication } from "./viewer_application.js";
import { buildViewerEntityLists, resourceSummary } from "./viewer_entity_list_renderer.js";
import {
  DEFAULT_WS_ADDR,
  HOSTED_ACCOUNT_LOGIN_COMPLETE_ROUTE,
  HOSTED_ACCOUNT_LOGIN_START_ROUTE,
  HOSTED_ACCOUNT_TEST_LOGIN_ROUTE,
  HOSTED_PLAYER_SESSION_ADMISSION_ROUTE,
  HOSTED_PLAYER_SESSION_REFRESH_INTERVAL_MS,
  HOSTED_PLAYER_SESSION_REFRESH_ROUTE,
  HOSTED_PLAYER_SESSION_RELEASE_ROUTE,
  HOSTED_PLAYER_SESSION_STORAGE_PREFIX,
  HOSTED_STRONG_AUTH_GRANT_ROUTE,
  LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE,
  MAX_DECISION_TRACES,
  MAX_EVENTS,
  PROMPT_OVERRIDES_VISIBILITY_STORAGE_PREFIX,
  RENDER_META_GLOBAL_NAME,
  SOFTWARE_RENDERER_MARKERS,
  SOFTWARE_SAFE_RENDER_MODE_ALIAS,
  TEST_API_GLOBAL_NAME,
  UI_LOCALE_STORAGE_PREFIX,
  VIEWER_AUTH_BOOTSTRAP_OBJECT,
  VIEWER_AUTH_PRIVATE_KEY,
  VIEWER_AUTH_PUBLIC_KEY,
  VIEWER_PLAYER_ID_KEY,
  VIEWER_RENDER_MODE,
  isHostedPublicJoinDeploymentMode,
} from "./software_safe_constants.js";
import { createSoftwareSafeState } from "./software_safe_state.js"; import { createWorldFeedTransport } from "./world_feed_transport.js"; import { isWorldScopedCrisisRuntimeEvent } from "./world_event_attention_policy.js";
import {
  buildAuthEnvelope,
  buildPromptControlSigningPayload,
  generateEphemeralEd25519Keypair,
  signAuthPayload,
} from "./viewer_auth_crypto.js";
import { createViewerSemanticTimeoutModule } from "./viewer_semantic_timeout_module.js";
import { createViewerSnapshotIngestionModule } from "./viewer_snapshot_ingestion_module.js";
import { createViewerSelectionProjectionModule } from "./viewer_selection_projection_module.js";
import { createViewerSessionRegisterModule } from "./viewer_session_register_module.js";
export const state = createSoftwareSafeState();
let socket = null;
let reconnectTimer = null;
let helloAckTimer = null;
let initialSnapshotRequested = false;
let currentConnectionSnapshotReceived = false;
let initialSnapshotReadFailed = false;
let initialSnapshotRetryTimer = null;
let initialSnapshotRetryCount = 0;
let hostedSessionRefreshTimer = null;
let requestId = 0;
let authNonceCounter = 0;
let localTestPlayerSessionInFlight = null;
let viewerPromptControlModule = null;
let viewerControlLossModule = null;
let semanticSendLoop = null;
const pendingControlFeedback = new Map();
const liveControlState = createLiveControlStateController(state, pendingControlFeedback);
const pendingSemanticCommands = [];
let pendingSessionRegisterWaiter = null;
const application = createViewerApplication({
  coordinateState: () => coordinateDependentQuotes(state),
  bootstrap,
  onBootstrapError(error) {
    state.connectionStatus = "error";
    state.auth.error = String(error);
    state.lastError = String(error);
  },
});
const worldFeedTransport = createWorldFeedTransport({ getSocket: () => socket, getState: () => state, render, requestSnapshot: () => requestSnapshotSafe(), sendJson });
export const requestWorldFeed = (...args) => worldFeedTransport.requestWorldFeed(...args); export const reloadWorldFeedFromAuthoritativeSnapshot = (...args) => worldFeedTransport.reloadWorldFeedFromAuthoritativeSnapshot(...args);
const HELLO_ACK_TIMEOUT_MS = 2000; const INITIAL_SNAPSHOT_RETRY_DELAY_MS = 1000; const INITIAL_SNAPSHOT_SLOW_RETRY_AFTER = 5;
const INITIAL_SNAPSHOT_SLOW_RETRY_DELAY_MS = 5000; const EMPTY_ENTITY_SNAPSHOT_REFRESH_DELAY_MS = 2500;
const SESSION_REGISTER_ACK_TIMEOUT_MS = 15000;
const AGENT_CHAT_ACK_TIMEOUT_MS = 30000;
const SEMANTIC_ACTION_ACK_TIMEOUT_MS = 30000;
const SEMANTIC_ACTION_OVERALL_TIMEOUT_MS = 45000;
const AGENT_CHAT_OVERALL_TIMEOUT_MS = resolveAgentChatOverallTimeoutMs();
const LOCAL_TEST_PLAYER_SESSION_STORAGE_PREFIX = "oasis7.viewer.localTestPlayerSession.v1";
const CHAT_HISTORY_STORAGE_PREFIX = "oasis7.viewer.chatHistory.v1";
const CHAT_HISTORY_LIMIT = 40;
const STARTER_AGENT_ID = "starter-agent-0";
const LOCAL_TEST_PLAYER_ID_PREFIX = "local-test-player-";
const PROMPT_CONTROL_RESULT_CAPABILITY = "prompt_control_result_v1";
function normalizeUiLocale(raw) {
  const value = String(raw || "").trim().toLowerCase();
  if (["zh", "zh-cn", "zh_cn", "cn", "chinese"].includes(value)) {
    return "zh";
  }
  if (["en", "en-us", "en_us", "english"].includes(value)) {
    return "en";
  }
  return null;
}
export function isLocaleZh(locale = state.uiLocale) {
  return normalizeUiLocale(locale) === "zh";
}
function localeText(locale, zh, en) {
  return isLocaleZh(locale) ? zh : en;
}
const {
  applyUiLocaleToDocument,
  resolveInitialUiLocale,
  resolveStoredPromptOverridesVisibility,
  setPromptOverridesVisible,
  setViewerLocale,
  togglePromptOverridesVisible,
  toggleViewerLocale,
} = createViewerLocalePreferencesModule({
  documentRef: document,
  getSearchParams,
  normalizeUiLocale,
  promptOverridesVisibilityStoragePrefix: PROMPT_OVERRIDES_VISIBILITY_STORAGE_PREFIX,
  renderViewer: render,
  state,
  uiLocaleStoragePrefix: UI_LOCALE_STORAGE_PREFIX,
  windowRef: window,
});
export { setViewerLocale, toggleViewerLocale, setPromptOverridesVisible, togglePromptOverridesVisible };
export const setSoftwareSafeLocale = setViewerLocale;
export function getSelectedSearch() {
  return state.selectedSearch;
}
export function setSelectedSearch(value) {
  state.selectedSearch = String(value || "");
  render();
}
export const setRenderHook = (nextHook) => application.setView(nextHook);
export const subscribeRenderHook = (nextHook) => application.attachView(nextHook);
function getSearchParams() {
  return new URLSearchParams(window.location.search || "");
}
function isTestApiEnabled() {
  if (__OASIS7_VISUAL_TEST__ !== true) return false;
  const value = String(getSearchParams().get("test_api") || "").trim().toLowerCase();
  return value === "1" || value === "true" || value === "yes" || value === "on";
}
function resolveAgentChatOverallTimeoutMs() {
  if (!isTestApiEnabled()) {
    return 45000;
  }
  const value = Number(getSearchParams().get("agent_chat_overall_timeout_ms"));
  if (!Number.isFinite(value) || value < 1) {
    return 45000;
  }
  return Math.min(value, 45000);
}
function clone(value) {
  return value == null ? value : JSON.parse(JSON.stringify(value));
}
const {
  clearPendingSessionRegisterWaiter, acceptRuntimeAckIdentity,
  recoverConnectedSessionStateAfterRuntimeAck, resolvePendingSessionRegisterWaiterAfterRuntimeAck,
  expirePendingSessionRegisterWaiterForTest, dispatchSessionRegisterRequest,
  retryPendingSessionRegisterWaiterWithForceRebind, rebindAgentIdFromRecoveryError,
  isHostedAuthExpired, latestRequestedAgentId, recoveryErrorRequiresExplicitRebind,
  ensureRegisteredPlayerSession, registerPlayerSessionForTest,
} = createViewerSessionRegisterModule({
  state, LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE, SESSION_REGISTER_ACK_TIMEOUT_MS,
  getWaiter: () => pendingSessionRegisterWaiter,
  setWaiter: (w) => { pendingSessionRegisterWaiter = w; },
  getSocket: () => socket,
  setSocket: (s) => { socket = s; },
  validateRuntimeAckIdentity, invalidateAuthConnection,
  cancelHostedLeaseRefresh: () => cancelHostedLeaseRefresh(),
  hasSigningIdentity, updateRegistrationGrant, stopHostedSessionRefreshLoop,
  clearHostedRuntimeSyncTimer: () => clearHostedRuntimeSyncTimer(),
  isSessionContextCurrent, captureSessionContext, authConnectionGeneration, isTestApiEnabled,
  ensureHostedAuthSigningKey, authCredentials,
  hostedWorldAccount: { renew: () => hostedWorldAccount.renew() },
  refreshHostedPlayerLease: () => refreshHostedPlayerLease(),
  buildSessionRegisterAuthProof, sendJson,
  waitForHostedTestLogin: () => waitForHostedTestLogin(),
  ensureHostedPlayerAuthAvailable,
  markCurrentGameplayActionFeedbackError, render,
});
const {
  handleRefineQuotePreflight,
  handleRefineQuoteError,
  injectRefineQuotePreflightForTest,
  installRefineQuotePreflightVisualFixture,
} = createRefineQuotePreflightStateModule({
  clone,
  getSearchParams,
  isTestApiEnabled,
  render,
  state,
});
const productValidationQuote = createProductValidationQuoteIntegration(() => ({ buildAuthEnvelope, clone, ensureHostedPlayerAuthAvailable, ensureRegisteredPlayerSession, getSearchParams, getSocket: () => socket, isTestApiEnabled, nextAuthNonce, render, sendJson, signAuthPayload, state }));
const { injectProductValidationQuoteForTest, requestProductValidationQuote } = productValidationQuote;
const powerSurvivalQuote = createPowerSurvivalQuoteIntegration(() => ({ buildAuthEnvelope, clone, ensureHostedPlayerAuthAvailable, ensureRegisteredPlayerSession, getSearchParams, getSocket: () => socket, isTestApiEnabled, nextAuthNonce, render, sendJson, signAuthPayload, state }));
const { injectPowerSurvivalQuoteForTest, requestPowerSurvivalQuote } = powerSurvivalQuote; const scheduleRecipeQuote = createScheduleRecipeQuoteIntegration(() => ({ buildAuthEnvelope, clone, ensureHostedPlayerAuthAvailable, ensureRegisteredPlayerSession, getSearchParams, getSocket: () => socket, isTestApiEnabled, nextAuthNonce, render, sendJson, signAuthPayload, state })); const { injectScheduleRecipeQuoteForTest, requestScheduleRecipeQuote } = scheduleRecipeQuote; const transferMaterialQuote = createTransferMaterialQuoteIntegration(() => ({ buildAuthEnvelope, clone, ensureHostedPlayerAuthAvailable, ensureRegisteredPlayerSession, getSearchParams, getSocket: () => socket, isTestApiEnabled, nextAuthNonce, render, sendJson, signAuthPayload, state })); const { injectTransferMaterialQuoteForTest, requestTransferMaterialQuote } = transferMaterialQuote;
const powerSaleQuote = createPowerSaleQuoteIntegration(() => ({ buildAuthEnvelope, clone, ensureHostedPlayerAuthAvailable, ensureRegisteredPlayerSession, getSocket: () => socket, isTestApiEnabled, nextAuthNonce, render, sendJson, signAuthPayload, state }));
const { injectPowerSaleQuoteForTest, requestPowerSaleQuote } = powerSaleQuote;
const fragmentRefillPreview = createFragmentRefillPreviewIntegration(() => ({ buildAuthEnvelope, clone, ensureHostedPlayerAuthAvailable, ensureRegisteredPlayerSession, getSocket: () => socket, nextAuthNonce, sendJson, signAuthPayload, state }));
const { requestFragmentRefillPreview } = fragmentRefillPreview;
const governanceVoteQuote = createGovernanceVoteQuoteIntegration({ buildAuthEnvelope, clone, ensureHostedPlayerAuthAvailable, ensureRegisteredPlayerSession, getSocket: () => socket, nextAuthNonce, sendJson, signAuthPayload, state }); const { requestGovernanceVoteQuote } = governanceVoteQuote;
const warDeclarationQuote = createWarDeclarationQuoteIntegration({ buildAuthEnvelope, clone, ensureHostedPlayerAuthAvailable, ensureRegisteredPlayerSession, getSocket: () => socket, nextAuthNonce, sendJson, signAuthPayload, state }); const { injectWarDeclarationQuoteForTest, requestWarDeclarationQuote } = warDeclarationQuote;
const marketQuoteDecision = createMarketQuoteDecisionIntegration({ buildAuthEnvelope, clone, ensureHostedPlayerAuthAvailable, ensureRegisteredPlayerSession, getSocket: () => socket, nextAuthNonce, sendJson, signAuthPayload, state });
const { injectMarketQuoteDecisionForTest, requestMarketQuoteDecision } = marketQuoteDecision;
const quoteProtocolFacade = createViewerQuoteProtocolFacade({ fragmentRefillPreview, governanceVoteQuote, handleRefineQuoteError, handleRefineQuotePreflight, marketQuoteDecision, powerSaleQuote, powerSurvivalQuote, productValidationQuote, scheduleRecipeQuote, state, transferMaterialQuote, warDeclarationQuote });
function normalizeFiniteNumber(value) {
  if (value == null) {
    return null;
  }
  const numeric = Number(value);
  return Number.isFinite(numeric) ? numeric : null;
}
function finitePositionComponents(pos) {
  if (!pos || typeof pos !== "object") {
    return null;
  }
  const x = normalizeFiniteNumber(pos.x_cm);
  const y = normalizeFiniteNumber(pos.y_cm);
  const z = normalizeFiniteNumber(pos.z_cm);
  if (x == null || y == null || z == null) {
    return null;
  }
  return { x, y, z };
}
function trimFixed(value, digits) {
  if (!Number.isFinite(value)) {
    return null;
  }
  const fixed = value.toFixed(digits);
  return fixed.replace(/\.0+$/, "").replace(/(\.\d*[1-9])0+$/, "$1");
}
const {
  formatPhysicalDistanceCm,
  formatWorldPositionCm,
  buildWorldScaleSurface,
  detectRendererMeta,
} = createViewerWorldScaleModule({
  documentRef: document,
  state,
  isLocaleZh,
  normalizeFiniteNumber,
  finitePositionComponents,
  trimFixed,
  getSearchParams,
  softwareRendererMarkers: SOFTWARE_RENDERER_MARKERS,
  softwareSafeRenderModeAlias: SOFTWARE_SAFE_RENDER_MODE_ALIAS,
  viewerRenderMode: VIEWER_RENDER_MODE,
});
const {
  authDeploymentHint,
  buildAuthSurfaceModel,
  buildHostedActionMatrixView,
  buildHostedRecoveryHint,
  buildSemanticCapability,
  hostedActionPolicy,
  resolveHostedAccessHint,
} = createViewerAuthSurfaceModule({
  getSearchParams,
  localeText,
  state,
  windowRef: window,
});
const {
  parseSelectionPayload, entityCollections, normalizeModuleVisualEntity,
  moduleVisualEntityIdFromEvent, moduleVisualEntityFromEvent, focusModuleFromEvent,
  agentBindingForId, isAgentVisibleToCurrentSession, currentBoundAgentControlError,
  selectedAgentId, selectedAgentPromptProfile, selectedAgentBindingInfo,
  selectedAgentExecutionDebugContext, syncAgentInteractionDrafts,
  applySelection, select, focus, parseStepCount,
} = createViewerSelectionProjectionModule({
  state,
  STARTER_AGENT_ID,
  LOCAL_TEST_PLAYER_ID_PREFIX,
  isTestApiEnabled,
  isAgentControlLost: (id) => Boolean(viewerControlLossModule?.isControlLost(id)),
  render,
});
const {
  buildGameplaySummary,
  describePromptVersionState,
  describeSemanticFeedback,
  snapshotControlFeedback,
  snapshotSemanticFeedback,
} = createViewerFeedbackModule({
  clone,
  feedbackBadgeClass,
  hostedActionPolicy,
  isAgentVisibleToCurrentSession,
  isLocaleZh,
  localeText,
  state,
});
function initialWsUrl() {
  return resolveViewerEndpoint(viewerRuntimeConfig(), getSearchParams());
}
const {
  chatHistoryStorageKey,
  hydrateChatHistoryFromStorage,
  normalizeChatHistoryEntry,
  persistChatHistory,
  persistLocalTestPlayerSession,
  resolveStoredLocalTestPlayerSession,
  setChatHistory,
} = createViewerBrowserPersistenceModule({
  chatHistoryLimit: CHAT_HISTORY_LIMIT,
  chatHistoryStoragePrefix: CHAT_HISTORY_STORAGE_PREFIX,
  clone,
  initialWsUrl,
  localTestPlayerIdPrefix: LOCAL_TEST_PLAYER_ID_PREFIX,
  localTestPlayerSessionStoragePrefix: LOCAL_TEST_PLAYER_SESSION_STORAGE_PREFIX,
  state,
  windowRef: window,
});
function shouldConnectViewerWs() {
  const mode = String(getSearchParams().get("connect") || "").trim().toLowerCase();
  return mode !== "0" && mode !== "false" && mode !== "off";
}
function shouldRunHostedBootstrap() {
  const mode = String(getSearchParams().get("hosted_bootstrap") || "").trim().toLowerCase();
  return mode !== "0" && mode !== "false" && mode !== "off";
}
const {
  authHasSigningKeyMaterial,
  clearHostedPlayerSession,
  persistHostedPlayerSession,
  resolveAuthBootstrap,
  resolveViewerAuthState,
} = createViewerHostedAuthStateModule({
  hostedPlayerSessionStoragePrefix: HOSTED_PLAYER_SESSION_STORAGE_PREFIX,
  initialWsUrl,
  viewerAuthBootstrapObject: VIEWER_AUTH_BOOTSTRAP_OBJECT,
  viewerAuthPrivateKey: VIEWER_AUTH_PRIVATE_KEY,
  viewerAuthPublicKey: VIEWER_AUTH_PUBLIC_KEY,
  viewerPlayerIdKey: VIEWER_PLAYER_ID_KEY,
  windowRef: window,
});
function resetHostedLoginChallenge() { resetHostedLoginChallengeState(state.hostedLogin); }
const { start: issueHostedTestLogin, waitForStart: waitForHostedTestLogin } = createViewerHostedTestLoginModule({ clone, fetchImpl: (...args) => fetch(...args), generateEphemeralEd25519Keypair, getSearchParams, isHostedPublicJoinDeploymentMode, persistHostedPlayerSession, render, resetHostedLoginChallenge, route: HOSTED_ACCOUNT_TEST_LOGIN_ROUTE, state });
const issueHostedAccountLogin = createViewerHostedAccountLoginIssuer({ state, canAutoIssueHostedPlayerSession, generateEphemeralEd25519Keypair, installSession, persistHostedPlayerSession, resetHostedLoginChallenge, render, clone, fetch: (...args) => fetch(...args), completeRoute: HOSTED_ACCOUNT_LOGIN_COMPLETE_ROUTE });
const hostedLoginRegistrationBridge = createViewerHostedLoginRegistrationBridge({ registerPlayerSession: ensureRegisteredPlayerSession, render, state });
const startHostedTestLogin = hostedLoginRegistrationBridge.wrapLogin(issueHostedTestLogin, "hosted_test_login");
const completeHostedAccountLogin = hostedLoginRegistrationBridge.wrapLogin(issueHostedAccountLogin, "hosted_browser_storage");
const hostedWorldAccount = createHostedWorldAccount({ state, generateKeypair: generateEphemeralEd25519Keypair, register: ensureRegisteredPlayerSession, render, clearLegacy: clearHostedPlayerSession, getGameplayScope: () => getGameplayRecoveryScope(state) });
export const joinHostedAccountWorld = (...args) => hostedWorldAccount.join(...args);
export const disableLegacyHostedSession = () => hostedWorldAccount.disableLegacy();
export const invalidateHostedAccountSession = () => hostedWorldAccount.invalidate();
async function ensureHostedAuthSigningKey(auth = state.auth) {
  if (auth?.source === "visual_fixture_projection") throw new Error("visual fixture authentication has no signing capability");
  if (!auth?.available || auth.source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE) {
    return auth;
  }
  if (hasSigningIdentity(auth)) return auth;
  const keypair = await generateEphemeralEd25519Keypair();
  auth = await installSession(state, { ...auth, ...authCredentials(auth), ...keypair }, auth);
  auth.registrationStatus = "issued";
  auth.sessionEpoch = auth.bindingEpoch = auth.authorityEpoch = auth.boundAgentId = null;
  viewerPromptControlModule?.clearPendingAuthoritativeRefresh();
  auth.runtimeStatus = "recovery_pending_key";
  auth.syncInFlight = false;
  auth.recoveryErrorCode = null;
  auth.recoveryErrorMessage = null;
  persistHostedPlayerSession(auth);
  return auth;
}
async function refreshHostedAdmissionState() {
  if (!isHostedPublicJoinDeploymentMode(state.hostedAccess?.deployment_mode)) {
    state.hostedAdmission = null;
    return null;
  }
  try {
    const response = await fetch(HOSTED_PLAYER_SESSION_ADMISSION_ROUTE, {
      method: "GET",
      cache: "no-store",
      headers: { Accept: "application/json" },
    });
    const payload = await response.json();
    state.hostedAdmission = payload?.admission ? clone(payload.admission) : null;
    return state.hostedAdmission;
  } catch (_) {
    return state.hostedAdmission;
  }
}
const { refreshHostedPlayerLease, cancelRefresh: cancelHostedLeaseRefresh } = createViewerHostedSessionRefreshModule({
  captureConnection: () => socket,
  clone,
  ensureHostedAuthSigningKey,
  fetchImpl: fetch,
  legacyViewerAuthBootstrapSource: LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE,
  persistHostedPlayerSession,
  refreshRoute: HOSTED_PLAYER_SESSION_REFRESH_ROUTE,
  state,
});
function stopHostedSessionRefreshLoop() {
  if (hostedSessionRefreshTimer) {
    window.clearInterval(hostedSessionRefreshTimer);
    hostedSessionRefreshTimer = null;
  }
}

function syncHostedSessionRefreshLoop() {
  const shouldRun = state.connectionStatus === "connected"
    && state.auth.available
    && state.auth.source !== LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE
    && state.auth.registrationStatus === "registered"
    && !!authCredentials(state.auth).releaseToken;
  if (!shouldRun) {
    stopHostedSessionRefreshLoop();
    return;
  }
  if (hostedSessionRefreshTimer) {
    return;
  }
  hostedSessionRefreshTimer = window.setInterval(() => {
    probeHostedRuntimeSession();
    void refreshHostedPlayerLease().then(() => render());
  }, HOSTED_PLAYER_SESSION_REFRESH_INTERVAL_MS);
}

function nextRequestId() {
  requestId += 1;
  return requestId;
}

function nextAuthNonce() {
  authNonceCounter += 1;
  return Date.now() + authNonceCounter;
}

const canonicalOwnerRead = createCanonicalOwnerReadModule({ state, sendJson, signAuthPayload, render, getSocket: () => socket });
const safeAlternative = createSafeAlternativeModule({ state, sendJson, render, refreshOwner: () => canonicalOwnerRead.refresh(undefined, true) });
const viewerAgentChatAuthModule = createViewerAgentChatAuthModule({ buildAuthEnvelope, nextAuthNonce, signAuthPayload, state });

function resetViewerProtocolForConnection() { canonicalOwnerRead.reset(); viewerPromptControlModule?.resetForConnection(); liveControlState.reset(); }
function promptControlCapabilitySelected() { return viewerPromptControlModule?.capabilitySelected() === true; }
function promptControlReadinessError(agentId) { return viewerPromptControlModule?.readinessError(agentId) || null; }
function attachPromptControlIdentity(request) { return viewerPromptControlModule?.attachIdentity(request) || request; }

function getState() {
  const authSurface = buildAuthSurfaceModel();
  const hostedActionMatrixView = buildHostedActionMatrixView();
  const hostedRecoveryHint = buildHostedRecoveryHint();
  const gameplaySummary = buildGameplaySummary();
  return {
    connectionStatus: state.connectionStatus,
    viewerProtocol: clone(state.viewerProtocol),
    logicalTime: state.logicalTime,
    eventSeq: state.eventSeq,
    tick: state.tick,
    selectedKind: state.selectedKind,
    selectedId: state.selectedId,
    errorCount: state.errorCount,
    lastError: state.lastError,
    eventCount: state.eventCount,
    traceCount: state.traceCount,
    cameraMode: state.cameraMode,
    cameraRadius: state.cameraRadius,
    cameraOrthoScale: state.cameraOrthoScale,
    lastControlFeedback: snapshotControlFeedback(state.lastControlFeedback),
    lastPromptFeedback: snapshotSemanticFeedback(state.lastPromptFeedback),
    lastChatFeedback: snapshotSemanticFeedback(state.lastChatFeedback),
    lastGameplayActionFeedback: snapshotSemanticFeedback(state.lastGameplayActionFeedback),
    renderMode: state.renderMode,
    rendererClass: state.rendererClass,
    viewerReason: state.viewerReason,
    softwareSafeReason: state.viewerReason,
    renderer: state.renderer,
    vendor: state.vendor,
    webglVersion: state.webglVersion,
    pixelWorldRuntimeStatus: state.pixelWorldRuntimeStatus,
    pixelWorldRuntimeSource: state.pixelWorldRuntimeSource,
    pixelWorldRuntimeModuleUrl: state.pixelWorldRuntimeModuleUrl,
    pixelWorldCamera: clone(state.pixelWorldCamera),
    pixelWorldFatal: clone(state.pixelWorldFatal),
    uiLocale: state.uiLocale,
    promptOverridesVisible: state.promptOverridesVisible,
    controlProfile: state.controlProfile,
    worldId: state.worldId,
    server: state.server,
    wsUrl: state.wsUrl,
    authReady: state.auth.available,
    authPlayerId: state.auth.playerId,
    authPublicKey: state.auth.publicKey,
    authError: state.auth.error,
    authRevokeReason: state.auth.revokeReason,
    authRevokedBy: state.auth.revokedBy,
    authRegistrationStatus: state.auth.registrationStatus,
    authSessionEpoch: state.auth.sessionEpoch, authBindingEpoch: state.auth.bindingEpoch, authAuthorityEpoch: state.auth.authorityEpoch,
    authRecoveryErrorCode: state.auth.recoveryErrorCode,
    authRecoveryErrorMessage: state.auth.recoveryErrorMessage,
    authRuntimeStatus: state.auth.runtimeStatus,
    authBoundAgentId: state.auth.boundAgentId,
    authControlLostAgentId: state.auth.controlLostAgentId,
    authPendingRequestedAgentId: state.auth.pendingRequestedAgentId,
    authPendingForceRebind: state.auth.pendingForceRebind,
    authRebindNotice: state.auth.rebindNotice,
    authTier: authSurface.currentTier,
    authSource: authSurface.source,
    authDeploymentHint: authSurface.deploymentHint,
    authSurface: clone(authSurface),
    hostedRecoveryHint: clone(hostedRecoveryHint),
    hostedAccess: clone(state.hostedAccess),
    hostedActionMatrix: clone(hostedActionMatrixView),
    hostedAdmission: clone(state.hostedAdmission),
    gameplaySummary: clone(gameplaySummary),
    lastDecisionTrace: snapshotDecisionTrace(state.recentDecisionTraces[0] || null),
    recentDecisionTracesCount: state.recentDecisionTraces.length,
    recentDecisionTraces: state.recentDecisionTraces
      .slice(0, 4)
      .map((trace) => snapshotDecisionTrace(trace)),
    strongAuthApprovalCodeConfigured: !!String(state.strongAuth.approvalCode || "").trim(),
    strongAuthLastGrantActionId: state.strongAuth.lastGrantActionId,
    strongAuthLastGrantExpiresAtUnixMs: state.strongAuth.lastGrantExpiresAtUnixMs,
    strongAuthLastGrantError: state.strongAuth.lastGrantError,
    selectedAgentDebug: clone(selectedAgentExecutionDebugContext()),
    selectedPromptVersion: state.promptDraft.currentVersion || 0,
    promptRollbackTargetVersion: state.promptDraft.rollbackTargetVersion || 0,
    chatHistoryCount: state.chatHistory.length,
    chatHistory: clone(state.chatHistory),
  };
}

function reportFatalError(message, source = "runtime") {
  const text = `${source}: ${String(message || "unknown runtime error")}`.trim();
  if (state.lastError !== text) {
    state.errorCount += 1;
  }
  state.connectionStatus = "error";
  state.lastError = text;
  render();
}

function promptDraftValuesForRequest(request) {
  return viewerPromptControlModule?.draftValuesForRequest(request) || null;
}

function reconcilePendingPromptAuthoritativeRefresh(snapshot) {
  return viewerPromptControlModule?.reconcilePendingAuthoritativeRefresh(snapshot) === true;
}

function controlActions() {
  return [
    {
      action: "play",
      description: "Start continuous world advancement",
      descriptionZh: "开始连续推进世界",
      examplePayload: null,
    },
    {
      action: "pause",
      description: "Pause continuous advancement",
      descriptionZh: "暂停连续推进",
      examplePayload: null,
    },
    {
      action: "step",
      description: "Advance fixed steps (payload.count)",
      descriptionZh: "推进固定步数（payload.count）",
      examplePayload: { count: 5 },
    },
  ];
}

function describeControls() {
  return {
    controls: controlActions(),
    semanticActions: [
      {
        action: "sendAgentChat",
        description: "Send a player-authenticated chat message to an agent",
      },
      {
        action: "sendPromptControl",
        description: "Preview, apply, or rollback prompt overrides for an agent",
      },
    ],
    usage: "Use fillControlExample(action), sendControl(action), sendGameplayAction(actionIdOrPayload), sendAgentChat(agentId, message), sendPromptControl(mode, payload).",
    notes: [
      "viewer consumes runtime snapshots/events without becoming a separate execution lane",
      "selectedAgentDebug reports the current provider-backed lane metadata when available",
      "without viewer auth bootstrap the browser stays guest_session only; hosted public join player-session issuance is still pending",
    ],
  };
}

function fillControlExample(action) {
  const normalized = String(action || "").trim().toLowerCase();
  return controlActions().find((entry) => entry.action === normalized)?.examplePayload ?? null;
}

function sendJson(payload) {
  if (!socket || socket.readyState !== WebSocket.OPEN) {
    throw new Error("viewer websocket is not connected");
  }
  socket.send(JSON.stringify(payload));
}

function clearInitialSnapshotRetryTimer() {
  if (initialSnapshotRetryTimer) {
    window.clearTimeout(initialSnapshotRetryTimer);
    initialSnapshotRetryTimer = null;
  }
}

function clearHelloAckTimer() {
  if (helloAckTimer) {
    window.clearTimeout(helloAckTimer);
    helloAckTimer = null;
  }
}

const {
  clearPendingAgentChatAckTimer,
  setPendingAgentChatAckTimer,
  clearPendingAgentChatOverallTimer,
  clearPendingPromptControlAckTimer,
  clearPendingGameplayActionAckTimer,
  clearHostedRuntimeSyncTimer,
  expireHostedRuntimeSyncTimeout,
  scheduleHostedRuntimeSyncTimeout,
  expireHostedRuntimeSyncTimeoutForTest,
  agentChatFeedbackInFlight,
  semanticFeedbackInFlight,
  sameAgentChatFeedback,
  sameSemanticFeedback,
  isAgentChatInFlight,
  markAgentChatFeedbackError,
  expireAgentChatOverallTimeout,
  scheduleAgentChatOverallTimeout,
  expirePendingAgentChatOverallTimeoutForTest,
  scheduleAgentChatAckTimeout,
  failPendingAgentChatAck,
  markSemanticFeedbackError,
  expirePromptControlAckTimeout,
  expireGameplayActionAckTimeout,
  schedulePromptControlAckTimeout,
  scheduleGameplayActionAckTimeout,
  expirePendingPromptControlAckTimeoutForTest,
  connectionGenerationForTest,
  expirePendingGameplayActionAckTimeoutForTest,
  failPendingPromptControlAck,
  failPendingGameplayActionAck,
} = createViewerSemanticTimeoutModule({
  state,
  SESSION_REGISTER_ACK_TIMEOUT_MS,
  AGENT_CHAT_OVERALL_TIMEOUT_MS,
  AGENT_CHAT_ACK_TIMEOUT_MS,
  SEMANTIC_ACTION_ACK_TIMEOUT_MS,
  hasPendingSessionRegisterWaiter: () => Boolean(pendingSessionRegisterWaiter),
  maybeRecoverLocalTestStarterBindingFromSnapshot: (snapshot) => maybeRecoverLocalTestStarterBindingFromSnapshot(snapshot),
  isTestApiEnabled,
  authConnectionGeneration,
  render,
});

function semanticCommandTimeoutError(command) {
  if (!Number.isFinite(command?.timeoutMs) || command.timeoutMs <= 0) {
    return null;
  }
  return new Promise((resolve) => {
    window.setTimeout(() => {
      resolve(new Error(`${command.kind || "semantic"} command timed out before send completed`));
    }, command.timeoutMs);
  });
}

async function executeSemanticCommand(command) {
  let executePromise;
  try {
    executePromise = Promise.resolve(command.execute());
  } catch (error) {
    executePromise = Promise.reject(error);
  }
  const timeoutPromise = semanticCommandTimeoutError(command);
  if (!timeoutPromise) {
    await executePromise;
    return;
  }
  const result = await Promise.race([
    executePromise.then(() => null),
    timeoutPromise,
  ]);
  if (result instanceof Error) {
    executePromise.catch(() => {});
    if (command.kind === "chat") {
      expireAgentChatOverallTimeout(command.feedback);
    } else if (command.kind === "prompt") {
      if (sameSemanticFeedback(state.lastPromptFeedback, command.feedback)) {
        markSemanticFeedbackError(
          command.feedback,
          "prompt_control timed out before live server ack/error completed",
          "prompt_control overall timeout",
        );
        state.lastPromptFeedback = command.feedback;
        render();
      }
    }
    return;
  }
}

function closeSocketForReconnect(targetSocket) {
  if (!targetSocket || targetSocket.readyState === WebSocket.CLOSING || targetSocket.readyState === WebSocket.CLOSED) {
    return;
  }
  try {
    targetSocket.close();
  } catch (_) {
  }
}

function scheduleHelloAckTimeout(targetSocket) {
  clearHelloAckTimer();
  helloAckTimer = window.setTimeout(() => {
    helloAckTimer = null;
    if (socket !== targetSocket || state.server || targetSocket.readyState !== WebSocket.OPEN) {
      return;
    }
    closeSocketForReconnect(targetSocket);
  }, HELLO_ACK_TIMEOUT_MS);
}

function sendInitialSnapshotRequest() {
  sendJson({ type: "subscribe", streams: ["snapshot", "events", "metrics"], event_kinds: [] }); worldFeedTransport.requestWorldFeed({ cursor: null });
  sendJson({ type: "request_snapshot" });
}

function scheduleInitialSnapshotRetry() {
  clearInitialSnapshotRetryTimer();
  if (currentConnectionSnapshotReceived) {
    return;
  }
  if (initialSnapshotRetryCount >= 8) {
    initialSnapshotReadFailed = true;
    state.lastError = "World snapshot unavailable. Refresh the snapshot to retry.";
    render(); return;
  }
  const retryDelay =
    initialSnapshotRetryCount >= INITIAL_SNAPSHOT_SLOW_RETRY_AFTER
      ? INITIAL_SNAPSHOT_SLOW_RETRY_DELAY_MS
      : INITIAL_SNAPSHOT_RETRY_DELAY_MS;
  initialSnapshotRetryTimer = window.setTimeout(() => {
    initialSnapshotRetryTimer = null;
    if (currentConnectionSnapshotReceived || !socket || socket.readyState !== WebSocket.OPEN) {
      return;
    }
    initialSnapshotRetryCount += 1;
    try {
      sendInitialSnapshotRequest();
    } catch (_) {
    } finally { scheduleInitialSnapshotRetry(); }
  }, retryDelay);
}

function gameplayActionByProtocolAction(protocolAction) {
  return normalizedGameplayActions().find((action) => gameplayProtocolAction(action) === protocolAction) || null;
}

function viewerControlGate(normalizedAction) {
  const protocolAction =
    state.controlProfile === "live"
      ? normalizedAction === "play"
        ? "live_control.play"
        : normalizedAction === "step"
          ? "live_control.step"
          : null
      : null;
  if (!protocolAction) {
    return null;
  }
  const gameplayAction = gameplayActionByProtocolAction(protocolAction);
  const disabledReason = String(gameplayAction?.disabled_reason || gameplayAction?.disabledReason || "").trim();
  if (!disabledReason) {
    return null;
  }
  return {
    reason: disabledReason,
    effect: `control blocked by gameplay gate: ${disabledReason}`,
    hint: state.snapshot?.player_gameplay?.next_step_hint || null,
  };
}

function sendViewerControl(action, payload) {
  const normalized = String(action || "").trim().toLowerCase();
  const currentRequestId = nextRequestId();
  const feedback = {
    id: currentRequestId,
    action: normalized,
    accepted: false,
    stage: "rejected",
    reason: null,
    hint: null,
    effect: null,
    baselineLogicalTime: state.logicalTime,
    baselineEventSeq: state.eventSeq,
    deltaLogicalTime: 0,
    deltaEventSeq: 0,
    deltaTraceCount: 0,
    requestId: currentRequestId,
  };

  let mode = null;
  if (normalized === "play") {
    mode = { mode: "play" };
  } else if (normalized === "pause") {
    mode = { mode: "pause" };
  } else if (normalized === "step") {
    const count = parseStepCount(payload);
    if (!count) {
      feedback.reason = "step requires numeric payload.count >= 1";
      feedback.effect = "request rejected before send";
      state.lastControlFeedback = feedback;
      render();
      return snapshotControlFeedback(feedback);
    }
    mode = { mode: "step", count };
  } else {
    feedback.reason = `unsupported action: ${normalized}`;
    feedback.effect = "request rejected before send";
    state.lastControlFeedback = feedback;
    render();
    return snapshotControlFeedback(feedback);
  }

  const gate = viewerControlGate(normalized);
  if (gate) {
    feedback.stage = "blocked";
    feedback.reason = gate.reason;
    feedback.hint = gate.hint;
    feedback.effect = gate.effect;
    state.lastControlFeedback = feedback;
    render();
    return snapshotControlFeedback(feedback);
  }

  try {
    if (state.controlProfile === "live") {
      sendJson({ type: "live_control", mode, request_id: currentRequestId });
    } else if (state.controlProfile === "playback") {
      sendJson({ type: "playback_control", mode, request_id: currentRequestId });
    } else {
      sendJson({ type: "control", mode, request_id: currentRequestId });
    }
    feedback.accepted = true;
    feedback.stage = "queued";
    feedback.effect = "queued, check getState().lastControlFeedback for world delta";
    pendingControlFeedback.set(currentRequestId, feedback);
    liveControlState.sent(feedback);
    state.lastControlFeedback = feedback;
    render();
    return snapshotControlFeedback(feedback);
  } catch (error) {
    feedback.reason = String(error);
    feedback.effect = "request send failed";
    state.lastControlFeedback = feedback;
    render();
    return snapshotControlFeedback(feedback);
  }
}

function sendControl(action, payload = null) {
  return sendViewerControl(action, payload);
}

function runSteps(payload) {
  const count = parseStepCount(payload);
  if (!count) {
    return { ok: false, reason: "payload must be non-empty step string or count" };
  }
  const feedback = sendControl("step", { count });
  return { ok: Boolean(feedback?.accepted), count, feedback };
}

function setMode() {
  return {
    ok: false,
    reason: "viewer does not expose 2d/3d camera modes",
  };
}

const {
  summarizeEventTitle, addRecentEvent, handleSnapshot, normalizedGameplayActions, gameplayActionId,
  gameplayProtocolAction, hasGameplayAction, hasSnapshotRefreshAction, needsEmptyEntitySnapshotRefresh,
  clearEmptyEntitySnapshotRefreshTimer, needsEmptyEntitySnapshotRefreshForTest,
  isEmptyEntitySnapshotRefreshPendingForTest, syncEmptyEntitySnapshotRefreshLoop,
  adoptCurrentPlayerBindingFromSnapshot, maybeRecoverLocalTestStarterBindingFromSnapshot,
  injectSnapshot, handleMetrics, snapshotDecisionTrace, handleDecisionTrace, handleControlCompletionAck,
} = createViewerSnapshotIngestionModule({
  state, MAX_EVENTS, MAX_DECISION_TRACES, EMPTY_ENTITY_SNAPSHOT_REFRESH_DELAY_MS,
  LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE, STARTER_AGENT_ID, LOCAL_TEST_PLAYER_ID_PREFIX,
  isWorldScopedCrisisRuntimeEvent,
  onSnapshotReceived() {
    if (initialSnapshotReadFailed) state.lastError = null;
    initialSnapshotReadFailed = false;
    currentConnectionSnapshotReceived = true;
  },
  clearInitialSnapshotRetryTimer, quoteProtocolFacade, recoverGameplayFeedback, hostedWorldAccount,
  getGameplayRecoveryScope, clearPendingGameplayActionAckTimer, requestGameplayResult, sendJson,
  entityCollections, applySelection, syncAgentInteractionDrafts, hydrateChatHistoryFromStorage,
  reconcilePendingPromptAuthoritativeRefresh, canonicalOwnerRead, worldFeedTransport, isTestApiEnabled,
  getSocket: () => socket, requestSnapshotSafe: () => requestSnapshotSafe(), persistHostedPlayerSession,
  hasPendingSessionRegisterWaiter: () => Boolean(pendingSessionRegisterWaiter),
  ensureRegisteredPlayerSession: (...args) => ensureRegisteredPlayerSession(...args),
  clone, render, getState, acceptRuntimeAckIdentity: (ack) => acceptRuntimeAckIdentity(ack),
  pendingControlFeedback, liveControlState,
});

async function buildAgentChatAuthProof(request, auth) {
  return viewerAgentChatAuthModule.buildAuthProof(request, auth);
}

function promptPatchFromDraft(currentValue, draftValue) {
  const current = currentValue == null ? "" : String(currentValue);
  const draft = String(draftValue ?? "");
  if (draft === current) {
    return { mode: "unchanged" };
  }
  if (draft.length === 0) {
    return currentValue == null ? { mode: "unchanged" } : { mode: "clear" };
  }
  return { mode: "set", value: draft };
}

async function buildPromptControlAuthProof(mode, request, auth) {
  return viewerPromptControlModule?.buildAuthProof(mode, request, auth);
}

async function buildPromptRollbackAuthProof(request, auth) {
  return viewerPromptControlModule?.buildRollbackAuthProof(request, auth);
}

async function buildSessionRegisterAuthProof(request, auth) {
  const nonce = nextAuthNonce();
  const payload = {
    operation: "session_register",
    player_id: auth.playerId,
    public_key: auth.publicKey,
    nonce,
  };
  if (request.requested_agent_id != null) {
    payload.requested_agent_id = request.requested_agent_id;
  }
  payload.force_rebind = request.force_rebind === true;
  const signingPayload = buildAuthEnvelope(payload);
  return {
    scheme: "ed25519",
    player_id: auth.playerId,
    public_key: auth.publicKey,
    nonce,
    signature: await signAuthPayload(signingPayload, auth),
  };
}

async function buildGameplayActionAuthProof(request, auth) {
  const nonce = nextAuthNonce();
  const payload = {
    operation: "gameplay_action",
    action_id: request.action_id,
    target_agent_id: request.target_agent_id,
  };
  if (request.actor_agent_id != null) {
    payload.actor_agent_id = request.actor_agent_id;
  }
  payload.player_id = auth.playerId;
  payload.public_key = auth.publicKey;
  payload.nonce = nonce;
  const signingPayload = buildAuthEnvelope(payload);
  return {
    scheme: "ed25519",
    player_id: auth.playerId,
    public_key: auth.publicKey,
    nonce,
    signature: await signAuthPayload(signingPayload, auth),
  };
}

async function buildRefineQuoteAuthProof(request, auth) {
  const nonce = nextAuthNonce();
  const signingPayload = buildAuthEnvelope({
    operation: "gameplay_action",
    action_id: "quote_refine_compound",
    target_agent_id: `compound_mass_g:${request.compound_mass_g}`,
    player_id: auth.playerId,
    public_key: auth.publicKey,
    nonce,
  });
  return {
    scheme: "ed25519",
    player_id: auth.playerId,
    public_key: auth.publicKey,
    nonce,
    signature: await signAuthPayload(signingPayload, auth),
  };
}

let refineQuoteRequestGeneration = 0;
async function requestRefineQuote(compoundMassG) {
  const generation = ++refineQuoteRequestGeneration;
  const compoundMassGNumber = Number(compoundMassG);
  if (!Number.isSafeInteger(compoundMassGNumber) || compoundMassGNumber <= 0) {
    state.refineQuoteRequest = { status: "error", errorCode: "invalid_amount", error: "refine quote requires a positive whole-number compound mass in grams" }; return { ok: false, reason: "refine quote requires a positive whole-number compound mass in grams" };
  }
  if (!socket || socket.readyState !== WebSocket.OPEN) {
    state.refineQuoteRequest = { status: "error", errorCode: "connection_unavailable", error: "refine quote requires a connected viewer websocket" };
    return { ok: false, reason: "refine quote requires a connected viewer websocket" };
  }
  try {
    await ensureHostedPlayerAuthAvailable();
    if (!state.auth.available) {
      state.refineQuoteRequest = { status: "error", errorCode: "player_session_required", error: state.auth.error || "refine quote requires an active player session" };
      return { ok: false, reason: state.auth.error || "refine quote requires an active player session" };
    }
    const boundAgentId = String(state.auth.boundAgentId || "").trim();
    if (!boundAgentId) {
      state.refineQuoteRequest = { status: "error", errorCode: "player_agent_binding_required", error: "refine quote requires a bound player Agent" };
      return { ok: false, reason: "refine quote requires a bound player Agent" };
    }
    await ensureRegisteredPlayerSession(boundAgentId);
    const request = {
      compound_mass_g: compoundMassGNumber,
      player_id: state.auth.playerId,
      public_key: state.auth.publicKey,
    };
    const dependencyFingerprint = quoteDependencyFingerprint(state, "refine");
    const requestSocket = socket;
    request.auth = await buildRefineQuoteAuthProof(request, state.auth);
    if (generation !== refineQuoteRequestGeneration || socket !== requestSocket
      || requestSocket?.readyState !== WebSocket.OPEN
      || dependencyFingerprint !== quoteDependencyFingerprint(state, "refine")) {
      return { ok: false, reason: "refine quote inputs changed during signing" };
    }
    state.refineQuotePreflight = null;
    state.refineQuoteRequest = { status: "pending", error: null, requestAuthNonce: request.auth.nonce, dependencyFingerprint, dependencyInputs: {} };
    sendJson({ type: "quote_refine_compound", request });
    return { ok: true, request: clone(request) };
  } catch (error) {
    if (generation !== refineQuoteRequestGeneration) return { ok: false, reason: "refine quote superseded" };
    const reason = `refine quote request failed: ${String(error)}`;
    state.refineQuoteRequest = { status: "error", errorCode: "session_sync_failed", error: reason };
    return { ok: false, reason };
  }
}

function canAutoIssueHostedPlayerSession() {
  return isHostedPublicJoinDeploymentMode(state.hostedAccess?.deployment_mode)
    && state.auth.source !== LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE;
}

function isLoopbackHostname(raw) {
  const value = String(raw || "").trim().toLowerCase();
  return value === "localhost" || value === "127.0.0.1" || value === "::1" || value === "[::1]" || value === "";
}

function hostnameFromUrl(raw, base = window.location.href) {
  const value = String(raw || "").trim();
  if (!value) return null;
  try {
    return new URL(value, base).hostname || null;
  } catch (_) {
    return null;
  }
}

function canAutoIssueLocalTestPlayerSession() {
  if (state.auth.available) {
    return false;
  }
  if (isHostedPublicJoinDeploymentMode(state.hostedAccess?.deployment_mode)) {
    return false;
  }
  const pageHost = String(window.location.hostname || "").trim();
  const wsHost = hostnameFromUrl(state.wsUrl || initialWsUrl());
  return isLoopbackHostname(pageHost) && isLoopbackHostname(wsHost);
}

async function issueLocalTestPlayerSession(expectedAuth) {
  const stored = resolveStoredLocalTestPlayerSession();
  let installed;
  try {
    if (stored) {
      installed = await installSession(state, stored, expectedAuth);
    } else {
      const keypair = await generateEphemeralEd25519Keypair();
      // A cleared guest session is a new context too. Cancel issuance that
      // started before it instead of reviving the previous local identity.
      if (state.auth !== expectedAuth || state.auth.available) return state.auth;
      const playerId = `local-test-player-${Date.now().toString(36)}-${authNonceCounter + 1}`;
      installed = await installSession(state, {
        available: true,
        hostedAccountId: null,
        playerId,
        loginChannel: null,
        maskedLoginHint: null,
        deviceSessionId: playerId,
        publicKey: keypair.publicKey,
        privateKey: keypair.privateKey,
        releaseToken: null,
        error: null,
        revokeReason: null,
        revokedBy: null,
        source: "local_test_api_ephemeral",
        registrationStatus: "issued",
        sessionEpoch: null,
        issuedAtUnixMs: Date.now(),
        recoveryErrorCode: null,
        recoveryErrorMessage: null,
        issueInFlight: false,
        syncInFlight: false,
        runtimeStatus: "issued",
        boundAgentId: null,
        controlLostAgentId: null,
        pendingRequestedAgentId: null,
        pendingForceRebind: false,
        rebindNotice: null,
      }, expectedAuth);
    }
  } catch (error) {
    if (state.auth !== expectedAuth) return state.auth;
    throw error;
  }
  if (state.auth !== installed) return state.auth;
  if (!stored) {
    if (__OASIS7_VISUAL_TEST__ === true) window.__OASIS7_PIXEL_WORLD_VISUAL_FIXTURE_AUTH_ALIGNMENT__?.();
    persistLocalTestPlayerSession(state.auth);
  }
  render();
  maybeRecoverLocalTestStarterBindingFromSnapshot(state.snapshot);
  return state.auth;
}

async function startHostedAccountLogin() {
  if (!canAutoIssueHostedPlayerSession()) {
    return { ok: false, reason: "hosted account login is unavailable on this lane" };
  }
  const channel = "email";
  state.hostedLogin.channel = channel;
  const handle = String(state.hostedLogin.handle || "").trim();
  if (!handle) {
    state.hostedLogin.error = "email is required before login can start";
    render();
    return { ok: false, reason: state.hostedLogin.error };
  }
  state.hostedLogin.startInFlight = true;
  state.hostedLogin.error = null;
  state.hostedLogin.retryAfterSeconds = null;
  render();
  try {
    const response = await fetch(HOSTED_ACCOUNT_LOGIN_START_ROUTE, {
      method: "POST",
      cache: "no-store",
      headers: {
        Accept: "application/json",
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        channel,
        handle,
      }),
    });
    const payload = await response.json();
    if (!response.ok || !payload?.ok || !payload?.challenge?.challenge_id) {
      const retryAfterSeconds = payload?.retry_after_seconds == null ? null : Number(payload.retry_after_seconds);
      const baseMessage = payload?.error || payload?.error_code || `hosted account login start failed with HTTP ${response.status}`;
      const message = retryAfterSeconds && Number.isFinite(retryAfterSeconds)
        ? `${baseMessage} (retry in ${retryAfterSeconds}s)`
        : baseMessage;
      const hostedLoginError = new Error(message);
      hostedLoginError.hostedLoginRetryAfterSeconds = retryAfterSeconds;
      throw hostedLoginError;
    }
    state.hostedLogin.challengeId = String(payload.challenge.challenge_id || "").trim() || null;
    state.hostedLogin.maskedLoginHint = String(payload.challenge.masked_login_hint || "").trim() || null;
    state.hostedLogin.deliveryMode = String(payload.challenge.delivery_mode || "").trim() || null;
    state.hostedLogin.code = "";
    state.hostedLogin.expiresAtUnixMs = payload?.challenge?.expires_at_unix_ms == null ? null : Number(payload.challenge.expires_at_unix_ms);
    state.hostedLogin.retryAfterSeconds = null;
    state.hostedLogin.accountExists = false;
    state.hostedLogin.startInFlight = false;
    state.hostedLogin.completeInFlight = false;
    state.hostedLogin.error = null;
    render();
    return { ok: true, challengeId: state.hostedLogin.challengeId };
  } catch (error) {
    state.hostedLogin.startInFlight = false;
    if (error?.hostedLoginRetryAfterSeconds != null) {
      state.hostedLogin.retryAfterSeconds = Number(error.hostedLoginRetryAfterSeconds);
    }
    state.hostedLogin.error = String(error);
    render();
    return { ok: false, reason: state.hostedLogin.error };
  }
}


async function issueHostedPlayerIdentity() {
  return completeHostedAccountLogin();
}

async function ensureHostedPlayerAuthAvailable() {
  if (canAutoIssueLocalTestPlayerSession()) {
    const expectedAuth = state.auth;
    if (localTestPlayerSessionInFlight?.auth !== expectedAuth) {
      const pending = { auth: expectedAuth, promise: null };
      pending.promise = issueLocalTestPlayerSession(expectedAuth).finally(() => {
        if (localTestPlayerSessionInFlight === pending) localTestPlayerSessionInFlight = null;
      });
      localTestPlayerSessionInFlight = pending;
    }
    return localTestPlayerSessionInFlight.promise;
  }
  return state.auth;
}

async function retryHostedPlayerIdentityIssue() {
  if (!canAutoIssueHostedPlayerSession()) {
    return { ok: false, reason: "hosted account login is unavailable on this lane" };
  }
  const auth = state.hostedLogin.challengeId
    ? await completeHostedAccountLogin()
    : await startHostedAccountLogin();
  render();
  return {
    ok: auth?.available === true || auth?.ok === true,
    playerId: auth?.playerId || null,
    error: auth?.error || state.hostedLogin.error,
  };
}

async function refreshPromptControlBinding() {
  return viewerPromptControlModule?.refreshBinding()
    || { ok: false, reason: "prompt control module is not ready" };
}

async function requestHostedStrongAuthGrant(actionId, agentId) {
  const auth = await ensureHostedAuthSigningKey(state.auth);
  const playerId = String(auth.playerId || "").trim();
  const publicKey = String(auth.publicKey || "").trim();
  const releaseToken = String(authCredentials(state.auth).releaseToken || "").trim();
  const approvalCode = String(state.strongAuth.approvalCode || "").trim();
  if (!playerId || !publicKey || !releaseToken) {
    throw new Error("hosted strong-auth grant requires an active player_session with release token and browser session signing key");
  }
  if (!approvalCode) {
    throw new Error("backend approval code is required before hosted strong auth can be granted");
  }
  const query = new URLSearchParams({
    player_id: playerId,
    public_key: publicKey,
    release_token: releaseToken,
    agent_id: String(agentId || "").trim(),
    action_id: String(actionId || "").trim(),
    approval_code: approvalCode,
  });
  const response = await fetch(`${HOSTED_STRONG_AUTH_GRANT_ROUTE}?${query.toString()}`, {
    method: "GET",
    cache: "no-store",
    headers: { Accept: "application/json" },
  });
  const payload = await response.json();
  if (payload?.admission) {
    state.hostedAdmission = clone(payload.admission);
  }
  if (!response.ok || !payload?.ok || !payload?.grant) {
    state.strongAuth.lastGrantError = payload?.error || payload?.error_code || `hosted strong-auth grant failed with HTTP ${response.status}`;
    throw new Error(state.strongAuth.lastGrantError);
  }
  state.strongAuth.lastGrantActionId = String(payload.grant.action_id || "").trim() || actionId;
  state.strongAuth.lastGrantExpiresAtUnixMs = payload?.grant?.expires_at_unix_ms == null
    ? null
    : Number(payload.grant.expires_at_unix_ms);
  state.strongAuth.lastGrantError = null;
  return payload.grant;
}

async function sendReconnectSync() {
  if (!state.auth.available || state.auth.source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE) {
    return;
  }
  const auth = await ensureHostedAuthSigningKey(state.auth);
  state.auth.syncInFlight = true;
  state.auth.registrationStatus = "registering";
  state.auth.runtimeStatus = "probing";
  state.auth.recoveryErrorCode = null;
  state.auth.recoveryErrorMessage = null;
  scheduleHostedRuntimeSyncTimeout();
  sendJson({
    type: "authoritative_recovery",
    command: {
      mode: "reconnect_sync",
      request: {
        player_id: auth.playerId,
        session_pubkey: auth.publicKey,
      },
    },
  });
}

function probeHostedRuntimeSession() {
  if (
    !state.auth.available
    || state.auth.source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE
    || state.connectionStatus !== "connected"
    || state.auth.registrationStatus !== "registered"
  ) {
    return;
  }
  state.auth.syncInFlight = true;
  state.auth.runtimeStatus = "probing";
  scheduleHostedRuntimeSyncTimeout();
  sendJson({
    type: "authoritative_recovery",
    command: {
      mode: "reconnect_sync",
      request: {
        player_id: state.auth.playerId,
        session_pubkey: state.auth.publicKey,
      },
    },
  });
}

async function releaseHostedPlayerSlot() {
  const playerId = String(state.auth.playerId || "").trim();
  const releaseToken = String(authCredentials(state.auth).releaseToken || "").trim();
  if (!playerId || !releaseToken || state.auth.source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE) {
    return { ok: false, skipped: true };
  }
  const query = `player_id=${encodeURIComponent(playerId)}&release_token=${encodeURIComponent(releaseToken)}`;
  const response = await fetch(`${HOSTED_PLAYER_SESSION_RELEASE_ROUTE}?${query}`, {
    method: "POST",
    cache: "no-store",
    headers: { Accept: "application/json" },
  });
  const payload = await response.json();
  if (!response.ok || !payload?.ok) {
    if (payload?.admission) {
      state.hostedAdmission = clone(payload.admission);
    }
    throw new Error(payload?.error || payload?.error_code || `hosted player-session release failed with HTTP ${response.status}`);
  }
  state.hostedAdmission = payload?.admission ? clone(payload.admission) : state.hostedAdmission;
  return payload;
}

function resetHostedPlayerAuthState(errorMessage = null, revocationMeta = null) {
  cancelHostedLeaseRefresh();
  clearPendingSessionRegisterWaiter("authentication session was invalidated");
  stopHostedSessionRefreshLoop();
  clearHostedPlayerSession();
  const bootstrap = resolveAuthBootstrap();
  const revokeReason = String(revocationMeta?.revokeReason || "").trim() || null;
  const revokedBy = String(revocationMeta?.revokedBy || "").trim() || null;
  clearSession(state, {
        ...bootstrap,
        source: "guest_only",
        registrationStatus: "guest",
        error: errorMessage,
        revokeReason,
        revokedBy,
        sessionEpoch: null,
        issuedAtUnixMs: null,
        releaseToken: null,
        recoveryErrorCode: null,
        recoveryErrorMessage: null,
        issueInFlight: false,
        syncInFlight: false,
        runtimeStatus: "guest",
        boundAgentId: null,
        controlLostAgentId: null,
        pendingRequestedAgentId: null,
        pendingForceRebind: false,
        rebindNotice: null,
      });
  if (bootstrap.available) void installSession(state, bootstrap).then(() => render());
  void refreshHostedAdmissionState().then(() => render());
}

async function logoutHostedPlayerSession() {
  if (!state.auth.available || state.auth.source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE) {
    return { ok: false, reason: "hosted browser session is unavailable" };
  }
  const revokeRequest = {
    player_id: state.auth.playerId,
    session_pubkey: state.auth.publicKey,
    revoke_reason: "player_logout",
    revoked_by: state.auth.playerId,
  };
  try {
    if (state.connectionStatus === "connected") {
      sendJson({
        type: "authoritative_recovery",
        command: {
          mode: "revoke_session",
          request: revokeRequest,
        },
      });
    }
  } catch (_) {
  }
  try {
    await releaseHostedPlayerSlot();
  } finally {
    resetHostedPlayerAuthState("hosted player session released locally");
    render();
  }
  return { ok: true };
}

function markHostedSessionRefreshFailure() {
  state.auth.syncInFlight = false;
  state.auth.registrationStatus = "issued";
  state.auth.runtimeStatus = "error";
  state.auth.recoveryErrorCode = "session_refresh_failed";
  state.auth.recoveryErrorMessage = state.auth.error || "hosted player session refresh failed; retry to recover this browser session";
  render();
}

const { syncHostedPlayerSessionOnConnect } = createViewerHostedSessionReconnectModule({
  authHasSigningKeyMaterial,
  legacyViewerAuthBootstrapSource: LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE,
  onRefreshFailure: markHostedSessionRefreshFailure,
  refreshHostedPlayerLease,
  registerHostedPlayerSession: () => ensureRegisteredPlayerSession(latestRequestedAgentId()),
  sendReconnectSync,
  state,
});

function buildPromptRequestFromDraft(agentId, draftOverrides) {
  const currentProfile = selectedAgentPromptProfile();
  if (!agentId || !currentProfile) {
    throw new Error("select an agent before editing prompt overrides");
  }
  const request = {
    agent_id: agentId,
    player_id: state.auth.playerId,
    public_key: state.auth.publicKey,
    expected_version: Number(currentProfile.version || 0),
    updated_by: state.auth.playerId,
    system_prompt_override: promptPatchFromDraft(currentProfile.system_prompt_override, draftOverrides.systemPrompt),
    short_term_goal_override: promptPatchFromDraft(currentProfile.short_term_goal_override, draftOverrides.shortTermGoal),
    long_term_goal_override: promptPatchFromDraft(currentProfile.long_term_goal_override, draftOverrides.longTermGoal),
  };
  if (promptControlCapabilitySelected()) request.request_id = `pc-${String(nextRequestId()).padStart(8, "0")}`;
  return request;
}

function encodePromptRequestForJson(request) {
  const encodePatch = (patch) => {
    if (!patch || patch.mode === "unchanged") {
      return undefined;
    }
    if (patch.mode === "clear") {
      return null;
    }
    return patch.value;
  };
  return {
    agent_id: request.agent_id,
    player_id: request.player_id,
    ...(request.request_id ? { request_id: request.request_id } : {}), ...(request.session_epoch != null ? { session_epoch: request.session_epoch } : {}),
    ...(request.binding_epoch != null ? { binding_epoch: request.binding_epoch } : {}), ...(request.expected_authority_epoch ? { expected_authority_epoch: request.expected_authority_epoch } : {}),
    public_key: request.public_key,
    expected_version: request.expected_version,
    updated_by: request.updated_by,
    system_prompt_override: encodePatch(request.system_prompt_override),
    short_term_goal_override: encodePatch(request.short_term_goal_override),
    long_term_goal_override: encodePatch(request.long_term_goal_override),
  };
}

function buildPromptRollbackRequest(agentId, toVersion) {
  const profile = selectedAgentPromptProfile();
  const targetVersion = Number(toVersion);
  if (!agentId || !profile) {
    throw new Error("select an agent before rolling back prompt overrides");
  }
  if (!Number.isInteger(targetVersion) || targetVersion < 0) {
    throw new Error("prompt rollback requires integer toVersion >= 0");
  }
  const request = {
    agent_id: agentId,
    player_id: state.auth.playerId,
    public_key: state.auth.publicKey,
    to_version: targetVersion,
    expected_version: Number(profile.version || 0),
    updated_by: state.auth.playerId,
  };
  if (promptControlCapabilitySelected()) request.request_id = `pc-${String(nextRequestId()).padStart(8, "0")}`;
  return request;
}

function pushChatHistory(entry) {
  const normalized = normalizeChatHistoryEntry(entry);
  if (!normalized) {
    return;
  }
  setChatHistory([normalized, ...(state.chatHistory || [])]);
  persistChatHistory();
}

function extractAgentSpokeEntry(event) {
  const kind = event?.kind;
  const kindType = String(kind?.type || "");
  if (!["agent_spoke", "AgentSpoke"].includes(kindType)) {
    return null;
  }
  const data = kind.data || {};
  return {
    id: `event-${event.id}`,
    source: "event",
    agentId: data.agent_id || null,
    locationId: data.location_id || null,
    message: data.message || "",
    tick: Number(event.time || 0),
    speaker: data.agent_id || null,
    targetAgentId: data.target_agent_id || null,
  };
}

function requestSnapshotSafe() {
  try { sendJson({ type: "request_snapshot" }); return true; } catch (_) { return false; }
}

function createSemanticFeedback(kind, action, agentId, extra = {}) {
  return bindFeedbackScope({
    id: nextRequestId(),
    kind,
    action,
    agentId,
    accepted: true,
    ok: false,
    stage: "queued",
    reason: null,
    effect: null,
    response: null,
    ...extra,
  }, state);
}

function markCurrentGameplayActionFeedbackError(error, effect = "gameplay action send failed") {
  const feedback = state.lastGameplayActionFeedback;
  if (!feedback || feedback.kind !== "gameplay_action") return;
  feedback.stage = "error"; feedback.ok = false; feedback.accepted = false;
  feedback.reason = String(error?.message || error); feedback.effect = effect;
  state.lastGameplayActionFeedback = feedback;
  if (feedback.requestAuthNonce) settleGameplayOriginal(hostedWorldAccount, feedback.requestAuthNonce, getGameplayRecoveryScope(state), state);
}

function markPendingSemanticRebind(message) {
  const text = String(message || "explicit rebind required; retrying player session registration").trim();
  for (const feedback of [state.lastChatFeedback, state.lastPromptFeedback]) {
    if (!feedback || feedback.stage !== "registering") {
      continue;
    }
    feedback.effect = text;
    feedback.reason = null;
  }
}

function enqueueSemanticCommand(command) {
  pendingSemanticCommands.push(command);
  if (!semanticSendLoop) {
    semanticSendLoop = processSemanticCommands();
  }
}

function handleSemanticCommandError(command, error) {
  if (command.kind === "chat") {
    if (!sameAgentChatFeedback(state.lastChatFeedback, command.feedback) || !agentChatFeedbackInFlight(state.lastChatFeedback)) {
      render();
      return;
    }
    clearPendingAgentChatAckTimer();
    clearPendingAgentChatOverallTimer();
  }
  if (command.kind === "prompt") {
    if (!sameSemanticFeedback(state.lastPromptFeedback, command.feedback) || !semanticFeedbackInFlight(state.lastPromptFeedback)) {
      render();
      return;
    }
    clearPendingPromptControlAckTimer();
  }
  const feedback = command.kind === "chat" ? state.lastChatFeedback : state.lastPromptFeedback;
  feedback.stage = "error";
  feedback.ok = false;
  feedback.accepted = false;
  feedback.reason = String(error);
  feedback.effect = "request build/send failed";
  render();
}

async function processSemanticCommands() {
  try {
    while (pendingSemanticCommands.length > 0) {
      const command = pendingSemanticCommands.shift();
      try {
        await executeSemanticCommand(command);
      } catch (error) {
        handleSemanticCommandError(command, error);
      }
    }
  } finally {
    semanticSendLoop = null;
    if (pendingSemanticCommands.length > 0) {
      semanticSendLoop = processSemanticCommands();
    }
  }
}

function assertSemanticCapability(actionId) {
  const capability = buildSemanticCapability(actionId);
  if (!capability.enabled) {
    throw new Error(capability.reason || state.auth.error || `${actionId} is unavailable`);
  }
}

function assertAgentChatFeedbackActive(feedback) {
  if (!sameAgentChatFeedback(state.lastChatFeedback, feedback) || !agentChatFeedbackInFlight(state.lastChatFeedback)) {
    throw new Error("agent_chat request expired before send completed");
  }
  state.lastChatFeedback = feedback;
}

function assertPromptFeedbackActive(feedback) {
  if (!sameSemanticFeedback(state.lastPromptFeedback, feedback) || !semanticFeedbackInFlight(state.lastPromptFeedback)) {
    throw new Error("prompt_control request expired before send completed");
  }
  state.lastPromptFeedback = feedback;
}

function sendAgentChat(agentIdOrPayload, maybeMessage) {
  let agentId = null;
  let message = null;
  if (typeof agentIdOrPayload === "object" && agentIdOrPayload !== null) {
    agentId = String(agentIdOrPayload.agentId || agentIdOrPayload.agent_id || selectedAgentId() || "");
    message = String(agentIdOrPayload.message || "");
  } else {
    agentId = String(agentIdOrPayload || selectedAgentId() || "");
    message = String(maybeMessage || "");
  }
  if (!agentId) {
    return { ok: false, reason: "agent chat requires a selected agent or explicit agentId" };
  }
  const controlError = currentBoundAgentControlError(agentId, "agent_chat");
  if (controlError) {
    return { ok: false, reason: controlError };
  }
  if (!message.trim()) {
    return { ok: false, reason: "agent chat message cannot be empty" };
  }
  if (isAgentChatInFlight()) {
    return { ok: false, reason: "agent_chat is already in flight; wait for ack/error before sending another message" };
  }
  const feedback = createSemanticFeedback("chat", "agent_chat", agentId, {
    effect: "queued for signing and send",
    pendingMessage: message,
    pendingPlayerId: state.auth.playerId || null,
  });
  state.lastChatFeedback = feedback;
  const command = {
    kind: "chat",
    feedback,
    timeoutMs: AGENT_CHAT_OVERALL_TIMEOUT_MS,
    execute: async () => {
      assertAgentChatFeedbackActive(feedback);
      scheduleAgentChatOverallTimeout(feedback);
      feedback.stage = "registering";
      feedback.effect = "registering player session";
      render();
      await ensureHostedPlayerAuthAvailable();
      assertAgentChatFeedbackActive(feedback);
      await ensureRegisteredPlayerSession(agentId);
      assertAgentChatFeedbackActive(feedback);
      await canonicalOwnerRead.refresh(agentId);
      assertSemanticCapability("agent_chat");
      feedback.stage = "signing";
      feedback.effect = "building auth proof";
      render();
      const request = {
        agent_id: agentId,
        message,
        player_id: state.auth.playerId,
        public_key: state.auth.publicKey,
      };
      request.auth = await buildAgentChatAuthProof(request, state.auth);
      assertAgentChatFeedbackActive(feedback);
      if (request.auth.player_id !== state.auth.playerId || request.auth.public_key !== state.auth.publicKey) throw new Error("agent_chat identity changed during signing");
      feedback.requestAuthNonce = request.auth?.nonce;
      feedback.canonicalRequest = request.canonical_authority ? request : null; feedback.canonicalRetryCount = 0;
      feedback.stage = "sent";
      feedback.effect = "agent_chat request sent; waiting for ack";
      state.lastChatFeedback = feedback;
      sendJson({ type: "agent_chat", request });
      scheduleAgentChatAckTimeout(feedback);
      state.chatDraft.message = "";
      state.chatDraft.dirty = false;
      render();
    },
  };
  enqueueSemanticCommand(command);
  render();
  return { ok: true, feedback: snapshotSemanticFeedback(feedback) };
}

function sendPromptControl(mode, payload = null) {
  const normalizedMode = String(mode || "").trim().toLowerCase();
  if (!["preview", "apply", "rollback"].includes(normalizedMode)) {
    return { ok: false, reason: "prompt control mode must be preview, apply, or rollback" };
  }
  const selectedId = selectedAgentId();
  const agentId = String(payload?.agentId || payload?.agent_id || selectedId || "");
  if (!agentId) {
    return { ok: false, reason: "prompt control requires a selected agent or explicit agentId" };
  }
  const controlError = currentBoundAgentControlError(agentId, "prompt_control");
  if (controlError) {
    return { ok: false, reason: controlError };
  }
  const protocolError = promptControlReadinessError(agentId);
  if (protocolError) {
    return { ok: false, reason: protocolError };
  }
  let request;
  try {
    if (normalizedMode === "rollback") {
      const currentVersion = Number(state.promptDraft.currentVersion || selectedAgentPromptProfile()?.version || 0);
      const fallbackVersion = Math.max(0, currentVersion - 1);
      const toVersion = payload?.toVersion ?? payload?.to_version ?? fallbackVersion;
      request = buildPromptRollbackRequest(agentId, toVersion);
    } else {
      request = buildPromptRequestFromDraft(agentId, {
        systemPrompt: payload?.systemPrompt ?? payload?.system_prompt_override ?? state.promptDraft.systemPrompt,
        shortTermGoal: payload?.shortTermGoal ?? payload?.short_term_goal_override ?? state.promptDraft.shortTermGoal,
        longTermGoal: payload?.longTermGoal ?? payload?.long_term_goal_override ?? state.promptDraft.longTermGoal,
      });
    }
  } catch (error) {
    return { ok: false, reason: String(error) };
  }

  const createdFeedback = createSemanticFeedback("prompt", `prompt_${normalizedMode}`, agentId, {
    effect: "queued for signing and send",
    toVersion: request.to_version ?? null,
    requestId: request.request_id || null,
  });
  state.lastPromptFeedback = createdFeedback;
  // Mutate the store-owned proxy, not the raw object: a rendered feedback field
  // has a reactive node, so direct writes to the detached object would never
  // update the visible stage, timer guards or ack/error correlation reads.
  const feedback = state.lastPromptFeedback;
  enqueueSemanticCommand({
    kind: "prompt",
    feedback,
    timeoutMs: SEMANTIC_ACTION_OVERALL_TIMEOUT_MS,
    execute: async () => {
      await ensureHostedPlayerAuthAvailable();
      assertPromptFeedbackActive(feedback);
      assertSemanticCapability("prompt_control");
      feedback.stage = "registering";
      feedback.effect = "registering player session";
      render();
      await ensureRegisteredPlayerSession(agentId);
      assertPromptFeedbackActive(feedback);
      attachPromptControlIdentity(request);
      let strongAuthGrant = null;
      if (isHostedPublicJoinDeploymentMode(state.hostedAccess?.deployment_mode)) {
        feedback.stage = "authorizing";
        feedback.effect = "requesting backend strong-auth grant";
        render();
        strongAuthGrant = await requestHostedStrongAuthGrant(
          normalizedMode === "rollback" ? "prompt_control_rollback" : `prompt_control_${normalizedMode}`,
          agentId,
        );
        assertPromptFeedbackActive(feedback);
      }
      feedback.stage = "signing";
      feedback.effect = "building auth proof";
      render();
      let commandRequest;
      if (normalizedMode === "rollback") {
        commandRequest = {
          ...request,
          auth: await buildPromptRollbackAuthProof(request, state.auth),
        };
        if (strongAuthGrant) {
          commandRequest.strong_auth_grant = strongAuthGrant;
        }
      } else {
        commandRequest = encodePromptRequestForJson(request);
        commandRequest.auth = await buildPromptControlAuthProof(normalizedMode, request, state.auth);
        if (strongAuthGrant) {
          commandRequest.strong_auth_grant = strongAuthGrant;
        }
      }
      assertPromptFeedbackActive(feedback);
      feedback.stage = "sent";
      feedback.effect = `prompt ${normalizedMode} request sent; waiting for ack`;
      state.lastPromptFeedback = feedback;
      feedback.requestId = request.request_id || null;
      feedback.requestAuthNonce = commandRequest.auth?.nonce;
      feedback.submittedDraft = promptDraftValuesForRequest(request);
      sendJson({
        type: "prompt_control",
        command: {
          mode: normalizedMode,
          request: commandRequest,
        },
      });
      schedulePromptControlAckTimeout(feedback);
      render();
    },
  });
  render();
  return { ok: true, feedback: snapshotSemanticFeedback(feedback) };
}

function gameplayActionRequiresActorAgent(actionId) {
  return actionId === "claim_agent" || actionId === "release_agent_claim";
}

function gameplayActionControlError(action) {
  const normalized = normalizeGameplayActionRequest(action);
  if (!normalized || normalized.protocol_action !== "gameplay_action.submit") {
    return null;
  }
  const actionId = String(normalized.action_id || "").trim();
  const targetAgentId = String(normalized.target_agent_id || "").trim();
  const actorAgentId = String(normalized.actor_agent_id || "").trim();
  if (!actionId || !targetAgentId || actionId === "claim_first_agent") {
    return null;
  }
  if (gameplayActionRequiresActorAgent(actionId)) {
    const actorControlError = currentBoundAgentControlError(
      actorAgentId || state.auth.boundAgentId,
      `${actionId} actor`,
    );
    if (actorControlError) {
      return actorControlError;
    }
    if (actorAgentId && actorAgentId !== state.auth.boundAgentId) {
      return `${actionId} actor ${actorAgentId} does not match current bound Agent ${state.auth.boundAgentId}`;
    }
    return null;
  }
  return currentBoundAgentControlError(targetAgentId, actionId);
}

function resolveGameplayActionRequest(actionOrId) {
  if (typeof actionOrId === "string") {
    const actions = Array.isArray(state.snapshot?.player_gameplay?.available_actions)
      ? state.snapshot.player_gameplay.available_actions
      : [];
    return actions.find((action) => action?.action_id === actionOrId) || null;
  }
  if (!actionOrId || typeof actionOrId !== "object") {
    return null;
  }
  if (typeof actionOrId.actionId === "string" && actionOrId.actionId.trim()) {
    const resolved = resolveGameplayActionRequest(actionOrId.actionId.trim());
    if (resolved) {
      return resolved;
    }
  }
  return normalizeGameplayActionRequest(actionOrId);
}

function sendGameplayAction(actionOrId, operation = { sessionRefreshRetryAttempted: false }, retryFeedback = null) {
  const action = resolveGameplayActionRequest(actionOrId);
  if (!action) {
    return { ok: false, reason: "gameplay action is unavailable in the current snapshot" };
  }

  const protocolAction = String(action.protocol_action || "").trim();
  if (protocolAction === "request_snapshot" || protocolAction === "world.request_snapshot") {
    if (requestSnapshotSafe() === false) {
      state.lastGameplayActionFeedback = {
        id: nextRequestId(), kind: "gameplay_action", action: action.action_id || "request_snapshot",
        agentId: action.target_agent_id || null, accepted: false, ok: false, stage: "rejected",
        reason: "viewer websocket is not connected", effect: "snapshot refresh failed to send",
        response: { action_id: action.action_id || "request_snapshot", target_agent_id: action.target_agent_id || "", message: "viewer websocket is not connected" },
      };
      render();
      return { ok: false, reason: "viewer websocket is not connected", feedback: snapshotSemanticFeedback(state.lastGameplayActionFeedback) };
    }
    if (hostedWorldAccount.hasPendingGameplayRequest()) {
      requestGameplayResult(state, getGameplayRecoveryScope(state), sendJson);
      return { ok: true, feedback: snapshotSemanticFeedback(state.lastGameplayActionFeedback) };
    }
    state.lastGameplayActionFeedback = {
      id: nextRequestId(), kind: "gameplay_action", action: action.action_id || "request_snapshot",
      agentId: action.target_agent_id || null, accepted: true, ok: true, stage: "ack", reason: null, effect: "snapshot refresh requested",
      response: { action_id: action.action_id || "request_snapshot", target_agent_id: action.target_agent_id || "", accepted_at_tick: state.logicalTime, message: "snapshot refresh requested" },
    };
    render();
    return { ok: true, feedback: snapshotSemanticFeedback(state.lastGameplayActionFeedback) };
  }
  if (protocolAction === "live_control.step") {
    return { ok: true, feedback: sendControl("step", { count: 1 }) };
  }
  if (protocolAction === "live_control.play") {
    return { ok: true, feedback: sendControl("play", null) };
  }
  if (protocolAction === "live_control.pause") {
    return { ok: true, feedback: sendControl("pause", null) };
  }
  if (protocolAction !== "gameplay_action.submit") {
    return { ok: false, reason: `unsupported gameplay action protocol: ${protocolAction || "(empty)"}` };
  }

  const actionId = String(action.action_id || "").trim();
  const targetAgentId = String(action.target_agent_id || "").trim();
  const actorAgentId = String(action.actor_agent_id || "").trim();
  if (!actionId || !targetAgentId) {
    return { ok: false, reason: "gameplay_action.submit requires action_id and target_agent_id" };
  }
  const disabledReason = String(action.disabled_reason || "").trim();
  if (disabledReason) {
    return { ok: false, reason: disabledReason };
  }
  const controlError = gameplayActionControlError(action);
  if (controlError) {
    return { ok: false, reason: controlError };
  }

  const previous = state.lastGameplayActionFeedback;
  const internalRetry = retryFeedback && sameSemanticFeedback(previous, retryFeedback)
    && previous.stage === "registering" && operation === retryFeedback.operation && operation.sessionRefreshRetryAttempted === true;
  if (hostedWorldAccount.hasPendingGameplayRequest() || (!internalRetry && previous && ((previous.action === actionId && (previous.targetAgentId || previous.agentId) === targetAgentId
    && ["sent", "pending", "received", "unknown"].includes(previous.stage)) || (state.auth.source === "hosted_account_cookie"
    && ["queued", "registering", "signing", "sent", "pending", "received", "unknown"].includes(previous.stage))))) {
    requestSnapshotSafe();
    return { ok: false, reason: "original gameplay request outcome is unresolved; refresh its authoritative result before submitting again" };
  }

  const feedback = createSemanticFeedback("gameplay_action", actionId, targetAgentId, {
    effect: "queued for signing and send",
    targetAgentId,
    protocolAction,
  });
  feedback.operation = operation;
  state.lastGameplayActionFeedback = feedback;
  render();

  void (async () => {
    try {
      await ensureHostedPlayerAuthAvailable();
      assertSemanticCapability(actionId);
      feedback.stage = "registering";
      feedback.effect = "registering player session";
      render();
      const registrationAgentId = gameplayActionRequiresActorAgent(actionId)
        ? actorAgentId || state.auth.boundAgentId || targetAgentId
        : actionId === "claim_first_agent"
          ? null
        : targetAgentId;
      await ensureRegisteredPlayerSession(registrationAgentId);
      if (!sameSemanticFeedback(state.lastGameplayActionFeedback, feedback)) return;
      if (!socket || socket.readyState !== WebSocket.OPEN) throw new Error("viewer websocket is not connected");
      feedback.stage = "signing";
      feedback.effect = "building auth proof";
      render();
      const request = {
        action_id: actionId,
        target_agent_id: targetAgentId,
        player_id: state.auth.playerId,
        public_key: state.auth.publicKey,
      };
      if (gameplayActionRequiresActorAgent(actionId)) {
        request.actor_agent_id = actorAgentId || state.auth.boundAgentId || registrationAgentId;
      }
      const sendContext = { socket, scope: getGameplayRecoveryScope(state), session: captureSessionContext(state, authConnectionGeneration(), state.wsUrl) };
      request.auth = await buildGameplayActionAuthProof(request, state.auth);
      if (!sameSemanticFeedback(state.lastGameplayActionFeedback, feedback)) return;
      if (request.auth.player_id !== state.auth.playerId || request.auth.public_key !== state.auth.publicKey) throw new Error("gameplay identity changed during signing");
      feedback.requestAuthNonce = request.auth.nonce;
      feedback.originalRequest = clone(request);
      feedback.gameplayScope = structuredClone(sendContext.scope);
      await hostedWorldAccount.rememberGameplayRequest(request, sendContext.scope);
      if (!sameSemanticFeedback(state.lastGameplayActionFeedback, feedback)) return;
      if (!gameplaySendContextCurrent(state, socket, sendContext)) {
        feedback.stage = "unknown"; feedback.effect = "original request context changed before send";
        state.lastGameplayActionFeedback = feedback; render(); return;
      }
      feedback.stage = "sent";
      feedback.effect = "gameplay action sent; waiting for ack";
      state.lastGameplayActionFeedback = feedback;
      sendJson({
        type: "gameplay_action",
        request,
      });
      scheduleGameplayActionAckTimeout(feedback);
      render();
    } catch (error) {
      if (!sameSemanticFeedback(state.lastGameplayActionFeedback, feedback)) return;
      markCurrentGameplayActionFeedbackError(error);
      render();
    }
  })();

  return { ok: true, feedback: snapshotSemanticFeedback(feedback) };
}

function handleGameplayActionAck(ack) {
  if (!acceptRuntimeAckIdentity(ack)) return;
  const pendingAction = state.lastGameplayActionFeedback;
  if (!pendingAction || !["sent", "pending", "received", "unknown"].includes(pendingAction.stage) || !matchesGameplayReply(ack, pendingAction)) return;
  clearPendingGameplayActionAckTimer();
  const feedback = state.lastGameplayActionFeedback || createSemanticFeedback(
    "gameplay_action",
    ack?.action_id || "gameplay_action",
    ack?.target_agent_id || null,
  );
  feedback.stage = ack?.submission_status || "ack";
  feedback.ok = ack?.submission_status == null || ack.submission_status === "committed";
  feedback.accepted = feedback.ok;
  feedback.reason = null;
  feedback.effect = ack?.message || `gameplay action accepted at tick ${Number(ack?.accepted_at_tick || state.logicalTime)}`;
  feedback.response = clone(ack);
  state.lastGameplayActionFeedback = feedback;
  if (ack?.submission_status === "committed") {
    settleGameplayOriginal(hostedWorldAccount, feedback.requestAuthNonce, getGameplayRecoveryScope(state), state);
  }
  if (ack?.action_id === "claim_first_agent" || Number(ack?.runtime_action_id) === 0) {
    feedback.stage = ["received", "pending", "committed"].includes(ack?.submission_status) ? ack.submission_status : "pending";
    feedback.ok = false;
    feedback.accepted = false;
    feedback.effect = "request received; waiting for authoritative result and owner binding";
    scheduleGameplayActionAckTimeout(feedback);
  }
  requestSnapshotSafe();
}

async function retryGameplayActionAfterMissingSession(feedback, error, settlement) {
  const actionId = String(error?.action_id || feedback?.action || "").trim();
  const targetAgentId = String(error?.target_agent_id || feedback?.agentId || "").trim();
  if (!actionId || !targetAgentId || feedback?.operation?.sessionRefreshRetryAttempted) {
    return;
  }
  const action = resolveGameplayActionRequest(actionId) || normalizeGameplayActionRequest({
    protocol_action: "gameplay_action.submit",
    action_id: actionId,
    target_agent_id: targetAgentId,
  });
  if (!action) {
    return;
  }
  const controlError = gameplayActionControlError(action);
  if (controlError) {
    feedback.stage = "error";
    feedback.ok = false;
    feedback.accepted = false;
    feedback.reason = controlError;
    feedback.effect = "player session refresh blocked by current account Agent boundary";
    state.lastGameplayActionFeedback = feedback;
    render();
    return;
  }
  feedback.operation ||= { sessionRefreshRetryAttempted: false };
  feedback.operation.sessionRefreshRetryAttempted = true;
  feedback.stage = "registering";
  feedback.ok = false;
  feedback.accepted = false;
  feedback.reason = "runtime session was missing; refreshing player session and retrying";
  feedback.effect = "refreshing player session";
  state.lastGameplayActionFeedback = feedback;
  render();
  try {
    const settled = await settlement;
    if (!sameSemanticFeedback(state.lastGameplayActionFeedback, feedback)) return;
    if (state.auth.source === "hosted_account_cookie" && settled !== true) {
      feedback.stage = "unknown"; feedback.reason = "original gameplay request settlement is not confirmed";
      state.lastGameplayActionFeedback = feedback; render(); return;
    }
    const registrationAgentId = gameplayActionRequiresActorAgent(actionId)
      ? action.actor_agent_id || action.actorAgentId || state.auth.boundAgentId || targetAgentId
      : actionId === "claim_first_agent"
        ? null
        : targetAgentId;
    await ensureRegisteredPlayerSession(registrationAgentId, { forceRebind: true });
    if (!sameSemanticFeedback(state.lastGameplayActionFeedback, feedback)) return;
    sendGameplayAction(action, feedback.operation, feedback);
  } catch (retryError) {
    if (!sameSemanticFeedback(state.lastGameplayActionFeedback, feedback)) return;
    markCurrentGameplayActionFeedbackError(
      retryError,
      "player session refresh failed before retrying gameplay action",
    );
    render();
  }
}

function handleGameplayActionError(error) {
  if (quoteProtocolFacade.handleQuoteGameplayActionError(error)) return;
  const feedback = state.lastGameplayActionFeedback;
  if (!feedback || !["sent", "pending", "received", "unknown"].includes(feedback.stage) || !matchesGameplayReply(error, feedback)) return;
  clearPendingGameplayActionAckTimer();
  const authExpired = isHostedAuthExpired(error);
  feedback.stage = (error?.code === "world_service_outcome_unknown" || authExpired) ? "unknown" : "error";
  feedback.ok = false;
  feedback.accepted = false;
  feedback.reason = error?.message || error?.code || "gameplay action failed";
  feedback.effect = authExpired ? "hosted authorization expired; reconciling authoritative result" : (error?.code || "gameplay action error");
  feedback.response = clone(error);
  state.lastGameplayActionFeedback = feedback;
  const settlement = feedback.stage !== "unknown" ? settleGameplayOriginal(hostedWorldAccount, feedback.requestAuthNonce, getGameplayRecoveryScope(state), state) : Promise.resolve(false);
  if (authExpired) {
    hostedWorldAccount.handleAuthorizationExpired?.(error);
    void hostedWorldAccount.renew().then(() => {
      state.gameplayResultLookup = null;
      requestGameplayResult(state, getGameplayRecoveryScope(state), sendJson);
    }).catch(() => {});
    return;
  }
  if (String(error?.code || "").trim() === "session_not_found") {
    void retryGameplayActionAfterMissingSession(feedback, error, settlement);
  }
}

function applyPromptAckLocally(ack) {
  const agentId = ack?.agent_id;
  if (!agentId || !state.snapshot?.model) {
    return;
  }
  if (!state.snapshot.model.agent_prompt_profiles) {
    state.snapshot.model.agent_prompt_profiles = {};
  }
  const current = state.snapshot.model.agent_prompt_profiles[agentId] || { agent_id: agentId };
  const nextProfile = {
    ...current,
    agent_id: agentId,
    version: Number(ack.version || current.version || 0),
    updated_at_tick: Number(ack.updated_at_tick || state.logicalTime),
    updated_by: state.auth.playerId || current.updated_by || "",
  };
  if (!ack.preview) {
    nextProfile.system_prompt_override = state.promptDraft.systemPrompt || null;
    nextProfile.short_term_goal_override = state.promptDraft.shortTermGoal || null;
    nextProfile.long_term_goal_override = state.promptDraft.longTermGoal || null;
  }
  state.snapshot.model.agent_prompt_profiles[agentId] = nextProfile;
  if (selectedAgentId() === agentId) {
    state.promptDraft = {
      agentId,
      currentVersion: nextProfile.version,
      rollbackTargetVersion: Math.max(0, Number(nextProfile.version || 0) - 1),
      updatedBy: nextProfile.updated_by,
      updatedAtTick: nextProfile.updated_at_tick,
      systemPrompt: String(nextProfile.system_prompt_override || ""),
      shortTermGoal: String(nextProfile.short_term_goal_override || ""),
      longTermGoal: String(nextProfile.long_term_goal_override || ""),
      dirty: false,
    };
  }
}

function handlePromptControlAck(ack) {
  if (acceptRuntimeAckIdentity(ack)) viewerPromptControlModule?.handleAck(ack);
}
function handlePromptControlError(error) {
  viewerPromptControlModule?.handleError(error);
}
function handleAgentChatAck(ack) {
  if (!acceptRuntimeAckIdentity(ack)) return;
  const feedback = state.lastChatFeedback;
  if (!agentChatFeedbackInFlight(feedback) || ack?.agent_id !== feedback.agentId) return;
  if (feedback.canonicalRequest && !canonicalAckMatches(ack, feedback.canonicalRequest)) return;
  if (feedback.requestAuthNonce != null && (ack?.request_auth_nonce ?? ack?.auth_nonce) !== feedback.requestAuthNonce) return;
  clearPendingAgentChatAckTimer();
  if (feedback.canonicalRequest && ack?.status === "pending") {
    setPendingAgentChatAckTimer(scheduleCanonicalGoalPending({ ack, feedback, state, clone, requestSnapshotSafe, sameAgentChatFeedback, failPendingAgentChatAck, sendJson, scheduleAgentChatAckTimeout, getSocket: () => socket }));
    return;
  }
  if (feedback.canonicalRequest && (ack?.status !== "accepted" || !ack?.intent_id)) {
    failPendingAgentChatAck("canonical goal commit was not confirmed"); return;
  }
  clearPendingAgentChatOverallTimer();
  applyCommittedChatAck({ ack, feedback, state, clone, pushChatHistory });
}

function handleAgentChatError(error) {
  const current = state.lastChatFeedback;
  if (!agentChatFeedbackInFlight(current) || error?.agent_id !== current.agentId) return;
  if (current.canonicalRequest && error?.request_auth_nonce !== current.canonicalRequest.auth?.nonce) return;
  if (current.requestAuthNonce != null && error?.request_auth_nonce !== current.requestAuthNonce) return;
  clearPendingAgentChatAckTimer();
  clearPendingAgentChatOverallTimer();
  const feedback = state.lastChatFeedback || createSemanticFeedback("chat", "agent_chat", error?.agent_id || selectedAgentId());
  const agentId = error?.agent_id || feedback.agentId || selectedAgentId() || null;
  const controlLost = viewerControlLossModule?.isControlLossError(error) === true;
  const response = controlLost ? viewerControlLossModule.markControlLost(agentId) : clone(error);
  feedback.stage = "error";
  feedback.ok = false;
  feedback.accepted = false;
  feedback.reason = controlLost ? "control_lost" : error?.message || error?.code || "agent chat failed";
  feedback.effect = controlLost ? "control_lost" : error?.code || "agent chat error";
  feedback.response = response;
  state.lastChatFeedback = feedback;
  pushChatHistory({
    id: `chat-error-${feedback.id}`,
    source: "error",
    agentId,
    targetAgentId: agentId,
    playerId: feedback.pendingPlayerId || state.auth.playerId || null,
    speaker: "runtime",
    message: feedback.reason,
    code: controlLost ? "control_lost" : error?.code || null,
    tick: Number(error?.accepted_at_tick || state.logicalTime || 0),
    locationId: error?.location_id || null,
    response,
  });
}

function adoptHostedRecoveryAck(ack) {
  if (!ack || !state.auth.available || (ack.status === "catch_up_ready" && ack.message === "snapshot_sync_metadata" && [ack.player_id, ack.session_pubkey, ack.session_epoch, ack.binding_epoch, ack.agent_id].every((value) => value == null))) return;
  if (!acceptRuntimeAckIdentity(ack, ack.status === "session_registered" || ack.status === "session_revoked")) return;
  if (ack.status === "session_registered" && (!pendingSessionRegisterWaiter || !isSessionContextCurrent(state, pendingSessionRegisterWaiter.context, authConnectionGeneration(), state.wsUrl))) return;
  clearHostedRuntimeSyncTimer();
  const usesLegacyPreviewBootstrap = state.auth.source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE;
  const hadPendingForceRebind = state.auth.pendingForceRebind === true;
  const previousRequestedAgentId = state.auth.pendingRequestedAgentId;
  const nextBoundAgentId = Object.prototype.hasOwnProperty.call(ack, "agent_id")
    ? (ack.agent_id || null)
    : (state.auth.boundAgentId || null);
  const nextRequestedAgentId = ack.agent_id || state.auth.pendingRequestedAgentId || state.auth.boundAgentId || null;
  state.auth.syncInFlight = false;
  state.auth.recoveryErrorCode = null;
  state.auth.recoveryErrorMessage = null;
  state.auth.error = null;
  state.auth.revokeReason = null;
  state.auth.revokedBy = null;
  if (ack.session_epoch != null) {
    state.auth.sessionEpoch = Number(ack.session_epoch);
  }
  if (Object.prototype.hasOwnProperty.call(ack, "binding_epoch")) {
    state.auth.bindingEpoch = ack.binding_epoch == null ? null : Number(ack.binding_epoch);
  }
  state.auth.boundAgentId = nextBoundAgentId;
  if (ack.status === "session_registered" || ack.status === "catch_up_ready") {
    viewerControlLossModule?.clearControlLost(nextBoundAgentId);
  }
  state.auth.pendingRequestedAgentId = nextRequestedAgentId;
  state.auth.pendingForceRebind = false;
  if (ack.status === "session_registered" && hadPendingForceRebind) {
    state.auth.rebindNotice = `Player session switched to ${ack.agent_id || previousRequestedAgentId || "requested agent"}.`;
  }
  state.auth.registrationStatus = ack.status === "session_registered" || ack.status === "catch_up_ready"
    ? "registered"
    : ack.status === "session_revoked"
      ? "guest"
      : "issued";
  if (ack.status === "session_registered" || ack.status === "catch_up_ready") {
    updateRegistrationGrant(state.auth, null);
  }
  state.auth.runtimeStatus = ack.status === "session_revoked"
    ? "revoked"
    : nextBoundAgentId
      ? "registered"
      : "registered_unbound";
  if (ack.status === "session_revoked") {
    if (usesLegacyPreviewBootstrap) {
      state.auth.registrationStatus = "issued";
      state.auth.runtimeStatus = "revoked";
      state.auth.error = ack.message || "legacy preview player session was revoked";
      state.auth.pendingRequestedAgentId = null;
      state.auth.pendingForceRebind = false;
    } else {
      void releaseHostedPlayerSlot().catch(() => {});
      resetHostedPlayerAuthState(
        ack.message || "hosted player session was revoked",
        {
          revokeReason: ack.revoke_reason || ack.message || null,
          revokedBy: ack.revoked_by || null,
        },
      );
    }
  } else {
    if (!usesLegacyPreviewBootstrap) {
      persistHostedPlayerSession(state.auth);
      void refreshHostedPlayerLease();
      syncHostedSessionRefreshLoop();
    }
  }
  if (ack.status === "session_registered") {
    resolvePendingSessionRegisterWaiterAfterRuntimeAck(ack);
  }
  maybeRecoverLocalTestStarterBindingFromSnapshot(state.snapshot);
  if (ack.status === "session_registered") {
    requestSnapshotSafe();
    requestGameplayResult(state, getGameplayRecoveryScope(state), sendJson);
  }
}

async function recoverHostedSessionFromError(error) {
  if (
    (!canAutoIssueHostedPlayerSession() && !state.auth.available)
    || state.auth.source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE
  ) {
    return;
  }
  if (isHostedAuthExpired(error)) {
    if (state.auth.source === "hosted_account_cookie") {
      await hostedWorldAccount.renew();
      return;
    }
  }
  const code = String(error?.code || "").trim();
  if (code === "session_not_found" && pendingSessionRegisterWaiter?.forceRebind) {
    return;
  }
  if (recoveryErrorRequiresExplicitRebind(error) && state.auth.pendingRequestedAgentId && !state.auth.pendingForceRebind) {
    await ensureRegisteredPlayerSession(state.auth.pendingRequestedAgentId, { forceRebind: true });
    return;
  }
  if (code === "session_not_found") {
    await ensureRegisteredPlayerSession(latestRequestedAgentId());
    return;
  }
  if (code === "session_revoked") {
    void releaseHostedPlayerSlot().catch(() => {});
    resetHostedPlayerAuthState(
      error?.message || code || "hosted player session failed",
      {
        revokeReason: error?.revoke_reason || error?.message || null,
        revokedBy: error?.revoked_by || null,
      },
    );
    render();
    return;
  }
  if (["session_key_mismatch", "session_player_id_invalid"].includes(code)) {
    void releaseHostedPlayerSlot().catch(() => {});
    resetHostedPlayerAuthState(error?.message || code || "hosted player session failed");
    render();
    await issueHostedPlayerIdentity();
    if (state.auth.available) {
      await ensureRegisteredPlayerSession(latestRequestedAgentId());
    }
  }
}

function handleAuthoritativeRecoveryAck(ack) {
  adoptHostedRecoveryAck(ack);
}

function handleAuthoritativeRecoveryError(error) {
  clearHostedRuntimeSyncTimer();
  if (String(error?.code || "").trim() === "session_not_found" && pendingSessionRegisterWaiter?.forceRebind) {
    state.auth.recoveryErrorCode = null;
    state.auth.recoveryErrorMessage = null;
    state.auth.runtimeStatus = "rebind_registering";
    state.auth.error = null;
    render();
    return;
  }
  const rebindAgentId = rebindAgentIdFromRecoveryError(error);
  if (
    pendingSessionRegisterWaiter
    && recoveryErrorRequiresExplicitRebind(error)
    && (pendingSessionRegisterWaiter.requestedAgentId || rebindAgentId)
    && !pendingSessionRegisterWaiter.forceRebind
  ) {
    if (!pendingSessionRegisterWaiter.requestedAgentId && rebindAgentId) {
      pendingSessionRegisterWaiter.requestedAgentId = rebindAgentId;
      state.auth.pendingRequestedAgentId = rebindAgentId;
    }
    state.auth.recoveryErrorCode = error?.code || null;
    state.auth.recoveryErrorMessage = error?.message || null;
    state.auth.error = error?.message || error?.code || "authoritative recovery failed";
    state.auth.registrationStatus = "registering";
    state.auth.runtimeStatus = "rebind_retrying";
    state.auth.pendingForceRebind = true;
    state.auth.rebindNotice = `Requested agent ${state.auth.pendingRequestedAgentId || "-"} needs explicit rebind; retrying now.`;
    markPendingSemanticRebind("explicit rebind required; retrying registration for the requested agent");
    render();
    void retryPendingSessionRegisterWaiterWithForceRebind().catch((retryError) => {
      handleAuthoritativeRecoveryError({
        code: "player_bind_failed",
        message: String(retryError),
      });
    });
    return;
  }
  if (
    !pendingSessionRegisterWaiter
    && state.auth.source === "local_test_api_ephemeral"
    && recoveryErrorRequiresExplicitRebind(error)
    && rebindAgentId
  ) {
    state.auth.recoveryErrorCode = error?.code || null;
    state.auth.recoveryErrorMessage = error?.message || null;
    state.auth.error = error?.message || error?.code || "authoritative recovery failed";
    state.auth.registrationStatus = "registering";
    state.auth.runtimeStatus = "rebind_retrying";
    state.auth.pendingRequestedAgentId = rebindAgentId;
    state.auth.pendingForceRebind = true;
    state.auth.rebindNotice = `Requested agent ${rebindAgentId} needs explicit rebind; retrying now.`;
    render();
    void ensureRegisteredPlayerSession(rebindAgentId, { forceRebind: true }).catch((retryError) => {
      handleAuthoritativeRecoveryError({
        code: "player_bind_failed",
        message: String(retryError),
        agent_id: rebindAgentId,
      });
    });
    return;
  }
  if (!state.auth.available || state.auth.source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE) {
    clearPendingSessionRegisterWaiter(error?.message || error?.code || "authoritative recovery failed");
    return;
  }
  if (isHostedAuthExpired(error)) {
    clearPendingSessionRegisterWaiter(error?.message || error?.code || "hosted authorization expired");
    hostedWorldAccount.handleAuthorizationExpired?.(error);
    return;
  }
  state.auth.syncInFlight = false;
  state.auth.recoveryErrorCode = error?.code || null; state.auth.recoveryErrorMessage = error?.message || null;
  state.auth.error = error?.message || error?.code || "authoritative recovery failed";
  state.auth.revokeReason = error?.revoke_reason || null; state.auth.revokedBy = error?.revoked_by || null;
  state.auth.registrationStatus = "issued";
  state.auth.runtimeStatus = error?.code === "session_revoked" ? "revoked" : error?.code === "session_not_found" ? "missing" : "error";
  if (!recoveryErrorRequiresExplicitRebind(error) && !isHostedAuthExpired(error)) state.auth.boundAgentId = null;
  clearPendingSessionRegisterWaiter(error?.message || error?.code || "authoritative recovery failed");
  syncHostedSessionRefreshLoop();
  void recoverHostedSessionFromError(error);
}

const worldViewTransport = createWorldViewTransport({ send: sendJson, getScope: () => worldViewScopeKey(state), getConnection: () => socket });
export const {request: requestWorldView, statistics: readWorldViewTransportStatistics} = worldViewTransport;
const scopeSnapshotTransport = createScopeSnapshotTransport({send:sendJson,getScope:()=>worldViewScopeKey(state),getConnection:()=>socket});
export const requestScopeSnapshot = () => scopeSnapshotTransport.request();
// Host authorizes the captured scope before this existing adopter triggers render.
export const acceptScopeSnapshot = snapshot => handleSnapshot(snapshot);
function handleViewerMessage(message, sourceSocket = null) { if (sourceSocket && socket !== sourceSocket) return;
  if (message?.request_auth_nonce != null) {
    for (const key of ["ack", "error", "quote"]) if (message[key]) message[key].request_auth_nonce = message.request_auth_nonce;
  }
  if (scopeSnapshotTransport.handle(message)) return;
  if (worldViewTransport.handle(message)) return;
  if (quoteProtocolFacade.handleQuoteViewerMessage(message)) return;
  switch (message?.type) {
    case "hello_ack":
      clearHelloAckTimer();
      viewerPromptControlModule?.handleHelloAck(message);
      state.server = message.server || null;
      state.worldId = message.world_id || null;
      state.controlProfile = message.control_profile || "playback";
      liveControlState.hello(message);
      hydrateChatHistoryFromStorage();
      if (!initialSnapshotRequested) {
        initialSnapshotRequested = true;
        initialSnapshotRetryCount = 0;
        sendInitialSnapshotRequest();
        scheduleInitialSnapshotRetry();
      }
      void ensureHostedPlayerAuthAvailable().then(() => {
        void syncHostedPlayerSessionOnConnect().catch((error) => { state.auth.error = String(error); render(); });
        render();
      }).catch((error) => { state.auth.error = String(error); render(); });
      break;
    case "live_control_state":
      liveControlState.receive(message.state);
      break;
    case "snapshot":
      handleSnapshot(message.snapshot);
      break;
    case "world_feed":
      worldFeedTransport.handleWorldFeed(message.feed, sourceSocket);
      hostedWorldAccount.restoreGameplayRequest(getGameplayRecoveryScope(state)); break;
    case "event": {
      addRecentEvent(message.event);
      const chatEntry = extractAgentSpokeEntry(message.event);
      if (chatEntry) {
        pushChatHistory(chatEntry);
      }
      state.logicalTime = Math.max(state.logicalTime, Number(message.event?.time || 0));
      state.tick = state.logicalTime; worldFeedTransport.refreshAfterWorldActivity();
      break;
    }
    case "metrics":
      handleMetrics(message.time, message.metrics);
      break;
    case "decision_trace":
      handleDecisionTrace(message.trace);
      break;
    case "control_completion_ack":
      handleControlCompletionAck(message.ack);
      break;
    case "prompt_control_ack":
      handlePromptControlAck(message.ack);
      break;
    case "prompt_control_error":
      handlePromptControlError(message.error);
      break;
    case "safe_alternative_result": safeAlternative.receive(message); break;
    case "canonical_agent_owner_read_context": void canonicalOwnerRead.handleContext(message.context); break;
    case "canonical_agent_owner_view": canonicalOwnerRead.handleView(message); break;
    case "agent_chat_ack":
      handleAgentChatAck(message.ack);
      break;
    case "agent_chat_error":
      handleAgentChatError(message.error);
      break;
    case "gameplay_action_result":
      if (receiveGameplayResult(state, getGameplayRecoveryScope(state), hostedWorldAccount, message)) {
        clearPendingGameplayActionAckTimer();
        requestSnapshotSafe();
        render();
      }
      break;
    case "gameplay_action_ack":
      handleGameplayActionAck(message.ack);
      break;
    case "gameplay_action_error":
      handleGameplayActionError(message.error);
      break;
    case "authoritative_recovery_ack":
      handleAuthoritativeRecoveryAck(message.ack);
      break;
    case "authoritative_recovery_error":
      handleAuthoritativeRecoveryError(message.error);
      break;
    case "error":
      reportFatalError(message.message, "viewer");
      break;
    default:
      break;
  }
  // These streams change telemetry only; preserve UI notification without
  // rebuilding the snapshot/selection/event-based pixel-world input.
  render({ pixelWorldChanged: message?.type !== "metrics" && message?.type !== "decision_trace" });
}

function attachSocket(ws) {
  ws.addEventListener("open", () => { if (socket !== ws) return; worldFeedTransport.resetGeneration(ws);
    resetViewerProtocolForConnection();
    state.connectionStatus = "connected";
    state.lastError = null;
    state.server = null;
    state.worldId = null;
    initialSnapshotRequested = false;
    currentConnectionSnapshotReceived = false;
    initialSnapshotReadFailed = false;
    initialSnapshotRetryCount = 0;
    clearHelloAckTimer();
    clearInitialSnapshotRetryTimer();
    sendJson({ type: "hello_v2", client: "viewer", version: 2, capabilities: [PROMPT_CONTROL_RESULT_CAPABILITY, LIVE_CONTROL_STATE_CAPABILITY, "gameplay_result_lookup_v1", "canonical_agent_chat_v1", "world_view", "world_view_pins_v1", "world_view_delta_v1", "world_view_aggregate_v1", "world_view_event_anchor_v1"] });
    scheduleHelloAckTimeout(ws);
    syncHostedSessionRefreshLoop();
    render();
  });

  ws.addEventListener("message", (event) => { if (socket !== ws) return;
    try {
      const message = JSON.parse(String(event.data || "null"));
      handleViewerMessage(message, ws);
    } catch (error) {
      reportFatalError(String(error), "viewer.parse");
    }
  });

  ws.addEventListener("error", () => { if (socket !== ws) return;
    reportFatalError("websocket error", "viewer.ws");
  });

  ws.addEventListener("close", () => { if (socket !== ws) return; invalidateAuthConnection(); cancelHostedLeaseRefresh(); worldFeedTransport.markDisconnected(ws);
    resetViewerProtocolForConnection();
    state.connectionStatus = "connecting";
    clearHostedRuntimeSyncTimer();
    if (state.auth.available && state.auth.source !== LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE) {
      state.auth.syncInFlight = false;
      state.auth.runtimeStatus = "disconnected";
    }
    worldViewTransport.clear();
    scopeSnapshotTransport.clear();
    clearPendingSessionRegisterWaiter("websocket disconnected during player session registration");
    failPendingAgentChatAck(
      "websocket disconnected before agent_chat ack/error returned",
      "agent_chat websocket disconnected",
    );
    failPendingPromptControlAck(
      "websocket disconnected before prompt_control ack/error returned",
      "prompt_control websocket disconnected",
    );
    failPendingGameplayActionAck(
      "websocket disconnected before gameplay_action ack/error returned",
      "gameplay_action websocket disconnected",
    );
    stopHostedSessionRefreshLoop();
    render();
    if (reconnectTimer) {
      window.clearTimeout(reconnectTimer);
    }
    clearHelloAckTimer();
    clearInitialSnapshotRetryTimer();
    reconnectTimer = window.setTimeout(connect, 1200);
  });
}

function connect() {
  invalidateAuthConnection();
  if (socket) {
    try {
      socket.close();
    } catch (_) {
    }
  }
  cancelHostedLeaseRefresh();
  state.wsUrl = initialWsUrl();
  state.connectionStatus = "connecting";
  render();
  socket = new WebSocket(state.wsUrl);
  attachSocket(socket);
}

function modelLists() {
  return buildViewerEntityLists({
    entityCollections,
    selectedSearch: state.selectedSearch,
    isAgentVisibleToCurrentSession,
  });
}

function buildTargetSyncProgress() {
  const { agents, locations } = entityCollections();
  const lists = modelLists();
  const snapshotReceived = currentConnectionSnapshotReceived;
  const serverReady = !!state.server;
  const sessionSyncing = !!state.auth.syncInFlight || !!pendingSessionRegisterWaiter;
  let stage = "connecting";
  if (state.connectionStatus === "connected") {
    if (initialSnapshotReadFailed) {
      stage = "error";
    } else if (!serverReady) {
      stage = "handshake";
    } else if (!snapshotReceived) {
      stage = "snapshot";
    } else if (sessionSyncing) {
      stage = "session";
    } else if (agents.length > 0 && lists.agents.length === 0) {
      stage = "visibility";
    } else {
      stage = "ready";
    }
  } else if (state.connectionStatus === "error") {
    stage = "error";
  }
  return {
    stage,
    connectionStatus: state.connectionStatus,
    serverReady,
    snapshotRequested: initialSnapshotRequested,
    snapshotRetryCount: initialSnapshotRetryCount,
    snapshotReceived,
    totalAgentCount: agents.length,
    visibleAgentCount: lists.agents.length,
    totalLocationCount: locations.length,
    visibleLocationCount: lists.locations.length,
    authAvailable: !!state.auth.available,
    authRuntimeStatus: state.auth.runtimeStatus || null,
    authRegistrationStatus: state.auth.registrationStatus || null,
    authSyncInFlight: sessionSyncing,
    pendingRequestedAgentId: state.auth.pendingRequestedAgentId || null,
    boundAgentId: state.auth.boundAgentId || null,
    lastError: state.lastError || state.auth.error || null,
  };
}

function connectionBadgeClass() {
  if (state.connectionStatus === "connected") return "badge badge--good";
  if (state.connectionStatus === "error") return "badge badge--bad";
  return "badge badge--warn";
}

function feedbackBadgeClass(feedback) {
  if (!feedback) return "badge";
  if (feedback.stage === "error") return "badge badge--bad";
  if (feedback.ok) return "badge badge--good";
  return "badge badge--warn";
}

function render(invalidation) {
  // Compatibility facade: every existing owner mutation still converges quote
  // dependencies before views observe it, including post-snapshot binding changes.
  application.coordinateAndPublish(invalidation);
}

function requestRender() {
  render();
}

function setStrongAuthApprovalCode(value) {
  state.strongAuth.approvalCode = String(value || "");
  render();
  return {
    ok: true,
    configured: !!state.strongAuth.approvalCode.trim(),
  };
}

function installTestApi() {
  if (__OASIS7_VISUAL_TEST__ !== true) return;
  if (!isTestApiEnabled()) {
    return;
  }
  window[TEST_API_GLOBAL_NAME] = {
    getState,
    describeControls,
    fillControlExample,
    sendControl,
    sendGameplayAction,
    requestRefineQuote,
    requestProductValidationQuote,
    requestPowerSurvivalQuote, requestFragmentRefillPreview, requestGovernanceVoteQuote, requestWarDeclarationQuote, requestPowerSaleQuote, requestScheduleRecipeQuote, requestTransferMaterialQuote,
    requestMarketQuoteDecision, injectMarketQuoteDecisionForTest,
    runSteps,
    setMode,
    focus,
    select,
    sendAgentChat,
    safeAlternative,
    sendPromptControl,
    setPromptOverridesVisible,
    togglePromptOverridesVisible,
    setStrongAuthApprovalCode,
    injectSnapshot,
    injectWorldFeedForTest(feed) {
      if (!isTestApiEnabled() || getSearchParams().get('connect') !== '0') throw new Error('feed fixture requires test_api=1&connect=0');
      worldFeedTransport.handleWorldFeed(clone(feed));
      render();
      return clone(state.worldFeed);
    },
    injectRefineQuotePreflightForTest,
    injectProductValidationQuoteForTest,
    injectPowerSaleQuoteForTest, injectPowerSurvivalQuoteForTest, injectWarDeclarationQuoteForTest, injectScheduleRecipeQuoteForTest, injectTransferMaterialQuoteForTest,
    logoutHostedPlayerSession,
    startHostedAccountLogin,
    completeHostedAccountLogin,
    startHostedTestLogin,
    retryHostedPlayerIdentityIssue,
    refreshPromptControlBinding,
    registerPlayerSessionForTest,
    expirePendingSessionRegisterWaiterForTest,
    expireHostedRuntimeSyncTimeoutForTest,
    expirePendingPromptControlAckTimeoutForTest,
    expirePendingGameplayActionAckTimeoutForTest,
    connectionGenerationForTest,
    offerBrowserRaceIdentityForTest: viewerBrowserRaceIdentityTestApi.offerBrowserRaceIdentityForTest, claimBrowserRaceIdentityForTest: viewerBrowserRaceIdentityTestApi.claimBrowserRaceIdentityForTest, connectBrowserRaceActorForTest: viewerBrowserRaceIdentityTestApi.connectBrowserRaceActorForTest,
    reportFatalError,
  };
}
const viewerBrowserRaceIdentityTestApi = createViewerBrowserRaceIdentityTestApi({ authHasSigningKeyMaterial, clone, connect, isTestApiEnabled, render, state,
  bumpRequestCounters() { requestId = Math.max(requestId, 1_000_000); authNonceCounter = Math.max(authNonceCounter, 1_000_000); } });

viewerControlLossModule = createViewerControlLossModule({ render, state });
viewerPromptControlModule = createViewerPromptControlModule({
  applyPromptAckLocally, assertPromptFeedbackActive, buildAuthEnvelope, buildPromptControlSigningPayload, clearPendingPromptControlAckTimer,
  clearPendingSessionRegisterWaiter, clone, createSemanticFeedback, ensureHostedPlayerAuthAvailable, ensureRegisteredPlayerSession,
  nextRequestId, nextAuthNonce, onControlLost: (agentId) => viewerControlLossModule?.markControlLost(agentId), render, requestSnapshotSafe, selectedAgentId, selectedAgentPromptProfile, signAuthPayload, state,
});

async function bootstrap() {
  state.uiLocale = resolveInitialUiLocale();
  state.promptOverridesVisible = resolveStoredPromptOverridesVisibility();
  applyUiLocaleToDocument(state.uiLocale);
  Object.assign(state, detectRendererMeta());
  state.hostedAccess = { ...(resolveHostedAccessHint() || {}), deployment_mode: viewerRuntimeConfig().deploymentMode };
  await installSession(state, resolveViewerAuthState());
  state.wsUrl = initialWsUrl();
  installRefineQuotePreflightVisualFixture();
  productValidationQuote.installProductValidationQuoteVisualFixture();
  powerSurvivalQuote.installPowerSurvivalQuoteVisualFixture(); scheduleRecipeQuote.installScheduleRecipeQuoteVisualFixture(); transferMaterialQuote.installTransferMaterialQuoteVisualFixture();
  window[RENDER_META_GLOBAL_NAME] = Object.freeze({
    renderMode: state.renderMode,
    rendererClass: state.rendererClass,
    viewerReason: state.viewerReason,
    softwareSafeReason: state.viewerReason,
    renderer: state.renderer,
    vendor: state.vendor,
    webglVersion: state.webglVersion,
  });
  installTestApi();
  render();
  const hostedBootstrap = shouldRunHostedBootstrap();
  if (hostedBootstrap) {
    void refreshHostedAdmissionState().then(() => render());
  }
  if (shouldConnectViewerWs()) {
    connect();
  } else {
    state.connectionStatus = "disconnected";
  }
  if (hostedBootstrap) {
    // Local key generation belongs to initialization; it must not outlive an
    // awaited bootstrap. Socket connection and hosted admission stay independent.
    await ensureHostedPlayerAuthAvailable();
    render();
  }
}

function updatePixelWorldRuntimeMeta(meta = {}) {
  if (!meta || typeof meta !== "object") {
    return getState();
  }
  if (Object.prototype.hasOwnProperty.call(meta, "runtimeStatus")) {
    state.pixelWorldRuntimeStatus = meta.runtimeStatus || "detached";
  }
  if (Object.prototype.hasOwnProperty.call(meta, "runtimeSource")) {
    state.pixelWorldRuntimeSource = meta.runtimeSource || "detached";
  }
  if (Object.prototype.hasOwnProperty.call(meta, "runtimeModuleUrl")) {
    state.pixelWorldRuntimeModuleUrl = meta.runtimeModuleUrl || null;
  }
  if (Object.prototype.hasOwnProperty.call(meta, "camera")) {
    state.pixelWorldCamera = clone(meta.camera || null);
  }
  if (Object.prototype.hasOwnProperty.call(meta, "fatal")) {
    state.pixelWorldFatal = clone(meta.fatal || null);
  }
  return getState();
}

export function initializeSoftwareSafeCore() {
  return application.start();
}

window.addEventListener("error", (event) => {
  const message = event?.message || event?.error?.message || "window error";
  reportFatalError(message, "window.error");
});
window.addEventListener("unhandledrejection", (event) => {
  const message = event?.reason?.message || String(event?.reason || "unhandled rejection");
  reportFatalError(message, "window.unhandledrejection");
});

export {
  applySelection,
  buildAuthSurfaceModel,
  buildGameplaySummary,
  buildHostedActionMatrixView,
  buildHostedRecoveryHint,
  buildTargetSyncProgress,
  buildWorldScaleSurface,
  clone,
  connectionBadgeClass,
  describeControls,
  describePromptVersionState,
  describeSemanticFeedback,
  entityCollections,
  focusModuleFromEvent,
  expireHostedRuntimeSyncTimeoutForTest,
  expirePendingAgentChatOverallTimeoutForTest,
  expirePendingSessionRegisterWaiterForTest,
  feedbackBadgeClass,
  fillControlExample,
  formatPhysicalDistanceCm,
  formatWorldPositionCm,
  focus,
  getState,
  handleControlCompletionAck,
  chatHistoryStorageKey,
  hostedActionPolicy,
  injectSnapshot,
  injectRefineQuotePreflightForTest,
  injectProductValidationQuoteForTest,
  injectPowerSaleQuoteForTest, injectPowerSurvivalQuoteForTest, injectWarDeclarationQuoteForTest, injectScheduleRecipeQuoteForTest, injectTransferMaterialQuoteForTest,
  isEmptyEntitySnapshotRefreshPendingForTest,
  isAgentChatInFlight,
  isAgentVisibleToCurrentSession,
  modelLists,
  needsEmptyEntitySnapshotRefreshForTest,
  pushChatHistory,
  refreshHostedAdmissionState,
  requestRender,
  reportFatalError,
  resourceSummary,
  startHostedAccountLogin,
  completeHostedAccountLogin,
  startHostedTestLogin,
  retryHostedPlayerIdentityIssue,
  refreshPromptControlBinding,
  registerPlayerSessionForTest,
  runSteps,
  select,
  selectedAgentBindingInfo,
  selectedAgentExecutionDebugContext,
  selectedAgentId,
  sendAgentChat,
  sendControl,
  sendGameplayAction,
  requestRefineQuote,
  requestProductValidationQuote,
  requestPowerSurvivalQuote, requestFragmentRefillPreview, requestGovernanceVoteQuote, requestWarDeclarationQuote, requestPowerSaleQuote, requestScheduleRecipeQuote, requestTransferMaterialQuote,
  requestMarketQuoteDecision, injectMarketQuoteDecisionForTest,
  sendPromptControl,
  setMode,
  setStrongAuthApprovalCode,
  expirePendingPromptControlAckTimeoutForTest,
  expirePendingGameplayActionAckTimeoutForTest,
  updatePixelWorldRuntimeMeta,
  snapshotControlFeedback,
  snapshotSemanticFeedback,
  summarizeEventTitle,
  logoutHostedPlayerSession,
};

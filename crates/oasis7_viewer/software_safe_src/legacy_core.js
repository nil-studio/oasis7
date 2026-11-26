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
import { createViewerHostedAuthLifecycleModule } from "./viewer_hosted_auth_lifecycle_module.js";
import { createViewerGameplayActionModule } from "./viewer_gameplay_action_module.js";
import { createViewerPromptRequestModule } from "./viewer_prompt_request_module.js";
import { createViewerSemanticQueueChatModule } from "./viewer_semantic_queue_chat_module.js";
import { createViewerAuthoritativeRecoveryModule } from "./viewer_authoritative_recovery_module.js";
import { createViewerWsTransportModule } from "./viewer_ws_transport_module.js";
import { createViewerLiveControlAndAuthProofModule } from "./viewer_live_control_and_auth_proof_module.js";
export const state = createSoftwareSafeState();
let socket = null;
let hostedSessionRefreshTimer = null;
let requestId = 0;
let authNonceCounter = 0;
let viewerPromptControlModule = null;
let viewerControlLossModule = null;
const pendingControlFeedback = new Map();
const liveControlState = createLiveControlStateController(state, pendingControlFeedback);
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
  canAutoIssueHostedPlayerSession, isLoopbackHostname, hostnameFromUrl,
  canAutoIssueLocalTestPlayerSession, issueLocalTestPlayerSession, startHostedAccountLogin,
  issueHostedPlayerIdentity, ensureHostedPlayerAuthAvailable, retryHostedPlayerIdentityIssue,
  requestHostedStrongAuthGrant, sendReconnectSync, probeHostedRuntimeSession, releaseHostedPlayerSlot,
} = createViewerHostedAuthLifecycleModule({
  state, LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE, HOSTED_ACCOUNT_LOGIN_START_ROUTE,
  HOSTED_STRONG_AUTH_GRANT_ROUTE, HOSTED_PLAYER_SESSION_RELEASE_ROUTE,
  isHostedPublicJoinDeploymentMode, initialWsUrl,
  resolveStoredLocalTestPlayerSession: () => resolveStoredLocalTestPlayerSession(),
  installSession, generateEphemeralEd25519Keypair,
  getAuthNonceCounter: () => authNonceCounter,
  persistLocalTestPlayerSession: (auth) => persistLocalTestPlayerSession(auth),
  maybeRecoverLocalTestStarterBindingFromSnapshot: (snapshot) => maybeRecoverLocalTestStarterBindingFromSnapshot(snapshot),
  completeHostedAccountLogin: () => completeHostedAccountLogin(),
  ensureHostedAuthSigningKey, authCredentials,
  scheduleHostedRuntimeSyncTimeout: () => scheduleHostedRuntimeSyncTimeout(),
  sendJson, clone, render,
});
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
  buildSessionRegisterAuthProof: (...args) => buildSessionRegisterAuthProof(...args), sendJson,
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
  onSnapshotReceived: () => markSnapshotReceived(),
  clearInitialSnapshotRetryTimer: () => clearInitialSnapshotRetryTimer(), quoteProtocolFacade, recoverGameplayFeedback, hostedWorldAccount,
  getGameplayRecoveryScope, clearPendingGameplayActionAckTimer, requestGameplayResult, sendJson,
  entityCollections, applySelection, syncAgentInteractionDrafts, hydrateChatHistoryFromStorage,
  reconcilePendingPromptAuthoritativeRefresh, canonicalOwnerRead, worldFeedTransport, isTestApiEnabled,
  getSocket: () => socket, requestSnapshotSafe: () => requestSnapshotSafe(), persistHostedPlayerSession,
  hasPendingSessionRegisterWaiter: () => Boolean(pendingSessionRegisterWaiter),
  ensureRegisteredPlayerSession: (...args) => ensureRegisteredPlayerSession(...args),
  clone, render, getState, acceptRuntimeAckIdentity: (ack) => acceptRuntimeAckIdentity(ack),
  pendingControlFeedback, liveControlState,
});

const {
  sendControl, runSteps, setMode,
  buildAgentChatAuthProof, buildPromptControlAuthProof, buildPromptRollbackAuthProof,
  buildSessionRegisterAuthProof, buildGameplayActionAuthProof, requestRefineQuote,
} = createViewerLiveControlAndAuthProofModule({
  state, getSocket: () => socket, getViewerPromptControlModule: () => viewerPromptControlModule,
  viewerAgentChatAuthModule, pendingControlFeedback, liveControlState,
  nextRequestId, nextAuthNonce, parseStepCount, snapshotControlFeedback,
  normalizedGameplayActions, gameplayProtocolAction,
  sendJson, render: () => render(), clone,
  ensureHostedPlayerAuthAvailable, ensureRegisteredPlayerSession,
});

async function refreshPromptControlBinding() {
  return viewerPromptControlModule?.refreshBinding()
    || { ok: false, reason: "prompt control module is not ready" };
}

const {
  resetHostedPlayerAuthState, logoutHostedPlayerSession, markHostedSessionRefreshFailure,
  handleAuthoritativeRecoveryAck, handleAuthoritativeRecoveryError,
} = createViewerAuthoritativeRecoveryModule({
  state, getPendingSessionRegisterWaiter: () => pendingSessionRegisterWaiter,
  getViewerControlLossModule: () => viewerControlLossModule, hostedWorldAccount,
  cancelHostedLeaseRefresh, clearPendingSessionRegisterWaiter, stopHostedSessionRefreshLoop,
  clearHostedPlayerSession, resolveAuthBootstrap, refreshHostedAdmissionState,
  releaseHostedPlayerSlot, sendJson: (...args) => sendJson(...args), render: () => render(),
  acceptRuntimeAckIdentity, clearHostedRuntimeSyncTimer, persistHostedPlayerSession,
  refreshHostedPlayerLease, syncHostedSessionRefreshLoop,
  resolvePendingSessionRegisterWaiterAfterRuntimeAck, maybeRecoverLocalTestStarterBindingFromSnapshot,
  requestSnapshotSafe: () => requestSnapshotSafe(), canAutoIssueHostedPlayerSession, isHostedAuthExpired,
  recoveryErrorRequiresExplicitRebind, ensureRegisteredPlayerSession, latestRequestedAgentId,
  issueHostedPlayerIdentity, rebindAgentIdFromRecoveryError,
  markPendingSemanticRebind: (...args) => markPendingSemanticRebind(...args),
  retryPendingSessionRegisterWaiterWithForceRebind,
});

const { syncHostedPlayerSessionOnConnect } = createViewerHostedSessionReconnectModule({
  authHasSigningKeyMaterial,
  legacyViewerAuthBootstrapSource: LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE,
  onRefreshFailure: markHostedSessionRefreshFailure,
  refreshHostedPlayerLease,
  registerHostedPlayerSession: () => ensureRegisteredPlayerSession(latestRequestedAgentId()),
  sendReconnectSync,
  state,
});

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

const {
  enqueueSemanticCommand, assertSemanticCapability, assertAgentChatFeedbackActive,
  assertPromptFeedbackActive, sendAgentChat, handleAgentChatAck, handleAgentChatError,
} = createViewerSemanticQueueChatModule({
  state, getSocket: () => socket, getViewerControlLossModule: () => viewerControlLossModule,
  canonicalOwnerRead, agentChatOverallTimeoutMs: AGENT_CHAT_OVERALL_TIMEOUT_MS,
  clone, render: () => render(), sendJson: (...args) => sendJson(...args),
  requestSnapshotSafe: () => requestSnapshotSafe(), selectedAgentId, currentBoundAgentControlError,
  buildSemanticCapability, isAgentChatInFlight, sameAgentChatFeedback, agentChatFeedbackInFlight,
  sameSemanticFeedback, semanticFeedbackInFlight, markSemanticFeedbackError,
  createSemanticFeedback, snapshotSemanticFeedback,
  expireAgentChatOverallTimeout, scheduleAgentChatOverallTimeout, scheduleAgentChatAckTimeout,
  clearPendingAgentChatAckTimer, clearPendingAgentChatOverallTimer, setPendingAgentChatAckTimer,
  failPendingAgentChatAck, clearPendingPromptControlAckTimer,
  ensureHostedPlayerAuthAvailable: (...args) => ensureHostedPlayerAuthAvailable(...args),
  ensureRegisteredPlayerSession: (...args) => ensureRegisteredPlayerSession(...args),
  buildAgentChatAuthProof, acceptRuntimeAckIdentity, pushChatHistory,
});

const { sendPromptControl, applyPromptAckLocally, handlePromptControlAck, handlePromptControlError } = createViewerPromptRequestModule({
  state, getViewerPromptControlModule: () => viewerPromptControlModule,
  semanticActionOverallTimeoutMs: SEMANTIC_ACTION_OVERALL_TIMEOUT_MS,
  selectedAgentId, selectedAgentPromptProfile, currentBoundAgentControlError,
  promptControlCapabilitySelected, promptControlReadinessError, attachPromptControlIdentity,
  promptDraftValuesForRequest, nextRequestId, createSemanticFeedback, snapshotSemanticFeedback,
  enqueueSemanticCommand, ensureHostedPlayerAuthAvailable: (...args) => ensureHostedPlayerAuthAvailable(...args),
  assertPromptFeedbackActive, assertSemanticCapability,
  ensureRegisteredPlayerSession: (...args) => ensureRegisteredPlayerSession(...args),
  requestHostedStrongAuthGrant: (...args) => requestHostedStrongAuthGrant(...args),
  buildPromptControlAuthProof, buildPromptRollbackAuthProof,
  sendJson: (...args) => sendJson(...args), schedulePromptControlAckTimeout,
  acceptRuntimeAckIdentity, render: () => render(),
});

const { sendGameplayAction, handleGameplayActionAck, handleGameplayActionError } = createViewerGameplayActionModule({
  state, getSocket: () => socket, hostedWorldAccount, quoteProtocolFacade, clone, nextRequestId,
  render: () => render(), sendJson: (...args) => sendJson(...args), requestSnapshotSafe: () => requestSnapshotSafe(),
  sendControl: (...args) => sendControl(...args), currentBoundAgentControlError, sameSemanticFeedback,
  snapshotSemanticFeedback, createSemanticFeedback,
  ensureHostedPlayerAuthAvailable: (...args) => ensureHostedPlayerAuthAvailable(...args),
  assertSemanticCapability: (...args) => assertSemanticCapability(...args),
  ensureRegisteredPlayerSession: (...args) => ensureRegisteredPlayerSession(...args),
  buildGameplayActionAuthProof: (...args) => buildGameplayActionAuthProof(...args),
  scheduleGameplayActionAckTimeout, clearPendingGameplayActionAckTimer,
  markCurrentGameplayActionFeedbackError: (...args) => markCurrentGameplayActionFeedbackError(...args),
  acceptRuntimeAckIdentity, isHostedAuthExpired,
});

const worldViewTransport = createWorldViewTransport({ send: sendJson, getScope: () => worldViewScopeKey(state), getConnection: () => socket });
export const {request: requestWorldView, statistics: readWorldViewTransportStatistics} = worldViewTransport;
const scopeSnapshotTransport = createScopeSnapshotTransport({send:sendJson,getScope:()=>worldViewScopeKey(state),getConnection:()=>socket});
export const requestScopeSnapshot = () => scopeSnapshotTransport.request();
// Host authorizes the captured scope before this existing adopter triggers render.
export const acceptScopeSnapshot = snapshot => handleSnapshot(snapshot);
const {
  clearInitialSnapshotRetryTimer, markSnapshotReceived, getInitialSnapshotState, handleViewerMessage, connect,
} = createViewerWsTransportModule({
  state, getSocket: () => socket, setSocket: (ws) => { socket = ws; },
  getViewerPromptControlModule: () => viewerPromptControlModule,
  helloAckTimeoutMs: HELLO_ACK_TIMEOUT_MS, initialSnapshotRetryDelayMs: INITIAL_SNAPSHOT_RETRY_DELAY_MS,
  initialSnapshotSlowRetryDelayMs: INITIAL_SNAPSHOT_SLOW_RETRY_DELAY_MS,
  initialSnapshotSlowRetryAfter: INITIAL_SNAPSHOT_SLOW_RETRY_AFTER,
  promptControlResultCapability: PROMPT_CONTROL_RESULT_CAPABILITY,
  sendJson, initialWsUrl, render: (inv) => render(inv), reportFatalError, resetViewerProtocolForConnection,
  worldFeedTransport, worldViewTransport, scopeSnapshotTransport, quoteProtocolFacade,
  liveControlState, hostedWorldAccount, safeAlternative, canonicalOwnerRead,
  hydrateChatHistoryFromStorage, ensureHostedPlayerAuthAvailable, syncHostedPlayerSessionOnConnect,
  handleSnapshot, addRecentEvent, extractAgentSpokeEntry, pushChatHistory,
  handleMetrics, handleDecisionTrace, handleControlCompletionAck,
  handlePromptControlAck, handlePromptControlError, handleAgentChatAck, handleAgentChatError,
  clearPendingGameplayActionAckTimer, requestSnapshotSafe,
  handleGameplayActionAck, handleGameplayActionError,
  handleAuthoritativeRecoveryAck, handleAuthoritativeRecoveryError,
  cancelHostedLeaseRefresh, clearHostedRuntimeSyncTimer, clearPendingSessionRegisterWaiter,
  failPendingAgentChatAck, failPendingPromptControlAck, failPendingGameplayActionAck,
  stopHostedSessionRefreshLoop, syncHostedSessionRefreshLoop,
});

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
  const { snapshotReceived, snapshotReadFailed, snapshotRequested, snapshotRetryCount } = getInitialSnapshotState();
  const serverReady = !!state.server;
  const sessionSyncing = !!state.auth.syncInFlight || !!pendingSessionRegisterWaiter;
  let stage = "connecting";
  if (state.connectionStatus === "connected") {
    if (snapshotReadFailed) {
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
    snapshotRequested,
    snapshotRetryCount,
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

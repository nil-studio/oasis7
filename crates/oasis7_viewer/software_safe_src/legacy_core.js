import { safeAlternativeMarkup, bindSafeAlternativeLegacy } from "./viewer_safe_alternative_legacy.js";
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
import { scheduleCanonicalGoalPending, canonicalGoalMarkup, applyCommittedChatAck, canonicalAckMatches } from "./viewer_canonical_goal_module.js";
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
import { createViewerRenderHookRegistry } from "./viewer_render_hook_registry.js";
import { buildViewerEntityLists, renderViewerEntityList, resourceSummary } from "./viewer_entity_list_renderer.js";
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
export const state = createSoftwareSafeState();
let socket = null;
let reconnectTimer = null;
let helloAckTimer = null;
let initialSnapshotRequested = false;
let initialSnapshotRetryTimer = null;
let initialSnapshotRetryCount = 0;
let emptyEntitySnapshotRefreshTimer = null;
let hostedSessionRefreshTimer = null;
let hostedRuntimeSyncTimer = null;
let pendingAgentChatAckTimer = null;
let pendingAgentChatOverallTimer = null;
let pendingPromptControlAckTimer = null;
let pendingGameplayActionAckTimer = null;
let firstAgentClaimAutoAdvanceTimer = null;
let firstAgentClaimAutoRefreshTimer = null;
let requestId = 0;
let authNonceCounter = 0;
let localTestPlayerSessionInFlight = null;
let viewerPromptControlModule = null;
let viewerControlLossModule = null;
let semanticSendLoop = null;
const pendingControlFeedback = new Map();
const pendingSemanticCommands = [];
let pendingSessionRegisterWaiter = null;
const elements = {};
const renderHook = createViewerRenderHookRegistry();
let bootstrapped = false;
let bootstrapPromise = null; const worldFeedTransport = createWorldFeedTransport({ getSocket: () => socket, getState: () => state, render, requestSnapshot: () => requestSnapshotSafe(), sendJson });
export const requestWorldFeed = (...args) => worldFeedTransport.requestWorldFeed(...args); export const reloadWorldFeedFromAuthoritativeSnapshot = (...args) => worldFeedTransport.reloadWorldFeedFromAuthoritativeSnapshot(...args);
const HELLO_ACK_TIMEOUT_MS = 2000; const INITIAL_SNAPSHOT_RETRY_DELAY_MS = 1000; const INITIAL_SNAPSHOT_SLOW_RETRY_AFTER = 5;
const INITIAL_SNAPSHOT_SLOW_RETRY_DELAY_MS = 5000; const EMPTY_ENTITY_SNAPSHOT_REFRESH_DELAY_MS = 2500;
const FIRST_AGENT_CLAIM_AUTO_ADVANCE_DELAY_MS = 450; const FIRST_AGENT_CLAIM_AUTO_REFRESH_DELAY_MS = 1200;
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
let localTestStarterRebindAttemptKey = null;
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
export const toggleSoftwareSafeLocale = toggleViewerLocale;
export function getSelectedSearch() {
  return state.selectedSearch;
}
export function setSelectedSearch(value) {
  state.selectedSearch = String(value || "");
  render();
}
export const setRenderHook = (nextHook) => renderHook.set(nextHook);
export const subscribeRenderHook = (nextHook) => renderHook.subscribe(nextHook);
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
function normalizeU64Display(value) {
  if (value == null) {
    return null;
  }
  if (typeof value === "bigint") {
    return value >= 0n ? value.toString() : `invalid_u64(${value.toString()})`;
  }
  if (typeof value === "number") {
    if (Number.isSafeInteger(value) && value >= 0) {
      return String(value);
    }
    return `unsafe_u64(${String(value)})`;
  }
  const text = String(value).trim();
  if (!text) {
    return null;
  }
  return /^\d+$/.test(text) ? text : `invalid_u64(${text})`;
}
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
const hostedWorldAccount = createHostedWorldAccount({ state, generateKeypair: generateEphemeralEd25519Keypair, register: ensureRegisteredPlayerSession, render, clearLegacy: clearHostedPlayerSession });
export const joinHostedAccountWorld = (...args) => hostedWorldAccount.join(...args);
export const disableLegacyHostedSession = () => hostedWorldAccount.disableLegacy();
export const releaseHostedAccountWorld = () => hostedWorldAccount.release();
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

function resetViewerProtocolForConnection() { canonicalOwnerRead.reset(); viewerPromptControlModule?.resetForConnection(); }
function promptControlCapabilitySelected() { return viewerPromptControlModule?.capabilitySelected() === true; }
function promptControlResultSelected() { return viewerPromptControlModule?.resultSelected() === true; }
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

function parseSelectionPayload(payload) {
  if (payload == null) {
    return null;
  }
  if (typeof payload === "string") {
    const trimmed = payload.trim();
    if (!trimmed) return null;
    const parts = trimmed.split(":");
    if (parts.length >= 2) {
      return { kind: parts[0], id: parts.slice(1).join(":") };
    }
    return { kind: "agent", id: trimmed };
  }
  if (typeof payload === "object") {
    const kind = payload.kind || payload.targetKind || payload.type;
    const id = payload.id || payload.targetId || payload.value;
    if (!kind || !id) return null;
    return { kind: String(kind), id: String(id) };
  }
  return null;
}

function entityCollections() {
  const model = state.snapshot?.model || {};
  return {
    agents: Object.values(model.agents || {}),
    locations: Object.values(model.locations || {}),
    moduleVisualEntities: Object.values(model.module_visual_entities || {}).map((entry) => normalizeModuleVisualEntity(entry)).filter(Boolean),
  };
}
function normalizeModuleVisualEntity(entry, fallbackId = "") { if (!entry || typeof entry !== "object") return null; const id = String(entry.entity_id || entry.id || fallbackId || "").trim(); if (!id) return null; return { ...entry, id, module_id: String(entry.module_id || entry.moduleId || "").trim(), kind: String(entry.kind || "artifact").trim() || "artifact", label: entry.label == null ? null : String(entry.label).trim() || null, anchor: entry.anchor || null }; }
function moduleVisualEntityIdFromEvent(event) { if (!event || typeof event !== "object" || typeof event.module_visual_entity_id !== "string") return null; const id = event.module_visual_entity_id.trim(); return id || null; }
function moduleVisualEntityFromEvent(event) { const id = moduleVisualEntityIdFromEvent(event); return id ? entityCollections().moduleVisualEntities.find((entry) => entry.id === id) || null : null; }
function focusModuleFromEvent(event) { const target = moduleVisualEntityFromEvent(event); return target ? applySelection({ kind: "module_visual", id: target.id }) : null; }

function agentBindingForId(agentId) {
  const id = String(agentId || "").trim();
  if (!id) {
    return { playerId: null, publicKey: null };
  }
  return {
    playerId: state.snapshot?.model?.agent_player_bindings?.[id] || null,
    publicKey: state.snapshot?.model?.agent_player_public_key_bindings?.[id] || null,
  };
}

function isAgentVisibleToCurrentSession(agentId) {
  const id = String(agentId || "").trim();
  if (!id) {
    return false;
  }
  const boundAgentId = String(state.auth.boundAgentId || "").trim();
  const currentPlayerId = String(state.auth.playerId || "").trim();
  const binding = agentBindingForId(id);
  const boundPlayerId = String(binding.playerId || "").trim();
  if (boundAgentId && id === boundAgentId) {
    return true;
  }
  if (boundPlayerId && currentPlayerId && boundPlayerId === currentPlayerId) {
    return true;
  }
  if (
    id === STARTER_AGENT_ID
    && isTestApiEnabled()
    && state.auth.source === "local_test_api_ephemeral"
    && boundPlayerId.startsWith(LOCAL_TEST_PLAYER_ID_PREFIX)
  ) {
    return true;
  }
  return false;
}

function currentBoundAgentControlError(agentId, actionLabel = "agent action") {
  const id = String(agentId || "").trim();
  if (!id) {
    return `${actionLabel} requires a non-empty agent id`;
  }
  const boundAgentId = String(state.auth.boundAgentId || "").trim();
  if (!boundAgentId) {
    return `${actionLabel} requires the current account to have a bound Agent`;
  }
  if (id !== boundAgentId) {
    return `${actionLabel} target ${id} does not match current bound Agent ${boundAgentId}`;
  }
  if (viewerControlLossModule?.isControlLost(id)) {
    return `${actionLabel} control was lost; re-authenticate and refresh the current Agent binding`;
  }
  return null;
}

function isAgentControlLost(agentId) {
  return viewerControlLossModule?.isControlLost(agentId) === true;
}

function selectedAgentId() {
  return state.selectedKind === "agent" ? state.selectedId : null;
}

function selectedAgentPromptProfile() {
  const agentId = selectedAgentId();
  if (!agentId) {
    return null;
  }
  return state.snapshot?.model?.agent_prompt_profiles?.[agentId] || {
    agent_id: agentId,
    version: 0,
    updated_at_tick: 0,
    updated_by: "",
    system_prompt_override: null,
    short_term_goal_override: null,
    long_term_goal_override: null,
  };
}

function selectedAgentBindingInfo() {
  const agentId = selectedAgentId();
  if (!agentId) {
    return null;
  }
  return agentBindingForId(agentId);
}

function selectedAgentExecutionDebugContext() {
  const agentId = selectedAgentId();
  if (!agentId) {
    return null;
  }
  return state.snapshot?.model?.agent_execution_debug_contexts?.[agentId] || null;
}

function syncAgentInteractionDrafts(force = false) {
  const agentId = selectedAgentId();
  const profile = selectedAgentPromptProfile();
  if (force || state.promptDraft.agentId !== agentId || (!state.promptDraft.dirty && agentId)) {
    const currentVersion = Number(profile?.version || 0);
    state.promptDraft = {
      agentId,
      currentVersion,
      rollbackTargetVersion: Math.max(0, currentVersion - 1),
      updatedBy: String(profile?.updated_by || ""),
      updatedAtTick: Number(profile?.updated_at_tick || 0),
      systemPrompt: String(profile?.system_prompt_override || ""),
      shortTermGoal: String(profile?.short_term_goal_override || ""),
      longTermGoal: String(profile?.long_term_goal_override || ""),
      dirty: false,
    };
  }
  if (force || state.chatDraft.agentId !== agentId) {
    state.chatDraft = {
      agentId,
      message: agentId === state.chatDraft.agentId ? state.chatDraft.message : "",
      dirty: false,
    };
  }
}

function promptDraftValuesForRequest(request) {
  return viewerPromptControlModule?.draftValuesForRequest(request) || null;
}

function reconcilePendingPromptAuthoritativeRefresh(snapshot) {
  return viewerPromptControlModule?.reconcilePendingAuthoritativeRefresh(snapshot) === true;
}

function applySelection(selection, options = {}) {
  if (!selection) return null;
  const kind = String(selection.kind || "").toLowerCase();
  const id = String(selection.id || "");
  const { agents, locations, moduleVisualEntities } = entityCollections();
  let object = null;
  if (kind === "agent") {
    object = agents.find((entry) => entry.id === id) || null;
  } else if (kind === "location") {
    object = locations.find((entry) => entry.id === id) || null;
  } else if (kind === "module_visual") {
    object = moduleVisualEntities.find((entry) => entry.id === id) || null;
  }
  if (!object) {
    return null;
  }
  state.selectedKind = kind;
  state.selectedId = id;
  state.selectedObject = object;
  syncAgentInteractionDrafts(options.preserveInteractionDrafts !== true);
  render();
  return { kind, id };
}

function select(payload) {
  const parsed = parseSelectionPayload(payload);
  if (!parsed) {
    return { ok: false, reason: "invalid selection payload" };
  }
  const applied = applySelection(parsed);
  if (!applied) {
    return { ok: false, reason: `target not found: ${parsed.kind}:${parsed.id}` };
  }
  return { ok: true, ...applied };
}

function focus(payload) {
  return select(payload);
}

function parseStepCount(payload) {
  if (payload == null) return 1;
  if (typeof payload === "number" && Number.isFinite(payload) && payload >= 1) {
    return Math.floor(payload);
  }
  if (typeof payload === "string") {
    const trimmed = payload.trim();
    if (!trimmed || trimmed === "step") return 1;
    const numeric = Number(trimmed);
    if (Number.isFinite(numeric) && numeric >= 1) {
      return Math.floor(numeric);
    }
    const matched = trimmed.match(/step\s*[:=]\s*(\d+)/i);
    if (matched) {
      return Number(matched[1]);
    }
    return null;
  }
  if (typeof payload === "object") {
    const numeric = Number(payload.count);
    if (Number.isFinite(numeric) && numeric >= 1) {
      return Math.floor(numeric);
    }
  }
  return null;
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

function clearPendingAgentChatAckTimer() {
  if (pendingAgentChatAckTimer) {
    window.clearTimeout(pendingAgentChatAckTimer);
    pendingAgentChatAckTimer = null;
  }
}

function clearPendingAgentChatOverallTimer() {
  if (pendingAgentChatOverallTimer) {
    window.clearTimeout(pendingAgentChatOverallTimer);
    pendingAgentChatOverallTimer = null;
  }
}

function clearPendingPromptControlAckTimer() {
  if (pendingPromptControlAckTimer) {
    window.clearTimeout(pendingPromptControlAckTimer);
    pendingPromptControlAckTimer = null;
  }
}

function clearPendingGameplayActionAckTimer() {
  if (pendingGameplayActionAckTimer) {
    window.clearTimeout(pendingGameplayActionAckTimer);
    pendingGameplayActionAckTimer = null;
  }
}

function clearHostedRuntimeSyncTimer() {
  if (hostedRuntimeSyncTimer) {
    window.clearTimeout(hostedRuntimeSyncTimer);
    hostedRuntimeSyncTimer = null;
  }
}

function expireHostedRuntimeSyncTimeout() {
  hostedRuntimeSyncTimer = null;
  if (!state.auth.syncInFlight || pendingSessionRegisterWaiter) {
    return;
  }
  state.auth.syncInFlight = false;
  state.auth.registrationStatus = state.auth.available ? "issued" : "guest";
  state.auth.runtimeStatus = "error";
  state.auth.recoveryErrorCode = "runtime_sync_timeout";
  state.auth.recoveryErrorMessage = "runtime session sync timed out waiting for ack/error from live server";
  state.auth.error = state.auth.recoveryErrorMessage;
  maybeRecoverLocalTestStarterBindingFromSnapshot(state.snapshot);
  render();
}

function scheduleHostedRuntimeSyncTimeout() {
  clearHostedRuntimeSyncTimer();
  hostedRuntimeSyncTimer = window.setTimeout(
    expireHostedRuntimeSyncTimeout,
    SESSION_REGISTER_ACK_TIMEOUT_MS,
  );
}

function expireHostedRuntimeSyncTimeoutForTest() {
  if (!isTestApiEnabled()) {
    throw new Error("expireHostedRuntimeSyncTimeoutForTest requires test_api=1");
  }
  clearHostedRuntimeSyncTimer();
  expireHostedRuntimeSyncTimeout();
}

function agentChatFeedbackInFlight(feedback) {
  return feedback && ["queued", "registering", "signing", "sent", "pending"].includes(String(feedback.stage || ""));
}

function semanticFeedbackInFlight(feedback) {
  return feedback && ["queued", "registering", "authorizing", "signing", "sent"].includes(String(feedback.stage || ""));
}

function sameAgentChatFeedback(left, right) {
  if (!left || !right) {
    return false;
  }
  if (left === right) {
    return true;
  }
  return left.id === right.id
    && left.kind === right.kind
    && left.action === right.action
    && left.agentId === right.agentId;
}

function sameSemanticFeedback(left, right) {
  if (!left || !right) {
    return false;
  }
  if (left === right) {
    return true;
  }
  return left.id === right.id
    && left.kind === right.kind
    && left.action === right.action
    && left.agentId === right.agentId;
}

function isAgentChatInFlight() {
  return agentChatFeedbackInFlight(state.lastChatFeedback);
}

function markAgentChatFeedbackError(feedback, reason, effect = "agent_chat failed") {
  if (!feedback) {
    return;
  }
  feedback.stage = feedback.canonicalRequest ? "unknown" : "error";
  feedback.ok = false;
  feedback.accepted = false;
  feedback.reason = reason;
  feedback.effect = feedback.canonicalRequest ? "canonical goal result unknown; refresh current world state" : effect;
  state.lastChatFeedback = feedback;
}

function expireAgentChatOverallTimeout(feedback) {
  if (!sameAgentChatFeedback(state.lastChatFeedback, feedback) || !agentChatFeedbackInFlight(state.lastChatFeedback)) {
    return false;
  }
  clearPendingAgentChatOverallTimer();
  clearPendingAgentChatAckTimer();
  markAgentChatFeedbackError(
    feedback,
    "agent_chat timed out before live server ack/error completed",
    "agent_chat overall timeout",
  );
  render();
  return true;
}

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

function scheduleAgentChatOverallTimeout(feedback) {
  clearPendingAgentChatOverallTimer();
  pendingAgentChatOverallTimer = window.setTimeout(() => {
    pendingAgentChatOverallTimer = null;
    expireAgentChatOverallTimeout(feedback);
  }, AGENT_CHAT_OVERALL_TIMEOUT_MS);
}

function expirePendingAgentChatOverallTimeoutForTest() {
  if (!isTestApiEnabled()) {
    throw new Error("expirePendingAgentChatOverallTimeoutForTest requires test_api=1");
  }
  clearPendingAgentChatOverallTimer();
  return expireAgentChatOverallTimeout(state.lastChatFeedback);
}

function scheduleAgentChatAckTimeout(feedback) {
  clearPendingAgentChatAckTimer();
  pendingAgentChatAckTimer = window.setTimeout(() => {
    pendingAgentChatAckTimer = null;
    if (!sameAgentChatFeedback(state.lastChatFeedback, feedback) || !["sent", "pending"].includes(state.lastChatFeedback.stage)) {
      return;
    }
    clearPendingAgentChatOverallTimer();
    markAgentChatFeedbackError(
      feedback,
      "agent_chat timed out waiting for ack/error from live server",
      "agent_chat ack timeout",
    );
    render();
  }, AGENT_CHAT_ACK_TIMEOUT_MS);
}

function failPendingAgentChatAck(reason, effect = "agent_chat ack failed") {
  clearPendingAgentChatAckTimer();
  clearPendingAgentChatOverallTimer();
  const feedback = state.lastChatFeedback;
  if (!agentChatFeedbackInFlight(feedback)) {
    return;
  }
  markAgentChatFeedbackError(feedback, reason, effect);
}

function markSemanticFeedbackError(feedback, reason, effect) {
  if (!feedback) {
    return;
  }
  feedback.stage = "error";
  feedback.ok = false;
  feedback.accepted = false;
  feedback.reason = reason;
  feedback.effect = effect;
}

function expirePromptControlAckTimeout(feedback) {
  if (!sameSemanticFeedback(state.lastPromptFeedback, feedback) || state.lastPromptFeedback.stage !== "sent") {
    return false;
  }
  clearPendingPromptControlAckTimer();
  markSemanticFeedbackError(
    feedback,
    "prompt_control timed out waiting for ack/error from live server",
    "prompt_control ack timeout",
  );
  state.lastPromptFeedback = feedback;
  render();
  return true;
}

function expireGameplayActionAckTimeout(feedback) {
  if (!sameSemanticFeedback(state.lastGameplayActionFeedback, feedback) || state.lastGameplayActionFeedback.stage !== "sent") {
    return false;
  }
  clearPendingGameplayActionAckTimer();
  markSemanticFeedbackError(
    feedback,
    "gameplay_action timed out waiting for ack/error from live server",
    "gameplay_action ack timeout",
  );
  state.lastGameplayActionFeedback = feedback;
  render();
  return true;
}

function schedulePromptControlAckTimeout(feedback) {
  clearPendingPromptControlAckTimer();
  pendingPromptControlAckTimer = window.setTimeout(() => {
    pendingPromptControlAckTimer = null;
    expirePromptControlAckTimeout(feedback);
  }, SEMANTIC_ACTION_ACK_TIMEOUT_MS);
}

function scheduleGameplayActionAckTimeout(feedback) {
  clearPendingGameplayActionAckTimer();
  pendingGameplayActionAckTimer = window.setTimeout(() => {
    pendingGameplayActionAckTimer = null;
    expireGameplayActionAckTimeout(feedback);
  }, SEMANTIC_ACTION_ACK_TIMEOUT_MS);
}

function expirePendingPromptControlAckTimeoutForTest() {
  if (!isTestApiEnabled()) {
    throw new Error("expirePendingPromptControlAckTimeoutForTest requires test_api=1");
  }
  clearPendingPromptControlAckTimer();
  return expirePromptControlAckTimeout(state.lastPromptFeedback);
}

function expirePendingGameplayActionAckTimeoutForTest() {
  if (!isTestApiEnabled()) {
    throw new Error("expirePendingGameplayActionAckTimeoutForTest requires test_api=1");
  }
  clearPendingGameplayActionAckTimer();
  return expireGameplayActionAckTimeout(state.lastGameplayActionFeedback);
}

function failPendingPromptControlAck(reason, effect = "prompt_control ack failed") {
  clearPendingPromptControlAckTimer();
  const feedback = state.lastPromptFeedback;
  if (!feedback || feedback.stage !== "sent") {
    return;
  }
  markSemanticFeedbackError(feedback, reason, effect);
  state.lastPromptFeedback = feedback;
}

function failPendingGameplayActionAck(reason, effect = "gameplay_action ack failed") {
  clearPendingGameplayActionAckTimer();
  const feedback = state.lastGameplayActionFeedback;
  if (!feedback || feedback.stage !== "sent") {
    return;
  }
  markSemanticFeedbackError(feedback, reason, effect);
  state.lastGameplayActionFeedback = feedback;
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
  if (state.snapshot) {
    return;
  }
  const retryDelay =
    initialSnapshotRetryCount >= INITIAL_SNAPSHOT_SLOW_RETRY_AFTER
      ? INITIAL_SNAPSHOT_SLOW_RETRY_DELAY_MS
      : INITIAL_SNAPSHOT_RETRY_DELAY_MS;
  initialSnapshotRetryTimer = window.setTimeout(() => {
    initialSnapshotRetryTimer = null;
    if (state.snapshot || !socket || socket.readyState !== WebSocket.OPEN) {
      return;
    }
    initialSnapshotRetryCount += 1;
    try {
      sendInitialSnapshotRequest();
      scheduleInitialSnapshotRetry();
    } catch (_) {
    }
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

function clearFirstAgentClaimAutoAdvanceTimers() {
  if (firstAgentClaimAutoAdvanceTimer != null) {
    window.clearTimeout(firstAgentClaimAutoAdvanceTimer);
    firstAgentClaimAutoAdvanceTimer = null;
  }
  if (firstAgentClaimAutoRefreshTimer != null) {
    window.clearTimeout(firstAgentClaimAutoRefreshTimer);
    firstAgentClaimAutoRefreshTimer = null;
  }
}

function scheduleFirstAgentClaimAutoAdvance() {
  if (!isTestApiEnabled()) {
    return;
  }
  clearFirstAgentClaimAutoAdvanceTimers();
  firstAgentClaimAutoAdvanceTimer = window.setTimeout(() => {
    firstAgentClaimAutoAdvanceTimer = null;
    const currentRequestId = nextRequestId();
    const feedback = {
      id: currentRequestId,
      action: "step",
      accepted: true,
      stage: "queued",
      reason: null,
      hint: "auto-advancing after first-agent claim ack",
      effect: "queued first-agent claim auto-advance",
      baselineLogicalTime: state.logicalTime,
      baselineEventSeq: state.eventSeq,
      deltaLogicalTime: 0,
      deltaEventSeq: 0,
      deltaTraceCount: 0,
      requestId: currentRequestId,
    };
    try {
      sendJson({
        type: "live_control",
        mode: { mode: "step", count: 1 },
        request_id: currentRequestId,
      });
      pendingControlFeedback.set(currentRequestId, feedback);
      state.lastControlFeedback = feedback;
      render();
    } catch (_) {
      requestSnapshotSafe();
    }
    firstAgentClaimAutoRefreshTimer = window.setTimeout(() => {
      firstAgentClaimAutoRefreshTimer = null;
      requestSnapshotSafe();
    }, FIRST_AGENT_CLAIM_AUTO_REFRESH_DELAY_MS);
  }, FIRST_AGENT_CLAIM_AUTO_ADVANCE_DELAY_MS);
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

function summarizeEventTitle(event) {
  const kind = event?.kind?.type || "unknown";
  return kind.replace(/_/g, " ");
}

function addRecentEvent(event) {
  state.eventSeq = Math.max(state.eventSeq, Number(event?.id || 0));
  if (isWorldScopedCrisisRuntimeEvent(event)) { return; }
  state.recentEvents.unshift(event);
  state.recentEvents = state.recentEvents.slice(0, MAX_EVENTS);
  state.eventCount = state.recentEvents.length;
}

function handleSnapshot(snapshot) {
  clearInitialSnapshotRetryTimer(); quoteProtocolFacade.invalidateSnapshotBoundQuotes();
  state.snapshot = snapshot;
  state.logicalTime = Math.max(state.logicalTime, Number(snapshot?.time || 0));
  state.tick = state.logicalTime;
  adoptCurrentPlayerBindingFromSnapshot(snapshot);
  maybeRecoverLocalTestStarterBindingFromSnapshot(snapshot);
  const { agents, locations } = entityCollections();
  if (!state.selectedObject) {
    if (agents[0]) {
      applySelection({ kind: "agent", id: agents[0].id });
    } else if (locations[0]) {
      applySelection({ kind: "location", id: locations[0].id });
    }
  } else if (state.selectedKind && state.selectedId) {
    if (!applySelection({ kind: state.selectedKind, id: state.selectedId }, { preserveInteractionDrafts: true })) { state.selectedKind = null; state.selectedId = null; state.selectedObject = null; syncAgentInteractionDrafts(true); }
  }
  hydrateChatHistoryFromStorage();
  syncAgentInteractionDrafts(reconcilePendingPromptAuthoritativeRefresh(snapshot));
  void canonicalOwnerRead.refresh().catch(() => {}); syncEmptyEntitySnapshotRefreshLoop(); if (snapshot?.model?.agents?.[STARTER_AGENT_ID]) { clearFirstAgentClaimAutoAdvanceTimers(); } worldFeedTransport.refreshAfterSnapshot();
}

function normalizedGameplayActions(snapshot = state.snapshot) {
  const actions = snapshot?.player_gameplay?.available_actions
    || snapshot?.player_gameplay?.availableActions
    || [];
  return Array.isArray(actions) ? actions : [];
}

function gameplayActionId(action) {
  return String(action?.action_id || action?.actionId || "").trim();
}

function gameplayProtocolAction(action) {
  return String(action?.protocol_action || action?.protocolAction || "").trim();
}

function hasGameplayAction(snapshot, actionId) {
  return normalizedGameplayActions(snapshot).some((action) => gameplayActionId(action) === actionId);
}

function hasSnapshotRefreshAction(snapshot) {
  return normalizedGameplayActions(snapshot).some((action) => {
    const protocol = gameplayProtocolAction(action);
    return gameplayActionId(action) === "request_snapshot"
      || protocol === "request_snapshot"
      || protocol === "world.request_snapshot";
  });
}

function needsEmptyEntitySnapshotRefresh(snapshot = state.snapshot) {
  const gameplay = snapshot?.player_gameplay || {};
  const blockerKind = String(gameplay.blocker_kind || gameplay.blockerKind || "").trim();
  if (blockerKind !== "runtime_snapshot_empty_entities") {
    return false;
  }
  if (hasGameplayAction(snapshot, "claim_first_agent")) {
    return false;
  }
  return hasSnapshotRefreshAction(snapshot);
}

function clearEmptyEntitySnapshotRefreshTimer() {
  if (emptyEntitySnapshotRefreshTimer) {
    window.clearTimeout(emptyEntitySnapshotRefreshTimer);
    emptyEntitySnapshotRefreshTimer = null;
  }
}

function needsEmptyEntitySnapshotRefreshForTest(snapshot = state.snapshot) {
  if (!isTestApiEnabled()) {
    throw new Error("needsEmptyEntitySnapshotRefreshForTest requires test_api=1");
  }
  return needsEmptyEntitySnapshotRefresh(snapshot);
}

function isEmptyEntitySnapshotRefreshPendingForTest() {
  if (!isTestApiEnabled()) {
    throw new Error("isEmptyEntitySnapshotRefreshPendingForTest requires test_api=1");
  }
  return Boolean(emptyEntitySnapshotRefreshTimer);
}

function syncEmptyEntitySnapshotRefreshLoop() {
  if (!needsEmptyEntitySnapshotRefresh()) {
    clearEmptyEntitySnapshotRefreshTimer();
    return;
  }
  if (emptyEntitySnapshotRefreshTimer || !socket || socket.readyState !== WebSocket.OPEN) {
    return;
  }
  emptyEntitySnapshotRefreshTimer = window.setTimeout(() => {
    emptyEntitySnapshotRefreshTimer = null;
    if (!needsEmptyEntitySnapshotRefresh()) {
      return;
    }
    if (!socket || socket.readyState !== WebSocket.OPEN) {
      return;
    }
    requestSnapshotSafe();
    syncEmptyEntitySnapshotRefreshLoop();
  }, EMPTY_ENTITY_SNAPSHOT_REFRESH_DELAY_MS);
}

function adoptCurrentPlayerBindingFromSnapshot(snapshot) {
  const playerId = String(state.auth.playerId || "").trim();
  if (!playerId) {
    return;
  }
  const bindings = snapshot?.model?.agent_player_bindings || {};
  const boundAgentId = Object.entries(bindings)
    .find(([, boundPlayerId]) => String(boundPlayerId || "").trim() === playerId)?.[0] || null;
  if (!boundAgentId || state.auth.boundAgentId === boundAgentId) {
    return;
  }
  state.auth.boundAgentId = boundAgentId;
  state.auth.pendingRequestedAgentId = boundAgentId;
  state.auth.registrationStatus = "registered";
  state.auth.runtimeStatus = "registered";
  state.auth.error = null;
  if (state.auth.available && state.auth.source !== LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE) {
    persistHostedPlayerSession(state.auth);
  }
}

function maybeRecoverLocalTestStarterBindingFromSnapshot(snapshot) {
  if (!isTestApiEnabled() || state.auth.source !== "local_test_api_ephemeral" || !state.auth.available) {
    return;
  }
  if (state.auth.boundAgentId || state.auth.syncInFlight || pendingSessionRegisterWaiter) {
    return;
  }
  if (!socket || socket.readyState !== WebSocket.OPEN) {
    return;
  }
  const agents = snapshot?.model?.agents || {};
  if (!agents[STARTER_AGENT_ID]) {
    return;
  }
  const currentPlayerId = String(state.auth.playerId || "").trim();
  const boundPlayerId = String(snapshot?.model?.agent_player_bindings?.[STARTER_AGENT_ID] || "").trim();
  if (
    !currentPlayerId
    || !boundPlayerId.startsWith(LOCAL_TEST_PLAYER_ID_PREFIX)
    || boundPlayerId === currentPlayerId
  ) {
    return;
  }
  const attemptKey = `${currentPlayerId}:${boundPlayerId}:${STARTER_AGENT_ID}`;
  if (localTestStarterRebindAttemptKey === attemptKey) {
    return;
  }
  localTestStarterRebindAttemptKey = attemptKey;
  state.auth.pendingRequestedAgentId = STARTER_AGENT_ID;
  state.auth.pendingForceRebind = true;
  state.auth.rebindNotice = `Local test player is taking over ${STARTER_AGENT_ID} from an earlier local test session.`;
  void ensureRegisteredPlayerSession(STARTER_AGENT_ID, { forceRebind: true }).catch((error) => {
    localTestStarterRebindAttemptKey = null;
    state.auth.syncInFlight = false;
    state.auth.pendingForceRebind = false;
    state.auth.runtimeStatus = "error";
    if (state.auth.recoveryErrorCode !== "session_register_timeout") {
      state.auth.recoveryErrorCode = "local_test_starter_rebind_failed";
      state.auth.recoveryErrorMessage = String(error);
    }
    state.auth.error = String(error);
    render();
  });
}

function injectSnapshot(snapshot, options = {}) {
  if (!isTestApiEnabled()) {
    throw new Error("injectSnapshot requires test_api=1");
  }
  handleSnapshot(clone(snapshot));
  render();
  if (options?.returnState === false) {
    return { ok: true };
  }
  return getState();
}

function handleMetrics(time, metrics) {
  state.metrics = metrics || null;
  state.traceCount = Number(metrics?.decision_trace_count || 0);
  state.logicalTime = Math.max(state.logicalTime, Number(time || 0), Number(metrics?.total_ticks || 0));
  state.tick = state.logicalTime;
}

function clipTraceText(value, limit = 480) {
  const text = String(value || "").trim();
  if (!text) {
    return null;
  }
  if (text.length <= limit) {
    return text;
  }
  return `${text.slice(0, limit)}…`;
}

function snapshotDecisionTrace(trace) {
  if (!trace || typeof trace !== "object") {
    return null;
  }
  return {
    agent_id: trace.agent_id || null,
    time: Number(trace.time || 0),
    decision: clone(trace.decision || null),
    llm_error: trace.llm_error || null,
    parse_error: trace.parse_error || null,
    llm_input_excerpt: clipTraceText(trace.llm_input),
    llm_output_excerpt: clipTraceText(trace.llm_output),
    llm_diagnostics: clone(trace.llm_diagnostics || null),
  };
}

function handleDecisionTrace(trace) {
  if (!trace || typeof trace !== "object") {
    return;
  }
  state.recentDecisionTraces.unshift(clone(trace));
  state.recentDecisionTraces = state.recentDecisionTraces.slice(0, MAX_DECISION_TRACES);
  state.traceCount = Math.max(state.traceCount, state.recentDecisionTraces.length);
  state.logicalTime = Math.max(state.logicalTime, Number(trace?.time || 0));
  state.tick = state.logicalTime;
}

function handleControlCompletionAck(ack) {
  if (!acceptRuntimeAckIdentity(ack)) return;
  const feedback = pendingControlFeedback.get(ack?.request_id);
  if (!feedback) return;
  feedback.deltaLogicalTime = Number(ack?.delta_logical_time || 0);
  feedback.deltaEventSeq = Number(ack?.delta_event_seq || 0);
  if (ack?.status === "advanced") {
    feedback.stage = "completed_advanced";
    feedback.effect = `control ack advanced: logicalTime +${feedback.deltaLogicalTime}, eventSeq +${feedback.deltaEventSeq}`;
    feedback.reason = null;
  } else if (ack?.status === "blocked") {
    feedback.stage = "blocked";
    feedback.reason =
      ack?.error_message || ack?.error_code || "control was blocked before runtime advance";
    feedback.hint = state.snapshot?.player_gameplay?.next_step_hint || feedback.hint;
    feedback.effect = `gameplay blocked before requested advance completed: logicalTime +${feedback.deltaLogicalTime}, eventSeq +${feedback.deltaEventSeq}`;
  } else {
    feedback.stage = "completed_no_progress";
    feedback.reason = "timeout_no_progress";
    feedback.effect = `no visible world delta: logicalTime +${feedback.deltaLogicalTime}, eventSeq +${feedback.deltaEventSeq}`;
  }
  state.lastControlFeedback = feedback;
  pendingControlFeedback.delete(feedback.requestId);
  if (feedback.stage === "completed_advanced") {
    requestSnapshotSafe();
  }
  render();
}

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

async function requestRefineQuote(compoundMassG) {
  const compoundMassGNumber = Number(compoundMassG);
  if (!Number.isSafeInteger(compoundMassGNumber) || compoundMassGNumber <= 0) {
    state.refineQuoteRequest = { status: "error", error: "refine quote requires a positive whole-number compound mass in grams" }; return { ok: false, reason: "refine quote requires a positive whole-number compound mass in grams" };
  }
  if (!socket || socket.readyState !== WebSocket.OPEN) {
    state.refineQuoteRequest = { status: "error", error: "refine quote requires a connected viewer websocket" };
    return { ok: false, reason: "refine quote requires a connected viewer websocket" };
  }
  try {
    await ensureHostedPlayerAuthAvailable();
    if (!state.auth.available) {
      state.refineQuoteRequest = { status: "error", error: state.auth.error || "refine quote requires an active player session" };
      return { ok: false, reason: state.auth.error || "refine quote requires an active player session" };
    }
    const boundAgentId = String(state.auth.boundAgentId || "").trim();
    if (!boundAgentId) {
      state.refineQuoteRequest = { status: "error", error: "refine quote requires a bound player Agent" };
      return { ok: false, reason: "refine quote requires a bound player Agent" };
    }
    await ensureRegisteredPlayerSession(boundAgentId);
    const request = {
      compound_mass_g: compoundMassGNumber,
      player_id: state.auth.playerId,
      public_key: state.auth.publicKey,
    };
    request.auth = await buildRefineQuoteAuthProof(request, state.auth);
    state.refineQuoteRequest = { status: "pending", error: null };
    sendJson({ type: "quote_refine_compound", request });
    return { ok: true, request: clone(request) };
  } catch (error) {
    const reason = `refine quote request failed: ${String(error)}`;
    state.refineQuoteRequest = { status: "error", error: reason };
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

function clearPendingSessionRegisterWaiter(error = null, options = {}) {
  if (!pendingSessionRegisterWaiter) {
    return;
  }
  const waiter = pendingSessionRegisterWaiter;
  pendingSessionRegisterWaiter = null;
  if (waiter.timeoutId) {
    window.clearTimeout(waiter.timeoutId);
  }
  if (error != null && options.reject !== false) {
    waiter.reject(error instanceof Error ? error : new Error(String(error)));
  }
}

function acceptRuntimeAckIdentity(ack, registration = false) {
  const error = validateRuntimeAckIdentity(ack, state.auth, registration, pendingSessionRegisterWaiter?.requestedAgentId);
  if (!error) return true;
  invalidateAuthConnection();
  const invalidSocket = socket;
  socket = null;
  cancelHostedLeaseRefresh();
  invalidSocket?.close();
  clearPendingSessionRegisterWaiter(error);
  if (hasSigningIdentity(state.auth)) updateRegistrationGrant(state.auth, null);
  state.auth.syncInFlight = false;
  state.auth.runtimeStatus = "error";
  state.auth.error = error;
  state.connectionStatus = "error";
  state.lastError = error;
  stopHostedSessionRefreshLoop();
  clearHostedRuntimeSyncTimer();
  return false;
}

function recoverConnectedSessionStateAfterRuntimeAck(ack = null) {
  if (state.connectionStatus === "error" && /player session registration timed out/i.test(String(state.lastError || ""))) {
    state.connectionStatus = "connected";
    state.lastError = null;
  }
  state.auth.syncInFlight = false;
  state.auth.recoveryErrorCode = null;
  state.auth.recoveryErrorMessage = null;
  state.auth.error = null;
  if (ack?.session_epoch != null) {
    state.auth.sessionEpoch = Number(ack.session_epoch);
  }
  if (Object.prototype.hasOwnProperty.call(ack || {}, "binding_epoch")) {
    state.auth.bindingEpoch = ack.binding_epoch == null ? null : Number(ack.binding_epoch);
  }
  state.auth.authorityEpoch = state.viewerProtocol.authorityEpoch || null;
}

function resolvePendingSessionRegisterWaiterAfterRuntimeAck(ack = null) {
  if (pendingSessionRegisterWaiter && !isSessionContextCurrent(state, pendingSessionRegisterWaiter.context, authConnectionGeneration(), state.wsUrl)) return;
  recoverConnectedSessionStateAfterRuntimeAck(ack);
  if (!pendingSessionRegisterWaiter) {
    return;
  }
  const waiter = pendingSessionRegisterWaiter;
  pendingSessionRegisterWaiter = null;
  if (waiter.timeoutId) {
    window.clearTimeout(waiter.timeoutId);
  }
  waiter.resolve(ack);
}

function expirePendingSessionRegisterWaiterForTest() {
  if (!isTestApiEnabled()) {
    throw new Error("expirePendingSessionRegisterWaiterForTest requires test_api=1");
  }
  if (!pendingSessionRegisterWaiter) {
    return false;
  }
  const message = "player session registration timed out waiting for ack/error from live server";
  state.auth.syncInFlight = false;
  state.auth.registrationStatus = state.auth.available ? "issued" : "guest";
  state.auth.runtimeStatus = "error";
  state.auth.error = message;
  state.auth.recoveryErrorCode = "session_register_timeout";
  state.auth.recoveryErrorMessage = message;
  clearPendingSessionRegisterWaiter(message);
  render();
  return true;
}

async function dispatchSessionRegisterRequest(requestedAgentId, forceRebind) {
  clearHostedRuntimeSyncTimer();
  const auth = state.auth.source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE
    ? state.auth
    : await ensureHostedAuthSigningKey(state.auth);
  const operationContext = captureSessionContext(state, authConnectionGeneration(), state.wsUrl);
  if (auth.source !== LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE && authCredentials(auth).releaseToken && !authCredentials(auth).registrationGrant) {
    const refreshed = await refreshHostedPlayerLease();
    if (!refreshed?.ok || !isSessionContextCurrent(state, operationContext, authConnectionGeneration(), state.wsUrl)) throw new Error("registration grant refresh was invalidated");
  }
  const normalizedRequestedAgentId = String(requestedAgentId || "").trim() || null;
  if (state.auth.source !== LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE) {
    state.auth.registrationStatus = "registering";
    state.auth.syncInFlight = true;
    state.auth.recoveryErrorCode = null;
    state.auth.recoveryErrorMessage = null;
    state.auth.runtimeStatus = forceRebind === true ? "rebind_registering" : "registering";
  }
  if (forceRebind === true) {
    state.auth.rebindNotice = `Switching player session to ${normalizedRequestedAgentId || "requested agent"}...`;
  }
  if (pendingSessionRegisterWaiter) pendingSessionRegisterWaiter.context = captureSessionContext(state, authConnectionGeneration(), state.wsUrl);
  state.auth.pendingRequestedAgentId = normalizedRequestedAgentId;
  state.auth.pendingForceRebind = forceRebind === true;
  const request = {
    player_id: auth.playerId,
    public_key: auth.publicKey,
  };
  if (authCredentials(auth).registrationGrant) {
    request.registration_grant = authCredentials(auth).registrationGrant;
  }
  if (normalizedRequestedAgentId) {
    request.requested_agent_id = normalizedRequestedAgentId;
  }
  if (forceRebind === true) {
    request.force_rebind = true;
  }
  request.auth = await buildSessionRegisterAuthProof(request, auth);
  if (!isSessionContextCurrent(state, operationContext, authConnectionGeneration(), state.wsUrl)) throw new Error("registration context was invalidated");
  sendJson({
    type: "authoritative_recovery",
    command: {
      mode: "register_session",
      request,
    },
  });
  render();
}

async function retryPendingSessionRegisterWaiterWithForceRebind() {
  const waiter = pendingSessionRegisterWaiter;
  if (!waiter) {
    return;
  }
  waiter.forceRebind = true;
  try {
    await dispatchSessionRegisterRequest(waiter.requestedAgentId, true);
  } catch (error) {
    clearPendingSessionRegisterWaiter(error);
    throw error;
  }
}

function rebindAgentIdFromRecoveryError(error) {
  const explicit = String(error?.agent_id || error?.agentId || error?.target_agent_id || error?.targetAgentId || "").trim();
  if (explicit) {
    return explicit;
  }
  const message = String(error?.message || "");
  const match = message.match(/^agent\s+(\S+)\s+is bound to player\s+\S+,\s+not\s+\S+/);
  return match?.[1] || null;
}

function latestRequestedAgentId(fallbackAgentId = null) {
  const agentId = String(
    fallbackAgentId
      || state.auth.pendingRequestedAgentId
      || state.auth.boundAgentId
      || "",
  ).trim();
  return agentId || null;
}

function recoveryErrorRequiresExplicitRebind(error) {
  if (String(error?.code || "").trim() !== "player_bind_failed") {
    return false;
  }
  const message = String(error?.message || "");
  return message.includes("explicit rebind required")
    || /^agent\s+\S+\s+is bound to player\s+\S+,\s+not\s+\S+/.test(message);
}

async function ensureRegisteredPlayerSession(requestedAgentId = null, options = {}) {
  await waitForHostedTestLogin();
  await ensureHostedPlayerAuthAvailable();
  if (!state.auth.available) {
    throw new Error(state.auth.error || "player session auth is unavailable");
  }
  const normalizedRequestedAgentId = String(requestedAgentId || "").trim() || null;
  const forceRebind = options?.forceRebind === true;
  if (
    state.auth.registrationStatus === "registered"
    && (state.auth.runtimeStatus === "registered" || state.auth.runtimeStatus === "registered_unbound")
    && !forceRebind
    && options?.renewAuthorization !== true
    && (
      normalizedRequestedAgentId == null
      || normalizedRequestedAgentId === state.auth.boundAgentId
    )
  ) {
    return state.auth;
  }
  if (pendingSessionRegisterWaiter) {
    const sameRequest = pendingSessionRegisterWaiter.requestedAgentId === normalizedRequestedAgentId
      && pendingSessionRegisterWaiter.forceRebind === forceRebind;
    if (!sameRequest) {
      throw new Error("another player session registration is already in flight");
    }
    return pendingSessionRegisterWaiter.promise;
  }
  let resolveWaiter;
  let rejectWaiter;
  const promise = new Promise((resolve, reject) => {
    resolveWaiter = resolve;
    rejectWaiter = reject;
  });
  pendingSessionRegisterWaiter = {
    context: captureSessionContext(state, authConnectionGeneration(), state.wsUrl),
    requestedAgentId: normalizedRequestedAgentId,
    forceRebind,
    promise,
    resolve: resolveWaiter,
    reject: rejectWaiter,
    timeoutId: null,
  };
  pendingSessionRegisterWaiter.timeoutId = window.setTimeout(() => {
    if (!pendingSessionRegisterWaiter || pendingSessionRegisterWaiter.promise !== promise) {
      return;
    }
    const message = "player session registration timed out waiting for ack/error from live server";
    state.auth.syncInFlight = false;
    state.auth.registrationStatus = state.auth.available ? "issued" : "guest";
    state.auth.runtimeStatus = "error";
    state.auth.error = message;
    state.auth.recoveryErrorCode = "session_register_timeout";
    state.auth.recoveryErrorMessage = message;
    clearPendingSessionRegisterWaiter(message);
    render();
  }, SESSION_REGISTER_ACK_TIMEOUT_MS);
  try {
    await dispatchSessionRegisterRequest(normalizedRequestedAgentId, forceRebind);
  } catch (error) {
    state.auth.syncInFlight = false;
    state.auth.registrationStatus = state.auth.available ? "issued" : "guest";
    state.auth.runtimeStatus = "error";
    state.auth.error = String(error);
    state.auth.recoveryErrorCode = "session_register_send_failed";
    state.auth.recoveryErrorMessage = String(error);
    clearPendingSessionRegisterWaiter(error, { reject: false });
    markCurrentGameplayActionFeedbackError(error, "player session registration failed");
    render();
    throw error;
  }
  return promise;
}

function registerPlayerSessionForTest(requestedAgentId = null, options = {}) {
  if (!isTestApiEnabled()) {
    throw new Error("registerPlayerSessionForTest requires test_api=1");
  }
  return ensureRegisteredPlayerSession(requestedAgentId, options);
}

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
  try {
    sendJson({ type: "request_snapshot" });
  } catch (_) {
  }
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
  if (!feedback || feedback.kind !== "gameplay_action") {
    return;
  }
  feedback.stage = "error";
  feedback.ok = false;
  feedback.accepted = false;
  feedback.reason = String(error);
  feedback.effect = effect;
  state.lastGameplayActionFeedback = feedback;
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
      assertSemanticCapability("agent_chat");
      await ensureRegisteredPlayerSession(agentId);
      assertAgentChatFeedbackActive(feedback);
      await canonicalOwnerRead.refresh(agentId);
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

  const feedback = createSemanticFeedback("prompt", `prompt_${normalizedMode}`, agentId, {
    effect: "queued for signing and send",
    toVersion: request.to_version ?? null,
    requestId: request.request_id || null,
  });
  state.lastPromptFeedback = feedback;
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

function normalizeGameplayActionRequest(action) {
  if (!action || typeof action !== "object") {
    return null;
  }
  const normalized = {
    ...action,
    protocol_action: action.protocol_action || action.protocolAction || null,
    action_id: action.action_id || action.actionId || null,
    target_agent_id: action.target_agent_id || action.targetAgentId || null,
    actor_agent_id: action.actor_agent_id || action.actorAgentId || null,
    disabled_reason: action.disabled_reason || action.disabledReason || null,
  };
  return normalized;
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

function sendGameplayAction(actionOrId) {
  const action = resolveGameplayActionRequest(actionOrId);
  if (!action) {
    return { ok: false, reason: "gameplay action is unavailable in the current snapshot" };
  }

  const protocolAction = String(action.protocol_action || "").trim();
  if (protocolAction === "request_snapshot" || protocolAction === "world.request_snapshot") {
    requestSnapshotSafe();
    state.lastGameplayActionFeedback = {
      id: nextRequestId(),
      kind: "gameplay_action",
      action: action.action_id || "request_snapshot",
      agentId: action.target_agent_id || null,
      accepted: true,
      ok: true,
      stage: "ack",
      reason: null,
      effect: "snapshot refresh requested",
      response: {
        action_id: action.action_id || "request_snapshot",
        target_agent_id: action.target_agent_id || "",
        accepted_at_tick: state.logicalTime,
        message: "snapshot refresh requested",
      },
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

  const feedback = createSemanticFeedback("gameplay_action", actionId, targetAgentId, {
    effect: "queued for signing and send",
    targetAgentId,
    protocolAction,
  });
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
      request.auth = await buildGameplayActionAuthProof(request, state.auth);
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
      markCurrentGameplayActionFeedbackError(error);
      render();
    }
  })();

  return { ok: true, feedback: snapshotSemanticFeedback(feedback) };
}

function handleGameplayActionAck(ack) {
  if (!acceptRuntimeAckIdentity(ack)) return;
  const pendingAction = state.lastGameplayActionFeedback;
  if (!pendingAction || ack?.action_id !== pendingAction.action || ack?.target_agent_id !== (pendingAction.targetAgentId || pendingAction.agentId)) return;
  clearPendingGameplayActionAckTimer();
  const feedback = state.lastGameplayActionFeedback || createSemanticFeedback(
    "gameplay_action",
    ack?.action_id || "gameplay_action",
    ack?.target_agent_id || null,
  );
  feedback.stage = "ack";
  feedback.ok = true;
  feedback.accepted = true;
  feedback.reason = null;
  feedback.effect = ack?.message || `gameplay action accepted at tick ${Number(ack?.accepted_at_tick || state.logicalTime)}`;
  feedback.response = clone(ack);
  state.lastGameplayActionFeedback = feedback;
  if (ack?.action_id === "claim_first_agent" && ack?.target_agent_id) {
    state.auth.boundAgentId = ack.target_agent_id;
    state.auth.pendingRequestedAgentId = ack.target_agent_id;
    state.auth.registrationStatus = "registered";
    state.auth.runtimeStatus = "registered";
    state.auth.error = null;
    scheduleFirstAgentClaimAutoAdvance();
  }
  requestSnapshotSafe();
}

async function retryGameplayActionAfterMissingSession(feedback, error) {
  const actionId = String(error?.action_id || feedback?.action || "").trim();
  const targetAgentId = String(error?.target_agent_id || feedback?.agentId || "").trim();
  if (!actionId || !targetAgentId || feedback?.sessionRefreshRetryAttempted) {
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
  feedback.sessionRefreshRetryAttempted = true;
  feedback.stage = "registering";
  feedback.ok = false;
  feedback.accepted = false;
  feedback.reason = "runtime session was missing; refreshing player session and retrying";
  feedback.effect = "refreshing player session";
  state.lastGameplayActionFeedback = feedback;
  render();
  try {
    const registrationAgentId = gameplayActionRequiresActorAgent(actionId)
      ? action.actor_agent_id || action.actorAgentId || state.auth.boundAgentId || targetAgentId
      : actionId === "claim_first_agent"
        ? null
        : targetAgentId;
    await ensureRegisteredPlayerSession(registrationAgentId, { forceRebind: true });
    sendGameplayAction(action);
  } catch (retryError) {
    markCurrentGameplayActionFeedbackError(
      retryError,
      "player session refresh failed before retrying gameplay action",
    );
    render();
  }
}

function handleGameplayActionError(error) {
  clearPendingGameplayActionAckTimer();
  if (quoteProtocolFacade.handleQuoteGameplayActionError(error)) {
    return;
  }
  const feedback = state.lastGameplayActionFeedback || createSemanticFeedback(
    "gameplay_action",
    error?.action_id || "gameplay_action",
    error?.target_agent_id || null,
  );
  feedback.stage = "error";
  feedback.ok = false;
  feedback.accepted = false;
  feedback.reason = error?.message || error?.code || "gameplay action failed";
  feedback.effect = error?.code || "gameplay action error";
  feedback.response = clone(error);
  state.lastGameplayActionFeedback = feedback;
  if (String(error?.code || "").trim() === "session_not_found") {
    void retryGameplayActionAfterMissingSession(feedback, error);
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
  clearPendingAgentChatAckTimer();
  if (feedback.canonicalRequest && ack?.status === "pending") {
    pendingAgentChatAckTimer = scheduleCanonicalGoalPending({ ack, feedback, state, clone, requestSnapshotSafe, sameAgentChatFeedback, failPendingAgentChatAck, sendJson, scheduleAgentChatAckTimeout, getSocket: () => socket });
    return;
  }
  if (feedback.canonicalRequest && (ack?.status !== "accepted" || !ack?.intent_id)) {
    failPendingAgentChatAck("canonical goal commit was not confirmed"); return;
  }
  clearPendingAgentChatOverallTimer();
  applyCommittedChatAck({ ack, feedback, state, clone, pushChatHistory });
}

function handleAgentChatError(error) {
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
  }
}

async function recoverHostedSessionFromError(error) {
  if (
    (!canAutoIssueHostedPlayerSession() && !state.auth.available)
    || state.auth.source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE
  ) {
    return;
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
  state.auth.syncInFlight = false;
  state.auth.recoveryErrorCode = error?.code || null;
  state.auth.recoveryErrorMessage = error?.message || null;
  state.auth.error = error?.message || error?.code || "authoritative recovery failed";
  state.auth.revokeReason = error?.revoke_reason || null;
  state.auth.revokedBy = error?.revoked_by || null;
  state.auth.registrationStatus = "issued";
  state.auth.runtimeStatus = error?.code === "session_revoked"
    ? "revoked"
    : error?.code === "session_not_found"
      ? "missing"
      : "error";
  if (!recoveryErrorRequiresExplicitRebind(error)) {
    state.auth.boundAgentId = null;
  }
  clearPendingSessionRegisterWaiter(error?.message || error?.code || "authoritative recovery failed");
  syncHostedSessionRefreshLoop();
  void recoverHostedSessionFromError(error);
}

const worldViewTransport = createWorldViewTransport({ send: sendJson, getScope: () => worldViewScopeKey(state), getConnection: () => socket });
export const requestWorldView = query => worldViewTransport.request(query);
const scopeSnapshotTransport = createScopeSnapshotTransport({send:sendJson,getScope:()=>worldViewScopeKey(state),getConnection:()=>socket});
export const requestScopeSnapshot = () => scopeSnapshotTransport.request();
// Host authorizes the captured scope before this existing adopter triggers render.
export const acceptScopeSnapshot = snapshot => handleSnapshot(snapshot);
function handleViewerMessage(message, sourceSocket = null) { if (sourceSocket && socket !== sourceSocket) return;
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
    case "snapshot":
      handleSnapshot(message.snapshot);
      break;
    case "world_feed": worldFeedTransport.handleWorldFeed(message.feed, sourceSocket); break;
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
    initialSnapshotRetryCount = 0;
    clearHelloAckTimer();
    clearInitialSnapshotRetryTimer();
    sendJson({ type: "hello_v2", client: "viewer", version: 2, capabilities: [PROMPT_CONTROL_RESULT_CAPABILITY, "world_view", "world_view_pins_v1", "world_view_delta_v1", "world_view_aggregate_v1", "world_view_event_anchor_v1"] });
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
  const snapshotReceived = !!state.snapshot;
  const serverReady = !!state.server;
  const sessionSyncing = !!state.auth.syncInFlight || !!pendingSessionRegisterWaiter;
  let stage = "connecting";
  if (state.connectionStatus === "connected") {
    if (!serverReady) {
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

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function renderLists() {
  elements.leftPanel.innerHTML = renderViewerEntityList({ state, lists: modelLists() });
}

function renderSummary() {
  const controlFeedback = snapshotControlFeedback(state.lastControlFeedback);
  const promptFeedback = snapshotSemanticFeedback(state.lastPromptFeedback);
  const chatFeedback = snapshotSemanticFeedback(state.lastChatFeedback);
  const authSurface = buildAuthSurfaceModel();
  const hostedActionMatrixView = buildHostedActionMatrixView();
  const hostedRecoveryHint = buildHostedRecoveryHint();
  const authBadgeClass = state.auth.available ? "badge badge--good" : "badge badge--warn";
  const selectedDebug = selectedAgentExecutionDebugContext();
  const tierBadgeClass = (status) =>
    status === "active" || status === "active_legacy_preview"
      ? "badge badge--good"
      : status === "superseded"
        ? "badge"
        : "badge badge--warn";
  const showRebindNotice = !!state.auth.pendingRequestedAgentId
    && (state.auth.pendingForceRebind
      || state.auth.runtimeStatus === "rebind_retrying"
      || state.auth.runtimeStatus === "rebind_registering");
  elements.centerPanel.innerHTML = `
    <div class="stack">
      <div class="badge-row">
        <span class="badge badge--accent">viewer</span>
        <span class="${connectionBadgeClass()}">${escapeHtml(state.connectionStatus)}</span>
        <span class="badge">rendererClass=${escapeHtml(state.rendererClass)}</span>
        <span class="badge">controlProfile=${escapeHtml(state.controlProfile)}</span>
      </div>
      <div class="summary-grid">
        <div class="metric"><div class="metric__label">Logical Time</div><div class="metric__value">${state.logicalTime}</div></div>
        <div class="metric"><div class="metric__label">Event Seq</div><div class="metric__value">${state.eventSeq}</div></div>
        <div class="metric"><div class="metric__label">World</div><div class="metric__value">${escapeHtml(state.worldId || "-")}</div></div>
        <div class="metric"><div class="metric__label">Viewer Server</div><div class="metric__value">${escapeHtml(state.server || "-")}</div></div>
      </div>
      <div class="badge-row">
        <span class="badge">ws=${escapeHtml(state.wsUrl || "-")}</span>
        <span class="badge">entryReason=${escapeHtml(state.viewerReason || "-")}</span>
        <span class="badge">renderer=${escapeHtml(state.renderer || "n/a")}</span>
      </div>
      <div class="panel panel--nested" style="background:rgba(255,255,255,0.02);">
        <div class="panel__header"><div class="panel__title">Execution Lanes</div></div>
        <div class="panel__body stack">
          ${selectedDebug
            ? `<div class="badge-row">
                <span class="badge badge--accent">selected agent lane</span>
                <span class="badge">renderMode=${escapeHtml(state.renderMode)}</span>
                <span class="badge">entryReason=${escapeHtml(state.viewerReason || "-")}</span>
                <span class="badge">provider=${escapeHtml(selectedDebug.provider_mode || "-")}</span>
                <span class="badge">mode=${escapeHtml(selectedDebug.execution_mode || "-")}</span>
                <span class="badge">env=${escapeHtml(selectedDebug.environment_class || "-")}</span>
              </div>
              <div class="badge-row">
                <span class="badge">obs=${escapeHtml(selectedDebug.observation_schema_version || "-")}</span>
                <span class="badge">act=${escapeHtml(selectedDebug.action_schema_version || "-")}</span>
                <span class="badge">agentProfile=${escapeHtml(selectedDebug.agent_profile || "-")}</span>
                <span class="badge">providerFallback=${escapeHtml(selectedDebug.fallback_reason || "-")}</span>
              </div>
              <pre class="json">${escapeHtml(JSON.stringify(selectedDebug, null, 2))}</pre>`
            : '<div class="empty">Select an agent to inspect the current execution-lane metadata.</div>'}
        </div>
      </div>
      <div class="badge-row">
        <span class="${authBadgeClass}">auth=${state.auth.available ? state.auth.registrationStatus || "ready" : "missing"}</span>
        <span class="badge badge--accent">tier=${escapeHtml(authSurface.currentTier)}</span>
        <span class="badge">source=${escapeHtml(authSurface.source)}</span>
        <span class="badge">deploymentHint=${escapeHtml(authSurface.deploymentHint)}</span>
        <span class="badge">player=${escapeHtml(state.auth.playerId || "-")}</span>
        <span class="badge">pubkey=${escapeHtml(state.auth.publicKey ? `${state.auth.publicKey.slice(0, 10)}…` : "-")}</span>
        <span class="badge">epoch=${escapeHtml(state.auth.sessionEpoch == null ? "-" : state.auth.sessionEpoch)}</span>
        <span class="badge">runtime=${escapeHtml(state.auth.runtimeStatus || "-")}</span>
        <span class="badge">boundAgent=${escapeHtml(state.auth.boundAgentId || "-")}</span>
        <span class="badge">requestedAgent=${escapeHtml(state.auth.pendingRequestedAgentId || "-")}</span>
        <span class="badge">${escapeHtml(state.auth.pendingForceRebind ? "rebind=forcing" : "rebind=idle")}</span>
      </div>
      ${state.auth.recoveryErrorCode || state.auth.recoveryErrorMessage
        ? `<div class="badge-row">
            <span class="badge badge--warn">recoveryError=${escapeHtml(state.auth.recoveryErrorCode || "-")}</span>
            <span class="badge">${escapeHtml(state.auth.recoveryErrorMessage || "-")}</span>
          </div>`
        : ""}
      ${showRebindNotice
        ? `<div class="badge-row">
            <span class="badge badge--accent">rebind</span>
            <span class="badge">target=${escapeHtml(state.auth.pendingRequestedAgentId || "-")}</span>
            <span class="badge">${escapeHtml(state.auth.pendingForceRebind ? "mode=force_rebind" : "mode=awaiting_retry")}</span>
          </div>
          <div class="empty">Player session is switching to the requested agent and the current action will continue after registration succeeds.</div>`
        : ""}
      ${state.auth.rebindNotice
        ? `<div class="empty">${escapeHtml(state.auth.rebindNotice)}</div>`
        : ""}
      ${state.hostedAdmission
        ? `<div class="badge-row">
            <span class="badge">activeSlots=${escapeHtml(`${state.hostedAdmission.active_player_sessions}/${state.hostedAdmission.max_player_sessions}`)}</span>
            <span class="badge">effectiveSlots=${escapeHtml(state.hostedAdmission.effective_player_sessions == null ? "-" : `${state.hostedAdmission.effective_player_sessions}/${state.hostedAdmission.max_player_sessions}`)}</span>
            <span class="badge">runtimeBound=${escapeHtml(state.hostedAdmission.runtime_bound_player_sessions == null ? "-" : state.hostedAdmission.runtime_bound_player_sessions)}</span>
            <span class="badge">runtimeOnly=${escapeHtml(state.hostedAdmission.runtime_only_player_sessions == null ? "-" : state.hostedAdmission.runtime_only_player_sessions)}</span>
            <span class="badge">runtimeProbe=${escapeHtml(state.hostedAdmission.runtime_probe_status || "-")}</span>
            <span class="badge">issueBudget=${escapeHtml(state.hostedAdmission.remaining_issue_budget)}</span>
            <span class="badge">leaseTTL=${escapeHtml(state.hostedAdmission.slot_lease_ttl_ms)}</span>
            <span class="badge">issued=${escapeHtml(state.hostedAdmission.issued_players_total)}</span>
            <span class="badge">released=${escapeHtml(state.hostedAdmission.released_players_total)}</span>
          </div>`
        : ""}
      ${state.hostedAdmission?.runtime_probe_error
        ? `<div class="badge-row">
            <span class="badge badge--warn">runtimeProbeError=${escapeHtml(state.hostedAdmission.runtime_probe_error)}</span>
          </div>`
        : ""}
      ${hostedRecoveryHint
        ? `<div class="panel panel--nested" style="background:rgba(255,255,255,0.02); border-color:rgba(255,184,77,0.35);">
            <div class="panel__header"><div class="panel__title">Hosted Recovery</div></div>
            <div class="panel__body stack">
              <div class="badge-row">
                <span class="badge badge--warn">${escapeHtml(hostedRecoveryHint.kind)}</span>
                <span class="badge">${escapeHtml(hostedRecoveryHint.title)}</span>
              </div>
              <div class="empty">${escapeHtml(hostedRecoveryHint.detail)}</div>
              <div class="toolbar"><button data-auth-action="retry-issue" ${state.auth.issueInFlight ? "disabled" : ""}>${escapeHtml(hostedRecoveryHint.cta)}</button></div>
            </div>
          </div>`
        : ""}
      ${!state.auth.available && isHostedPublicJoinDeploymentMode(state.hostedAccess?.deployment_mode)
        ? hostedRecoveryHint
          ? ""
          : `<div class="toolbar"><button data-auth-action="retry-issue" ${state.auth.issueInFlight ? "disabled" : ""}>Acquire Hosted Player Session</button></div>`
        : ""}
      ${state.auth.available && state.auth.source !== LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE
        ? `<div class="toolbar"><button data-auth-action="logout">Release Hosted Player Session</button></div>`
        : ""}
      <div class="panel panel--nested" style="background:rgba(255,255,255,0.02);">
        <div class="panel__header"><div class="panel__title">Session Ladder</div></div>
        <div class="panel__body stack">
          <div class="empty">${escapeHtml(authSurface.currentTierReason)}</div>
          <div class="event-list">
            ${authSurface.tiers
              .map(
                (tier) => `
                  <div class="event-card">
                    <div class="event-card__title">
                      <span>${escapeHtml(tier.label)}</span>
                      <span class="${tierBadgeClass(tier.status)}">${escapeHtml(tier.status)}</span>
                    </div>
                    <div class="event-card__meta">${escapeHtml(tier.reason)}</div>
                  </div>`,
              )
              .join("")}
          </div>
          <div class="badge-row">
            <span class="${authSurface.capabilities.prompt_control.enabled ? "badge badge--good" : "badge badge--warn"}">prompt=${escapeHtml(authSurface.capabilities.prompt_control.enabled ? "enabled" : authSurface.capabilities.prompt_control.code)}</span>
            <span class="${authSurface.capabilities.agent_chat.enabled ? "badge badge--good" : "badge badge--warn"}">chat=${escapeHtml(authSurface.capabilities.agent_chat.enabled ? "enabled" : authSurface.capabilities.agent_chat.code)}</span>
            <span class="badge badge--warn">mainToken=${escapeHtml(authSurface.capabilities.main_token_transfer.code)}</span>
          </div>
          <div class="empty">${escapeHtml(authSurface.reconnect)}</div>
        </div>
      </div>
      ${hostedActionMatrixView.length
        ? `<div class="panel panel--nested" style="background:rgba(255,255,255,0.02);">
            <div class="panel__header"><div class="panel__title">Hosted Action Matrix</div></div>
            <div class="panel__body stack">
              <div class="empty">This is the hosted public-join truth surface exported by the launcher. QA should read these action ids directly instead of inferring from button state alone.</div>
              <div class="event-list">
                ${hostedActionMatrixView
                  .map(
                    (item) => `
                      <div class="event-card">
                        <div class="event-card__title">
                          <span>${escapeHtml(item.actionId)}</span>
                          <span class="${item.enabled ? "badge badge--good" : "badge badge--warn"}">${escapeHtml(item.enabled ? "enabled" : item.code || "blocked")}</span>
                        </div>
                        <div class="event-card__meta">required_auth=${escapeHtml(item.requiredAuth)} · availability=${escapeHtml(item.availability)}</div>
                        <div class="empty">${escapeHtml(item.reason || "-")}</div>
                        ${item.capabilityReason && item.capabilityReason !== item.reason
                          ? `<div class="empty">viewer=${escapeHtml(item.capabilityReason)}</div>`
                          : ""}
                      </div>`,
                  )
                  .join("")}
              </div>
            </div>
          </div>`
        : ""}
      <div class="panel panel--nested" style="background:rgba(255,255,255,0.02);">
        <div class="panel__header"><div class="panel__title">Playback Controls</div></div>
        <div class="panel__body stack">
          <div class="toolbar">
            <button data-action="play">Play</button>
            <button data-action="pause">Pause</button>
            <button data-action="step">Step x1</button>
          </div>
          <div class="control-grid">
            <div class="field">
              <label for="step-count">Step count</label>
              <input id="step-count" type="number" min="1" step="1" value="3" />
            </div>
            <div class="field" style="align-self:end;">
              <button data-action="step-count">Step custom count</button>
            </div>
          </div>
          ${controlFeedback
            ? `<div class="badge-row">
                <span class="badge">action=${escapeHtml(controlFeedback.action)}</span>
                <span class="badge">stage=${escapeHtml(controlFeedback.stage)}</span>
                <span class="badge">Δtick=${controlFeedback.deltaLogicalTime}</span>
                <span class="badge">Δevent=${controlFeedback.deltaEventSeq}</span>
              </div>
              <pre class="json">${escapeHtml(JSON.stringify(controlFeedback, null, 2))}</pre>`
            : '<div class="empty">No control feedback yet.</div>'}
        </div>
      </div>
      <div class="summary-grid">
        <div class="metric">
          <div class="metric__label">Prompt Feedback</div>
          <div class="metric__value">${escapeHtml(promptFeedback?.stage || "idle")}</div>
          ${promptFeedback ? `<div class="badge-row" style="margin-top:8px;"><span class="${feedbackBadgeClass(promptFeedback)}">${escapeHtml(promptFeedback.effect || promptFeedback.reason || "ready")}</span></div>` : ""}
        </div>
        <div class="metric">
          <div class="metric__label">Chat Feedback</div>
          <div class="metric__value">${escapeHtml(chatFeedback?.stage || "idle")}</div>
          ${chatFeedback ? `<div class="badge-row" style="margin-top:8px;"><span class="${feedbackBadgeClass(chatFeedback)}">${escapeHtml(chatFeedback.effect || chatFeedback.reason || "ready")}</span></div>` : ""}
        </div>
      </div>
      <div>
        <div class="panel__title" style="margin-bottom:10px;">Recent Events</div>
        <div class="event-list">
          ${state.recentEvents.length
            ? state.recentEvents
                .map(
                  (event) => `
                    <div class="event-card">
                      <div class="event-card__title">
                        <span>${escapeHtml(summarizeEventTitle(event))}</span>
                        <span>#${Number(event.id || 0)}</span>
                      </div>
                      <div class="event-card__meta">time=${Number(event.time || 0)}</div>
                      <pre class="json">${escapeHtml(JSON.stringify(event.kind, null, 2))}</pre>
                    </div>`,
                )
                .join("")
            : '<div class="empty">Waiting for live events…</div>'}
        </div>
      </div>
    </div>
  `;
}

function renderInteractionPanel() {
  const rawAgentId = selectedAgentId();
  const agentId = rawAgentId && isAgentVisibleToCurrentSession(rawAgentId) ? rawAgentId : null;
  if (!agentId) {
    return '<div class="empty">Select an agent to unlock prompt/chat controls.</div>';
  }
  const binding = selectedAgentBindingInfo();
  const promptFeedback = snapshotSemanticFeedback(state.lastPromptFeedback);
  const chatFeedback = snapshotSemanticFeedback(state.lastChatFeedback);
  const authSurface = buildAuthSurfaceModel();
  const promptCapability = authSurface.capabilities.prompt_control;
  const chatCapability = authSurface.capabilities.agent_chat;
  const mainTokenTransferCapability = authSurface.capabilities.main_token_transfer;
  const mainTokenTransferPolicy = hostedActionPolicy("main_token_transfer");
  const interactionEnabled = promptCapability.enabled;
  const strongAuthGrantHint = authSurface.capabilities.prompt_control.enabled
    && isHostedPublicJoinDeploymentMode(state.hostedAccess?.deployment_mode)
    ? `<div class="field">
         <label for="strong-auth-approval-code">Backend Approval Code</label>
         <input id="strong-auth-approval-code" type="password" autocomplete="off" value="${escapeHtml(state.strongAuth.approvalCode || "")}" />
       </div>`
    : "";
  const authNotice = interactionEnabled
    ? `<div class="badge-row"><span class="badge badge--good">${escapeHtml(authSurface.currentTier)}</span><span class="badge">player=${escapeHtml(state.auth.playerId)}</span><span class="badge">source=${escapeHtml(authSurface.source)}</span></div>
       <div class="empty">${escapeHtml(promptCapability.reason)}</div>`
    : `<div class="empty">${escapeHtml(promptCapability.reason)}</div>`;
  const chatHistory = state.chatHistory
    .filter((entry) => entry.agentId === agentId || entry.targetAgentId === agentId)
    .slice(0, 12);
  const assetLaneStatusText = mainTokenTransferCapability.enabled ? "preview_only" : mainTokenTransferCapability.code || "blocked";
  const assetLaneDetail = mainTokenTransferCapability.enabled
    ? "Contract marks main_token_transfer as strong_auth-capable on this lane, but viewer still exposes no transfer form here."
    : mainTokenTransferCapability.reason;

  return `
    <div class="stack">
      <div class="badge-row">
        <span class="badge badge--accent">Agent Interaction</span>
        <span class="badge">agent=${escapeHtml(agentId)}</span>
        <span class="badge">promptVersion=${Number(state.promptDraft.currentVersion || 0)}</span>
      </div>
      ${authNotice}
      <div class="badge-row">
        <span class="badge">boundPlayer=${escapeHtml(binding?.playerId || "-")}</span>
        <span class="badge">boundKey=${escapeHtml(binding?.publicKey ? `${binding.publicKey.slice(0, 10)}…` : "-")}</span>
        <span class="${promptCapability.enabled ? "badge badge--good" : "badge badge--warn"}">prompt=${escapeHtml(promptCapability.enabled ? "enabled" : promptCapability.code)}</span>
        <span class="${chatCapability.enabled ? "badge badge--good" : "badge badge--warn"}">chat=${escapeHtml(chatCapability.enabled ? "enabled" : chatCapability.code)}</span>
        <span class="${mainTokenTransferCapability.enabled ? "badge badge--good" : "badge badge--warn"}">mainToken=${escapeHtml(assetLaneStatusText)}</span>
      </div>
      <div class="empty">${escapeHtml(assetLaneDetail)}</div>
      <div class="panel panel--nested" style="background:rgba(255,255,255,0.02);">
        <div class="panel__header"><div class="panel__title">Prompt Overrides</div></div>
        <div class="panel__body stack">
          ${strongAuthGrantHint}
          <div class="field">
            <label for="prompt-system">System Prompt Override</label>
            <textarea id="prompt-system" rows="4" ${promptCapability.enabled ? "" : "disabled"}>${escapeHtml(state.promptDraft.systemPrompt)}</textarea>
          </div>
          <div class="field">
            <label for="prompt-short">Short-Term Goal Override</label>
            <textarea id="prompt-short" rows="3" ${promptCapability.enabled ? "" : "disabled"}>${escapeHtml(state.promptDraft.shortTermGoal)}</textarea>
          </div>
          <div class="field">
            <label for="prompt-long">Long-Term Goal Override</label>
            <textarea id="prompt-long" rows="3" ${promptCapability.enabled ? "" : "disabled"}>${escapeHtml(state.promptDraft.longTermGoal)}</textarea>
          </div>
          <div class="toolbar">
            <button data-prompt-action="preview" ${promptCapability.enabled ? "" : "disabled"}>Preview Prompt</button>
            <button data-prompt-action="apply" ${promptCapability.enabled ? "" : "disabled"}>Apply Prompt</button>
          </div>
          <div class="toolbar">
            <div class="field" style="margin:0; min-width:180px; flex:1;">
              <label for="prompt-rollback-version">Rollback Target Version</label>
              <input id="prompt-rollback-version" type="number" min="0" step="1" value="${Number(state.promptDraft.rollbackTargetVersion || 0)}" ${promptCapability.enabled ? "" : "disabled"} />
            </div>
            <button data-prompt-action="rollback" ${promptCapability.enabled ? "" : "disabled"}>Rollback Prompt</button>
          </div>
          ${promptFeedback
            ? `<div class="badge-row"><span class="${feedbackBadgeClass(promptFeedback)}">${escapeHtml(promptFeedback.stage)}</span></div>
               <pre class="json">${escapeHtml(JSON.stringify(promptFeedback, null, 2))}</pre>`
            : '<div class="empty">No prompt feedback yet.</div>'}
          ${state.strongAuth.lastGrantActionId
            ? `<div class="empty">lastGrant=${escapeHtml(state.strongAuth.lastGrantActionId)} expiresAt=${escapeHtml(String(state.strongAuth.lastGrantExpiresAtUnixMs || "-"))}</div>`
            : ""}
          ${state.strongAuth.lastGrantError
            ? `<div class="empty" style="color:var(--bad);">${escapeHtml(state.strongAuth.lastGrantError)}</div>`
            : ""}
        </div>
      </div>
      <div class="panel panel--nested" style="background:rgba(255,255,255,0.02);">
        <div class="panel__header"><div class="panel__title">Asset / Governance Lane</div></div>
        <div class="panel__body stack">
          <div class="badge-row">
            <span class="${mainTokenTransferCapability.enabled ? "badge badge--good" : "badge badge--warn"}">main_token_transfer=${escapeHtml(assetLaneStatusText)}</span>
            <span class="badge">required_auth=${escapeHtml(mainTokenTransferPolicy?.required_auth || "-")}</span>
            <span class="badge">availability=${escapeHtml(mainTokenTransferPolicy?.availability || "-")}</span>
          </div>
          <div class="empty">${escapeHtml(assetLaneDetail)}</div>
          <div class="empty">${escapeHtml(mainTokenTransferPolicy?.reason || "No hosted action policy is available for main_token_transfer on this lane.")}</div>
          <div class="toolbar">
            <button disabled>Main Token Transfer (Not Exposed Here Yet)</button>
          </div>
        </div>
      </div>
      <div class="panel panel--nested" style="background:rgba(255,255,255,0.02);">
        <div class="panel__header"><div class="panel__title">Agent Chat</div></div>
        <div class="panel__body stack">
          <div class="field">
            ${safeAlternativeMarkup(state, escapeHtml)}<label for="agent-chat-message">Message</label>${canonicalGoalMarkup(state, selectedAgentId(), escapeHtml)}
            <textarea id="agent-chat-message" rows="4" placeholder="Send a message to the selected agent" ${chatCapability.enabled ? "" : "disabled"}>${escapeHtml(state.chatDraft.message)}</textarea>
          </div>
          <div class="toolbar">
            <button data-chat-send="1" ${chatCapability.enabled ? "" : "disabled"}>Send Chat</button>
          </div>
          ${chatFeedback
            ? `<div class="badge-row"><span class="${feedbackBadgeClass(chatFeedback)}">${escapeHtml(chatFeedback.stage)}</span></div>
               <pre class="json">${escapeHtml(JSON.stringify(chatFeedback, null, 2))}</pre>`
            : '<div class="empty">No chat feedback yet.</div>'}
          <div>
            <div class="panel__title" style="margin-bottom:10px;">Message Flow</div>
            <div class="event-list">
              ${chatHistory.length
                ? chatHistory
                    .map(
                      (entry) => `
                        <div class="event-card">
                          <div class="event-card__title">
                            <span>${escapeHtml(entry.source === "player" ? `player → ${entry.targetAgentId || entry.agentId || "agent"}` : `${entry.agentId || "agent"} spoke`)}</span>
                            <span>tick=${Number(entry.tick || 0)}</span>
                          </div>
                          <div class="event-card__meta">speaker=${escapeHtml(entry.speaker || entry.playerId || "-")} · location=${escapeHtml(entry.locationId || "-")}</div>
                          <pre class="json">${escapeHtml(JSON.stringify(entry, null, 2))}</pre>
                        </div>`,
                    )
                    .join("")
                : '<div class="empty">No chat history for this agent yet.</div>'}
            </div>
          </div>
        </div>
      </div>
    </div>
  `;
}

function renderDetails() {
  const selectedLabel = state.selectedKind && state.selectedId
    ? `${state.selectedKind}:${state.selectedId}`
    : "nothing selected";
  elements.rightPanel.innerHTML = `
    <div class="stack">
      <div class="badge-row">
        <span class="badge badge--accent">Selected</span>
        <span class="badge">${escapeHtml(selectedLabel)}</span>
      </div>
      ${renderInteractionPanel()}
      ${state.selectedObject
        ? `<pre class="json">${escapeHtml(JSON.stringify(state.selectedObject, null, 2))}</pre>`
        : '<div class="empty">Select an agent or location from Targets first.</div>'}
      <div>
        <div class="panel__title" style="margin-bottom:10px;">Snapshot Summary</div>
        <pre class="json">${escapeHtml(
          JSON.stringify(
            {
              config: state.snapshot?.config || null,
              counts: {
                agents: Object.keys(state.snapshot?.model?.agents || {}).length,
                locations: Object.keys(state.snapshot?.model?.locations || {}).length,
                promptProfiles: Object.keys(state.snapshot?.model?.agent_prompt_profiles || {}).length,
                executionDebugContexts: Object.keys(state.snapshot?.model?.agent_execution_debug_contexts || {}).length,
              },
    metrics: state.metrics,
    hostedAccess: clone(state.hostedAccess),
            },
            null,
            2,
          ),
        )}</pre>
      </div>
      ${state.lastError
        ? `<div>
            <div class="panel__title" style="margin-bottom:10px; color: var(--bad);">Last Error</div>
            <pre class="json">${escapeHtml(state.lastError)}</pre>
          </div>`
        : ""}
    </div>
  `;
}

function bindEvents() {
  const searchInput = document.getElementById("entity-search");
  if (searchInput) {
    searchInput.addEventListener("input", (event) => {
      state.selectedSearch = String(event.target.value || "");
      renderLists();
      bindEvents();
    });
  }

  document.querySelectorAll("[data-select-kind][data-select-id]").forEach((button) => {
    button.addEventListener("click", () => {
      applySelection({
        kind: button.getAttribute("data-select-kind"),
        id: button.getAttribute("data-select-id"),
      });
    });
  });

  document.querySelectorAll("[data-action]").forEach((button) => {
    button.addEventListener("click", () => {
      const action = button.getAttribute("data-action");
      if (action === "step-count") {
        const value = Number(document.getElementById("step-count")?.value || 1);
        sendControl("step", { count: Math.max(1, Math.floor(value || 1)) });
        return;
      }
      sendControl(action, null);
    });
  });

  const promptSystem = document.getElementById("prompt-system");
  if (promptSystem) {
    promptSystem.addEventListener("input", (event) => {
      state.promptDraft.systemPrompt = String(event.target.value || "");
      state.promptDraft.dirty = true;
    });
  }
  const promptShort = document.getElementById("prompt-short");
  if (promptShort) {
    promptShort.addEventListener("input", (event) => {
      state.promptDraft.shortTermGoal = String(event.target.value || "");
      state.promptDraft.dirty = true;
    });
  }
  const promptLong = document.getElementById("prompt-long");
  if (promptLong) {
    promptLong.addEventListener("input", (event) => {
      state.promptDraft.longTermGoal = String(event.target.value || "");
      state.promptDraft.dirty = true;
    });
  }
  const promptRollbackVersion = document.getElementById("prompt-rollback-version");
  if (promptRollbackVersion) {
    promptRollbackVersion.addEventListener("input", (event) => {
      const nextValue = Number(event.target.value || 0);
      state.promptDraft.rollbackTargetVersion = Math.max(0, Math.floor(nextValue || 0));
    });
  }
  const strongAuthApprovalCode = document.getElementById("strong-auth-approval-code");
  if (strongAuthApprovalCode) {
    strongAuthApprovalCode.addEventListener("input", (event) => {
      state.strongAuth.approvalCode = String(event.target.value || "");
    });
  }
  document.querySelectorAll("[data-prompt-action]").forEach((button) => {
    button.addEventListener("click", () => {
      const action = button.getAttribute("data-prompt-action");
      if (action === "rollback") {
        sendPromptControl("rollback", {
          toVersion: Number(state.promptDraft.rollbackTargetVersion || 0),
        });
        return;
      }
      sendPromptControl(action, null);
    });
  });

  bindSafeAlternativeLegacy(state, safeAlternative, render);
  const chatMessage = document.getElementById("agent-chat-message");
  if (chatMessage) {
    chatMessage.addEventListener("input", (event) => {
      state.chatDraft.message = String(event.target.value || "");
      state.chatDraft.dirty = true;
    });
  }
  document.querySelectorAll("[data-chat-send]").forEach((button) => {
    button.addEventListener("click", () => {
      sendAgentChat(selectedAgentId(), state.chatDraft.message);
    });
  });
  document.querySelectorAll("[data-auth-action]").forEach((button) => {
    button.addEventListener("click", () => {
      const action = button.getAttribute("data-auth-action");
      if (action === "logout") {
        void logoutHostedPlayerSession();
        return;
      }
      if (action === "retry-issue") {
        void retryHostedPlayerIdentityIssue();
      }
    });
  });
}

function render(invalidation) {
  renderHook.invoke(invalidation);
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
  if (bootstrapped) return bootstrapPromise;
  bootstrapped = true;
  bootstrapPromise = bootstrap().catch((error) => { state.connectionStatus = "error"; state.auth.error = String(error); state.lastError = String(error); render(); throw error; });
  return bootstrapPromise;
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
  bindEvents,
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
  isAgentControlLost,
  isAgentVisibleToCurrentSession,
  modelLists,
  needsEmptyEntitySnapshotRefreshForTest,
  pushChatHistory,
  refreshHostedAdmissionState,
  requestRender,
  renderInteractionPanel,
  renderLists,
  renderSummary,
  renderDetails,
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

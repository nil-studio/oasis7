import { canonicalAgentChatView } from "./viewer_canonical_owner_read_module.js";
import {
  LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE,
  isHostedPublicJoinDeploymentMode,
} from "./software_safe_constants.js";

export function createViewerAuthSurfaceModule({
  getSearchParams,
  localeText,
  state,
  windowRef,
}) {
  function resolveHostedAccessHint() {
    const raw = getSearchParams().get("hosted_access");
    if (!raw) {
      return null;
    }
    try {
      const parsed = JSON.parse(raw);
      return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed : null;
    } catch (_) {
      return null;
    }
  }

  function hostnameFromUrl(raw) {
    const value = String(raw || "").trim();
    if (!value) return null;
    try {
      return new URL(value, windowRef.location.href).hostname || null;
    } catch (_) {
      return null;
    }
  }

  function isLoopbackHostname(raw) {
    const value = String(raw || "").trim().toLowerCase();
    return value === "localhost" || value === "127.0.0.1" || value === "::1" || value === "[::1]";
  }

  function authDeploymentHint(auth) {
    const hostedMode = String(state.hostedAccess?.deployment_mode || "").trim();
    if (isHostedPublicJoinDeploymentMode(hostedMode)) {
      if (auth.available && auth.source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE) {
        return "hosted_public_join_contract_with_legacy_bootstrap";
      }
      return auth.available
        ? "hosted_public_join_contract_with_browser_session"
        : "hosted_public_join_contract";
    }
    if (hostedMode === "trusted_local_only") {
      return auth.available ? "trusted_local_contract" : "trusted_local_contract_guest";
    }
    const params = getSearchParams();
    const wsHost = hostnameFromUrl(state.wsUrl || params.get("ws") || params.get("addr") || "");
    const pageHost = String(windowRef.location.hostname || "").trim();
    const remoteOriginLikely = [pageHost, wsHost].filter(Boolean).some((host) => !isLoopbackHostname(host));
    if (auth.available) {
      return remoteOriginLikely ? "remote_origin_legacy_bootstrap" : "trusted_local_preview";
    }
    return remoteOriginLikely ? "hosted_public_join_likely" : "guest_only_or_missing_bootstrap";
  }

  function isHostedPublicJoinHint(deploymentHint) {
    return [
      "hosted_public_join_contract",
      "hosted_public_join_contract_with_browser_session",
      "hosted_public_join_contract_with_legacy_bootstrap",
      "hosted_public_join_likely",
    ].includes(deploymentHint);
  }

  function hostedActionPolicy(actionId) {
    const normalizedActionId = actionId === "prompt_control"
      ? "prompt_control_apply"
      : actionId;
    return state.hostedAccess?.action_matrix?.find((policy) => policy?.action_id === normalizedActionId) || null;
  }

  function guestSessionReason(auth, deploymentHint) {
    if (auth.available) {
      return auth.source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE
        ? "guest session has already been superseded by the current preview player auth lane"
        : "guest session has already been superseded by a hosted-issued player identity";
    }
    if (isHostedPublicJoinHint(deploymentHint)) {
      return auth.error || "this browser is still guest-only; hosted public join must complete hosted account login before low-risk interaction unlocks";
    }
    return auth.error || "viewer auth bootstrap is unavailable, so the browser cannot leave guest session";
  }

  function playerSessionReason(auth, deploymentHint) {
    if (auth.available) {
      if (auth.source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE) {
        return "player interaction is currently unlocked through legacy viewer auth bootstrap in trusted preview mode";
      }
      if (auth.registrationStatus === "registered") {
        return "player interaction is unlocked through hosted-issued player_id + browser device session plus an in-memory browser-generated Ed25519 session key";
      }
      if (auth.registrationStatus === "registering" || auth.registrationStatus === "issued") {
        return "browser device session is ready; runtime player-session registration is still in progress";
      }
      return auth.error || "hosted player identity exists, but runtime registration still needs recovery";
    }
    if (isHostedPublicJoinHint(deploymentHint)) {
      return auth.error || "player session upgrade/login is still pending hosted account verification";
    }
    return auth.error || "viewer auth bootstrap is missing or incomplete";
  }

  function strongAuthReason() {
    return "strong auth remains a separate upgrade plane; viewer already supports hosted player-session issue/reconnect/release, but backend reauth stays preview-only for prompt_control and still does not unlock hosted access ready asset/governance proofs";
  }

  function buildStrongAuthTier() {
    const promptPolicy = hostedActionPolicy("prompt_control");
    if (!promptPolicy || promptPolicy.required_auth !== "strong_auth") {
      return {
        status: "separate_upgrade_plane",
        reason: strongAuthReason(),
      };
    }
    if (promptPolicy.availability === "public_player_plane_with_backend_reauth_preview") {
      if (!state.auth.available) {
        return {
          status: "upgrade_after_player_session",
          reason:
            "hosted preview backend reauth is available on this join lane after the browser acquires a player_session",
        };
      }
      if (state.auth.registrationStatus === "registered") {
        return {
          status: "preview_backend_reauth_available",
          reason:
            "hosted preview backend reauth is available after the browser device-session-backed player_session has completed runtime registration for prompt_control",
        };
      }
      return {
        status: "issued_pending_register",
        reason:
          "hosted preview backend reauth stays pending until the browser device-session-backed player_session finishes runtime registration",
      };
    }
    if (promptPolicy.availability === "trusted_local_preview_only") {
      return {
        status: state.auth.available ? "active_legacy_preview" : "trusted_local_only",
        reason:
          "trusted_local_preview keeps prompt_control on the legacy local preview lane; hosted/public strong_auth still remains outside this window",
      };
    }
    return {
      status: "blocked_until_strong_auth",
      reason: promptPolicy.reason || strongAuthReason(),
    };
  }

  function isStrongAuthSensitiveAction(actionId) {
    const policy = hostedActionPolicy(actionId);
    if (policy) {
      return policy.required_auth === "strong_auth";
    }
    return actionId === "prompt_control" || actionId === "main_token_transfer";
  }

  function promptControlProtocolGate() {
    const protocol = state.viewerProtocol;
    if (!protocol || protocol.negotiated == null) {
      return null;
    }
    if (protocol.negotiated === false) {
      return {
        code: "prompt_protocol_negotiating",
        reason: "waiting for runtime hello_v2 prompt-control capability negotiation",
      };
    }
    const capabilities = Array.isArray(protocol.capabilities) ? protocol.capabilities : [];
    if (!capabilities.includes("prompt_control_result_v1")) {
      return null;
    }
    if (!String(protocol.authorityEpoch || state.auth.authorityEpoch || "").trim()) {
      return {
        code: "prompt_authority_epoch_unavailable",
        reason: "prompt control is waiting for a current runtime authority epoch",
      };
    }
    if (state.auth.registrationStatus !== "registered" || state.auth.sessionEpoch == null) {
      return {
        code: "prompt_session_not_ready",
        reason: "prompt control is waiting for a current registered player session",
      };
    }
    if (!String(state.auth.boundAgentId || "").trim() || state.auth.bindingEpoch == null) {
      return {
        code: "prompt_binding_not_ready",
        reason: "prompt control is waiting for a current Agent binding recovery ack",
      };
    }
    if (!Number.isSafeInteger(Number(state.promptDraft?.currentVersion))) {
      return {
        code: "prompt_version_unavailable",
        reason: "prompt control is waiting for the authorized prompt version",
      };
    }
    return null;
  }

  function agentChatProtocolGate() {
    if (!state.viewerProtocol?.capabilities?.includes("canonical_agent_chat_v1")) return null;
    const view = canonicalAgentChatView(state);
    const extension = view?.canonical_authority;
    if (state.auth.registrationStatus !== "registered" || !state.auth.boundAgentId || !view
      || !["ready", "replay", "empty"].includes(state.worldFeed?.status)
      || view.authority_scope !== "player_agent_chat"
      || typeof extension?.branch_id !== "string" || !extension.branch_id.trim()
      || !Number.isSafeInteger(extension.agent_identity_generation) || extension.agent_identity_generation <= 0) {
      return { code: "canonical_owner_not_ready", reason: localeText(state.uiLocale,
        "正在核对你对该 Agent 的控制资格。请等待同步；若未恢复，请刷新玩法快照。",
        "Your control of this Agent is being verified. Wait for sync, or refresh the gameplay snapshot if it does not recover.") };
    }
    return null;
  }

  function buildSemanticCapability(actionId) {
    const deploymentHint = authDeploymentHint(state.auth);
    const strongAuthSensitive = isStrongAuthSensitiveAction(actionId);
    const policy = hostedActionPolicy(actionId);
    const protocolGate = actionId === "prompt_control" ? promptControlProtocolGate()
      : actionId === "agent_chat" ? agentChatProtocolGate() : null;
    const finalize = (result) => {
      if (protocolGate && result.enabled) {
        return {
          ...result,
          enabled: false,
          code: protocolGate.code,
          reason: protocolGate.reason,
        };
      }
      return result;
    };
    if (policy) {
      if (policy.required_auth === "strong_auth") {
        const isLocalPreviewOnly = policy.availability === "trusted_local_preview_only";
        const isBackendGrantPreview = policy.availability === "public_player_plane_with_backend_reauth_preview";
        if (isLocalPreviewOnly && state.auth.available && !isHostedPublicJoinHint(deploymentHint)) {
          return finalize({
            actionId,
            enabled: true,
            code: null,
            reason: policy.reason || "trusted local preview currently allows this strong-auth-marked action through preview bootstrap",
          });
        }
        if (isBackendGrantPreview && state.auth.available) {
          return finalize({
            actionId,
            enabled: true,
            code: null,
            reason: policy.reason || `${actionId} is available through browser-local player auth plus backend re-authorization`,
          });
        }
        if (isBackendGrantPreview && !state.auth.available) {
          return {
            actionId,
            enabled: false,
            code: "auth_level_insufficient",
            reason: `${actionId} requires player_session before backend re-authorization can upgrade it to strong_auth`,
          };
        }
        return finalize({
          actionId,
          enabled: false,
          code: "strong_auth_required",
          reason: policy.reason || strongAuthReason(),
        });
      }
      if (!state.auth.available) {
        return finalize({
          actionId,
          enabled: false,
          code: "auth_level_insufficient",
          reason: `${actionId} requires ${policy.required_auth}; current browser remains guest_session only`,
        });
      }
      return finalize({
        actionId,
        enabled: true,
        code: null,
        reason: policy.reason || `${actionId} is allowed on the ${policy.required_auth} lane`,
      });
    }
    if (strongAuthSensitive && isHostedPublicJoinHint(deploymentHint)) {
      const hostedStrongAuthReason = state.auth.available
        ? `${actionId} still requires strong_auth on the hosted public join path; this browser only has a legacy preview player_session, so backend re-authorization or a private operator plane must take over`
        : `${actionId} requires strong_auth on the hosted public join path; acquire a player_session first, then complete the hosted re-authorization step for this action`;
      return finalize({
        actionId,
        enabled: false,
        code: "strong_auth_required",
        reason: hostedStrongAuthReason,
      });
    }
    if (strongAuthSensitive && state.auth.available && deploymentHint === "remote_origin_legacy_bootstrap") {
      return finalize({
        actionId,
        enabled: false,
        code: "strong_auth_required",
        reason: `${actionId} is blocked on remote-origin legacy bootstrap; hosted/public prompt control must move to strong_auth or private operator plane`,
      });
    }
    if (!state.auth.available) {
      const reason = isHostedPublicJoinHint(deploymentHint)
        ? `${actionId} requires player_session; this browser is still guest_session only on the hosted public join path`
        : `${actionId} requires viewer auth bootstrap; current status: ${state.auth.error || "missing"}`;
      return finalize({
        actionId,
        enabled: false,
        code: "auth_level_insufficient",
        reason,
      });
    }
    return finalize({
      actionId,
      enabled: true,
      code: null,
      reason: strongAuthSensitive
        ? "prompt_control stays enabled only in trusted_local_preview via legacy viewer auth bootstrap; hosted/public strong_auth remains pending"
        : "player_session is active via legacy viewer auth bootstrap preview",
    });
  }

  function buildAuthSurfaceModel() {
    const deploymentHint = authDeploymentHint(state.auth);
    const promptCapability = buildSemanticCapability("prompt_control");
    const chatCapability = buildSemanticCapability("agent_chat");
    const mainTokenTransferCapability = buildSemanticCapability("main_token_transfer");
    const strongAuthTier = buildStrongAuthTier();
    const currentTier = state.auth.available ? "player_session" : "guest_session";
    const source = isHostedPublicJoinDeploymentMode(state.hostedAccess?.deployment_mode)
      ? state.auth.available
        ? state.auth.source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE
          ? `${LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE}+hosted_access_hint`
          : "hosted_player_issue+browser_local_device_session"
        : "hosted_access_hint"
      : state.auth.available
        ? state.auth.source
        : "guest_only";
    return {
      deploymentHint,
      source,
      currentTier,
      currentTierReason:
        currentTier === "player_session"
          ? playerSessionReason(state.auth, deploymentHint)
          : guestSessionReason(state.auth, deploymentHint),
      tiers: [
        {
          id: "guest_session",
          label: "guest_session",
          status: state.auth.available ? "superseded" : "active",
          reason: guestSessionReason(state.auth, deploymentHint),
        },
        {
          id: "player_session",
          label: "player_session",
          status: state.auth.available
            ? state.auth.source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE
              ? "active_legacy_preview"
              : state.auth.registrationStatus === "registered"
                ? "active_hosted_session"
                : "issued_pending_register"
            : "not_issued",
          reason: playerSessionReason(state.auth, deploymentHint),
        },
        {
          id: "strong_auth",
          label: "strong_auth",
          status: strongAuthTier.status,
          reason: strongAuthTier.reason,
        },
      ],
      capabilities: {
        prompt_control: promptCapability,
        agent_chat: chatCapability,
        main_token_transfer: mainTokenTransferCapability,
        strong_auth_actions: mainTokenTransferCapability,
      },
      reconnect: state.auth.available
        ? state.auth.source === LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE
          ? "reconnect still depends on the current preview bootstrap; hosted player-session reconnect/release is available only after switching away from legacy bootstrap"
          : state.auth.registrationStatus === "registered"
            ? "page reload will reuse the hosted device session, mint a fresh browser session key, and attempt reconnect_sync first"
            : "hosted device session is persisted locally, but runtime player-session restore is still pending this page load"
        : isHostedPublicJoinHint(deploymentHint)
          ? buildHostedRecoveryHint("en")?.detail
            || "hosted public join recovers by acquiring a player_session first, then re-registering it through reconnect_sync"
          : "page reload is possible once viewer auth bootstrap or hosted account login succeeds",
    };
  }

  function buildHostedActionMatrixView() {
    const matrix = Array.isArray(state.hostedAccess?.action_matrix)
      ? state.hostedAccess.action_matrix
      : [];
    return matrix.map((policy) => {
      const actionId = String(policy?.action_id || "").trim();
      const capability = buildSemanticCapability(actionId);
      return {
        actionId,
        requiredAuth: String(policy?.required_auth || "").trim() || "unknown",
        availability: String(policy?.availability || "").trim() || "unknown",
        reason: String(policy?.reason || capability.reason || "").trim(),
        enabled: capability.enabled === true,
        code: capability.code || null,
        capabilityReason: capability.reason || null,
      };
    });
  }

  function buildHostedRecoveryHint(locale = state.uiLocale) {
    if (!isHostedPublicJoinDeploymentMode(state.hostedAccess?.deployment_mode)) {
      return null;
    }
    if (state.auth.available) {
      return null;
    }
    const errorText = String(state.auth.error || "").trim();
    const revokeReason = String(state.auth.revokeReason || "").trim();
    const revokedBy = String(state.auth.revokedBy || "").trim();
    if (!errorText) {
      return null;
    }
    if (errorText.includes("released locally")) {
      return {
        kind: "released",
        title: localeText(locale, "当前浏览器已主动释放会话", "This browser already released its session"),
        detail: localeText(
          locale,
          "当前 player_session 已在本地释放。重新登录 Hosted Account 并领取新的 player_session 后，viewer 会再做 reconnect_sync。",
          "The current player_session was released locally. Re-login to the hosted account, acquire a fresh player session, and viewer will attempt reconnect_sync again.",
        ),
        cta: localeText(locale, "重新登录 Hosted Account", "Re-login to Hosted Account"),
      };
    }
    if (revokeReason === "agent_already_bound" || errorText.includes("agent is already bound")) {
      return {
        kind: "agent_already_bound",
        title: localeText(locale, "所选 Agent 已绑定其他玩家", "Selected agent is already bound"),
        detail: localeText(
          locale,
          "当前 Agent 已被其他 player_session 占用。请回到当前账号的 Agent 入口，认领或等待同步自己的 Agent。",
          "The selected Agent is already owned by another player_session. Return to this account's Agent entry, then claim or wait for your own Agent to sync.",
        ),
        cta: localeText(locale, "回到我的 Agent 入口", "Return to My Agent Entry"),
      };
    }
    if (revokeReason === "runtime_registration_failed") {
      return {
        kind: "runtime_registration_failed",
        title: localeText(locale, "Runtime 注册没有完成", "Runtime registration did not finish"),
        detail: localeText(
          locale,
          "浏览器已经拿到 hosted identity，但 runtime register/reconnect 失败。请重新登录 Hosted Account 并重试注册 / reconnect，再检查 launcher/runtime 日志。",
          "The browser already received a hosted identity, but runtime register/reconnect failed. Re-login to the hosted account and retry register / reconnect, then inspect launcher/runtime logs.",
        ),
        cta: localeText(locale, "重试注册 / reconnect", "Retry register / reconnect"),
      };
    }
    if (revokedBy) {
      return {
        kind: "revoked",
        title: localeText(locale, "当前会话已被回收", "The current session was revoked"),
        detail: localeText(
          locale,
          `运行时或操作员 ${revokedBy} 已回收当前浏览器会话，原因是 ${revokeReason || errorText || "unknown"}。需要重新登录 Hosted Account 并领取新的 player_session，玩法、聊天和 prompt 才能继续。`,
          `The runtime or operator revoked this browser session by ${revokedBy}. Reason: ${revokeReason || errorText || "unknown"}. You need to re-login to the hosted account and acquire a fresh player session before gameplay, chat, or prompt actions can continue.`,
        ),
        cta: localeText(locale, "重新登录 Hosted Account", "Re-login to Hosted Account"),
      };
    }
    return {
      kind: "issue_required",
      title: localeText(locale, "当前浏览器还没有 player_session", "This browser still has no player_session"),
      detail: localeText(
        locale,
        "当前 hosted public join 需要先完成 Hosted Account 登录并领取 player_session，再让 runtime 完成 register/reconnect。",
        "Hosted public join must complete hosted account login and acquire a player_session first, then let runtime finish register/reconnect.",
      ),
      cta: localeText(locale, "登录 Hosted Account", "Login to Hosted Account"),
    };
  }

  return {
    authDeploymentHint,
    buildAuthSurfaceModel,
    buildHostedActionMatrixView,
    buildHostedRecoveryHint,
    buildSemanticCapability,
    hostedActionPolicy,
    resolveHostedAccessHint,
  };
}

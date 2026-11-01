// Closed OwnerSetup V1 client. Browser copies stay in per-instance RAM; originals are sent to the selected custody service.
const API = "/api/public/hosted-account/v2";
export const CUSTODY_PATHS = Object.freeze({ parents_query_path: "/v1/management/parents/query",
  options_path: `${API}/custody/approvals/options`, complete_path: `${API}/custody/approvals/complete` });
const fail = code => Object.assign(new Error(code), { code });
// JSON object order is not protocol data; retain exact fields and array order.
function same(a, b) {
  if (a === b) return true;
  if (!a || !b || typeof a !== "object" || typeof b !== "object") return false;
  if (Array.isArray(a) || Array.isArray(b)) return Array.isArray(a) && Array.isArray(b)
    && a.length === b.length && a.every((value, index) => same(value, b[index]));
  const keys = Object.keys(a);
  return keys.length === Object.keys(b).length && keys.every(key => Object.hasOwn(b, key) && same(a[key], b[key]));
}
const id = value => typeof value === "string" && value.length > 0 && value.length <= 256 && !/[\u0000-\u001f\u007f]/.test(value);
const decimal = value => typeof value === "string" && /^[1-9][0-9]{0,19}$/.test(value);
const clone = value => structuredClone(value);
// Native DecimalU64 milliseconds stay lossless; never coerce server expiry to Number.
const expiryMillis = value => decimal(value) && BigInt(value) <= 18446744073709551615n;
function localMillis(now) {
  const value = now();
  if (!Number.isSafeInteger(value) || value < 0) throw fail("custody_clock_invalid");
  return BigInt(value);
}
// Native owns canonical hashes. The browser keeps exact returned commitments
// and checks their visible binding; it never reimplements Rust signing codecs.
function proxyBasisMatches(contract, budget, prepared, started, time) {
  if (!budget || budget.approval_contract !== "native_custody_model_proxy_v2"
    || contract?.api_version !== 2 || contract.contract !== "full_test_session_proxy_v2"
    || Object.keys(contract).sort().join(",") !== "api_version,basis,basis_digest,contract,full_session"
    || !/^[0-9a-f]{64}$/.test(contract.basis_digest)) return false;
  const b = contract.basis;
  if (b?.api_version !== 2 || Object.keys(b).sort().join(",") !== "api_version,approval_request_id,budget_digest,connection_generation,connection_ref_digest,context_digest,endpoint_policy_digest,expires_at,intent_digest,issued_at,ledger_id,model,native_account_id,native_instance_public_key,physical_graph_digest,preset_id,preset_revision,price_policy_digest"
    || b.approval_request_id !== prepared.approval_request_id || b.intent_digest !== started.intent_digest
    || !expiryMillis(budget.deadline) || b.ledger_id !== budget.ledger_id || b.native_account_id !== budget.native_account_id
    || b.native_instance_public_key !== budget.native_instance_public_key
    || b.preset_id !== budget.preset_id || b.preset_revision !== budget.preset_revision
    || b.model !== budget.model || b.physical_graph_digest !== budget.physical_graph_digest
    || !(b.connection_generation === "0" || expiryMillis(b.connection_generation)) || !expiryMillis(b.expires_at)
    || !(b.issued_at === "0" || expiryMillis(b.issued_at))
    || BigInt(b.issued_at) > time || BigInt(b.expires_at) <= time
    || BigInt(b.expires_at) > BigInt(budget.deadline)) return false;
  return [b.budget_digest, b.connection_ref_digest, b.context_digest, b.endpoint_policy_digest,
    b.native_instance_public_key, b.price_policy_digest, b.intent_digest].every(v => /^[0-9a-f]{64}$/.test(v));
}
function randomHex(crypto) { const bytes = new Uint8Array(32); crypto.getRandomValues(bytes); return [...bytes].map(b => b.toString(16).padStart(2, "0")).join(""); }
export function custodyCapability(value) {
  return value?.api_version === 1 && value.available === true && id(value.audience) && id(value.environment)
    && Object.entries(CUSTODY_PATHS).every(([key, path]) => value[key] === path);
}
export function createOwnerSetupClient({ fetch: fetchImpl = globalThis.fetch, credentials = globalThis.navigator?.credentials,
  crypto = globalThis.crypto, now = () => Date.now(), browserPasskeyOptions, serializePasskey } = {}) {
  let generation = 0, scope = null, pending = null, flight = null;
  const check = revision => { if (revision !== generation || !scope) throw fail("custody_session_changed"); };
  function reset() { generation += 1; pending = null; flight = null; }
  function setSession(account, authentication, capability) {
    const next = account?.account_id && authentication?.method === "webauthn" && custodyCapability(capability)
      ? { account: account.account_id, capability: clone(capability) } : null;
    if (!same(next, scope)) { reset(); scope = next; }
    return Boolean(scope);
  }
  async function request(path, body, revision) {
    check(revision);
    const headers = { Accept: "application/json" };
    if (body !== undefined) {
      const capability = await request(`${API}/capabilities`, undefined, revision);
      check(revision);
      if (typeof capability.csrf_token !== "string" || !capability.csrf_token) throw fail("custody_csrf_unavailable");
      if (!custodyCapability(capability.custody_management) || !same(capability.custody_management, scope.capability)) throw fail("custody_configuration_changed");
      headers["Content-Type"] = "application/json"; headers["X-Oasis7-CSRF"] = capability.csrf_token;
    }
    let response;
    try { response = await fetchImpl(path, { method: body === undefined ? "GET" : "POST", headers,
      credentials: "same-origin", cache: "no-store", ...(body === undefined ? {} : { body: JSON.stringify(body) }) }); }
    catch { check(revision); throw fail("custody_outcome_unknown"); }
    check(revision);
    let value;
    try { const text = await response.text(); if (new TextEncoder().encode(text).length > 65536) throw fail("size"); value = JSON.parse(text); }
    catch { throw fail("custody_response_invalid"); }
    check(revision);
    if (!response.ok || value?.ok === false) {
      const error = fail(response.status >= 500 || response.status === 202 ? "custody_outcome_unknown" : "custody_request_rejected");
      error.status = response.status; throw error;
    }
    if (!value || typeof value !== "object" || Array.isArray(value)) throw fail("custody_response_invalid");
    return value;
  }
  function oneFlight(action) {
    if (flight) return flight;
    const revision = generation;
    const promise = Promise.resolve().then(() => { check(revision); return action(revision); });
    flight = promise;
    promise.finally(() => { if (flight === promise) flight = null; }).catch(() => {});
    return promise;
  }
  function route(provision, method) { if (!id(provision)) throw fail("custody_parent_invalid"); return `/v1/account-provisions/${encodeURIComponent(provision)}/owner-setups/${method}`; }
  async function parents(cursor = null, revision = null) {
    return oneFlight(async current => {
      if ((cursor === null) !== (revision === null) || cursor !== null && (!id(cursor) || !id(revision))) throw fail("custody_cursor_invalid");
      const page = await request(CUSTODY_PATHS.parents_query_path, { api_version: 1, limit: 16,
        after_custody_account_id: cursor, directory_revision: revision }, current);
      if (page.api_version !== 1 || !id(page.directory_revision) || typeof page.frozen !== "boolean" || typeof page.inventory_complete !== "boolean"
        || !Array.isArray(page.items) || page.items.length > 16 || new TextEncoder().encode(JSON.stringify(page)).length > 32768
        || page.next_after_custody_account_id !== null && !id(page.next_after_custody_account_id)
        || revision !== null && revision !== page.directory_revision || page.items.some(item => !id(item.custody_account_id) || !id(item.provision_id)
          || item.environment !== scope.capability.environment || !id(item.world_id))) throw fail("custody_directory_invalid");
      return clone(page);
    });
  }
  function prepare(parent, goalText) {
    return oneFlight(async revision => {
      if (pending) throw fail("custody_original_pending");
      if (!id(parent?.custody_account_id) || !id(parent?.provision_id) || parent.environment !== scope.capability.environment || !id(parent.world_id)
        || typeof goalText !== "string" || !goalText.trim() || new TextEncoder().encode(goalText).length > 3500) throw fail("custody_plan_invalid");
      const basis = await request(route(parent.provision_id, "basis"), { api_version: 1 }, revision);
      if (basis.api_version !== 1 || basis.custody_account_id !== parent.custody_account_id || basis.provision_id !== parent.provision_id
        || basis.world_identity?.world_id !== parent.world_id || !id(basis.owner_key?.key_id) || !id(basis.executor_key?.key_id)
        || !decimal(basis.bootstrap_config_version)) throw fail("custody_basis_invalid");
      const prefix = `owner-setup-${randomHex(crypto)}`;
      const plan = { api_version: 1, setup_id: prefix, provision_id: basis.provision_id, participant_receipt_digest: basis.participant_receipt_digest,
        world_identity: clone(basis.world_identity), owner_key_ref: basis.owner_key.key_id, executor_key_ref: basis.executor_key.key_id,
        bootstrap_config_version: basis.bootstrap_config_version, goal_content: { goal_text: goalText, soft_constraints: [] },
        create_operation_id: `${prefix}-create`, prepare_operation_id: `${prefix}-prepare`, prepare_nonce: randomHex(crypto),
        publish_operation_id: `${prefix}-publish`, delegation_operation_id: `${prefix}-delegate`, bind_operation_id: `${prefix}-bind`,
        executor_id: `${prefix}-executor`, executor_public_key: basis.executor_key.public_key, delegation_generation: "1", delegation_nonce: "1",
        resource_refs: [], deadline: String(now() + 120000), request_limit: "6", transport_limit: "584", write_limit: "4" };
      const original = { api_version: 1, approval_request_id: `${prefix}-approval`, plan };
      // Freeze before any request: an unknown prepare reuses these exact IDs and bytes.
      pending = { parent: clone(parent), basis: clone(basis), plan, original, prepared: null, complete: null, approved: null,
        completeUnknown: false, approveUnknown: false, cancelled: false, ceremonyExpiresAt: null, state: "prepare_unknown" };
      return prepareOriginal(revision);
    });
  }
  function prepareV2(parent, original) {
    return oneFlight(async revision => {
      if (pending) throw fail("custody_original_pending");
      const plan = original?.plan;
      if (original?.api_version !== 2 || plan?.api_version !== 2 || !id(original.approval_request_id)
        || !Object.hasOwn(plan, "model_budget") || !id(plan.setup_id) || !id(parent?.custody_account_id)
        || !id(parent?.provision_id) || parent.environment !== scope.capability.environment
        || plan.provision_id !== parent.provision_id || plan.world_identity?.world_id !== parent.world_id
        || !expiryMillis(plan.deadline) || BigInt(plan.deadline) <= localMillis(now)) throw fail("custody_plan_invalid");
      // This explicit API accepts a frozen typed V2 draft; it never upgrades the
      // ordinary V1 Move0 builder or chooses a service, key, preset or graph.
      pending = { parent: clone(parent), plan: clone(plan), original: clone(original), prepared: null,
        complete: null, approved: null, completeUnknown: false, approveUnknown: false,
        cancelled: false, ceremonyExpiresAt: null, state: "prepare_unknown" };
      return prepareOriginal(revision);
    });
  }
  function prepareSessionV2(parent, original) {
    return oneFlight(async revision => {
      if (pending) throw fail("custody_original_pending");
      const plan = original?.plan, sessionScope = original?.scope;
      if (original?.api_version !== 2 || plan?.api_version !== 2 || !id(original.approval_request_id)
        || !Object.hasOwn(plan, "model_budget") || !plan.termination || !id(sessionScope?.session_id)
        || !id(parent?.custody_account_id) || !id(parent?.provision_id)
        || parent.environment !== scope.capability.environment || plan.termination.provision_id !== parent.provision_id
        || sessionScope.client_pairing?.target?.custody_account_id !== parent.custody_account_id
        || sessionScope.client_pairing?.target?.provision_id !== parent.provision_id
        || plan.termination.session_id !== sessionScope.session_id
        || !expiryMillis(plan.write_deadline) || BigInt(plan.write_deadline) <= localMillis(now)) throw fail("custody_plan_invalid");
      pending = { kind: "session_v2", parent: clone(parent), plan: clone(plan), sessionScope: clone(sessionScope),
        original: clone(original), prepared: null, complete: null, approved: null,
        completeUnknown: false, approveUnknown: false, cancelled: false, ceremonyExpiresAt: null, state: "prepare_unknown" };
      return prepareOriginal(revision);
    });
  }
  function pendingRoute(method) {
    if (pending.kind !== "session_v2") return route(pending.parent.provision_id, method);
    return `/v1/account-provisions/${encodeURIComponent(pending.parent.provision_id)}/test-sessions/${method}`;
  }
  async function prepareOriginal(revision) {
    const prepared = await request(pendingRoute("prepare"), pending.original, revision);
    const { context, intent } = prepared;
    if (prepared.approval_request_id !== pending.original.approval_request_id || context?.custody_account_id !== pending.parent.custody_account_id
      || context?.environment !== scope.capability.environment || context?.audience !== scope.capability.audience
      || !decimal(context?.policy_version) || !decimal(context?.security_version) || !decimal(context?.service_epoch)
      ) throw fail("custody_prepare_invalid");
    if (pending.kind === "session_v2") {
      if (intent?.kind !== "test_session_v2" || !same(intent.plan, pending.plan)
        || !same(intent.scope, pending.sessionScope)) throw fail("custody_prepare_invalid");
    } else if (pending.plan.api_version === 2) {
      if (intent?.kind !== "owner_setup_v2" || !same(intent.plan, pending.plan)) throw fail("custody_prepare_invalid");
    } else if (intent?.kind !== "owner_setup" || intent.setup_id !== pending.plan.setup_id || intent.provision_id !== pending.plan.provision_id
      || !same(intent.world_identity, pending.plan.world_identity) || intent.participant_receipt_digest !== pending.plan.participant_receipt_digest
      || intent.owner_key_ref !== pending.plan.owner_key_ref || intent.executor_key_ref !== pending.plan.executor_key_ref
      || intent.bootstrap_config_version !== pending.plan.bootstrap_config_version || intent.deadline !== pending.plan.deadline
      || intent.request_limit !== "6" || intent.transport_limit !== "584" || intent.write_limit !== "4"
      || !/^[0-9a-f]{64}$/.test(intent.setup_plan_digest)) throw fail("custody_prepare_invalid");
    pending.prepared = clone(prepared); pending.state = "prepared"; return summary();
  }
  function summary() { return pending ? clone({ parent: pending.parent, plan: pending.plan, kind: pending.kind, sessionScope: pending.sessionScope, prepared: pending.prepared, state: pending.state,
    completeUnknown: pending.completeUnknown, approveUnknown: pending.approveUnknown, cancelled: pending.cancelled, ceremonyExpiresAt: pending.ceremonyExpiresAt }) : null; }
  async function completeOriginal(revision) {
    pending.completeUnknown = true; pending.state = "complete_unknown";
    const result = await request(CUSTODY_PATHS.complete_path, pending.complete, revision);
    if (result.api_version !== 1 || result.phase !== "completed" || !result.approval?.proof || typeof result.approval.signature !== "string"
      || result.approval.proof.approval_id !== pending.complete.approval_id || result.approval.proof.intent_digest !== pending.complete.intent_digest
      || !same(result.approval.proof.context, pending.prepared.context)) throw fail("custody_approval_invalid");
    if ((pending.kind === "session_v2" || pending.plan.api_version === 2 && pending.plan.model_budget !== null)
      && !same(result.approval.proof.model_budget_summary_contract, pending.nativeBudgetContract)) throw fail("custody_approval_invalid");
    pending.approved = { api_version: pending.plan.api_version, input: { operation_id: pending.kind === "session_v2" ? pending.sessionScope.session_id : pending.plan.setup_id,
      request: clone(pending.prepared), approval: clone(result.approval) }, ...(pending.kind === "session_v2" ? {} : { plan: clone(pending.plan) }) };
    pending.complete = null; pending.completeUnknown = false; return approveOriginal(revision);
  }
  async function approveOriginal(revision) {
    pending.approveUnknown = true; pending.state = "approve_unknown";
    const progress = await request(pendingRoute("approve"), pending.approved, revision);
    if (pending.kind === "session_v2") {
      if (progress.api_version !== 2 || progress.custody_account_id !== pending.parent.custody_account_id
        || progress.provision_id !== pending.parent.provision_id || progress.session_id !== pending.sessionScope.session_id
        || progress.phase !== "completed") throw fail("custody_progress_invalid");
      pending.approved = null; pending.approveUnknown = false; pending.state = "approved";
      return { stage: "v2_session_approval_completed", session_id: progress.session_id, execution_started: false };
    }
    if (pending.plan.api_version === 2) {
      if (progress.api_version !== 2 || progress.custody_account_id !== pending.parent.custody_account_id
        || progress.provision_id !== pending.parent.provision_id || progress.setup_id !== pending.plan.setup_id || progress.phase !== "completed") throw fail("custody_progress_invalid");
      pending.approved = null; pending.approveUnknown = false; pending.state = "approved";
      return { stage: "v2_approval_completed", setup_id: progress.setup_id, execution_started: false };
    }
    if (progress.api_version !== 1 || progress.setup_id !== pending.plan.setup_id || progress.custody_account_id !== pending.parent.custody_account_id
      || progress.provision_id !== pending.parent.provision_id || typeof progress.stage !== "string") throw fail("custody_progress_invalid");
    pending.approved = null; pending.approveUnknown = false; pending.state = "approved";
    return { stage: progress.stage, setup_id: progress.setup_id, execution_started: false };
  }
  function approve({ confirmModelBudget, confirmSession } = {}) {
    return oneFlight(async revision => {
      if (!pending?.prepared || pending.cancelled || pending.state !== "prepared" || !credentials?.get) throw fail("custody_not_ready");
      const started = await request(CUSTODY_PATHS.options_path, pending.prepared, revision);
      if (started.api_version !== 1 || started.phase !== "pending" || !id(started.approval_id) || !id(started.ceremony_id) || !id(started.operation_id)
        || !/^[0-9a-f]{64}$/.test(started.intent_digest) || started.audience !== scope.capability.audience || !started.options
        || !expiryMillis(started.ceremony_expires_at) || BigInt(started.ceremony_expires_at) <= localMillis(now)) throw fail("custody_options_invalid");
      pending.ceremonyExpiresAt = started.ceremony_expires_at;
      const budget = pending.plan.api_version === 2 ? pending.plan.model_budget : null;
      const full = pending.kind === "session_v2";
      const fullContract = started.model_budget_summary_contract;
      const proxy = full && fullContract?.contract === "full_test_session_proxy_v2";
      const fullBase = proxy ? fullContract.full_session : fullContract;
      if (proxy && !proxyBasisMatches(fullContract, budget, pending.prepared, started, localMillis(now))) throw fail("custody_budget_summary_required");
      if (full && (fullBase?.api_version !== 2 || fullBase.contract !== "full_test_session_v2"
        || Object.keys(fullBase).sort().join(",") !== "api_version,contract,model_budget_contract,summary_digest"
        || !/^[0-9a-f]{64}$/.test(fullBase.summary_digest)
        || !same(started.custody_session_summary_v2, { api_version: 2, plan: pending.plan, scope: pending.sessionScope })
        || typeof confirmSession !== "function")) throw fail("custody_session_summary_required");
      if (budget !== null && budget !== undefined) {
        const contract = full ? fullBase.model_budget_contract : started.model_budget_summary_contract;
        if ([budget.max_calls, budget.max_input_tokens, budget.max_output_tokens, budget.max_total_tokens,
          budget.max_concurrent, budget.inference_physical_http, budget.deadline, budget.preset_revision,
          budget.price_policy?.valid_until, budget.price_policy?.max_total_micro_units].some(v => !expiryMillis(v))
          || !Array.isArray(budget.probe_physical_http) || budget.probe_physical_http.length !== 2
          || budget.probe_physical_http.some(v => !expiryMillis(v))
          || budget.max_concurrent !== "1" || budget.approval_contract !== (proxy ? "native_custody_model_proxy_v2" : "native_custody_model_budget_v1")
          || !same(started.model_budget_summary, budget) || contract?.api_version !== 1
          || Object.keys(contract).sort().join(",") !== "api_version,summary_digest"
          || !/^[0-9a-f]{64}$/.test(contract.summary_digest) || !full && typeof confirmModelBudget !== "function") throw fail("custody_budget_summary_required");
        pending.state = "budget_summary";
        try { if (!full) await confirmModelBudget(clone({ parent: pending.parent, plan: pending.plan,
          context: pending.prepared.context, budget, contract })); }
        catch (error) { check(revision); pending.state = "prepared"; throw error; }
        check(revision);
        if (BigInt(started.ceremony_expires_at) <= localMillis(now) || BigInt(full ? pending.plan.write_deadline : pending.plan.deadline) <= localMillis(now)
          || BigInt(budget.deadline) <= localMillis(now)) {
          pending.state = "prepared"; throw fail("custody_budget_summary_expired");
        }
        pending.nativeBudgetContract = clone(contract);
      } else if (started.model_budget_summary != null || (full ? fullBase.model_budget_contract !== null : started.model_budget_summary_contract != null)) throw fail("custody_options_invalid");
      if (full) {
        pending.state = "session_summary";
        try { await confirmSession(clone({ parent: pending.parent, plan: pending.plan, scope: pending.sessionScope,
          context: pending.prepared.context, budget, contract: fullContract })); }
        catch (error) { check(revision); pending.state = "prepared"; throw error; }
        check(revision);
        if (BigInt(started.ceremony_expires_at) <= localMillis(now) || BigInt(pending.plan.write_deadline) <= localMillis(now)
          || proxy && BigInt(fullContract.basis.expires_at) <= localMillis(now)
          || budget !== null && budget !== undefined && BigInt(budget.deadline) <= localMillis(now)) {
          pending.state = "prepared"; throw fail("custody_session_summary_expired");
        }
        pending.nativeBudgetContract = clone(fullContract);
      }
      pending.state = "authenticator";
      let credential;
      try { credential = await credentials.get(browserPasskeyOptions(started.options)); }
      catch { check(revision); pending.cancelled = true; pending.state = "cancelled"; throw fail("custody_authenticator_cancelled"); }
      check(revision);
      if (!credential) { pending.cancelled = true; pending.state = "cancelled"; throw fail("custody_authenticator_cancelled"); }
      pending.complete = { approval_id: started.approval_id, ceremony_id: started.ceremony_id, operation_id: started.operation_id,
        intent_digest: started.intent_digest, response: serializePasskey(credential) };
      return completeOriginal(revision);
    });
  }
  function discardCancelled() { return oneFlight(async () => {
    if (!pending?.cancelled || pending.state !== "cancelled" || !pending.prepared
      || pending.complete || pending.approved || pending.completeUnknown || pending.approveUnknown
      || !expiryMillis(pending.ceremonyExpiresAt)) throw fail("custody_original_pending");
    if (localMillis(now) < BigInt(pending.ceremonyExpiresAt)) throw fail("custody_cancelled_not_expired");
    // A settled, never-submitted get has no complete/approve original to reconcile.
    // This only drops local RAM; it sends no cancellation and starts no new request.
    reset();
    return null;
  }); }
  function retry() { return oneFlight(async revision => {
    if (pending?.complete && pending.completeUnknown) return completeOriginal(revision);
    if (pending?.approved && pending.approveUnknown) return approveOriginal(revision);
    if (pending?.state === "prepare_unknown") return prepareOriginal(revision);
    throw fail("custody_not_ready");
  }); }
  function progress() { return oneFlight(async revision => {
    if (!pending?.approveUnknown) throw fail("custody_not_ready");
    const result = await request(`/v1/account-provisions/${encodeURIComponent(pending.parent.provision_id)}/progress`, undefined, revision);
    // Provision progress is not an OwnerSetup operation receipt; retain unknown.
    return { stage: typeof result.stage === "string" ? result.stage : "unknown", approval_confirmed: false };
  }); }
  return { setSession, parents, prepare, prepareV2, prepareSessionV2, approve, retry, progress, summary, discardCancelled, clear() { reset(); scope = null; } };
}

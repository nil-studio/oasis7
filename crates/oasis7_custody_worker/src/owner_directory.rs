//! Owner-private association coordinator. Account DOs remain account authorities.
use crate::{
    directory_config::{DirectoryConfig, clock, denied, digest},
    directory_rpc::*,
};
use js_sys::Array;
use oasis7_custody_core::{DecimalU64, approval::CustodySessionStatusV1, provisioning::*};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use wasm_bindgen::{JsCast, prelude::*};
use worker::*;

const KEY: &str = "manager-directory-v1";
const MAX_BYTES: usize = 1024 * 1024;
const FANOUT_BATCH: usize = 8;
#[wasm_bindgen(module = "/platform/sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = initializeTextRecord)]
    fn initialize(
        state: &JsValue,
        key: &str,
        initial: &str,
    ) -> std::result::Result<String, JsValue>;
    #[wasm_bindgen(catch, js_name = readTextRecord)]
    fn read(state: &JsValue, key: &str) -> std::result::Result<String, JsValue>;
    #[wasm_bindgen(catch, js_name = updateAuditedTextRecord)]
    fn update(
        state: &JsValue,
        key: &str,
        callback: &js_sys::Function,
    ) -> std::result::Result<String, JsValue>;
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    schema: u8,
    identity_instance_epoch: DecimalU64,
    index: ManagerAccountIndex,
    drafts: BTreeMap<String, AccountDraftV1>,
    audit_sequence: DecimalU64,
    #[serde(default)]
    freeze_cursor: Option<String>,
}
fn decode(raw: &str) -> Result<Snapshot> {
    if raw.len() > MAX_BYTES {
        return Err(denied());
    }
    let s: Snapshot = serde_json::from_str(raw).map_err(|_| denied())?;
    if s.schema != 1 || s.index.entries().count() != s.drafts.len() {
        return Err(denied());
    }
    for entry in s.index.entries() {
        let d = s.drafts.get(&entry.custody_account_id).ok_or_else(denied)?;
        if &d.entry != entry
            || d.plan.digest().map_err(|_| denied())? != entry.plan_digest
            || d.plan.environment != entry.environment
            || d.plan.world_identity != d.world_identity.world_id
        {
            return Err(denied());
        }
    }
    Ok(s)
}

// Advance before awaiting I/O: unconfirmable targets stay pending without monopolizing retries.
fn next_freeze_batch(s: &mut Snapshot) -> Vec<AccountFreezeTarget> {
    let pending = s
        .index
        .pending_freeze_targets()
        .cloned()
        .collect::<Vec<_>>();
    let start = pending
        .iter()
        .position(|target| {
            s.freeze_cursor
                .as_ref()
                .is_none_or(|cursor| target.custody_account_id.as_str() > cursor.as_str())
        })
        .unwrap_or(0);
    let selected = pending
        .iter()
        .skip(start)
        .chain(pending.iter().take(start))
        .take(FANOUT_BATCH)
        .cloned()
        .collect::<Vec<_>>();
    if let Some(last) = selected.last() {
        s.freeze_cursor = Some(last.custody_account_id.clone());
    }
    selected
}

fn encode(s: &Snapshot) -> Result<String> {
    let raw = serde_json::to_string(s)?;
    if raw.len() > MAX_BYTES {
        return Err(denied());
    }
    Ok(raw)
}
#[durable_object]
pub struct ManagerAssociations {
    state: JsValue,
    env: Env,
}
impl DurableObject for ManagerAssociations {
    fn new(state: State, env: Env) -> Self {
        Self {
            state: state._inner().into(),
            env,
        }
    }
    async fn fetch(&self, request: Request) -> Result<Response> {
        let url = request.url()?;
        if request.method() != Method::Post || url.query().is_some() {
            return Response::error("not found", 404);
        }
        let raw = match request_text(&request).await {
            Ok(v) => v,
            Err(_) => return Response::error("invalid request", 400),
        };
        let config = match DirectoryConfig::from_env(&self.env) {
            Ok(Some(v)) => v,
            _ => return Response::error("not ready", 503),
        };
        let result = match url.path() {
            "/internal/reserve" => self.reserve(&raw, &config).await,
            "/internal/read" => self.view(&raw, &config),
            "/internal/freeze" => self.freeze(&raw, &config).await,
            _ => return Response::error("not found", 404),
        };
        match result {
            Ok(v) => Response::from_json(&v),
            Err(_) => Response::error("management request rejected or pending", 409),
        }
    }
}
impl ManagerAssociations {
    fn snapshot(
        &self,
        config: &DirectoryConfig,
        session: &CustodySessionStatusV1,
        create: bool,
    ) -> Result<Snapshot> {
        let principal = config.principal(session)?;
        let raw = match read(&self.state, KEY) {
            Ok(v) => v,
            Err(_) if create => {
                if !config.managers.iter().any(|m| {
                    m.principal == principal && m.environments.contains(&config.environment)
                }) {
                    return Err(denied());
                }
                let s = Snapshot {
                    schema: 1,
                    identity_instance_epoch: config.epoch,
                    index: ManagerAccountIndex::new_for_environment(
                        principal.clone(),
                        config.environment.clone(),
                    )
                    .map_err(|_| denied())?,
                    drafts: BTreeMap::new(),
                    audit_sequence: DecimalU64(0),
                    freeze_cursor: None,
                };
                initialize(&self.state, KEY, &encode(&s)?).map_err(|_| denied())?
            }
            Err(_) => return Err(denied()),
        };
        let s = decode(&raw)?;
        if s.identity_instance_epoch != config.epoch
            || s.index.manager() != &principal
            || s.index.environment() != Some(config.environment.as_str())
        {
            return Err(denied());
        }
        Ok(s)
    }
    fn transaction(
        &self,
        config: &DirectoryConfig,
        session: &CustodySessionStatusV1,
        event: &str,
        mutate: impl Fn(&mut Snapshot) -> Result<()>,
    ) -> Result<Snapshot> {
        let mut callback = |old: String| -> std::result::Result<Array, JsValue> {
            let action = || -> Result<Array> {
                let principal = config.principal(session)?;
                let mut s = decode(&old)?;
                if s.identity_instance_epoch != config.epoch
                    || s.index.manager() != &principal
                    || s.index.environment() != Some(config.environment.as_str())
                {
                    return Err(denied());
                }
                let before = s.clone();
                mutate(&mut s)?;
                let result = Array::new();
                if s == before {
                    result.push(&old.into());
                    result.push(&"".into());
                    result.push(&"".into());
                } else {
                    s.audit_sequence =
                        DecimalU64(s.audit_sequence.0.checked_add(1).ok_or_else(denied)?);
                    let next = encode(&s)?;
                    let audit = serde_json::to_string(&(
                        s.audit_sequence,
                        event,
                        digest("oasis7.custody.directory-state.v1", &s)?,
                    ))?;
                    result.push(&next.into());
                    result.push(&s.audit_sequence.0.to_string().into());
                    result.push(&audit.into());
                }
                Ok(result)
            };
            action().map_err(|_| JsValue::from_str("directory transaction rejected"))
        };
        let closure = Closure::borrow_mut(&mut callback);
        let result =
            update(&self.state, KEY, closure.as_ref().unchecked_ref()).map_err(|_| denied());
        drop(closure);
        decode(&result?)
    }
    async fn account(
        &self,
        path: &str,
        body: &impl Serialize,
        id: &str,
    ) -> Result<serde_json::Value> {
        let ns = self.env.durable_object("ACCOUNTS")?;
        let stub = ns.id_from_name(id)?.get_stub()?;
        let mut init = RequestInit::new();
        init.with_method(Method::Post)
            .with_body(Some(serde_json::to_string(body)?.into()));
        let mut request =
            Request::new_with_init(&format!("https://account.internal{path}"), &init)?;
        request
            .headers_mut()?
            .set("Content-Type", "application/json")?;
        let response = stub.fetch_with_request(request).await?;
        if response.status_code() != 200 {
            return Err(denied());
        }
        let text = read_response(response).await?;
        serde_json::from_str(&text).map_err(|_| denied())
    }
    async fn reserve(&self, raw: &str, config: &DirectoryConfig) -> Result<serde_json::Value> {
        let rpc: ReserveRpc = serde_json::from_str(raw).map_err(|_| denied())?;
        if rpc.request.api_version != 1 {
            return Err(denied());
        }
        let principal = config.principal(&rpc.manager_session)?;
        // Validate before creating a new namespace; no key or native/world call occurs here.
        if read(&self.state, KEY).is_err() {
            let mut probe = ManagerAccountIndex::new_for_environment(
                principal.clone(),
                config.environment.clone(),
            )
            .map_err(|_| denied())?;
            probe
                .reserve(
                    &principal,
                    &rpc.request.client_request_id,
                    &rpc.request.plan,
                    &config.policy(),
                    clock()?,
                )
                .map_err(|_| denied())?;
        }
        self.snapshot(config, &rpc.manager_session, true)?;
        let s = self.transaction(config, &rpc.manager_session, "draft_reserved", |s| {
            let e = s
                .index
                .reserve(
                    &principal,
                    &rpc.request.client_request_id,
                    &rpc.request.plan,
                    &config.policy(),
                    clock()?,
                )
                .map_err(|_| denied())?;
            let draft = AccountDraftV1 {
                entry: e.clone(),
                plan: rpc.request.plan.clone(),
                world_identity: config.world.clone(),
            };
            if let Some(old) = s.drafts.get(&e.custody_account_id) {
                if old != &draft {
                    return Err(denied());
                }
            } else {
                s.drafts.insert(e.custody_account_id.clone(), draft);
            }
            Ok(())
        })?;
        let draft = s
            .drafts
            .values()
            .find(|d| d.entry.client_request_id == rpc.request.client_request_id)
            .cloned()
            .ok_or_else(denied)?;
        if clock()? >= draft.plan.expires_at.0 {
            return self.view_entry(config, &rpc.manager_session, &draft.entry.provision_id);
        }
        let response = self
            .account("/internal/draft", &draft, &draft.entry.custody_account_id)
            .await;
        if let Ok(value) = response {
            let view: ProvisionDraftViewV1 = serde_json::from_value(value).map_err(|_| denied())?;
            if view.phase != DraftPhase::DraftInitialized
                || view.entry != draft.entry
                || view.plan != draft.plan
                || view.world_identity != draft.world_identity
            {
                return Err(denied());
            }
            // A concurrent freeze leaves the reserved target in the inventory and rejects this late acknowledgement.
            self.transaction(config, &rpc.manager_session, "draft_initialized", |s| {
                s.index
                    .mark_initialized(&draft.entry.custody_account_id, view.security_epoch.0)
                    .map_err(|_| denied())
            })?;
        }
        self.view_entry(config, &rpc.manager_session, &draft.entry.provision_id)
    }
    fn view(&self, raw: &str, config: &DirectoryConfig) -> Result<serde_json::Value> {
        let rpc: ReadRpc = serde_json::from_str(raw).map_err(|_| denied())?;
        self.view_entry(config, &rpc.manager_session, &rpc.provision_id)
    }
    fn view_entry(
        &self,
        config: &DirectoryConfig,
        session: &CustodySessionStatusV1,
        id: &str,
    ) -> Result<serde_json::Value> {
        let s = self.snapshot(config, session, false)?;
        let draft = s
            .drafts
            .values()
            .find(|d| d.entry.provision_id == id)
            .ok_or_else(denied)?;
        let account = s
            .index
            .account(&draft.entry.custody_account_id)
            .ok_or_else(denied)?;
        let phase = if s.index.is_frozen() {
            DraftPhase::Frozen
        } else if account.phase() == AssociationPhase::Initialized {
            DraftPhase::DraftInitialized
        } else {
            DraftPhase::DraftPending
        };
        Ok(serde_json::to_value(ProvisionDraftViewV1 {
            api_version: 1,
            phase,
            entry: draft.entry.clone(),
            plan: draft.plan.clone(),
            world_identity: draft.world_identity.clone(),
            security_epoch: DecimalU64(account.security_epoch()),
        })?)
    }
    async fn freeze(&self, raw: &str, config: &DirectoryConfig) -> Result<serde_json::Value> {
        let rpc: FreezeRpc = serde_json::from_str(raw).map_err(|_| denied())?;
        if rpc.request.api_version != 1 {
            return Err(denied());
        }
        let principal = config.principal(&rpc.manager_session)?;
        self.snapshot(config, &rpc.manager_session, true)?;
        let batch = std::cell::RefCell::new(Vec::new());
        self.transaction(
            config,
            &rpc.manager_session,
            "manager_freeze_started",
            |s| {
                // Current authentication cannot rewrite the original event's source versions.
                let notice = if let Some(original) = s.index.revocation() {
                    if original.operation_id != rpc.request.operation_id {
                        return Err(denied());
                    }
                    original.clone()
                } else {
                    ManagerRevocationNotice {
                        operation_id: rpc.request.operation_id.clone(),
                        manager: principal.clone(),
                        environment: config.environment.clone(),
                        identity_instance_epoch: config.epoch,
                        source_security_version: rpc.manager_session.security_version,
                        source_credential_ref: Some(rpc.manager_session.credential_ref.clone()),
                        source_credential_version: Some(rpc.manager_session.credential_version),
                    }
                };
                s.index.begin_revocation(notice).map_err(|_| denied())?;
                *batch.borrow_mut() = next_freeze_batch(s);
                Ok(())
            },
        )?;
        let targets = batch.into_inner();
        for target in targets {
            if let Ok(value) = self
                .account("/internal/freeze", &target, &target.custody_account_id)
                .await
            {
                if let Ok(receipt) = serde_json::from_value::<AccountFreezeReceipt>(value) {
                    if receipt.target != target {
                        continue;
                    }
                    // Persist every confirmation separately. Unknown/partial fan-out remains pending.
                    let _ = self.transaction(config, &rpc.manager_session, "account_frozen", |s| {
                        s.index
                            .record_freeze_receipt(receipt.clone())
                            .map_err(|_| denied())
                    });
                }
            }
        }
        let current = self.snapshot(config, &rpc.manager_session, false)?;
        Ok(serde_json::to_value(ManagerClosureViewV1 {
            api_version: 1,
            status: current.index.closure_status(),
            associated_accounts: DecimalU64(current.drafts.len() as u64),
            pending_accounts: current
                .index
                .pending_freeze_targets()
                .map(|t| t.custody_account_id.clone())
                .collect(),
            pending_operations: current
                .index
                .pending_originals()
                .map(|(_, id)| id.to_owned())
                .collect(),
        })?)
    }
}
async fn read_response(response: Response) -> Result<String> {
    // Internal DO responses are bounded by the same streaming transport adapter.
    #[wasm_bindgen(module = "/platform/sql.mjs")]
    extern "C" {
        #[wasm_bindgen(catch,js_name=readBoundedText)]
        async fn read_body(body: &JsValue, limit: u32) -> std::result::Result<JsValue, JsValue>;
    }
    let raw: web_sys::Response = response.into();
    read_body(raw.as_ref(), 32768)
        .await
        .map_err(|_| denied())?
        .as_string()
        .ok_or_else(denied)
}

pub(crate) async fn call_directory(
    env: &Env,
    path: &str,
    raw: &str,
    session: &CustodySessionStatusV1,
) -> Result<String> {
    if raw.len() > 32768 {
        return Err(denied());
    }
    let config = DirectoryConfig::from_env(env)?.ok_or_else(denied)?;
    let principal = config.principal(session)?;
    let name = digest(
        "oasis7.custody.manager-directory.v1",
        &(&config.environment, &principal),
    )?;
    let stub = env
        .durable_object("MANAGER_ASSOCIATIONS")?
        .id_from_name(&name)?
        .get_stub()?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_body(Some(raw.into()));
    let response = stub
        .fetch_with_request(Request::new_with_init(
            &format!("https://directory.internal{path}"),
            &init,
        )?)
        .await?;
    if response.status_code() != 200 {
        return Err(denied());
    }
    read_response(response).await
}

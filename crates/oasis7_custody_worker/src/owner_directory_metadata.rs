//! Read-only business projection with independent, same-transaction metadata charge.
use super::*;
use oasis7_custody_core::management_metadata::*;
#[wasm_bindgen(module = "/platform/management_metadata_sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=managementParentsTransition)]
    fn metadata_transition(
        state: &JsValue,
        key: &str,
        callback: &js_sys::Function,
    ) -> std::result::Result<String, JsValue>;
}
impl ManagerAssociations {
    pub(super) fn query_parents(
        &self,
        raw: &str,
        config: &DirectoryConfig,
    ) -> Result<serde_json::Value> {
        let rpc: ParentsQueryRpc = serde_json::from_str(raw).map_err(|_| denied())?;
        rpc.request.validate().map_err(|_| denied())?;
        let metadata = crate::management_metadata_policy::Config::enabled(&self.env)?;
        let principal = config.principal(&rpc.manager_session)?;
        let bucket = digest(
            "oasis7.custody.management-metadata-manager.v1",
            &(&metadata.realm, &metadata.origin, &principal, config.epoch),
        )?;
        let mut callback = |directory: String,
                            old: JsValue,
                            inventory: String|
         -> std::result::Result<Array, JsValue> {
            let action = || -> Result<Array> {
                crate::management_metadata_policy::Config::enabled(&self.env)?;
                if config.principal(&rpc.manager_session)? != principal {
                    return Err(denied());
                }
                if inventory.len() > 2048 {
                    return Err(denied());
                }
                let tables: Vec<String> = serde_json::from_str(&inventory).map_err(|_| denied())?;
                if tables.len() >= 16
                    || tables.iter().any(|name| {
                        !matches!(
                            name.as_str(),
                            "table:custody_records"
                                | "table:custody_audit"
                                | "table:sqlite_sequence"
                                | "table:_cf_METADATA"
                                | "table:custody_management_metadata"
                        )
                    })
                {
                    return Err(denied());
                }
                let s = decode(&directory)?;
                if s.identity_instance_epoch != config.epoch
                    || s.index.manager() != &principal
                    || s.index.environment() != Some(config.environment.as_str())
                {
                    return Err(denied());
                }
                let old = if old.is_null() {
                    None
                } else {
                    let raw = old.as_string().ok_or_else(denied)?;
                    if raw.len() > 1024 {
                        return Err(denied());
                    }
                    Some(
                        serde_json::from_str::<ManagementMetadataQuotaV1>(&raw)
                            .map_err(|_| denied())?,
                    )
                };
                let charged = ManagementMetadataQuotaV1::charge(old, &bucket, 16, clock()?)
                    .map_err(|_| denied())?;
                // Even a stale cursor consumes this actual authenticated metadata attempt.
                let response = project(&s, config, &rpc.request)
                    .and_then(|p| serde_json::to_string(&p).map_err(Into::into))
                    .unwrap_or_default();
                let result = Array::new();
                result.push(&serde_json::to_string(&charged)?.into());
                result.push(&response.into());
                Ok(result)
            };
            action().map_err(|_| JsValue::from_str("management metadata rejected"))
        };
        let closure = Closure::borrow_mut(&mut callback);
        let raw = metadata_transition(&self.state, KEY, closure.as_ref().unchecked_ref())
            .map_err(|_| denied())?;
        if raw.is_empty() || raw.len() > MAX_PAGE_BYTES {
            return Err(denied());
        }
        Ok(serde_json::from_str(&raw)?)
    }
}
fn project(
    s: &Snapshot,
    config: &DirectoryConfig,
    q: &ManagementParentsQueryV1,
) -> Result<ManagementParentsPageV1> {
    q.validate().map_err(|_| denied())?;
    let revision = digest(
        "oasis7.custody.management-parents-revision.v1",
        &(
            &s.index,
            s.identity_instance_epoch,
            config.version,
            s.inventory_complete,
        ),
    )?;
    if q.directory_revision
        .as_ref()
        .is_some_and(|r| r != &revision)
    {
        return Err(denied());
    }
    let mut entries = s.index.entries().collect::<Vec<_>>();
    entries.sort_by(|a, b| a.custody_account_id.cmp(&b.custody_account_id));
    if q.after_custody_account_id
        .as_ref()
        .is_some_and(|a| !entries.iter().any(|e| &e.custody_account_id == a))
    {
        return Err(denied());
    }
    let mut entries = entries.into_iter().filter(|e| {
        q.after_custody_account_id
            .as_ref()
            .is_none_or(|a| &e.custody_account_id > a)
    });
    let mut items = Vec::new();
    for e in entries.by_ref().take(usize::from(q.limit)) {
        let draft = s.drafts.get(&e.custody_account_id).ok_or_else(denied)?;
        items.push(ManagementParentV1 {
            custody_account_id: e.custody_account_id.clone(),
            provision_id: e.provision_id.clone(),
            environment: e.environment.clone(),
            world_id: draft.world_identity.world_id.clone(),
        });
    }
    let next_after_custody_account_id = entries
        .next()
        .map(|_| items.last().unwrap().custody_account_id.clone());
    let page = ManagementParentsPageV1 {
        api_version: 1,
        directory_revision: revision,
        frozen: s.index.is_frozen(),
        inventory_complete: s.inventory_complete,
        items,
        next_after_custody_account_id,
    };
    page.validate(q).map_err(|_| denied())?;
    Ok(page)
}

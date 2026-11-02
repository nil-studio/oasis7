//! Explicit V2 addresses and wire; legacy actors never parse this format.
use super::*;
use oasis7_custody_core::context_body_v2::*;
impl ContextBody {
    fn check_fullsigned_scope(&self, scope: &ContextBodyScopeV2, live: bool) -> Result<()> {
        scope.validate().map_err(|_| denied())?;
        let configured = self
            .env
            .var("CUSTODY_ROLE_KEY_ENVIRONMENT")
            .map_err(|_| denied())?
            .to_string();
        let expected = self
            .env
            .durable_object("CONTEXT_BODIES")?
            .id_from_name(&scope.derived_actor_name().map_err(|_| denied())?)?
            .to_string();
        if configured != scope.identity_scope.context.identity.environment
            || expected != self.actor_id
            || (live && now()? >= scope.write_deadline.0)
        {
            return Err(denied());
        }
        Ok(())
    }
    pub(super) fn handle_fullsigned_v2(&self, path: &str, raw: &str) -> Result<Response> {
        match path {
            "/internal/fullsigned-v2/put" => {
                let candidate: ContextBodyRecordV2 =
                    serde_json::from_str(raw).map_err(|_| denied())?;
                self.check_fullsigned_scope(&candidate.scope, true)?;
                Response::from_json(&store_v2::put(&self.state, &candidate).map_err(|_| denied())?)
            }
            "/internal/fullsigned-v2/read" => {
                let scope: ContextBodyScopeV2 = serde_json::from_str(raw).map_err(|_| denied())?;
                self.check_fullsigned_scope(&scope, true)?;
                Response::from_json(&store_v2::read(&self.state, &scope).map_err(|_| denied())?)
            }
            "/internal/fullsigned-v2/metadata" => {
                let scope: ContextBodyScopeV2 = serde_json::from_str(raw).map_err(|_| denied())?;
                self.check_fullsigned_scope(&scope, false)?;
                Response::from_json(&store_v2::metadata(&self.state, &scope).map_err(|_| denied())?)
            }
            _ => Response::error("not found", 404),
        }
    }
}

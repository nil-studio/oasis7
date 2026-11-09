//! Query identity is a display fence, never authorization or a world version.
use super::*;

#[derive(Default)]
pub(super) struct Runtime {
    pub retired_generation: u64,
    pub navigation_highwater: u64,
    pub pending_result: Option<String>,
}

impl Runtime {
    pub fn retire(&mut self, id: &str) {
        self.retired_generation = self.retired_generation.max(id.parse::<u64>().unwrap_or(0));
    }
    pub fn retire_seen(&mut self) {
        self.retired_generation = self.retired_generation.max(self.navigation_highwater);
        self.pending_result = None;
    }
}

#[derive(Clone, Debug, Default)]
pub(super) enum Declaration {
    #[default]
    Legacy,
    Declared(Value),
}
impl<'de> Deserialize<'de> for Declaration {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Missing uses Default; explicit null remains a declared invalid value.
        Value::deserialize(deserializer).map(Self::Declared)
    }
}

pub(super) fn context(view: &Value) -> Value {
    let version = &view["source_binding"]["version"];
    let commit = &version["commit"];
    let binding = &commit["binding"];
    let world = &commit["world"];
    let authority = json!([
        view["world_id"],
        view["scope_id"],
        view["reorg_epoch"],
        version["visibility_scope"],
        version["projection_revision"],
        world["world_id"],
        world["genesis_digest"],
        binding["provider_world_id"],
        binding["branch_id"],
        binding["reorg_generation"],
        binding["authority_generation"],
        binding["permission_generation"],
        binding["governing_manifest_ref"]
    ]);
    json!({"binding":[authority,view["source_cut"],view["request_generation"]],
        "completeness":view["completeness"],"source_frozen":view["source_frozen"],"display_limited":false})
}

fn matches_binding(declaration: &Declaration, state: &RenderState) -> bool {
    let Declaration::Declared(request) = declaration else {
        return true;
    };
    let Some(context) = state.overview_query_context.as_ref() else {
        return false;
    };
    let binding = &request["binding"];
    request["version"].as_u64() == Some(1)
        && binding.is_array()
        && binding.as_array().is_some_and(|parts| {
            parts.len() == 3
                && parts[0].as_array().is_some_and(|a| {
                    a.len() == 13
                        && a[0].as_str().is_some_and(|s| !s.is_empty())
                        && a[1].as_str().is_some_and(|s| !s.is_empty())
                })
                && parts[1].as_str().is_some_and(|s| !s.is_empty())
                && parts[2].as_str().is_some_and(|s| !s.is_empty())
        })
        && context["binding"] == *binding
}

pub(super) fn terminally_invalid(declaration: &Declaration, state: &RenderState) -> bool {
    if matches!(declaration, Declaration::Legacy) {
        return false;
    }
    !matches_binding(declaration, state)
        || state.overview_query_context.as_ref().is_none_or(|c| {
            !matches!(c["source_frozen"], Value::Null | Value::Bool(false))
                || !matches!(c["completeness"].as_str(), Some("partial" | "complete"))
        })
}

pub(super) fn permits(declaration: &Declaration, state: &RenderState) -> bool {
    matches!(declaration, Declaration::Legacy)
        || (!terminally_invalid(declaration, state)
            && state
                .overview_query_context
                .as_ref()
                .is_some_and(|c| c["completeness"] == "complete"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_declaration_is_legacy_but_explicit_null_is_not() {
        let old: navigation::Navigation =
            serde_json::from_value(json!({"mode":"overview","request_id":"2"})).unwrap();
        assert!(matches!(old.overview_query, Declaration::Legacy));
        let new: navigation::Navigation = serde_json::from_value(
            json!({"mode":"overview","request_id":"2","overview_query":null}),
        )
        .unwrap();
        assert!(matches!(
            new.overview_query,
            Declaration::Declared(Value::Null)
        ));
    }
}

//! Borrowed closed invocation columns; no signature or installation authority.
use crate::{
    CapabilityInvocationContext,
    incremental_runtime_authorization_audit::AuthorizationAuditValueV1 as V,
};
use oasis7_wasm_abi::{CapabilityAudience, CapabilityPresenter, CapabilitySubject};
pub fn visit_authorization_invocation_columns_v1<E>(
    context: &CapabilityInvocationContext,
    mut sink: impl FnMut(&'static str, V<'_>) -> Result<(), E>,
) -> Result<(), E> {
    let CapabilityInvocationContext {
        grant_id,
        subject,
        presenter,
        audience,
        catalog_snapshot_id,
        module_id,
        module_version,
        response_nonce,
    } = context;
    sink("grant_id", V::Text(grant_id))?;
    match subject {
        CapabilitySubject::Agent {
            agent_id,
            owner_binding,
            generation,
        } => {
            sink("subject.kind", V::Text("agent"))?;
            sink("subject.agent_id", V::Text(agent_id))?;
            sink("subject.owner_binding", V::Text(owner_binding))?;
            sink("subject.generation", V::Unsigned(*generation))?;
        }
        CapabilitySubject::Module {
            module_id,
            module_version,
            instance_id,
        } => {
            sink("subject.kind", V::Text("module"))?;
            sink("subject.module_id", V::Text(module_id))?;
            sink("subject.module_version", V::Text(module_version))?;
            sink("subject.instance_id", V::Text(instance_id))?;
        }
        CapabilitySubject::System { system_id, epoch } => {
            sink("subject.kind", V::Text("system"))?;
            sink("subject.system_id", V::Text(system_id))?;
            sink("subject.epoch", V::Unsigned(*epoch))?;
        }
    }
    let CapabilityPresenter {
        presenter_id,
        presenter_kind,
        session_id,
        attestation_ref,
    } = presenter;
    sink("presenter.presenter_id", V::Text(presenter_id))?;
    sink("presenter.presenter_kind", V::Text(presenter_kind))?;
    sink(
        "presenter.session_id",
        V::OptionalText(session_id.as_deref()),
    )?;
    sink(
        "presenter.attestation_ref",
        V::OptionalText(attestation_ref.as_deref()),
    )?;
    let CapabilityAudience {
        world_id,
        branch_id,
        finality_epoch,
        target_kind,
        target_id,
    } = audience;
    sink("audience.world_id", V::Text(world_id))?;
    sink("audience.branch_id", V::Text(branch_id))?;
    sink("audience.finality_epoch", V::Unsigned(*finality_epoch))?;
    sink("audience.target_kind", V::Text(target_kind))?;
    sink("audience.target_id", V::OptionalText(target_id.as_deref()))?;
    sink("catalog_snapshot_id", V::Text(catalog_snapshot_id))?;
    sink("module_id", V::Text(module_id))?;
    sink("module_version", V::Text(module_version))?;
    sink("response_nonce", V::Text(response_nonce))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invocation_variants_and_optional_empty_are_distinct() {
        for (subject, expected) in [
            (
                serde_json::json!({"kind":"agent","agent_id":"a","owner_binding":"o","generation":u64::MAX}),
                "agent",
            ),
            (
                serde_json::json!({"kind":"module","module_id":"inner","module_version":"v","instance_id":"i"}),
                "module",
            ),
            (
                serde_json::json!({"kind":"system","system_id":"s","epoch":u64::MAX}),
                "system",
            ),
        ] {
            let source:CapabilityInvocationContext=serde_json::from_value(serde_json::json!({
                "grant_id":"g","subject":subject,
                "presenter":{"presenter_id":"p","presenter_kind":"provider","session_id":"","attestation_ref":null},
                "audience":{"world_id":"w","branch_id":"b","finality_epoch":0,"target_kind":"world","target_id":""},
                "catalog_snapshot_id":"c","module_id":"outer","module_version":"v","response_nonce":"n"
            })).unwrap();
            let mut columns = std::collections::BTreeSet::new();
            visit_authorization_invocation_columns_v1(&source, |key, value| {
                assert!(columns.insert(key));
                match (key, value) {
                    ("subject.kind", V::Text(v)) => assert_eq!(v, expected),
                    ("presenter.session_id", V::OptionalText(v)) => assert_eq!(v, Some("")),
                    ("presenter.attestation_ref", V::OptionalText(v)) => assert_eq!(v, None),
                    ("audience.target_id", V::OptionalText(v)) => assert_eq!(v, Some("")),
                    ("module_id", V::Text(v)) => assert_eq!(v, "outer"),
                    _ => (),
                }
                Ok::<_, ()>(())
            })
            .unwrap();
            assert_eq!(columns.contains("subject.agent_id"), expected == "agent");
            assert_eq!(columns.contains("subject.module_id"), expected == "module");
            assert_eq!(columns.contains("subject.system_id"), expected == "system");
        }
    }
}

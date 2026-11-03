//! The original Runtime grant/policy data, never a substitute business permit.
use crate::{
    effect::{CapabilityGrant, OriginKind},
    incremental_field_member::{MemberPayloadCodecV1, MemberPayloadRefV1},
    incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape},
    policy::{PolicyDecision, PolicyRule, PolicyWhen},
};
use oasis7_formal_protocol::incremental_state::StateNamespaceV1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleAuthorizationFieldV1 {
    Capabilities,
    PolicyRules,
}
impl ModuleAuthorizationFieldV1 {
    pub const ALL: [Self; 2] = [Self::Capabilities, Self::PolicyRules];
    pub fn descriptor(self) -> FieldDescriptor {
        FieldDescriptor {
            owner: FieldOwner::Runtime,
            name: match self {
                Self::Capabilities => "capabilities",
                Self::PolicyRules => "policies.rules",
            },
            shape: match self {
                Self::Capabilities => FieldShape::Map,
                Self::PolicyRules => FieldShape::Ordered,
            },
            destination: FieldDestination::State(StateNamespaceV1::Authorization),
        }
    }
    pub fn schema(self) -> [u8; 32] {
        let mut hash = blake3::Hasher::new();
        hash.update(b"oasis7.iws.runtime-module-authorization.v1\0");
        hash.update(self.descriptor().name.as_bytes());
        *hash.finalize().as_bytes()
    }
    fn label(self) -> &'static str {
        match self {
            Self::Capabilities => "grant",
            Self::PolicyRules => "policy_rule",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleAuthorizationAddressV1 {
    pub column: &'static str,
    pub index: Option<u64>,
}
impl ModuleAuthorizationAddressV1 {
    fn fixed(column: &'static str) -> Self {
        Self {
            column,
            index: None,
        }
    }
    fn semantic(
        self,
        field: ModuleAuthorizationFieldV1,
    ) -> impl crate::incremental_semantic_key::SemanticKeyV1 {
        (
            (field.label(), self.column),
            (u64::from(self.index.is_some()), self.index.unwrap_or(0)),
        )
    }
}
#[derive(Clone, Copy)]
pub(super) struct Column {
    pub codec: MemberPayloadCodecV1,
    pub nullable: bool,
    pub labels: &'static [&'static str],
}
pub(super) fn column(
    field: ModuleAuthorizationFieldV1,
    address: ModuleAuthorizationAddressV1,
) -> Option<Column> {
    use MemberPayloadCodecV1::{Text, U64};
    let (codec, nullable, labels): (_, _, &'static [&'static str]) =
        match (field, address.column, address.index) {
            (ModuleAuthorizationFieldV1::Capabilities, "name", None) => (Text, false, &[]),
            (ModuleAuthorizationFieldV1::Capabilities, "expiry", None) => (U64, true, &[]),
            (ModuleAuthorizationFieldV1::Capabilities, "effect_kinds.length", None) => {
                (U64, false, &[])
            }
            (ModuleAuthorizationFieldV1::Capabilities, "effect_kinds", Some(_)) => {
                (Text, false, &[])
            }
            (
                ModuleAuthorizationFieldV1::PolicyRules,
                "when.effect_kind" | "when.cap_name",
                None,
            ) => (Text, true, &[]),
            (ModuleAuthorizationFieldV1::PolicyRules, "when.origin_kind", None) => {
                (Text, true, &["reducer", "plan", "module", "system"])
            }
            (ModuleAuthorizationFieldV1::PolicyRules, "decision.kind", None) => {
                (Text, false, &["allow", "deny"])
            }
            (ModuleAuthorizationFieldV1::PolicyRules, "decision.reason", None) => (Text, true, &[]),
            _ => return None,
        };
    Some(Column {
        codec,
        nullable,
        labels,
    })
}
pub(super) fn validate(
    field: ModuleAuthorizationFieldV1,
    address: ModuleAuthorizationAddressV1,
    payload: MemberPayloadRefV1<'_>,
) -> Result<(), &'static str> {
    let spec = column(field, address).ok_or("module authorization column")?;
    match payload {
        MemberPayloadRefV1::Unit if spec.nullable => Ok(()),
        MemberPayloadRefV1::U64(_) if spec.codec == MemberPayloadCodecV1::U64 => Ok(()),
        MemberPayloadRefV1::Text(text)
            if spec.codec == MemberPayloadCodecV1::Text
                && (spec.labels.is_empty() || spec.labels.contains(&text)) =>
        {
            Ok(())
        }
        _ => Err("module authorization primitive"),
    }
}
fn optional_text(value: Option<&str>) -> MemberPayloadRefV1<'_> {
    value
        .map(MemberPayloadRefV1::Text)
        .unwrap_or(MemberPayloadRefV1::Unit)
}
/// Original map key is supplied only to the outer producer, independent of name.
pub fn visit_module_capability_grant_v1<'a, E>(
    source: &'a CapabilityGrant,
    mut emit: impl FnMut(ModuleAuthorizationAddressV1, MemberPayloadRefV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    let CapabilityGrant {
        name,
        effect_kinds,
        expiry,
    } = source;
    emit(
        ModuleAuthorizationAddressV1::fixed("name"),
        MemberPayloadRefV1::Text(name),
    )?;
    emit(
        ModuleAuthorizationAddressV1::fixed("expiry"),
        expiry
            .map(MemberPayloadRefV1::U64)
            .unwrap_or(MemberPayloadRefV1::Unit),
    )?;
    emit(
        ModuleAuthorizationAddressV1::fixed("effect_kinds.length"),
        MemberPayloadRefV1::U64(effect_kinds.len() as u64),
    )?;
    for (index, kind) in effect_kinds.iter().enumerate() {
        emit(
            ModuleAuthorizationAddressV1 {
                column: "effect_kinds",
                index: Some(index as u64),
            },
            MemberPayloadRefV1::Text(kind),
        )?;
    }
    Ok(())
}
pub fn visit_module_policy_rule_v1<'a, E>(
    source: &'a PolicyRule,
    mut emit: impl FnMut(ModuleAuthorizationAddressV1, MemberPayloadRefV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    let PolicyRule { when, decision } = source;
    let PolicyWhen {
        effect_kind,
        origin_kind,
        cap_name,
    } = when;
    emit(
        ModuleAuthorizationAddressV1::fixed("when.effect_kind"),
        optional_text(effect_kind.as_deref()),
    )?;
    let origin = origin_kind.as_ref().map(|kind| match kind {
        OriginKind::Reducer => "reducer",
        OriginKind::Plan => "plan",
        OriginKind::Module => "module",
        OriginKind::System => "system",
    });
    emit(
        ModuleAuthorizationAddressV1::fixed("when.origin_kind"),
        optional_text(origin),
    )?;
    emit(
        ModuleAuthorizationAddressV1::fixed("when.cap_name"),
        optional_text(cap_name.as_deref()),
    )?;
    let (kind, reason) = match decision {
        PolicyDecision::Allow => ("allow", None),
        PolicyDecision::Deny { reason } => ("deny", Some(reason.as_str())),
    };
    emit(
        ModuleAuthorizationAddressV1::fixed("decision.kind"),
        MemberPayloadRefV1::Text(kind),
    )?;
    emit(
        ModuleAuthorizationAddressV1::fixed("decision.reason"),
        optional_text(reason),
    )?;
    Ok(())
}

#[path = "incremental_runtime_module_authorization_genesis.rs"]
mod genesis;
pub use genesis::*;

#[path = "incremental_runtime_module_authorization_read.rs"]
mod read;
pub use read::{
    ModuleAuthorizationBodyQualificationV1, ModuleAuthorizationReadWorkV1,
    ModuleAuthorizationValueV1, read_qualified_module_authorization_cell_v1,
};
#[path = "incremental_runtime_module_authorization_closure.rs"]
mod closure;
pub use closure::qualify_module_authorization_body_v1;

#[path = "incremental_runtime_module_authorization_family.rs"]
mod family;
pub use family::{
    ModuleAuthorizationFamilyQualificationV1, ModuleAuthorizationRecordKeyV1,
    open_qualified_module_authorization_body_v1, qualify_module_authorization_family_v1,
    readmit_module_authorization_family_ordinary_v1,
};

#[cfg(test)]
#[path = "incremental_runtime_module_authorization_closure_tests.rs"]
mod closure_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_grant_and_policy_options_labels_and_ordinals_are_borrowed_exactly() {
        let grant = CapabilityGrant {
            name: "embedded name".into(),
            effect_kinds: vec!["".into(), "x.*".into(), "x.*".into()],
            expiry: Some(0),
        };
        let mut rows = Vec::new();
        visit_module_capability_grant_v1(&grant, |address, payload| {
            validate(ModuleAuthorizationFieldV1::Capabilities, address, payload).unwrap();
            rows.push((address, payload));
            Ok::<_, ()>(())
        })
        .unwrap();
        assert_eq!(rows.len(), 6);
        assert!(matches!(rows[1].1, MemberPayloadRefV1::U64(0)));
        for (index, expected) in ["", "x.*", "x.*"].into_iter().enumerate() {
            assert_eq!(rows[index + 3].0.index, Some(index as u64));
            assert!(matches!(rows[index+3].1,MemberPayloadRefV1::Text(actual) if actual==expected));
        }
        let rule = PolicyRule {
            when: PolicyWhen {
                effect_kind: Some("".into()),
                origin_kind: Some(OriginKind::Module),
                cap_name: None,
            },
            decision: PolicyDecision::Deny { reason: "".into() },
        };
        let mut rows = Vec::new();
        visit_module_policy_rule_v1(&rule, |address, payload| {
            validate(ModuleAuthorizationFieldV1::PolicyRules, address, payload).unwrap();
            rows.push(payload);
            Ok::<_, ()>(())
        })
        .unwrap();
        assert!(matches!(
            rows.as_slice(),
            [
                MemberPayloadRefV1::Text(""),
                MemberPayloadRefV1::Text("module"),
                MemberPayloadRefV1::Unit,
                MemberPayloadRefV1::Text("deny"),
                MemberPayloadRefV1::Text("")
            ]
        ));
    }
}

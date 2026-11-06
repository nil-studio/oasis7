//! Closed borrowed authorization record bodies.
#[path = "incremental_runtime_authorization_tree.rs"]
mod tree;
use crate::{
    CapabilityAuthorizationAuditReceipt, CapabilityInvocationContext,
    incremental_field_member::*,
    incremental_json_records::{JsonRecordProfileV1, JsonWriteWorkV1, prepare_json_subtree_v1},
    incremental_runtime_authorization_audit::{
        AuthorizationAuditValueV1 as V, visit_authorization_audit_columns_v1,
    },
    incremental_runtime_authorization_invocation::visit_authorization_invocation_columns_v1,
};
use oasis7_formal_protocol::{
    incremental_state::{StatePageWriter, StateRootV1},
    incremental_value::ValueNodeReader,
};
pub use tree::AuthorizationWriteWorkV1;
use tree::*;
fn column<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    root: &mut StateRootV1,
    kind: AuthorizationRecordV1,
    key: &'static str,
    value: V<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut AuthorizationWriteWorkV1,
    json: &mut JsonWriteWorkV1,
) -> Result<(), MemberRecordError> {
    match value {
        V::Text(v) => text(root, kind, key, v, ctx, work),
        V::Unsigned(v) => u64_value(root, kind, key, v, ctx, work),
        V::Signed(v) => i64_value(root, kind, key, v, ctx, work),
        V::OptionalText(v) => option_text(root, kind, key, v, ctx, work),
        V::OptionalUnsigned(v) => option_u64(root, kind, key, v, ctx, work),
        V::OptionalSigned(v) => {
            member(
                root,
                kind,
                &(key, "present"),
                MemberPayloadRefV1::Bool(v.is_some()),
                ctx,
                work,
            )?;
            if let Some(v) = v {
                member(
                    root,
                    kind,
                    &(key, "value"),
                    MemberPayloadRefV1::I64(v),
                    ctx,
                    work,
                )?;
            }
            Ok(())
        }
        V::Json(v) => json_column(root, kind, key, v, ctx, work, json),
        V::OptionalJson(v) => {
            member(
                root,
                kind,
                &(key, "present"),
                MemberPayloadRefV1::Bool(v.is_some()),
                ctx,
                work,
            )?;
            if let Some(v) = v {
                json_column(root, kind, key, v, ctx, work, json)?;
            }
            Ok(())
        }
        V::TextSet(v) => {
            member(
                root,
                kind,
                &(key, "count"),
                MemberPayloadRefV1::U64(v.len() as u64),
                ctx,
                work,
            )?;
            for item in v {
                member(
                    root,
                    kind,
                    &(key, ("member", item.as_str())),
                    MemberPayloadRefV1::Unit,
                    ctx,
                    work,
                )?;
            }
            Ok(())
        }
    }
}
fn json_column<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    root: &mut StateRootV1,
    kind: AuthorizationRecordV1,
    key: &'static str,
    value: &serde_json::Value,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut AuthorizationWriteWorkV1,
    json: &mut JsonWriteWorkV1,
) -> Result<(), MemberRecordError> {
    let profile = JsonRecordProfileV1 {
        field: kind.descriptor(),
        schema: kind.schema(),
    };
    let child = prepare_json_subtree_v1(profile, value, ctx, json)?;
    root_member(root, kind, &key, kind, child, ctx, work)
}
pub fn prepare_authorization_audit_body_v1<
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    source: &CapabilityAuthorizationAuditReceipt,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut AuthorizationWriteWorkV1,
    json: &mut JsonWriteWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let mut root = child_root()?;
    metadata(&mut root, AuthorizationRecordV1::Audit, ctx, work)?;
    visit_authorization_audit_columns_v1(source, |key, value| {
        column(
            &mut root,
            AuthorizationRecordV1::Audit,
            key,
            value,
            ctx,
            work,
            json,
        )
    })?;
    Ok(root)
}
pub fn prepare_authorization_invocation_body_v1<
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    source: &CapabilityInvocationContext,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut AuthorizationWriteWorkV1,
    json: &mut JsonWriteWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let mut root = child_root()?;
    metadata(&mut root, AuthorizationRecordV1::Invocation, ctx, work)?;
    visit_authorization_invocation_columns_v1(source, |key, value| {
        column(
            &mut root,
            AuthorizationRecordV1::Invocation,
            key,
            value,
            ctx,
            work,
            json,
        )
    })?;
    Ok(root)
}

/// Emit an original audit map member with its independent original map key.
pub fn prepare_authorization_audit_member_v1<
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    key: &str,
    source: &CapabilityAuthorizationAuditReceipt,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut AuthorizationWriteWorkV1,
    json: &mut JsonWriteWorkV1,
) -> Result<MemberRecordV1, MemberRecordError> {
    let child = prepare_authorization_audit_body_v1(source, ctx, work, json)?;
    outer_member(
        "capability_authorization_receipts",
        AuthorizationRecordV1::Audit,
        key,
        child,
        ctx,
    )
}
/// Emit an original invocation map member with its independent original key.
pub fn prepare_authorization_invocation_member_v1<
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    key: &str,
    source: &CapabilityInvocationContext,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut AuthorizationWriteWorkV1,
    json: &mut JsonWriteWorkV1,
) -> Result<MemberRecordV1, MemberRecordError> {
    let child = prepare_authorization_invocation_body_v1(source, ctx, work, json)?;
    outer_member(
        "capability_invocation_contexts",
        AuthorizationRecordV1::Invocation,
        key,
        child,
        ctx,
    )
}
fn outer_member<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    name: &'static str,
    kind: AuthorizationRecordV1,
    key: &str,
    child: StateRootV1,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    let reference = root_reference(kind, &key, child, ctx)?;
    let field = FieldDescriptor {
        owner: FieldOwner::Runtime,
        name,
        shape: FieldShape::Map,
        destination: FieldDestination::State(
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization,
        ),
    };
    let result = write_member_record_v1(
        field,
        WorldFieldRecordKindV1::Entry,
        &key,
        MemberPayloadRefV1::RecordRoot(&reference),
        ctx,
    )?;
    if (ctx.cancelled)() {
        return Err(oasis7_formal_protocol::incremental_value::ValueTreeError::Cancelled.into());
    }
    Ok(result)
}

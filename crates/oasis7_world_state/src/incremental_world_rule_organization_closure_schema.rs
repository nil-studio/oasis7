//! Closed, borrowed relative-key decoder. Parsing does not confer qualification.
use super::*;
use crate::incremental_semantic_key::{SemanticKeyRefV1 as T, read_semantic_key_v1};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrganizationRelativeRecordV1<'a> {
    Case,
    Estate,
    Grant(&'a str),
    Recovery(&'a str),
}
impl OrganizationRelativeRecordV1<'_> {
    fn profile(self) -> super::super::Profile {
        match self {
            Self::Case => super::super::Profile::Case,
            Self::Estate => super::super::Profile::Estate,
            Self::Grant(_) => super::super::Profile::Grant,
            Self::Recovery(_) => super::super::Profile::Recovery,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrganizationRelativeKeyV1<'a> {
    /// Slot in the explicit fixed profile; optional-value absence is checked
    /// against its flag by the owner receiver, not by key decoding.
    Column {
        record: OrganizationRelativeRecordV1<'a>,
        slot: usize,
    },
    Member(&'a str),
    Scope {
        grant: &'a str,
        object: &'a str,
        right: &'a str,
    },
    Receipt(u64),
}
fn pair(node: T<'_>) -> Result<Option<(T<'_>, T<'_>)>, ValueTreeError> {
    match node {
        T::Pair { left, right } => Ok(Some((
            left.decode().map_err(ValueTreeError::Invalid)?,
            right.decode().map_err(ValueTreeError::Invalid)?,
        ))),
        _ => Ok(None),
    }
}
fn record<'a>(
    node: T<'a>,
    owner: OrganizationRootKeyV1<'_>,
) -> Result<Option<OrganizationRelativeRecordV1<'a>>, ValueTreeError> {
    if node == T::Unit {
        return Ok(Some(match owner {
            OrganizationRootKeyV1::Case(_) => OrganizationRelativeRecordV1::Case,
            OrganizationRootKeyV1::Estate(_) => OrganizationRelativeRecordV1::Estate,
        }));
    }
    if !matches!(owner, OrganizationRootKeyV1::Case(_)) {
        return Ok(None);
    }
    let Some((left, T::Text(key))) = pair(node)? else {
        return Ok(None);
    };
    let Some((T::Unit, T::Text(collection))) = pair(left)? else {
        return Ok(None);
    };
    Ok(match collection {
        "grants" => Some(OrganizationRelativeRecordV1::Grant(key)),
        "requests" => Some(OrganizationRelativeRecordV1::Recovery(key)),
        _ => None,
    })
}
fn column(
    record: OrganizationRelativeRecordV1<'_>,
    matches: impl Fn(RecordPathV1<'_>) -> bool,
) -> Option<OrganizationRelativeKeyV1<'_>> {
    record
        .profile()
        .specs()
        .iter()
        .position(|spec| matches(spec.path))
        .map(|slot| OrganizationRelativeKeyV1::Column { record, slot })
}
/// Accepts only registered relative tuple paths and preserves original strings,
/// empty strings, NUL, Unicode, scope labels and exact U64 ordinal values.
pub fn inspect_organization_relative_key_v1<'a>(
    owner: OrganizationRootKeyV1<'_>,
    bytes: &'a [u8],
) -> Result<OrganizationRelativeKeyV1<'a>, ValueTreeError> {
    let node = read_semantic_key_v1(bytes).map_err(ValueTreeError::Invalid)?;
    if let Some(record) = record(node, owner)? {
        return column(record, |path| matches!(path, RecordPathV1::Record)).ok_or(
            ValueTreeError::Invalid("organization relative record marker"),
        );
    }
    let Some((left, right)) = pair(node)? else {
        return Err(ValueTreeError::Invalid("organization relative key"));
    };
    if let (Some(record), T::Text(field)) = (record(left, owner)?, right)
        && let Some(column) = column(
            record,
            |path| matches!(path, RecordPathV1::Field(name) if name == field),
        )
    {
        return Ok(column);
    }
    let Some((root, T::Text(parent))) = pair(left)? else {
        return Err(ValueTreeError::Invalid("organization relative path"));
    };
    let Some(record) = record(root, owner)? else {
        return Err(ValueTreeError::Invalid("organization relative ancestor"));
    };
    if let Some(column) = column(record, |path| match (path, right) {
        (RecordPathV1::Collection(name), T::Unit) => name == parent,
        (RecordPathV1::FieldOptionPresence(name), T::Unit) => name == parent,
        (RecordPathV1::FieldOptionValue(name), T::Text("value")) => name == parent,
        (RecordPathV1::Nested(name, field), T::Text(actual)) => name == parent && field == actual,
        _ => false,
    }) {
        return Ok(column);
    }
    match (record, parent, right) {
        (OrganizationRelativeRecordV1::Case, "members", T::Text(member)) => {
            Ok(OrganizationRelativeKeyV1::Member(member))
        }
        (OrganizationRelativeRecordV1::Case, "stage_receipts", T::Unsigned(ordinal)) => {
            Ok(OrganizationRelativeKeyV1::Receipt(ordinal))
        }
        (OrganizationRelativeRecordV1::Grant(grant), "scopes", scope) => {
            let Some((T::Text(object), T::Text(right))) = pair(scope)? else {
                return Err(ValueTreeError::Invalid("organization relative scope"));
            };
            if !matches!(
                right,
                "preserve"
                    | "return_trust"
                    | "distribute_residual"
                    | "finalize_case"
                    | "advance_case"
            ) {
                return Err(ValueTreeError::Invalid("organization relative scope right"));
            }
            Ok(OrganizationRelativeKeyV1::Scope {
                grant,
                object,
                right,
            })
        }
        _ => Err(ValueTreeError::Invalid(
            "organization relative unknown field",
        )),
    }
}

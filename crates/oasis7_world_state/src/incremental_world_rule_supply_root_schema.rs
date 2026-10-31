use super::*;
use crate::incremental_semantic_key::{SemanticKeyRefV1 as T, read_semantic_key_v1};
pub(super) enum Key<'a> {
    Column(usize),
    Member(&'static str, &'a str),
    FrozenField(&'a str),
}
fn pair(n: T<'_>) -> Result<Option<(T<'_>, T<'_>)>, ValueTreeError> {
    if let T::Pair { left, right } = n {
        Ok(Some((
            left.decode().map_err(ValueTreeError::Invalid)?,
            right.decode().map_err(ValueTreeError::Invalid)?,
        )))
    } else {
        Ok(None)
    }
}
fn field(n: T<'_>, name: &str) -> Result<bool, ValueTreeError> {
    Ok(matches!(pair(n)?,Some((T::Unit,T::Text(s))) if s==name))
}
fn nested(n: T<'_>, parent: &str, name: &str) -> Result<bool, ValueTreeError> {
    if let Some((left, T::Text(s))) = pair(n)? {
        Ok(s == name && field(left, parent)?)
    } else {
        Ok(false)
    }
}
fn matches(n: T<'_>, p: RecordPathV1<'_>) -> Result<bool, ValueTreeError> {
    Ok(match p {
        RecordPathV1::Record => n == T::Unit,
        RecordPathV1::Field(name) => field(n, name)?,
        RecordPathV1::Nested(parent, name) => nested(n, parent, name)?,
        RecordPathV1::Collection(name) => {
            if let Some((left, T::Unit)) = pair(n)? {
                field(left, name)?
            } else {
                false
            }
        }
        RecordPathV1::OptionPresence(parent, name) => {
            if let Some((left, T::Unit)) = pair(n)? {
                nested(left, parent, name)?
            } else {
                false
            }
        }
        RecordPathV1::OptionValue(parent, name) => {
            if let Some((left, T::Text("value"))) = pair(n)? {
                nested(left, parent, name)?
            } else {
                false
            }
        }
        _ => return Err(ValueTreeError::Invalid("supply fixed path profile")),
    })
}
fn member(n: T<'_>) -> Result<Option<(&str, &str)>, ValueTreeError> {
    let Some((left, T::Text(member))) = pair(n)? else {
        return Ok(None);
    };
    let Some((T::Unit, T::Text(collection))) = pair(left)? else {
        return Ok(None);
    };
    Ok(Some((collection, member)))
}
pub(super) fn inspect<'a>(
    owner: SupplyRootKeyV1<'_>,
    bytes: &'a [u8],
) -> Result<Key<'a>, ValueTreeError> {
    let n = read_semantic_key_v1(bytes).map_err(ValueTreeError::Invalid)?;
    for slot in 0..owner.count() {
        if matches(n, owner.spec(slot).path)? {
            return Ok(Key::Column(slot));
        }
    }
    if let Some((collection, m)) = member(n)? {
        let collection = match (owner, collection) {
            (SupplyRootKeyV1::Authorization(_), "beneficiaries") => "beneficiaries",
            (SupplyRootKeyV1::Authorization(_), "requests") => "requests",
            (SupplyRootKeyV1::Batch(_), "demands") => "demands",
            (SupplyRootKeyV1::Batch(_), "allocation") => "allocation",
            (SupplyRootKeyV1::Batch(_), "frozen_lineages") => "frozen_lineages",
            _ => return Err(ValueTreeError::Invalid("supply unknown collection")),
        };
        return Ok(Key::Member(collection, m));
    }
    if matches!(owner, SupplyRootKeyV1::Batch(_))
        && let Some((left, T::Text(field))) = pair(n)?
        && let Some(("frozen_lineages", m)) = member(left)?
    {
        if !matches!(field, "target" | "fulfilled" | "reserved") {
            return Err(ValueTreeError::Invalid("supply frozen unknown field"));
        }
        return Ok(Key::FrozenField(m));
    }
    Err(ValueTreeError::Invalid("supply relative unknown field"))
}

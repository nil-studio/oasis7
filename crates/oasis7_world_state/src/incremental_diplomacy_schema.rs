use super::*;
use crate::incremental_semantic_key::{SemanticKeyRefV1 as T, read_semantic_key_v1};
pub(super) enum Row {
    Column(usize),
    Member(u64),
    Outcome(u64, Option<&'static str>),
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
    Ok(matches!(pair(n)?,Some((T::Unit,T::Text(s)))if s==name))
}
fn matches(n: T<'_>, path: Path) -> Result<bool, ValueTreeError> {
    Ok(match path {
        Path::Root => n == T::Unit,
        Path::Field(s) => field(n, s)?,
        Path::Flag(s) | Path::Meta(s) => {
            if let Some((left, T::Unit)) = pair(n)? {
                field(left, s)?
            } else {
                false
            }
        }
        Path::Value(s) => {
            if let Some((left, T::Text("value"))) = pair(n)? {
                field(left, s)?
            } else {
                false
            }
        }
        Path::Length(s) => {
            if let Some((left, T::Text("length"))) = pair(n)? {
                field(left, s)?
            } else {
                false
            }
        }
        _ => false,
    })
}
pub(super) fn inspect(field_id: DiplomacyFieldV1, semantic: &[u8]) -> Result<Row, ValueTreeError> {
    let n = read_semantic_key_v1(semantic).map_err(ValueTreeError::Invalid)?;
    for (slot, spec) in profile::specs(field_id).iter().enumerate() {
        if matches(n, spec.path)? {
            return Ok(Row::Column(slot));
        }
    }
    if let Some((left, T::Unsigned(i))) = pair(n)? {
        if field_id == DiplomacyFieldV1::Alliances && field(left, "members")? {
            return Ok(Row::Member(i));
        }
        if field_id == DiplomacyFieldV1::Wars && field(left, "participant_outcomes")? {
            return Ok(Row::Outcome(i, None));
        }
    }
    if field_id == DiplomacyFieldV1::Wars
        && let Some((left, T::Text(name))) = pair(n)?
        && let Some((left, T::Unsigned(i))) = pair(left)?
        && field(left, "participant_outcomes")?
    {
        let name = match name {
            "agent_id" => "agent_id",
            "electricity_delta" => "electricity_delta",
            "data_delta" => "data_delta",
            "reputation_delta" => "reputation_delta",
            _ => return Err(ValueTreeError::Invalid("diplomacy outcome column")),
        };
        return Ok(Row::Outcome(i, Some(name)));
    }
    Err(ValueTreeError::Invalid("diplomacy unknown relative key"))
}

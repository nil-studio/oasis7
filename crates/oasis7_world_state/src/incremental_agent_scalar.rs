use super::*;
const DOMAIN: &[u8] = b"oasis7.iws.agent-scalar.v1\0";
pub const AGENT_I64_VALUE_BYTES: usize = DOMAIN.len() + 9;
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum AgentScalarRefV1<'a> {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    I64(i64),
    Bool(bool),
    Text(&'a str),
    OptionalText(Option<&'a str>),
    OptionalU64(Option<u64>),
}
impl AgentScalarRefV1<'_> {
    pub fn kind(self) -> ScalarKindV1 {
        match self {
            Self::U8(_) => ScalarKindV1::U8,
            Self::U16(_) => ScalarKindV1::U16,
            Self::U32(_) => ScalarKindV1::U32,
            Self::U64(_) => ScalarKindV1::U64,
            Self::I64(_) => ScalarKindV1::I64,
            Self::Bool(_) => ScalarKindV1::Bool,
            Self::Text(_) => ScalarKindV1::Text,
            Self::OptionalText(_) => ScalarKindV1::OptionalText,
            Self::OptionalU64(_) => ScalarKindV1::OptionalU64,
        }
    }
    pub fn encoded_len(self) -> Result<u64, &'static str> {
        let payload = match self {
            Self::U8(_) | Self::Bool(_) => 1,
            Self::U16(_) => 2,
            Self::U32(_) => 4,
            Self::U64(_) | Self::I64(_) => 8,
            Self::Text(v) => 8u64
                .checked_add(v.len() as u64)
                .ok_or("agent scalar length")?,
            Self::OptionalText(None) | Self::OptionalU64(None) => 1,
            Self::OptionalText(Some(v)) => 9u64
                .checked_add(v.len() as u64)
                .ok_or("agent scalar length")?,
            Self::OptionalU64(Some(_)) => 9,
        };
        payload
            .checked_add(DOMAIN.len() as u64 + 1)
            .ok_or("agent scalar length")
    }
    pub fn write_to(self, out: &mut impl Write) -> std::io::Result<()> {
        out.write_all(DOMAIN)?;
        let tag = match self.kind() {
            ScalarKindV1::U8 => 1,
            ScalarKindV1::U16 => 2,
            ScalarKindV1::U32 => 3,
            ScalarKindV1::U64 => 4,
            ScalarKindV1::I64 => 5,
            ScalarKindV1::Bool => 6,
            ScalarKindV1::Text => 7,
            ScalarKindV1::OptionalText => 8,
            ScalarKindV1::OptionalU64 => 9,
        };
        out.write_all(&[tag])?;
        match self {
            Self::U8(v) => out.write_all(&[v]),
            Self::U16(v) => out.write_all(&v.to_be_bytes()),
            Self::U32(v) => out.write_all(&v.to_be_bytes()),
            Self::U64(v) => out.write_all(&v.to_be_bytes()),
            Self::I64(v) => out.write_all(&v.to_be_bytes()),
            Self::Bool(v) => out.write_all(&[u8::from(v)]),
            Self::Text(v) => {
                out.write_all(&(v.len() as u64).to_be_bytes())?;
                out.write_all(v.as_bytes())
            }
            Self::OptionalText(v) => {
                out.write_all(&[u8::from(v.is_some())])?;
                if let Some(v) = v {
                    out.write_all(&(v.len() as u64).to_be_bytes())?;
                    out.write_all(v.as_bytes())?;
                }
                Ok(())
            }
            Self::OptionalU64(v) => {
                out.write_all(&[u8::from(v.is_some())])?;
                if let Some(v) = v {
                    out.write_all(&v.to_be_bytes())?;
                }
                Ok(())
            }
        }
    }
}
impl<'a> AgentScalarRefV1<'a> {
    /// Borrowed full-value decoder; callers admit and bound any preceding IO.
    pub fn from_canonical_bytes(
        bytes: &'a [u8],
        field: AgentFieldV1,
    ) -> Result<Self, &'static str> {
        let mut bytes = bytes.strip_prefix(DOMAIN).ok_or("agent scalar domain")?;
        let tag = take(&mut bytes, 1)?[0];
        let value = match tag {
            1 => Self::U8(take(&mut bytes, 1)?[0]),
            2 => Self::U16(u16::from_be_bytes(take(&mut bytes, 2)?.try_into().unwrap())),
            3 => Self::U32(u32::from_be_bytes(take(&mut bytes, 4)?.try_into().unwrap())),
            4 => Self::U64(number(&mut bytes)?),
            5 => Self::I64(i64::from_be_bytes(take(&mut bytes, 8)?.try_into().unwrap())),
            6 => Self::Bool(match take(&mut bytes, 1)?[0] {
                0 => false,
                1 => true,
                _ => return Err("agent scalar bool"),
            }),
            7 => Self::Text(text(&mut bytes)?),
            8 => Self::OptionalText(match take(&mut bytes, 1)?[0] {
                0 => None,
                1 => Some(text(&mut bytes)?),
                _ => return Err("agent scalar optional"),
            }),
            9 => Self::OptionalU64(match take(&mut bytes, 1)?[0] {
                0 => None,
                1 => Some(number(&mut bytes)?),
                _ => return Err("agent scalar optional"),
            }),
            _ => return Err("agent scalar tag"),
        };
        if !bytes.is_empty() || value.kind() != field.kind() {
            return Err("agent scalar type/trailing");
        }
        field.validate_value(value)?;
        Ok(value)
    }
}
fn take<'a>(bytes: &mut &'a [u8], len: usize) -> Result<&'a [u8], &'static str> {
    let (value, rest) = bytes
        .split_at_checked(len)
        .ok_or("agent scalar truncated")?;
    *bytes = rest;
    Ok(value)
}
fn number(bytes: &mut &[u8]) -> Result<u64, &'static str> {
    Ok(u64::from_be_bytes(take(bytes, 8)?.try_into().unwrap()))
}
fn text<'a>(bytes: &mut &'a [u8]) -> Result<&'a str, &'static str> {
    let len = usize::try_from(number(bytes)?).map_err(|_| "agent scalar text length")?;
    std::str::from_utf8(take(bytes, len)?).map_err(|_| "agent scalar UTF-8")
}

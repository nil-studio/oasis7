use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerKeyRefV1<'a> {
    World,
    Agent(&'a str),
    Site(&'a str),
    Factory(&'a str),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticKeyRefV1<'a> {
    Unit,
    Text(&'a str),
    Unsigned(u64),
    Resource(ResourceKind),
    Ledger(LedgerKeyRefV1<'a>),
    Pair {
        left: SemanticKeyTokenV1<'a>,
        right: SemanticKeyTokenV1<'a>,
    },
}
/// Constructed only after validating the entire enclosing key. Decoding a
/// child borrows its original bytes; recursive values never require Box/Vec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticKeyTokenV1<'a>(&'a [u8]);
impl<'a> SemanticKeyTokenV1<'a> {
    pub fn decode(self) -> Result<SemanticKeyRefV1<'a>, &'static str> {
        decode(self.0)
    }
}
fn text_end(bytes: &[u8]) -> Result<usize, &'static str> {
    if bytes.len() < 9 || bytes[0] != 1 {
        return Err("semantic text framing");
    }
    let length = usize::try_from(u64::from_be_bytes(bytes[1..9].try_into().unwrap()))
        .map_err(|_| "semantic text length")?;
    let end = 9usize
        .checked_add(length)
        .filter(|end| *end <= bytes.len())
        .ok_or("semantic text length")?;
    std::str::from_utf8(&bytes[9..end]).map_err(|_| "semantic text UTF-8")?;
    Ok(end)
}
fn end(bytes: &[u8], depth: usize) -> Result<usize, &'static str> {
    match bytes.first().copied().ok_or("semantic key truncated")? {
        0 => Ok(1),
        1 => text_end(bytes),
        2 if bytes.len() >= 9 => Ok(9),
        3 if matches!(bytes.get(1), Some(1 | 2)) => Ok(2),
        4 => match bytes.get(1) {
            Some(0) => Ok(2),
            Some(1..=3) => Ok(2 + text_end(&bytes[2..])?),
            _ => Err("semantic ledger tag"),
        },
        5 => {
            if depth >= SEMANTIC_KEY_MAX_PAIR_DEPTH {
                return Err("semantic key pair depth");
            }
            let left = end(&bytes[1..], depth + 1)?;
            let right = end(&bytes[1 + left..], depth + 1)?;
            Ok(1 + left + right)
        }
        _ => Err("semantic key token/tag"),
    }
}
fn decode(bytes: &[u8]) -> Result<SemanticKeyRefV1<'_>, &'static str> {
    if end(bytes, 0)? != bytes.len() {
        return Err("semantic key trailing bytes");
    }
    Ok(match bytes[0] {
        0 => SemanticKeyRefV1::Unit,
        1 => SemanticKeyRefV1::Text(
            std::str::from_utf8(&bytes[9..]).map_err(|_| "semantic text UTF-8")?,
        ),
        2 => SemanticKeyRefV1::Unsigned(u64::from_be_bytes(bytes[1..9].try_into().unwrap())),
        3 => SemanticKeyRefV1::Resource(if bytes[1] == 1 {
            ResourceKind::Electricity
        } else {
            ResourceKind::Data
        }),
        4 => SemanticKeyRefV1::Ledger(match bytes[1] {
            0 => LedgerKeyRefV1::World,
            tag => {
                let text = std::str::from_utf8(&bytes[11..]).map_err(|_| "semantic text UTF-8")?;
                match tag {
                    1 => LedgerKeyRefV1::Agent(text),
                    2 => LedgerKeyRefV1::Site(text),
                    3 => LedgerKeyRefV1::Factory(text),
                    _ => return Err("semantic ledger tag"),
                }
            }
        }),
        5 => {
            let left_end = 1 + end(&bytes[1..], 1)?;
            SemanticKeyRefV1::Pair {
                left: SemanticKeyTokenV1(&bytes[1..left_end]),
                right: SemanticKeyTokenV1(&bytes[left_end..]),
            }
        }
        _ => return Err("semantic key token/tag"),
    })
}
pub fn read_semantic_key_v1(bytes: &[u8]) -> Result<SemanticKeyRefV1<'_>, &'static str> {
    let token = bytes
        .strip_prefix(SEMANTIC_KEY_DOMAIN)
        .ok_or("semantic key domain")?;
    decode(token)
}

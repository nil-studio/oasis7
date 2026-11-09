//! Canonical, borrowed source keys. Serde is retained only for existing debug
//! visitors; the identity writer never calls Serialize or clones a ledger ID.
use crate::MaterialLedgerId;
use oasis7_agent_api::world_action::ResourceKind;
use std::io::{self, Write};

pub const SEMANTIC_KEY_DOMAIN: &[u8] = b"oasis7.iws.semantic-key.v1\0";
pub const SEMANTIC_KEY_MAX_PAIR_DEPTH: usize = 16;
mod sealed {
    use super::*;
    pub trait Key {
        fn encode<W: Write>(&self, output: &mut W, _depth: usize) -> io::Result<()>;
    }
}
/// Closed source set. Registering a new Rust key type must explicitly choose
/// an encoding profile instead of inheriting arbitrary serde serialization.
pub trait SemanticKeyV1: serde::Serialize + sealed::Key {}
impl<T: serde::Serialize + sealed::Key + ?Sized> SemanticKeyV1 for T {}

fn text(output: &mut impl Write, value: &str) -> io::Result<()> {
    output.write_all(&[1])?;
    output.write_all(&(value.len() as u64).to_be_bytes())?;
    output.write_all(value.as_bytes())
}
impl sealed::Key for () {
    fn encode<W: Write>(&self, output: &mut W, _depth: usize) -> io::Result<()> {
        output.write_all(&[0])
    }
}
impl sealed::Key for str {
    fn encode<W: Write>(&self, output: &mut W, _depth: usize) -> io::Result<()> {
        text(output, self)
    }
}
impl sealed::Key for String {
    fn encode<W: Write>(&self, output: &mut W, _depth: usize) -> io::Result<()> {
        text(output, self)
    }
}
impl sealed::Key for u64 {
    fn encode<W: Write>(&self, output: &mut W, _depth: usize) -> io::Result<()> {
        output.write_all(&[2])?;
        output.write_all(&self.to_be_bytes())
    }
}
impl sealed::Key for ResourceKind {
    fn encode<W: Write>(&self, output: &mut W, _depth: usize) -> io::Result<()> {
        output.write_all(&[
            3,
            match self {
                Self::Electricity => 1,
                Self::Data => 2,
            },
        ])
    }
}
impl sealed::Key for MaterialLedgerId {
    fn encode<W: Write>(&self, output: &mut W, _depth: usize) -> io::Result<()> {
        let (kind, id) = match self {
            Self::World => (0, None),
            Self::Agent(id) => (1, Some(id.as_str())),
            Self::Site(id) => (2, Some(id.as_str())),
            Self::Factory(id) => (3, Some(id.as_str())),
        };
        output.write_all(&[4, kind])?;
        if let Some(id) = id {
            text(output, id)?;
        }
        Ok(())
    }
}
impl<T: sealed::Key + ?Sized> sealed::Key for &T {
    fn encode<W: Write>(&self, output: &mut W, depth: usize) -> io::Result<()> {
        T::encode(*self, output, depth)
    }
}
impl<A: sealed::Key, B: sealed::Key> sealed::Key for (A, B) {
    fn encode<W: Write>(&self, output: &mut W, depth: usize) -> io::Result<()> {
        if depth >= SEMANTIC_KEY_MAX_PAIR_DEPTH {
            return Err(io::Error::other("semantic key pair depth"));
        }
        output.write_all(&[5])?;
        self.0.encode(output, depth + 1)?;
        self.1.encode(output, depth + 1)
    }
}

/// Borrowed view of an already encoded semantic-key-v1 value. This is useful
/// when a fixed, bounded buffer is the only place a dynamically resolved
/// composite key can live; parsing first keeps the source set closed and the
/// writer re-emits the exact canonical token bytes without allocating.
#[derive(serde::Serialize)]
pub(crate) struct CanonicalSemanticKeyRefV1<'a>(#[serde(borrow)] &'a [u8]);

impl<'a> CanonicalSemanticKeyRefV1<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Result<Self, &'static str> {
        read_semantic_key_v1(bytes)?;
        Ok(Self(bytes))
    }
}

impl sealed::Key for CanonicalSemanticKeyRefV1<'_> {
    fn encode<W: Write>(&self, output: &mut W, _depth: usize) -> io::Result<()> {
        let token = self
            .0
            .strip_prefix(SEMANTIC_KEY_DOMAIN)
            .ok_or_else(|| io::Error::other("semantic key reference domain"))?;
        output.write_all(token)
    }
}

/// Output remains provisional until this function succeeds. A node sink owns
/// allocation/IO admission; this codec itself keeps no key-sized buffer.
pub fn write_semantic_key_v1<K: SemanticKeyV1 + ?Sized>(
    key: &K,
    output: &mut impl Write,
) -> io::Result<()> {
    output.write_all(SEMANTIC_KEY_DOMAIN)?;
    key.encode(output, 0)
}

#[path = "incremental_semantic_key_read.rs"]
mod read;
pub use read::*;
#[path = "incremental_semantic_key_hash.rs"]
mod hash;
pub use hash::*;

#[cfg(test)]
#[path = "incremental_semantic_key_tests.rs"]
mod tests;

//! Resident canonical authorization projection DATA. Qualification is separate:
//! a hint, its hash, or seven caller maps can never mint a Current source owner.
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalError, bounded_context::FormalBoundedContextV1,
};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryClass, MemoryLimitExceeded, ReservedValue};
use sha2::{Digest, Sha256};
use std::io::{self, Write};
#[path = "incremental_authorization_root_current_cbor.rs"]
mod cbor;
#[path = "incremental_authorization_root_current_decode.rs"]
mod decode;
#[path = "incremental_authorization_root_current_genesis.rs"]
mod genesis;
#[path = "incremental_authorization_root_current_tree.rs"]
mod tree;
#[path = "incremental_authorization_root_current_wire.rs"]
mod wire;
use tree::{CanonicalKeyV1, CanonicalValueV1, ValueRef};
#[derive(Debug)]
pub(in crate::world) enum AuthorizationRootCacheErrorV1 {
    Value(ValueTreeError),
    Formal(FormalError),
    Io(io::Error),
    Canonical(oasis7_wasm_abi::CanonicalCborStreamError),
    Decode(serde_cbor::Error),
    Invalid(&'static str),
    TaskTerminal(ReservedValue<String>),
}
impl From<ValueTreeError> for AuthorizationRootCacheErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
impl From<FormalError> for AuthorizationRootCacheErrorV1 {
    fn from(e: FormalError) -> Self {
        Self::Formal(e)
    }
}
impl From<MemoryLimitExceeded> for AuthorizationRootCacheErrorV1 {
    fn from(e: MemoryLimitExceeded) -> Self {
        Self::Value(ValueTreeError::Memory(e))
    }
}
impl From<io::Error> for AuthorizationRootCacheErrorV1 {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
fn limited() -> AuthorizationRootCacheErrorV1 {
    ValueTreeError::ResourceLimited("authorization canonical cache").into()
}
fn invalid(s: &'static str) -> AuthorizationRootCacheErrorV1 {
    AuthorizationRootCacheErrorV1::Invalid(s)
}
#[derive(Clone, Copy)]
pub(in crate::world) struct AuthorizationRootCacheLimitsV1 {
    pub max_wire_bytes: usize,
    pub max_depth: usize,
    pub max_map_keys: usize,
    pub max_key_bytes: usize,
}
impl AuthorizationRootCacheLimitsV1 {
    fn check(self) -> Result<(), AuthorizationRootCacheErrorV1> {
        if self.max_wire_bytes == 0 || self.max_map_keys < 7 || self.max_key_bytes == 0 {
            return Err(limited());
        }
        Ok(())
    }
}
impl std::fmt::Display for AuthorizationRootCacheErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for AuthorizationRootCacheErrorV1 {}
/// An unqualified data projection. No accepted/Current constructor is exposed.
pub(in crate::world) struct ProvisionalAuthorizationProjectionV1 {
    root: ValueRef,
    class: MemoryClass,
    limits: AuthorizationRootCacheLimitsV1,
}
pub(in crate::world) enum AuthorizationProjectionPathV1<'a> {
    Field(&'a str),
    Index(u64),
}
impl ProvisionalAuthorizationProjectionV1 {
    pub(in crate::world) fn read_hint_data(
        raw: &ReservedValue<Vec<u8>>,
        limits: AuthorizationRootCacheLimitsV1,
        common: &mut FormalBoundedContextV1<'_>,
        class: MemoryClass,
    ) -> Result<Self, AuthorizationRootCacheErrorV1> {
        limits.check()?;
        if !raw.uses_budget(common.memory())
            || raw.reserved_bytes() < raw.len()
            || raw.len() > limits.max_wire_bytes
        {
            return Err(invalid("authorization hint admission"));
        }
        let root = cbor::parse(raw, limits, common, class)?;
        let out = Self {
            root,
            class,
            limits,
        };
        out.check_seven_fields(common)?;
        Ok(out)
    }
    fn check_seven_fields(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), AuthorizationRootCacheErrorV1> {
        let CanonicalValueV1::Map { len, entries } = &**self.root else {
            return Err(invalid("authorization seven-field projection"));
        };
        if *len != 7 {
            return Err(invalid("authorization seven-field count"));
        }
        for name in [
            "grants",
            "revocation",
            "invocation_contexts",
            "budget_accounts",
            "nonce_records",
            "receipts",
            "effect_receipt_links",
        ] {
            let key = cbor::text_key(name, self.limits, common, self.class)?;
            if tree::find(entries, &key, common)?.is_none() {
                return Err(invalid("authorization seven-field identity"));
            }
        }
        let key = cbor::text_key("revocation", self.limits, common, self.class)?;
        let revocation = tree::find(entries, &key, common)?
            .ok_or_else(|| invalid("authorization revocation"))?;
        let CanonicalValueV1::Map { entries, .. } = &**revocation else {
            return Err(invalid("authorization revocation struct"));
        };
        let key = cbor::text_key("world_service_results", self.limits, common, self.class)?;
        if tree::find(entries, &key, common)?.is_some() {
            return Err(invalid("authorization correlation field must be omitted"));
        }
        Ok(())
    }
    /// Read-only data access for fixed cold point-join validators. It cannot
    /// assert membership in Current or construct a qualification.
    pub(in crate::world) fn write_subtree(
        &self,
        path: &[AuthorizationProjectionPathV1<'_>],
        out: &mut impl Write,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<u64, AuthorizationRootCacheErrorV1> {
        self.check_pool(common)?;
        let node = self
            .at(path, common)?
            .ok_or_else(|| invalid("authorization cache path absent"))?;
        cbor::write(
            &node,
            out,
            common,
            self.limits.max_depth,
            self.limits.max_wire_bytes,
        )
    }
    fn at(
        &self,
        path: &[AuthorizationProjectionPathV1<'_>],
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Option<ValueRef>, AuthorizationRootCacheErrorV1> {
        let mut node = self.root.clone();
        for step in path {
            let (entries, key) = match (&**node, step) {
                (
                    CanonicalValueV1::Map { entries, .. },
                    AuthorizationProjectionPathV1::Field(s),
                ) => (entries, cbor::text_key(s, self.limits, common, self.class)?),
                (
                    CanonicalValueV1::Array { entries, .. },
                    AuthorizationProjectionPathV1::Index(i),
                ) => (entries, CanonicalKeyV1::Index(*i)),
                _ => return Err(invalid("authorization canonical path domain")),
            };
            let Some(next) = tree::find(entries, &key, common)? else {
                return Ok(None);
            };
            node = next;
        }
        Ok(Some(node))
    }
    /// Provisional changed-path DATA, not permission. Only the fixed seven-
    /// source validator may qualify it after joining real changed Current rows.
    pub(in crate::world) fn replace_serialized<T: serde::Serialize>(
        &self,
        path: &[AuthorizationProjectionPathV1<'_>],
        after: &T,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self, AuthorizationRootCacheErrorV1> {
        self.check_pool(common)?;
        if path.len() > self.limits.max_depth {
            return Err(limited());
        }
        let raw = wire::encode(after, self.limits, common, self.class)?;
        let after = cbor::parse(&raw, self.limits, common, self.class)?;
        let root = patch(
            &self.root,
            path,
            Some(after),
            self.limits,
            common,
            self.class,
        )?;
        let next = Self {
            root,
            class: self.class,
            limits: self.limits,
        };
        next.check_seven_fields(common)?;
        Ok(next)
    }
    pub(in crate::world) fn remove(
        &self,
        path: &[AuthorizationProjectionPathV1<'_>],
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self, AuthorizationRootCacheErrorV1> {
        self.check_pool(common)?;
        if path.len() > self.limits.max_depth {
            return Err(limited());
        }
        let root = patch(&self.root, path, None, self.limits, common, self.class)?;
        let next = Self {
            root,
            class: self.class,
            limits: self.limits,
        };
        next.check_seven_fields(common)?;
        Ok(next)
    }
    fn check_pool(
        &self,
        common: &FormalBoundedContextV1<'_>,
    ) -> Result<(), AuthorizationRootCacheErrorV1> {
        if !self.root.uses_budget(common.memory()) || self.root.allocation_class() != self.class {
            return Err(invalid("authorization cache pool/class"));
        }
        Ok(())
    }
    pub(in crate::world) fn write_hint(
        &self,
        out: &mut impl Write,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<u64, AuthorizationRootCacheErrorV1> {
        self.check_pool(common)?;
        cbor::write(
            &self.root,
            out,
            common,
            self.limits.max_depth,
            self.limits.max_wire_bytes,
        )
    }
    /// Exactly original canonical_sha256_hex, streamed over resident canonical
    /// nodes. It does not substitute a RootSet/Merkle digest or reuse an old root.
    pub(in crate::world) fn hash(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<String>, AuthorizationRootCacheErrorV1> {
        struct Hash(Sha256);
        impl Write for Hash {
            fn write(&mut self, b: &[u8]) -> io::Result<usize> {
                self.0.update(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let fee = common
            .memory()
            .reserve(std::mem::size_of::<String>() + 64, self.class)?;
        let mut hash = Hash(Sha256::new());
        self.write_hint(&mut hash, common)?;
        let digest = hash.0.finalize();
        let mut text = String::with_capacity(64);
        if text.capacity() > 64 {
            return Err(limited());
        }
        const HEX: &[u8; 16] = b"0123456789abcdef";
        for byte in digest {
            common.consume_steps(1)?;
            text.push(HEX[(byte >> 4) as usize] as char);
            text.push(HEX[(byte & 15) as usize] as char);
        }
        Ok(fee.retain(text))
    }
}
fn patch(
    node: &ValueRef,
    path: &[AuthorizationProjectionPathV1<'_>],
    after: Option<ValueRef>,
    limits: AuthorizationRootCacheLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
    class: MemoryClass,
) -> Result<ValueRef, AuthorizationRootCacheErrorV1> {
    common.consume_steps(1)?;
    let Some((step, rest)) = path.split_first() else {
        return after.ok_or_else(|| invalid("authorization projection root removal"));
    };
    let (entries, len, key, array) = match (&***node, step) {
        (CanonicalValueV1::Map { entries, len }, AuthorizationProjectionPathV1::Field(s)) => (
            entries,
            *len,
            cbor::text_key(s, limits, common, class)?,
            false,
        ),
        (CanonicalValueV1::Array { entries, len }, AuthorizationProjectionPathV1::Index(i))
            if *i <= *len =>
        {
            (entries, *len, CanonicalKeyV1::Index(*i), true)
        }
        _ => return Err(invalid("authorization projection changed path")),
    };
    let before = tree::find(entries, &key, common)?;
    let next = if rest.is_empty() {
        after
    } else {
        Some(patch(
            &before.ok_or_else(|| invalid("authorization projection missing ancestor"))?,
            rest,
            after,
            limits,
            common,
            class,
        )?)
    };
    let (entries, len) = match next {
        Some(next) => {
            let (root, added) = tree::put(entries, key, next, common, class)?;
            (
                Some(root),
                if added {
                    len.checked_add(1).ok_or_else(limited)?
                } else {
                    len
                },
            )
        }
        None => {
            if array
                && !matches!(step,AuthorizationProjectionPathV1::Index(i) if i.checked_add(1)==Some(len))
            {
                return Err(invalid("authorization array removal must be tail"));
            }
            let (root, removed) = tree::remove(entries, &key, common, class)?;
            if !removed {
                return Err(invalid("authorization projection absent removal"));
            }
            (root, len.checked_sub(1).ok_or_else(limited)?)
        }
    };
    tree::value(
        if array {
            CanonicalValueV1::Array { len, entries }
        } else {
            CanonicalValueV1::Map { len, entries }
        },
        common,
        class,
    )
}

#[cfg(test)]
#[path = "incremental_authorization_root_current_tests.rs"]
mod tests;

#[path = "incremental_authorization_root_current_join.rs"]
mod join;
#[path = "incremental_authorization_root_current_owner.rs"]
mod owner;
#[path = "incremental_authorization_root_current_qualify.rs"]
mod qualify;
#[path = "incremental_authorization_root_current_read.rs"]
mod read;
#[path = "incremental_authorization_root_current_sources.rs"]
mod sources;
pub(in crate::world) use owner::{
    AcceptedAuthorizationProjectionV1, AuthorizationProjectionRefreshV1,
    CurrentAuthorizationProjectionV1, FinalAuthorizationProjectionV1, OwnedProjectedOutcomeV1,
    RootPublishedOutcomeV1, WarmError, qualify_accepted_parent,
};
pub(in crate::world) use read::{AuthorizationRootSourceReadWorkV1, SourceError};

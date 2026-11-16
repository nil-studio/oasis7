//! Same projection domains and canonical JSON, with continuing caller work.
//! Outputs and refusals own their allocation fees; this is DATA, not read scope.
use super::*;
use oasis7_wasm_abi::{
    CanonicalCborAllocator, CanonicalCborStreamError, CapabilityCanonicalWorkV1,
};
use std::{
    fmt,
    io::{self, Write},
};
pub struct PaidProjectionDigestDataV1<C> {
    text: String,
    _fee: Option<C>,
}
impl<C> PaidProjectionDigestDataV1<C> {
    pub fn as_str(&self) -> &str {
        &self.text
    }
}
pub struct PaidProjectionRefusalDataV1<C> {
    message: String,
    _fee: Option<C>,
}
impl<C> PaidProjectionRefusalDataV1<C> {
    pub fn message(&self) -> &str {
        &self.message
    }
}
impl<C> fmt::Debug for PaidProjectionRefusalDataV1<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("projection refusal")
            .field(&self.message)
            .finish()
    }
}
impl<C> fmt::Display for PaidProjectionRefusalDataV1<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl<C> std::error::Error for PaidProjectionRefusalDataV1<C> {}
pub enum ProjectionCommitmentWorkErrorV1<C> {
    Work(CanonicalCborStreamError),
    Protocol(PaidProjectionRefusalDataV1<C>),
}
impl<C> fmt::Debug for ProjectionCommitmentWorkErrorV1<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Work(e) => f.debug_tuple("Work").field(e).finish(),
            Self::Protocol(e) => fmt::Debug::fmt(e, f),
        }
    }
}
impl<C> fmt::Display for ProjectionCommitmentWorkErrorV1<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Work(e) => e.fmt(f),
            Self::Protocol(e) => fmt::Display::fmt(e, f),
        }
    }
}
impl<C> std::error::Error for ProjectionCommitmentWorkErrorV1<C> {}
type ResultW<T, A: CanonicalCborAllocator> =
    Result<T, ProjectionCommitmentWorkErrorV1<<A as CanonicalCborAllocator>::Charge>>;
fn pay<A: CapabilityCanonicalWorkV1>(source: &A, s: u64, b: u64) -> ResultW<(), A> {
    source
        .charge_work(s, b)
        .map_err(ProjectionCommitmentWorkErrorV1::Work)
}
fn reserve<A: CapabilityCanonicalWorkV1>(source: &A, n: usize) -> ResultW<Option<A::Charge>, A> {
    if !source.is_metered() {
        return Ok(None);
    }
    pay(source, 1, n as u64)?;
    source
        .reserve(n)
        .map(Some)
        .map_err(ProjectionCommitmentWorkErrorV1::Work)
}
fn refusal<A: CapabilityCanonicalWorkV1>(
    note: impl fmt::Display,
    source: &A,
) -> ProjectionCommitmentWorkErrorV1<A::Charge> {
    struct Count<'a, A> {
        source: &'a A,
        n: usize,
        error: Option<CanonicalCborStreamError>,
    }
    impl<A: CapabilityCanonicalWorkV1> fmt::Write for Count<'_, A> {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            if self.error.is_some() {
                return Err(fmt::Error);
            }
            if let Err(e) = self.source.charge_work(1, s.len() as u64) {
                self.error = Some(e);
                return Err(fmt::Error);
            }
            self.n = self.n.checked_add(s.len()).ok_or(fmt::Error)?;
            Ok(())
        }
    }
    let mut count = Count {
        source,
        n: 0,
        error: None,
    };
    let counted = fmt::write(&mut count, format_args!("{note}"));
    if let Some(e) = count.error {
        return ProjectionCommitmentWorkErrorV1::Work(e);
    }
    if counted.is_err() {
        return ProjectionCommitmentWorkErrorV1::Work(CanonicalCborStreamError::ResourceLimited(
            "projection refusal count",
        ));
    }
    let fee = match reserve(
        source,
        std::mem::size_of::<PaidProjectionRefusalDataV1<A::Charge>>().saturating_add(count.n),
    ) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let mut text = String::with_capacity(count.n);
    if fmt::write(&mut text, format_args!("{note}")).is_err() || text.len() != count.n {
        return ProjectionCommitmentWorkErrorV1::Work(CanonicalCborStreamError::Invalid(
            "projection refusal stable format",
        ));
    }
    ProjectionCommitmentWorkErrorV1::Protocol(PaidProjectionRefusalDataV1 {
        message: text,
        _fee: fee,
    })
}
/// Actual Serialize preflight admits the old to_value graph and sorter scratch
/// before either exists. The only live callers provide the closed formatter.
fn phase<T: Serialize + ?Sized, A: CapabilityCanonicalWorkV1>(
    value: &T,
    source: &A,
) -> ResultW<Option<A::Charge>, A> {
    if !source.is_metered() {
        return Ok(None);
    }
    struct Count<'a, A> {
        source: &'a A,
        n: usize,
        error: Option<CanonicalCborStreamError>,
    }
    impl<A: CapabilityCanonicalWorkV1> Write for Count<'_, A> {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            if self.error.is_some() {
                return Err(io::ErrorKind::Other.into());
            }
            if let Err(e) = self.source.charge_work(1, b.len() as u64) {
                self.error = Some(e);
                return Err(io::ErrorKind::Other.into());
            }
            self.n = self.n.checked_add(b.len()).ok_or(io::ErrorKind::Other)?;
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count {
        source,
        n: 0,
        error: None,
    };
    let result = serde_json::to_writer(&mut count, value);
    if let Some(e) = count.error {
        return Err(ProjectionCommitmentWorkErrorV1::Work(e));
    }
    if let Err(e) = result {
        return Err(refusal(e, source));
    }
    let n = count
        .n
        .checked_mul(1024)
        .and_then(|n| n.checked_add(32768))
        .ok_or_else(|| {
            ProjectionCommitmentWorkErrorV1::Work(CanonicalCborStreamError::ResourceLimited(
                "projection graph layout",
            ))
        })?;
    reserve(source, n)
}
struct JsonOut<'a, A> {
    source: &'a A,
    bytes: Vec<u8>,
    bound: usize,
    error: Option<CanonicalCborStreamError>,
    bound_hit: bool,
}
impl<A: CapabilityCanonicalWorkV1> Write for JsonOut<'_, A> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.error.is_some() {
            return Err(io::ErrorKind::Other.into());
        }
        if let Err(e) = self.source.charge_work(1, b.len() as u64) {
            self.error = Some(e);
            return Err(io::ErrorKind::Other.into());
        }
        let len = self
            .bytes
            .len()
            .checked_add(b.len())
            .ok_or(io::ErrorKind::Other)?;
        if len > self.bound {
            self.bound_hit = true;
            return Err(io::ErrorKind::WriteZero.into());
        }
        self.bytes.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
/// Original UTF-8 ordering and JSON scalar writer. Caller admits the maximum
/// output and key-pointer scratch before entering this one shared recursion.
fn json<A: CapabilityCanonicalWorkV1>(value: &Value, out: &mut JsonOut<'_, A>) -> ResultW<(), A> {
    pay(out.source, 1, 0)?;
    macro_rules! emit {
        ($b:expr) => {{
            if let Err(_) = out.write_all($b) {
                if let Some(e) = out.error.take() {
                    return Err(ProjectionCommitmentWorkErrorV1::Work(e));
                }
                return Err(refusal("formal projection byte bound", out.source));
            }
        }};
    }
    match value {
        Value::Object(map) => {
            emit!(b"{");
            let mut keys = map.keys().collect::<Vec<_>>();
            // serde_json::Map is ordered in this build; the explicit fallible sort is
            // shared with the leaf tree and preserves the original UTF-8 comparator.
            fallible_sort(&mut keys, &mut |a, b| {
                pay(out.source, 1, (a.len() + b.len()) as u64)?;
                Ok(a.cmp(b))
            })?;
            for (i, key) in keys.iter().enumerate() {
                pay(out.source, 1, key.len() as u64)?;
                if i > 0 {
                    emit!(b",")
                }
                if let Err(e) = serde_json::to_writer(&mut *out, key) {
                    if let Some(e) = out.error.take() {
                        return Err(ProjectionCommitmentWorkErrorV1::Work(e));
                    }
                    if out.bound_hit {
                        return Err(refusal("formal projection byte bound", out.source));
                    }
                    return Err(refusal(e, out.source));
                }
                emit!(b":");
                json(&map[*key], out)?
            }
            emit!(b"}");
        }
        Value::Array(values) => {
            emit!(b"[");
            for (i, value) in values.iter().enumerate() {
                pay(out.source, 1, 0)?;
                if i > 0 {
                    emit!(b",")
                }
                json(value, out)?
            }
            emit!(b"]");
        }
        scalar => {
            if let Err(e) = serde_json::to_writer(&mut *out, scalar) {
                if let Some(e) = out.error.take() {
                    return Err(ProjectionCommitmentWorkErrorV1::Work(e));
                }
                if out.bound_hit {
                    return Err(refusal("formal projection byte bound", out.source));
                }
                return Err(refusal(e, out.source));
            }
        }
    }
    if out.bytes.len() > MAX_PROJECTION_BYTES {
        return Err(refusal("formal projection byte bound", out.source));
    }
    Ok(())
}
pub(super) fn hash_paid<T: Serialize + ?Sized, A: CapabilityCanonicalWorkV1>(
    domain: &str,
    value: &T,
    source: &A,
) -> ResultW<PaidProjectionDigestDataV1<A::Charge>, A> {
    let owner = oasis7_agent_api::world_cognition::canonical_blake3_data_with_work_v1(
        &("oasis7.world-service.v1", domain, value),
        source,
    )
    .map_err(ProjectionCommitmentWorkErrorV1::Work)?;
    let text = owner
        .as_str()
        .strip_prefix("blake3:")
        .ok_or_else(|| refusal("formal projection digest encoding", source))?;
    let fee = reserve(
        source,
        std::mem::size_of::<PaidProjectionDigestDataV1<A::Charge>>() + 64,
    )?;
    pay(source, 1, 64)?;
    Ok(PaidProjectionDigestDataV1 {
        text: text.to_owned(),
        _fee: fee,
    })
}
pub fn projection_leaf_with_work_v1<T: Serialize + ?Sized, A: CapabilityCanonicalWorkV1>(
    audience: &ProjectionAudienceV1,
    value: &T,
    source: &A,
) -> ResultW<PaidProjectionDigestDataV1<A::Charge>, A> {
    pay(
        source,
        1,
        (audience.scope_id.len() + audience.reader_public_key.as_ref().map_or(0, String::len))
            as u64,
    )?;
    audience.validate().map_err(|e| refusal(e, source))?;
    let _graph = phase(value, source)?;
    let value = serde_json::to_value(value).map_err(|e| refusal(e, source))?;
    let _bytes = reserve(
        source,
        std::mem::size_of::<Vec<u8>>() + MAX_PROJECTION_BYTES,
    )?;
    let mut out = JsonOut {
        source,
        bytes: Vec::with_capacity(MAX_PROJECTION_BYTES),
        bound: MAX_PROJECTION_BYTES,
        error: None,
        bound_hit: false,
    };
    json(&value, &mut out)?;
    hash_paid("formal-projection-leaf-v1", &(audience, &out.bytes), source)
}
fn fallible_sort<T, E>(
    items: &mut [T],
    cmp: &mut impl FnMut(&T, &T) -> Result<std::cmp::Ordering, E>,
) -> Result<(), E> {
    fn sift<T, E>(
        a: &mut [T],
        mut root: usize,
        end: usize,
        cmp: &mut impl FnMut(&T, &T) -> Result<std::cmp::Ordering, E>,
    ) -> Result<(), E> {
        loop {
            let Some(mut child) = root
                .checked_mul(2)
                .and_then(|v| v.checked_add(1))
                .filter(|i| *i < end)
            else {
                return Ok(());
            };
            if child + 1 < end && cmp(&a[child], &a[child + 1])?.is_lt() {
                child += 1
            }
            if !cmp(&a[root], &a[child])?.is_lt() {
                return Ok(());
            }
            a.swap(root, child);
            root = child
        }
    }
    for root in (0..items.len() / 2).rev() {
        sift(items, root, items.len(), cmp)?
    }
    for end in (1..items.len()).rev() {
        items.swap(0, end);
        sift(items, 0, end, cmp)?
    }
    Ok(())
}
pub struct PaidProjectionCommitmentsDataV1<C> {
    pub(super) value: ProjectionCommitmentsV1,
    _graph: Option<C>,
}
impl<C> PaidProjectionCommitmentsDataV1<C> {
    pub fn value(&self) -> &ProjectionCommitmentsV1 {
        &self.value
    }
}
pub fn projection_tree_with_work_v1<A: CapabilityCanonicalWorkV1>(
    mut leaves: Vec<(ProjectionAudienceV1, String)>,
    source: &A,
) -> ResultW<PaidProjectionCommitmentsDataV1<A::Charge>, A> {
    if leaves.is_empty() || leaves.len() > MAX_PROJECTION_SCOPES {
        return Err(refusal("formal projection scope count", source));
    }
    let graph = phase(&leaves, source)?;
    fallible_sort(&mut leaves, &mut |a, b| {
        pay(
            source,
            1,
            (a.0.scope_id.len() + b.0.scope_id.len() + 128) as u64,
        )?;
        Ok(a.0.cmp(&b.0))
    })?;
    let mut public = false;
    let mut invalid = false;
    for (scope, hash) in &leaves {
        pay(source, 1, (scope.scope_id.len() + 128) as u64)?;
        public |= scope.scope_id == "public" && scope.reader_public_key.is_none();
        invalid |= scope.validate().is_err() || !hash_valid(hash)
    }
    for pair in leaves.windows(2) {
        pay(
            source,
            1,
            (pair[0].0.scope_id.len() + pair[1].0.scope_id.len() + 128) as u64,
        )?;
        invalid |= pair[0].0 == pair[1].0
    }
    if !public || invalid {
        return Err(refusal("formal projection duplicate/invalid scope", source));
    }
    let (scopes, hashes): (Vec<_>, Vec<_>) = leaves.into_iter().unzip();
    let mut levels = vec![hashes];
    while levels.last().expect("leaf level").len() > 1 {
        pay(source, 1, 0)?;
        let mut next = Vec::with_capacity(levels.last().unwrap().len().div_ceil(2));
        for pair in levels.last().unwrap().chunks(2) {
            let value = hash_paid(
                "formal-projection-node-v1",
                &(&pair[0], pair.get(1).unwrap_or(&pair[0])),
                source,
            )?;
            pay(source, 1, 64)?;
            next.push(value.as_str().to_owned())
        }
        levels.push(next)
    }
    Ok(PaidProjectionCommitmentsDataV1 {
        value: ProjectionCommitmentsV1 { scopes, levels },
        _graph: graph,
    })
}
pub fn projection_root_with_work_v1<A: CapabilityCanonicalWorkV1>(
    tree: &ProjectionCommitmentsV1,
    source: &A,
) -> ResultW<PaidProjectionDigestDataV1<A::Charge>, A> {
    hash_paid(
        "formal-projection-root-v1",
        &(
            tree.scopes.len() as u64,
            &tree.levels.last().expect("root")[0],
        ),
        source,
    )
}

pub(super) fn legacy_error_string(error: ProjectionCommitmentWorkErrorV1<()>) -> String {
    match error {
        ProjectionCommitmentWorkErrorV1::Protocol(e) => e.message,
        ProjectionCommitmentWorkErrorV1::Work(e) => e.to_string(),
    }
}
pub(super) fn legacy_digest_string(
    value: Result<PaidProjectionDigestDataV1<()>, ProjectionCommitmentWorkErrorV1<()>>,
) -> Result<String, String> {
    value.map(|v| v.text).map_err(legacy_error_string)
}

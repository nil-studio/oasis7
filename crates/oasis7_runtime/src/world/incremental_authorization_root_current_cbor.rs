//! Canonical-byte cache parser/writer, not a business DTO restore.
use super::*;
use tree::{CanonicalKeyV1, CanonicalValueV1, ValueRef};

pub(super) fn header(major: u8, n: u64) -> ([u8; 9], usize) {
    let mut out = [0; 9];
    let len;
    if n < 24 {
        out[0] = (major << 5) | n as u8;
        len = 1;
    } else if n <= u8::MAX as u64 {
        out[0] = (major << 5) | 24;
        out[1] = n as u8;
        len = 2;
    } else if n <= u16::MAX as u64 {
        out[0] = (major << 5) | 25;
        out[1..3].copy_from_slice(&(n as u16).to_be_bytes());
        len = 3;
    } else if n <= u32::MAX as u64 {
        out[0] = (major << 5) | 26;
        out[1..5].copy_from_slice(&(n as u32).to_be_bytes());
        len = 5;
    } else {
        out[0] = (major << 5) | 27;
        out[1..9].copy_from_slice(&n.to_be_bytes());
        len = 9;
    }
    (out, len)
}
pub(super) fn text_key(
    s: &str,
    limits: AuthorizationRootCacheLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
    class: MemoryClass,
) -> Result<CanonicalKeyV1, AuthorizationRootCacheErrorV1> {
    if s.len() > limits.max_key_bytes {
        return Err(limited());
    }
    let (head, n) = header(3, s.len() as u64);
    let len = n.checked_add(s.len()).ok_or_else(limited)?;
    let fee = common
        .memory()
        .reserve(len + std::mem::size_of::<Vec<u8>>(), class)?;
    common.charge(1, len as u64)?;
    let mut raw = Vec::new();
    raw.try_reserve_exact(len).map_err(|_| limited())?;
    if raw.capacity() > len {
        return Err(limited());
    }
    raw.extend_from_slice(&head[..n]);
    raw.extend_from_slice(s.as_bytes());
    let key = tree::bytes(&raw, common, class)?;
    drop(fee);
    Ok(CanonicalKeyV1::Text(key))
}
struct Parser<'a, 'c, 'm> {
    raw: &'a [u8],
    at: usize,
    limits: AuthorizationRootCacheLimitsV1,
    common: &'c mut FormalBoundedContextV1<'m>,
    class: MemoryClass,
}
impl Parser<'_, '_, '_> {
    fn head(&mut self) -> Result<(u8, u8, u64), AuthorizationRootCacheErrorV1> {
        self.common.consume_steps(1)?;
        let first = *self
            .raw
            .get(self.at)
            .ok_or_else(|| invalid("authorization CBOR truncated head"))?;
        self.at += 1;
        let major = first >> 5;
        let ai = first & 31;
        let extra = match ai {
            0..=23 => 0,
            24 => 1,
            25 => 2,
            26 => 4,
            27 => 8,
            _ => return Err(invalid("authorization CBOR definite head")),
        };
        let end = self
            .at
            .checked_add(extra)
            .filter(|n| *n <= self.raw.len())
            .ok_or_else(|| invalid("authorization CBOR truncated argument"))?;
        self.common.consume_read_bytes((1 + extra) as u64)?;
        let mut n = if extra == 0 { ai as u64 } else { 0 };
        for b in &self.raw[self.at..end] {
            n = (n << 8) | u64::from(*b);
        }
        self.at = end;
        if major != 7
            && ((ai == 24 && n < 24)
                || (ai == 25 && n <= u8::MAX as u64)
                || (ai == 26 && n <= u16::MAX as u64)
                || (ai == 27 && n <= u32::MAX as u64))
        {
            return Err(invalid("authorization CBOR minimal argument"));
        }
        Ok((major, ai, n))
    }
    fn value(&mut self, depth: usize) -> Result<ValueRef, AuthorizationRootCacheErrorV1> {
        if depth > self.limits.max_depth {
            return Err(limited());
        }
        let start = self.at;
        let (major, ai, n) = self.head()?;
        match major {
            0 | 1 => tree::value(
                CanonicalValueV1::Atom(tree::bytes(
                    &self.raw[start..self.at],
                    self.common,
                    self.class,
                )?),
                self.common,
                self.class,
            ),
            2 | 3 => {
                let len = usize::try_from(n).map_err(|_| limited())?;
                let end = self
                    .at
                    .checked_add(len)
                    .filter(|n| *n <= self.raw.len())
                    .ok_or_else(|| invalid("authorization CBOR string length"))?;
                if major == 3 {
                    self.common.consume_read_bytes(len as u64)?;
                    std::str::from_utf8(&self.raw[self.at..end])
                        .map_err(|_| invalid("authorization CBOR UTF8"))?;
                }
                self.at = end;
                tree::value(
                    CanonicalValueV1::Atom(tree::bytes(
                        &self.raw[start..end],
                        self.common,
                        self.class,
                    )?),
                    self.common,
                    self.class,
                )
            }
            4 => {
                let mut entries = None;
                for i in 0..n {
                    let child = self.value(depth + 1)?;
                    let (root, added) = tree::put(
                        &entries,
                        CanonicalKeyV1::Index(i),
                        child,
                        self.common,
                        self.class,
                    )?;
                    if !added {
                        return Err(invalid("authorization CBOR array index"));
                    }
                    entries = Some(root);
                }
                tree::value(
                    CanonicalValueV1::Array { len: n, entries },
                    self.common,
                    self.class,
                )
            }
            5 => {
                if n > self.limits.max_map_keys as u64 {
                    return Err(limited());
                }
                let mut entries = None;
                let mut previous = None;
                for _ in 0..n {
                    let key_start = self.at;
                    let (major, _, len) = self.head()?;
                    if major != 3 {
                        return Err(invalid("authorization projection original string map key"));
                    }
                    let len = usize::try_from(len).map_err(|_| limited())?;
                    if len > self.limits.max_key_bytes {
                        return Err(limited());
                    }
                    let end = self
                        .at
                        .checked_add(len)
                        .filter(|n| *n <= self.raw.len())
                        .ok_or_else(|| invalid("authorization CBOR map key"))?;
                    self.common.consume_read_bytes(len as u64)?;
                    std::str::from_utf8(&self.raw[self.at..end])
                        .map_err(|_| invalid("authorization CBOR map UTF8"))?;
                    self.at = end;
                    let key = CanonicalKeyV1::Text(tree::bytes(
                        &self.raw[key_start..end],
                        self.common,
                        self.class,
                    )?);
                    if let Some(previous) = &previous {
                        if tree::compare(previous, &key, self.common)? != std::cmp::Ordering::Less {
                            return Err(invalid("authorization canonical map order"));
                        }
                    }
                    previous = Some(key.clone());
                    let child = self.value(depth + 1)?;
                    let (root, added) = tree::put(&entries, key, child, self.common, self.class)?;
                    if !added {
                        return Err(invalid("authorization duplicate map key"));
                    }
                    entries = Some(root);
                }
                tree::value(
                    CanonicalValueV1::Map { len: n, entries },
                    self.common,
                    self.class,
                )
            }
            6 => {
                let child = self.value(depth + 1)?;
                tree::value(
                    CanonicalValueV1::Tag {
                        tag: n,
                        value: child,
                    },
                    self.common,
                    self.class,
                )
            }
            7 if matches!(ai,20..=23|25..=27) => tree::value(
                CanonicalValueV1::Atom(tree::bytes(
                    &self.raw[start..self.at],
                    self.common,
                    self.class,
                )?),
                self.common,
                self.class,
            ),
            _ => Err(invalid("authorization CBOR scalar domain")),
        }
    }
}
pub(super) fn parse(
    raw: &[u8],
    limits: AuthorizationRootCacheLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
    class: MemoryClass,
) -> Result<ValueRef, AuthorizationRootCacheErrorV1> {
    if raw.len() > limits.max_wire_bytes {
        return Err(limited());
    }
    let mut p = Parser {
        raw,
        at: 0,
        limits,
        common,
        class,
    };
    let root = p.value(0)?;
    if p.at != raw.len() {
        return Err(invalid("authorization CBOR trailing data"));
    }
    p.common.consume_steps(0)?;
    Ok(root)
}
struct Output<'a, 'c, 'm, W> {
    out: &'a mut W,
    common: &'c mut FormalBoundedContextV1<'m>,
    bytes: u64,
    max: u64,
    depth: usize,
}
impl<W: Write> Output<'_, '_, '_, W> {
    fn emit(&mut self, raw: &[u8]) -> Result<(), AuthorizationRootCacheErrorV1> {
        let mut at = 0;
        while at < raw.len() {
            let part = &raw[at..raw.len().min(at + 16 * 1024)];
            self.common.charge(1, part.len() as u64)?;
            if self
                .bytes
                .checked_add(part.len() as u64)
                .filter(|n| *n <= self.max)
                .is_none()
            {
                return Err(limited());
            }
            let n = self.out.write(part)?;
            if n > part.len() {
                return Err(invalid("authorization hint writer count"));
            }
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "authorization hint partial output",
                )
                .into());
            }
            self.bytes = self.bytes.checked_add(n as u64).ok_or_else(limited)?;
            at += n;
        }
        Ok(())
    }
    fn head(&mut self, major: u8, n: u64) -> Result<(), AuthorizationRootCacheErrorV1> {
        let (b, n) = header(major, n);
        self.emit(&b[..n])
    }
    fn entries(
        &mut self,
        tree: &Option<tree::TreeRef>,
        depth: usize,
        array: bool,
        next: &mut u64,
    ) -> Result<(), AuthorizationRootCacheErrorV1> {
        let Some(node) = tree else {
            return Ok(());
        };
        self.common.consume_steps(1)?;
        self.entries(&node.left, depth, array, next)?;
        match (&node.key, array) {
            (CanonicalKeyV1::Text(key), false) => self.emit(key)?,
            (CanonicalKeyV1::Index(i), true) if i == next => {}
            _ => return Err(invalid("authorization canonical tree key")),
        }
        *next = next.checked_add(1).ok_or_else(limited)?;
        self.value(&node.value, depth)?;
        self.entries(&node.right, depth, array, next)
    }
    fn value(
        &mut self,
        node: &ValueRef,
        depth: usize,
    ) -> Result<(), AuthorizationRootCacheErrorV1> {
        self.common.consume_steps(1)?;
        if depth > self.depth {
            return Err(limited());
        }
        match &***node {
            CanonicalValueV1::Atom(raw) => self.emit(raw),
            CanonicalValueV1::Tag { tag, value } => {
                self.head(6, *tag)?;
                self.value(value, depth + 1)
            }
            CanonicalValueV1::Array { len, entries } | CanonicalValueV1::Map { len, entries } => {
                let array = matches!(&***node, CanonicalValueV1::Array { .. });
                self.head(if array { 4 } else { 5 }, *len)?;
                let mut count = 0;
                self.entries(entries, depth + 1, array, &mut count)?;
                if count != *len {
                    return Err(invalid("authorization canonical tree count"));
                }
                Ok(())
            }
        }
    }
}
pub(super) fn write(
    root: &ValueRef,
    out: &mut impl Write,
    common: &mut FormalBoundedContextV1<'_>,
    max_depth: usize,
    max_bytes: usize,
) -> Result<u64, AuthorizationRootCacheErrorV1> {
    let mut output = Output {
        out,
        common,
        bytes: 0,
        max: max_bytes as u64,
        depth: max_depth,
    };
    output.value(root, 0)?;
    output.common.consume_steps(0)?;
    Ok(output.bytes)
}
pub(super) fn key_text(key: &CanonicalKeyV1) -> Result<&str, AuthorizationRootCacheErrorV1> {
    let CanonicalKeyV1::Text(raw) = key else {
        return Err(invalid("authorization map original key"));
    };
    let first = *raw
        .first()
        .ok_or_else(|| invalid("authorization map key frame"))?;
    if first >> 5 != 3 {
        return Err(invalid("authorization map key type"));
    }
    let header = match first & 31 {
        0..=23 => 1,
        24 => 2,
        25 => 3,
        26 => 5,
        27 => 9,
        _ => return Err(invalid("authorization map key header")),
    };
    std::str::from_utf8(
        raw.get(header..)
            .ok_or_else(|| invalid("authorization map key payload"))?,
    )
    .map_err(|_| invalid("authorization map key UTF8"))
}
pub(super) fn encode_node(
    root: &ValueRef,
    limits: AuthorizationRootCacheLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
    class: MemoryClass,
) -> Result<ReservedValue<Vec<u8>>, AuthorizationRootCacheErrorV1> {
    struct Count(usize);
    impl Write for Count {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0 = self.0.checked_add(b.len()).ok_or(io::ErrorKind::Other)?;
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    write(
        root,
        &mut count,
        common,
        limits.max_depth,
        limits.max_wire_bytes,
    )?;
    let fee = common.memory().reserve(
        count
            .0
            .checked_add(std::mem::size_of::<Vec<u8>>())
            .ok_or_else(limited)?,
        class,
    )?;
    let mut raw = Vec::new();
    raw.try_reserve_exact(count.0).map_err(|_| limited())?;
    if raw.capacity() > count.0 {
        return Err(limited());
    }
    struct Exact<'a>(&'a mut Vec<u8>, usize);
    impl Write for Exact<'_> {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            if b.len() > self.1 - self.0.len() {
                return Err(io::Error::other("authorization cached node count"));
            }
            self.0.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    write(
        root,
        &mut Exact(&mut raw, count.0),
        common,
        limits.max_depth,
        limits.max_wire_bytes,
    )?;
    if raw.len() != count.0 {
        return Err(invalid("authorization cached node length"));
    }
    Ok(fee.retain(raw))
}

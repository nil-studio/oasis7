use super::{BoundedRouterErrorV1 as Error, RouterWorkV1};
use serde::Serialize;
use std::{io, mem::size_of, ops::Range};

// Arbitrary precision gives Number an owned String not covered by a Node slot.
// Feature unification must reject that allocation model before this arena runs.
const _: () = assert!(
    !std::mem::needs_drop::<serde_json::Number>(),
    "bounded router requires scalar serde_json Number",
);
#[derive(Debug)]
pub(super) enum InternalError {
    Cancelled,
    ResourceLimited(&'static str),
    Invalid(&'static str),
    Serialize,
}
pub(super) fn step<C: FnMut() -> bool>(
    w: &mut RouterWorkV1,
    c: &mut C,
) -> Result<(), InternalError> {
    if c() {
        return Err(InternalError::Cancelled);
    }
    w.match_steps_remaining = w
        .match_steps_remaining
        .checked_sub(1)
        .ok_or(InternalError::ResourceLimited("router work"))?;
    Ok(())
}
pub(super) enum Kind {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(Range<usize>),
    Array,
    Object,
}
pub(super) struct Node {
    pub kind: Kind,
    pub first: Option<usize>,
}
pub(super) struct Edge {
    pub key: Option<Range<usize>>,
    pub child: usize,
    pub next: Option<usize>,
}
pub(super) struct Arena<R> {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub strings: Vec<u8>,
    _reservation: R,
}
struct Writer<'a, C> {
    bytes: Option<&'a mut Vec<u8>>,
    len: usize,
    limit: usize,
    depth: usize,
    in_string: bool,
    escaped: bool,
    w: &'a mut RouterWorkV1,
    c: &'a mut C,
    error: Option<InternalError>,
}
impl<C: FnMut() -> bool> io::Write for Writer<'_, C> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        for byte in b {
            if let Err(e) = step(self.w, self.c) {
                self.error = Some(e);
                return Err(io::Error::other("bounded"));
            }
            if self.in_string {
                if self.escaped {
                    self.escaped = false
                } else if *byte == b'\\' {
                    self.escaped = true
                } else if *byte == b'"' {
                    self.in_string = false
                }
            } else {
                match byte {
                    b'"' => self.in_string = true,
                    b'[' | b'{' => {
                        self.depth += 1;
                        if self.depth > self.w.max_depth.saturating_add(1) {
                            self.error = Some(InternalError::ResourceLimited("depth"));
                            return Err(io::Error::other("bounded"));
                        }
                    }
                    b']' | b'}' => self.depth = self.depth.saturating_sub(1),
                    _ => {}
                }
            }
        }
        let Some(n) = self.len.checked_add(b.len()).filter(|n| *n <= self.limit) else {
            self.error = Some(InternalError::ResourceLimited("serialized bytes"));
            return Err(io::Error::other("bounded"));
        };
        if let Some(v) = self.bytes.as_mut() {
            v.extend_from_slice(b)
        }
        self.len = n;
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(super) fn build_action_json<T: Serialize + ?Sized, C: FnMut() -> bool, R, E>(
    v: &T,
    w: &mut RouterWorkV1,
    c: &mut C,
    reserve: &mut impl FnMut(usize) -> Result<R, E>,
) -> Result<Arena<R>, Error<E>> {
    let mut writer = Writer {
        bytes: None,
        len: 0,
        limit: usize::MAX,
        depth: 0,
        in_string: false,
        escaped: false,
        w,
        c,
        error: None,
    };
    if serde_json::to_writer(&mut writer, v).is_err() {
        return Err(Error::from_internal(
            writer.error.unwrap_or(InternalError::Serialize),
        ));
    }
    let n = writer.len;
    let charge = n
        .checked_mul(size_of::<Node>() + size_of::<Edge>() + 2)
        .ok_or(Error::ResourceLimited("capacity overflow"))?;
    let reservation = reserve(charge).map_err(Error::Admission)?;
    let mut bytes = Vec::new();
    let mut arena = Arena {
        nodes: Vec::new(),
        edges: Vec::new(),
        strings: Vec::new(),
        _reservation: reservation,
    };
    bytes
        .try_reserve_exact(n)
        .map_err(|_| Error::ResourceLimited("allocation"))?;
    arena
        .nodes
        .try_reserve_exact(n)
        .map_err(|_| Error::ResourceLimited("allocation"))?;
    arena
        .edges
        .try_reserve_exact(n)
        .map_err(|_| Error::ResourceLimited("allocation"))?;
    arena
        .strings
        .try_reserve_exact(n)
        .map_err(|_| Error::ResourceLimited("allocation"))?;
    if [
        bytes.capacity(),
        arena.nodes.capacity(),
        arena.edges.capacity(),
        arena.strings.capacity(),
    ]
    .into_iter()
    .any(|capacity| capacity > n)
    {
        return Err(Error::ResourceLimited(
            "allocator capacity exceeds admission",
        ));
    }
    let mut writer = Writer {
        bytes: Some(&mut bytes),
        len: 0,
        limit: n,
        depth: 0,
        in_string: false,
        escaped: false,
        w,
        c,
        error: None,
    };
    if serde_json::to_writer(&mut writer, v).is_err() {
        return Err(Error::from_internal(
            writer.error.unwrap_or(InternalError::Serialize),
        ));
    }
    if writer.len != n {
        return Err(Error::Invalid("unstable serialization"));
    }
    let mut parser = Parser {
        b: &bytes,
        i: 0,
        a: &mut arena,
        w,
        c,
    };
    parser.value(0).map_err(Error::from_internal)?;
    if parser.i != n {
        return Err(Error::Invalid("trailing JSON"));
    }
    Ok(arena)
}
struct Parser<'a, R, C> {
    b: &'a [u8],
    i: usize,
    a: &'a mut Arena<R>,
    w: &'a mut RouterWorkV1,
    c: &'a mut C,
}
impl<R, C: FnMut() -> bool> Parser<'_, R, C> {
    fn take(&mut self) -> Result<u8, InternalError> {
        step(self.w, self.c)?;
        let x = *self.b.get(self.i).ok_or(InternalError::Invalid("JSON"))?;
        self.i += 1;
        Ok(x)
    }
    fn string(&mut self) -> Result<Range<usize>, InternalError> {
        if self.take()? != b'"' {
            return Err(InternalError::Invalid("string"));
        }
        let start = self.a.strings.len();
        loop {
            let x = self.take()?;
            if x == b'"' {
                break;
            }
            if x != b'\\' {
                self.a.strings.push(x);
                continue;
            }
            let e = self.take()?;
            match e {
                b'"' | b'\\' | b'/' => self.a.strings.push(e),
                b'b' => self.a.strings.push(8),
                b'f' => self.a.strings.push(12),
                b'n' => self.a.strings.push(10),
                b'r' => self.a.strings.push(13),
                b't' => self.a.strings.push(9),
                b'u' => {
                    let mut u = self.hex()?;
                    if (0xd800..=0xdbff).contains(&u) {
                        if self.take()? != b'\\' || self.take()? != b'u' {
                            return Err(InternalError::Invalid("surrogate"));
                        }
                        let low = self.hex()?;
                        if !(0xdc00..=0xdfff).contains(&low) {
                            return Err(InternalError::Invalid("surrogate"));
                        }
                        u = 0x10000 + ((u - 0xd800) << 10) + (low - 0xdc00)
                    }
                    let ch = char::from_u32(u).ok_or(InternalError::Invalid("unicode"))?;
                    let mut buf = [0; 4];
                    self.a
                        .strings
                        .extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                }
                _ => return Err(InternalError::Invalid("escape")),
            }
        }
        let end = self.a.strings.len();
        self.w.string_bytes_remaining = self
            .w
            .string_bytes_remaining
            .checked_sub((end - start) as u64)
            .ok_or(InternalError::ResourceLimited("string bytes"))?;
        Ok(start..end)
    }
    fn hex(&mut self) -> Result<u32, InternalError> {
        let mut n = 0;
        for _ in 0..4 {
            n = n * 16
                + (self.take()? as char)
                    .to_digit(16)
                    .ok_or(InternalError::Invalid("hex"))?
        }
        Ok(n)
    }
    fn value(&mut self, depth: usize) -> Result<usize, InternalError> {
        step(self.w, self.c)?;
        if depth > self.w.max_depth {
            return Err(InternalError::ResourceLimited("depth"));
        }
        self.w.nodes_remaining = self
            .w
            .nodes_remaining
            .checked_sub(1)
            .ok_or(InternalError::ResourceLimited("nodes"))?;
        let id = self.a.nodes.len();
        self.a.nodes.push(Node {
            kind: Kind::Null,
            first: None,
        });
        let x = *self.b.get(self.i).ok_or(InternalError::Invalid("JSON"))?;
        let kind = match x {
            b'"' => Kind::String(self.string()?),
            b'[' | b'{' => {
                self.take()?;
                let close = if x == b'[' { b']' } else { b'}' };
                let mut last: Option<usize> = None;
                if self.b.get(self.i) != Some(&close) {
                    loop {
                        let key = if x == b'{' {
                            let k = self.string()?;
                            if self.take()? != b':' {
                                return Err(InternalError::Invalid("colon"));
                            }
                            Some(k)
                        } else {
                            None
                        };
                        let child = self.value(depth + 1)?;
                        self.w.edges_remaining = self
                            .w
                            .edges_remaining
                            .checked_sub(1)
                            .ok_or(InternalError::ResourceLimited("edges"))?;
                        let e = self.a.edges.len();
                        self.a.edges.push(Edge {
                            key,
                            child,
                            next: None,
                        });
                        if let Some(last) = last {
                            self.a.edges[last].next = Some(e)
                        } else {
                            self.a.nodes[id].first = Some(e)
                        }
                        last = Some(e);
                        if self.b.get(self.i) == Some(&close) {
                            break;
                        }
                        if self.take()? != b',' {
                            return Err(InternalError::Invalid("comma"));
                        }
                    }
                }
                self.take()?;
                if x == b'[' { Kind::Array } else { Kind::Object }
            }
            b'n' | b't' | b'f' => {
                let lit: &[u8] = match x {
                    b'n' => b"null",
                    b't' => b"true",
                    _ => b"false",
                };
                for expected in lit {
                    if self.take()? != *expected {
                        return Err(InternalError::Invalid("literal"));
                    }
                }
                match x {
                    b'n' => Kind::Null,
                    b't' => Kind::Bool(true),
                    _ => Kind::Bool(false),
                }
            }
            _ => {
                let start = self.i;
                while self
                    .b
                    .get(self.i)
                    .is_some_and(|b| matches!(b, b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E'))
                {
                    self.take()?;
                }
                let token = std::str::from_utf8(&self.b[start..self.i])
                    .map_err(|_| InternalError::Invalid("number"))?;
                let number =
                    if token.bytes().any(|b| matches!(b, b'.' | b'e' | b'E')) || token == "-0" {
                        let f = token
                            .parse::<f64>()
                            .map_err(|_| InternalError::Invalid("float"))?;
                        serde_json::Number::from_f64(f)
                            .ok_or(InternalError::Invalid("nonfinite number"))?
                    } else if token.starts_with('-') {
                        serde_json::Number::from(
                            token
                                .parse::<i64>()
                                .map_err(|_| InternalError::Invalid("integer range"))?,
                        )
                    } else {
                        serde_json::Number::from(
                            token
                                .parse::<u64>()
                                .map_err(|_| InternalError::Invalid("integer range"))?,
                        )
                    };
                Kind::Number(number)
            }
        };
        self.a.nodes[id].kind = kind;
        Ok(id)
    }
}

//! Constant-space closed semantic-key validation; never retains a group/member String.
use super::*;
use std::io::{self, Write};
#[derive(Clone, Copy)]
pub(super) enum Shape {
    Group,
    Member,
    Unit,
}
#[derive(Default)]
struct Utf8 {
    bytes: [u8; 4],
    used: usize,
    width: usize,
}
impl Utf8 {
    fn byte(&mut self, b: u8) -> io::Result<()> {
        if self.used == 0 {
            self.width = match b {
                0..=0x7f => 1,
                0xc2..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf4 => 4,
                _ => return Err(bad()),
            };
        }
        self.bytes[self.used] = b;
        self.used += 1;
        if self.used == self.width {
            std::str::from_utf8(&self.bytes[..self.used]).map_err(|_| bad())?;
            self.used = 0;
        }
        Ok(())
    }
}
fn bad() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "collection semantic key")
}
// domain, root tag, group tag, group length, group text, member tag,
// member length, member text, done. Unit terminates immediately after root tag.
pub(super) struct Probe {
    shape: Shape,
    state: u8,
    offset: usize,
    length: [u8; 8],
    remaining: u64,
    utf8: Utf8,
    hash: blake3::Hasher,
}
impl Probe {
    pub fn new(shape: Shape) -> Self {
        let mut hash = blake3::Hasher::new();
        hash.update(SEMANTIC_KEY_DOMAIN);
        Self {
            shape,
            state: 0,
            offset: 0,
            length: [0; 8],
            remaining: 0,
            utf8: Utf8::default(),
            hash,
        }
    }
    fn byte(&mut self, b: u8) -> io::Result<()> {
        match self.state {
            0 => {
                if SEMANTIC_KEY_DOMAIN.get(self.offset) != Some(&b) {
                    return Err(bad());
                }
                self.offset += 1;
                if self.offset == SEMANTIC_KEY_DOMAIN.len() {
                    self.offset = 0;
                    self.state = 1;
                }
            }
            1 => match self.shape {
                Shape::Unit => {
                    if b != 0 {
                        return Err(bad());
                    }
                    self.state = 8;
                }
                Shape::Group => {
                    if b != 1 {
                        return Err(bad());
                    }
                    self.hash.update(&[b]);
                    self.state = 3;
                }
                Shape::Member => {
                    if b != 5 {
                        return Err(bad());
                    }
                    self.state = 2;
                }
            },
            2 => {
                if b != 1 {
                    return Err(bad());
                }
                self.hash.update(&[b]);
                self.state = 3;
            }
            3 | 6 => {
                self.length[self.offset] = b;
                self.offset += 1;
                if self.state == 3 {
                    self.hash.update(&[b]);
                }
                if self.offset == 8 {
                    self.remaining = u64::from_be_bytes(self.length);
                    self.offset = 0;
                    self.state = if self.remaining == 0 {
                        if self.state == 3 {
                            if matches!(self.shape, Shape::Group) {
                                8
                            } else {
                                5
                            }
                        } else {
                            8
                        }
                    } else if self.state == 3 {
                        4
                    } else {
                        7
                    };
                }
            }
            4 | 7 => {
                self.utf8.byte(b)?;
                if self.state == 4 {
                    self.hash.update(&[b]);
                }
                self.remaining -= 1;
                if self.remaining == 0 {
                    if self.utf8.used != 0 {
                        return Err(bad());
                    }
                    self.state = if self.state == 4 && matches!(self.shape, Shape::Member) {
                        5
                    } else {
                        8
                    };
                }
            }
            5 => {
                if b != 1 {
                    return Err(bad());
                }
                self.state = 6;
            }
            _ => return Err(bad()),
        }
        Ok(())
    }
    pub fn finish(self) -> Result<[u8; 32], ValueTreeError> {
        if self.state != 8 || self.utf8.used != 0 {
            return Err(invalid());
        }
        Ok(*self.hash.finalize().as_bytes())
    }
}
impl Write for Probe {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        for &b in bytes {
            self.byte(b)?;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

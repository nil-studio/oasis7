//! Allocation-free accounting of the CBOR forms accepted by serde_cbor .13.
//! This is a resource probe, never a substitute for the original typed codec.
use super::*;
#[derive(Default, Clone, Copy, Debug)]
pub struct CborDataShapeV1 {
    pub values: u64,
    pub strings: u64,
    pub sequences: u64,
    pub maps: u64,
    pub depth: usize,
}
pub fn probe_cbor_data_shape_v1(
    raw: &[u8],
    max_depth: usize,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<CborDataShapeV1, BoundedCborDataErrorV1> {
    struct Scan<'a, 'b, 'c> {
        raw: &'a [u8],
        at: usize,
        out: CborDataShapeV1,
        max_depth: usize,
        common: &'b mut FormalBoundedContextV1<'c>,
    }
    impl Scan<'_, '_, '_> {
        fn take(&mut self, n: usize) -> Result<bool, BoundedCborDataErrorV1> {
            let Some(end) = self.at.checked_add(n).filter(|n| *n <= self.raw.len()) else {
                return Ok(false);
            };
            self.common
                .charge(0, u64::try_from(n).map_err(|_| limited())?)?;
            self.at = end;
            Ok(true)
        }
        fn count(slot: &mut u64, n: u64) -> Result<(), BoundedCborDataErrorV1> {
            *slot = slot.checked_add(n).ok_or_else(limited)?;
            Ok(())
        }
        fn value(&mut self, depth: usize) -> Result<bool, BoundedCborDataErrorV1> {
            if depth > self.max_depth.min(128) {
                return Err(limited());
            }
            self.common.consume_steps(1)?;
            let Some(&first) = self.raw.get(self.at) else {
                return Ok(false);
            };
            Self::count(&mut self.out.values, 1)?;
            self.out.depth = self.out.depth.max(depth);
            self.take(1)?;
            let major = first >> 5;
            let ai = first & 31;
            let n = match ai {
                0..=23 => ai as u64,
                24..=27 => {
                    let width = 1usize << (ai - 24);
                    let start = self.at;
                    if !self.take(width)? {
                        return Ok(false);
                    };
                    let mut n = 0u64;
                    for b in &self.raw[start..self.at] {
                        n = (n << 8) | u64::from(*b)
                    }
                    n
                }
                31 => {
                    match major {
                        2 | 3 => loop {
                            let Some(&b) = self.raw.get(self.at) else {
                                return Ok(false);
                            };
                            if b == 255 {
                                self.take(1)?;
                                break;
                            }
                            if b >> 5 != major || b & 31 == 31 {
                                return Ok(false);
                            }
                            if !self.value(depth + 1)? {
                                return Ok(false);
                            }
                        },
                        4 => loop {
                            let Some(&b) = self.raw.get(self.at) else {
                                return Ok(false);
                            };
                            if b == 255 {
                                self.take(1)?;
                                break;
                            }
                            Self::count(&mut self.out.sequences, 1)?;
                            if !self.value(depth + 1)? {
                                return Ok(false);
                            }
                        },
                        5 => loop {
                            let Some(&b) = self.raw.get(self.at) else {
                                return Ok(false);
                            };
                            if b == 255 {
                                self.take(1)?;
                                break;
                            }
                            Self::count(&mut self.out.maps, 1)?;
                            if !self.value(depth + 1)? || !self.value(depth + 1)? {
                                return Ok(false);
                            }
                        },
                        _ => return Ok(false),
                    }
                    return Ok(true);
                }
                _ => return Ok(false),
            };
            match major {
                0 | 1 | 7 => Ok(true),
                2 | 3 => {
                    Self::count(&mut self.out.strings, n)?;
                    self.take(usize::try_from(n).map_err(|_| limited())?)
                }
                4 | 5 => {
                    let count = if major == 4 {
                        Self::count(&mut self.out.sequences, n)?;
                        n
                    } else {
                        Self::count(&mut self.out.maps, n)?;
                        n.checked_mul(2).ok_or_else(limited)?
                    };
                    // A malformed huge hint must be admitted before the real serde visitor
                    // can reserve from it, even though the declared children do not exist.
                    if count > self.raw.len().saturating_sub(self.at) as u64 {
                        return Ok(false);
                    }
                    for _ in 0..count {
                        if !self.value(depth + 1)? {
                            return Ok(false);
                        }
                    }
                    Ok(true)
                }
                6 => self.value(depth + 1),
                _ => Ok(false),
            }
        }
    }
    let mut scanner = Scan {
        raw,
        at: 0,
        out: CborDataShapeV1::default(),
        max_depth,
        common,
    };
    let _valid = scanner.value(0)?;
    // The original decoder decides malformed/trailing bytes and all predicates.
    // The prefix plus every observed declared hint already bounds its attempts.
    Ok(scanner.out)
}

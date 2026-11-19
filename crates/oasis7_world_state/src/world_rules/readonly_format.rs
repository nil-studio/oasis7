//! Original std::fmt output with a work-only, exact allocation admission.
use std::{fmt, mem::size_of};
struct Output<'a, E, W> {
    work: &'a mut W,
    first: Option<E>,
    n: usize,
    max: usize,
    bytes: Option<Vec<u8>>,
}
impl<E, W: FnMut(u64, usize, usize) -> Result<(), E>> fmt::Write for Output<'_, E, W> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        if self.first.is_some() {
            return Err(fmt::Error);
        }
        for chunk in text.as_bytes().chunks(8192) {
            if let Err(error) = (self.work)(1, chunk.len(), 0) {
                self.first = Some(error);
                return Err(fmt::Error);
            }
            self.n = self
                .n
                .checked_add(chunk.len())
                .expect("readonly format length overflow");
            assert!(self.n <= self.max, "readonly fmt changed length");
            if let Some(bytes) = &mut self.bytes {
                bytes.extend_from_slice(chunk);
            }
        }
        Ok(())
    }
}
pub fn format<E, W: FnMut(u64, usize, usize) -> Result<(), E>>(
    args: fmt::Arguments<'_>,
    work: &mut W,
) -> Result<String, E> {
    let mut count = Output {
        work,
        first: None,
        n: 0,
        max: usize::MAX,
        bytes: None,
    };
    let result = fmt::write(&mut count, args);
    if let Some(error) = count.first.take() {
        return Err(error);
    }
    result.expect("original readonly fmt implementation failed");
    let n = count.n;
    drop(count);
    work(
        1,
        0,
        n.checked_add(size_of::<String>())
            .expect("readonly format allocation overflow"),
    )?;
    let bytes = Vec::with_capacity(n);
    assert_eq!(bytes.capacity(), n, "readonly format exact capacity");
    let mut output = Output {
        work,
        first: None,
        n: 0,
        max: n,
        bytes: Some(bytes),
    };
    let result = fmt::write(&mut output, args);
    if let Some(error) = output.first.take() {
        return Err(error);
    }
    result.expect("original readonly fmt implementation failed");
    assert_eq!(output.n, n, "readonly fmt changed length");
    Ok(String::from_utf8(output.bytes.take().unwrap()).expect("std fmt emitted invalid UTF-8"))
}

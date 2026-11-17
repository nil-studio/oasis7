//! The counting point is the controlled body reader/writer, not serialization
//! or a reqwest body producer. HTTP headers/framing must bypass these methods.
use super::operation_ledger::{AttemptPermit, Direction, ExchangePermit, Ledger};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

pub(super) fn remaining(deadline: Instant, request_timeout: Duration) -> Result<Duration, String> {
    let left = deadline
        .checked_duration_since(Instant::now())
        .filter(|left| !left.is_zero())
        .ok_or("attempt_expired")?;
    Ok(left.min(request_timeout))
}

/// Sets the actual socket timeout after durable reservation using fresh remaining.
/// Each call is one window, so a slow peer cannot refresh the absolute deadline.
pub(super) fn write_window(
    writer: &mut TcpStream,
    ledger: &mut Ledger,
    attempt: &AttemptPermit,
    request: &ExchangePermit,
    body: &[u8],
    deadline: Instant,
    request_timeout: Duration,
) -> Result<usize, String> {
    let deadline = deadline.min(attempt.deadline());
    remaining(deadline, Duration::MAX)?;
    if body.is_empty() {
        return Ok(0);
    }
    let window = ledger.body_window_limit(Direction::Tx, body.len())?;
    let token = ledger.reserve(attempt, request, Direction::Tx, window as u64)?;
    // Persistence can consume the remainder of the deadline. No socket call
    // has happened yet, so releasing this exact unused reservation is safe.
    if let Err(error) = remaining(deadline, Duration::MAX) {
        ledger.settle(&token, 0)?;
        return Err(error);
    }
    let timeout = match remaining(deadline, request_timeout) {
        Ok(timeout) => timeout,
        Err(error) => {
            ledger.settle(&token, 0)?;
            return Err(error);
        }
    };
    if let Err(error) = writer.set_write_timeout(Some(timeout)) {
        ledger.settle(&token, 0)?;
        return Err(error.to_string());
    }
    match writer.write(&body[..window]) {
        Ok(n) => {
            ledger.settle(&token, n as u64)?;
            remaining(deadline, Duration::MAX)?;
            if n == 0 {
                return Err("full-sync body writer made no progress".into());
            }
            Ok(n)
        }
        // TcpStream reports no accepted bytes
        // on Err. TLS adapters with ambiguous failures cannot use this primitive.
        Err(error) => {
            ledger.settle(&token, 0)?;
            Err(format!("full-sync body write: {error}"))
        }
    }
}

pub(super) fn read_window(
    reader: &mut TcpStream,
    ledger: &mut Ledger,
    attempt: &AttemptPermit,
    request: &ExchangePermit,
    output: &mut [u8],
    deadline: Instant,
    request_timeout: Duration,
) -> Result<usize, String> {
    let deadline = deadline.min(attempt.deadline());
    remaining(deadline, Duration::MAX)?;
    if output.is_empty() {
        return Err("empty body read window cannot establish EOF".into());
    }
    let window = ledger.body_window_limit(Direction::Rx, output.len())?;
    let token = ledger.reserve(attempt, request, Direction::Rx, window as u64)?;
    if let Err(error) = remaining(deadline, Duration::MAX) {
        ledger.settle(&token, 0)?;
        return Err(error);
    }
    let timeout = match remaining(deadline, request_timeout) {
        Ok(timeout) => timeout,
        Err(error) => {
            ledger.settle(&token, 0)?;
            return Err(error);
        }
    };
    if let Err(error) = reader.set_read_timeout(Some(timeout)) {
        ledger.settle(&token, 0)?;
        return Err(error.to_string());
    }
    match reader.read(&mut output[..window]) {
        Ok(n) => {
            ledger.settle(&token, n as u64)?;
            remaining(deadline, Duration::MAX)?;
            Ok(n)
        }
        Err(error) => {
            ledger.settle(&token, 0)?;
            Err(format!("full-sync body read: {error}"))
        }
    }
}

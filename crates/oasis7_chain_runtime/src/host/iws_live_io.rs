//! Bounded local JSON-lines DATA transport. No request replenishes the finite
//! CLI accounts or the service's distinct Common account. Replies are fully
//! encoded before stdout receives any part of an accepted observation.
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CommandV1<'a> {
    pub command: &'a str,
    pub action_id: Option<u64>,
    pub payload_hex: Option<&'a str>,
    pub sender: Option<&'a str>,
    pub receiver: Option<&'a str>,
}
impl CommandV1<'_> {
    pub(super) fn validate(&self) -> Result<(), String> {
        let submission = self.action_id.is_some() && self.payload_hex.is_some();
        let no_submission = self.action_id.is_none() && self.payload_hex.is_none();
        let agents = self.sender.is_some() && self.receiver.is_some();
        let no_agents = self.sender.is_none() && self.receiver.is_none();
        match self.command {
            "submit" if submission && no_agents => Ok(()),
            "observe" if agents && no_submission => Ok(()),
            "status" | "start" | "stop" if no_submission && no_agents => Ok(()),
            _ => Err("unsupported local command or fields".into()),
        }
    }
}

pub(super) struct LocalIoV1 {
    request: Vec<u8>,
    reply: Vec<u8>,
    max_request: usize,
    max_reply: usize,
    steps: u64,
    bytes: u64,
    // Buffers, JSON scratch, a bounded decoded submission and errors all drop
    // before this fee. Admitted Node retention has its own lifetime envelope.
    _fee: MemoryReservation,
}
fn charge(steps: &mut u64, bytes: &mut u64, amount: usize) -> std::io::Result<()> {
    let amount = u64::try_from(amount).map_err(std::io::Error::other)?;
    if *steps == 0 || amount > *bytes {
        return Err(std::io::Error::other("finite CLI work exhausted"));
    }
    *steps -= 1;
    *bytes -= amount;
    Ok(())
}
impl LocalIoV1 {
    pub(super) fn new(
        memory: &MemoryBudget,
        max_request: usize,
        max_reply: usize,
        steps: u64,
        bytes: u64,
    ) -> Result<Self, String> {
        let layout = max_request
            .checked_mul(4)
            .and_then(|n| n.checked_add(max_reply))
            .and_then(|n| n.checked_add(64 * 1024))
            .ok_or("local transport memory overflow")?;
        let fee = memory
            .reserve(layout, MemoryClass::Ordinary)
            .map_err(|e| e.to_string())?;
        let mut request = Vec::new();
        request
            .try_reserve_exact(max_request)
            .map_err(|e| e.to_string())?;
        let mut reply = Vec::new();
        reply
            .try_reserve_exact(max_reply)
            .map_err(|e| e.to_string())?;
        if request.capacity() > max_request || reply.capacity() > max_reply {
            return Err("local transport allocation exceeded its admitted capacity".into());
        }
        Ok(Self {
            request,
            reply,
            max_request,
            max_reply,
            steps,
            bytes,
            _fee: fee,
        })
    }

    pub(super) fn read(&mut self, input: &mut impl BufRead) -> Result<bool, String> {
        self.request.clear();
        loop {
            let available = input.fill_buf().map_err(|e| e.to_string())?;
            if available.is_empty() {
                return Ok(!self.request.is_empty());
            }
            let length = available
                .iter()
                .position(|b| *b == b'\n')
                .map_or(available.len(), |n| n + 1);
            if length > self.max_request.saturating_sub(self.request.len()) {
                return Err("local request exceeds its explicit wire bound".into());
            }
            charge(&mut self.steps, &mut self.bytes, length).map_err(|e| e.to_string())?;
            self.request.extend_from_slice(&available[..length]);
            let complete = available[length - 1] == b'\n';
            input.consume(length);
            if complete {
                return Ok(true);
            }
        }
    }
    /// Parsing borrows strings directly from the admitted line. Escaped string
    /// spellings are intentionally unsupported; no command owns an extra graph.
    pub(super) fn command(&mut self) -> Result<CommandV1<'_>, String> {
        charge(&mut self.steps, &mut self.bytes, self.request.len()).map_err(|e| e.to_string())?;
        let command: CommandV1<'_> = serde_json::from_slice(&self.request)
            .map_err(|_| "invalid local command JSON".to_owned())?;
        command.validate()?;
        Ok(command)
    }
    pub(super) fn encode(&mut self, value: &impl Serialize) -> Result<(), String> {
        self.reply.clear();
        struct Sink<'a> {
            output: &'a mut Vec<u8>,
            max: usize,
            steps: &'a mut u64,
            bytes: &'a mut u64,
        }
        impl Write for Sink<'_> {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if bytes.len() > self.max.saturating_sub(self.output.len()) {
                    return Err(std::io::Error::other(
                        "local reply exceeds its explicit wire bound",
                    ));
                }
                charge(self.steps, self.bytes, bytes.len())?;
                self.output.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut sink = Sink {
            output: &mut self.reply,
            max: self.max_reply,
            steps: &mut self.steps,
            bytes: &mut self.bytes,
        };
        serde_json::to_writer(&mut sink, value).map_err(|e| e.to_string())?;
        sink.write_all(b"\n").map_err(|e| e.to_string())
    }
    pub(super) fn emit(&self, output: &mut impl Write) -> Result<(), String> {
        output
            .write_all(&self.reply)
            .and_then(|()| output.flush())
            .map_err(|e| e.to_string())
    }
}

#[derive(Clone, Copy)]
pub(super) struct HexIdV1<'a>(pub &'a [u8; 32]);
impl Serialize for HexIdV1<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut output = [0u8; 64];
        hex::encode_to_slice(self.0, &mut output).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(std::str::from_utf8(&output).map_err(serde::ser::Error::custom)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_json_lines_enforces_wire_and_work_without_refills() {
        let memory = MemoryBudget::new(1 << 20, 0).unwrap();
        let mut io = LocalIoV1::new(&memory, 64, 64, 100, 1000).unwrap();
        let used = memory.used();
        let mut source = std::io::Cursor::new(b"{\"command\":\"status\"}\n");
        assert!(io.read(&mut source).unwrap());
        assert_eq!(io.command().unwrap().command, "status");
        assert!(!io.read(&mut source).unwrap());
        assert!(io.read(&mut std::io::Cursor::new([b'x'; 65])).is_err());
        assert_eq!(memory.used(), used);
        drop(io);
        assert_eq!(memory.used(), 0);
        let mut io = LocalIoV1::new(&memory, 64, 64, 1, 1).unwrap();
        assert!(io.read(&mut std::io::Cursor::new(b"{}\n")).is_err());
        assert!(io.read(&mut std::io::Cursor::new(b"{}\n")).is_err());
    }
    #[test]
    fn local_json_rejects_unknown_commands_and_unrelated_fields() {
        for raw in [
            r#"{"command":"submit"}"#,
            r#"{"command":"status","action_id":1}"#,
            r#"{"command":"observe","sender":"a"}"#,
            r#"{"command":"world"}"#,
            r#"{"command":"status","unchecked":true}"#,
        ] {
            let parsed = serde_json::from_str::<CommandV1<'_>>(raw);
            assert!(parsed.is_err() || parsed.unwrap().validate().is_err());
        }
    }
    #[test]
    fn failed_reply_consumes_original_work_without_refills() {
        let memory = MemoryBudget::new(1 << 20, 0).unwrap();
        let mut io = LocalIoV1::new(&memory, 64, 4, 100, 1000).unwrap();
        assert!(io.encode(&"accepted").is_err());
        let before = io.bytes;
        assert!(io.encode(&"accepted").is_err());
        assert!(io.bytes < before);
    }
}

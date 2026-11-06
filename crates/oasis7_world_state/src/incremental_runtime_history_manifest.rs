//! Exact Runtime History capsule codec. Decoding grants no qualification.
use oasis7_formal_protocol::incremental_state::{STATE_ROOT_BYTES, StateNamespaceV1, StateRootV1};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
pub const RUNTIME_HISTORY_MAGIC_V1: &[u8] = b"oasis7.runtime.history-view.v1\0";
pub const RUNTIME_HISTORY_MANIFEST_BYTES_V1: usize =
    RUNTIME_HISTORY_MAGIC_V1.len() + 32 + 33 + 4 * (STATE_ROOT_BYTES + 24) + 16;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RuntimeHistoryLaneV1 {
    Snapshot = 0,
    Journal = 1,
    Tick = 2,
    Audit = 3,
}
impl RuntimeHistoryLaneV1 {
    pub const ALL: [Self; 4] = [Self::Snapshot, Self::Journal, Self::Tick, Self::Audit];
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Snapshot => "history.snapshot.index.v1",
            Self::Journal => "history.journal.index.v1",
            Self::Tick => "history.tick.index.v1",
            Self::Audit => "history.audit.index.v1",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeHistoryWindowV1 {
    pub root: StateRootV1,
    pub total: u64,
    pub start: u64,
    pub count: u64,
}
impl RuntimeHistoryWindowV1 {
    pub fn end(&self) -> Result<u64, ValueTreeError> {
        self.start
            .checked_add(self.count)
            .filter(|n| *n <= self.total)
            .ok_or_else(invalid)
    }
    pub fn validate(&self) -> Result<(), ValueTreeError> {
        self.root.validate().map_err(|_| invalid())?;
        self.end()?;
        if self.root.namespace != StateNamespaceV1::Artifacts
            || self.root.leaf_count != self.total.checked_add(2).ok_or_else(invalid)?
        {
            return Err(invalid());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeHistoryManifestV1 {
    pub roots: [u8; 32],
    pub previous_view: Option<[u8; 32]>,
    pub lanes: [RuntimeHistoryWindowV1; 4],
    pub max_snapshots: u64,
    pub archived_tick_count: u64,
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Runtime History capsule framing/window")
}
impl RuntimeHistoryManifestV1 {
    pub fn validate(&self) -> Result<(), ValueTreeError> {
        for lane in &self.lanes {
            lane.validate()?;
        }
        if self.lanes[0].count > self.max_snapshots
            || self.archived_tick_count > self.lanes[2].total
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn canonical_bytes(
        &self,
    ) -> Result<[u8; RUNTIME_HISTORY_MANIFEST_BYTES_V1], ValueTreeError> {
        self.validate()?;
        let mut out = [0; RUNTIME_HISTORY_MANIFEST_BYTES_V1];
        let mut at = 0;
        fn put(out: &mut [u8], at: &mut usize, v: &[u8]) {
            out[*at..*at + v.len()].copy_from_slice(v);
            *at += v.len();
        }
        put(&mut out, &mut at, RUNTIME_HISTORY_MAGIC_V1);
        put(&mut out, &mut at, &self.roots);
        put(&mut out, &mut at, &[u8::from(self.previous_view.is_some())]);
        put(&mut out, &mut at, &self.previous_view.unwrap_or([0; 32]));
        for lane in &self.lanes {
            put(
                &mut out,
                &mut at,
                &lane.root.canonical_bytes().map_err(|_| invalid())?,
            );
            for n in [lane.total, lane.start, lane.count] {
                put(&mut out, &mut at, &n.to_be_bytes());
            }
        }
        for n in [self.max_snapshots, self.archived_tick_count] {
            put(&mut out, &mut at, &n.to_be_bytes());
        }
        Ok(out)
    }
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ValueTreeError> {
        if bytes.len() != RUNTIME_HISTORY_MANIFEST_BYTES_V1
            || !bytes.starts_with(RUNTIME_HISTORY_MAGIC_V1)
        {
            return Err(invalid());
        }
        struct Read<'a> {
            bytes: &'a [u8],
            at: usize,
        }
        impl Read<'_> {
            fn take(&mut self, n: usize) -> &[u8] {
                let start = self.at;
                self.at += n;
                &self.bytes[start..self.at]
            }
            fn num(&mut self) -> u64 {
                u64::from_be_bytes(self.take(8).try_into().expect("fixed checked framing"))
            }
        }
        let mut r = Read {
            bytes,
            at: RUNTIME_HISTORY_MAGIC_V1.len(),
        };
        let roots = r.take(32).try_into().map_err(|_| invalid())?;
        let present = r.take(1)[0];
        let previous: [u8; 32] = r.take(32).try_into().map_err(|_| invalid())?;
        let previous_view = match present {
            0 if previous == [0; 32] => None,
            1 => Some(previous),
            _ => return Err(invalid()),
        };
        fn lane(r: &mut Read<'_>) -> Result<RuntimeHistoryWindowV1, ValueTreeError> {
            Ok(RuntimeHistoryWindowV1 {
                root: StateRootV1::from_canonical_bytes(r.take(STATE_ROOT_BYTES))
                    .map_err(|_| invalid())?,
                total: r.num(),
                start: r.num(),
                count: r.num(),
            })
        }
        let result = Self {
            roots,
            previous_view,
            lanes: [lane(&mut r)?, lane(&mut r)?, lane(&mut r)?, lane(&mut r)?],
            max_snapshots: r.num(),
            archived_tick_count: r.num(),
        };
        result.validate()?;
        Ok(result)
    }
}

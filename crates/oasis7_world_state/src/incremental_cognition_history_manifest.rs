//! Fixed storage-view metadata inside an ordinary Result original. These
//! public values are unqualified input; decoding does not select an accepted
//! head or prove any index/record closure.
use super::*;
use oasis7_formal_protocol::incremental_material::FactLocationV1;
use oasis7_formal_protocol::incremental_state::{STATE_ROOT_BYTES, StateNamespaceV1, StateRootV1};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

pub const COGNITION_HISTORY_VIEW_MAGIC: &[u8] = b"oasis7.cognition.history-view.v1\0";
pub const COGNITION_HISTORY_VIEW_BYTES: usize = COGNITION_HISTORY_VIEW_MAGIC.len()
    + 32
    + 1
    + 8
    + 1
    + 48
    + 3 * (8 + 1 + 32)
    + 4 * (1 + 8 + STATE_ROOT_BYTES);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CognitionHistoryHeadV1 {
    pub sequence: u64,
    /// None encodes the original empty digest, not an invented zero digest.
    pub digest: Option<[u8; 32]>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CognitionHistoryIndexRefV1 {
    pub present: bool,
    pub records: u64,
    pub root: StateRootV1,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CognitionHistoryViewV1 {
    pub roots: [u8; 32],
    pub enabled: bool,
    /// Storage-view revision only. Array ordinal remains view-local.
    pub revision: u64,
    /// A locator hint, authenticated through the selected accepted prefix.
    pub previous_view: Option<FactLocationV1>,
    /// Journal, economy journal, provisioning journal, in that order.
    pub heads: [CognitionHistoryHeadV1; 3],
    /// The exact order is CognitionHistoryFieldV1::ALL.
    pub indexes: [CognitionHistoryIndexRefV1; 4],
}
impl CognitionHistoryViewV1 {
    pub fn index_ref(&self, field: CognitionHistoryFieldV1) -> &CognitionHistoryIndexRefV1 {
        &self.indexes[match field {
            CognitionHistoryFieldV1::JournalEvents => 0,
            CognitionHistoryFieldV1::Responses => 1,
            CognitionHistoryFieldV1::EconomyJournal => 2,
            CognitionHistoryFieldV1::EconomyProvisionJournal => 3,
        }]
    }
    pub fn validate_shape(&self) -> Result<(), ValueTreeError> {
        for index in &self.indexes {
            index
                .root
                .validate()
                .map_err(|_| ValueTreeError::Invalid("Cognition History index root"))?;
            if index.root.namespace != StateNamespaceV1::Artifacts {
                return Err(ValueTreeError::Invalid("Cognition History index namespace"));
            }
            let expected = if index.present {
                index.records.checked_add(2).ok_or(ValueTreeError::Invalid(
                    "Cognition History index count overflow",
                ))?
            } else {
                if index.records != 0 {
                    return Err(ValueTreeError::Invalid(
                        "absent Cognition History index count",
                    ));
                }
                0
            };
            if index.root.leaf_count != expected {
                return Err(ValueTreeError::Invalid(
                    "Cognition History index cardinality",
                ));
            }
        }
        if self.enabled {
            if !self.indexes[0].present
                || !self.indexes[1].present
                || self.indexes[2].present != self.indexes[3].present
            {
                return Err(ValueTreeError::Invalid("Cognition History cohort presence"));
            }
            if !self.indexes[2].present
                && self.heads[1..]
                    .iter()
                    .any(|h| h.sequence != 0 || h.digest.is_some())
            {
                return Err(ValueTreeError::Invalid("absent Cognition economy head"));
            }
        } else if self.indexes.iter().any(|i| i.present)
            || self
                .heads
                .iter()
                .any(|h| h.sequence != 0 || h.digest.is_some())
        {
            return Err(ValueTreeError::Invalid(
                "disabled Cognition History contents",
            ));
        }
        Ok(())
    }
    /// Fixed-width canonical metadata; all heap/scratch is admitted before
    /// the four small existing StateRoot encoders allocate their fixed wire.
    pub fn canonical_bytes(
        &self,
        memory: &MemoryBudget,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<ReservedValue<[u8; COGNITION_HISTORY_VIEW_BYTES]>, ValueTreeError> {
        let mut charge = memory.reserve(8192, MemoryClass::Ordinary)?;
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        self.validate_shape()?;
        let mut bytes = [0u8; COGNITION_HISTORY_VIEW_BYTES];
        let mut at = 0;
        let mut put = |part: &[u8]| {
            bytes[at..at + part.len()].copy_from_slice(part);
            at += part.len();
        };
        put(COGNITION_HISTORY_VIEW_MAGIC);
        put(&self.roots);
        put(&[u8::from(self.enabled)]);
        put(&self.revision.to_be_bytes());
        put(&[u8::from(self.previous_view.is_some())]);
        let previous = self.previous_view.unwrap_or(FactLocationV1 {
            original: [0; 32],
            height: 0,
            ordinal: 0,
        });
        put(&previous.original);
        put(&previous.height.to_be_bytes());
        put(&previous.ordinal.to_be_bytes());
        for head in &self.heads {
            put(&head.sequence.to_be_bytes());
            put(&[u8::from(head.digest.is_some())]);
            put(&head.digest.unwrap_or([0; 32]));
        }
        for index in &self.indexes {
            if cancelled() {
                return Err(ValueTreeError::Cancelled);
            }
            put(&[u8::from(index.present)]);
            put(&index.records.to_be_bytes());
            put(&index
                .root
                .canonical_bytes()
                .map_err(|_| ValueTreeError::Invalid("Cognition History root encoding"))?);
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        let retained = charge
            .split(std::mem::size_of_val(&bytes))
            .map_err(|_| ValueTreeError::Invalid("Cognition History manifest accounting"))?;
        Ok(retained.retain(bytes))
    }
    pub fn from_canonical_bytes(
        bytes: &[u8],
        memory: &MemoryBudget,
        class: MemoryClass,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<ReservedValue<Self>, ValueTreeError> {
        let charge = memory.reserve(8192, class)?;
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        if bytes.len() != COGNITION_HISTORY_VIEW_BYTES
            || !bytes.starts_with(COGNITION_HISTORY_VIEW_MAGIC)
        {
            return Err(ValueTreeError::Invalid(
                "Cognition History manifest format/length",
            ));
        }
        let mut reader = Reader {
            bytes,
            at: COGNITION_HISTORY_VIEW_MAGIC.len(),
        };
        let roots = reader.array()?;
        let enabled = reader.flag()?;
        let revision = u64::from_be_bytes(reader.array()?);
        let previous_view = reader.optional_location()?;
        let mut head = || -> Result<_, ValueTreeError> {
            Ok(CognitionHistoryHeadV1 {
                sequence: u64::from_be_bytes(reader.array()?),
                digest: reader.optional_digest()?,
            })
        };
        let heads = [head()?, head()?, head()?];
        let mut index = || -> Result<_, ValueTreeError> {
            if cancelled() {
                return Err(ValueTreeError::Cancelled);
            }
            Ok(CognitionHistoryIndexRefV1 {
                present: reader.flag()?,
                records: u64::from_be_bytes(reader.array()?),
                root: StateRootV1::from_canonical_bytes(reader.take(STATE_ROOT_BYTES)?)
                    .map_err(|_| ValueTreeError::Invalid("Cognition History root decoding"))?,
            })
        };
        let indexes = [index()?, index()?, index()?, index()?];
        let value = Self {
            roots,
            enabled,
            revision,
            previous_view,
            heads,
            indexes,
        };
        value.validate_shape()?;
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(charge.retain(value))
    }
}
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], ValueTreeError> {
        let end = self
            .at
            .checked_add(length)
            .ok_or(ValueTreeError::Invalid("Cognition History frame overflow"))?;
        let bytes = self
            .bytes
            .get(self.at..end)
            .ok_or(ValueTreeError::Invalid("Cognition History truncated frame"))?;
        self.at = end;
        Ok(bytes)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], ValueTreeError> {
        self.take(N)?
            .try_into()
            .map_err(|_| ValueTreeError::Invalid("Cognition History frame width"))
    }
    fn flag(&mut self) -> Result<bool, ValueTreeError> {
        match self.take(1)?[0] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(ValueTreeError::Invalid("Cognition History flag")),
        }
    }
    fn optional_digest(&mut self) -> Result<Option<[u8; 32]>, ValueTreeError> {
        let present = self.flag()?;
        let bytes = self.array()?;
        if !present && bytes != [0; 32] {
            return Err(ValueTreeError::Invalid(
                "Cognition History absent digest padding",
            ));
        }
        Ok(present.then_some(bytes))
    }
    fn optional_location(&mut self) -> Result<Option<FactLocationV1>, ValueTreeError> {
        let present = self.flag()?;
        let original = self.array()?;
        let height = u64::from_be_bytes(self.array()?);
        let ordinal = u64::from_be_bytes(self.array()?);
        if !present && (original != [0; 32] || height != 0 || ordinal != 0) {
            return Err(ValueTreeError::Invalid(
                "Cognition History absent location padding",
            ));
        }
        Ok(present.then_some(FactLocationV1 {
            original,
            height,
            ordinal,
        }))
    }
}

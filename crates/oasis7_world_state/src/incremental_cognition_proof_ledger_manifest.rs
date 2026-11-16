//! Independent ProofLedger Result profile. Never a RootSet Current leaf.
use oasis7_formal_protocol::incremental_state::{STATE_ROOT_BYTES, StateNamespaceV1, StateRootV1};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
pub const COGNITION_PROOF_LEDGER_MAGIC_V1: &[u8] = b"oasis7.cognition.proof-ledger.v1\0";
pub const COGNITION_PROOF_LEDGER_BYTES_V1: usize =
    COGNITION_PROOF_LEDGER_MAGIC_V1.len() + 32 + 33 + 5 * (1 + STATE_ROOT_BYTES);
pub const COGNITION_PROOF_LEDGER_MAGIC_V2: &[u8] = b"oasis7.cognition.proof-ledger.v2\0";
pub const COGNITION_PROOF_LEDGER_BYTES_V2: usize =
    COGNITION_PROOF_LEDGER_MAGIC_V2.len() + 32 + 33 + 6 * (1 + STATE_ROOT_BYTES);
pub const COGNITION_PROOF_LEDGER_MAGIC_V3: &[u8] = b"oasis7.cognition.proof-ledger.v3\0";
pub const COGNITION_PROOF_LEDGER_BYTES_V3: usize =
    COGNITION_PROOF_LEDGER_MAGIC_V3.len() + 32 + 33 + 8 * (1 + STATE_ROOT_BYTES);
pub const COGNITION_PROOF_LEDGER_MAX_BYTES: usize = COGNITION_PROOF_LEDGER_BYTES_V3;
/// The wire version is explicit even when later optional subtrees are absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CognitionProofLedgerWireVersionV1 {
    V1,
    V2,
    V3,
}
impl CognitionProofLedgerWireVersionV1 {
    pub fn field_count(self) -> usize {
        match self {
            Self::V1 => 5,
            Self::V2 => 6,
            Self::V3 => 8,
        }
    }
    pub fn encoded_len(self) -> usize {
        match self {
            Self::V1 => COGNITION_PROOF_LEDGER_BYTES_V1,
            Self::V2 => COGNITION_PROOF_LEDGER_BYTES_V2,
            Self::V3 => COGNITION_PROOF_LEDGER_BYTES_V3,
        }
    }
    fn magic(self) -> &'static [u8] {
        match self {
            Self::V1 => COGNITION_PROOF_LEDGER_MAGIC_V1,
            Self::V2 => COGNITION_PROOF_LEDGER_MAGIC_V2,
            Self::V3 => COGNITION_PROOF_LEDGER_MAGIC_V3,
        }
    }
}
/// Bounded stack carrier. Only its initialized wire prefix is exposed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CognitionProofLedgerCanonicalBytesV1 {
    bytes: [u8; COGNITION_PROOF_LEDGER_MAX_BYTES],
    len: usize,
}
impl AsRef<[u8]> for CognitionProofLedgerCanonicalBytesV1 {
    fn as_ref(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}
impl std::ops::Deref for CognitionProofLedgerCanonicalBytesV1 {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.as_ref()
    }
}
impl std::ops::DerefMut for CognitionProofLedgerCanonicalBytesV1 {
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.bytes[..self.len]
    }
}
pub fn is_cognition_proof_ledger_original_v1(raw: &[u8]) -> bool {
    raw.starts_with(COGNITION_PROOF_LEDGER_MAGIC_V1)
        || raw.starts_with(COGNITION_PROOF_LEDGER_MAGIC_V2)
        || raw.starts_with(COGNITION_PROOF_LEDGER_MAGIC_V3)
}
pub fn is_cognition_proof_ledger_length_v1(len: usize) -> bool {
    len == COGNITION_PROOF_LEDGER_BYTES_V1
        || len == COGNITION_PROOF_LEDGER_BYTES_V2
        || len == COGNITION_PROOF_LEDGER_BYTES_V3
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CognitionProofLedgerFieldV1 {
    CommitRecords = 0,
    ReceiptRegistry = 1,
    ReceiptLineageRegistry = 2,
    Recovery = 3,
    RuntimeBinding = 4,
    DelegationReceipts = 5,
    ServiceCausalProposals = 6,
    ActionDigests = 7,
}
impl CognitionProofLedgerFieldV1 {
    pub const ALL: [Self; 8] = [
        Self::CommitRecords,
        Self::ReceiptRegistry,
        Self::ReceiptLineageRegistry,
        Self::Recovery,
        Self::RuntimeBinding,
        Self::DelegationReceipts,
        Self::ServiceCausalProposals,
        Self::ActionDigests,
    ];
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::CommitRecords => "commit_records",
            Self::ReceiptRegistry => "receipt_registry",
            Self::ReceiptLineageRegistry => "receipt_lineage_registry",
            Self::Recovery => "recovery",
            Self::RuntimeBinding => "runtime_binding",
            Self::DelegationReceipts => "delegation.receipts",
            Self::ServiceCausalProposals => "service_causal_proposals",
            Self::ActionDigests => "action_digests",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CognitionProofLedgerManifestV1 {
    pub wire_version: CognitionProofLedgerWireVersionV1,
    pub roots: [u8; 32],
    pub previous_record: Option<[u8; 32]>,
    pub fields: [Option<StateRootV1>; 8],
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Cognition ProofLedger manifest")
}
impl CognitionProofLedgerManifestV1 {
    pub fn require_field_source(
        &self,
        field: CognitionProofLedgerFieldV1,
    ) -> Result<(), ValueTreeError> {
        if field.index() >= self.wire_version.field_count() {
            return Err(ValueTreeError::Invalid(match field {
                CognitionProofLedgerFieldV1::DelegationReceipts => {
                    "legacy ProofLedger delegation receipts source missing"
                }
                _ => "legacy ProofLedger cognition service field source missing",
            }));
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), ValueTreeError> {
        if self.fields[self.wire_version.field_count()..]
            .iter()
            .any(Option::is_some)
        {
            return Err(invalid());
        }
        if self.fields[..4].iter().any(Option::is_none) {
            return Err(invalid());
        }
        for root in self.fields.iter().flatten() {
            root.validate().map_err(|_| invalid())?;
            if root.namespace != StateNamespaceV1::Artifacts {
                return Err(invalid());
            }
        }
        Ok(())
    }
    pub fn canonical_bytes(&self) -> Result<CognitionProofLedgerCanonicalBytesV1, ValueTreeError> {
        self.validate()?;
        let mut out = [0; COGNITION_PROOF_LEDGER_MAX_BYTES];
        let mut at = 0;
        fn put(out: &mut [u8], at: &mut usize, v: &[u8]) {
            out[*at..*at + v.len()].copy_from_slice(v);
            *at += v.len();
        }
        put(&mut out, &mut at, self.wire_version.magic());
        put(&mut out, &mut at, &self.roots);
        put(
            &mut out,
            &mut at,
            &[u8::from(self.previous_record.is_some())],
        );
        put(&mut out, &mut at, &self.previous_record.unwrap_or([0; 32]));
        for root in &self.fields[..self.wire_version.field_count()] {
            put(&mut out, &mut at, &[u8::from(root.is_some())]);
            match root {
                Some(root) => put(
                    &mut out,
                    &mut at,
                    &root.canonical_bytes().map_err(|_| invalid())?,
                ),
                None => put(&mut out, &mut at, &[0; STATE_ROOT_BYTES]),
            }
        }
        Ok(CognitionProofLedgerCanonicalBytesV1 {
            bytes: out,
            len: at,
        })
    }
    pub fn from_canonical_bytes(raw: &[u8]) -> Result<Self, ValueTreeError> {
        let wire_version = if raw.len() == COGNITION_PROOF_LEDGER_BYTES_V1
            && raw.starts_with(COGNITION_PROOF_LEDGER_MAGIC_V1)
        {
            CognitionProofLedgerWireVersionV1::V1
        } else if raw.len() == COGNITION_PROOF_LEDGER_BYTES_V2
            && raw.starts_with(COGNITION_PROOF_LEDGER_MAGIC_V2)
        {
            CognitionProofLedgerWireVersionV1::V2
        } else if raw.len() == COGNITION_PROOF_LEDGER_BYTES_V3
            && raw.starts_with(COGNITION_PROOF_LEDGER_MAGIC_V3)
        {
            CognitionProofLedgerWireVersionV1::V3
        } else {
            return Err(invalid());
        };
        let mut at = wire_version.magic().len();
        let roots = raw[at..at + 32].try_into().map_err(|_| invalid())?;
        at += 32;
        let flag = raw[at];
        at += 1;
        let id: [u8; 32] = raw[at..at + 32].try_into().map_err(|_| invalid())?;
        at += 32;
        let previous_record = match flag {
            0 if id == [0; 32] => None,
            1 => Some(id),
            _ => return Err(invalid()),
        };
        let mut fields = std::array::from_fn(|_| None);
        for field in &mut fields[..wire_version.field_count()] {
            let flag = raw[at];
            at += 1;
            let bytes = &raw[at..at + STATE_ROOT_BYTES];
            at += STATE_ROOT_BYTES;
            *field = match flag {
                0 if bytes.iter().all(|b| *b == 0) => None,
                1 => Some(StateRootV1::from_canonical_bytes(bytes).map_err(|_| invalid())?),
                _ => return Err(invalid()),
            };
        }
        let result = Self {
            wire_version,
            roots,
            previous_record,
            fields,
        };
        result.validate()?;
        Ok(result)
    }
}

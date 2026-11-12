//! Independent ProofLedger Result profile. Never a RootSet Current leaf.
use oasis7_formal_protocol::incremental_state::{STATE_ROOT_BYTES, StateNamespaceV1, StateRootV1};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
pub const COGNITION_PROOF_LEDGER_MAGIC_V1: &[u8] = b"oasis7.cognition.proof-ledger.v1\0";
pub const COGNITION_PROOF_LEDGER_BYTES_V1: usize =
    COGNITION_PROOF_LEDGER_MAGIC_V1.len() + 32 + 33 + 5 * (1 + STATE_ROOT_BYTES);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CognitionProofLedgerFieldV1 {
    CommitRecords = 0,
    ReceiptRegistry = 1,
    ReceiptLineageRegistry = 2,
    Recovery = 3,
    RuntimeBinding = 4,
}
impl CognitionProofLedgerFieldV1 {
    pub const ALL: [Self; 5] = [
        Self::CommitRecords,
        Self::ReceiptRegistry,
        Self::ReceiptLineageRegistry,
        Self::Recovery,
        Self::RuntimeBinding,
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
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CognitionProofLedgerManifestV1 {
    pub roots: [u8; 32],
    pub previous_record: Option<[u8; 32]>,
    pub fields: [Option<StateRootV1>; 5],
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Cognition ProofLedger manifest")
}
impl CognitionProofLedgerManifestV1 {
    pub fn validate(&self) -> Result<(), ValueTreeError> {
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
    pub fn canonical_bytes(&self) -> Result<[u8; COGNITION_PROOF_LEDGER_BYTES_V1], ValueTreeError> {
        self.validate()?;
        let mut out = [0; COGNITION_PROOF_LEDGER_BYTES_V1];
        let mut at = 0;
        fn put(out: &mut [u8], at: &mut usize, v: &[u8]) {
            out[*at..*at + v.len()].copy_from_slice(v);
            *at += v.len();
        }
        put(&mut out, &mut at, COGNITION_PROOF_LEDGER_MAGIC_V1);
        put(&mut out, &mut at, &self.roots);
        put(
            &mut out,
            &mut at,
            &[u8::from(self.previous_record.is_some())],
        );
        put(&mut out, &mut at, &self.previous_record.unwrap_or([0; 32]));
        for root in &self.fields {
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
        Ok(out)
    }
    pub fn from_canonical_bytes(raw: &[u8]) -> Result<Self, ValueTreeError> {
        if raw.len() != COGNITION_PROOF_LEDGER_BYTES_V1
            || !raw.starts_with(COGNITION_PROOF_LEDGER_MAGIC_V1)
        {
            return Err(invalid());
        }
        let mut at = COGNITION_PROOF_LEDGER_MAGIC_V1.len();
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
        for field in &mut fields {
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
            roots,
            previous_record,
            fields,
        };
        result.validate()?;
        Ok(result)
    }
}

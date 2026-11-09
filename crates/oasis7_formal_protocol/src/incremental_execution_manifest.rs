use super::*;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionManifestModeV1 {
    Genesis,
    Execution {
        parent_commit: [u8; 32],
        parent_roots: [u8; 32],
        operation: [u8; 32],
        input: [u8; 32],
        continuation: OriginalArtifactRefV1,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IwsExecutionManifestV1 {
    pub scope: WorldScopeV1,
    pub format_profile: [u8; 32],
    pub native: [u8; 32],
    pub executable: [u8; 32],
    pub wasm: [u8; 32],
    pub rules: [u8; 32],
    pub policy: OriginalArtifactRefV1,
    pub mode: ExecutionManifestModeV1,
}
impl IwsExecutionManifestV1 {
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ExecutionError> {
        self.scope.validate()?;
        let mut w = Writer::new();
        w.scope(&self.scope);
        w.hash(&self.format_profile);
        for hash in [
            &self.native,
            &self.executable,
            &self.wasm,
            &self.rules,
            &self.policy.id,
        ] {
            w.hash(hash);
        }
        w.u64(self.policy.byte_len);
        match &self.mode {
            ExecutionManifestModeV1::Genesis => w.u8(0),
            ExecutionManifestModeV1::Execution {
                parent_commit,
                parent_roots,
                operation,
                input,
                continuation,
            } => {
                w.u8(1);
                for hash in [
                    parent_commit,
                    parent_roots,
                    operation,
                    input,
                    &continuation.id,
                ] {
                    w.hash(hash);
                }
                w.u64(continuation.byte_len);
            }
        }
        w.finish(b"oasis7.iws.execution-manifest.v1\0", 1024)
    }
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ExecutionError> {
        let mut r = Reader::new(bytes, b"oasis7.iws.execution-manifest.v1\0", 1024)?;
        let scope = r.scope()?;
        let format_profile = r.array()?;
        let native = r.array()?;
        let executable = r.array()?;
        let wasm = r.array()?;
        let rules = r.array()?;
        let policy = OriginalArtifactRefV1 {
            id: r.array()?,
            byte_len: r.u64()?,
        };
        let mode = match r.u8()? {
            0 => ExecutionManifestModeV1::Genesis,
            1 => ExecutionManifestModeV1::Execution {
                parent_commit: r.array()?,
                parent_roots: r.array()?,
                operation: r.array()?,
                input: r.array()?,
                continuation: OriginalArtifactRefV1 {
                    id: r.array()?,
                    byte_len: r.u64()?,
                },
            },
            _ => return Err(ExecutionError::Invalid("manifest mode")),
        };
        r.finish()?;
        scope.validate()?;
        Ok(Self {
            scope,
            format_profile,
            native,
            executable,
            wasm,
            rules,
            policy,
            mode,
        })
    }
    pub fn digest(&self) -> Result<[u8; 32], ExecutionError> {
        Ok(*blake3::hash(&self.canonical_bytes()?).as_bytes())
    }
}

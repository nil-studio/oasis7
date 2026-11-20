//! Narrow host capability for development source compilation.
//! Implementations may perform IO; callers retain admission and deployment authority.
use crate::world_action::ModuleSourcePackage;
use std::{fmt, io};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleSourceCompileError {
    ModuleChangeInvalid { reason: String },
    Io(String),
    Unavailable(String),
}
impl From<io::Error> for ModuleSourceCompileError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.to_string())
    }
}
impl fmt::Display for ModuleSourceCompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(reason) => f.write_str(reason),
            _ => write!(f, "{self:?}"),
        }
    }
}
impl std::error::Error for ModuleSourceCompileError {}

pub trait ModuleSourceCompiler: Send + Sync {
    /// Stream compiler output through the caller's metered sink. Implementors
    /// must poll the original caller work during IO and process waits. There is
    /// no fallback to the bare-Vec method, which cannot enforce an output cap.
    fn compile_to(
        &self,
        _module_id: &str,
        _package: &ModuleSourcePackage,
        _max_output_bytes: usize,
        _output: &mut dyn io::Write,
        _progress: &mut dyn FnMut(u64, u64) -> Result<(), ModuleSourceCompileError>,
    ) -> Result<(), ModuleSourceCompileError> {
        Err(ModuleSourceCompileError::Unavailable(
            "host compiler does not provide metered streaming output".into(),
        ))
    }

    fn compile(
        &self,
        module_id: &str,
        package: &ModuleSourcePackage,
    ) -> Result<Vec<u8>, ModuleSourceCompileError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct LegacyCompiler(AtomicUsize);
    impl ModuleSourceCompiler for LegacyCompiler {
        fn compile(
            &self,
            _: &str,
            _: &ModuleSourcePackage,
        ) -> Result<Vec<u8>, ModuleSourceCompileError> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(vec![0; 32])
        }
    }

    #[test]
    fn metered_streaming_refuses_an_unbounded_legacy_compiler_without_using_it() {
        let compiler = LegacyCompiler(AtomicUsize::new(0));
        let package = ModuleSourcePackage {
            manifest_path: "Cargo.toml".into(),
            files: Default::default(),
        };
        let mut output = vec![7];
        let mut progress_calls = 0;
        let error = compiler
            .compile_to("module", &package, 1, &mut output, &mut |_, _| {
                progress_calls += 1;
                Ok(())
            })
            .unwrap_err();
        assert!(matches!(error, ModuleSourceCompileError::Unavailable(_)));
        assert_eq!(compiler.0.load(Ordering::Relaxed), 0);
        assert_eq!(progress_calls, 0);
        assert_eq!(output, [7]);
        assert_eq!(compiler.compile("module", &package).unwrap(), vec![0; 32]);
    }
}

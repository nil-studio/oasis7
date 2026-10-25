//! Per-operation provenance across legacy tree/proof APIs returning String.
use super::*;
use std::cell::RefCell;

pub(super) fn with_typed_business_source<T>(
    reader: &dyn BusinessIndexNodeReader,
    operation: impl FnOnce(&ScopedBusinessReader<'_>) -> Result<T, String>,
) -> Result<T, FormalReadError> {
    let scoped = ScopedBusinessReader {
        reader,
        error: RefCell::new(None),
    };
    operation(&scoped).map_err(|diagnostic| {
        scoped
            .error
            .into_inner()
            .unwrap_or(FormalReadError::Integrity(diagnostic))
    })
}

pub(super) struct ScopedBusinessReader<'a> {
    reader: &'a dyn BusinessIndexNodeReader,
    error: RefCell<Option<FormalReadError>>,
}
impl ScopedBusinessReader<'_> {
    fn capture<T>(
        &self,
        result: Result<T, oasis7_distfs::controlled_authority::formal::FormalError>,
    ) -> Result<T, String> {
        result.map_err(|error| {
            let error = FormalReadError::from(error);
            let diagnostic = error.to_string();
            self.error.borrow_mut().get_or_insert(error);
            diagnostic
        })
    }
}
impl BusinessIndexNodeReader for ScopedBusinessReader<'_> {
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        self.capture(self.reader.read_node_for_read(digest))
    }
    fn prefetch_path(
        &self,
        root: &oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexRootV1,
        key: &str,
    ) -> Result<(), String> {
        self.capture(self.reader.prefetch_path_for_read(root, key))
    }
    fn read_cached_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        self.capture(self.reader.read_cached_node_for_read(digest))
    }
    fn cache_content_node(&self, digest: &str, bytes: &[u8]) -> Result<(), String> {
        self.capture(self.reader.cache_content_node_for_read(digest, bytes))
    }
}

impl BusinessIndexNodeReader for NativeHistoryResolver {
    fn read_node_for_read(
        &self,
        digest: &str,
    ) -> Result<Option<Vec<u8>>, oasis7_distfs::controlled_authority::formal::FormalError> {
        self.business_reader.read_node_for_read(digest)
    }
    fn prefetch_path_for_read(
        &self,
        root: &oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexRootV1,
        key: &str,
    ) -> Result<(), oasis7_distfs::controlled_authority::formal::FormalError> {
        self.business_reader.prefetch_path_for_read(root, key)
    }
    fn read_cached_node_for_read(
        &self,
        digest: &str,
    ) -> Result<Option<Vec<u8>>, oasis7_distfs::controlled_authority::formal::FormalError> {
        self.business_reader.read_cached_node_for_read(digest)
    }
    fn cache_content_node_for_read(
        &self,
        digest: &str,
        bytes: &[u8],
    ) -> Result<(), oasis7_distfs::controlled_authority::formal::FormalError> {
        self.business_reader
            .cache_content_node_for_read(digest, bytes)
    }

    fn prefetch_path(
        &self,
        root: &oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexRootV1,
        key: &str,
    ) -> Result<(), String> {
        self.business_reader.prefetch_path(root, key)
    }

    fn read_cached_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        self.business_reader.read_cached_node(digest)
    }

    fn cache_content_node(&self, digest: &str, bytes: &[u8]) -> Result<(), String> {
        self.business_reader.cache_content_node(digest, bytes)
    }

    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        self.business_reader.read_node(digest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_distfs::controlled_authority::formal::FormalError;

    struct Source(FormalError);
    impl BusinessIndexNodeReader for Source {
        fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
            panic!("the scoped adapter must use the typed source hook")
        }
        fn read_node_for_read(&self, _: &str) -> Result<Option<Vec<u8>>, FormalError> {
            Err(self.0.clone())
        }
    }
    #[test]
    fn scoped_business_proof_errors_preserve_source_and_reset_between_operations() {
        let diagnostic = "HistoryUnavailable grant node not ready".to_owned();
        let source = Source(FormalError::Conflict(diagnostic.clone()));
        assert_eq!(
            with_typed_business_source(&source, |reader| reader.read_node("digest")),
            Err(FormalReadError::Integrity(source.0.to_string()))
        );
        let source = Source(FormalError::Io(diagnostic.clone()));
        assert_eq!(
            with_typed_business_source(&source, |reader| reader.read_node("digest")),
            Err(FormalReadError::Internal(diagnostic.clone()))
        );
        // A pure proof computation error cannot inherit a prior source error.
        assert_eq!(
            with_typed_business_source::<()>(&source, |_| Err(diagnostic.clone())),
            Err(FormalReadError::Integrity(diagnostic))
        );
    }
}

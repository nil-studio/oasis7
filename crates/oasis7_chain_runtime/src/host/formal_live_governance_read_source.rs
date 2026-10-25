//! Source capture belongs to private replay projections, never shared resolver state.
use super::*;
use oasis7_distfs::controlled_authority::formal::FormalError;
use std::sync::Mutex;

pub(super) struct GovernanceProjection {
    pub(super) snapshot: oasis7_runtime::VerifiedGovernanceHistorySnapshot,
    pub(super) reader: Arc<GovernanceProjectionReader>,
}

pub(super) struct GovernanceProjectionReader {
    source: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
    error: Mutex<Option<FormalReadError>>,
}
impl GovernanceProjectionReader {
    pub(super) fn new(source: Arc<dyn BusinessIndexNodeReader + Send + Sync>) -> Arc<Self> {
        Arc::new(Self {
            source,
            error: Mutex::new(None),
        })
    }
    pub(super) fn take_error(&self) -> Result<Option<FormalReadError>, FormalReadError> {
        self.error
            .lock()
            .map(|mut error| error.take())
            .map_err(|_| FormalReadError::Internal("governance source capture poisoned".into()))
    }
    fn capture<T>(&self, result: Result<T, FormalError>) -> Result<T, String> {
        result.map_err(|error| {
            let error = FormalReadError::from(error);
            let diagnostic = error.to_string();
            match self.error.lock() {
                Ok(mut captured) => {
                    captured.get_or_insert(error);
                    diagnostic
                }
                Err(_) => "governance source capture poisoned".into(),
            }
        })
    }
}
impl BusinessIndexNodeReader for GovernanceProjectionReader {
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        match self.source.read_node_for_read(digest) {
            Ok(None) => {
                self.error
                    .lock()
                    .map_err(|_| "governance source capture poisoned")?
                    .get_or_insert_with(|| {
                        FormalReadError::HistoryUnavailable(format!(
                            "required governance history node {digest} is unavailable"
                        ))
                    });
                Ok(None)
            }
            result => self.capture(result),
        }
    }
    fn prefetch_path(
        &self,
        root: &oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexRootV1,
        key: &str,
    ) -> Result<(), String> {
        self.capture(self.source.prefetch_path_for_read(root, key))
    }
    fn read_cached_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        self.capture(self.source.read_cached_node_for_read(digest))
    }
    fn cache_content_node(&self, digest: &str, bytes: &[u8]) -> Result<(), String> {
        self.capture(self.source.cache_content_node_for_read(digest, bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexRootV1;
    struct Source(Option<FormalError>);
    impl BusinessIndexNodeReader for Source {
        fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
            panic!("typed hook required")
        }
        fn read_node_for_read(&self, _: &str) -> Result<Option<Vec<u8>>, FormalError> {
            match &self.0 {
                Some(error) => Err(error.clone()),
                None => Ok(None),
            }
        }
    }
    #[test]
    fn governance_tree_source_capture_distinguishes_io_integrity_and_required_absence() {
        let root = BusinessIndexRootV1 {
            node_root: "ab".repeat(32),
            leaf_count: 1,
        };
        for source in [
            Some(FormalError::Io("HistoryUnavailable grant not ready".into())),
            Some(FormalError::Conflict(
                "HistoryUnavailable grant not ready".into(),
            )),
            None,
        ] {
            let reader = GovernanceProjectionReader::new(Arc::new(Source(source.clone())));
            assert!(root.get(&"cd".repeat(32), reader.as_ref()).is_err());
            let captured = reader.take_error().unwrap().unwrap();
            match source {
                Some(FormalError::Io(_)) => {
                    assert!(matches!(captured, FormalReadError::Internal(_)))
                }
                Some(_) => assert!(matches!(captured, FormalReadError::Integrity(_))),
                None => assert!(matches!(captured, FormalReadError::HistoryUnavailable(_))),
            }
            assert_eq!(reader.take_error().unwrap(), None);
            assert_eq!(reader.read_cached_node("digest").unwrap(), None);
            assert_eq!(
                reader.take_error().unwrap(),
                None,
                "a cache miss is not missing history"
            );
        }
    }
}

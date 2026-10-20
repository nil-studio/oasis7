use super::{parse_marker_name, publish_immutable};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct PrivateTempRoot(PathBuf);

impl PrivateTempRoot {
    fn new() -> Self {
        let base = std::env::temp_dir()
            .canonicalize()
            .expect("canonical temp root");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = base.join(format!(
            "oasis7-durable-set-marker-{}-{now}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("create private temp root");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
                .expect("restrict private temp root");
        }
        Self(path)
    }
}

impl Drop for PrivateTempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn durable_set_marker_names_are_canonical_and_bounded() {
    assert_eq!(parse_marker_name("00000000000000000001.cbor").unwrap(), 1);
    assert!(parse_marker_name("00000000000000000000.cbor").is_err());
    assert!(parse_marker_name("1.cbor").is_err());
    assert!(parse_marker_name("00000000000000000001.json").is_err());
    assert!(parse_marker_name("18446744073709551616.cbor").is_err());
}

#[test]
fn durable_set_marker_publication_is_immutable_and_retryable() {
    let root = PrivateTempRoot::new();
    let directory = root.0.join("markers");
    fs::create_dir(&directory).expect("create marker directory");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
            .expect("restrict marker directory");
    }
    let path = directory.join("00000000000000000042.cbor");
    let original = b"canonical marker fixture";

    assert!(publish_immutable(&directory, &path, original).unwrap());
    assert_eq!(fs::read(&path).unwrap().as_slice(), original.as_slice());
    assert!(!publish_immutable(&directory, &path, original).unwrap());
    assert_eq!(fs::read(&path).unwrap().as_slice(), original.as_slice());
    assert!(!publish_immutable(&directory, &path, b"conflicting marker").unwrap());
    assert_eq!(fs::read(&path).unwrap().as_slice(), original.as_slice());
}

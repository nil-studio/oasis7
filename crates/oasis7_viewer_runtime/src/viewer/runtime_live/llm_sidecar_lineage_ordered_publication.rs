//! Process-local ordering for every writer of one lineage path.
//! This is not a cross-process lock or a physical antirollback guarantee.
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{
    Arc, Mutex, OnceLock, Weak,
    atomic::{AtomicU64, Ordering},
};
struct Publisher {
    next: AtomicU64,
    published: AtomicU64,
    rename: Mutex<()>,
}
pub(super) struct Reservation {
    publisher: Arc<Publisher>,
    generation: u64,
    path: PathBuf,
}
fn identity(path: &Path) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|_| "lineage publication current directory unavailable")?
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            value => normalized.push(value.as_os_str()),
        }
    }
    Ok(normalized)
}
pub(super) fn reserve(path: &Path) -> Result<Reservation, String> {
    static PUBLISHERS: OnceLock<Mutex<BTreeMap<PathBuf, Weak<Publisher>>>> = OnceLock::new();
    let path = identity(path)?;
    let mut publishers = PUBLISHERS
        .get_or_init(Mutex::default)
        .lock()
        .map_err(|_| "lineage publisher registry poisoned")?;
    publishers.retain(|_, publisher| publisher.strong_count() > 0);
    let publisher = publishers
        .get(&path)
        .and_then(Weak::upgrade)
        .unwrap_or_else(|| {
            let publisher = Arc::new(Publisher {
                next: AtomicU64::new(1),
                published: AtomicU64::new(0),
                rename: Mutex::new(()),
            });
            publishers.insert(path.clone(), Arc::downgrade(&publisher));
            publisher
        });
    drop(publishers);
    // Reservation never waits on the publication guard held across rename.
    let generation = publisher
        .next
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |next| {
            next.checked_add(1)
        })
        .map_err(|_| "lineage publication generation exhausted")?;
    Ok(Reservation {
        publisher,
        generation,
        path,
    })
}
pub(super) enum PublicationFailure {
    PermanentlyBlocked(String),
    RenameUnpublished(std::io::Error),
}
impl PublicationFailure {
    pub(super) fn permanently_blocked(&self) -> bool {
        matches!(self, Self::PermanentlyBlocked(_))
    }
}
impl std::fmt::Display for PublicationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PermanentlyBlocked(reason) => formatter.write_str(reason),
            Self::RenameUnpublished(error) => {
                write!(formatter, "lineage publication rename failed: {error}")
            }
        }
    }
}
impl Reservation {
    pub(super) fn publish(&self, temporary: &Path) -> Result<(), PublicationFailure> {
        // Check and rename share one linearization guard. Encoding/temp writes
        // and test gates never hold it. Rejected original bytes remain at their
        // unique temp path, including rename failures and unknown outcomes.
        let _publication = self.publisher.rename.lock().map_err(|_| {
            PublicationFailure::PermanentlyBlocked("lineage publisher poisoned".into())
        })?;
        if self.generation < self.publisher.published.load(Ordering::Acquire) {
            return Err(PublicationFailure::PermanentlyBlocked(
                "lineage publication superseded; original checkpoint retained".into(),
            ));
        }
        std::fs::rename(temporary, &self.path).map_err(|error| {
            // This syscall error proves that no replacement occurred. Keep the
            // same reservation and fully written temporary for a repaired-path retry.
            // All other errors retain the conservative permanent fence.
            if error.kind() == std::io::ErrorKind::IsADirectory {
                PublicationFailure::RenameUnpublished(error)
            } else {
                PublicationFailure::PermanentlyBlocked(format!(
                    "lineage publication rename failed: {error}"
                ))
            }
        })?;
        self.publisher
            .published
            .store(self.generation, Ordering::Release);
        Ok(())
    }
}

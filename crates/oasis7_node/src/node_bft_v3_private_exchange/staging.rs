//! One private, bounded descriptor. No path reopening or writable aliases.
use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    time::Instant,
};

pub(super) struct PrivateStage {
    file: File,
    limit: u64,
    deadline: Instant,
}
impl PrivateStage {
    pub(super) fn new(limit: usize, deadline: Instant) -> io::Result<Self> {
        if limit > super::wire::MAX_PACKAGE || Instant::now() >= deadline {
            return Err(io::Error::other("private staging capacity/deadline"));
        }
        let stage = Self {
            file: private_file()?,
            limit: limit as u64,
            deadline,
        };
        stage.live()?;
        Ok(stage)
    }
    fn live(&self) -> io::Result<()> {
        if Instant::now() >= self.deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "private staging deadline",
            ));
        }
        Ok(())
    }
}
impl Read for PrivateStage {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.live()?;
        let count = self.file.read(bytes)?;
        self.live()?;
        Ok(count)
    }
}
impl Write for PrivateStage {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.live()?;
        if self
            .file
            .stream_position()?
            .checked_add(bytes.len() as u64)
            .is_none_or(|end| end > self.limit)
        {
            return Err(io::Error::other("private staging disk capacity"));
        }
        let count = self.file.write(bytes)?;
        self.live()?;
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.live()?;
        self.file.flush()?;
        self.live()
    }
}
impl Seek for PrivateStage {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.live()?;
        let position = self.file.seek(position)?;
        self.live()?;
        if position > self.limit {
            return Err(io::Error::other("private staging seek capacity"));
        }
        Ok(position)
    }
}

#[cfg(any(unix, windows))]
fn private_file() -> io::Result<File> {
    let mut random = [0; 16];
    getrandom::fill(&mut random).map_err(|_| io::Error::other("private staging random source"))?;
    let path = std::env::temp_dir().join(format!("o7-private-stage-{}", hex::encode(random)));
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_ATTRIBUTE_TEMPORARY, FILE_FLAG_DELETE_ON_CLOSE,
        };
        options
            .share_mode(0)
            .attributes(FILE_ATTRIBUTE_TEMPORARY)
            .custom_flags(FILE_FLAG_DELETE_ON_CLOSE);
    }
    let file = options.open(&path)?;
    #[cfg(unix)]
    if let Err(error) = std::fs::remove_file(&path) {
        // No private bytes have been written. Close before retrying cleanup.
        drop(file);
        let _ = std::fs::remove_file(&path);
        return Err(error);
    }
    Ok(file)
}
#[cfg(not(any(unix, windows)))]
fn private_file() -> io::Result<File> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "private staging unavailable",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_descriptor_bounds_and_original_deadline_survive_rewind() {
        let mut stage =
            PrivateStage::new(3, Instant::now() + std::time::Duration::from_secs(5)).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            let metadata = stage.file.metadata().unwrap();
            assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
            assert_eq!(
                metadata.nlink(),
                0,
                "no visible private file before any write"
            );
        }
        stage.write_all(b"abc").unwrap();
        assert!(stage.write_all(b"d").is_err());
        stage.rewind().unwrap();
        assert!(stage.write_all(b"abcd").is_err());
        assert_eq!(stage.file.metadata().unwrap().len(), 3);
        let mut read = [0; 3];
        stage.read_exact(&mut read).unwrap();
        assert_eq!(&read, b"abc");
        assert!(stage.seek(SeekFrom::Start(4)).is_err());
        stage.deadline = Instant::now();
        assert!(stage.rewind().is_err());
        assert!(stage.read(&mut read).is_err());
        assert!(stage.write(b"x").is_err());
        assert!(stage.flush().is_err());
        assert!(PrivateStage::new(1, Instant::now()).is_err());
    }
}

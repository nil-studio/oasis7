use std::fs;
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

#[allow(dead_code)]
pub(crate) fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|err| format!("create state dir {} failed: {}", parent.display(), err))?;
    }
    let temp_path = path.with_extension("json.tmp");
    fs::write(&temp_path, bytes)
        .map_err(|err| format!("write state temp {} failed: {}", temp_path.display(), err))?;
    fs::rename(&temp_path, path).map_err(|err| {
        format!(
            "rename state temp {} -> {} failed: {}",
            temp_path.display(),
            path.display(),
            err
        )
    })
}

pub(crate) fn sha256_file_hex(path: &Path) -> io::Result<String> {
    let file = fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Sha256DirTree {
    pub(crate) sha256_tree: String,
    pub(crate) file_count: u64,
    pub(crate) total_bytes: u64,
}

pub(crate) fn sha256_dir_tree_hex(path: &Path) -> io::Result<Sha256DirTree> {
    let mut files = Vec::new();
    collect_files(path, &mut files)?;
    files.sort();

    let mut combined = Sha256::new();
    let mut total_bytes = 0_u64;
    for file in files.iter() {
        let rel = file.strip_prefix(path).map_err(io::Error::other)?;
        let rel = rel.to_string_lossy().replace('\\', "/");
        let digest = sha256_file_hex(file.as_path())?;
        let size = fs::metadata(file.as_path())?.len();
        combined.update(rel.as_bytes());
        combined.update(b"\0");
        combined.update(digest.as_bytes());
        combined.update(b"\0");
        combined.update(size.to_string().as_bytes());
        combined.update(b"\n");
        total_bytes += size;
    }
    Ok(Sha256DirTree {
        sha256_tree: hex::encode(combined.finalize()),
        file_count: files.len() as u64,
        total_bytes,
    })
}

fn collect_files(path: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();
        if child.is_dir() {
            collect_files(child.as_path(), files)?;
        } else if child.is_file() {
            files.push(child);
        }
    }
    Ok(())
}

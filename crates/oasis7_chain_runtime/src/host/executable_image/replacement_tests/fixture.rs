//! Own the disposable images and the loaded child for the replacement test.
use std::fs::File;
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Directory(PathBuf);

impl Directory {
    fn create(path: PathBuf) -> std::io::Result<Self> {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o700).create(&path)?;
        Ok(Self(path))
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(super) struct Images {
    root: Directory,
    pub(super) original_digest: String,
    pub(super) replacement_digest: String,
}

impl Images {
    pub(super) fn new() -> Self {
        let root = Directory::create(
            std::env::temp_dir()
                .canonicalize()
                .unwrap()
                .join(format!("oasis7-image-replace-{}", std::process::id())),
        )
        .unwrap();
        let image = root.0.join("image");
        let replacement = root.0.join("replacement");
        std::fs::copy(std::env::current_exe().unwrap(), &image).unwrap();
        std::fs::copy(&image, &replacement).unwrap();
        std::fs::OpenOptions::new()
            .append(true)
            .open(&replacement)
            .unwrap()
            .write_all(b"different replacement artifact bytes")
            .unwrap();
        Self {
            original_digest: digest(&image),
            replacement_digest: digest(&replacement),
            root,
        }
    }

    pub(super) fn replace_while_child_is_loaded(&self) -> String {
        let image = self.root.0.join("image");
        let mut process = Process(
            Command::new(&image)
                .args([
                    "--exact",
                    "executable_image::replacement_tests::executable_replacement_child",
                    "--ignored",
                    "--nocapture",
                ])
                .env("OASIS7_IMAGE_REPLACEMENT_CHILD", "1")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let mut output = std::io::BufReader::new(process.0.stdout.take().unwrap());
        let mut line = String::new();
        loop {
            line.clear();
            assert_ne!(output.read_line(&mut line).unwrap(), 0);
            if line.trim() == "image-child-ready" {
                break;
            }
        }
        std::fs::rename(&image, self.root.0.join("loaded-original")).unwrap();
        std::fs::rename(self.root.0.join("replacement"), &image).unwrap();
        process.0.stdin.take().unwrap().write_all(&[1]).unwrap();
        let mut remainder = String::new();
        output.read_to_string(&mut remainder).unwrap();
        assert!(process.0.wait().unwrap().success());
        remainder
    }
}

fn digest(path: &Path) -> String {
    let mut file = File::open(path).unwrap();
    let mut hash = blake3::Hasher::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer).unwrap();
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    hash.finalize().to_hex().to_string()
}

#[test]
fn failed_directory_creation_does_not_remove_an_existing_directory() {
    let path = std::env::temp_dir().join(format!("oasis7-image-owner-{}", std::process::id()));
    let directory = Directory::create(path.clone()).unwrap();
    let marker = directory.0.join("existing-owner");
    std::fs::write(&marker, b"preserve").unwrap();
    assert!(Directory::create(path).is_err());
    assert_eq!(std::fs::read(marker).unwrap(), b"preserve");
}

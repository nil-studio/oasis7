//! Atomic replacement regression runs only a private copy of the test binary.
use super::*;
use std::io::{BufRead, Read, Write};
#[test]
#[ignore = "private copied binary subprocess helper"]
fn executable_replacement_child() {
    if std::env::var_os("OASIS7_IMAGE_REPLACEMENT_CHILD").is_none() {
        return;
    }
    println!("image-child-ready");
    std::io::stdout().flush().unwrap();
    let mut byte = [0u8; 1];
    std::io::stdin().read_exact(&mut byte).unwrap();
    match crate::controlled_execution_policy::executable_digest() {
        Ok(digest) => println!("image-child-digest:{digest}"),
        Err(_) => println!("image-child-refused"),
    }
}
#[test]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn frozen_artifact_digest_never_hashes_an_atomic_replacement_as_loaded_image() {
    struct Process(std::process::Child);
    impl Drop for Process {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let root = Directory(
        std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("oasis7-image-replace-{}", std::process::id())),
    );
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&root.0)
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
    let digest = |path: &std::path::Path| {
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
    };
    let original_digest = digest(&image);
    let replacement_digest = digest(&replacement);
    assert_ne!(original_digest, replacement_digest);
    let mut process = Process(
        std::process::Command::new(&image)
            .args([
                "--exact",
                "executable_image::replacement_tests::executable_replacement_child",
                "--ignored",
                "--nocapture",
            ])
            .env("OASIS7_IMAGE_REPLACEMENT_CHILD", "1")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
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
    std::fs::rename(&image, root.0.join("loaded-original")).unwrap();
    std::fs::rename(&replacement, &image).unwrap();
    process.0.stdin.take().unwrap().write_all(&[1]).unwrap();
    let mut remainder = String::new();
    output.read_to_string(&mut remainder).unwrap();
    assert!(process.0.wait().unwrap().success());
    #[cfg(target_os = "macos")]
    assert!(
        remainder.lines().any(|line| line == "image-child-refused"),
        "{remainder}"
    );
    #[cfg(target_os = "linux")]
    assert!(
        remainder
            .lines()
            .any(|line| line == format!("image-child-digest:{original_digest}")),
        "{remainder}"
    );
    assert!(
        !remainder
            .lines()
            .any(|line| line == format!("image-child-digest:{replacement_digest}"))
    );
}

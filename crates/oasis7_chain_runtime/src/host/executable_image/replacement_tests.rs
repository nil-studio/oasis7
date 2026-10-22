//! Atomic replacement regression runs only a private copy of the test binary.
use std::io::{Read, Write};

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod expectations;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod fixture;

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
    let images = fixture::Images::new();
    assert_ne!(images.original_digest, images.replacement_digest);
    let output = images.replace_while_child_is_loaded();
    expectations::assert_platform_report(&output, &images.original_digest);
    expectations::assert_replacement_not_reported(&output, &images.replacement_digest);
}

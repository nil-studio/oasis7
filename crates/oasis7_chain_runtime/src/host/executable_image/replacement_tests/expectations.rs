//! Keep the platform oracle separate from subprocess and filesystem mechanics.
fn contains_line(output: &str, expected: &str) -> bool {
    for line in output.lines() {
        if line == expected {
            return true;
        }
    }
    false
}

#[cfg(target_os = "linux")]
pub(super) fn assert_platform_report(output: &str, original_digest: &str) {
    let expected = format!("image-child-digest:{original_digest}");
    assert!(contains_line(output, &expected), "{output}");
}

#[cfg(target_os = "macos")]
pub(super) fn assert_platform_report(output: &str, _original_digest: &str) {
    assert!(contains_line(output, "image-child-refused"), "{output}");
}

pub(super) fn assert_replacement_not_reported(output: &str, replacement_digest: &str) {
    let forbidden = format!("image-child-digest:{replacement_digest}");
    assert!(!contains_line(output, &forbidden));
}

#[test]
fn report_matches_whole_lines_without_accepting_a_digest_prefix() {
    let output = "image-child-digest:original\nimage-child-refused\n";
    assert!(contains_line(output, "image-child-digest:original"));
    assert!(contains_line(output, "image-child-refused"));
    assert!(!contains_line(output, "image-child-digest:orig"));
    assert!(!contains_line(output, "image-child-digest:replacement"));
}

#[test]
#[should_panic]
fn replacement_digest_is_always_rejected() {
    assert_replacement_not_reported("image-child-digest:replacement\n", "replacement");
}

#[test]
#[should_panic]
fn platform_report_rejects_a_different_digest() {
    assert_platform_report("image-child-digest:replacement\n", "original");
}

#[test]
#[should_panic]
fn platform_report_rejects_empty_output() {
    assert_platform_report("", "original");
}

#[test]
fn platform_report_accepts_the_platform_result() {
    #[cfg(target_os = "linux")]
    let output = "image-child-digest:original\n";
    #[cfg(target_os = "macos")]
    let output = "image-child-refused\n";
    assert_platform_report(output, "original");
    assert_replacement_not_reported(output, "replacement");
}

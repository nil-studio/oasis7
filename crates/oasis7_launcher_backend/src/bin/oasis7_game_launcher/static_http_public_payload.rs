//! Cache only public content-addressed renderer bytes, never HTML or identity.
use std::path::Path;

fn is_public_hash_payload(path: &Path) -> bool {
    path.parent()
        .and_then(Path::file_name)
        .and_then(|v| v.to_str())
        == Some("viewer-optional-payload")
        && path.extension().and_then(|v| v.to_str()) == Some("wasm")
        && path
            .file_stem()
            .and_then(|v| v.to_str())
            .is_some_and(|hash| {
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
}
fn accepts_gzip(request: &str) -> bool {
    request
        .lines()
        .filter_map(|line| line.split_once(':'))
        .filter(|(key, _)| key.eq_ignore_ascii_case("accept-encoding"))
        .flat_map(|(_, value)| value.split(','))
        .any(|item| {
            let mut parts = item.trim().split(';');
            if !parts
                .next()
                .is_some_and(|v| v.trim().eq_ignore_ascii_case("gzip"))
            {
                return false;
            }
            parts.all(|part| {
                part.trim()
                    .strip_prefix("q=")
                    .is_none_or(|q| q.parse::<f32>().is_ok_and(|q| q > 0.0 && q <= 1.0))
            })
        })
}

pub(super) fn response(
    root: &Path,
    path: &Path,
    request: &str,
) -> Result<Option<(Vec<u8>, String)>, String> {
    let immutable = is_public_hash_payload(path);
    let static_code = matches!(
        path.extension().and_then(|v| v.to_str()),
        Some("js" | "css")
    );
    if !immutable && !static_code {
        return Ok(None);
    }
    let mut headers = if immutable {
        "Cache-Control: public, max-age=31536000, immutable\r\nVary: Accept-Encoding\r\n"
    } else {
        "Cache-Control: no-cache\r\nVary: Accept-Encoding\r\n"
    }
    .to_string();
    let extension = path.extension().and_then(|v| v.to_str()).unwrap_or("");
    let compressed = path.with_extension(format!("{extension}.gz"));
    let target = if accepts_gzip(request) && compressed.is_file() {
        let canonical_root = root.canonicalize().map_err(|e| e.to_string())?;
        let canonical = compressed.canonicalize().map_err(|e| e.to_string())?;
        if !canonical.starts_with(canonical_root) {
            return Err("compressed payload escapes static root".into());
        }
        headers.push_str("Content-Encoding: gzip\r\n");
        canonical
    } else {
        path.to_path_buf()
    };
    let canonical_root = root.canonicalize().map_err(|e| e.to_string())?;
    let target = target.canonicalize().map_err(|e| e.to_string())?;
    if !target.starts_with(canonical_root) {
        return Err("public payload escapes static root".into());
    }
    let bytes = std::fs::read(target).map_err(|e| format!("public payload read failed: {e}"))?;
    Ok(Some((bytes, headers)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn immutable_is_restricted_to_the_public_hash_namespace() {
        let public = format!("/root/viewer-optional-payload/{}.wasm", "a".repeat(64));
        assert!(is_public_hash_payload(Path::new(&public)));
        for path in [
            "/root/index.html",
            "/root/optional-payloads.json",
            "/root/viewer.js",
            "/root/private.wasm",
        ] {
            assert!(!is_public_hash_payload(Path::new(path)));
        }
        assert!(!is_public_hash_payload(Path::new(
            &public.replace("viewer-optional-payload", "sessions")
        )));
    }
    #[test]
    fn encoding_negotiation_honors_explicit_gzip_and_zero_quality() {
        assert!(accepts_gzip(
            "GET / HTTP/1.1\r\nAccept-Encoding: br, gzip;q=0.8\r\n"
        ));
        for value in ["gzip;q=0", "br", "gzip;q=invalid", "gzip;q=2", "notgzip"] {
            assert!(!accepts_gzip(&format!("Accept-Encoding: {value}")));
        }
    }
    #[test]
    fn gzip_and_identity_share_immutable_headers_and_exact_bodies() {
        let root =
            std::env::temp_dir().join(format!("oasis7-public-payload-{}", std::process::id()));
        let dir = root.join("viewer-optional-payload");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{}.wasm", "b".repeat(64)));
        std::fs::write(&path, b"canonical").unwrap();
        std::fs::write(path.with_extension("wasm.gz"), b"compressed").unwrap();
        let (body, headers) = response(&root, &path, "Accept-Encoding: gzip")
            .unwrap()
            .unwrap();
        assert_eq!(body, b"compressed");
        assert!(headers.contains("Content-Encoding: gzip"));
        let (body, headers) = response(&root, &path, "Accept-Encoding: gzip;q=0")
            .unwrap()
            .unwrap();
        assert_eq!(body, b"canonical");
        assert!(!headers.contains("Content-Encoding"));
        assert!(headers.contains("immutable"));
        assert!(headers.contains("Vary: Accept-Encoding"));
        let js = root.join("viewer.js");
        std::fs::write(&js, b"code").unwrap();
        let (_, headers) = response(&root, &js, "").unwrap().unwrap();
        assert!(headers.contains("Cache-Control: no-cache"));
        assert!(!headers.contains("immutable"));
        assert!(
            response(&root, &root.join("index.html"), "")
                .unwrap()
                .is_none()
        );
        #[cfg(unix)]
        {
            let outside = root.with_extension("outside");
            std::fs::write(&outside, b"private").unwrap();
            let sidecar = path.with_extension("wasm.gz");
            std::fs::remove_file(&sidecar).unwrap();
            std::os::unix::fs::symlink(&outside, &sidecar).unwrap();
            assert!(response(&root, &path, "Accept-Encoding: gzip").is_err());
            std::fs::remove_file(outside).unwrap();
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
